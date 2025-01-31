use core::error::Error;
use core::fmt::Display;
use core::str::Utf8Error;

use embassy_net::tcp;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack as NetStack;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Duration;
use embassy_time::Timer;
use embedded_cli::token::TokenizeError;
use getargs::Arg;
use getargs::Argument;
use getargs::Options;
use heapless::Vec;
use scuffed_write::async_writeln;

use crate::log;
use crate::util::ByteSliceExt;

pub async fn cli_task<M: RawMutex, const N: usize>(
    port: u16,
    log: &log::Channel<M, N>,
    net_up: &Signal<M, ()>,
    stack: NetStack<'_>,
) -> ! {
    let mut rx_buf = [0; 4096];
    let mut tx_buf = [0; 4096];

    net_up.wait().await;
    let mut server = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
    server.set_keep_alive(Some(Duration::from_secs(10)));
    server.set_timeout(Some(Duration::from_secs(20)));

    loop {
        if let Err(e) = server.accept(port).await {
            let Ok(()) =
                async_writeln!(log.writer().await, "failed to accept connection: {e:?}")
                    .await;
            Timer::after_secs(1).await;
            continue;
        }

        let result = handle_cli_connection(&mut server, stack, log).await;
        if let Err(e) = result {
            let mut log_writer = log.writer().await;
            let Ok(()) = async_writeln!(log_writer, "cli error: {}", e).await;
            let Ok(()) = async_writeln!(log_writer, "cli connection closed!").await;
            drop(log_writer)
        }
    }
}

async fn handle_cli_connection<M: RawMutex, const N: usize>(
    socket: &mut TcpSocket<'_>,
    stack: NetStack<'_>,
    log: &log::Channel<M, N>,
) -> Result<(), SessionError> {
    let mut buf = [0; 512];

    // REPL
    loop {
        let len = socket.read(&mut buf).await.map_err(SessionError::Read)?;
        if len == 0 {
            break;
        }
        evaluate(&mut buf[..len], socket, stack, log).await?;
    }
    let Ok(()) = async_writeln!(log.writer().await, "connection closed!").await;
    Ok(())
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
enum SessionError {
    Read(tcp::Error),
    Write(tcp::Error),
}

impl Display for SessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "session error: ")?;
        match *self {
            | SessionError::Read(e) => {
                write!(f, "failed to read from connection: {}", TcpErrorDisplay(e))
            }
            | SessionError::Write(e) => {
                write!(f, "failed to write to connection: {}", TcpErrorDisplay(e))
            }
        }
    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
struct TcpErrorDisplay(tcp::Error);

impl Display for TcpErrorDisplay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "TCP error: {}",
            match self.0 {
                | tcp::Error::ConnectionReset => "connection reset",
            }
        )
    }
}

impl core::error::Error for TcpErrorDisplay {}
impl From<tcp::Error> for TcpErrorDisplay {
    fn from(e: tcp::Error) -> Self {
        Self(e)
    }
}

impl From<TcpErrorDisplay> for tcp::Error {
    fn from(wrapper: TcpErrorDisplay) -> Self {
        wrapper.0
    }
}

