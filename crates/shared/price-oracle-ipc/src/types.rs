use alloy_primitives::B256;
use borsh::{BorshDeserialize, BorshSerialize};

pub const PROTOCOL_VERSION: u16 = 2;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub struct FeedKey {
    pub provider_id: B256,
    pub feed_id: B256,
}

impl FeedKey {
    pub const fn new(provider_id: B256, feed_id: B256) -> Self {
        Self {
            provider_id,
            feed_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum OracleFrame {
    Hello {
        protocol_version: u16,
        provider_id: B256,
        feeds: Vec<B256>,
        heartbeat_interval_sec: u32,
    },
    PriceUpdate {
        provider_id: B256,
        feed_id: B256,
        payload: Vec<u8>,
        delivery_time_ms: u64,
        source_time_ms: u64,
    },
    Heartbeat {
        send_time_ms: u64,
    },
}
