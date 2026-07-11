//! The FT8 listener Tauri commands (spec §Commands, §NewCommands). Thin
//! async wrappers: validation + the ft8 writer mutex + spawn_blocking into
//! the service's blocking-context surface. Testable bodies live in the
//! `_inner` fns; the `#[tauri::command]` shells only extract state.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::config::{self, Config};
use crate::ft8::service::{Ft8ListenerState, Ft8Snapshot};
use crate::winlink::ax25::devices::StableAudioId;
use tuxlink_capture::state::{BlockedReason, ServiceAxis};

/// Machine-readable error for every FT8 Tauri command in this phase (spec
/// §NewCommands). A2 owns this definition; A3/A4/A5/A7 `use` it — none
/// re-defines it. `kind` is a bare `String` (not an enum) so new tags can
/// land without a breaking type change; the FULL kebab-case vocabulary for
/// the whole phase is: `device-reserved | device-in-use | device-not-found
/// | modem-busy | rig-not-configured | probe-timeout | invalid-grid |
/// invalid-band`. The UI branches on `kind`, never parses `detail` —
/// `detail` is human-readable text only.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ft8CmdError {
    pub kind: String,
    pub detail: String,
}

impl Ft8CmdError {
    pub(crate) fn new(kind: &str, detail: impl Into<String>) -> Self {
        Self { kind: kind.into(), detail: detail.into() }
    }
}

/// One serialized ft8 RMW cycle, delegating to the CRATE-WIDE gate
/// (`config::update_config`, Step 2a): read → mutate → validate → atomic
/// write under config.rs's one static writer lock. Returns the updated
/// Ft8Config so callers can push it into the service.
fn with_ft8_config_writer(
    mutate: impl FnOnce(&mut Config) -> Result<(), String>,
) -> Result<crate::config::Ft8Config, String> {
    config::update_config(mutate).map(|cfg| cfg.ft8)
}

fn wedged_refusal(state: &Ft8ListenerState) -> Result<(), String> {
    if matches!(
        state.axis(),
        ServiceAxis::Blocked(BlockedReason::CaptureWedged)
    ) {
        return Err(
            "the FT8 capture thread is wedged and may still hold the sound card; \
             restart Tuxlink to recover"
                .into(),
        );
    }
    Ok(())
}

// ---- inner (testable) bodies ------------------------------------------------

pub(crate) fn ft8_listener_start_inner(state: &Arc<Ft8ListenerState>) -> Result<(), String> {
    wedged_refusal(state)?;
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.enabled = true;
        Ok(())
    })?;
    state.set_ft8_config(ft8);
    state.start() // idempotent: live supervisor → sequence re-run signal
}

pub(crate) fn ft8_listener_stop_inner(state: &Arc<Ft8ListenerState>) -> Result<(), String> {
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.enabled = false;
        Ok(())
    })?;
    state.set_ft8_config(ft8);
    state.stop();
    Ok(())
}

pub(crate) fn ft8_set_device_inner(
    state: &Arc<Ft8ListenerState>,
    stable_id: StableAudioId,
) -> Result<(), String> {
    // From capture-wedged, set_device (like start) refuses: a detached
    // thread may still hold the PCM; a second capture path in a process
    // that can no longer arbitrate the card is worse than refusing.
    wedged_refusal(state)?;
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.device = Some(stable_id);
        Ok(())
    })?;
    state.set_ft8_config(ft8);
    // From any blocked state except capture-wedged (refused above), a
    // device pick retriggers the start sequence; from stopped it stays
    // persist-only (the operator's start click is the trigger).
    if matches!(state.axis(), ServiceAxis::Blocked(_)) {
        state.start()?;
    }
    Ok(())
}

