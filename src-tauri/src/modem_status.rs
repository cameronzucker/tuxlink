use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
}

struct ModemSessionInner {
    status: ModemStatus,
    consent_token: Option<String>,
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
    /// Cloneable cmd-socket writer (the transport's side-channel abort
    /// handle). Installed via [`ModemSession::install_abort_writer`] BEFORE
    /// `connect_arq` begins blocking, and consumed by
    /// [`ModemSession::abort_in_flight`] to send `ABORT\r` to ardopcf while
    /// the connect path is stuck in its recv loop (tuxlink-o3f2 — P1
    /// abort-during-connect fix).
    ///
    /// Cleared by [`ModemSession::reset_to_stopped`] so a fresh connect
    /// installs a fresh writer.
    abort_writer: Option<std::net::TcpStream>,
}

// Manual `Debug` impl: `Box<dyn ModemTransport>` does not implement `Debug`,
// so `#[derive(Debug)]` would fail. Print the non-transport fields verbatim
// and a placeholder for the transport handle. The consent token is redacted
// even in Debug — it's not a secret, but no value to log a live one.
impl std::fmt::Debug for ModemSessionInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModemSessionInner")
            .field("status", &self.status)
            .field(
                "consent_token",
                &self.consent_token.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "transport",
                &self
                    .transport
                    .as_ref()
                    .map(|_| "Some(<dyn ModemTransport>)"),
            )
            .field(
                "abort_writer",
                &self.abort_writer.as_ref().map(|_| "Some(<TcpStream>)"),
            )
            .finish()
    }
}

