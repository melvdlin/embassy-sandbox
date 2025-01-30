use core::fmt::Display;

use embassy_net::tcp;
use embassy_net::tcp::TcpSocket;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Duration;
use embassy_time::Timer;
use getargs::Options;
use heapless::Vec;
use scuffed_write::async_writeln;

use crate::log;
use crate::util::ByteSliceExt;

pub async fn cli_task<M, const N: usize>(
    port: u16,
    log: &log::Channel<M, N>,
    net_up: &Signal<M, ()>,
    stack: embassy_net::Stack<'_>,
) -> !
where
    M: RawMutex,
{
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

        let result = handle_cli_connection(&mut server, log).await;
        if let Err(e) = result {
            let mut log_writer = log.writer().await;
            let Ok(()) = async_writeln!(log_writer, "cli error: {}", e).await;
            let Ok(()) = async_writeln!(log_writer, "cli connection closed!").await;
            drop(log_writer)
        }
    }
}

async fn handle_cli_connection<M, const N: usize>(
    socket: &mut TcpSocket<'_>,
    log: &log::Channel<M, N>,
) -> Result<(), CliError>
where
    M: RawMutex,
{
    let mut buf = [0; 512];
    loop {
        let len = match socket.read(&mut buf).await {
            | Err(e) => {
                break Err(CliError::Read(e));
            }
            | Ok(0) => {
                let Ok(()) =
                    async_writeln!(log.writer().await, "connection closed!").await;
                break Ok(());
            }
            | Ok(len) => len,
        };
        let buf = &mut buf[..len].trim_ascii_mut();
        let mut tokens = Vec::<&[u8], 16>::new();
        for token in embedded_cli::token::inplace::Tokens::new_cli(buf) {
            match token {
                | Err(e) => {
                    async_writeln!(socket, "{e}").await.map_err(CliError::Write)?
                }
                | Ok(token) => todo!(),
            }
        }
        let opts = Options::new(tokens.into_iter());
        todo!()
    }
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
