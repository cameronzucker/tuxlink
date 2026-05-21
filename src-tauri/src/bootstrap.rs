//! App-start Pat bootstrap: the decision logic + the `.setup()` worker.
//!
//! Spec: docs/superpowers/specs/2026-05-20-pat-spawn-bootstrap-design.md
//!       §3.3 (bootstrap), §3.5 (sidecar resolution), §3.6 (Pat paths),
//!       §3.7 (drain task).
//! bd issue: tuxlink-22l (Task D).
//!
//! Two layers:
//!
//! 1. [`bootstrap_decision`] — a PURE classification of `read_config()`'s result
//!    into a [`BootstrapAction`]. No I/O, no Tauri; unit-tested directly. This
//!    is the spec §3.3 / adrev #14,#15 gate: pre-wizard + offline both stay
//!    "not connected"; a malformed config is an explicit config error (NOT
//!    "not connected"); only `wizard_completed && connect_to_cms` spawns Pat.
//!
//! 2. [`run`] — the `.setup()` worker that executes the action: spawns a
//!    dedicated `std::thread` (owns the up-to-10s BLOCKING `PatBackend::spawn`)
//!    which drives the [`BackendState`] phase, resolves the sidecar, spawns
//!    Pat, installs the backend, and starts the async session-log drain. ALL
//!    paths are non-fatal — the app always launches.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::app_backend::{BackendPhase, BackendState};
use crate::config::{Config, ConfigReadError};
use crate::session_log::SessionLogState;
use crate::winlink_backend::{
    LogLevel, LogLine, LogSource, NativeBackend, PatBackend, PatBackendSpawnOptions,
};

/// What the bootstrap should do, decided purely from `read_config()`'s result.
#[derive(Debug)]
pub enum BootstrapAction {
    /// Leave the backend `NotConfigured` (the "not connected" empty state):
    /// pre-wizard (no config / `NotFound`), wizard still rendering
    /// (`!wizard_completed`), or offline mode (`!connect_to_cms`). No Pat.
    NotConnected,
    /// A config file exists but is unusable (`Serde`/`Validation`/`Io`). Surface
    /// an explicit config error — do NOT masquerade as "not connected" (adrev
    /// #15). Carries the reason for the ribbon + the synthetic session-log line.
    ConfigError(String),
    /// CMS configured (`wizard_completed && connect_to_cms`): spawn Pat. The
    /// `Config` is boxed because it is the largest variant and is moved into the
    /// spawn path (avoids a large enum + a needless clone).
    Spawn(Box<Config>),
}

/// Classify `read_config()`'s result into a [`BootstrapAction`] (spec §3.3,
/// adrev #14,#15). Pure: no I/O, no side effects — the unit-test seam for the
/// bootstrap's branch selection.
///
/// - `Err(NotFound)` (pre-wizard, no config) → [`BootstrapAction::NotConnected`].
/// - `Err(Serde | Validation | Io)` (config exists but unusable) →
///   [`BootstrapAction::ConfigError`] carrying the error's `Display`.
/// - `Ok(cfg)` with `!wizard_completed` (wizard still rendering, adrev #14) →
///   [`BootstrapAction::NotConnected`].
/// - `Ok(cfg)` with `wizard_completed && !connect_to_cms` (offline mode, no
///   Pat) → [`BootstrapAction::NotConnected`].
/// - `Ok(cfg)` with `wizard_completed && connect_to_cms` (CMS mode) →
///   [`BootstrapAction::Spawn`].
pub fn bootstrap_decision(cfg: Result<Config, ConfigReadError>) -> BootstrapAction {
    match cfg {
        // Pre-wizard: no config file yet. Not connected; the wizard renders.
        Err(ConfigReadError::NotFound { .. }) => BootstrapAction::NotConnected,
        // A config exists but is unusable. Explicit error, not "not connected"
        // (adrev #15). `Display` carries the path / serde / validation detail.
        Err(e @ (ConfigReadError::Serde { .. }
        | ConfigReadError::Validation { .. }
        | ConfigReadError::Io { .. })) => BootstrapAction::ConfigError(e.to_string()),
        Ok(cfg) => {
            if !cfg.wizard_completed {
                // The wizard is still rendering (adrev #14): never spawn Pat
                // mid-wizard. Not connected until the wizard writes a completed
                // config.
                BootstrapAction::NotConnected
            } else if !cfg.connect.connect_to_cms {
                // Offline mode: no CMS, no Pat. Genuinely "not connected".
                BootstrapAction::NotConnected
            } else {
                // CMS mode: spawn Pat.
                BootstrapAction::Spawn(Box::new(cfg))
            }
        }
    }
}

