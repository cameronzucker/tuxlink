//! Backend-abstraction trait for tuxlink's Winlink interactions.
//!
//! Spec: docs/superpowers/specs/2026-05-18-winlink-backend-trait-design.md
//! bd issue: tuxlink-z5f
//!
//! This module defines the `WinlinkBackend` trait — the architectural
//! boundary that decouples tuxlink's UI/config layer from any one Winlink
//! protocol implementation. Two implementations live here:
//!
//! - [`PatBackend`] — wraps the existing [`crate::pat_client::PatClient`]
//!   plus [`crate::pat_process::PatProcess`]. v0.0.1 ships only this one.
//! - [`NativeBackend`] — stub returning [`BackendError::NotImplemented`]
//!   for every method. Real native logic arrives in v0.5 Steps 3–10.
//!
//! Per [feedback_discipline_triage_rule]: the trait is the hard-to-undo
//! architectural decision; once defined, implementations are TDD plumbing.

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use thiserror::Error;

// Re-export MailboxFolder so the trait surface doesn't reach into the
// Pat-specific module.
pub use crate::pat_client::MailboxFolder;

// Native backend wiring (see the NativeBackend section below).
use crate::config::{broadcast_grid, CmsTransport, Config};
use crate::native_mailbox::Mailbox;
use crate::winlink::message::Message;
use crate::winlink::proposal::Answer;
use crate::winlink::{compose, session, telnet};
use std::path::PathBuf;

// ============================================================================
// Supporting types (spec §3.2)
// ============================================================================

/// Newtype around the Winlink Message ID (MID) string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new(s: impl Into<String>) -> Self {
        MessageId(s.into())
    }
}

/// Light header-only view returned by `list_messages`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MessageMeta {
    pub id: MessageId,
    pub subject: String,
    pub from: String,
    /// Recipient list. Drives the list "To" column (esp. Sent/Outbox).
    /// Added by Task 12 (tuxlink-zsm). Pat 1.0.0's `/api/mailbox` list DTO
    /// does NOT expose a To array (verified against the shipped fixture in
    /// `pat_client_test.rs`), so `PatBackend::list_messages` degrades this to
    /// an empty vec — see `pat_client::Message::to` and spec §2.1 graceful
    /// degradation. The field stays on the trait surface so a future Pat
    /// (or NativeBackend) that DOES provide recipients can populate it.
    pub to: Vec<String>,
    /// RFC 3339 UTC timestamp. Backend emits canonical form.
    pub date: String,
    pub unread: bool,
    pub body_size: u64,
    /// Attachment-presence indicator for the list `#` column. Added by Task
    /// 12 (tuxlink-zsm). Pat 1.0.0's list DTO does not expose attachment
    /// metadata, so `PatBackend::list_messages` degrades this to `false`
    /// (spec §2.1). The full attachment list is materialized at read time
    /// (Task 13's RFC5322 parse), not in the list view.
    pub has_attachments: bool,
}

/// Full body returned by `read_message`. Byte fidelity per spec §3.2 v2
/// P0 #2 — Winlink B2F messages can carry binary MIME parts; UTF-8
/// conversion happens at the display boundary (Tauri command), not here.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MessageBody {
    pub id: MessageId,
    pub raw_rfc5322: Vec<u8>,
}

/// Outbound message — what `send_message` consumes. Intentionally NOT
/// `#[non_exhaustive]` (per spec §3.2) to keep caller-construction
/// ergonomic. Adding fields is an acknowledged breaking change.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// RFC 3339 UTC timestamp. Caller provides; backend validates.
    pub date: String,
}

/// Transport selector for `connect`. `#[non_exhaustive]` so v0.5+ can add
/// Packet/Pactor/VARA HF/VARA FM/AX.25/KISS variants without breaking
/// existing match arms.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TransportConfig {
    /// CMS Telnet (plain or TLS), per existing `config::CmsTransport`.
    Cms { mode: crate::config::CmsTransport },
}

/// Backend-instance identifier minted at backend construction time. Embedded
/// in every `Session` so `disconnect` can validate the session came from
/// this backend instance (v2 P0 #1). Process-local `AtomicU64` counter; no
/// UUID dep needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendInstanceId(pub(crate) u64);

static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(1);

