//! Orchestration + persistence for "Update propagation data" (tuxlink-ot71).
//!
//! Pure of network/Tauri: callers do the I/O (internet fetch via `reqwest`, or
//! an over-radio catalog reply) and hand the bytes here. This parses them with
//! [`super::solar`], persists the SSN forecast ([`super::ssn`]) and a live-indices
//! snapshot, and reports an [`UpdateOutcome`]. `now_ms` is injected so the whole
//! thing is deterministically unit-testable against a tempdir.
//!
//! Two persisted artifacts, deliberately separate:
//!  - `ssn-forecast.json`  — the SSN table VOACAP consumes (the prediction input).
//!  - `solar-snapshot.json` — live SFI/A/K + a freshness stamp + provenance, for
//!    the conditions bar and the "solar data N old" caption. NOT a VOACAP input.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::solar::{self, SolarIndices};
use super::ssn::SsnForecast;
use super::PropagationError;

/// Where the live-indices snapshot is persisted (beside the forecast + prefs).
pub fn snapshot_path(config_dir: &Path) -> PathBuf {
    config_dir.join("solar-snapshot.json")
}

/// Persisted live solar conditions + provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolarSnapshot {
    pub indices: Option<SolarIndices>,
    /// Unix ms when captured (drives the freshness caption).
    pub updated_at_ms: u64,
    /// Provenance: `"swpc"` (internet) | `"rf-wwv"` (over radio, Winlink catalog
    /// text) | `"rf-wwv-voice"` (off-air WWV voice decode, no radio-network hop).
    pub source: String,
    /// True when this update also refreshed the SSN forecast table (the internet
    /// smoothed-SSN path always does; an RF reply does too, via a derived value).
    pub forecast_updated: bool,
}