// ============================================================================
// Pat sidecar path resolution (spec §3.5, adrev #12)
// ============================================================================

/// The error surfaced when the bundled Pat sidecar is a 0-byte dev stub or is
/// missing (adrev #12). `build.rs` writes a 0-byte `sidecars/pat-<triple>` stub
/// in debug builds (release-only path bundles the real binary), so a dev `tauri
/// dev` run would otherwise try to exec an empty file. The message tells the
/// operator how to point at a real Pat for a dev run.
const SIDECAR_STUB_REASON: &str = "Pat binary unavailable: the bundled sidecar is a 0-byte dev stub (release builds bundle the real binary). Set PAT_BINARY=/path/to/pat for a dev run.";

/// Resolve the Pat binary to exec (spec §3.5, adrev #12).
///
/// Resolution order:
/// 1. `PAT_BINARY` env override — if set AND points at an existing non-empty
///    file, use it verbatim (the documented dev/test escape hatch). If set but
///    the file is missing/empty, fall through to (2) (the override is a hint,
///    not a hard assertion — an empty override file is treated like no usable
///    binary and yields the stub error below).
/// 2. The bundled sidecar, resolved RELATIVE TO THE CURRENT EXECUTABLE — the
///    exact algorithm `tauri-plugin-shell`'s `app.shell().sidecar("pat")` uses
///    internally (its private `relative_command_path`): take
///    `tauri::utils::platform::current_exe()` (the same primitive the plugin
///    calls), go to its parent, step up out of a `deps/` dir for test/dev
///    layouts, and join the sidecar base name `"pat"`. At bundle time Tauri
///    renames `sidecars/pat-<target-triple>` → `pat` next to the executable, so
///    the per-target suffix is resolved by the bundler, not at runtime.
/// 3. Detect a missing OR zero-byte resolved file → `Err(SIDECAR_STUB_REASON)`.
///
/// `PatBackend::spawn` consumes the returned `PathBuf` via `std::process::Command`
/// (not the shell plugin's `Command`), which is why we resolve a raw `PathBuf`
/// here rather than handing back a `tauri_plugin_shell::process::Command` (whose
/// resolved program path is not publicly extractable).
pub fn resolve_pat_binary(app: &AppHandle) -> Result<PathBuf, String> {
    // AppHandle is kept in the signature for API symmetry + future resource-dir
    // resolution; the bundled-sidecar resolution uses `current_exe`, which needs
    // no handle. The decision itself is the pure `resolve_pat_binary_inner`, so
    // the override + zero-byte-stub branches are unit-testable without a Tauri
    // app (adrev #12 test seam).
    let _ = app;
    resolve_pat_binary_inner(std::env::var_os("PAT_BINARY"), resolve_sidecar_path("pat"))
}

/// Pure core of [`resolve_pat_binary`] — no env reads, no `AppHandle`, no
/// `current_exe`; the inputs are injected so the override + zero-byte-stub
/// branches (adrev #12) are unit-testable.
///
/// - `env_override`: the raw `PAT_BINARY` value, if set.
/// - `sidecar`: the bundled-sidecar path resolution (`Ok(path)` or a resolution
///   error string).
///
/// Order: a `PAT_BINARY` pointing at a non-empty file wins; otherwise the
/// bundled sidecar is used iff it resolves AND is a non-empty file; a missing /
/// zero-byte sidecar (or a resolution error) → `Err(SIDECAR_STUB_REASON)`. A
/// set-but-unusable `PAT_BINARY` (missing / empty) falls through to the sidecar
/// path, so the operator still gets the actionable stub message.
fn resolve_pat_binary_inner(
    env_override: Option<std::ffi::OsString>,
    sidecar: Result<PathBuf, String>,
) -> Result<PathBuf, String> {
    // (1) PAT_BINARY override — use only if it points at a non-empty file.
    if let Some(raw) = env_override {
        let candidate = PathBuf::from(raw);
        if is_nonempty_file(&candidate) {
            return Ok(candidate);
        }
        // Set-but-unusable: fall through to the bundled sidecar / stub error.
    }

    // (2)+(3) Bundled sidecar + stub/missing detection (adrev #12).
    match sidecar {
        Ok(resolved) if is_nonempty_file(&resolved) => Ok(resolved),
        // current_exe failed (extremely unusual) — fold into the same
        // "unavailable" class so the bootstrap surfaces a single Failed state.
        Err(e) => Err(format!("{SIDECAR_STUB_REASON} (resolution error: {e})")),
        // Resolved but missing or 0-byte (the debug stub).
        Ok(_) => Err(SIDECAR_STUB_REASON.to_string()),
    }
}