impl BackendInstanceId {
    pub(crate) fn next() -> Self {
        BackendInstanceId(NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Opaque session handle. Carries the backend-instance id so cross-backend
/// `disconnect` calls return `BackendError::InvalidSession`. See spec
/// §3.5 for Drop semantics rationale.
#[derive(Debug)]
pub struct Session {
    pub(crate) backend_id: BackendInstanceId,
    /// Backend-specific session payload. v0.0.1 PatBackend `connect` mints
    /// a stub variant (no HTTP call yet — full Pat /api/connect integration
    /// is deferred to v0.5 Step 5); the field is held for future-use match
    /// arms in `disconnect` to call out to Pat or native cleanup.
    #[allow(dead_code)]
    pub(crate) inner: SessionInner,
}

#[derive(Debug)]
#[allow(dead_code)] // pat_session_id will be read in v0.5 Step 5 PatBackend disconnect
pub(crate) enum SessionInner {
    Pat { pat_session_id: String },
    /// NativeBackend stub never produces sessions. Variant kept for future
    /// v0.5+ NativeBackend session shapes.
    Native(()),
}

impl Drop for Session {
    fn drop(&mut self) {
        // Local cleanup only — see spec §3.5. No remote-disconnect call;
        // explicit WinlinkBackend::disconnect is the guaranteed release path.
        // PatBackend sessions auto-time-out server-side; future native
        // sessions will close their socket fd via Drop on the inner stream.
    }
}

/// Backend connection status. Implementations cache + update internally;
/// `status()` reads the cache (MUST NOT do I/O).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BackendStatus {
    Disconnected,
    Connecting { transport: String },
    Connected { transport: String, peer: String, since_iso: String },
    Disconnecting,
    Error { reason: String },
}

/// Backend log line emitted via `stream_log()`.
#[derive(Debug, Clone)]
pub struct LogLine {
    /// Monotonic sequence number assigned by `SessionLogState::append`.
    /// 0 means "not yet assigned" (pre-append). The bridge writes to the
    /// `SessionLogState` buffer first; `seq` is set by `append`, never
    /// by the bridge or callers directly.
    pub seq: u64,
    pub timestamp_iso: String,
    pub level: LogLevel,
    pub source: LogSource,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogLevel { Trace, Debug, Info, Warn, Error }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogSource { Backend, Pat, Transport, Wire }

// ============================================================================
// Error model (spec §3.3)
// ============================================================================

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BackendError {
    #[error("backend not configured: {0}")]
    NotConfigured(String),

    #[error("message not found: {0:?}")]
    NotFound(MessageId),

    #[error("authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("transport failed: {reason}")]
    TransportFailed {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    #[error("backend rejected message: {0}")]
    MessageRejected(String),

    #[error("backend unavailable: {reason}")]
    BackendUnavailable {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    #[error("session does not belong to this backend instance")]
    InvalidSession,

    #[error("operation cancelled")]
    Cancelled,

    #[error("not implemented (this backend does not support this operation)")]
    NotImplemented,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal error: {msg}")]
    Internal {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },
}

// ============================================================================
// Trait surface (spec §3.1)
// ============================================================================

/// Backend abstraction for Winlink interactions. See spec §3.1 for the
/// full contract; key invariants:
///
/// - `Send + Sync` — implementors MUST NOT hold a `std::sync::MutexGuard`
///   across an `.await`; use `tokio::sync::Mutex` or contain blocking work
///   in `tokio::task::spawn_blocking`.
/// - `status()` is non-async — implementations cache the value internally
///   and update during connect/disconnect/op flows. MUST NOT do I/O.
/// - `stream_log()` returns `BoxStream<'static, LogLine>` whose Drop
///   cancels the subscription.
#[async_trait]
pub trait WinlinkBackend: Send + Sync {
    async fn list_messages(&self, folder: MailboxFolder)
        -> Result<Vec<MessageMeta>, BackendError>;

    /// Read a message from a specific folder. Added by Task 12
    /// (tuxlink-zsm): reading a Sent/Outbox message requires the folder,
    /// not just the MID — the prior `read_message` hardcoded Inbox
    /// (winlink_backend.rs, pre-zsm). `read_message` now delegates here
    /// with `MailboxFolder::Inbox` for back-compat. Implementors override
    /// this; `read_message` has a provided default that forwards.
    async fn read_message_in(&self, folder: MailboxFolder, id: &MessageId)
        -> Result<MessageBody, BackendError>;

    /// Back-compat shim: read from the Inbox folder. Prefer
    /// [`WinlinkBackend::read_message_in`] when the folder is known
    /// (spec §2.1). Provided default forwards to `read_message_in(Inbox, id)`.
    async fn read_message(&self, id: &MessageId)
        -> Result<MessageBody, BackendError> {
        self.read_message_in(MailboxFolder::Inbox, id).await
    }

    /// Mark a message read. Best-effort: backends with no read-state (e.g.
    /// `PatBackend`, which delegates read-state to Pat's own store) inherit
    /// this no-op default. `NativeBackend` overrides it to drop a read-marker
    /// in its store. A failure here MUST NOT fail the read that triggered it —
    /// the caller (`message_read`) treats read-state as best-effort
    /// (tuxlink-xgn).
    async fn mark_read(&self, _folder: MailboxFolder, _id: &MessageId)
        -> Result<(), BackendError> {
        Ok(())
    }

    /// Returns `Ok(Some(id))` when the backend assigns a MID at queue
    /// time, `Ok(None)` when it does not (current Pat 1.0.0 behavior:
    /// returns a plain-text confirmation, no MID).
    async fn send_message(&self, msg: OutboundMessage)
        -> Result<Option<MessageId>, BackendError>;

    async fn connect(&self, transport: TransportConfig)
        -> Result<Session, BackendError>;

    async fn disconnect(&self, session: Session)
        -> Result<(), BackendError>;

    /// Abort an in-flight [`WinlinkBackend::connect`] (tuxlink-9z2): shut down the
    /// connecting socket to unblock a slow TLS/login/exchange phase and return the
    /// backend to `Disconnected`. The aborted `connect` resolves to
    /// [`BackendError::Cancelled`]. Default is a no-op `Ok` for backends with no
    /// in-flight socket to cancel (e.g. `PatBackend`). Safe to call when idle.
    async fn abort(&self) -> Result<(), BackendError> {
        Ok(())
    }

    fn status(&self) -> BackendStatus;

    fn stream_log(&self) -> BoxStream<'static, LogLine>;
}

// ============================================================================
// NativeBackend stub (spec §3.9)
// ============================================================================

/// A sink for per-step connect progress messages (tuxlink-gqo). The connect path
/// runs in `spawn_blocking`, so the sink must be `Send + Sync`; production wires
/// it (in `bootstrap::install_native`) to append a `LogSource::Transport` line to
/// the session log and emit it live. Decoupled from the `LogLine` machinery on
/// purpose — `winlink::telnet` only ever calls it with a `&str` phase message.
pub type ProgressSink = Arc<dyn Fn(&str) + Send + Sync>;

/// The native Winlink backend: speaks B2F directly (no Pat), stores messages in
/// its own [`Mailbox`], and connects over plaintext or TLS telnet. `connect`
/// runs the real CMS exchange on a blocking task; the actual on-air protocol is
/// validated by `src/bin/native_cms_probe.rs` and the `winlink::*` tests.
pub struct NativeBackend {
    backend_id: BackendInstanceId,
    config: Config,
    mailbox: Arc<Mailbox>,
    log_tx: broadcast::Sender<LogLine>,
    status: Arc<RwLock<BackendStatus>>,
    progress: ProgressSink,
    /// Shutdown handle for the in-flight connect socket (tuxlink-9z2): a clone of
    /// the connecting `TcpStream`, set once TCP connects, taken + shut down by
    /// [`WinlinkBackend::abort`] to unblock a slow TLS/login/exchange phase.
    abort_handle: Arc<Mutex<Option<TcpStream>>>,
    /// Set by `abort` so the connect's resulting error maps to `Cancelled` (status
    /// `Disconnected`) rather than `Error`.
    aborting: Arc<AtomicBool>,
    /// Single-flight guard (Codex #1): true while a `connect` is running. A second
    /// concurrent `connect` is rejected rather than racing on the shared abort
    /// state and re-sending the outbox. Cleared by a connect-scoped RAII guard so
    /// it is released on every exit (return, `?`, panic).
    connect_in_progress: Arc<AtomicBool>,
    /// Live position source-of-truth (tuxlink-686). When present, the on-air
    /// locator is `arbiter.broadcast_grid()` — live + precision-reduced —
    /// superseding the stale `config` snapshot's grid. `None` in tests / the
    /// no-arbiter path, where `cms_locator(config)` is the fallback.
    position: Option<Arc<crate::position::PositionArbiter>>,
}

/// Clears the single-flight + abort state when a `connect` ends, however it ends
/// (Codex #1 + #7): normal return, early `?`, or a panic in the blocking task.
struct ConnectGuard {
    in_progress: Arc<AtomicBool>,
    handle: Arc<Mutex<Option<TcpStream>>>,
}

impl Drop for ConnectGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.handle.lock() {
            *slot = None;
        }
        self.in_progress.store(false, Ordering::SeqCst);
    }
}

impl NativeBackend {
    /// Create a backend for `config`, storing messages under `mailbox_root`, with
    /// a no-op progress sink. Production uses [`NativeBackend::with_progress`] to
    /// surface connect progress in the session log; tests use this no-op form.
    pub fn new(config: Config, mailbox_root: impl Into<PathBuf>) -> Self {
        Self::with_progress(config, mailbox_root, Arc::new(|_: &str| {}))
    }

    /// Like [`NativeBackend::new`] but with a connect-progress sink (tuxlink-gqo).
    pub fn with_progress(
        config: Config,
        mailbox_root: impl Into<PathBuf>,
        progress: ProgressSink,
    ) -> Self {
        let (log_tx, _rx) = broadcast::channel(256);
        Self {
            backend_id: BackendInstanceId::next(),
            config,
            mailbox: Arc::new(Mailbox::new(mailbox_root)),
            log_tx,
            status: Arc::new(RwLock::new(BackendStatus::Disconnected)),
            progress,
            abort_handle: Arc::new(Mutex::new(None)),
            aborting: Arc::new(AtomicBool::new(false)),
            connect_in_progress: Arc::new(AtomicBool::new(false)),
            position: None,
        }
    }

    /// Attach the live position arbiter (tuxlink-686). Builder-style so existing
    /// constructors and tests are unaffected.
    pub fn with_position(mut self, arbiter: Arc<crate::position::PositionArbiter>) -> Self {
        self.position = Some(arbiter);
        self
    }

    fn set_status(&self, status: BackendStatus) {
        if let Ok(mut s) = self.status.write() {
            *s = status;
        }
    }
}

#[async_trait]
impl WinlinkBackend for NativeBackend {
    async fn list_messages(&self, folder: MailboxFolder) -> Result<Vec<MessageMeta>, BackendError> {
        self.mailbox.list(folder)
    }

    async fn read_message_in(
        &self,
        folder: MailboxFolder,
        id: &MessageId,
    ) -> Result<MessageBody, BackendError> {
        self.mailbox.read(folder, id)
    }

    async fn mark_read(&self, folder: MailboxFolder, id: &MessageId) -> Result<(), BackendError> {
        self.mailbox.mark_read(folder, id)
    }

    async fn send_message(
        &self,
        msg: OutboundMessage,
    ) -> Result<Option<MessageId>, BackendError> {
        let callsign = self
            .config
            .identity
            .callsign
            .clone()
            .ok_or_else(|| BackendError::NotConfigured("identity.callsign".into()))?;
        // The trait carries an RFC 3339 date; fall back to now if unparseable.
        let unix_secs = parse_rfc3339_secs(&msg.date).unwrap_or_else(now_unix_secs);
        let to: Vec<&str> = msg.to.iter().map(String::as_str).collect();
        let cc: Vec<&str> = msg.cc.iter().map(String::as_str).collect();
        let message =
            compose::compose_message(&callsign, &to, &cc, &msg.subject, &msg.body, unix_secs);
        let id = self.mailbox.store(MailboxFolder::Outbox, &message.to_bytes())?;
        Ok(Some(id))
    }

    async fn connect(&self, transport: TransportConfig) -> Result<Session, BackendError> {
        let TransportConfig::Cms { mode } = transport;

        // Single-flight (Codex #1): refuse a concurrent connect rather than racing
        // on the shared abort state and re-sending the outbox. The RAII guard
        // releases the flag + clears the abort handle on EVERY exit — normal
        // return, early `?`, or a panic in the blocking task (Codex #7).
        if self
            .connect_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackendError::BackendUnavailable {
                reason: "a CMS connection is already in progress".to_string(),
                source: None,
            });
        }
        let _guard = ConnectGuard {
            in_progress: self.connect_in_progress.clone(),
            handle: self.abort_handle.clone(),
        };

        let config = self.config.clone();
        let mailbox = self.mailbox.clone();

        // Fresh abort epoch: clear any stale flag/handle from a prior connect so
        // an earlier abort can't bleed into this one (tuxlink-9z2).
        self.aborting.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.abort_handle.lock() {
            *slot = None;
        }

        self.set_status(BackendStatus::Connecting {
            transport: format!("{mode:?}"),
        });

        // The exchange is blocking (sockets + files); run it off the async runtime.
        // `progress` surfaces per-step connect progress in the session log
        // (tuxlink-gqo); `abort_handle` receives the connecting socket so abort can
        // shut it down (tuxlink-9z2). Both are Arcs cloned into the blocking task.
        // `position` is the live arbiter clone (tuxlink-686): when present,
        // `native_connect` uses the arbiter's `broadcast_grid()` as the on-air
        // locator, superseding the stale `config` snapshot's grid.
        let progress = self.progress.clone();
        let abort_handle = self.abort_handle.clone();
        let aborting = self.aborting.clone();
        let position = self.position.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            native_connect(&config, &mailbox, mode, &*progress, &abort_handle, &aborting, position.as_deref())
        })
        .await
        .map_err(|e| BackendError::Internal {
            msg: format!("native connect task failed: {e}"),
            source: None,
        })?;

