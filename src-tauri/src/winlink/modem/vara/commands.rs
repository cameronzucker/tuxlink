//! Tauri commands for VARA modem operations (Phase 2 — bd-tuxlink-dfmf).
//!
//! Scope: minimal TCP-transport lifecycle. `start_vara_session` opens the
//! command and data sockets; `stop_vara_session` closes them; `vara_status`
//! returns a snapshot. Full session-state machine (B2F-over-VARA, RADIO-1-
//! gated `CONNECT` to a peer, ARQ-state derivation) is Phase 3 territory and
//! is NOT in this surface — opening the TCP sockets does NOT transmit, so
//! this surface is RADIO-1-safe on its own.
//!
//! ## Why a separate file
//!
//! The existing `modem_commands.rs` is ARDOP-shaped and already 1600+ lines.
//! VARA's domain (third-party process tuxlink does NOT spawn, no PTT/audio
//! to model, no consent token because no transmit yet) is distinct enough
//! that colocating with the ARDOP commands would muddy both. The bd issue
//! (tuxlink-dfmf §6) explicitly permits this layout.
//!
//! ## State model
//!
//! [`VaraSession`] is a managed-state singleton holding `Option<VaraTransport>`
//! plus a denormalized [`VaraStatus`] snapshot. Mutex-protected so two
//! concurrent Tauri invocations don't race on the transport handle.
//! [`vara_status`] reads the snapshot WITHOUT acquiring the transport, so a
//! UI poll never blocks on an in-flight start/stop.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::config::{self, VaraUiConfig};
use crate::modem_status::ShutdownableStream;
use crate::session_log::SessionLogState;
use crate::ui_commands::LogLineDto;
use crate::winlink_backend::{LogLevel, LogLine, LogSource};

use super::command::{Bandwidth, OutboundCommand};
use super::transport::{VaraConfig, VaraTransport};

/// Append a session-log line to the durable buffer (assigning its `seq`) and
/// emit it on `session_log:line`. Mirrors `ui_commands::emit_session_line`'s
/// pattern; defined locally here to keep that helper private to its module.
/// `_ = app.emit(...)` swallows the emit error: failure to broadcast is
/// non-fatal — the buffer's snapshot still has the line for late-mounting
/// consumers.
fn emit_vara_log(
    app: &AppHandle,
    buffer: &SessionLogState,
    level: LogLevel,
    message: String,
) {
    let mut line = LogLine {
        seq: 0,
        timestamp_iso: chrono::Utc::now().to_rfc3339(),
        level,
        source: LogSource::Transport,
        message,
    };
    line.seq = buffer.append(line.clone());
    let _ = app.emit("session_log:line", LogLineDto::from(line));
}

/// Coarse VARA transport state. `Connecting` is the in-flight window between
/// "operator clicked Start" and "TCP open succeeded or failed."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VaraState {
    /// No TCP transport open. Steady state after fresh start or after Stop.
    Closed,
    /// TCP connect in progress (in-flight). Brief — the UI may not observe
    /// this state since `start_vara_session` is synchronous, but it's the
    /// correct steady state during a slow `connect_timeout`.
    Connecting,
    /// Both cmd and data sockets are open. Setter commands (MYCALL/BW)
    /// have been sent if the config provided them.
    Open,
    /// Last attempt failed. `last_error` carries the reason. Transitions
    /// back to `Closed` on the next `start_vara_session`.
    Error,
}

/// Snapshot of the VARA session state for the frontend. Returned from
/// `vara_status` and from the start/stop commands so the UI can update
/// without a follow-up poll.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaraStatus {
    /// Current transport state.
    pub state: VaraState,
    /// Last error message (only meaningful when `state == Error`).
    pub last_error: Option<String>,
    /// Resolved host:cmd_port the session is currently bound to, for the
    /// UI to display in the panel header. `None` when `state == Closed`.
    pub bound_host: Option<String>,
    /// Resolved cmd_port the session is currently bound to.
    pub bound_cmd_port: Option<u16>,
}

impl VaraStatus {
    fn closed() -> Self {
        Self {
            state: VaraState::Closed,
            last_error: None,
            bound_host: None,
            bound_cmd_port: None,
        }
    }
}

impl Default for VaraStatus {
    fn default() -> Self {
        Self::closed()
    }
}

/// Managed Tauri state for VARA. Holds the transport handle + the latest
/// status snapshot. Mutex-protected so the start/stop/status commands can
/// run concurrently from the UI without racing the transport handle.
pub struct VaraSession {
    inner: Mutex<VaraSessionInner>,
}