/// `true` iff `path` exists, is a regular file, and is non-empty. A 0-byte file
/// (the debug sidecar stub) returns `false` — the adrev #12 detection.
fn is_nonempty_file(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

/// Resolve a sidecar base name to its on-disk path next to the current
/// executable — a faithful reimplementation of `tauri-plugin-shell`'s private
/// `relative_command_path` (the plugin does not expose it, and we need a raw
/// `PathBuf` for `std::process::Command`). Steps up out of a `deps/` directory
/// so dev/test binary layouts resolve to the workspace target dir, matching the
/// plugin's behavior exactly.
fn resolve_sidecar_path(base_name: &str) -> Result<PathBuf, String> {
    let exe = tauri::utils::platform::current_exe()
        .map_err(|e| format!("could not resolve current executable: {e}"))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "current executable has no parent directory".to_string())?;
    let base_dir = if exe_dir.ends_with("deps") {
        exe_dir.parent().unwrap_or(exe_dir)
    } else {
        exe_dir
    };
    Ok(base_dir.join(base_name))
}

// ============================================================================
// .setup() bootstrap worker (spec §3.3, §3.6, §3.7)
// ============================================================================

/// Run the app-start Pat bootstrap (spec §3.3). Spawns a dedicated
/// `std::thread` and returns IMMEDIATELY so the webview paints without waiting
/// on Pat's up-to-10s announce — every path inside the thread is non-fatal, so
/// the app ALWAYS launches.
///
/// **Background mechanism — `std::thread`, not `async_runtime::spawn` +
/// `spawn_blocking` (spec §3.3, adrev #5):** `PatBackend::spawn` BLOCKS up to
/// ~10s on Pat's port announce. A dedicated OS thread owns that blocking work
/// cleanly without parking a Tokio worker for 10s. From inside the thread we
/// start the async session-log drain via `tauri::async_runtime::spawn`, which
/// dispatches onto Tauri's GLOBAL runtime handle (a `OnceLock`, valid after
/// setup) and is callable from any thread — so we never need a runtime in the
/// thread's own scope. We do NOT use a raw `tokio::spawn` (adrev #5: no runtime
/// in a bare `std::thread`'s scope).
///
/// **AppHandle (adrev #6):** the caller clones the `AppHandle` and moves the
/// clone into the thread; the thread re-enters Tauri only via that owned handle
/// (managed-state lookups, `emit`), never via a borrowed `app`/`State`.
pub fn run(app_handle: AppHandle) {
    std::thread::spawn(move || {
        let action = bootstrap_decision(crate::config::read_config());
        let state = app_handle.state::<BackendState>();

        match action {
            // Pre-wizard / wizard-rendering / offline: leave NotConfigured.
            BootstrapAction::NotConnected => {
                state.set_phase(BackendPhase::NotConfigured);
            }
            // Config exists but unusable: explicit ConfigError + one synthetic
            // session-log line carrying the reason (spec §3.3, adrev #15).
            BootstrapAction::ConfigError(reason) => {
                state.set_phase(BackendPhase::ConfigError {
                    reason: reason.clone(),
                });
                emit_backend_line(&app_handle, LogLevel::Error, reason);
            }
            // CMS mode: install the native Winlink backend (no Pat). The
            // BootstrapAction is still named `Spawn` (it gates the same
            // `wizard_completed && connect_to_cms` config), but as of the native
            // cutover (tuxlink-0ic) it installs `NativeBackend` rather than
            // spawning Pat. The Pat path (`spawn_pat`) is retained but unused
            // until/unless a fallback is wanted.
            BootstrapAction::Spawn(cfg) => {
                install_native(&app_handle, &state, *cfg);
            }
        }
    });
}