        // An error after an operator abort is a cancellation, not a failure. The
        // `_guard` clears the abort handle + single-flight flag when this fn returns.
        match abort_aware_outcome(outcome, self.aborting.load(Ordering::SeqCst)) {
            Ok(()) => {
                self.set_status(BackendStatus::Connected {
                    transport: format!("{mode:?}"),
                    peer: CMS_HOST.to_string(),
                    since_iso: now_iso8601_utc(),
                });
                Ok(Session {
                    backend_id: self.backend_id,
                    inner: SessionInner::Native(()),
                })
            }
            Err(BackendError::Cancelled) => {
                self.set_status(BackendStatus::Disconnected);
                Err(BackendError::Cancelled)
            }
            Err(e) => {
                self.set_status(BackendStatus::Error {
                    reason: e.to_string(),
                });
                Err(e)
            }
        }
    }

    async fn disconnect(&self, session: Session) -> Result<(), BackendError> {
        if session.backend_id != self.backend_id {
            return Err(BackendError::InvalidSession);
        }
        self.set_status(BackendStatus::Disconnected);
        Ok(())
    }

    async fn abort(&self) -> Result<(), BackendError> {
        // Mark the abort (so the in-flight connect's error maps to Cancelled), shut
        // down the connecting socket to unblock a slow TLS/login/exchange phase, and
        // return to Disconnected. A no-op if nothing is in flight (handle is None).
        self.aborting.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.abort_handle.lock() {
            if let Some(sock) = slot.take() {
                let _ = sock.shutdown(Shutdown::Both);
            }
        }
        self.set_status(BackendStatus::Disconnected);
        Ok(())
    }

    fn status(&self) -> BackendStatus {
        self.status
            .read()
            .map(|s| s.clone())
            .unwrap_or(BackendStatus::Error {
                reason: "status RwLock poisoned".to_string(),
            })
    }

    fn stream_log(&self) -> BoxStream<'static, LogLine> {
        let rx = self.log_tx.subscribe();
        BroadcastStream::new(rx)
            .filter_map(|res| async move { res.ok() })
            .boxed()
    }
}

/// The Winlink CMS host. **DEV DEFAULT = `cms-z.winlink.org`** (2026-05-21,
/// operator directive): production (`server.winlink.org`) rejects unregistered
/// client SIDs, and tuxlink is not yet registered — so cms-z (which accepts the
/// unregistered client) is the only working target. `TUXLINK_CMS_HOST` still
/// overrides this.
/// TODO(register): revert the default to `server.winlink.org` once tuxlink's
/// client name is registered with Winlink (the production blocker).
const CMS_HOST: &str = "cms-z.winlink.org";

