//! Polite-client cache for the station-list poll.
//!
//! Guarantees (design §"Polite-client requirements (hard)"):
//! - **TTL**: a fresh entry (age < `ttl_ms`) is served without a network hit. For v1 the TTL
//!   gate IS the minimum-refetch floor (TTL 30 min ≥ the design's 15 min min-refetch), so there
//!   is no separate min-refetch knob — that would be an unimplemented fiction.
//! - **Per-key coalescing**: concurrent callers for the SAME key serialize on a per-key async
//!   mutex; the follower finds the leader's freshly-stored result and returns it WITHOUT a second
//!   fetch. The global map lock is never held across the network `await`, so a slow fetch for one
//!   mode never blocks a cached read (or an independent fetch) for another mode.
//! - **Stale-on-error**: if a refetch fails but a prior (now-stale) entry exists, the stale entry
//!   is returned (carrying its original `fetched_at_ms` so the UI can stamp "as of <time>").
//!
//! No new crate (fork / minimal-dep ethos): `std::sync::Mutex` for the tiny map critical sections
//! (never held across `await`) + a registry of per-key `tokio::sync::Mutex` for single-flight.

use crate::catalog::stations::{ListingMode, StationListing};
use crate::catalog::stations_disk;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    fn now_millis(&self) -> u64;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    pub mode: ListingMode,
    pub service_codes: String,
    pub history_hours: u32,
}

pub struct StationsCache {
    ttl_ms: u64,
    /// Minimum interval between network ATTEMPTS for one key. TTL alone doesn't bound
    /// outage retries: after the TTL lapses, a failing endpoint leaves the entry stale on
    /// every call, so without this floor each caller would re-hammer. Honors the polite
    /// per-endpoint floor (design §"respect a minimum refetch interval").
    min_refetch_ms: u64,
    clock: Arc<dyn Clock>,
    /// Data store. Locked only for brief get/insert — never across `await`.
    data: Mutex<HashMap<CacheKey, StationListing>>,
    /// Last network-attempt time per key (negative-cache timestamp), distinct from the
    /// entry's `fetched_at_ms` (which stays at the last SUCCESS for the UI "as of" stamp).
    attempts: Mutex<HashMap<CacheKey, u64>>,
    /// Per-key single-flight locks (registry locked only briefly to clone an Arc).
    locks: Mutex<HashMap<CacheKey, Arc<tokio::sync::Mutex<()>>>>,
    /// Optional path for disk persistence. `None` → pure in-memory (default); `Some` →
    /// snapshot written after every successful fetch so cold restarts can serve stale data.
    persist_path: Option<PathBuf>,
}

impl StationsCache {
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

    /// Build a cache backed by disk persistence at `path`.
    ///
    /// Seeds both `data` and `attempts` from the existing file (if any) so that a cold
    /// restart can immediately serve the last-known-good entries via the stale-on-error
    /// path. If the file is missing or unparseable, both maps start empty — a load error
    /// is never fatal (mirrors `stations_disk::load`'s quarantine behaviour).
    ///
    /// After every successful network fetch the cache atomically snapshots both maps to
    /// `path` via [`stations_disk::save`]. A write error is logged but never propagated —
    /// a cache that cannot write to disk continues serving in-memory results correctly.
    pub fn new_persistent(
        ttl_ms: u64,
        min_refetch_ms: u64,
        clock: Arc<dyn Clock>,
        path: PathBuf,
    ) -> Self {
        let (loaded_data, loaded_attempts) = stations_disk::load(&path);
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

    /// Snapshot both maps to disk if a `persist_path` is configured.
    ///
    /// IMPORTANT — no `std::sync::Mutex` guard is held across this call:
    /// the snapshot is taken under a brief lock scope, the guards are dropped,
    /// and then the synchronous `stations_disk::save` (pure `std::fs`) runs
    /// with no locks held. This is safe to call from any point in `get_or_fetch`
    /// after the relevant guards have been released.
    fn persist(&self) {
        let path = match &self.persist_path {
            Some(p) => p,
            None => return,
        };
        // Take snapshots under their respective locks, then release immediately.
        let data_snapshot = self.data.lock().unwrap().clone();
        let attempts_snapshot = self.attempts.lock().unwrap().clone();
        // Locks are dropped here — no Mutex guard crosses the save call.
        if let Err(e) = stations_disk::save(path, &data_snapshot, &attempts_snapshot) {
            eprintln!("stations_cache: failed to persist to {}: {e}", path.display());
        }
    }

    fn fresh_clone(&self, key: &CacheKey, now: u64) -> Option<StationListing> {
        let data = self.data.lock().unwrap();
        let entry = data.get(key)?;
        let age = now.saturating_sub(entry.fetched_at_ms.unwrap_or(0));
        if age < self.ttl_ms {
            Some(entry.clone())
        } else {
            None
        }
    }

    fn key_lock(&self, key: &CacheKey) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().unwrap();
        locks.entry(key.clone()).or_default().clone()
    }

