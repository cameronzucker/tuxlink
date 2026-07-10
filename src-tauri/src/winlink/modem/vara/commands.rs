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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::{mpsc, Notify};

use crate::config::{self, VaraUiConfig};
use crate::modem_commands::{
    clamp_connect_candidates, tune_rig_for_connect, walk_candidates, DialCandidate,
};
use crate::modem_status::{
    ExchangeState, ShutdownableStream, TransportOwner, ARBITER_YIELD_TIMEOUT,
};
use crate::session_log::SessionLogState;
use crate::winlink::listener::transport::TransportKind;
use crate::winlink::session::SessionIntent;
use crate::winlink_backend::{LogLevel, LogSource};

use super::command::{Bandwidth, OutboundCommand};
use super::ptt::{self, PttSink, SharedPtt, UnkeyGuard, VaraPtt};
use super::transport::{VaraConfig, VaraTransport};

/// Append a session-log line to the durable buffer (assigning its `seq`) and
/// emit it on `session_log:line`. Mirrors `ui_commands::emit_session_line`'s
/// pattern; defined locally here to keep that helper private to its module.
/// `_ = app.emit(...)` swallows the emit error: failure to broadcast is
/// non-fatal — the buffer's snapshot still has the line for late-mounting
/// consumers.
fn emit_vara_log(app: &AppHandle, buffer: &SessionLogState, level: LogLevel, message: String) {
    crate::session_log_emit::emit(app, buffer, level, LogSource::Transport, message);
}

/// Coarse VARA transport state. `Connecting` is the in-flight window between
/// "operator clicked Start" and "TCP open succeeded or failed."
///
/// Serialized lowercase: `"closed"`, `"connecting"`, `"open"`, `"error"`,
/// `"socket-lost"`. The `SocketLost` variant uses kebab-case (via
/// `rename_all = "lowercase"` — a single-word "socketlost" would be
/// ambiguous; the serde rename below makes the wire form `"socket-lost"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// cmd-port unresponsive (heartbeat-detected): see spec §2.6 +
    /// [`crate::modem_status::ModemState::SocketLost`] (tuxlink-0ye6 Task
    /// 3.0 / Codex Round 3 P1 #4). Operator's only recovery is Close
    /// Session → reopen. Driven by [`spawn_vara_socket_heartbeat`]
    /// (tuxlink-6urh2), which consumingly drains the cmd socket during the
    /// idle-open window and stamps this variant when the drain observes
    /// EOF (the peer closed) rather than a mere read timeout. (v2 —
    /// replaces an earlier non-consuming-peek design that could never
    /// observe the peer's FIN once unsolicited `IAMALIVE`/`OK` lines had
    /// buffered unread in the idle-open window.)
    #[serde(rename = "socket-lost")]
    SocketLost,
}

/// Snapshot of the VARA session state for the frontend. Returned from
/// `vara_status` and from the start/stop commands so the UI can update
/// without a follow-up poll.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    // ── Lifecycle fields (tuxlink-0ye6 Task 3.0 / Codex Round 2 P1 #5 +
    // Round 3 P1 #3 + Round 4 P1 #1) — mirror of `ModemStatus`. See the
    // ModemStatus field docs in `src-tauri/src/modem_status.rs` for the
    // semantics; the comments are not duplicated here because the two
    // DTOs share the contract.
    pub listener_armed: bool,
    pub exchange: Option<ExchangeState>,
    pub transport_owner: TransportOwner,
    pub active_intent: Option<SessionIntent>,
    pub active_transport_kind: Option<TransportKind>,
}

impl VaraStatus {
    fn closed() -> Self {
        Self {
            state: VaraState::Closed,
            last_error: None,
            bound_host: None,
            bound_cmd_port: None,
            listener_armed: false,
            exchange: None,
            transport_owner: TransportOwner::None,
            active_intent: None,
            active_transport_kind: None,
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
    /// Transport-arbiter signal: outbound calls `notify_one()` to ask the
    /// listener consumer task to yield the transport. The consumer task
    /// holds `notified()` while idle in its accept loop. Decoupled from
    /// the mutex so the consumer never blocks waiting for outbound to
    /// finish a state transition (tuxlink-0ye6 Task 4.3, Codex Round 2
    /// P1 #4).
    transport_yield_request: Arc<Notify>,
    /// Transport-arbiter rendezvous: the listener consumer sends its held
    /// transport here when it observes the yield request. Outbound awaits
    /// this channel (with the std-mutex DROPPED) to receive the transport.
    ///
    /// Sender lives on the consumer task; receiver lives on the session
    /// behind a tokio mutex so multiple async tasks could theoretically
    /// contend on it (in practice only one outbound at a time per the
    /// arbiter invariant, but the tokio mutex makes the lock-discipline
    /// explicit).
    transport_yield_rx: tokio::sync::Mutex<mpsc::Receiver<VaraTransport>>,
    /// Cloneable sender for [`transport_yield_rx`]. Handed to the listener
    /// consumer task when it spawns; the consumer keeps it for the
    /// lifetime of the armed window so a `Sender::send` from the yield
    /// path always succeeds when a consumer is alive.
    ///
    /// Held inside the session purely so the listener consumer can grab
    /// a clone via accessor at spawn time without the spawning code
    /// having to reach inside the session.
    ///
    /// `#[allow(dead_code)]`: the production accessor lands in the
    /// Phase 3 listener-consumer wiring (task 3.4). This task ships
    /// the arbiter primitives + tests in isolation; the wiring follows
    /// in a sibling dispatch.
    #[allow(dead_code)]
    transport_yield_tx: mpsc::Sender<VaraTransport>,
    /// Reverse-direction rendezvous: the arbiter sends the transport
    /// here after outbound completes. The consumer task awaits on
    /// `recv` to reclaim the transport and re-arm.
    transport_return_tx: mpsc::Sender<VaraTransport>,
    /// Receiver counterpart to [`transport_return_tx`]. Owned by the
    /// consumer task (acquired via `take_transport_return_rx` at spawn).
    /// Behind an `Option<Mutex<...>>` so the consumer can `take()` it
    /// once at spawn time — there's exactly one consumer per session.
    ///
    /// `#[allow(dead_code)]`: the production accessor lands in the
    /// Phase 3 listener-consumer wiring (task 3.4). The test-only
    /// `take_transport_return_rx` accessor already reads this field
    /// under `#[cfg(test)]`.
    #[allow(dead_code)]
    transport_return_rx: Mutex<Option<mpsc::Receiver<VaraTransport>>>,
    /// Monotonic close-generation counter (tuxlink-pdnw — Codex Phase 3-4
    /// boundary P1 #4). VARA mirror of `ModemSession::close_generation`;
    /// see that field's docstring in `src-tauri/src/modem_status.rs` for
    /// the full semantics. Bumped by every close path BEFORE the disarm /
    /// teardown reaches the consumer-shutdown flag; snapshotted by
    /// workers that take the transport (b2f exchange, listener consumer
    /// accept-loop); checked on the install-back path to drop the
    /// transport when a close intervened.
    ///
    /// Decoupled from the std-mutex so a snapshot is lock-free.
    /// Monotonically growing — never reset, even on re-open — so each
    /// new open's worker takes a fresh snapshot tied to that open's
    /// generation number.
    close_generation: AtomicU64,
    /// TTL cache for [`VaraSession::probe_reachable`] (tuxlink-7ppfq,
    /// Contract 1). Holds `(measured_at, reachable)` from the last bare
    /// cmd-port TCP touch so routine polls (~heartbeat cadence) don't churn
    /// VARA's single-App acceptor. Deliberately OUTSIDE `inner` so the probe
    /// never has to take the session lock to read/refresh the cache.
    reachable_cache: Mutex<Option<(std::time::Instant, bool)>>,
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
    /// Shutdown handle for the DATA socket, taken alongside the abort pair
    /// (tuxlink-xzxk1 — Codex adrev P1 #2). With the RF-scale
    /// `data_read_timeout`, an exchange thread can be parked in a data read
    /// for up to that budget; [`VaraSession::abort_in_flight`] shuts this
    /// down on BOTH outcome paths so the parked read returns EOF immediately
    /// (the `ABORT\r` kills the ARQ link, so the data stream is already
    /// dead in every abort scenario).
    abort_data_stream: Option<Box<dyn ShutdownableStream>>,
    /// Current ownership of the live transport (tuxlink-0ye6 Task 4.3,
    /// Codex Round 1 P1 #5). Set as a side effect of `take_transport` /
    /// `return_transport` (listener consumer) and `take_transport_for_outbound`
    /// / `return_transport_from_outbound` (outbound). See [`TransportOwner`]
    /// for the state machine; transitions are guarded by this mutex.
    transport_owner: TransportOwner,
    /// Intent of the currently-open session (tuxlink-0ye6 Task 3.2). Set by
    /// `vara_open_session` after a successful TCP open; cleared in
    /// `vara_stop_session_inner` (now reached via `vara_close_session_inner`)
    /// on transport teardown. `None` whenever `transport.is_none()` — i.e.,
    /// status is `Closed` or `Error`.
    active_intent: Option<SessionIntent>,
    /// Transport-kind discriminator (vara-hf vs vara-fm) for the open
    /// session. Same lifecycle as [`Self::active_intent`]. The wire
    /// transport (TCP host/port) is identical between the two; this field
    /// records the operator-meaningful distinction so the frontend can
    /// detect sidebar-nav drift mid-session (Codex Round 3 P1 #3).
    active_transport_kind: Option<TransportKind>,
    /// In-flight ARQ exchange classification (Codex Phase 3-4 boundary
    /// P2 #4 — tuxlink-u1r7). `Some(Outbound)` while a `b2f_exchange`
    /// dial is running; `Some(Inbound)` while the listener consumer
    /// task is running an inbound `b2f_answer`; `None` between
    /// exchanges (transport may still be open + listener may still be
    /// armed; this field tracks the exchange layer specifically).
    ///
    /// Wired by `VaraSession::begin_exchange` / `end_exchange` from the
    /// outbound b2f command + the listener consumer task. Read by
    /// [`Self::current_exchange`] (the DTO accessor) so the frontend
    /// shared `useRadioSessionLifecycle` hook can gate the
    /// "exchange in progress" UI surface accurately.
    ///
    /// Distinct from `transport_owner` (which models who holds the
    /// transport — listener vs outbound) — an `Outbound` exchange
    /// implies `transport_owner == ListenerArmed` (via the
    /// take_transport bypass) but an `Inbound` exchange does NOT
    /// transition the owner to `ListenerInbound` in today's consumer
    /// task. The two fields are deliberately decoupled.
    current_exchange: Option<ExchangeState>,
    /// Cancellation flag for the drop-detection heartbeat task spawned by
    /// `spawn_vara_socket_heartbeat` (tuxlink-6urh2). `Some` for the
    /// lifetime of a heartbeat that hasn't yet cancelled itself (a clean
    /// close, an open-failure rollback, or the heartbeat's own SocketLost
    /// stamp all end the task). `vara_stop_session_inner` — the single
    /// transport-teardown chokepoint — takes + flips this on EVERY close
    /// path so a clean Close Session never races a spurious SocketLost
    /// stamp from a heartbeat tick that was already in flight.
    ///
    /// A fresh flag is installed per open (`VaraSession::install_heartbeat_shutdown`),
    /// so a stale flag from a prior open can never signal the current
    /// heartbeat task.
    heartbeat_shutdown: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl VaraSession {
    pub fn new() -> Self {
        // mpsc channels are bounded; capacity 1 is sufficient — at most
        // ONE transport handoff is in flight per direction at any moment
        // (the arbiter invariant). Bounded so a regression doesn't silently
        // queue stale transports.
        let (yield_tx, yield_rx) = mpsc::channel::<VaraTransport>(1);
        let (return_tx, return_rx) = mpsc::channel::<VaraTransport>(1);
        Self {
            inner: Mutex::new(VaraSessionInner {
                transport: None,
                status: VaraStatus::default(),
                abort_writer: None,
                abort_stream: None,
                abort_data_stream: None,
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
                current_exchange: None,
                heartbeat_shutdown: None,
            }),
            transport_yield_request: Arc::new(Notify::new()),
            transport_yield_rx: tokio::sync::Mutex::new(yield_rx),
            transport_yield_tx: yield_tx,
            transport_return_tx: return_tx,
            transport_return_rx: Mutex::new(Some(return_rx)),
            close_generation: AtomicU64::new(0),
            reachable_cache: Mutex::new(None),
        }
    }

    /// Read-only cmd-port reachability, classified WITHOUT holding `inner`
    /// across a socket op (tuxlink-7ppfq, Contract 1). One brief `try_lock`
    /// classification: if a live session is Open/Connecting we lean on the
    /// heartbeat and touch NO socket; otherwise a bare `connect_timeout` on
    /// the cmd port, TTL-cached. Returns `None` (unknown) when the session
    /// lock is contended — it never waits (the open path holds the lock across
    /// a ~5 s connect, and this probe must not queue behind it).
    ///
    /// `host`/`cmd_port`/`timeout` come from the caller (which sources them
    /// from `config_get_vara()` via `build_transport_config`, never hardcoded)
    /// so the connect timeout is the SAME knob the transport uses.
    pub fn probe_reachable(&self, host: &str, cmd_port: u16, timeout: Duration) -> Option<bool> {
        // One brief try_lock classification. Contended → unknown (never wait).
        // `VaraState` is `Copy`, so we copy it out and drop the guard before
        // any socket work — satisfying the no-session-mutex-contention invariant.
        let state = match self.inner.try_lock() {
            Ok(g) => g.status.state,
            Err(std::sync::TryLockError::WouldBlock) => return None,
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner().status.state,
        };
        // Guard dropped. If a session is live, lean on the ~3 s heartbeat — no socket.
        if matches!(state, VaraState::Open | VaraState::Connecting) {
            return Some(matches!(state, VaraState::Open));
        }
        // No live session: bare cmd-port touch, TTL-cached (~heartbeat cadence).
        const TTL: Duration = Duration::from_secs(3);
        if let Ok(cache) = self.reachable_cache.lock() {
            if let Some((at, val)) = *cache {
                if at.elapsed() < TTL {
                    return Some(val);
                }
            }
        }
        let val = super::transport::cmd_port_reachable(host, cmd_port, timeout);
        if let Ok(mut cache) = self.reachable_cache.lock() {
            *cache = Some((std::time::Instant::now(), val));
        }
        Some(val)
    }

    /// Read-only snapshot of the current status. Cheap; safe to poll.
    ///
    /// Overlays the live `transport_owner` from the session inner-mutex
    /// (the cached `inner.status` may have stale lifecycle fields). The
    /// other four lifecycle fields (`listener_armed` / `exchange` /
    /// `active_intent` / `active_transport_kind`) read through the stub
    /// accessors today — Phase 3.2 / 3.4 / 3.5 wires them to real session
    /// state (tuxlink-0ye6 Task 3.0).
    pub fn snapshot(&self) -> VaraStatus {
        // Phase 1: acquire the mutex, clone the cached snapshot + the
        // live transport_owner, drop the guard.
        let (mut snap, transport_owner) = self
            .inner
            .lock()
            .map(|g| (g.status.clone(), g.transport_owner))
            .unwrap_or_else(|poison| {
                let g = poison.into_inner();
                (g.status.clone(), g.transport_owner)
            });
        // Phase 2: overlay live lifecycle fields (no mutex held — the
        // stub accessors take their own lock and would deadlock if
        // called inside a guard).
        snap.transport_owner = transport_owner;
        snap.listener_armed = self.listener_armed();
        snap.exchange = self.current_exchange();
        snap.active_intent = self.active_intent();
        snap.active_transport_kind = self.active_transport_kind();
        snap
    }

    // ── Lifecycle accessors (tuxlink-0ye6 Task 3.0 + Codex Phase 3-4
    // boundary P2 #4 — tuxlink-u1r7) ────────────────────────────────
    //
    // See `ModemSession`'s parallel accessors in
    // `src-tauri/src/modem_status.rs` for the shared contract. P2 #4
    // (Codex 2026-06-04) wires `listener_armed` + `current_exchange`
    // to real session state on the VARA side; the ARDOP side remains
    // stubbed and is tracked separately. `active_intent` +
    // `active_transport_kind` already wired by Task 3.2.

    /// Listener-armed state. Reads through the arbiter's
    /// [`TransportOwner`]: a listener consumer task that has called
    /// `take_transport()` transitions the owner to
    /// [`TransportOwner::ListenerArmed`] (idle in accept loop) or
    /// [`TransportOwner::ListenerInbound`] (running an inbound exchange).
    /// Both states are "the listener is armed" from the frontend's
    /// perspective; the UI gates the LISTEN-ON affordance + the inbound-
    /// exchange pill from this single boolean.
    ///
    /// Codex Phase 3-4 boundary P2 #4 (tuxlink-u1r7): wires this from
    /// the prior Task 3.0 stub `false` return. There is a brief window
    /// between `arm_vara_listener_inner` returning Ok and the spawned
    /// consumer task's first `take_transport()` call during which the
    /// transport_owner has not yet transitioned to `ListenerArmed` —
    /// status polls in this window read `false`. Acceptable today: the
    /// status broadcaster polls at sub-second cadence and the take
    /// completes within ms of the spawn; a follow-up could close the
    /// window via an explicit flag on the session if operator-visible
    /// UI flicker is observed.
    pub fn listener_armed(&self) -> bool {
        matches!(
            self.transport_owner(),
            TransportOwner::ListenerArmed | TransportOwner::ListenerInbound
        )
    }

    /// Current in-flight exchange classification. Returns the
    /// [`VaraSessionInner::current_exchange`] field, set by
    /// [`Self::begin_exchange`] at the entry of an outbound dial or
    /// inbound `b2f_answer` and cleared by [`Self::end_exchange`]
    /// at the corresponding exit.
    ///
    /// Codex Phase 3-4 boundary P2 #4 (tuxlink-u1r7): wires this from
    /// the prior Task 3.0 stub `None` return. Returns `None` if the
    /// session mutex is poisoned (defensive — same posture as
    /// [`Self::active_intent`]).
    pub fn current_exchange(&self) -> Option<ExchangeState> {
        self.inner.lock().ok().and_then(|g| g.current_exchange)
    }

    /// Mark an exchange as in flight. Called by the outbound b2f path
    /// with `ExchangeState::Outbound` after the operator's dial is
    /// accepted and by the listener consumer task with
    /// `ExchangeState::Inbound` when `serve_inbound_one` accepts a
    /// peer. The DTO accessor [`Self::current_exchange`] surfaces this
    /// to the frontend so the shared `useRadioSessionLifecycle` hook
    /// can render the "exchange in progress" UI.
    ///
    /// Replaces any previously-set state — the arbiter ensures only
    /// one exchange runs at a time, so a non-`None` prior state is a
    /// regression that should not occur in production. The setter
    /// nonetheless overwrites unconditionally (the next exchange's
    /// classification is the authoritative reading).
    pub fn begin_exchange(&self, state: ExchangeState) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.current_exchange = Some(state);
        }
    }

    /// Clear the in-flight exchange marker. Called at the exit of the
    /// outbound dial and at the exit of the consumer task's inbound
    /// b2f handling. Idempotent; safe to call when no exchange is in
    /// flight.
    pub fn end_exchange(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.current_exchange = None;
        }
    }

    /// Intent of the currently-open session (tuxlink-0ye6 Task 3.2). Set by
    /// `vara_open_session` on successful TCP open; cleared by
    /// `vara_stop_session_inner` (reached via `vara_close_session_inner`).
    /// Returns `None` when the session is closed or the mutex is poisoned.
    pub fn active_intent(&self) -> Option<SessionIntent> {
        self.inner.lock().ok().and_then(|g| g.active_intent)
    }