/// Resolve the CMS `(port, transport)` from the configured `mode` plus optional
/// dev overrides (tuxlink-gqo). `TUXLINK_CMS_PLAINTEXT` forces plaintext telnet —
/// the dev escape hatch for hosts that expose no TLS (the dev default cms-z has no
/// 8773 TLS listener, while production `server.winlink.org` does); `TUXLINK_CMS_PORT`
/// overrides the port. With no overrides the configured transport stands, so the
/// persisted/production CmsSsl default keeps its 8773 TLS endpoint. Mirrors the
/// `bin/native_cms_probe` env contract so the app and the probe agree.
fn resolve_cms_endpoint(
    mode: CmsTransport,
    plaintext_override: bool,
    port_override: Option<u16>,
) -> (u16, telnet::Transport) {
    let transport = if plaintext_override {
        telnet::Transport::Plaintext
    } else {
        match mode {
            CmsTransport::CmsSsl => telnet::Transport::Tls,
            CmsTransport::Telnet => telnet::Transport::Plaintext,
        }
    };
    let default_port = match transport {
        telnet::Transport::Tls => 8773,
        telnet::Transport::Plaintext => 8772,
    };
    (port_override.unwrap_or(default_port), transport)
}

/// Map a raw connect outcome to the caller-facing result (tuxlink-9z2): an error
/// that follows an operator abort becomes `Cancelled`; a success stands (the
/// connect completed before the abort landed); a non-aborted error stands.
fn abort_aware_outcome(
    outcome: Result<(), BackendError>,
    aborted: bool,
) -> Result<(), BackendError> {
    match outcome {
        Err(_) if aborted => Err(BackendError::Cancelled),
        other => other,
    }
}

/// The grid locator advertised in the CMS handshake, reduced to the configured
/// broadcast precision (tuxlink-882). Empty when no grid is set. This is the single
/// on-air position surface today; it MUST go through `broadcast_grid` so a stored
/// 6-char grid never leaks past a 4-char privacy setting.
fn cms_locator(config: &Config) -> String {
    config
        .identity
        .grid
        .as_deref()
        .map(|g| broadcast_grid(g, config.privacy.position_precision))
        .unwrap_or_default()
}

/// The on-air locator: the live arbiter's broadcast grid when present
/// (authoritative — reflects runtime Manual/GPS changes the backend's `config`
/// snapshot does not), else the (startup) config grid reduced to precision.
/// `broadcast_grid()` already applies `position_precision`, so the Some case is
/// reduced. Empty string only when the arbiter has NO position at all (no manual
/// grid AND no usable fix); a GPS source with a stale fix still falls back to the
/// manual grid (see `PositionArbiter::active_grid`). The arbiter is authoritative —
/// we do NOT fall back to a possibly-stale config grid when the arbiter is present.
fn resolve_locator(config: &Config, position: Option<&crate::position::PositionArbiter>) -> String {
    match position {
        Some(a) => a.broadcast_grid().unwrap_or_default(),
        None => cms_locator(config),
    }
}

/// Run one CMS exchange (blocking): build the outbox into proposals, connect over
/// the chosen transport, accept all offered messages, then file what arrived into
/// the inbox and move what was sent into the sent folder.
fn native_connect(
    config: &Config,
    mailbox: &Mailbox,
    mode: CmsTransport,
    progress: &dyn Fn(&str),
    abort_handle: &Mutex<Option<TcpStream>>,
    aborting: &AtomicBool,
    position: Option<&crate::position::PositionArbiter>,
) -> Result<(), BackendError> {
    let callsign = config
        .identity
        .callsign
        .clone()
        .ok_or_else(|| BackendError::NotConfigured("identity.callsign".into()))?
        .trim()
        .to_uppercase();
    // tuxlink-686: resolve the on-air locator from the live arbiter when present
    // (authoritative — reflects runtime Manual/GPS changes the backend's `config`
    // snapshot does not), else fall back to the startup config grid (tuxlink-882
    // precision reduction applies in both paths).
    let locator = resolve_locator(config, position);
    let password = keyring::Entry::new("tuxlink-pat", &callsign)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|p| !p.is_empty());

    // Dev overrides (tuxlink-gqo) mirror `bin/native_cms_probe`: TUXLINK_CMS_PLAINTEXT
    // forces plaintext (cms-z exposes no 8773 TLS), TUXLINK_CMS_PORT overrides the
    // port. Absent both, the configured transport stands (production = CmsSsl/8773).
    let plaintext_override = std::env::var("TUXLINK_CMS_PLAINTEXT").is_ok();
    let port_override = std::env::var("TUXLINK_CMS_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok());
    let (port, transport) = resolve_cms_endpoint(mode, plaintext_override, port_override);

    // Turn each queued outbox message into a proposal + compressed body.
    let mut outbound = Vec::new();
    for meta in mailbox.list(MailboxFolder::Outbox)? {
        let body = mailbox.read(MailboxFolder::Outbox, &meta.id)?;
        if let Ok(message) = Message::from_bytes(&body.raw_rfc5322) {
            if let Some((proposal, compressed)) = message.to_proposal() {
                let title = message.header("Subject").unwrap_or_default().to_string();
                outbound.push(session::OutboundMessage {
                    proposal,
                    title,
                    compressed,
                });
            }
        }
    }

    let exchange_config = session::ExchangeConfig {
        mycall: callsign,
        targetcall: telnet::CMS_TARGET_CALL.to_string(),
        locator,
        password,
    };

    // The CMS host defaults to production; `TUXLINK_CMS_HOST` overrides it for
    // dev (e.g. `cms-z.winlink.org`, which accepts the unregistered client).
    let host = std::env::var("TUXLINK_CMS_HOST").unwrap_or_else(|_| CMS_HOST.to_string());

    // Hand each freshly-connected socket to the abort handle (tuxlink-9z2) so an
    // operator abort can `.shutdown()` it. A clone failure just leaves abort a
    // no-op for this attempt — connect proceeds normally. If an abort already
    // landed during the (un-abortable) TCP-connect window, shut this socket down
    // immediately so the connect fails fast instead of running to completion in
    // the background.
    let register_socket = |sock: &TcpStream| {
        if let Ok(clone) = sock.try_clone() {
            // Check `aborting` INSIDE the abort_handle lock (Codex #2): abort() sets
            // `aborting` then locks to take the socket, so doing the check + store
            // under the same lock means whichever side acquires it first, the socket
            // still ends up shut down if an abort has fired — no TOCTOU window.
            if let Ok(mut slot) = abort_handle.lock() {
                if aborting.load(Ordering::SeqCst) {
                    let _ = clone.shutdown(Shutdown::Both);
                } else {
                    *slot = Some(clone);
                }
            }
        }
    };

    let result = telnet::connect_and_exchange(
        &host,
        port,
        transport,
        &exchange_config,
        outbound,
        progress,
        &register_socket,
        |proposals| {
            proposals
                .iter()
                .map(|_| Answer::Accept { resume_offset: 0 })
                .collect()
        },
    )
    .map_err(|e| BackendError::TransportFailed {
        reason: format!("{e:?}"),
        source: None,
    })?;

    // File received messages into the inbox; move delivered ones to sent.
    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &MessageId(mid.clone()))?;
    }
    Ok(())
}

/// Seconds since the Unix epoch, now.
fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parse an RFC 3339 timestamp to seconds since the epoch.
fn parse_rfc3339_secs(s: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp().max(0) as u64)
}

// ============================================================================
// PatBackend (spec §3.8)
// ============================================================================

use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::pat_client::{PatClient, PatClientError};
use crate::session_log::SessionLogState;

