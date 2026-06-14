//! Operator propagation preferences: antenna preset, required SNR, TX power.
//!
//! Persisted as a small JSON file beside the main config (its own file, like the
//! telnet allowlist and the station cache — keeps these prediction-only knobs out
//! of the big `Config` struct and its many constructor sites). Missing or corrupt
//! → defaults; never panics, never deletes the operator's file.
//!
//! Atomic write (tmp + rename) mirrors `stations_disk::save` / `FavoritesStore`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::antenna::AntennaPreset;

/// File name under the config directory.
const PREFS_FILE: &str = "propagation_prefs.json";

/// REQ.SNR default when the connecting mode is unknown, in dB-Hz.
///
/// VOACAP's REQ.SNR is signal-to-noise referenced to a **1 Hz** noise bandwidth:
/// `REQ.SNR[dB-Hz] = SNR[dB] + 10·log₁₀(bandwidth_Hz)` (voacap.com "Ten common
/// mistakes" #3; the official VOACAP blog gives the formula + worked values). So
/// the right number for a *reliable* digital link is the mode's in-channel SNR
/// plus its bandwidth term — NOT the mode's absolute decode floor.
///
/// Published anchors (dB-Hz): CW ≈ 19, FT8 ≈ 13, SSB voice ≈ 38–44,
/// **VARA-HF reliable connect ≈ 35–37**, ARDOP ≈ 24–27. The prior 22 was
/// CW-grade — near VARA's *absolute decode floor* (~12–20), where a link
/// establishes then drops — so it predicted "excellent" reachability for short
/// NVIS paths even at 1 W (confirmed by direct voacapl runs 2026-06-14). 38 is
/// the VOACAP author's SSB value: mildly conservative against VARA/ARDOP, the
/// safe direction for an availability predictor. Operator-adjustable; per-mode
/// derivation is the follow-up. Full rationale + per-mode table + citations:
/// docs/design/2026-06-14-find-a-station-prediction-recalibration.md.
pub const DEFAULT_REQ_SNR_DB: f64 = 38.0;

/// TX power default, watts. Operator-adjustable.
pub const DEFAULT_TX_POWER_W: f64 = 100.0;

fn default_req_snr_db() -> f64 {
    DEFAULT_REQ_SNR_DB
}
fn default_tx_power_w() -> f64 {
    DEFAULT_TX_POWER_W
}

/// Operator preferences that shape an HF prediction. `#[serde(default)]` on every
/// field migrates older/partial files field-by-field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropagationPrefs {
    /// The operator's own-station antenna (TX end).
    #[serde(default)]
    pub antenna_preset: AntennaPreset,
    /// Required SNR (dB-Hz) for the VOACAP SYSTEM card. Bounded 0..100 on write.
    #[serde(default = "default_req_snr_db")]
    pub req_snr_db: f64,
    /// TX power in watts. Must be > 0 on write.
    #[serde(default = "default_tx_power_w")]
    pub tx_power_w: f64,
}

impl Default for PropagationPrefs {
    fn default() -> Self {
        PropagationPrefs {
            antenna_preset: AntennaPreset::default(),
            req_snr_db: DEFAULT_REQ_SNR_DB,
            tx_power_w: DEFAULT_TX_POWER_W,
        }
    }
}

/// The prefs path under `config_dir` (its parent is created on save).
pub fn prefs_path(config_dir: &Path) -> PathBuf {
    config_dir.join(PREFS_FILE)
}

/// Load prefs from `path`. Missing file → defaults. Unparseable → defaults, with
/// a quarantine note; the original bytes are preserved for inspection.
pub fn load(path: &Path) -> PropagationPrefs {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return PropagationPrefs::default(),
        Err(e) => {
            eprintln!(
                "propagation_prefs: failed to read {}: {e} — using defaults",
                path.display()
            );
            return PropagationPrefs::default();
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "propagation_prefs: {} is unparseable, using defaults (original preserved): {e}",
                path.display()
            );
            PropagationPrefs::default()
        }
    }
}

/// Atomically persist prefs: write `<path>.tmp`, then rename over `path`.
pub fn save(path: &Path, prefs: &PropagationPrefs) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(prefs)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| PREFS_FILE.to_string());
    let tmp = path.with_file_name(format!("{name}.tmp"));

    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_loads_defaults() {
        let dir = tempdir().unwrap();
        let p = prefs_path(dir.path());
        assert_eq!(load(&p), PropagationPrefs::default());
        assert_eq!(load(&p).req_snr_db, DEFAULT_REQ_SNR_DB);
        assert_eq!(load(&p).antenna_preset, AntennaPreset::EfhwSloper);
    }

    #[test]
    fn round_trips_a_saved_prefs() {
        let dir = tempdir().unwrap();
        let p = prefs_path(dir.path());
        let prefs = PropagationPrefs {
            antenna_preset: AntennaPreset::BaseVerticalRadials,
            req_snr_db: 24.0,
            tx_power_w: 50.0,
        };
        save(&p, &prefs).unwrap();
        assert_eq!(load(&p), prefs);
    }

    #[test]
    fn corrupt_file_loads_defaults_and_is_preserved() {
        let dir = tempdir().unwrap();
        let p = prefs_path(dir.path());
        std::fs::write(&p, b"{ not valid json").unwrap();
        assert_eq!(load(&p), PropagationPrefs::default());
        assert!(p.exists(), "load must not delete the corrupt file");
    }

    #[test]
    fn partial_file_migrates_missing_fields_to_defaults() {
        let dir = tempdir().unwrap();
        let p = prefs_path(dir.path());
        // Only antenna_preset present; the other two come from serde defaults.
        std::fs::write(&p, br#"{"antenna_preset":"mobile-hf-whip"}"#).unwrap();
        let loaded = load(&p);
        assert_eq!(loaded.antenna_preset, AntennaPreset::MobileHfWhip);
        assert_eq!(loaded.req_snr_db, DEFAULT_REQ_SNR_DB);
        assert_eq!(loaded.tx_power_w, DEFAULT_TX_POWER_W);
    }

    #[test]
    fn save_leaves_no_tmp() {
        let dir = tempdir().unwrap();
        let p = prefs_path(dir.path());
        save(&p, &PropagationPrefs::default()).unwrap();
        let tmps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "no .tmp must remain after save");
    }
}
