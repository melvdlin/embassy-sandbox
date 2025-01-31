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
) -> Result<(), CliError> {
    let mut buf = [0; 512];

    // REPL
    loop {
        let len = socket.read(&mut buf).await.map_err(CliError::Read)?;
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
enum CliError {
    Read(tcp::Error),
    Write(tcp::Error),
}

impl Display for CliError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CLI error: ")?;
        match *self {
            | CliError::Read(e) => {
                write!(f, "failed to read from connection: {}", TcpErrorDisplay(e))
            }
            | CliError::Write(e) => {
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
) -> Result<(), CliError> {
    let buf = &mut input.trim_ascii_mut();
    let tokens =
        match Result::<Result<Vec<&str, 16>, Utf8Error>, TokenizeError>::from_iter(
            embedded_cli::token::inplace::Tokens::new_cli(buf)
                .map(|r| r.map(core::str::from_utf8)),
        ) {
            // tokenize error
            | Err(e) => {
                async_writeln!(socket, "{e}").await.map_err(CliError::Write)?;
                return Ok(());
            }
            // utf8 error
            | Ok(Err(e)) => {
                async_writeln!(socket, "{e}").await.map_err(CliError::Write)?;
                return Ok(());
            }
            | Ok(Ok(tokens)) => tokens,
        };
    let mut opts = Options::new(tokens.into_iter());

    let Some(command) = opts.next_positional() else {
        return Ok(());
    };

    Ok(match Command::try_from_str(command) {
        | Err(e) => async_writeln!(socket, "{e}").await.map_err(CliError::Write),
        | Ok(cmd) => cmd.run(opts, socket, stack, log).await,
    }?)
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
pub enum Command {
    Download,
}

impl Command {
    pub fn try_from_str(s: &str) -> Result<Self, UnknownCommandError> {
        Ok(if s.eq_ignore_ascii_case("download") {
            Self::Download
        } else {
            return Err(UnknownCommandError);
        })
    }

    pub async fn run<'a, M, I, const N: usize>(
        self,
        args: Options<&'a str, I>,
        sock: &mut TcpSocket<'_>,
        stack: NetStack<'_>,
        log: &log::Channel<M, N>,
    ) -> Result<(), CliError>
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
#[derive(Hash)]
pub struct UnknownCommandError;

impl Display for UnknownCommandError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown command")
    }
}

impl Error for UnknownCommandError {}

mod command {
    use embassy_net::tcp::TcpSocket;
    use embassy_net::Stack as NetStack;
    use getargs::Options;

    use super::*;
    use crate::log;

    pub async fn download<'a, M, I, const N: usize>(
        mut args: Options<&'a str, I>,
        sock: &mut TcpSocket<'_>,
        stack: NetStack<'_>,
        log: &log::Channel<M, N>,
    ) -> Result<(), CliError>
    where
        I: Iterator<Item = &'a str>,
        M: RawMutex,
    {
        todo!()
    }
}
