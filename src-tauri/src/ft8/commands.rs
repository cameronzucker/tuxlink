//! The FT8 listener Tauri commands (spec §Commands, §NewCommands). Thin
//! async wrappers: validation + the ft8 writer mutex + spawn_blocking into
//! the service's blocking-context surface. Testable bodies live in the
//! `_inner` fns; the `#[tauri::command]` shells only extract state.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::config::{self, Config};
use crate::ft8::meter::{open_and_meter, MeterDto};
use crate::ft8::records::AudioDeviceChoice;
use crate::ft8::service::{Ft8ListenerState, Ft8Snapshot};
use crate::winlink::ax25::devices::StableAudioId;
use tuxlink_capture::state::{BlockedReason, ServiceAxis};

/// Machine-readable error for every FT8 Tauri command in this phase (spec
/// §NewCommands). A2 owns this definition; A3/A4/A5/A7 `use` it — none
/// re-defines it. `kind` is a bare `String` (not an enum) so new tags can
/// land without a breaking type change; the FULL kebab-case vocabulary of
/// error kinds actually EMITTED in this phase is: `device-reserved |
/// device-not-found | modem-busy | rig-not-configured | probe-timeout |
/// invalid-grid | invalid-band`. The UI branches on `kind`, never parses
/// `detail` — `detail` is human-readable text only.
///
/// NOTE: a device being busy is NOT an error kind. `ft8_device_meter`
/// surfaces a contended/busy device as `Ok(MeterDto { state: "in-use" })`
/// (an Ok value, never an EBUSY `Err`), so the frontend reads busy from the
/// meter's `state`, not from an `Ft8CmdError { kind: "device-in-use" }` —
/// that error kind is never produced.
///
/// One additional generic tag, `internal-error`, is reserved for genuine
/// infrastructure failures (a real `write_config_atomic` disk/permission
/// error, a panicked `spawn_blocking` task) — NOT a rejected input. The UI
/// shows a generic "couldn't save" message for `internal-error` and does
/// NOT tell the user to fix their input; the real cause travels in
/// `detail`. Keep the eight validation-facing tags strictly for actual
/// input rejections so the UI never misdirects troubleshooting.
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

/// `ft8_cat_probe` result (spec §NewCommands `ft8_cat_probe` row,
/// tuxlink-b026z.4 Task A4): the rig's current VFO dial, mapped to the
/// nearest FT8 band table entry (`"unknown"` when the dial falls outside
/// every band's ±3 kHz window — mirrors the label `start_rig_labeling`
/// assigns on the same miss). Consumed by the setup surface's "Test CAT"
/// button (Task C9b) and the sweep-enable gate (Task C10).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatProbeDto {
    pub dial_hz: u64,
    pub band: String,
}

/// `ft8_waterfall_subscribe` result (Task A6): the fresh subscription token the
/// frontend holds for the lifetime of an open waterfall view and passes back to
/// `ft8_waterfall_unsubscribe` on window close. Wire key is camelCase
/// (`subscriptionId`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubDto {
    pub subscription_id: String,
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
    // QA round-3 finding 1 (operator ruling 2026-07-12): listening is
    // SESSION-SCOPED. start/stop no longer persist `ft8.enabled`, and boot
    // never autostarts from it — a fresh launch always opens quiet, with a
    // green "Listening" only after the operator turns it on this session.
    // (`enabled` stays in the schema so configs that carry it still parse;
    // it is simply never read or written again.)
    state.start() // idempotent: live supervisor → sequence re-run signal
}

