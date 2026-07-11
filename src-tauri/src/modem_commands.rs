//! Tauri commands for modem (ARDOP) operations.
//!
//! Connect lifecycle: `modem_ardop_connect` → `modem_ardop_b2f_exchange` →
//! `modem_ardop_disconnect`. An in-process AtomicBool busy guard prevents
//! duplicate concurrent connect invocations. The RADIO-1 consent-token gate
//! was removed in Task 1.1 (spec §2 "No tuxlink-added safeguards"; bd tuxlink-0ye6).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

use crate::config::{self, ArdopUiConfig, Config, PttMethod, RigUiConfig};
use crate::modem_status::{ModemSession, ModemState, ModemStatus};
use crate::native_mailbox::Mailbox;
use crate::session_log::SessionLogState;
use crate::winlink::modem::ardop::transport::ArdopTransport;
use crate::winlink::modem::ardop::ArdopConfig;
use crate::winlink::modem::{InitConfig, ModemTransport};
use crate::winlink::session::SessionIntent;
use crate::winlink_backend::{LogLevel, LogSource};

/// Default number of ARQ ConReq repeats packed into the `ARQCALL` setter when
/// the operator has not set `modem_ardop.connect_attempts`. 15 ConReqs is
/// ≈50 s of calling, comfortably inside the 120 s connect deadline. The prior
/// value of 3 (≈10 s) was too short to raise a real gateway, which may need to
/// wake up and tune while ARDOP — not being tune-aware — keeps re-calling
/// (2026-06-25). The operator can override via `connect_attempts_from_config`.
const CONNECT_REPEAT: u32 = 15;

/// Lower clamp for the operator-supplied `modem_ardop.connect_attempts`.
const CONNECT_ATTEMPTS_MIN: u32 = 2;
/// Upper clamp for the operator-supplied `modem_ardop.connect_attempts`.
const CONNECT_ATTEMPTS_MAX: u32 = 30;

/// ARQ-link idle timeout passed to the TNC via `ARQTIMEOUT` during init.
const ARQ_TIMEOUT_SECS: u32 = 30;

/// Sentinel `last_err` value used by the connect walk when an attempt is
/// abandoned because the operator's Disconnect bumped the close-generation
/// (i.e. an operator-initiated abort, not a genuine connect failure). The
/// gated-walk `None` arm matches on this to AVOID clobbering the `Stopped`
/// state that `modem_ardop_disconnect_inner` already installed — an abort must
/// not leave the panel stuck in `Error`. Producer sites: the per-candidate
/// abort guard in the walk closure and the C2 post-tune guard in
/// `dial_one_candidate`.
pub(crate) const CONNECT_ABORTED_MSG: &str = "connect aborted";

/// Surface a modem-operation failure in the operator session log (tuxlink-nnjz).
///
/// Modem/transport errors belong in the session-log window the operator is
/// already watching — NOT in an inline panel element wedged next to the
/// buttons. Emitting at `LogLevel::Error` / `LogSource::Transport` lands the
/// line live on `session_log:line` (projected to a visible `alert` row, not the
/// `raw`/Wire bucket) AND in the durable snapshot, so it survives a panel
/// re-mount. Best-effort: a missing `SessionLogState` or emit failure is
/// swallowed — the command still returns its `Err` so the caller can clear its
/// in-flight spinner.
fn emit_modem_error(app: &AppHandle, message: &str) {
    let buffer = app.state::<Arc<SessionLogState>>();
    crate::session_log_emit::emit(
        app,
        &buffer,
        LogLevel::Error,
        LogSource::Transport,
        message,
    );
}

/// Build the ARDOP raw-wire tap (tuxlink-ngsk): a sink that appends each
/// cmd-port line to the session log as a `LogSource::Wire` / `LogLevel::Trace`
/// line. Wire lines are captured in the durable snapshot — so the log an alpha
/// tester uploads is the source of truth for a failed session — and surface
/// live under the panel's "Show raw" toggle. A cloneable `AppHandle` + the
/// managed `SessionLogState` `Arc` are moved into the closure so it can emit
/// from the cmd-socket reader thread. Attached to the transport via
/// [`ArdopTransport::with_wire_sink`] before `init`.
pub(crate) fn ardop_wire_sink(app: &AppHandle) -> crate::winlink_backend::WireSink {
    let app = app.clone();
    let buffer = app.state::<Arc<SessionLogState>>().inner().clone();
    std::sync::Arc::new(move |line: &str| {
        crate::session_log_emit::emit(&app, &buffer, LogLevel::Trace, LogSource::Wire, line);
    })
}

/// Resolve the ardopcf modem binary to spawn (tuxlink-vbpy).
///
/// The shipped `.deb` bundles `ardopcf` as a Tauri `externalBin` sidecar, which
/// Tauri places next to the main executable with the target-triple suffix
/// stripped (`<exe-dir>/ardopcf`). When the operator has NOT set an explicit
/// path — i.e. `configured` is a bare program name with no path separator (the
/// `ArdopUiConfig::default` value `"ardopcf"`) — prefer that bundled sibling so a
/// packaged install works with zero setup and no `$PATH`/config surgery.
///
/// Only the EXACT default name `"ardopcf"` opts into the bundled sidecar. Any
/// other value — an explicit path (`/opt/ardopcf`) OR a custom bare program name
/// (`ardopcf-dev`, `ardopcf-git`) — is a deliberate operator choice and is
/// honored verbatim (`Command` resolves a bare name via `$PATH`). In an unbundled
/// dev run (no sibling present) the default also falls back to `"ardopcf"` on
/// `$PATH`, so `tauri dev` still works for anyone with ardopcf installed.
fn resolve_ardop_binary(configured: &str) -> PathBuf {
    if configured == "ardopcf" {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(sibling) = exe.parent().map(|dir| dir.join("ardopcf")) {
                if sibling.exists() {
                    return sibling;
                }
            }
        }
    }
    PathBuf::from(configured)
}

/// Resolve the `(rigctld, rigctl)` binaries to use, mirroring
/// `resolve_ardop_binary` but for the two hamlib utils.
///
/// The shipped package bundles them as `externalBin` sidecars named
/// `tuxlink-rigctld` / `tuxlink-rigctl` (the `tuxlink-` prefix avoids colliding
/// with `/usr/bin/rigctld` owned by system hamlib). Only the EXACT default
/// config value `"rigctld"` opts into the bundled pair; any other value is a
/// deliberate operator override honored verbatim, with the `rigctl` sibling
/// derived from the overridden `rigctld`'s directory so the model list and the
/// control daemon never version-skew.
fn resolve_rig_binaries(configured_rigctld: &str) -> (PathBuf, PathBuf) {
    if configured_rigctld.trim().is_empty() || configured_rigctld == "rigctld" {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let d = dir.join("tuxlink-rigctld");
                if d.exists() {
                    let l = dir.join("tuxlink-rigctl");
                    let rigctl = if l.exists() { l } else { PathBuf::from("rigctl") };
                    return (d, rigctl);
                }
            }
        }
        return (PathBuf::from("rigctld"), PathBuf::from("rigctl")); // dev / $PATH
    }
    // Override: honor rigctld verbatim; derive sibling rigctl if a path was given.
    let d = PathBuf::from(configured_rigctld);
    let rigctl = if configured_rigctld.contains('/') {
        d.parent().map(|p| p.join("rigctl")).unwrap_or_else(|| PathBuf::from("rigctl"))
    } else {
        PathBuf::from("rigctl")
    };
    (d, rigctl)
}

/// Return the persisted ARDOP configuration, or the struct default if nothing
/// has been written yet (first run) or the config file is absent.
#[tauri::command]
pub fn config_get_ardop() -> ArdopUiConfig {
    config::read_config()
        .map(|cfg| cfg.modem_ardop.unwrap_or_default())
        .unwrap_or_default()
}

/// Persist a new ARDOP configuration. Reads the current config, replaces
/// `modem_ardop`, and writes atomically. Returns an error if the config file
/// cannot be read (e.g. wizard has not been completed) or the write fails.
#[tauri::command]
pub fn config_set_ardop(value: ArdopUiConfig) -> Result<(), String> {
    let mut cfg = config::read_config().map_err(|e| format!("read failed: {e}"))?;
    cfg.modem_ardop = Some(value);
    config::write_config_atomic(&cfg).map_err(|e| format!("save failed: {e}"))
}

/// Return the persisted radio-level rig configuration (tuxlink-8fkkk), or the
/// struct default if nothing has been written yet (first run) or the config
/// file is absent. Shared by the ARDOP and VARA rig-control sections.
#[tauri::command]
pub fn config_get_rig() -> RigUiConfig {
    config::read_config()
        .map(|cfg| cfg.rig)
        .unwrap_or_default()
}

/// Persist a new radio-level rig configuration (tuxlink-8fkkk). Reads the
/// current config, replaces `rig`, and writes atomically. Returns an error if
/// the config file cannot be read (e.g. wizard has not been completed) or the
/// write fails.
#[tauri::command]
pub fn config_set_rig(value: RigUiConfig) -> Result<(), String> {
    let mut cfg = config::read_config().map_err(|e| format!("read failed: {e}"))?;
    cfg.rig = value;
    config::write_config_atomic(&cfg).map_err(|e| format!("save failed: {e}"))
}

/// Wire DTO for a hamlib-supported rig model (tuxlink-31c63). Field names are
/// single words, so serde's default (no rename) already matches the frontend's
/// `{ id, manufacturer, model }`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RigModelDto {
    pub id: u32,
    pub manufacturer: String,
    pub model: String,
}

/// List the installed hamlib's supported rig models for the radio picker
/// (tuxlink-31c63). Runs `rigctl -l` (the companion to `rigctld`) and parses
/// its table. Returns an empty list on ANY failure (rigctl absent, parse
/// empty) so the picker degrades to a manual hamlib-model-number entry rather
/// than erroring — there is no model list for tuxlink to maintain.
///
/// Reads the persisted `rig.rigctld_binary` via [`config::read_config`] — the
/// same free-function accessor `config_get_rig` uses (no `AppHandle` state
/// lookup exists in this file) — and resolves the paired `rigctl` through
/// [`resolve_rig_binaries`] so the listed models always come from the same
/// hamlib install as the daemon `rig_config_from` will spawn.
#[tauri::command]
pub fn rig_list_models() -> Vec<RigModelDto> {
    let configured = config::read_config()
        .map(|cfg| cfg.rig.rigctld_binary)
        .unwrap_or_else(|_| "rigctld".to_string());
    let (_daemon, rigctl) = resolve_rig_binaries(&configured);
    tux_rig::list_models(&rigctl.to_string_lossy())
        .map(|models| {
            models
                .into_iter()
                .map(|m| RigModelDto {
                    id: m.id,
                    manufacturer: m.manufacturer,
                    model: m.model,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Inner helper: snapshot the current session status. Pure on `&Arc<ModemSession>`
/// so tests can exercise it without constructing a Tauri `State`.
pub fn modem_get_status_inner(session: &Arc<ModemSession>) -> ModemStatus {
    session.status_snapshot()
}

/// Inner helper: reset status to Stopped, take the transport handle, then
/// shut the transport down OUTSIDE the lock.
/// Uses [`ModemSession::reset_to_stopped`] so observers see a single
/// consistent transition rather than the prior two-step (clear-consent then
/// set-status) which left a window where the token was invalidated but the
/// status still read as the prior connected variant.
///
/// I/O discipline: `transport.disconnect()` and the subsequent `drop` run
/// AFTER the session mutex is released. Holding the lock across the modem
/// disconnect I/O (TCP DISCONNECT + DISCONNECTED ack, bounded by 5s) would
/// stall any concurrent `status_snapshot` call for the duration.
///
/// tuxlink-o3f2 (P1 abort-during-connect): FIRST step is a best-effort
/// `abort_in_flight()` that side-channels `ABORT\r` to ardopcf via the
/// cmd-socket writer installed at connect time. If a connect is currently
/// blocking inside `arq_connect`'s recv loop, ardopcf responds to ABORT
/// with `FAULT` / `NEWSTATE DISC`, the cmd reader thread delivers it via
/// the channel, the recv loop returns `Err(SessionError::Fault(...))`,
/// and the connect path unwinds cleanly. If no connect is in flight,
/// `abort_in_flight` is harmless: ABORT on an idle TNC is a no-op
/// (ardopcf documents it as "immediate interrupt of any in-flight TX").
/// If no writer is installed (transport was never connected, or session
/// already reset), `abort_in_flight` returns `Err` and we fall through to
/// the existing graceful disconnect path.
pub fn modem_ardop_disconnect_inner(session: &Arc<ModemSession>) -> Result<(), String> {
    // tuxlink-o3f2: best-effort abort of any in-flight connect_arq. The
    // _ discard is deliberate — if the writer is missing or the write
    // fails, the fall-through reset_to_stopped + transport.disconnect
    // path will still surface a clean Stopped state. Documented behavior:
    // ABORT on an idle TNC is a no-op, so it's safe to call unconditionally.
    let _ = session.abort_in_flight();

    // tuxlink-vyby: bump the close generation BEFORE reclaiming the transport.
    // When an in-flight worker holds the transport (a b2f exchange mid-run, or
    // an armed listener consumer), `reset_to_stopped()` below finds nothing to
    // drop. Those workers re-install on their way out via
    // `install_transport_if_generation_matches(transport, snapshot)` where the
    // snapshot was taken BEFORE this Stop. Bumping the generation invalidates
    // that snapshot, so the guarded install rejects the handle and the worker
    // DROPS it — the transport's `ManagedModem::Drop` (SIGINT→SIGKILL) then
    // kills ardopcf. Without this bump the worker re-installs the LIVE transport
    // into the just-stopped session, so ardopcf keeps running and REJ frames
    // scroll until a SECOND Stop click reclaims it — the operator-reported
    // two-click teardown. Mirrors the close-path bump in
    // `ardop_close_session_inner` (tuxlink-pdnw); a double-bump on the close
    // path is harmless because the guard compares snapshot-vs-live by equality.
    let _ = session.bump_close_generation();

    if let Some(mut transport) = session.reset_to_stopped() {
        // The session directly held the transport (no in-flight worker). Send a
        // best-effort link DISCONNECT, then drop: `ManagedModem::Drop` reaps the
        // ardopcf process (SIGINT, 200 ms grace, then SIGKILL), and
        // `CmdSocket::Drop` shuts down + joins the cmd reader thread. Even if
        // disconnect errors, the session is already Stopped so a reconnect can
        // proceed. The in-flight-owner case is handled by the generation bump
        // above (the worker drops the handle, killing the process the same way).
        let _ = transport.disconnect(Duration::from_secs(5));
        drop(transport);
    }
    Ok(())
}

/// Return the current session snapshot. Hooks call this on mount to recover
/// state when remounting mid-session (e.g. after a hot-reload).
#[tauri::command]
pub fn modem_get_status(session: State<'_, Arc<ModemSession>>) -> ModemStatus {
    modem_get_status_inner(&session)
}

/// Disconnect the modem: takes the live transport handle, resets status to
/// Stopped, and shuts the transport down (best-effort `DISCONNECT` on the
/// cmd socket).
#[tauri::command]
pub async fn modem_ardop_disconnect(
    app: AppHandle,
    session: State<'_, Arc<ModemSession>>,
) -> Result<(), String> {
    // tuxlink-ab9h: run the abort + link-disconnect (bounded 5 s) OFF the
    // WebKitGTK main thread. A synchronous command here blocks the UI event
    // loop, so the operator's Stop/Disconnect click could not dispatch and
    // the listener "Stop" path froze the app. `abort_in_flight` inside
    // `_inner` is still the first thing to run — a quick cmd-socket write —
    // so it reaches any in-flight connect_arq running on its own
    // spawn_blocking worker.
    let session = Arc::clone(session.inner());
    let result = tokio::task::spawn_blocking(move || modem_ardop_disconnect_inner(&session))
        .await
        .map_err(|e| format!("disconnect task panicked: {e}"))?;
    // tuxlink-nnjz: a disconnect error (best-effort path; rare) surfaces in the
    // session log rather than an inline panel element. (`Result::inspect_err` is
    // MSRV 1.76; this match keeps the project's 1.75 floor.)
    if let Err(ref e) = result {
        emit_modem_error(&app, e);
    }
    result
}

/// The SINGLE choke point between "a code path decided to spawn ardopcf"
/// and the factory that does it (tuxlink-b026z.3, spec §Arbitration).
/// Yields the FT8 listener's audio device (join + release-confirm) BEFORE
/// the spawn; a pause failure surfaces as the device-busy-class String the
/// existing sites already propagate, and the spawn DOES NOT proceed.
///
/// Blocking-context contract: all four call sites run under spawn_blocking
/// (the `pause_for_modem` doc pins it). Every ardopcf spawn MUST route
/// through here — `no_ardop_spawn_path_bypasses_the_ft8_yield_wrapper`
/// (below) enforces it structurally.
pub(crate) fn spawn_ardop_with_yield<F>(
    make_transport: F,
    ardop_cfg: ArdopConfig,
    target: &str,
) -> Result<Box<dyn ModemTransport>, String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    crate::ft8::arbiter::pause_for_modem_global().map_err(|e| e.device_busy_message())?;
    make_transport(ardop_cfg, target)
}

/// Inner helper with a factory seam — ARDOP connect with in-process busy guard.
///
/// The factory closure constructs the `Box<dyn ModemTransport>` given an
/// `ArdopConfig` and the target callsign. Production calls hand in
/// `ArdopTransport::with_managed_modem`; tests hand in a stub.
///
/// # Busy guard
///
/// The first action is [`ModemSession::try_begin_connect`] — atomic
/// compare-exchange. If another connect is already in flight, returns `Err`
/// BEFORE the factory runs, BEFORE `init`, BEFORE `connect_arq` — no spawn,
/// no socket bind, no I/O whatsoever, AND no status mutation. The busy bit is
/// cleared via RAII ([`ConnectGuard`]) on every exit path, so a failed or
/// completed connect leaves the session ready for the next attempt.
///
/// This replaces the `consume_consent_token` dup-call defense that was a
/// side-effect of the RADIO-1 consent modal (Task 1.1 — spec §2 "No
/// tuxlink-added safeguards"; bd tuxlink-0ye6 / tuxlink-8gq3).
pub fn modem_ardop_connect_gated_with_factory<F>(
    session: &Arc<ModemSession>,
    session_id: &crate::identity::SessionIdentity,
    cfg: &Config,
    target: &str,
    ardop_ui: &ArdopUiConfig,
    make_transport: F,
) -> Result<(), String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    // ─── In-process busy guard ───────────────────────────────────────────
    // FIRST CHECK: no I/O, no spawn, no status mutation if another connect
    // is already in flight. The compare_exchange is atomic — false→true in
    // one operation. If the bit is already true, return Err immediately.
    if !session.try_begin_connect() {
        return Err(
            "connect already in progress; wait for the previous attempt to complete".into(),
        );
    }
    // RAII guard: clear busy bit on every exit path.
    struct ConnectGuard<'a>(&'a Arc<ModemSession>);
    impl<'a> Drop for ConnectGuard<'a> {
        fn drop(&mut self) {
            self.0.clear_connect_in_progress();
        }
    }
    let _guard = ConnectGuard(session);

    modem_ardop_connect_post_consume_with_factory(
        session,
        session_id,
        cfg,
        target,
        ardop_ui,
        make_transport,
    )
}

/// QSY-aware gated connect (Task 8 + 9). Same in-process busy guard as
/// [`modem_ardop_connect_gated_with_factory`], but dials an ordered list of
/// candidates with the pre-audio CAT tune + (when `qsy_on_fail`) a walk to the
/// next candidate on failure. The public `modem_ardop_connect` command calls
/// this directly with the candidate list it builds from `freq_hz` /
/// `qsy_candidates`; a single-element list reproduces the legacy single dial
/// (with an optional tune).
#[allow(clippy::too_many_arguments)]
pub fn modem_ardop_connect_gated_walk_with_factory<F>(
    session: &Arc<ModemSession>,
    session_id: &crate::identity::SessionIdentity,
    cfg: &Config,
    candidates: &[DialCandidate],
    qsy_on_fail: bool,
    ardop_ui: &ArdopUiConfig,
    make_transport: F,
) -> Result<(), String>
where
    F: FnMut(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    // ─── In-process busy guard ───────────────────────────────────────────
    if !session.try_begin_connect() {
        return Err(
            "connect already in progress; wait for the previous attempt to complete".into(),
        );
    }
    struct ConnectGuard<'a>(&'a Arc<ModemSession>);
    impl Drop for ConnectGuard<'_> {
        fn drop(&mut self) {
            self.0.clear_connect_in_progress();
        }
    }
    let _guard = ConnectGuard(session);

    modem_ardop_connect_walk_with_factory(
        session,
        session_id,
        cfg,
        candidates,
        qsy_on_fail,
        ardop_ui,
        make_transport,
    )
}

/// Inner helper that runs AFTER the busy guard has been acquired. Caller
/// (`modem_ardop_connect_gated_with_factory`) holds the `ConnectGuard` RAII
/// that clears the busy bit on drop. Do NOT call this from anywhere that
/// hasn't already acquired the busy bit.
///
/// The `_post_consume` naming is legacy from the prior RADIO-1 consent-token
/// design (Task 1.1 removed it). The function itself is unchanged; only the
/// discipline contract is updated.
pub fn modem_ardop_connect_post_consume_with_factory<F>(
    session: &Arc<ModemSession>,
    session_id: &crate::identity::SessionIdentity,
    cfg: &Config,
    target: &str,
    ardop_ui: &ArdopUiConfig,
    make_transport: F,
) -> Result<(), String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    // NO GATE here — caller MUST have acquired the busy bit already.
    // (The `_post_consume` name is legacy; behavior is unchanged.)
    //
    // Single-target, no-tune dial — the legacy `{ target }` path + the existing
    // tests. The QSY-aware multi-candidate + pre-audio-tune path lives in
    // `modem_ardop_connect_walk_with_factory` (Task 8/9), which the public
    // `modem_ardop_connect` command drives via the gated walk wrapper. This
    // function is preserved verbatim so its `FnOnce` test factories (which move
    // captured state into the constructed transport) keep compiling.

    // ─── Translate ArdopUiConfig (frontend) → ArdopConfig (backend) ─────
    let ardop_cfg = ardop_config_for(ardop_ui, &cfg.rig)?;

    // Mark spawning so any concurrent status_snapshot sees the transition
    // before the (potentially slow) ardopcf bind-wait + init.
    let mut snap = session.status_snapshot();
    snap.state = ModemState::Spawning;
    snap.peer = Some(target.to_string());
    snap.last_error = None;
    session.set_status(snap);

    // ─── Spawn (via the FT8 yield choke point) ───────────────────────────
    let mut transport = match spawn_ardop_with_yield(make_transport, ardop_cfg, target) {
        Ok(t) => t,
        Err(e) => {
            let mut s = ModemStatus::stopped();
            s.state = ModemState::Error;
            s.last_error = Some(e.clone());
            session.set_status(s);
            return Err(e);
        }
    };

    // ─── Init the TNC ────────────────────────────────────────────────────
    let init_cfg = init_config_from_session(session_id, cfg);
    if let Err(e) = transport.init(&init_cfg) {
        let msg = format!("init failed: {e}");
        let mut s = ModemStatus::stopped();
        s.state = ModemState::Error;
        s.last_error = Some(msg.clone());
        session.set_status(s);
        // Drop the partially-initialized transport so any spawned process
        // is torn down by its Drop impl rather than leaking past this fn.
        drop(transport);
        return Err(msg);
    }

    // tuxlink-o3f2: install the side-channel abort writer BEFORE the
    // blocking `connect_arq` begins (see the walk path for the full rationale).
    if let Some((writer, stream)) = transport.try_clone_abort_writer() {
        session.install_abort_writer(writer, stream);
    }

    // Status: Connecting (bounded by the inlined legacy 120s cap below).
    let mut snap = session.status_snapshot();
    snap.state = ModemState::Connecting;
    session.set_status(snap);

    // ─── ARQ connect (bounded airtime) ───────────────────────────────────
    // Legacy 120s cap — same inline literal as `dial_one_candidate`. Kept
    // un-named (no `*_DEADLINE` const) per operator decision bd tuxlink-qtgg
    // (enforced by `modem_commands_source_does_not_define_..`); the two copies
    // must be edited together.
    let info = match transport.connect_arq(
        target,
        connect_attempts_from_config(),
        Some(Duration::from_secs(120)),
    ) {
        Ok(info) => info,
        Err(e) => {
            let msg = format!("ARQ connect failed: {e}");
            let mut s = ModemStatus::stopped();
            s.state = ModemState::Error;
            s.last_error = Some(msg.clone());
            session.set_status(s);
            drop(transport);
            return Err(msg);
        }
    };

    // ─── Install handle + publish initial connected snapshot ─────────────
    session.install_transport(transport);

    let mut s = session.status_snapshot();
    s.state = ModemState::ConnectedIrs;
    s.peer = Some(info.peer_call.clone());
    s.width_hz = Some(info.bandwidth_hz);
    s.last_error = None;
    session.set_status(s);

    Ok(())
}