/// Format the current wall-clock instant as an RFC 3339 / ISO-8601 UTC string
/// (`YYYY-MM-DDTHH:MM:SSZ`). Minimal epoch-based formatter — no `chrono`
/// dependency in this module. Mirrors the manual formatter in `ui_commands.rs`
/// (`format_unix_ts`) and `wizard.rs`; precision is whole seconds, which is all
/// the v0.0.1 session-log ingestion timestamp needs (spec §3.2: ingestion time,
/// not Pat's own parsed timestamp).
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

/// Convert days since 1970-01-01 to (year, month, day) on the proleptic
/// Gregorian calendar (Howard Hinnant's `civil_from_days`). Same algorithm as
/// `ui_commands::days_to_ymd`; duplicated locally to keep the two modules'
/// timestamp helpers self-contained (each is a few lines; a shared util module
/// is out of scope for v0.0.1).
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

/// Map one raw Pat stderr line into a [`LogLine`], append it to the durable
/// `SessionLogState` ring buffer (which assigns the monotonic `seq`), and
/// broadcast it live on `log_tx`. Returns the `LogLine` actually stored +
/// broadcast (with `seq` populated by the buffer).
///
/// Effects are exactly the append + the broadcast send. The broadcast `send`
/// failing (0 receivers) is fine — durability is provided by the buffer, the
/// broadcast is live-notify only (spec §3.2, adrev #2). Unit-tested directly
/// in `winlink_backend_test.rs` (`ingest_pat_line_appends_and_broadcasts_with_seq`).
///
/// `#[doc(hidden) pub]` (mirrors `push_log_line_for_test`) so the integration
/// test crate can drive it without a real Pat process.
#[doc(hidden)]
pub fn ingest_pat_line(
    raw: String,
    buffer: &SessionLogState,
    log_tx: &broadcast::Sender<LogLine>,
) -> LogLine {
    // Durable append (assigns + sets the monotonic seq, stores the line).
    let line = append_pat_line(raw, buffer);
    // Live notify; 0 receivers is fine (durability is in `buffer`).
    let _ = log_tx.send(line.clone());
    line
}

/// Append-only counterpart to [`ingest_pat_line`]: map one raw Pat stderr line
/// into a `LogSource::Pat` [`LogLine`] and append it to the durable buffer
/// (which assigns + sets the monotonic `seq`), WITHOUT broadcasting. Returns the
/// stored line (with `seq` populated).
///
/// Used by the `PatBackend::spawn` failure path (tuxlink-22l Codex R2 FIX 2):
/// when `PatProcess::spawn` returns `Err`, Pat's pre-exit stderr diagnostics may
/// still be sitting in the mpsc receiver, but the live broadcast has no
/// subscribers yet (and the backend is never constructed), so a broadcast is
/// pointless there — durability into the buffer is what carries the lines to the
/// UI (via the snapshot + the buffer-polling drain).
fn append_pat_line(raw: String, buffer: &SessionLogState) -> LogLine {
    let mut line = LogLine {
        seq: 0,
        timestamp_iso: now_iso8601_utc(),
        level: LogLevel::Info,
        source: LogSource::Pat,
        message: raw,
    };
    // `append` assigns + returns the monotonic seq and stores the line.
    line.seq = buffer.append(line.clone());
    line
}

/// Options for [`PatBackend::spawn`] — the full-lifecycle constructor.
/// (spec §3.1). The resolved Pat sidecar path + the Pat config/mbox/pid paths
/// + tuxlink's config are supplied by the bootstrap (`lib.rs` `.setup()`); see
/// spec §3.5/§3.6 for path-resolution responsibilities (the bootstrap owns
/// resolution, not `spawn`).
pub struct PatBackendSpawnOptions {
    /// Resolved Pat sidecar binary path.
    pub binary: std::path::PathBuf,
    /// Where `PatProcess` renders Pat's config.json before exec.
    pub config_path: std::path::PathBuf,
    /// Pat mailbox dir.
    pub mbox_dir: std::path::PathBuf,
    /// Pat pid file.
    pub pid_file: std::path::PathBuf,
    /// Tuxlink config — `PatProcess` renders Pat's config from it.
    pub tuxlink_config: crate::config::Config,
}

/// Wraps the existing [`PatClient`] (HTTP) and forwards Pat's stderr log
/// stream into a tokio broadcast channel for `stream_log` subscribers.
///
/// Two constructors:
/// - [`PatBackend::from_url`] — for tests + situations where Pat is
///   already running (or being mocked). No PatProcess managed.
/// - [`PatBackend::spawn`] — full lifecycle: spawn Pat, attach a
///   PatClient, multiplex stderr to subscribers, register backend-id.
pub struct PatBackend {
    backend_id: BackendInstanceId,
    client: PatClient,
    log_tx: broadcast::Sender<LogLine>,
    status: Arc<RwLock<BackendStatus>>,
    /// The supervised Pat child process. `Some` for [`PatBackend::spawn`]
    /// (the backend owns Pat's lifetime); `None` for [`PatBackend::from_url`]
    /// (Pat is managed externally / mocked). `Drop` gracefully shuts down a
    /// `Some` process; a `None` Drop is a no-op (spec §3.1 step 5, adrev #16).
    /// Wrapped in `Option` so `Drop` can `take()` it (needs `&mut`).
    _process: Option<crate::pat_process::PatProcess>,
    /// Join handle for the stderr→LogLine bridge thread (`spawn` only; `None`
    /// for `from_url`). Stored so the thread is not silently orphaned (adrev
    /// #17). The thread exits when Pat's stderr closes → `PatProcess`'s reader
    /// thread drops the mpsc sender → the bridge's `rx.recv()` returns `Err`.
    /// `Drop` joins it with a bounded wait.
    _bridge_thread: Option<JoinHandle<()>>,
}

impl PatBackend {
    /// Construct a PatBackend that talks to a Pat HTTP server already
    /// reachable at `base_url`. Used for tests (against mockito) and for
    /// scenarios where Pat is managed externally. No log forwarding from
    /// a spawned Pat process — `stream_log` only emits if callers
    /// directly push to the internal broadcast channel via test helpers.
    pub fn from_url(base_url: impl Into<String>) -> Self {
        let (log_tx, _rx) = broadcast::channel(256);
        Self {
            backend_id: BackendInstanceId::next(),
            client: PatClient::new(base_url),
            log_tx,
            status: Arc::new(RwLock::new(BackendStatus::Disconnected)),
            // No spawned process / bridge — Pat is external (spec §3.1).
            _process: None,
            _bridge_thread: None,
        }
    }