pub(crate) fn ft8_listener_stop_inner(state: &Arc<Ft8ListenerState>) -> Result<(), String> {
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
    // Emit unconditionally right after persist (Task A7), same pattern as
    // set_sweep_bands (~line 219): the L3 panel re-hydrates on every
    // config.ft8 write, so the UI's device-row "selected" state and the
    // CTA's disable-reason clear off this snapshot.
    state.emit_listening_change();
    // Persist-only, ALWAYS (operator decision, 2026-07-12 QA wave 2):
    // set_device used to auto-retrigger `state.start()` from any blocked
    // axis, so the setup surface's "Use this device" row silently started
    // capture — the ONE thing that starts capture must be the explicit
    // Start CTA. A device pick — from any axis, blocked or not — now only
    // persists `config.ft8.device` and emits the change above; it never
    // calls `state.start()`.
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
    // Verified (Task A7): `apply_sweep_enabled` already calls
    // `self.emit_listening_change()` unconditionally at its end
    // (service.rs), so this setter already emits on every call — no
    // separate emit needed here.
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
    let ft8 = with_ft8_config_writer(move |c| {
        // RMW the FULL ft8 config through the crate-wide writer: only
        // .sweep.bands is assigned, every sibling field (device, band,
        // sweep.enabled, sweep.dwell_slots) survives untouched (hoi1
        // two-face guard — a partial replace here would wipe device).
        c.ft8.sweep.bands = bands;
        Ok(())
    })
    // The bands are already validated above, so any error here is a
    // genuine infrastructure failure (disk full, permissions) — NOT a bad
    // band. Tagging it "invalid-band" would misdirect the operator to
    // reselect a band; "internal-error" carries the real cause in `detail`
    // and the UI shows a generic "couldn't save" message instead.
    .map_err(|e| Ft8CmdError::new("internal-error", e))?;
    state.set_ft8_config(ft8);
    state.emit_listening_change();
    Ok(())
}

/// `ft8_device_meter` (spec §NewCommands). The `stable_id` arg is the
/// [`StableAudioId::value`] string of a currently-enumerated capture device
/// (the setup rows pass the value the picker showed). Resolves it against a
/// FRESH enumeration (never a cached name — the card index can change), takes
/// the meter reservation, opens the device, and reads a short level window.
///
/// Error mapping: no enumerated match → `device-not-found`; the listener holds
/// the device (reservation) → `device-reserved`. A device that is open-but-busy
/// or errors surfaces as an `Ok(MeterDto)` with `state:"in-use"`/`"error"` (the
/// live bar shows the condition) — NEVER an unhandled EBUSY.
pub(crate) fn ft8_device_meter_inner(
    state: &Arc<Ft8ListenerState>,
    stable_id: String,
) -> Result<MeterDto, Ft8CmdError> {
    // Fresh enumeration (≤1 s; sysfs read) — resolve the value to a live device.
    let Some(dev) = state
        .platform
        .enumerate_capture()
        .into_iter()
        .find(|d| d.stable_id.value == stable_id)
    else {
        return Err(Ft8CmdError::new(
            "device-not-found",
            format!("no capture device with stable id {stable_id:?}"),
        ));
    };
    // Reservation check: if the listener claimed priority, refuse rather than
    // race its open (spec §NewCommands reservation rule). Hold the guard across
    // the whole ALSA session so a concurrent listener acquire_priority waits for
    // us to finish instead of EBUSY-ing its open into a spurious pause.
    let Some(_meter_guard) = state.reservation().try_meter(&dev.stable_id) else {
        return Err(Ft8CmdError::new(
            "device-reserved",
            "the FT8 listener is opening this device",
        ));
    };
    // Hold `_meter_guard` across the whole read; the read is PREEMPTIBLE — it
    // polls the same reservation for a mid-flight listener priority claim and
    // aborts to `in-use` rather than finishing its window, so the listener's
    // acquire_priority wins within ~one read iteration.
    Ok(open_and_meter(state.reservation(), &dev.stable_id, &dev.alsa_hw))
}