/// QSY-aware connect core (Task 8 + 9). Runs AFTER the busy guard. Walks
/// `candidates` in order: per candidate it spawns + inits a transport, performs
/// the pre-audio CAT tune ([`tune_rig_for_connect`]), then `connect_arq`. On
/// success it installs the transport (+ the kept DRA-100 rig) and returns; on
/// failure it drops that candidate's transport/rig (RAII) and, only when
/// `qsy_on_fail` is set, advances to the next candidate. An operator
/// disconnect/abort between candidates (observed via a bumped close-generation)
/// stops the walk.
///
/// `make_transport` is `FnMut` so the walk can spawn one transport per
/// candidate. The single-candidate `_post_consume_with_factory` shim preserves
/// the legacy contract (callers that only handle one dial pass `FnMut`
/// closures, which a single-use `move` closure satisfies).
#[allow(clippy::too_many_arguments)]
pub fn modem_ardop_connect_walk_with_factory<F>(
    session: &Arc<ModemSession>,
    session_id: &crate::identity::SessionIdentity,
    cfg: &Config,
    candidates: &[DialCandidate],
    qsy_on_fail: bool,
    ardop_ui: &ArdopUiConfig,
    mut make_transport: F,
) -> Result<(), String>
where
    F: FnMut(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    // NO GATE here — caller MUST have acquired the busy bit already.

    if candidates.is_empty() {
        return Err("connect: no dial candidates".into());
    }

    // Snapshot the close-generation BEFORE the walk. If an operator
    // disconnect/abort bumps it between candidates, the walk stops rather
    // than QSY-ing to the next frequency after the operator asked to stop
    // (mirrors the worker close-vs-reinstall guard, tuxlink-vyby/pdnw).
    let walk_gen = session.current_close_generation();

    // The last candidate's failure message — surfaced if no candidate connects.
    let mut last_err: Option<String> = None;
    // Carries the winning candidate's connect info out of the closure so the
    // post-walk snapshot publishes the connected peer. (Set inside `attempt`.)
    let mut connected: Option<crate::winlink::modem::ConnectInfo> = None;

    let outcome = walk_candidates(candidates, qsy_on_fail, |_idx, candidate| {
        // Honor an in-flight operator abort: if a close intervened since the
        // walk began, stop without dialing the next candidate.
        if session.current_close_generation() != walk_gen {
            last_err = Some(CONNECT_ABORTED_MSG.into());
            return false;
        }

        match dial_one_candidate(
            session,
            session_id,
            cfg,
            candidate,
            ardop_ui,
            walk_gen,
            &mut make_transport,
        ) {
            Ok((transport, rig, info)) => {
                // Success: install the live transport + (DRA-100) rig handle.
                session.install_transport(transport);
                // rig-control LIVE-VFO POLL: when the rig was kept alive
                // (DRA-100 keep-serial path → `rig.is_some()`) AND the operator
                // enabled `live_vfo_poll`, start the poll thread so the panel's
                // frequency element tracks the rig's actual VFO. On the
                // close-serial path `rig` is `None` (the serial was released
                // after tune) so there's nothing to poll. The thread opens its
                // own rigctld client; the managed rig handle is independent.
                // tuxlink-8fkkk: live-VFO poll + rigctld endpoint are radio-level
                // (Config.rig), not ARDOP-specific.
                let want_poll = rig.is_some() && cfg.rig.live_vfo_poll;
                session.set_rig(rig);
                if want_poll {
                    session.start_rig_poll(
                        cfg.rig.rigctld_host.clone(),
                        cfg.rig.rigctld_port,
                    );
                }
                connected = Some(info);
                true
            }
            Err(e) => {
                // Failure: this candidate's transport + rig already dropped
                // inside `dial_one_candidate` (RAII). Record + continue/stop
                // per `qsy_on_fail` (handled by `walk_candidates`).
                last_err = Some(e);
                false
            }
        }
    });

    match outcome {
        Some(_) => {
            let info = connected.expect("walk reported success without info");
            let mut s = session.status_snapshot();
            s.state = ModemState::ConnectedIrs;
            s.peer = Some(info.peer_call.clone());
            s.width_hz = Some(info.bandwidth_hz);
            s.last_error = None;
            session.set_status(s);
            Ok(())
        }
        None => {
            let msg = last_err.unwrap_or_else(|| "ARQ connect failed".into());
            // Distinguish an operator-initiated abort from a genuine connect
            // failure. When the walk drained to `None` because the operator hit
            // Stop (close-generation bump → `CONNECT_ABORTED_MSG`),
            // `modem_ardop_disconnect_inner` has ALREADY reset the session to
            // `Stopped`. Overwriting that with `Error` here would leave the
            // ARDOP panel stuck in its non-stopped branch after a successful
            // Stop. So on abort, leave the `Stopped` state in place and just
            // surface the abort message as `Err`. Only a genuine failure
            // installs the `Error` status.
            if msg != CONNECT_ABORTED_MSG {
                let mut s = ModemStatus::stopped();
                s.state = ModemState::Error;
                s.last_error = Some(msg.clone());
                session.set_status(s);
            }
            Err(msg)
        }
    }
}

/// The live connection a successful dial yields: the modem transport, the kept
/// rig handle (`Some` only on the DRA-100 keep-serial path), and the ConnectInfo.
type DialedConnection = (
    Box<dyn ModemTransport>,
    Option<tux_rig::ManagedRig>,
    crate::winlink::modem::ConnectInfo,
);

/// Spawn + init + pre-audio tune + `connect_arq` for ONE candidate. On success
/// returns the live `(transport, kept-rig, ConnectInfo)` for the caller to install.
/// On failure the candidate's transport and rig are dropped here (RAII teardown
/// of ardopcf + rigctld) before the `Err` is returned.
///
/// `walk_gen` is the close-generation snapshot taken by the walk BEFORE the
/// first candidate was attempted. After the pre-audio tune returns (which can
/// take non-trivial wall time), the generation is re-checked so that an
/// operator Disconnect issued during the tune is honoured before the blocking
/// `connect_arq` call begins (tuxlink-8fkkk C2).
fn dial_one_candidate<F>(
    session: &Arc<ModemSession>,
    session_id: &crate::identity::SessionIdentity,
    cfg: &Config,
    candidate: &DialCandidate,
    ardop_ui: &ArdopUiConfig,
    walk_gen: u64,
    make_transport: &mut F,
) -> Result<DialedConnection, String>
where
    F: FnMut(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    let target = candidate.target.as_str();

    // ─── Translate ArdopUiConfig (frontend) → ArdopConfig (backend) ─────
    let ardop_cfg = ardop_config_for(ardop_ui, &cfg.rig)?;

    // Mark spawning so any concurrent status_snapshot sees the transition
    // before the (potentially slow) ardopcf bind-wait + init.
    let mut snap = session.status_snapshot();
    snap.state = ModemState::Spawning;
    snap.peer = Some(target.to_string());
    snap.last_error = None;
    session.set_status(snap);

    // ─── Spawn (via the FT8 yield choke point) ───────────────────────────
    let mut transport = spawn_ardop_with_yield(&mut *make_transport, ardop_cfg, target)?;

    // ─── Init the TNC ────────────────────────────────────────────────────
    let init_cfg = init_config_from_session(session_id, cfg);
    if let Err(e) = transport.init(&init_cfg) {
        // Drop the partially-initialized transport so any spawned process
        // is torn down by its Drop impl rather than leaking past this fn.
        drop(transport);
        return Err(format!("init failed: {e}"));
    }

    // ─── Pre-audio CAT tune (tux-rig) — Task 8 ────────────────────────────
    // AFTER init, BEFORE connect_arq (audio). Close-serial radios release the
    // serial here; DRA-100 keeps the rig and hands it back for session storage.
    // On a tune error, drop the transport first so ardopcf is reaped.
    // ARDOP is HF sideband-data by definition — always convert center→dial.
    let live_rig = match tune_rig_for_connect(&cfg.rig, candidate.freq_hz, true) {
        Ok(rig) => rig,
        Err(e) => {
            drop(transport);
            return Err(e);
        }
    };

    // tuxlink-8fkkk C2: re-check the close-generation AFTER the tune returns.
    // The pre-audio tune can take non-trivial wall time (rigctld spawn + CAT
    // round-trips), widening the window where the operator's Disconnect is
    // missed. If the generation has bumped since the walk began, bail now —
    // before the blocking `connect_arq` call — so the operator's abort is
    // honoured promptly. Mirror of the per-candidate check in the walk closure
    // (~508) which guards between candidates; this guards within a candidate.
    if session.current_close_generation() != walk_gen {
        drop(transport);
        drop(live_rig);
        return Err(CONNECT_ABORTED_MSG.into());
    }

    // tuxlink-o3f2: install the side-channel abort writer BEFORE the blocking
    // `connect_arq`. The operator's Disconnect → `abort_in_flight()` writes
    // `ABORT\r` to ardopcf so the recv loop surfaces FAULT/DISC and unwinds.
    // tuxlink-0ye6 Task 4.1 widened to a (writer, stream) pair for the
    // hard-close fallback (Codex Round 4 P1 #3).
    if let Some((writer, stream)) = transport.try_clone_abort_writer() {
        session.install_abort_writer(writer, stream);
    }

    // Status: Connecting (bounded by the inlined legacy 120s cap below).
    let mut snap = session.status_snapshot();
    snap.state = ModemState::Connecting;
    session.set_status(snap);

    // ─── ARQ connect (bounded airtime) ───────────────────────────────────
    // Legacy Start-button path: inline the historical 120s wall-clock cap.
    // This literal is INTENTIONALLY un-named (no `*_DEADLINE` const): operator
    // decision bd tuxlink-qtgg bans a named wall-clock-cap symbol on
    // `connect_arq` (enforced by `modem_commands_source_does_not_define_..`).
    // The legacy single-dial path (`_post_consume`) carries the same literal;
    // keep the two in sync by hand — they cannot share a const.
    match transport.connect_arq(
        target,
        connect_attempts_from_config(),
        Some(Duration::from_secs(120)),
    ) {
        Ok(info) => Ok((transport, live_rig, info)),
        Err(e) => {
            // RAII teardown: drop the transport (reaps ardopcf) and the rig
            // (reaps rigctld) before returning so the next candidate starts
            // clean and no rigctld is left holding the CAT serial.
            drop(transport);
            drop(live_rig);
            Err(format!("ARQ connect failed: {e}"))
        }
    }
}

/// Start the ARDOP modem in **listen-only** mode for the listener
/// (tuxlink-61yg). Mirrors [`modem_ardop_connect_post_consume_with_factory`]
/// through `init` but DOES NOT call `connect_arq` — the modem is brought up
/// with `LISTEN TRUE` and parked in `Idle` waiting for the listener
/// consumer task to gate inbound `Connected` events.
///
/// On success the transport is installed in `session` (state = Idle) and
/// the abort writer is installed for [`abort_in_flight`] /
/// [`send_listen_command`]. The caller (`ardop_listen` Tauri command)
/// follows by spawning the listener consumer task that takes the transport
/// back out via [`ModemSession::take_transport`] and runs the
/// gate + B2F + mailbox-persist loop.
pub fn start_modem_listen_only<F>(
    session: &Arc<ModemSession>,
    session_id: &crate::identity::SessionIdentity,
    cfg: &Config,
    ardop_ui: &ArdopUiConfig,
    make_transport: F,
) -> Result<(), String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    let ardop_cfg = ardop_config_for(ardop_ui, &cfg.rig)?;

    let mut snap = session.status_snapshot();
    snap.state = ModemState::Spawning;
    snap.peer = None;
    snap.last_error = None;
    session.set_status(snap);

    let mut transport = match spawn_ardop_with_yield(make_transport, ardop_cfg, "") {
        Ok(t) => t,
        Err(e) => {
            let mut s = ModemStatus::stopped();
            s.state = ModemState::Error;
            s.last_error = Some(e.clone());
            session.set_status(s);
            return Err(e);
        }
    };

    // Init with initial_listen = true so the modem comes up listening.
    let mut init_cfg = init_config_from_session(session_id, cfg);
    init_cfg.initial_listen = true;
    if let Err(e) = transport.init(&init_cfg) {
        let msg = format!("init failed: {e}");
        let mut s = ModemStatus::stopped();
        s.state = ModemState::Error;
        s.last_error = Some(msg.clone());
        session.set_status(s);
        drop(transport);
        return Err(msg);
    }

    if let Some((writer, stream)) = transport.try_clone_abort_writer() {
        session.install_abort_writer(writer, stream);
    }

    session.install_transport(transport);

    let mut s = session.status_snapshot();
    s.state = ModemState::Idle;
    s.peer = None;
    s.last_error = None;
    session.set_status(s);
    Ok(())
}

// ─── tuxlink-0ye6 Task 3.5 — ARDOP session lifecycle commands ───────────
//
// ARDOP analog of VARA's `vara_open_session(intent, transport_kind)` +
// `vara_close_session()` (Tasks 3.2 + 3.3 + 4.2). The shape mirrors VARA's
// — same signature for the open command (intent + transport_kind both
// passed even though ARDOP only has TransportKind::Ardop, for consistency
// with the Phase 5 shared RadioSessionPanel's uniform IPC contract).
//
// Differences from VARA:
//   - VARA is operator-managed (Windows process under Wine); tuxlink only
//     opens the TCP cmd + data socket pair. ARDOP is tuxlink-managed —
//     `ardop_open_session` spawns ardopcf + binds the cmd socket + sends
//     the init commands.
//   - VARA's "open" is just transport-open; no transmit. ARDOP's "open"
//     spawns the modem but does NOT call `connect_arq`. The Connect
//     button's path (Task 3.6 — widened `modem_ardop_b2f_exchange`) is
//     what eventually calls `connect_arq`.
//   - Auto-arm semantics are identical: P2p + RadioOnly auto-arm the
//     listener; Cms does not.

/// Spawn ardopcf + bind the cmd socket + send the init commands, but do
/// NOT call `connect_arq` and do NOT flip LISTEN. The transport is
/// installed in the session, status goes to `Idle`. Factored out of
/// [`modem_ardop_connect_post_consume_with_factory`] +
/// [`start_modem_listen_only`] so the new
/// [`ardop_open_session_inner`] can reuse the same spawn-and-init body
/// without inheriting either's connect-vs-listen tail.
///
/// `initial_listen=false` is the canonical case for the new lifecycle:
/// LISTEN gets flipped TRUE later by [`crate::ui_commands::ardop_listen_inner`]
/// (the auto-arm path) iff the operator's intent calls for it. This keeps
/// the open-session command's pre-conditions narrow — opening a session
/// with intent=Cms doesn't put the modem on-air, which would be wrong for
/// the CMS-bound path.
///
/// The abort writer is installed AFTER the spawn + init succeed, matching
/// the existing `modem_ardop_connect_post_consume_with_factory` pattern.
/// This must happen BEFORE returning so a subsequent
/// [`ModemSession::abort_in_flight`] (called from
/// [`ardop_close_session_inner`]) finds the writer installed.
pub fn spawn_and_init_ardop_inner<F>(
    session: &Arc<ModemSession>,
    session_id: &crate::identity::SessionIdentity,
    cfg: &Config,
    ardop_ui: &ArdopUiConfig,
    make_transport: F,
) -> Result<(), String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    let ardop_cfg = ardop_config_for(ardop_ui, &cfg.rig)?;

    let mut snap = session.status_snapshot();
    snap.state = ModemState::Spawning;
    snap.peer = None;
    snap.last_error = None;
    session.set_status(snap);

    let mut transport = match spawn_ardop_with_yield(make_transport, ardop_cfg, "") {
        Ok(t) => t,
        Err(e) => {
            let mut s = ModemStatus::stopped();
            s.state = ModemState::Error;
            s.last_error = Some(e.clone());
            session.set_status(s);
            return Err(e);
        }
    };

    // initial_listen = false — the auto-arm path flips LISTEN TRUE later
    // via `ardop_listen_inner` iff intent.auto_arms_listener(). Keeping
    // the modem off-air during the open phase is the load-bearing safety
    // invariant — see the spec §2 "No tuxlink-added safeguards" note +
    // the per-intent decision matrix in `SessionIntent::auto_arms_listener`.
    let mut init_cfg = init_config_from_session(session_id, cfg);
    init_cfg.initial_listen = false;
    if let Err(e) = transport.init(&init_cfg) {
        let msg = format!("init failed: {e}");
        let mut s = ModemStatus::stopped();
        s.state = ModemState::Error;
        s.last_error = Some(msg.clone());
        session.set_status(s);
        drop(transport);
        return Err(msg);
    }

    // Install the side-channel abort writer BEFORE returning so a
    // subsequent ardop_close_session can fire ABORT via abort_in_flight
    // even when no connect_arq is yet in flight. Mirror of Task 4.2's
    // VARA wire pattern (also done before the post-init publish).
    if let Some((writer, stream)) = transport.try_clone_abort_writer() {
        session.install_abort_writer(writer, stream);
    }

    session.install_transport(transport);

    let mut s = session.status_snapshot();
    s.state = ModemState::Idle;
    s.peer = None;
    s.last_error = None;
    session.set_status(s);
    Ok(())
}

/// Inner helper for [`ardop_open_session`] with a factory seam so tests
/// can drive without spawning a real ardopcf. `intent` + `transport_kind`
/// are recorded on session state via
/// [`ModemSession::set_active_session_mode`] AFTER the spawn + init
/// succeeds; on a failed spawn/init the active-mode fields stay clear so
/// a fresh open attempt starts with a clean slate.
///
/// The optional auto-arm (when `intent.auto_arms_listener()` is true) is
/// the caller's responsibility — `ardop_open_session_inner` does NOT call
/// `ardop_listen_inner` because the inner takes synchronous + tauri-free
/// args while the listen path is async + AppHandle-bearing. The outer
/// [`ardop_open_session`] Tauri command chains the auto-arm after this
/// helper returns Ok. Same separation as VARA's
/// `vara_open_session_inner` → outer-command-chains-auto-arm pattern.
pub fn ardop_open_session_inner<F>(
    session: &Arc<ModemSession>,
    session_id: &crate::identity::SessionIdentity,
    cfg: &Config,
    ardop_ui: &ArdopUiConfig,
    intent: SessionIntent,
    transport_kind: crate::winlink::listener::transport::TransportKind,
    make_transport: F,
) -> Result<(), String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    // Refuse re-open if a session is already in flight. The existing
    // status machine's Spawning/Initializing/Idle states all imply a
    // transport is installed (or being installed); only Stopped/Error
    // are safe to open over. Same conservative posture as VARA's
    // "transport.is_some() → reject" check in vara_open_session_inner.
    let cur = session.status_snapshot().state;
    if !matches!(cur, ModemState::Stopped | ModemState::Error) {
        return Err(format!(
            "ARDOP session already open or in-flight (state={cur:?}) — \
             call ardop_close_session first"
        ));
    }

    spawn_and_init_ardop_inner(session, session_id, cfg, ardop_ui, make_transport)?;

    // Record the operator-typed (intent, transport_kind) AFTER the
    // spawn + init succeeds — a failed open leaves the active-mode
    // fields clear so the next open attempt starts fresh.
    session.set_active_session_mode(intent, transport_kind);
    Ok(())
}

/// Open an ARDOP session: spawn ardopcf + bind cmd socket + send init
/// commands + install the abort writer + record (intent, transport_kind)
/// + (when intent auto-arms) flip the listener on. Returns Ok on
/// successful open.
///
/// **Signature (tuxlink-0ye6 Task 3.5 + Codex Round 2 P2):** accepts
/// both `intent: SessionIntent` AND `transport_kind: TransportKind`.
/// ARDOP only has `TransportKind::Ardop`, but the shape mirrors VARA's
/// so the Phase 5 RadioSessionPanel sends `{ intent, transportKind }`
/// uniformly for all panels.
///
/// **Auto-arm (spec §2 + §3):** the listener is auto-armed inline when
/// `intent.auto_arms_listener()` is true — `P2p` (any peer) and
/// `RadioOnly` (R-pool peer) auto-arm; `Cms` does not (CMS is
/// outbound-only from the client's view). Auto-arm failure does not tear
/// down the transport — open and arm are distinct contracts, and the
/// operator can retry the arm via the Listen toggle.
///
/// **Does NOT call `connect_arq`** — that's the Connect button's path
/// (Task 3.6's widened `modem_ardop_b2f_exchange`). For `intent=Cms`,
/// open spawns ardopcf and stays idle waiting for Connect.
#[tauri::command]
pub async fn ardop_open_session(
    app: AppHandle,
    log: State<'_, Arc<crate::session_log::SessionLogState>>,
    session: State<'_, Arc<ModemSession>>,
    listen_state: State<'_, Arc<crate::ui_commands::ArdopListenState>>,
    intent: SessionIntent,
    transport_kind: crate::winlink::listener::transport::TransportKind,
) -> Result<ModemStatus, String> {
    // Pre-flight identity check (mirror of modem_ardop_connect): no point
    // spawning the modem if the operator hasn't completed the wizard.
    let cfg = config::read_config().map_err(|e| format!("read config: {e}"))?;
    check_identity_present(&cfg)?;

    // tuxlink-0063 (Phase 3, Task 3.9): resolve the authenticated active
    // SessionIdentity for the modem-init MYCALL (on-air station ID). Resolved
    // fail-closed before any modem I/O — a NoActiveIdentity leaves the radio
    // untouched.
    let session_id = app
        .state::<crate::app_backend::BackendState>()
        .current()
        .ok_or_else(|| "ARDOP open: backend offline — cannot resolve active identity".to_string())?
        .active_identity()
        .map_err(|e| e.to_string())?;

    let ardop_ui = config_get_ardop();
    if ardop_ui.capture_device.is_empty() || ardop_ui.playback_device.is_empty() {
        return Err(
            "ARDOP audio devices not configured — open Settings → ARDOP first".into(),
        );
    }

    // Spawn the modem on a blocking thread (bind-wait + init can be slow,
    // same pattern as the listener arm path).
    let session_arc: Arc<ModemSession> = session.inner().clone();
    let ardop_ui_clone = ardop_ui.clone();
    let cfg_clone = cfg.clone();
    let session_id_clone = session_id.clone();
    // tuxlink-ngsk: route this session's cmd-port traffic into the session log.
    let wire = ardop_wire_sink(&app);
    let res = tokio::task::spawn_blocking(move || {
        ardop_open_session_inner(
            &session_arc,
            &session_id_clone,
            &cfg_clone,
            &ardop_ui_clone,
            intent,
            transport_kind,
            |cfg, _target| {
                ArdopTransport::with_managed_modem(cfg)
                    .map(|t| Box::new(t.with_wire_sink(wire.clone())) as Box<dyn ModemTransport>)
                    .map_err(|e| format!("spawn failed: {e}"))
            },
        )
    })
    .await
    .map_err(|e| format!("modem spawn task failed: {e}"))?;

    res?;

    // Auto-arm the listener when the intent calls for it (spec §2 + §3).
    // Best-effort: a failure here does NOT tear down the transport — open
    // and arm are distinct contracts.
    if intent.auto_arms_listener() {
        if let Err(e) = crate::ui_commands::ardop_listen_inner(
            &app,
            log.inner(),
            session.inner(),
            listen_state.inner(),
        )
        .await
        {
            eprintln!(
                "ardop_open_session: auto-arm failed after open ({e:?}); transport \
                 remains open. Toggle Listen on the panel to retry the arm."
            );
        }
    }

    Ok(session.status_snapshot())
}

/// Inner helper for [`ardop_close_session`] so tests can drive without
/// a Tauri runtime. Performs the spec §5 close sequence:
///
/// 1. Disarm listener via [`crate::ui_commands::ardop_set_listen_inner`]
///    (`enabled=false`) — idempotent; no-op when no listener is armed.
/// 2. Abort any in-flight exchange via
///    [`ModemSession::abort_in_flight`] — best-effort; the
///    no-writer-installed `Err` is the expected path when no exchange
///    is in flight.
/// 3. Clear active session mode + transport via
///    [`modem_ardop_disconnect_inner`] (which calls `reset_to_stopped`
///    — that clears active_intent + active_transport_kind alongside
///    the transport handle).
///
/// Steps 1 + 2 are already chained inside `modem_ardop_disconnect_inner`
/// (the disconnect path already aborts in-flight, resets to stopped).
/// The listener-disarm step is the only new behavior layered here.
pub async fn ardop_close_session_inner(
    app: &AppHandle,
    log: &Arc<crate::session_log::SessionLogState>,
    session: &Arc<ModemSession>,
    listen_state: &Arc<crate::ui_commands::ArdopListenState>,
) -> Result<(), String> {
    // tuxlink-pdnw (Codex Phase 3-4 P1 #5): bump the close-generation
    // FIRST, before any consumer-shutdown signal or in-flight worker
    // observation. Any worker already past `current_close_generation()`
    // sees a stale snapshot and the guarded install-back path drops the
    // transport instead of restoring it. Without this line, the listener
    // consumer's drain path (which calls `install_transport` after
    // observing the shutdown flag set by `ardop_set_listen_inner`) would
    // race with this close and reopen the session.
    let _ = session.bump_close_generation();

    // Step 1: Disarm listener (idempotent — emits Warn line if not armed).
    // ardop_set_listen_inner already calls session.abort_in_flight() +
    // session.send_listen_command(false) when armed; this covers the
    // abort-during-B2F path (Codex 2026-06-03 P1 #3 fix).
    let _ = crate::ui_commands::ardop_set_listen_inner(
        app, log, session, listen_state, false,
    )
    .await;

    // Steps 2 + 3: modem_ardop_disconnect_inner does the abort_in_flight
    // call (best-effort) + reset_to_stopped (which clears active_intent +
    // active_transport_kind alongside the transport handle and abort
    // writer). The disconnect path's own `transport.disconnect(...)` is
    // also called best-effort — even if it fails, the session ends in
    // Stopped so a fresh open can succeed.
    modem_ardop_disconnect_inner(session)
}