    /// Full-lifecycle constructor (spec §3.1, §11.2): spawn Pat in HTTP mode,
    /// wire a `PatClient` to the announced port, bridge Pat's stderr into BOTH
    /// the durable `log_buffer` (so `session_log_snapshot` sees startup lines)
    /// AND the live broadcast, hold + supervise the process, and report a
    /// truthful initial status of [`BackendStatus::Disconnected`].
    ///
    /// **Initial status is `Disconnected`, NOT `Connected`** (adrev #10): Pat's
    /// local HTTP server being up is NOT a CMS link. A real `Connected` is only
    /// minted by an operator-triggered `connect()`.
    ///
    /// **Part 97 (spec §6):** the spawned argv is `pat … http --addr 127.0.0.1:<port>`
    /// — HTTP mode only, loopback only; this constructor never calls Pat's
    /// connect/send APIs. Serving the local HTTP API is not a CMS session and
    /// not a transmission.
    ///
    /// Errors: any `PatProcess::spawn` `io::Error` (binary missing, announce
    /// timeout, config render failure) maps to
    /// [`BackendError::BackendUnavailable`] with the source chain preserved.
    pub fn spawn(
        opts: PatBackendSpawnOptions,
        log_buffer: Arc<SessionLogState>,
    ) -> Result<Self, BackendError> {
        // Channel for Pat stderr lines: PatProcess's reader thread is the
        // sender; our bridge thread is the receiver.
        let (tx, rx) = std::sync::mpsc::channel::<String>();

        // Spawn Pat in HTTP mode on an ephemeral loopback port (http_listen_port
        // = 0). `log_sink: Some(tx)` makes PatProcess forward EVERY stderr line
        // (incl. pre-announce) into our channel (spec §3.1 step 2).
        let process = match crate::pat_process::PatProcess::spawn(
            crate::pat_process::PatSpawnOptions {
                binary: opts.binary,
                config_path: opts.config_path,
                mbox_dir: opts.mbox_dir,
                http_listen_port: 0,
                pid_file: opts.pid_file,
                log_sink: Some(tx),
                tuxlink_config: opts.tuxlink_config,
                http_announce_timeout: Duration::from_secs(10),
            },
        ) {
            Ok(p) => p,
            Err(e) => {
                // FIX 2 (tuxlink-22l Codex R2): Pat may have printed its failure
                // cause to stderr before exiting; PatProcess's reader thread
                // forwarded those lines into `rx` before EOF. The bridge thread
                // is NOT started on this path, so without draining, those
                // diagnostics are lost. Drain `rx` into the durable buffer so
                // Pat's failure cause reaches the UI (via the snapshot + the
                // buffer-polling drain). `PatProcess::spawn`'s error paths all
                // kill + reap the child first, so the reader thread has hit (or
                // is about to hit) EOF and will drop its sender — `recv_timeout`
                // therefore terminates promptly (the timeout is a defensive
                // bound against a pathological reader, not the expected path).
                while let Ok(raw) = rx.recv_timeout(Duration::from_secs(1)) {
                    append_pat_line(raw, &log_buffer);
                }
                return Err(BackendError::BackendUnavailable {
                    reason: format!("Pat failed to start: {e}"),
                    source: Some(Box::new(e)),
                });
            }
        };

        // Wire the HTTP client to the port Pat actually bound (loopback).
        let port = process.http_port();
        let client = PatClient::new(format!("http://127.0.0.1:{port}"));

        // Live-notify broadcast channel, same shape as `from_url` (cap 256).
        let (log_tx, _rx) = broadcast::channel::<LogLine>(256);

        // Bridge thread: drain Pat stderr lines from the mpsc receiver, append
        // each to the durable buffer (assigns seq) AND broadcast it live
        // (spec §3.2). A blocking `std::thread` — `mpsc::Receiver::recv` is
        // blocking, consistent with PatProcess's reader. Exits when the mpsc
        // sender closes (Pat exits → PatProcess reader drops its sender).
        let bridge_buffer = log_buffer.clone();
        let bridge_log_tx = log_tx.clone();
        let bridge_thread = std::thread::spawn(move || {
            while let Ok(raw) = rx.recv() {
                ingest_pat_line(raw, &bridge_buffer, &bridge_log_tx);
            }
        });

        Ok(Self {
            backend_id: BackendInstanceId::next(),
            client,
            log_tx,
            // Initial status is Disconnected ("backend ready"), NOT Connected
            // — Pat's HTTP server is up but no CMS link exists (adrev #10).
            status: Arc::new(RwLock::new(BackendStatus::Disconnected)),
            _process: Some(process),
            _bridge_thread: Some(bridge_thread),
        })
    }

    /// Test-only: push a synthetic log line into the broadcast channel
    /// for verification of `stream_log()` semantics. Returns the number
    /// of receivers that got the message (0 if no active subscribers).
    #[doc(hidden)]
    pub fn push_log_line_for_test(&self, line: LogLine) -> usize {
        self.log_tx.send(line).unwrap_or(0)
    }
}

impl Drop for PatBackend {
    /// Graceful teardown of a spawned Pat (spec §3.1 step 5, adrev #16,#17).
    ///
    /// For a `spawn`ed backend (`_process: Some`): call
    /// `PatProcess::shutdown(5s)` — a SIGTERM→reap→SIGKILL-on-timeout cycle
    /// (vs `PatProcess`'s own `Drop`, which is an immediate SIGKILL). Killing
    /// Pat closes its stderr → `PatProcess`'s reader thread sees EOF and exits
    /// → it drops the mpsc sender → the bridge thread's `rx.recv()` returns
    /// `Err` → the bridge exits. We then join the bridge with a bounded wait
    /// so it is not orphaned.
    ///
    /// For a `from_url` backend (`_process: None`): a no-op (nothing to stop).
    fn drop(&mut self) {
        if let Some(mut process) = self._process.take() {
            // Graceful stop; ignore the io::Error (best-effort teardown).
            let _ = process.shutdown(Duration::from_secs(5));
            // `process` is dropped here, after its child is reaped, closing
            // stderr for good and unblocking the bridge thread's recv().
        }
        // Join the bridge thread so it is not silently orphaned. By the time
        // shutdown() has returned, Pat is reaped and its stderr is closed, so
        // the bridge's `rx.recv()` has already returned `Err` (or is about to)
        // — the join is bounded in practice. A `from_url` backend has no bridge
        // thread (`None`), so this is skipped there.
        //
        // FIX 3 (tuxlink-22l Codex R2): the join is BOUNDED. An unbounded
        // `handle.join()` would hang app teardown if the bridge thread never
        // exits — e.g. a Pat *descendant* inherited and held Pat's stderr write
        // end open, so the reader (and hence the bridge) never sees EOF despite
        // Pat itself being reaped. We join on a tiny throwaway joiner thread that
        // signals completion over an mpsc, and wait at most 2s. On timeout we
        // DETACH (drop the JoinHandle without joining): a single leaked,
        // short-lived thread blocked on `recv` is strictly better than a hung
        // application teardown. (The leaked thread exits on its own whenever the
        // held-open stderr finally closes.)
        if let Some(handle) = self._bridge_thread.take() {
            let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
            std::thread::spawn(move || {
                let _ = handle.join();
                // Receiver may already be gone (we timed out + detached); the
                // send error is expected and ignored in that case.
                let _ = done_tx.send(());
            });
            match done_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(()) => { /* bridge thread joined cleanly. */ }
                Err(_) => {
                    // Timed out (or joiner disconnected): detach. The bridge
                    // thread is leaked rather than blocking teardown forever.
                }
            }
        }
    }
}

#[async_trait]
impl WinlinkBackend for PatBackend {
    async fn list_messages(&self, folder: MailboxFolder)
        -> Result<Vec<MessageMeta>, BackendError>
    {
        let metas = self.client
            .list(folder)
            .await
            .map_err(|e| translate_pat_err(e, "list_messages"))?;

        Ok(metas
            .into_iter()
            .map(|m| MessageMeta {
                id: MessageId(m.mid),
                subject: m.subject,
                from: m.from,
                // Pat 1.0.0's list DTO carries no recipient list nor
                // attachment metadata; `Message` already degrades these
                // (pat_client.rs). Carried through faithfully so a future
                // Pat that exposes them populates the trait without a
                // mapping change. Spec §2.1 graceful degradation.
                to: m.to,
                date: m.date,
                unread: m.unread,
                body_size: m.body_size,
                has_attachments: m.has_attachments,
            })
            .collect())
    }

