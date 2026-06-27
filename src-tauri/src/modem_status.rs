use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, Notify};

use crate::winlink::listener::transport::TransportKind;
use crate::winlink::session::SessionIntent;

/// Hard-close handle for a cmd-socket clone (tuxlink-0ye6 Task 4.1 / Codex
/// Round 4 P1 #3). Paired with the cooperative write half installed via
/// [`ModemSession::install_abort_writer`] so a wedged peer (one that doesn't
/// drain its cmd socket) can still be torn down inside the bounded
/// `abort_in_flight` budget: the cooperative write times out, the fallback
/// calls `shutdown_both` to RST the TCP stream, and the modem notices and
/// halts TX on its end.
///
/// Implemented for [`std::net::TcpStream`] (calls `TcpStream::shutdown(Both)`)
/// and for test-only spies that record the invocation. Both ARDOP and VARA
/// session layers share this trait so a single discipline covers both
/// transports (Codex Round 4 P1 #4).
pub trait ShutdownableStream: Send {
    /// Shut down the underlying stream for BOTH read and write directions,
    /// best-effort. Returning `Ok(())` does not promise the peer noticed;
    /// callers in the abort fallback path discard the result and surface
    /// `Err("...; hard-closed")` to the operator regardless.
    fn shutdown_both(&mut self) -> std::io::Result<()>;
}

impl ShutdownableStream for std::net::TcpStream {
    fn shutdown_both(&mut self) -> std::io::Result<()> {
        std::net::TcpStream::shutdown(self, std::net::Shutdown::Both)
    }
}

/// Bound on cooperative ABORT/DISCONNECT writes (Codex Round 3 P1 #1).
///
/// Sized to absorb a single send-buffer-full retry on a healthy peer
/// without exceeding the spec §2 "abort within ~2s" contract. A wedged
/// peer that doesn't drain its cmd socket trips this timeout and the
/// fallback hard-close runs, keeping the total `abort_in_flight` budget
/// under 2 seconds end-to-end.
pub const ABORT_WRITE_TIMEOUT: Duration = Duration::from_millis(1500);

/// Bounded timeout on the arbiter's listener-yield wait (Codex Round 3 P1 #2).
///
/// If the listener consumer task is wedged in its accept loop, dead, or
/// missed the notify, an unbounded await would leave outbound stuck in
/// [`TransportOwner::OutboundPending`] forever — the operator's only
/// recovery is `Close Session`. Three seconds is long enough to absorb
/// the consumer task's accept-loop wake latency on a stressed Pi (the
/// consumer polls its inbound socket with ~100 ms granularity) but short
/// enough that an operator who fires a Connect during a wedged-consumer
/// race gets a useful "modem busy" error within a tick.
///
/// Shared between ARDOP ([`ModemSession`]) and VARA
/// (`winlink::modem::vara::commands::VaraSession`) so the operator-facing
/// timeout characteristic is uniform across transports.
pub const ARBITER_YIELD_TIMEOUT: Duration = Duration::from_secs(3);

/// Coarse ownership state for the live modem transport — the arbiter
/// (tuxlink-0ye6 Task 4.3; Codex Round 1 P1 #5).
///
/// The spec assumes the listener can stay armed across multiple
/// operator-initiated outbound dials within a single session. Without an
/// arbiter the bare `take_transport()` calls on both sides race: outbound
/// either finds no transport to dial with (consumer holds it) or pulls it
/// out from under the consumer (silently disarms the listener). This enum
/// is the load-bearing invariant — **at most ONE owner** holds the
/// transport at any moment.
///
/// Defined here (in the parent `modem_status` module) rather than on
/// either of the transport-specific session types because the semantics
/// are identical for ARDOP + VARA and downstream wire-up (Task 5.x shared
/// panel) wants to reason about ownership generically.
///
/// Transitions are guarded by the owning session's `inner` std-mutex; per
/// Codex Round 2 P1 #4 that lock MUST be dropped before any async await,
/// so each session's `take_transport_for_outbound` snapshots+records
/// under the lock then awaits the yield channel with the lock released.
///
/// **Serde shape (Codex Round 4 P2 — tuxlink-0ye6 Task 3.0):** the parent
/// `ModemStatus` / `VaraStatus` structs carry `#[serde(rename_all =
/// "camelCase")]`, but that only renames struct FIELDS — enum variants
/// need their own `rename_all` derive. Without this, frontend would
/// receive `"ListenerArmed"` (PascalCase) while expecting `"listenerArmed"`
/// (camelCase) for the `transportOwner` field. Variants serialize as:
/// `none`, `listenerArmed`, `listenerInbound`, `outboundPending`,
/// `outbound`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportOwner {
    /// Session closed — no transport handle is installed. Outbound
    /// requests reject with "session not open".
    None,
    /// Listener consumer task holds the transport AND no inbound exchange
    /// is in flight (i.e. the consumer is idle in its accept loop).
    /// Outbound MAY request the transport; the consumer yields it on
    /// notify.
    ListenerArmed,
    /// Listener consumer task holds the transport AND an inbound
    /// exchange is currently running through it. Outbound requests
    /// reject with "modem busy — inbound exchange in progress".
    ListenerInbound,
    /// Outbound has signalled the consumer to yield and is awaiting the
    /// transport via the yield channel. Intermediate state — bounded by
    /// [`ARBITER_YIELD_TIMEOUT`] on the outbound side.
    OutboundPending,
    /// Outbound exchange holds the transport. Another outbound request
    /// rejects with "outbound exchange already in flight".
    Outbound,
}