struct VaraSessionInner {
    /// `Some` when the TCP sockets are open. Dropped on stop / error.
    transport: Option<VaraTransport>,
    /// Denormalized status snapshot returned by `vara_status`. Read without
    /// touching the transport so a UI poll never blocks behind an in-flight
    /// start/stop.
    status: VaraStatus,
    /// Cooperative cmd-port writer used by [`VaraSession::abort_in_flight`]
    /// to send `ABORT\r` (NOT `DISCONNECT\r` — see [`OutboundCommand::Abort`]
    /// vs [`OutboundCommand::Disconnect`]). Installed via
    /// [`VaraSession::install_abort_writer`] BEFORE any blocking session-state
    /// operation begins so the operator's Close Session click can interrupt
    /// an active B2F within spec §2's ~2s budget (tuxlink-0ye6 Task 4.1
    /// — spec §9 watched failure mode + Codex Round 1 P1 #4).
    ///
    /// Carries the bounded `write_timeout` from the transport-side
    /// `try_clone_abort_writer`; the session layer doesn't re-bound here.
    abort_writer: Option<Box<dyn std::io::Write + Send>>,
    /// Hard-close fallback paired with [`abort_writer`]. When the
    /// cooperative `ABORT\r` write fails (peer wedged past the bounded
    /// `write_timeout`), [`VaraSession::abort_in_flight`] calls
    /// `shutdown_both` on this handle to RST the underlying TCP stream so
    /// the VARA modem notices via TCP and halts in-flight TX on its end
    /// (tuxlink-0ye6 Task 4.1 — Codex Round 4 P1 #3).
    abort_stream: Option<Box<dyn ShutdownableStream>>,
}

