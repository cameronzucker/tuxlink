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
use std::collections::HashMap;
use std::future::Future;
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
                self.data.lock().unwrap().insert(key, listing.clone());
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
}