pub(crate) fn ft8_set_band_inner(
    state: &Arc<Ft8ListenerState>,
    band: String,
) -> Result<(), String> {
    // Validate BEFORE persisting (rejects out-of-table).
    if tuxlink_capture::bands::dial_hz(&band).is_none() {
        return Err(format!("{band:?} is not an FT8 band"));
    }
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.band = band.clone();
        Ok(())
    })?;
    state.set_ft8_config(ft8);
    if state.axis() == ServiceAxis::Listening {
        if state.platform.rig_configured() {
            // Listening + CAT: the chip is a QSY command. Through the
            // arbiter when installed — the ARBITER lock (rig_session) is
            // what excludes a concurrent pause_for_modem; qsy_to_band owns
            // the RIG lock itself (rig_session takes ONLY the arbiter lock:
            // lock order arbiter > rig > state, each at most once — T14's
            // non-reentrancy contract).
            let do_qsy = || state.qsy_to_band(&band, crate::ft8::records::BandSource::CatConfirmed);
            match crate::ft8::arbiter::FT8_ARBITER.get() {
                Some(arb) => arb.rig_session(do_qsy)?,
                None => do_qsy()?,
            }
        } else {
            // Listening, no CAT: the chip is a STATEMENT — relabel with
            // operator-asserted provenance + instructed dial; k resets.
            state.assert_band_operator(&band)?;
        }
    }
    // Not listening: persist-only — never touches the radio (only a running
    // listener the operator started moves the dial).
    Ok(())
}

pub(crate) fn ft8_set_sweep_inner(
    state: &Arc<Ft8ListenerState>,
    enabled: bool,
) -> Result<(), String> {
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.sweep.enabled = enabled;
        Ok(()) // validate() enforces sweep.enabled ⇒ rig configured
    })?;
    state.set_ft8_config(ft8);
    state.apply_sweep_enabled(enabled);
    Ok(())
}

pub(crate) fn ft8_set_sweep_bands_inner(
    state: &Arc<Ft8ListenerState>,
    bands: Vec<String>,
) -> Result<(), Ft8CmdError> {
    // Validate BEFORE persisting — mirrors ft8_set_band_inner (~line 88):
    // reject the WHOLE request and touch nothing on disk on any bad entry,
    // including an empty list (Config::validate()'s per-band loop is a
    // no-op on an empty Vec, so the empty guard lives here, not there).
    if bands.is_empty() {
        return Err(Ft8CmdError::new("invalid-band", "sweep band list must not be empty"));
    }
    for band in &bands {
        if tuxlink_capture::bands::dial_hz(band).is_none() {
            return Err(Ft8CmdError::new("invalid-band", format!("{band:?} is not an FT8 band")));
        }
    }
    let ft8 = with_ft8_config_writer(|c| {
        // RMW the FULL ft8 config through the crate-wide writer: only
        // .sweep.bands is assigned, every sibling field (device, band,
        // sweep.enabled, sweep.dwell_slots) survives untouched (hoi1
        // two-face guard — a partial replace here would wipe device).
        c.ft8.sweep.bands = bands.clone();
        Ok(())
    })
    // The only realistic failure left after the pre-check above is a
    // physical config-write failure (disk full, permissions); no other
    // Ft8CmdError kind in the phase's vocabulary fits a bands-write
    // failure, so it is tagged with this command's own "invalid-band" kind
    // and the real cause travels in `detail`.
    .map_err(|e| Ft8CmdError::new("invalid-band", e))?;
    state.set_ft8_config(ft8);
    state.emit_listening_change();
    Ok(())
}

// ---- tauri shells -------------------------------------------------------

#[tauri::command]
pub async fn ft8_listener_start(
    state: State<'_, Arc<Ft8ListenerState>>,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_listener_start_inner(&s))
        .await
        .map_err(|e| format!("start task failed: {e}"))?
}

#[tauri::command]
pub async fn ft8_listener_stop(
    state: State<'_, Arc<Ft8ListenerState>>,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_listener_stop_inner(&s))
        .await
        .map_err(|e| format!("stop task failed: {e}"))?
}

#[tauri::command]
pub fn ft8_listener_snapshot(
    state: State<'_, Arc<Ft8ListenerState>>,
) -> Result<Ft8Snapshot, String> {
    Ok(state.snapshot())
}

#[tauri::command]
pub async fn ft8_set_device(
    state: State<'_, Arc<Ft8ListenerState>>,
    stable_id: StableAudioId,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_set_device_inner(&s, stable_id))
        .await
        .map_err(|e| format!("set-device task failed: {e}"))?
}

