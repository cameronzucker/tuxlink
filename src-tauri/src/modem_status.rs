use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
        }
    }
}

/// Shared per-app modem session state.
///
/// Wraps the current `ModemStatus` snapshot + the in-process RADIO-1 consent
/// token + the live `ModemTransport` handle (when a connect has succeeded).
/// `Arc<ModemSession>` is stored in Tauri state and shared between command
/// handlers and the broadcaster.
#[derive(Debug)]
pub struct ModemSession {
    inner: Mutex<ModemSessionInner>,
    /// Busy guard: set to `true` while a connect is in flight.
    /// Guards against duplicate concurrent connect invocations (the dup-call
    /// defense previously provided as a side-effect by the consent token's
    /// consume semantics). Set via [`try_begin_connect`] BEFORE any I/O;
    /// cleared via [`clear_connect_in_progress`] on every exit path via RAII.
    connect_in_progress: AtomicBool,
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
    /// future modems (Dire Wolf, tuxmodem, etc.) can swap in without
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
            .finish()
    }
}

impl ModemSession {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ModemSessionInner {
                status: ModemStatus::stopped(),
                transport: None,
                abort_writer: None,
                abort_stream: None,
            }),
            connect_in_progress: AtomicBool::new(false),
        }
    }

    pub fn status_snapshot(&self) -> ModemStatus {
        self.inner.lock().unwrap().status.clone()
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
        let mut inner = self.inner.lock().unwrap();
        // Clone the snapshot before mutating so that a panic inside
        // `drain_status_events` leaves the persisted status untouched
        // (poison-aware: the next acquirer will see the pre-drain state).
        let mut snap = inner.status.clone();
        if let Some(transport) = inner.transport.as_mut() {
            transport.drain_status_events(&mut snap);
        }
        inner.status = snap.clone();
        snap
    }

    /// Install a live `ModemTransport` handle in the session. Called from
    /// `modem_ardop_connect_post_consume_with_factory` after a successful
    /// `init` + `connect_arq`.
    pub fn install_transport(&self, t: Box<dyn crate::winlink::modem::ModemTransport>) {
        self.inner.lock().unwrap().transport = Some(t);
    }

    /// Take ownership of the live transport handle, if any. The caller is
    /// responsible for calling `disconnect()` + dropping it. Intended for
    /// flows that want to shut down the transport WITHOUT also resetting
    /// session status (rare). Most disconnect paths should use
    /// [`reset_to_stopped`].
    pub fn take_transport(&self) -> Option<Box<dyn crate::winlink::modem::ModemTransport>> {
        self.inner.lock().unwrap().transport.take()
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
    pub fn reset_to_stopped(&self) -> Option<Box<dyn crate::winlink::modem::ModemTransport>> {
        let mut inner = self.inner.lock().unwrap();
        inner.status = ModemStatus::stopped();
        inner.abort_writer = None;
        inner.abort_stream = None;
        inner.transport.take()
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
            _: std::time::Duration,
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
}