/// Coarse classification of an in-flight ARQ exchange (tuxlink-0ye6
/// Task 3.0 / Codex Round 2 P1 #5). Distinct from [`ModemState`] /
/// [`VaraState`] (which model the transport posture) — `ExchangeState`
/// describes what *kind* of exchange is currently running over an open
/// transport. `None` on the parent DTO means "no exchange in flight."
///
/// Serialized kebab-case per the Round 2 plan: `"dialing"` / `"outbound"`
/// / `"inbound"` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExchangeState {
    /// `connect_arq` in flight — the outbound dial is bringing up the
    /// ARQ link but B2F has not started yet.
    Dialing,
    /// CONNECTED; B2F handshake or message drain is running (outbound
    /// dial side).
    Outbound,
    /// Listener accepted an inbound peer; B2F handshake or message
    /// drain is running (peer-initiated side).
    Inbound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ModemState {
    Stopped,
    Spawning,
    Initializing,
    Idle,
    Connecting,
    ConnectedIrs,
    ConnectedIss,
    Disconnecting,
    Error,
    /// cmd-port unresponsive or ardopcf process exit detected by the
    /// per-session heartbeat probe (Codex Round 3 P1 #4 / spec §2.6,
    /// added with tuxlink-0ye6 Task 3.0). Operator's only recovery is
    /// Close Session → reopen; the backend tears down the dead handle
    /// on Close. The frontend `useRadioSessionLifecycle` hook maps this
    /// state to its `'crash-recovery'` UI surface, distinct from
    /// `Error` (per-action failure).
    ///
    /// Heartbeat infrastructure that drives this transition is deferred
    /// to a follow-up task (the probe needs Tauri commands
    /// `vara_open_session` / `ardop_open_session` that don't exist yet
    /// — Phase 3.2 / 3.5). This variant ships now so that follow-up
    /// task is a pure additive wire-in.
    SocketLost,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArqFlags {
    pub busy: bool,
    pub rx: bool,
    pub tx: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModemStatus {
    pub state: ModemState,
    pub peer: Option<String>,
    pub mode: Option<String>,
    pub width_hz: Option<u32>,
    pub ptt_backend: Option<String>, // "rts" | "cat" | "vox"
    pub sn_db: Option<f32>,
    pub vu_dbfs: Option<f32>,
    pub throughput_bps: Option<u32>,
    pub bytes_rx: u64,
    pub bytes_tx: u64,
    pub uptime_sec: u64,
    pub arq_flags: ArqFlags,
    pub last_error: Option<String>,
    /// ardopcf Quality score (0..=100), populated from PINGACK / PING events.
    /// `None` until the first ping has been observed; held across the rest
    /// of the session as the last-known reading. Closes tuxlink-1637 — the
    /// Signal-section "Quality" big-number indicator (spec §5.3) reads
    /// this field via the `modem:status` event.
    pub quality: Option<u8>,
    // ── Lifecycle fields (tuxlink-0ye6 Task 3.0 / Codex Round 2 P1 #5 +
    // Round 3 P1 #3 + Round 4 P1 #1) ──────────────────────────────────
    //
    // The shared `useRadioSessionLifecycle` hook (Phase 5 / Task 5.2)
    // derives the open-session UI state from these fields rather than
    // holding it in React local state. Without them the frontend cannot
    // detect listener-armed / exchange-in-flight / active-mode without a
    // separate command surface per concern.
    //
    /// True iff a listener consumer task is currently armed on this
    /// transport. Reflects the operator's "session-open" affordance plus
    /// the listener-arm side effect for intents that auto-arm
    /// (`SessionIntent::auto_arms_listener` — `P2p` + `RadioOnly`).
    ///
    /// Stub today: returns `false` until Phase 3.6 wires this to the real
    /// listener-consumer state. The DTO field ships now so the wire-in is
    /// purely additive (no DTO shape change downstream).
    pub listener_armed: bool,
    /// In-flight ARQ exchange classification. `None` means no exchange is
    /// running over the transport (the transport may still be open + the
    /// listener may still be armed; this field tracks the exchange layer
    /// specifically). See [`ExchangeState`].
    ///
    /// Stub today: returns `None` until Phase 3.5 (outbound `b2f_exchange`)
    /// + Phase 3.4 (listener consumer task) wire this from the arbiter +
    ///   exchange-runner state.
    pub exchange: Option<ExchangeState>,
    /// Coarse ownership of the live transport — the arbiter's view
    /// (tuxlink-0ye6 Task 4.3). Real value, sourced from
    /// [`ModemSession::transport_owner`]. Frontend uses this to disable
    /// outbound dial while the listener holds the transport for an
    /// inbound exchange, and to render the "modem busy" pill.
    pub transport_owner: TransportOwner,
    /// The intent of the currently-open session, if any (Codex Round 3
    /// P1 #3 / Round 4 P1 #1). `None` when no session is open. Used by
    /// the frontend sidebar-navigation guard (spec §2.5) to detect mode
    /// drift when the operator clicks a different sidebar entry while a
    /// session is still open under the original intent.
    ///
    /// Stub today: returns `None` until Phase 3.2 + 3.5 wire this to
    /// the session-open command.
    pub active_intent: Option<SessionIntent>,
    /// The transport-kind of the currently-open session, if any. Pairs
    /// with [`Self::active_intent`] for the navigation-drift guard:
    /// VARA HF and VARA FM are distinct sidebar entries that the
    /// backend gates separately, so the frontend needs both intent +
    /// transport-kind to detect a navigation drift.
    ///
    /// Stub today: returns `None` until Phase 3.2 + 3.5 wire this.
    pub active_transport_kind: Option<TransportKind>,
    /// Live VFO frequency (Hz) read back from the rig over CAT, when the
    /// live-VFO poll thread is running (rig-control LIVE-VFO POLL). Present
    /// only on the DRA-100 keep-serial path with `live_vfo_poll` enabled:
    /// a dedicated poll thread (its own rigctld client) writes the latest
    /// reading here every ~2 s via [`ModemSession::set_rig_freq_hz`].
    ///
    /// `None` until the first successful read, and cleared by
    /// [`ModemSession::reset_to_stopped`] on disconnect. The frontend's
    /// ARDOP frequency element renders the live MHz when this is `Some`,
    /// falling back to the configured/idle frequency otherwise.
    pub rig_freq_hz: Option<u64>,
}

impl ModemStatus {
    pub fn stopped() -> Self {
        Self {
            state: ModemState::Stopped,
            peer: None,
            mode: None,
            width_hz: None,
            ptt_backend: None,
            sn_db: None,
            vu_dbfs: None,
            throughput_bps: None,
            bytes_rx: 0,
            bytes_tx: 0,
            uptime_sec: 0,
            arq_flags: ArqFlags { busy: false, rx: false, tx: false },
            last_error: None,
            quality: None,
            listener_armed: false,
            exchange: None,
            transport_owner: TransportOwner::None,
            active_intent: None,
            active_transport_kind: None,
            rig_freq_hz: None,
        }
    }
}

/// Shared per-app modem session state.
///
/// Wraps the current `ModemStatus` snapshot + the in-process RADIO-1 consent
/// token + the live `ModemTransport` handle (when a connect has succeeded).
/// `Arc<ModemSession>` is stored in Tauri state and shared between command
/// handlers and the broadcaster.
pub struct ModemSession {
    inner: Mutex<ModemSessionInner>,
    /// Busy guard: set to `true` while a connect is in flight.
    /// Guards against duplicate concurrent connect invocations (the dup-call
    /// defense previously provided as a side-effect by the consent token's
    /// consume semantics). Set via [`try_begin_connect`] BEFORE any I/O;
    /// cleared via [`clear_connect_in_progress`] on every exit path via RAII.
    connect_in_progress: AtomicBool,
    /// Monotonic close-generation counter (tuxlink-pdnw — Codex Phase 3-4
    /// boundary P1 #1, #5). Bumped by every close path BEFORE the close
    /// reaches `reset_to_stopped` / disarms the listener consumer. Workers
    /// that take the transport (b2f exchange, listener consumer task)
    /// snapshot the value before the take; on the return-to-session path
    /// (`install_transport_if_generation_matches` /
    /// `return_transport_from_outbound`) they check that the snapshot still
    /// matches the live generation. If a close intervened, the snapshot is
    /// stale and the transport is dropped instead of re-installed —
    /// preventing the close-vs-armed-consumer race where the worker
    /// reinstalls the transport into a session the operator just closed.
    ///
    /// Decoupled from the inner mutex so a snapshot is lock-free (the
    /// `Acquire` load is faster than acquiring the std-mutex and gives the
    /// same ordering guarantees against the `bump` path's `AcqRel`).
    /// Monotonically growing — never reset, even on re-open — so each new
    /// open's worker takes a fresh snapshot tied to that open's generation
    /// number. Survives close → re-open cycles cleanly.
    close_generation: AtomicU64,
    /// Transport-arbiter signal: outbound calls `notify_one()` to ask the
    /// listener consumer task to yield the transport. The consumer task
    /// holds `notified()` while idle in its accept loop. Decoupled from
    /// the std-mutex so the consumer never blocks waiting for outbound to
    /// finish a state transition (tuxlink-0ye6 Task 4.3, Codex Round 2
    /// P1 #4).
    transport_yield_request: Arc<Notify>,
    /// Transport-arbiter rendezvous: the listener consumer sends its held
    /// transport here when it observes the yield request. Outbound awaits
    /// this channel (with the std-mutex DROPPED) to receive the transport.
    transport_yield_rx: tokio::sync::Mutex<mpsc::Receiver<Box<dyn crate::winlink::modem::ModemTransport>>>,
    /// Cloneable sender for [`transport_yield_rx`]. Handed to the listener
    /// consumer task when it spawns.
    ///
    /// `#[allow(dead_code)]`: the production accessor lands in the
    /// Phase 3 listener-consumer wiring (task 3.6). This task ships
    /// the arbiter primitives + tests in isolation; the wiring follows
    /// in a sibling dispatch.
    #[allow(dead_code)]
    transport_yield_tx: mpsc::Sender<Box<dyn crate::winlink::modem::ModemTransport>>,
    /// Reverse-direction rendezvous: the arbiter sends the transport
    /// here after outbound completes. The consumer task awaits on
    /// `recv` to reclaim the transport and re-arm.
    transport_return_tx: mpsc::Sender<Box<dyn crate::winlink::modem::ModemTransport>>,
    /// Receiver counterpart to [`transport_return_tx`]. Owned by the
    /// consumer task at spawn time (acquired via
    /// `take_transport_return_rx`).
    ///
    /// `#[allow(dead_code)]`: the production accessor lands in the
    /// Phase 3 listener-consumer wiring (task 3.6). The test-only
    /// accessor already reads this field under `#[cfg(test)]`.
    #[allow(dead_code)]
    transport_return_rx: Mutex<Option<mpsc::Receiver<Box<dyn crate::winlink::modem::ModemTransport>>>>,
    /// Stop flag for the live-VFO poll thread (rig-control LIVE-VFO POLL).
    /// Cloned into the poll thread when it spawns; the thread checks it each
    /// loop iteration and exits when it flips to `true`. Set by
    /// [`ModemSession::stop_rig_poll`] (called from every disconnect path
    /// before the rig is dropped) so the thread stops cooperatively rather
    /// than only via its independent client erroring after rigctld dies.
    ///
    /// Decoupled from the inner mutex so the poll thread never contends with
    /// status ticks: it loads the flag lock-free and only grabs the mutex to
    /// write a fresh frequency reading.
    rig_poll_stop: Arc<AtomicBool>,
    /// Join handle for the live-VFO poll thread, if one is running. Taken +
    /// joined by [`ModemSession::stop_rig_poll`] so a disconnect blocks until
    /// the prior poll thread has exited — preventing a thread leak across
    /// reconnects (a fresh connect would otherwise spawn a second poller while
    /// the first is still draining its final read/sleep).
    rig_poll_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl std::fmt::Debug for ModemSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModemSession")
            .field("inner", &self.inner)
            .field("connect_in_progress", &self.connect_in_progress)
            .finish_non_exhaustive()
    }
}

struct ModemSessionInner {
    status: ModemStatus,
    /// Live transport handle, present after a successful
    /// `modem_ardop_connect`. `Box<dyn ModemTransport>` is `Send` (per
    /// `winlink/modem/mod.rs:47`), so the surrounding `Mutex` is still
    /// `Sync` — `Arc<ModemSession>` can flow through Tauri's managed state.
    ///
    /// Trait-object hand-off: ownership of the live transport lives in
    /// `Option<Box<dyn ModemTransport>>` rather than a generic type so that
    /// future modems (Dire Wolf, sonde, etc.) can swap in without
    /// reshaping the session struct.
    transport: Option<Box<dyn crate::winlink::modem::ModemTransport>>,
    /// Cooperative cmd-socket writer (the transport's side-channel abort
    /// handle). Installed via [`ModemSession::install_abort_writer`] BEFORE
    /// `connect_arq` begins blocking, and consumed by
    /// [`ModemSession::abort_in_flight`] to send `ABORT\r` to ardopcf while
    /// the connect path is stuck in its recv loop (tuxlink-o3f2 — P1
    /// abort-during-connect fix).
    ///
    /// The writer carries a bounded `write_timeout` ([`ABORT_WRITE_TIMEOUT`])
    /// so a wedged peer cannot stall the abort budget past spec §2's
    /// "~2s" contract (Codex Round 3 P1 #1).
    ///
    /// Cleared by [`ModemSession::reset_to_stopped`] so a fresh connect
    /// installs a fresh writer.
    abort_writer: Option<Box<dyn std::io::Write + Send>>,
    /// Hard-close handle paired with [`abort_writer`]. When the cooperative
    /// write fails (peer wedged, send buffer full past `write_timeout`),
    /// [`ModemSession::abort_in_flight`] takes this handle, calls
    /// `shutdown_both`, and surfaces an error to the operator (Codex
    /// Round 4 P1 #3). The TCP RST forces the modem to notice the
    /// teardown and halt any in-flight TX even though the cooperative
    /// command never made it across.
    ///
    /// Cleared together with [`abort_writer`] in [`reset_to_stopped`].
    abort_stream: Option<Box<dyn ShutdownableStream>>,
    /// Current ownership of the live transport (tuxlink-0ye6 Task 4.3,
    /// Codex Round 1 P1 #5). See [`TransportOwner`] for the state
    /// machine; transitions are guarded by the enclosing std-mutex.
    transport_owner: TransportOwner,
    /// Operator-typed intent for the currently-open session, if any
    /// (tuxlink-0ye6 Task 3.5 — ARDOP analog of VARA's
    /// `VaraSessionInner::active_intent`). Set by
    /// [`ardop_open_session_inner`](crate::modem_commands::ardop_open_session_inner)
    /// after the spawn + init succeeds; cleared by
    /// [`ModemSession::reset_to_stopped`] /
    /// [`ModemSession::clear_active_session_mode`].
    ///
    /// Exposed read-only via [`ModemSession::active_intent`] (which today
    /// is a Task 3.0 stub returning `None`; Task 3.5 wires it to this
    /// field). Pairs with [`Self::active_transport_kind`] for the
    /// frontend's sidebar-navigation drift guard (spec §2.5).
    active_intent: Option<SessionIntent>,
    /// Transport-kind of the currently-open session, if any. Same
    /// lifecycle as [`Self::active_intent`]. For ARDOP this is always
    /// `Some(TransportKind::Ardop)` after a successful open; the field
    /// exists for shape parity with VARA (which discriminates `VaraHf`
    /// vs `VaraFm`) so the frontend's sidebar-nav drift guard reads a
    /// uniform `(intent, transport_kind)` pair across modems.
    active_transport_kind: Option<TransportKind>,
    /// Live CAT rig handle for the connected session (tuxlink rig-control
    /// Task 8/9). Present only on the DRA-100 (keep-serial) path: the
    /// pre-audio tune left rigctld running so the operator keeps CAT control
    /// for the session's duration. `None` on the close-serial (internal-codec)
    /// path, where [`crate::modem_commands::tune_rig_for_connect`] released
    /// the serial before audio.
    ///
    /// Dropped by [`ModemSession::reset_to_stopped`] — the only full-teardown
    /// path — so rigctld stops when the operator disconnects. `ManagedRig::Drop`
    /// SIGKILLs + reaps its child, so the drop releases the CAT serial. NOT
    /// cleared by [`Self::take_transport`], which is a temporary borrow
    /// (b2f-exchange / listener arbiter) that re-installs the transport into
    /// the still-live session.
    rig: Option<tux_rig::ManagedRig>,
}

// Manual `Debug` impl: `Box<dyn ModemTransport>` does not implement `Debug`,
// so `#[derive(Debug)]` would fail. Print the non-transport fields verbatim
// and a placeholder for the transport handle.
impl std::fmt::Debug for ModemSessionInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModemSessionInner")
            .field("status", &self.status)
            .field(
                "transport",
                &self
                    .transport
                    .as_ref()
                    .map(|_| "Some(<dyn ModemTransport>)"),
            )
            .field(
                "abort_writer",
                &self.abort_writer.as_ref().map(|_| "Some(<dyn Write>)"),
            )
            .field(
                "abort_stream",
                &self.abort_stream.as_ref().map(|_| "Some(<dyn ShutdownableStream>)"),
            )
            .field("transport_owner", &self.transport_owner)
            .field("active_intent", &self.active_intent)
            .field("active_transport_kind", &self.active_transport_kind)
            .field("rig", &self.rig.as_ref().map(|_| "Some(<ManagedRig>)"))
            .finish()
    }
}

impl ModemSession {
    pub fn new() -> Self {
        // mpsc channels are bounded; capacity 1 — only ONE transport
        // handoff is in flight per direction at any moment (the arbiter
        // invariant). Bounded so a regression doesn't silently queue
        // stale transports.
        let (yield_tx, yield_rx) = mpsc::channel(1);
        let (return_tx, return_rx) = mpsc::channel(1);
        Self {
            inner: Mutex::new(ModemSessionInner {
                status: ModemStatus::stopped(),
                transport: None,
                abort_writer: None,
                abort_stream: None,
                transport_owner: TransportOwner::None,
                active_intent: None,
                active_transport_kind: None,
                rig: None,
            }),
            connect_in_progress: AtomicBool::new(false),
            close_generation: AtomicU64::new(0),
            transport_yield_request: Arc::new(Notify::new()),
            transport_yield_rx: tokio::sync::Mutex::new(yield_rx),
            transport_yield_tx: yield_tx,
            transport_return_tx: return_tx,
            transport_return_rx: Mutex::new(Some(return_rx)),
            rig_poll_stop: Arc::new(AtomicBool::new(false)),
            rig_poll_handle: Mutex::new(None),
        }
    }