/// `ft8_list_devices` (spec §NewCommands). The same enumeration the snapshot's
/// `availableDevices` uses, incl. `alsaHw`. Enumeration is infallible; the
/// `Ft8CmdError` result arm exists only for the `spawn_blocking` panic path in
/// the shell.
pub(crate) fn ft8_list_devices_inner(
    state: &Arc<Ft8ListenerState>,
) -> Result<Vec<AudioDeviceChoice>, Ft8CmdError> {
    Ok(state.platform.enumerate_capture())
}

/// `ft8_cat_probe` (spec §NewCommands): read-only rig-dial probe. Reads the
/// rig's current VFO via `Ft8Platform::rig_read_dial()` ONLY — no `Inner`
/// state is touched (unlike `start_rig_labeling`'s step-5 QSY, this never
/// writes `band` / `dial_hz` / `band_source`; a "Test CAT" click must not
/// silently relabel the listener's live band out from under it).
///
/// Refusal order (both checks are config/state reads, not rig I/O, so they
/// run before either lock — mirrors the `wedged_refusal` fail-fast shape
/// above and step 5's `rig_configured()` probe at service.rs:804):
/// 1. `modem-busy` when `!platform.modem_resume_eligible()` — a live modem
///    session holds the proceed-set complement (spec's "active set"); the
///    probe refuses rather than contend the rig with it.
/// 2. `rig-not-configured` when `!platform.rig_configured()`.
///
/// **Lock architecture (pinned):** the actual rig read acquires ONLY
/// `state.rig_lock()` — the same leaf lock `start_rig_labeling` and
/// `qsy_to_band` each own internally — wrapped in the arbiter's
/// `rig_session` when installed (arbiter lock only, never the rig lock;
/// same composition as `ft8_set_band_inner`'s CAT-confirmed QSY block
/// above and the supervisor's step 5). Lock order stays arbiter > rig >
/// state, each acquired at most once per thread, so this cannot deadlock
/// against `pause_for_modem` (arbiter, then briefly rig) — the probe never
/// takes the state (`Inner`) mutex at all, so there is no state-lock leg to
/// order against either.
pub(crate) fn ft8_cat_probe_inner(state: &Arc<Ft8ListenerState>) -> Result<CatProbeDto, Ft8CmdError> {
    if !state.platform.modem_resume_eligible() {
        return Err(Ft8CmdError::new(
            "modem-busy",
            "a modem session is active; stop it before probing CAT",
        ));
    }
    if !state.platform.rig_configured() {
        return Err(Ft8CmdError::new("rig-not-configured", "no rig is configured"));
    }
    let read_dial = || -> Result<CatProbeDto, Ft8CmdError> {
        let rig = state.rig_lock();
        let _rig = rig.lock().unwrap_or_else(|p| p.into_inner());
        let dial_hz = state
            .platform
            .rig_read_dial()
            .map_err(|e| Ft8CmdError::new("internal-error", e))?;
        let band = tuxlink_capture::bands::band_for_dial(dial_hz)
            .unwrap_or("unknown")
            .to_string();
        Ok(CatProbeDto { dial_hz, band })
    };
    match crate::ft8::arbiter::FT8_ARBITER.get() {
        Some(arb) => arb.rig_session(read_dial),
        None => read_dial(),
    }
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
    // A JoinError here means the blocking task panicked — an infrastructure
    // failure, not a bad band, so it maps to "internal-error" (the eight
    // validation kinds stay reserved for actual input rejections).
    tauri::async_runtime::spawn_blocking(move || ft8_set_sweep_bands_inner(&s, bands))
        .await
        .map_err(|e| {
            Ft8CmdError::new("internal-error", format!("set-sweep-bands task failed: {e}"))
        })?
}

#[tauri::command]
pub async fn ft8_device_meter(
    state: State<'_, Arc<Ft8ListenerState>>,
    stable_id: String,
) -> Result<MeterDto, Ft8CmdError> {
    let s = (*state).clone();
    // spawn_blocking: the meter opens ALSA + reads a ~250 ms window — blocking
    // work that must stay off the async runtime. A JoinError is a panicked task
    // (infrastructure), so it maps to `internal-error`, not a device tag.
    tauri::async_runtime::spawn_blocking(move || ft8_device_meter_inner(&s, stable_id))
        .await
        .map_err(|e| Ft8CmdError::new("internal-error", format!("meter task failed: {e}")))?
}