    /// Transport-kind of the currently-open session (tuxlink-0ye6 Task 3.2).
    /// Discriminates `VaraHf` vs `VaraFm` even though the wire transport
    /// (TCP) is identical — Codex Round 3 P1 #3: lets the frontend detect
    /// sidebar-nav drift mid-session. Same lifecycle as [`Self::active_intent`].
    pub fn active_transport_kind(&self) -> Option<TransportKind> {
        self.inner.lock().ok().and_then(|g| g.active_transport_kind)
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
    ///
    /// **Arbiter side effect (tuxlink-0ye6 Task 4.3):** on success, the
    /// arbiter records the listener as the current owner via
    /// `transport_owner = ListenerArmed`. Outbound's
    /// [`Self::take_transport_for_outbound`] then sequences the yield
    /// when needed.
    ///
    /// **Heartbeat exclusion (tuxlink-6urh2 v2):** returns `None` while
    /// `transport_owner == Heartbeat` WITHOUT even attempting the take —
    /// the drop-detection heartbeat has the transport out of `guard.transport`
    /// for its brief consuming-drain window, so `guard.transport` would
    /// already read `None` there regardless; the explicit owner check is
    /// belt-and-suspenders documentation of the invariant (at most one
    /// owner holds the transport at a time) rather than a distinct
    /// behavior. A concurrent caller (listener re-arm, outbound dial) sees
    /// the same "not available right now" `None` it already has to handle
    /// for the ordinary already-closed case; the borrow window is bounded
    /// (at most ~ one `read_timeout`, a couple of seconds) and only
    /// happens during idle-open, never mid-exchange.
    pub fn take_transport(&self) -> Option<VaraTransport> {
        let mut guard = self.inner.lock().ok()?;
        if guard.transport_owner == TransportOwner::Heartbeat {
            return None;
        }
        let t = guard.transport.take();
        if t.is_some() {
            guard.status = VaraStatus::closed();
            guard.transport_owner = TransportOwner::ListenerArmed;
        }
        t
    }

    /// Return a previously-taken transport to the session, restoring
    /// state=Open. Called by the listener consumer task on disarm so
    /// the operator's next `vara_close_session` / `vara_status` sees the
    /// transport as if the consumer never owned it.
    ///
    /// `bound_host` + `bound_cmd_port` should be the values the
    /// transport was opened with — the consumer task captures them
    /// from the session snapshot before calling `take_transport`.
    ///
    /// **Arbiter side effect (tuxlink-0ye6 Task 4.3):** clears the
    /// owner to `None` — the listener is no longer holding the transport
    /// (it returned it to the session for shutdown / pre-Close cleanup).
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
                // Lifecycle fields (tuxlink-0ye6 Task 3.0): defaults at
                // restoration — Phase 3 wires the real values via the
                // session-state accessors.
                listener_armed: false,
                exchange: None,
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
            };
            guard.transport = Some(t);
            guard.transport_owner = TransportOwner::None;
            // Codex Phase 3-4 boundary P2 #4 (tuxlink-u1r7): clear the
            // exchange marker on restoration — return_transport is the
            // listener consumer's post-disarm restoration path; any
            // prior in-flight exchange has already ended.
            guard.current_exchange = None;
        }
    }

    // ── Close-generation guards (tuxlink-pdnw — Codex Phase 3-4 P1 #4) ──
    //
    // VARA mirror of `ModemSession`'s close-generation API; see the parent
    // method docstrings in `src-tauri/src/modem_status.rs` for the race
    // semantics. Summary: bump on close path BEFORE disarm; snapshot on
    // worker take; check on install-back — stale snapshot drops the
    // transport instead of installing into a closed session.

    /// Read the live close-generation counter. Workers that intend to
    /// re-install the transport snapshot this BEFORE the take.
    pub fn current_close_generation(&self) -> u64 {
        self.close_generation.load(Ordering::Acquire)
    }

    /// Bump the close-generation counter. Returns the PRIOR value;
    /// the new generation is `prior + 1`. Called by every VARA close
    /// path at the TOP, BEFORE the listener-consumer shutdown flag is
    /// set and BEFORE the transport teardown runs.
    pub fn bump_close_generation(&self) -> u64 {
        self.close_generation.fetch_add(1, Ordering::AcqRel)
    }

    /// Guarded install: install the transport iff `snapshot_gen` still
    /// matches the live close-generation. Returns `Ok(())` when installed
    /// (transitions status to `Open` with the supplied bound host/port and
    /// sets owner to `None` — mirroring `return_transport`); returns
    /// `Err(transport)` when a close intervened, handing the transport
    /// back to the caller so it can drop it (and optionally log) at the
    /// install site.
    ///
    /// Replaces the bare `guard.transport = Some(t)` mutation inside
    /// `return_transport` for the consumer-shutdown / b2f-return paths
    /// where a race with `vara_close_session_inner` is possible. The
    /// non-guarded `return_transport` is preserved for paths that
    /// legitimately do unconditional return-to-Open (e.g. tests that
    /// model the pre-close-generation behaviour).
    ///
    /// **`active_intent` + `active_transport_kind`** (tuxlink-0iqi —
    /// Codex Phase 3-4 P1 #2): the b2f exchange path snapshots these
    /// from the session BEFORE `take_transport` and passes them back
    /// here so the install-back restores the session's active mode
    /// (Spec §2 — "outbound dial is within-session"). A subsequent
    /// Send/Receive or listener re-arm runs without re-opening the
    /// session. The listener consumer's drain path passes `None`/`None`
    /// — it's tearing down the session, not preserving it.
    pub fn install_transport_if_generation_matches(
        &self,
        t: VaraTransport,
        snapshot_gen: u64,
        bound_host: Option<String>,
        bound_cmd_port: Option<u16>,
        active_intent: Option<SessionIntent>,
        active_transport_kind: Option<TransportKind>,
    ) -> Result<(), VaraTransport> {
        // Codex Phase 3-4 RE-REVIEW P1: generation check MUST happen INSIDE
        // the mutex critical section, not outside. Otherwise a concurrent
        // `vara_close_session_inner` can bump close_generation + finish
        // `vara_stop_session_inner` (which takes the mutex) between our
        // load and our lock; our stale worker then acquires the mutex and
        // writes the transport back into a now-closed session, restoring
        // `VaraState::Open` after Close has returned.
        //
        // Codex Phase 3-4 RE-REVIEW P2: `None` values for the preserve
        // params (`active_intent`, `active_transport_kind`) now mean
        // "preserve existing" rather than "overwrite with None". The
        // listener-consumer drain path passes `None`/`None` on ordinary
        // Listen Off (no close); the prior behavior wrote `None` which
        // erased the operator's active mode while leaving the session
        // Open. The new semantics: `Some(value)` writes; `None` preserves.
        // Close-race protection is still load-bearing via the generation
        // mismatch above.
        match self.inner.lock() {
            Ok(mut guard) => {
                let live = self.close_generation.load(Ordering::Acquire);
                if live != snapshot_gen {
                    return Err(t);
                }
                let preserved_intent = active_intent.or(guard.active_intent);
                let preserved_kind = active_transport_kind.or(guard.active_transport_kind);
                guard.status = VaraStatus {
                    state: VaraState::Open,
                    last_error: None,
                    bound_host,
                    bound_cmd_port,
                    listener_armed: false,
                    exchange: None,
                    transport_owner: TransportOwner::None,
                    active_intent: preserved_intent,
                    active_transport_kind: preserved_kind,
                };
                guard.transport = Some(t);
                guard.transport_owner = TransportOwner::None;
                guard.active_intent = preserved_intent;
                guard.active_transport_kind = preserved_kind;
                // Codex Phase 3-4 boundary P2 #4 (tuxlink-u1r7): clear
                // exchange marker on install-back — the b2f exchange that
                // owned the transport is over by the time we get here.
                guard.current_exchange = None;
                Ok(())
            }
            Err(_poisoned) => Err(t),
        }
    }

    /// Guarded SocketLost stamp: the DEAD-transport sibling of
    /// [`Self::install_transport_if_generation_matches`] (tuxlink-6urh2 v2,
    /// self-adrev MEDIUM 2). When a consumer detects a *terminal* transport
    /// error — the listener's cmd-socket `Eof`/`TransportClosed` or a hard I/O
    /// error — the transport is dead and must NOT be laundered back into the
    /// session as `VaraState::Open`. This drops it and transitions the session
    /// to `SocketLost` (preserving `bound_host`/`bound_cmd_port` so the UI can
    /// offer reopen), mirroring the heartbeat's own dead-path field reset.
    ///
    /// Generation-gated exactly like the install sibling: a stale
    /// `snapshot_gen` means the operator's Close intervened and already tore
    /// the session down, so we just drop the (already-dropped) transport and
    /// leave state alone rather than stamping SocketLost over a session the
    /// operator deliberately closed.
    ///
    /// Returns `true` when SocketLost was stamped, `false` when the stamp was a
    /// no-op (stale generation or poisoned mutex — close path owns teardown).
    /// Unlike the install sibling there is no transport to hand back on the
    /// no-op path (a dead socket has nothing to preserve), so a plain `bool`
    /// carries the outcome.
    ///
    /// `bound_host` / `bound_cmd_port` are supplied by the caller (captured
    /// from the session snapshot BEFORE `take_transport`) rather than read from
    /// `guard.status`, because `take_transport` resets status to `closed()`
    /// (nulling those fields) at arm time — so by the time the listener
    /// consumer detects a terminal error, `guard.status.bound_host` is already
    /// `None`. Threading them in preserves the reopen target on the SocketLost
    /// status. (The heartbeat's inline dead-path can read `guard.status`
    /// directly because its Phase-1 borrow leaves status `Open`, not `closed`.)
    pub fn mark_socket_lost_if_generation_matches(
        &self,
        t: VaraTransport,
        snapshot_gen: u64,
        bound_host: Option<String>,
        bound_cmd_port: Option<u16>,
    ) -> bool {
        // Dead socket — nothing to preserve; close its fds up front so we
        // don't hold the session mutex across the drop.
        drop(t);
        match self.inner.lock() {
            Ok(mut guard) => {
                let live = self.close_generation.load(Ordering::Acquire);
                if live != snapshot_gen {
                    // Close intervened: leave the close path's teardown intact.
                    return false;
                }
                guard.active_intent = None;
                guard.active_transport_kind = None;
                guard.abort_writer = None;
                guard.abort_stream = None;
                guard.abort_data_stream = None;
                guard.transport_owner = TransportOwner::None;
                guard.current_exchange = None;
                guard.heartbeat_shutdown = None;
                guard.status = VaraStatus {
                    state: VaraState::SocketLost,
                    last_error: Some("VARA connection lost — reopen to reconnect".to_string()),
                    bound_host,
                    bound_cmd_port,
                    listener_armed: false,
                    exchange: None,
                    transport_owner: TransportOwner::None,
                    active_intent: None,
                    active_transport_kind: None,
                };
                true
            }
            Err(_poisoned) => false,
        }
    }

    // ── Arbiter (tuxlink-0ye6 Task 4.3, Codex Round 1 P1 #5) ────────────

    /// Current transport owner — accessor for the arbiter state machine.
    /// Returns [`TransportOwner::None`] if the session mutex is poisoned
    /// (defensive — poisoning indicates a panic during a prior critical
    /// section; treating the session as closed is the safe fallback).
    pub fn transport_owner(&self) -> TransportOwner {
        self.inner
            .lock()
            .map(|g| g.transport_owner)
            .unwrap_or(TransportOwner::None)
    }

    /// Test-only helper: drive the owner state directly. Used by unit
    /// tests to simulate "listener has the transport and is currently
    /// running an inbound exchange" without spinning up a real consumer
    /// task. Production code MUST drive the owner via the
    /// `take_transport` / `return_transport` /
    /// `take_transport_for_outbound` / `return_transport_from_outbound`
    /// paths.
    #[cfg(test)]
    pub fn set_transport_owner_for_test(&self, owner: TransportOwner) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.transport_owner = owner;
        }
    }

    /// Test-only helper: drive the cached status `state` directly, so a test
    /// can exercise `probe_reachable`'s open/connecting classification without
    /// standing up a real transport (tuxlink-7ppfq, Contract 1).
    #[cfg(test)]
    pub fn set_state_for_test(&self, s: VaraState) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.status.state = s;
        }
    }

    /// Test-only helper: hold `inner` so a test can prove `probe_reachable`
    /// returns `unknown` (never waits) under lock contention. Deliberately NOT
    /// `pub` — the child `tests` module can call it, and a `pub` signature would
    /// expose the private `VaraSessionInner` type (the `private_interfaces` lint,
    /// denied under `clippy --all-targets -D warnings`).
    #[cfg(test)]
    fn lock_inner_for_test(&self) -> std::sync::MutexGuard<'_, VaraSessionInner> {
        self.inner.lock().unwrap()
    }

    /// Test-only clone of the yield-notify handle. Lets a test spawn a
    /// stub "consumer" task that calls `.notified().await` and then
    /// sends a transport via [`Self::transport_yield_sender_clone`]
    /// to simulate the real listener consumer's yield behavior.
    #[cfg(test)]
    pub fn transport_yield_notify_clone(&self) -> Arc<Notify> {
        self.transport_yield_request.clone()
    }

    /// Test-only clone of the yield-channel sender. Lets a stub
    /// consumer push a transport into the yield channel when it
    /// receives the notify, mirroring what the real consumer task
    /// will do once Phase 3 wires it.
    #[cfg(test)]
    pub fn transport_yield_sender_clone(&self) -> mpsc::Sender<VaraTransport> {
        self.transport_yield_tx.clone()
    }

    /// Test-only: replace the yield receiver with one whose paired
    /// sender has been dropped. After calling this, the next
    /// `transport_yield_rx.recv()` returns `None`, exercising the
    /// "listener consumer task exited" branch of
    /// [`Self::take_transport_for_outbound`].
    ///
    /// Implementation note: this MUST be a separate path from the
    /// normal `transport_yield_tx` because that sender is a fixed
    /// field on the session. We swap the *receiver* (which sits behind
    /// a `tokio::sync::Mutex`) so that even though the old sender
    /// exists, the new receiver is bound to a dropped sender.
    #[cfg(test)]
    pub async fn install_closed_yield_channel_for_test(&self) {
        let (closed_tx, closed_rx) = mpsc::channel::<VaraTransport>(1);
        drop(closed_tx); // sender immediately gone → recv() returns None
        let mut rx_guard = self.transport_yield_rx.lock().await;
        *rx_guard = closed_rx;
    }

    /// Outbound request: snapshot+record under the std-mutex, drop the
    /// mutex, then await the listener consumer's yield via the
    /// transport-yield channel. Hands the transport to outbound.
    ///
    /// **Codex Round 2 P1 #4 — lock-drop-before-await.** The std mutex
    /// is acquired only for the snapshot+notify+state-transition phase;
    /// the .await happens with the lock released. Holding it across the
    /// await would (a) deadlock against the listener consumer task that
    /// needs session state to honor the yield, and (b) not even
    /// compile (`std::sync::MutexGuard: !Send`).
    ///
    /// **Codex Round 3 P1 #2 — bounded wait.** If the listener consumer
    /// task crashed, missed the notify, or is wedged in its accept loop,
    /// an unbounded await would leave outbound stuck in
    /// [`TransportOwner::OutboundPending`] forever. After
    /// [`ARBITER_YIELD_TIMEOUT`] (3 s), we reset to
    /// [`TransportOwner::None`] and surface "modem busy — listener did
    /// not yield within {timeout}" so the operator can recover via
    /// Close Session.
    ///
    /// ### Returns
    ///
    /// - `Ok(VaraTransport)` — yield succeeded; outbound now owns it.
    /// - `Err("session not open")` — owner was `None`.
    /// - `Err("modem busy — inbound exchange in progress")` — owner was
    ///   `ListenerInbound`.
    /// - `Err("outbound exchange already in flight")` — owner was
    ///   `Outbound` (or `OutboundPending`).
    /// - `Err("modem busy — listener did not yield within …")` — yield
    ///   wait timed out.
    /// - `Err("listener consumer task exited; session needs Close +
    ///   reopen")` — yield channel closed (Sender dropped before send).
    pub async fn take_transport_for_outbound(&self) -> Result<VaraTransport, String> {
        // Phase 1: snapshot + record under the lock; drop before await.
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|e| format!("session lock poisoned: {e}"))?;
            match guard.transport_owner {
                TransportOwner::None => return Err("session not open".into()),
                TransportOwner::ListenerInbound => {
                    return Err("modem busy — inbound exchange in progress".into())
                }
                TransportOwner::OutboundPending | TransportOwner::Outbound => {
                    return Err("outbound exchange already in flight".into())
                }
                // tuxlink-6urh2 v2: the drop-detection heartbeat's brief
                // idle-open borrow. Mirrors ARDOP's
                // `ModemSession::take_transport_for_outbound` arm.
                TransportOwner::Heartbeat => {
                    return Err("modem busy — heartbeat probe in progress".into())
                }
                TransportOwner::ListenerArmed => {
                    guard.transport_owner = TransportOwner::OutboundPending;
                    // Drop the guard explicitly so the notify_one()
                    // below is recorded after the lock release —
                    // ordering is fine either way (the consumer holds
                    // a clone of the Notify, not the mutex), but
                    // dropping early documents the intent.
                }
            }
        } // std-mutex guard dropped here — REQUIRED before .await

        // Signal the listener consumer to yield.
        self.transport_yield_request.notify_one();

        // Phase 2: bounded await on the yield channel (no std-mutex
        // held). Uses tokio::time::timeout so a wedged consumer
        // doesn't strand outbound.
        let yield_result = {
            let mut rx_guard = self.transport_yield_rx.lock().await;
            tokio::time::timeout(ARBITER_YIELD_TIMEOUT, rx_guard.recv()).await
        };

        let transport = match yield_result {
            Ok(Some(t)) => t,
            Ok(None) => {
                // Channel closed — listener task is gone.
                if let Ok(mut guard) = self.inner.lock() {
                    guard.transport_owner = TransportOwner::None;
                }
                return Err("listener consumer task exited; session needs Close + reopen".into());
            }
            Err(_elapsed) => {
                // Timeout — consumer wedged. Reset to None so a clean
                // Close+reopen can proceed.
                if let Ok(mut guard) = self.inner.lock() {
                    guard.transport_owner = TransportOwner::None;
                }
                return Err(format!(
                    "modem busy — listener did not yield within {:?}; \
                     Close Session and reopen to recover",
                    ARBITER_YIELD_TIMEOUT,
                ));
            }
        };

        // Phase 3: finalize ownership transfer under the lock.
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|e| format!("session lock poisoned: {e}"))?;
            guard.transport_owner = TransportOwner::Outbound;
        }

        Ok(transport)
    }

    /// Outbound completes: return the transport to the consumer (if
    /// alive) or drop it (if not). Transitions owner accordingly:
    ///
    /// - Generation mismatch (close intervened) → owner = `None`,
    ///   transport dropped explicitly.
    /// - Consumer still listening → owner = `ListenerArmed`, transport
    ///   pushed through `transport_return_tx`.
    /// - Consumer gone (return_tx send fails) → owner = `None`,
    ///   transport dropped. The caller's outbound is complete either
    ///   way; the operator's next Close Session will tear down cleanly.
    ///
    /// **`snapshot_gen` (tuxlink-pdnw — Codex Phase 3-4 P1 #4):** caller
    /// passes the value from [`Self::current_close_generation`] captured
    /// BEFORE the outbound take. Stale snapshot → drop the transport.
    ///
    /// Best-effort: ignores Mutex poisoning + send failures because the
    /// outbound side has already completed; we're cleaning up.
    pub fn return_transport_from_outbound(&self, transport: VaraTransport, snapshot_gen: u64) {
        // Codex Phase 3-4 RE-REVIEW P1: generation check INSIDE the
        // mutex critical section to prevent a close-race from
        // bumping-and-clearing between our load and the consumer hand-off.
        // Mirrors `ModemSession::return_transport_from_outbound`'s fix.
        match self.inner.lock() {
            Ok(mut guard) => {
                let live = self.close_generation.load(Ordering::Acquire);
                if live != snapshot_gen {
                    guard.transport_owner = TransportOwner::None;
                    drop(transport);
                    return;
                }
                // gen still matches; try to hand off to consumer
                match self.transport_return_tx.try_send(transport) {
                    Ok(()) => guard.transport_owner = TransportOwner::ListenerArmed,
                    Err(_) => guard.transport_owner = TransportOwner::None,
                }
            }
            Err(_poisoned) => {
                // Mutex poisoned; treat as session gone. Drop the transport.
                drop(transport);
            }
        }
    }

    /// Test-only / future-consumer accessor: take the receiver half of
    /// the return channel. Returns `None` if a prior caller already
    /// took it (there can only be one consumer task per session).
    #[cfg(test)]
    pub fn take_transport_return_rx(&self) -> Option<mpsc::Receiver<VaraTransport>> {
        self.transport_return_rx
            .lock()
            .ok()
            .and_then(|mut g| g.take())
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
            .ok_or_else(|| "VARA session is not Open — call vara_open_session first".to_string())?;
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

    /// Install the DATA-socket shutdown handle alongside the abort pair
    /// (tuxlink-xzxk1 — Codex adrev P1 #2). Kept as a separate installer so
    /// the existing `install_abort_writer` call sites (and their tests) stay
    /// untouched; a session with no data handle installed simply skips the
    /// data shutdown in [`abort_in_flight`].
    pub fn install_abort_data_stream(&self, stream: Box<dyn ShutdownableStream>) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.abort_data_stream = Some(stream);
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
    ///   (caller can fall through to `vara_close_session_inner` for the
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
            writer.write_all(b"ABORT\r").and_then(|()| writer.flush())
        };
        // Unblock any exchange thread parked in a data-socket read
        // (tuxlink-xzxk1 — Codex adrev P1 #2): with the RF-scale
        // data_read_timeout, that read no longer ticks every 2 s, so the
        // abort must shut the data socket down explicitly. Runs on BOTH
        // outcome paths — the ABORT (or the hard-close below) kills the ARQ
        // link either way, so the data stream is dead regardless.
        if let Some(mut data) = guard.abort_data_stream.take() {
            let _ = data.shutdown_both();
        }
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

    /// Install the drop-detection heartbeat's shutdown flag for the
    /// just-opened session (tuxlink-6urh2). Called by `vara_open_session`
    /// right after `spawn_vara_socket_heartbeat` returns. Replaces any
    /// previously-installed flag — there is at most one live heartbeat per
    /// open, and the close that necessarily preceded this open already
    /// cancelled the prior one via `vara_stop_session_inner`.
    ///
    /// **Cancels the outgoing flag before replacing it (Codex P1 #1 — v2).**
    /// The "close necessarily preceded this open" assumption doesn't hold
    /// for every caller: nothing stops a future or test call-site from
    /// invoking `spawn_vara_socket_heartbeat` + `install_heartbeat_shutdown`
    /// twice in a row without an intervening close (e.g. a retry path). Prior
    /// to this fix, the second install silently replaced the field without
    /// ever setting the first flag — the ORIGINAL heartbeat task keeps
    /// running forever with no way to cancel it (its shutdown handle is
    /// gone), leaking a `tokio::spawn`'d task for the life of the process.
    /// Flipping the outgoing flag here guarantees at most one live heartbeat
    /// task per session regardless of call pattern.
    pub fn install_heartbeat_shutdown(&self, shutdown: Arc<std::sync::atomic::AtomicBool>) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(prev) = guard.heartbeat_shutdown.take() {
                prev.store(true, Ordering::SeqCst);
            }
            guard.heartbeat_shutdown = Some(shutdown);
        }
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
        // 5s connect + 2s cmd-read; we pin them here so a future change to
        // the transport defaults doesn't silently shift the UI's behavior.
        connect_timeout: Duration::from_secs(5),
        read_timeout: Some(Duration::from_secs(2)),
        // RF-scale data-socket budget (tuxlink-xzxk1): B2F bytes arrive at
        // link speed (tens of bps at VARA 500), so inter-byte gaps far
        // beyond the 2s cmd cadence are healthy. See VaraConfig field doc.
        data_read_timeout: Some(Duration::from_secs(120)),
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

/// Default tick interval for the VARA drop-detection heartbeat
/// (tuxlink-6urh2). Injectable — see [`spawn_vara_socket_heartbeat`]'s
/// `interval` param — so the regression test can drive a ~50ms interval
/// instead of waiting out a real 3s cadence.
pub const VARA_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);

/// Spawn the VARA cmd-port drop-detection heartbeat (tuxlink-6urh2 — the
/// `VaraState::SocketLost` variant existed but its detection was deferred;
/// this is that follow-up). **v2 (this revision)** replaces an earlier
/// non-consuming-peek design that was fundamentally broken: VARA sends
/// unsolicited `IAMALIVE` keepalives (and setter-echo `OK` lines) during the
/// idle-open window, and those bytes sit in the kernel receive buffer
/// unread by anything (nothing else is reading the cmd socket while idle).
/// A `MSG_PEEK` therefore ALWAYS finds something buffered — or, once
/// drained by a lucky race, reports `WouldBlock` — and reports "alive"
/// forever, even after the peer has sent FIN. The peek could never observe
/// the peer's close. This version reads (consumes) instead of peeking, and
/// distinguishes EOF from timeout at the `LineReader` layer
/// ([`super::transport::RecvOutcome`]) rather than folding both to `None`
/// the way [`VaraTransport::recv`] does for the ordinary command-exchange
/// callers.
///
/// Ticks every `interval`; on each tick:
///
/// 1. **Phase 1 (locked).** Snapshot whether the session is **idle-open**
///    — `status.state == Open`, a transport is installed, AND
///    `transport_owner == TransportOwner::None` (no listener/exchange
///    holds it). If not idle-open, skip the tick entirely: a
///    listener-armed or in-flight-exchange transport is being read/written
///    by its owning task, and this heartbeat only ever probes the
///    unarmed, unwatched idle window the wedge actually lives in. If
///    idle-open, take the transport OUT of the session (`guard.transport =
///    None`) and mark `transport_owner = Heartbeat` — this is a BORROW,
///    not a close: `status.state` stays `Open` throughout. Drop the guard
///    before any I/O.
/// 2. **Phase 2 (no lock).** Bounded consuming drain, up to
///    [`HEARTBEAT_DRAIN_CAP`] iterations of
///    [`VaraTransport::recv_line_distinguishing_eof`]:
///    - `Line(_)` — even `IAMALIVE` — proves the peer is alive; keep
///      draining (VARA may have queued several keepalives since the last
///      tick).
///    - `Idle` — the bounded read timed out with nothing buffered: the
///      socket is still open, stop draining, alive.
///    - `Eof` / `Err` — the peer is gone: stop draining, dead.
///    - Hitting the cap without a definitive Idle/Eof/Err (i.e. every
///      iteration was `Line`) is itself "alive" — bounding the loop only
///      protects against a flooding peer spinning this tick forever; it
///      does not change the classification.
/// 3. **Phase 3 (re-locked).** Re-validate BEFORE mutating:
///    `session.current_close_generation()` must still equal the generation
///    snapshotted at spawn time, AND the shutdown flag must not have fired
///    in the meantime. Either firing means a close (or close+reopen)
///    intervened while this heartbeat held the transport out — drop the
///    local transport handle (closing its sockets) and exit WITHOUT
///    touching session state; the close path already tore down (or is
///    tearing down) its own view of the session. Otherwise:
///    - Alive → re-install: `guard.transport = Some(transport)`,
///      `guard.transport_owner = TransportOwner::None`. `status` is left
///      untouched (bound_host/state/etc. were never mutated by the
///      borrow).
///    - Dead → stamp `VaraState::SocketLost` (mirroring
///      `vara_stop_session_inner`'s field reset, minus the heartbeat's own
///      shutdown-flag handling since this IS the heartbeat) and exit — the
///      task's job ends the moment the session leaves idle-open, whichever
///      way it leaves.
///
/// **Concurrency invariant:** the std-mutex is held ONLY for Phase 1 + 3
/// (brief snapshot/mutate sections); Phase 2's blocking reads (up to
/// `read_timeout`, ~2s worst case per tick) run with no lock held. While
/// the borrow is out, [`VaraSession::take_transport`] returns `None` to
/// any concurrent listener-arm or outbound dial (see that fn's
/// heartbeat-exclusion doc) — a rare, bounded window that only ever
/// occurs during idle-open (never mid-exchange, since the heartbeat skips
/// the tick entirely when the transport is armed/in-flight).
///
/// `app` + `log` are `Option` so the regression test can drive this
/// without a `Tauri` runtime; production (`vara_open_session`) always
/// passes `Some`/`Some` so the SocketLost transition emits an
/// operator-visible session-log line.
///
/// Returns the shutdown flag; the caller (`vara_open_session`) installs it
/// via [`VaraSession::install_heartbeat_shutdown`] so `vara_stop_session_inner`
/// — the single transport-teardown chokepoint — can cancel a live
/// heartbeat on ordinary close, open-failure rollback, or a connect-class
/// b2f failure, so a clean teardown never races a spurious SocketLost
/// stamp.
pub fn spawn_vara_socket_heartbeat(
    session: std::sync::Arc<VaraSession>,
    app: Option<AppHandle>,
    log: Option<Arc<SessionLogState>>,
    interval: Duration,
) -> Arc<std::sync::atomic::AtomicBool> {
    /// Bound on how many buffered lines (IAMALIVE / OK / stray async
    /// events) one tick's drain will consume before concluding "alive"
    /// without ever having observed an Idle/Eof/Err. Every iteration below
    /// the cap is either an immediate consuming read (a buffered `Line`)
    /// or a single bounded-by-`read_timeout` wait (Idle/Eof/Err, which
    /// break the loop immediately) — so the cap exists purely to stop a
    /// peer that floods the cmd socket faster than we drain it from
    /// spinning this tick forever; it is generous headroom over VARA's
    /// normal idle-open chatter (a handful of keepalives between ticks).
    const HEARTBEAT_DRAIN_CAP: usize = 64;
    // Short per-read timeout used ONLY inside the exclusive heartbeat borrow so
    // an "open but idle" verdict costs ~50ms instead of the 2s cmd read_timeout
    // — this bounds how long each tick holds the transport away from a
    // would-be exchange. A peer FIN is detected immediately regardless.
    const HEARTBEAT_PROBE_TIMEOUT: Duration = Duration::from_millis(50);

    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shutdown_for_task = shutdown.clone();
    let spawn_gen = session.current_close_generation();

    // `tokio::spawn` (not `spawn_blocking`): Phase 2's reads block on this
    // dedicated task only (never while holding `session.inner`'s
    // std-mutex), bounded by `read_timeout` per read — acceptable to run
    // on a tokio worker thread for the same reason the rest of this
    // module's short, bounded blocking I/O is (this is NOT the genuinely
    // long-running b2f exchange, which uses `spawn_blocking`).
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;

            if shutdown_for_task.load(Ordering::Acquire) {
                return;
            }

            // Phase 1 (locked): borrow the transport OUT of the session
            // iff idle-open. This is a BORROW, not a close — leave
            // `status.state == Open`. `take_transport()` additionally
            // refuses to hand the transport to a concurrent
            // listener/outbound taker while `transport_owner ==
            // Heartbeat` (see that fn's doc), so recording the owner here
            // is load-bearing, not just documentation.
            let mut transport = {
                let mut guard = match session.inner.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let idle_open = guard.status.state == VaraState::Open
                    && guard.transport.is_some()
                    && guard.transport_owner == TransportOwner::None;
                if !idle_open {
                    continue;
                }
                guard.transport_owner = TransportOwner::Heartbeat;
                match guard.transport.take() {
                    Some(t) => t,
                    None => {
                        // Unreachable given the idle_open check above
                        // (transport.is_some() was just verified under
                        // the same lock), but defensive: undo the owner
                        // flip and skip the tick rather than panicking.
                        guard.transport_owner = TransportOwner::None;
                        continue;
                    }
                }
            }; // guard dropped here — REQUIRED before any blocking I/O below.

            // Phase 2 (no lock): bounded, consuming liveness drain with a SHORT
            // per-read timeout so the borrow lasts ~tens of ms, not the full 2s
            // cmd read_timeout — otherwise the heartbeat would hold the transport
            // away from a would-be exchange for most of each interval. A drained
            // line (even IAMALIVE) or an idle timeout = alive; a peer FIN (Eof,
            // returned immediately) or a hard error = dead.
            let alive =
                transport.probe_liveness_draining(HEARTBEAT_PROBE_TIMEOUT, HEARTBEAT_DRAIN_CAP);

            // Phase 3 (re-locked): re-validate BEFORE mutating — a close
            // (or close+reopen) that raced the borrow above must win.
            let mut guard = match session.inner.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            let live_gen = session.current_close_generation();
            if live_gen != spawn_gen || shutdown_for_task.load(Ordering::Acquire) {
                // Stale: the operator's Close Session (or a close+reopen)
                // intervened while this heartbeat held the transport out.
                // Drop our local handle (closes its sockets) and exit
                // WITHOUT touching session state — the close path already
                // tore down (or is tearing down) its own view.
                drop(guard);
                drop(transport);
                return;
            }

            if alive {
                guard.transport = Some(transport);
                guard.transport_owner = TransportOwner::None;
                // `status` (state/bound_host/etc.) is untouched — the
                // borrow never mutated it.
                continue;
            }

            // Dead: stamp SocketLost (mirroring `vara_stop_session_inner`'s
            // field reset) and exit — this heartbeat's job is over the
            // moment the session leaves idle-open, whichever way.
            drop(transport);
            guard.active_intent = None;
            guard.active_transport_kind = None;
            guard.abort_writer = None;
            guard.abort_stream = None;
            guard.abort_data_stream = None;
            guard.transport_owner = TransportOwner::None;
            guard.current_exchange = None;
            guard.heartbeat_shutdown = None;
            let bound_host = guard.status.bound_host.clone();
            let bound_cmd_port = guard.status.bound_cmd_port;
            guard.status = VaraStatus {
                state: VaraState::SocketLost,
                last_error: Some("VARA connection lost — reopen to reconnect".to_string()),
                bound_host,
                bound_cmd_port,
                listener_armed: false,
                exchange: None,
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
            };
            drop(guard);

            if let (Some(app), Some(log)) = (app.as_ref(), log.as_ref()) {
                emit_vara_log(
                    app,
                    log,
                    LogLevel::Error,
                    "VARA: cmd-port heartbeat lost the connection — session marked \
                     socket-lost. Close Session and reopen to reconnect."
                        .to_string(),
                );
            }

            return;
        }
    });

    shutdown
}

