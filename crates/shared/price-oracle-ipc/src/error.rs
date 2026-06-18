#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("connection closed by peer")]
    Closed,

    #[error("frame too large: {0} bytes")]
    FrameTooLarge(usize),

    #[error("connect timed out")]
    ConnectTimeout,

    #[error("read timed out")]
    ReadTimeout,

    #[error("write timed out")]
    WriteTimeout,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("frame codec error: {0}")]
    Codec(std::io::Error),
}