    pub fn status_snapshot(&self) -> ModemStatus {
        let inner = self.inner.lock().unwrap();
        let mut snap = inner.status.clone();
        // Overlay live lifecycle fields (tuxlink-0ye6 Task 3.0). The
        // cached `inner.status` may have stale lifecycle values from a
        // prior `set_status` overwrite; the session's mutex-protected
        // `transport_owner` is the authoritative source. The remaining
        // four fields (listener_armed / exchange / active_intent /
        // active_transport_kind) read through the stub accessors today —
        // Phase 3.2 / 3.4 / 3.5 wires them to real session state.
        snap.transport_owner = inner.transport_owner;
        drop(inner); // release the lock before calling the stubs
        snap.listener_armed = self.listener_armed();
        snap.exchange = self.current_exchange();
        snap.active_intent = self.active_intent();
        snap.active_transport_kind = self.active_transport_kind();
        snap
    }

    pub fn set_status(&self, s: ModemStatus) {
        self.inner.lock().unwrap().status = s;
    }

    /// Single-lock variant of `status_snapshot`: drain the installed
    /// transport's pending status events into the cached snapshot, persist
    /// the updated snapshot, and return a clone. Called by
    /// [`ModemStatusBroadcaster`] every [`STATUS_POLL_INTERVAL`].
    ///
    /// If no transport is installed (e.g. the session is `Stopped` and
    /// nothing has connected yet), there are no events to drain and the
    /// cached snapshot is returned as-is.
    ///
    /// The transport's `drain_status_events` MUST be non-blocking — see
    /// [`crate::winlink::modem::ModemTransport::drain_status_events`].
    pub fn tick_and_snapshot(&self) -> ModemStatus {
        // Phase 1: drain transport events into the cached snapshot
        // under the lock, persist it back. Then drop the lock to read
        // the live lifecycle fields via the stub accessors (they take
        // the same mutex — calling them inside the guard would deadlock).
        let (mut snap, transport_owner) = {
            let mut inner = self.inner.lock().unwrap();
            // Clone the snapshot before mutating so that a panic inside
            // `drain_status_events` leaves the persisted status untouched
            // (poison-aware: the next acquirer will see the pre-drain state).
            let mut working = inner.status.clone();
            if let Some(transport) = inner.transport.as_mut() {
                transport.drain_status_events(&mut working);
            }
            inner.status = working.clone();
            (working, inner.transport_owner)
        };
        // Phase 2: overlay live lifecycle fields (tuxlink-0ye6 Task 3.0).
        snap.transport_owner = transport_owner;
        snap.listener_armed = self.listener_armed();
        snap.exchange = self.current_exchange();
        snap.active_intent = self.active_intent();
        snap.active_transport_kind = self.active_transport_kind();
        snap
    }

    /// Install a live `ModemTransport` handle in the session. Called from
    /// `modem_ardop_connect_post_consume_with_factory` after a successful
    /// `init` + `connect_arq`.
    pub fn install_transport(&self, t: Box<dyn crate::winlink::modem::ModemTransport>) {
        self.inner.lock().unwrap().transport = Some(t);
    }

    /// Store (or clear) the live CAT rig handle for the connected session
    /// (rig-control Task 8/9). Called from the ARDOP connect flow after a
    /// successful pre-audio tune on the DRA-100 (keep-serial) path, with the
    /// `ManagedRig` that
    /// [`crate::modem_commands::tune_rig_for_connect`] kept alive. Pass `None`
    /// (or simply never call it) on the close-serial path, where the helper
    /// already released the serial before audio.
    ///
    /// Replacing an existing rig drops the prior handle (its `Drop` SIGKILLs
    /// rigctld); the swap runs under the lock but the drop of the *prior*
    /// handle happens after the guard is released, so no rigctld teardown
    /// runs while the mutex is held.
    pub fn set_rig(&self, rig: Option<tux_rig::ManagedRig>) {
        let prior = {
            let mut inner = self.inner.lock().unwrap();
            std::mem::replace(&mut inner.rig, rig)
        };
        drop(prior);
    }

    /// Write the live VFO frequency read back from the rig into the cached
    /// status snapshot (rig-control LIVE-VFO POLL). Called by the poll thread
    /// every ~2 s with `Some(hz)` on a successful read. The lock is held only
    /// for the field write — NEVER across the rigctld I/O that produced the
    /// value (the read happens on the poll thread's own client, lock-free).
    ///
    /// `None` clears the reading; [`Self::reset_to_stopped`] already resets the
    /// whole snapshot, so the poller does not need to clear on exit.
    pub fn set_rig_freq_hz(&self, hz: Option<u64>) {
        let mut inner = self.inner.lock().unwrap();
        inner.status.rig_freq_hz = hz;
    }

    /// Spawn the live-VFO poll thread (rig-control LIVE-VFO POLL). The thread
    /// opens its OWN timeout-bounded rigctld client (rigctld accepts multiple
    /// clients; the managed client on the session is left untouched) and loops
    /// every [`RIG_POLL_INTERVAL`]: read the VFO frequency, write it into the
    /// cached status via [`Self::set_rig_freq_hz`], sleep, repeat. It exits
    /// when [`Self::stop_rig_poll`] flips the stop flag, or when its client read
    /// errors (e.g. rigctld was SIGKILLed by the dropped `ManagedRig` on
    /// disconnect — a belt-and-suspenders exit even if the flag is missed).
    ///
    /// Caller contract (the ARDOP connect flow on the DRA-100 keep-serial
    /// path, when `live_vfo_poll` is enabled): call AFTER the rig handle is
    /// stored on the session. Re-spawning replaces any prior poller — this
    /// first stops + joins the old thread via [`Self::stop_rig_poll`] so no
    /// poller leaks across reconnects.
    ///
    /// `host` / `port` are the rigctld endpoint (same values the managed rig
    /// connected to). No-op-safe to call with rigctld down: the connect inside
    /// the thread fails and the thread exits immediately (the readout simply
    /// stays absent).
    pub fn start_rig_poll(self: &Arc<Self>, host: String, port: u16) {
        // Stop + join any prior poller first so we never run two at once.
        self.stop_rig_poll();

        // Clear the stop flag for this generation of the poller.
        self.rig_poll_stop.store(false, Ordering::Release);
        let stop = self.rig_poll_stop.clone();
        let session = self.clone();

        let handle = std::thread::Builder::new()
            .name("rig-vfo-poll".into())
            .spawn(move || {
                // `read_status` is a `tux_rig::Rig` trait method — bring the
                // trait into scope so the call resolves.
                use tux_rig::Rig;
                // Open an independent, timeout-bounded client. A hung rigctld
                // cannot wedge this thread: the bounded read returns an error
                // and the loop exits.
                let mut client = match tux_rig::RigctldClient::connect_with_timeout(
                    &host,
                    port,
                    RIG_POLL_READ_TIMEOUT,
                ) {
                    Ok(c) => c,
                    // rigctld not reachable (down, or racing the spawn): nothing
                    // to poll. Exit; the readout stays absent.
                    Err(_) => return,
                };
                while !stop.load(Ordering::Acquire) {
                    match client.read_status() {
                        Ok(s) => session.set_rig_freq_hz(Some(s.freq_hz)),
                        // Read failed (timeout, rigctld died, socket reset).
                        // Stop polling — the session's dropped ManagedRig will
                        // have killed rigctld on disconnect; a transient error
                        // is also a reasonable exit (the readout freezes at the
                        // last good value rather than thrashing reconnects).
                        Err(_) => break,
                    }
                    // Sleep in short slices so the stop flag is observed
                    // promptly on disconnect rather than after a full interval.
                    let mut slept = Duration::ZERO;
                    while slept < RIG_POLL_INTERVAL && !stop.load(Ordering::Acquire) {
                        std::thread::sleep(RIG_POLL_SLEEP_SLICE);
                        slept += RIG_POLL_SLEEP_SLICE;
                    }
                }
            })
            .expect("failed to spawn rig-vfo-poll thread");

        *self.rig_poll_handle.lock().unwrap() = Some(handle);
    }

    /// Signal the live-VFO poll thread to stop and join it (rig-control
    /// LIVE-VFO POLL). Idempotent + safe to call when no poller is running.
    /// Called by every disconnect path (via [`Self::reset_to_stopped`]) BEFORE
    /// the rig is dropped, so the thread stops cooperatively. The join blocks
    /// until the thread has exited — bounded by one short sleep slice plus one
    /// in-flight bounded read — guaranteeing no poller outlives the session.
    ///
    /// The handle is taken OUTSIDE any other lock and joined with the inner
    /// mutex NOT held, so the joined thread's final [`Self::set_rig_freq_hz`]
    /// (which takes the inner mutex) can complete without deadlocking against
    /// the joiner.
    pub fn stop_rig_poll(&self) {
        self.rig_poll_stop.store(true, Ordering::Release);
        let handle = self.rig_poll_handle.lock().unwrap().take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }

    /// Take ownership of the live transport handle, if any. The caller is
    /// responsible for calling `disconnect()` + dropping it. Intended for
    /// flows that want to shut down the transport WITHOUT also resetting
    /// session status (rare). Most disconnect paths should use
    /// [`reset_to_stopped`].
    ///
    /// **Arbiter side effect (tuxlink-0ye6 Task 4.3):** on a successful
    /// take, the arbiter records the listener as the current owner via
    /// `transport_owner = ListenerArmed`. Outbound's
    /// [`Self::take_transport_for_outbound`] then sequences the yield
    /// when needed.
    pub fn take_transport(&self) -> Option<Box<dyn crate::winlink::modem::ModemTransport>> {
        let mut inner = self.inner.lock().unwrap();
        let t = inner.transport.take();
        if t.is_some() {
            inner.transport_owner = TransportOwner::ListenerArmed;
        }
        t
    }

    /// Atomically take the transport handle and reset the status to `Stopped`.
    /// Returns the prior transport (if any) so the caller can call
    /// `transport.disconnect(...) + drop` OUTSIDE the lock — never call I/O
    /// while holding the session mutex.
    ///
    /// Single lock acquisition: observers see a consistent
    /// `(status=Stopped, transport=None, abort_writer=None)` state.
    ///
    /// tuxlink-o3f2: also clears `abort_writer`, since the underlying
    /// TCP write half is owned by the transport's cmd socket and will
    /// close when the transport is dropped by the caller. A stale writer
    /// pointing at a dead socket is a footgun for the next connect.
    ///
    /// **Arbiter side effect (tuxlink-0ye6 Task 4.3):** also resets
    /// `transport_owner` to `None` — the session is back to a closed
    /// posture and no owner is holding the transport.
    pub fn reset_to_stopped(&self) -> Option<Box<dyn crate::winlink::modem::ModemTransport>> {
        // rig-control LIVE-VFO POLL: stop + join the poll thread FIRST, before
        // the rig is dropped. The join must run with the inner mutex NOT held
        // (the poll thread's final `set_rig_freq_hz` takes it), so this is the
        // very first step — outside the critical section below. Idempotent /
        // no-op when no poller is running.
        self.stop_rig_poll();
        // Take both the transport and the live rig (if any) out under one lock,
        // then drop the rig AFTER releasing the guard so `ManagedRig::Drop`
        // (a SIGKILL + reap of rigctld) never runs while the session mutex is
        // held — same "no I/O under the lock" discipline the transport handle
        // follows (the caller drops the transport outside the lock).
        let (transport, rig) = {
            let mut inner = self.inner.lock().unwrap();
            inner.status = ModemStatus::stopped();
            inner.abort_writer = None;
            inner.abort_stream = None;
            inner.transport_owner = TransportOwner::None;
            // tuxlink-0ye6 Task 3.5: clear active session mode on full reset so
            // a follow-up open starts with a clean slate. Mirrors VARA's
            // `vara_stop_session_inner` clearing the same two fields.
            inner.active_intent = None;
            inner.active_transport_kind = None;
            (inner.transport.take(), inner.rig.take())
        };
        // rig-control Task 8/9: stop rigctld on disconnect (DRA-100 keep path).
        // `None` on the close-serial path — nothing to drop.
        drop(rig);
        transport
    }