    async fn read_message_in(&self, folder: MailboxFolder, id: &MessageId)
        -> Result<MessageBody, BackendError>
    {
        // Task 12 (tuxlink-zsm): folder is now explicit so a Sent/Outbox
        // message reads from the right folder. The prior impl hardcoded
        // Inbox; `read_message` retains Inbox semantics via the trait's
        // default forwarder.
        let bytes = self.client
            .read(folder, &id.0)
            .await
            .map_err(|e| translate_pat_err_for_read(e, id))?;
        Ok(MessageBody {
            id: id.clone(),
            raw_rfc5322: bytes,
        })
    }

    async fn send_message(&self, msg: OutboundMessage)
        -> Result<Option<MessageId>, BackendError>
    {
        let to_refs: Vec<&str> = msg.to.iter().map(String::as_str).collect();
        self.client
            .send(&to_refs, &msg.subject, &msg.body, &msg.date)
            .await
            .map_err(|e| translate_pat_err(e, "send_message"))?;
        // Pat 1.0.0 returns plain-text confirmation, no MID — see
        // pat_client_test.rs::test_send_message_posts_multipart_form_data
        // for the verified API behavior. Honestly return None.
        Ok(None)
    }

    async fn connect(&self, transport: TransportConfig)
        -> Result<Session, BackendError>
    {
        // v0.0.1 stub: PatBackend's `connect` mints a synthetic session
        // tied to this backend's instance id; actual remote connection to
        // CMS is initiated implicitly by send_message in Pat's model. A
        // full Pat HTTP /api/connect integration is deferred to v0.5
        // Step 5 (CMS telnet client) where the trait's connect-semantics
        // align with native backend's session-state needs.
        //
        // For now: update status to Connecting → Connected and return a
        // session handle the caller can later pass to disconnect.
        let transport_label = match &transport {
            TransportConfig::Cms { mode } => format!("CMS-{:?}", mode),
        };
        {
            let mut s = self.status.write().map_err(|_| BackendError::Internal {
                msg: "status RwLock poisoned".to_string(),
                source: None,
            })?;
            *s = BackendStatus::Connected {
                transport: transport_label.clone(),
                peer: "cms.winlink.org".to_string(),
                since_iso: "2026-05-18T00:00:00Z".to_string(),
            };
        }
        Ok(Session {
            backend_id: self.backend_id,
            inner: SessionInner::Pat {
                pat_session_id: format!("pat-stub-{}", self.backend_id.0),
            },
        })
    }

    async fn disconnect(&self, session: Session)
        -> Result<(), BackendError>
    {
        if session.backend_id != self.backend_id {
            return Err(BackendError::InvalidSession);
        }
        {
            let mut s = self.status.write().map_err(|_| BackendError::Internal {
                msg: "status RwLock poisoned".to_string(),
                source: None,
            })?;
            *s = BackendStatus::Disconnected;
        }
        Ok(())
    }

    fn status(&self) -> BackendStatus {
        self.status
            .read()
            .map(|s| s.clone())
            .unwrap_or(BackendStatus::Error {
                reason: "status RwLock poisoned".to_string(),
            })
    }

    fn stream_log(&self) -> BoxStream<'static, LogLine> {
        let rx = self.log_tx.subscribe();
        BroadcastStream::new(rx)
            .filter_map(|res| async move { res.ok() })
            .boxed()
    }
}

// ============================================================================
// PatClientError → BackendError translation (spec §3.3)
// ============================================================================

fn translate_pat_err(err: PatClientError, context: &'static str) -> BackendError {
    match err {
        PatClientError::Http(e) if e.is_connect() => BackendError::BackendUnavailable {
            reason: "could not reach Pat HTTP sidecar".to_string(),
            source: Some(Box::new(e)),
        },
        PatClientError::Http(e) if e.is_timeout() => BackendError::TransportFailed {
            reason: "Pat HTTP request timed out".to_string(),
            source: Some(Box::new(e)),
        },
        PatClientError::Http(e) => BackendError::Internal {
            msg: format!("Pat HTTP client error in {context}"),
            source: Some(Box::new(e)),
        },
        PatClientError::Status(401) => BackendError::AuthFailed {
            reason: "Pat returned 401".to_string(),
        },
        PatClientError::Status(404) => BackendError::Internal {
            msg: format!("Pat returned 404 in {context}"),
            source: None,
        },
        PatClientError::Status(n) => BackendError::Internal {
            msg: format!("Pat returned status {n} in {context}"),
            source: None,
        },
        PatClientError::TooLarge { cap } => BackendError::Internal {
            msg: format!("Pat message exceeded the {cap}-byte read cap in {context}"),
            source: None,
        },
    }
}

/// Variant for `read_message` where 404 means the message doesn't exist
/// (vs other contexts where 404 is an unexpected internal error).
fn translate_pat_err_for_read(err: PatClientError, id: &MessageId) -> BackendError {
    match err {
        PatClientError::Status(404) => BackendError::NotFound(id.clone()),
        other => translate_pat_err(other, "read_message"),
    }
}

#[cfg(test)]
mod native_read_state_tests {
    use super::*;
    use crate::config::{
        CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PositionPrecision,
        PositionSource, PrivacyConfig, CONFIG_SCHEMA_VERSION,
    };
    use crate::native_mailbox::Mailbox;
    use crate::winlink::compose::compose_message;
    use tempfile::tempdir;

    fn offline_config() -> Config {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig { connect_to_cms: false, transport: CmsTransport::Telnet },
            identity: IdentityConfig { callsign: None, identifier: None, grid: None },
            privacy: PrivacyConfig {
                gps_state: GpsState::Off,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
        }
    }

    // tuxlink-882: the CMS handshake locator must be reduced to the configured
    // broadcast precision — a stored 6-char grid never leaks past a 4-char setting.
    #[test]
    fn cms_locator_reduces_to_broadcast_precision() {
        let mut cfg = offline_config();
        cfg.identity.grid = Some("CN87ux".to_string());

        cfg.privacy.position_precision = PositionPrecision::FourCharGrid;
        assert_eq!(cms_locator(&cfg), "CN87", "default precision must broadcast 4-char");

        cfg.privacy.position_precision = PositionPrecision::SixCharGrid;
        assert_eq!(cms_locator(&cfg), "CN87ux", "opt-in precision broadcasts 6-char");
    }

    #[test]
    fn cms_locator_empty_when_no_grid() {
        assert_eq!(cms_locator(&offline_config()), "");
    }

    // ========================================================================
    // tuxlink-686: resolve_locator — arbiter-sourced locator tests
    // ========================================================================

    fn cfg_with_grid(grid: &str) -> Config {
        let mut cfg = offline_config();
        cfg.identity.grid = Some(grid.to_string());
        cfg.privacy.position_precision = PositionPrecision::FourCharGrid;
        cfg
    }