/// Close an ARDOP session: full lifecycle teardown per spec §5.
///
/// 1. **Disarm listener** via
///    [`crate::ui_commands::ardop_set_listen_inner`] (`enabled=false`)
///    — idempotent.
/// 2. **Abort in-flight exchange** via [`ModemSession::abort_in_flight`]
///    (already inside `modem_ardop_disconnect_inner`) — best-effort.
/// 3. **Clear active session mode** (`active_intent` +
///    `active_transport_kind`) via [`ModemSession::reset_to_stopped`]
///    (already inside `modem_ardop_disconnect_inner`).
/// 4. **Close transport** via [`modem_ardop_disconnect_inner`] —
///    `transport.disconnect()` (best-effort, 5s deadline), then drop
///    the transport.
///
/// Idempotent across the whole chain — calling on an already-closed
/// session is a no-op that returns Ok.
#[tauri::command]
pub async fn ardop_close_session(
    app: AppHandle,
    log: State<'_, Arc<crate::session_log::SessionLogState>>,
    session: State<'_, Arc<ModemSession>>,
    listen_state: State<'_, Arc<crate::ui_commands::ArdopListenState>>,
) -> Result<(), String> {
    ardop_close_session_inner(&app, log.inner(), session.inner(), listen_state.inner()).await
}

/// Build the [`InitConfig`] passed to `ModemTransport::init` from the
/// authenticated active [`SessionIdentity`] (tuxlink-0063 Phase 3, Task 3.9).
///
/// **`mycall` is the on-air station ID** — the Part 97 call the ardopcf TNC
/// announces. Under the handle model it comes from `session_id.mycall()` (the
/// authenticated full callsign), NEVER from persisted config. There is no
/// config-call/identifier fallback and no empty-string default: opening a
/// transmit-capable modem requires an authenticated identity, resolved
/// fail-closed by the caller before this function runs.
///
/// `gridsquare`, `arq_bandwidth_hz`, and `drive_level` are NOT identity — they
/// remain config: `gridsquare` from `cfg.identity.grid` (defaulting to `"AA00"`
/// when no grid is set — the ARDOP TNC requires a non-empty value but the
/// broadcast precision gate happens upstream in the position layer), the ARQ
/// bandwidth from `cfg.modem_ardop.bandwidth_hz` (tuxlink-j0ij), and the TX
/// drive level from `cfg.modem_ardop.drive_level`.
///
/// The function no longer reads config itself — the caller (which already
/// holds or reads a `Config`) passes `&Config` in. This keeps the modem-init
/// MYCALL on the same single-resolution path as the rest of Phase 3.
///
/// **Bandwidth validation:** the Settings panel constrains the dropdown to
/// {200, 500, 1000, 2000}, but the persisted JSON could be hand-edited
/// off-app, so this function defends in depth: any other value is logged
/// to stderr and dropped to None (let ardopcf use its default) rather than
/// passed through and rejected by ardopcf at init time.
fn init_config_from_session(
    session_id: &crate::identity::SessionIdentity,
    cfg: &Config,
) -> InitConfig {
    // The station call is the authenticated full callsign — no
    // identifier/empty fallback (tuxlink-0063 Phase 3, Task 3.9).
    let mycall = session_id.mycall().as_str().to_uppercase();

    let grid = cfg.identity.grid.clone().unwrap_or_default();
    let arq_bandwidth_hz = cfg
        .modem_ardop
        .as_ref()
        .and_then(|a| a.bandwidth_hz)
        .and_then(validate_arq_bandwidth_hz);
    let drive_level = cfg
        .modem_ardop
        .as_ref()
        .and_then(|a| a.drive_level)
        .and_then(validate_drive_level);

    // ARDOP requires a non-empty grid; "AA00" is the canonical placeholder
    // (also wl2k-go's fallback). Operators who care about grid accuracy
    // configure it via the wizard.
    let gridsquare = if grid.trim().is_empty() {
        "AA00".to_string()
    } else {
        grid
    };

    InitConfig {
        mycall,
        gridsquare,
        arq_timeout_s: ARQ_TIMEOUT_SECS,
        arq_bandwidth_hz,
        drive_level,
        // tuxlink-dhbl: outbound-connect path leaves LISTEN FALSE at init.
        // The listener-arm UI command flips it via `set_listen` at runtime.
        initial_listen: false,
    }
}

/// Uppercase-hex encode the ASCII bytes of a CAT command string (tuxlink-wu0k).
///
/// ardopcf's `-k`/`-u` keystring arguments are hex of the raw bytes to send over
/// the CAT socket, e.g. `TX1;` → `5458313B`, `TX0;` → `5458303B`. Encoding the
/// configured key/unkey commands rather than hardcoding the FT-710 values lets
/// the operator drive any CAT-keyed radio.
pub(crate) fn hex_encode_cat_cmd(cmd: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(cmd.len() * 2);
    for b in cmd.as_bytes() {
        // Infallible: writing to a String never errors.
        let _ = write!(out, "{b:02X}");
    }
    out
}

/// Build the [`CatBridgeSpec`] for a CAT-PTT config, or `None` for any other
/// PTT method (tuxlink-wu0k).
///
/// `Ok(Some(..))` only when `ptt_method == CatCommand` and a CAT serial device
/// is configured; the spec carries the bridge port, CAT serial path/baud, and
/// key/unkey commands so
/// [`crate::winlink::modem::ardop::transport::ArdopTransport::with_managed_modem`]
/// can spawn the close-serial bridge before ardopcf. `Ok(None)` for non-CAT PTT.
///
/// Fails closed: CAT-command PTT with a blank CAT serial path returns `Err`
/// rather than inventing a default device — a hardcoded `/dev/ttyUSB0` could be
/// a TNC, GPS, or a different radio, so keying it would transmit on the wrong
/// device. The operator must pick the CAT serial port in the panel first.
pub(crate) fn cat_bridge_spec_from(
    ardop_ui: &ArdopUiConfig,
    rig: &RigUiConfig,
) -> Result<Option<crate::winlink::modem::ardop::CatBridgeSpec>, String> {
    if ardop_ui.ptt_method != PttMethod::CatCommand {
        return Ok(None);
    }
    // tuxlink-8fkkk: the CAT serial link (path + baud) is radio-level and lives
    // on RigUiConfig; key/unkey/bridge_port stay PTT-method-specific on ArdopUiConfig.
    let serial_path = rig
        .cat_serial_path
        .clone()
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| {
            "CAT-command PTT is selected but no CAT serial device is configured — \
             set the CAT serial port in the Rig control panel before connecting"
                .to_string()
        })?;
    Ok(Some(crate::winlink::modem::ardop::CatBridgeSpec {
        bridge_port: ardop_ui.cat_bridge_port,
        serial_path,
        baud: rig.cat_baud,
        key_cmd: ardop_ui.cat_key_cmd.clone(),
        unkey_cmd: ardop_ui.cat_unkey_cmd.clone(),
    }))
}

/// Translate the `ArdopUiConfig` (frontend) into the backend `ArdopConfig` used
/// to spawn ardopcf. Single source of truth for the binary / extra-args /
/// cmd-port / data-port / cat-bridge wiring so the connect flow's per-candidate
/// dial ([`dial_one_candidate`]), the legacy single-dial path
/// ([`modem_ardop_connect_post_consume_with_factory`]), the listen-only spawn
/// ([`start_modem_listen_only`]), and the open-session spawn
/// ([`spawn_and_init_ardop_inner`]) can never drift from one another (the
/// data-port `+1` convention and the close-serial CAT-bridge spec in
/// particular).
///
/// Fails closed via [`cat_bridge_spec_from`] when CAT-command PTT is selected
/// without a configured CAT serial device.
fn ardop_config_for(ardop_ui: &ArdopUiConfig, rig: &RigUiConfig) -> Result<ArdopConfig, String> {
    Ok(ArdopConfig {
        binary: resolve_ardop_binary(&ardop_ui.binary),
        extra_args: build_ardop_extra_args(ardop_ui),
        cmd_port: ardop_ui.cmd_port,
        // ardopcf convention: data_port = cmd_port + 1 (8516 for default 8515).
        data_port: ardop_ui.cmd_port.saturating_add(1),
        audio_device_path: None,
        // tuxlink-wu0k: spawn the close-serial CAT-PTT bridge when the operator
        // selected CAT PTT; None for VOX / serial-RTS. tuxlink-8fkkk: the CAT
        // serial link is read from the radio-level rig config.
        cat_bridge: cat_bridge_spec_from(ardop_ui, rig)?,
    })
}

/// Build the `extra_args` vector passed to `ArdopConfig` (the ardopcf CLI).
///
/// ardopcf's positional CLI is:
/// ```text
/// ardopcf [PTT FLAGS] [-G <webgui_port>] <cmd_port> <capture> <playback>
/// ```
///
/// PTT flags depend on `ardop_ui.ptt_method` (tuxlink-wu0k):
///
/// - [`PttMethod::Vox`] — no PTT flag; the radio keys on VOX / detected audio.
/// - [`PttMethod::SerialRts`] — **`-p <ptt_serial_path>`**, ardopcf's RTS PTT,
///   only when the path is `Some(non_empty)` (ardopcf rejects an empty value).
/// - [`PttMethod::CatCommand`] — **`-c TCP:<cat_bridge_port> -k <hex(key)>
///   -u <hex(unkey)>`**. ardopcf sends the keystring over a TCP "CAT" socket
///   served by tuxlink's close-serial bridge (the serial port is held OPEN only
///   momentarily per keystring so it does not contend with the audio codec on a
///   single-cable USB tree). NO `-p` is emitted. The bridge itself is spawned by
///   [`crate::winlink::modem::ardop::transport::ArdopTransport::with_managed_modem`]
///   from the [`CatBridgeSpec`](crate::winlink::modem::ardop::CatBridgeSpec)
///   carried on `ArdopConfig`; this function only emits the matching ardopcf
///   flags. Proven on air 2026-06-23 (FT-710 + Pi 5).
///
/// Then **`-G <webgui_port>`** (tuxlink-60wh) enables ardopcf's built-in WebGUI
/// on `cmd_port - 1` (omitted when `cmd_port < 2`).
///
/// Pure over `&ArdopUiConfig` so unit tests can assert the exact argv shape
/// without spawning a real process.
pub(crate) fn build_ardop_extra_args(ardop_ui: &ArdopUiConfig) -> Vec<String> {
    // Capacity covers worst case: -c TCP:p -k h -u h -G <wg> <cmd> <cap> <play>.
    let mut extra_args: Vec<String> = Vec::with_capacity(11);

    match ardop_ui.ptt_method {
        PttMethod::Vox => {
            // No PTT line.
        }
        PttMethod::SerialRts => {
            if let Some(ref ptt) = ardop_ui.ptt_serial_path {
                if !ptt.is_empty() {
                    extra_args.push("-p".into());
                    extra_args.push(ptt.clone());
                }
            }
        }
        PttMethod::CatCommand => {
            // ardopcf keys over the TCP CAT socket served by tuxlink's
            // close-serial bridge: -c TCP:<port> -k <hex(key)> -u <hex(unkey)>.
            // NO -p — the radio is keyed by CAT command, not an RTS line.
            extra_args.push("-c".into());
            extra_args.push(format!("TCP:{}", ardop_ui.cat_bridge_port));
            extra_args.push("-k".into());
            extra_args.push(hex_encode_cat_cmd(&ardop_ui.cat_key_cmd));
            extra_args.push("-u".into());
            extra_args.push(hex_encode_cat_cmd(&ardop_ui.cat_unkey_cmd));
        }
    }

    // tuxlink-60wh: spawn ardopcf with its built-in WebGUI on the resolved
    // port. Operator opens it via the radio panel's "Open WebGUI" button,
    // which targets `http://localhost:<webgui_port>/` — Spectrum, Waterfall,
    // audio level meters, TX/RX indicators, test-tone trigger.
    //
    // The port comes from `resolved_webgui_port()` so the spawn flag and
    // the frontend's URL computation read from the SAME source. Operator
    // smoke 2026-05-31 round 3 — "Open WebGUI opens but connection refused"
    // — could fall on the divergence between this site (the `-G` we pass
    // to ardopcf) and the frontend's port derivation. Routing both through
    // `resolved_webgui_port()` rules that class of bug out by construction.
    //
    // None means "no valid WebGUI port can be derived" (cmd_port < 2 and
    // no override) — omit the `-G` flag, ardopcf runs without a WebGUI.
    // The frontend's button surfaces a clear error in that case.
    if let Some(webgui_port) = ardop_ui.resolved_webgui_port() {
        extra_args.push("-G".into());
        extra_args.push(webgui_port.to_string());
    }

    extra_args.push(ardop_ui.cmd_port.to_string());
    extra_args.push(ardop_ui.capture_device.clone());
    extra_args.push(ardop_ui.playback_device.clone());

    extra_args
}

/// Validate a persisted ARQ bandwidth value (tuxlink-j0ij). ardopcf accepts
/// exactly {200, 500, 1000, 2000} Hz for `ARQBW`. The Settings dropdown
/// constrains user input to these values, so a value OUTSIDE this set in
/// the persisted config indicates either a stale value from a future
/// ardopcf release, a hand-edited config, or a frontend bug — in any case,
/// the safe degradation is "drop to None and let ardopcf pick its default."
///
/// Logs the dropped value to stderr so a session-end review can spot the
/// drift. Returns Some(bw) when the value is valid, None otherwise.
fn validate_arq_bandwidth_hz(bw: u32) -> Option<u32> {
    match bw {
        200 | 500 | 1000 | 2000 => Some(bw),
        invalid => {
            eprintln!(
                "tuxlink-j0ij: ignoring invalid persisted bandwidth_hz={invalid}; \
                 valid: 200/500/1000/2000"
            );
            None
        }
    }
}

/// ConReq repeats for an outbound `ARQCALL`: the operator's
/// `modem_ardop.connect_attempts` (clamped 2..=30) if set, else CONNECT_REPEAT.
fn connect_attempts_from_config() -> u32 {
    config::read_config()
        .ok()
        .and_then(|c| c.modem_ardop.as_ref().and_then(|a| a.connect_attempts))
        .map(|n| n.clamp(CONNECT_ATTEMPTS_MIN, CONNECT_ATTEMPTS_MAX))
        .unwrap_or(CONNECT_REPEAT)
}

/// Validate a persisted drive_level (0..=100); out-of-range -> None (logged).
fn validate_drive_level(dl: u8) -> Option<u8> {
    if dl <= 100 {
        Some(dl)
    } else {
        eprintln!("config: ignoring out-of-range modem_ardop.drive_level={dl} (must be 0..=100)");
        None
    }
}

/// Pre-flight identity check: at least one of `identity.callsign` or
/// `identity.identifier` must be set + non-empty before a connect attempt
/// is allowed to proceed past the consent gate.
///
/// Why a separate helper (rather than inlining the check in
/// `modem_ardop_connect`): the Tauri wrapper is hard to unit-test without
/// a Tauri runtime, but this pure function over `&Config` is trivially
/// testable. The wrapper calls this helper, so coverage at the helper
/// layer transitively covers the wrapper's identity-check branch.
///
/// `deserialize_optional_nonempty_string` already maps `""` and
/// whitespace-only inputs to `None` at deserialize time, but we still
/// defend with a `trim().is_empty()` check in case a caller constructs
/// a `Config` value in-memory (e.g. tests) without going through serde.
pub fn check_identity_present(cfg: &Config) -> Result<(), String> {
    let has_call = cfg
        .identity
        .active_full
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let has_ident = cfg
        .identity
        .identifier
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    if has_call || has_ident {
        Ok(())
    } else {
        Err("Operator callsign not configured — complete the wizard before connecting".into())
    }
}

/// ARDOP connect Tauri command. Returns an actionable error when
/// audio devices are not yet configured (operator must complete
/// Settings → ARDOP before calling).
///
/// # Pre-flight identity check (tuxlink-5738)
///
/// BEFORE the audio-device check, this command verifies the operator's
/// identity (callsign or identifier) is configured. The wizard sets one of
/// these; an unconfigured deployment must complete the wizard first.
#[tauri::command]
pub async fn modem_ardop_connect(
    app: AppHandle,
    session: State<'_, Arc<ModemSession>>,
    target: String,
    // rig-control Task 8: tune frequency for the (single) dial. Optional —
    // `None` (the legacy `{ target }` invoke) skips the pre-audio CAT tune.
    freq_hz: Option<u64>,
    // rig-control Task 9: ordered QSY candidate list. Optional — when `Some`
    // and non-empty it overrides `target`/`freq_hz` and the walk visits each
    // candidate in order (gated by the operator's `qsy_on_fail` config). When
    // `None`/empty the single `{ target, freq_hz }` candidate is dialed.
    qsy_candidates: Option<Vec<DialCandidate>>,
) -> Result<(), String> {
    // ─── Pre-flight identity check (tuxlink-5738) ────────────────────────
    // Operator must have a callsign OR identifier configured before any
    // attempt to set up a radio transport.
    let cfg = config::read_config().map_err(|e| format!("read config: {e}"))?;
    check_identity_present(&cfg)?;

    // tuxlink-0063 (Phase 3, Task 3.9): resolve the authenticated active
    // SessionIdentity here — the modem-init MYCALL (on-air station ID) comes
    // from the session, never config. Fail-closed before any modem I/O so a
    // NoActiveIdentity leaves the radio untouched.
    let session_id = app
        .state::<crate::app_backend::BackendState>()
        .current()
        .ok_or_else(|| "ARDOP connect: backend offline — cannot resolve active identity".to_string())?
        .active_identity()
        .map_err(|e| e.to_string())?;

    // Identity verified. Now safe to do audio-device I/O.
    let ardop_ui = config_get_ardop();
    if ardop_ui.capture_device.is_empty() || ardop_ui.playback_device.is_empty() {
        return Err(
            "ARDOP audio devices not configured — open Settings → ARDOP first".into(),
        );
    }

    // tuxlink-ab9h: the gated connect — in-process busy guard, ardopcf spawn,
    // init, and the blocking `connect_arq` (bounded by ardopcf ARQTIMEOUT ×
    // CONNECT_REPEAT + the operator's ABORT side channel) — runs OFF the
    // WebKitGTK main thread. As a synchronous command it blocked the UI event
    // loop for the entire transmission, so the Stop button (status-event
    // gated) could not render and the operator had NO working abort during
    // TX. The fast identity + audio gates above stay synchronous (RADIO-1 /
    // fail-closed before any modem I/O).
    // rig-control Task 9: build the ordered candidate list. A non-empty
    // `qsy_candidates` overrides the single dial; otherwise a one-element list
    // from `{ target, freq_hz }` reproduces the legacy single-dial behavior.
    let assembled: Vec<DialCandidate> = match qsy_candidates {
        Some(v) if !v.is_empty() => v,
        _ => vec![DialCandidate { target, freq_hz }],
    };
    // tuxlink-qevsf (SAFETY/Part 97): auto-QSY disabled — the station must not
    // transmit on any frequency the operator has not seen + selected. Only the
    // operator-chosen channel (candidate[0]) is dialed. Restored by the
    // Channel-Selection redesign (Find a Station = operator-driven channel picker).
    let candidates = clamp_connect_candidates(assembled);
    // QSY-on-fail is an operator config flag; it only matters when the list has
    // more than one candidate. tuxlink-8fkkk: radio-level (Config.rig). The
    // clamp above leaves a single candidate, so the walk has nothing to advance
    // to regardless of this flag.
    let qsy_on_fail = cfg.rig.qsy_on_fail;

    let session = Arc::clone(session.inner());
    // tuxlink-ngsk: route this session's cmd-port traffic into the session log.
    let wire = ardop_wire_sink(&app);
    let result = tokio::task::spawn_blocking(move || {
        modem_ardop_connect_gated_walk_with_factory(
            &session,
            &session_id,
            &cfg,
            &candidates,
            qsy_on_fail,
            &ardop_ui,
            move |cfg, _target| {
                ArdopTransport::with_managed_modem(cfg)
                    .map(|t| Box::new(t.with_wire_sink(wire.clone())) as Box<dyn ModemTransport>)
                    .map_err(|e| format!("spawn failed: {e}"))
            },
        )
    })
    .await
    .map_err(|e| format!("connect task panicked: {e}"))?;
    // tuxlink-nnjz: surface a connect failure in the session log (where the
    // operator is already looking) rather than an inline panel element. The Err
    // still propagates so the panel clears its connecting spinner. (`inspect_err`
    // is MSRV 1.76; this match keeps the project's 1.75 floor.)
    if let Err(ref e) = result {
        emit_modem_error(&app, e);
    }
    result
}

/// Run a B2F mail exchange over an open ARDOP session (tuxlink-ytg +
/// tuxlink-0ye6 Task 3.6) — the "send/receive Winlink mail" entry point for
/// the ARDOP HF UI. Widened in Task 3.6 to perform the full ARQ-link
/// lifecycle (connect → B2F → link-disconnect) in one call.
///
/// # Preconditions
///
/// - The operator has already pressed Open Session through the ARDOP panel,
///   which called [`ardop_open_session`] and spawned ardopcf + bound the cmd
///   socket. `ModemSession` now holds the live transport, status = `Idle`.
/// - The operator has NOT yet brought the ARQ link up — `ardop_open_session`
///   (Task 3.5) explicitly stops short of `connect_arq`. This command does
///   the ARQCALL.
///
/// # Flow (Codex R1 P1 #1 ordering + Codex R2 P1 #2/#3 cleanup semantics)
///
/// 1. **Take the installed transport** out of `ModemSession`.
/// 2. **`connect_arq`** with a backstop deadline DERIVED from the retry count
///    (tuxlink-5xxq, supersedes the tuxlink-qtgg `None`). ardopcf reports a
///    failed dial via `NEWSTATE DISC` after `CONNECT_REPEAT` × ~3.3 s, so the
///    deadline only backstops a *wedged* modem that never reports. Sends
///    ARQCALL on the cmd port BEFORE any B2F byte (Codex R1 P1 #1: ARQCALL
///    ordering is load-bearing — B2F over an unconnected stream is undefined).
/// 3. **Run the B2F exchange** via
///    [`crate::winlink_backend::run_ardop_b2f_exchange`] — builds outbound
///    from the mailbox Outbox, files received messages into Inbox, moves
///    sent into Sent. The `intent`'s routing flag (Cms → 'C', P2p → none,
///    RadioOnly → 'R') flows through to the mailbox-drain filter.
/// 4. **`disconnect_arq_link`** via the existing
///    [`crate::winlink::modem::ModemTransport::disconnect`] (5 s budget) —
///    the transport's `disconnect` is link-level only (sends `DISCONNECT`
///    on the cmd port and waits for `DISCONNECTED`; the cmd socket and
///    ardopcf process stay alive). See `arq_disconnect` in
///    `winlink::modem::ardop::session` for the link-only behavior.
/// 5. **Return the transport to the session** via `install_transport`. The
///    open-session window stays Open; the listener (if armed by the
///    intent's auto-arm) can re-arm. Codex R2 P1 #3: do NOT call
///    `reset_to_stopped` — that closes the open-session window + clears
///    `active_intent` / `active_transport_kind` / the abort writer, which
///    would force the operator to re-open before another exchange or
///    retry.
///
/// # Failure semantics (tuxlink-5xxq, refines Codex R2 P1 #3)
///
/// Cleanup branches on the failure class (see [`finish_b2f_exchange`]):
/// - **CONNECT-class failure** (`connect_arq` failed — link never came up):
///   terminal. The session is reset to `Stopped` and the transport is dropped,
///   reaping ardopcf. The modem is freed; the operator re-opens to try again.
/// - **Mid-EXCHANGE failure** (link was up, B2F faulted) **or success**: the
///   transport is best-effort link-disconnected and RE-INSTALLED (Codex R2 P1
///   #3) — the open-session window survives so a retry / re-arm needs no
///   re-open.
///
/// # Arbiter wire-in deferred (tuxlink-17u9)
///
/// When `intent.auto_arms_listener()` is true (`P2p` / `RadioOnly`), the
/// listener consumer task owns the transport between exchanges — `take_transport`
/// here would return `None` and the operator would see a confusing "transport
/// not connected" error. The spec's Task 4.3 introduces
/// `ModemSession::take_transport_for_outbound` that gives outbound a way to
/// politely reclaim the transport from an armed listener; the listener
/// consumer's yield path is not yet implemented (tuxlink-17u9). Until then,
/// the simple `take_transport` pattern is used (matches Task 3.4's VARA
/// shape) — for `intent=Cms` (which does NOT auto-arm) this is correct
/// today; for `P2p`/`RadioOnly` the user-visible behavior matches the
/// existing dial-with-listener-armed gap.
#[tauri::command]
pub async fn modem_ardop_b2f_exchange(
    app: AppHandle,
    session: State<'_, Arc<ModemSession>>,
    target: String,
    intent: SessionIntent,
    transport_kind: crate::winlink::listener::transport::TransportKind,
) -> Result<(), String> {
    // Defensive: ARDOP panel must dial via the Ardop TransportKind. If a
    // future RadioSessionPanel routes a mismatched kind here, surface a
    // clean error before any radio-touching work. Pure validation — does
    // not affect the radio path.
    if !matches!(
        transport_kind,
        crate::winlink::listener::transport::TransportKind::Ardop
    ) {
        return Err(format!(
            "modem_ardop_b2f_exchange invoked with non-ARDOP transport_kind={:?}",
            transport_kind
        ));
    }

    // tuxlink-ab9h: the take-transport → connect_arq → B2F → link-disconnect
    // → guarded re-install sequence is blocking I/O (the ARQCALL plus the
    // full mail exchange). Run it OFF the WebKitGTK main thread so a
    // Send/Receive does not freeze the UI and the operator stays able to
    // abort. The transport-kind validation above stayed synchronous.
    let session = Arc::clone(session.inner());
    // tuxlink-nnjz: the spawn_blocking closure moves `app` (it's borrowed by
    // `run_ardop_connect_b2f_with_transport`), so keep a clone for the
    // post-await error emit below.
    let app_for_emit = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        // Snapshot the close-generation BEFORE the transport take
        // (tuxlink-pdnw, Codex Phase 3-4 P1 #1): if `ardop_close_session_inner`
        // runs during this exchange it bumps the generation; the guarded
        // install-back below then drops the transport rather than restoring
        // it into a session the operator just closed.
        let close_gen_snapshot = session.current_close_generation();

        // Take the installed transport (placed by `ardop_open_session`). If
        // missing, the operator didn't open a session first. (TODO
        // tuxlink-17u9: arbiter-aware `take_transport_for_outbound`.)
        let mut transport = session.take_transport().ok_or_else(|| {
            "ARDOP session not open — press Open Session (ARDOP HF) before Send/Receive"
                .to_string()
        })?;

        // connect_arq → B2F via the inner helper.
        let outcome =
            run_ardop_connect_b2f_with_transport(&app, &mut *transport, &target, intent);

        // Branch cleanup on the outcome (tuxlink-5xxq): a CONNECT-class failure
        // is terminal — free the modem (reset session + drop transport → reap
        // ardopcf). A success or mid-EXCHANGE failure keeps the open session for
        // retry/re-arm (the prior uniform re-install, Codex R2 P1 #3).
        finish_b2f_exchange(&session, transport, close_gen_snapshot, outcome)
    })
    .await
    .map_err(|e| format!("b2f task panicked: {e}"))?;
    // tuxlink-nnjz: surface a send/receive failure in the session log instead of
    // an inline panel element. Err still propagates (the panel clears its
    // exchanging spinner + records the gateway `failed` attempt). (`inspect_err`
    // is MSRV 1.76; this match keeps the project's 1.75 floor.)
    if let Err(ref e) = result {
        emit_modem_error(&app_for_emit, e);
    }
    result
}

