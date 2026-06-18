use std::collections::BTreeMap;

use alloy_primitives::B256;
use borsh::{BorshDeserialize, BorshSerialize};
use sov_modules_api::capabilities::SequencingDataTrait;
use sov_modules_api::HDTimestamp;

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

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SerializedPriceUpdates(pub BTreeMap<FeedKey, Vec<u8>>);

impl SerializedPriceUpdates {
    pub fn get(&self, key: &FeedKey) -> Option<&Vec<u8>> {
        self.0.get(key)
    }

    pub fn retain_keys(&mut self, keep: &std::collections::BTreeSet<FeedKey>) {
        self.0.retain(|k, _| keep.contains(k));
    }
}

impl SequencingDataTrait for SerializedPriceUpdates {
    fn get_maybe_timestamp(self) -> Option<HDTimestamp> {
        None
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct UsedFeedKeys(pub Vec<FeedKey>);
