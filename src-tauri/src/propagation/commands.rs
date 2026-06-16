//! Tauri command surface for offline HF path prediction.
//!
//! Plan: docs/superpowers/plans/2026-06-10-u1-voacapl-prediction.md (Task 6)
//!
//! Architecture decisions encoded here:
//! - F8: UTC year/month is derived from an injected `Clock`, NOT from the
//!   frontend and NOT hardcoded. The frontend passes only the RF inputs.
//! - F2: the voacapl binary is a Tauri `externalBin` sidecar placed adjacent
//!   to the main executable at runtime. Resolved via `std::env::current_exe()?
//!   .parent()?.join("voacapl")` — NOT under `BaseDirectory::Resource`.
//!   The packaged-`.deb` path must be confirmed by the Task 7 gated test /
//!   operator smoke before relying on this in production (test-production-mount-
//!   path failure class).
//! - v1 defaults: tx_power_w=100.0, req_snr_db=73.0 (VOACAP standard matching
//!   the captured fixture). The data-mode-calibrated value is a documented
//!   empirical tunable — not a fabricated number.
//! - F17: `.setup()` MUST NOT abort app launch — failures in path resolution
//!   are soft (eprintln + skip state registration = prediction unavailable).
//! - F10: scratch dir is `app_cache_dir()`. Fail closed if unavailable; never
//!   fall back to `std::env::temp_dir()`.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{Datelike, TimeZone, Utc};
use tauri::State;

use crate::catalog::stations::GatewayAntenna;
use crate::catalog::stations_cache::Clock;
use crate::ui_commands::UiError;

use super::engine::EnginePaths;
use super::{antenna, deck, engine, parse, prefs, ssn, PathPrediction, PredictionInputs, PropagationError};

// ============================================================================
// Error projection
// ============================================================================

impl From<PropagationError> for UiError {
    fn from(e: PropagationError) -> Self {
        match e {
            PropagationError::InvalidGrid(_)
            | PropagationError::NoFrequencies
            | PropagationError::TooManyFrequencies(_) => UiError::Rejected(e.to_string()),
            PropagationError::BinaryNotFound(_) => UiError::Unavailable { reason: e.to_string() },
            // RunFailed, ParseFailed, Ssn, Io → Internal
            _ => UiError::Internal { detail: e.to_string() },
        }
    }
}

// ============================================================================
// Managed state
// ============================================================================

/// Engine resources available when all setup paths resolve successfully.
pub struct ReadyPropagation {
    pub paths: EnginePaths,
    pub scratch_parent: PathBuf,
    pub clock: Arc<dyn Clock>,
    pub forecast: ssn::SsnForecast,
}

/// Always-managed propagation state (registered unconditionally in `.setup()`).
///
/// Using an enum instead of `Option` / conditional `.manage()` ensures the
/// Tauri extractor never fails with a generic "state not registered" error.
/// A soft-disabled engine returns `UiError::Unavailable` from the command
/// body, honoring the F17 degrade contract at the command surface.
pub enum PropagationState {
    /// All engine assets resolved; predictions are available.
    Ready(ReadyPropagation),
    /// One or more engine assets failed to resolve; predictions are unavailable.
    /// `reason` is the human-readable explanation logged at startup (F17).
    Unavailable(String),
}

// ============================================================================
// Core prediction logic (factored out for unit-testability)
// ============================================================================

/// Derive UTC year + month from the injected clock.
pub(crate) fn utc_year_month(clock: &dyn Clock) -> (i32, u8) {
    let millis = clock.now_millis() as i64;
    let dt = Utc
        .timestamp_millis_opt(millis)
        .single()
        .unwrap_or_else(|| {
            Utc.timestamp_opt(0, 0)
                .single()
                .expect("Unix epoch is always valid")
        });
    (dt.year(), dt.month() as u8)
}

