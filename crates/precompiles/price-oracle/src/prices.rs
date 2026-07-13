use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use bytes::Bytes;
use parking_lot::RwLock;

use crate::{FeedKey, PriceReports, B256};

static ORACLE_STORE: LazyLock<RwLock<OracleStore>> =
    LazyLock::new(|| RwLock::new(OracleStore::default()));

#[derive(Debug, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    Stale,
    Unexpected,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RegisterOutcome {
    pub evicted: usize,
    pub feeds_diverged: bool,
}

struct SourceFeeds {
    provider_id: B256,
    feeds: BTreeSet<B256>,
}

#[derive(Default)]
struct OracleStore {
    reports: BTreeMap<FeedKey, (Bytes, u64)>,
    source_feeds: BTreeMap<String, SourceFeeds>,
    allowed_feeds: BTreeMap<B256, BTreeSet<B256>>,
}

impl OracleStore {
    fn snapshot(&self) -> PriceReports {
        PriceReports(
            self.reports
                .iter()
                .map(|(key, (payload, _))| (*key, payload.clone()))
                .collect(),
        )
    }

    fn insert_if_newer(
        &mut self,
        provider_id: B256,
        feed_id: B256,
        payload: Vec<u8>,
        payload_time_ms: u64,
    ) -> InsertOutcome {
        let allowed = self
            .allowed_feeds
            .get(&provider_id)
            .is_some_and(|feeds| feeds.contains(&feed_id));
        if !allowed {
            return InsertOutcome::Unexpected;
        }
        let key = FeedKey::new(provider_id, feed_id);
        if let Some((_, existing_time)) = self.reports.get(&key) {
            if *existing_time >= payload_time_ms {
                return InsertOutcome::Stale;
            }
        }
        self.reports.insert(key, (Bytes::from(payload), payload_time_ms));
        InsertOutcome::Inserted
    }

    fn register(
        &mut self,
        source_name: &str,
        provider_id: B256,
        feeds: Vec<B256>,
    ) -> RegisterOutcome {
        self.source_feeds.insert(
            source_name.to_owned(),
            SourceFeeds {
                provider_id,
                feeds: feeds.into_iter().collect(),
            },
        );
        self.recompute_allowed_feeds(provider_id);
        let evicted = self.retain_allowed_feeds();
        RegisterOutcome {
            evicted,
            feeds_diverged: self.feeds_diverged(provider_id),
        }
    }

    fn remove_source(&mut self, source_name: &str) -> usize {
        let Some(removed) = self.source_feeds.remove(source_name) else {
            return 0;
        };
        self.recompute_allowed_feeds(removed.provider_id);
        self.retain_allowed_feeds()
    }

    fn recompute_allowed_feeds(&mut self, provider_id: B256) {
        let union: BTreeSet<B256> = self
            .source_feeds
            .values()
            .filter(|entry| entry.provider_id == provider_id)
            .flat_map(|entry| entry.feeds.iter().copied())
            .collect();
        if union.is_empty() {
            self.allowed_feeds.remove(&provider_id);
        } else {
            self.allowed_feeds.insert(provider_id, union);
        }
    }

    fn retain_allowed_feeds(&mut self) -> usize {
        let keep: BTreeSet<FeedKey> = self
            .allowed_feeds
            .iter()
            .flat_map(|(provider_id, feeds)| {
                feeds
                    .iter()
                    .map(|feed_id| FeedKey::new(*provider_id, *feed_id))
            })
            .collect();
        let before = self.reports.len();
        self.reports.retain(|key, _| keep.contains(key));
        before - self.reports.len()
    }

    fn feeds_diverged(&self, provider_id: B256) -> bool {
        let mut sets = self
            .source_feeds
            .values()
            .filter(|entry| entry.provider_id == provider_id)
            .map(|entry| &entry.feeds);
        let Some(first) = sets.next() else {
            return false;
        };
        sets.any(|set| set != first)
    }
}

pub fn snapshot_prices() -> PriceReports {
    ORACLE_STORE.read().snapshot()
}

