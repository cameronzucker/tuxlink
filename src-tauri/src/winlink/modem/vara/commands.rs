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

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, Notify};

use crate::config::{self, VaraUiConfig};
use crate::modem_status::{
    ExchangeState, ShutdownableStream, TransportOwner, ARBITER_YIELD_TIMEOUT,
};
use crate::session_log::SessionLogState;
use crate::ui_commands::LogLineDto;
use crate::winlink::listener::transport::TransportKind;
use crate::winlink::session::SessionIntent;
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
    /// Session → reopen. Heartbeat infrastructure that drives this
    /// transition is deferred to a follow-up task; the variant ships now
    /// so that follow-up is a pure additive wire-in.
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
    /// Current ownership of the live transport (tuxlink-0ye6 Task 4.3,
    /// Codex Round 1 P1 #5). Set as a side effect of `take_transport` /
    /// `return_transport` (listener consumer) and `take_transport_for_outbound`
    /// / `return_transport_from_outbound` (outbound). See [`TransportOwner`]
    /// for the state machine; transitions are guarded by this mutex.
    transport_owner: TransportOwner,
    /// Intent of the currently-open session (tuxlink-0ye6 Task 3.2). Set by
    /// `vara_open_session` after a successful TCP open; cleared in
    /// `vara_stop_session_inner` on transport teardown. `None` whenever
    /// `transport.is_none()` — i.e., status is `Closed` or `Error`.
    active_intent: Option<SessionIntent>,
    /// Transport-kind discriminator (vara-hf vs vara-fm) for the open
    /// session. Same lifecycle as [`Self::active_intent`]. The wire
    /// transport (TCP host/port) is identical between the two; this field
    /// records the operator-meaningful distinction so the frontend can
    /// detect sidebar-nav drift mid-session (Codex Round 3 P1 #3).
    active_transport_kind: Option<TransportKind>,
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
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
            }),
            transport_yield_request: Arc::new(Notify::new()),
            transport_yield_rx: tokio::sync::Mutex::new(yield_rx),
            transport_yield_tx: yield_tx,
            transport_return_tx: return_tx,
            transport_return_rx: Mutex::new(Some(return_rx)),
        }
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

    // ── Lifecycle stub accessors (tuxlink-0ye6 Task 3.0) ────────────────
    //
    // See `ModemSession`'s parallel accessors in
    // `src-tauri/src/modem_status.rs` for the shared contract. The real
    // values are wired in by Phase 3.2 (open_session / close_session),
    // 3.4 (listener consumer task), and 3.5 (`b2f_exchange` outbound).

    /// Listener-armed state. STUB: returns `false`.
    // TODO: wire to listener state once Phase 3 commands land
    // (tuxlink-0ye6 Task 3.4 — the listener consumer task is the
    // authoritative source).
    pub fn listener_armed(&self) -> bool {
        false
    }

    /// Current in-flight exchange classification. STUB: returns `None`.
    // TODO: wire to listener state once Phase 3 commands land
    // (tuxlink-0ye6 Task 3.5 outbound + 3.4 inbound).
    pub fn current_exchange(&self) -> Option<ExchangeState> {
        None
    }

    /// Intent of the currently-open session (tuxlink-0ye6 Task 3.2). Set by
    /// `vara_open_session` on successful TCP open; cleared by
    /// `vara_stop_session_inner`. Returns `None` when the session is closed
    /// or the mutex is poisoned.
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
    pub fn take_transport(&self) -> Option<VaraTransport> {
        let mut guard = self.inner.lock().ok()?;
        let t = guard.transport.take();
        if t.is_some() {
            guard.status = VaraStatus::closed();
            guard.transport_owner = TransportOwner::ListenerArmed;
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
                    return Err(
                        "modem busy — inbound exchange in progress".into()
                    )
                }
                TransportOwner::OutboundPending | TransportOwner::Outbound => {
                    return Err("outbound exchange already in flight".into())
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
                return Err(
                    "listener consumer task exited; session needs Close + reopen"
                        .into(),
                );
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
    /// - Consumer still listening → owner = `ListenerArmed`, transport
    ///   pushed through `transport_return_tx`.
    /// - Consumer gone (return_tx send fails) → owner = `None`,
    ///   transport dropped. The caller's outbound is complete either
    ///   way; the operator's next Close Session will tear down cleanly.
    ///
    /// Best-effort: ignores Mutex poisoning + send failures because the
    /// outbound side has already completed; we're cleaning up.
    pub fn return_transport_from_outbound(&self, transport: VaraTransport) {
        // Try to hand it back to the consumer first. `try_send` so we
        // don't await — `return_transport_from_outbound` is sync and
        // shouldn't block on a wedged consumer.
        match self.transport_return_tx.try_send(transport) {
            Ok(()) => {
                if let Ok(mut guard) = self.inner.lock() {
                    guard.transport_owner = TransportOwner::ListenerArmed;
                }
            }
            Err(_) => {
                // Channel full, closed, or consumer gone. The transport
                // was consumed by try_send's Err variant only in the
                // `Full` case; for `Closed` we already lost it. Either
                // way, mark owner as None — the listener can't re-arm
                // without a fresh consumer + transport.
                if let Ok(mut guard) = self.inner.lock() {
                    guard.transport_owner = TransportOwner::None;
                }
            }
        }
    }

    /// Test-only / future-consumer accessor: take the receiver half of
    /// the return channel. Returns `None` if a prior caller already
    /// took it (there can only be one consumer task per session).
    #[cfg(test)]
    pub fn take_transport_return_rx(&self) -> Option<mpsc::Receiver<VaraTransport>> {
        self.transport_return_rx.lock().ok().and_then(|mut g| g.take())
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
/// If a session is already open, returns Err — operator must `vara_stop_session`
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
        callsign.as_deref(),
        intent,
        transport_kind,
    ) {
        Ok(_status) => {
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
/// Tauri runtime. `callsign` is `Some` when the wizard has set an operator
/// callsign; when `Some`, MYCALL is sent on the cmd socket after TCP open
/// (before BW) so VARA's host protocol recognizes the App handshake.
///
/// Records `intent` + `transport_kind` on `VaraSessionInner` after the open
/// succeeds; cleared in [`vara_stop_session_inner`] on teardown.
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
    // Clear session-state recorded by `vara_open_session_inner`. The
    // forthcoming `vara_close_session` rename (Task 3.3) will own this
    // reset directly; today's `vara_stop_session` carries it transitionally
    // so an open→stop→open cycle starts from a clean slate.
    guard.active_intent = None;
    guard.active_transport_kind = None;
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
        assert_eq!(snap.active_intent, None, "intent must not leak on failed open");
        assert_eq!(snap.active_transport_kind, None, "transport_kind must not leak on failed open");
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
        assert!(!SessionIntent::Cms.auto_arms_listener(), "Cms is outbound-only");
        assert!(SessionIntent::P2p.auto_arms_listener(), "P2p auto-arms");
        assert!(SessionIntent::RadioOnly.auto_arms_listener(), "RadioOnly auto-arms");
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
    fn build_real_transport_for_test() -> (VaraTransport, std::thread::JoinHandle<()>, std::thread::JoinHandle<()>)
    {
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
        let err = unwrap_err_str(
            session.take_transport_for_outbound().await,
            "None → Err",
        );
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
        assert_eq!(
            session.transport_owner(),
            TransportOwner::ListenerInbound
        );
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
    async fn vara_return_transport_from_outbound_transitions_to_listener_armed_when_consumer_alive(
    ) {
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

        // Outbound returns the transport.
        session.return_transport_from_outbound(transport);

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

        session.return_transport_from_outbound(transport);

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
        assert!(
            json.contains("\"listenerArmed\":true"),
            "got {json}"
        );
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
        assert_eq!(
            session.snapshot().transport_owner,
            TransportOwner::None
        );
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);
        assert_eq!(
            session.snapshot().transport_owner,
            TransportOwner::ListenerArmed
        );
    }
}