/// The CMS-mode install path (native cutover, tuxlink-0ic). Constructs the
/// native Winlink backend over its own on-disk mailbox (`<app_data>/native-mbox`)
/// and installs it — no Pat process, no blocking spawn, no sidecar. Non-fatal: a
/// path-resolver failure surfaces as `Failed` + a session-log line.
///
/// NOTE: the native client presents the SID `tuxlink`, which the production CMS
/// rejects until registered with Winlink (it directs unknown clients to
/// `cms-z.winlink.org`). The backend is installed and the mailbox/compose UI
/// works regardless; a CMS connect against production needs that registration.
fn install_native(app_handle: &AppHandle, state: &BackendState, cfg: Config) {
    let mbox_dir = match app_handle.path().app_data_dir() {
        Ok(dir) => dir.join("native-mbox"),
        Err(e) => {
            let reason = format!("could not resolve app data dir for the native mailbox: {e}");
            state.set_phase(BackendPhase::Failed {
                reason: reason.clone(),
            });
            emit_backend_line(app_handle, LogLevel::Error, reason);
            return;
        }
    };

    let backend = NativeBackend::new(cfg, mbox_dir);
    state.install(Arc::new(backend));
    emit_backend_line(
        app_handle,
        LogLevel::Info,
        "Native Winlink backend ready (no Pat).".to_string(),
    );
}

/// The CMS-mode spawn path (spec §3.3 step 2-3, §3.6). Sets `Spawning`,
/// resolves the sidecar + Pat paths, calls the BLOCKING `PatBackend::spawn`,
/// installs the backend on success (+ starts the drain), or sets `Failed` +
/// emits an error line on any failure. All non-fatal.
///
/// Retained but UNUSED as of the native cutover (tuxlink-0ic): the bootstrap now
/// installs `NativeBackend` instead. Kept (not deleted) per "don't delete Pat
/// until native reaches parity" — easy to re-wire as a fallback.
#[allow(dead_code)]
fn spawn_pat(app_handle: &AppHandle, state: &BackendState, cfg: Config) {
    state.set_phase(BackendPhase::Spawning);

    // Resolve the Pat binary (adrev #12: stub/missing → Failed).
    let binary = match resolve_pat_binary(app_handle) {
        Ok(b) => b,
        Err(reason) => {
            state.set_phase(BackendPhase::Failed {
                reason: reason.clone(),
            });
            emit_backend_line(app_handle, LogLevel::Error, reason);
            return;
        }
    };

    // Derive Pat's config / mbox / pid paths from Tauri's path resolver
    // (spec §3.6): config under the app-config dir, mbox + pid under the
    // app-data dir. One consistent mechanism, honoring the platform's dirs.
    let paths = match resolve_pat_paths(app_handle) {
        Ok(p) => p,
        Err(reason) => {
            state.set_phase(BackendPhase::Failed {
                reason: reason.clone(),
            });
            emit_backend_line(app_handle, LogLevel::Error, reason);
            return;
        }
    };

    // Clone the SAME Arc<SessionLogState> the `session_log_snapshot` command
    // reads, so the spawn's bridge thread appends startup lines to the buffer
    // the UI snapshots (spec §11.1). Keep a second clone for the drain (FIX 1):
    // the drain polls THIS durable buffer, so it must outlive the move into
    // `PatBackend::spawn` below.
    let buffer: Arc<SessionLogState> = (*app_handle.state::<Arc<SessionLogState>>()).clone();
    let drain_buffer = buffer.clone();

    // Start the session-log drain BEFORE the (blocking) spawn (Codex R3 #2).
    // The drain polls the durable buffer, so it emits whatever lands there on
    // EITHER outcome: the bridge's live Pat lines on Ok, AND (critically) the
    // drained Pat stderr diagnostics + synthetic error line on Err. Starting it
    // only on Ok left a failed spawn's diagnostics un-emitted until a frontend
    // remount — defeating the three-state "explicit error + reason" surface.
    start_drain(app_handle.clone(), drain_buffer);

    // BLOCKING: waits up to ~10s for Pat's port announce. This is why `run`
    // uses a dedicated std::thread (not a Tokio worker).
    match PatBackend::spawn(
        PatBackendSpawnOptions {
            binary,
            config_path: paths.config_path,
            mbox_dir: paths.mbox_dir,
            pid_file: paths.pid_file,
            tuxlink_config: cfg,
        },
        buffer,
    ) {
        Ok(backend) => {
            // Install (Ready, Some(backend)) atomically. The drain (already
            // running, above) polls the durable buffer, so it needs no backend
            // handle; installing makes status coherent.
            let arc = Arc::new(backend);
            state.install(arc);
        }
        Err(e) => {
            let reason = e.to_string();
            state.set_phase(BackendPhase::Failed {
                reason: reason.clone(),
            });
            emit_backend_line(app_handle, LogLevel::Error, reason);
        }
    }
}

