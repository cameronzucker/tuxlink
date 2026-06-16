//! SSN (smoothed sunspot number) source: a bundled forecast table.
//! Offline-first: the bundled table always yields a value; no network, no disk
//! writes in v1. A writable on-disk cache + opportunistic refresh is a deferred
//! follow-up (spec §12) — not implemented here, must never add a network precondition.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use super::PropagationError;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SsnForecast {
    pub monthly: BTreeMap<String, f64>,
}

/// Writable on-disk forecast path. Mirrors `prefs::prefs_path` — the runtime
/// forecast lives beside the operator's other prefs in the config dir, so an
/// internet/RF propagation update persists across restarts (tuxlink-ot71).
pub fn forecast_path(config_dir: &Path) -> PathBuf {
    config_dir.join("ssn-forecast.json")
}

impl SsnForecast {
    pub fn from_json(text: &str) -> Result<Self, PropagationError> {
        serde_json::from_str(text).map_err(|e| PropagationError::Ssn(e.to_string()))
    }

    /// Load the runtime forecast: prefer a writable on-disk forecast (written by
    /// a prior internet/RF update), else fall back to the bundled table. Never
    /// fails — a missing, empty, or corrupt writable file degrades silently to
    /// the bundled forecast (offline-first; an update must never be able to
    /// brick prediction). This is the runtime-mutable read side the
    /// "Update propagation data" feature writes through.
    pub fn load_writable_then_bundled(config_dir: &Path) -> Self {
        let path = forecast_path(config_dir);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(f) = Self::from_json(&text) {
                if !f.monthly.is_empty() {
                    return f;
                }
            }
            // Corrupt or empty writable file → ignore it, use bundled.
        }
        Self::from_json(BUNDLED_SSN_FORECAST).unwrap_or_default()
    }

    /// Persist this forecast to the writable path, atomically (write a temp file
    /// then rename) so a crash mid-write can't leave a half-written forecast that
    /// the next `load_writable_then_bundled` would reject.
    pub fn persist(&self, config_dir: &Path) -> Result<(), PropagationError> {
        let path = forecast_path(config_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| PropagationError::Ssn(e.to_string()))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// SSN for `year`-`month`; falls back to the nearest EARLIER month, else the
    /// last known value, else a conservative solar-minimum default (10.0).
    /// (BTreeMap range works because "YYYY-MM" zero-padded keys sort chronologically.)
    ///
    /// Fallback chain:
    /// 1. Exact key match → return it.
    /// 2. `range(..=key).next_back()` → nearest earlier entry.
    /// 3. `.iter().next_back()` → last (highest-key, most-future) entry.
    ///    NOTE: when the query is BEFORE every table entry, this returns the LAST/highest
    ///    entry, not the first/earliest. This is the plan's specified fallback; for a
    ///    solar-cycle-decline scenario the last entry is typically the lowest SSN,
    ///    which errs on the conservative side for propagation prediction.
    ///    A future operator could prefer `.iter().next()` (earliest) instead; document
    ///    that preference in spec §12 if changed.
    /// 4. Empty table → conservative solar-minimum default 10.0.
    ///
    /// // follow-up: when the writable on-disk cache is implemented (spec §12), this
    /// // method should prefer a cached entry over the bundled table when the cache
    /// // contains a more-recent SWPC forecast for the same month.
    pub fn ssn_for(&self, year: i32, month: u8) -> f64 {
        let key = format!("{year:04}-{month:02}");
        if let Some(v) = self.monthly.get(&key) {
            return *v;
        }
        self.monthly
            .range(..=key)
            .next_back()
            .or_else(|| self.monthly.iter().next_back())
            .map(|(_, v)| *v)
            .unwrap_or(10.0)
    }
}

