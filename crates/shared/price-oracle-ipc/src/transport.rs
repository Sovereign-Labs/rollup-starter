use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};

use crate::error::IpcError;

pub const DEFAULT_BACKOFF_MIN: Duration = Duration::from_secs(1);
pub const DEFAULT_BACKOFF_MAX: Duration = Duration::from_secs(15);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_KEEPALIVE_IDLE: Duration = Duration::from_secs(15);
pub const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);
pub const DEFAULT_KEEPALIVE_RETRIES: u32 = 3;

fn apply_tcp_keepalive(stream: &TcpStream) -> Result<(), IpcError> {
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(DEFAULT_KEEPALIVE_IDLE)
        .with_interval(DEFAULT_KEEPALIVE_INTERVAL)
        .with_retries(DEFAULT_KEEPALIVE_RETRIES);
    socket2::SockRef::from(stream).set_tcp_keepalive(&keepalive)?;
    Ok(())
}

#[derive(Debug)]
pub struct OracleStream(TcpStream);

impl AsyncRead for OracleStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

impl AsyncWrite for OracleStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.0.is_write_vectored()
    }
}

pub async fn connect(address: &str) -> Result<OracleStream, IpcError> {
    let stream = tokio::time::timeout(DEFAULT_CONNECT_TIMEOUT, TcpStream::connect(address))
        .await
        .map_err(|_| IpcError::ConnectTimeout)??;
    stream.set_nodelay(true)?;
    apply_tcp_keepalive(&stream)?;
    Ok(OracleStream(stream))
}

pub struct OracleListener(TcpListener);

impl OracleListener {
    pub async fn accept(&self) -> Result<OracleStream, IpcError> {
        let (stream, _addr) = self.0.accept().await?;
        stream.set_nodelay(true)?;
        apply_tcp_keepalive(&stream)?;
        Ok(OracleStream(stream))
    }

    pub fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.0.local_addr().ok()
    }
}

pub async fn bind(address: &str) -> Result<OracleListener, IpcError> {
    Ok(OracleListener(TcpListener::bind(address).await?))
}

pub struct BoundListener {
    listener: OracleListener,
    address: String,
}

impl BoundListener {
    pub async fn bind(address: impl Into<String>) -> Result<Self, IpcError> {
        let requested = address.into();
        let listener = bind(&requested).await?;
        let address = listener
            .local_addr()
            .map(|addr| addr.to_string())
            .unwrap_or(requested);
        Ok(Self { listener, address })
    }

    pub async fn accept(&self) -> Result<OracleStream, IpcError> {
        self.listener.accept().await
    }

    pub fn address(&self) -> &str {
        &self.address
    }
}

#[derive(Debug, Clone)]
pub struct Backoff {
    current: Duration,
    min: Duration,
    max: Duration,
}

impl Backoff {
    pub fn new(min: Duration, max: Duration) -> Self {
        Self {
            current: min,
            min,
            max,
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = (self.current * 2).min(self.max);
        delay
    }

    pub fn reset(&mut self) {
        self.current = self.min;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(DEFAULT_BACKOFF_MIN, DEFAULT_BACKOFF_MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_then_saturates() {
        let mut b = Backoff::new(Duration::from_secs(1), Duration::from_secs(8));
        assert_eq!(b.next_delay(), Duration::from_secs(1));
        assert_eq!(b.next_delay(), Duration::from_secs(2));
        assert_eq!(b.next_delay(), Duration::from_secs(4));
        assert_eq!(b.next_delay(), Duration::from_secs(8));
        assert_eq!(b.next_delay(), Duration::from_secs(8));
        b.reset();
        assert_eq!(b.next_delay(), Duration::from_secs(1));
    }

    #[tokio::test]
    async fn tcp_bind_then_connect_roundtrip() {
        let listener = BoundListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.address().to_string();

        let client = tokio::spawn(async move { connect(&address).await.is_ok() });
        let accepted = listener.accept().await.is_ok();
        assert!(accepted);
        assert!(client.await.unwrap());
    }
}