#[tauri::command]
pub async fn ft8_list_devices(
    state: State<'_, Arc<Ft8ListenerState>>,
) -> Result<Vec<AudioDeviceChoice>, Ft8CmdError> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_list_devices_inner(&s))
        .await
        .map_err(|e| Ft8CmdError::new("internal-error", format!("list-devices task failed: {e}")))?
}

/// `ft8_cat_probe` (spec §NewCommands). `spawn_blocking` (the rig read is a
/// blocking spawn-read-drop `ManagedRig` session, per `Ft8Platform::
/// rig_read_dial`'s doc-comment) bounded to 3 s: a wedged/unresponsive CAT
/// serial link surfaces as `probe-timeout` instead of hanging the "Test
/// CAT" button forever. The blocking task itself is not cancelled on
/// timeout (std threads cannot be preempted) — it finishes or errors on its
/// own and its result is simply discarded once the command has already
/// returned `probe-timeout`.
#[tauri::command]
pub async fn ft8_cat_probe(
    state: State<'_, Arc<Ft8ListenerState>>,
) -> Result<CatProbeDto, Ft8CmdError> {
    let s = (*state).clone();
    let task = tauri::async_runtime::spawn_blocking(move || ft8_cat_probe_inner(&s));
    match tokio::time::timeout(std::time::Duration::from_secs(3), task).await {
        Ok(join_result) => join_result
            .map_err(|e| Ft8CmdError::new("internal-error", format!("cat-probe task failed: {e}")))?,
        Err(_elapsed) => Err(Ft8CmdError::new(
            "probe-timeout",
            "CAT probe did not complete within 3s",
        )),
    }
}

/// `ft8_waterfall_subscribe` (Task A6): register a waterfall subscription,
/// spawning the single FFT consumer thread on the registry 0→1 edge. Sync +
/// cheap (a HashMap insert + at most one thread spawn); no `spawn_blocking`
/// needed. Idempotent by construction — each call mints a fresh id.
#[tauri::command]
pub fn ft8_waterfall_subscribe(
    state: State<'_, Arc<Ft8ListenerState>>,
) -> Result<SubDto, Ft8CmdError> {
    let s = (*state).clone();
    Ok(SubDto { subscription_id: crate::ft8::waterfall::subscribe(&s) })
}