pub fn insert_if_newer(
    provider_id: B256,
    feed_id: B256,
    payload: Vec<u8>,
    payload_time_ms: u64,
) -> InsertOutcome {
    ORACLE_STORE
        .write()
        .insert_if_newer(provider_id, feed_id, payload, payload_time_ms)
}

pub fn register_feeds(source_name: &str, provider_id: B256, feeds: Vec<B256>) -> RegisterOutcome {
    ORACLE_STORE
        .write()
        .register(source_name, provider_id, feeds)
}

pub fn remove_source(source_name: &str) -> usize {
    ORACLE_STORE.write().remove_source(source_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(byte: u8) -> B256 {
        B256::repeat_byte(byte)
    }

    fn feed(byte: u8) -> B256 {
        B256::repeat_byte(byte)
    }

    #[test]
    fn rejects_unknown_feeds() {
        let mut store = OracleStore::default();
        store.register("chainlink-1", provider(0x01), vec![feed(0xaa)]);
        assert_eq!(
            store.insert_if_newer(provider(0x01), feed(0xbb), vec![1], 100),
            InsertOutcome::Unexpected
        );
        assert!(store.reports.is_empty());
    }

    #[test]
    fn keeps_newest() {
        let mut store = OracleStore::default();
        store.register("chainlink-1", provider(0x01), vec![feed(0xaa)]);
        assert_eq!(
            store.insert_if_newer(provider(0x01), feed(0xaa), vec![1], 100),
            InsertOutcome::Inserted
        );
        assert_eq!(
            store.insert_if_newer(provider(0x01), feed(0xaa), vec![2], 90),
            InsertOutcome::Stale
        );
        assert_eq!(
            store.insert_if_newer(provider(0x01), feed(0xaa), vec![3], 100),
            InsertOutcome::Stale
        );
        assert_eq!(
            store.insert_if_newer(provider(0x01), feed(0xaa), vec![4], 110),
            InsertOutcome::Inserted
        );
        let snapshot = store.snapshot();
        assert_eq!(
            snapshot
                .get(&FeedKey::new(provider(0x01), feed(0xaa)))
                .unwrap(),
            &Bytes::from(vec![4])
        );
    }

    #[test]
    fn unions_replica_feeds() {
        let mut store = OracleStore::default();
        let out = store.register("chainlink-1", provider(0x01), vec![feed(0xaa), feed(0xbb)]);
        assert!(!out.feeds_diverged);
        let out = store.register("chainlink-2", provider(0x01), vec![feed(0xaa)]);
        assert!(out.feeds_diverged);
        assert_eq!(
            store.insert_if_newer(provider(0x01), feed(0xbb), vec![1], 100),
            InsertOutcome::Inserted
        );
    }

    #[test]
    fn evicts_when_all_replicas_drop() {
        let mut store = OracleStore::default();
        store.register("chainlink-1", provider(0x01), vec![feed(0xaa), feed(0xbb)]);
        store.register("chainlink-2", provider(0x01), vec![feed(0xaa), feed(0xbb)]);
        store.insert_if_newer(provider(0x01), feed(0xbb), vec![1], 100);

        let out = store.register("chainlink-1", provider(0x01), vec![feed(0xaa)]);
        assert_eq!(out.evicted, 0);
        assert!(store
            .reports
            .contains_key(&FeedKey::new(provider(0x01), feed(0xbb))));

        let out = store.register("chainlink-2", provider(0x01), vec![feed(0xaa)]);
        assert_eq!(out.evicted, 1);
        assert!(!store
            .reports
            .contains_key(&FeedKey::new(provider(0x01), feed(0xbb))));
    }

    #[test]
    fn remove_source_evicts() {
        let mut store = OracleStore::default();
        store.register("chainlink-1", provider(0x01), vec![feed(0xaa)]);
        store.insert_if_newer(provider(0x01), feed(0xaa), vec![1], 100);
        assert_eq!(store.remove_source("chainlink-1"), 1);
        assert!(store.reports.is_empty());
        assert!(store.allowed_feeds.is_empty());
    }
}