/// Start the session-log drain task (spec §3.7).
///
/// **FIX 1 (tuxlink-22l Codex R2): polls the DURABLE buffer, not the broadcast.**
/// The prior drain subscribed to `PatBackend::stream_log()` (a tokio broadcast)
/// and emitted only live, post-subscribe events. But the spawn's bridge thread
/// appends + broadcasts Pat's startup lines DURING the blocking `PatBackend::spawn`
/// — i.e. BEFORE this drain (started only after spawn returns) can subscribe.
/// Those early lines were broadcast to zero receivers and lost; the frontend's
/// one-shot snapshot also ran (empty) during the blocking spawn, so the startup
/// log never reached the UI (Codex #1). Broadcast lag could also silently drop
/// events under load (Codex #7).
///
/// The durable [`SessionLogState`] ring buffer is the source of truth: every
/// line the bridge ingests is `append`ed there with a monotonic `seq`, whether
/// or not anyone is listening. Polling `snapshot_since(last_seq)` therefore
/// emits EVERY buffered line exactly once, in seq order, immune to both
/// subscriber-timing and broadcast lag. The frontend already dedupes on `seq`
/// (and seeds from `session_log_snapshot`), so re-delivery of a line it already
/// has via snapshot is harmless.
///
/// Dispatched via `tauri::async_runtime::spawn` (Tauri's global runtime, valid
/// post-setup, callable from this std::thread — NOT a raw `tokio::spawn`). The
/// task runs for the app's lifetime, polling at a 250ms cadence (latency floor
/// for a backend log line reaching an already-open pane; the snapshot covers
/// re-opens). `PatBackend::stream_log()`'s broadcast remains available for other
/// consumers; this drain no longer depends on it.
///
/// Retained but UNUSED as of the native cutover (tuxlink-0ic) — only `spawn_pat`
/// started it. Native backend logging is wired separately when added.
#[allow(dead_code)]
fn start_drain(app_handle: AppHandle, buffer: Arc<SessionLogState>) {
    tauri::async_runtime::spawn(async move {
        let mut last_seq: u64 = 0;
        loop {
            // Pull every line newer than the cursor and advance it. Factored
            // into `drain_step` so the cursor-advance + emit loop is unit-tested
            // (the Tauri `emit` is the only un-testable part, injected as a
            // closure). `drain_step` returns the new cursor.
            last_seq = drain_step(&buffer, last_seq, |line| {
                let _ = app_handle
                    .emit("session_log:line", crate::ui_commands::LogLineDto::from(line));
            });
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    });
}

/// One iteration of the buffer-polling drain (FIX 1): emit every buffered line
/// with `seq > last_seq` (oldest first), advancing the cursor past each.
/// Returns the updated cursor (the max `seq` emitted, or `last_seq` unchanged if
/// nothing was newer). `emit` receives each line in seq order exactly once.
///
/// Pure w.r.t. the cursor logic (the side effect is the injected `emit`), so the
/// "emit each new line once, never re-emit, advance monotonically" contract is
/// unit-tested without a Tauri runtime — see `tests::drain_step_*`.
fn drain_step(
    buffer: &SessionLogState,
    last_seq: u64,
    mut emit: impl FnMut(LogLine),
) -> u64 {
    let mut cursor = last_seq;
    for line in buffer.snapshot_since(last_seq) {
        cursor = line.seq;
        emit(line);
    }
    cursor
}

/// Bundle of the three Pat-process paths the bootstrap derives (spec §3.6).
/// Retained but UNUSED as of the native cutover (tuxlink-0ic).
#[allow(dead_code)]
struct PatPaths {
    config_path: PathBuf,
    mbox_dir: PathBuf,
    pid_file: PathBuf,
}

/// Derive Pat's config/mbox/pid paths from Tauri's path resolver (spec §3.6).
/// Config: `<app_config_dir>/pat/config.json`. Mbox: `<app_data_dir>/pat-mbox/`.
/// Pid: `<app_data_dir>/pat.pid`. `PatProcess::spawn` creates the mbox + pid
/// parent dirs and renders the config; we only compute the paths here. A
/// path-resolver failure (no home dir, etc.) → `Err` so the caller surfaces a
/// single `Failed` state. Retained but UNUSED as of the native cutover.
#[allow(dead_code)]
fn resolve_pat_paths(app_handle: &AppHandle) -> Result<PatPaths, String> {
    let config_dir = app_handle
        .path()
        .app_config_dir()
        .map_err(|e| format!("could not resolve app config dir: {e}"))?;
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve app data dir: {e}"))?;
    Ok(PatPaths {
        config_path: config_dir.join("pat").join("config.json"),
        mbox_dir: data_dir.join("pat-mbox"),
        pid_file: data_dir.join("pat.pid"),
    })
}

/// Append a synthetic `LogSource::Backend` line to the durable buffer (so it
/// survives in `session_log_snapshot`) AND emit it live on `session_log:line`
/// (so an already-listening UI sees it immediately). Used for the bootstrap's
/// own error / config-error lines (spec §3.3, §5). Best-effort: a poisoned
/// buffer lock (append → seq 0) or an emit error is swallowed — the phase
/// transition is the primary signal; the log line is the explanatory detail.
fn emit_backend_line(app_handle: &AppHandle, level: LogLevel, message: String) {
    let mut line = LogLine {
        seq: 0,
        timestamp_iso: now_iso8601_utc(),
        level,
        source: LogSource::Backend,
        message,
    };
    let buffer = app_handle.state::<Arc<SessionLogState>>();
    line.seq = buffer.append(line.clone());
    let _ = app_handle.emit("session_log:line", crate::ui_commands::LogLineDto::from(line));
}

/// Whole-second UTC ISO-8601 timestamp (`YYYY-MM-DDTHH:MM:SSZ`). A local copy
/// of the same minimal formatter in `winlink_backend.rs` / `ui_commands.rs` /
/// `wizard.rs` (each is self-contained by design; a shared util module is out
/// of scope for v0.0.1 — see `winlink_backend::now_iso8601_utc`'s note). Used
/// only for the bootstrap's synthetic `LogSource::Backend` lines.
fn now_iso8601_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hour = (secs / 3600) % 24;
    let days = secs / 86400;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian (Howard
/// Hinnant's `civil_from_days`). Same algorithm as the sibling modules' copies.
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CmsTransport, Config, ConfigReadError, ConfigValidationError, ConnectConfig, GpsState,
        IdentityConfig, PositionPrecision, PrivacyConfig, CONFIG_SCHEMA_VERSION,
    };

    /// CMS-mode config fixture (`wizard_completed = true`, `connect_to_cms =
    /// true`). Built like the `ui_commands` config tests.
    fn cms_config() -> Config {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig {
                connect_to_cms: true,
                transport: CmsTransport::CmsSsl,
            },
            identity: IdentityConfig {
                callsign: Some("W4PHS".into()),
                identifier: None,
                grid: Some("EM10ab".into()),
            },
            privacy: PrivacyConfig {
                gps_state: GpsState::BroadcastAtPrecision,
                position_precision: PositionPrecision::FourCharGrid,
            },
            pat_mbo_address: None,
        }
    }

    // Err(NotFound) — pre-wizard, no config file → NotConnected.
    #[test]
    fn not_found_is_not_connected() {
        let action = bootstrap_decision(Err(ConfigReadError::NotFound {
            path: "/nonexistent/config.json".into(),
        }));
        assert!(matches!(action, BootstrapAction::NotConnected));
    }

    // Err(Serde) — config exists but won't parse → ConfigError(..).
    #[test]
    fn serde_error_is_config_error() {
        let serde_err = serde_json::from_str::<Config>("{ not json").unwrap_err();
        let action = bootstrap_decision(Err(ConfigReadError::Serde { source: serde_err }));
        match action {
            BootstrapAction::ConfigError(reason) => {
                assert!(!reason.is_empty(), "ConfigError carries a non-empty reason");
            }
            other => panic!("expected ConfigError, got {other:?}"),
        }
    }

    // Err(Validation) — config parsed but failed semantic validation →
    // ConfigError(..).
    #[test]
    fn validation_error_is_config_error() {
        let action = bootstrap_decision(Err(ConfigReadError::Validation {
            source: ConfigValidationError::CmsPathMissingCallsign,
        }));
        match action {
            BootstrapAction::ConfigError(reason) => {
                assert!(reason.contains("callsign"), "reason mentions the validation cause");
            }
            other => panic!("expected ConfigError, got {other:?}"),
        }
    }

    // Err(Io) — config path unreadable (not NotFound) → ConfigError(..).
    #[test]
    fn io_error_is_config_error() {
        let action = bootstrap_decision(Err(ConfigReadError::Io {
            path: "/some/config.json".into(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        }));
        assert!(matches!(action, BootstrapAction::ConfigError(_)));
    }

    // Ok(cfg) with !wizard_completed — the wizard is still rendering (adrev
    // #14) → NotConnected (never spawn Pat mid-wizard).
    #[test]
    fn wizard_incomplete_is_not_connected() {
        let mut cfg = cms_config();
        cfg.wizard_completed = false;
        let action = bootstrap_decision(Ok(cfg));
        assert!(matches!(action, BootstrapAction::NotConnected));
    }

    // Ok(cfg) with wizard_completed && !connect_to_cms — offline mode →
    // NotConnected (no Pat).
    #[test]
    fn offline_mode_is_not_connected() {
        let mut cfg = cms_config();
        cfg.connect.connect_to_cms = false;
        // Offline config forbids a callsign (Config::validate), but
        // bootstrap_decision does not re-validate — it only reads the two
        // gating flags. Clear callsign anyway to keep the fixture coherent.
        cfg.identity.callsign = None;
        let action = bootstrap_decision(Ok(cfg));
        assert!(matches!(action, BootstrapAction::NotConnected));
    }

    // Ok(cfg) with wizard_completed && connect_to_cms — CMS mode → Spawn.
    #[test]
    fn cms_mode_is_spawn() {
        let action = bootstrap_decision(Ok(cms_config()));
        match action {
            BootstrapAction::Spawn(cfg) => {
                assert!(cfg.connect.connect_to_cms);
                assert!(cfg.wizard_completed);
            }
            other => panic!("expected Spawn, got {other:?}"),
        }
    }

    // ========================================================================
    // Task D (tuxlink-22l) — Pat sidecar resolution (spec §3.5, adrev #12)
    // `resolve_pat_binary_inner` is the pure core (env + sidecar injected), so
    // these need no Tauri AppHandle and mutate no process env.
    // ========================================================================

    // The zero-byte detector: a 0-byte file is NOT a usable binary (the debug
    // sidecar stub). A non-empty file is. A missing path is not.
    #[test]
    fn is_nonempty_file_rejects_zero_byte_and_missing() {
        // Zero-byte tempfile → false (the adrev #12 stub case).
        let empty = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(empty.as_file().metadata().unwrap().len(), 0);
        assert!(
            !is_nonempty_file(empty.path()),
            "0-byte file is not a usable binary"
        );

        // Non-empty tempfile → true.
        let mut nonempty = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut nonempty, b"#!/bin/sh\n").unwrap();
        assert!(
            is_nonempty_file(nonempty.path()),
            "non-empty file is a usable binary"
        );

        // Missing path → false.
        assert!(
            !is_nonempty_file(std::path::Path::new("/nonexistent/pat-binary")),
            "missing file is not a usable binary"
        );
    }

    // adrev #12: a 0-byte sidecar (debug stub) with NO usable PAT_BINARY →
    // Err carrying the actionable stub message. This is the exact dev-run
    // failure the bootstrap must surface as `Failed`, not a silent empty state.
    #[test]
    fn resolve_inner_zero_byte_sidecar_no_override_is_stub_err() {
        let stub = tempfile::NamedTempFile::new().unwrap(); // 0 bytes
        let err = resolve_pat_binary_inner(None, Ok(stub.path().to_path_buf()))
            .expect_err("0-byte sidecar must be an Err");
        assert_eq!(err, SIDECAR_STUB_REASON);
        assert!(err.contains("PAT_BINARY"), "message tells the operator the override");
    }

    // A PAT_BINARY pointing at a 0-byte file is unusable → falls through to the
    // (also-stub) sidecar → the stub Err. (point at a 0-byte tempfile → Err.)
    #[test]
    fn resolve_inner_zero_byte_override_falls_through_to_stub_err() {
        let empty_override = tempfile::NamedTempFile::new().unwrap(); // 0 bytes
        let empty_sidecar = tempfile::NamedTempFile::new().unwrap(); // 0 bytes
        let err = resolve_pat_binary_inner(
            Some(empty_override.path().as_os_str().to_owned()),
            Ok(empty_sidecar.path().to_path_buf()),
        )
        .expect_err("0-byte override + 0-byte sidecar must be an Err");
        assert_eq!(err, SIDECAR_STUB_REASON);
    }

    // A PAT_BINARY pointing at a NON-EMPTY file wins (the dev/test escape hatch),
    // even when the bundled sidecar is the 0-byte stub.
    #[test]
    fn resolve_inner_nonempty_override_wins() {
        let mut good = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut good, b"#!/bin/sh\necho pat\n").unwrap();
        let stub_sidecar = tempfile::NamedTempFile::new().unwrap(); // 0 bytes
        let resolved = resolve_pat_binary_inner(
            Some(good.path().as_os_str().to_owned()),
            Ok(stub_sidecar.path().to_path_buf()),
        )
        .expect("non-empty PAT_BINARY override is used");
        assert_eq!(resolved, good.path());
    }

    // A current_exe resolution error (no override) folds into the stub-class Err
    // so the bootstrap surfaces a single Failed state.
    #[test]
    fn resolve_inner_sidecar_resolution_error_is_err() {
        let err = resolve_pat_binary_inner(None, Err("current_exe blew up".to_string()))
            .expect_err("a sidecar resolution error must be an Err");
        assert!(err.starts_with("Pat binary unavailable"));
        assert!(err.contains("current_exe blew up"), "preserves the resolution detail");
    }

    // ========================================================================
    // FIX 1 (tuxlink-22l Codex R2) — drain_step: buffer-polling cursor logic
    // The drain emits EVERY buffered line exactly once, in seq order, advancing
    // a monotonic cursor — immune to broadcast subscriber-timing/lag because it
    // reads the durable buffer (source of truth). Tested via a closure sink so
    // no Tauri runtime is needed.
    // ========================================================================

    fn log_line(msg: &str) -> LogLine {
        LogLine {
            seq: 0, // append() assigns the real seq
            timestamp_iso: "2026-05-20T00:00:00Z".into(),
            level: LogLevel::Info,
            source: LogSource::Pat,
            message: msg.into(),
        }
    }

    // A first poll from cursor 0 emits ALL buffered lines (incl. startup lines
    // appended during the blocking spawn, BEFORE any drain existed — the Codex
    // #1 bug) in seq order, and advances the cursor to the last seq.
    #[test]
    fn drain_step_first_poll_emits_all_buffered_lines_in_seq_order() {
        let buf = SessionLogState::new(16);
        for m in ["startup-a", "startup-b", "startup-c"] {
            buf.append(log_line(m));
        }
        let mut emitted: Vec<(u64, String)> = Vec::new();
        let new_cursor = drain_step(&buf, 0, |l| emitted.push((l.seq, l.message)));

        assert_eq!(
            emitted,
            vec![
                (1, "startup-a".to_string()),
                (2, "startup-b".to_string()),
                (3, "startup-c".to_string()),
            ],
            "every pre-existing line is emitted once, oldest-first (Codex #1 fix)"
        );
        assert_eq!(new_cursor, 3, "cursor advances to the last emitted seq");
    }

    // A subsequent poll emits only lines newer than the cursor (no re-emit), and
    // a poll with nothing new leaves the cursor unchanged and emits nothing.
    #[test]
    fn drain_step_advances_cursor_and_never_reemits() {
        let buf = SessionLogState::new(16);
        for m in ["a", "b"] {
            buf.append(log_line(m));
        }
        let mut first: Vec<u64> = Vec::new();
        let cursor = drain_step(&buf, 0, |l| first.push(l.seq));
        assert_eq!(first, vec![1, 2]);
        assert_eq!(cursor, 2);

        // Nothing new: empty emit, cursor unchanged.
        let mut empty: Vec<u64> = Vec::new();
        let cursor = drain_step(&buf, cursor, |l| empty.push(l.seq));
        assert!(empty.is_empty(), "no re-emit when nothing is newer than the cursor");
        assert_eq!(cursor, 2, "cursor unchanged when nothing newer");

        // Append more; next poll emits only the new ones.
        for m in ["c", "d"] {
            buf.append(log_line(m));
        }
        let mut next: Vec<u64> = Vec::new();
        let cursor = drain_step(&buf, cursor, |l| next.push(l.seq));
        assert_eq!(next, vec![3, 4], "only lines newer than the cursor are emitted");
        assert_eq!(cursor, 4);
    }
}
