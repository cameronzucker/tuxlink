//! Polite-client cache for the Winlink gateway **channels** JSON API
//! (tuxlink-nkzng). The structural twin of [`super::stations_cache::StationsCache`]
//! with the cached value swapped from `StationListing` to [`ChannelsFeed`].
//!
//! Guarantees (same contract as the stations cache):
//! - **TTL 60 min**: a fresh entry is served without a network hit.
//! - **Per-key coalescing**: concurrent callers for the same key serialize on a
//!   per-key async mutex; the follower returns the leader's fresh result with no
//!   second fetch. The map lock is never held across the network `await`.
//! - **Min-refetch 15 min**: after the TTL lapses, a failing endpoint is retried
//!   at most once per 15 min while stale data exists (no re-hammering).
//! - **Stale-on-error**: a failed refetch serves the last good feed with its
//!   original `fetched_at_ms` (so the UI can stamp "as of <time>").
//!
//! The cache key is the operator's `service_codes` string (the only request
//! parameter that changes which gateways come back; the API key is a fixed access
//! token, not part of the key).
//!
//! Disk persistence at `channels-feed-cache.json` seeds the cache on cold start so
//! an offline launch can serve last-known-good channel bandwidth/frequency data.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::catalog::channels_api::ChannelsFeed;
use crate::catalog::stations_cache::Clock;

const SCHEMA: &str = "tuxlink-channels-feed-cache-v1";

/// A cached channels feed plus the wall-clock millis it was fetched at (the "as
/// of" stamp; also the TTL/stale basis).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFeed {
    pub feed: ChannelsFeed,
    pub fetched_at_ms: u64,
}

#[derive(Serialize, Deserialize)]
struct PersistedChannelsCache {
    schema: String,
    #[serde(default)]
    data: HashMap<String, CachedFeed>,
    #[serde(default)]
    attempts: HashMap<String, u64>,
}

/// Load the persisted channels cache. Missing file → empty; unparseable →
/// quarantine-log + empty (never panics, never deletes the file). Mirrors
/// `stations_disk::load`.
fn load(path: &Path) -> (HashMap<String, CachedFeed>, HashMap<String, u64>) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (HashMap::new(), HashMap::new())
        }
        Err(e) => {
            eprintln!(
                "channels_cache: failed to read {}: {e} - starting with empty cache",
                path.display()
            );
            return (HashMap::new(), HashMap::new());
        }
    };
    match serde_json::from_slice::<PersistedChannelsCache>(&bytes) {
        Ok(p) => (p.data, p.attempts),
        Err(e) => {
            eprintln!(
                "channels_cache: {} is unparseable, starting empty (original preserved): {e}",
                path.display()
            );
            (HashMap::new(), HashMap::new())
        }
    }
}

/// Atomically persist the cache (serialize → write `<path>.tmp` → rename).
/// Mirrors `stations_disk::save`.
fn save(
    path: &Path,
    data: &HashMap<String, CachedFeed>,
    attempts: &HashMap<String, u64>,
) -> std::io::Result<()> {
    let persisted = PersistedChannelsCache {
        schema: SCHEMA.to_string(),
        data: data.clone(),
        attempts: attempts.clone(),
    };
    let json = serde_json::to_string_pretty(&persisted)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "channels-feed-cache.json".to_string());
    let tmp = path.with_file_name(format!("{name}.tmp"));
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub struct ChannelsCache {
    ttl_ms: u64,
    min_refetch_ms: u64,
    clock: Arc<dyn Clock>,
    data: Mutex<HashMap<String, CachedFeed>>,
    attempts: Mutex<HashMap<String, u64>>,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    persist_path: Option<PathBuf>,
}

impl ChannelsCache {
    pub fn new(ttl_ms: u64, min_refetch_ms: u64, clock: Arc<dyn Clock>) -> Self {
        Self {
            ttl_ms,
            min_refetch_ms,
            clock,
            data: Mutex::new(HashMap::new()),
            attempts: Mutex::new(HashMap::new()),
            locks: Mutex::new(HashMap::new()),
            persist_path: None,
        }
    }

    /// Build a cache backed by disk persistence at `path`, seeding both maps from
    /// the existing file (if any) so a cold restart can serve last-known-good data.
    pub fn new_persistent(
        ttl_ms: u64,
        min_refetch_ms: u64,
        clock: Arc<dyn Clock>,
        path: PathBuf,
    ) -> Self {
        let (loaded_data, loaded_attempts) = load(&path);
        Self {
            ttl_ms,
            min_refetch_ms,
            clock,
            data: Mutex::new(loaded_data),
            attempts: Mutex::new(loaded_attempts),
            locks: Mutex::new(HashMap::new()),
            persist_path: Some(path),
        }
    }

