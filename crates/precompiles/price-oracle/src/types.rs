use std::collections::{BTreeMap, BTreeSet};

use alloy_primitives::{Address, B256};
use borsh::{BorshDeserialize, BorshSerialize};
use bytes::Bytes;
use sequencing_registry::PrecompileSequencing;

use crate::precompile::PRICE_ORACLE_PRECOMPILE_ADDRESS;

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

/// Latest opaque signed payload per feed. Payloads are Bytes so cloning the map
/// for each transaction only bumps refcounts instead of copying large blobs.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SerializedPriceUpdates(pub BTreeMap<FeedKey, Bytes>);

impl SerializedPriceUpdates {
    pub fn get(&self, key: &FeedKey) -> Option<&Bytes> {
        self.0.get(key)
    }

    pub fn retain_keys(&mut self, keep: &BTreeSet<FeedKey>) {
        self.0.retain(|k, _| keep.contains(k));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct UsedFeedKeys(pub Vec<FeedKey>);

pub struct PriceOracleSequencing;

impl PrecompileSequencing for PriceOracleSequencing {
    const ADDRESS: Address = PRICE_ORACLE_PRECOMPILE_ADDRESS;
    type Data = SerializedPriceUpdates;
    type Used = UsedFeedKeys;

    fn prune(mut data: Self::Data, used: Self::Used) -> Self::Data {
        let keep: BTreeSet<FeedKey> = used.0.into_iter().collect();
        data.retain_keys(&keep);
        data
    }

    fn is_empty(data: &Self::Data) -> bool {
        data.0.is_empty()
    }
}
