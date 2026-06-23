use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

use bytes::Bytes;
use price_oracle::{FeedKey, SerializedPriceUpdates, B256};

static PRICE_SOURCE: LazyLock<Mutex<SerializedPriceUpdates>> =
    LazyLock::new(|| Mutex::new(SerializedPriceUpdates(BTreeMap::new())));

pub fn snapshot_prices() -> SerializedPriceUpdates {
    PRICE_SOURCE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub fn insert(provider_id: B256, feed_id: B256, payload: Vec<u8>) {
    PRICE_SOURCE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .0
        .insert(FeedKey::new(provider_id, feed_id), Bytes::from(payload));
}
