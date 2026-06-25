//! Integration with the per-transaction sequencing data and scratchpad.

use std::collections::BTreeSet;

use borsh::BorshDeserialize;
use bytes::Bytes;
use sov_modules_api::{Context, Spec};

use crate::types::{FeedKey, PriceReports, UsedFeedKeys};

pub fn prune_unused(mut data: PriceReports, scratchpad: Option<Bytes>) -> PriceReports {
    let Some(scratchpad) = scratchpad else {
        return PriceReports::default();
    };
    let used = match UsedFeedKeys::try_from_slice(&scratchpad) {
        Ok(used) => used,
        Err(err) => {
            tracing::error!(%err, "sequencing scratchpad is malformed, publishing full sequencing data");
            return data;
        }
    };
    let keep: BTreeSet<FeedKey> = used.0.into_iter().collect();
    data.retain_keys(&keep);
    data
}

pub(crate) fn record_used_feed_key<S: Spec>(
    context: &Context<S>,
    key: FeedKey,
) -> Result<(), std::io::Error> {
    context.sequencing_scratchpad().with_value(|slot| {
        let mut used = match slot.as_deref() {
            Some(bytes) => UsedFeedKeys::try_from_slice(bytes)?,
            None => UsedFeedKeys::default(),
        };
        used.0.push(key);
        let bytes = borsh::to_vec(&used).expect("in-memory borsh serialization is infallible");
        *slot = Some(Bytes::from(bytes));
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use alloy_primitives::B256;

    use super::*;

    fn feed_key(suffix: u8) -> FeedKey {
        FeedKey::new(B256::repeat_byte(0xc1), B256::repeat_byte(suffix))
    }

    fn prices(keys: &[FeedKey]) -> PriceReports {
        PriceReports(
            keys.iter()
                .map(|k| (*k, Bytes::from_static(b"payload")))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn scratchpad(used: &[FeedKey]) -> Bytes {
        Bytes::from(borsh::to_vec(&UsedFeedKeys(used.to_vec())).unwrap())
    }

    #[test]
    fn none_scratchpad_drops_all_feeds() {
        let pruned = prune_unused(prices(&[feed_key(1), feed_key(2)]), None);
        assert_eq!(pruned, PriceReports::default());
    }

    #[test]
    fn malformed_scratchpad_keeps_full_data() {
        let data = prices(&[feed_key(1), feed_key(2)]);
        let pruned = prune_unused(data.clone(), Some(Bytes::from(vec![0xff, 0xff, 0xff])));
        assert_eq!(pruned, data);
    }

    #[test]
    fn prunes_to_used_subset() {
        let kept = feed_key(1);
        let pruned = prune_unused(
            prices(&[feed_key(1), feed_key(2), feed_key(3)]),
            Some(scratchpad(&[kept])),
        );
        assert_eq!(pruned.0.len(), 1);
        assert!(pruned.get(&kept).is_some());
    }

    #[test]
    fn empty_used_set_drops_all_feeds() {
        let pruned = prune_unused(prices(&[feed_key(1), feed_key(2)]), Some(scratchpad(&[])));
        assert_eq!(pruned, PriceReports::default());
    }
}