    // No-arbiter fallback: resolve_locator(cfg, None) == cms_locator(cfg).
    #[test]
    fn resolve_locator_no_arbiter_falls_back_to_config() {
        let cfg = cfg_with_grid("CN87ux");
        assert_eq!(
            resolve_locator(&cfg, None),
            cms_locator(&cfg),
            "no arbiter: resolve_locator must equal cms_locator"
        );
        assert_eq!(
            resolve_locator(&cfg, None),
            "CN87",
            "config fallback must apply 4-char reduction"
        );
    }

    // Arbiter reduces to precision.
    #[test]
    fn resolve_locator_arbiter_reduces_to_precision() {
        let cfg = offline_config();
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Manual,
            Some("CN87ux".into()),
            PositionPrecision::FourCharGrid,
        );
        assert_eq!(
            resolve_locator(&cfg, Some(&arbiter)),
            "CN87",
            "arbiter with FourCharGrid precision must broadcast 4-char grid"
        );
    }

    // ★ KEY TEST: arbiter SUPERSEDES a stale config grid.
    // This proves that a runtime grid change (or GPS fix) reaches the air
    // even though the backend's config snapshot was taken at construction time.
    #[test]
    fn resolve_locator_arbiter_supersedes_stale_config_grid() {
        // Config was baked at startup with DM33; arbiter has been updated to CN87ux.
        let cfg = cfg_with_grid("DM33"); // stale startup snapshot
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Manual,
            Some("CN87ux".into()),
            PositionPrecision::FourCharGrid,
        );
        let locator = resolve_locator(&cfg, Some(&arbiter));
        // Must be the live arbiter's grid, NOT the stale config grid.
        assert_eq!(
            locator, "CN87",
            "arbiter must supersede the stale config snapshot (got {}; expected CN87, not DM33)",
            locator
        );
        assert_ne!(
            locator, "DM33",
            "stale config grid must NOT reach the air when the arbiter is present"
        );
    }

    // Arbiter authoritative when empty (no position): resolve_locator returns ""
    // and does NOT fall back to the config grid.
    #[test]
    fn resolve_locator_arbiter_authoritative_when_empty() {
        let cfg = cfg_with_grid("CN87ux"); // config has a grid
        // Arbiter with GPS source but no fix yet — broadcast_grid() returns None.
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None, // no manual grid fallback either
            PositionPrecision::FourCharGrid,
        );
        assert_eq!(
            resolve_locator(&cfg, Some(&arbiter)),
            "",
            "when the arbiter has no position it must return '' — NOT fall back to the config grid"
        );
    }

    // SixCharGrid opt-in: arbiter passes the full 6-char grid through to the air.
    #[test]
    fn resolve_locator_arbiter_respects_six_char_precision() {
        let cfg = offline_config();
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Manual,
            Some("CN87ux".into()),
            PositionPrecision::SixCharGrid,
        );
        assert_eq!(
            resolve_locator(&cfg, Some(&arbiter)),
            "CN87ux",
            "SixCharGrid opt-in must broadcast the full 6-char grid"
        );
    }

    // tuxlink-xgn: the NativeBackend override of `mark_read` flips a message
    // from unread to read as observed through `list_messages` (the surface the
    // mailbox_list command consumes). Seeding goes through a sibling Mailbox at
    // the same root — the backend's `mailbox` field is private, so sharing the
    // on-disk root is the public seam (no test-only production code).
    #[tokio::test]
    async fn native_backend_mark_read_flips_unread_seen_via_list() {
        let dir = tempdir().unwrap();
        let seed = Mailbox::new(dir.path());
        let raw = compose_message("N7CPZ", &["W1AW"], &[], "Hi", "body", 1_716_200_000).to_bytes();
        let id = seed.store(MailboxFolder::Inbox, &raw).unwrap();

        let backend = NativeBackend::new(offline_config(), dir.path());
        assert!(
            backend.list_messages(MailboxFolder::Inbox).await.unwrap()[0].unread,
            "seeded inbox message should start unread"
        );

        backend.mark_read(MailboxFolder::Inbox, &id).await.unwrap();

        assert!(
            !backend.list_messages(MailboxFolder::Inbox).await.unwrap()[0].unread,
            "after mark_read the message should be read"
        );
    }

    // tuxlink-gqo: the dev transport resolver. With no env overrides the configured
    // transport stands (production keeps CmsSsl/8773); TUXLINK_CMS_PLAINTEXT forces
    // plaintext/8772 so the app can reach cms-z (which exposes no 8773 TLS).
    #[test]
    fn resolve_cms_endpoint_defaults_to_configured_transport() {
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::CmsSsl, false, None),
            (8773, telnet::Transport::Tls)
        );
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::Telnet, false, None),
            (8772, telnet::Transport::Plaintext)
        );
    }

    #[test]
    fn resolve_cms_endpoint_plaintext_override_forces_plaintext_8772() {
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::CmsSsl, true, None),
            (8772, telnet::Transport::Plaintext)
        );
    }

    #[test]
    fn resolve_cms_endpoint_honors_explicit_port_override() {
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::CmsSsl, false, Some(8774)),
            (8774, telnet::Transport::Tls)
        );
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::CmsSsl, true, Some(2323)),
            (2323, telnet::Transport::Plaintext)
        );
    }

    // tuxlink-9z2: an error that follows an operator abort is a cancellation;
    // otherwise the raw outcome stands (success keeps, real error keeps).
    #[test]
    fn abort_aware_outcome_maps_error_to_cancelled_when_aborted() {
        let mapped = abort_aware_outcome(
            Err(BackendError::TransportFailed { reason: "socket shutdown".into(), source: None }),
            true,
        );
        assert!(matches!(mapped, Err(BackendError::Cancelled)));
    }

    #[test]
    fn abort_aware_outcome_preserves_real_error_when_not_aborted() {
        let mapped = abort_aware_outcome(
            Err(BackendError::TransportFailed { reason: "real failure".into(), source: None }),
            false,
        );
        assert!(matches!(mapped, Err(BackendError::TransportFailed { .. })));
    }

    #[test]
    fn abort_aware_outcome_preserves_success_even_if_aborted() {
        // The connect completed before the abort landed — keep the success.
        assert!(abort_aware_outcome(Ok(()), true).is_ok());
    }

    #[tokio::test]
    async fn native_backend_abort_is_safe_with_no_inflight_connect() {
        let dir = tempdir().unwrap();
        let backend = NativeBackend::new(offline_config(), dir.path());
        // Nothing in flight: abort must not panic, returns Ok, leaves Disconnected.
        backend.abort().await.unwrap();
        assert!(matches!(backend.status(), BackendStatus::Disconnected));
    }

    // Codex #1: single-flight. With a connect already in flight, a second connect
    // is rejected immediately (before any network/config work) rather than racing
    // on the shared abort state and re-sending the outbox.
    #[tokio::test]
    async fn connect_rejects_a_concurrent_connect() {
        let dir = tempdir().unwrap();
        let backend = NativeBackend::new(offline_config(), dir.path());
        backend.connect_in_progress.store(true, Ordering::SeqCst);
        let result = backend
            .connect(TransportConfig::Cms { mode: CmsTransport::Telnet })
            .await;
        assert!(
            matches!(result, Err(BackendError::BackendUnavailable { .. })),
            "a concurrent connect should be rejected, got {result:?}"
        );
    }
}
