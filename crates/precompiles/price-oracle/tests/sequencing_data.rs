use std::collections::BTreeMap;
use std::collections::BTreeSet;

use alloy_primitives::{keccak256, B256};
use borsh::BorshDeserialize;
use price_oracle::{decode_feed_request, lookup_feed_update, FeedKey, SerializedPriceUpdates};

fn feed_id(suffix: u8) -> B256 {
    let mut bytes = [0u8; 32];
    bytes[1] = 0x03;
    bytes[31] = suffix;
    B256::from(bytes)
}

fn request(provider_id: B256, feed_id: B256) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(provider_id.as_slice());
    bytes.extend_from_slice(feed_id.as_slice());
    bytes
}

fn updates_with(entries: &[(FeedKey, &[u8])]) -> SerializedPriceUpdates {
    let map: BTreeMap<FeedKey, Vec<u8>> = entries.iter().map(|(k, p)| (*k, p.to_vec())).collect();
    SerializedPriceUpdates(map)
}

#[test]
fn calldata_and_sequencing_data_round_trip() {
    let key = FeedKey::new(keccak256("chainlink"), feed_id(1));
    let payload = b"signed-update-bytes".to_vec();
    let updates = updates_with(&[(key, &payload)]);

    let bytes = request(key.provider_id, key.feed_id);
    let (p, f) = decode_feed_request(&bytes).unwrap();
    assert_eq!(p, key.provider_id);
    assert_eq!(f, key.feed_id);

    let got = lookup_feed_update(&updates, p, f).unwrap();
    assert_eq!(got, &payload);
}

#[test]
fn missing_provider_is_a_miss() {
    let key = FeedKey::new(B256::repeat_byte(0x99), feed_id(1));
    let updates = updates_with(&[(key, b"present")]);
    assert!(lookup_feed_update(&updates, keccak256("chainlink"), feed_id(1)).is_none());
}

#[test]
fn retain_keys_drops_unused_entries() {
    let kept = FeedKey::new(keccak256("chainlink"), feed_id(1));
    let dropped = FeedKey::new(keccak256("chainlink"), feed_id(2));
    let mut updates = updates_with(&[(kept, b"keep-me"), (dropped, b"drop-me")]);

    let keep: BTreeSet<_> = [kept].into_iter().collect();
    updates.retain_keys(&keep);

    assert_eq!(updates.0.len(), 1);
    assert!(updates.get(&kept).is_some());
    assert!(updates.get(&dropped).is_none());
}

#[test]
fn retain_keys_with_empty_set_yields_empty_map() {
    let key = FeedKey::new(keccak256("chainlink"), feed_id(1));
    let mut updates = updates_with(&[(key, b"payload")]);

    updates.retain_keys(&BTreeSet::new());
    assert!(updates.0.is_empty());
}

#[test]
fn retain_keys_ignores_unknown_keep_entries() {
    let present = FeedKey::new(keccak256("chainlink"), feed_id(1));
    let absent = FeedKey::new(keccak256("chainlink"), feed_id(99));
    let mut updates = updates_with(&[(present, b"present")]);

    let keep: BTreeSet<_> = [present, absent].into_iter().collect();
    updates.retain_keys(&keep);

    assert_eq!(updates.0.len(), 1);
    assert!(updates.get(&present).is_some());
}

#[test]
fn sequencing_data_borsh_round_trip_is_canonical() {
    let mut entries = BTreeMap::new();
    entries.insert(
        FeedKey::new(keccak256("chainlink"), feed_id(2)),
        b"second".to_vec(),
    );
    entries.insert(
        FeedKey::new(keccak256("chainlink"), feed_id(1)),
        b"first".to_vec(),
    );
    let updates = SerializedPriceUpdates(entries);

    let bytes = borsh::to_vec(&updates).expect("borsh encode");
    let decoded = SerializedPriceUpdates::try_from_slice(&bytes).expect("borsh decode");
    assert_eq!(decoded, updates);

    let bytes_again = borsh::to_vec(&decoded).expect("borsh re-encode");
    assert_eq!(bytes_again, bytes);
}