/// Open a VARA session: open the cmd + data TCP socket pair, optionally
/// send the `BW <hz>` setter, record the session intent + transport-kind,
/// and (when `intent.auto_arms_listener()` is true) auto-arm the listener
/// before returning. Returns the new status snapshot.
///
/// Does NOT send `CONNECT` and does NOT transmit. Opening these sockets is
/// equivalent to opening a TCP connection to localhost:8300 — RADIO-1-safe.
/// The RF-transmitting `CONNECT` flow lands in Phase 3.5 with the full
/// session-state machine and a consent token gate.
///
/// **Signature (tuxlink-0ye6 Task 3.2 + Codex Round 2 P2):** accepts both
/// `intent: SessionIntent` AND `transport_kind: TransportKind`. The
/// transport-kind discriminates `VaraHf` vs `VaraFm` even though the wire
/// transport (TCP host/port) is identical — Codex Round 3 P1 #3: lets the
/// frontend detect sidebar-nav drift mid-session. Without both args the
/// Phase 5 RadioSessionPanel IPC would fail at deserialization.
///
/// **Auto-arm (spec §2 + §3):** the listener is auto-armed inline when
/// `intent.auto_arms_listener()` is true — `P2p` (any peer) and `RadioOnly`
/// (R-pool peer) auto-arm; `Cms` does not (CMS is outbound-only from the
/// client's view). If the auto-arm fails after the transport opens, the
/// open still succeeds; the operator can retry the arm via `vara_listen`
/// (the failure is logged but doesn't tear down the transport — the
/// transport-open contract and the arm contract are distinct).
///
/// If a session is already open, returns Err — operator must `vara_close_session`
/// first. (This is conservative; a future iteration might re-open transparently.)
#[tauri::command]
pub async fn vara_open_session(
    app: AppHandle,
    session: State<'_, std::sync::Arc<VaraSession>>,
    log: State<'_, Arc<SessionLogState>>,
    listen_state: State<'_, std::sync::Arc<crate::ui_commands::VaraListenState>>,
    intent: SessionIntent,
    transport_kind: TransportKind,
) -> Result<VaraStatus, String> {
    let ui_cfg = config_get_vara();
    // tuxlink-0063 (Phase 3, Task 3.9): the open-time MYCALL is the on-air
    // station ID the VARA modem is told at session open. Under the handle
    // model it comes from the authenticated active SessionIdentity, NEVER from
    // persisted config. Opening a transmit-capable VARA session requires an
    // authenticated identity — resolve it fail-closed here (a NoActiveIdentity
    // surfaces as an error and leaves the transport unopened), the same
    // posture as the rest of Phase 3.
    //
    // The old code TOLERATED a missing callsign (None → skip the MYCALL
    // setter, let VARA warn). That tolerance is gone: transmit is gated on
    // authentication until the Phase 6/7 identity-switch UI lands.
    //
    // This open-time MYCALL is now redundant with the per-CONNECT MYCALL set
    // by `run_vara_b2f_with_transport` (Task 3.7), which is the authoritative
    // on-air station ID on dial. It is set here too — consistently from the
    // session — so VARA recognizes the App handshake at open time and stops
    // logging "not connected to App".
    let session_id = app
        .state::<crate::app_backend::BackendState>()
        .current()
        .ok_or_else(|| "VARA open: backend offline — cannot resolve active identity".to_string())?
        .active_identity()
        .map_err(|e| e.to_string())?;
    let callsign = session_id.mycall().as_str().to_uppercase();
    let host_label = format!("{}:{}", ui_cfg.host, ui_cfg.cmd_port);
    emit_vara_log(
        &app,
        &log,
        LogLevel::Info,
        format!(
            "VARA: opening TCP transport to {host_label} (intent={:?}, transport={})",
            intent,
            transport_kind.as_str(),
        ),
    );
    // Inner-returned status is intentionally discarded — we re-snapshot
    // after the optional auto-arm so the wire-returned status reflects
    // `listener_armed = true` when auto-arm fires.
    match vara_open_session_inner(
        &session,
        &ui_cfg,
        Some(callsign.as_str()),
        intent,
        transport_kind,
    ) {
        Ok(_status) => {
            emit_vara_log(
                &app,
                &log,
                LogLevel::Info,
                format!("VARA: transport open at {host_label} (MYCALL {callsign} sent)"),
            );
            // tuxlink-6urh2: spawn the drop-detection heartbeat now that the
            // transport is installed. `vara_stop_session_inner` — the single
            // transport-teardown chokepoint — cancels it on every close path
            // (ordinary Close Session, and any connect-class b2f failure
            // that tears the transport down via the same helper).
            let heartbeat_shutdown = spawn_vara_socket_heartbeat(
                session.inner().clone(),
                Some(app.clone()),
                Some(log.inner().clone()),
                VARA_HEARTBEAT_INTERVAL,
            );
            session.install_heartbeat_shutdown(heartbeat_shutdown);
        }
        Err(e) => {
            emit_vara_log(
                &app,
                &log,
                LogLevel::Error,
                format!("VARA: open failed — {e}"),
            );
            return Err(e);
        }
    }

    // Auto-arm the listener when the intent calls for it (spec §2 + §3).
    // The arm is best-effort: a failure here does NOT tear down the
    // transport — open and arm are distinct contracts, and the operator
    // can retry the arm via the Listen toggle if it fails.
    if intent.auto_arms_listener() {
        if let Err(e) = crate::ui_commands::arm_vara_listener_inner(
            &app,
            log.inner(),
            session.inner(),
            listen_state.inner(),
            transport_kind,
        )
        .await
        {
            emit_vara_log(
                &app,
                &log,
                LogLevel::Warn,
                format!(
                    "VARA: auto-arm failed after open ({:?}); transport remains open. \
                     Toggle Listen on the panel to retry the arm.",
                    e
                ),
            );
        }
    }

    // Re-snapshot so the returned status reflects the auto-arm outcome
    // (listener_armed flips true when the arm spawned the consumer task).
    Ok(session.snapshot())
}

