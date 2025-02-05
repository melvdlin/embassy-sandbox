use core::error::Error;
use core::ffi::CStr;
use core::fmt::Debug;
use core::fmt::Display;

use embassy_net::udp::RecvError;
use embassy_net::udp::SendError;
use embassy_net::udp::UdpMetadata;
use embassy_net::udp::UdpSocket;
use embedded_io_async::Read;
use embedded_io_async::Write;
use ttftp::client::download;
use ttftp::client::upload;
use ttftp::client::upload::*;
use ttftp::client::FilenameError;
use ttftp::client::TransferError as TtftpError;
use ttftp::Mode;

pub async fn upload<'filename, F: Read>(
    filename: &'filename CStr,
    file: F,
    sock: &UdpSocket<'_>,
    remote: UdpMetadata,
    file_buf: &mut [u8; ttftp::BLOCK_SIZE],
    rx: &mut [u8; ttftp::PACKET_SIZE],
    tx: &mut [u8; ttftp::PACKET_SIZE],
) -> Result<(), TransferError<'filename, 'static, F::Error>> {
    assert!(sock.payload_recv_capacity() >= ttftp::PACKET_SIZE);

    let mut file = file;
    let mut buf_offset = 0;

    let mut state;
    let send;
    (state, send) = upload::new(tx, filename, Mode::Octect)?;

    loop {
        sock.send_to(&tx[..send], remote).await?;
        let received = loop {
            let (received, sender) = sock.recv_from(rx).await?;
            if sender.endpoint == remote.endpoint {
                break received;
            }
        };

        let buf_len = buf_offset
            + fill_buf(&mut file, &mut file_buf[buf_offset..])
                .await
                .map_err(TransferError::File)?;
        let (result, send) =
            state.process(&rx[..received], tx, file_buf[..buf_len].iter().copied());

        if let Some(send) = send {
            sock.send_to(&tx[..send], remote).await?;
        }

        let consumed;
        (state, consumed) = match result.map_err(TtftpError::strip)? {
            | AckReceived::NextBlock(awaiting_ack, consumed) => (awaiting_ack, consumed),
            | AckReceived::TransferComplete => break,
            | AckReceived::Retransmission(awaiting_ack) => (awaiting_ack, 0),
        };

        buf_offset = buf_len - consumed;
    }

    Ok(())
}

async fn fill_buf<F: Read>(file: F, buf: &mut [u8]) -> Result<usize, F::Error> {
    let mut file = file;
    let mut written = 0;
    while written < buf.len() {
        match file.read(&mut buf[written..]).await? {
            | 0 => return Ok(written),
            | n => written += n,
        }
    }
    Ok(written)
}

pub async fn download<'filename, F: Write>(
    filename: &'filename CStr,
    file: F,
    sock: &UdpSocket<'_>,
    remote: UdpMetadata,
    rx: &mut [u8; ttftp::PACKET_SIZE],
    tx: &mut [u8; ttftp::PACKET_SIZE],
) -> Result<(), TransferError<'filename, 'static, F::Error>> {
    assert!(sock.payload_recv_capacity() >= ttftp::PACKET_SIZE);

    let mut file = file;

    let mut state;
    let send;
    (state, send) = download::new(tx, filename, Mode::Octect)?;

    loop {
        sock.send_to(&tx[..send], remote).await?;
        let received = loop {
            let (received, sender) = sock.recv_from(rx).await?;
            if sender.endpoint == remote.endpoint {
                break received;
            }
        };

        let (result, send) = state.process(&rx[..received], tx);

        if let Some(send) = send {
            sock.send_to(&tx[..send], remote).await?;
        }

        state = match result.map_err(TtftpError::strip)? {
            | download::BlockReceived::Intermediate(awaiting_data, block) => {
                file.write_all(block).await.map_err(TransferError::File)?;
                awaiting_data
            }
            | download::BlockReceived::Final(block) => {
                file.write_all(block).await.map_err(TransferError::File)?;
                break;
            }
            | download::BlockReceived::Retransmission(awaiting_data) => awaiting_data,
        }
    }

    Ok(())
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub enum TransferError<'filename, 'rx, File> {
    Filename(FilenameError<'filename>),
    Tftp(TtftpError<'rx>),
    Send(SendError),
    Recv(RecvError),
    File(File),
}

impl<File> Display for TransferError<'_, '_, File> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (msg, cause): (&str, &dyn Display) = match self {
            | TransferError::Filename(e) => ("bad filename", e),
            | TransferError::Tftp(e) => ("TTFTP", e),
            | TransferError::Send(e) => (
                "UDP send",
                match e {
                    | SendError::NoRoute => &"no route",
                    | SendError::SocketNotBound => &"socket not bound",
                    | SendError::PacketTooLarge => &"packet too large",
                },
            ),
            | TransferError::Recv(e) => (
                "UDP receive",
                match e {
                    | RecvError::Truncated => &"truncated",
                },
            ),
            | TransferError::File(_e) => ("file read or write", &""),
        };

        write!(f, "file transfer failed: {msg}: {cause}")
    }
}

impl<File: Debug> Error for TransferError<'_, '_, File> {}

impl<'filename, File> From<FilenameError<'filename>>
    for TransferError<'filename, 'static, File>
{
    fn from(filename: FilenameError<'filename>) -> Self {
        TransferError::Filename(filename)
    }
}

impl<'rx, File> From<TtftpError<'rx>> for TransferError<'static, 'rx, File> {
    fn from(tftp: TtftpError<'rx>) -> Self {
        TransferError::Tftp(tftp)
    }
}

impl<File> From<SendError> for TransferError<'static, 'static, File> {
    fn from(send: SendError) -> Self {
        TransferError::Send(send)
    }
}

impl<File> From<RecvError> for TransferError<'static, 'static, File> {
    fn from(recv: RecvError) -> Self {
        TransferError::Recv(recv)
    }
}
