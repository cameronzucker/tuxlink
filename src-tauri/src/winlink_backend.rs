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
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

// Re-export MailboxFolder so the trait surface doesn't reach into the
// Pat-specific module.
pub use crate::pat_client::MailboxFolder;

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

    /// Returns `Ok(Some(id))` when the backend assigns a MID at queue
    /// time, `Ok(None)` when it does not (current Pat 1.0.0 behavior:
    /// returns a plain-text confirmation, no MID).
    async fn send_message(&self, msg: OutboundMessage)
        -> Result<Option<MessageId>, BackendError>;

    async fn connect(&self, transport: TransportConfig)
        -> Result<Session, BackendError>;

    async fn disconnect(&self, session: Session)
        -> Result<(), BackendError>;

    fn status(&self) -> BackendStatus;

    fn stream_log(&self) -> BoxStream<'static, LogLine>;
}

// ============================================================================
// NativeBackend stub (spec §3.9)
// ============================================================================

/// v0.5 prep stub. Every method returns [`BackendError::NotImplemented`];
/// `status()` returns `Disconnected`; `stream_log()` is an empty stream.
/// Real native logic lands in v0.5 Steps 3–10.
pub struct NativeBackend {
    backend_id: BackendInstanceId,
}

impl NativeBackend {
    pub fn new() -> Self {
        Self { backend_id: BackendInstanceId::next() }
    }

    #[allow(dead_code)] // exposed for v0.5+ session-validity tests
    pub(crate) fn backend_id(&self) -> BackendInstanceId {
        self.backend_id
    }
}

impl Default for NativeBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WinlinkBackend for NativeBackend {
    async fn list_messages(&self, _folder: MailboxFolder)
        -> Result<Vec<MessageMeta>, BackendError>
    {
        Err(BackendError::NotImplemented)
    }

    async fn read_message_in(&self, _folder: MailboxFolder, _id: &MessageId)
        -> Result<MessageBody, BackendError>
    {
        Err(BackendError::NotImplemented)
    }

    async fn send_message(&self, _msg: OutboundMessage)
        -> Result<Option<MessageId>, BackendError>
    {
        Err(BackendError::NotImplemented)
    }

    async fn connect(&self, _transport: TransportConfig)
        -> Result<Session, BackendError>
    {
        Err(BackendError::NotImplemented)
    }

    async fn disconnect(&self, _session: Session)
        -> Result<(), BackendError>
    {
        Err(BackendError::NotImplemented)
    }

    fn status(&self) -> BackendStatus {
        BackendStatus::Disconnected
    }

    fn stream_log(&self) -> BoxStream<'static, LogLine> {
        futures::stream::empty().boxed()
    }
}

// ============================================================================
// PatBackend (spec §3.8)
// ============================================================================

use std::sync::{Arc, RwLock};
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
    let mut line = LogLine {
        seq: 0,
        timestamp_iso: now_iso8601_utc(),
        level: LogLevel::Info,
        source: LogSource::Pat,
        message: raw,
    };
    // `append` assigns + returns the monotonic seq and stores the line.
    let seq = buffer.append(line.clone());
    line.seq = seq;
    // Live notify; 0 receivers is fine (durability is in `buffer`).
    let _ = log_tx.send(line.clone());
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
        let process = crate::pat_process::PatProcess::spawn(crate::pat_process::PatSpawnOptions {
            binary: opts.binary,
            config_path: opts.config_path,
            mbox_dir: opts.mbox_dir,
            http_listen_port: 0,
            pid_file: opts.pid_file,
            log_sink: Some(tx),
            tuxlink_config: opts.tuxlink_config,
            http_announce_timeout: Duration::from_secs(10),
        })
        .map_err(|e| BackendError::BackendUnavailable {
            reason: format!("Pat failed to start: {e}"),
            source: Some(Box::new(e)),
        })?;

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
        if let Some(handle) = self._bridge_thread.take() {
            let _ = handle.join();
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
