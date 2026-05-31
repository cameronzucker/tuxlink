//! App-start bootstrap: the decision logic + the `.setup()` worker.
//!
//! bd issue: tuxlink-9phd (P5).
//!
//! Two layers:
//!
//! 1. [`bootstrap_decision`] — a PURE classification of `read_config()`'s result
//!    into a [`BootstrapAction`]. No I/O, no Tauri; unit-tested directly. This
//!    is the spec §3.3 / adrev #14,#15 gate: pre-wizard + offline both stay
//!    "not connected"; a malformed config is an explicit config error (NOT
//!    "not connected"); only `wizard_completed && connect_to_cms` installs
//!    the native backend.
//!
//! 2. [`run`] — the `.setup()` worker that executes the action: spawns a
//!    dedicated `std::thread` which drives the [`BackendState`] phase and
//!    installs the backend. ALL paths are non-fatal — the app always launches.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::app_backend::{BackendPhase, BackendState};
use crate::config::{Config, ConfigReadError};
use crate::session_log::SessionLogState;
use crate::winlink_backend::{LogLevel, LogLine, LogSource, NativeBackend, ProgressSink, WireSink};

/// What the bootstrap should do, decided purely from `read_config()`'s result.
#[derive(Debug)]
pub enum BootstrapAction {
    /// Leave the backend `NotConfigured` (the "not connected" empty state):
    /// pre-wizard (no config / `NotFound`), wizard still rendering
    /// (`!wizard_completed`), or offline mode (`!connect_to_cms`).
    NotConnected,
    /// A config file exists but is unusable (`Serde`/`Validation`/`Io`). Surface
    /// an explicit config error — do NOT masquerade as "not connected" (adrev
    /// #15). Carries the reason for the ribbon + the synthetic session-log line.
    ConfigError(String),
    /// CMS configured (`wizard_completed && connect_to_cms`): install the native
    /// backend. The `Config` is boxed because it is the largest variant and is
    /// moved into the install path (avoids a large enum + a needless clone).
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
/// - `Ok(cfg)` with `wizard_completed && !connect_to_cms` (offline mode) →
///   [`BootstrapAction::NotConnected`].
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
                // Offline mode: no CMS. Genuinely "not connected".
                BootstrapAction::NotConnected
            } else {
                // CMS mode: install native backend.
                BootstrapAction::Spawn(Box::new(cfg))
            }
        }
    }
}

// ============================================================================
// .setup() bootstrap worker
// ============================================================================

/// Run the app-start bootstrap. Spawns a dedicated `std::thread` and returns
/// IMMEDIATELY so the webview paints without waiting on the backend install —
/// every path inside the thread is non-fatal, so the app ALWAYS launches.
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
            // CMS mode: install the native Winlink backend.
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

    // Per-step connect progress (tuxlink-gqo): the native connect runs in a
    // blocking task with no `AppHandle`, so it reports each phase through this
    // sink, which appends a `LogSource::Transport` line to the session log (so it
    // survives in the snapshot) and emits it live. Mirrors `emit_backend_line`,
    // but tagged Transport rather than Backend.
    let progress_app = app_handle.clone();
    let progress: ProgressSink = Arc::new(move |msg: &str| {
        let buffer = progress_app.state::<Arc<SessionLogState>>();
        let mut line = LogLine {
            seq: 0,
            timestamp_iso: now_iso8601_utc(),
            level: LogLevel::Info,
            source: LogSource::Transport,
            message: msg.to_string(),
        };
        line.seq = buffer.append(line.clone());
        let _ = progress_app.emit("session_log:line", crate::ui_commands::LogLineDto::from(line));
    });

    // tuxlink-nki: raw B2F wire lines. The native connect tees every on-wire
    // protocol line (both directions) into this sink, which appends a
    // `LogSource::Wire` line to the session log + emits it live — so the operator
    // can watch the real `[WL2K-...]`/`;FW`/`FF`/`FQ` dialogue under "Raw output"
    // (the Human view suppresses wire lines). LogLevel::Trace — verbose detail.
    // Mirrors the progress sink above, tagged Wire rather than Transport.
    let wire_app = app_handle.clone();
    let wire: WireSink = Arc::new(move |msg: &str| {
        let buffer = wire_app.state::<Arc<SessionLogState>>();
        let mut line = LogLine {
            seq: 0,
            timestamp_iso: now_iso8601_utc(),
            level: LogLevel::Trace,
            source: LogSource::Wire,
            message: msg.to_string(),
        };
        line.seq = buffer.append(line.clone());
        let _ = wire_app.emit("session_log:line", crate::ui_commands::LogLineDto::from(line));
    });

    // tuxlink-686: inject the live PositionArbiter so the on-air CMS locator is
    // the arbiter's broadcast_grid() (live + precision-reduced) rather than the
    // stale config snapshot the backend was constructed with. The arbiter is
    // managed state registered in lib.rs::run() above the .setup() call; the Arc
    // ref-count is incremented here, not moved, so the lib.rs binding stays alive.
    let arbiter = (*app_handle.state::<Arc<crate::position::PositionArbiter>>()).clone();
    let backend = NativeBackend::with_progress(cfg, mbox_dir, progress)
        .with_wire_log(wire)
        .with_position(arbiter);
    state.install(Arc::new(backend));
    emit_backend_line(
        app_handle,
        LogLevel::Info,
        "Native Winlink backend ready (no Pat).".to_string(),
    );
}