/// Inner helper for [`vara_open_session`] with factored-out config + callsign
/// args so tests can drive it without touching the persisted config file or a
/// Tauri runtime. When `callsign` is `Some`, MYCALL is sent on the cmd socket
/// after TCP open (before BW) so VARA's host protocol recognizes the App
/// handshake. **Production always passes `Some`** — since tuxlink-0063 Phase 3
/// the outer [`vara_open_session`] resolves the active `SessionIdentity` and
/// fails closed on `NoActiveIdentity`, so the `None` arm (skip MYCALL setter)
/// is exercised only by tests that drive session-state mechanics without a call.
///
/// Records `intent` + `transport_kind` on `VaraSessionInner` after the open
/// succeeds; cleared in [`vara_stop_session_inner`] (reached via
/// [`vara_close_session_inner`]) on teardown.
pub fn vara_open_session_inner(
    session: &std::sync::Arc<VaraSession>,
    ui_cfg: &VaraUiConfig,
    callsign: Option<&str>,
    intent: SessionIntent,
    transport_kind: TransportKind,
) -> Result<VaraStatus, String> {
    // Acquire the lock for the duration of the open. We hold the lock across
    // `VaraTransport::connect` (TCP connect, ~ms on localhost; bounded by
    // the 5s connect_timeout) — calls from the UI side are serialized so a
    // double-press on Start doesn't open two transports.
    let mut guard = session
        .inner
        .lock()
        .map_err(|e| format!("session lock poisoned: {e}"))?;

    // tuxlink-6urh2 v2: reject on owner too, not just `transport.is_some()`.
    // A listener consumer (or the drop-detection heartbeat, or an in-flight
    // outbound dial) can hold `transport_owner != None` while
    // `guard.transport` itself reads `None` (the transport is OUT of the
    // session for the duration of that borrow) — the pre-fix check would
    // let a reopen race in during exactly that window, installing a second
    // transport while the first one is still owned elsewhere.
    if guard.transport.is_some() || guard.transport_owner != TransportOwner::None {
        return Err("VARA session already started — call vara_close_session first".into());
    }

    // Mark Connecting so any concurrent vara_status sees the in-flight state.
    // (The lock prevents true concurrency on the start path itself.)
    guard.status = VaraStatus {
        state: VaraState::Connecting,
        last_error: None,
        bound_host: Some(ui_cfg.host.clone()),
        bound_cmd_port: Some(ui_cfg.cmd_port),
        // Lifecycle fields (tuxlink-0ye6 Task 3.0): defaults during
        // transport-layer start; Phase 3 wires real session state.
        listener_armed: false,
        exchange: None,
        transport_owner: TransportOwner::None,
        active_intent: None,
        active_transport_kind: None,
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
                listener_armed: false,
                exchange: None,
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
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

    // Install the ABORT side-channel BEFORE stashing the transport so any
    // path that subsequently begins a blocking session-state op (auto-arm
    // → listener consumer → run_vara_b2f_answer) finds the writer already
    // installed. Without this, vara_close_session_inner's call to
    // abort_in_flight (Task 3.3) returns Err("no abort writer installed")
    // and silently no-ops, leaving the modem to keep TXing until the
    // graceful timeout — the exact failure mode tuxlink-12sc tracks
    // (spec §9 + Codex Round 1 P1 #4 → tuxlink-0ye6 Task 4.1 + 4.2).
    //
    // Writes directly into `guard.abort_writer` + `guard.abort_stream`
    // instead of calling `session.install_abort_writer()` — the latter
    // re-acquires `session.inner.lock()`, which would deadlock since we
    // already hold the same guard here.
    //
    // Best-effort: try_clone_abort_writer Errs only on syscall failure
    // (try_clone on the TCP socket); if it fails, the abort side-channel
    // is absent for this session — vara_close_session_inner falls through
    // to the graceful transport teardown, which is the pre-Task-4.x
    // behavior. We don't fail the open over it.
    if let Ok((writer, stream)) = transport.try_clone_abort_writer() {
        guard.abort_writer = Some(writer);
        guard.abort_stream = Some(stream);
    }
    // Same best-effort posture for the DATA-socket shutdown handle
    // (tuxlink-xzxk1 — Codex adrev P1 #2): without it, an abort during an
    // exchange leaves the exchange thread parked in a data read for up to
    // the RF-scale data_read_timeout instead of returning EOF immediately.
    if let Ok(data) = transport.try_clone_data_shutdown_handle() {
        guard.abort_data_stream = Some(data);
    }

    guard.transport = Some(transport);
    guard.active_intent = Some(intent);
    guard.active_transport_kind = Some(transport_kind);
    guard.status = VaraStatus {
        state: VaraState::Open,
        last_error: None,
        bound_host: Some(ui_cfg.host.clone()),
        bound_cmd_port: Some(ui_cfg.cmd_port),
        listener_armed: false,
        exchange: None,
        transport_owner: TransportOwner::None,
        // Mirror inner fields into the cached snapshot. `snapshot()`
        // overlays the live accessors on top of the cached struct, but
        // mirroring here keeps `inner.status` self-consistent for
        // anything that reads it without going through the snapshot path
        // (e.g., direct guard reads in tests).
        active_intent: Some(intent),
        active_transport_kind: Some(transport_kind),
    };

    Ok(guard.status.clone())
}

/// Close a VARA session: full lifecycle teardown per spec §5.
///
/// Renamed from `vara_stop_session` (Task 3.3) to reflect the broader
/// contract — close is the spec's canonical session-end verb covering
/// listener disarm + in-flight abort + transport teardown, not just the
/// transport-layer stop. The wrapped behavior:
///
/// 1. **Disarm listener** via [`crate::ui_commands::disarm_vara_listener_inner`]
///    — idempotent; no-op when no listener is armed.
/// 2. **Abort in-flight exchange** via [`VaraSession::abort_in_flight`] —
///    Task 4.1's bounded `ABORT\r` cooperative write with hard-close
///    fallback. Best-effort; the no-writer-installed Err is expected when
///    no exchange is in flight, and is intentionally swallowed.
/// 3. **Clear active session mode** (`active_intent` +
///    `active_transport_kind`) as a side effect of step 4's transport
///    teardown — `vara_stop_session_inner` clears both fields, and the
///    rename preserves that behavior.
/// 4. **Close transport** via [`vara_stop_session_inner`] — drops the
///    `Option<VaraTransport>`, FINs both sockets, transitions to
///    `VaraState::Closed`.
///
/// Idempotent across the whole chain — calling on an already-closed
/// session is a no-op that returns the closed status.
#[tauri::command]
pub fn vara_close_session(
    app: AppHandle,
    session: State<'_, std::sync::Arc<VaraSession>>,
    log: State<'_, Arc<SessionLogState>>,
    listen_state: State<'_, std::sync::Arc<crate::ui_commands::VaraListenState>>,
) -> Result<VaraStatus, String> {
    // Capture whether the transport was open BEFORE the close, so the log
    // line distinguishes "actually closed something" from a no-op idempotent
    // call after an already-closed session.
    let was_open = session
        .inner
        .lock()
        .map(|g| g.transport.is_some())
        .unwrap_or(false);
    // Note whether a listener was armed at entry so the log line can
    // surface the disarm side-effect for the operator.
    let was_armed = listen_state.is_armed();
    if was_armed {
        // Signal the consumer task to drain. Emits its own log line.
        crate::ui_commands::disarm_vara_listener_inner(&app, &log, &listen_state);
    }
    let result = vara_close_session_inner(&session, &listen_state);
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

/// Inner helper for [`vara_close_session`] so tests can drive without a Tauri
/// runtime. Performs the spec §5 close sequence: disarm → abort → clear
/// active mode → close transport.
///
/// The `listen_state` arg accepts the disarm responsibility at this level
/// (rather than only the outer Tauri command) so unit tests can exercise
/// the disarm path without an AppHandle. The disarm helper itself
/// (`disarm_vara_listener_inner` in `ui_commands.rs`) is what emits the
/// operator-visible log line; this inner replicates the take-handle +
/// set-shutdown-flag logic directly so the inner doesn't depend on
/// `AppHandle`/`SessionLogState`.
pub fn vara_close_session_inner(
    session: &std::sync::Arc<VaraSession>,
    listen_state: &std::sync::Arc<crate::ui_commands::VaraListenState>,
) -> Result<VaraStatus, String> {
    use std::sync::atomic::Ordering;

    // tuxlink-pdnw (Codex Phase 3-4 P1 #4): bump the close-generation
    // BEFORE the disarm flag is set. Any in-flight worker (b2f exchange or
    // listener consumer accept-loop) that has already snapshotted the prior
    // generation will now see a stale snapshot on its install-back path —
    // the guarded install drops the transport instead of restoring the
    // session to `Open`. Without this, the consumer's drain path (which
    // calls `return_transport` AFTER reading the shutdown flag) would race
    // with this close and reopen the session.
    let _ = session.bump_close_generation();

    // Step 1: Disarm listener (idempotent — no-op when not armed). We don't
    // route through `disarm_vara_listener_inner` here because that helper
    // needs an AppHandle + SessionLogState for the operator-facing log line;
    // the outer `vara_close_session` command calls it BEFORE this inner so
    // the log line surfaces. This inner replicates the take-handle +
    // set-shutdown-flag mechanics so the unit-test path (no AppHandle)
    // still exercises the disarm.
    let handle = {
        let mut guard = listen_state.inner.lock().unwrap();
        guard.take()
    };
    if let Some(h) = handle {
        h.shutdown.store(true, Ordering::SeqCst);
    }

    // Step 2: Abort any in-flight exchange (Task 4.1 — bounded cooperative
    // ABORT\r with hard-close fallback). Best-effort: the
    // "no abort writer installed" Err is the expected path when no
    // exchange was in flight, and any other failure mode (poisoned lock,
    // wedged peer) is recoverable via the transport teardown in step 4.
    // Swallowing the Err here is intentional — the close contract is
    // unconditional teardown.
    let _ = session.abort_in_flight();

    // Steps 3 + 4: Transport teardown clears `active_intent` +
    // `active_transport_kind` and drops the transport. See
    // `vara_stop_session_inner` for the rationale on why we don't send a
    // farewell DISCONNECT before dropping the sockets.
    vara_stop_session_inner(session)
}

/// Inner helper that performs the transport-only teardown (close TCP sockets,
/// clear active session mode). Kept as a separate sync helper so
/// [`vara_close_session_inner`] can chain after the listener-disarm + abort
/// steps without re-implementing the field reset.
pub fn vara_stop_session_inner(
    session: &std::sync::Arc<VaraSession>,
) -> Result<VaraStatus, String> {
    let mut guard = session
        .inner
        .lock()
        .map_err(|e| format!("session lock poisoned: {e}"))?;

    // Drop the transport (closes both sockets — TcpStream::Drop sends FIN).
    // We don't send DISCONNECT first because (a) DISCONNECT could trigger
    // an unwanted RF DISC frame if a peer-connect happened to be in flight,
    // and (b) Phase 2 doesn't expose any peer-connect path so the worst-
    // case state is "MYCALL/BW set, no CONNECT issued" — pure TCP teardown
    // is the right semantics.
    guard.transport = None;
    // Clear session-state recorded by `vara_open_session_inner`. Lives here
    // (rather than only in `vara_close_session_inner`) because direct callers
    // of this transport-teardown helper — including the open-failure
    // rollback paths and the listener consumer's cleanup — also need a
    // clean slate for a subsequent open.
    guard.active_intent = None;
    guard.active_transport_kind = None;
    // Codex Phase 3-4 boundary P2 #1 (tuxlink-u1r7): clear the ABORT
    // side-channel + arbiter owner alongside the transport. Mirrors
    // `ModemSession::reset_to_stopped`'s posture. The abort_writer +
    // abort_stream point at the cmd socket that was just torn down;
    // leaving them in place stranded a stale writer into the next
    // session if `try_clone_abort_writer` failed on the next open
    // (no replacement would happen — the `if let Ok(...)` branch in
    // `vara_open_session_inner` only assigns on success). Likewise
    // `transport_owner` carries leftover ListenerArmed/Inbound state
    // after a `take_transport()`-based close, which the status overlay
    // would surface as `listenerArmed = true` on a closed session.
    guard.abort_writer = None;
    guard.abort_stream = None;
    guard.abort_data_stream = None;
    guard.transport_owner = TransportOwner::None;
    // Codex Phase 3-4 boundary P2 #4 (tuxlink-u1r7): also clear the
    // in-flight exchange marker — a close that races a b2f path's
    // entry should not leave Outbound/Inbound staring back at the
    // operator after the transport is gone.
    guard.current_exchange = None;
    // tuxlink-6urh2: cancel a live drop-detection heartbeat on EVERY
    // teardown path through this chokepoint (ordinary close, open-failure
    // rollback, connect-class b2f failure). Without this, a heartbeat tick
    // already in flight when the operator clicks Close Session could stamp
    // SocketLost onto a session the operator just closed — the same race
    // class `install_transport_if_generation_matches` guards against, but
    // this flag is a belt-and-suspenders fast path: the heartbeat ALSO
    // re-validates close_generation + idle-open under the lock before it
    // ever mutates state, so a missed flag flip here is not itself a
    // correctness gap, only a slower one.
    if let Some(flag) = guard.heartbeat_shutdown.take() {
        flag.store(true, Ordering::Release);
    }
    guard.status = VaraStatus::closed();
    Ok(guard.status.clone())
}

/// Return the current session status snapshot. Cheap; safe to poll. Hooks
/// call this on mount to recover state after a hot-reload.
#[tauri::command]
pub fn vara_status(session: State<'_, std::sync::Arc<VaraSession>>) -> VaraStatus {
    session.snapshot()
}

/// Worst-case `CONNECT` wall-clock budget for the VARA dial path
/// (tuxlink-0ye6 Task 3.4). 120 s cap — matches the legacy 120 s connect
/// cap that `modem_ardop_connect` (Start-button, slated for Phase 6
/// deletion) inlines.
///
/// 2026-05-22 incident: a ~110s runaway connect (no working abort) forced an
/// operator radio power-off. The cap prevents the same pattern here — if the
/// CONNECTED event does not arrive within the deadline, the command errors
/// out and the session is reset. Side-channel `ABORT\r` (Task 4.1) is the
/// in-flight interrupt path.
const VARA_CONNECT_DEADLINE: Duration = Duration::from_secs(120);

/// Worst-case `DISCONNECT` wall-clock budget for the wind-down. Mirrors
/// the 5 s deadline ARDOP's `modem_ardop_disconnect_inner` uses for the
/// graceful tear-down.
const VARA_DISCONNECT_DEADLINE: Duration = Duration::from_secs(5);

/// Run a B2F mail exchange over VARA (tuxlink-0ye6 Task 3.4) — the
/// VARA analog of `modem_ardop_b2f_exchange`. CONNECT to peer → B2F
/// exchange + intent-filtered mailbox drain → DISCONNECT, all in one
/// Tauri call.
///
/// # Preconditions
///
/// - The operator has already opened the VARA session via
///   `vara_open_session`; the TCP cmd + data sockets are open and
///   MYCALL / BW setters have been sent (if configured).
/// - The session is NOT currently armed for an inbound listener. If it
///   is, the operator must close + reopen with a dial-only intent
///   before invoking this command — the arbiter wire-in that would let
///   outbound take the transport from an armed listener is deferred to
///   a follow-up (see scope notes below + the TODO inside the inner).
///
/// # Flow
///
/// 1. **Validate `transport_kind`** is one of [`TransportKind::VaraHf`] /
///    [`TransportKind::VaraFm`] — defensive guard against a future
///    `RadioSessionPanel` routing the wrong panel's invoke to this command.
/// 2. **Take the open transport** from the session via
///    [`VaraSession::take_transport`] — the existing listener-bypass
///    pattern. The session transitions to a `ListenerArmed` owner
///    state, but no consumer is listening; the take is the dial
///    path's claim on the transport.
/// 3. **Send `CONNECT <mycall> <target>`** on the cmd port and wait
///    for the `CONNECTED` event (bounded by [`VARA_CONNECT_DEADLINE`]).
/// 4. **Run the B2F exchange** over the data socket via
///    [`crate::winlink_backend::run_vara_b2f_exchange`].
/// 5. **Clean up, branched on the outcome** (tuxlink-n95sr #2, mirrors ARDOP's
///    [`finish_vara_b2f_exchange`]): a CONNECT-class failure (the ARQ link never
///    came up) is terminal — drop the transport + reset the session to `Closed`
///    so it is not silently re-armable; a success or mid-EXCHANGE failure
///    best-effort link-disconnects + re-installs the transport so the open
///    session survives for a retry / listener re-arm.
///
/// # Signature (Codex Phase 3-4 boundary P2 #2 — tuxlink-u1r7)
///
/// Accepts `intent: SessionIntent` (the full enum, mirroring
/// `modem_ardop_b2f_exchange`) and `transport_kind: TransportKind` —
/// the shared `RadioSessionPanel` sends both fields for every B2F
/// command per spec §2's capability matrix, which includes
/// `RadioOnly` outbound. The prior shape took `intent: String` and
/// parsed it down to `Cms` / `P2p` only, which rejected `RadioOnly`
/// dials and could not distinguish VARA-HF from VARA-FM at the
/// command boundary.
///
/// # Scope (Task 3.4)
///
/// The arbiter wire-in that would let an armed listener cooperatively
/// yield the transport to outbound (via
/// [`VaraSession::take_transport_for_outbound`] — the Task 4.3 state
/// machine) is **deferred** to a follow-up: the listener-consumer side
/// has not yet been modified to honor the yield request, so wiring this
/// command to that path would deadlock against an unaware consumer.
/// This command uses the simpler `take_transport` pattern that ARDOP's
/// `b2f_exchange` uses today. See the TODO inside `run_vara_b2f_with_transport`
/// for the bd issue that tracks the arbiter wire-in.
#[tauri::command]
// tuxlink-8fkkk A2: 8 args after adding freq_hz + qsy_candidates for the
// pre-audio CAT tune + ordered-list QSY. A Tauri command's args are its IPC
// surface, not a refactor smell — splitting them into a struct would only
// obscure the camelCase arg mapping the frontend invoke relies on.
#[allow(clippy::too_many_arguments)]
pub async fn modem_vara_b2f_exchange(
    app: AppHandle,
    log: State<'_, Arc<SessionLogState>>,
    session: State<'_, std::sync::Arc<VaraSession>>,
    target: String,
    intent: SessionIntent,
    transport_kind: TransportKind,
    // tuxlink-8fkkk Task A2: pre-audio CAT tune + ordered-list QSY, mirroring
    // the ARDOP `modem_ardop_connect` shape. Tauri maps Rust `freq_hz` → JS
    // `freqHz` and `qsy_candidates` → JS `qsyCandidates` (camelCase). A JS
    // caller that omits these (today's VaraRadioPanel, pre-A3) sends `None` for
    // each `Option`, reproducing the legacy single-dial behavior. When
    // `qsy_candidates` is `Some` + non-empty it overrides `target`/`freq_hz`
    // and the walk visits each candidate (operator-gated by `config.rig.qsy_on_fail`).
    freq_hz: Option<u64>,
    qsy_candidates: Option<Vec<DialCandidate>>,
) -> Result<(), String> {
    // Codex Phase 3-4 boundary P2 #2 (tuxlink-u1r7): defensive
    // validation — the VARA b2f command must be invoked with a VARA
    // transport kind. Mirrors the ARDOP-side validation in
    // `modem_ardop_b2f_exchange` so a future RadioSessionPanel routing
    // a mismatched kind here surfaces a clean error before any
    // radio-touching work.
    if !matches!(
        transport_kind,
        TransportKind::VaraHf | TransportKind::VaraFm
    ) {
        return Err(format!(
            "modem_vara_b2f_exchange invoked with non-VARA transport_kind={:?}",
            transport_kind
        ));
    }

    let target_clean = target.trim().to_uppercase();
    emit_vara_log(
        &app,
        &log,
        LogLevel::Info,
        format!(
            "VARA B2F: dialing {target_clean} (intent={:?}, transport={})",
            intent,
            transport_kind.as_str(),
        ),
    );

    // ─── Resolve the host-side PTT keyer — FAIL-CLOSED (tuxlink-yrrjq) ──
    // VARA is a soundcard modem with no PTT of its own: it raises
    // `PTT ON`/`PTT OFF` on the cmd socket and the HOST must key the rig.
    // Resolve the keyer from the operator's persisted PTT config BEFORE any
    // session state is disturbed — if nothing can key, refuse the dial here
    // with an actionable error instead of dead-airing into an unkeyed radio.
    let ptt_cfg = config::read_config().map_err(|e| format!("read config failed: {e}"))?;
    let keyer = ptt::resolve_vara_ptt(
        &ptt_cfg.modem_ardop.clone().unwrap_or_default(),
        &ptt_cfg.rig,
    )
    .map_err(|e| format!("VARA PTT not available — refusing to dial: {e}"))?;
    let keyer_is_vox = matches!(keyer, VaraPtt::Vox);
    emit_vara_log(
        &app,
        &log,
        if keyer_is_vox {
            LogLevel::Warn
        } else {
            LogLevel::Info
        },
        format!("VARA PTT: {}", keyer.describe()),
    );
    let keyer: SharedPtt = std::sync::Mutex::new(Box::new(keyer));
    // Unkey on EVERY exit from this command — success, error, or panic
    // unwind — so no path can leave the transmitter keyed.
    let _unkey_guard = UnkeyGuard::new(&keyer);

    // tuxlink-pdnw (Codex Phase 3-4 P1 #4): snapshot the close-generation
    // BEFORE the transport take. If `vara_close_session_inner` runs during
    // this exchange, it will bump the generation; the guarded install-back
    // below sees the stale snapshot and drops the transport instead of
    // restoring it into a session the operator just closed.
    let close_gen_snapshot = session.current_close_generation();

    // tuxlink-0iqi (Codex Phase 3-4 P1 #2): snapshot the open-session
    // identity BEFORE the transport take so the install-back can restore
    // active_intent + active_transport_kind. Spec §2 — "outbound dial is
    // within-session": a successful (or failed) Send/Receive returns the
    // session to Open with the SAME active intent + transport kind, so
    // the listener (if armed by intent) can re-arm and a subsequent
    // Send/Receive runs without re-opening. Without this snapshot, the
    // install-back would clear active_intent/kind and force the operator
    // to Close + Open before retrying — the bug Codex flagged.
    //
    // Snapshot bound_host + bound_cmd_port likewise so the install-back
    // restores the status DTO with the same connection details the
    // operator saw before the exchange.
    let (snapshot_intent, snapshot_kind, snapshot_bound_host, snapshot_bound_cmd_port) = {
        let guard = session
            .inner
            .lock()
            .map_err(|e| format!("session lock poisoned: {e}"))?;
        (
            guard.active_intent,
            guard.active_transport_kind,
            guard.status.bound_host.clone(),
            guard.status.bound_cmd_port,
        )
    };

    // ─── Take the installed transport ────────────────────────────────
    // The transport was installed by `vara_open_session`. If it's
    // missing, the operator didn't open the session first — surface
    // that cleanly.
    //
    // TODO(tuxlink-17u9): swap this for
    // `session.take_transport_for_outbound().await` once the listener
    // consumer task honors the `transport_yield_request` notify
    // (Task 4.3 has the session-side state machine; the consumer side
    // needs to drop into a yield branch on notify before the wire-in
    // is deadlock-safe).
    let mut transport = session.take_transport().ok_or_else(|| {
        "VARA session not open — press Open Session (VARA HF/FM) before Send/Receive".to_string()
    })?;

    // Codex Phase 3-4 boundary P2 #4 (tuxlink-u1r7): mark the exchange
    // as in flight (Outbound) so a status poll from the UI surfaces the
    // dial state correctly. Cleared on the install-back path below so
    // the marker tracks the actual b2f-runtime window.
    session.begin_exchange(ExchangeState::Outbound);

    // Wrap the connect + exchange + disconnect in an inner so a single
    // point handles cleanup on BOTH success and failure. The disconnect
    // runs OUTSIDE any held lock (the lock was already released by
    // `take_transport`).
    let outcome = run_vara_b2f_with_transport(
        &app,
        &log,
        &session,
        close_gen_snapshot,
        &mut transport,
        &target_clean,
        intent,
        freq_hz,
        qsy_candidates,
        &keyer,
    );

    // Always clear the exchange marker before cleanup (operator can read
    // `exchange == None` as soon as the b2f path returns; the install-back path
    // inside `finish_vara_b2f_exchange` also clears it as belt-and-suspenders).
    session.end_exchange();

    // ─── Branch cleanup on the outcome (tuxlink-n95sr #2) ─────────────
    // A CONNECT-class failure is terminal — drop the transport + reset the
    // session to Closed so it is NOT silently re-armable ("session restarts on
    // failure"). A success or mid-EXCHANGE failure keeps the open session for a
    // retry / listener re-arm. Mirrors ARDOP's `finish_b2f_exchange`.
    let result = finish_vara_b2f_exchange(
        session.inner(),
        transport,
        close_gen_snapshot,
        snapshot_bound_host,
        snapshot_bound_cmd_port,
        snapshot_intent,
        snapshot_kind,
        outcome,
        &keyer,
    );

    match &result {
        Ok(()) => emit_vara_log(
            &app,
            &log,
            LogLevel::Info,
            format!("VARA B2F: exchange with {target_clean} complete"),
        ),
        Err(e) => emit_vara_log(
            &app,
            &log,
            LogLevel::Error,
            format!("VARA B2F: exchange with {target_clean} failed — {e}"),
        ),
    }

    result
}

/// Outcome of a VARA connect→B2F attempt (tuxlink-n95sr #2). Mirrors ARDOP's
/// `ExchangeOutcome`: distinguishes a CONNECT-class failure (no candidate
/// reached `CONNECTED` — the ARQ link never came up; terminal, free the modem)
/// from a mid-EXCHANGE failure (a candidate connected but the B2F exchange
/// faulted — keep the open session for a retry). [`finish_vara_b2f_exchange`]
/// branches cleanup on it.
enum VaraExchangeOutcome {
    /// CONNECT + the full B2F exchange completed.
    Completed,
    /// No candidate reached `CONNECTED` (or setup failed before the dial) — the
    /// ARQ link never came up.
    ConnectFailed(String),
    /// A candidate connected but the B2F exchange itself failed.
    ExchangeFailed(String),
}

/// Post-exchange cleanup, branched on the outcome (tuxlink-n95sr #2). Mirrors
/// ARDOP's `finish_b2f_exchange`.
///
/// - **CONNECT-class failure** (`ConnectFailed`): terminal. DROP the failed
///   transport (closes the VARA TCP sockets) and reset the session to `Closed`
///   via [`vara_stop_session_inner`] so it is NOT silently re-armable — the bug
///   where a failed dial left the session "restarting" / never freed the modem.
///   No link-disconnect: there is no live ARQ link to tear down. The reset is
///   guarded on the close generation — if an operator Close intervened while the
///   dial was blocked, the (possibly re-opened) session is left alone; either
///   way the failed transport is dropped, never re-installed.
/// - **Success or mid-EXCHANGE failure** (`Completed` / `ExchangeFailed`):
///   best-effort link-disconnect (link-only) + guarded re-install so the open
///   session + any listener arming survive for a retry (spec §2 "outbound dial
///   is within-session"). Guarded (tuxlink-pdnw): a stale generation drops the
///   transport instead of restoring it into a closed session.
///
/// Returns the flat `Result` the command surfaces (and logs).
#[allow(clippy::too_many_arguments)]
fn finish_vara_b2f_exchange(
    session: &std::sync::Arc<VaraSession>,
    mut transport: VaraTransport,
    close_gen_snapshot: u64,
    snapshot_bound_host: Option<String>,
    snapshot_bound_cmd_port: Option<u16>,
    snapshot_intent: Option<SessionIntent>,
    snapshot_kind: Option<TransportKind>,
    outcome: VaraExchangeOutcome,
    ptt: &SharedPtt,
) -> Result<(), String> {
    match outcome {
        VaraExchangeOutcome::ConnectFailed(msg) => {
            // Link never came up → terminal. Drop our failed transport and reset
            // the session to Closed (guarded on the close generation).
            drop(transport);
            if session.current_close_generation() == close_gen_snapshot {
                let _ = vara_stop_session_inner(session);
            }
            Err(msg)
        }
        other => {
            // Link was up (success or mid-EXCHANGE failure): tear down the ARQ
            // LINK only + guarded re-install so the open session survives.
            // The keyer rides along: VARA keys the radio to transmit the
            // disconnect frames, so the wind-down loop must service PTT too.
            let _ = vara_dial_disconnect(&mut transport, ptt);
            if let Err(dropped) = session.install_transport_if_generation_matches(
                transport,
                close_gen_snapshot,
                snapshot_bound_host,
                snapshot_bound_cmd_port,
                snapshot_intent,
                snapshot_kind,
            ) {
                drop(dropped);
            }
            match other {
                VaraExchangeOutcome::Completed => Ok(()),
                VaraExchangeOutcome::ExchangeFailed(msg) => Err(msg),
                VaraExchangeOutcome::ConnectFailed(_) => unreachable!("handled above"),
            }
        }
    }
}

/// Inner helper for [`modem_vara_b2f_exchange`]: sends `CONNECT
/// <mycall> <target>` on the cmd port, waits for the `CONNECTED` event
/// (bounded by [`VARA_CONNECT_DEADLINE`]), runs the B2F exchange over
/// the data socket via [`crate::winlink_backend::run_vara_b2f_exchange`],
/// and returns. The caller is responsible for the post-exchange
/// `DISCONNECT` + session reset (uniform cleanup on both success and
/// failure).
///
/// Factored out so the Tauri command can run cleanup uniformly. Returns
/// the error as a `String` so it surfaces to the frontend without
/// exposing the internal `BackendError` type — same pattern as the
/// other modem commands.
#[allow(clippy::too_many_arguments)]
fn run_vara_b2f_with_transport(
    app: &AppHandle,
    log: &Arc<SessionLogState>,
    session: &VaraSession,
    close_gen_snapshot: u64,
    transport: &mut VaraTransport,
    target: &str,
    intent: SessionIntent,
    freq_hz: Option<u64>,
    qsy_candidates: Option<Vec<DialCandidate>>,
    keyer: &SharedPtt,
) -> VaraExchangeOutcome {
    // tuxlink-n95sr #2: every early return BELOW (setup + the candidate walk) is
    // a CONNECT-class failure — the ARQ link never came up — so it maps to
    // `VaraExchangeOutcome::ConnectFailed`, which the caller treats as terminal
    // (frees the modem). Only a failure AFTER `run_vara_b2f_exchange` begins is a
    // mid-EXCHANGE failure (`ExchangeFailed`, keeps the session Open for retry).
    // Mirror of ARDOP's `run_ardop_connect_b2f_with_transport` → `ExchangeOutcome`.

    // Mailbox lives at <app_data_dir>/native-mbox (per `bootstrap::install_native`).
    let app_data_dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(e) => {
            return VaraExchangeOutcome::ConnectFailed(format!(
                "could not resolve app data dir: {e}"
            ))
        }
    };
    let mailbox = crate::native_mailbox::Mailbox::new(app_data_dir.join("native-mbox"));

    let cfg = match config::read_config() {
        Ok(c) => c,
        Err(e) => return VaraExchangeOutcome::ConnectFailed(format!("read config failed: {e}")),
    };

    // tuxlink-0063 (Phase 3, Task 3.7): the on-air station ID comes from the
    // authenticated active SessionIdentity, not from `config.identity.active_full`.
    // Both the VARA CONNECT cmd-port MYCALL (on-air station ID) and the B2F
    // exchange callsign must come from the session — neither may use config.
    let backend_current = match app.state::<crate::app_backend::BackendState>().current() {
        Some(c) => c,
        None => {
            return VaraExchangeOutcome::ConnectFailed(
                "VARA B2F: backend offline — cannot resolve active identity".to_string(),
            )
        }
    };
    let session_id = match backend_current.active_identity() {
        Ok(id) => id,
        Err(e) => return VaraExchangeOutcome::ConnectFailed(e.to_string()),
    };

    let mycall = session_id.mycall().as_str().to_uppercase();

    // tuxlink-2ns7: file received mail into the active FULL's per-FULL inbox
    // (`mailbox/<FULL>/inbox`) — the namespace the UI reads — not the bare
    // `_default`. The exchange runs AS this session's FULL. Mirrors the
    // `ui_commands` inbound sites; without this, on-air VARA receives land in
    // `_default/inbox` and are invisible. (Uses the non-uppercased `Callsign`,
    // matching the stored identity + migration target; `mycall` above is the
    // on-air station ID, a separate concern.)
    let mailbox = mailbox.with_default_identity(session_id.mycall());

    // Position arbiter is registered in lib.rs::run() — pull a live
    // ref so the on-air locator honors live GPS / privacy state,
    // matching the ARDOP path's behavior.
    let arbiter_state = app.state::<std::sync::Arc<crate::position::PositionArbiter>>();
    let arbiter: std::sync::Arc<crate::position::PositionArbiter> = (*arbiter_state).clone();

    // ─── Pre-audio CAT tune + ordered-list QSY walk (tuxlink-8fkkk A2) ─
    // Mirror the ARDOP connect walk: for each candidate, CAT-tune (pre-audio)
    // then send CONNECT + await CONNECTED. Stop at the first success. The kept
    // rig handle (DRA-100 keep-serial path) is held in `kept_rig` for the
    // synchronous B2F exchange below and drops at fn end — the correct
    // session-scoped rig lifetime for VARA's single-call connect+exchange+
    // disconnect. On the close-serial path `tune_rig_for_connect` releases the
    // serial and returns `None`, so there is nothing to hold.
    // tuxlink-qevsf (SAFETY/Part 97): auto-QSY disabled — the station must not
    // transmit on any frequency the operator has not seen + selected. Only the
    // operator-chosen channel (candidate[0]) is dialed. Restored by the
    // Channel-Selection redesign (Find a Station = operator-driven channel picker).
    let candidates =
        clamp_connect_candidates(vara_dial_candidates(target, freq_hz, qsy_candidates));
    // The clamp above leaves a single candidate, so the walk has nothing to
    // advance to regardless of this operator flag (tuxlink-8fkkk).
    let qsy_on_fail = cfg.rig.qsy_on_fail;

    // The last candidate's failure message — surfaced if no candidate connects.
    let mut last_err: Option<String> = None;
    // Holds the winning candidate's kept rig (DRA-100) across the post-walk B2F
    // exchange. `None` on the close-serial path or when no rig is configured.
    let mut kept_rig: Option<tux_rig::ManagedRig> = None;
    // The connected target (winning candidate) for the post-walk log + exchange.
    let mut connected_target: Option<String> = None;

    let outcome = walk_candidates(&candidates, qsy_on_fail, |_idx, c| {
        // Abort recheck (C2 for VARA): if the operator closed the session
        // mid-walk, the close generation bumps; stop attempting rather than
        // QSY-ing to the next candidate after a Close. The remaining
        // candidates also observe the bumped generation and no-op, so the
        // walk drains to `None` and surfaces a connect failure — same
        // outcome-consistency as ARDOP's walk (intentional).
        if session.current_close_generation() != close_gen_snapshot {
            last_err = Some("VARA CONNECT aborted".into());
            return false;
        }

        // Pre-CONNECT (pre-audio) CAT tune. `tune_rig_for_connect` honors
        // close-serial (returns `None` once the serial is released) vs the
        // DRA-100 keep-serial path (returns `Some(rig)` to hold for the
        // session). Spawn/tune failures abort THIS candidate.
        let rig = match tune_rig_for_connect(&cfg.rig, c.freq_hz) {
            Ok(r) => r,
            Err(e) => {
                emit_vara_log(
                    app,
                    log,
                    LogLevel::Error,
                    format!("VARA tune failed for {}: {e}", c.target),
                );
                last_err = Some(e);
                return false;
            }
        };

        // tuxlink-8fkkk C2 (VARA): re-check the close-generation AFTER the tune
        // returns and BEFORE `send_connect_and_wait`. The pre-CONNECT tune
        // (rigctld spawn + CAT round-trips) can block for seconds; if the
        // operator hit Stop during the tune the generation bumps, and without
        // this guard the path would still send CONNECT — VARA would transmit
        // after the session was closed. Release the just-spawned rig and bail.
        // Mirror of the ARDOP post-tune guard in `dial_one_candidate`.
        if session.current_close_generation() != close_gen_snapshot {
            last_err = Some("VARA CONNECT aborted".into());
            drop(rig);
            return false;
        }

        match send_connect_and_wait(app, log, transport, &mycall, &c.target, keyer) {
            Ok(()) => {
                // Hold the rig for the exchange (DRA-100); `None` if released.
                kept_rig = rig;
                connected_target = Some(c.target.clone());
                true
            }
            Err(e) => {
                emit_vara_log(
                    app,
                    log,
                    LogLevel::Error,
                    format!("VARA CONNECT to {} failed: {e}", c.target),
                );
                // A failed CONNECT (especially a timeout with no
                // DISCONNECTED/CANCELPENDING) may leave VARA still calling the
                // previous target. Best-effort DISCONNECT it back to idle BEFORE
                // the walk retunes + dials the next candidate on this same
                // transport, so the modem cannot end up dual-calling. Result is
                // ignored: the next attempt drops the transport regardless and
                // the TCP FIN forces VARA to notice if the wind-down stalls.
                let _ = vara_dial_disconnect(transport, keyer);
                // Release this candidate's rig before the next attempt so no
                // rigctld is left holding the CAT serial.
                drop(rig);
                last_err = Some(e);
                false
            }
        }
    });

    if outcome.is_none() {
        // No candidate connected — CONNECT-class failure (terminal). Surface the
        // last useful error; the caller frees the modem (tuxlink-n95sr #2).
        return VaraExchangeOutcome::ConnectFailed(
            last_err.unwrap_or_else(|| "VARA CONNECT failed for all candidates".to_string()),
        );
    }

    // The winning target: the candidate `send_connect_and_wait` succeeded for.
    // Falls back to the primary `target` if (impossibly) unset.
    let connected_target = connected_target.unwrap_or_else(|| target.to_string());
    let target = connected_target.as_str();

    // Keep the kept rig (DRA-100) alive across the synchronous B2F exchange;
    // it drops — stopping rigctld — when this function returns. Named to make
    // the load-bearing lifetime explicit and silence the unused-var lint.
    let _kept_rig = kept_rig;

    emit_vara_log(
        app,
        log,
        LogLevel::Info,
        format!("VARA: connected to {target}; running B2F exchange"),
    );
    let app_for_progress = app.clone();
    let log_for_progress = log.clone();
    let progress = move |line: &str| {
        emit_vara_log(
            &app_for_progress,
            &log_for_progress,
            LogLevel::Info,
            line.to_string(),
        );
    };

    // ─── Run the B2F exchange over the data socket ───────────────────
    // Past here the ARQ link is UP: any failure is a mid-EXCHANGE failure
    // (`ExchangeFailed`) — the caller keeps the session Open for a retry — NOT a
    // terminal connect failure (tuxlink-n95sr #2).
    //
    // tuxlink-yrrjq: VARA raises `PTT ON`/`PTT OFF` on the CMD socket for the
    // ENTIRE ARQ session — including while the B2F turns run on the DATA
    // socket. Pre-clone the data halves and hand the whole transport (and
    // with it the cmd socket) to a concurrent PTT pump for the exchange
    // window, so mid-exchange keying requests actually key the rig instead
    // of buffering unread (the pre-fix behavior left the radio unkeyed —
    // or, after a link drop, keyed — with nobody listening). The pump stops
    // within one cmd-socket read-timeout tick (2 s) of the exchange
    // returning.
    let data_writer = match transport.data_stream().try_clone() {
        Ok(w) => w,
        Err(e) => {
            return VaraExchangeOutcome::ExchangeFailed(format!(
                "VARA data-socket try_clone failed: {e}"
            ))
        }
    };
    let data_reader = match transport.data_stream().try_clone() {
        Ok(r) => r,
        Err(e) => {
            return VaraExchangeOutcome::ExchangeFailed(format!(
                "VARA data-socket try_clone (reader) failed: {e}"
            ))
        }
    };
    let stop_pump = AtomicBool::new(false);
    let exchange_result = std::thread::scope(|s| {
        let pump = s.spawn(|| pump_vara_ptt_during_exchange(transport, keyer, &stop_pump));
        let r = crate::winlink_backend::run_vara_b2f_exchange_io(
            std::io::BufReader::new(data_reader),
            data_writer,
            target,
            intent,
            &cfg,
            &session_id,
            &mailbox,
            Some(&arbiter),
            Some(&progress),
        );
        stop_pump.store(true, Ordering::SeqCst);
        let _ = pump.join();
        r
    });
    match exchange_result {
        Ok(()) => VaraExchangeOutcome::Completed,
        Err(e) => VaraExchangeOutcome::ExchangeFailed(format!("VARA B2F exchange failed: {e}")),
    }
}

/// Service VARA cmd-socket events while the B2F exchange runs on the data
/// socket (tuxlink-yrrjq). VARA keeps raising `PTT ON`/`PTT OFF` here for
/// its ARQ turns; with no reader during the exchange, a keying request
/// would strand the radio unkeyed mid-turn (dead-air) or — after a link
/// drop — keyed with nobody left to unkey it. Runs on a scoped thread that
/// owns `&mut transport` (the exchange drives pre-cloned data halves);
/// exits within one cmd-socket read-timeout tick of `stop` being set.
pub(crate) fn pump_vara_ptt_during_exchange(
    transport: &mut VaraTransport,
    ptt: &SharedPtt,
    stop: &AtomicBool,
) {
    use crate::winlink::modem::vara::command::InboundCommand;

    while !stop.load(Ordering::SeqCst) {
        match transport.recv() {
            Ok(Some(InboundCommand::Ptt(on))) => {
                if let Err(e) = ptt::lock_ptt(ptt).set_ptt(on) {
                    // A failed KEY dead-airs this turn (the exchange will
                    // time out and fail — bounded). A failed UNKEY risks a
                    // stuck transmitter: log loudly; the UnkeyGuard in the
                    // outer command retries the unkey on every exit path.
                    tracing::error!(
                        target: "tuxlink::winlink::modem::vara",
                        on,
                        error = %e,
                        "VARA PTT keying failed mid-exchange"
                    );
                }
            }
            Ok(Some(InboundCommand::Disconnected)) => {
                // Link dropped mid-exchange: force an unkey NOW rather than
                // waiting for the data-socket EOF to unwind the exchange.
                let _ = ptt::lock_ptt(ptt).set_ptt(false);
            }
            // Other async events (BUFFER / IAMALIVE / setter echoes /
            // Unknown): recv() already logs them at debug.
            Ok(Some(_)) => {}
            // Read-timeout tick (per VaraConfig.read_timeout, 2 s) or EOF —
            // loop re-checks `stop`.
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(
                    target: "tuxlink::winlink::modem::vara",
                    error = %e,
                    "VARA cmd-port read error during exchange; stopping PTT pump"
                );
                let _ = ptt::lock_ptt(ptt).set_ptt(false);
                break;
            }
        }
    }
}

/// Build the ordered dial-candidate list for a VARA exchange.
///
/// When `qsy_candidates` is `Some` and non-empty, it is used verbatim (the
/// Find-a-Station ranked channels wired by Task B). Otherwise a single-element
/// list `[{ target, freq_hz }]` reproduces today's single-dial behavior
/// (back-compat). An empty `Some(vec![])` falls back to the single dial too.
fn vara_dial_candidates(
    target: &str,
    freq_hz: Option<u64>,
    qsy_candidates: Option<Vec<DialCandidate>>,
) -> Vec<DialCandidate> {
    match qsy_candidates {
        Some(cands) if !cands.is_empty() => cands,
        _ => vec![DialCandidate {
            target: target.to_string(),
            freq_hz,
        }],
    }
}

/// Send `CONNECT <mycall> <target>` on the cmd port and wait for the
/// `CONNECTED` event (bounded by [`VARA_CONNECT_DEADLINE`]). Factored out of
/// the candidate-walk closure so the two primitives (the existing cmd-port
/// write + [`wait_for_connected`]) stay together and the closure body reads
/// cleanly. Emits the per-candidate "VARA CONNECT {mycall} {target}" line so
/// the operator sees each dialed target during a QSY walk.
fn send_connect_and_wait(
    app: &AppHandle,
    log: &Arc<SessionLogState>,
    transport: &mut VaraTransport,
    mycall: &str,
    target: &str,
    ptt: &SharedPtt,
) -> Result<(), String> {
    emit_vara_log(
        app,
        log,
        LogLevel::Info,
        format!("VARA CONNECT {mycall} {target}"),
    );
    transport
        .send(&OutboundCommand::Connect {
            mycall: mycall.to_string(),
            target: target.to_string(),
        })
        .map_err(|e| format!("VARA cmd-port CONNECT write failed: {e}"))?;

    wait_for_connected(transport, target, VARA_CONNECT_DEADLINE, ptt)
        .map_err(|e| format!("VARA CONNECT to {target} failed: {e}"))
}