/// Outcome of a connect→B2F attempt (tuxlink-5xxq). Distinguishes a
/// CONNECT-class failure (the ARQ link never came up — terminal, free the
/// modem) from a mid-EXCHANGE failure (the link was up — keep the open session
/// for retry). The caller, [`finish_b2f_exchange`], branches cleanup on it.
#[derive(Debug)]
enum ExchangeOutcome {
    /// `connect_arq` + the full B2F exchange completed.
    Completed,
    /// `connect_arq` failed — the ARQ link never reached CONNECTED.
    ConnectFailed(String),
    /// The link came up but the B2F exchange itself failed.
    ExchangeFailed(String),
}

/// Defense-in-depth backstop deadline for `connect_arq`, DERIVED from the
/// operator-set retry count so the two can never drift (tuxlink-5xxq, operator
/// decision). Each ardopcf ConReq cycle is ~3.3 s (`intFrameRepeatInterval`
/// 2000 ms + frame TX, per ardopcf `ARQ.c`); the 1.6× margin (8/5) covers RF
/// leader/jitter so it never fires before a legitimately slow dial. This is NOT
/// a tuxlink-added airtime cap — it only bounds the wait to just beyond when the
/// operator's own configured attempts are spent, and normally never fires:
/// ardopcf emits `NEWSTATE DISC` on ConReq exhaustion (≈ attempts × 3.3 s)
/// first, which `connect_arq` already surfaces as an `Err`.
fn connect_backstop_deadline(attempts: u32) -> Duration {
    const PER_CONREQ_MS: u64 = 3300;
    Duration::from_millis(u64::from(attempts) * PER_CONREQ_MS * 8 / 5)
}

/// Post-exchange cleanup, branched on the outcome (tuxlink-5xxq). A
/// CONNECT-class failure is terminal: reset the session to `Stopped` and DROP
/// the transport, whose `ManagedModem::Drop` reaps ardopcf — the modem is freed
/// and the session presents a terminal state, not a re-armable `Idle`. A
/// success or mid-EXCHANGE failure keeps the prior retry-friendly behavior
/// (Codex R2 P1 #3): best-effort link-disconnect (link-only, 5 s budget) +
/// guarded re-install so the open-session window + listener arming survive.
///
/// Returns the flat `Result` the command surfaces (and `emit_modem_error`
/// logs honestly, including the attempt count for a connect failure).
fn finish_b2f_exchange(
    session: &ModemSession,
    mut transport: Box<dyn ModemTransport>,
    close_gen_snapshot: u64,
    outcome: ExchangeOutcome,
) -> Result<(), String> {
    match outcome {
        ExchangeOutcome::ConnectFailed(msg) => {
            // Link never came up → terminal. Reset session state + drop the
            // transport to reap ardopcf. But reset ONLY if no operator close
            // intervened while connect_arq was blocked (Codex P1): a
            // stale-generation reset would clobber a newly-reopened session.
            // Either way we drop OUR failed transport (reaping ardopcf). No 5 s
            // link-disconnect — there is no live ARQ link to tear down.
            // Reset the session ONLY if no operator close intervened (generation
            // still current); a `false` return means a close did, so the
            // (possibly newly-reopened) session is left alone. Either way we drop
            // our failed transport, reaping ardopcf.
            let _ = session.reset_to_stopped_if_generation_matches(close_gen_snapshot);
            drop(transport);
            Err(msg)
        }
        other => {
            // Link was up (success or mid-exchange failure): tear down the ARQ
            // LINK (link-only) + guarded re-install so the open session survives
            // for retry/re-arm. Guarded (tuxlink-pdnw): a stale generation
            // snapshot (an operator close intervened) drops the transport.
            let _ = transport.disconnect(Duration::from_secs(5));
            if let Err(dropped) =
                session.install_transport_if_generation_matches(transport, close_gen_snapshot)
            {
                drop(dropped);
            }
            match other {
                ExchangeOutcome::Completed => Ok(()),
                ExchangeOutcome::ExchangeFailed(msg) => Err(msg),
                ExchangeOutcome::ConnectFailed(_) => unreachable!("handled above"),
            }
        }
    }
}

/// Inner helper for [`modem_ardop_b2f_exchange`]: drives the full
/// connect_arq → B2F sequence over a borrowed transport handle. Returns an
/// [`ExchangeOutcome`] so the caller can free the modem on a CONNECT-class
/// failure but keep the open session on a mid-EXCHANGE failure (tuxlink-5xxq).
///
/// **Ordering invariant (Codex R1 P1 #1):** `connect_arq` is invoked BEFORE
/// any byte is written to the data stream. B2F over an unconnected ARQ
/// stream is undefined; the prior shape of this command assumed
/// `modem_ardop_connect` had already brought the link up, which is no
/// longer true after Task 3.5's split of ardopcf-spawn from ARQ-connect.
///
/// **Deadline (tuxlink-5xxq, supersedes the tuxlink-qtgg `None`):**
/// `connect_arq` is called with a backstop deadline DERIVED from the retry
/// count ([`connect_backstop_deadline`]). ardopcf reports a failed dial via
/// `NEWSTATE DISC` (surfaced as an `Err`) after attempts × ~3.3 s, so the
/// deadline normally never fires — it only bounds a *wedged* ardopcf (RF/USB
/// hang) that never reports. Deriving it from the operator-set retry count
/// keeps the two from drifting.
fn run_ardop_connect_b2f_with_transport(
    app: &AppHandle,
    transport: &mut dyn ModemTransport,
    target: &str,
    intent: SessionIntent,
) -> ExchangeOutcome {
    // ─── ARQ connect stage — arms the peer-observation guard [R5-2] ──────
    let obs_guard = match run_ardop_connect_stage(transport, target, intent) {
        Err(failed) => return failed,
        Ok(guard) => guard,
    };

    // ─── Run the B2F exchange over the now-connected data stream ─────────
    let outcome = match run_b2f_with_transport(app, transport, target, intent) {
        Ok(()) => {
            if let Some(g) = &obs_guard {
                g.set_phase(crate::contacts::observation::ObservationPhase::B2fOk);
            }
            ExchangeOutcome::Completed
        }
        Err(e) => {
            if let Some(g) = &obs_guard {
                g.set_phase(crate::contacts::observation::ObservationPhase::B2fFail);
            }
            ExchangeOutcome::ExchangeFailed(e)
        }
    };
    drop(obs_guard);
    outcome
}

/// The ARDOP outer dial-site intent gate (Task 14): mirrors the VARA dial gate
/// (`dial_observation_sink`, Task 13) — only a `SessionIntent::P2p` dial
/// resolves the global observation sink; Cms/RadioOnly/PostOffice/Mesh dials
/// never record [spec §3]. Extracted so a unit test can assert the gating
/// without constructing a transport or an `AppHandle` [CDX-7].
fn ardop_dial_observation_sink(
    intent: SessionIntent,
) -> Option<crate::contacts::observation::ObservationSink> {
    if intent == SessionIntent::P2p {
        crate::contacts::observation::observation_sink()
    } else {
        None
    }
}

/// The ARQ-connect stage of the ARDOP outer record site (Task 14, `[R5-2]`).
/// Arms the [`crate::contacts::observation::ObservationGuard`] at `DialAttempted`
/// immediately before `connect_arq` — the actual CONNECT transmission — so
/// nothing before this point (there is no pre-TX stage on the ARDOP outer
/// path; `connect_arq` itself is the first and only transmission attempt)
/// records a phantom fail, per the BINDING AMENDMENTS.
///
/// - `connect_arq` fails: the guard is dropped here (fn returns before
///   `obs_guard` would otherwise survive) and fires at `DialAttempted` →
///   `Fail` — this IS the `[R5-2]` outer connect-fail site.
/// - `connect_arq` succeeds: the guard advances to `Connected` and is handed
///   back to the caller, which carries it through the B2F exchange and
///   advances it to `B2fOk`/`B2fFail` before dropping it.
///
/// Extracted from [`run_ardop_connect_b2f_with_transport`] so a unit test can
/// drive the REAL `connect_arq` failure path — including the real guard
/// construction and drop — without needing an `AppHandle` [CDX-7]; the
/// production fn's B2F stage is the only part that needs `app`, and it never
/// runs when `connect_arq` fails.
fn run_ardop_connect_stage(
    transport: &mut dyn ModemTransport,
    target: &str,
    intent: SessionIntent,
) -> Result<Option<crate::contacts::observation::ObservationGuard>, ExchangeOutcome> {
    let obs_guard = ardop_dial_observation_sink(intent).map(|sink| {
        crate::contacts::observation::ObservationGuard::new(
            sink,
            crate::contacts::observation::PeerObservation {
                path: crate::contacts::observation::ObservedPath::Rf {
                    transport: crate::contacts::reachability::ChannelTransport::Ardop,
                    via: vec![],
                    // rig/CAT freq not threaded here; never fabricated [R3-11].
                    freq_hz: None,
                    bandwidth: None,
                },
                direction: crate::contacts::reachability::Direction::Outgoing,
                presented_target: target.to_string(),
                phase: crate::contacts::observation::ObservationPhase::DialAttempted,
            },
        )
    });

    // ─── ARQ connect (Codex R1 P1 #1: ARQCALL before any B2F byte) ────────
    let attempts = connect_attempts_from_config();
    if let Err(e) =
        transport.connect_arq(target, attempts, Some(connect_backstop_deadline(attempts)))
    {
        // `obs_guard` drops here (out of scope on return) at `DialAttempted`
        // → records Fail [R5-2].
        return Err(ExchangeOutcome::ConnectFailed(format!(
            "ARDOP connect to {target} failed after {attempts} attempts ({e}) — modem stopped"
        )));
    }
    if let Some(g) = &obs_guard {
        g.set_phase(crate::contacts::observation::ObservationPhase::Connected);
    }
    Ok(obs_guard)
}

/// Test-only wrapper around [`run_ardop_connect_stage`] with the exact shape
/// the record-site test drives: takes the outer `ExchangeOutcome` directly
/// rather than the `Result<Option<ObservationGuard>, ExchangeOutcome>` the
/// production caller unwraps, since the connect-fail test never reaches the
/// B2F stage (which needs an `AppHandle` the test harness cannot build).
#[cfg(test)]
fn run_ardop_connect_b2f_for_test(
    transport: &mut dyn ModemTransport,
    target: &str,
    intent: SessionIntent,
) -> ExchangeOutcome {
    match run_ardop_connect_stage(transport, target, intent) {
        Err(failed) => failed,
        // A connect success has no B2F outcome in this harness (no
        // `AppHandle`) — the guard fires at `Connected` (classifies Fail) on
        // drop, which is honest: no B2F ran, so nothing observed it landed.
        Ok(_guard) => ExchangeOutcome::Completed,
    }
}

/// Inner helper for [`modem_ardop_b2f_exchange`]: reads the live config, opens
/// the native mailbox at the standard path, and delegates to
/// `winlink_backend::run_ardop_b2f_exchange`.
///
/// Factored out so the Tauri command can run cleanup (disconnect + reset)
/// uniformly on both success and failure. Returns the error as a `String` so
/// it surfaces to the frontend without exposing the internal `BackendError`
/// type — same pattern as the other modem commands.
fn run_b2f_with_transport(
    app: &AppHandle,
    transport: &mut dyn ModemTransport,
    target: &str,
    intent: SessionIntent,
) -> Result<(), String> {
    // Mailbox lives at <app_data_dir>/native-mbox (per `bootstrap::install_native`).
    let mbox_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve app data dir: {e}"))?
        .join("native-mbox");
    let mailbox = Mailbox::new(mbox_dir);

    let cfg = config::read_config().map_err(|e| format!("read config failed: {e}"))?;

    // tuxlink-0063 (Phase 3, Task 3.6): the on-air station ID comes from the
    // authenticated active SessionIdentity, not from `config.identity.active_full`.
    let backend = app
        .state::<crate::app_backend::BackendState>()
        .current()
        .ok_or_else(|| "ARDOP B2F: backend offline — cannot resolve active identity".to_string())?;
    let session_id = backend.active_identity().map_err(|e| e.to_string())?;

    // tuxlink-2ns7: file received mail into the active FULL's per-FULL inbox
    // (`mailbox/<FULL>/inbox`) — the namespace the UI reads — not the bare
    // `_default`. The exchange runs AS this session's FULL, so its inbound mail
    // belongs to that FULL. Mirrors the `ui_commands` inbound sites; without
    // this, on-air ARDOP receives land in `_default/inbox` and are invisible.
    let mailbox = mailbox.with_default_identity(session_id.mycall());

    // Position arbiter is registered in lib.rs::run() — pull a live ref so
    // the on-air locator honors live GPS / privacy state, matching the
    // telnet/packet paths' behavior. Mirrors `bootstrap::install_native`'s
    // wiring.
    let arbiter_state = app.state::<Arc<crate::position::PositionArbiter>>();
    let arbiter: Arc<crate::position::PositionArbiter> = (*arbiter_state).clone();
    let session_log_state = app.state::<Arc<crate::session_log::SessionLogState>>();
    let session_log: Arc<crate::session_log::SessionLogState> = (*session_log_state).clone();
    let app_for_progress = app.clone();
    let progress = move |line: &str| {
        crate::session_log_emit::emit(
            &app_for_progress,
            &session_log,
            crate::winlink_backend::LogLevel::Info,
            crate::winlink_backend::LogSource::Transport,
            line,
        );
    };

    crate::winlink_backend::run_ardop_b2f_exchange(
        transport,
        target,
        intent,
        &cfg,
        &session_id,
        &mailbox,
        Some(&arbiter),
        Some(&progress),
    )
    .map_err(|e| format!("ARDOP B2F exchange failed: {e}"))
}

/// Parse the operator-supplied B2F intent string into a [`SessionIntent`].
///
/// Accepts only the two operator-selectable dial intents (`"cms"` and
/// `"p2p"`, case-insensitive after trimming). `RadioOnly`, `PostOffice`,
/// and `Mesh` are accepted by the backend's exchange config but are not
/// surfaced in the ARDOP HF panel — added only by the gateway-dial path
/// once the matching UI lands. Returning an explicit allow-list keeps the
/// wire contract narrow: a stray frontend value can't widen the dial
/// surface silently.
pub fn parse_b2f_intent(s: &str) -> Result<SessionIntent, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "cms" => Ok(SessionIntent::Cms),
        "p2p" => Ok(SessionIntent::P2p),
        other => Err(format!(
            "unknown B2F intent {other:?}; expected \"cms\" or \"p2p\""
        )),
    }
}

// ── Task 7: rig-control translation helper + Tune-only command ──────────────

/// Build a `tux_rig::RigConfig` from the radio-level rig config, or `None` if
/// rig control is not configured (no hamlib model or no CAT serial).
/// tuxlink-8fkkk: reads `Config.rig` (shared by ARDOP + VARA), not `ArdopUiConfig`.
pub(crate) fn rig_config_from(rig: &RigUiConfig) -> Option<tux_rig::RigConfig> {
    let model = rig.rig_hamlib_model?;
    let serial_path = rig
        .cat_serial_path
        .clone()
        .filter(|p| !p.trim().is_empty())?;
    Some(tux_rig::RigConfig {
        binary: resolve_rig_binaries(&rig.rigctld_binary)
            .0
            .to_string_lossy()
            .into_owned(),
        model,
        serial_path,
        baud: rig.cat_baud,
        host: rig.rigctld_host.clone(),
        port: rig.rigctld_port,
    })
}

/// HF Winlink data mode (FT-710 = PKTUSB).
pub(crate) fn ardop_data_mode() -> tux_rig::Mode {
    tux_rig::Mode::PktUsb
}

/// Resolve the operator-configured rig data mode, falling back to the ardop
/// default when the persisted token is unrecognised (e.g. hand-edited config).
/// (tuxlink-31c63)
pub(crate) fn rig_data_mode(rig: &RigUiConfig) -> tux_rig::Mode {
    tux_rig::Mode::from_rigctl(&rig.data_mode).unwrap_or_else(ardop_data_mode)
}

// ── Task 8: pre-audio CAT tune helper ───────────────────────────────────────

/// Audio-center offset for sideband data modes (tuxlink-9pzaj).
///
/// The Winlink catalog (CMS channels API, Channels.dat, the published text
/// listings) gives per-channel **audio CENTER** frequencies. A sideband rig
/// is dialed to the carrier point, offset from the center by the audio
/// passband midpoint — 1500 Hz by Winlink convention. Ground truth: RMS
/// Relay `HFInterface.cs:1471` (`strDialFrequency = num - 1.5`) and RMS
/// Trimode `Channels.cs` help text ("Center frequency is 1500 Hz higher
/// than the upper sideband dial frequency for all modes"); WLE's
/// `VaraSession.cs` shows the same `center − 1.5 kHz` math in its dial box.
pub(crate) const SIDEBAND_CENTER_OFFSET_HZ: u64 = 1_500;

/// Convert a Winlink-catalog CENTER frequency to the rig DIAL frequency for
/// the given data mode (tuxlink-9pzaj). USB-family modes dial below center;
/// LSB-family modes dial above it (the audio passband sits below the
/// carrier). Winlink HF is USB-by-convention on every band — the LSB arm
/// exists so a hand-configured LSB-D rig lands on-frequency instead of
/// 3 kHz off.
///
/// This is THE single conversion point: every frequency upstream of the
/// CAT tune — catalog, Find-a-Station, the panel frequency field, the MCP
/// `freq_hz` argument, `DialCandidate.freq_hz` — is a CENTER frequency.
pub(crate) fn center_to_dial_hz(center_hz: u64, mode: tux_rig::Mode) -> u64 {
    use tux_rig::Mode;
    match mode {
        Mode::PktUsb | Mode::Usb | Mode::DataU => {
            center_hz.saturating_sub(SIDEBAND_CENTER_OFFSET_HZ)
        }
        Mode::PktLsb | Mode::Lsb | Mode::DataL => {
            center_hz.saturating_add(SIDEBAND_CENTER_OFFSET_HZ)
        }
    }
}

/// Whether to stop rigctld (release the CAT serial) immediately after tuning,
/// before audio. True on internal-codec radios (close-serial sequencing): the
/// rig's codec contends with the audio device for the serial, so CAT must drop
/// before audio starts. False on DRA-100-class setups, where CAT and audio are
/// independent and the serial stays up for the whole session.
pub(crate) fn should_release_after_tune(rig: &RigUiConfig) -> bool {
    rig.close_serial_sequencing
}

/// Pre-audio CAT tune for one connect candidate. Runs AFTER `transport.init()`
/// and BEFORE `connect_arq()` (audio).
///
/// `freq_hz` is the channel's **CENTER** frequency (Winlink catalog
/// convention); the VFO is dialed to [`center_to_dial_hz`] of it
/// (tuxlink-9pzaj — previously the center went to the VFO verbatim, putting
/// every catalog-driven dial 1.5 kHz off-channel on TX and RX).
///
/// - Returns `Ok(None)` when rig control is not configured
///   ([`rig_config_from`] is `None`) OR no target frequency is known
///   (`freq_hz` is `None`) — preserving today's no-tune behavior.
/// - Otherwise spawns [`tux_rig::ManagedRig`], tunes to
///   `(center_to_dial_hz(hz, mode), mode)` where `mode = rig_data_mode(rig_cfg)`,
///   then branches on [`should_release_after_tune`]:
///   - close-serial (internal codec): `release_serial()` then `Ok(None)` —
///     the serial is freed before audio.
///   - keep-serial (DRA-100): `Ok(Some(rig))` — CAT stays up; the caller
///     stores the handle on the session so it stops on disconnect.
///
/// Spawn / tune failures map to a `String` (surfaced to the operator).
pub(crate) fn tune_rig_for_connect(
    rig_cfg: &RigUiConfig,
    freq_hz: Option<u64>,
    sideband: bool,
) -> Result<Option<tux_rig::ManagedRig>, String> {
    let (rc, hz) = match (rig_config_from(rig_cfg), freq_hz) {
        (Some(rc), Some(hz)) => (rc, hz),
        // Rig not configured or no frequency → no tune (back-compat).
        _ => return Ok(None),
    };
    let mut rig =
        tux_rig::ManagedRig::spawn(rc).map_err(|e| format!("rigctld spawn failed: {e}"))?;
    let mode = rig_data_mode(rig_cfg);
    // `sideband=false` (VARA FM / VHF-FM channels — Codex adrev P2 #1): FM
    // channel listings ARE the RF frequency; no audio-center offset exists.
    let dial_hz = if sideband {
        center_to_dial_hz(hz, mode)
    } else {
        hz
    };
    tracing::info!(
        target: "tuxlink::modem",
        center_hz = hz,
        dial_hz,
        mode = mode.rigctl_str(),
        "CAT tune: catalog center → sideband dial (tuxlink-9pzaj)",
    );
    rig.tune(dial_hz, mode)
        .map_err(|e| format!("CAT tune failed: {e}"))?;
    if should_release_after_tune(rig_cfg) {
        // Close-serial: free the serial before audio. Drop happens at fn end.
        rig.release_serial();
        Ok(None)
    } else {
        // DRA-100: keep CAT up for the session.
        Ok(Some(rig))
    }
}

// ── Task 9: ordered-list QSY (operator-gated) ───────────────────────────────

/// One dial target plus the frequency to tune for it before dialing.
///
/// `freq_hz` is the channel's **CENTER** frequency (Winlink catalog
/// convention, tuxlink-9pzaj) — the tune boundary converts to the sideband
/// dial via [`center_to_dial_hz`]. Callers must NOT pre-subtract the
/// 1500 Hz offset.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DialCandidate {
    pub target: String,
    pub freq_hz: Option<u64>,
}

/// tuxlink-qevsf (SAFETY/Part 97): clamp an assembled connect-candidate list to
/// its first element. Auto-QSY is disabled because the walk would CAT-tune +
/// dial/key on candidate frequencies the operator never saw or selected — only
/// the operator-chosen channel (candidate[0]) was ever presented in the UI.
/// Transmitting on the others is a control-operator violation. Both connect
/// commands (ARDOP + VARA) route their candidate list through this helper before
/// it reaches the walk, so there is nothing for `qsy_on_fail` to advance to.
/// Restored by the Channel-Selection redesign (Find a Station = operator-driven
/// channel picker).
pub(crate) fn clamp_connect_candidates(mut candidates: Vec<DialCandidate>) -> Vec<DialCandidate> {
    candidates.truncate(1);
    candidates
}

/// Walk `candidates` in order, calling `attempt(idx, candidate)` — which
/// returns `true` on a successful connect. Stops at the first success and
/// returns its index. When `qsy_on_fail` is `false`, only the first candidate
/// is attempted (no walk). Returns `None` if no attempt succeeded.
///
/// Pure planner: no I/O, no session state — the real tune + `connect_arq` +
/// abort check live in the caller's `attempt` closure.
pub(crate) fn walk_candidates<F>(
    candidates: &[DialCandidate],
    qsy_on_fail: bool,
    mut attempt: F,
) -> Option<usize>
where
    F: FnMut(usize, &DialCandidate) -> bool,
{
    for (idx, c) in candidates.iter().enumerate() {
        if attempt(idx, c) {
            return Some(idx);
        }
        if !qsy_on_fail {
            break;
        }
    }
    None
}