    /// Record the operator-typed `intent` + `transport_kind` on the session
    /// (tuxlink-0ye6 Task 3.5). Called by
    /// [`crate::modem_commands::ardop_open_session_inner`] after the
    /// spawn + init succeeds. After this returns,
    /// [`Self::active_intent`] / [`Self::active_transport_kind`] report
    /// the recorded values instead of the Task 3.0 stub `None`.
    ///
    /// Returns the prior `(intent, transport_kind)` pair if one was set —
    /// callers can use this to detect a stale "open over an already-open
    /// session" path (today the outer open helper rejects that case
    /// before reaching this setter, so the return is informational only).
    pub fn set_active_session_mode(
        &self,
        intent: SessionIntent,
        transport_kind: TransportKind,
    ) -> Option<(SessionIntent, TransportKind)> {
        let mut inner = self.inner.lock().unwrap();
        let prior = inner.active_intent.zip(inner.active_transport_kind);
        inner.active_intent = Some(intent);
        inner.active_transport_kind = Some(transport_kind);
        prior
    }

    /// Clear the recorded session mode (tuxlink-0ye6 Task 3.5). Called by
    /// [`crate::modem_commands::ardop_close_session_inner`] before the
    /// transport teardown so a partial-close path still leaves the
    /// session-mode fields consistent. The full
    /// [`Self::reset_to_stopped`] also clears these, so a clean
    /// open → close cycle resets via either path.
    pub fn clear_active_session_mode(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.active_intent = None;
        inner.active_transport_kind = None;
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

    // ── Lifecycle stub accessors (tuxlink-0ye6 Task 3.0) ────────────────
    //
    // The status DTO (`ModemStatus`) exposes `listener_armed`,
    // `exchange`, `active_intent`, and `active_transport_kind`. The
    // real production values for these will be wired in by Phase 3.2
    // (open_session / close_session commands), 3.4 (listener consumer
    // task), and 3.5 (`b2f_exchange` outbound command). Until then the
    // DTO ships with stable stub returns so the wire-in is a pure
    // additive change downstream.

    /// Listener-armed state. STUB: returns `false`.
    // TODO: wire to listener state once Phase 3 commands land
    // (tuxlink-0ye6 Task 3.4 / 3.6 — the listener consumer task is the
    // authoritative source).
    pub fn listener_armed(&self) -> bool {
        false
    }

    /// Current in-flight exchange classification. STUB: returns `None`.
    // TODO: wire to listener state once Phase 3 commands land
    // (tuxlink-0ye6 Task 3.5 outbound + 3.4 inbound — the
    // exchange-runner sets the classification at handshake boundary).
    pub fn current_exchange(&self) -> Option<ExchangeState> {
        None
    }

    /// Intent of the currently-open session, or `None` if no session is
    /// open. Wired to [`ModemSessionInner::active_intent`] by tuxlink-0ye6
    /// Task 3.5 — `ardop_open_session(intent, transport_kind)` captures
    /// the operator-typed intent via [`Self::set_active_session_mode`];
    /// [`Self::clear_active_session_mode`] / [`Self::reset_to_stopped`]
    /// clear it on close.
    ///
    /// Returns `None` if the session mutex is poisoned (defensive — same
    /// posture as [`Self::transport_owner`]).
    pub fn active_intent(&self) -> Option<SessionIntent> {
        self.inner.lock().ok().and_then(|g| g.active_intent)
    }

    /// Transport-kind of the currently-open session, or `None` if no
    /// session is open. Same lifecycle + poisoning semantics as
    /// [`Self::active_intent`]; for ARDOP this is always
    /// `Some(TransportKind::Ardop)` after a successful open.
    pub fn active_transport_kind(&self) -> Option<TransportKind> {
        self.inner.lock().ok().and_then(|g| g.active_transport_kind)
    }

    /// Test-only helper: drive the owner state directly. Used by unit
    /// tests to simulate "listener has the transport and is currently
    /// running an inbound exchange" without spinning up a real consumer
    /// task. Production code MUST drive the owner via the
    /// `take_transport` / `reset_to_stopped` /
    /// `take_transport_for_outbound` / `return_transport_from_outbound`
    /// paths.
    #[cfg(test)]
    pub fn set_transport_owner_for_test(&self, owner: TransportOwner) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.transport_owner = owner;
        }
    }

    /// Test-only clone of the yield-notify handle.
    #[cfg(test)]
    pub fn transport_yield_notify_clone(&self) -> Arc<Notify> {
        self.transport_yield_request.clone()
    }

    /// Test-only clone of the yield-channel sender.
    #[cfg(test)]
    pub fn transport_yield_sender_clone(
        &self,
    ) -> mpsc::Sender<Box<dyn crate::winlink::modem::ModemTransport>> {
        self.transport_yield_tx.clone()
    }

    /// Test-only: replace the yield receiver with one whose paired
    /// sender has been dropped. Models "listener consumer task exited"
    /// for the bounded-yield error path.
    #[cfg(test)]
    pub async fn install_closed_yield_channel_for_test(&self) {
        let (closed_tx, closed_rx) =
            mpsc::channel::<Box<dyn crate::winlink::modem::ModemTransport>>(1);
        drop(closed_tx);
        let mut rx_guard = self.transport_yield_rx.lock().await;
        *rx_guard = closed_rx;
    }

    /// Test-only / future-consumer accessor: take the receiver half of
    /// the return channel. Returns `None` if a prior caller already
    /// took it (there can only be one consumer task per session).
    #[cfg(test)]
    pub fn take_transport_return_rx(
        &self,
    ) -> Option<mpsc::Receiver<Box<dyn crate::winlink::modem::ModemTransport>>> {
        self.transport_return_rx.lock().ok().and_then(|mut g| g.take())
    }

    /// Outbound request: snapshot+record under the std-mutex, drop the
    /// mutex, then await the listener consumer's yield via the
    /// transport-yield channel. Hands the transport to outbound.
    ///
    /// **Codex Round 2 P1 #4 — lock-drop-before-await.** The std mutex
    /// is acquired only for the snapshot+notify+state-transition phase;
    /// the .await happens with the lock released. Holding it across the
    /// await would (a) deadlock against the listener consumer task that
    /// needs session state to honor the yield, and (b) not even compile
    /// (`std::sync::MutexGuard: !Send`).
    ///
    /// **Codex Round 3 P1 #2 — bounded wait.** After
    /// [`ARBITER_YIELD_TIMEOUT`] (3 s) we reset to
    /// [`TransportOwner::None`] and surface "modem busy — listener did
    /// not yield within {timeout}".
    ///
    /// See the docstring on `VaraSession::take_transport_for_outbound`
    /// for the matching ARDOP-side semantics — the two are deliberately
    /// kept in lockstep.
    pub async fn take_transport_for_outbound(
        &self,
    ) -> Result<Box<dyn crate::winlink::modem::ModemTransport>, String> {
        // Phase 1: snapshot + record under the std-mutex; drop before await.
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|e| format!("session lock poisoned: {e}"))?;
            match inner.transport_owner {
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
                    inner.transport_owner = TransportOwner::OutboundPending;
                }
            }
        } // std-mutex guard dropped here — REQUIRED before .await

        self.transport_yield_request.notify_one();

        // Phase 2: bounded await on the yield channel (no std-mutex held).
        let yield_result = {
            let mut rx_guard = self.transport_yield_rx.lock().await;
            tokio::time::timeout(ARBITER_YIELD_TIMEOUT, rx_guard.recv()).await
        };

        let transport = match yield_result {
            Ok(Some(t)) => t,
            Ok(None) => {
                if let Ok(mut inner) = self.inner.lock() {
                    inner.transport_owner = TransportOwner::None;
                }
                return Err(
                    "listener consumer task exited; session needs Close + reopen"
                        .into(),
                );
            }
            Err(_elapsed) => {
                if let Ok(mut inner) = self.inner.lock() {
                    inner.transport_owner = TransportOwner::None;
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
            let mut inner = self
                .inner
                .lock()
                .map_err(|e| format!("session lock poisoned: {e}"))?;
            inner.transport_owner = TransportOwner::Outbound;
        }

        Ok(transport)
    }

    /// Outbound completes: return the transport to the consumer (if
    /// alive) or drop it (if not). Transitions owner accordingly:
    ///
    /// - Generation mismatch (close intervened) → owner = `None`, transport
    ///   dropped via the returned channel-send path NOT taken; the
    ///   transport is moved into a local and dropped explicitly.
    /// - Consumer still listening → owner = `ListenerArmed`, transport
    ///   pushed through `transport_return_tx`.
    /// - Consumer gone (return_tx send fails) → owner = `None`,
    ///   transport dropped.
    ///
    /// **`snapshot_gen` (tuxlink-pdnw — Codex Phase 3-4 P1 #1, #5):** the
    /// caller passes the value captured via
    /// [`Self::current_close_generation`] BEFORE the outbound take. If a
    /// close path bumped the generation while outbound was in flight, the
    /// snapshot is stale and the transport is dropped instead of returned —
    /// preventing the close-vs-armed-consumer race where the worker would
    /// otherwise restore the transport into a session the operator just
    /// closed.
    ///
    /// Best-effort: ignores Mutex poisoning + send failures because the
    /// outbound side has already completed; we're cleaning up.
    pub fn return_transport_from_outbound(
        &self,
        transport: Box<dyn crate::winlink::modem::ModemTransport>,
        snapshot_gen: u64,
    ) {
        // Codex Phase 3-4 RE-REVIEW P1: same TOCTOU class as
        // `install_transport_if_generation_matches` — the generation check
        // MUST happen inside the mutex critical section so a concurrent
        // close cannot bump-and-clear between our load and our hand-off
        // attempt. The mutex serializes with `reset_to_stopped`'s
        // clear-and-release; once we hold the mutex AND see a matching
        // generation, close is either "fully done before us" (we drop) or
        // "fully not started before us" (we proceed). No interleaved
        // close-already-won-but-not-yet-applied state is observable.
        match self.inner.lock() {
            Ok(mut inner) => {
                let live = self.close_generation.load(Ordering::Acquire);
                if live != snapshot_gen {
                    inner.transport_owner = TransportOwner::None;
                    drop(transport);
                    return;
                }
                // gen still matches; try to hand off to consumer
                match self.transport_return_tx.try_send(transport) {
                    Ok(()) => inner.transport_owner = TransportOwner::ListenerArmed,
                    Err(_) => inner.transport_owner = TransportOwner::None,
                }
            }
            Err(_poisoned) => {
                // Mutex poisoned; treat as session gone. Drop the transport.
                drop(transport);
            }
        }
    }

    /// Install the side-channel cmd-socket writer + hard-close stream used by
    /// [`abort_in_flight`]. Called from
    /// `modem_ardop_connect_post_consume_with_factory` AFTER `init` opens the
    /// cmd socket but BEFORE `connect_arq` begins blocking on its recv loop —
    /// that ordering is the whole point of the abort-during-connect fix
    /// (tuxlink-o3f2).
    ///
    /// **Two-arg form (tuxlink-0ye6 Task 4.1 — Codex Round 4 P1 #3):** the
    /// `writer` is the cooperative path (sends `ABORT\r`), and `stream` is
    /// the hard-close fallback used when the cooperative write fails. The
    /// caller is responsible for setting `writer`'s `write_timeout` to
    /// [`ABORT_WRITE_TIMEOUT`] before passing it in — the bound must live
    /// at the socket layer, not the session layer, because the session
    /// doesn't own the concrete `TcpStream`.
    ///
    /// Replaces any previously-installed writer / stream.
    pub fn install_abort_writer(
        &self,
        writer: Box<dyn std::io::Write + Send>,
        stream: Box<dyn ShutdownableStream>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.abort_writer = Some(writer);
        inner.abort_stream = Some(stream);
    }

    /// Best-effort bounded abort of any in-flight TX. Cooperatively writes
    /// `ABORT\r` via the installed writer; on Err, falls back to a
    /// hard-close of the underlying stream via `shutdown_both` so the modem
    /// notices via TCP RST and halts TX on its end (tuxlink-0ye6 Task 4.1 —
    /// Codex Round 4 P1 #3).
    ///
    /// Returns:
    /// - `Ok(())` when the cooperative write succeeded.
    /// - `Err(BrokenPipe)` with message `"... hard-closed"` when the
    ///   cooperative write failed and the fallback fired (regardless of
    ///   whether `shutdown_both` itself returned Ok or Err — the
    ///   operator-surfaced fact is "tear-down ran via the fallback path").
    /// - `Err(NotConnected)` when no writer is installed (caller can
    ///   fall through to the graceful `take_transport`/`disconnect` path).
    ///
    /// Bounded by the writer's `write_timeout` (1500 ms on production
    /// transports) + a single `shutdown_both` syscall — total runtime
    /// fits under spec §2's "abort within ~2s" contract regardless of
    /// which path executes (Codex Round 3 P1 #1).
    ///
    /// tuxlink-o3f2 history: while `arq_connect` is blocking on the
    /// cmd-socket recv channel, the transport is held as a stack local
    /// in the connect call's frame — `take_transport` would observe
    /// `None`. Sending `ABORT` via the side-channel writer causes
    /// ardopcf to halt the in-flight TX and emit `FAULT` (or `NEWSTATE
    /// DISC`), which the cmd reader thread delivers via the existing
    /// channel; `arq_connect`'s recv loop then returns
    /// `Err(SessionError::Fault(...))` and the connect path unwinds
    /// cleanly. The Task 4.1 hard-close fallback covers the case where
    /// the modem stops draining its cmd socket entirely (Codex's
    /// wedged-peer scenario).
    pub fn abort_in_flight(&self) -> std::io::Result<()> {
        use std::io::Write;
        let mut inner = self.inner.lock().unwrap();
        if inner.abort_writer.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "no abort writer installed",
            ));
        }
        // Phase 1: cooperative bounded write. Use a single write_all so the
        // writer's write_timeout governs the upper bound on this path.
        let cooperative = {
            let writer = inner.abort_writer.as_mut().expect("checked above");
            writer
                .write_all(b"ABORT\r")
                .and_then(|()| writer.flush())
        };
        if cooperative.is_ok() {
            return Ok(());
        }
        // Phase 2: cooperative write failed (timeout, WouldBlock, BrokenPipe,
        // etc.) — take the stream and hard-close it. Drop the writer too:
        // it's pointing at the same wedged socket and is no longer useful.
        // Discard the shutdown result deliberately — even an Err here means
        // the underlying socket is gone, which IS the effective tear-down.
        inner.abort_writer = None;
        if let Some(mut stream) = inner.abort_stream.take() {
            let _ = stream.shutdown_both();
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "ARDOP cmd port unresponsive; hard-closed",
        ))
    }

    /// Read-only check: is a transport currently installed? Used by the ARDOP
    /// listener Tauri command to decide between "start modem in listen-only
    /// mode" vs "modem already running — just flip LISTEN TRUE."
    pub fn snapshot_transport_present(&self) -> bool {
        self.inner
            .lock()
            .map(|g| g.transport.is_some())
            .unwrap_or(false)
    }

    /// Best-effort send of `LISTEN TRUE\r` or `LISTEN FALSE\r` to ardopcf via
    /// the side-channel writer (the same `abort_writer` used by
    /// [`abort_in_flight`] — both commands ride the same cmd-socket clone).
    ///
    /// Used by the listener Tauri commands (`ardop_listen` /
    /// `ardop_set_listen`) to toggle the modem's inbound-accept flag on a
    /// running modem. Returns `Err(NotConnected)` when no writer is installed
    /// (i.e. the modem isn't running yet); the caller surfaces that as a
    /// "start the modem first" message to the operator.
    ///
    /// Note: this only toggles the modem's LISTEN flag. CONNECTED-event
    /// routing to the application-layer gate + B2F answerer is a separate
    /// concern tracked under the inbound-mail symmetry follow-up bd issue.
    pub fn send_listen_command(&self, enabled: bool) -> std::io::Result<()> {
        use std::io::Write;
        let mut inner = self.inner.lock().unwrap();
        if let Some(writer) = inner.abort_writer.as_mut() {
            let cmd: &[u8] = if enabled { b"LISTEN TRUE\r" } else { b"LISTEN FALSE\r" };
            writer.write_all(cmd)?;
            writer.flush()?;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "no cmd writer installed (modem not running)",
            ))
        }
    }

    /// Test-only helper: install ONLY the cooperative writer (no fallback
    /// stream). Used by tests that want to exercise the writer path
    /// without constructing a paired ShutdownableStream. Production code
    /// MUST use [`install_abort_writer`] so the hard-close fallback is
    /// available.
    #[cfg(test)]
    pub fn install_abort_writer_test_only(
        &self,
        writer: Box<dyn std::io::Write + Send>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.abort_writer = Some(writer);
        inner.abort_stream = None;
    }

    // ── Close-generation guards (tuxlink-pdnw — Codex Phase 3-4 P1 #1, #5) ──
    //
    // See the `close_generation` field docstring on `ModemSession` for the
    // race the generation guards. Summary: a worker takes the transport
    // (b2f exchange or listener consumer accept-loop) and then a close path
    // runs; the worker's return-to-session path re-installs the transport
    // into a session the operator just closed. The generation snapshot +
    // check prevents that — close bumps; worker's snapshot is stale;
    // install path drops the transport instead.

    /// Read the live close-generation counter. Workers that intend to
    /// re-install the transport snapshot this BEFORE the take.
    pub fn current_close_generation(&self) -> u64 {
        self.close_generation.load(Ordering::Acquire)
    }

    /// Bump the close-generation counter. Returns the PRIOR value (so the
    /// caller can log the transition if useful); the new generation is
    /// `prior + 1`. Called by every close path at the TOP, BEFORE
    /// `reset_to_stopped` / `clear_active_session_mode` / disarm. Workers
    /// already in flight after this point will observe the new generation
    /// and their install-back will be a drop, not an install.
    pub fn bump_close_generation(&self) -> u64 {
        self.close_generation.fetch_add(1, Ordering::AcqRel)
    }

    /// Guarded install: install the transport iff `snapshot_gen` still
    /// matches the live close-generation. Returns `Ok(())` when installed;
    /// returns `Err(transport)` with the (un-installed) transport handed
    /// back to the caller when a close intervened. The caller decides
    /// whether to drop the transport or log + drop — production callers
    /// drop unconditionally because the session is in a closed posture
    /// and a leaked transport would tie up modem state.
    ///
    /// **Why the `Result` instead of a silent drop?** Returning the
    /// transport lets the caller emit a diagnostic log line at the
    /// install site (where the right context is in scope), rather than
    /// the session doing it from inside a method without good context.
    /// The drop happens at the caller's discretion.
    pub fn install_transport_if_generation_matches(
        &self,
        t: Box<dyn crate::winlink::modem::ModemTransport>,
        snapshot_gen: u64,
    ) -> Result<(), Box<dyn crate::winlink::modem::ModemTransport>> {
        // Codex Phase 3-4 RE-REVIEW P1: the generation check MUST happen
        // INSIDE the mutex critical section, not outside. Without this,
        // a close path can bump close_generation + run reset_to_stopped
        // (which takes the mutex, clears + releases) between our load and
        // our lock — our stale worker then acquires the mutex and writes
        // the transport into a now-closed session. Re-reading the
        // generation while holding the mutex makes "close intervened"
        // atomic with "install the transport."
        //
        // Mutex-poisoned is treated as "session gone" — hand the
        // transport back. (Poisoning indicates a panic in a prior critical
        // section, same defensive posture as `transport_owner()`.)
        match self.inner.lock() {
            Ok(mut inner) => {
                let live = self.close_generation.load(Ordering::Acquire);
                if live != snapshot_gen {
                    return Err(t);
                }
                inner.transport = Some(t);
                Ok(())
            }
            Err(_poisoned) => Err(t),
        }
    }

    /// Try to begin a connect. Returns `true` if the caller now owns the busy
    /// bit; `false` if another connect is already in flight. Caller MUST call
    /// [`clear_connect_in_progress`] in every exit path (use the RAII guard in
    /// `modem_ardop_connect_gated_with_factory`).
    pub fn try_begin_connect(&self) -> bool {
        self.connect_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Release the busy bit. Must pair with a successful [`try_begin_connect`].
    pub fn clear_connect_in_progress(&self) {
        self.connect_in_progress.store(false, Ordering::Release);
    }
}