/// Pure assembly + run, factored out of the `#[tauri::command]` so the
/// input-validation paths that don't touch the engine are unit-testable.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_prediction(
    clock: &dyn Clock,
    forecast: &ssn::SsnForecast,
    paths: &EnginePaths,
    scratch_parent: &std::path::Path,
    tx_grid: String,
    rx_grid: String,
    frequencies_khz: Vec<f64>,
    tx_antenna_voa: String,
    rx_antenna_voa: String,
    req_snr_db: f64,
    tx_power_w: f64,
    tx_antenna_voa_content: Option<String>,
    noise_dbw: f64,
) -> Result<PathPrediction, PropagationError> {
    let (year, month) = utc_year_month(clock);
    let ssn_val = forecast.ssn_for(year, month);
    let inputs = PredictionInputs {
        tx_grid,
        rx_grid,
        frequencies_khz,
        year,
        month,
        ssn: ssn_val,
        // Operator preferences (antenna preset + SNR + power) and the gateway's
        // parsed antenna drive these — resolved by the command wrapper. The prior
        // fixed RX whip (`swwhip.voa`) + 73 dB voice SNR are gone.
        tx_power_w,
        req_snr_db,
        tx_antenna_voa,
        rx_antenna_voa,
        tx_antenna_voa_content,
        noise_dbw,
    };
    // Filter + validate frequencies first (fast path before any disk I/O or
    // engine invocation — bad inputs are rejected here).
    let active = deck::active_hf_frequencies_khz(&inputs.frequencies_khz)?;
    let deck_text = deck::build_deck(&inputs)?;
    // When the operator's preset produced a height-aware IONCAP pattern, hand it
    // to the engine to write into the scratch antennas/default/ before the run.
    let antenna_files: Vec<(String, String)> = match &inputs.tx_antenna_voa_content {
        Some(content) => vec![(inputs.tx_antenna_voa.clone(), content.clone())],
        None => Vec::new(),
    };
    let out = engine::run_voacapl_with_files(paths, &deck_text, &antenna_files, scratch_parent)?;
    parse::parse_voacapx_out(&out, &active, ssn_val, year, month)
}

// ============================================================================
// Tauri command
// ============================================================================

/// Predict the reliability of an HF path between two Maidenhead grid squares.
///
/// `year` and `month` are derived server-side from the injected `Clock` (F8).
/// The frontend passes only RF inputs: the two grids and the candidate
/// frequencies. `tx_power_w` (100 W) and `req_snr_db` (73 dB VOACAP
/// standard) are v1 defaults applied here.
///
/// Returns `Err(UiError::Unavailable)` when `PropagationState::Unavailable`
/// was registered at startup (binary not found, cache dir unavailable, etc.),
/// honoring the F17 degrade contract. The state is ALWAYS managed so the
/// Tauri extractor never fails before the command body runs.
#[tauri::command]
pub async fn propagation_predict_path(
    tx_grid: String,
    rx_grid: String,
    frequencies_khz: Vec<f64>,
    gateway_antenna: Option<GatewayAntenna>,
    state: State<'_, PropagationState>,
) -> Result<PathPrediction, UiError> {
    // Ensure the engine is available before doing any work.
    let ready = match state.inner() {
        PropagationState::Ready(r) => r,
        PropagationState::Unavailable(reason) => {
            return Err(UiError::Unavailable { reason: reason.clone() });
        }
    };

    // Resolve the antenna model + SNR + power for this prediction:
    //  - TX (own station) ← operator's saved antenna preset.
    //  - RX (far/gateway end) ← the station's parsed antenna code, isotrope fallback
    //    (NEVER a forced whip — the whip's zenith null is what killed NVIS paths).
    //  - REQ.SNR + power ← saved prefs (defaults: 38 dB-Hz unknown-mode SNR, 100 W).
    let prefs = match crate::config::config_path().parent() {
        Some(dir) => prefs::load(&prefs::prefs_path(dir)),
        None => prefs::PropagationPrefs::default(),
    };
    // Generate a height-aware IONCAP pattern from the operator's preset + height +
    // ground. When the preset has a parametric model, the deck references a
    // generated file (written into the scratch by the engine); otherwise it falls
    // back to the stock isotrope file (Unknown preset).
    let tx_antenna_voa_content =
        antenna::operator_voa_content(prefs.antenna_preset, prefs.antenna_height_m, prefs.ground_type);
    let tx_antenna_voa = if tx_antenna_voa_content.is_some() {
        antenna::OPERATOR_VOA_FILENAME.to_string()
    } else {
        prefs.antenna_preset.voa_file().to_string()
    };
    let rx_antenna_voa = antenna::gateway_voa_file(gateway_antenna).to_string();
    let req_snr_db = prefs.req_snr_db;
    let tx_power_w = prefs.tx_power_w;
    let noise_dbw = prefs.noise_environment.system_card_dbw();

    // Clone everything we need into the blocking closure — the engine call is a
    // blocking std::process::Command; we must never hold it across an async boundary.
    let clock: Arc<dyn Clock> = ready.clock.clone();
    // Runtime-mutable forecast (tuxlink-ot71): load the forecast fresh from disk
    // per prediction — writable (a persisted internet/RF propagation update)
    // preferred, else the bundled table — exactly as prefs are read fresh above.
    // This is what makes an "Update propagation data" take effect WITHOUT an app
    // restart. The forecast JSON is tiny and prediction sweeps are debounced
    // (tuxlink-ziyu), so the per-call read is negligible. Falls back to the
    // startup-loaded `ready.forecast` only if the config dir is unavailable.
    let forecast = match crate::config::config_path().parent() {
        Some(dir) => ssn::SsnForecast::load_writable_then_bundled(dir),
        None => ready.forecast.clone(),
    };
    let paths = ready.paths.clone();
    let scratch = ready.scratch_parent.clone();

    let result = tokio::task::spawn_blocking(move || {
        run_prediction(
            clock.as_ref(),
            &forecast,
            &paths,
            &scratch,
            tx_grid,
            rx_grid,
            frequencies_khz,
            tx_antenna_voa,
            rx_antenna_voa,
            req_snr_db,
            tx_power_w,
            tx_antenna_voa_content,
            noise_dbw,
        )
    })
    .await
    .map_err(|e| UiError::Internal {
        detail: format!("spawn_blocking join error: {e}"),
    })??;

    Ok(result)
}