    /// Snapshot both maps to disk if a `persist_path` is configured. No
    /// `std::sync::Mutex` guard is held across the synchronous `save`.
    fn persist(&self) {
        let path = match &self.persist_path {
            Some(p) => p,
            None => return,
        };
        let data_snapshot = self.data.lock().unwrap().clone();
        let attempts_snapshot = self.attempts.lock().unwrap().clone();
        if let Err(e) = save(path, &data_snapshot, &attempts_snapshot) {
            eprintln!("channels_cache: failed to persist to {}: {e}", path.display());
        }
    }

    fn fresh_clone(&self, key: &str, now: u64) -> Option<CachedFeed> {
        let data = self.data.lock().unwrap();
        let entry = data.get(key)?;
        let age = now.saturating_sub(entry.fetched_at_ms);
        if age < self.ttl_ms {
            Some(entry.clone())
        } else {
            None
        }
    }

    fn key_lock(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().unwrap();
        locks.entry(key.to_string()).or_default().clone()
    }

    /// Serve fresh-from-cache, else fetch (coalescing same-key concurrent
    /// callers), else stale. `fetch` yields the raw [`ChannelsFeed`]; the cache
    /// stamps `fetched_at_ms` from its clock and returns the wrapped [`CachedFeed`].
    pub async fn get_or_fetch<F, E>(&self, key: String, fetch: F) -> Result<CachedFeed, E>
    where
        F: Future<Output = Result<ChannelsFeed, E>>,
    {
        let now = self.clock.now_millis();
        if let Some(fresh) = self.fresh_clone(&key, now) {
            return Ok(fresh);
        }

        let key_lock = self.key_lock(&key);
        let _guard = key_lock.lock().await;

        // Re-check under the per-key lock (a leader may have just filled it).
        let now = self.clock.now_millis();
        if let Some(fresh) = self.fresh_clone(&key, now) {
            return Ok(fresh);
        }

        // Polite min-refetch floor: throttle retries against a failing endpoint
        // while a stale entry exists.
        if let Some(&last) = self.attempts.lock().unwrap().get(&key) {
            if now.saturating_sub(last) < self.min_refetch_ms {
                if let Some(stale) = self.data.lock().unwrap().get(&key).cloned() {
                    return Ok(stale);
                }
            }
        }
        self.attempts.lock().unwrap().insert(key.clone(), now);

        match fetch.await {
            Ok(feed) => {
                let cached = CachedFeed {
                    feed,
                    fetched_at_ms: self.clock.now_millis(),
                };
                self.data.lock().unwrap().insert(key, cached.clone());
                self.persist();
                Ok(cached)
            }
            Err(e) => {
                if let Some(stale) = self.data.lock().unwrap().get(&key).cloned() {
                    return Ok(stale);
                }
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    struct MockClock(AtomicU64);
    impl MockClock {
        fn new(t: u64) -> Self {
            Self(AtomicU64::new(t))
        }
        fn advance(&self, d: u64) {
            self.0.fetch_add(d, Ordering::SeqCst);
        }
    }
    impl Clock for MockClock {
        fn now_millis(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn feed_with(callsign: &str) -> ChannelsFeed {
        let mut f = ChannelsFeed::new();
        f.insert(callsign.to_string(), vec![]);
        f
    }

    #[tokio::test]
    async fn second_call_within_ttl_does_not_refetch() {
        let clock = Arc::new(MockClock::new(0));
        let cache = ChannelsCache::new(60 * 60 * 1000, 0, clock.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let mk = || {
            let c = calls.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(feed_with("W1AW"))
            }
        };
        let first = cache.get_or_fetch("PUBLIC".to_string(), mk()).await.unwrap();
        assert_eq!(first.fetched_at_ms, 0);
        assert!(first.feed.contains_key("W1AW"));
        clock.advance(30 * 60 * 1000);
        cache.get_or_fetch("PUBLIC".to_string(), mk()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "within TTL must not refetch");
    }

    #[tokio::test]
    async fn call_after_ttl_refetches() {
        let clock = Arc::new(MockClock::new(0));
        let cache = ChannelsCache::new(60 * 60 * 1000, 0, clock.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let mk = || {
            let c = calls.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(feed_with("W1AW"))
            }
        };
        cache.get_or_fetch("PUBLIC".to_string(), mk()).await.unwrap();
        clock.advance(60 * 60 * 1000 + 1);
        cache.get_or_fetch("PUBLIC".to_string(), mk()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn error_after_success_serves_stale_with_original_stamp() {
        let clock = Arc::new(MockClock::new(0));
        let cache = ChannelsCache::new(60 * 60 * 1000, 0, clock.clone());
        cache
            .get_or_fetch("PUBLIC".to_string(), async { Ok::<_, String>(feed_with("GOOD")) })
            .await
            .unwrap();
        clock.advance(60 * 60 * 1000 + 1); // force a refetch attempt
        let stale = cache
            .get_or_fetch("PUBLIC".to_string(), async {
                Err::<ChannelsFeed, String>("network down".into())
            })
            .await
            .unwrap();
        assert!(stale.feed.contains_key("GOOD"));
        assert_eq!(stale.fetched_at_ms, 0, "original success stamp preserved");
    }

    #[tokio::test]
    async fn error_with_empty_cache_propagates() {
        let clock = Arc::new(MockClock::new(0));
        let cache = ChannelsCache::new(60 * 60 * 1000, 0, clock);
        let res = cache
            .get_or_fetch("PUBLIC".to_string(), async {
                Err::<ChannelsFeed, String>("down".into())
            })
            .await;
        assert_eq!(res.unwrap_err(), "down");
    }

    #[tokio::test]
    async fn concurrent_same_key_callers_coalesce_to_one_fetch() {
        let clock = Arc::new(MockClock::new(0));
        let cache = Arc::new(ChannelsCache::new(60 * 60 * 1000, 0, clock));
        let calls = Arc::new(AtomicUsize::new(0));
        let slow = || {
            let c = calls.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                Ok::<_, String>(feed_with("W1AW"))
            }
        };
        let a = {
            let cache = cache.clone();
            let f = slow();
            tokio::spawn(async move { cache.get_or_fetch("PUBLIC".to_string(), f).await })
        };
        let b = {
            let cache = cache.clone();
            let f = slow();
            tokio::spawn(async move { cache.get_or_fetch("PUBLIC".to_string(), f).await })
        };
        a.await.unwrap().unwrap();
        b.await.unwrap().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn min_refetch_floor_throttles_retries_during_outage() {
        let clock = Arc::new(MockClock::new(0));
        let cache = ChannelsCache::new(60 * 60 * 1000, 15 * 60 * 1000, clock.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let failing = || {
            let c = calls.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<ChannelsFeed, String>("down".into())
            }
        };
        cache
            .get_or_fetch("PUBLIC".to_string(), async { Ok::<_, String>(feed_with("GOOD")) })
            .await
            .unwrap();
        clock.advance(60 * 60 * 1000 + 1);
        cache.get_or_fetch("PUBLIC".to_string(), failing()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        clock.advance(60 * 1000); // still within the 15-min floor
        cache.get_or_fetch("PUBLIC".to_string(), failing()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "within min-refetch must not re-hit");
        clock.advance(15 * 60 * 1000);
        cache.get_or_fetch("PUBLIC".to_string(), failing()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cold_load_serves_last_known_good() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("channels-feed-cache.json");

        // Seed the disk file with one good entry stamped at T=1000.
        let mut seed_data = HashMap::new();
        seed_data.insert(
            "PUBLIC".to_string(),
            CachedFeed { feed: feed_with("SEED"), fetched_at_ms: 1_000 },
        );
        save(&path, &seed_data, &HashMap::new()).expect("seed save");

        // Clock far ahead so the entry is stale; fetch errs → stale-on-error serves disk.
        let clock = Arc::new(MockClock::new(200_000));
        let cache = ChannelsCache::new_persistent(60 * 60 * 1000, 0, clock, path.clone());
        let got = cache
            .get_or_fetch("PUBLIC".to_string(), async {
                Err::<ChannelsFeed, String>("network is down".into())
            })
            .await
            .expect("stale-on-error must succeed");
        assert!(got.feed.contains_key("SEED"));
        assert_eq!(got.fetched_at_ms, 1_000);
    }

    #[tokio::test]
    async fn good_fetch_persists_to_disk() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("channels-feed-cache.json");
        let clock = Arc::new(MockClock::new(0));
        let cache = ChannelsCache::new_persistent(60 * 60 * 1000, 0, clock, path.clone());
        cache
            .get_or_fetch("PUBLIC".to_string(), async { Ok::<_, String>(feed_with("PERSISTED")) })
            .await
            .unwrap();
        assert!(path.exists());
        let (loaded, _) = load(&path);
        assert!(loaded.get("PUBLIC").unwrap().feed.contains_key("PERSISTED"));
    }

    #[tokio::test]
    async fn in_memory_new_does_no_disk_io() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let clock = Arc::new(MockClock::new(0));
        let cache = ChannelsCache::new(60 * 60 * 1000, 0, clock);
        cache
            .get_or_fetch("PUBLIC".to_string(), async { Ok::<_, String>(feed_with("X")) })
            .await
            .unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().filter_map(|e| e.ok()).collect();
        assert!(entries.is_empty(), "in-memory cache must not write a file");
    }
}
