use alloy_primitives::B256;

use crate::types::{FeedKey, SerializedPriceUpdates};

pub fn lookup_feed_update(
    updates: &SerializedPriceUpdates,
    provider_id: B256,
    feed_id: B256,
) -> Option<&Vec<u8>> {
    updates.get(&FeedKey::new(provider_id, feed_id))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use alloy_primitives::keccak256;

    use super::*;

    fn feed_id(suffix: u8) -> B256 {
        let mut bytes = [0u8; 32];
        bytes[31] = suffix;
        B256::from(bytes)
    }

    fn updates_with(entries: &[(B256, B256, &[u8])]) -> SerializedPriceUpdates {
        let map = entries
            .iter()
            .map(|(p, f, payload)| (FeedKey::new(*p, *f), payload.to_vec()))
            .collect::<BTreeMap<_, _>>();
        SerializedPriceUpdates(map)
    }

    #[test]
    fn lookup_returns_payload_for_present_provider_and_feed() {
        let payload = b"opaque-provider-update".to_vec();
        let provider = B256::repeat_byte(0x99);
        let updates = updates_with(&[(provider, feed_id(1), &payload)]);

        let got = lookup_feed_update(&updates, provider, feed_id(1)).unwrap();
        assert_eq!(got, &payload);
    }

    #[test]
    fn lookup_misses_when_provider_absent() {
        let updates = updates_with(&[(B256::repeat_byte(0x99), feed_id(1), b"present")]);
        assert!(lookup_feed_update(&updates, keccak256("chainlink"), feed_id(1)).is_none());
    }

    #[test]
    fn lookup_misses_when_feed_absent_for_present_provider() {
        let provider = keccak256("chainlink");
        let updates = updates_with(&[(provider, feed_id(1), b"present")]);
        assert!(lookup_feed_update(&updates, provider, feed_id(2)).is_none());
    }
}