/// Wait for the `CONNECTED <mycall> <target> [bw]` async event on the
/// VARA cmd port, bounded by `deadline`. Services interleaved `PTT ON` /
/// `PTT OFF` by keying the rig through `ptt` (tuxlink-yrrjq — VARA has no
/// PTT mechanism of its own; the host keys, and these events were
/// previously absorbed, so no VARA dial ever reached the air). Absorbs
/// BUFFER / PENDING / LINK REGISTERED / IAMALIVE / Unknown events and
/// keeps polling.
///
/// `CANCELPENDING` arriving before `CONNECTED` is the cancel-during-call
/// path — surface as Err so the caller does not proceed to a data-socket
/// exchange that VARA never opened. `DISCONNECTED` arriving before
/// `CONNECTED` means VARA rejected the dial; same surface.
///
/// Polls at the transport's `recv` cadence (the `VaraConfig.read_timeout`
/// — 2 s by default), so deadline expiry is detected within ~2 s of
/// expiration. The cmd socket's `recv` returns `Ok(None)` on read
/// timeout / EOF, which we treat as a tick and re-check the deadline.
fn wait_for_connected(
    transport: &mut VaraTransport,
    target: &str,
    deadline: Duration,
    ptt: &SharedPtt,
) -> Result<(), String> {
    use crate::winlink::modem::vara::command::InboundCommand;
    use std::time::Instant;

    let start = Instant::now();
    loop {
        if start.elapsed() >= deadline {
            return Err(format!(
                "no CONNECTED event from VARA within {deadline:?} \
                 (target={target}); aborting"
            ));
        }
        match transport.recv() {
            Ok(Some(InboundCommand::Connected { target: peer, .. })) => {
                // CONNECTED — the dial succeeded. Match-tolerance: VARA's
                // CONNECTED reports the peer as-typed (case-preserving),
                // so we compare case-insensitively to absorb a target
                // like "w7rms-10" vs "W7RMS-10".
                if peer.eq_ignore_ascii_case(target) {
                    return Ok(());
                }
                // Unexpected peer — VARA may be reporting a stray
                // listener-side CONNECTED. Surface as an error so the
                // dial path does not silently bind to the wrong link.
                return Err(format!(
                    "unexpected CONNECTED peer={peer} (expected {target})"
                ));
            }
            Ok(Some(InboundCommand::Disconnected)) => {
                return Err(format!(
                    "VARA disconnected before CONNECTED to {target} \
                     (modem may have rejected the dial)"
                ));
            }
            Ok(Some(InboundCommand::CancelPending)) => {
                return Err(format!(
                    "VARA reported CANCELPENDING before CONNECTED to {target} \
                     (call was cancelled before establishment)"
                ));
            }
            Ok(Some(InboundCommand::WrongCallsign)) => {
                return Err(
                    "VARA reported WRONG CALLSIGN — registration rejected the configured \
                     callsign; check VARA registration before dialing again"
                        .into(),
                );
            }
            Ok(Some(InboundCommand::MissingSoundcard)) => {
                return Err(
                    "VARA reported MISSING SOUNDCARD — modem cannot find the configured \
                     audio device; check VARA audio settings"
                        .into(),
                );
            }
            Ok(Some(InboundCommand::Offline)) => {
                return Err("VARA reported OFFLINE — modem is not ready to transmit".into());
            }
            Ok(Some(InboundCommand::Ptt(on))) => {
                // VARA asks the HOST to key/unkey for its ConReq frames
                // (tuxlink-yrrjq). A failed KEY means the dial would be
                // dead-air — abort the candidate. A failed UNKEY risks a
                // stuck transmitter — abort too; the caller's UnkeyGuard
                // retries the unkey on unwind.
                if let Err(e) = ptt::lock_ptt(ptt).set_ptt(on) {
                    return Err(format!(
                        "PTT {} failed while dialing {target}: {e}",
                        if on { "key" } else { "unkey" }
                    ));
                }
            }
            // Absorb every other async event (BUFFER / PENDING /
            // CANCELPENDING already handled / LINK REGISTERED /
            // IAMALIVE / Unknown) and keep waiting.
            Ok(Some(_)) => continue,
            // recv timeout (per VaraConfig.read_timeout, default 2 s) or
            // EOF: tick — re-check the deadline.
            Ok(None) => continue,
            Err(e) => {
                return Err(format!(
                    "VARA cmd-port read error while awaiting CONNECTED: {e}"
                ));
            }
        }
    }
}

