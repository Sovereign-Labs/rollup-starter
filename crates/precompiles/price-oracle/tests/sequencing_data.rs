use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use alloy_primitives::{keccak256, B256};
use borsh::BorshDeserialize;
use bytes::Bytes;
use price_oracle::{FeedKey, PriceReports};

static PROVIDER_ID: LazyLock<B256> = LazyLock::new(|| keccak256("chainlink"));

fn feed_id(suffix: u8) -> B256 {
    let mut bytes = [0u8; 32];
    bytes[1] = 0x03;
    bytes[31] = suffix;
    B256::from(bytes)
}

fn reports_with(entries: &[(FeedKey, &[u8])]) -> PriceReports {
    let map: BTreeMap<FeedKey, Bytes> = entries
        .iter()
        .map(|(k, p)| (*k, Bytes::copy_from_slice(p)))
        .collect();
    PriceReports(map)
}

#[test]
fn retain_keys_drops_unused_entries() {
    let kept = FeedKey::new(*PROVIDER_ID, feed_id(1));
    let dropped = FeedKey::new(*PROVIDER_ID, feed_id(2));
    let mut reports = reports_with(&[(kept, b"keep-me"), (dropped, b"drop-me")]);

    reports.retain_keys(&[kept].into_iter().collect());

    assert_eq!(reports.0.len(), 1);
    assert!(reports.get(&kept).is_some());
    assert!(reports.get(&dropped).is_none());
}

#[test]
fn retain_keys_empty_set_clears_map() {
    let key = FeedKey::new(*PROVIDER_ID, feed_id(1));
    let mut reports = reports_with(&[(key, b"payload")]);

    reports.retain_keys(&BTreeSet::new());
    assert!(reports.0.is_empty());
}

#[test]
fn retain_keys_ignores_unknown_keys() {
    let present = FeedKey::new(*PROVIDER_ID, feed_id(1));
    let absent = FeedKey::new(*PROVIDER_ID, feed_id(99));
    let mut reports = reports_with(&[(present, b"present")]);

    reports.retain_keys(&[present, absent].into_iter().collect());

    assert_eq!(reports.0.len(), 1);
    assert!(reports.get(&present).is_some());
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
    let reports = PriceReports(entries);

    let bytes = borsh::to_vec(&reports).expect("borsh encode");
    let decoded = PriceReports::try_from_slice(&bytes).expect("borsh decode");
    assert_eq!(decoded, reports);
    assert_eq!(borsh::to_vec(&decoded).expect("borsh re-encode"), bytes);
}