impl ModemSession {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ModemSessionInner {
                status: ModemStatus::stopped(),
                consent_token: None,
                transport: None,
                abort_writer: None,
            }),
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

    /// Generate + remember a new consent token. Returns the token so the
    /// frontend can pass it to `modem_ardop_connect`.
    pub fn mint_consent_token(&self) -> String {
        // 16 random hex chars — enough for in-process uniqueness; not a secret.
        let token: String = (0..16)
            .map(|_| {
                let n: u8 = rand::random::<u8>() & 0xF;
                std::char::from_digit(n as u32, 16).unwrap()
            })
            .collect();
        self.inner.lock().unwrap().consent_token = Some(token.clone());
        token
    }

    /// WARNING: non-destructive equality check; does NOT consume the token.
    /// Reserved for tests and disconnect-path verification. The per-invocation
    /// consent path (RADIO-1) MUST use [`consume_consent_token`] so a single
    /// minted token cannot authorize more than one on-air connect.
    pub fn has_valid_token(&self, candidate: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.consent_token.as_deref() == Some(candidate)
    }

    /// Atomically verify a candidate token matches the stored token AND clear
    /// it in the same lock acquisition. Returns true iff the candidate matched
    /// (and the stored token is now `None`). Returns false if there was no
    /// stored token, or if the candidate didn't match.
    ///
    /// This is the per-invocation consent path: every successful call consumes
    /// the token, so the operator must mint a fresh one (via the RADIO-1
    /// modal) before the next on-air connect. Closes the replay window the
    /// 2026-05-30 Codex adrev round flagged on the non-destructive
    /// `has_valid_token` check.
    pub fn consume_consent_token(&self, candidate: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let matches = inner.consent_token.as_deref() == Some(candidate);
        if matches {
            inner.consent_token = None;
        }
        matches
    }

    pub fn clear_consent_token(&self) {
        self.inner.lock().unwrap().consent_token = None;
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

    /// Atomically take the transport handle, clear the consent token, and
    /// reset the status to `Stopped`. Returns the prior transport (if any)
    /// so the caller can call `transport.disconnect(...) + drop` OUTSIDE
    /// the lock — never call I/O while holding the session mutex.
    ///
    /// Single lock acquisition: observers see a consistent
    /// `(token=None, status=Stopped, transport=None, abort_writer=None)`
    /// state. Closes the inconsistent-intermediate window the Task 3.2
    /// code-quality review flagged on `modem_ardop_disconnect_inner` (the
    /// prior split between `clear_consent_token()` + `set_status(Stopped)`
    /// widened once Task 3.3 stretched the disconnect path across
    /// `transport.disconnect()` I/O + SIGINT).
    ///
    /// tuxlink-o3f2: also clears `abort_writer`, since the underlying
    /// TCP write half is owned by the transport's cmd socket and will
    /// close when the transport is dropped by the caller. A stale writer
    /// pointing at a dead socket is a footgun for the next connect.
    pub fn reset_to_stopped(&self) -> Option<Box<dyn crate::winlink::modem::ModemTransport>> {
        let mut inner = self.inner.lock().unwrap();
        inner.consent_token = None;
        inner.status = ModemStatus::stopped();
        inner.abort_writer = None;
        inner.transport.take()
    }

    /// Install the side-channel cmd-socket writer used by
    /// [`abort_in_flight`]. Called from `modem_ardop_connect_post_consume_with_factory`
    /// AFTER `init` opens the cmd socket but BEFORE `connect_arq` begins
    /// blocking on its recv loop — that ordering is the whole point of the
    /// abort-during-connect fix (tuxlink-o3f2).
    ///
    /// Replaces any previously-installed writer.
    pub fn install_abort_writer(&self, writer: std::net::TcpStream) {
        self.inner.lock().unwrap().abort_writer = Some(writer);
    }

    /// Best-effort send of `ABORT\r` to ardopcf via the side-channel
    /// writer installed by [`install_abort_writer`]. Returns `Ok(())` iff a
    /// writer was installed and the write succeeded; returns
    /// `Err(NotConnected)` when no writer is installed (caller can fall
    /// through to the graceful `take_transport`/`disconnect` path).
    ///
    /// tuxlink-o3f2: this is the P1 abort-during-connect fix. While
    /// `arq_connect` is blocking on the cmd-socket recv channel, the
    /// transport is held as a stack local in the connect call's frame —
    /// `take_transport` would observe `None`. Sending `ABORT` via the
    /// side-channel writer causes ardopcf to halt the in-flight TX and
    /// emit `FAULT` (or `NEWSTATE DISC`), which the cmd reader thread
    /// delivers via the existing channel; `arq_connect`'s recv loop then
    /// returns `Err(SessionError::Fault(...))` and the connect path
    /// unwinds cleanly.
    pub fn abort_in_flight(&self) -> std::io::Result<()> {
        use std::io::Write;
        let mut inner = self.inner.lock().unwrap();
        if let Some(writer) = inner.abort_writer.as_mut() {
            writer.write_all(b"ABORT\r")?;
            writer.flush()?;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "no abort writer installed",
            ))
        }
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
    fn modem_session_starts_stopped_with_no_token() {
        let s = ModemSession::new();
        assert_eq!(s.status_snapshot().state, ModemState::Stopped);
        assert!(!s.has_valid_token("any-token"));
    }

    #[test]
    fn modem_session_accepts_minted_token_and_invalidates_on_clear() {
        let s = ModemSession::new();
        let t = s.mint_consent_token();
        assert!(s.has_valid_token(&t));
        s.clear_consent_token();
        assert!(!s.has_valid_token(&t));
    }

    #[test]
    fn consume_consent_token_returns_true_and_clears_on_match() {
        let s = ModemSession::new();
        let t = s.mint_consent_token();
        // First call: matches and consumes.
        assert!(s.consume_consent_token(&t));
        // After consumption the token is gone — a replay must fail.
        assert!(!s.has_valid_token(&t));
        assert!(!s.consume_consent_token(&t));
    }

    #[test]
    fn consume_consent_token_returns_false_on_mismatch() {
        let s = ModemSession::new();
        let _t = s.mint_consent_token();
        // Wrong candidate must NOT consume the stored token.
        assert!(!s.consume_consent_token("wrong-token"));
        // The minted token is still valid because the failed consume did not
        // clear it. (Equality check failed, so no clear.)
        assert!(s.has_valid_token(&_t));
    }

    #[test]
    fn consume_consent_token_returns_false_when_no_token_stored() {
        let s = ModemSession::new();
        // No mint at all → consume must return false (and not panic).
        assert!(!s.consume_consent_token("any-candidate"));
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

    #[test]
    fn abort_writer_install_then_abort_writes_to_socket() {
        use std::io::Read;
        use std::time::Duration;
        let (writer, mut reader) = loopback_writer_pair();
        let session = ModemSession::new();

        session.install_abort_writer(writer);

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
        session.install_abort_writer(writer);

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
}
