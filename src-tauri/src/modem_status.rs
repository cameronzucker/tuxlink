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
            }),
        }
    }

    pub fn status_snapshot(&self) -> ModemStatus {
        self.inner.lock().unwrap().status.clone()
    }

    pub fn set_status(&self, s: ModemStatus) {
        self.inner.lock().unwrap().status = s;
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

    pub fn has_valid_token(&self, candidate: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.consent_token.as_deref() == Some(candidate)
    }

    pub fn clear_consent_token(&self) {
        self.inner.lock().unwrap().consent_token = None;
    }

    /// Install a live `ModemTransport` handle in the session. Called from
    /// `modem_ardop_connect_inner` after a successful `init` + `connect_arq`.
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
    /// `(token=None, status=Stopped, transport=None)` state. Closes the
    /// inconsistent-intermediate window the Task 3.2 code-quality review
    /// flagged on `modem_ardop_disconnect_inner` (the prior split between
    /// `clear_consent_token()` + `set_status(Stopped)` widened once Task 3.3
    /// stretched the disconnect path across `transport.disconnect()` I/O +
    /// SIGINT).
    pub fn reset_to_stopped(&self) -> Option<Box<dyn crate::winlink::modem::ModemTransport>> {
        let mut inner = self.inner.lock().unwrap();
        inner.consent_token = None;
        inner.status = ModemStatus::stopped();
        inner.transport.take()
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

/// Background thread that polls `ModemSession::status_snapshot()` every
/// `STATUS_POLL_INTERVAL` and emits each snapshot via the provided closure.
///
/// In production the closure is
/// `|s| { let _ = app_handle.emit(STATUS_EVENT, s); }` — fire-and-forget
/// against the WebView. For v1 the broadcaster just rebroadcasts the cached
/// snapshot; richer flows (poll the ardopcf cmd-socket for live S/N,
/// throughput, ARQ flags) are filed as follow-ups.
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
                let snap = session.status_snapshot();
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
        emit(session.status_snapshot());
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
}