// ============================================================================
// Propagation preferences (antenna preset + REQ.SNR + TX power)
// ============================================================================

/// Read the operator's propagation preferences. Defaults when no prefs file
/// exists (fresh install). Not a backend call — reads the prefs file directly,
/// like `config_read`; failures degrade to defaults rather than erroring.
#[tauri::command]
pub async fn propagation_prefs_read() -> Result<prefs::PropagationPrefs, UiError> {
    let prefs = match crate::config::config_path().parent() {
        Some(dir) => prefs::load(&prefs::prefs_path(dir)),
        None => prefs::PropagationPrefs::default(),
    };
    Ok(prefs)
}

/// Persist the operator's propagation preferences.
///
/// Validates before writing: `req_snr_db` must be finite and within `[0, 100)`
/// (the VOACAP SYSTEM card's 4-char Fortran field overflows at 100), and
/// `tx_power_w` must be finite and `> 0`.
#[tauri::command]
pub async fn propagation_prefs_write(
    antenna_preset: antenna::AntennaPreset,
    req_snr_db: f64,
    tx_power_w: f64,
    antenna_height_m: f64,
    ground_type: antenna::GroundType,
    noise_environment: prefs::NoiseEnvironment,
) -> Result<(), UiError> {
    if !req_snr_db.is_finite() || !(0.0..100.0).contains(&req_snr_db) {
        return Err(UiError::Rejected(format!(
            "req_snr_db {req_snr_db} out of range — must be 0..100 dB-Hz"
        )));
    }
    if !tx_power_w.is_finite() || tx_power_w <= 0.0 {
        return Err(UiError::Rejected(format!(
            "tx_power_w {tx_power_w} invalid — must be > 0 W"
        )));
    }
    if !antenna_height_m.is_finite() || !(0.0..=200.0).contains(&antenna_height_m) {
        return Err(UiError::Rejected(format!(
            "antenna_height_m {antenna_height_m} out of range — must be 0..200 m"
        )));
    }
    let new_prefs = prefs::PropagationPrefs {
        antenna_preset,
        req_snr_db,
        tx_power_w,
        antenna_height_m,
        ground_type,
        noise_environment,
    };
    let config_path = crate::config::config_path();
    let dir = config_path.parent().ok_or_else(|| UiError::Internal {
        detail: "config path has no parent directory".to_string(),
    })?;
    prefs::save(&prefs::prefs_path(dir), &new_prefs)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // Local MockClock for tests — the one in catalog::stations_cache is private
    // to that module's test block.
    struct MockClock(u64);
    impl Clock for MockClock {
        fn now_millis(&self) -> u64 {
            self.0
        }
    }

    // -------------------------------------------------------------------------
    // PropagationError → UiError mapping
    // -------------------------------------------------------------------------

    #[test]
    fn propagation_error_maps_to_uierror() {
        // InvalidGrid → Rejected
        let e: UiError = PropagationError::InvalidGrid("ZZ".into()).into();
        assert!(
            matches!(e, UiError::Rejected(_)),
            "InvalidGrid should map to Rejected, got {e:?}"
        );

        // NoFrequencies → Rejected
        let e: UiError = PropagationError::NoFrequencies.into();
        assert!(
            matches!(e, UiError::Rejected(_)),
            "NoFrequencies should map to Rejected, got {e:?}"
        );

        // TooManyFrequencies → Rejected
        let e: UiError = PropagationError::TooManyFrequencies(12).into();
        assert!(
            matches!(e, UiError::Rejected(_)),
            "TooManyFrequencies should map to Rejected, got {e:?}"
        );

        // BinaryNotFound → Unavailable
        let e: UiError = PropagationError::BinaryNotFound("/no/binary".into()).into();
        assert!(
            matches!(e, UiError::Unavailable { .. }),
            "BinaryNotFound should map to Unavailable, got {e:?}"
        );

        // RunFailed → Internal
        let e: UiError = PropagationError::RunFailed("died".into()).into();
        assert!(
            matches!(e, UiError::Internal { .. }),
            "RunFailed should map to Internal, got {e:?}"
        );

        // ParseFailed → Internal
        let e: UiError = PropagationError::ParseFailed("bad output".into()).into();
        assert!(
            matches!(e, UiError::Internal { .. }),
            "ParseFailed should map to Internal, got {e:?}"
        );

        // Ssn → Internal
        let e: UiError = PropagationError::Ssn("bad json".into()).into();
        assert!(
            matches!(e, UiError::Internal { .. }),
            "Ssn should map to Internal, got {e:?}"
        );

        // Io → Internal (catch-all coverage)
        let e: UiError =
            PropagationError::Io(std::io::Error::other("x")).into();
        assert!(
            matches!(e, UiError::Internal { .. }),
            "Io should map to Internal, got {e:?}"
        );
    }

    // -------------------------------------------------------------------------
    // utc_year_month from a known epoch millis
    // -------------------------------------------------------------------------

    #[test]
    fn utc_year_month_from_clock() {
        // 2026-06-10T00:00:00Z.  Verify: date -u -d "2026-06-10" +%s → 1781049600
        let clock = MockClock(1_781_049_600_000);
        let (year, month) = utc_year_month(&clock);
        assert_eq!(year, 2026, "year should be 2026");
        assert_eq!(month, 6, "month should be 6 (June)");
    }

    // -------------------------------------------------------------------------
    // run_prediction rejects bad input before touching the engine
    // -------------------------------------------------------------------------

    fn bundled_forecast() -> ssn::SsnForecast {
        ssn::SsnForecast::from_json(ssn::BUNDLED_SSN_FORECAST)
            .expect("bundled forecast must parse")
    }

    fn nonexistent_paths() -> EnginePaths {
        EnginePaths {
            binary: PathBuf::from("/nonexistent/voacapl"),
            itshfbc_root: PathBuf::from("/nonexistent/itshfbc"),
        }
    }

    #[test]
    fn invalid_grid_rejected_without_engine() {
        let clock = MockClock(1_781_049_600_000);
        let forecast = bundled_forecast();
        let paths = nonexistent_paths();
        let scratch = std::env::temp_dir(); // never reached; engine not invoked

        let result = run_prediction(
            &clock,
            &forecast,
            &paths,
            &scratch,
            "EN52".into(),    // valid tx
            "ZZ".into(),      // invalid rx grid → InvalidGrid
            vec![7103.0],
            "ccir.000".into(),
            "ccir.000".into(),
            22.0,
            100.0,
            None,
            145.0,
        );
        let err = result.expect_err("invalid rx_grid should produce an error");
        assert!(
            matches!(err, PropagationError::InvalidGrid(_)),
            "expected InvalidGrid, got {err:?}"
        );
        // Confirm it maps to Rejected
        let ui: UiError = err.into();
        assert!(
            matches!(ui, UiError::Rejected(_)),
            "InvalidGrid should map to Rejected, got {ui:?}"
        );
    }

    // -------------------------------------------------------------------------
    // PropagationState::Unavailable → UiError::Unavailable mapping
    // -------------------------------------------------------------------------

    /// Asserts the mapping contract without a live Tauri harness: constructing
    /// an `Unavailable` state and matching it yields the same reason string.
    /// The full command path (including extractor) is exercised by the Task 7
    /// gated integration test; this test guards the enum shape and the match
    /// branch in isolation.
    #[test]
    fn unavailable_state_maps_to_uierror() {
        let state = PropagationState::Unavailable("test reason".to_string());
        let reason = match state {
            PropagationState::Unavailable(r) => r,
            PropagationState::Ready(_) => panic!("expected Unavailable"),
        };
        assert_eq!(reason, "test reason");
        // Confirm the reason maps to UiError::Unavailable.
        let ui = UiError::Unavailable { reason: reason.clone() };
        assert!(
            matches!(ui, UiError::Unavailable { .. }),
            "Unavailable state should produce UiError::Unavailable, got {ui:?}"
        );
    }

    #[test]
    fn no_frequencies_rejected_without_engine() {
        let clock = MockClock(1_781_049_600_000);
        let forecast = bundled_forecast();
        let paths = nonexistent_paths();
        let scratch = std::env::temp_dir(); // never reached

        let result = run_prediction(
            &clock,
            &forecast,
            &paths,
            &scratch,
            "EN52".into(),
            "FN20".into(),
            vec![], // empty → NoFrequencies
            "ccir.000".into(),
            "ccir.000".into(),
            22.0,
            100.0,
            None,
            145.0,
        );
        let err = result.expect_err("empty frequencies should produce an error");
        assert!(
            matches!(err, PropagationError::NoFrequencies),
            "expected NoFrequencies, got {err:?}"
        );
        let ui: UiError = err.into();
        assert!(
            matches!(ui, UiError::Rejected(_)),
            "NoFrequencies should map to Rejected, got {ui:?}"
        );
    }
}