/// `ft8_waterfall_unsubscribe` (Task A6): release a waterfall subscription id,
/// joining the consumer thread on the registry 1→0 edge. Idempotent: an
/// unknown/duplicate id is a no-op. The join is bounded (~one consumer poll),
/// so a sync command is fine.
#[tauri::command]
pub fn ft8_waterfall_unsubscribe(
    state: State<'_, Arc<Ft8ListenerState>>,
    subscription_id: String,
) -> Result<(), Ft8CmdError> {
    let s = (*state).clone();
    crate::ft8::waterfall::unsubscribe(&s, &subscription_id);
    Ok(())
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

    /// QA round-3 finding 1 (operator ruling): listening is session-scoped —
    /// start/stop must NOT persist `ft8.enabled`, so a later launch always
    /// opens quiet instead of autostarting into a green "Listening" chip.
    #[test]
    fn start_and_stop_do_not_persist_enabled() {
        let (_env, _dir) = seed_config();
        let state = state_with(cfg_with_device());
        ft8_listener_start_inner(&state).unwrap();
        assert!(!crate::config::read_config().unwrap().ft8.enabled);
        ft8_listener_stop_inner(&state).unwrap();
        assert!(!crate::config::read_config().unwrap().ft8.enabled);
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

    // ---- Task A7: emit on set_device + hoi1 two-face guards on ALL FOUR
    // config.ft8 writers (set_device, set_band, set_sweep — set_sweep_bands
    // already covered above). ------------------------------------------------

    /// (a) a successful `ft8_set_device` write emits ft8-listening:change,
    /// same as the sibling setters — closes the Task A7 emit gap (the L3
    /// panel re-hydrates on every config.ft8 writer, not just
    /// `set_sweep_bands`).
    #[test]
    fn set_device_emits_listening_change() {
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
        ft8_set_device_inner(&state, test_stable_id()).unwrap();
        assert_eq!(sink.listening_changes.lock().unwrap().len(), 1);
    }

    /// (b) hoi1 for `ft8_set_device`: a sweep.bands list seeded on disk
    /// BEFORE the call survives an unrelated device-pick write untouched
    /// (testing-pitfalls §7 absent-field-erases / multi-writer clobber).
    #[test]
    fn set_device_preserves_sweep_bands() {
        let (_env, _dir) = seed_config();
        crate::config::update_config(|c| {
            c.ft8.sweep.bands = vec!["20m".into(), "40m".into()];
            Ok(())
        })
        .unwrap();
        let state = state_with(Ft8Config::default());
        ft8_set_device_inner(&state, test_stable_id()).unwrap();
        let after = crate::config::read_config().unwrap();
        assert_eq!(
            after.ft8.sweep.bands,
            vec!["20m".to_string(), "40m".to_string()],
            "hoi1: sweep.bands survives an unrelated set_device write"
        );
    }

    // ---- Operator QA wave 2 (2026-07-12): "Use this device" selects, only
    // the Start CTA starts. `ft8_set_device_inner` used to auto-retrigger
    // `state.start()` from any Blocked axis; that made the setup surface's
    // device-row click silently start capture, which the operator called
    // out as wrong — the row picks a device, the CTA is the one thing that
    // starts listening. -----------------------------------------------------

    /// A device pick from a Blocked axis (`needs-device-selection`, reached
    /// here via a real `test_run_sequence()` with no device configured) is
    /// now persist-only: the axis is UNCHANGED after `ft8_set_device_inner`
    /// returns, even though the picked device would otherwise resolve fine.
    #[test]
    fn set_device_from_blocked_is_persist_only_axis_unchanged() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        state.test_run_sequence();
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::NeedsDeviceSelection));

        ft8_set_device_inner(&state, test_stable_id()).unwrap();

        assert_eq!(
            state.axis(),
            ServiceAxis::Blocked(BlockedReason::NeedsDeviceSelection),
            "set_device is persist-only now — it must never auto-start capture"
        );
        assert_eq!(
            crate::config::read_config().unwrap().ft8.device,
            Some(test_stable_id()),
            "the device pick still persists even though it no longer starts"
        );
        state.test_teardown();
    }

    /// (c) hoi1 for `ft8_set_band`: a device seeded on disk BEFORE the call
    /// survives an unrelated band write untouched (not-listening path, so
    /// the write is persist-only — mirrors
    /// `set_band_not_listening_is_persist_only`).
    #[test]
    fn set_band_preserves_device() {
        let (_env, _dir) = seed_config();
        crate::config::update_config(|c| {
            c.ft8.device = Some(test_stable_id());
            Ok(())
        })
        .unwrap();
        let state = state_with(Ft8Config::default());
        ft8_set_band_inner(&state, "40m".into()).unwrap();
        let after = crate::config::read_config().unwrap();
        assert_eq!(
            after.ft8.device,
            Some(test_stable_id()),
            "hoi1: device survives an unrelated set_band write"
        );
    }

    /// (d) hoi1 for `ft8_set_sweep`: a device AND sweep.bands seeded on disk
    /// BEFORE the call BOTH survive an unrelated sweep-enabled write
    /// untouched. Uses `seed_config_with_rig` because `validate()` enforces
    /// sweep.enabled ⇒ rig configured (mirrors
    /// `set_sweep_without_rig_is_rejected`'s negative case).
    #[test]
    fn set_sweep_preserves_device_and_bands() {
        let (_env, _dir) = seed_config_with_rig();
        crate::config::update_config(|c| {
            c.ft8.device = Some(test_stable_id());
            c.ft8.sweep.bands = vec!["20m".into(), "40m".into()];
            Ok(())
        })
        .unwrap();
        let state = state_with(Ft8Config::default());
        ft8_set_sweep_inner(&state, true).unwrap();
        let after = crate::config::read_config().unwrap();
        assert_eq!(
            after.ft8.device,
            Some(test_stable_id()),
            "hoi1: device survives an unrelated set_sweep write"
        );
        assert_eq!(
            after.ft8.sweep.bands,
            vec!["20m".to_string(), "40m".to_string()],
            "hoi1: sweep.bands survives an unrelated set_sweep write"
        );
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

    // ---- ft8_device_meter + ft8_list_devices (spec §NewCommands) -----------

    /// A `stable_id` that matches no enumerated device → `device-not-found`
    /// (before any ALSA open is attempted).
    #[test]
    fn device_meter_unknown_stable_id_is_not_found() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        let e = ft8_device_meter_inner(&state, "nonexistent-card-99".into()).unwrap_err();
        assert_eq!(e.kind, "device-not-found");
    }

    /// While the listener holds priority on the device, a concurrent meter is
    /// refused with `device-reserved` — it never reaches the ALSA open, so it
    /// cannot race the listener into a spurious pause. This is the command-level
    /// face of the reservation barrier (service.rs owns the barrier-race unit).
    #[test]
    fn device_meter_is_reserved_while_listener_holds_priority() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        // FakePlatform::happy() enumerates one device: value "usb-DRA-100-00".
        let id = test_stable_id();
        let _prio = state.reservation().acquire_priority(&id);
        let e = ft8_device_meter_inner(&state, id.value.clone()).unwrap_err();
        assert_eq!(e.kind, "device-reserved");
    }

    /// `ft8_list_devices` returns the same shape as the snapshot's
    /// `availableDevices`, including the camelCase `alsaHw` key.
    #[test]
    fn list_devices_returns_enumeration_with_alsa_hw() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        let devices = ft8_list_devices_inner(&state).unwrap();
        assert_eq!(devices.len(), 1, "FakePlatform::happy enumerates one device");
        assert_eq!(devices[0].alsa_hw, "hw:1,0");
        let v = serde_json::to_value(&devices).unwrap();
        assert_eq!(v[0]["alsaHw"], "hw:1,0", "camelCase wire key");
        assert!(v[0]["stableId"].is_object(), "stable id serialized as {{kind,value}}");
    }

    // ---- ft8_cat_probe (spec §NewCommands, tuxlink-b026z.4 Task A4) --------

    /// (a) happy path: rig configured, dial at the pinned 20m frequency, no
    /// active modem session → Ok, camelCase `dialHz` on the wire.
    #[test]
    fn cat_probe_happy_path_reads_dial_and_labels_band() {
        let (_env, _dir) = seed_config();
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        // FakePlatform::happy() already seeds rig_dial = Ok(14_074_000) and
        // modem_eligible = true (proceed-set default: Stopped/Error/SocketLost).
        let state = state_with_platform(p, Ft8Config::default());
        let dto = ft8_cat_probe_inner(&state).unwrap();
        assert_eq!(dto.dial_hz, 14_074_000);
        assert_eq!(dto.band, "20m");
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["dialHz"], 14_074_000, "camelCase wire key");
        assert_eq!(v["band"], "20m");
    }

    /// (b) an ACTIVE modem session (state NOT in the proceed set —
    /// `modem_resume_eligible()` false) refuses with `modem-busy`; the term
    /// split is spec-pinned: proceed set = `{Stopped, Error, SocketLost}`,
    /// active set = its complement.
    #[test]
    fn cat_probe_refuses_modem_busy_during_active_modem_session() {
        let (_env, _dir) = seed_config();
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        *p.modem_eligible.lock().unwrap() = false; // active set: modem session live
        let state = state_with_platform(p, Ft8Config::default());
        let e = ft8_cat_probe_inner(&state).unwrap_err();
        assert_eq!(e.kind, "modem-busy");
    }

    /// (c) no `Config.rig` (`platform.rig_configured()` false) →
    /// `rig-not-configured`.
    #[test]
    fn cat_probe_refuses_rig_not_configured() {
        let (_env, _dir) = seed_config();
        // FakePlatform::happy() defaults rig_configured to false.
        let state = state_with(Ft8Config::default());
        let e = ft8_cat_probe_inner(&state).unwrap_err();
        assert_eq!(e.kind, "rig-not-configured");
    }

    /// (d) the probe is read-only: `Inner`'s band/dial_hz/band_source
    /// (surfaced via the snapshot — `Inner` itself is private to
    /// service.rs) are unchanged before and after — unlike
    /// `start_rig_labeling`'s step-5 QSY, a "Test CAT" click must never
    /// relabel the listener's live band out from under it.
    #[test]
    fn cat_probe_does_not_mutate_inner_state() {
        let (_env, _dir) = seed_config();
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        let state = state_with_platform(p, Ft8Config::default());
        let before = state.snapshot();
        ft8_cat_probe_inner(&state).unwrap();
        let after = state.snapshot();
        assert_eq!(before.band, after.band, "band unchanged");
        assert_eq!(before.dial_hz, after.dial_hz, "dial_hz unchanged");
        assert_eq!(before.band_source, after.band_source, "band_source unchanged");
    }

    /// A dial outside every band's ±3 kHz window labels `"unknown"` rather
    /// than erroring — mirrors the label `start_rig_labeling` assigns on the
    /// same miss (service.rs's `nearest_band` fallback).
    #[test]
    fn cat_probe_out_of_band_dial_labels_unknown() {
        let (_env, _dir) = seed_config();
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        *p.rig_dial.lock().unwrap() = Ok(9_000_000); // between 40m and 30m, outside tolerance
        let state = state_with_platform(p, Ft8Config::default());
        let dto = ft8_cat_probe_inner(&state).unwrap();
        assert_eq!(dto.band, "unknown");
        assert_eq!(dto.dial_hz, 9_000_000);
    }

    /// Refusal PRECEDENCE: when BOTH the rig is unconfigured AND a modem
    /// session is active, `modem-busy` wins — it is checked first (pins the
    /// documented order against a future reordering that would leak
    /// `rig-not-configured` while a modem holds the rig).
    #[test]
    fn cat_probe_modem_busy_precedes_rig_not_configured() {
        let (_env, _dir) = seed_config();
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = false; // also unconfigured
        *p.modem_eligible.lock().unwrap() = false; // active modem session
        let state = state_with_platform(p, Ft8Config::default());
        let e = ft8_cat_probe_inner(&state).unwrap_err();
        assert_eq!(e.kind, "modem-busy", "modem-busy is checked before rig-not-configured");
    }

    /// A `rig_read_dial` I/O failure surfaces as `internal-error` (a genuine
    /// infrastructure failure, not a rejected input) — the eight
    /// validation-facing kinds stay reserved per this module's top
    /// doc-comment.
    #[test]
    fn cat_probe_rig_read_failure_is_internal_error() {
        let (_env, _dir) = seed_config();
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        *p.rig_dial.lock().unwrap() = Err("rigctld spawn failed".into());
        let state = state_with_platform(p, Ft8Config::default());
        let e = ft8_cat_probe_inner(&state).unwrap_err();
        assert_eq!(e.kind, "internal-error");
    }
}