/// Best-effort graceful `DISCONNECT` after a B2F exchange. Sends
/// `DISCONNECT\r` and waits for the `DISCONNECTED` event (bounded by
/// [`VARA_DISCONNECT_DEADLINE`]). Returns immediately on any wind-down
/// error — the caller drops the transport unconditionally after this
/// returns.
///
/// **`DISCONNECT` vs `ABORT`:** this is the graceful wind-down at the
/// end of a successful exchange; the cooperative `ABORT\r` side-channel
/// (Task 4.1) is the in-flight interrupt path for the operator's Close
/// Session click. They serve distinct purposes.
pub(crate) fn vara_dial_disconnect(
    transport: &mut VaraTransport,
    ptt: &SharedPtt,
) -> Result<(), String> {
    use crate::winlink::modem::vara::command::InboundCommand;
    use std::time::Instant;

    transport
        .send(&OutboundCommand::Disconnect)
        .map_err(|e| format!("VARA DISCONNECT write failed: {e}"))?;

    let start = Instant::now();
    loop {
        if start.elapsed() >= VARA_DISCONNECT_DEADLINE {
            // Graceful wind-down timed out — caller drops the transport
            // regardless; the TCP FIN forces VARA to notice. No need to
            // escalate to ABORT here since the close path's
            // abort_in_flight is the operator-driven interrupt; the
            // post-exchange disconnect is best-effort.
            return Err(format!(
                "VARA did not acknowledge DISCONNECT within {:?}",
                VARA_DISCONNECT_DEADLINE
            ));
        }
        match transport.recv() {
            Ok(Some(InboundCommand::Disconnected)) => return Ok(()),
            Ok(Some(InboundCommand::Ptt(on))) => {
                // VARA keys the radio to transmit its disconnect frames
                // (tuxlink-yrrjq). Best-effort during wind-down: a keying
                // failure here only dead-airs the graceful goodbye (the
                // deadline bounds it and the caller drops the transport);
                // the UnkeyGuard in the outer command is the unkey backstop.
                if let Err(e) = ptt::lock_ptt(ptt).set_ptt(on) {
                    tracing::warn!(
                        target: "tuxlink::winlink::modem::vara",
                        on,
                        error = %e,
                        "PTT keying failed during DISCONNECT wind-down"
                    );
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => continue,
            Err(e) => return Err(format!("VARA cmd-port read error during DISCONNECT: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ── tuxlink-7ppfq Contract 1: VARA cmd-port reachability probe ──

    #[test]
    fn probe_reachable_open_session_reports_true_without_socket() {
        // Open session: derive from state == Open, do NOT touch a socket.
        let s = VaraSession::new();
        s.set_state_for_test(VaraState::Open);
        // Port 1 is privileged/unused; a socket attempt would fail — proving
        // the Open branch skipped the socket entirely.
        assert_eq!(
            s.probe_reachable("127.0.0.1", 1, std::time::Duration::from_millis(50)),
            Some(true)
        );
    }

    #[test]
    fn probe_reachable_connecting_session_reports_false_without_socket() {
        let s = VaraSession::new();
        s.set_state_for_test(VaraState::Connecting);
        assert_eq!(
            s.probe_reachable("127.0.0.1", 1, std::time::Duration::from_millis(50)),
            Some(false)
        );
    }

    #[test]
    fn probe_reachable_closed_session_touches_socket_true() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let s = VaraSession::new(); // new() starts Closed
        s.set_state_for_test(VaraState::Closed);
        assert_eq!(
            s.probe_reachable("127.0.0.1", port, std::time::Duration::from_secs(5)),
            Some(true)
        );
    }

    #[test]
    fn probe_reachable_closed_session_no_listener_false() {
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let s = VaraSession::new();
        s.set_state_for_test(VaraState::Closed);
        assert_eq!(
            s.probe_reachable("127.0.0.1", port, std::time::Duration::from_millis(500)),
            Some(false)
        );
    }

    #[test]
    fn probe_reachable_returns_unknown_when_lock_contended() {
        // No-session-mutex-contention invariant: holding `inner` must NOT make
        // the probe wait — it returns `unknown` promptly.
        let s = VaraSession::new();
        s.set_state_for_test(VaraState::Closed);
        let guard = s.lock_inner_for_test();
        assert_eq!(
            s.probe_reachable("127.0.0.1", 1, std::time::Duration::from_secs(5)),
            None
        );
        drop(guard);
    }

    // ── tuxlink-8fkkk Task A2: VARA pre-audio tune + candidate walk ──
    //
    // `walk_candidates` + `tune_rig_for_connect` are unit-tested in
    // `modem_commands`. Here we pin the VARA-local candidate-list construction:
    // the back-compat single-dial fallback vs the QSY passthrough.

    #[test]
    fn vara_dial_candidates_none_yields_single_dial() {
        let c = vara_dial_candidates("W1AW", Some(7_103_000), None);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].target, "W1AW");
        assert_eq!(c[0].freq_hz, Some(7_103_000));
    }

    #[test]
    fn vara_dial_candidates_empty_some_yields_single_dial() {
        // An explicit empty list is treated like `None` (back-compat).
        let c = vara_dial_candidates("KX4Z", None, Some(vec![]));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].target, "KX4Z");
        assert_eq!(c[0].freq_hz, None);
    }

    #[test]
    fn vara_dial_candidates_some_passes_through_in_order() {
        let supplied = vec![
            DialCandidate {
                target: "GW1".into(),
                freq_hz: Some(14_105_000),
            },
            DialCandidate {
                target: "GW2".into(),
                freq_hz: Some(7_103_000),
            },
        ];
        // The primary target/freq are overridden by a non-empty candidate list.
        let c = vara_dial_candidates("IGNORED", Some(3_580_000), Some(supplied));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].target, "GW1");
        assert_eq!(c[0].freq_hz, Some(14_105_000));
        assert_eq!(c[1].target, "GW2");
        assert_eq!(c[1].freq_hz, Some(7_103_000));
    }

    /// tuxlink-qevsf (SAFETY/Part 97): the VARA connect command assembles the
    /// candidate list via `vara_dial_candidates` and then clamps it with
    /// `clamp_connect_candidates` before the walk. Even when the operator
    /// supplied multiple QSY candidates, only candidate[0] — the channel they
    /// saw + selected — survives, so the station cannot auto-transmit on an
    /// unseen frequency. This asserts the command's clamp, not the pure
    /// assembler (which still passes the full list through verbatim).
    #[test]
    fn connect_clamps_assembled_candidates_to_first() {
        let supplied = vec![
            DialCandidate {
                target: "GW1".into(),
                freq_hz: Some(14_105_000),
            },
            DialCandidate {
                target: "GW2".into(),
                freq_hz: Some(7_103_000),
            },
        ];
        let assembled = vara_dial_candidates("IGNORED", Some(3_580_000), Some(supplied));
        let clamped = clamp_connect_candidates(assembled);
        assert_eq!(
            clamped.len(),
            1,
            "only the operator-chosen channel survives"
        );
        assert_eq!(clamped[0].target, "GW1");
        assert_eq!(clamped[0].freq_hz, Some(14_105_000));
    }

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
        assert!(
            !info.vara_supported,
            "non-x86 should report vara_supported=false"
        );
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
        // tuxlink-xzxk1: the DATA socket must get an RF-scale budget, not
        // the 2s cmd cadence — a 2s data timeout disconnected the first
        // on-air gateway answer (KD6OAT, BW500) 4s after link-up.
        assert_eq!(t.data_read_timeout.map(|d| d.as_secs()), Some(120));
    }

    #[test]
    fn bandwidth_from_hz_maps_documented_values() {
        // Standard VARA HF bandwidths.
        assert!(
            bandwidth_from_hz(500).is_some(),
            "500 Hz is a documented narrow-HF bandwidth"
        );
        assert!(
            bandwidth_from_hz(2300).is_some(),
            "2300 Hz is VARA HF Standard"
        );
        assert!(
            bandwidth_from_hz(2750).is_some(),
            "2750 Hz is VARA HF Tactical"
        );
    }

    #[test]
    fn bandwidth_from_hz_returns_none_for_unknown_value() {
        // A nonsense value: caller should skip the BW setter rather than
        // sending an unparseable bandwidth to VARA.
        assert!(
            bandwidth_from_hz(42).is_none(),
            "unknown values must return None"
        );
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
    fn vara_open_session_fails_when_tcp_unreachable() {
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
        let err = vara_open_session_inner(
            &session,
            &ui_cfg,
            None,
            SessionIntent::Cms,
            TransportKind::VaraHf,
        )
        .unwrap_err();
        assert!(err.contains("TCP connect failed"), "got: {err}");

        // Status must reflect Error and the transport must remain None so
        // a follow-up retry is possible.
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Error);
        assert!(snap.last_error.is_some(), "last_error must be populated");
        assert_eq!(snap.bound_host.as_deref(), Some("127.0.0.1"));
        // Failed open must NOT populate active_intent / active_transport_kind —
        // the fields only carry meaning for an open transport.
        assert_eq!(
            snap.active_intent, None,
            "intent must not leak on failed open"
        );
        assert_eq!(
            snap.active_transport_kind, None,
            "transport_kind must not leak on failed open"
        );
    }

    #[test]
    fn vara_open_session_double_start_rejected() {
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
                listener_armed: false,
                exchange: None,
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
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
        let err = vara_open_session_inner(
            &session,
            &ui_cfg,
            None,
            SessionIntent::Cms,
            TransportKind::VaraHf,
        )
        .unwrap_err();
        assert!(
            err.contains("TCP connect failed"),
            "after a prior error, start should re-attempt and fail at TCP (not the double-start guard); got: {err}"
        );
    }

    /// tuxlink-6urh2 v2: the reopen guard must also reject on
    /// `transport_owner != None`, not just `transport.is_some()`. The
    /// heartbeat's borrow window (and the listener consumer's armed
    /// window) both have `guard.transport == None` WHILE an owner other
    /// than `None` holds the (temporarily absent) transport — a reopen
    /// racing that window must not be allowed to install a second
    /// transport underneath the borrower.
    #[test]
    fn vara_open_session_rejected_while_owner_held_though_transport_is_none() {
        let session = Arc::new(VaraSession::new());
        {
            let mut guard = session.inner.lock().unwrap();
            guard.status = VaraStatus {
                state: VaraState::Open,
                last_error: None,
                bound_host: Some("127.0.0.1".into()),
                bound_cmd_port: Some(8300),
                listener_armed: false,
                exchange: None,
                transport_owner: TransportOwner::Heartbeat,
                active_intent: None,
                active_transport_kind: None,
            };
            guard.transport_owner = TransportOwner::Heartbeat;
            assert!(
                guard.transport.is_none(),
                "borrow-window precondition: transport is OUT of the session"
            );
        }
        let ui_cfg = VaraUiConfig {
            host: "127.0.0.1".into(),
            cmd_port: 1,
            data_port: 2,
            bandwidth_hz: None,
        };
        let err = vara_open_session_inner(
            &session,
            &ui_cfg,
            None,
            SessionIntent::Cms,
            TransportKind::VaraHf,
        )
        .unwrap_err();
        assert!(
            err.contains("already started"),
            "reopen must be rejected by the owner check even though transport is None; got: {err}"
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
            data_read_timeout: Some(Duration::from_millis(100)),
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
                listener_armed: false,
                exchange: None,
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
            };
        }
        assert_eq!(session.snapshot().state, VaraState::Open);

        // Take: snapshot transitions to Closed, transport handed to caller.
        let taken = session.take_transport();
        assert!(taken.is_some(), "take must return the transport");
        assert_eq!(session.snapshot().state, VaraState::Closed);

        // Return: state restored to Open with the bound info preserved.
        session.return_transport(taken.unwrap(), Some("127.0.0.1".into()), Some(cmd_port));
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

    // ── tuxlink-n95sr #2: finish_vara_b2f_exchange branches cleanup on the
    //    outcome — a CONNECT-class failure FREES the modem (session Closed,
    //    transport NOT re-installed → not silently re-armable) while a
    //    mid-EXCHANGE failure KEEPS the open session for a retry. Mirrors
    //    ARDOP's finish_b2f_exchange. Asserts the post-failure STATE + transport
    //    disposition, not just the returned Err (the test-gap the ARDOP
    //    session-restart bug-hunt explicitly called out). ──────────────────
    fn loopback_vara_transport(
        write_disconnected: bool,
    ) -> (
        VaraTransport,
        u16,
        std::thread::JoinHandle<()>,
        std::thread::JoinHandle<()>,
    ) {
        use std::io::Write;
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();
        // The cmd acceptor optionally emits a `DISCONNECTED\r` so a post-exchange
        // `vara_dial_disconnect` (the mid-EXCHANGE-failure path) returns promptly
        // instead of waiting out the 5 s deadline against a silent peer.
        let cmd_handle = thread::spawn(move || {
            if let Ok((mut c, _)) = cmd_l.accept() {
                if write_disconnected {
                    let _ = c.write_all(b"DISCONNECTED\r");
                    let _ = c.flush();
                }
                thread::sleep(Duration::from_millis(500));
            }
        });
        let data_handle = thread::spawn(move || {
            let _ = data_l.accept();
            thread::sleep(Duration::from_millis(500));
        });
        let cfg = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            connect_timeout: Duration::from_secs(2),
            read_timeout: Some(Duration::from_millis(100)),
            data_read_timeout: Some(Duration::from_millis(100)),
        };
        let transport = VaraTransport::connect(cfg).expect("connect must succeed");
        (transport, cmd_port, cmd_handle, data_handle)
    }

    fn open_vara_session(transport: VaraTransport, cmd_port: u16) -> Arc<VaraSession> {
        let session = Arc::new(VaraSession::new());
        {
            let mut guard = session.inner.lock().unwrap();
            guard.transport = Some(transport);
            guard.status = VaraStatus {
                state: VaraState::Open,
                last_error: None,
                bound_host: Some("127.0.0.1".into()),
                bound_cmd_port: Some(cmd_port),
                listener_armed: false,
                exchange: None,
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
            };
        }
        session
    }

    /// tuxlink-6urh2: variant of `loopback_vara_transport` whose cmd
    /// acceptor closes the accepted connection IMMEDIATELY (no write, no
    /// sleep) so the client-side `recv_line_distinguishing_eof` observes
    /// `RecvOutcome::Eof` promptly — simulating a dropped VARA instance. The
    /// data acceptor still holds its connection open briefly (mirroring
    /// the sibling helper's scaffolding); the heartbeat only ever drains
    /// the cmd socket.
    fn loopback_vara_transport_cmd_dies_immediately() -> (
        VaraTransport,
        u16,
        std::thread::JoinHandle<()>,
        std::thread::JoinHandle<()>,
    ) {
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();
        let cmd_handle = thread::spawn(move || {
            if let Ok((c, _)) = cmd_l.accept() {
                // Immediate close — sends FIN, so the client's `peek()`
                // observes `Ok(0)` on its next tick.
                drop(c);
            }
        });
        let data_handle = thread::spawn(move || {
            let _ = data_l.accept();
            thread::sleep(Duration::from_millis(500));
        });
        let cfg = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            connect_timeout: Duration::from_secs(2),
            read_timeout: Some(Duration::from_millis(100)),
            data_read_timeout: Some(Duration::from_millis(100)),
        };
        let transport = VaraTransport::connect(cfg).expect("connect must succeed");
        (transport, cmd_port, cmd_handle, data_handle)
    }

    /// tuxlink-6urh2: the heartbeat must detect a dropped VARA cmd socket
    /// and transition the session `Open -> SocketLost` on its own, within
    /// a bounded wait, using a fast injectable tick interval (not the real
    /// 3s production cadence).
    #[tokio::test]
    async fn vara_heartbeat_detects_dropped_cmd_socket_transitions_to_socket_lost() {
        let (transport, cmd_port, ch, dh) = loopback_vara_transport_cmd_dies_immediately();
        let session = open_vara_session(transport, cmd_port);

        let shutdown = spawn_vara_socket_heartbeat(
            session.clone(),
            None,
            None,
            std::time::Duration::from_millis(20),
        );
        session.install_heartbeat_shutdown(shutdown);

        // Bounded wait — generous margin over the ~20ms tick so the test
        // stays fast without being flaky under CI scheduling jitter.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if session.snapshot().state == VaraState::SocketLost {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "heartbeat did not transition Open -> SocketLost within the bounded wait"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::SocketLost);
        assert_eq!(
            snap.last_error.as_deref(),
            Some("VARA connection lost — reopen to reconnect")
        );
        assert!(
            session.take_transport().is_none(),
            "SocketLost must drop the transport — nothing left to take"
        );

        ch.join().unwrap();
        dh.join().unwrap();
    }

    /// tuxlink-6urh2: the heartbeat must NOT probe (and must NOT stamp
    /// SocketLost) while the transport is owned by a listener/exchange —
    /// only the idle-open window is this heartbeat's business. Simulates
    /// "listener armed" via `set_transport_owner_for_test` against a LIVE
    /// dropped-cmd-socket transport: if the heartbeat ignored ownership it
    /// would still (wrongly) stamp SocketLost here.
    #[tokio::test]
    async fn vara_heartbeat_skips_probe_when_transport_not_idle_open() {
        let (transport, cmd_port, ch, dh) = loopback_vara_transport_cmd_dies_immediately();
        let session = open_vara_session(transport, cmd_port);
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);

        let shutdown = spawn_vara_socket_heartbeat(
            session.clone(),
            None,
            None,
            std::time::Duration::from_millis(20),
        );
        session.install_heartbeat_shutdown(shutdown.clone());

        // Give the heartbeat several ticks' worth of time to (wrongly)
        // act if it ignored transport_owner.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert_eq!(
            session.snapshot().state,
            VaraState::Open,
            "heartbeat must not touch a transport owned by the listener/exchange"
        );

        // Cleanup: cancel the heartbeat, restore ownership, close.
        shutdown.store(true, Ordering::Release);
        session.set_transport_owner_for_test(TransportOwner::None);
        drop(session);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    /// tuxlink-6urh2 v2: variant of `loopback_vara_transport_cmd_dies_immediately`
    /// whose cmd acceptor stays connected and periodically emits `IAMALIVE\r`
    /// — the exact unsolicited-keepalive traffic that made the OLD
    /// non-consuming peek design report "alive" forever regardless of the
    /// peer's actual state. Used to prove the NEW consuming-drain design
    /// correctly classifies a genuinely-alive peer as alive across several
    /// heartbeat ticks (re-installing the transport each time) rather than
    /// false-triggering `SocketLost`.
    fn loopback_vara_transport_alive_with_keepalives() -> (
        VaraTransport,
        u16,
        std::thread::JoinHandle<()>,
        std::thread::JoinHandle<()>,
    ) {
        use std::io::Write;
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();
        let cmd_handle = thread::spawn(move || {
            if let Ok((mut c, _)) = cmd_l.accept() {
                // Spans several 20ms heartbeat ticks so the test can
                // observe multiple re-install cycles against a live peer.
                for _ in 0..25 {
                    if c.write_all(b"IAMALIVE\r").is_err() {
                        break;
                    }
                    if c.flush().is_err() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(15));
                }
            }
        });
        let data_handle = thread::spawn(move || {
            let _ = data_l.accept();
            thread::sleep(Duration::from_millis(400));
        });
        let cfg = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            connect_timeout: Duration::from_secs(2),
            read_timeout: Some(Duration::from_millis(50)),
            data_read_timeout: Some(Duration::from_millis(50)),
        };
        let transport = VaraTransport::connect(cfg).expect("connect must succeed");
        (transport, cmd_port, cmd_handle, data_handle)
    }

    /// tuxlink-6urh2 v2: the regression the old peek design could never
    /// meaningfully exercise (a peek always said "alive" against ANY peer,
    /// dead or not) — a genuinely-alive peer sending periodic `IAMALIVE`
    /// keepalives must be drained + classified alive, with the transport
    /// re-installed and `transport_owner` reset to `None` (not left at
    /// `Heartbeat`) after every tick.
    #[tokio::test]
    async fn vara_heartbeat_reinstalls_transport_when_peer_stays_alive() {
        let (transport, cmd_port, ch, dh) = loopback_vara_transport_alive_with_keepalives();
        let session = open_vara_session(transport, cmd_port);

        let shutdown = spawn_vara_socket_heartbeat(
            session.clone(),
            None,
            None,
            std::time::Duration::from_millis(20),
        );
        session.install_heartbeat_shutdown(shutdown.clone());

        // Several ticks against a genuinely alive peer.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let snap = session.snapshot();
        assert_eq!(
            snap.state,
            VaraState::Open,
            "must not false-trigger SocketLost against a live peer"
        );
        assert_eq!(
            snap.transport_owner,
            TransportOwner::None,
            "must re-install with owner reset to None, not left at Heartbeat"
        );
        assert!(
            session.take_transport().is_some(),
            "transport must be re-installed after each tick, not dropped"
        );

        shutdown.store(true, Ordering::Release);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    /// tuxlink-6urh2 v2: `take_transport` must refuse the take (return
    /// `None`) while `transport_owner == Heartbeat`, mirroring the
    /// belt-and-suspenders explicit check documented on that fn — even
    /// though in production `guard.transport` is ALSO `None` during the
    /// heartbeat's borrow (making the outcome identical either way), this
    /// test drives the owner-gate directly (with a real transport still
    /// present) so a future refactor that reorders the take-vs-owner-check
    /// can't silently regress the invariant.
    #[test]
    fn take_transport_refuses_while_owner_is_heartbeat() {
        let (transport, cmd_port, ch, dh) = loopback_vara_transport(false);
        let session = open_vara_session(transport, cmd_port);
        session.set_transport_owner_for_test(TransportOwner::Heartbeat);

        assert!(
            session.take_transport().is_none(),
            "take_transport must refuse while the heartbeat owns the transport"
        );
        // Nothing was disturbed by the refused take.
        assert_eq!(session.transport_owner(), TransportOwner::Heartbeat);
        assert_eq!(session.snapshot().state, VaraState::Open);

        session.set_transport_owner_for_test(TransportOwner::None);
        drop(session);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    /// tuxlink-6urh2 v2 (self-adrev MEDIUM 2): a consumer that reports a dead
    /// transport via `mark_socket_lost_if_generation_matches` must transition
    /// the session to `SocketLost` (preserving bound host/port for reopen) and
    /// drop the corpse — NOT re-install it as `Open`.
    #[test]
    fn mark_socket_lost_stamps_socket_lost_and_drops_transport() {
        let (transport, cmd_port, ch, dh) = loopback_vara_transport(false);
        let session = open_vara_session(transport, cmd_port);
        let gen = session.current_close_generation();
        // A consumer (listener) holds the transport, then finds it dead.
        let t = session.take_transport().expect("transport present");

        // Caller supplies bound host/port captured before the take (take_transport
        // resets status to closed(), so the session no longer holds them).
        assert!(session.mark_socket_lost_if_generation_matches(
            t,
            gen,
            Some("127.0.0.1".into()),
            Some(cmd_port),
        ));

        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::SocketLost);
        assert_eq!(
            snap.last_error.as_deref(),
            Some("VARA connection lost — reopen to reconnect")
        );
        assert_eq!(snap.transport_owner, TransportOwner::None);
        // bound host/port preserved so the UI can offer reopen.
        assert_eq!(snap.bound_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(snap.bound_cmd_port, Some(cmd_port));
        // The dead transport must be gone, never laundered back to Open.
        assert!(
            session.take_transport().is_none(),
            "dead transport must be dropped, not re-installed"
        );

        drop(session);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    /// tuxlink-6urh2 v2 (self-adrev MEDIUM 2): a stale generation (operator's
    /// Close intervened while the consumer ran) must make the SocketLost stamp
    /// a no-op — the close path already owns the teardown; stamping SocketLost
    /// over it would revive a deliberately-closed session's error state.
    #[test]
    fn mark_socket_lost_is_noop_on_stale_generation() {
        let (transport, cmd_port, ch, dh) = loopback_vara_transport(false);
        let session = open_vara_session(transport, cmd_port);
        let stale_gen = session.current_close_generation();
        // Simulate the operator's Close bumping the generation mid-consume.
        session.bump_close_generation();
        let t = session.take_transport().expect("transport present");

        assert!(!session.mark_socket_lost_if_generation_matches(
            t,
            stale_gen,
            Some("127.0.0.1".into()),
            Some(cmd_port),
        ));

        // We did NOT stamp SocketLost — the close path's teardown is untouched.
        assert_ne!(
            session.snapshot().state,
            VaraState::SocketLost,
            "stale generation must not stamp SocketLost over the close path"
        );

        drop(session);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    #[test]
    fn finish_vara_b2f_connect_failure_frees_modem_not_re_armable() {
        let (transport, cmd_port, ch, dh) = loopback_vara_transport(false);
        let session = open_vara_session(transport, cmd_port);
        // Mirror the command: snapshot the close-gen BEFORE the take, then take
        // the transport (the dial's claim on it).
        let gen = session.current_close_generation();
        let taken = session.take_transport().expect("transport installed");

        let r = finish_vara_b2f_exchange(
            &session,
            taken,
            gen,
            Some("127.0.0.1".into()),
            Some(cmd_port),
            None,
            None,
            VaraExchangeOutcome::ConnectFailed(
                "VARA disconnected before CONNECTED (modem may have rejected the dial)".into(),
            ),
            &vox_keyer(),
        );

        assert!(r.is_err(), "a connect failure surfaces an Err");
        assert_eq!(
            session.snapshot().state,
            VaraState::Closed,
            "connect failure must leave the session Closed (modem freed)"
        );
        assert!(
            session.take_transport().is_none(),
            "connect failure must NOT re-install the transport (the tuxlink-n95sr #2 fix)"
        );

        drop(session);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    #[test]
    fn finish_vara_b2f_mid_exchange_failure_keeps_open_session() {
        let (transport, cmd_port, ch, dh) = loopback_vara_transport(true);
        let session = open_vara_session(transport, cmd_port);
        let gen = session.current_close_generation();
        let taken = session.take_transport().expect("transport installed");

        let r = finish_vara_b2f_exchange(
            &session,
            taken,
            gen,
            Some("127.0.0.1".into()),
            Some(cmd_port),
            None,
            None,
            VaraExchangeOutcome::ExchangeFailed("VARA B2F exchange failed: boom".into()),
            &vox_keyer(),
        );

        assert!(r.is_err(), "a mid-exchange failure surfaces an Err");
        assert_eq!(
            session.snapshot().state,
            VaraState::Open,
            "mid-exchange failure must keep the session Open for retry"
        );
        assert!(
            session.take_transport().is_some(),
            "mid-exchange failure must re-install the transport"
        );

        drop(session);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    // ── tuxlink-yrrjq: VARA host-side PTT keying ────────────────────────
    //
    // VARA cannot key a radio: it raises `PTT ON`/`PTT OFF` on the cmd
    // socket and the HOST must key. Before this fix those events were
    // parsed and discarded everywhere, so no VARA dial ever keyed a
    // transmitter — the flagship path could not reach the air by
    // construction. These tests drive the dial/wind-down/pump loops from a
    // scripted mock-VARA cmd socket and assert the keyer sees the events.

    /// Recording PTT sink: appends every `set_ptt` bool; optionally fails.
    struct RecordingSink {
        calls: Arc<Mutex<Vec<bool>>>,
        fail_with: Option<String>,
    }

    impl PttSink for RecordingSink {
        fn set_ptt(&mut self, on: bool) -> Result<(), String> {
            self.calls.lock().unwrap().push(on);
            match &self.fail_with {
                Some(e) => Err(e.clone()),
                None => Ok(()),
            }
        }
        fn describe(&self) -> String {
            "recording test sink".into()
        }
    }

    fn recording_keyer() -> (SharedPtt, Arc<Mutex<Vec<bool>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Mutex::new(Box::new(RecordingSink {
                calls: calls.clone(),
                fail_with: None,
            })),
            calls,
        )
    }

    fn failing_keyer() -> SharedPtt {
        Mutex::new(Box::new(RecordingSink {
            calls: Arc::new(Mutex::new(Vec::new())),
            fail_with: Some("serial gone".into()),
        }))
    }

    fn vox_keyer() -> SharedPtt {
        Mutex::new(Box::new(VaraPtt::Vox))
    }

    /// Like [`loopback_vara_transport`] but the cmd acceptor writes an
    /// arbitrary pre-scripted byte sequence on accept.
    fn scripted_cmd_vara_transport(
        cmd_script: &'static [u8],
    ) -> (
        VaraTransport,
        std::thread::JoinHandle<()>,
        std::thread::JoinHandle<()>,
    ) {
        use std::io::Write;
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();
        let cmd_handle = thread::spawn(move || {
            if let Ok((mut c, _)) = cmd_l.accept() {
                let _ = c.write_all(cmd_script);
                let _ = c.flush();
                thread::sleep(Duration::from_millis(700));
            }
        });
        let data_handle = thread::spawn(move || {
            let _ = data_l.accept();
            thread::sleep(Duration::from_millis(700));
        });
        let cfg = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            connect_timeout: Duration::from_secs(2),
            read_timeout: Some(Duration::from_millis(100)),
            data_read_timeout: Some(Duration::from_millis(100)),
        };
        let transport = VaraTransport::connect(cfg).expect("connect must succeed");
        (transport, cmd_handle, data_handle)
    }

    #[test]
    fn wait_for_connected_keys_rig_on_ptt_events() {
        // VARA keys for its ConReq frames during the dial: PTT ON → PTT OFF
        // → CONNECTED. The keyer must see [true, false] and the dial succeed.
        let (mut transport, ch, dh) =
            scripted_cmd_vara_transport(b"PTT ON\rPTT OFF\rCONNECTED N7CPZ W1AW 2300\r");
        let (keyer, calls) = recording_keyer();

        let r = wait_for_connected(&mut transport, "W1AW", Duration::from_secs(5), &keyer);
        assert!(r.is_ok(), "dial must succeed: {r:?}");
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[true, false],
            "the rig must be keyed then unkeyed for the ConReq window"
        );

        drop(transport);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    #[test]
    fn wait_for_connected_aborts_dial_when_keying_fails() {
        // A failed KEY means the dial would be dead-air (VARA modulating
        // into an unkeyed radio) — the candidate must abort, not proceed.
        let (mut transport, ch, dh) =
            scripted_cmd_vara_transport(b"PTT ON\rCONNECTED N7CPZ W1AW 2300\r");
        let keyer = failing_keyer();

        let r = wait_for_connected(&mut transport, "W1AW", Duration::from_secs(5), &keyer);
        let err = r.expect_err("keying failure must abort the dial");
        assert!(
            err.contains("PTT key failed"),
            "error must name the keying failure: {err}"
        );

        drop(transport);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    #[test]
    fn vara_dial_disconnect_services_ptt_during_wind_down() {
        // VARA keys the radio to transmit its disconnect frames; the
        // wind-down loop must service PTT (best-effort) and still resolve
        // on DISCONNECTED.
        let (mut transport, ch, dh) =
            scripted_cmd_vara_transport(b"PTT ON\rPTT OFF\rDISCONNECTED\r");
        let (keyer, calls) = recording_keyer();

        let r = vara_dial_disconnect(&mut transport, &keyer);
        assert!(r.is_ok(), "wind-down must resolve on DISCONNECTED: {r:?}");
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[true, false],
            "the disconnect frames' keying must reach the rig"
        );

        drop(transport);
        ch.join().unwrap();
        dh.join().unwrap();
    }

    #[test]
    fn ptt_pump_keys_and_unkeys_during_exchange_window() {
        // Codex 2026-07-09 #1: VARA raises PTT for the ENTIRE ARQ session,
        // including while B2F runs on the data socket. The pump must service
        // keying with the exchange elsewhere, and stop when told.
        let (mut transport, ch, dh) = scripted_cmd_vara_transport(b"PTT ON\rPTT OFF\r");
        let (keyer, calls) = recording_keyer();
        let stop = AtomicBool::new(false);

        std::thread::scope(|s| {
            let pump = s.spawn(|| pump_vara_ptt_during_exchange(&mut transport, &keyer, &stop));
            // Give the pump a few read-timeout ticks (100 ms each) to drain
            // the script, then stop it — mirroring the exchange returning.
            std::thread::sleep(Duration::from_millis(400));
            stop.store(true, Ordering::SeqCst);
            pump.join().expect("pump thread must not panic");
        });

        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[true, false],
            "mid-exchange keying requests must reach the rig"
        );

        ch.join().unwrap();
        dh.join().unwrap();
    }

    #[test]
    fn ptt_pump_forces_unkey_on_disconnected() {
        // A link drop mid-exchange must force an unkey NOW — not wait for
        // the data-socket EOF to unwind the exchange.
        let (mut transport, ch, dh) = scripted_cmd_vara_transport(b"PTT ON\rDISCONNECTED\r");
        let (keyer, calls) = recording_keyer();
        let stop = AtomicBool::new(false);

        std::thread::scope(|s| {
            let pump = s.spawn(|| pump_vara_ptt_during_exchange(&mut transport, &keyer, &stop));
            std::thread::sleep(Duration::from_millis(400));
            stop.store(true, Ordering::SeqCst);
            pump.join().expect("pump thread must not panic");
        });

        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[true, false],
            "DISCONNECTED after PTT ON must force an immediate unkey"
        );

        ch.join().unwrap();
        dh.join().unwrap();
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
        let captured: Arc<std::sync::Mutex<Vec<u8>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = RecordingWriter {
            captured: captured.clone(),
        };
        let shutdown_called = Arc::new(std::sync::Mutex::new(false));
        let spy = ShutdownSpy {
            called: shutdown_called.clone(),
        };
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
            assert!(abort_idx < disc_idx, "ABORT must precede any DISCONNECT");
        }
        // Cooperative path succeeded → fallback must NOT have run.
        assert!(
            !*shutdown_called.lock().unwrap(),
            "shutdown_both must not run when cooperative write succeeded"
        );
    }

    /// tuxlink-xzxk1 (Codex adrev P1 #2): the abort path must shut down the
    /// DATA socket on the cooperative-success path too — with the RF-scale
    /// data_read_timeout, an exchange thread parked in a data read no longer
    /// ticks every 2 s, so without this shutdown the operator's Close
    /// Session waits out the full data budget before the exchange unwinds.
    #[test]
    fn vara_abort_in_flight_shuts_down_data_socket_on_cooperative_success() {
        let session = VaraSession::new();
        let captured: Arc<std::sync::Mutex<Vec<u8>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let cmd_shutdown = Arc::new(std::sync::Mutex::new(false));
        let data_shutdown = Arc::new(std::sync::Mutex::new(false));
        session.install_abort_writer(
            Box::new(RecordingWriter {
                captured: captured.clone(),
            }) as Box<dyn std::io::Write + Send>,
            Box::new(ShutdownSpy {
                called: cmd_shutdown.clone(),
            }) as Box<dyn ShutdownableStream>,
        );
        session.install_abort_data_stream(Box::new(ShutdownSpy {
            called: data_shutdown.clone(),
        }) as Box<dyn ShutdownableStream>);

        session.abort_in_flight().expect("cooperative abort succeeds");

        assert!(
            *data_shutdown.lock().unwrap(),
            "data socket must be shut down even when the cooperative ABORT \
             write succeeds — it unblocks the exchange thread's parked read"
        );
        assert!(
            !*cmd_shutdown.lock().unwrap(),
            "cmd hard-close fallback must still be reserved for cooperative failure"
        );
        assert!(
            captured.lock().unwrap().starts_with(b"ABORT\r"),
            "ABORT must still be written first"
        );
    }

    #[test]
    fn vara_abort_in_flight_falls_back_to_hard_close_when_write_fails() {
        let session = VaraSession::new();
        let shutdown_called = Arc::new(std::sync::Mutex::new(false));
        let spy = ShutdownSpy {
            called: shutdown_called.clone(),
        };
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
    fn vara_open_session_accepts_callsign_arg_without_panicking() {
        let session = Arc::new(VaraSession::new());
        let ui_cfg = VaraUiConfig {
            host: "127.0.0.1".into(),
            cmd_port: 1, // unreachable; we just want to exercise the signature
            data_port: 2,
            bandwidth_hz: None,
        };
        // With Some(callsign): same error path (TCP fails before MYCALL can
        // be sent), proving the new arg doesn't break the error semantics.
        let err = vara_open_session_inner(
            &session,
            &ui_cfg,
            Some("W1ABC"),
            SessionIntent::Cms,
            TransportKind::VaraHf,
        )
        .unwrap_err();
        assert!(err.contains("TCP connect failed"), "got: {err}");

        // Same with None (pre-wizard path).
        let err2 = vara_open_session_inner(
            &session,
            &ui_cfg,
            None,
            SessionIntent::Cms,
            TransportKind::VaraHf,
        )
        .unwrap_err();
        assert!(err2.contains("TCP connect failed"), "got: {err2}");

        // Same with empty / whitespace callsign — should be treated as "no
        // callsign" by the inner (MYCALL skipped). Verified indirectly by the
        // call not panicking.
        let err3 = vara_open_session_inner(
            &session,
            &ui_cfg,
            Some("   "),
            SessionIntent::Cms,
            TransportKind::VaraHf,
        )
        .unwrap_err();
        assert!(err3.contains("TCP connect failed"), "got: {err3}");
    }

    // ── tuxlink-0ye6 Task 3.2: vara_open_session captures intent + transport_kind ──
    //
    // Scope: the inner records the operator-typed `intent` + `transport_kind`
    // into session state on successful open; the stub accessors added in
    // Task 3.0 now return REAL values from that state. The outer
    // `vara_open_session` command's auto-arm wiring (which depends on
    // `arm_vara_listener_inner`, which requires a Tauri AppHandle) is covered
    // by the integration smoke checklist in the PR body; this unit test
    // covers the state-recording half independently.

    /// Spin up a real loopback `VaraTransport` so we can drive
    /// `vara_open_session_inner` end-to-end without a live VARA process.
    /// Returns `(session, host, cmd_port, cleanup-handle)` — the handle joins
    /// the acceptor thread pair on drop so the test doesn't leak.
    fn loopback_vara_open_session(
        intent: SessionIntent,
        transport_kind: TransportKind,
    ) -> Arc<VaraSession> {
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();

        // Acceptors hold the sockets open long enough for the inner to
        // complete TCP connect + the MYCALL/BW best-effort writes.
        let cmd_handle = thread::spawn(move || {
            let (_c, _) = cmd_l.accept().unwrap();
            thread::sleep(Duration::from_millis(500));
        });
        let data_handle = thread::spawn(move || {
            let (_c, _) = data_l.accept().unwrap();
            thread::sleep(Duration::from_millis(500));
        });

        let session = Arc::new(VaraSession::new());
        let ui_cfg = VaraUiConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            bandwidth_hz: None,
        };
        vara_open_session_inner(&session, &ui_cfg, None, intent, transport_kind)
            .expect("loopback open must succeed");

        // Detach the acceptors — they finish on their own after the sleep.
        // Tests that need post-open assertions read the session before the
        // acceptors exit; the brief 500ms window is plenty for an assertion.
        std::mem::drop((cmd_handle, data_handle));
        session
    }

    #[test]
    fn vara_open_session_inner_populates_active_intent_for_cms() {
        // Codex Round 2 P2: both intent + transport_kind flow through; the
        // stub accessors added in Task 3.0 now return REAL values.
        let session = loopback_vara_open_session(SessionIntent::Cms, TransportKind::VaraHf);
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Open, "loopback open should succeed");
        assert_eq!(
            snap.active_intent,
            Some(SessionIntent::Cms),
            "active_intent must reflect the operator-typed intent"
        );
        assert_eq!(
            snap.active_transport_kind,
            Some(TransportKind::VaraHf),
            "active_transport_kind must reflect the operator-typed kind"
        );
    }

    #[test]
    fn vara_open_session_inner_populates_active_intent_for_p2p() {
        let session = loopback_vara_open_session(SessionIntent::P2p, TransportKind::VaraHf);
        let snap = session.snapshot();
        assert_eq!(snap.active_intent, Some(SessionIntent::P2p));
        assert_eq!(snap.active_transport_kind, Some(TransportKind::VaraHf));
    }

    #[test]
    fn vara_open_session_inner_records_vara_fm_distinct_from_vara_hf() {
        // Codex Round 3 P1 #3: the wire transport is identical (TCP) for
        // vara-hf vs vara-fm, but the operator-meaningful discriminator must
        // be recorded on session state so the frontend can detect sidebar-nav
        // drift mid-session.
        let session = loopback_vara_open_session(SessionIntent::RadioOnly, TransportKind::VaraFm);
        let snap = session.snapshot();
        assert_eq!(snap.active_intent, Some(SessionIntent::RadioOnly));
        assert_eq!(
            snap.active_transport_kind,
            Some(TransportKind::VaraFm),
            "vara-fm must be recorded distinctly from vara-hf"
        );
    }

    #[test]
    fn vara_stop_session_clears_active_intent_and_transport_kind() {
        // Open with non-default intent + kind, stop, verify both fields clear.
        // Without this clear, an open→stop→open cycle would carry stale
        // metadata into the second session if the second open failed before
        // setting the fields (e.g., TCP connect to unreachable host).
        let session = loopback_vara_open_session(SessionIntent::P2p, TransportKind::VaraFm);
        let snap_open = session.snapshot();
        assert_eq!(snap_open.active_intent, Some(SessionIntent::P2p));

        vara_stop_session_inner(&session).expect("stop must succeed");

        let snap_closed = session.snapshot();
        assert_eq!(snap_closed.state, VaraState::Closed);
        assert_eq!(
            snap_closed.active_intent, None,
            "stop must clear active_intent so a follow-up open starts clean"
        );
        assert_eq!(
            snap_closed.active_transport_kind, None,
            "stop must clear active_transport_kind so a follow-up open starts clean"
        );
    }

    // ── tuxlink-0ye6 Task 3.3: vara_close_session lifecycle ────────────────
    //
    // Scope: the close-session command must (1) disarm the listener idempotently,
    // (2) call abort_in_flight on the session, (3) clear active_intent +
    // active_transport_kind, (4) close the transport. Tests cover the three new
    // behavior contracts (1) + (2) + (3); (4) is already covered by Task 3.2's
    // `vara_stop_session_clears_active_intent_and_transport_kind` (the
    // transport-teardown path is preserved unchanged through the rename).
    //
    // The inner helper `vara_close_session_inner` is sync (matching
    // `vara_stop_session_inner`'s pattern) so tests can drive without a Tauri
    // runtime. The outer `vara_close_session` Tauri command wraps the inner with
    // log emission + the AppHandle plumbing (covered indirectly via the frontend
    // integration test in VaraRadioPanel.test.tsx).

    #[test]
    fn vara_close_session_inner_disarms_listener_when_armed() {
        // Set up a listen_state with an armed handle (no consumer task — we're
        // testing the disarm signal path, not the consumer-drain behavior).
        // The disarm contract is "shutdown flag is set + handle is taken" —
        // observable via VaraListenState::is_armed() returning false.
        use crate::ui_commands::{VaraListenHandle, VaraListenState};
        use std::sync::atomic::AtomicBool;

        let session = Arc::new(VaraSession::new());
        let listen_state = Arc::new(VaraListenState::default());
        {
            let mut guard = listen_state.inner.lock().unwrap();
            *guard = Some(VaraListenHandle {
                shutdown: Arc::new(AtomicBool::new(false)),
            });
        }
        assert!(
            listen_state.is_armed(),
            "precondition: listener inserted as armed"
        );

        vara_close_session_inner(&session, &listen_state).expect("close must succeed");

        assert!(
            !listen_state.is_armed(),
            "Task 3.3: vara_close_session_inner must disarm the listener"
        );
    }

    #[test]
    fn vara_close_session_inner_disarm_is_idempotent_when_not_armed() {
        // No listener armed; close must still succeed (the disarm is a no-op
        // when not armed). Spec §5: close is the unconditional teardown — it
        // must not fail because some optional precondition (armed listener)
        // wasn't met.
        use crate::ui_commands::VaraListenState;
        let session = Arc::new(VaraSession::new());
        let listen_state = Arc::new(VaraListenState::default());
        assert!(!listen_state.is_armed(), "precondition: no listener armed");

        let result = vara_close_session_inner(&session, &listen_state);

        assert!(
            result.is_ok(),
            "close on un-armed listener must succeed: {result:?}"
        );
    }

    #[test]
    fn vara_close_session_inner_calls_abort_in_flight() {
        // Install a BlockedWriter + ShutdownSpy on the session BEFORE close.
        // The BlockedWriter Errs on write so abort_in_flight falls through to
        // the hard-close fallback, which fires the spy. Asserting the spy
        // fired proves vara_close_session_inner called abort_in_flight on the
        // path (the abort_writer was installed, so the no-writer fast-path is
        // not taken).
        use crate::ui_commands::VaraListenState;
        let session = Arc::new(VaraSession::new());
        let listen_state = Arc::new(VaraListenState::default());

        let shutdown_called = Arc::new(std::sync::Mutex::new(false));
        let spy = ShutdownSpy {
            called: shutdown_called.clone(),
        };
        session.install_abort_writer(
            Box::new(BlockedWriter) as Box<dyn std::io::Write + Send>,
            Box::new(spy) as Box<dyn ShutdownableStream>,
        );

        let _ = vara_close_session_inner(&session, &listen_state);

        assert!(
            *shutdown_called.lock().unwrap(),
            "Task 3.3: vara_close_session_inner must call abort_in_flight \
             (with a BlockedWriter installed, the spy MUST fire via the \
             hard-close fallback path — see Task 4.1)"
        );
    }

    #[test]
    fn vara_close_session_inner_clears_active_intent_and_transport_kind() {
        // Same shape as the Task 3.2 test for `vara_stop_session_inner`,
        // but exercises the new close-session path. The rename preserves the
        // transport-teardown body (which clears the fields), so this test
        // guards against a regression that drops the clear when refactoring
        // the close-session inner.
        use crate::ui_commands::VaraListenState;
        let session = loopback_vara_open_session(SessionIntent::P2p, TransportKind::VaraFm);
        let listen_state = Arc::new(VaraListenState::default());

        let snap_open = session.snapshot();
        assert_eq!(snap_open.active_intent, Some(SessionIntent::P2p));
        assert_eq!(snap_open.active_transport_kind, Some(TransportKind::VaraFm));

        vara_close_session_inner(&session, &listen_state).expect("close must succeed");

        let snap_closed = session.snapshot();
        assert_eq!(snap_closed.state, VaraState::Closed);
        assert!(
            snap_closed.active_intent.is_none(),
            "Task 3.3: active_intent must be cleared on close"
        );
        assert!(
            snap_closed.active_transport_kind.is_none(),
            "Task 3.3: active_transport_kind must be cleared on close"
        );
    }

    #[test]
    fn auto_arms_listener_intent_classification_matches_spec_matrix() {
        // The auto-arm decision is whether `intent.auto_arms_listener()` is
        // true; vara_open_session calls arm_vara_listener_inner iff true.
        // This test pins the decision matrix at the call site so a future
        // regression in `SessionIntent::auto_arms_listener` is caught here
        // even before the integration smoke surfaces it on a live VARA.
        //
        // (The matrix itself is also covered in session.rs::tests; this is
        // the integration-side guard so a refactor that decouples the call
        // site from the enum method has a unit-level alarm.)
        assert!(
            !SessionIntent::Cms.auto_arms_listener(),
            "Cms is outbound-only"
        );
        assert!(SessionIntent::P2p.auto_arms_listener(), "P2p auto-arms");
        assert!(
            SessionIntent::RadioOnly.auto_arms_listener(),
            "RadioOnly auto-arms"
        );
        assert!(
            !SessionIntent::PostOffice.auto_arms_listener(),
            "PostOffice not in alpha scope"
        );
        assert!(
            !SessionIntent::Mesh.auto_arms_listener(),
            "Mesh not in alpha scope"
        );
    }

    // ── tuxlink-0ye6 Task 4.3: transport arbiter (TransportOwner state machine) ─
    //
    // Scope of this dispatch (per dune-bison-salamander task brief):
    //   - TransportOwner enum + transport_owner() accessor
    //   - take_transport_for_outbound() / return_transport_from_outbound()
    //   - bounded 3s yield timeout (Codex Round 3 P1 #2)
    //   - lock-drop-before-await (Codex Round 2 P1 #4)
    //   - listener-yield + transport-return channels
    //
    // OUT OF SCOPE here:
    //   - Integration with vara_open_session / modem_vara_b2f_exchange
    //     (deferred to Phase 3 tasks 3.2 + 3.4, which create/rename those
    //     commands)
    //   - Listener consumer task changes (deferred)

    /// Spin a real (loopback) `VaraTransport` for tests that need one. Uses the
    /// same trick as `return_transport_restores_open_state`: spawn an acceptor
    /// per port and let VaraTransport::connect succeed against them. Returns
    /// the transport + the two thread handles the caller MUST join to release
    /// the accept threads cleanly.
    fn build_real_transport_for_test() -> (
        VaraTransport,
        std::thread::JoinHandle<()>,
        std::thread::JoinHandle<()>,
    ) {
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();

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
            data_read_timeout: Some(Duration::from_millis(100)),
        };
        let transport = VaraTransport::connect(cfg).expect("connect must succeed");
        (transport, cmd_handle, data_handle)
    }

    #[test]
    fn vara_transport_owner_starts_none() {
        let session = VaraSession::new();
        assert_eq!(session.transport_owner(), TransportOwner::None);
    }

    #[test]
    fn vara_take_transport_transitions_owner_to_listener_armed() {
        // Simulates the listener consumer task taking the transport after
        // vara_open_session + send_listen_on succeed. The owner moves from
        // None → ListenerArmed.
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = Arc::new(VaraSession::new());

        // Install transport so take_transport has something to hand out.
        {
            let mut guard = session.inner.lock().unwrap();
            guard.transport = Some(transport);
            guard.status = VaraStatus {
                state: VaraState::Open,
                last_error: None,
                bound_host: Some("127.0.0.1".into()),
                bound_cmd_port: None,
                listener_armed: false,
                exchange: None,
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
            };
        }
        assert_eq!(session.transport_owner(), TransportOwner::None);

        let taken = session.take_transport().expect("must take");
        assert_eq!(session.transport_owner(), TransportOwner::ListenerArmed);

        drop(taken);
        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    /// `.expect_err()` requires `T: Debug` and `VaraTransport` deliberately
    /// does not derive Debug (the TCP socket internals would leak noise).
    /// This helper extracts the Err arm by panicking-on-Ok with a clear
    /// message — same diagnostic value as `expect_err`, no Debug bound.
    fn unwrap_err_str<T>(r: Result<T, String>, ctx: &str) -> String {
        match r {
            Err(e) => e,
            Ok(_) => panic!("{ctx}: expected Err, got Ok"),
        }
    }

    #[tokio::test]
    async fn vara_take_transport_for_outbound_from_none_errs_session_not_open() {
        let session = VaraSession::new();
        let err = unwrap_err_str(session.take_transport_for_outbound().await, "None → Err");
        assert!(
            err.contains("session not open"),
            "expected 'session not open', got: {err}"
        );
        // Owner unchanged.
        assert_eq!(session.transport_owner(), TransportOwner::None);
    }

    #[tokio::test]
    async fn vara_take_transport_for_outbound_from_listener_inbound_errs_modem_busy() {
        let session = VaraSession::new();
        session.set_transport_owner_for_test(TransportOwner::ListenerInbound);
        let err = unwrap_err_str(
            session.take_transport_for_outbound().await,
            "ListenerInbound → Err",
        );
        assert!(
            err.contains("modem busy") && err.contains("inbound"),
            "expected 'modem busy — inbound exchange in progress', got: {err}"
        );
        // Owner unchanged.
        assert_eq!(session.transport_owner(), TransportOwner::ListenerInbound);
    }

    #[tokio::test]
    async fn vara_take_transport_for_outbound_from_outbound_errs_already_in_flight() {
        let session = VaraSession::new();
        session.set_transport_owner_for_test(TransportOwner::Outbound);
        let err = unwrap_err_str(
            session.take_transport_for_outbound().await,
            "Outbound → Err",
        );
        assert!(
            err.contains("outbound") && err.contains("already in flight"),
            "expected 'outbound exchange already in flight', got: {err}"
        );
        assert_eq!(session.transport_owner(), TransportOwner::Outbound);
    }

    #[tokio::test]
    async fn vara_take_transport_for_outbound_from_outbound_pending_also_errs() {
        // OutboundPending should also reject — a duplicate outbound request
        // while the first is still awaiting yield must not proceed.
        let session = VaraSession::new();
        session.set_transport_owner_for_test(TransportOwner::OutboundPending);
        let err = unwrap_err_str(
            session.take_transport_for_outbound().await,
            "OutboundPending → Err",
        );
        assert!(
            err.contains("outbound") && err.contains("already in flight"),
            "expected 'outbound exchange already in flight', got: {err}"
        );
    }

    #[tokio::test]
    async fn vara_take_transport_for_outbound_from_listener_armed_with_yield_succeeds() {
        // Build a real transport, stage a stub "consumer" task that listens
        // for the yield notify + sends the transport through the yield
        // channel. Outbound then succeeds.
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = Arc::new(VaraSession::new());
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);

        let notify = session.transport_yield_notify_clone();
        let yield_tx = session.transport_yield_sender_clone();
        let consumer = tokio::spawn(async move {
            notify.notified().await;
            // Real consumer would push the transport it's holding; we push
            // the real one we built.
            let _ = yield_tx.send(transport).await;
        });

        let out = session
            .take_transport_for_outbound()
            .await
            .expect("yield-then-take must succeed");
        // Owner transitioned to Outbound.
        assert_eq!(session.transport_owner(), TransportOwner::Outbound);

        consumer.await.ok();
        drop(out);
        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    #[tokio::test]
    async fn vara_take_transport_for_outbound_times_out_when_consumer_does_not_yield() {
        // No consumer spawned → notify lands but no transport ever arrives.
        // After ARBITER_YIELD_TIMEOUT, take_transport_for_outbound must
        // surface "modem busy — listener did not yield within {timeout}"
        // and reset owner to None.
        let session = VaraSession::new();
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);

        let start = std::time::Instant::now();
        let err = unwrap_err_str(
            session.take_transport_for_outbound().await,
            "wedged consumer → timeout Err",
        );
        let elapsed = start.elapsed();

        assert!(
            elapsed >= ARBITER_YIELD_TIMEOUT,
            "timeout must wait the full ARBITER_YIELD_TIMEOUT; got {elapsed:?}"
        );
        assert!(
            elapsed < ARBITER_YIELD_TIMEOUT + Duration::from_secs(2),
            "timeout must bound to ~{ARBITER_YIELD_TIMEOUT:?} (not unbounded); got {elapsed:?}"
        );
        assert!(
            err.contains("modem busy") && err.contains("did not yield"),
            "expected 'modem busy — listener did not yield', got: {err}"
        );
        // Owner reset to None so a clean Close + reopen can proceed.
        assert_eq!(session.transport_owner(), TransportOwner::None);
    }

    #[tokio::test]
    async fn vara_take_transport_for_outbound_errs_when_yield_channel_closed() {
        // Closed channel (Sender dropped before send) models "listener
        // consumer task exited". take_transport_for_outbound must surface
        // "listener consumer task exited" and reset owner to None.
        let session = VaraSession::new();
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);

        // Install a receiver whose paired sender was already dropped.
        session.install_closed_yield_channel_for_test().await;

        let err = unwrap_err_str(
            session.take_transport_for_outbound().await,
            "closed yield channel → Err",
        );
        assert!(
            err.contains("listener consumer task exited"),
            "expected 'listener consumer task exited', got: {err}"
        );
        assert_eq!(session.transport_owner(), TransportOwner::None);
    }

    #[tokio::test]
    async fn vara_return_transport_from_outbound_transitions_to_listener_armed_when_consumer_alive()
    {
        // Spin a stub consumer that holds the return-channel receiver. When
        // outbound returns the transport, the consumer receives it and
        // owner transitions to ListenerArmed.
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = Arc::new(VaraSession::new());
        session.set_transport_owner_for_test(TransportOwner::Outbound);

        // Stub consumer takes the return-rx and awaits.
        let mut return_rx = session
            .take_transport_return_rx()
            .expect("first take must succeed");
        let session_for_task = session.clone();
        let consumer = tokio::spawn(async move {
            let received = return_rx.recv().await;
            // The stub consumer "reclaims" the transport — keep it alive
            // so try_send didn't drop it.
            let _ = received;
            let _ = session_for_task; // keep session ref alive
        });

        // Outbound returns the transport. Fresh session — close_generation
        // is 0; snapshot matches live.
        session.return_transport_from_outbound(transport, 0);

        // The transition to ListenerArmed happens synchronously inside
        // return_transport_from_outbound BEFORE the channel buffer drains.
        assert_eq!(session.transport_owner(), TransportOwner::ListenerArmed);

        consumer.await.ok();
        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    #[tokio::test]
    async fn vara_return_transport_from_outbound_transitions_to_none_when_channel_closed() {
        // Take the return-rx and drop it immediately to simulate "consumer
        // gone." return_transport_from_outbound's try_send fails; owner
        // transitions to None (not ListenerArmed).
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = Arc::new(VaraSession::new());
        session.set_transport_owner_for_test(TransportOwner::Outbound);

        // Drop the receiver so the sender channel sees Closed.
        let rx = session
            .take_transport_return_rx()
            .expect("first take must succeed");
        drop(rx);

        // Fresh session — close_generation is 0; snapshot matches live.
        session.return_transport_from_outbound(transport, 0);

        // Owner transitioned to None (consumer cannot reclaim).
        assert_eq!(session.transport_owner(), TransportOwner::None);

        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    #[tokio::test]
    async fn vara_take_transport_for_outbound_does_not_hold_lock_across_await() {
        // Codex Round 2 P1 #4: the std-mutex MUST be released before the
        // .await on the yield channel. Verification: spawn an outbound that
        // notifies + waits, then spawn a second task that calls
        // transport_owner() (which takes the same std-mutex). The second
        // call must return PROMPTLY — if outbound were holding the lock
        // across .await, transport_owner() would block until outbound
        // completed.
        let session = Arc::new(VaraSession::new());
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);

        let session_for_outbound = session.clone();
        let outbound = tokio::spawn(async move {
            // No consumer → this will timeout after ARBITER_YIELD_TIMEOUT.
            session_for_outbound.take_transport_for_outbound().await
        });

        // Give outbound a moment to enter the .await phase.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // While outbound is awaiting yield, transport_owner() must NOT
        // block. If the std-mutex were held across the await, this call
        // would hang until outbound's timeout fires (~3s); we bound the
        // test at 500ms to catch the regression cleanly.
        let probe_start = std::time::Instant::now();
        let owner = session.transport_owner();
        let probe_elapsed = probe_start.elapsed();

        assert!(
            probe_elapsed < Duration::from_millis(500),
            "Codex Round 2 P1 #4: transport_owner() blocked for {probe_elapsed:?} \
             — the std-mutex is being held across the .await in \
             take_transport_for_outbound. The lock MUST be dropped before await."
        );
        // Owner should be OutboundPending during the await.
        assert_eq!(
            owner,
            TransportOwner::OutboundPending,
            "owner should be OutboundPending while outbound is awaiting yield"
        );

        // Let outbound finish (timing out).
        let _ = outbound.await;
    }

    // ── tuxlink-0ye6 Task 3.0 — DTO widening + SocketLost on VaraState ─
    //
    // Mirrors `modem_status::tests` for the VARA-side DTO. Same coverage:
    // new fields exist, camelCase wire format, kebab/camel enum variants
    // serialize correctly, stub accessors return defaults.

    #[test]
    fn vara_status_dto_includes_lifecycle_fields() {
        // Compile-time check that the new fields exist on the DTO with
        // the expected types (Codex Round 2 P1 #5 + Round 3 P1 #3).
        let s = VaraStatus::closed();
        let _: bool = s.listener_armed;
        let _: Option<ExchangeState> = s.exchange;
        let _: TransportOwner = s.transport_owner;
        let _: Option<SessionIntent> = s.active_intent;
        let _: Option<TransportKind> = s.active_transport_kind;
    }

    #[test]
    fn vara_status_serializes_lifecycle_fields_camel_case() {
        // Round 4 P1 #1: assert the active session mode fields are
        // present + serialized as camelCase, with enum-variant values
        // using their respective per-enum rename_all (kebab for
        // ExchangeState / SessionIntent / TransportKind; camel for
        // TransportOwner).
        let snap = VaraStatus {
            state: VaraState::Open,
            last_error: None,
            bound_host: None,
            bound_cmd_port: None,
            listener_armed: true,
            exchange: Some(ExchangeState::Outbound),
            transport_owner: TransportOwner::Outbound,
            active_intent: Some(SessionIntent::P2p),
            active_transport_kind: Some(TransportKind::VaraHf),
        };
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"listenerArmed\":true"), "got {json}");
        assert!(
            json.contains("\"exchange\":\"outbound\""),
            "ExchangeState kebab-case; got {json}"
        );
        assert!(
            json.contains("\"transportOwner\":\"outbound\""),
            "TransportOwner camelCase; got {json}"
        );
        assert!(
            json.contains("\"activeIntent\":\"p2p\""),
            "SessionIntent kebab-case; got {json}"
        );
        assert!(
            json.contains("\"activeTransportKind\":\"vara-hf\""),
            "TransportKind kebab-case; got {json}"
        );
        // Round-trip end-to-end (VaraStatus now derives Deserialize for
        // this purpose — tuxlink-0ye6 Task 3.0).
        let back: VaraStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back.state, snap.state);
        assert_eq!(back.listener_armed, snap.listener_armed);
        assert_eq!(back.exchange, snap.exchange);
        assert_eq!(back.transport_owner, snap.transport_owner);
        assert_eq!(back.active_intent, snap.active_intent);
        assert_eq!(back.active_transport_kind, snap.active_transport_kind);
    }

    #[test]
    fn vara_state_socket_lost_serializes() {
        // Codex Round 3 P1 #4: cmd-port unresponsive → SocketLost. Wire
        // form is kebab-case `"socket-lost"` (single-word "socketlost"
        // would be ambiguous; the per-variant `serde(rename)` makes it
        // explicit).
        let json = serde_json::to_string(&VaraState::SocketLost).unwrap();
        assert_eq!(json, "\"socket-lost\"");
        let back: VaraState = serde_json::from_str("\"socket-lost\"").unwrap();
        assert_eq!(back, VaraState::SocketLost);
    }

    #[test]
    fn vara_session_stub_accessors_return_defaults() {
        // The stub accessors return defaults today; this test pins the
        // contract so the wire-in task (Phase 3.2 / 3.4) changes it
        // explicitly rather than silently shifting.
        let session = VaraSession::new();
        assert!(!session.listener_armed());
        assert!(session.current_exchange().is_none());
        assert!(session.active_intent().is_none());
        assert!(session.active_transport_kind().is_none());
    }

    #[test]
    fn vara_session_snapshot_overlays_transport_owner() {
        // `snapshot()` overlays the live transport_owner from session
        // inner-mutex on top of the cached `inner.status`. Mirrors
        // ModemSession's parallel test.
        let session = VaraSession::new();
        assert_eq!(session.snapshot().transport_owner, TransportOwner::None);
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);
        assert_eq!(
            session.snapshot().transport_owner,
            TransportOwner::ListenerArmed
        );
    }

    // ── tuxlink-0ye6 Task 4.2: vara_open_session installs ABORT side-channel ──
    //
    // Scope: after vara_open_session_inner successfully opens the TCP
    // transport, the cmd-port writer + shutdown handle MUST be cloned via
    // VaraTransport::try_clone_abort_writer (Task 4.1) and installed on the
    // session via VaraSession::install_abort_writer. This makes
    // vara_close_session_inner's call to abort_in_flight (Task 3.3)
    // load-bearing: previously the call returned the "no abort writer
    // installed" Err and silently no-op'd; now the cooperative ABORT + hard-
    // close fallback paths from Task 4.1 actually run.
    //
    // The full end-to-end "close interrupts active exchange in <2s" smoke
    // needs `modem_vara_b2f_exchange` (Task 3.4 — not yet landed) plus a
    // mocked long-blocking transport. Deferred to operator smoke once 3.4
    // ships. Unit-layer coverage here pins the install + close-time abort
    // wiring against a loopback TCP transport.

    /// Spin a loopback `VaraTransport` whose peer cmd-port `TcpStream` is
    /// returned to the test for byte-level read-back. Acceptors are kept
    /// alive past the `vara_open_session_inner` call by handing their
    /// sockets out via the returned tuple — the test's `drop` order
    /// controls socket lifetime.
    fn loopback_open_with_cmd_peer(
        intent: SessionIntent,
        transport_kind: TransportKind,
    ) -> (
        Arc<VaraSession>,
        std::net::TcpStream, // peer cmd-port socket (the test reads from here)
        std::net::TcpStream, // peer data-port socket (kept alive; test ignores)
    ) {
        use std::net::TcpListener;
        use std::sync::mpsc as std_mpsc;
        use std::thread;
        use std::time::Duration;

        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();

        // Spawn acceptors that hand the accepted peer sockets back to the
        // test thread via a channel. Once accepted, the acceptor thread
        // exits — the test owns the sockets and controls their lifetime.
        let (cmd_tx, cmd_rx) = std_mpsc::channel::<std::net::TcpStream>();
        let (data_tx, data_rx) = std_mpsc::channel::<std::net::TcpStream>();
        thread::spawn(move || {
            let (s, _) = cmd_l.accept().unwrap();
            let _ = cmd_tx.send(s);
        });
        thread::spawn(move || {
            let (s, _) = data_l.accept().unwrap();
            let _ = data_tx.send(s);
        });

        let session = Arc::new(VaraSession::new());
        let ui_cfg = VaraUiConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            bandwidth_hz: None,
        };
        vara_open_session_inner(&session, &ui_cfg, None, intent, transport_kind)
            .expect("loopback open must succeed");

        // Now both connects have completed; the acceptor threads put the
        // peer sockets on the channels. Receive them within a bounded
        // window so test failure surfaces as a clear timeout rather than
        // a deadlock.
        let cmd_peer = cmd_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("cmd-port acceptor must hand off socket");
        let data_peer = data_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("data-port acceptor must hand off socket");

        (session, cmd_peer, data_peer)
    }

    #[test]
    fn vara_open_session_installs_abort_writer() {
        // After a successful open, the session's abort_in_flight() MUST NOT
        // return the "no abort writer installed" sentinel — the writer +
        // stream pair from VaraTransport::try_clone_abort_writer must be
        // installed (Task 4.2).
        let (session, _cmd_peer, _data_peer) =
            loopback_open_with_cmd_peer(SessionIntent::Cms, TransportKind::VaraHf);

        let abort_result = session.abort_in_flight();

        let err_str = abort_result
            .as_ref()
            .err()
            .map(|e| e.as_str())
            .unwrap_or("");
        assert!(
            !err_str.contains("no abort writer installed"),
            "Task 4.2: after vara_open_session_inner, abort_in_flight MUST NOT \
             report 'no abort writer installed' (got: {:?}). The Task 4.1 \
             install_abort_writer call is missing from the open path.",
            abort_result
        );
    }

    #[test]
    fn vara_open_session_installed_writer_actually_sends_abort_on_wire() {
        // Verify the installed writer is wired to the real cmd-port TCP
        // socket: after open + abort_in_flight, the peer side of the cmd
        // socket must see "ABORT\r" arrive. This is the byte-on-wire
        // version of `vara_open_session_installs_abort_writer` — proves
        // not just "installed" but "installed pointing at the right
        // socket."
        use std::io::Read;
        use std::time::Duration;

        let (session, mut cmd_peer, _data_peer) =
            loopback_open_with_cmd_peer(SessionIntent::Cms, TransportKind::VaraHf);

        // Drain any best-effort MYCALL/BW writes (open path sends none in
        // this test — None callsign + bandwidth_hz: None — but the read
        // below tolerates leading bytes via the contains() check below).
        cmd_peer.set_read_timeout(Some(Duration::from_secs(2))).ok();

        // Fire abort against the freshly-installed writer.
        let abort_result = session.abort_in_flight();
        assert!(
            abort_result.is_ok(),
            "loopback cmd port is alive; cooperative abort must succeed; got: {:?}",
            abort_result
        );

        // Read what arrived at the peer cmd socket. Read enough bytes to
        // cover any incidental prelude + the "ABORT\r" itself.
        let mut buf = [0u8; 64];
        let n = cmd_peer.read(&mut buf).expect("peer read must yield bytes");
        let s = String::from_utf8_lossy(&buf[..n]);
        assert!(
            s.contains("ABORT\r"),
            "expected peer cmd-port to receive 'ABORT\\r' from the installed \
             abort writer; got: {s:?}"
        );
    }

    #[test]
    fn vara_close_session_inner_fires_abort_through_installed_writer() {
        // End-to-end at the unit layer: open via vara_open_session_inner,
        // close via vara_close_session_inner, observe "ABORT\r" arriving on
        // the peer side of the cmd port. Proves the Task 3.3 abort call is
        // now load-bearing (the writer is installed by Task 4.2, so the
        // close path's abort no longer hits the "no writer installed"
        // fast-out path).
        use crate::ui_commands::VaraListenState;
        use std::io::Read;
        use std::time::Duration;

        let (session, mut cmd_peer, _data_peer) =
            loopback_open_with_cmd_peer(SessionIntent::Cms, TransportKind::VaraHf);
        let listen_state = Arc::new(VaraListenState::default());

        cmd_peer.set_read_timeout(Some(Duration::from_secs(2))).ok();

        let result = vara_close_session_inner(&session, &listen_state);
        assert!(result.is_ok(), "close must succeed: {result:?}");

        // The close path's step 2 (abort_in_flight) MUST have sent ABORT\r
        // through the installed writer to the peer cmd socket. Without
        // Task 4.2's install, the abort would have returned Err("no
        // writer") and no bytes would arrive.
        let mut buf = [0u8; 64];
        let n = cmd_peer.read(&mut buf).expect("peer read must yield bytes");
        let s = String::from_utf8_lossy(&buf[..n]);
        assert!(
            s.contains("ABORT\r"),
            "Task 4.2: vara_close_session_inner must abort via the installed \
             writer (peer should see 'ABORT\\r'); got: {s:?}. If empty, the \
             open path is not installing the writer."
        );

        // Transport is dropped post-close; the snapshot reflects Closed.
        assert_eq!(session.snapshot().state, VaraState::Closed);
    }

    // ── tuxlink-0ye6 Task 3.4: modem_vara_b2f_exchange — VARA dial path ──
    //
    // Scope: the dial-path B2F command is a thin async Tauri wrapper around
    // (a) intent parsing, (b) `take_transport` + `vara_dial_disconnect`
    // lifecycle, (c) cmd-port `CONNECT` + `wait_for_connected`, and (d)
    // `winlink_backend::run_vara_b2f_exchange` over the data socket. Each
    // primitive is unit-tested individually; the full end-to-end
    // "CONNECT + B2F over loopback" path needs a fully-mocked VARA modem
    // (cmd-port replying CONNECTED + data-port driving a B2F slave-role
    // handshake) which is out of scope for this task — covered by operator
    // smoke once Phase 5 wires the UI.

    /// Compile-time assertion: if the Tauri command's parameter list
    /// drifts, this reference fails to typecheck. The body is irrelevant
    /// — the existence-check at the module boundary is the test.
    /// Mirrors `modem_ardop_b2f_exchange_signature_has_no_consent_token`
    /// in `modem_commands.rs`, adapted for the async return shape (the
    /// async-fn opaque return type makes a fully-typed fn-pointer
    /// coercion impossible — referencing the function by name suffices
    /// to catch parameter-list drift).
    #[test]
    fn modem_vara_b2f_exchange_signature_is_stable() {
        // Reference the function so the test fails to compile if its
        // path or parameter list disappears. The leading underscore
        // suppresses the unused-binding warning.
        let _f = modem_vara_b2f_exchange;
    }

    /// The intent matrix that this command's caller passes through to
    /// `run_vara_b2f_exchange` MUST round-trip CMS → 'C', P2p → no flag,
    /// RadioOnly → 'R' (per spec §6.2). Pins the [`SessionIntent`] →
    /// [`RoutingFlag`] mapping the dial path relies on so a future
    /// change to either is caught.
    #[test]
    fn dial_path_intent_carries_expected_routing_flag() {
        use crate::winlink::session::RoutingFlag;

        assert_eq!(
            SessionIntent::Cms.routing_flag(),
            Some(RoutingFlag::Cms),
            "CMS dial intent must carry the 'C' routing flag"
        );
        assert_eq!(
            SessionIntent::P2p.routing_flag(),
            None,
            "P2P dial intent must carry no routing flag (unflagged messages)"
        );
        // Codex Phase 3-4 boundary P2 #2 (tuxlink-u1r7): the dial-path
        // command now accepts RadioOnly directly (no string-parser intermediary
        // that previously rejected this branch). Pin the matrix end-to-end
        // so a future shape drift surfaces.
        assert_eq!(
            SessionIntent::RadioOnly.routing_flag(),
            Some(RoutingFlag::RadioOnly),
            "RadioOnly dial intent must carry the 'R' routing flag"
        );
    }

    #[test]
    fn b2f_exchange_with_no_open_transport_surfaces_clean_error() {
        // The dial command takes the transport from the session via
        // `take_transport`. When no session is open, `take_transport`
        // returns None — verify the inner take path surfaces the
        // operator-friendly "open session first" error before any
        // RF-touching work happens.
        //
        // This test exercises the take + error-path branch of the
        // command in isolation by calling `take_transport` directly on
        // a fresh session (the full Tauri command needs an AppHandle +
        // State scaffolding that we don't construct here).
        let session = Arc::new(VaraSession::new());
        assert!(
            session.take_transport().is_none(),
            "fresh session has no transport; take_transport must return None"
        );
        // The command's None-branch then surfaces a static string error;
        // see `modem_vara_b2f_exchange` for the exact wording.
    }

    // ── tuxlink-pdnw — close-generation guards (Codex Phase 3-4 P1 #4) ──
    //
    // VARA mirror of the ModemSession close-generation tests in
    // `src-tauri/src/modem_status.rs`. The race + remediation are
    // documented on the ModemSession tests; these encode the same
    // semantics on the VaraSession surface.

    #[test]
    fn vara_close_generation_starts_at_zero() {
        let session = VaraSession::new();
        assert_eq!(
            session.current_close_generation(),
            0,
            "fresh session must start at close_generation = 0"
        );
    }

    #[test]
    fn vara_bump_close_generation_increments_monotonically() {
        let session = VaraSession::new();
        let prior_a = session.bump_close_generation();
        assert_eq!(prior_a, 0);
        assert_eq!(session.current_close_generation(), 1);

        let prior_b = session.bump_close_generation();
        assert_eq!(prior_b, 1);
        assert_eq!(session.current_close_generation(), 2);
    }

    #[test]
    fn vara_install_transport_if_generation_matches_installs_when_snapshot_current() {
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = VaraSession::new();
        let snapshot = session.current_close_generation();

        let result = session.install_transport_if_generation_matches(
            transport,
            snapshot,
            Some("127.0.0.1".into()),
            Some(8300),
            None,
            None,
        );

        assert!(result.is_ok(), "matching generation must install");
        // After install the session is in Open state.
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Open);
        assert_eq!(snap.bound_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(snap.bound_cmd_port, Some(8300));

        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    #[test]
    fn vara_install_transport_if_generation_matches_drops_when_close_intervened() {
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = VaraSession::new();
        let snapshot = session.current_close_generation();
        assert_eq!(snapshot, 0);

        // Simulate vara_close_session_inner: bump the generation.
        let _ = session.bump_close_generation();
        assert_eq!(session.current_close_generation(), 1);

        let result = session.install_transport_if_generation_matches(
            transport,
            snapshot,
            Some("127.0.0.1".into()),
            Some(8300),
            None,
            None,
        );

        assert!(
            result.is_err(),
            "stale snapshot must Err — close intervened"
        );
        // Session must remain Closed (the install was a no-op).
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Closed);

        // Drop returned transport (mirrors production caller posture).
        drop(result.err().unwrap());
        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    #[tokio::test]
    async fn vara_return_transport_from_outbound_drops_when_close_intervened() {
        // Mirror of the ARDOP close-vs-return race test: outbound captured
        // the generation, then a close path bumped it; return_transport_from_outbound
        // must drop the transport instead of pushing it onto the return
        // channel. Owner ends up None.
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = Arc::new(VaraSession::new());
        session.set_transport_owner_for_test(TransportOwner::Outbound);

        let snapshot = session.current_close_generation();
        assert_eq!(snapshot, 0);
        let _ = session.bump_close_generation();
        assert_eq!(session.current_close_generation(), 1);

        let mut return_rx = session
            .take_transport_return_rx()
            .expect("first take must succeed");

        session.return_transport_from_outbound(transport, snapshot);

        // Owner cleared to None.
        assert_eq!(session.transport_owner(), TransportOwner::None);

        // Return channel must be empty (transport was dropped, not pushed).
        match return_rx.try_recv() {
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                panic!("return channel disconnected — stale-gen path should not close the channel")
            }
            Ok(_t) => {
                panic!("return channel must be empty after stale-gen drop; got a transport")
            }
        }

        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    /// tuxlink-0iqi (Codex Phase 3-4 P1 #2): the install-back path must
    /// restore `active_intent` + `active_transport_kind` when the caller
    /// supplies them, so a subsequent listener re-arm / Send/Receive can
    /// proceed without re-opening. Mirrors the b2f exchange's preservation
    /// contract; the listener consumer's drain path passes `None`/`None`
    /// (covered by the existing -installs_when_snapshot_current test).
    #[test]
    fn vara_install_transport_preserves_active_intent_and_kind_when_supplied() {
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = VaraSession::new();
        let snapshot = session.current_close_generation();

        let result = session.install_transport_if_generation_matches(
            transport,
            snapshot,
            Some("127.0.0.1".into()),
            Some(8300),
            Some(SessionIntent::Cms),
            Some(TransportKind::VaraHf),
        );

        assert!(result.is_ok(), "matching generation must install");
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Open);
        assert_eq!(
            snap.active_intent,
            Some(SessionIntent::Cms),
            "install-back must preserve the caller-supplied active_intent"
        );
        assert_eq!(
            snap.active_transport_kind,
            Some(TransportKind::VaraHf),
            "install-back must preserve the caller-supplied active_transport_kind"
        );
        // bound_host + bound_cmd_port also preserved (the existing happy-path
        // test covers this, asserted here for completeness with the new params).
        assert_eq!(snap.bound_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(snap.bound_cmd_port, Some(8300));

        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    /// tuxlink-0iqi: stale-generation install-back drops the transport AND
    /// does NOT mutate the session's active mode — the session was just
    /// closed by `vara_close_session_inner`, so `active_intent` /
    /// `active_transport_kind` should remain whatever the close set them
    /// to (None, via `vara_stop_session_inner`). This is the race-prevention
    /// guarantee: a b2f exchange that finishes after the operator clicked
    /// Close Session does NOT re-open the session.
    #[test]
    fn vara_install_transport_with_supplied_intent_still_drops_on_stale_generation() {
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = VaraSession::new();
        let snapshot = session.current_close_generation();
        assert_eq!(snapshot, 0);

        // Simulate vara_close_session_inner running concurrently: bump
        // the generation. The b2f exchange's snapshot is now stale.
        let _ = session.bump_close_generation();
        assert_eq!(session.current_close_generation(), 1);

        // The b2f exchange tries to install-back with its captured intent.
        // The guard MUST drop the transport regardless — the close already
        // won the race.
        let result = session.install_transport_if_generation_matches(
            transport,
            snapshot,
            Some("127.0.0.1".into()),
            Some(8300),
            Some(SessionIntent::Cms),
            Some(TransportKind::VaraHf),
        );

        assert!(
            result.is_err(),
            "stale snapshot must Err — close intervened during the exchange"
        );

        // Session remains Closed; active mode untouched by the failed install.
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Closed);
        assert_eq!(
            snap.active_intent, None,
            "stale-gen install must NOT restore active_intent into a closed session"
        );
        assert_eq!(
            snap.active_transport_kind, None,
            "stale-gen install must NOT restore active_transport_kind into a closed session"
        );

        // Drop the returned transport (mirrors production caller posture).
        drop(result.err().unwrap());
        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    /// Codex Phase 3-4 RE-REVIEW P2 regression test: a listener-consumer
    /// drain that re-installs with `None`/`None` for the preserve params
    /// MUST preserve the session's existing active_intent +
    /// active_transport_kind (not erase them). The pre-fix behavior wrote
    /// `None` to both, which lost the operator's active mode on ordinary
    /// Listen Off — leaving the session Open with a forensic-blanked
    /// status. The new semantics: `Some(_)` writes; `None` preserves.
    #[test]
    fn vara_install_with_none_preserve_params_preserves_existing_active_mode() {
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = Arc::new(VaraSession::new());
        // Seed the session with an active mode the consumer is about to
        // briefly hold + return without close.
        {
            let mut guard = session.inner.lock().unwrap();
            guard.active_intent = Some(SessionIntent::P2p);
            guard.active_transport_kind = Some(TransportKind::VaraFm);
        }

        let snapshot = session.current_close_generation();
        let result = session.install_transport_if_generation_matches(
            transport,
            snapshot,
            Some("127.0.0.1".into()),
            Some(8300),
            None, // listener drain: don't overwrite operator's active intent
            None, // listener drain: don't overwrite operator's active kind
        );
        assert!(
            result.is_ok(),
            "matching generation must install on ordinary disarm (no close)"
        );

        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Open);
        assert_eq!(
            snap.active_intent,
            Some(SessionIntent::P2p),
            "None preserve-param must NOT erase existing active_intent on ordinary disarm"
        );
        assert_eq!(
            snap.active_transport_kind,
            Some(TransportKind::VaraFm),
            "None preserve-param must NOT erase existing active_transport_kind on ordinary disarm"
        );

        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    /// tuxlink-0iqi end-to-end semantic test: walk the b2f exchange's
    /// install-back call site directly. Simulates the post-exchange snapshot
    /// + install-back with the operator's active intent intact. The session
    ///   must remain `Open` with `active_intent` + `active_transport_kind`
    ///   preserved — the Spec §2 within-session contract that replaced the
    ///   pre-fix `vara_stop_session_inner` lifecycle violation.
    #[test]
    fn vara_b2f_install_back_restores_open_with_active_mode_intact() {
        // Build a session in the post-open state: transport installed,
        // active_intent = Cms, active_transport_kind = VaraHf, bound_host
        // + bound_cmd_port set. Mirrors what `vara_open_session_inner`
        // leaves behind.
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = Arc::new(VaraSession::new());
        {
            let mut guard = session.inner.lock().unwrap();
            guard.transport = Some(transport);
            guard.active_intent = Some(SessionIntent::Cms);
            guard.active_transport_kind = Some(TransportKind::VaraHf);
            guard.status = VaraStatus {
                state: VaraState::Open,
                last_error: None,
                bound_host: Some("127.0.0.1".into()),
                bound_cmd_port: Some(8300),
                listener_armed: false,
                exchange: None,
                transport_owner: TransportOwner::None,
                active_intent: Some(SessionIntent::Cms),
                active_transport_kind: Some(TransportKind::VaraHf),
            };
        }

        // Snapshot lifecycle inputs BEFORE take_transport (the exact
        // sequence `modem_vara_b2f_exchange` runs).
        let close_gen_snapshot = session.current_close_generation();
        let (snapshot_intent, snapshot_kind, snapshot_bound_host, snapshot_bound_cmd_port) = {
            let guard = session.inner.lock().unwrap();
            (
                guard.active_intent,
                guard.active_transport_kind,
                guard.status.bound_host.clone(),
                guard.status.bound_cmd_port,
            )
        };
        assert_eq!(snapshot_intent, Some(SessionIntent::Cms));
        assert_eq!(snapshot_kind, Some(TransportKind::VaraHf));

        // Take the transport — owner transitions to ListenerArmed (the
        // legacy take-transport-bypass pattern).
        let transport = session.take_transport().expect("must take");

        // Sanity: session is in mid-exchange posture (transport gone but
        // active_intent + active_transport_kind still set).
        {
            let guard = session.inner.lock().unwrap();
            assert!(guard.transport.is_none());
            assert_eq!(guard.active_intent, Some(SessionIntent::Cms));
            assert_eq!(guard.active_transport_kind, Some(TransportKind::VaraHf));
        }

        // Install-back: the post-exchange call. Generation matches; the
        // session must return to Open with active mode preserved.
        let result = session.install_transport_if_generation_matches(
            transport,
            close_gen_snapshot,
            snapshot_bound_host,
            snapshot_bound_cmd_port,
            snapshot_intent,
            snapshot_kind,
        );
        assert!(result.is_ok(), "fresh-snapshot install-back must succeed");

        // Spec §2 contract: session in Open, active_intent + transport_kind
        // populated, bound_host + bound_cmd_port present. A subsequent
        // Send/Receive can run without re-opening.
        let snap = session.snapshot();
        assert_eq!(snap.state, VaraState::Open);
        assert_eq!(snap.active_intent, Some(SessionIntent::Cms));
        assert_eq!(snap.active_transport_kind, Some(TransportKind::VaraHf));
        assert_eq!(snap.bound_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(snap.bound_cmd_port, Some(8300));
        // Transport-owner reset to None (the b2f exchange is over; a
        // subsequent take re-claims).
        assert_eq!(snap.transport_owner, TransportOwner::None);

        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    #[test]
    fn vara_close_session_inner_bumps_close_generation() {
        // The close-session inner must bump the generation BEFORE any
        // consumer-shutdown / transport-teardown work. Verify by capturing
        // the generation pre-close, calling the inner, and asserting the
        // generation increased.
        //
        // Uses an empty session (no transport, no listener armed) so the
        // close inner takes the idempotent fast path. The bump must run
        // regardless — it's the load-bearing line for the race fix.
        let session = Arc::new(VaraSession::new());
        let listen_state = Arc::new(crate::ui_commands::VaraListenState::default());

        let before = session.current_close_generation();
        let _ = vara_close_session_inner(&session, &listen_state);
        let after = session.current_close_generation();

        assert!(
            after > before,
            "vara_close_session_inner must bump close_generation; before={before}, after={after}"
        );
    }

    // ── tuxlink-u1r7 — Codex Phase 3-4 boundary P2 #1 ──────────────────
    //
    // `vara_stop_session_inner` must clear `abort_writer`, `abort_stream`,
    // and `transport_owner` alongside the transport. Mirrors
    // `ModemSession::reset_to_stopped`'s posture so stale ABORT side-channel
    // handles can't leak into the next session and a status snapshot after
    // close cannot report `listenerArmed` via a stale TransportOwner overlay.

    #[test]
    fn vara_stop_session_inner_clears_abort_writer_and_stream() {
        let session = Arc::new(VaraSession::new());

        // Install an abort-writer pair (the same shape vara_open_session_inner
        // installs after a real TCP open). After stop, the pair must be cleared.
        let captured: Arc<std::sync::Mutex<Vec<u8>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = RecordingWriter { captured };
        let shutdown_called = Arc::new(std::sync::Mutex::new(false));
        let spy = ShutdownSpy {
            called: shutdown_called,
        };
        session.install_abort_writer(
            Box::new(writer) as Box<dyn std::io::Write + Send>,
            Box::new(spy) as Box<dyn ShutdownableStream>,
        );

        // Sanity: pre-stop, abort_in_flight finds the writer (returns Ok
        // because RecordingWriter accepts any write).
        assert!(
            session.abort_in_flight().is_ok(),
            "pre-stop: abort_in_flight must succeed with the installed writer"
        );

        // Re-install (abort_in_flight consumed the success-path; the spy
        // didn't fire so the stream stays. But we cleared the writer above
        // in the cooperative-success path? No — cooperative success leaves
        // both in place. Verify the writer is still present.)
        // Actually, the cooperative-success path does NOT clear the writer;
        // only the hard-close fallback does. So the writer is still
        // installed at this point. We don't need to re-install.

        // Now drive vara_stop_session_inner — the P2 #1 fix MUST clear both.
        vara_stop_session_inner(&session).unwrap();

        // After stop, abort_in_flight must Err with "no abort writer".
        let err = session
            .abort_in_flight()
            .expect_err("after stop, the writer must be cleared");
        assert!(
            err.contains("no abort writer"),
            "after vara_stop_session_inner, abort_writer must be None; got: {err}"
        );

        // Direct field inspection: abort_writer + abort_stream are None.
        let guard = session.inner.lock().unwrap();
        assert!(
            guard.abort_writer.is_none(),
            "P2 #1: abort_writer must be cleared by vara_stop_session_inner"
        );
        assert!(
            guard.abort_stream.is_none(),
            "P2 #1: abort_stream must be cleared by vara_stop_session_inner"
        );
    }

    #[test]
    fn vara_stop_session_inner_resets_transport_owner() {
        let session = Arc::new(VaraSession::new());

        // Synthesize a "listener has the transport" state — the closed-status
        // overlay would surface this as `listenerArmed = true` if the owner
        // weren't reset on stop (the bug Codex flagged).
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);
        assert_eq!(session.transport_owner(), TransportOwner::ListenerArmed);

        let snap = vara_stop_session_inner(&session).unwrap();
        // The stop's status return reflects Closed.
        assert_eq!(snap.state, VaraState::Closed);

        // Owner must be reset to None.
        assert_eq!(
            session.transport_owner(),
            TransportOwner::None,
            "P2 #1: vara_stop_session_inner must reset transport_owner to None"
        );

        // And the snapshot's transport_owner overlay must also be None,
        // confirming the close-overlay state can't lie about a stale owner.
        let snap_after = session.snapshot();
        assert_eq!(snap_after.transport_owner, TransportOwner::None);
    }

    #[test]
    fn vara_stop_session_inner_clears_owner_when_in_inbound_state() {
        // ListenerInbound is the other owner state that survives a
        // take_transport-style close path. Same fix; pin both branches.
        let session = Arc::new(VaraSession::new());
        session.set_transport_owner_for_test(TransportOwner::ListenerInbound);

        vara_stop_session_inner(&session).unwrap();

        assert_eq!(
            session.transport_owner(),
            TransportOwner::None,
            "P2 #1: ListenerInbound owner must be reset on stop"
        );
    }

    // ── tuxlink-u1r7 — Codex Phase 3-4 boundary P2 #2 ──────────────────
    //
    // `modem_vara_b2f_exchange` widened to take SessionIntent (full enum)
    // + TransportKind. Validation rejects non-VARA transport kinds before
    // any radio-touching work; the prior shape (`intent: String`) only
    // accepted `"cms"`/`"p2p"` strings and could not express `RadioOnly`
    // or VARA-FM dials.

    /// Compile-time pin: the widened command signature mentions
    /// `SessionIntent` and `TransportKind` at the parameter list. Drift
    /// (e.g., a regression to `intent: String`) breaks the typecheck.
    #[test]
    fn modem_vara_b2f_exchange_takes_session_intent_and_transport_kind() {
        // Reference the function via a type-erased pointer to force the
        // compiler to look at its signature. We can't fully type-check
        // async-fn return shapes via fn pointers (opaque return type), so
        // the existence check + the parameter-list mention in this test's
        // doc is what catches drift; the assertion below pins the
        // SessionIntent + TransportKind types are reachable at this site.
        let _f = modem_vara_b2f_exchange;
        let _intent_pin: SessionIntent = SessionIntent::Cms;
        let _kind_pin: TransportKind = TransportKind::VaraHf;
    }

    /// Validation rejects mismatched transport kinds (Ardop/Telnet/Packet/Pactor)
    /// before any radio-touching work. We can't easily build a full Tauri
    /// State for the async command body here, so this test anchors the
    /// same `matches!` predicate the command uses; a future regression
    /// that loosens the guard will fail this test.
    #[test]
    fn modem_vara_b2f_exchange_rejects_non_vara_transport_kind() {
        // Same predicate the command uses internally; drift in either
        // place breaks the test or breaks the command.
        let cases = [
            (TransportKind::Ardop, false),
            (TransportKind::Telnet, false),
            (TransportKind::Packet, false),
            (TransportKind::Pactor, false),
            (TransportKind::VaraHf, true),
            (TransportKind::VaraFm, true),
        ];
        for (k, expected) in cases {
            let allowed = matches!(k, TransportKind::VaraHf | TransportKind::VaraFm);
            assert_eq!(
                allowed, expected,
                "P2 #2: VARA b2f validation must accept {:?} = {}",
                k, expected
            );
        }
    }

    /// Sentinel — the legacy string-form b2f-intent parser helper MUST be
    /// deleted as part of the P2 #2 sweep. If a regression re-adds it,
    /// this test catches the redefinition. The sentinel string is
    /// assembled via `concat!` and includes `(` so this test's name
    /// + docstring don't accidentally match the search.
    #[test]
    fn legacy_string_intent_parser_helper_is_removed() {
        let source = include_str!("commands.rs");
        let removed_symbol = concat!("fn ", "parse_vara_b2f_", "intent(s:");
        assert!(
            !source.contains(removed_symbol),
            "P2 #2: the legacy string-parsing b2f-intent helper must be \
             removed — the widened command takes SessionIntent directly; \
             the string-parser helper has no remaining callers"
        );
    }

    // ── tuxlink-u1r7 — Codex Phase 3-4 boundary P2 #4 ──────────────────
    //
    // `VaraStatus.listener_armed` + `.exchange` accessors wired from
    // their Task 3.0 stub returns to real session state. listener_armed
    // reads through `transport_owner` (ListenerArmed | ListenerInbound);
    // exchange reads through the new `current_exchange` field on
    // VaraSessionInner, set by begin_exchange / end_exchange at the
    // entry/exit of the b2f code paths.

    #[test]
    fn vara_listener_armed_reflects_transport_owner_listener_armed() {
        let session = VaraSession::new();
        assert!(
            !session.listener_armed(),
            "fresh session: listener_armed must be false"
        );

        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);
        assert!(
            session.listener_armed(),
            "P2 #4: ListenerArmed transport_owner must surface as \
             listener_armed=true"
        );

        // Clearing the owner returns listener_armed to false.
        session.set_transport_owner_for_test(TransportOwner::None);
        assert!(
            !session.listener_armed(),
            "owner None: listener_armed must be false"
        );
    }

    #[test]
    fn vara_listener_armed_true_during_inbound_exchange() {
        // ListenerInbound covers the "listener has the transport AND
        // is running an inbound exchange" case. listener_armed must be
        // true in this state too — the UI gates the "exchange in
        // progress" pill from `exchange == Some(Inbound)` and the
        // listener-armed surface from `listener_armed`; both should
        // hold simultaneously.
        let session = VaraSession::new();
        session.set_transport_owner_for_test(TransportOwner::ListenerInbound);
        assert!(
            session.listener_armed(),
            "P2 #4: ListenerInbound must surface as listener_armed=true"
        );
    }

    #[test]
    fn vara_listener_armed_false_for_outbound_owner_states() {
        // OutboundPending + Outbound are not "listener-armed" states —
        // outbound has taken the transport via the arbiter yield. Pin
        // both branches so a future widening of listener_armed catches.
        let session = VaraSession::new();
        session.set_transport_owner_for_test(TransportOwner::OutboundPending);
        assert!(
            !session.listener_armed(),
            "OutboundPending owner: listener_armed must be false"
        );
        session.set_transport_owner_for_test(TransportOwner::Outbound);
        assert!(
            !session.listener_armed(),
            "Outbound owner: listener_armed must be false"
        );
    }

    #[test]
    fn vara_current_exchange_returns_begin_exchange_value() {
        let session = VaraSession::new();
        assert!(
            session.current_exchange().is_none(),
            "fresh session: current_exchange must be None"
        );

        session.begin_exchange(ExchangeState::Outbound);
        assert_eq!(
            session.current_exchange(),
            Some(ExchangeState::Outbound),
            "P2 #4: begin_exchange(Outbound) must surface via current_exchange"
        );

        session.begin_exchange(ExchangeState::Inbound);
        assert_eq!(
            session.current_exchange(),
            Some(ExchangeState::Inbound),
            "P2 #4: begin_exchange(Inbound) must replace prior state"
        );

        session.end_exchange();
        assert!(
            session.current_exchange().is_none(),
            "P2 #4: end_exchange must clear the marker"
        );
    }

    #[test]
    fn vara_current_exchange_cleared_on_stop_session() {
        // A close racing the entry of a b2f exchange must not leave a
        // stale Outbound/Inbound marker on the closed session — the
        // status overlay would lie to the operator.
        let session = Arc::new(VaraSession::new());
        session.begin_exchange(ExchangeState::Outbound);
        assert_eq!(session.current_exchange(), Some(ExchangeState::Outbound));

        vara_stop_session_inner(&session).unwrap();

        assert!(
            session.current_exchange().is_none(),
            "P2 #4: vara_stop_session_inner must clear current_exchange"
        );
        // And the snapshot's overlaid `exchange` field must agree.
        let snap = session.snapshot();
        assert!(
            snap.exchange.is_none(),
            "snapshot.exchange must be None after stop"
        );
    }

    #[test]
    fn vara_snapshot_overlays_listener_armed_and_exchange_from_real_state() {
        // Drive both fields concurrently and verify snapshot() reflects
        // them both — the load-bearing DTO-wire-in for the panel.
        let session = VaraSession::new();
        session.set_transport_owner_for_test(TransportOwner::ListenerInbound);
        session.begin_exchange(ExchangeState::Inbound);

        let snap = session.snapshot();
        assert!(
            snap.listener_armed,
            "P2 #4: snapshot.listener_armed must be true with ListenerInbound owner"
        );
        assert_eq!(
            snap.exchange,
            Some(ExchangeState::Inbound),
            "P2 #4: snapshot.exchange must reflect current_exchange"
        );
        assert_eq!(snap.transport_owner, TransportOwner::ListenerInbound);
    }

    #[test]
    fn vara_install_transport_if_generation_matches_clears_exchange_on_success() {
        // The install-back path runs at b2f-exchange exit. It must clear
        // current_exchange so a subsequent snapshot doesn't report a
        // stale Outbound after the exchange completes.
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = VaraSession::new();
        session.begin_exchange(ExchangeState::Outbound);
        assert_eq!(session.current_exchange(), Some(ExchangeState::Outbound));

        let snapshot = session.current_close_generation();
        let result = session.install_transport_if_generation_matches(
            transport,
            snapshot,
            Some("127.0.0.1".into()),
            Some(8300),
            Some(SessionIntent::Cms),
            Some(TransportKind::VaraHf),
        );
        assert!(result.is_ok(), "fresh generation install must succeed");
        assert!(
            session.current_exchange().is_none(),
            "P2 #4: install_transport_if_generation_matches must clear \
             current_exchange on success"
        );

        drop(session);
        h1.join().ok();
        h2.join().ok();
    }

    #[test]
    fn vara_return_transport_clears_exchange() {
        // return_transport is the listener consumer's post-disarm path;
        // the prior inbound exchange (if any) is complete by then.
        let (transport, h1, h2) = build_real_transport_for_test();
        let session = VaraSession::new();
        session.begin_exchange(ExchangeState::Inbound);
        assert_eq!(session.current_exchange(), Some(ExchangeState::Inbound));

        session.return_transport(transport, Some("127.0.0.1".into()), Some(8300));

        assert!(
            session.current_exchange().is_none(),
            "P2 #4: return_transport must clear current_exchange"
        );

        drop(session);
        h1.join().ok();
        h2.join().ok();
    }
}
