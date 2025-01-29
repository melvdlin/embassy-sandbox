use embassy_net::tcp;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::mutex::MutexGuard;
use embassy_sync::pipe::Pipe;
use embassy_sync::signal::Signal;
use embassy_time::Duration;
use embassy_time::Timer;
use embassy_time::WithTimeout;
use embedded_io_async::ErrorType;
use embedded_io_async::Read;
use embedded_io_async::Write;

pub struct Channel<M: RawMutex, const N: usize> {
    pipe: Pipe<M, N>,
    write_lock: Mutex<M, ()>,
}

impl<M: RawMutex, const N: usize> Channel<M, N> {
    pub const fn new() -> Self {
        Self {
            pipe: Pipe::new(),
            write_lock: Mutex::new(()),
        }
    }

    pub async fn writer(&self) -> WriteGuard<'_, M, N> {
        WriteGuard {
            channel: self,
            guard: self.write_lock.lock().await,
        }
    }
}

impl<M: RawMutex, const N: usize> Default for Channel<M, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: RawMutex, const N: usize> ErrorType for Channel<M, N> {
    type Error = <Pipe<M, N> as ErrorType>::Error;
}

impl<M: RawMutex, const N: usize> ErrorType for &'_ Channel<M, N> {
    type Error = <Channel<M, N> as ErrorType>::Error;
}

impl<M: RawMutex, const N: usize> Read for &'_ Channel<M, N> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        Ok(self.pipe.read(buf).await)
    }
}

pub struct WriteGuard<'a, M: RawMutex, const N: usize> {
    channel: &'a Channel<M, N>,
    #[expect(unused)]
    guard: MutexGuard<'a, M, ()>,
}

impl<M: RawMutex, const N: usize> ErrorType for WriteGuard<'_, M, N> {
    type Error = <Channel<M, N> as ErrorType>::Error;
}

impl<M: RawMutex, const N: usize> Write for WriteGuard<'_, M, N> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let written = self.channel.pipe.write(buf).await;
        Ok(written)
    }
}

pub async fn log_task<M: RawMutex, const BUF: usize>(
    endpoint: impl Into<embassy_net::IpEndpoint>,
    net_up: &Signal<M, ()>,
    mut messages: &Channel<M, BUF>,
    log_up: &Signal<M, bool>,
    stack: embassy_net::Stack<'_>,
) -> ! {
    let mut rx_buf = [0; 128];
    let mut tx_buf = [0; BUF];

    net_up.wait().await;

    let mut sock = tcp::TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
    sock.set_keep_alive(Some(Duration::from_secs(10)));
    sock.set_timeout(Some(Duration::from_secs(10)));

    let endpoint = endpoint.into();
    loop {
        'connection: {
            if sock.connect(endpoint).await.is_err() {
                break 'connection;
            }
            log_up.signal(true);
            let mut message = [0; BUF];
            loop {
                let Ok(len) = read_with_timeout(
                    &mut messages,
                    &mut message,
                    Duration::from_millis(100),
                )
                .await;
                if sock.write_all(&message[..len]).await.is_err() {
                    log_up.signal(false);
                    break 'connection;
                }
            }
        }
        sock.abort();
        _ = sock.flush().await;
        Timer::after_secs(10).await;
    }
}

async fn read_with_timeout<'a, R: Read>(
    reader: &'a mut R,
    buf: &'a mut [u8],
    timeout: Duration,
) -> Result<usize, R::Error> {
    let mut read = 0;
    async {
        while read < buf.len() {
            read += reader.read(&mut buf[read..]).await?;
        }
        Ok(())
    }
    .with_timeout(timeout)
    .await
    .unwrap_or(Ok(()))?;
    Ok(read)
}