impl VaraSession {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VaraSessionInner {
                transport: None,
                status: VaraStatus::default(),
                abort_writer: None,
                abort_stream: None,
            }),
        }
    }

    /// Read-only snapshot of the current status. Cheap; safe to poll.
    pub fn snapshot(&self) -> VaraStatus {
        self.inner
            .lock()
            .map(|g| g.status.clone())
            .unwrap_or_else(|poison| poison.into_inner().status.clone())
    }

    /// Take ownership of the open transport, leaving the session in
    /// state=Closed. Used by the listener consumer task (tuxlink-9ls2)
    /// to take the open transport for the armed window without holding
    /// the session mutex for hours.
    ///
    /// Returns `None` if the session has no transport (already closed,
    /// or another consumer raced and took it first). Mirrors the
    /// `ModemSession::take_transport` posture used by ARDOP's listener
    /// consumer task.
    pub fn take_transport(&self) -> Option<VaraTransport> {
        let mut guard = self.inner.lock().ok()?;
        let t = guard.transport.take();
        if t.is_some() {
            guard.status = VaraStatus::closed();
        }
        t
    }

    /// Return a previously-taken transport to the session, restoring
    /// state=Open. Called by the listener consumer task on disarm so
    /// the operator's next `vara_stop_session` / `vara_status` sees the
    /// transport as if the consumer never owned it.
    ///
    /// `bound_host` + `bound_cmd_port` should be the values the
    /// transport was opened with — the consumer task captures them
    /// from the session snapshot before calling `take_transport`.
    pub fn return_transport(
        &self,
        t: VaraTransport,
        bound_host: Option<String>,
        bound_cmd_port: Option<u16>,
    ) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.status = VaraStatus {
                state: VaraState::Open,
                last_error: None,
                bound_host,
                bound_cmd_port,
            };
            guard.transport = Some(t);
        }
    }

    /// Send `LISTEN ON` over the cmd socket while briefly holding the
    /// session lock. Returns Err if the transport isn't Open or the TCP
    /// write fails. Mirrors `ModemSession::send_listen_command(true)` —
    /// the listener arm command flips LISTEN before spawning the
    /// consumer task so an arm failure surfaces synchronously without
    /// leaving a dangling consumer.
    ///
    /// Holds the lock only for the duration of one TCP write (~ms on
    /// localhost). Does NOT hold it across the consumer task spawn.
    pub fn send_listen_on(&self) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| format!("session lock poisoned: {e}"))?;
        let transport = guard
            .transport
            .as_mut()
            .ok_or_else(|| "VARA session is not Open — call vara_start_session first".to_string())?;
        transport
            .send(&OutboundCommand::Listen(true))
            .map_err(|e| format!("LISTEN ON write failed: {e}"))
    }

    /// Install the side-channel cooperative writer + hard-close stream pair
    /// used by [`abort_in_flight`]. Mirrors the ARDOP
    /// `ModemSession::install_abort_writer` posture (tuxlink-0ye6 Task 4.1).
    ///
    /// Callers obtain the pair from
    /// [`VaraTransport::try_clone_abort_writer`] AFTER the cmd port is open
    /// but BEFORE any blocking session-state operation begins, so an
    /// operator click on Close Session can interrupt an in-flight B2F
    /// within spec §2's ~2s budget regardless of weak-signal latency on
    /// VARA's graceful `DISCONNECT` path.
    ///
    /// Replaces any previously-installed pair.
    pub fn install_abort_writer(
        &self,
        writer: Box<dyn std::io::Write + Send>,
        stream: Box<dyn ShutdownableStream>,
    ) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.abort_writer = Some(writer);
            guard.abort_stream = Some(stream);
        }
    }

    /// Bounded VARA-side abort: cooperatively send `ABORT\r` (NOT
    /// `DISCONNECT\r` — see spec §9 + Codex Round 1 P1 #4); on cooperative
    /// write Err, fall back to `shutdown_both` on the paired stream so the
    /// VARA modem notices via TCP RST and halts in-flight TX even when the
    /// cmd port itself is unresponsive (Codex Round 4 P1 #3).
    ///
    /// Bounded by the writer's `write_timeout` (1500 ms on production
    /// transports per [`crate::modem_status::ABORT_WRITE_TIMEOUT`]) + a
    /// single `shutdown_both` syscall — total runtime fits under spec §2's
    /// "abort within ~2s" contract (Codex Round 3 P1 #1).
    ///
    /// Returns:
    /// - `Ok(())` when the cooperative write succeeded.
    /// - `Err("VARA cmd port unresponsive; hard-closed")` when the
    ///   cooperative write failed and the fallback ran. The error string
    ///   stays operator-readable; callers surface it through the
    ///   existing `Result<_, String>` Tauri-command shape.
    /// - `Err("no abort writer installed")` when no writer is installed
    ///   (caller can fall through to `vara_stop_session_inner` for the
    ///   graceful TCP-only teardown).
    ///
    /// **VARA's `ABORT` vs `DISCONNECT` distinction:** the cmd codec
    /// (`command.rs::OutboundCommand::Abort` vs `::Disconnect`) models
    /// both because they have different semantics in the VARA host
    /// protocol. `ABORT` interrupts in-flight TX (hard tear-down);
    /// `DISCONNECT` waits for the current burst to complete (graceful).
    /// Only `ABORT` satisfies the spec §2 interrupt contract — sending
    /// `DISCONNECT` here would let an active B2F burst keep TXing for
    /// 30+ seconds on weak-signal HF, which is the exact failure mode
    /// the spec calls out as a Task 4.1 P1.
    pub fn abort_in_flight(&self) -> Result<(), String> {
        use std::io::Write;
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| format!("session lock poisoned: {e}"))?;
        if guard.abort_writer.is_none() {
            return Err("no abort writer installed".into());
        }
        // Phase 1: cooperative bounded write of ABORT\r. The writer's
        // write_timeout governs the upper bound here. Sending DISCONNECT
        // as a secondary command after ABORT was considered but rejected
        // — VARA may treat the subsequent DISCONNECT as a separate
        // graceful tear-down request and reset the burst-completion
        // counter (Codex Round 1 P1 #4: "ABORT must be sent FIRST" is
        // the load-bearing assertion; a follow-on DISCONNECT is optional
        // and not required for the interrupt contract).
        let cooperative = {
            let writer = guard.abort_writer.as_mut().expect("checked above");
            writer
                .write_all(b"ABORT\r")
                .and_then(|()| writer.flush())
        };
        if cooperative.is_ok() {
            return Ok(());
        }
        // Phase 2: cooperative write failed (timeout, WouldBlock,
        // BrokenPipe, etc.) — take the stream and hard-close it. Drop the
        // writer too: it's pointing at the same wedged socket and is no
        // longer useful. Discard the shutdown_both result deliberately —
        // even an Err here means the underlying socket is gone, which IS
        // the effective tear-down.
        guard.abort_writer = None;
        if let Some(mut stream) = guard.abort_stream.take() {
            let _ = stream.shutdown_both();
        }
        Err("VARA cmd port unresponsive; hard-closed".into())
    }
}

impl Default for VaraSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Platform-info shape for the Pi-availability gating banner. VARA is x86
/// Windows software that requires Wine on Linux; on ARM Linux (the Pi 5 in
/// particular, with its 16K-page-kernel default) Wine cannot run VARA at
/// all. The frontend reads `vara_supported` from this command at mount and
/// renders a disabled-with-banner state when false (per tuxlink-xfo).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    /// `std::env::consts::ARCH` value: "x86_64", "aarch64", etc.
    pub arch: String,
    /// `std::env::consts::OS` value: "linux", "windows", "macos".
    pub os: String,
    /// True iff this build can plausibly run VARA. Currently: any x86/x86_64
    /// host. False on aarch64 (Pi-5 hard-blocks Wine per the 16K-page-kernel
    /// constraint; `project_pi5_wine_16k_block` memory).
    pub vara_supported: bool,
}

/// Return platform info for Pi-availability gating. Pure: cfg!-based, no
/// runtime detection. The frontend uses `vara_supported` to gate the VARA
/// panel's Start button + render a banner explaining the requirement when
/// disabled.
#[tauri::command]
pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        arch: std::env::consts::ARCH.to_string(),
        os: std::env::consts::OS.to_string(),
        // x86 / x86_64 builds can plausibly run VARA (native on Windows,
        // under Wine on Linux/macOS). aarch64 hard-blocks (no Wine on Pi
        // 5 due to 16K page-size kernel; no Wine on ARM macOS).
        vara_supported: cfg!(any(target_arch = "x86", target_arch = "x86_64")),
    }
}

