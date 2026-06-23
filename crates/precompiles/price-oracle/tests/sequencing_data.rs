use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use alloy_primitives::{keccak256, B256};
use borsh::BorshDeserialize;
use bytes::Bytes;
use price_oracle::{FeedKey, SerializedPriceUpdates};

static PROVIDER_ID: LazyLock<B256> = LazyLock::new(|| keccak256("chainlink"));

fn feed_id(suffix: u8) -> B256 {
    let mut bytes = [0u8; 32];
    bytes[1] = 0x03;
    bytes[31] = suffix;
    B256::from(bytes)
}

fn updates_with(entries: &[(FeedKey, &[u8])]) -> SerializedPriceUpdates {
    let map: BTreeMap<FeedKey, Bytes> = entries
        .iter()
        .map(|(k, p)| (*k, Bytes::copy_from_slice(p)))
        .collect();
    SerializedPriceUpdates(map)
}

#[test]
fn retain_keys_drops_unused_entries() {
    let kept = FeedKey::new(*PROVIDER_ID, feed_id(1));
    let dropped = FeedKey::new(*PROVIDER_ID, feed_id(2));
    let mut updates = updates_with(&[(kept, b"keep-me"), (dropped, b"drop-me")]);

    updates.retain_keys(&[kept].into_iter().collect());

    assert_eq!(updates.0.len(), 1);
    assert!(updates.get(&kept).is_some());
    assert!(updates.get(&dropped).is_none());
}

#[test]
fn retain_keys_empty_set_clears_map() {
    let key = FeedKey::new(*PROVIDER_ID, feed_id(1));
    let mut updates = updates_with(&[(key, b"payload")]);

    updates.retain_keys(&BTreeSet::new());
    assert!(updates.0.is_empty());
}

#[test]
fn retain_keys_ignores_unknown_keys() {
    let present = FeedKey::new(*PROVIDER_ID, feed_id(1));
    let absent = FeedKey::new(*PROVIDER_ID, feed_id(99));
    let mut updates = updates_with(&[(present, b"present")]);

    updates.retain_keys(&[present, absent].into_iter().collect());

    assert_eq!(updates.0.len(), 1);
    assert!(updates.get(&present).is_some());
}

#[test]
fn borsh_round_trip_is_canonical() {
    let mut entries = BTreeMap::new();
    entries.insert(
        FeedKey::new(*PROVIDER_ID, feed_id(2)),
        Bytes::from_static(b"second"),
    );
    entries.insert(
        FeedKey::new(*PROVIDER_ID, feed_id(1)),
        Bytes::from_static(b"first"),
    );
    let updates = SerializedPriceUpdates(entries);

    let bytes = borsh::to_vec(&updates).expect("borsh encode");
    let decoded = SerializedPriceUpdates::try_from_slice(&bytes).expect("borsh decode");
    assert_eq!(decoded, updates);
    assert_eq!(borsh::to_vec(&decoded).expect("borsh re-encode"), bytes);
}