#[tauri::command]
pub async fn ft8_set_band(
    state: State<'_, Arc<Ft8ListenerState>>,
    band: String,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_set_band_inner(&s, band))
        .await
        .map_err(|e| format!("set-band task failed: {e}"))?
}

#[tauri::command]
pub async fn ft8_set_sweep(
    state: State<'_, Arc<Ft8ListenerState>>,
    enabled: bool,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_set_sweep_inner(&s, enabled))
        .await
        .map_err(|e| format!("set-sweep task failed: {e}"))?
}

#[tauri::command]
pub async fn ft8_set_sweep_bands(
    state: State<'_, Arc<Ft8ListenerState>>,
    bands: Vec<String>,
) -> Result<(), Ft8CmdError> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_set_sweep_bands_inner(&s, bands))
        .await
        .map_err(|e| {
            Ft8CmdError::new("invalid-band", format!("set-sweep-bands task failed: {e}"))
        })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Ft8Config;
    use crate::ft8::service::Ft8Deps;
    use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink};
    use crate::modem_commands::test_env::{lock_config_dir, ConfigDirGuard};

    /// Point TUXLINK_CONFIG_DIR at a fresh pid-suffixed tempdir — via the
    /// crate-shared guard (Step 3a), which serializes env mutation AND
    /// restores the prior value on drop — and write a minimal VALID config
    /// there (mirrors the seed fixture shape modem_commands.rs's
    /// `round_trip_persists_through_config` uses). Callers hold the returned
    /// guard for the whole test: `let (_env, _dir) = seed_config();`.
    fn seed_config() -> (ConfigDirGuard, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "tuxlink-ft8-cmd-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let guard = lock_config_dir(&dir);
        let body = serde_json::json!({
            "schema_version": crate::config::CONFIG_SCHEMA_VERSION,
            "wizard_completed": true,
            "connect": { "connect_to_cms": false, "transport": "Telnet" },
            "identity": { "callsign": null, "identifier": "W1TEST", "grid": null },
            "privacy": { "gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid" }
        });
        std::fs::write(dir.join("config.json"), serde_json::to_vec_pretty(&body).unwrap())
            .unwrap();
        (guard, dir)
    }

    /// seed_config + a configured rig (model + serial) so sweep/QSY
    /// validation passes — through the crate-wide gate, like production.
    fn seed_config_with_rig() -> (ConfigDirGuard, std::path::PathBuf) {
        let (guard, dir) = seed_config();
        crate::config::update_config(|c| {
            c.rig.rig_hamlib_model = Some(1043);
            c.rig.cat_serial_path = Some("/dev/ttyUSB0".into());
            Ok(())
        })
        .unwrap();
        (guard, dir)
    }

    fn test_stable_id() -> crate::winlink::ax25::devices::StableAudioId {
        crate::winlink::ax25::devices::StableAudioId {
            kind: crate::winlink::ax25::devices::StableIdKind::ByIdSymlink,
            value: "usb-DRA-100-00".into(),
        }
    }

    fn cfg_with_device() -> Ft8Config {
        Ft8Config { device: Some(test_stable_id()), ..Ft8Config::default() }
    }

    fn state_with_platform(
        platform: Arc<FakePlatform>,
        cfg: Ft8Config,
    ) -> Arc<Ft8ListenerState> {
        crate::ft8::service::Ft8ListenerState::new(
            Ft8Deps {
                platform,
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg,
        )
    }

    fn state_with(cfg: Ft8Config) -> Arc<Ft8ListenerState> {
        state_with_platform(FakePlatform::happy(), cfg)
    }

    /// set_band rejects out-of-table BEFORE any persistence.
    #[test]
    fn set_band_rejects_unknown_bands_without_persisting() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        assert!(ft8_set_band_inner(&state, "23cm".into()).is_err());
        let on_disk = crate::config::read_config().unwrap();
        assert_eq!(on_disk.ft8.band, "20m", "rejected band never reached disk");
    }

    /// set_band while NOT listening: persist-only — the radio is never
    /// touched (zero rig_tune calls on the platform fake).
    #[test]
    fn set_band_not_listening_is_persist_only() {
        let (_env, _dir) = seed_config();
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        let state = state_with_platform(p.clone(), Ft8Config::default());
        ft8_set_band_inner(&state, "40m".into()).unwrap();
        assert!(p.tuned_to.lock().unwrap().is_empty(), "persist-only: no QSY");
        assert_eq!(crate::config::read_config().unwrap().ft8.band, "40m");
    }

    /// set_band while listening + CAT: QSY + relabel + k reset.
    #[test]
    fn set_band_listening_with_cat_qsys_and_relabels() {
        let (_env, _dir) = seed_config_with_rig(); // rig model + serial in the seeded file
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        let state = state_with_platform(p.clone(), cfg_with_device());
        state.test_run_sequence();
        // Age k: two band-dead slots.
        state.record_slot(state.test_base_record(1, crate::ft8::records::RingOutcome::BandDead));
        state.record_slot(state.test_base_record(2, crate::ft8::records::RingOutcome::BandDead));
        assert_eq!(state.snapshot().k_consecutive, 2);
        ft8_set_band_inner(&state, "40m".into()).unwrap();
        let snap = state.snapshot();
        assert_eq!(*p.tuned_to.lock().unwrap().last().unwrap(), 7_074_000);
        assert_eq!(snap.band, "40m");
        assert_eq!(snap.band_source, crate::ft8::records::BandSource::CatConfirmed);
        assert_eq!(snap.k_consecutive, 0, "k resets on band change");
        state.test_teardown();
    }

    /// capture-wedged refuses start AND set_device with the
    /// restart-required error.
    #[test]
    fn wedged_refuses_start_and_set_device() {
        let (_env, _dir) = seed_config();
        let state = state_with(cfg_with_device());
        state.test_force_capture_wedged(); // helper: machine.on_capture_wedged()
        let e1 = ft8_listener_start_inner(&state).unwrap_err();
        let e2 = ft8_set_device_inner(&state, test_stable_id()).unwrap_err();
        for e in [e1, e2] {
            assert!(e.contains("restart Tuxlink"), "restart-required error, got: {e}");
        }
    }

    /// Idempotent start: two starts in a row both Ok; one supervisor.
    #[test]
    fn start_is_idempotent() {
        let (_env, _dir) = seed_config();
        let state = state_with(cfg_with_device());
        ft8_listener_start_inner(&state).unwrap();
        ft8_listener_start_inner(&state).unwrap();
        state.test_teardown();
    }

    /// Writer-mutex serialization: two threads doing set_* RMW cycles
    /// concurrently — both mutations land (no lost update). Threads, not
    /// loom (per the plan: loom not required).
    #[test]
    fn writer_mutex_serializes_concurrent_rmw() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        let s1 = state.clone();
        let s2 = state.clone();
        let t1 = std::thread::spawn(move || ft8_set_band_inner(&s1, "40m".into()));
        let t2 = std::thread::spawn(move || {
            ft8_set_device_inner(&s2, test_stable_id())
        });
        t1.join().unwrap().unwrap();
        t2.join().unwrap().unwrap();
        let on_disk = crate::config::read_config().unwrap();
        assert_eq!(on_disk.ft8.band, "40m", "band write survived");
        assert!(on_disk.ft8.device.is_some(), "device write survived (no lost update)");
    }

    /// CRATE-WIDE gate (Step 2a): an ft8 write racing a NON-ft8 config
    /// write — both fields survive. An ft8-only mutex could not protect
    /// this pairing; config::update_config's one static lock does.
    #[test]
    fn ft8_write_racing_non_ft8_write_loses_neither() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        let s1 = state.clone();
        let t1 = std::thread::spawn(move || ft8_set_band_inner(&s1, "40m".into()));
        let t2 = std::thread::spawn(|| {
            crate::config::update_config(|c| {
                c.rig.rig_hamlib_model = Some(1043);
                Ok(())
            })
        });
        t1.join().unwrap().unwrap();
        t2.join().unwrap().unwrap();
        let on_disk = crate::config::read_config().unwrap();
        assert_eq!(on_disk.ft8.band, "40m", "ft8 write survived");
        assert_eq!(
            on_disk.rig.rig_hamlib_model,
            Some(1043),
            "non-ft8 write survived (no cross-subsystem lost update)"
        );
    }

    /// set_sweep(enabled=true) without a rig is rejected by validate()
    /// inside the writer cycle — nothing persists.
    #[test]
    fn set_sweep_without_rig_is_rejected() {
        let (_env, _dir) = seed_config(); // no rig in the seed
        let state = state_with(Ft8Config::default());
        assert!(ft8_set_sweep_inner(&state, true).is_err());
        assert!(!crate::config::read_config().unwrap().ft8.sweep.enabled);
    }

    /// (a)+(d) valid bands persist to config.ft8.sweep.bands through the
    /// writer mutex, AND the hoi1 two-face guard holds: a sibling field
    /// (device) seeded on disk BEFORE the call survives an unrelated
    /// sweep-bands write untouched.
    #[test]
    fn set_sweep_bands_persists_and_preserves_device() {
        let (_env, _dir) = seed_config();
        crate::config::update_config(|c| {
            c.ft8.device = Some(test_stable_id());
            Ok(())
        })
        .unwrap();
        let state = state_with(Ft8Config::default());
        ft8_set_sweep_bands_inner(&state, vec!["40m".into(), "80m".into()]).unwrap();
        let after = crate::config::read_config().unwrap();
        assert_eq!(after.ft8.sweep.bands, vec!["40m".to_string(), "80m".to_string()]);
        assert_eq!(
            after.ft8.device,
            Some(test_stable_id()),
            "hoi1: device survives an unrelated sweep-bands write"
        );
    }

    /// (b) an out-of-table band anywhere in the list rejects the WHOLE
    /// request before persisting — the on-disk bands list is unchanged.
    #[test]
    fn set_sweep_bands_rejects_out_of_table_band_without_persisting() {
        let (_env, _dir) = seed_config();
        let original = crate::config::read_config().unwrap().ft8.sweep.bands;
        let state = state_with(Ft8Config::default());
        let e = ft8_set_sweep_bands_inner(&state, vec!["20m".into(), "60m".into()]).unwrap_err();
        assert_eq!(e.kind, "invalid-band");
        let after = crate::config::read_config().unwrap();
        assert_eq!(after.ft8.sweep.bands, original, "rejected band list never reached disk");
    }

    /// (c) an empty list is rejected (invalid-band) — Config::validate()'s
    /// per-band loop is a no-op on empty, so this guard lives in the
    /// command, not in validate().
    #[test]
    fn set_sweep_bands_rejects_empty_list() {
        let (_env, _dir) = seed_config();
        let original = crate::config::read_config().unwrap().ft8.sweep.bands;
        let state = state_with(Ft8Config::default());
        let e = ft8_set_sweep_bands_inner(&state, vec![]).unwrap_err();
        assert_eq!(e.kind, "invalid-band");
        let after = crate::config::read_config().unwrap();
        assert_eq!(after.ft8.sweep.bands, original, "empty list never persisted");
    }

    /// A successful sweep-bands write emits ft8-listening:change, same as
    /// the sibling setters (spec §NewCommands `ft8_set_sweep_bands` row).
    #[test]
    fn set_sweep_bands_emits_listening_change() {
        let (_env, _dir) = seed_config();
        let sink = Arc::new(RecordingSink::default());
        let state = crate::ft8::service::Ft8ListenerState::new(
            Ft8Deps {
                platform: FakePlatform::happy(),
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: sink.clone(),
            },
            Ft8Config::default(),
        );
        ft8_set_sweep_bands_inner(&state, vec!["40m".into()]).unwrap();
        assert_eq!(sink.listening_changes.lock().unwrap().len(), 1);
    }

    /// camelCase wire contract: Ft8CmdError's serialized keys are `kind`
    /// and `detail` (no snake_case leakage), per `#[serde(rename_all =
    /// "camelCase")]` — the UI branches on `kind`, never parses strings.
    #[test]
    fn ft8_cmd_error_serializes_camel_case() {
        let e = Ft8CmdError::new("invalid-band", "\"60m\" is not an FT8 band");
        let v = serde_json::to_value(&e).unwrap();
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 2, "exactly kind + detail, no extra fields");
        assert_eq!(v["kind"], "invalid-band");
        assert_eq!(v["detail"], "\"60m\" is not an FT8 band");
    }
}