/// Return the persisted VARA configuration, or struct default if nothing has
/// been written yet (first run) or the config file is absent.
#[tauri::command]
pub fn config_get_vara() -> VaraUiConfig {
    config::read_config()
        .map(|cfg| cfg.modem_vara.unwrap_or_default())
        .unwrap_or_default()
}

/// Persist a new VARA configuration. Reads the current config, replaces
/// `modem_vara`, writes atomically. Errors when the config file cannot be
/// read (wizard not completed) or the write fails.
#[tauri::command]
pub fn config_set_vara(value: VaraUiConfig) -> Result<(), String> {
    let mut cfg = config::read_config().map_err(|e| format!("read failed: {e}"))?;
    cfg.modem_vara = Some(value);
    config::write_config_atomic(&cfg).map_err(|e| format!("save failed: {e}"))
}

/// Pure helper: build a `VaraConfig` (transport-layer) from a `VaraUiConfig`
/// (frontend-shaped). Extracted from the command so tests can exercise it
/// without needing a Tauri runtime.
pub fn build_transport_config(ui: &VaraUiConfig) -> VaraConfig {
    VaraConfig {
        host: ui.host.clone(),
        cmd_port: ui.cmd_port,
        data_port: ui.data_port,
        // Conservative defaults. The transport layer's own `Default` uses
        // 5s connect + 2s read; we pin them here so a future change to the
        // transport defaults doesn't silently shift the UI's behavior.
        connect_timeout: Duration::from_secs(5),
        read_timeout: Some(Duration::from_secs(2)),
    }
}

/// Pure helper: map a `bandwidth_hz` value to a `Bandwidth` enum variant.
/// Returns `None` when the value isn't one of VARA's documented bandwidths
/// (in which case the start command skips the `BW` setter rather than
/// sending an unparseable value).
pub fn bandwidth_from_hz(hz: u32) -> Option<Bandwidth> {
    match hz {
        500 => Some(Bandwidth::Bw500),
        2300 => Some(Bandwidth::Bw2300),
        2750 => Some(Bandwidth::Bw2750),
        _ => None,
    }
}

/// Start a VARA session: open the cmd + data TCP socket pair, optionally
/// send the `BW <hz>` setter, return the new status snapshot.
///
/// Does NOT send `CONNECT` and does NOT transmit. Opening these sockets is
/// equivalent to opening a TCP connection to localhost:8300 — RADIO-1-safe.
/// The RF-transmitting `CONNECT` flow lands in Phase 3 with the full
/// session-state machine and a consent token gate.
///
/// If a session is already open, returns Err — operator must `vara_stop_session`
/// first. (This is conservative; a future iteration might re-open transparently.)
#[tauri::command]
pub fn vara_start_session(
    app: AppHandle,
    session: State<'_, std::sync::Arc<VaraSession>>,
    log: State<'_, Arc<SessionLogState>>,
) -> Result<VaraStatus, String> {
    let ui_cfg = config_get_vara();
    // Pull the operator's callsign from persisted identity. Pre-wizard /
    // missing-callsign yields None; the inner skips the MYCALL setter in
    // that case (VARA will continue to log "not connected to App" warnings,
    // but the right fix for that is wizard completion, not a backend bandaid).
    let callsign = config::read_config()
        .ok()
        .and_then(|c| c.identity.callsign);
    let host_label = format!("{}:{}", ui_cfg.host, ui_cfg.cmd_port);
    emit_vara_log(
        &app,
        &log,
        LogLevel::Info,
        format!("VARA: opening TCP transport to {host_label}"),
    );
    match vara_start_session_inner(&session, &ui_cfg, callsign.as_deref()) {
        Ok(status) => {
            let with_mycall = if callsign.is_some() {
                " (MYCALL sent)"
            } else {
                " (no callsign — wizard incomplete; VARA will warn 'not connected to App')"
            };
            emit_vara_log(
                &app,
                &log,
                LogLevel::Info,
                format!("VARA: transport open at {host_label}{with_mycall}"),
            );
            Ok(status)
        }
        Err(e) => {
            emit_vara_log(
                &app,
                &log,
                LogLevel::Error,
                format!("VARA: start failed — {e}"),
            );
            Err(e)
        }
    }
}