impl Default for ModemSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Poll interval for the status broadcaster — 4 Hz heartbeat from the Rust
/// side to the WebView. Hardcoded for v1; the cmd-socket polling work that
/// will replace the cached-snapshot rebroadcast (v0.3+) can revisit this.
pub const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Interval between live-VFO frequency reads (rig-control LIVE-VFO POLL). 2 s
/// keeps CAT serial chatter light while still feeling live in the readout.
pub const RIG_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Sleep-slice granularity inside the poll loop's inter-read wait. The loop
/// sleeps in slices (rather than one `RIG_POLL_INTERVAL` sleep) so the stop
/// flag is observed within ~`RIG_POLL_SLEEP_SLICE` of a disconnect instead of
/// after a full interval — bounding `stop_rig_poll`'s join latency.
pub const RIG_POLL_SLEEP_SLICE: Duration = Duration::from_millis(100);

/// Read timeout on the poll thread's independent rigctld client. Bounds a
/// single socket read so a hung rigctld cannot wedge the poll thread; on
/// timeout the read errors and the loop exits.
pub const RIG_POLL_READ_TIMEOUT: Duration = Duration::from_secs(1);

/// Tauri event name the broadcaster emits on. The frontend's `useModemStatus`
/// hook (Task 1.3) subscribes to this exact string — do not rename without
/// updating `src/hooks/useModemStatus.ts`.
pub const STATUS_EVENT: &str = "modem:status";

/// Background thread that polls [`ModemSession::tick_and_snapshot`] every
/// [`STATUS_POLL_INTERVAL`] and emits each snapshot via the provided
/// closure.
///
/// In production the closure is
/// `|s| { let _ = app_handle.emit(STATUS_EVENT, s); }` — fire-and-forget
/// against the WebView. `tick_and_snapshot` does double duty: it drains
/// any pending events from the installed [`crate::winlink::modem::ModemTransport`]
/// into the cached status before returning a clone, so the broadcaster
/// emits live state transitions / peer / bandwidth / ARQ flags / last error
/// at the 4 Hz tick rate (tuxlink-926y).
///
/// Zero-sized "namespace" type — no per-instance state, just `spawn` +
/// `tick_for_test`.
pub struct ModemStatusBroadcaster;

impl ModemStatusBroadcaster {
    /// Run the broadcaster on a dedicated thread named
    /// `modem-status-broadcaster` (so it's visible as such in `top` / `htop`
    /// / `gdb`). Returns the `JoinHandle<()>` — the caller is free to drop
    /// it; the thread runs for the lifetime of the process. No shutdown
    /// signal in v1 (the broadcaster owns no transport state so a clean
    /// shutdown costs more than it's worth; revisit if/when the broadcaster
    /// polls the cmd-socket directly).
    pub fn spawn<F>(session: Arc<ModemSession>, emit: F) -> std::thread::JoinHandle<()>
    where
        F: Fn(ModemStatus) + Send + 'static,
    {
        std::thread::Builder::new()
            .name("modem-status-broadcaster".into())
            .spawn(move || loop {
                let snap = session.tick_and_snapshot();
                emit(snap);
                std::thread::sleep(STATUS_POLL_INTERVAL);
            })
            .expect("failed to spawn modem status broadcaster")
    }