/// The bundled SSN table shipped with the binary.
///
/// Contains a single verified anchor point (2026-06 = 100.0) captured during
/// the 2026-06-10 voacapl grounding run — NOT a fabricated forecast trend.
/// The fallback chain in `ssn_for` means any date resolves to this one real
/// value (stale-but-real beats invented). The operator extends this table with
/// authoritative SWPC monthly smoothed-SSN values before relying on
/// out-of-month predictions; a writable on-disk cache that accepts operator
/// updates is deferred to spec §12.
pub const BUNDLED_SSN_FORECAST: &str =
    include_str!("../../resources/propagation/ssn-forecast.json");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_forecast_parses() {
        let f = SsnForecast::from_json(BUNDLED_SSN_FORECAST).expect("bundled forecast should parse");
        assert!(!f.monthly.is_empty(), "bundled forecast monthly map must be non-empty");
    }

    #[test]
    fn bundled_forecast_has_anchor_month() {
        let f = SsnForecast::from_json(BUNDLED_SSN_FORECAST).expect("bundled forecast should parse");
        // The 2026-06 anchor value MUST be exactly 100.0 — this is the SSN used in the
        // captured grounding fixture. Any change here invalidates the fixture.
        let v = f.ssn_for(2026, 6);
        assert_eq!(v, 100.0, "anchor month 2026-06 must be exactly 100.0, got {v}");
    }

    #[test]
    fn exact_month_hit() {
        let f = SsnForecast::from_json(r#"{"monthly":{"2026-06":100.0}}"#)
            .expect("should parse");
        assert_eq!(f.ssn_for(2026, 6), 100.0);
    }

    #[test]
    fn falls_back_to_nearest_earlier_month() {
        // Table has 2026-01 and 2026-06 but NOT 2026-04.
        // Query for 2026-04 → should return nearest earlier = 2026-01's value (80.0).
        let f = SsnForecast::from_json(r#"{"monthly":{"2026-06":100.0,"2026-01":80.0}}"#)
            .expect("should parse");
        assert_eq!(f.ssn_for(2026, 4), 80.0,
            "2026-04 not in table; nearest earlier is 2026-01 = 80.0");
    }

    #[test]
    fn falls_back_to_last_when_query_before_table() {
        // Table starts at 2026-06; query is 2025-01 (before every key).
        // range(..="2025-01") returns None (nothing <= "2025-01" in the map).
        // Falls through to .iter().next_back() which returns the LAST/HIGHEST entry.
        // This is the plan's specified fallback chain; it returns the latest known value,
        // not the earliest. Rationale: during solar-cycle decline a later entry is
        // typically lower; using the last (lowest-in-series) errs on the conservative
        // side for propagation prediction, which is the safer failure mode.
        // NOTE: "last" in a BTreeMap means highest key, i.e., the most-future entry.
        let f = SsnForecast::from_json(r#"{"monthly":{"2026-06":100.0}}"#)
            .expect("should parse");
        // Only entry is 2026-06 → next_back returns it regardless of query direction.
        assert_eq!(f.ssn_for(2025, 1), 100.0,
            "query before table start should return last known value via iter().next_back()");
    }

    #[test]
    fn empty_table_uses_conservative_default() {
        let f = SsnForecast::from_json(r#"{"monthly":{}}"#).expect("should parse");
        assert_eq!(f.ssn_for(2026, 6), 10.0,
            "empty table must return conservative solar-minimum default 10.0");
    }

    #[test]
    fn malformed_json_is_error() {
        let result = SsnForecast::from_json("{not json");
        assert!(matches!(result, Err(PropagationError::Ssn(_))),
            "malformed JSON must yield PropagationError::Ssn, got: {:?}", result);
    }

    // ---- runtime-mutable forecast (tuxlink-ot71 prerequisite) --------------

    #[test]
    fn load_falls_back_to_bundled_when_no_writable_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        // No ssn-forecast.json on disk → bundled anchor (2026-06 = 100.0).
        let f = SsnForecast::load_writable_then_bundled(dir.path());
        assert_eq!(f.ssn_for(2026, 6), 100.0);
    }

    #[test]
    fn persisted_forecast_is_preferred_over_bundled_on_reload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut monthly = BTreeMap::new();
        monthly.insert("2026-06".to_string(), 142.0); // differs from bundled 100.0
        let updated = SsnForecast { monthly };
        updated.persist(dir.path()).expect("persist should succeed");

        let reloaded = SsnForecast::load_writable_then_bundled(dir.path());
        assert_eq!(
            reloaded.ssn_for(2026, 6),
            142.0,
            "a persisted runtime forecast must win over the bundled table"
        );
    }

    #[test]
    fn corrupt_writable_file_degrades_to_bundled() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(forecast_path(dir.path()), "{garbage").expect("write");
        let f = SsnForecast::load_writable_then_bundled(dir.path());
        // Corrupt writable file is ignored → bundled anchor, never a panic/empty.
        assert_eq!(f.ssn_for(2026, 6), 100.0);
    }

    #[test]
    fn empty_writable_forecast_degrades_to_bundled() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(forecast_path(dir.path()), r#"{"monthly":{}}"#).expect("write");
        let f = SsnForecast::load_writable_then_bundled(dir.path());
        assert_eq!(f.ssn_for(2026, 6), 100.0, "empty writable table is not 'usable'");
    }

    #[test]
    fn persist_then_from_json_roundtrips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut monthly = BTreeMap::new();
        monthly.insert("2026-01".to_string(), 107.8);
        let f = SsnForecast { monthly };
        f.persist(dir.path()).expect("persist");
        let text = std::fs::read_to_string(forecast_path(dir.path())).expect("read");
        let back = SsnForecast::from_json(&text).expect("reparse");
        assert_eq!(back.ssn_for(2026, 1), 107.8);
    }
}