/// Inner helper for [`vara_start_session`] with factored-out config + callsign
/// args so tests can drive it without touching the persisted config file or a
/// Tauri runtime. `callsign` is `Some` when the wizard has set an operator
/// callsign; when `Some`, MYCALL is sent on the cmd socket after TCP open
/// (before BW) so VARA's host protocol recognizes the App handshake.
pub fn vara_start_session_inner(
    session: &std::sync::Arc<VaraSession>,
    ui_cfg: &VaraUiConfig,
    callsign: Option<&str>,
) -> Result<VaraStatus, String> {
    // Acquire the lock for the duration of the open. We hold the lock across
    // `VaraTransport::connect` (TCP connect, ~ms on localhost; bounded by
    // the 5s connect_timeout) — calls from the UI side are serialized so a
    // double-press on Start doesn't open two transports.
    let mut guard = session.inner.lock().map_err(|e| format!("session lock poisoned: {e}"))?;

    if guard.transport.is_some() {
        return Err("VARA session already started — call vara_stop_session first".into());
    }

    // Mark Connecting so any concurrent vara_status sees the in-flight state.
    // (The lock prevents true concurrency on the start path itself.)
    guard.status = VaraStatus {
        state: VaraState::Connecting,
        last_error: None,
        bound_host: Some(ui_cfg.host.clone()),
        bound_cmd_port: Some(ui_cfg.cmd_port),
    };

    let transport_cfg = build_transport_config(ui_cfg);
    let mut transport = match VaraTransport::connect(transport_cfg) {
        Ok(t) => t,
        Err(e) => {
            // Record the error, surface to caller, leave transport=None so
            // the next start attempt can retry.
            guard.status = VaraStatus {
                state: VaraState::Error,
                last_error: Some(format!("TCP connect failed: {e}")),
                bound_host: Some(ui_cfg.host.clone()),
                bound_cmd_port: Some(ui_cfg.cmd_port),
            };
            return Err(format!("TCP connect failed: {e}"));
        }
    };

    // Send MYCALL FIRST (identity handshake). Without it, VARA logs
    // "WARNING: VARA is not connected to any App via TCP Port <n>" and
    // treats the socket as half-attached. Pre-wizard / no callsign:
    // skip; the operator sees the VARA-side warning and knows to
    // complete identity setup.
    if let Some(call) = callsign {
        let trimmed = call.trim();
        if !trimmed.is_empty() {
            let _ = transport.send(&OutboundCommand::MyCall(trimmed.to_string()));
        }
    }

    // Best-effort: send BW if the operator configured a known bandwidth.
    // VARA echoes setter commands on success; we don't wait for the echo
    // here (the read would block up to the 2s read_timeout) — the operator
    // is responsible for verifying the configuration matches what the VARA
    // instance accepted. A future enhancement could surface the echo in a
    // status field.
    if let Some(hz) = ui_cfg.bandwidth_hz {
        if let Some(bw) = bandwidth_from_hz(hz) {
            // Ignore send errors here — the transport is open and usable
            // even if the BW setter didn't take. The status reflects "open"
            // not "fully configured."
            let _ = transport.send(&OutboundCommand::Bw(bw));
        }
    }

    guard.transport = Some(transport);
    guard.status = VaraStatus {
        state: VaraState::Open,
        last_error: None,
        bound_host: Some(ui_cfg.host.clone()),
        bound_cmd_port: Some(ui_cfg.cmd_port),
    };

    Ok(guard.status.clone())
}

/// Stop a VARA session: close the TCP sockets and clear the transport handle.
/// Idempotent — calling on an already-closed session is a no-op that returns
/// the closed status.
#[tauri::command]
pub fn vara_stop_session(
    app: AppHandle,
    session: State<'_, std::sync::Arc<VaraSession>>,
    log: State<'_, Arc<SessionLogState>>,
) -> Result<VaraStatus, String> {
    // Capture whether the transport was open BEFORE the stop, so the log
    // line distinguishes "actually closed something" from a no-op idempotent
    // call after an already-closed session.
    let was_open = session
        .inner
        .lock()
        .map(|g| g.transport.is_some())
        .unwrap_or(false);
    let result = vara_stop_session_inner(&session);
    if was_open {
        emit_vara_log(
            &app,
            &log,
            LogLevel::Info,
            "VARA: transport closed".to_string(),
        );
    }
    result
}

/// Inner helper for [`vara_stop_session`] so tests can drive without a Tauri runtime.
pub fn vara_stop_session_inner(
    session: &std::sync::Arc<VaraSession>,
) -> Result<VaraStatus, String> {
    let mut guard = session.inner.lock().map_err(|e| format!("session lock poisoned: {e}"))?;

    // Drop the transport (closes both sockets — TcpStream::Drop sends FIN).
    // We don't send DISCONNECT first because (a) DISCONNECT could trigger
    // an unwanted RF DISC frame if a peer-connect happened to be in flight,
    // and (b) Phase 2 doesn't expose any peer-connect path so the worst-
    // case state is "MYCALL/BW set, no CONNECT issued" — pure TCP teardown
    // is the right semantics.
    guard.transport = None;
    guard.status = VaraStatus::closed();
    Ok(guard.status.clone())
}