    /// Run a single tick — used by unit tests to avoid sleeping the test
    /// thread for 250 ms.
    #[cfg(test)]
    pub fn tick_for_test<F>(session: &Arc<ModemSession>, emit: &F) -> std::io::Result<()>
    where
        F: Fn(ModemStatus),
    {
        emit(session.tick_and_snapshot());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_serializes_to_documented_shape() {
        let s = ModemStatus::stopped();
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["state"], "stopped");
        assert_eq!(json["bytesRx"], 0);
        assert!(json["peer"].is_null());
        assert_eq!(json["arqFlags"]["busy"], false);
    }

    #[test]
    fn connected_irs_roundtrips() {
        let s = ModemStatus {
            state: ModemState::ConnectedIrs,
            peer: Some("W7RMS-10".into()),
            mode: Some("4FSK 500".into()),
            width_hz: Some(500),
            ptt_backend: Some("rts".into()),
            sn_db: Some(8.4),
            vu_dbfs: Some(-18.0),
            throughput_bps: Some(540),
            bytes_rx: 4128,
            bytes_tx: 982,
            uptime_sec: 222,
            arq_flags: ArqFlags { busy: true, rx: true, tx: false },
            last_error: None,
            quality: Some(72),
            listener_armed: false,
            exchange: None,
            transport_owner: TransportOwner::None,
            active_intent: None,
            active_transport_kind: None,
            rig_freq_hz: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: ModemStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        // confirm the wire form has camelCase + kebab-case for state
        assert!(json.contains("\"state\":\"connected-irs\""));
        assert!(json.contains("\"bytesRx\":4128"));
    }

    #[test]
    fn modem_session_has_no_consent_token_methods() {
        // SENTINEL: do NOT uncomment — these lines must NOT compile after
        // Task 1.4 lands.
        // let session = ModemSession::new();
        // let _ = session.mint_consent_token();
        // let _ = session.consume_consent_token("foo");
        // let _ = session.clear_consent_token();
        // let _ = session.has_valid_token("foo");
    }

    #[test]
    fn modem_session_starts_stopped() {
        let s = ModemSession::new();
        assert_eq!(s.status_snapshot().state, ModemState::Stopped);
    }

    // ── rig-control LIVE-VFO POLL ──────────────────────────────────────────

    #[test]
    fn rig_freq_hz_default_none_and_setter_round_trips() {
        let session = ModemSession::new();
        assert!(
            session.status_snapshot().rig_freq_hz.is_none(),
            "fresh session must have no live VFO reading"
        );
        session.set_rig_freq_hz(Some(7_102_000));
        assert_eq!(session.status_snapshot().rig_freq_hz, Some(7_102_000));
        session.set_rig_freq_hz(None);
        assert!(session.status_snapshot().rig_freq_hz.is_none());
    }

    #[test]
    fn reset_to_stopped_clears_rig_freq_hz() {
        let session = ModemSession::new();
        session.set_rig_freq_hz(Some(14_105_000));
        assert_eq!(session.status_snapshot().rig_freq_hz, Some(14_105_000));
        let _ = session.reset_to_stopped();
        assert!(
            session.status_snapshot().rig_freq_hz.is_none(),
            "reset_to_stopped must clear the live VFO reading"
        );
    }

    #[test]
    fn stop_rig_poll_is_a_no_op_when_no_poller_running() {
        // Idempotent + safe to call with no poller — exercised by every
        // disconnect on the close-serial path (where no poller was spawned).
        let session = ModemSession::new();
        session.stop_rig_poll();
        session.stop_rig_poll();
    }

    /// In-process fake rigctld: bind a TCP port and answer `f`/`m`/`t` with
    /// fixed values for as many clients as connect. Returns the bound port.
    /// Serves until the process exits (test-scoped daemon thread).
    fn fake_rigctld_for_poll(freq_line: &'static str) -> u16 {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut writer = stream.try_clone().unwrap();
                    let mut reader = BufReader::new(stream);
                    let mut line = String::new();
                    while reader.read_line(&mut line).unwrap_or(0) > 0 {
                        let reply = match line.trim_end().chars().next() {
                            Some('f') => format!("{freq_line}\n"),
                            Some('m') => "PKTUSB\n3000\n".to_string(),
                            Some('t') => "0\n".to_string(),
                            _ => "RPRT -1\n".to_string(),
                        };
                        if writer.write_all(reply.as_bytes()).is_err() {
                            break;
                        }
                        let _ = writer.flush();
                        line.clear();
                    }
                });
            }
        });
        port
    }

    #[test]
    fn start_rig_poll_reads_freq_then_stop_joins_cleanly() {
        let port = fake_rigctld_for_poll("7102000");
        let session = Arc::new(ModemSession::new());
        session.start_rig_poll("127.0.0.1".into(), port);

        // Wait (bounded) for the first reading to land. The poller reads
        // immediately on entry, so this should be quick.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if session.status_snapshot().rig_freq_hz == Some(7_102_000) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "poll thread never wrote the VFO reading"
            );
            std::thread::sleep(Duration::from_millis(25));
        }

        // stop_rig_poll must signal + join the thread without hanging.
        session.stop_rig_poll();
        // After join, no second poller leaks: handle slot is empty.
        assert!(
            session.rig_poll_handle.lock().unwrap().is_none(),
            "join must clear the handle slot"
        );
    }

    #[test]
    fn start_rig_poll_replaces_prior_poller_no_leak() {
        let port = fake_rigctld_for_poll("14105000");
        let session = Arc::new(ModemSession::new());
        session.start_rig_poll("127.0.0.1".into(), port);
        // Re-spawn: the second start must stop + join the first internally so
        // exactly one poller is live afterward.
        session.start_rig_poll("127.0.0.1".into(), port);
        // Confirm a reading still lands from the live (second) poller.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while session.status_snapshot().rig_freq_hz != Some(14_105_000) {
            assert!(
                std::time::Instant::now() < deadline,
                "second poller never produced a reading"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        session.stop_rig_poll();
    }

    #[test]
    fn start_rig_poll_exits_immediately_when_rigctld_unreachable() {
        // No listener bound on this port → connect_with_timeout fails → the
        // thread returns without ever writing a reading. stop must still join
        // cleanly (the thread already exited).
        let session = Arc::new(ModemSession::new());
        // Port 1 is privileged + unbound in test env → connect refused fast.
        session.start_rig_poll("127.0.0.1".into(), 1);
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            session.status_snapshot().rig_freq_hz.is_none(),
            "no reading when rigctld is unreachable"
        );
        session.stop_rig_poll();
    }

    #[test]
    fn broadcaster_emits_initial_stopped_snapshot() {
        use std::cell::RefCell;
        let session = Arc::new(ModemSession::new());
        let recorded: RefCell<Vec<ModemStatus>> = RefCell::new(Vec::new());
        let emit = |s: ModemStatus| recorded.borrow_mut().push(s);
        let one_tick = ModemStatusBroadcaster::tick_for_test(&session, &emit);
        assert!(one_tick.is_ok());
        let recorded = recorded.into_inner();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].state, ModemState::Stopped);
    }

    /// Stub transport whose `drain_status_events` mutates the snapshot — used
    /// to prove `tick_and_snapshot` routes through the installed transport.
    struct StubTransport;
    impl crate::winlink::modem::ModemTransport for StubTransport {
        fn init(
            &mut self,
            _: &crate::winlink::modem::InitConfig,
        ) -> Result<(), crate::winlink::modem::SessionError> {
            Ok(())
        }
        fn connect_arq(
            &mut self,
            _: &str,
            _: u32,
            _: Option<std::time::Duration>,
        ) -> Result<crate::winlink::modem::ConnectInfo, crate::winlink::modem::SessionError>
        {
            unimplemented!("stub")
        }
        fn disconnect(
            &mut self,
            _: std::time::Duration,
        ) -> Result<(), crate::winlink::modem::SessionError> {
            Ok(())
        }
        fn data_stream(
            &mut self,
        ) -> std::io::Result<&mut dyn crate::winlink::modem::ReadWrite> {
            Err(std::io::Error::other("stub"))
        }
        fn drain_status_events(&mut self, status: &mut ModemStatus) {
            status.peer = Some("STUB-DRAINED".into());
            status.width_hz = Some(1234);
        }
    }

    #[test]
    fn tick_and_snapshot_routes_through_installed_transport() {
        let session = ModemSession::new();
        session.install_transport(Box::new(StubTransport));
        let snap = session.tick_and_snapshot();
        assert_eq!(snap.peer.as_deref(), Some("STUB-DRAINED"));
        assert_eq!(snap.width_hz, Some(1234));
        // The drained-into snapshot must also persist on the session — a
        // subsequent `status_snapshot` should reflect the mutation.
        let cached = session.status_snapshot();
        assert_eq!(cached.peer.as_deref(), Some("STUB-DRAINED"));
        assert_eq!(cached.width_hz, Some(1234));
    }

    #[test]
    fn tick_and_snapshot_is_a_no_op_when_no_transport_installed() {
        let session = ModemSession::new();
        let snap = session.tick_and_snapshot();
        assert_eq!(snap.state, ModemState::Stopped);
        assert!(snap.peer.is_none());
    }

    // ── tuxlink-o3f2: abort_writer install / abort_in_flight / reset ──────

    /// Spawn a local TCP listener and return `(connected_writer, listener)`
    /// where `connected_writer` is the client end of a connected loopback
    /// pair. Used to feed `install_abort_writer` a real TCP stream whose
    /// bytes the test can read back from the listener side.
    fn loopback_writer_pair() -> (std::net::TcpStream, std::net::TcpStream) {
        use std::net::{TcpListener, TcpStream};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (server, _peer) = listener.accept().unwrap();
        // The "writer" is the client end (what ModemSession holds); the
        // "reader" is the server end (what the test asserts on).
        (client, server)
    }

    /// Pack the loopback client end into the new two-arg install form: a
    /// boxed writer plus a clone-as-ShutdownableStream for the hard-close
    /// fallback. Mirrors what production code does in
    /// `modem_commands.rs::install_abort_writer` after tuxlink-0ye6 Task 4.1.
    fn install_loopback_writer_pair(session: &ModemSession, client: std::net::TcpStream) {
        let stream_clone = client.try_clone().expect("clone for shutdown handle");
        session.install_abort_writer(
            Box::new(client) as Box<dyn std::io::Write + Send>,
            Box::new(stream_clone) as Box<dyn ShutdownableStream>,
        );
    }

    #[test]
    fn abort_writer_install_then_abort_writes_to_socket() {
        use std::io::Read;
        use std::time::Duration;
        let (writer, mut reader) = loopback_writer_pair();
        let session = ModemSession::new();

        install_loopback_writer_pair(&session, writer);

        session
            .abort_in_flight()
            .expect("abort_in_flight must succeed when writer is installed");

        // Read what arrived on the listener side — bound by a generous
        // timeout so a regression doesn't hang the suite.
        reader.set_read_timeout(Some(Duration::from_secs(2))).ok();
        let mut buf = [0u8; 16];
        let n = reader.read(&mut buf).expect("must read the side-channel bytes");
        assert_eq!(&buf[..n], b"ABORT\r", "must write exactly the ABORT host line");
    }

    #[test]
    fn abort_in_flight_with_no_writer_returns_err() {
        let session = ModemSession::new();
        let err = session
            .abort_in_flight()
            .expect_err("abort_in_flight must Err when no writer is installed");
        assert_eq!(err.kind(), std::io::ErrorKind::NotConnected);
    }

    #[test]
    fn reset_to_stopped_clears_abort_writer() {
        let (writer, _reader) = loopback_writer_pair();
        let session = ModemSession::new();
        install_loopback_writer_pair(&session, writer);

        // Sanity: writer is installed.
        // (Calling abort_in_flight here would also consume nothing — the
        // method writes through the still-installed handle, it doesn't
        // remove it. So we test the reset path directly.)

        // No transport installed → reset returns None but still clears
        // the writer.
        assert!(session.reset_to_stopped().is_none());

        let err = session
            .abort_in_flight()
            .expect_err("after reset, no writer must be installed");
        assert_eq!(err.kind(), std::io::ErrorKind::NotConnected);
    }

    // ── tuxlink-0ye6 Task 4.1 (Codex Round 4 P1 #3): hard-close fallback ──

    /// A writer that always returns WouldBlock — models a wedged peer that
    /// isn't draining its cmd socket past the bounded `write_timeout`.
    struct BlockedWriter;
    impl std::io::Write for BlockedWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "test: wedged peer",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Spy that records whether `shutdown_both` was called. The shared
    /// `Arc<Mutex<bool>>` lets the test assert from the outside after
    /// `abort_in_flight` returns.
    struct ShutdownSpy {
        called: Arc<Mutex<bool>>,
    }
    impl ShutdownableStream for ShutdownSpy {
        fn shutdown_both(&mut self) -> std::io::Result<()> {
            *self.called.lock().unwrap() = true;
            Ok(())
        }
    }

    #[test]
    fn ardop_abort_in_flight_falls_back_to_hard_close_when_write_fails() {
        let session = ModemSession::new();
        let shutdown_called = Arc::new(Mutex::new(false));
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
            "Codex Round 4 P1 #4: ARDOP abort must bound the write + hard-close on failure"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "Codex Round 3 P1 #1: abort budget must stay under 2s; got {:?}",
            elapsed
        );
    }

    // ── tuxlink-0ye6 Task 4.3: ARDOP arbiter (TransportOwner state machine) ─
    //
    // Mirrors the VARA-side arbiter tests in
    // src/winlink/modem/vara/commands.rs. Both transports must carry the
    // same arbiter discipline — Codex Round 2 P2 explicitly flagged the
    // VARA-only worker outcome as incomplete.
    //
    // Scope of this dispatch (per dune-bison-salamander task brief):
    //   - TransportOwner enum + transport_owner() accessor
    //   - take_transport_for_outbound() / return_transport_from_outbound()
    //   - bounded 3s yield timeout (Codex Round 3 P1 #2)
    //   - lock-drop-before-await (Codex Round 2 P1 #4)
    //   - listener-yield + transport-return channels
    //
    // OUT OF SCOPE here: integration with the actual b2f_exchange Tauri
    // command (deferred to Phase 3 follow-up).

    /// Reusable test ModemTransport that does nothing — sufficient to be
    /// the "transport" the arbiter passes back and forth. Distinct from
    /// the existing `StubTransport` in this module so the arbiter tests
    /// can opt in without dragging in the drain-events behavior.
    struct ArbiterTestTransport;
    impl crate::winlink::modem::ModemTransport for ArbiterTestTransport {
        fn init(
            &mut self,
            _: &crate::winlink::modem::InitConfig,
        ) -> Result<(), crate::winlink::modem::SessionError> {
            Ok(())
        }
        fn connect_arq(
            &mut self,
            _: &str,
            _: u32,
            _: Option<std::time::Duration>,
        ) -> Result<
            crate::winlink::modem::ConnectInfo,
            crate::winlink::modem::SessionError,
        > {
            unimplemented!("arbiter test transport")
        }
        fn disconnect(
            &mut self,
            _: std::time::Duration,
        ) -> Result<(), crate::winlink::modem::SessionError> {
            Ok(())
        }
        fn data_stream(
            &mut self,
        ) -> std::io::Result<&mut dyn crate::winlink::modem::ReadWrite> {
            Err(std::io::Error::other("arbiter test transport"))
        }
        fn drain_status_events(&mut self, _: &mut ModemStatus) {}
    }

    /// Helper: extract Err arm without requiring `T: Debug` (the trait
    /// object `Box<dyn ModemTransport>` does not implement Debug).
    fn unwrap_err_str<T>(r: Result<T, String>, ctx: &str) -> String {
        match r {
            Err(e) => e,
            Ok(_) => panic!("{ctx}: expected Err, got Ok"),
        }
    }

    #[test]
    fn ardop_transport_owner_starts_none() {
        let session = ModemSession::new();
        assert_eq!(session.transport_owner(), TransportOwner::None);
    }

    #[test]
    fn ardop_take_transport_transitions_owner_to_listener_armed() {
        // Simulates the listener consumer task taking the transport after
        // modem startup. The owner moves from None → ListenerArmed.
        let session = ModemSession::new();
        session.install_transport(Box::new(ArbiterTestTransport));
        assert_eq!(session.transport_owner(), TransportOwner::None);

        let taken = session.take_transport();
        assert!(taken.is_some(), "must take the installed transport");
        assert_eq!(session.transport_owner(), TransportOwner::ListenerArmed);
    }

    #[test]
    fn ardop_reset_to_stopped_clears_owner_to_none() {
        let session = ModemSession::new();
        session.install_transport(Box::new(ArbiterTestTransport));
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);

        let _ = session.reset_to_stopped();
        assert_eq!(session.transport_owner(), TransportOwner::None);
    }

    #[tokio::test]
    async fn ardop_take_transport_for_outbound_from_none_errs_session_not_open() {
        let session = ModemSession::new();
        let err = unwrap_err_str(
            session.take_transport_for_outbound().await,
            "None → Err",
        );
        assert!(
            err.contains("session not open"),
            "expected 'session not open', got: {err}"
        );
        assert_eq!(session.transport_owner(), TransportOwner::None);
    }

    #[tokio::test]
    async fn ardop_take_transport_for_outbound_from_listener_inbound_errs_modem_busy() {
        let session = ModemSession::new();
        session.set_transport_owner_for_test(TransportOwner::ListenerInbound);
        let err = unwrap_err_str(
            session.take_transport_for_outbound().await,
            "ListenerInbound → Err",
        );
        assert!(
            err.contains("modem busy") && err.contains("inbound"),
            "expected 'modem busy — inbound exchange in progress', got: {err}"
        );
        assert_eq!(
            session.transport_owner(),
            TransportOwner::ListenerInbound
        );
    }

    #[tokio::test]
    async fn ardop_take_transport_for_outbound_from_outbound_errs_already_in_flight() {
        let session = ModemSession::new();
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
    async fn ardop_take_transport_for_outbound_from_outbound_pending_also_errs() {
        let session = ModemSession::new();
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
    async fn ardop_take_transport_for_outbound_from_listener_armed_with_yield_succeeds() {
        let session = Arc::new(ModemSession::new());
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);

        let notify = session.transport_yield_notify_clone();
        let yield_tx = session.transport_yield_sender_clone();
        let consumer = tokio::spawn(async move {
            notify.notified().await;
            let _ = yield_tx
                .send(Box::new(ArbiterTestTransport)
                    as Box<dyn crate::winlink::modem::ModemTransport>)
                .await;
        });

        let _out = session
            .take_transport_for_outbound()
            .await
            .expect("yield-then-take must succeed");
        assert_eq!(session.transport_owner(), TransportOwner::Outbound);

        consumer.await.ok();
    }

    #[tokio::test]
    async fn ardop_take_transport_for_outbound_times_out_when_consumer_does_not_yield() {
        let session = ModemSession::new();
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
        assert_eq!(session.transport_owner(), TransportOwner::None);
    }

    #[tokio::test]
    async fn ardop_take_transport_for_outbound_errs_when_yield_channel_closed() {
        let session = ModemSession::new();
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);
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
    async fn ardop_return_transport_from_outbound_transitions_to_listener_armed_when_consumer_alive(
    ) {
        let session = Arc::new(ModemSession::new());
        session.set_transport_owner_for_test(TransportOwner::Outbound);

        let mut return_rx = session
            .take_transport_return_rx()
            .expect("first take must succeed");
        let session_for_task = session.clone();
        let consumer = tokio::spawn(async move {
            let received = return_rx.recv().await;
            let _ = received;
            let _ = session_for_task;
        });

        // Fresh session — close_generation is 0; snapshot matches live.
        session.return_transport_from_outbound(Box::new(ArbiterTestTransport), 0);

        assert_eq!(session.transport_owner(), TransportOwner::ListenerArmed);
        consumer.await.ok();
    }

    #[tokio::test]
    async fn ardop_return_transport_from_outbound_transitions_to_none_when_channel_closed() {
        let session = Arc::new(ModemSession::new());
        session.set_transport_owner_for_test(TransportOwner::Outbound);

        let rx = session
            .take_transport_return_rx()
            .expect("first take must succeed");
        drop(rx);

        // Fresh session — close_generation is 0; snapshot matches live.
        session.return_transport_from_outbound(Box::new(ArbiterTestTransport), 0);

        assert_eq!(session.transport_owner(), TransportOwner::None);
    }

    #[tokio::test]
    async fn ardop_take_transport_for_outbound_does_not_hold_lock_across_await() {
        // Codex Round 2 P1 #4: the std-mutex MUST be released before the
        // .await on the yield channel. Verification: spawn an outbound
        // that notifies + waits; while it awaits, call transport_owner()
        // (which takes the same std-mutex) and assert it returns
        // promptly.
        let session = Arc::new(ModemSession::new());
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);

        let session_for_outbound = session.clone();
        let outbound = tokio::spawn(async move {
            session_for_outbound.take_transport_for_outbound().await
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let probe_start = std::time::Instant::now();
        let owner = session.transport_owner();
        let probe_elapsed = probe_start.elapsed();

        assert!(
            probe_elapsed < Duration::from_millis(500),
            "Codex Round 2 P1 #4: transport_owner() blocked for {probe_elapsed:?} \
             — the std-mutex is being held across the .await in \
             take_transport_for_outbound. The lock MUST be dropped before await."
        );
        assert_eq!(
            owner,
            TransportOwner::OutboundPending,
            "owner should be OutboundPending while outbound is awaiting yield"
        );

        let _ = outbound.await;
    }

    // ── tuxlink-0ye6 Task 3.0 — DTO widening + ExchangeState + SocketLost ─
    //
    // Codex Round 2 P1 #5 + Round 3 P1 #3 + Round 3 P1 #4 + Round 4 P1 #1
    // + Round 4 P2.

    #[test]
    fn transport_owner_serializes_camel_case() {
        // Codex Round 4 P2: enum variants need their own rename_all derive
        // — the parent struct's `camelCase` only renames fields. Without
        // the derive on TransportOwner, the wire form would be PascalCase
        // ("ListenerArmed") while frontend expects camelCase.
        assert_eq!(
            serde_json::to_string(&TransportOwner::None).unwrap(),
            "\"none\""
        );
        assert_eq!(
            serde_json::to_string(&TransportOwner::ListenerArmed).unwrap(),
            "\"listenerArmed\""
        );
        assert_eq!(
            serde_json::to_string(&TransportOwner::ListenerInbound).unwrap(),
            "\"listenerInbound\""
        );
        assert_eq!(
            serde_json::to_string(&TransportOwner::OutboundPending).unwrap(),
            "\"outboundPending\""
        );
        assert_eq!(
            serde_json::to_string(&TransportOwner::Outbound).unwrap(),
            "\"outbound\""
        );
    }

    #[test]
    fn transport_owner_round_trips() {
        for owner in [
            TransportOwner::None,
            TransportOwner::ListenerArmed,
            TransportOwner::ListenerInbound,
            TransportOwner::OutboundPending,
            TransportOwner::Outbound,
        ] {
            let json = serde_json::to_string(&owner).unwrap();
            let back: TransportOwner = serde_json::from_str(&json).unwrap();
            assert_eq!(back, owner, "round-trip failed for {owner:?}");
        }
    }

    #[test]
    fn exchange_state_serializes_kebab_case() {
        // ExchangeState uses `rename_all = "kebab-case"` per the Round 2
        // plan; the wire form is `"dialing"` / `"outbound"` / `"inbound"`.
        assert_eq!(
            serde_json::to_string(&ExchangeState::Dialing).unwrap(),
            "\"dialing\""
        );
        assert_eq!(
            serde_json::to_string(&ExchangeState::Outbound).unwrap(),
            "\"outbound\""
        );
        assert_eq!(
            serde_json::to_string(&ExchangeState::Inbound).unwrap(),
            "\"inbound\""
        );
    }

    #[test]
    fn exchange_state_round_trips() {
        for state in [
            ExchangeState::Dialing,
            ExchangeState::Outbound,
            ExchangeState::Inbound,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let back: ExchangeState = serde_json::from_str(&json).unwrap();
            assert_eq!(back, state);
        }
    }

    #[test]
    fn modem_state_socket_lost_serializes() {
        // Spec §2.6 / §5: cmd-port unresponsive / ardopcf process exit
        // transitions to socket-lost; recovery is Close Session → reopen.
        // Wire form is kebab-case ("socket-lost") matching the existing
        // ModemState serde discipline.
        let json = serde_json::to_string(&ModemState::SocketLost).unwrap();
        assert_eq!(json, "\"socket-lost\"");
        let back: ModemState = serde_json::from_str("\"socket-lost\"").unwrap();
        assert_eq!(back, ModemState::SocketLost);
    }

    #[test]
    fn modem_status_dto_includes_lifecycle_fields() {
        // Compile-time check that the new fields exist on the DTO with
        // the expected types. A failure here means the DTO shape drifted
        // away from the spec (Codex Round 2 P1 #5 + Round 3 P1 #3).
        let s = ModemStatus::stopped();
        let _: bool = s.listener_armed;
        let _: Option<ExchangeState> = s.exchange;
        let _: TransportOwner = s.transport_owner;
        let _: Option<SessionIntent> = s.active_intent;
        let _: Option<TransportKind> = s.active_transport_kind;
    }

    #[test]
    fn modem_status_serializes_lifecycle_fields_camel_case() {
        // Round 4 P1 #1: the lifecycle fields serialize as camelCase on
        // the wire (per the parent struct's rename_all derive), and the
        // enum variant values use their respective per-enum rename_all
        // (camelCase for TransportOwner, kebab for SessionIntent /
        // TransportKind / ExchangeState).
        let s = ModemStatus {
            state: ModemState::ConnectedIrs,
            peer: None,
            mode: None,
            width_hz: None,
            ptt_backend: None,
            sn_db: None,
            vu_dbfs: None,
            throughput_bps: None,
            bytes_rx: 0,
            bytes_tx: 0,
            uptime_sec: 0,
            arq_flags: ArqFlags { busy: false, rx: false, tx: false },
            last_error: None,
            quality: None,
            listener_armed: true,
            exchange: Some(ExchangeState::Outbound),
            transport_owner: TransportOwner::ListenerInbound,
            active_intent: Some(SessionIntent::P2p),
            active_transport_kind: Some(TransportKind::Ardop),
            rig_freq_hz: Some(7_102_000),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains("\"rigFreqHz\":7102000"),
            "field `rig_freq_hz` must serialize as `rigFreqHz`; got {json}"
        );
        assert!(
            json.contains("\"listenerArmed\":true"),
            "field `listener_armed` must serialize as `listenerArmed`; got {json}"
        );
        assert!(
            json.contains("\"exchange\":\"outbound\""),
            "ExchangeState::Outbound serializes kebab-case; got {json}"
        );
        assert!(
            json.contains("\"transportOwner\":\"listenerInbound\""),
            "TransportOwner::ListenerInbound serializes camelCase; got {json}"
        );
        assert!(
            json.contains("\"activeIntent\":\"p2p\""),
            "SessionIntent::P2p serializes kebab-case; got {json}"
        );
        assert!(
            json.contains("\"activeTransportKind\":\"ardop\""),
            "TransportKind::Ardop serializes kebab-case; got {json}"
        );
        // Round-trip end-to-end.
        let back: ModemStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn modem_session_stub_accessors_return_defaults() {
        // `listener_armed` + `current_exchange` are still stubs (Phase 3.4 /
        // 3.5 outbound wires them later). `active_intent` /
        // `active_transport_kind` were wired by Task 3.5 — a fresh session
        // with no open returns `None` for both, which matches the previous
        // stub contract by coincidence. This test asserts the
        // "no-session-open" baseline; the post-open behavior is covered by
        // the Task 3.5 tests in `modem_commands.rs`.
        let session = ModemSession::new();
        assert!(!session.listener_armed(), "stub returns false");
        assert!(session.current_exchange().is_none(), "stub returns None");
        assert!(
            session.active_intent().is_none(),
            "no-open baseline returns None"
        );
        assert!(
            session.active_transport_kind().is_none(),
            "no-open baseline returns None"
        );
    }

    // ── tuxlink-0ye6 Task 3.5: ModemSession active-session-mode wiring ──
    //
    // The stub accessors are now wired through `ModemSessionInner`. These
    // tests cover the storage layer directly (set / clear / round-trip);
    // the open/close commands that use them ship in `modem_commands.rs`.

    #[test]
    fn modem_session_set_active_session_mode_round_trips_via_accessors() {
        let session = ModemSession::new();
        assert!(session.active_intent().is_none());
        assert!(session.active_transport_kind().is_none());

        let prior = session.set_active_session_mode(SessionIntent::P2p, TransportKind::Ardop);
        assert!(prior.is_none(), "no prior recorded on a fresh session");

        assert_eq!(session.active_intent(), Some(SessionIntent::P2p));
        assert_eq!(
            session.active_transport_kind(),
            Some(TransportKind::Ardop)
        );
    }

    #[test]
    fn modem_session_clear_active_session_mode_resets_accessors() {
        let session = ModemSession::new();
        session.set_active_session_mode(SessionIntent::Cms, TransportKind::Ardop);
        assert_eq!(session.active_intent(), Some(SessionIntent::Cms));

        session.clear_active_session_mode();

        assert!(session.active_intent().is_none());
        assert!(session.active_transport_kind().is_none());
    }

    #[test]
    fn modem_session_reset_to_stopped_also_clears_active_session_mode() {
        // The destructive reset path (e.g. disconnect-on-error) must clear
        // the session mode along with the transport — a stale
        // (intent, transport_kind) on a closed session would lie to the
        // frontend's sidebar-nav guard.
        let session = ModemSession::new();
        session.set_active_session_mode(SessionIntent::RadioOnly, TransportKind::Ardop);
        assert_eq!(
            session.active_intent(),
            Some(SessionIntent::RadioOnly)
        );

        let _ = session.reset_to_stopped();

        assert!(
            session.active_intent().is_none(),
            "reset_to_stopped must clear active_intent"
        );
        assert!(
            session.active_transport_kind().is_none(),
            "reset_to_stopped must clear active_transport_kind"
        );
    }

    #[test]
    fn modem_session_status_snapshot_reflects_active_session_mode() {
        // status_snapshot() overlays the active-session-mode fields the same
        // way it overlays transport_owner — so a snapshot taken after
        // set_active_session_mode reports the recorded values without
        // requiring a full tick cycle.
        let session = ModemSession::new();
        session.set_active_session_mode(SessionIntent::P2p, TransportKind::Ardop);

        let snap = session.status_snapshot();
        assert_eq!(snap.active_intent, Some(SessionIntent::P2p));
        assert_eq!(snap.active_transport_kind, Some(TransportKind::Ardop));
    }

    #[test]
    fn modem_session_status_snapshot_overlays_transport_owner() {
        // `status_snapshot()` overlays the live `transport_owner` from
        // the session inner-mutex on top of the cached `inner.status`,
        // so a transport_owner change is reflected without a full
        // tick + drain cycle.
        let session = ModemSession::new();
        assert_eq!(
            session.status_snapshot().transport_owner,
            TransportOwner::None
        );
        session.set_transport_owner_for_test(TransportOwner::ListenerArmed);
        assert_eq!(
            session.status_snapshot().transport_owner,
            TransportOwner::ListenerArmed
        );
    }

    // ── tuxlink-pdnw — close-generation guards (Codex Phase 3-4 P1 #1, #5) ──
    //
    // The close-generation counter prevents the close-vs-armed-consumer
    // race: a worker (b2f exchange or listener consumer) takes the
    // transport, then a close path runs; the worker's install-back path
    // would otherwise restore the transport into a session the operator
    // just closed. Workers snapshot the generation before taking the
    // transport; the guarded install-back path checks the snapshot is
    // still current — if a close intervened, the transport is dropped
    // instead of installed.

    #[test]
    fn ardop_close_generation_starts_at_zero() {
        let session = ModemSession::new();
        assert_eq!(
            session.current_close_generation(),
            0,
            "fresh session must start at close_generation = 0"
        );
    }

    #[test]
    fn ardop_bump_close_generation_increments_monotonically() {
        let session = ModemSession::new();
        let prior_a = session.bump_close_generation();
        assert_eq!(prior_a, 0, "first bump must report prior gen = 0");
        assert_eq!(session.current_close_generation(), 1);

        let prior_b = session.bump_close_generation();
        assert_eq!(prior_b, 1, "second bump must report prior gen = 1");
        assert_eq!(session.current_close_generation(), 2);

        let prior_c = session.bump_close_generation();
        assert_eq!(prior_c, 2);
        assert_eq!(
            session.current_close_generation(),
            3,
            "generation must grow monotonically across bumps"
        );
    }

    #[test]
    fn ardop_install_transport_if_generation_matches_installs_when_snapshot_current() {
        // Happy path: snapshot taken at gen=0; no close intervened;
        // install must succeed and the session must now have the
        // transport.
        let session = ModemSession::new();
        let snapshot = session.current_close_generation();

        let result = session
            .install_transport_if_generation_matches(Box::new(ArbiterTestTransport), snapshot);

        assert!(result.is_ok(), "matching generation must install");
        assert!(
            session.snapshot_transport_present(),
            "session must have the transport after a successful install"
        );
    }

    #[test]
    fn ardop_install_transport_if_generation_matches_drops_when_close_intervened() {
        // Race path: snapshot taken at gen=0; a close path bumps the
        // generation; install must Err(transport) instead of installing.
        // The returned transport is the one we handed in (caller drops).
        let session = ModemSession::new();
        let snapshot = session.current_close_generation();
        assert_eq!(snapshot, 0);

        // Simulate close: bump the generation.
        let _ = session.bump_close_generation();
        assert_eq!(session.current_close_generation(), 1);

        let result = session
            .install_transport_if_generation_matches(Box::new(ArbiterTestTransport), snapshot);

        assert!(
            result.is_err(),
            "stale snapshot must Err — close intervened"
        );
        // The transport is handed back via Err so the caller can drop /
        // log; verify it did NOT get installed into the session.
        assert!(
            !session.snapshot_transport_present(),
            "session must NOT have a transport after a stale-gen install attempt"
        );
        // Drop the returned transport explicitly (mirrors production
        // caller posture).
        drop(result.err().unwrap());
    }

    #[tokio::test]
    async fn ardop_return_transport_from_outbound_drops_when_close_intervened() {
        // Race path on the arbiter return contract: outbound captured the
        // generation, then a close path bumped it; return_transport_from_outbound
        // must drop the transport instead of pushing it back onto the
        // return channel. The owner must end up `None`.
        let session = Arc::new(ModemSession::new());
        session.set_transport_owner_for_test(TransportOwner::Outbound);

        // Snapshot at gen=0, then simulate close intervening.
        let snapshot = session.current_close_generation();
        assert_eq!(snapshot, 0);
        let _ = session.bump_close_generation();
        assert_eq!(session.current_close_generation(), 1);

        // Set up a return-channel receiver that we'll prove was NOT used.
        let mut return_rx = session
            .take_transport_return_rx()
            .expect("first take must succeed");

        session.return_transport_from_outbound(Box::new(ArbiterTestTransport), snapshot);

        // Owner must be cleared to None (close intervened, session is gone).
        assert_eq!(session.transport_owner(), TransportOwner::None);

        // The return channel must NOT have received the transport — the
        // stale-gen path drops the transport instead of pushing it. The
        // recv with try_recv expects Empty. (Can't `{other:?}` because
        // `Box<dyn ModemTransport>` doesn't implement Debug; we cover the
        // outcomes by-hand.)
        match return_rx.try_recv() {
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                panic!("return channel disconnected — stale-gen path should not close the channel")
            }
            Ok(_t) => {
                panic!("return channel must be empty after stale-gen drop; got a transport")
            }
        }
    }
}
