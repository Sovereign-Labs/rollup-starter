use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};

use crate::error::IpcError;

pub const DEFAULT_BACKOFF_MIN: Duration = Duration::from_secs(1);
pub const DEFAULT_BACKOFF_MAX: Duration = Duration::from_secs(30);
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Unix(PathBuf),
    Tcp(String),
}

impl Endpoint {
    pub fn unix(path: impl Into<PathBuf>) -> Self {
        Self::Unix(path.into())
    }

    pub fn tcp(address: impl Into<String>) -> Self {
        Self::Tcp(address.into())
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Endpoint::Unix(path) => write!(f, "unix:{}", path.display()),
            Endpoint::Tcp(address) => write!(f, "tcp:{address}"),
        }
    }
}

#[derive(Debug)]
pub enum OracleStream {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl AsyncRead for OracleStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            OracleStream::Unix(s) => Pin::new(s).poll_read(cx, buf),
            OracleStream::Tcp(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for OracleStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            OracleStream::Unix(s) => Pin::new(s).poll_write(cx, buf),
            OracleStream::Tcp(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            OracleStream::Unix(s) => Pin::new(s).poll_flush(cx),
            OracleStream::Tcp(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            OracleStream::Unix(s) => Pin::new(s).poll_shutdown(cx),
            OracleStream::Tcp(s) => Pin::new(s).poll_shutdown(cx),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            OracleStream::Unix(s) => Pin::new(s).poll_write_vectored(cx, bufs),
            OracleStream::Tcp(s) => Pin::new(s).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            OracleStream::Unix(s) => s.is_write_vectored(),
            OracleStream::Tcp(s) => s.is_write_vectored(),
        }
    }
}

pub async fn connect(endpoint: &Endpoint) -> Result<OracleStream, IpcError> {
    match endpoint {
        Endpoint::Unix(path) => {
            let stream = tokio::time::timeout(DEFAULT_CONNECT_TIMEOUT, UnixStream::connect(path))
                .await
                .map_err(|_| IpcError::ConnectTimeout)??;
            Ok(OracleStream::Unix(stream))
        }
        Endpoint::Tcp(address) => {
            let stream = tokio::time::timeout(DEFAULT_CONNECT_TIMEOUT, TcpStream::connect(address))
                .await
                .map_err(|_| IpcError::ConnectTimeout)??;
            // Price updates are small and latency-sensitive; disable Nagle.
            stream.set_nodelay(true)?;
            apply_tcp_keepalive(&stream)?;
            Ok(OracleStream::Tcp(stream))
        }
    }
}

pub enum OracleListener {
    Unix(UnixListener),
    Tcp(TcpListener),
}

impl OracleListener {
    pub async fn accept(&self) -> Result<OracleStream, IpcError> {
        match self {
            OracleListener::Unix(listener) => {
                let (stream, _addr) = listener.accept().await?;
                Ok(OracleStream::Unix(stream))
            }
            OracleListener::Tcp(listener) => {
                let (stream, _addr) = listener.accept().await?;
                stream.set_nodelay(true)?;
                apply_tcp_keepalive(&stream)?;
                Ok(OracleStream::Tcp(stream))
            }
        }
    }

    pub fn local_addr(&self) -> Option<std::net::SocketAddr> {
        match self {
            OracleListener::Unix(_) => None,
            OracleListener::Tcp(listener) => listener.local_addr().ok(),
        }
    }
}

pub async fn bind(endpoint: &Endpoint) -> Result<OracleListener, IpcError> {
    match endpoint {
        Endpoint::Unix(path) => {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            Ok(OracleListener::Unix(UnixListener::bind(path)?))
        }
        Endpoint::Tcp(address) => Ok(OracleListener::Tcp(TcpListener::bind(address).await?)),
    }
}

pub struct BoundListener {
    listener: OracleListener,
    endpoint: Endpoint,
}

impl BoundListener {
    pub async fn bind(endpoint: Endpoint) -> Result<Self, IpcError> {
        let listener = bind(&endpoint).await?;
        let endpoint = match (&endpoint, listener.local_addr()) {
            (Endpoint::Tcp(_), Some(addr)) => Endpoint::Tcp(addr.to_string()),
            _ => endpoint,
        };
        Ok(Self { listener, endpoint })
    }

    pub async fn accept(&self) -> Result<OracleStream, IpcError> {
        self.listener.accept().await
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl Drop for BoundListener {
    fn drop(&mut self) {
        if let Endpoint::Unix(path) = &self.endpoint {
            let _ = std::fs::remove_file(path);
        }
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
    async fn unix_bind_then_connect_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oracle.sock");
        let listener = BoundListener::bind(Endpoint::unix(path.clone()))
            .await
            .unwrap();

        let endpoint = listener.endpoint().clone();
        let client = tokio::spawn(async move { connect(&endpoint).await.is_ok() });
        let accepted = listener.accept().await.is_ok();
        assert!(accepted);
        assert!(client.await.unwrap());
    }

    #[tokio::test]
    async fn unix_bind_clears_stale_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oracle.sock");
        let first = BoundListener::bind(Endpoint::unix(path.clone()))
            .await
            .unwrap();
        drop(first);
        std::fs::write(&path, b"stale").ok();
        assert!(BoundListener::bind(Endpoint::unix(path.clone()))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn tcp_bind_then_connect_roundtrip() {
        let listener = BoundListener::bind(Endpoint::tcp("127.0.0.1:0"))
            .await
            .unwrap();
        let endpoint = listener.endpoint().clone();
        assert!(matches!(endpoint, Endpoint::Tcp(_)));

        let client = tokio::spawn(async move { connect(&endpoint).await.is_ok() });
        let accepted = listener.accept().await.is_ok();
        assert!(accepted);
        assert!(client.await.unwrap());
    }

    #[test]
    fn endpoint_display() {
        assert_eq!(
            Endpoint::tcp("127.0.0.1:9802").to_string(),
            "tcp:127.0.0.1:9802"
        );
        assert_eq!(
            Endpoint::unix("/run/x.sock").to_string(),
            "unix:/run/x.sock"
        );
    }

    #[tokio::test]
    async fn dropping_bound_listener_removes_unix_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oracle.sock");
        let listener = BoundListener::bind(Endpoint::unix(path.clone()))
            .await
            .unwrap();
        assert!(path.exists());
        drop(listener);
        assert!(!path.exists());
    }
}