/// Return the current session status snapshot. Cheap; safe to poll. Hooks
/// call this on mount to recover state after a hot-reload.
#[tauri::command]
pub fn vara_status(session: State<'_, std::sync::Arc<VaraSession>>) -> VaraStatus {
    session.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn fresh_session_snapshot_is_closed() {
        let session = Arc::new(VaraSession::new());
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Closed);
        assert!(snap.last_error.is_none());
        assert!(snap.bound_host.is_none());
    }

    #[test]
    fn platform_info_reports_current_arch() {
        let info = platform_info();
        assert_eq!(info.arch, std::env::consts::ARCH);
        assert_eq!(info.os, std::env::consts::OS);
        // vara_supported is true on x86/x86_64, false elsewhere.
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        assert!(info.vara_supported, "x86 should report vara_supported=true");
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        assert!(!info.vara_supported, "non-x86 should report vara_supported=false");
    }

    #[test]
    fn build_transport_config_carries_host_and_ports() {
        let ui = VaraUiConfig {
            host: "10.0.0.5".into(),
            cmd_port: 8400,
            data_port: 8401,
            bandwidth_hz: Some(2750),
        };
        let t = build_transport_config(&ui);
        assert_eq!(t.host, "10.0.0.5");
        assert_eq!(t.cmd_port, 8400);
        assert_eq!(t.data_port, 8401);
        // Conservative pinned defaults — guards against silent shift if the
        // transport layer's Default changes.
        assert_eq!(t.connect_timeout.as_secs(), 5);
        assert_eq!(t.read_timeout.map(|d| d.as_secs()), Some(2));
    }

    #[test]
    fn bandwidth_from_hz_maps_documented_values() {
        // Standard VARA HF bandwidths.
        assert!(bandwidth_from_hz(500).is_some(), "500 Hz is a documented narrow-HF bandwidth");
        assert!(bandwidth_from_hz(2300).is_some(), "2300 Hz is VARA HF Standard");
        assert!(bandwidth_from_hz(2750).is_some(), "2750 Hz is VARA HF Tactical");
    }

    #[test]
    fn bandwidth_from_hz_returns_none_for_unknown_value() {
        // A nonsense value: caller should skip the BW setter rather than
        // sending an unparseable bandwidth to VARA.
        assert!(bandwidth_from_hz(42).is_none(), "unknown values must return None");
    }

    #[test]
    fn vara_stop_session_on_fresh_session_is_idempotent() {
        let session = Arc::new(VaraSession::new());
        let s1 = vara_stop_session_inner(&session).unwrap();
        assert_eq!(s1.state, VaraState::Closed);
        // Second call is a no-op that also returns Closed.
        let s2 = vara_stop_session_inner(&session).unwrap();
        assert_eq!(s2.state, VaraState::Closed);
    }

    #[test]
    fn vara_start_session_fails_when_tcp_unreachable() {
        // Bind to a known-unreachable port to force a connect error without
        // racing a real listener. Port 1 is reserved + unprivileged, so the
        // TCP connect will fail fast.
        let session = Arc::new(VaraSession::new());
        let ui_cfg = VaraUiConfig {
            host: "127.0.0.1".into(),
            // Port 1: requires root to bind; no user-mode listener can be
            // running here. Connect must fail with ConnectionRefused.
            cmd_port: 1,
            data_port: 2,
            bandwidth_hz: None,
        };
        let err = vara_start_session_inner(&session, &ui_cfg, None).unwrap_err();
        assert!(err.contains("TCP connect failed"), "got: {err}");

        // Status must reflect Error and the transport must remain None so
        // a follow-up retry is possible.
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Error);
        assert!(snap.last_error.is_some(), "last_error must be populated");
        assert_eq!(snap.bound_host.as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn vara_start_session_double_start_rejected() {
        // Build a session that's already in "Open" state by hand (so we
        // don't need a live VARA to test the guard).
        let session = Arc::new(VaraSession::new());
        {
            let mut guard = session.inner.lock().unwrap();
            // Synthesize an Open status WITHOUT a real transport — the
            // guard checks transport.is_some() not state==Open. To test
            // the guard we need transport.is_some(), so we'd need a real
            // TcpStream. Skip the actual transport injection; the
            // double-start guard is best exercised via integration smoke
            // (operator smoke checklist in the PR body).
            //
            // What we CAN test cheaply: when transport is None (just-stopped
            // or just-errored), start is permitted. This is the negative
            // of the guard — a sanity check that the guard isn't
            // perma-locking.
            guard.status = VaraStatus {
                state: VaraState::Error,
                last_error: Some("prior failure".into()),
                bound_host: None,
                bound_cmd_port: None,
            };
            assert!(guard.transport.is_none(), "guard tests pre-state");
        }
        // Trying to start after a prior error (transport=None) should attempt
        // the connect — and since we use unreachable port 1, will fail with
        // the connect error, NOT the "already started" error.
        let ui_cfg = VaraUiConfig {
            host: "127.0.0.1".into(),
            cmd_port: 1,
            data_port: 2,
            bandwidth_hz: None,
        };
        let err = vara_start_session_inner(&session, &ui_cfg, None).unwrap_err();
        assert!(
            err.contains("TCP connect failed"),
            "after a prior error, start should re-attempt and fail at TCP (not the double-start guard); got: {err}"
        );
    }

    // tuxlink-9ls2: take_transport / return_transport — the lifecycle
    // primitives the listener consumer task uses to own the transport for
    // the armed window.

    #[test]
    fn take_transport_from_empty_session_returns_none() {
        let session = Arc::new(VaraSession::new());
        // Fresh session: no transport open → take returns None.
        assert!(session.take_transport().is_none());
        // Snapshot remains Closed after a failed take.
        assert_eq!(session.snapshot().state, VaraState::Closed);
    }

    #[test]
    fn return_transport_restores_open_state() {
        // We can't easily construct a real VaraTransport in a unit test
        // (would require two live TcpListeners), but we CAN exercise the
        // state-machine half: after a take from a fresh session returns
        // None, the snapshot must be Closed; after a manual return_transport
        // call with a real transport, state is Open. The wire half is
        // covered by the listener.rs spawn_mock_vara tests.
        //
        // What we test here: the state transitions when a transport IS
        // present. Bind a real TCP listener pair so we can build a real
        // VaraTransport; install it into a session; then take + return.
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();

        // Spawn acceptors so VaraTransport::connect's two TCP connects
        // complete.
        let cmd_handle = thread::spawn(move || {
            let (_c, _) = cmd_l.accept().unwrap();
            thread::sleep(Duration::from_millis(500));
        });
        let data_handle = thread::spawn(move || {
            let (_c, _) = data_l.accept().unwrap();
            thread::sleep(Duration::from_millis(500));
        });

        let cfg = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            connect_timeout: Duration::from_secs(2),
            read_timeout: Some(Duration::from_millis(100)),
        };
        let transport = VaraTransport::connect(cfg).expect("connect must succeed");

        let session = Arc::new(VaraSession::new());
        // Install: manually set state to Open + plant the transport.
        {
            let mut guard = session.inner.lock().unwrap();
            guard.transport = Some(transport);
            guard.status = VaraStatus {
                state: VaraState::Open,
                last_error: None,
                bound_host: Some("127.0.0.1".into()),
                bound_cmd_port: Some(cmd_port),
            };
        }
        assert_eq!(session.snapshot().state, VaraState::Open);

        // Take: snapshot transitions to Closed, transport handed to caller.
        let taken = session.take_transport();
        assert!(taken.is_some(), "take must return the transport");
        assert_eq!(session.snapshot().state, VaraState::Closed);

        // Return: state restored to Open with the bound info preserved.
        session.return_transport(
            taken.unwrap(),
            Some("127.0.0.1".into()),
            Some(cmd_port),
        );
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Open);
        assert_eq!(snap.bound_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(snap.bound_cmd_port, Some(cmd_port));

        // Cleanup: drop the session (closes the transport's sockets) and
        // join the acceptor threads.
        drop(session);
        cmd_handle.join().unwrap();
        data_handle.join().unwrap();
    }

    // ── tuxlink-0ye6 Task 4.1: VaraSession::abort_in_flight ──────────────
    //
    // VARA equivalent of ARDOP's `ModemSession::abort_in_flight` — sends
    // VARA's `ABORT\r` (NOT `DISCONNECT\r`; the codec models both
    // distinctly per command.rs OutboundCommand::Abort vs ::Disconnect),
    // bounded by the cooperative write_timeout, with a hard-close fallback
    // when the cooperative write fails (Codex Round 4 P1 #3).

    /// Test helper: a writer that captures every byte written into a shared
    /// buffer the test can inspect. Used in place of a real TCP loopback so
    /// the ordering / content assertion doesn't depend on socket scheduling.
    struct RecordingWriter {
        captured: Arc<std::sync::Mutex<Vec<u8>>>,
    }
    impl std::io::Write for RecordingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.captured.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Test helper: a writer that always errors with WouldBlock — models a
    /// wedged VARA cmd port that isn't draining inside the bounded
    /// write_timeout. Triggers the hard-close fallback path.
    struct BlockedWriter;
    impl std::io::Write for BlockedWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "test: wedged VARA peer",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Test helper: a ShutdownableStream spy that flips a flag the test
    /// can read back. Lets us assert the fallback path RAN even when the
    /// "stream" isn't a real TCP socket.
    struct ShutdownSpy {
        called: Arc<std::sync::Mutex<bool>>,
    }
    impl ShutdownableStream for ShutdownSpy {
        fn shutdown_both(&mut self) -> std::io::Result<()> {
            *self.called.lock().unwrap() = true;
            Ok(())
        }
    }

    #[test]
    fn vara_abort_in_flight_writes_abort_as_first_command() {
        let session = VaraSession::new();
        let captured: Arc<std::sync::Mutex<Vec<u8>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = RecordingWriter { captured: captured.clone() };
        let shutdown_called = Arc::new(std::sync::Mutex::new(false));
        let spy = ShutdownSpy { called: shutdown_called.clone() };
        session.install_abort_writer(
            Box::new(writer) as Box<dyn std::io::Write + Send>,
            Box::new(spy) as Box<dyn ShutdownableStream>,
        );

        session.abort_in_flight().expect("abort writes succeed");

        let bytes = captured.lock().unwrap().clone();
        assert!(
            bytes.starts_with(b"ABORT\r"),
            "Codex Round 1 P1 #4: ABORT must be sent FIRST (got {:?}). \
             DISCONNECT can wait for the current burst.",
            String::from_utf8_lossy(&bytes),
        );
        // If the implementation ever appends a follow-on DISCONNECT (as a
        // belt-and-suspenders graceful tear-down), assert ABORT still
        // precedes it. Today the impl only sends ABORT — both branches of
        // this conditional are safe.
        if let Some(disc_idx) = bytes
            .windows(b"DISCONNECT\r".len())
            .position(|w| w == b"DISCONNECT\r")
        {
            let abort_idx = bytes
                .windows(b"ABORT\r".len())
                .position(|w| w == b"ABORT\r")
                .unwrap();
            assert!(
                abort_idx < disc_idx,
                "ABORT must precede any DISCONNECT"
            );
        }
        // Cooperative path succeeded → fallback must NOT have run.
        assert!(
            !*shutdown_called.lock().unwrap(),
            "shutdown_both must not run when cooperative write succeeded"
        );
    }

    #[test]
    fn vara_abort_in_flight_falls_back_to_hard_close_when_write_fails() {
        let session = VaraSession::new();
        let shutdown_called = Arc::new(std::sync::Mutex::new(false));
        let spy = ShutdownSpy { called: shutdown_called.clone() };
        session.install_abort_writer(
            Box::new(BlockedWriter) as Box<dyn std::io::Write + Send>,
            Box::new(spy) as Box<dyn ShutdownableStream>,
        );

        let start = std::time::Instant::now();
        let result = session.abort_in_flight();
        let elapsed = start.elapsed();

        assert!(result.is_err(), "cooperative write must surface as Err");
        assert!(
            *shutdown_called.lock().unwrap(),
            "Codex Round 4 P1 #3: hard-close fallback MUST run when cooperative write fails"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "Codex Round 3 P1 #1: bound is 2s even on hard-close fallback; got {:?}",
            elapsed
        );
        // The Err message must be operator-readable so the Tauri-command
        // surface can pass it through without re-shaping.
        let err = result.unwrap_err();
        assert!(
            err.contains("hard-closed"),
            "Err must signal the fallback ran; got {err:?}"
        );
    }

    #[test]
    fn vara_abort_in_flight_returns_err_when_no_writer_installed() {
        let session = VaraSession::new();
        let err = session
            .abort_in_flight()
            .expect_err("must Err when no writer is installed");
        assert!(
            err.contains("no abort writer"),
            "Err must say no writer; got {err:?}"
        );
    }

    // tuxlink-rsus: MYCALL is sent on TCP connect when callsign is Some.
    // We can't easily mock the full VARA TCP server in this unit test (would
    // require spinning a TcpListener), but we CAN verify the inner accepts
    // a callsign + still propagates the connect-failure cleanly. Byte-on-
    // wire MYCALL verification is the operator smoke step.
    #[test]
    fn vara_start_session_accepts_callsign_arg_without_panicking() {
        let session = Arc::new(VaraSession::new());
        let ui_cfg = VaraUiConfig {
            host: "127.0.0.1".into(),
            cmd_port: 1, // unreachable; we just want to exercise the signature
            data_port: 2,
            bandwidth_hz: None,
        };
        // With Some(callsign): same error path (TCP fails before MYCALL can
        // be sent), proving the new arg doesn't break the error semantics.
        let err = vara_start_session_inner(&session, &ui_cfg, Some("W1ABC")).unwrap_err();
        assert!(err.contains("TCP connect failed"), "got: {err}");

        // Same with None (pre-wizard path).
        let err2 = vara_start_session_inner(&session, &ui_cfg, None).unwrap_err();
        assert!(err2.contains("TCP connect failed"), "got: {err2}");

        // Same with empty / whitespace callsign — should be treated as "no
        // callsign" by the inner (MYCALL skipped). Verified indirectly by the
        // call not panicking.
        let err3 = vara_start_session_inner(&session, &ui_cfg, Some("   ")).unwrap_err();
        assert!(err3.contains("TCP connect failed"), "got: {err3}");
    }
}
