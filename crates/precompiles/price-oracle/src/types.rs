use std::collections::{BTreeMap, BTreeSet};

use alloy_primitives::B256;
use borsh::{BorshDeserialize, BorshSerialize};
use bytes::Bytes;
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

/// Latest opaque signed report per feed.
/// Reports are bytes so cloning the map for each transaction only bumps refcounts
/// instead of copying large blobs.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PriceReports(pub BTreeMap<FeedKey, Bytes>);

impl PriceReports {
    pub fn get(&self, key: &FeedKey) -> Option<&Bytes> {
        self.0.get(key)
    }

    pub fn retain_keys(&mut self, keep: &BTreeSet<FeedKey>) {
        self.0.retain(|k, _| keep.contains(k));
    }
}

impl SequencingDataTrait for PriceReports {
    // Carries no sequencer timestamp hence this is always None.
    fn get_maybe_timestamp(self) -> Option<HDTimestamp> {
        None
    }
}

/// Feed keys a transaction actually read.
/// Recorded in the sequencing scratchpad during execution.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct UsedFeedKeys(pub BTreeSet<FeedKey>);
