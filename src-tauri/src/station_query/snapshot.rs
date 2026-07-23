//! A normalized, TTL'd station-population snapshot + its in-memory store.
//!
//! A snapshot pins the curated population a base query produced, keyed by an
//! app-minted [`SnapshotId`]. `explore` / `lookup` narrow *against* a snapshot so
//! population counts stay **stable** between calls (the agent can trust that
//! "1,391 matched" doesn't drift under it) and so a follow-up filter can only
//! ever narrow — never widen — the scope (enforced by
//! [`StationFilters::is_narrowing_of`]). Snapshots expire (TTL): an expired /
//! unknown id is a typed, retryable error telling the agent to re-issue the base
//! query, not a silent empty result.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tuxlink_mcp_core::ports::GatewayDto;
use tuxlink_mcp_core::station_query::{SnapshotId, StationFilters};

/// The curated population a base query produced, pinned for narrowing.
#[derive(Debug, Clone, PartialEq)]
pub struct StationSnapshot {
    pub id: SnapshotId,
    pub fetched_at_ms: u64,
    pub expires_at_ms: u64,
    /// The operator grid distances/bearings were computed from (`None` when
    /// unresolved). Provenance the response echoes.
    pub operator_grid: Option<String>,
    /// The filters this snapshot was built under. A follow-up query's filters
    /// must [`narrow`](StationFilters::is_narrowing_of) these — a widening filter
    /// is rejected so a snapshot id can never be used to escape its own scope.
    pub base_filters: StationFilters,
    /// The curated population (full curated set for the base query). The engine
    /// applies additive narrowing filters on top; it never adds rows.
    pub gateways: Vec<GatewayDto>,
}

/// Why a snapshot lookup failed. Both are retryable by re-issuing the base query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotError {
    /// No snapshot with that id (never created, or already evicted).
    Unknown,
    /// The snapshot existed but its TTL elapsed.
    Expired,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::Unknown => write!(
                f,
                "unknown snapshot id (never created or evicted); re-issue the base query"
            ),
            SnapshotError::Expired => write!(
                f,
                "snapshot expired; re-issue the base query to get a fresh snapshot"
            ),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// An in-memory, TTL'd store of [`StationSnapshot`]s. Thread-safe; intended to be
/// held as Tauri managed state (an `Arc<SnapshotStore>`) alongside the caches.
#[derive(Debug)]
pub struct SnapshotStore {
    inner: Mutex<HashMap<String, StationSnapshot>>,
    counter: AtomicU64,
    ttl_ms: u64,
}

impl SnapshotStore {
    /// A store whose snapshots live `ttl_ms` after their `fetched_at_ms`.
    #[must_use]
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(0),
            ttl_ms,
        }
    }

    /// Mint + store a snapshot over `gateways`, returning a clone. `now_ms` is
    /// injected (not read from a clock) so callers and tests stay deterministic.
    pub fn create(
        &self,
        gateways: Vec<GatewayDto>,
        operator_grid: Option<String>,
        base_filters: StationFilters,
        now_ms: u64,
    ) -> StationSnapshot {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let id = SnapshotId::from_truncated(&format!("sq_{now_ms:x}{n:x}"));
        let snapshot = StationSnapshot {
            id: id.clone(),
            fetched_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(self.ttl_ms),
            operator_grid,
            base_filters,
            gateways,
        };
        let mut map = self.inner.lock().expect("snapshot store mutex poisoned");
        map.insert(id.as_str().to_string(), snapshot.clone());
        snapshot
    }

    /// Fetch a snapshot by id, honoring its TTL against `now_ms`. An expired
    /// snapshot is evicted and reported as [`SnapshotError::Expired`].
    pub fn get(&self, id: &str, now_ms: u64) -> Result<StationSnapshot, SnapshotError> {
        let mut map = self.inner.lock().expect("snapshot store mutex poisoned");
        match map.get(id) {
            None => Err(SnapshotError::Unknown),
            Some(s) if now_ms > s.expires_at_ms => {
                map.remove(id);
                Err(SnapshotError::Expired)
            }
            Some(s) => Ok(s.clone()),
        }
    }

    /// Number of live (not-yet-evicted) snapshots. Test/observability helper.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().expect("snapshot store mutex poisoned").len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway(callsign: &str) -> GatewayDto {
        GatewayDto {
            mode: tuxlink_mcp_core::ports::StationModeDto::VaraHf,
            channel: "7104.0 VARA HF".into(),
            callsign: callsign.into(),
            grid: Some("FN31".into()),
            frequencies_khz: vec![7104.0],
            channels: Vec::new(),
            antenna: None,
            distance_km: Some(100.0),
            distance_mi: Some(62.1),
            bearing_deg: Some(90.0),
            ft8_corroborated: None,
        }
    }

    #[test]
    fn create_then_get_round_trips() {
        let store = SnapshotStore::new(60_000);
        let snap = store.create(
            vec![gateway("W1ABC"), gateway("K2XYZ")],
            Some("DM43".into()),
            StationFilters::default(),
            1_000,
        );
        let got = store.get(snap.id.as_str(), 1_500).unwrap();
        assert_eq!(got.gateways.len(), 2);
        assert_eq!(got.operator_grid.as_deref(), Some("DM43"));
        assert_eq!(got.expires_at_ms, 61_000);
    }

    #[test]
    fn unknown_id_is_typed() {
        let store = SnapshotStore::new(60_000);
        assert_eq!(store.get("sq_nope", 1_000), Err(SnapshotError::Unknown));
    }

    #[test]
    fn expiry_is_typed_and_evicts() {
        let store = SnapshotStore::new(60_000);
        let snap = store.create(vec![gateway("W1ABC")], None, StationFilters::default(), 1_000);
        // Just past expiry.
        assert_eq!(
            store.get(snap.id.as_str(), 61_001),
            Err(SnapshotError::Expired)
        );
        // Evicted — a second get is Unknown, not Expired.
        assert_eq!(store.get(snap.id.as_str(), 61_001), Err(SnapshotError::Unknown));
    }

    #[test]
    fn counts_are_stable_across_reads() {
        // The pinned population never changes, so any count derived from it is
        // stable between calls — the property `explore`/`lookup` rely on.
        let store = SnapshotStore::new(60_000);
        let snap = store.create(
            (0..311).map(|i| gateway(&format!("W{i}AA"))).collect(),
            None,
            StationFilters::default(),
            1_000,
        );
        let a = store.get(snap.id.as_str(), 2_000).unwrap();
        let b = store.get(snap.id.as_str(), 3_000).unwrap();
        assert_eq!(a.gateways.len(), 311);
        assert_eq!(a.gateways.len(), b.gateways.len());
    }

    #[test]
    fn distinct_ids_per_create() {
        let store = SnapshotStore::new(60_000);
        let a = store.create(vec![], None, StationFilters::default(), 1_000);
        let b = store.create(vec![], None, StationFilters::default(), 1_000);
        assert_ne!(a.id.as_str(), b.id.as_str());
        assert_eq!(store.len(), 2);
    }
}