/// One iteration of the buffer-polling drain: emit every buffered line with
/// `seq > last_seq` (oldest first), advancing the cursor past each. Returns the
/// updated cursor (the max `seq` emitted, or `last_seq` unchanged if nothing was
/// newer). `emit` receives each line in seq order exactly once.
///
/// Pure w.r.t. the cursor logic (the side effect is the injected `emit`), so the
/// "emit each new line once, never re-emit, advance monotonically" contract is
/// unit-tested without a Tauri runtime — see `tests::drain_step_*`.
///
/// Currently only consumed by unit tests; the production caller (`start_drain`)
/// was removed in tuxlink-9phd P5 when native logging stopped using the
/// broadcast-based drain. Retained because the test seam is the value.
#[cfg_attr(not(test), allow(dead_code))]
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
        IdentityConfig, PacketConfig, PositionPrecision, PositionSource, PrivacyConfig,
        CONFIG_SCHEMA_VERSION,
    };

    /// CMS-mode config fixture (`wizard_completed = true`, `connect_to_cms =
    /// true`). Built like the `ui_commands` config tests.
    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn cms_config() -> Config {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig {
                connect_to_cms: true,
                transport: CmsTransport::CmsSsl,
                host: crate::config::default_cms_host(),
            },
            identity: IdentityConfig {
                callsign: Some("W4PHS".into()),
                identifier: None,
                grid: Some("EM10ab".into()),
            },
            privacy: PrivacyConfig {
                gps_state: GpsState::BroadcastAtPrecision,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: PacketConfig::default(),
            modem_ardop: None,
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
    // #14) → NotConnected (never install backend mid-wizard).
    #[test]
    fn wizard_incomplete_is_not_connected() {
        let mut cfg = cms_config();
        cfg.wizard_completed = false;
        let action = bootstrap_decision(Ok(cfg));
        assert!(matches!(action, BootstrapAction::NotConnected));
    }

    // Ok(cfg) with wizard_completed && !connect_to_cms — offline mode →
    // NotConnected.
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
    // drain_step: buffer-polling cursor logic
    // The drain emits EVERY buffered line exactly once, in seq order, advancing
    // a monotonic cursor. Tested via a closure sink so no Tauri runtime is
    // needed.
    // ========================================================================

    fn log_line(msg: &str) -> LogLine {
        LogLine {
            seq: 0, // append() assigns the real seq
            timestamp_iso: "2026-05-20T00:00:00Z".into(),
            level: LogLevel::Info,
            source: LogSource::Backend,
            message: msg.into(),
        }
    }

    // A first poll from cursor 0 emits ALL buffered lines in seq order, and
    // advances the cursor to the last seq.
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
            "every pre-existing line is emitted once, oldest-first"
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
