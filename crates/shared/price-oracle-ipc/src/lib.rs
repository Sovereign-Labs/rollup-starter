pub mod codec;
pub mod error;
pub mod transport;
pub mod types;

pub use codec::{
    read_frame, read_frame_with_timeout, write_frame, write_frame_with_timeout, MAX_FRAME_LEN,
};
pub use error::IpcError;
pub use transport::{
    bind, connect, Backoff, BoundListener, Endpoint, OracleListener, OracleStream,
    DEFAULT_CONNECT_TIMEOUT,
};
pub use types::{FeedKey, OracleFrame, PROTOCOL_VERSION};

pub use alloy_primitives::B256;
