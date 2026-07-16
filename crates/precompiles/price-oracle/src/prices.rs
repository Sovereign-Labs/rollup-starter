use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bytes::Bytes;
use parking_lot::RwLock;

use crate::{FeedKey, PriceReports, B256};

static ORACLE_STORE: LazyLock<RwLock<OracleStore>> =
    LazyLock::new(|| RwLock::new(OracleStore::default()));

#[derive(Debug, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    Duplicate,
    Conflict,
    Stale,
    Unexpected,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RegisterOutcome {
    pub evicted: usize,
    pub feed_set_conflicts: Vec<B256>,
}

struct SourceFeeds {
    provider_id: B256,
    feeds: BTreeSet<B256>,
}

struct Report {
    payload: Bytes,
    source_time_ms: u64,
    updated: Instant,
}

#[derive(Default)]
struct OracleStore {
    reports: BTreeMap<FeedKey, Report>,
    source_feeds: BTreeMap<String, SourceFeeds>,
    allowed_feeds: BTreeMap<B256, BTreeSet<B256>>,
}

impl OracleStore {
    fn snapshot(&self) -> PriceReports {
        PriceReports(
            self.reports
                .iter()
                .map(|(key, report)| (*key, report.payload.clone()))
                .collect(),
        )
    }

    fn insert_if_newer(
        &mut self,
        provider_id: B256,
        feed_id: B256,
        payload: Vec<u8>,
        source_time_ms: u64,
    ) -> InsertOutcome {
        let allowed = self
            .allowed_feeds
            .get(&provider_id)
            .is_some_and(|feeds| feeds.contains(&feed_id));
        if !allowed {
            return InsertOutcome::Unexpected;
        }
        let key = FeedKey::new(provider_id, feed_id);
        let now = Instant::now();
        if let Some(existing) = self.reports.get_mut(&key) {
            if existing.source_time_ms > source_time_ms {
                return InsertOutcome::Stale;
            }
            let tied = existing.source_time_ms == source_time_ms;
            if tied && existing.payload.as_ref() == payload.as_slice() {
                existing.updated = now;
                return InsertOutcome::Duplicate;
            }
            *existing = Report {
                payload: Bytes::from(payload),
                source_time_ms,
                updated: now,
            };
            return if tied {
                InsertOutcome::Conflict
            } else {
                InsertOutcome::Inserted
            };
        }
        self.reports.insert(
            key,
            Report {
                payload: Bytes::from(payload),
                source_time_ms,
                updated: now,
            },
        );
        InsertOutcome::Inserted
    }

    fn evict_expired(&mut self, now: Instant, ttl: Duration) -> Vec<FeedKey> {
        let expired: Vec<FeedKey> = self
            .reports
            .iter()
            .filter(|(_, report)| now.duration_since(report.updated) >= ttl)
            .map(|(key, _)| *key)
            .collect();
        for key in &expired {
            self.reports.remove(key);
        }
        expired
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
            feed_set_conflicts: self.feed_set_conflicts(provider_id),
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

    fn feed_set_conflicts(&self, provider_id: B256) -> Vec<B256> {
        let mut sets = self
            .source_feeds
            .values()
            .filter(|entry| entry.provider_id == provider_id)
            .map(|entry| &entry.feeds);
        let Some(first) = sets.next() else {
            return Vec::new();
        };
        let mut union = first.clone();
        let mut intersection = first.clone();
        for set in sets {
            union.extend(set.iter().copied());
            intersection.retain(|feed| set.contains(feed));
        }
        union.difference(&intersection).copied().collect()
    }
}

pub fn snapshot_prices() -> PriceReports {
    ORACLE_STORE.read().snapshot()
}

pub fn insert_if_newer(
    provider_id: B256,
    feed_id: B256,
    payload: Vec<u8>,
    source_time_ms: u64,
) -> InsertOutcome {
    ORACLE_STORE
        .write()
        .insert_if_newer(provider_id, feed_id, payload, source_time_ms)
}

pub fn register_feeds(source_name: &str, provider_id: B256, feeds: Vec<B256>) -> RegisterOutcome {
    ORACLE_STORE
        .write()
        .register(source_name, provider_id, feeds)
}

pub fn remove_source(source_name: &str) -> usize {
    ORACLE_STORE.write().remove_source(source_name)
}

pub fn evict_expired(ttl: Duration) -> Vec<FeedKey> {
    ORACLE_STORE.write().evict_expired(Instant::now(), ttl)
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
    fn equal_timestamp_conflict_overwrites() {
        let mut store = OracleStore::default();
        store.register("chainlink-1", provider(0x01), vec![feed(0xaa)]);
        store.insert_if_newer(provider(0x01), feed(0xaa), vec![1], 100);
        assert_eq!(
            store.insert_if_newer(provider(0x01), feed(0xaa), vec![2], 100),
            InsertOutcome::Conflict
        );
        let snapshot = store.snapshot();
        assert_eq!(
            snapshot
                .get(&FeedKey::new(provider(0x01), feed(0xaa)))
                .unwrap(),
            &Bytes::from(vec![2])
        );
    }

    #[test]
    fn equal_timestamp_duplicate_is_skipped() {
        let mut store = OracleStore::default();
        store.register("chainlink-1", provider(0x01), vec![feed(0xaa)]);
        store.insert_if_newer(provider(0x01), feed(0xaa), vec![1], 100);
        assert_eq!(
            store.insert_if_newer(provider(0x01), feed(0xaa), vec![1], 100),
            InsertOutcome::Duplicate
        );
    }

    #[test]
    fn evicts_expired_reports() {
        let mut store = OracleStore::default();
        store.register("chainlink-1", provider(0x01), vec![feed(0xaa)]);
        store.insert_if_newer(provider(0x01), feed(0xaa), vec![1], 100);
        let now = Instant::now();
        let ttl = Duration::from_secs(300);
        assert!(store.evict_expired(now, ttl).is_empty());
        let evicted = store.evict_expired(now + Duration::from_secs(600), ttl);
        assert_eq!(evicted, vec![FeedKey::new(provider(0x01), feed(0xaa))]);
        assert!(store.reports.is_empty());
    }

    #[test]
    fn duplicate_refreshes_expiry() {
        let mut store = OracleStore::default();
        store.register("chainlink-1", provider(0x01), vec![feed(0xaa)]);
        store.insert_if_newer(provider(0x01), feed(0xaa), vec![1], 100);
        let key = FeedKey::new(provider(0x01), feed(0xaa));
        store.reports.get_mut(&key).unwrap().updated -= Duration::from_secs(600);
        store.insert_if_newer(provider(0x01), feed(0xaa), vec![1], 100);
        assert!(store
            .evict_expired(Instant::now(), Duration::from_secs(300))
            .is_empty());
    }

    #[test]
    fn unions_replica_feeds() {
        let mut store = OracleStore::default();
        let out = store.register("chainlink-1", provider(0x01), vec![feed(0xaa), feed(0xbb)]);
        assert!(out.feed_set_conflicts.is_empty());
        let out = store.register("chainlink-2", provider(0x01), vec![feed(0xaa)]);
        assert_eq!(out.feed_set_conflicts, vec![feed(0xbb)]);
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
