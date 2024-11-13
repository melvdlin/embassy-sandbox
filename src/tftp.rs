use core::ffi::CStr;

use embassy_net::udp::{RecvError, SendError, UdpMetadata, UdpSocket};
use embedded_io_async::{Read, Write};

use ttftp::client::upload;
use ttftp::client::upload::*;
use ttftp::client::FilenameError;
use ttftp::client::TransferError as TtftpError;
use ttftp::Mode;

pub async fn upload<'filename, 'rx, F: Read>(
    filename: &'filename CStr,
    file: F,
    sock: &UdpSocket<'_>,
    remote: UdpMetadata,
    file_buf: &mut [u8; ttftp::BLOCK_SIZE],
    rx: &'rx mut [u8; ttftp::PACKET_SIZE],
    tx: &mut [u8; ttftp::PACKET_SIZE],
) -> Result<(), TransferError<'filename, 'rx, F::Error>> {
    assert!(sock.payload_recv_capacity() >= ttftp::PACKET_SIZE);

    let mut state;
    let send;

    (state, send) = upload::new(tx, filename, Mode::Octect)?;
    let mut file = file;
    let mut buf_offset = 0;

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

pub async fn download<'filename, 'rx, F: Write>(
    filename: &'filename CStr,
    file: F,
    sock: &UdpSocket<'_>,
    rx: &'rx mut [u8; ttftp::PACKET_SIZE],
    tx: &mut [u8; ttftp::PACKET_SIZE],
) -> Result<(), TransferError<'filename, 'rx, F::Error>> {
    assert!(sock.payload_recv_capacity() >= ttftp::PACKET_SIZE);

    todo!()
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