impl SolarSnapshot {
    pub fn load(config_dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(snapshot_path(config_dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn persist(&self, config_dir: &Path) -> Result<(), PropagationError> {
        let path = snapshot_path(config_dir);
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
}

/// Result of an update attempt — returned to the frontend for its summary.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateOutcome {
    pub forecast_updated: bool,
    pub indices: Option<SolarIndices>,
    pub source: String,
}

/// Apply an internet (NOAA SWPC) update. `predicted_ssn_json` refreshes the
/// smoothed-SSN forecast table (the VOACAP input); `wwv_text` sets the live
/// conditions. Either may be `None` (a partial fetch still persists what it got),
/// but a malformed predicted-SSN payload is an error (don't silently keep stale).
pub fn apply_swpc_update(
    predicted_ssn_json: Option<&str>,
    wwv_text: Option<&str>,
    now_ms: u64,
    config_dir: &Path,
) -> Result<UpdateOutcome, PropagationError> {
    let mut forecast_updated = false;
    if let Some(json) = predicted_ssn_json {
        let forecast = solar::parse_swpc_predicted_ssn(json)?;
        forecast.persist(config_dir)?;
        forecast_updated = true;
    }
    let indices = wwv_text.and_then(solar::parse_wwv);
    let outcome = UpdateOutcome {
        forecast_updated,
        indices,
        source: "swpc".to_string(),
    };
    SolarSnapshot {
        indices,
        updated_at_ms: now_ms,
        source: "swpc".to_string(),
        forecast_updated,
    }
    .persist(config_dir)?;
    Ok(outcome)
}

/// Apply an over-radio catalog reply (PROP_WWV / PROP_SGAS). Parses SFI/A/K,
/// derives an SSN from the SFI (only daily SFI crosses the air — the documented
/// coarser fallback), and writes it into the writable forecast as the current
/// `year`-`month` so the next prediction uses it offline, preserving any other
/// persisted months.
pub fn apply_rf_solar_reply(
    body: &str,
    year: i32,
    month: u8,
    now_ms: u64,
    config_dir: &Path,
) -> Result<UpdateOutcome, PropagationError> {
    let indices = solar::parse_wwv(body).ok_or_else(|| {
        PropagationError::Ssn("RF solar reply had no parsable solar flux".to_string())
    })?;
    apply_rf_solar_indices(indices, "rf-wwv", year, month, now_ms, config_dir)
}

/// Apply pre-parsed RF solar indices under an explicit `source` provenance tag.
/// Derives an SSN from the SFI (only daily SFI crosses the air — the documented
/// coarser fallback), writes it into the writable forecast as the current
/// `year`-`month` (preserving other months), and persists the live snapshot.
pub fn apply_rf_solar_indices(
    indices: SolarIndices,
    source: &str,
    year: i32,
    month: u8,
    now_ms: u64,
    config_dir: &Path,
) -> Result<UpdateOutcome, PropagationError> {
    let mut forecast = SsnForecast::load_writable_then_bundled(config_dir);
    let derived = solar::derive_ssn_from_sfi(indices.sfi);
    forecast.monthly.insert(format!("{year:04}-{month:02}"), derived);
    forecast.persist(config_dir)?;
    SolarSnapshot {
        indices: Some(indices),
        updated_at_ms: now_ms,
        source: source.to_string(),
        forecast_updated: true,
    }
    .persist(config_dir)?;
    Ok(UpdateOutcome { forecast_updated: true, indices: Some(indices), source: source.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SWPC_JSON: &str = r#"[
        {"time-tag":"2026-06","predicted_ssn":133.0,"predicted_f10.7":150.0},
        {"time-tag":"2026-07","predicted_ssn":131.5}
    ]"#;
    const WWV: &str = "Solar flux 117 and estimated planetary A-index 6.\n\
        The estimated planetary K-index at 1200 UTC on 16 June was 1.33.\n";

    #[test]
    fn swpc_update_persists_forecast_and_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let out = apply_swpc_update(Some(SWPC_JSON), Some(WWV), 1_000, dir.path()).unwrap();
        assert!(out.forecast_updated);
        assert_eq!(out.source, "swpc");
        // Forecast applied: a reload sees the SWPC smoothed value, not bundled 100.
        let f = SsnForecast::load_writable_then_bundled(dir.path());
        assert_eq!(f.ssn_for(2026, 6), 133.0);
        // Snapshot captured the live indices + stamp.
        let snap = SolarSnapshot::load(dir.path()).expect("snapshot persisted");
        assert_eq!(snap.indices.unwrap().sfi, 117.0);
        assert_eq!(snap.updated_at_ms, 1_000);
        assert!(snap.forecast_updated);
    }

    #[test]
    fn swpc_update_wwv_only_sets_indices_without_touching_forecast() {
        let dir = tempfile::tempdir().unwrap();
        let out = apply_swpc_update(None, Some(WWV), 2_000, dir.path()).unwrap();
        assert!(!out.forecast_updated);
        assert_eq!(out.indices.unwrap().sfi, 117.0);
        // No forecast written → prediction still reads bundled.
        let f = SsnForecast::load_writable_then_bundled(dir.path());
        assert_eq!(f.ssn_for(2026, 6), 100.0);
    }

    #[test]
    fn swpc_update_malformed_predicted_ssn_is_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(apply_swpc_update(Some("{bad"), Some(WWV), 3_000, dir.path()).is_err());
    }

    #[test]
    fn rf_reply_derives_ssn_into_current_month() {
        let dir = tempfile::tempdir().unwrap();
        let out = apply_rf_solar_reply(WWV, 2026, 6, 4_000, dir.path()).unwrap();
        assert!(out.forecast_updated);
        assert_eq!(out.source, "rf-wwv");
        let f = SsnForecast::load_writable_then_bundled(dir.path());
        // SFI 117 → derived ~67-68 (NOT the bundled 100), written as 2026-06.
        let v = f.ssn_for(2026, 6);
        assert!((60.0..75.0).contains(&v), "derived SSN for SFI 117 ~67, got {v}");
        let snap = SolarSnapshot::load(dir.path()).unwrap();
        assert_eq!(snap.source, "rf-wwv");
    }

    #[test]
    fn rf_reply_preserves_other_persisted_months() {
        let dir = tempfile::tempdir().unwrap();
        // Seed a SWPC forecast for two months first.
        apply_swpc_update(Some(SWPC_JSON), None, 1_000, dir.path()).unwrap();
        // RF update for 2026-06 must not drop 2026-07.
        apply_rf_solar_reply(WWV, 2026, 6, 5_000, dir.path()).unwrap();
        let f = SsnForecast::load_writable_then_bundled(dir.path());
        assert_eq!(f.ssn_for(2026, 7), 131.5, "other months preserved");
    }

    #[test]
    fn rf_reply_unparsable_body_is_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(apply_rf_solar_reply("no flux here", 2026, 6, 6_000, dir.path()).is_err());
    }

    #[test]
    fn rf_voice_source_tag_persists_and_updates_forecast() {
        let dir = tempfile::tempdir().unwrap();
        let indices = SolarIndices { sfi: 150.0, a_index: Some(8.0), k_index: Some(2.0) };
        let out = apply_rf_solar_indices(indices, "rf-wwv-voice", 2026, 7, 1_000, dir.path()).unwrap();
        assert!(out.forecast_updated);
        assert_eq!(out.source, "rf-wwv-voice");
        let snap = SolarSnapshot::load(dir.path()).unwrap();
        assert_eq!(snap.source, "rf-wwv-voice");
        // Forecast got the derived SSN for the current month.
        let f = SsnForecast::load_writable_then_bundled(dir.path());
        assert!(f.monthly.contains_key("2026-07"));
    }

    #[test]
    fn snapshot_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let snap = SolarSnapshot {
            indices: Some(SolarIndices { sfi: 142.0, a_index: Some(5.0), k_index: Some(2.0) }),
            updated_at_ms: 7_000,
            source: "swpc".to_string(),
            forecast_updated: true,
        };
        snap.persist(dir.path()).unwrap();
        assert_eq!(SolarSnapshot::load(dir.path()).unwrap(), snap);
    }
}