async fn evaluate<M: RawMutex, const N: usize>(
    input: &mut [u8],
    socket: &mut TcpSocket<'_>,
    stack: NetStack<'_>,
    log: &log::Channel<M, N>,
) -> Result<(), SessionError> {
    let buf = &mut input.trim_ascii_mut();
    let tokens =
        match Result::<Result<Vec<&str, 16>, Utf8Error>, TokenizeError>::from_iter(
            embedded_cli::token::inplace::Tokens::new_cli(buf)
                .map(|r| r.map(core::str::from_utf8)),
        ) {
            // tokenize error
            | Err(e) => {
                async_writeln!(socket, "{e}").await.map_err(SessionError::Write)?;
                return Ok(());
            }
            // utf8 error
            | Ok(Err(e)) => {
                async_writeln!(socket, "{e}").await.map_err(SessionError::Write)?;
                return Ok(());
            }
            | Ok(Ok(tokens)) => tokens,
        };
    let mut opts = Options::new(tokens.into_iter());

    let Some(command) = opts.next_positional() else {
        return Ok(());
    };

    let cmd_result = match Command::try_from_str(command) {
        | Err(e) => async_writeln!(socket, "{e}")
            .await
            .map_err(SessionError::Write)
            .map(|_| Ok(())),
        | Ok(cmd) => cmd.run(opts, socket, stack, log).await,
    }?;
    if let Err(e) = cmd_result {
        async_writeln!(socket, "{e}").await.map_err(SessionError::Write)?
    }
    Ok(())
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
pub enum Command {
    Download,
}

impl Command {
    pub fn try_from_str(s: &str) -> Result<Self, CliError<&str>> {
        Ok(if s.eq_ignore_ascii_case("download") {
            Self::Download
        } else {
            return Err(CliError::UnknownCommand(s));
        })
    }

    pub async fn run<'a, M, I, const N: usize>(
        self,
        args: Options<&'a str, I>,
        sock: &mut TcpSocket<'_>,
        stack: NetStack<'_>,
        log: &log::Channel<M, N>,
    ) -> Result<Result<(), CliError<&'a str>>, SessionError>
    where
        I: Iterator<Item = &'a str>,
        M: RawMutex,
    {
        match self {
            | Command::Download => command::download(args, sock, stack, log).await,
        }
    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub enum CliError<A: Argument> {
    Tokenize(TokenizeError),
    UnknownCommand(A),
    ArgValue(getargs::Error<A>),
    UnknownArg(getargs::Arg<A>),
    MissingArg(getargs::Arg<A>),
}

impl<A> Display for CliError<A>
where
    A: Display,
    A: Argument,
    A::ShortOpt: Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CLI error: ")?;
        match *self {
            | Self::Tokenize(e) => write!(f, "{e}"),
            | Self::UnknownCommand(cmd) => write!(f, "unknown command: {cmd}"),
            | Self::ArgValue(e) => write!(f, "{e}"),
            | Self::UnknownArg(arg) => write!(f, "unknown arg: {arg}"),
            | Self::MissingArg(arg) => write!(f, "missing arg: {arg}"),
        }
    }
}

impl<A> Error for CliError<A>
where
    A: Display,
    A: Argument,
    A::ShortOpt: Display,
{
}

impl<A> From<getargs::Error<A>> for CliError<A>
where
    A: Argument,
{
    fn from(value: getargs::Error<A>) -> Self {
        Self::ArgValue(value)
    }
}

mod command {
    use embassy_net::tcp::TcpSocket;
    use embassy_net::Stack as NetStack;
    use getargs::Arg;
    use getargs::Options;

    use super::*;
    use crate::log;

    pub async fn download<'a, M, I, const N: usize>(
        mut args: Options<&'a str, I>,
        sock: &mut TcpSocket<'_>,
        stack: NetStack<'_>,
        log: &log::Channel<M, N>,
    ) -> Result<Result<(), CliError<&'a str>>, SessionError>
    where
        I: Iterator<Item = &'a str>,
        M: RawMutex,
    {
        let host = match args.next_arg() {
            | Err(e) => return Ok(Err(CliError::ArgValue(e))),
            | Ok(None) => return Ok(Err(CliError::MissingArg(Arg::Positional("host")))),
            | Ok(Some(arg)) => match arg {
                | Arg::Short(_) | Arg::Long(_) => {
                    return Ok(Err(CliError::UnknownArg(arg)))
                }
                | Arg::Positional(host) => host,
            },
        };

        let filename = match args.next_arg() {
            | Err(e) => return Ok(Err(CliError::ArgValue(e))),
            | Ok(None) => {
                return Ok(Err(CliError::MissingArg(Arg::Positional("filename"))))
            }
            | Ok(Some(arg)) => match arg {
                | Arg::Short(_) | Arg::Long(_) => {
                    return Ok(Err(CliError::UnknownArg(arg)))
                }
                | Arg::Positional(filename) => filename,
            },
        };

        match args.next_arg() {
            | Err(e) => return Ok(Err(CliError::ArgValue(e))),
            | Ok(Some(arg)) => return Ok(Err(CliError::UnknownArg(arg))),
            | Ok(None) => {}
        }

        // todo: rework return type; nested Results are rather unergonomic
        todo!()
    }
}