    /// Insert a listing directly, bypassing the network (tuxlink-xrbw). Used to
    /// ingest a radio-delivered station list parsed from a received `PUB_*`
    /// "Update Via Radio" reply: it lands under `key` exactly as a fetch would, so
    /// the finder's next `get_or_fetch` for that key serves it fresh-from-cache
    /// with no internet. Stamps `fetched_at_ms = now` and persists to disk.
    /// Mirrors the post-fetch insert path in `get_or_fetch` (lock dropped before
    /// `persist`, which must not run while the data mutex is held).
    pub fn insert(&self, key: CacheKey, mut listing: StationListing) {
        listing.fetched_at_ms = Some(self.clock.now_millis());
        self.data.lock().unwrap().insert(key, listing);
        self.persist();
    }

    /// Serve fresh-from-cache, else fetch (coalescing same-key concurrent callers), else stale.
    pub async fn get_or_fetch<F, E>(&self, key: CacheKey, fetch: F) -> Result<StationListing, E>
    where
        F: Future<Output = Result<StationListing, E>>,
    {
        let now = self.clock.now_millis();
        if let Some(fresh) = self.fresh_clone(&key, now) {
            return Ok(fresh);
        }

        // Serialize same-key callers; different keys hold different locks (no cross-key block).
        let key_lock = self.key_lock(&key);
        let _guard = key_lock.lock().await;

        // Re-check under the per-key lock: a leader may have just filled it (coalesce → no fetch).
        let now = self.clock.now_millis();
        if let Some(fresh) = self.fresh_clone(&key, now) {
            return Ok(fresh);
        }

        // Polite min-refetch floor: if we attempted recently AND have a stale entry to serve,
        // skip the network this round (prevents hammering a failing endpoint whose entry stays
        // past-TTL). A first-ever fetch (no stale entry) always proceeds.
        if let Some(&last) = self.attempts.lock().unwrap().get(&key) {
            if now.saturating_sub(last) < self.min_refetch_ms {
                if let Some(stale) = self.data.lock().unwrap().get(&key).cloned() {
                    return Ok(stale);
                }
            }
        }
        self.attempts.lock().unwrap().insert(key.clone(), now);

        match fetch.await {
            Ok(mut listing) => {
                listing.fetched_at_ms = Some(self.clock.now_millis());
                // Insert into the data map; the Mutex guard is dropped at the end of this
                // block — persist() must not be called while holding it.
                self.data.lock().unwrap().insert(key, listing.clone());
                // Guard is released here. Now safe to snapshot + save with no lock held.
                self.persist();
                Ok(listing)
            }
            Err(e) => {
                // Stale-on-error: serve the last good entry (with its original SUCCESS timestamp).
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
    use crate::catalog::stations::ListingMode;
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

    fn key(mode: ListingMode) -> CacheKey {
        CacheKey { mode, service_codes: "PUBLIC".into(), history_hours: 168 }
    }

    fn listing(mode: ListingMode, raw: &str) -> StationListing {
        StationListing { mode, title: None, gateways: vec![], raw: raw.into(), parsed_ok: true, fetched_at_ms: None }
    }

    #[tokio::test]
    async fn second_call_within_ttl_does_not_refetch() {
        let clock = Arc::new(MockClock::new(0));
        let cache = StationsCache::new(60_000, 0, clock.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let mk = || {
            let c = calls.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(listing(ListingMode::VaraHf, "v"))
            }
        };
        let first = cache.get_or_fetch(key(ListingMode::VaraHf), mk()).await.unwrap();
        assert_eq!(first.fetched_at_ms, Some(0)); // cache stamps the timestamp
        clock.advance(30_000);
        cache.get_or_fetch(key(ListingMode::VaraHf), mk()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn call_after_ttl_refetches() {
        let clock = Arc::new(MockClock::new(0));
        let cache = StationsCache::new(60_000, 0, clock.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let mk = || {
            let c = calls.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(listing(ListingMode::VaraHf, "v"))
            }
        };
        cache.get_or_fetch(key(ListingMode::VaraHf), mk()).await.unwrap();
        clock.advance(60_001);
        cache.get_or_fetch(key(ListingMode::VaraHf), mk()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_same_key_callers_coalesce_to_one_fetch() {
        let clock = Arc::new(MockClock::new(0));
        let cache = Arc::new(StationsCache::new(60_000, 0, clock));
        let calls = Arc::new(AtomicUsize::new(0));
        let slow_fetch = || {
            let c = calls.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                Ok::<_, String>(listing(ListingMode::VaraHf, "v"))
            }
        };
        let a = { let cache = cache.clone(); let f = slow_fetch(); tokio::spawn(async move { cache.get_or_fetch(key(ListingMode::VaraHf), f).await }) };
        let b = { let cache = cache.clone(); let f = slow_fetch(); tokio::spawn(async move { cache.get_or_fetch(key(ListingMode::VaraHf), f).await }) };
        a.await.unwrap().unwrap();
        b.await.unwrap().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "concurrent same-key callers must coalesce");
    }

    #[tokio::test]
    async fn distinct_key_not_blocked_by_other_keys_inflight_fetch() {
        // Mode-A fetch blocks on a gate; Mode-B must complete without waiting for A.
        let clock = Arc::new(MockClock::new(0));
        let cache = Arc::new(StationsCache::new(60_000, 0, clock));
        let gate = Arc::new(tokio::sync::Notify::new());
        let a = {
            let cache = cache.clone();
            let gate = gate.clone();
            tokio::spawn(async move {
                let f = async move { gate.notified().await; Ok::<_, String>(listing(ListingMode::ArdopHf, "a")) };
                cache.get_or_fetch(key(ListingMode::ArdopHf), f).await
            })
        };
        // B should resolve promptly even while A is gated.
        let b = cache
            .get_or_fetch(key(ListingMode::Pactor), async { Ok::<_, String>(listing(ListingMode::Pactor, "b")) })
            .await
            .unwrap();
        assert_eq!(b.raw, "b");
        gate.notify_one(); // release A
        a.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn error_after_success_serves_stale() {
        let clock = Arc::new(MockClock::new(0));
        let cache = StationsCache::new(60_000, 0, clock.clone());
        cache
            .get_or_fetch(key(ListingMode::VaraHf), async { Ok::<_, String>(listing(ListingMode::VaraHf, "good")) })
            .await
            .unwrap();
        clock.advance(60_001); // force a refetch attempt
        let stale = cache
            .get_or_fetch(key(ListingMode::VaraHf), async { Err::<StationListing, String>("network down".into()) })
            .await
            .unwrap();
        assert_eq!(stale.raw, "good");
        assert_eq!(stale.fetched_at_ms, Some(0)); // original timestamp, for "as of" stamp
    }

    #[tokio::test]
    async fn error_with_empty_cache_propagates_err() {
        let clock = Arc::new(MockClock::new(0));
        let cache = StationsCache::new(60_000, 0, clock);
        let res = cache
            .get_or_fetch(key(ListingMode::VaraHf), async { Err::<StationListing, String>("network down".into()) })
            .await;
        assert_eq!(res.unwrap_err(), "network down");
    }

    // ---- disk-persistence tests (Task 2 — RED phase) --------------------------------

    /// `cold_load_serves_last_known_good`: a NEW `new_persistent` cache loaded from a
    /// pre-existing file with one good entry must return that entry via the stale-on-error
    /// path when the fetch closure returns `Err`. This is the core U2 value: offline cold
    /// start serves disk data.
    #[tokio::test]
    async fn cold_load_serves_last_known_good() {
        use crate::catalog::stations_disk;
        use std::collections::HashMap;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("station_cache.json");

        // Seed the disk file with one good entry stamped at T=1000.
        let seed_key = key(ListingMode::VaraHf);
        let mut seed_listing = listing(ListingMode::VaraHf, "seed-raw");
        seed_listing.parsed_ok = true;
        seed_listing.fetched_at_ms = Some(1_000);

        let mut seed_data = HashMap::new();
        seed_data.insert(seed_key.clone(), seed_listing.clone());
        let seed_attempts: HashMap<CacheKey, u64> = HashMap::new();
        stations_disk::save(&path, &seed_data, &seed_attempts).expect("seed save must succeed");

        // Build a NEW cache from the same path; clock is far ahead (TTL=60s, clock=200s)
        // so the entry is stale. The fetch returns Err → stale-on-error must serve the disk entry.
        let clock = Arc::new(MockClock::new(200_000));
        let cache = StationsCache::new_persistent(60_000, 0, clock, path.clone());

        let result = cache
            .get_or_fetch(
                seed_key.clone(),
                async { Err::<StationListing, String>("network is down".into()) },
            )
            .await
            .expect("stale-on-error must succeed");

        assert_eq!(result.raw, "seed-raw", "must serve the disk entry's raw content");
        assert_eq!(
            result.fetched_at_ms,
            Some(1_000),
            "must preserve the original fetched_at_ms (the 'as of' stamp)"
        );
    }

    /// `good_fetch_persists_to_disk`: after a successful fetch on a `new_persistent` cache,
    /// the file must exist and a fresh `stations_disk::load` of it must return the entry.
    #[tokio::test]
    async fn good_fetch_persists_to_disk() {
        use crate::catalog::stations_disk;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("station_cache.json");

        let clock = Arc::new(MockClock::new(0));
        let cache = StationsCache::new_persistent(60_000, 0, clock, path.clone());

        cache
            .get_or_fetch(
                key(ListingMode::ArdopHf),
                async {
                    let mut l = listing(ListingMode::ArdopHf, "persisted-raw");
                    l.parsed_ok = true;
                    Ok::<_, String>(l)
                },
            )
            .await
            .unwrap();

        assert!(path.exists(), "disk file must exist after a successful fetch");

        let (loaded_data, _) = stations_disk::load(&path);
        let loaded = loaded_data.get(&key(ListingMode::ArdopHf)).expect("entry must be in file");
        assert_eq!(loaded.raw, "persisted-raw");
    }

    /// `failed_first_fetch_does_not_persist`: if the first fetch fails and there is no prior
    /// entry, nothing must be written to disk (or the file has no entries).
    #[tokio::test]
    async fn failed_first_fetch_does_not_persist() {
        use crate::catalog::stations_disk;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("station_cache.json");

        let clock = Arc::new(MockClock::new(0));
        let cache = StationsCache::new_persistent(60_000, 0, clock, path.clone());

        let _ = cache
            .get_or_fetch(
                key(ListingMode::VaraHf),
                async { Err::<StationListing, String>("first-time failure".into()) },
            )
            .await;

        // Either the file was never written, or if written it contains no entries.
        if path.exists() {
            let (data, _) = stations_disk::load(&path);
            assert!(data.is_empty(), "a first-fetch error must not persist any entries to disk");
        }
        // else: file not written at all — also correct.
    }

    /// `in_memory_new_does_no_disk_io`: `new(...)` (no path); a good fetch completes
    /// and persist is a no-op (no file created anywhere in the temp dir).
    #[tokio::test]
    async fn in_memory_new_does_no_disk_io() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let clock = Arc::new(MockClock::new(0));
        // Plain new() — no persist_path.
        let cache = StationsCache::new(60_000, 0, clock);

        cache
            .get_or_fetch(
                key(ListingMode::VaraHf),
                async {
                    let mut l = listing(ListingMode::VaraHf, "in-memory-only");
                    l.parsed_ok = true;
                    Ok::<_, String>(l)
                },
            )
            .await
            .unwrap();

        // Assert no file was written anywhere in the temp dir (the dir is otherwise empty).
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            entries.is_empty(),
            "in-memory-only cache must not write any file; found: {:?}",
            entries.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );
    }

    // ---- pre-existing tests (unchanged) ------------------------------------------

    #[tokio::test]
    async fn min_refetch_floor_throttles_retries_during_outage() {
        let clock = Arc::new(MockClock::new(0));
        let cache = StationsCache::new(60_000, 30_000, clock.clone()); // TTL 60s, min-refetch 30s
        let calls = Arc::new(AtomicUsize::new(0));
        let failing = || {
            let c = calls.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<StationListing, String>("down".into())
            }
        };
        // Seed a good entry.
        cache
            .get_or_fetch(key(ListingMode::VaraHf), async { Ok::<_, String>(listing(ListingMode::VaraHf, "good")) })
            .await
            .unwrap();
        // Past TTL: a failing refetch (attempt #1) serves stale.
        clock.advance(60_001);
        assert_eq!(cache.get_or_fetch(key(ListingMode::VaraHf), failing()).await.unwrap().raw, "good");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Within the min-refetch window: must NOT hit the network again — serves stale, no new attempt.
        clock.advance(10_000);
        assert_eq!(cache.get_or_fetch(key(ListingMode::VaraHf), failing()).await.unwrap().raw, "good");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "within min-refetch must not re-hit the endpoint");
        // After the floor elapses: it retries.
        clock.advance(30_000);
        cache.get_or_fetch(key(ListingMode::VaraHf), failing()).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn inserted_listing_is_served_without_fetching() {
        // tuxlink-xrbw: a radio-delivered listing inserted directly must be served
        // fresh-from-cache so the finder shows it with no network round-trip.
        let clock = Arc::new(MockClock::new(1_000));
        let cache = StationsCache::new(60_000, 0, clock.clone());
        cache.insert(key(ListingMode::VaraHf), listing(ListingMode::VaraHf, "radio-delivered"));

        let calls = Arc::new(AtomicUsize::new(0));
        let mk = || {
            let c = calls.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(listing(ListingMode::VaraHf, "from-network"))
            }
        };
        let got = cache.get_or_fetch(key(ListingMode::VaraHf), mk()).await.unwrap();
        assert_eq!(got.raw, "radio-delivered"); // the ingested copy, not a fetch
        assert_eq!(got.fetched_at_ms, Some(1_000)); // stamped fresh at insert time
        assert_eq!(calls.load(Ordering::SeqCst), 0, "insert must satisfy the finder without a fetch");
    }
}
