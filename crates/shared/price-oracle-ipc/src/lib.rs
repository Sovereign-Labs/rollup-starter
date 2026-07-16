pub mod codec;
pub mod error;
pub mod transport;
pub mod types;

pub use codec::{
    read_frame, read_frame_with_timeout, write_frame, write_frame_with_timeout, MAX_FRAME_LEN,
};
pub use error::IpcError;
pub use transport::{
    bind, connect, Backoff, BoundListener, OracleListener, OracleStream, DEFAULT_CONNECT_TIMEOUT,
};
pub use types::{
    FeedKey, OracleFrame, FEEDS_MAX, HEARTBEAT_INTERVAL, PROTOCOL_VERSION, READ_DEADLINE,
};

pub use alloy_primitives::B256;