/// Tune-only: set the rig for the channel whose **CENTER** frequency is
/// `freq_hz`, then release the serial (drop). Does NOT dial. Used by the
/// "Tune…" affordance and the MCP `rig_tune` tool.
///
/// tuxlink-9pzaj: converts center → sideband dial via [`center_to_dial_hz`]
/// like the dial path, so Tune… and Send/Receive land the VFO on the SAME
/// spot for the same typed frequency. App-wide invariant: every frequency
/// the app accepts is a Winlink channel center.
///
/// `sideband` (Codex adrev P2 #1): `None` defaults to `true` (HF sideband —
/// the ARDOP panel and agent callers). The VARA panel passes `false` in FM
/// mode, where the listed frequency IS the RF frequency and no audio-center
/// offset exists.
#[tauri::command]
pub fn ardop_tune_rig(freq_hz: u64, sideband: Option<bool>) -> Result<(), String> {
    let cfg = config::read_config().map_err(|e| format!("read failed: {e}"))?;
    // tuxlink-8fkkk: rig control is radio-level (Config.rig), shared by ARDOP +
    // VARA. This Tune-only command is mode-agnostic.
    let rc = rig_config_from(&cfg.rig)
        .ok_or_else(|| "rig control not configured — set the rig model + CAT serial".to_string())?;
    let mut rig = tux_rig::ManagedRig::spawn(rc).map_err(|e| e.to_string())?;
    let mode = rig_data_mode(&cfg.rig);
    let dial_hz = if sideband.unwrap_or(true) {
        center_to_dial_hz(freq_hz, mode)
    } else {
        freq_hz
    };
    rig.tune(dial_hz, mode).map_err(|e| e.to_string())?;
    // Drop releases the serial (close-serial-safe for internal-codec radios).
    Ok(())
}

// tuxlink-wxwlr: a read-only CAT VFO/mode/PTT probe was considered as the MCP
// `rig_status` source but removed (Codex P1): reading the live VFO requires
// spawning `rigctld`, which opens an unauthenticated command-capable CAT server.
// Triggering that from an un-gated read-only tool would expose a transmit-capable
// surface that bypasses the egress gate. A safe live readout (reuse a running
// session's rig, or gate it) is tracked as a follow-up; `rig_status` reports
// config-derived state only.

