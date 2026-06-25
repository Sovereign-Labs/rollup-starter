use alloy_primitives::B256;
use bytes::Bytes;

use crate::types::{FeedKey, PriceReports};

pub fn lookup_feed_report(
    reports: &PriceReports,
    provider_id: B256,
    feed_id: B256,
) -> Option<&Bytes> {
    reports.get(&FeedKey::new(provider_id, feed_id))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::LazyLock;

    use alloy_primitives::keccak256;

    use super::*;

    static PROVIDER_ID: LazyLock<B256> = LazyLock::new(|| keccak256("chainlink"));

    fn feed_id(suffix: u8) -> B256 {
        let mut bytes = [0u8; 32];
        bytes[1] = 0x03;
        bytes[31] = suffix;
        B256::from(bytes)
    }

    fn reports_with(entries: &[(B256, B256, &[u8])]) -> PriceReports {
        let map = entries
            .iter()
            .map(|(p, f, payload)| (FeedKey::new(*p, *f), Bytes::copy_from_slice(payload)))
            .collect::<BTreeMap<_, _>>();
        PriceReports(map)
    }

    #[test]
    fn returns_payload_on_hit() {
        let payload = b"opaque-provider-update";
        let reports = reports_with(&[(*PROVIDER_ID, feed_id(1), payload)]);

        let got = lookup_feed_report(&reports, *PROVIDER_ID, feed_id(1)).unwrap();
        assert_eq!(got.as_ref(), payload.as_slice());
    }

    #[test]
    fn misses_on_absent_provider() {
        let reports = reports_with(&[(*PROVIDER_ID, feed_id(1), b"present")]);
        assert!(lookup_feed_report(&reports, B256::repeat_byte(0x99), feed_id(1)).is_none());
    }

    #[test]
    fn misses_on_absent_feed() {
        let reports = reports_with(&[(*PROVIDER_ID, feed_id(1), b"present")]);
        assert!(lookup_feed_report(&reports, *PROVIDER_ID, feed_id(2)).is_none());
    }
}