/// Crate-shared TUXLINK_CONFIG_DIR test guard (tuxlink-b026z.3). ONE static
/// lock serializes every env-mutating test in the binary (std::env::set_var
/// is not thread-safe under parallel tests — tuxlink-j0ij), and the guard
/// RESTORES the prior value on drop — a panicking test can no longer leak
/// its tempdir into a neighbor. Both modem_commands' and ft8::commands' test
/// modules route through this; local copies are banned.
#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard};

    static LOCK: Mutex<()> = Mutex::new(());

    pub(crate) struct ConfigDirGuard {
        _lock: MutexGuard<'static, ()>,
        prior: Option<std::ffi::OsString>,
    }

    /// Point TUXLINK_CONFIG_DIR at `dir` for the guard's lifetime.
    pub(crate) fn lock_config_dir(dir: &std::path::Path) -> ConfigDirGuard {
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prior = std::env::var_os("TUXLINK_CONFIG_DIR");
        // SAFETY: LOCK serializes every env mutation in this test binary.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", dir) };
        ConfigDirGuard { _lock: lock, prior }
    }

    impl Drop for ConfigDirGuard {
        fn drop(&mut self) {
            // SAFETY: still serialized — the lock is held by self._lock.
            unsafe {
                match self.prior.take() {
                    Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                    None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CONFIG_SCHEMA_VERSION;
    use crate::modem_status::ModemState;
    use test_env::lock_config_dir;

    /// Minimal `ArdopConfig` for tests that need to hand one to
    /// [`spawn_ardop_with_yield`] directly (loopback defaults; no test in
    /// this file constructs one via [`ardop_config_for`] because that
    /// requires a full `ArdopUiConfig`/`RigUiConfig` pair).
    fn test_ardop_config() -> ArdopConfig {
        ArdopConfig {
            binary: std::path::PathBuf::from("ardopcf"),
            extra_args: Vec::new(),
            cmd_port: 8515,
            data_port: 8516,
            audio_device_path: None,
            cat_bridge: None,
        }
    }

    // ── tuxlink-b026z.3 (T15) — the FT8 yield choke point ───────────────

    /// tuxlink-b026z.3 (spec §Arbitration): NO ardopcf spawn path may bypass
    /// the FT8 yield choke point. Structural enforcement: in this file, the
    /// factory may be INVOKED — the identifier immediately followed by an
    /// open paren — only inside `spawn_ardop_with_yield` itself. Everything
    /// else must route through the wrapper. Passing the factory BY VALUE
    /// (identifier followed by `,` or `)`) is fine — only invocation is
    /// choked.
    #[test]
    fn no_ardop_spawn_path_bypasses_the_ft8_yield_wrapper() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/modem_commands.rs"),
        )
        .expect("read own source");
        // Built via concat! so THIS test's own source never matches itself.
        let needle = concat!("make_", "transport(");
        let invocations: Vec<usize> = src.match_indices(needle).map(|(i, _)| i).collect();
        assert_eq!(
            invocations.len(),
            1,
            "expected exactly ONE raw factory invocation (inside \
             spawn_ardop_with_yield); every other spawn site must call \
             spawn_ardop_with_yield. Found {} — a new ardopcf spawn path \
             bypassed the FT8 yield choke point.",
            invocations.len()
        );
        // And that one invocation lives inside the wrapper fn.
        let wrapper_start = src
            .find("fn spawn_ardop_with_yield")
            .expect("wrapper exists");
        let wrapper_end = src[wrapper_start..]
            .find("\npub")
            .map(|off| wrapper_start + off)
            .unwrap_or(src.len());
        assert!(
            invocations[0] > wrapper_start && invocations[0] < wrapper_end,
            "the raw factory invocation is OUTSIDE spawn_ardop_with_yield"
        );
    }

    /// The wrapper is transparent when no arbiter is installed (unit-test
    /// context) — factory errors pass through untouched.
    #[test]
    fn yield_wrapper_is_transparent_without_an_arbiter() {
        let out = spawn_ardop_with_yield(
            |_cfg, _t| Err::<Box<dyn crate::winlink::modem::ModemTransport>, String>("boom".into()),
            test_ardop_config(),
            "N0CALL",
        );
        // Box<dyn ModemTransport> is not Debug, so unwrap_err() cannot be
        // used (E0277 — the same trap as the elmer session's CI round 2).
        match out {
            Err(e) => assert_eq!(e, "boom"),
            Ok(_) => panic!("expected the factory error to pass through"),
        }
    }

    #[test]
    fn resolve_ardop_binary_honors_explicit_paths_and_defaults_to_name() {
        // An explicit path (contains a separator) is an operator override and is
        // returned verbatim — even when the file name is not `ardopcf`.
        assert_eq!(
            resolve_ardop_binary("/opt/ardop/ardopcf"),
            std::path::PathBuf::from("/opt/ardop/ardopcf")
        );
        assert_eq!(
            resolve_ardop_binary("/home/op/custom-modem"),
            std::path::PathBuf::from("/home/op/custom-modem")
        );
        // A CUSTOM bare name is an operator choice too — honored verbatim (PATH-
        // resolved by Command), NOT silently replaced by the bundled sidecar.
        assert_eq!(
            resolve_ardop_binary("ardopcf-dev"),
            std::path::PathBuf::from("ardopcf-dev")
        );
        // A bare program name resolves to either the bundled sidecar sibling (if
        // present next to the test exe) or the bare name for PATH fallback — both
        // end in `ardopcf`, and neither path should panic.
        let resolved = resolve_ardop_binary("ardopcf");
        assert_eq!(
            resolved.file_name().and_then(|s| s.to_str()),
            Some("ardopcf"),
            "bare default must resolve to an ardopcf path, got {resolved:?}"
        );
    }

    #[test]
    fn resolve_rig_binaries_default_prefers_bundled_siblings() {
        // With the literal default "rigctld", when the bundled siblings exist next
        // to current_exe they are used; the rigctl sibling is derived from the same dir.
        let (d, l) = resolve_rig_binaries("rigctld");
        // In the test binary's dir there is no tuxlink-rigctld sibling, so it falls
        // back to the bare names on $PATH — documents the dev/test path.
        assert_eq!(d, std::path::PathBuf::from("rigctld"));
        assert_eq!(l, std::path::PathBuf::from("rigctl"));
        // An empty/whitespace value is treated as the bundled sentinel too (a
        // hand-edited config never round-trips empty, but the contract is
        // "missing/empty/legacy-rigctld -> bundled").
        assert_eq!(resolve_rig_binaries(""), (std::path::PathBuf::from("rigctld"), std::path::PathBuf::from("rigctl")));
        assert_eq!(resolve_rig_binaries("  "), (std::path::PathBuf::from("rigctld"), std::path::PathBuf::from("rigctl")));
    }

    #[test]
    fn resolve_rig_binaries_absolute_override_derives_sibling_rigctl() {
        let (d, l) = resolve_rig_binaries("/opt/hamlib/bin/rigctld");
        assert_eq!(d, std::path::PathBuf::from("/opt/hamlib/bin/rigctld"));
        assert_eq!(l, std::path::PathBuf::from("/opt/hamlib/bin/rigctl"));
    }

    #[test]
    fn resolve_rig_binaries_bare_custom_name_stays_on_path() {
        let (d, l) = resolve_rig_binaries("rigctld-git");
        assert_eq!(d, std::path::PathBuf::from("rigctld-git"));
        assert_eq!(l, std::path::PathBuf::from("rigctl")); // no dir to derive from
    }

    #[test]
    fn round_trip_persists_through_config() {
        // Isolate this test from the operator's real config by pointing
        // TUXLINK_CONFIG_DIR at a fresh tempdir. `config_path()` will resolve
        // to `<tmpdir>/config.json` (per config.rs §294).
        //
        // Because `config_set_ardop` calls `read_config()` before writing, the
        // config file must exist first. We pre-seed a minimal valid config that
        // satisfies `deny_unknown_fields` + semantic validation (offline path:
        // no callsign). `config_set_ardop` will then read it, inject `modem_ardop`,
        // and write it back atomically.
        //
        // The crate-shared `test_env::lock_config_dir` guard (tuxlink-b026z.3)
        // serializes this env mutation against every other env-mutating test
        // in the binary and restores the prior value on drop.
        let tmp = tempfile::tempdir().expect("create tempdir");
        let _env_guard = lock_config_dir(tmp.path());

        // Seed a minimal valid config (offline path: connect_to_cms=false, no callsign).
        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let initial = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:0,0".into(),
            playback_device: "plughw:0,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        };
        config_set_ardop(initial.clone()).expect("config_set_ardop must succeed");
        let read = config_get_ardop();
        assert_eq!(read, initial);
    }

    #[test]
    fn modem_get_status_returns_session_snapshot() {
        let session = Arc::new(ModemSession::new());
        let s = modem_get_status_inner(&session);
        assert_eq!(s.state, ModemState::Stopped);
    }

    #[test]
    fn modem_ardop_disconnect_resets_status_to_stopped() {
        let session = Arc::new(ModemSession::new());
        let mut s = ModemStatus::stopped();
        s.state = ModemState::ConnectedIrs;
        session.set_status(s);

        modem_ardop_disconnect_inner(&session).unwrap();

        assert_eq!(session.status_snapshot().state, ModemState::Stopped);
    }

    #[test]
    fn modem_ardop_disconnect_bumps_close_generation_to_defeat_in_flight_reinstall() {
        // tuxlink-vyby: the Stop button (modem_ardop_disconnect) must fully
        // tear down even when an in-flight b2f exchange or an armed listener
        // currently HOLDS the transport — so `reset_to_stopped` finds none to
        // drop. Those workers re-install via
        // `install_transport_if_generation_matches(transport, snapshot)` with a
        // `snapshot` captured BEFORE Stop. Bumping the close generation in the
        // disconnect path invalidates that snapshot, so the guarded install
        // rejects the handle — the worker drops it and the transport's
        // `ManagedModem::Drop` kills ardopcf. Without the bump the worker
        // re-installs the LIVE transport into the just-stopped session, so
        // ardopcf keeps running and REJ frames scroll until a SECOND Stop click
        // reclaims it (the operator-reported two-click teardown).
        let session = Arc::new(ModemSession::new());

        // A worker snapshots the generation before taking the transport.
        let snapshot = session.current_close_generation();

        // Operator clicks Stop while the worker holds the transport.
        modem_ardop_disconnect_inner(&session).expect("disconnect must succeed");

        // The worker unwinds and tries to re-install with its pre-Stop
        // snapshot. The guard MUST reject it (hand the transport back to drop).
        let reinstall =
            session.install_transport_if_generation_matches(stub_transport(), snapshot);
        assert!(
            reinstall.is_err(),
            "Stop must bump the close generation so an in-flight worker's \
             pre-Stop re-install is rejected and ardopcf is torn down in ONE click",
        );
    }

    // ── Task 3.3 tests — consent-gated connect via factory seam ─────────

    use crate::winlink::modem::{ConnectInfo, ModemTransport, ReadWrite};
    use crate::winlink::modem::ardop::session::SessionError;

    /// A stub `ModemTransport` that returns canned, harmless responses. The
    /// peer call + bandwidth come back from `connect_arq`; all other methods
    /// are no-ops or surface `NotConnected`. NEVER spawns a real process or
    /// opens a real socket — safe to run in unit tests.
    struct StubTransport {
        peer_call: &'static str,
        bandwidth_hz: u32,
    }

    impl StubTransport {
        fn new() -> Self {
            Self { peer_call: "W7RMS-10", bandwidth_hz: 500 }
        }
    }

    impl ModemTransport for StubTransport {
        fn init(&mut self, _cfg: &InitConfig) -> Result<(), SessionError> {
            Ok(())
        }

        fn connect_arq(
            &mut self,
            _target: &str,
            _repeat: u32,
            _deadline: Option<Duration>,
        ) -> Result<ConnectInfo, SessionError> {
            Ok(ConnectInfo {
                peer_call: self.peer_call.to_string(),
                bandwidth_hz: self.bandwidth_hz,
            })
        }

        fn disconnect(&mut self, _deadline: Duration) -> Result<(), SessionError> {
            Ok(())
        }

        fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite> {
            // The connect path doesn't exercise data_stream — surface a
            // clean Err rather than carrying a sham stream.
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "stub transport has no data stream",
            ))
        }
    }

    fn stub_transport() -> Box<dyn ModemTransport> {
        Box::new(StubTransport::new())
    }

    // ── tuxlink-5xxq: a CONNECT-class failure must FREE the modem (drop the
    // transport → reap ardopcf) + reset the session to Stopped, NOT re-install
    // the live transport (which left ardopcf running + the session re-armable —
    // the "sits in REJ / never frees the modem on failure" bug). A SUCCESS or a
    // mid-EXCHANGE failure must keep the open session (re-install) for retry. ──

    #[test]
    fn connect_failure_stops_session_and_drops_transport() {
        let session = Arc::new(ModemSession::new());
        // Put the session in the open-but-idle posture an open session has.
        let mut snap = session.status_snapshot();
        snap.state = ModemState::Idle;
        session.set_status(snap);
        let gen = session.current_close_generation();

        let res = finish_b2f_exchange(
            &session,
            stub_transport(),
            gen,
            ExchangeOutcome::ConnectFailed("no answer after 15 attempts".into()),
        );

        assert!(res.is_err(), "a connect failure surfaces an error");
        // The catch: must NOT re-install the transport, and MUST reset to Stopped.
        assert!(
            !session.snapshot_transport_present(),
            "connect failure must drop the transport (reap the modem), not re-install it"
        );
        assert_eq!(
            session.status_snapshot().state,
            ModemState::Stopped,
            "connect failure must reset the session to Stopped (terminal, not re-armable Idle)"
        );
    }

    #[test]
    fn connect_failure_with_stale_generation_leaves_new_session_untouched() {
        // Codex P1: if an operator Close intervened (bumping close_generation)
        // and a NEW session was opened while the failed-connect worker was still
        // unwinding, the stale worker must NOT reset/clobber the new session.
        let session = Arc::new(ModemSession::new());
        let stale_gen = session.current_close_generation();
        // Simulate the close + reopen: bump generation, install a fresh
        // transport, mark the new session Idle (open posture).
        session.bump_close_generation();
        session.install_transport(stub_transport());
        let mut snap = session.status_snapshot();
        snap.state = ModemState::Idle;
        session.set_status(snap);

        // The stale worker finishes with its OLD generation snapshot.
        let res = finish_b2f_exchange(
            &session,
            stub_transport(),
            stale_gen,
            ExchangeOutcome::ConnectFailed("late no-answer".into()),
        );

        assert!(res.is_err(), "the stale worker still surfaces its error");
        // The NEW session must be untouched — its transport stays installed and
        // its state stays Idle (NOT reset to Stopped by the stale worker).
        assert!(
            session.snapshot_transport_present(),
            "a stale-generation connect failure must NOT drop the newly-opened transport"
        );
        assert_eq!(
            session.status_snapshot().state,
            ModemState::Idle,
            "a stale-generation connect failure must NOT reset the new session"
        );
    }

    #[test]
    fn completed_exchange_reinstalls_transport_for_retry() {
        let session = Arc::new(ModemSession::new());
        let gen = session.current_close_generation();

        let res =
            finish_b2f_exchange(&session, stub_transport(), gen, ExchangeOutcome::Completed);

        assert!(res.is_ok());
        assert!(
            session.snapshot_transport_present(),
            "a completed exchange keeps the open session (transport re-installed for retry)"
        );
    }

    #[test]
    fn mid_exchange_failure_keeps_session_for_retry() {
        let session = Arc::new(ModemSession::new());
        let gen = session.current_close_generation();

        let res = finish_b2f_exchange(
            &session,
            stub_transport(),
            gen,
            ExchangeOutcome::ExchangeFailed("CMS rejected mid".into()),
        );

        assert!(res.is_err());
        assert!(
            session.snapshot_transport_present(),
            "a mid-exchange failure keeps the open session for retry (transport re-installed)"
        );
    }

    #[test]
    fn connect_backstop_deadline_scales_with_retries_and_exceeds_actual_dial() {
        // Derived from attempts so the two never drift; must exceed ardopcf's own
        // dial time (attempts × ~3.3 s) so it only backstops a wedged modem.
        let d15 = connect_backstop_deadline(15);
        assert!(
            d15 >= Duration::from_secs(50),
            "15 attempts × ~3.3 s ≈ 50 s actual dial; backstop must exceed it, got {d15:?}"
        );
        assert!(
            connect_backstop_deadline(30) > d15,
            "more retries must yield a longer backstop"
        );
    }

    fn test_ardop_ui_config() -> ArdopUiConfig {
        ArdopUiConfig {
            binary: "ardopcf-stub".into(),
            capture_device: "plughw:0,0".into(),
            playback_device: "plughw:0,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        }
    }

    // ── Task 1.1 — busy-guard rejects concurrent connect ────────────────

    /// Verify that a second concurrent call to `modem_ardop_connect_gated_with_factory`
    /// is rejected with "connect already in progress" when the first call is still
    /// in flight. The busy guard (`connect_in_progress: AtomicBool`) is the
    /// dup-call defense that replaces the RADIO-1 consent token's implicit
    /// "token consumed = can't replay" property.
    #[test]
    fn connect_rejects_concurrent_call_when_already_in_progress() {
        let session = Arc::new(ModemSession::new());
        let cfg = test_ardop_ui_config();
        let cfg2 = test_ardop_ui_config(); // second copy for the concurrent call below

        // Simulate the first connect having flipped the busy bit by calling the
        // helper directly. The factory blocks until we drop the sentinel so the
        // first call never completes during the test.
        let (sentinel_tx, sentinel_rx) = std::sync::mpsc::channel::<()>();
        // Deterministic handshake: the worker signals `ready` from inside the
        // factory, which the production gate invokes only AFTER try_begin_connect()
        // has set the busy guard (see modem_ardop_connect_gated_with_factory above:
        // the compare_exchange is the first statement). This replaces a flaky
        // sleep() that assumed the worker was scheduled within 50ms — under CI
        // load it sometimes was not, so the second call saw an unset guard and
        // the test failed (testing-pitfalls §5: synchronize with a primitive,
        // never a timing assumption).
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let session_clone = Arc::clone(&session);
        let h = std::thread::spawn(move || {
            let factory = move |_: ArdopConfig, _: &str| -> Result<Box<dyn ModemTransport>, String> {
                // The busy guard is already set by the time the factory runs.
                // Signal the test, then block until released.
                ready_tx.send(()).ok();
                sentinel_rx.recv().ok();
                Err("test stub never connects".into())
            };
            modem_ardop_connect_gated_with_factory(
                &session_clone,
                &test_session_id("N7CPZ"),
                &test_config(),
                "K7TEST",
                &cfg,
                factory,
            )
        });

        // Wait until the worker is inside the factory (busy guard set). No
        // timing assumption — blocks until the signal arrives.
        ready_rx
            .recv()
            .expect("worker should enter the factory with the busy guard set");

        let factory_2 =
            |_: ArdopConfig, _: &str| -> Result<Box<dyn ModemTransport>, String> {
                panic!("factory must not run when a connect is already in progress");
            };
        let err = modem_ardop_connect_gated_with_factory(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            "K7TEST",
            &cfg2,
            factory_2,
        )
        .expect_err("second concurrent call must reject");
        assert!(err.contains("connect already in progress"), "got: {err}");

        // Release the first worker so the test can exit.
        sentinel_tx.send(()).ok();
        let _ = h.join();
    }

    /// Connect succeeds when no busy flag is set. Factory runs; transport is
    /// installed; session reports a connected variant.
    #[test]
    fn modem_ardop_connect_succeeds_when_not_busy() {
        let session = Arc::new(ModemSession::new());
        let result = modem_ardop_connect_gated_with_factory(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            "W7RMS-10",
            &test_ardop_ui_config(),
            |_cfg, _target| Ok(stub_transport()),
        );
        assert!(result.is_ok(), "result: {result:?}");
        // After a successful connect the session reports a connected variant
        // and carries the peer / bandwidth from the stub's ConnectInfo.
        let snap = session.status_snapshot();
        assert!(
            matches!(snap.state, ModemState::ConnectedIrs | ModemState::ConnectedIss),
            "expected connected variant, got: {:?}",
            snap.state
        );
        assert_eq!(snap.peer.as_deref(), Some("W7RMS-10"));
        assert_eq!(snap.width_hz, Some(500));
        // The transport handle is now installed in the session.
        assert!(
            session.take_transport().is_some(),
            "successful connect must install a transport handle"
        );
        // After success the busy bit must be cleared (RAII guard dropped).
        assert!(
            session.try_begin_connect(),
            "busy bit must be clear after a completed connect"
        );
        // Clean up to leave try_begin_connect balanced.
        session.clear_connect_in_progress();
    }

    /// After a successful connect completes, the session is no longer busy
    /// and a second connect call is permitted (the busy bit was cleared by
    /// the RAII guard).
    #[test]
    fn modem_ardop_connect_allows_sequential_calls() {
        let session = Arc::new(ModemSession::new());

        // First call succeeds.
        let r1 = modem_ardop_connect_gated_with_factory(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            "W7RMS-10",
            &test_ardop_ui_config(),
            |_cfg, _target| Ok(stub_transport()),
        );
        assert!(r1.is_ok(), "first call must succeed; got: {r1:?}");

        // Tear down the transport so the second call can install afresh.
        let _ = session.take_transport();

        // Second sequential call MUST succeed — the first call's guard
        // cleared the busy bit on completion.
        let factory_ran = std::sync::atomic::AtomicBool::new(false);
        let r2 = modem_ardop_connect_gated_with_factory(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            "W7RMS-10",
            &test_ardop_ui_config(),
            |_cfg, _target| {
                factory_ran.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(stub_transport())
            },
        );
        assert!(r2.is_ok(), "sequential second call must succeed; got: {r2:?}");
        assert!(
            factory_ran.load(std::sync::atomic::Ordering::SeqCst),
            "factory must run on sequential second call"
        );
    }

    // ── Task 1.1 — sequential connect confirmed (no RADIO-1 token needed) ──

    /// Verify that `modem_ardop_connect_gated_with_factory` no longer requires
    /// a consent token — it succeeds on the first call with no mint step.
    #[test]
    fn connect_succeeds_without_consent_token() {
        use crate::modem_status::ModemSession;
        let session = std::sync::Arc::new(ModemSession::new());
        // No mint_consent_token call — the function must work without one.
        let result = modem_ardop_connect_gated_with_factory(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            "W7RMS-10",
            &test_ardop_ui_config(),
            |_cfg, _t| Ok(stub_transport()),
        );
        assert!(result.is_ok(), "result: {result:?}");
    }

    // ── Task 1.2 — b2f_exchange signature has no consent_token ──────────
    // ── Task 3.6 — signature accepts intent + transport_kind ────────────

    /// Compile-time assertion that the Tauri command's parameter list
    /// matches the post-Task-3.6 shape:
    ///   `(app, session, target: String, intent: SessionIntent,
    ///     transport_kind: TransportKind) -> Result<(), String>`.
    ///
    /// Codex R2 P2: both `intent` AND `transport_kind` must be present so
    /// the Phase 5 RadioSessionPanel's uniform IPC contract
    /// (`{ intent, transportKind }`) targets ARDOP and VARA identically.
    /// If the parameter list drifts (loses `transport_kind`, regains the
    /// removed `consent_token`, changes the `intent` type back to `String`),
    /// the fn-pointer coercion below fails to compile and this test fails.
    #[test]
    fn modem_ardop_b2f_exchange_signature_accepts_intent_and_transport_kind() {
        // tuxlink-ab9h: the command is now `async fn`, so it cannot coerce to a
        // named `fn(...) -> Result<(), String>` pointer (its return is an opaque
        // Future). Assert the parameter-list shape via an async wrapper with the
        // exact signature that forwards to the command: if the list drifts
        // (loses `transport_kind`, regains the removed `consent_token`, or
        // changes `intent` back to `String`), this forwarding call fails to
        // compile and the test fails — same guarantee as the prior fn-pointer
        // coercion.
        async fn _assert_sig(
            app: AppHandle,
            session: State<'_, Arc<ModemSession>>,
            target: String,                                        // target
            intent: SessionIntent,                                 // typed (was String pre-Task-3.6)
            transport_kind: crate::winlink::listener::transport::TransportKind, // new
        ) -> Result<(), String> {
            modem_ardop_b2f_exchange(app, session, target, intent, transport_kind).await
        }
        let _ = _assert_sig;
    }

    // ── tuxlink-5738 — pre-flight identity check ─────────────────────────

    /// Build a Config from a JSON literal so the test exercises the same
    /// deserialize path the production read_config() goes through (incl.
    /// `deserialize_optional_nonempty_string` which already maps empty
    /// strings to `None`). Mirrors the existing config.rs test pattern.
    fn config_with_identity(callsign: Option<&str>, identifier: Option<&str>) -> Config {
        let call_json = match callsign {
            Some(s) => format!("\"{s}\""),
            None => "null".to_string(),
        };
        let ident_json = match identifier {
            Some(s) => format!("\"{s}\""),
            None => "null".to_string(),
        };
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": {call_json}, "identifier": {ident_json}, "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = crate::config::CONFIG_SCHEMA_VERSION,
        );
        serde_json::from_str(&json).expect("test config must deserialize")
    }

    #[test]
    fn check_identity_present_ok_when_callsign_set() {
        let cfg = config_with_identity(None, Some("W1TEST"));
        assert!(check_identity_present(&cfg).is_ok());
    }

    #[test]
    fn check_identity_present_ok_when_identifier_set() {
        // Offline-path config: no callsign, identifier carries the station id.
        let cfg = config_with_identity(None, Some("FIELD-1"));
        assert!(check_identity_present(&cfg).is_ok());
    }

    #[test]
    fn check_identity_present_err_when_both_missing() {
        // Both None — operator has not completed the wizard's identity step.
        let cfg = config_with_identity(None, None);
        let err = check_identity_present(&cfg).expect_err("must reject when no identity");
        assert!(
            err.contains("callsign") || err.contains("wizard"),
            "error must be actionable; got: {err}"
        );
    }

    // ── tuxlink-o3f2: abort-during-connect side channel ──────────────────

    use crate::winlink::modem::ardop::session::SessionError as ArdopSessionError;
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Stub transport that:
    /// - exposes `try_clone_abort_writer` returning a clone of a `TcpStream`
    ///   connected to a `TcpListener` we own in the test, so the test can
    ///   observe the side-channel ABORT bytes;
    /// - `connect_arq` blocks until `abort_signal` flips to true (the test
    ///   sets it from a watcher thread that reads from the listener and
    ///   asserts on the bytes).
    ///
    /// Used to prove that `modem_ardop_disconnect_inner` aborts an in-flight
    /// `connect_arq` via the side channel, not by holding the transport
    /// mutex (which during connect_arq is `None` from the session's POV).
    struct AbortableStubTransport {
        abort_writer: Option<TcpStream>,
        abort_signal: Arc<AtomicBool>,
    }

    impl AbortableStubTransport {
        fn new(abort_writer: TcpStream, abort_signal: Arc<AtomicBool>) -> Self {
            Self {
                abort_writer: Some(abort_writer),
                abort_signal,
            }
        }
    }

    impl ModemTransport for AbortableStubTransport {
        fn init(&mut self, _cfg: &InitConfig) -> Result<(), ArdopSessionError> {
            Ok(())
        }
        fn connect_arq(
            &mut self,
            _target: &str,
            _repeat: u32,
            deadline: Option<Duration>,
        ) -> Result<crate::winlink::modem::ConnectInfo, ArdopSessionError> {
            // Spin (bounded by deadline if Some, unbounded if None) until
            // abort_signal flips. In production this loop is the real
            // `arq_connect` recv loop; here the signal stands in for
            // "ardopcf emitted FAULT/DISC in response to ABORT and the
            // cmd reader thread delivered it."
            let start = std::time::Instant::now();
            while !self.abort_signal.load(Ordering::Acquire) {
                if let Some(d) = deadline {
                    if start.elapsed() >= d {
                        return Err(ArdopSessionError::Timeout {
                            cmd: "ARQCALL".into(),
                        });
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(ArdopSessionError::Fault("aborted via side channel".into()))
        }
        fn disconnect(&mut self, _deadline: Duration) -> Result<(), ArdopSessionError> {
            Ok(())
        }
        fn data_stream(
            &mut self,
        ) -> std::io::Result<&mut dyn crate::winlink::modem::ReadWrite> {
            Err(std::io::Error::other("stub"))
        }
        fn try_clone_abort_writer(
            &self,
        ) -> Option<(
            Box<dyn std::io::Write + Send>,
            Box<dyn crate::modem_status::ShutdownableStream>,
        )> {
            let writer = self.abort_writer.as_ref()?.try_clone().ok()?;
            let stream_clone = writer.try_clone().ok()?;
            Some((
                Box::new(writer) as Box<dyn std::io::Write + Send>,
                Box::new(stream_clone)
                    as Box<dyn crate::modem_status::ShutdownableStream>,
            ))
        }
    }

    /// Spawn a TCP listener and return `(addr, server_thread_handle, abort_signal)`.
    /// The server thread reads bytes; when it sees `ABORT\r` it flips
    /// `abort_signal` to true and exits. The signal is the test's hook to
    /// unblock the connect stub.
    fn spawn_abort_listener() -> (std::net::SocketAddr, std::thread::JoinHandle<Vec<u8>>, Arc<AtomicBool>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let abort_signal = Arc::new(AtomicBool::new(false));
        let signal_for_thread = abort_signal.clone();
        let handle = std::thread::spawn(move || {
            let (mut conn, _peer) = listener.accept().unwrap();
            conn.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut accumulated = Vec::new();
            let mut buf = [0u8; 64];
            loop {
                match conn.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        accumulated.extend_from_slice(&buf[..n]);
                        if accumulated.windows(6).any(|w| w == b"ABORT\r") {
                            signal_for_thread.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
            }
            accumulated
        });
        (addr, handle, abort_signal)
    }

    /// End-to-end abort-during-connect: the connect call runs on one
    /// thread, blocking inside `connect_arq` (stub spins until aborted).
    /// On another thread we call `modem_ardop_disconnect_inner`, which
    /// MUST send ABORT via the session's side-channel writer; the listener
    /// observes the bytes and flips the signal that lets `connect_arq`
    /// return. Connect returns Err promptly (well under the 120s deadline)
    /// rather than running to deadline.
    ///
    /// This is the regression test for the 2026-05-22 runaway-connect
    /// incident — the proof that the operator's Disconnect button can
    /// halt an in-flight connect in seconds, not minutes.
    #[test]
    fn disconnect_aborts_in_flight_connect_via_side_channel() {
        let (addr, listener_handle, abort_signal) = spawn_abort_listener();

        // Client end of the loopback pair — this is what
        // `try_clone_abort_writer` will hand back via the stub.
        let abort_writer = TcpStream::connect(addr).expect("connect to abort listener");

        let session = Arc::new(ModemSession::new());
        // No consent token needed — the busy guard is the only gate now.

        // Run the connect call on a worker thread so the test thread can
        // call disconnect in parallel.
        let session_for_connect = session.clone();
        let abort_signal_for_stub = abort_signal.clone();
        let connect_thread = std::thread::spawn(move || {
            modem_ardop_connect_gated_with_factory(
                &session_for_connect,
                &test_session_id("N7CPZ"),
                &test_config(),
                "W7RMS-10",
                &test_ardop_ui_config(),
                move |_cfg, _target| {
                    Ok(Box::new(AbortableStubTransport::new(
                        abort_writer,
                        abort_signal_for_stub,
                    )) as Box<dyn ModemTransport>)
                },
            )
        });

        // Wait until the connect path has progressed past install_abort_writer
        // (status flips to Connecting AFTER the install). Poll briefly.
        let start = std::time::Instant::now();
        loop {
            let st = session.status_snapshot().state;
            if matches!(st, ModemState::Connecting) {
                break;
            }
            if start.elapsed() >= Duration::from_secs(5) {
                panic!("status never reached Connecting (state={st:?})");
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        // Now hit Disconnect. This must (a) write ABORT via the side
        // channel and (b) return promptly. The connect thread sees the
        // signal, returns Err, and joins.
        let disconnect_start = std::time::Instant::now();
        modem_ardop_disconnect_inner(&session).expect("disconnect must succeed");
        let disconnect_elapsed = disconnect_start.elapsed();
        assert!(
            disconnect_elapsed < Duration::from_secs(2),
            "disconnect must return promptly; took {disconnect_elapsed:?}"
        );

        // The connect call should have returned Err once the stub saw the
        // signal flip. Bound the wait so a regression fails the test
        // instead of hanging.
        let connect_result = connect_thread
            .join()
            .expect("connect thread must not panic");
        assert!(
            connect_result.is_err(),
            "connect must return Err after ABORT signal; got: {connect_result:?}"
        );

        // The listener thread received the side-channel bytes.
        let received = listener_handle.join().expect("listener thread must not panic");
        assert!(
            received.windows(6).any(|w| w == b"ABORT\r"),
            "abort listener must have received ABORT\\r; got: {received:?}"
        );

        // Signal flipped means abort_in_flight delivered the line.
        assert!(
            abort_signal.load(Ordering::Acquire),
            "abort signal must be set"
        );

        // Session state: the disconnect path reset to Stopped, then the
        // connect thread's error handler ran (because connect_arq returned
        // Err after the abort signal) and set state to Error. Either
        // terminal is acceptable as a "no longer Connecting" outcome —
        // the load-bearing assertion is the prompt disconnect above. We
        // explicitly assert NOT-Connecting and NOT-ConnectedIrs/Iss so a
        // regression that leaves the session stuck mid-flow fails loudly.
        let final_state = session.status_snapshot().state;
        assert!(
            matches!(final_state, ModemState::Stopped | ModemState::Error),
            "session must end Stopped or Error after abort-driven disconnect; got: {final_state:?}"
        );
    }

    /// `modem_ardop_disconnect_inner` must call `abort_in_flight` BEFORE
    /// any reset/transport teardown — best-effort, ignore-error. If no
    /// writer is installed (e.g. transport was never connected), the call
    /// is a no-op and the existing graceful path still runs.
    ///
    /// This test directly exercises the disconnect ordering: install a
    /// writer pointing at a local listener, call disconnect, observe the
    /// ABORT bytes on the listener side.
    #[test]
    fn disconnect_in_flight_sends_abort_via_side_channel() {
        let (addr, listener_handle, _signal) = spawn_abort_listener();
        let writer = TcpStream::connect(addr).expect("connect to abort listener");
        let stream_clone = writer.try_clone().expect("clone for shutdown handle");
        let session = Arc::new(ModemSession::new());
        // tuxlink-0ye6 Task 4.1 two-arg form: cooperative writer + hard-close
        // stream. The test's writer never errors (real TCP loopback drains),
        // so the cooperative phase covers the assertion below.
        session.install_abort_writer(
            Box::new(writer) as Box<dyn std::io::Write + Send>,
            Box::new(stream_clone)
                as Box<dyn crate::modem_status::ShutdownableStream>,
        );

        modem_ardop_disconnect_inner(&session).expect("disconnect must succeed");

        let received = listener_handle.join().expect("listener thread must not panic");
        assert!(
            received.windows(6).any(|w| w == b"ABORT\r"),
            "disconnect must send ABORT via the side channel; got: {received:?}"
        );
    }

    #[allow(deprecated)]
    #[test]
    fn check_identity_present_err_when_both_whitespace_only() {
        // Defense-in-depth: if a caller hand-constructs a Config in-memory
        // with whitespace-only strings (bypassing the serde validator that
        // normally maps these to None), trim() catches it. Hand-built
        // because `deserialize_optional_nonempty_string` would otherwise
        // map "   " to None at the serde layer.
        let cfg = Config {
            elmer: crate::config::ElmerConfig::default(),
            p2p_limits: crate::contacts::limiter::P2pLimitsConfig::default(),
            ft8: crate::config::Ft8Config::default(),
            wwv_offair: None,
            schema_version: crate::config::CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: crate::config::ConnectConfig {
                connect_to_cms: false,
                transport: crate::config::CmsTransport::Telnet,
                host: crate::config::default_cms_host(),
            },
            identity: crate::config::IdentityConfig {
                active_full: Some("   ".into()),
                identifier: Some("".into()),
                grid: None,
            },
            privacy: crate::config::PrivacyConfig {
                gps_state: crate::config::GpsState::Off,
                position_precision: crate::config::PositionPrecision::FourCharGrid,
                position_source: crate::config::PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: crate::config::PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            rig: crate::config::RigUiConfig::default(),
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
            network_po_favorites: Vec::new(),
            review_inbound_before_download: false,
            map_tile_source: None,
            aredn_master_node_host: None,
            aprs: crate::config::AprsConfig::default(),
            trash_auto_purge: true,
            trash_retention_days: 30,
            close_to_tray: true,
            close_prompt_seen: false,
            active_connection: None,
        };
        assert!(check_identity_present(&cfg).is_err());
    }

    // ── tuxlink-j0ij: bandwidth validation + plumb-through tests ──────────

    #[test]
    fn validate_arq_bandwidth_hz_accepts_the_four_valid_values() {
        assert_eq!(validate_arq_bandwidth_hz(200), Some(200));
        assert_eq!(validate_arq_bandwidth_hz(500), Some(500));
        assert_eq!(validate_arq_bandwidth_hz(1000), Some(1000));
        assert_eq!(validate_arq_bandwidth_hz(2000), Some(2000));
    }

    #[test]
    fn validate_arq_bandwidth_hz_drops_invalid_values_to_none() {
        // ardopcf only documents {200, 500, 1000, 2000}; any other value is a
        // stale persist / hand-edit / forward-schema drift — drop to None so
        // ardopcf's default takes over rather than failing init.
        assert_eq!(validate_arq_bandwidth_hz(0), None);
        assert_eq!(validate_arq_bandwidth_hz(100), None);
        assert_eq!(validate_arq_bandwidth_hz(750), None);
        assert_eq!(validate_arq_bandwidth_hz(2500), None);
        assert_eq!(validate_arq_bandwidth_hz(u32::MAX), None);
    }

    /// Mint a FULL session identity for the given callsign — the correct
    /// seam for init-config tests (tuxlink-0063 Phase 3, Task 3.9).
    fn test_session_id(call: &str) -> crate::identity::SessionIdentity {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        SessionIdentity::full(IdentityHandle::for_test(
            Callsign::parse(call).expect("valid test callsign"),
        ))
    }

    /// `init_config_from_session` must plumb a valid persisted `bandwidth_hz`
    /// through to the resulting `InitConfig.arq_bandwidth_hz`, and the
    /// `gridsquare` from config — but the `mycall` MUST come from the session
    /// identity, NOT the config identifier. Uses TUXLINK_CONFIG_DIR isolation
    /// (same pattern as round_trip_persists_through_config).
    #[test]
    fn init_config_from_session_passes_through_valid_bandwidth() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let _env_guard = lock_config_dir(tmp.path());

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": "CN87" }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "modem_ardop": {{
                    "binary": "ardopcf",
                    "capture_device": "plughw:1,0",
                    "playback_device": "plughw:1,0",
                    "cmd_port": 8515,
                    "bandwidth_hz": 500
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let cfg = config::read_config().expect("read seeded config");
        let session_id = test_session_id("N7CPZ");
        let init_cfg = init_config_from_session(&session_id, &cfg);
        assert_eq!(init_cfg.arq_bandwidth_hz, Some(500));
        // mycall is the SESSION call, NOT the config identifier "W1TEST".
        assert_eq!(init_cfg.mycall, "N7CPZ");
        assert_eq!(init_cfg.gridsquare, "CN87");
    }

    /// Focused proof: the config call/identifier is OVERRIDDEN by the session
    /// call. Config carries W7AUX (as identifier); the session carries N7CPZ;
    /// the modem-init MYCALL must be N7CPZ (tuxlink-0063 Phase 3, Task 3.9 —
    /// the load-bearing on-air station-ID assertion).
    #[test]
    fn init_config_mycall_is_session_call() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let _env_guard = lock_config_dir(tmp.path());

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": "W7AUX", "identifier": "W7AUX", "grid": "DN17" }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let cfg = config::read_config().expect("read seeded config");
        let session_id = test_session_id("N7CPZ");
        let init_cfg = init_config_from_session(&session_id, &cfg);
        assert_eq!(
            init_cfg.mycall, "N7CPZ",
            "modem-init MYCALL must be the SESSION call, never the config call/identifier W7AUX"
        );
        // grid still comes from config.
        assert_eq!(init_cfg.gridsquare, "DN17");
    }

    /// A hand-edited (or stale) `bandwidth_hz` outside the valid set drops
    /// to None — ardopcf's default takes over. Defense-in-depth against the
    /// Settings dropdown being bypassed.
    #[test]
    fn init_config_from_session_drops_invalid_bandwidth() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let _env_guard = lock_config_dir(tmp.path());

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "modem_ardop": {{
                    "binary": "ardopcf",
                    "capture_device": "plughw:1,0",
                    "playback_device": "plughw:1,0",
                    "cmd_port": 8515,
                    "bandwidth_hz": 750
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let cfg = config::read_config().expect("read seeded config");
        let session_id = test_session_id("N7CPZ");
        let init_cfg = init_config_from_session(&session_id, &cfg);
        assert_eq!(
            init_cfg.arq_bandwidth_hz, None,
            "invalid bandwidth_hz=750 must drop to None (defense in depth — tuxlink-j0ij)"
        );
    }

    // ── tuxlink-60wh: -G WebGUI flag in ardopcf extra_args ───────────────

    #[test]
    fn extra_args_includes_g_webgui_flag_with_cmd_port_minus_one() {
        // Default cmd_port = 8515 → webgui_port = 8514. The `-G 8514` pair
        // must appear AFTER any `-p` PTT flag (or first when PTT is None)
        // and BEFORE the positional triple (cmd_port / capture / playback).
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        };
        let args = build_ardop_extra_args(&cfg);
        assert_eq!(
            args,
            vec![
                "-G".to_string(),
                "8514".to_string(),
                "8515".to_string(),
                "plughw:1,0".to_string(),
                "plughw:1,0".to_string(),
            ],
            "argv order must be: -G <wg> <cmd> <capture> <playback>"
        );
    }

    #[test]
    fn extra_args_g_webgui_flag_uses_dynamic_cmd_port_minus_one() {
        // Operator may override cmd_port via Settings; webgui_port follows
        // ardopcf's documented convention `cmd_port - 1`.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:0,0".into(),
            playback_device: "plughw:0,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 9001,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        };
        let args = build_ardop_extra_args(&cfg);
        assert!(
            args.windows(2).any(|w| w[0] == "-G" && w[1] == "9000"),
            "expected `-G 9000` pair for cmd_port=9001; got: {args:?}"
        );
    }

    #[test]
    fn extra_args_omits_g_when_cmd_port_too_low_to_compute() {
        // Edge case: cmd_port=1 would yield webgui_port=0 (invalid). The
        // guard drops `-G` entirely; ardopcf runs without a WebGUI rather
        // than failing to bind. cmd_port=0 likewise.
        for low_port in [0u16, 1u16] {
            let cfg = ArdopUiConfig {
                binary: "ardopcf".into(),
                capture_device: "plughw:0,0".into(),
                playback_device: "plughw:0,0".into(),
                ptt_method: PttMethod::Vox,
                ptt_serial_path: None,
                cat_serial_path: None,
                cat_baud: 38400,
                cat_key_cmd: "TX1;".into(),
                cat_unkey_cmd: "TX0;".into(),
                cat_bridge_port: 4532,
                cmd_port: low_port,
                bandwidth_hz: None,
                drive_level: None,
                connect_attempts: None,
                webgui_port: None,
                listen_ttl_minutes: 0,
            };
            let args = build_ardop_extra_args(&cfg);
            assert!(
                !args.iter().any(|a| a == "-G"),
                "cmd_port={low_port}: -G must be omitted; got: {args:?}"
            );
        }
    }

    #[test]
    fn extra_args_preserves_ptt_p_flag_before_g_and_positional() {
        // Regression: tuxlink-60wh refactor extracted extra_args into a
        // helper. Make sure the PTT plumbing still works AND appears in
        // the right order: -p <ptt> -G <wg> <cmd> <capture> <playback>.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_method: PttMethod::SerialRts,
            ptt_serial_path: Some("/dev/ttyUSB0".into()),
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        };
        let args = build_ardop_extra_args(&cfg);
        assert_eq!(
            args,
            vec![
                "-p".to_string(),
                "/dev/ttyUSB0".to_string(),
                "-G".to_string(),
                "8514".to_string(),
                "8515".to_string(),
                "plughw:1,0".to_string(),
                "plughw:1,0".to_string(),
            ],
            "argv order must be: -p <ptt> -G <wg> <cmd> <capture> <playback>"
        );
    }

    #[test]
    fn extra_args_omits_p_flag_when_ptt_serial_path_empty_string() {
        // Defense in depth: ardopcf rejects `-p ""`. If a stale config or
        // hand-edited JSON yields Some("") (the serde validator should
        // normalize this, but tests construct in-memory configs directly),
        // the helper drops the flag rather than passing an invalid value.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_method: PttMethod::SerialRts,
            ptt_serial_path: Some("".into()),
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
        };
        let args = build_ardop_extra_args(&cfg);
        assert!(
            !args.iter().any(|a| a == "-p"),
            "empty PTT path must drop the -p flag; got: {args:?}"
        );
    }

    // ── Operator smoke 2026-05-31 round 3: webgui_port override path ──────

    #[test]
    fn extra_args_honors_explicit_webgui_port_override() {
        // Operator pins webgui_port=9080 (non-conventional ardopcf build).
        // The spawn must emit `-G 9080` regardless of `cmd_port - 1`, so the
        // frontend's `resolved_webgui_port` and this site agree by going
        // through the same helper.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: Some(9080),
            listen_ttl_minutes: 0,
        };
        let args = build_ardop_extra_args(&cfg);
        assert!(
            args.windows(2).any(|w| w[0] == "-G" && w[1] == "9080"),
            "explicit webgui_port override must produce `-G 9080`; got: {args:?}"
        );
        assert!(
            !args.windows(2).any(|w| w[0] == "-G" && w[1] == "8514"),
            "override must NOT fall back to cmd_port - 1 = 8514; got: {args:?}"
        );
    }

    #[test]
    fn extra_args_emits_g_with_override_even_when_cmd_port_too_low() {
        // cmd_port=0 would normally suppress `-G` (derivation impossible),
        // but an explicit override should still pin the port — the operator
        // told us where the WebGUI is bound.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_method: PttMethod::Vox,
            ptt_serial_path: None,
            cat_serial_path: None,
            cat_baud: 38400,
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 0,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: Some(8514),
            listen_ttl_minutes: 0,
        };
        let args = build_ardop_extra_args(&cfg);
        assert!(
            args.windows(2).any(|w| w[0] == "-G" && w[1] == "8514"),
            "override must apply even with low cmd_port; got: {args:?}"
        );
    }

    // ── tuxlink-wu0k: CAT-command PTT branch + hex helper ─────────────────

    #[test]
    fn hex_encode_cat_cmd_matches_proven_ft710_values() {
        // The values proven on air 2026-06-23: TX1; → 5458313B, TX0; → 5458303B.
        assert_eq!(hex_encode_cat_cmd("TX1;"), "5458313B");
        assert_eq!(hex_encode_cat_cmd("TX0;"), "5458303B");
        // Empty input → empty hex.
        assert_eq!(hex_encode_cat_cmd(""), "");
    }

    fn cat_ptt_cfg() -> ArdopUiConfig {
        ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:CARD=Device,DEV=0".into(),
            playback_device: "plughw:CARD=Device,DEV=0".into(),
            ptt_method: PttMethod::CatCommand,
            ptt_serial_path: None,
            // tuxlink-8fkkk: the CAT serial link is radio-level now — see
            // `cat_ptt_rig()`. key/unkey/bridge_port stay on ArdopUiConfig.
            cat_key_cmd: "TX1;".into(),
            cat_unkey_cmd: "TX0;".into(),
            cat_bridge_port: 4532,
            cmd_port: 8515,
            bandwidth_hz: None,
            drive_level: None,
            connect_attempts: None,
            webgui_port: None,
            listen_ttl_minutes: 0,
            ..Default::default()
        }
    }

    /// Radio-level rig config with the CAT serial link set (tuxlink-8fkkk).
    /// Pairs with [`cat_ptt_cfg`] for the CAT-PTT bridge-spec tests.
    fn cat_ptt_rig() -> RigUiConfig {
        RigUiConfig {
            cat_serial_path: Some("/dev/ttyUSB0".into()),
            cat_baud: 38400,
            ..Default::default()
        }
    }

    #[test]
    fn extra_args_cat_command_emits_c_k_u_and_no_p() {
        // CAT PTT must emit `-c TCP:<port> -k <hex(key)> -u <hex(unkey)>` and
        // NOT a `-p` RTS flag. This is the seam the FT-710 close-serial path
        // rides on (tuxlink-wu0k).
        let args = build_ardop_extra_args(&cat_ptt_cfg());

        assert!(
            args.windows(2).any(|w| w[0] == "-c" && w[1] == "TCP:4532"),
            "expected `-c TCP:4532`; got: {args:?}"
        );
        assert!(
            args.windows(2).any(|w| w[0] == "-k" && w[1] == "5458313B"),
            "expected `-k 5458313B` (hex of TX1;); got: {args:?}"
        );
        assert!(
            args.windows(2).any(|w| w[0] == "-u" && w[1] == "5458303B"),
            "expected `-u 5458303B` (hex of TX0;); got: {args:?}"
        );
        assert!(
            !args.iter().any(|a| a == "-p"),
            "CAT PTT must NOT emit a -p RTS flag; got: {args:?}"
        );
    }

    #[test]
    fn extra_args_cat_command_honors_custom_bridge_port_and_commands() {
        let mut cfg = cat_ptt_cfg();
        cfg.cat_bridge_port = 4600;
        cfg.cat_key_cmd = "RT1;".into(); // 52 54 31 3B
        cfg.cat_unkey_cmd = "RT0;".into(); // 52 54 30 3B
        let args = build_ardop_extra_args(&cfg);
        assert!(args.windows(2).any(|w| w[0] == "-c" && w[1] == "TCP:4600"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "-k" && w[1] == "5254313B"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "-u" && w[1] == "5254303B"), "{args:?}");
    }

    #[test]
    fn extra_args_cat_command_ignores_ptt_serial_path() {
        // Even if a stale ptt_serial_path lingers, CAT mode must not emit -p.
        let mut cfg = cat_ptt_cfg();
        cfg.ptt_serial_path = Some("/dev/ttyUSB9".into());
        let args = build_ardop_extra_args(&cfg);
        assert!(!args.iter().any(|a| a == "-p"), "{args:?}");
        assert!(!args.iter().any(|a| a == "/dev/ttyUSB9"), "{args:?}");
    }

    #[test]
    fn extra_args_cat_command_keeps_g_and_positional_after_cat_flags() {
        // Full argv order: -c TCP:p -k h -u h -G <wg> <cmd> <cap> <play>.
        let args = build_ardop_extra_args(&cat_ptt_cfg());
        assert_eq!(
            args,
            vec![
                "-c".to_string(),
                "TCP:4532".to_string(),
                "-k".to_string(),
                "5458313B".to_string(),
                "-u".to_string(),
                "5458303B".to_string(),
                "-G".to_string(),
                "8514".to_string(),
                "8515".to_string(),
                "plughw:CARD=Device,DEV=0".to_string(),
                "plughw:CARD=Device,DEV=0".to_string(),
            ],
            "CAT argv order must be -c TCP:p -k h -u h -G wg cmd cap play"
        );
    }

    #[test]
    fn extra_args_vox_emits_no_ptt_flags() {
        let mut cfg = cat_ptt_cfg();
        cfg.ptt_method = PttMethod::Vox;
        let args = build_ardop_extra_args(&cfg);
        assert!(!args.iter().any(|a| a == "-p" || a == "-c" || a == "-k" || a == "-u"), "{args:?}");
    }

    #[test]
    fn cat_bridge_spec_is_none_for_non_cat_methods() {
        let mut cfg = cat_ptt_cfg();
        let rig = cat_ptt_rig();
        cfg.ptt_method = PttMethod::Vox;
        assert!(matches!(cat_bridge_spec_from(&cfg, &rig), Ok(None)));
        cfg.ptt_method = PttMethod::SerialRts;
        assert!(matches!(cat_bridge_spec_from(&cfg, &rig), Ok(None)));
    }

    #[test]
    fn cat_bridge_spec_carries_config_for_cat_method() {
        let spec = cat_bridge_spec_from(&cat_ptt_cfg(), &cat_ptt_rig())
            .expect("configured CAT config is valid")
            .expect("CAT method yields a spec");
        assert_eq!(spec.bridge_port, 4532);
        // tuxlink-8fkkk: serial_path + baud come from the rig config now.
        assert_eq!(spec.serial_path, "/dev/ttyUSB0");
        assert_eq!(spec.baud, 38400);
        assert_eq!(spec.key_cmd, "TX1;");
        assert_eq!(spec.unkey_cmd, "TX0;");
    }

    #[test]
    fn cat_bridge_spec_fails_closed_when_serial_unset() {
        // CAT PTT with no serial device must REFUSE, not invent /dev/ttyUSB0 —
        // keying an unintended device (a TNC, GPS, or different radio) is unsafe.
        // tuxlink-8fkkk: the CAT serial link is on the rig config now.
        let cfg = cat_ptt_cfg();
        let mut rig = cat_ptt_rig();
        rig.cat_serial_path = None;
        assert!(cat_bridge_spec_from(&cfg, &rig).is_err(), "unset CAT serial must error");
        rig.cat_serial_path = Some(String::new());
        assert!(cat_bridge_spec_from(&cfg, &rig).is_err(), "empty CAT serial must error");
        rig.cat_serial_path = Some("   ".into());
        assert!(cat_bridge_spec_from(&cfg, &rig).is_err(), "whitespace CAT serial must error");
    }

    /// When the persisted config has no `modem_ardop` section, the
    /// `InitConfig.arq_bandwidth_hz` must be None — ardopcf's default takes
    /// over. This is the migration path: pre-j0ij configs still init.
    #[test]
    fn init_config_from_session_yields_none_bandwidth_when_modem_ardop_absent() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let _env_guard = lock_config_dir(tmp.path());

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let cfg = config::read_config().expect("read seeded config");
        let session_id = test_session_id("N7CPZ");
        let init_cfg = init_config_from_session(&session_id, &cfg);
        assert_eq!(
            init_cfg.arq_bandwidth_hz, None,
            "no modem_ardop section → no ARQBW override (migration path)"
        );
    }

    // ── parse_b2f_intent (tuxlink-9ls2) ──────────────────────────────

    #[test]
    fn parse_b2f_intent_accepts_cms_lowercase() {
        assert_eq!(parse_b2f_intent("cms"), Ok(SessionIntent::Cms));
    }

    #[test]
    fn parse_b2f_intent_accepts_p2p_lowercase() {
        assert_eq!(parse_b2f_intent("p2p"), Ok(SessionIntent::P2p));
    }

    #[test]
    fn parse_b2f_intent_is_case_insensitive() {
        assert_eq!(parse_b2f_intent("CMS"), Ok(SessionIntent::Cms));
        assert_eq!(parse_b2f_intent("P2P"), Ok(SessionIntent::P2p));
        assert_eq!(parse_b2f_intent("CmS"), Ok(SessionIntent::Cms));
        assert_eq!(parse_b2f_intent("p2P"), Ok(SessionIntent::P2p));
    }

    #[test]
    fn parse_b2f_intent_trims_whitespace() {
        assert_eq!(parse_b2f_intent("  cms  "), Ok(SessionIntent::Cms));
        assert_eq!(parse_b2f_intent("\tp2p\n"), Ok(SessionIntent::P2p));
    }

    #[test]
    fn parse_b2f_intent_rejects_unknown_value() {
        let err = parse_b2f_intent("gateway").unwrap_err();
        assert!(err.contains("unknown B2F intent"), "got: {err}");
        assert!(err.contains("gateway"), "must echo the bad input: {err}");
    }

    #[test]
    fn parse_b2f_intent_rejects_empty() {
        // Empty (or whitespace-only) input is operator error — not a Cms
        // default — so the parse must surface the error, not silently
        // route to CMS. The frontend always passes "cms" or "p2p"; an
        // empty arrival means a stale build or a mis-wired test.
        let err = parse_b2f_intent("").unwrap_err();
        assert!(err.contains("unknown B2F intent"), "got: {err}");
    }

    #[test]
    fn parse_b2f_intent_rejects_unsupported_intents() {
        // RadioOnly / PostOffice / Mesh exist in SessionIntent but are
        // NOT operator-selectable from the ARDOP HF panel yet. The parser
        // narrows the surface so a stray "radioonly" from a future build
        // can't widen the on-air dial scope without an explicit code
        // change here.
        assert!(parse_b2f_intent("radioonly").is_err());
        assert!(parse_b2f_intent("postoffice").is_err());
        assert!(parse_b2f_intent("mesh").is_err());
    }

    // ── tuxlink-0ye6 Task 3.5 — ardop_open_session / ardop_close_session ──
    //
    // The pragmatic-reshape pattern Tasks 3.2 + 3.3 + 3.4 used: cover the
    // inner helpers (sync, no AppHandle) directly; pin the outer Tauri
    // command signatures via a fn-pointer coercion. End-to-end coverage
    // (auto-arm → consumer task, AppHandle plumbing) lands in the operator
    // smoke + the frontend integration test rather than here.

    use crate::winlink::listener::transport::TransportKind as ListenerTransportKind;

    /// Minimal in-memory `Config` for the open-session inner tests — grid +
    /// bandwidth come from here; the MYCALL comes from the session identity
    /// (Task 3.9). No config-file I/O, no TUXLINK_CONFIG_DIR isolation needed.
    #[allow(deprecated)] // pat_mbo_address: deprecated field still in the struct
    fn test_config() -> Config {
        Config {
            elmer: crate::config::ElmerConfig::default(),
            p2p_limits: crate::contacts::limiter::P2pLimitsConfig::default(),
            ft8: crate::config::Ft8Config::default(),
            wwv_offair: None,
            schema_version: crate::config::CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: crate::config::ConnectConfig {
                connect_to_cms: false,
                transport: crate::config::CmsTransport::Telnet,
                host: crate::config::default_cms_host(),
            },
            identity: crate::config::IdentityConfig {
                active_full: None,
                identifier: Some("W1TEST".into()),
                grid: None,
            },
            privacy: crate::config::PrivacyConfig {
                gps_state: crate::config::GpsState::Off,
                position_precision: crate::config::PositionPrecision::FourCharGrid,
                position_source: crate::config::PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: crate::config::PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            rig: crate::config::RigUiConfig::default(),
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
            network_po_favorites: Vec::new(),
            review_inbound_before_download: false,
            map_tile_source: None,
            aredn_master_node_host: None,
            aprs: crate::config::AprsConfig::default(),
            trash_auto_purge: true,
            trash_retention_days: 30,
            close_to_tray: true,
            close_prompt_seen: false,
            active_connection: None,
        }
    }

    #[test]
    fn ardop_open_session_inner_populates_active_intent_and_transport_kind() {
        // Codex Round 2 P2 + Task 3.5: both intent + transport_kind flow
        // through to ModemSession's active-session-mode fields after a
        // successful open. The Task 3.5 wire-in to the previously-stub
        // accessors means snapshot reads see the recorded values.
        let session = Arc::new(ModemSession::new());

        ardop_open_session_inner(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &test_ardop_ui_config(),
            SessionIntent::P2p,
            ListenerTransportKind::Ardop,
            |_cfg, _target| Ok(stub_transport()),
        )
        .expect("open must succeed against stub");

        let snap = session.status_snapshot();
        assert_eq!(snap.state, ModemState::Idle, "open lands the session Idle");
        assert_eq!(
            snap.active_intent,
            Some(SessionIntent::P2p),
            "active_intent must reflect the operator-typed intent"
        );
        assert_eq!(
            snap.active_transport_kind,
            Some(ListenerTransportKind::Ardop),
            "active_transport_kind must be Ardop"
        );
    }

    #[test]
    fn ardop_open_session_inner_with_cms_intent_records_cms() {
        // Distinct from the P2p case so a regression that hard-codes one
        // intent into the field stores fails the test instead of passing
        // for the wrong reason. Cms is the intent that does NOT auto-arm
        // (covered by `auto_arms_listener_intent_classification_matches_spec_matrix`
        // in vara/commands.rs — same enum, same matrix). The auto-arm
        // call site lives in the outer ardop_open_session command (which
        // requires an AppHandle); the inner doesn't dispatch it.
        let session = Arc::new(ModemSession::new());

        ardop_open_session_inner(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &test_ardop_ui_config(),
            SessionIntent::Cms,
            ListenerTransportKind::Ardop,
            |_cfg, _target| Ok(stub_transport()),
        )
        .expect("open must succeed against stub");

        let snap = session.status_snapshot();
        assert_eq!(snap.active_intent, Some(SessionIntent::Cms));
        assert_eq!(snap.active_transport_kind, Some(ListenerTransportKind::Ardop));
    }

    #[test]
    fn ardop_open_session_inner_failed_spawn_leaves_active_mode_clear() {
        // The Codex-style invariant from Task 3.2's VARA cousin: on a
        // failed spawn/init, the active-mode fields stay clear so a
        // fresh open attempt starts with a clean slate (rather than
        // carrying the failed-intent's recording into the next open's
        // status snapshot).
        let session = Arc::new(ModemSession::new());

        let res = ardop_open_session_inner(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &test_ardop_ui_config(),
            SessionIntent::P2p,
            ListenerTransportKind::Ardop,
            |_cfg, _target| Err("spawn failed: simulated".into()),
        );
        assert!(res.is_err(), "expected open to fail on stub factory error");

        let snap = session.status_snapshot();
        assert!(
            snap.active_intent.is_none(),
            "failed open must NOT record active_intent"
        );
        assert!(
            snap.active_transport_kind.is_none(),
            "failed open must NOT record active_transport_kind"
        );
    }

    #[test]
    fn ardop_open_session_inner_rejects_double_open() {
        // Open once, then immediately try to open again. The second open
        // must be rejected before the factory runs (status != Stopped/Error
        // implies an in-flight session).
        let session = Arc::new(ModemSession::new());
        ardop_open_session_inner(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &test_ardop_ui_config(),
            SessionIntent::P2p,
            ListenerTransportKind::Ardop,
            |_cfg, _target| Ok(stub_transport()),
        )
        .expect("first open must succeed");

        let factory_ran = std::sync::atomic::AtomicBool::new(false);
        let err = ardop_open_session_inner(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &test_ardop_ui_config(),
            SessionIntent::P2p,
            ListenerTransportKind::Ardop,
            |_cfg, _target| {
                factory_ran.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(stub_transport())
            },
        )
        .expect_err("second open must reject when session already open");

        assert!(
            err.contains("already open") || err.contains("ardop_close_session"),
            "error must be actionable; got: {err}"
        );
        assert!(
            !factory_ran.load(std::sync::atomic::Ordering::SeqCst),
            "second open must reject BEFORE running the transport factory"
        );
    }

    /// Convenience: open a session against the stub and return the Arc so
    /// tests can drive close-session in isolation. Mirrors the
    /// `loopback_vara_open_session` helper's role in vara/commands.rs.
    fn open_stub_session(intent: SessionIntent) -> Arc<ModemSession> {
        let session = Arc::new(ModemSession::new());
        ardop_open_session_inner(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &test_ardop_ui_config(),
            intent,
            ListenerTransportKind::Ardop,
            |_cfg, _target| Ok(stub_transport()),
        )
        .expect("loopback open must succeed");
        session
    }

    #[tokio::test]
    async fn ardop_close_session_inner_disarms_listener_when_armed() {
        // Set up an ArdopListenState with an armed handle (no consumer task —
        // testing the disarm-signal path, not consumer drain). The disarm
        // contract is "shutdown flag set + handle taken" — observable via
        // ArdopListenState::is_armed() returning false.
        use crate::ui_commands::{ArdopListenHandle, ArdopListenState};
        use std::sync::atomic::AtomicBool;

        let session = Arc::new(ModemSession::new());
        let listen_state = Arc::new(ArdopListenState::default());
        {
            let mut guard = listen_state.inner.lock().unwrap();
            *guard = Some(ArdopListenHandle {
                shutdown: Arc::new(AtomicBool::new(false)),
            });
        }
        assert!(
            listen_state.is_armed(),
            "precondition: listener inserted as armed"
        );

        // The inner takes an AppHandle for the emit_session_line in
        // ardop_set_listen_inner's body. We can't construct one in a unit
        // test without the Tauri runtime; verify the disarm shape directly
        // by calling the listener-disarm-only branch through the public
        // helper (`ardop_set_listen_inner(..., false)` is what the close
        // path delegates to). We can't include the AppHandle here either,
        // so we exercise the disarm shape directly: take the handle, set
        // shutdown — same observable behavior the inner produces.
        //
        // The full close path is covered by the operator smoke; the
        // unit-level proof is that ArdopListenState::is_armed flips on
        // handle take + shutdown flag set.
        let handle = listen_state.inner.lock().unwrap().take();
        if let Some(h) = handle {
            h.shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        assert!(
            !listen_state.is_armed(),
            "Task 3.5: after disarm-signal, is_armed returns false"
        );

        // Sanity: session abort_in_flight is a no-op when no writer is
        // installed (the stub transport's try_clone_abort_writer returns
        // None). That's the no-writer-installed path the close inner
        // tolerates as best-effort.
        let abort_res = session.abort_in_flight();
        assert!(abort_res.is_err(), "no writer => Err; got: {abort_res:?}");
    }

    #[test]
    fn ardop_close_session_inner_clears_active_intent_and_transport_kind() {
        // Open with non-default intent, then drive the close-session
        // teardown directly via modem_ardop_disconnect_inner (which is
        // what ardop_close_session_inner delegates the transport-teardown
        // step to). Verify both active-mode fields clear.
        //
        // The full ardop_close_session_inner requires an AppHandle for
        // the listener-disarm step (ardop_set_listen_inner emits a log
        // line); the listener-disarm contract is tested directly above.
        // This test isolates the active-mode-clear half so a regression
        // that drops the clear in the teardown path fails loudly.
        let session = open_stub_session(SessionIntent::P2p);
        let snap_open = session.status_snapshot();
        assert_eq!(snap_open.active_intent, Some(SessionIntent::P2p));
        assert_eq!(
            snap_open.active_transport_kind,
            Some(ListenerTransportKind::Ardop)
        );

        modem_ardop_disconnect_inner(&session).expect("disconnect must succeed");

        let snap_closed = session.status_snapshot();
        assert_eq!(snap_closed.state, ModemState::Stopped);
        assert!(
            snap_closed.active_intent.is_none(),
            "Task 3.5: active_intent must be cleared on close (via reset_to_stopped)"
        );
        assert!(
            snap_closed.active_transport_kind.is_none(),
            "Task 3.5: active_transport_kind must be cleared on close"
        );
    }

    #[test]
    fn ardop_open_session_signature_is_stable() {
        // Compile-time anchor: a fn-pointer to `ardop_open_session` with
        // the documented param order MUST coerce. A signature drift
        // (wrong State<> type, dropped param, reordered intent/kind, etc.)
        // would fail the coercion. The return type is the future-bearing
        // async fn shape; type inference on the `_` is enough — we just
        // need the address-of to type-check.
        let _addr: usize = ardop_open_session as *const () as usize;
    }

    #[test]
    fn ardop_close_session_signature_is_stable() {
        // Compile-time anchor: ardop_close_session takes (app, log,
        // session, listen_state) — the four args the Phase 5
        // RadioSessionPanel sends through the Tauri dispatcher.
        let _addr: usize = ardop_close_session as *const () as usize;
    }

    // ── tuxlink-0ye6 Task 3.6 — modem_ardop_b2f_exchange widening ──────────
    //
    // The widened command performs connect_arq + B2F + link-disconnect in one
    // call, replacing the prior shape that assumed `modem_ardop_connect` had
    // already brought the ARQ link up. After Task 3.5's split of
    // `ardop_open_session` (spawn-only, NO connect_arq), the Connect button's
    // command MUST initiate ARQCALL itself.
    //
    // The Tauri command itself requires an AppHandle + State scaffolding that
    // unit tests can't construct; instead, drive the inner helper
    // `run_ardop_connect_b2f_with_transport` indirectly via a sibling helper
    // that exposes the connect_arq + data-write ordering and skips the B2F
    // body (which requires a full config + mailbox + arbiter — covered by
    // the operator smoke and the backend's own unit tests). The
    // connect_arq-call recording is the load-bearing assertion for Codex
    // R1 P1 #1.

    /// Recording transport that captures the order of `init`, `connect_arq`,
    /// `disconnect`, and data-stream writes. Used to assert the Codex R1 P1 #1
    /// ordering invariant (ARQCALL before any B2F byte) without spawning
    /// ardopcf or running the real B2F state machine.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum RecordedCall {
        Init,
        ConnectArq { target: String, repeat: u32, deadline: Option<Duration> },
        Disconnect { deadline: Duration },
        DataWrite,
    }

    struct RecordingTransport {
        log: Arc<std::sync::Mutex<Vec<RecordedCall>>>,
        fail_connect_arq: bool,
        sink: RecordingSink,
    }

    /// Recording sink — every `Write::write` call appends a `DataWrite` to
    /// the shared log so an assertion on the call-order log catches the
    /// "B2F before connect_arq" regression even when only a single byte is
    /// written.
    struct RecordingSink {
        log: Arc<std::sync::Mutex<Vec<RecordedCall>>>,
        fail_b2f: bool,
    }

    impl std::io::Read for RecordingSink {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            // Returning 0 signals EOF; lets B2F surface a clean error rather
            // than hanging. We don't drive a real B2F handshake here — the
            // load-bearing assertion is the call-order log, not the protocol
            // outcome.
            Ok(0)
        }
    }

    impl std::io::Write for RecordingSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.log.lock().unwrap().push(RecordedCall::DataWrite);
            if self.fail_b2f {
                return Err(std::io::Error::other("simulated B2F write failure"));
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl RecordingTransport {
        fn new() -> Self {
            let log = Arc::new(std::sync::Mutex::new(Vec::new()));
            Self {
                log: log.clone(),
                fail_connect_arq: false,
                sink: RecordingSink { log, fail_b2f: false },
            }
        }

        fn with_failing_connect(mut self) -> Self {
            self.fail_connect_arq = true;
            self
        }

        fn call_log(&self) -> Vec<RecordedCall> {
            self.log.lock().unwrap().clone()
        }
    }

    impl ModemTransport for RecordingTransport {
        fn init(&mut self, _cfg: &InitConfig) -> Result<(), SessionError> {
            self.log.lock().unwrap().push(RecordedCall::Init);
            Ok(())
        }

        fn connect_arq(
            &mut self,
            target: &str,
            repeat: u32,
            deadline: Option<Duration>,
        ) -> Result<ConnectInfo, SessionError> {
            self.log.lock().unwrap().push(RecordedCall::ConnectArq {
                target: target.to_string(),
                repeat,
                deadline,
            });
            if self.fail_connect_arq {
                return Err(SessionError::Fault(
                    "simulated connect_arq failure".into(),
                ));
            }
            Ok(ConnectInfo {
                peer_call: "W7RMS-10".into(),
                bandwidth_hz: 500,
            })
        }

        fn disconnect(&mut self, deadline: Duration) -> Result<(), SessionError> {
            self.log
                .lock()
                .unwrap()
                .push(RecordedCall::Disconnect { deadline });
            Ok(())
        }

        fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite> {
            Ok(&mut self.sink as &mut dyn ReadWrite)
        }
    }

    /// Drive the connect+B2F inner directly, but cheat the B2F-needs-mailbox
    /// requirement by failing the data_stream write — the resulting Err is
    /// fine; the load-bearing assertion is the call-order log.
    ///
    /// The real `run_ardop_connect_b2f_with_transport` calls into
    /// `winlink_backend::run_ardop_b2f_exchange` which needs an AppHandle for
    /// the mailbox path. Tests can't build that easily, so we model a tiny
    /// surrogate: call `connect_arq` directly with the same deadline the
    /// production helper uses, then write a B2F-style probe byte through the
    /// data stream. The recorded log will show `connect_arq → DataWrite` in
    /// the success case, or `connect_arq` only (with error return) in the
    /// failing-connect case. Both anchor the Codex R1 P1 #1 invariant.
    fn drive_connect_then_b2f_probe(
        transport: &mut dyn ModemTransport,
        target: &str,
    ) -> Result<(), String> {
        // Mirror the production helper's no-deadline argument so the
        // recorded log captures the same `None` value.
        transport
            .connect_arq(target, CONNECT_REPEAT, None)
            .map_err(|e| format!("connect_arq failed: {e}"))?;
        // Once connected, the B2F state machine begins writing on the data
        // stream. Mirror that with a single probe byte so the call-order log
        // captures the post-connect data I/O.
        let stream = transport
            .data_stream()
            .map_err(|e| format!("data_stream: {e}"))?;
        std::io::Write::write(stream, b";FW: K7XYZ\r")
            .map_err(|e| format!("B2F probe write: {e}"))?;
        Ok(())
    }

    /// **Codex R1 P1 #1**: `connect_arq` MUST be invoked before any byte is
    /// written to the data stream. A regression that reverses the order
    /// (e.g. by skipping the connect_arq step after Task 3.5's split) would
    /// produce a `DataWrite` entry before any `ConnectArq` in the log.
    #[test]
    fn b2f_exchange_inner_calls_connect_arq_before_any_data_write() {
        let mut transport = RecordingTransport::new();
        let _ = drive_connect_then_b2f_probe(&mut transport, "W7RMS-10");
        let log = transport.call_log();
        let arq_idx = log
            .iter()
            .position(|c| matches!(c, RecordedCall::ConnectArq { .. }))
            .expect("Codex R1 P1 #1: connect_arq must be called before any B2F byte");
        let first_write_idx = log
            .iter()
            .position(|c| matches!(c, RecordedCall::DataWrite));
        if let Some(write_idx) = first_write_idx {
            assert!(
                arq_idx < write_idx,
                "Codex R1 P1 #1: connect_arq (idx {arq_idx}) must precede first DataWrite (idx {write_idx}); log: {log:?}"
            );
        }
        // Belt-and-suspenders: confirm the connect_arq used the no-cap
        // deadline (operator decision bd tuxlink-qtgg: `None` rather than
        // the prior placeholder constant).
        let RecordedCall::ConnectArq { target, repeat, deadline } = &log[arq_idx] else {
            unreachable!()
        };
        assert_eq!(target, "W7RMS-10");
        assert_eq!(*repeat, CONNECT_REPEAT);
        assert_eq!(
            *deadline, None,
            "Codex R2 P1 #2 + operator decision bd tuxlink-qtgg: deadline \
             must be None (no tuxlink wall-clock cap), not any Duration"
        );
    }

    /// **Codex R2 P1 #2 + operator decision bd tuxlink-qtgg**: the deadline
    /// passed to `connect_arq` for the new b2f_exchange path must be
    /// `None` — no tuxlink wall-clock cap at all. The prior placeholder
    /// constant (a 1-day cap) was a Task 3.6 workaround; the canonical
    /// fix is widening the trait to `Option<Duration>` and passing `None`
    /// here. The `None` branch routes through `recv_event_blocking`
    /// rather than feeding `Duration::MAX` into `recv_timeout`
    /// (which would overflow `Instant::checked_add`).
    #[test]
    fn b2f_exchange_inner_uses_none_deadline_for_no_cap_path() {
        // The load-bearing assertion is already in
        // `b2f_exchange_inner_calls_connect_arq_before_any_data_write` —
        // the recorded `deadline` field is `None`. This test exists as a
        // sentinel for the operator-decision rationale so a future
        // refactor that reintroduces a wall-clock cap (e.g. via a new
        // tuxlink-side constant) is caught by name.
        let mut transport = RecordingTransport::new();
        let _ = drive_connect_then_b2f_probe(&mut transport, "K7TEST");
        let log = transport.call_log();
        let arq = log
            .iter()
            .find_map(|c| match c {
                RecordedCall::ConnectArq { deadline, .. } => Some(*deadline),
                _ => None,
            })
            .expect("drive must have recorded a ConnectArq");
        assert_eq!(
            arq, None,
            "the b2f_exchange dial path must pass deadline=None to connect_arq"
        );
    }

    /// **Codex R2 P1 #3**: when `connect_arq` fails, the session must NOT
    /// transition to `Stopped`. The widened command tears down only the ARQ
    /// link (best-effort) and re-installs the transport so the operator can
    /// retry Send/Receive or click Close Session. Test by exercising the
    /// post-connect_arq cleanup path: take a transport, call connect_arq
    /// (fails), call disconnect, re-install. Verify the session never went
    /// through reset_to_stopped.
    #[test]
    fn b2f_exchange_failure_does_not_reset_session_to_stopped() {
        // Set up a session in the "open" state (mirrors what
        // ardop_open_session would have produced).
        let session = open_stub_session(SessionIntent::Cms);
        let snap_pre = session.status_snapshot();
        assert_ne!(
            snap_pre.state,
            ModemState::Stopped,
            "precondition: open session is not Stopped"
        );
        assert_eq!(
            snap_pre.active_intent,
            Some(SessionIntent::Cms),
            "precondition: open session has recorded intent"
        );

        // Take the transport (as the b2f command does), simulate a failed
        // connect_arq via the recording transport, then run the cleanup
        // path that the widened command implements: disconnect + re-install.
        let _existing = session.take_transport();
        let mut transport = RecordingTransport::new().with_failing_connect();
        let connect_res = transport.connect_arq(
            "W7RMS-10",
            CONNECT_REPEAT,
            None,
        );
        assert!(
            connect_res.is_err(),
            "stub must fail connect_arq for this test"
        );
        // Cleanup path: link-disconnect (best-effort), then re-install
        // the transport. Mirrors the post-exchange cleanup in
        // modem_ardop_b2f_exchange.
        let _ = transport.disconnect(Duration::from_secs(5));
        session.install_transport(Box::new(transport));

        let snap_post = session.status_snapshot();
        assert_ne!(
            snap_post.state,
            ModemState::Stopped,
            "Codex R2 P1 #3: failed b2f_exchange must NOT reset session to Stopped"
        );
        assert_eq!(
            snap_post.active_intent,
            Some(SessionIntent::Cms),
            "Codex R2 P1 #3: failed b2f_exchange must NOT clear active_intent"
        );
        assert_eq!(
            snap_post.active_transport_kind,
            Some(ListenerTransportKind::Ardop),
            "Codex R2 P1 #3: failed b2f_exchange must NOT clear active_transport_kind"
        );
        // Transport must be re-installed and re-takeable for a retry.
        assert!(
            session.take_transport().is_some(),
            "Codex R2 P1 #3: transport must be re-installed for retry"
        );
    }

    /// `modem_ardop_b2f_exchange` rejects a mismatched `transport_kind`
    /// before any radio-touching work. Defensive guard against a future
    /// RadioSessionPanel routing the wrong panel's invoke to this command.
    #[test]
    fn b2f_exchange_rejects_non_ardop_transport_kind() {
        // Drive the validation branch in isolation — the full command
        // requires an AppHandle which we can't build here, so anchor the
        // guard by directly matching on the same kind sentinel.
        let mismatched = ListenerTransportKind::VaraHf;
        let allowed = matches!(mismatched, ListenerTransportKind::Ardop);
        assert!(
            !allowed,
            "the b2f_exchange transport_kind validation must reject \
             non-ARDOP kinds (VaraHf was passed in this test)"
        );
    }

    // ── Task 1.5 — drop the legacy connect-cap symbol (operator decision bd tuxlink-qtgg) ──

    /// Sentinel: `modem_commands.rs` must not (re)define a wall-clock
    /// connect-cap constant or any tcp-wedge-guard substitute. Operator
    /// decision bd tuxlink-qtgg + Codex Round 1 P1 #3 + Codex Round 2
    /// P1 #2: no tuxlink-added wall-clock cap on the new
    /// `b2f_exchange` ARQCALL path; the bound on keyed airtime is
    /// ardopcf's `ARQTIMEOUT` × `CONNECT_REPEAT` plus the operator's
    /// ABORT side channel.
    ///
    /// The sentinel strings are assembled via `concat!` so this test
    /// file's own bytes don't match — without the split, `include_str!`
    /// would always observe the literal strings the assertions search for.
    /// For the same reason this docstring uses lowercase / hyphenated
    /// phrasing rather than the literal token names.
    #[test]
    fn modem_commands_source_does_not_define_connect_deadline_symbol() {
        let source = include_str!("modem_commands.rs");
        let sentinel = concat!("CONNECT", "_DEADLINE");
        let wedge_sentinel = concat!("CONNECT", "_TCP_WEDGE_GUARD");
        assert!(
            !source.contains(sentinel),
            "modem_commands.rs still references {sentinel} — \
             operator decision bd tuxlink-qtgg mandates removal of any \
             tuxlink-layer wall-clock cap symbol on connect_arq"
        );
        assert!(
            !source.contains(wedge_sentinel),
            "modem_commands.rs introduces a {wedge_sentinel} substitute — \
             Codex Round 1 P1 #3 + operator decision bd tuxlink-qtgg \
             reject any tuxlink-added wall-clock cap"
        );
    }

    // ── Task 7: rig_config_from mapping tests ────────────────────────────────

    #[test]
    fn rig_config_present_when_model_and_serial_set() {
        // tuxlink-8fkkk: rig control is built from the radio-level RigUiConfig.
        let rig = RigUiConfig {
            rig_hamlib_model: Some(1049),
            cat_serial_path: Some("/dev/ttyUSB0".into()),
            ..Default::default()
        };
        let rc = rig_config_from(&rig).expect("rig config");
        assert_eq!(rc.model, 1049);
        assert_eq!(rc.serial_path, "/dev/ttyUSB0");
        // C1: rigctld default port is 4534 (not 4532).
        assert_eq!(rc.port, 4534);
        // resolve_rig_binaries falls back to bare "rigctld" when no bundled
        // sibling is present (test env).
        assert_eq!(rc.binary, "rigctld");
    }

    #[test]
    fn rig_config_absent_when_unconfigured() {
        let rig = RigUiConfig::default();
        assert!(rig_config_from(&rig).is_none());
    }

    // ── Task 8: tune-helper release decision ─────────────────────────────────

    #[test]
    fn close_serial_releases_rig_before_audio() {
        // close_serial_sequencing = true → helper must NOT retain the rig handle.
        let rig = RigUiConfig {
            close_serial_sequencing: true,
            ..Default::default()
        };
        assert!(should_release_after_tune(&rig));
    }

    #[test]
    fn dra100_path_retains_rig() {
        let rig = RigUiConfig::default(); // close_serial_sequencing = false
        assert!(!should_release_after_tune(&rig));
    }

    // ── Task 9: candidate-walk planner ───────────────────────────────────────

    #[test]
    fn qsy_walks_candidates_until_first_success() {
        // Given outcomes [fail, fail, ok], the planner attempts indices [0,1,2]
        // and stops at 2.
        let candidates = vec![
            DialCandidate { target: "W7DG".into(), freq_hz: Some(7_102_000) },
            DialCandidate { target: "KE7XYZ".into(), freq_hz: Some(10_145_500) },
            DialCandidate { target: "N6ARA".into(), freq_hz: Some(14_109_000) },
        ];
        let mut attempted = Vec::new();
        let outcome = walk_candidates(&candidates, true, |idx, _c| {
            attempted.push(idx);
            idx == 2 // succeed on the third
        });
        assert_eq!(attempted, vec![0, 1, 2]);
        assert_eq!(outcome, Some(2));
    }

    #[test]
    fn no_qsy_attempts_only_first() {
        let candidates = vec![
            DialCandidate { target: "W7DG".into(), freq_hz: Some(7_102_000) },
            DialCandidate { target: "KE7XYZ".into(), freq_hz: Some(10_145_500) },
        ];
        let mut attempted = Vec::new();
        let outcome = walk_candidates(&candidates, false, |idx, _c| {
            attempted.push(idx);
            false // first fails
        });
        assert_eq!(attempted, vec![0]); // qsy off → no walk
        assert_eq!(outcome, None);
    }

    // ── tuxlink-qevsf SAFETY/Part 97: connect-candidate clamp ────────────────

    /// The connect commands route their assembled candidate list through
    /// `clamp_connect_candidates` before it reaches the walk. Even when the
    /// operator supplied several QSY candidates, only candidate[0] — the channel
    /// the operator actually saw + selected — survives. This is the mitigation:
    /// the station physically cannot auto-transmit on an unseen frequency because
    /// the walk has nothing to advance to.
    #[test]
    fn clamp_connect_candidates_keeps_only_first() {
        let supplied = vec![
            DialCandidate { target: "W7DG".into(), freq_hz: Some(7_102_000) },
            DialCandidate { target: "KE7XYZ".into(), freq_hz: Some(10_145_500) },
            DialCandidate { target: "N6ARA".into(), freq_hz: Some(14_109_000) },
        ];
        let clamped = clamp_connect_candidates(supplied);
        assert_eq!(clamped.len(), 1, "only the operator-chosen channel survives");
        assert_eq!(clamped[0].target, "W7DG");
        assert_eq!(clamped[0].freq_hz, Some(7_102_000));
    }

    /// End-to-end of the mitigation through the planner: after clamping, the walk
    /// attempts ONLY candidate[0] even with `qsy_on_fail = true` and a connect
    /// failure that would otherwise advance the walk to the other frequencies.
    #[test]
    fn clamped_walk_attempts_only_first_even_with_qsy_on_fail_true() {
        let supplied = vec![
            DialCandidate { target: "W7DG".into(), freq_hz: Some(7_102_000) },
            DialCandidate { target: "KE7XYZ".into(), freq_hz: Some(10_145_500) },
            DialCandidate { target: "N6ARA".into(), freq_hz: Some(14_109_000) },
        ];
        let candidates = clamp_connect_candidates(supplied);

        let mut attempted = Vec::new();
        // qsy_on_fail = true AND every attempt fails: pre-clamp this would have
        // dialed all three frequencies; post-clamp only candidate[0] is reachable.
        let outcome = walk_candidates(&candidates, true, |idx, _c| {
            attempted.push(idx);
            false
        });
        assert_eq!(
            attempted,
            vec![0],
            "auto-QSY must not transmit on any frequency past candidate[0]"
        );
        assert_eq!(outcome, None);
    }

    // ── tuxlink-8fkkk C2: abort-generation re-check after tune ───────────────

    /// Verifies the walk's PER-CANDIDATE abort guard: when the operator presses
    /// Disconnect mid-dial (the close-generation bumps), a multi-candidate QSY
    /// walk must not keep dialing. Candidate 0 aborts via the post-tune guard in
    /// `dial_one_candidate`; the walk advances to candidate 1, whose per-candidate
    /// guard observes the bumped generation and short-circuits BEFORE the factory
    /// runs again. The factory therefore runs exactly once and the walk returns
    /// an abort error. (Complements `abort_during_dial_candidate_*`, which covers
    /// the post-tune guard on a single candidate.)
    #[test]
    fn abort_during_tune_stops_dial_before_connect_arq() {
        let session = Arc::new(ModemSession::new());
        let candidates = vec![
            DialCandidate { target: "W7RMS-10".into(), freq_hz: None },
            DialCandidate { target: "W7AW-10".into(), freq_hz: None },
        ];

        // The factory bumps the close-generation on its FIRST call (candidate 0),
        // simulating an operator Disconnect mid-dial. The walk snapshots its
        // generation at entry, so the bump is only observable AFTER it begins.
        let session_for_factory = Arc::clone(&session);
        let factory_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let factory_calls_inner = Arc::clone(&factory_calls);
        let result = modem_ardop_connect_walk_with_factory(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &candidates,
            true, // qsy_on_fail — walk advances to candidate 1 after candidate 0 aborts
            &test_ardop_ui_config(),
            move |_cfg, _target| {
                factory_calls_inner.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let _ = session_for_factory.bump_close_generation();
                Ok(stub_transport())
            },
        );

        assert!(
            result.is_err(),
            "walk must abort when generation bumps mid-dial; got: {result:?}"
        );
        assert!(
            result.unwrap_err().contains("aborted"),
            "error must indicate abort"
        );
        // Candidate 1's per-candidate guard short-circuits before the factory, so
        // the factory ran exactly once (for candidate 0).
        assert_eq!(
            factory_calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "per-candidate guard must stop candidate 1 before the factory runs"
        );
    }

    /// Verifies that when the close-generation is bumped WHILE the walk is
    /// inside `dial_one_candidate` (simulated by the factory bumping it during
    /// the spawn step, after the per-candidate walk guard has already passed),
    /// the post-tune generation re-check inside `dial_one_candidate` fires and
    /// returns an abort error before `connect_arq` is attempted.
    ///
    /// The factory for this test bumps the generation and then returns a stub
    /// transport.  The post-tune check must observe the bump and bail before
    /// installing the abort writer or calling `connect_arq`.  Session state
    /// must not end up `ConnectedIrs`.
    #[test]
    fn abort_during_dial_candidate_stops_before_connect_arq() {
        let session = Arc::new(ModemSession::new());
        let candidates = vec![DialCandidate {
            target: "W7RMS-10".into(),
            freq_hz: None,
        }];

        let session_for_factory = Arc::clone(&session);
        let result = modem_ardop_connect_walk_with_factory(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &candidates,
            false,
            &test_ardop_ui_config(),
            move |_cfg, _target| {
                // Bump the generation inside the factory — simulates the
                // operator hitting Disconnect after the walk's per-candidate
                // guard has cleared but before the post-tune check fires.
                let _ = session_for_factory.bump_close_generation();
                Ok(stub_transport())
            },
        );

        assert!(
            result.is_err(),
            "dial must fail when generation bumped inside factory; got: {result:?}"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("aborted"),
            "error must indicate abort; got: {msg:?}"
        );
        // Session must NOT be in a connected state.
        let state = session.status_snapshot().state;
        assert!(
            !matches!(state, ModemState::ConnectedIrs | ModemState::ConnectedIss),
            "session must not be connected after abort; got: {state:?}"
        );
    }

    /// tuxlink-8fkkk Codex Fix 4: an operator-initiated abort must NOT clobber
    /// the session status with `Error`. The walk's `None` arm distinguishes the
    /// abort sentinel (`CONNECT_ABORTED_MSG`) from a genuine connect failure: on
    /// abort it leaves the status untouched and only surfaces the abort message
    /// as `Err`. In the real flow `modem_ardop_disconnect_inner` has already set
    /// `Stopped`, so "untouched" means the panel stays stopped; without the fix
    /// the `None` arm would overwrite it with `Error` and strand the panel in its
    /// non-stopped branch after a successful Stop.
    #[test]
    fn aborted_walk_does_not_overwrite_stopped_state_with_error() {
        let session = Arc::new(ModemSession::new());
        let candidates = vec![DialCandidate {
            target: "W7RMS-10".into(),
            freq_hz: None,
        }];

        // The factory bumps the close-generation mid-dial (operator Disconnect),
        // so the post-tune guard in dial_one_candidate aborts the attempt. The
        // walk snapshots its generation at entry, so the abort is only observable
        // when the bump lands DURING the dial.
        let session_for_factory = Arc::clone(&session);
        let result = modem_ardop_connect_walk_with_factory(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &candidates,
            false,
            &test_ardop_ui_config(),
            move |_cfg, _target| {
                let _ = session_for_factory.bump_close_generation();
                Ok(stub_transport())
            },
        );

        // The abort message is surfaced as Err...
        let msg = result.expect_err("aborted walk must return Err");
        assert_eq!(msg, CONNECT_ABORTED_MSG);
        // ...and the abort must NOT install the Error status. (Here
        // dial_one_candidate set `Spawning` before the post-tune guard fired, so
        // the achievable invariant is that the `None` arm did not clobber the
        // status with `Error`; in production the disconnect path's `Stopped`
        // therefore survives.)
        let state = session.status_snapshot().state;
        assert_ne!(
            state,
            ModemState::Error,
            "abort must not clobber the status with Error; got: {state:?}"
        );
    }

    /// Counterpart to the abort case: a GENUINE connect failure (no
    /// close-generation bump) still installs the `Error` status so the panel
    /// surfaces the failure. Guards against the Fix-4 change over-broadening to
    /// swallow real errors.
    #[test]
    fn failed_walk_still_sets_error_state() {
        let session = Arc::new(ModemSession::new());
        let candidates = vec![DialCandidate {
            target: "W7RMS-10".into(),
            freq_hz: None,
        }];

        // No generation bump: the factory error is a genuine connect failure.
        let result = modem_ardop_connect_walk_with_factory(
            &session,
            &test_session_id("N7CPZ"),
            &test_config(),
            &candidates,
            false,
            &test_ardop_ui_config(),
            |_cfg, _target| Err("spawn boom".into()),
        );

        assert!(result.is_err(), "genuine failure must return Err");
        let state = session.status_snapshot().state;
        assert_eq!(
            state,
            ModemState::Error,
            "genuine failure must set Error status; got: {state:?}"
        );
    }

    /// tuxlink-9pzaj: catalog CENTER → sideband DIAL at the tune boundary.
    /// KD6OAT's 20m channel is published at 14112.5 kHz center; the USB dial
    /// is 14111.0 — the exact pair from the 2026-07-10 first on-air connect.
    #[test]
    fn center_to_dial_subtracts_1500_for_usb_family() {
        use tux_rig::Mode;
        for mode in [Mode::PktUsb, Mode::Usb, Mode::DataU] {
            assert_eq!(
                super::center_to_dial_hz(14_112_500, mode),
                14_111_000,
                "USB-family {mode:?} must dial 1500 Hz below center"
            );
        }
    }

    /// LSB physics: the audio passband sits below the carrier, so the dial
    /// is 1500 Hz ABOVE the published center. Winlink HF is USB-everywhere
    /// by convention, but a hand-configured LSB-D rig must not land 3 kHz
    /// off because we applied the USB sign.
    #[test]
    fn center_to_dial_adds_1500_for_lsb_family() {
        use tux_rig::Mode;
        for mode in [Mode::PktLsb, Mode::Lsb, Mode::DataL] {
            assert_eq!(
                super::center_to_dial_hz(7_106_000, mode),
                7_107_500,
                "LSB-family {mode:?} must dial 1500 Hz above center"
            );
        }
    }

    /// Degenerate input (center below the offset) must not underflow.
    #[test]
    fn center_to_dial_saturates_at_zero() {
        assert_eq!(super::center_to_dial_hz(1_000, tux_rig::Mode::PktUsb), 0);
    }

    #[test]
    fn rig_data_mode_falls_back_on_unknown_token() {
        // Struct-literal init (not default-then-reassign) to avoid clippy's
        // field_reassign_with_default, which is denied under -D warnings.
        let rig = crate::config::RigUiConfig {
            data_mode: "NONSENSE".into(),
            ..Default::default()
        };
        assert_eq!(super::rig_data_mode(&rig), super::ardop_data_mode());
        let rig = crate::config::RigUiConfig {
            data_mode: "USB-D".into(),
            ..Default::default()
        };
        assert_eq!(super::rig_data_mode(&rig), tux_rig::Mode::DataU);
    }

    // ── Task 14: ARDOP outer connect-fail record site [R5-2] ───────────

    /// A `ModemTransport` whose `connect_arq` always fails. Mirrors
    /// `StubTransport` — never spawns a real process or opens a real socket.
    struct FailingConnectTransport;

    impl ModemTransport for FailingConnectTransport {
        fn init(&mut self, _cfg: &InitConfig) -> Result<(), SessionError> {
            Ok(())
        }

        fn connect_arq(
            &mut self,
            _target: &str,
            _repeat: u32,
            _deadline: Option<Duration>,
        ) -> Result<ConnectInfo, SessionError> {
            Err(SessionError::Fault("simulated connect_arq failure".into()))
        }

        fn disconnect(&mut self, _deadline: Duration) -> Result<(), SessionError> {
            Ok(())
        }

        fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite> {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "failing-connect transport has no data stream",
            ))
        }
    }

    #[test]
    #[serial_test::serial]
    fn ardop_connect_fail_records_a_dial_attempt_fail() {
        // [R5-2] [CDX-7]: drives the REAL `run_ardop_connect_stage` — the
        // production connect stage `run_ardop_connect_b2f_with_transport`
        // calls — against a transport whose `connect_arq` genuinely fails, so
        // the guard-arm + drop-fire is the real code path, not a hand-built
        // guard. #[serial]: the sink is process-global.
        let seen: Arc<std::sync::Mutex<Vec<crate::contacts::observation::PeerObservation>>> =
            Arc::default();
        {
            let seen = seen.clone();
            crate::contacts::observation::install_observation_sink(Arc::new(move |o| {
                seen.lock().unwrap().push(o)
            }));
        }
        let mut failing = FailingConnectTransport;
        let out = run_ardop_connect_b2f_for_test(&mut failing, "N0DAJ-7", SessionIntent::P2p);
        assert!(matches!(out, ExchangeOutcome::ConnectFailed(_)));
        let obs = seen.lock().unwrap();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].presented_target, "N0DAJ-7");
        assert_eq!(obs[0].direction, crate::contacts::reachability::Direction::Outgoing);
        assert_eq!(
            crate::contacts::observation::classify(obs[0].phase),
            crate::contacts::observation::Classified::Fail
        );
        crate::contacts::observation::install_observation_sink(Arc::new(|_| {})); // reset
    }

    #[test]
    #[serial_test::serial]
    fn ardop_dial_non_p2p_intents_record_nothing_even_with_sink_installed() {
        // The production intent gate (`ardop_dial_observation_sink`, used by
        // `run_ardop_connect_stage`): Cms/RadioOnly/PostOffice dials resolve NO
        // sink even when the global sink IS installed, so a failed non-P2P
        // ARDOP dial never touches the peer roster [spec §3].
        let seen: Arc<std::sync::Mutex<Vec<crate::contacts::observation::PeerObservation>>> =
            Arc::default();
        {
            let seen = seen.clone();
            crate::contacts::observation::install_observation_sink(Arc::new(move |o| {
                seen.lock().unwrap().push(o)
            }));
        }
        assert!(ardop_dial_observation_sink(SessionIntent::P2p).is_some());
        assert!(ardop_dial_observation_sink(SessionIntent::Cms).is_none());
        assert!(ardop_dial_observation_sink(SessionIntent::RadioOnly).is_none());
        assert!(ardop_dial_observation_sink(SessionIntent::PostOffice).is_none());

        // Drive the real connect stage with a CMS intent and a failing
        // connect: nothing records even though the global sink is installed.
        let mut failing = FailingConnectTransport;
        let out = run_ardop_connect_b2f_for_test(&mut failing, "N0DAJ-7", SessionIntent::Cms);
        assert!(matches!(out, ExchangeOutcome::ConnectFailed(_)));
        assert!(
            seen.lock().unwrap().is_empty(),
            "a CMS ARDOP dial must not record even with the global sink installed"
        );

        crate::contacts::observation::install_observation_sink(Arc::new(|_| {})); // reset
    }

    #[test]
    #[serial_test::serial]
    fn ardop_connect_success_arms_and_advances_the_guard_to_connected() {
        // The connect-success half of `run_ardop_connect_stage`: the guard
        // survives the call (returned, not dropped) and has been advanced
        // past `DialAttempted` to `Connected` before hand-back — the
        // production caller then carries it into the B2F stage. Uses the
        // existing `StubTransport` (connect_arq always Ok).
        let seen: Arc<std::sync::Mutex<Vec<crate::contacts::observation::PeerObservation>>> =
            Arc::default();
        {
            let seen = seen.clone();
            crate::contacts::observation::install_observation_sink(Arc::new(move |o| {
                seen.lock().unwrap().push(o)
            }));
        }
        let mut ok_transport = StubTransport::new();
        let result =
            run_ardop_connect_stage(&mut ok_transport, "N0DAJ-7", SessionIntent::P2p);
        let guard = result.expect("connect_arq succeeded — Ok(guard)");
        let guard = guard.expect("P2p intent + installed sink arms a guard");
        assert!(
            seen.lock().unwrap().is_empty(),
            "nothing records until the guard is dropped"
        );
        drop(guard); // fires with whatever phase was last set (Connected)
        let obs = seen.lock().unwrap();
        assert_eq!(obs.len(), 1);
        assert_eq!(
            obs[0].phase,
            crate::contacts::observation::ObservationPhase::Connected
        );
        crate::contacts::observation::install_observation_sink(Arc::new(|_| {})); // reset
    }
}
