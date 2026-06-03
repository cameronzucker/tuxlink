//! Backend-abstraction trait for tuxlink's Winlink interactions.
//!
//! Spec: docs/superpowers/specs/2026-05-18-winlink-backend-trait-design.md
//! bd issue: tuxlink-z5f
//!
//! This module defines the `WinlinkBackend` trait — the architectural
//! boundary that decouples tuxlink's UI/config layer from any one Winlink
//! protocol implementation. One implementation lives here:
//!
//! - [`NativeBackend`] — speaks B2F directly, stores messages in its own
//!   mailbox, and connects over plaintext or TLS telnet.
//!
//! Per [feedback_discipline_triage_rule]: the trait is the hard-to-undo
//! architectural decision; once defined, implementations are TDD plumbing.

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

/// Mailbox folder selector. `#[non_exhaustive]` per tuxlink-z5f v2 P1 #5 —
/// future folders (Drafts, Spam, custom) added without breaking exhaustive
/// matches at call sites. `Copy + Clone + Debug` so the trait re-export
/// carries useful semantics.
///
/// Canonical path: `winlink_backend::MailboxFolder` (moved from the deleted
/// `pat_client` module in tuxlink-9phd Phase 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MailboxFolder { Inbox, Sent, Outbox, Archive }

impl MailboxFolder {
    #[allow(dead_code)]
    pub(crate) fn as_path(&self) -> &'static str {
        match self {
            MailboxFolder::Inbox => "in",
            MailboxFolder::Sent => "sent",
            MailboxFolder::Outbox => "out",
            MailboxFolder::Archive => "archive",
        }
    }
}

// Native backend wiring (see the NativeBackend section below).
use crate::config::{broadcast_grid, CmsTransport, Config};
use crate::native_mailbox::Mailbox;
use crate::winlink::ax25::{Address, KissLinkConfig};
use crate::winlink::message::Message;
use crate::winlink::proposal::Answer;
use crate::winlink::session::ExchangeRole;
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
    /// Recipient list. Added by Task 12 (tuxlink-zsm). NativeBackend populates
    /// this from the stored RFC5322 headers; spec §2.1 graceful degradation
    /// for backends that don't expose a recipient list.
    pub to: Vec<String>,
    /// RFC 3339 UTC timestamp. Backend emits canonical form.
    pub date: String,
    pub unread: bool,
    pub body_size: u64,
    /// Attachment-presence indicator for the list `#` column. Added by Task
    /// 12 (tuxlink-zsm). The full attachment list is materialized at read time
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

/// Attachment carried in an outbound message. Spec §6.2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundAttachment {
    pub filename: String,
    pub bytes: Vec<u8>,
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
    pub attachments: Vec<OutboundAttachment>,
}

/// Transport selector for `connect`. `#[non_exhaustive]` so v0.5+ can add
/// Packet/Pactor/VARA HF/VARA FM/AX.25/KISS variants without breaking
/// existing match arms.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TransportConfig {
    /// CMS Telnet (plain or TLS), per existing `config::CmsTransport`.
    Cms { mode: crate::config::CmsTransport },
    /// AX.25 1200-baud packet over a KISS link (TCP / serial). The SSID rides
    /// the AX.25 *link* address; the B2F identity uses the base call (spec §4.4).
    Packet {
        link: KissLinkConfig,
        ssid: u8,
        role: PacketRole,
    },
}

/// What a packet connection does. `DialTo` is the operator pressing "Connect to"
/// (gateway OR peer — tuxlink reacts to the challenge, not a mode flag); `Listen`
/// is the idle armed-to-answer state (spec §2, §4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketRole {
    DialTo { call: String, path: Vec<String> },
    Listen,
}

/// What a `PacketRole` + identity resolves into for the lifecycle: the SSID'd
/// link address, the base B2F call, the exchange role, and (for a dial) the
/// target + digipeater addresses. Mirrors `resolve_cms_endpoint`'s "config →
/// concrete endpoint" job for the packet transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPacket {
    pub link_mycall: Address,
    pub base_mycall: String,
    pub role: ExchangeRole,
    /// `Some((target, digis))` for a dial; `None` for listen.
    pub dial: Option<(Address, Vec<Address>)>,
}

/// Parse a `CALL` or `CALL-SSID` string into an [`Address`]. A bare call has
/// SSID 0. Rejects an SSID outside 0–15 or a malformed token.
fn parse_call_ssid(s: &str) -> Result<Address, BackendError> {
    let (call, ssid) = match s.rsplit_once('-') {
        Some((c, s_part)) => {
            let n: u8 = s_part
                .parse()
                .map_err(|_| BackendError::NotConfigured(format!("bad SSID in '{s_part}'")))?;
            (c, n)
        }
        None => (s, 0),
    };
    if ssid > 15 || call.is_empty() {
        return Err(BackendError::NotConfigured(format!("bad call/ssid '{s}'")));
    }
    Ok(Address { call: call.to_uppercase(), ssid })
}

/// Resolve identity + role into the concrete addresses + exchange role. Enforces
/// the 0–2 digipeater cap (spec §1) and the identity split (spec §4.4).
pub fn resolve_packet_endpoint(
    base_mycall: &str,
    ssid: u8,
    role: PacketRole,
) -> Result<ResolvedPacket, BackendError> {
    let base = base_mycall.trim().to_uppercase();
    let link_mycall = Address { call: base.clone(), ssid };
    match role {
        PacketRole::Listen => Ok(ResolvedPacket {
            link_mycall,
            base_mycall: base,
            role: ExchangeRole::Answer,
            dial: None,
        }),
        PacketRole::DialTo { call, path } => {
            if path.len() > 2 {
                return Err(BackendError::NotConfigured(format!(
                    "at most 2 digipeaters allowed (got {})",
                    path.len()
                )));
            }
            let target = parse_call_ssid(&call)?;
            let digis = path
                .iter()
                .map(|p| parse_call_ssid(p))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ResolvedPacket {
                link_mycall,
                base_mycall: base,
                role: ExchangeRole::Dial,
                dial: Some((target, digis)),
            })
        }
    }
}

/// Build the per-message proposals + compressed bodies for a B2F exchange
/// from a Mailbox's Outbox folder. Skips messages whose bytes fail to parse
/// or whose body cannot be turned into a proposal — mirroring the inline
/// pattern in `native_telnet_exchange` / `native_packet_exchange` /
/// `run_ardop_b2f_exchange` (winlink_backend.rs).
///
/// Pulled out so paths that bypass `NativeBackend::connect` (in particular
/// `ui_commands::telnet_p2p_connect` for tuxlink-l55l) build the same shape
/// of outbound without duplicating the loop.
pub fn build_outbound_proposals(
    mailbox: &Mailbox,
) -> Result<Vec<session::OutboundMessage>, BackendError> {
    let mut outbound = Vec::new();
    for meta in mailbox.list(MailboxFolder::Outbox)? {
        let body = mailbox.read(MailboxFolder::Outbox, &meta.id)?;
        if let Ok(message) = Message::from_bytes(&body.raw_rfc5322) {
            if let Some((proposal, compressed)) = message.to_proposal() {
                let title = message.header("Subject").unwrap_or_default().to_string();
                outbound.push(session::OutboundMessage { proposal, title, compressed });
            }
        }
    }
    Ok(outbound)
}

#[cfg(test)]
mod build_outbound_proposals_tests {
    use super::*;
    use crate::native_mailbox::Mailbox;
    use crate::winlink::compose::compose_message;
    use tempfile::tempdir;

    #[test]
    fn empty_outbox_returns_empty_vec() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let out = build_outbound_proposals(&mailbox).unwrap();
        assert!(out.is_empty(), "empty outbox should produce no proposals; got {out:?}");
    }

    #[test]
    fn queued_drafts_produce_one_proposal_each() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        // tuxlink-l55l: two queued outbound drafts addressed to two different
        // recipients — P2P semantics send ALL of them; recipient routing is
        // the peer's job (WLE-as-Post-Office). Build via the same path the
        // compose flow uses so the bytes are a valid Winlink message.
        let m1 = compose_message(
            "N7CPZ",
            &["W7AUX"],
            &[],
            "P2P-test-1",
            "first body",
            1_716_200_000,
        );
        let m2 = compose_message(
            "N7CPZ",
            &["cameronzucker@gmail.com"],
            &[],
            "P2P-test-2",
            "second body",
            1_716_200_001,
        );
        mailbox.store(MailboxFolder::Outbox, &m1.to_bytes()).unwrap();
        mailbox.store(MailboxFolder::Outbox, &m2.to_bytes()).unwrap();

        let out = build_outbound_proposals(&mailbox).unwrap();
        assert_eq!(
            out.len(),
            2,
            "two queued drafts should produce two proposals; got {} ({out:?})",
            out.len()
        );
        let titles: Vec<&str> = out.iter().map(|o| o.title.as_str()).collect();
        assert!(titles.contains(&"P2P-test-1"));
        assert!(titles.contains(&"P2P-test-2"));
    }

    #[test]
    fn no_per_peer_filtering_ships_all_drafts() {
        // P2P semantics (handoff): tuxlink should NOT filter outbox by peer
        // callsign at dial-time. The peer (typically WLE) acts as the
        // post-office and routes via its own CMS uplink. This test pins the
        // contract: queue a draft addressed to a third party, dial peer X,
        // and the draft must still be offered.
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        let third_party = compose_message(
            "N7CPZ",
            &["unrelated@example.org"],
            &[],
            "Routed-through-peer",
            "body",
            1_716_200_002,
        );
        mailbox
            .store(MailboxFolder::Outbox, &third_party.to_bytes())
            .unwrap();

        let out = build_outbound_proposals(&mailbox).unwrap();
        assert_eq!(
            out.len(),
            1,
            "drafts addressed to a third party MUST still be offered to the peer; got {out:?}"
        );
        assert_eq!(out[0].title, "Routed-through-peer");
    }
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
    /// Backend-specific session payload. Held for future-use match arms in
    /// `disconnect` to call out to native cleanup.
    #[allow(dead_code)]
    pub(crate) inner: SessionInner,
}

#[derive(Debug)]
pub(crate) enum SessionInner {
    /// NativeBackend session. Variant kept for future v0.5+ session shapes.
    Native(()),
}

impl Drop for Session {
    fn drop(&mut self) {
        // Local cleanup only — see spec §3.5. No remote-disconnect call;
        // explicit WinlinkBackend::disconnect is the guaranteed release path.
        // Future native sessions will close their socket fd via Drop on the
        // inner stream.
    }
}

/// Backend connection status. Implementations cache + update internally;
/// `status()` reads the cache (MUST NOT do I/O).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BackendStatus {
    Disconnected,
    Connecting { transport: String },
    /// Packet armed-but-idle: the AX.25 layer is listening to answer an inbound
    /// SABM, but no session is up. Distinct from `Connecting` (an active dial)
    /// and `Disconnected` (not armed). Carries the transport so the ribbon can
    /// render "Listening · Packet 1200". (tuxlink-orj)
    Listening { transport: String },
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
pub enum LogSource { Backend, Transport, Wire }

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

    /// Mark a message read. Best-effort: the default is a no-op.
    /// `NativeBackend` overrides it to drop a read-marker in its store.
    /// A failure here MUST NOT fail the read that triggered it — the caller
    /// (`message_read`) treats read-state as best-effort (tuxlink-xgn).
    async fn mark_read(&self, _folder: MailboxFolder, _id: &MessageId)
        -> Result<(), BackendError> {
        Ok(())
    }

    /// Move a message between folders (tuxlink-ca5x). The Inbox → Archive path
    /// is the canonical use today; future user folders (tuxlink-f62f) flow
    /// through the same trait method. `NativeBackend` overrides this to
    /// dispatch to its [`Mailbox::move_to`] (which carries the read-marker
    /// across folders + best-effort updates the search index). Default
    /// returns [`BackendError::NotImplemented`].
    async fn move_message(
        &self,
        _from: MailboxFolder,
        _to: MailboxFolder,
        _id: &MessageId,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }

    // ========================================================================
    // User folders (tuxlink-f62f — Phase 2 of the user-folders work).
    // ========================================================================

    /// List the user folders registered for this backend. `NativeBackend`
    /// reads `<root>/.folders.json`; default is empty.
    async fn list_user_folders(
        &self,
    ) -> Result<Vec<crate::user_folders::UserFolder>, BackendError> {
        Ok(Vec::new())
    }

    /// Create a new user folder with the given display name. Validates and
    /// slug-derives. Default `NotImplemented`.
    async fn create_user_folder(
        &self,
        _display_name: &str,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Delete a user folder. `on_messages` controls cascade behavior
    /// (spec §6 D6). Default `NotImplemented`.
    async fn delete_user_folder(
        &self,
        _slug: &str,
        _on_messages: crate::native_mailbox::DeleteAction,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Rename a user folder (display name only; slug is stable per spec §3.1).
    /// Default `NotImplemented`.
    async fn rename_user_folder(
        &self,
        _slug: &str,
        _new_display_name: &str,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// List the messages in a user folder. Default empty.
    async fn list_user_messages(
        &self,
        _slug: &str,
    ) -> Result<Vec<MessageMeta>, BackendError> {
        Ok(Vec::new())
    }

    /// Read one message from a user folder. Default `NotFound`.
    async fn read_user_message(
        &self,
        _slug: &str,
        id: &MessageId,
    ) -> Result<MessageBody, BackendError> {
        Err(BackendError::NotFound(id.clone()))
    }

    /// Move a message between any two folder references (system↔user etc).
    /// `NativeBackend` delegates to [`Mailbox::move_between`]; default
    /// `NotImplemented`.
    async fn move_between_folders(
        &self,
        _from: crate::native_mailbox::FolderRef,
        _to: crate::native_mailbox::FolderRef,
        _id: &MessageId,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Returns `Ok(id)` with the MID assigned at queue time.
    ///
    /// `NativeBackend` assigns a real filesystem-derived MID at queue time.
    async fn send_message(&self, msg: OutboundMessage)
        -> Result<MessageId, BackendError>;

    async fn connect(&self, transport: TransportConfig)
        -> Result<Session, BackendError>;

    async fn disconnect(&self, session: Session)
        -> Result<(), BackendError>;

    /// Abort an in-flight [`WinlinkBackend::connect`] (tuxlink-9z2): shut down the
    /// connecting socket to unblock a slow TLS/login/exchange phase and return the
    /// backend to `Disconnected`. The aborted `connect` resolves to
    /// [`BackendError::Cancelled`]. Default is a no-op `Ok`. Safe to call when idle.
    async fn abort(&self) -> Result<(), BackendError> {
        Ok(())
    }

    /// Refresh the live config the connect paths read (tuxlink-ka7 / tuxlink-p5u).
    /// `NativeBackend` originally froze its `config` at construction, so the connect
    /// path read that stale snapshot — a UI host/transport/packet-param change only
    /// took effect after an app restart. The config-writing UI commands call this
    /// after persisting, so the NEXT connect honors the change restart-free. Default
    /// no-op for backends that hold no config snapshot.
    fn set_config(&self, _config: Config) {}

    fn status(&self) -> BackendStatus;

    fn stream_log(&self) -> BoxStream<'static, LogLine>;
}

// ============================================================================
// NativeBackend (spec §3.9)
// ============================================================================

/// A sink for per-step connect progress messages (tuxlink-gqo). The connect path
/// runs in `spawn_blocking`, so the sink must be `Send + Sync`; production wires
/// it (in `bootstrap::install_native`) to append a `LogSource::Transport` line to
/// the session log and emit it live. Decoupled from the `LogLine` machinery on
/// purpose — `winlink::telnet` only ever calls it with a `&str` phase message.
pub type ProgressSink = Arc<dyn Fn(&str) + Send + Sync>;

/// A sink for raw B2F wire lines (tuxlink-nki). The connect path tees every
/// on-wire protocol line (both directions) into this; `bootstrap::install_native`
/// wires it to append a `LogSource::Wire` line to the session log + emit it live,
/// so the operator can watch the real `[WL2K-...]`/`;FW`/`FF`/`FQ` dialogue under
/// the "Raw output" view. No-op by default (tests + the no-progress path).
pub type WireSink = Arc<dyn Fn(&str) + Send + Sync>;

/// The native Winlink backend: speaks B2F directly (no Pat), stores messages in
/// its own [`Mailbox`], and connects over plaintext or TLS telnet. `connect`
/// runs the real CMS exchange on a blocking task; the actual on-air protocol is
/// validated by `src/bin/native_cms_probe.rs` and the `winlink::*` tests.
pub struct NativeBackend {
    backend_id: BackendInstanceId,
    /// Live config, refreshable via [`WinlinkBackend::set_config`] (tuxlink-ka7 /
    /// tuxlink-p5u). Behind a `RwLock` so a UI host/transport/packet-param change
    /// reaches the connect + send paths without an app restart; reads clone through
    /// [`Self::live_config`].
    config: RwLock<Config>,
    mailbox: Arc<Mailbox>,
    log_tx: broadcast::Sender<LogLine>,
    status: Arc<RwLock<BackendStatus>>,
    /// Broadcasts every BackendStatus transition (2026-05-31): the frontend's
    /// 5s status poll missed sub-second CMS-Z exchanges. Subscribers (the
    /// bootstrap's emitter task) translate these to Tauri events. Best-effort
    /// — send failures (no receivers) are swallowed in set_status.
    status_tx: broadcast::Sender<BackendStatus>,
    progress: ProgressSink,
    /// Sink for raw B2F wire lines (tuxlink-nki): tees the on-wire dialogue into
    /// the session log as `LogSource::Wire` so it surfaces under "Raw output". No-op
    /// by default; production wires it in `bootstrap::install_native`.
    wire: WireSink,
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
    /// Test-injected Packet listener allowlist (tuxlink-inde). When `Some`, the
    /// Packet `Listen` path uses this in-memory list instead of loading from
    /// `<config-dir>/listener/packet/allowed_stations.json`. Production
    /// (`bootstrap`/UI) leaves this `None` so the disk file is authoritative;
    /// tests inject a permissive list (e.g. `allow_all=TRUE`) to bypass the
    /// architectural default of "reject all until operator curates."
    packet_allowlist_override: Option<crate::winlink::listener::AllowedStations>,
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
        // 2026-05-31 operator-flagged: the 5s status poll missed sub-second
        // CMS-Z exchanges. status_tx broadcasts every BackendStatus
        // transition; bootstrap::install_native subscribes + emits Tauri
        // `backend_status:change` events so the frontend sees every state
        // (including the brief Connected window) without poll-rate aliasing.
        // Capacity 64: bursts of state churn (Connecting → Connected →
        // Disconnecting → Disconnected) fit; slow listeners just lose the
        // oldest event (acceptable — the periodic snapshot poll backstops).
        let (status_tx, _status_rx) = broadcast::channel(64);
        Self {
            backend_id: BackendInstanceId::next(),
            config: RwLock::new(config),
            mailbox: Arc::new(Mailbox::new(mailbox_root)),
            log_tx,
            status: Arc::new(RwLock::new(BackendStatus::Disconnected)),
            status_tx,
            progress,
            wire: Arc::new(|_: &str| {}),
            abort_handle: Arc::new(Mutex::new(None)),
            aborting: Arc::new(AtomicBool::new(false)),
            connect_in_progress: Arc::new(AtomicBool::new(false)),
            position: None,
            packet_allowlist_override: None,
        }
    }

    /// Subscribe to live status transitions. The bootstrap installs an
    /// emitter task that consumes this receiver and emits Tauri
    /// `backend_status:change` events. No-op when nothing is subscribed
    /// (broadcast::send returns Err that we swallow in set_status).
    pub fn subscribe_status(&self) -> broadcast::Receiver<BackendStatus> {
        self.status_tx.subscribe()
    }

    /// Attach the live position arbiter (tuxlink-686). Builder-style so existing
    /// constructors and tests are unaffected.
    pub fn with_position(mut self, arbiter: Arc<crate::position::PositionArbiter>) -> Self {
        self.position = Some(arbiter);
        self
    }

    /// Inject a Packet listener allowlist (tuxlink-inde). When set, the
    /// `Listen` role bypasses the disk-backed
    /// `<config-dir>/listener/packet/allowed_stations.json` lookup and uses
    /// this in-memory list instead. Production wires the disk file via
    /// `bootstrap`/UI; tests use this to permit the dialer's callsign without
    /// touching the user's filesystem.
    pub fn with_packet_allowlist(
        mut self,
        allowed: crate::winlink::listener::AllowedStations,
    ) -> Self {
        self.packet_allowlist_override = Some(allowed);
        self
    }

    /// Attach a search index to the mailbox so incremental index hooks run on
    /// every `store`/`move_to`/`mark_read` (Codex adrev — find-messages P1).
    /// Builder-style; must be called before the `mailbox` Arc is cloned (i.e.
    /// before the backend is installed into `BackendState`). Panics if the
    /// Arc is already shared — that would be a programmer error in the boot path.
    pub fn with_index(mut self, index: Arc<std::sync::Mutex<crate::search::index::Index>>) -> Self {
        let mbox = Arc::try_unwrap(self.mailbox)
            .unwrap_or_else(|_| panic!("with_index called after Arc<Mailbox> was shared — call before install"))
            .with_index(index);
        self.mailbox = Arc::new(mbox);
        self
    }

    /// Attach a raw-wire log sink (tuxlink-nki). Builder-style so existing
    /// constructors and tests are unaffected; no-op by default.
    pub fn with_wire_log(mut self, wire: WireSink) -> Self {
        self.wire = wire;
        self
    }

    /// Clone the live config (tuxlink-ka7 / tuxlink-p5u). The connect + send paths
    /// read through here so a [`WinlinkBackend::set_config`] refresh applies on the
    /// next operation without an app restart. Recovers a poisoned lock's inner value
    /// rather than panicking — a poisoned config lock must not brick every connect.
    fn live_config(&self) -> Config {
        self.config
            .read()
            .map(|c| c.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    fn set_status(&self, status: BackendStatus) {
        if let Ok(mut s) = self.status.write() {
            *s = status.clone();
        }
        // Best-effort broadcast for the event-emitter task (2026-05-31). Send
        // returns Err when there are no active subscribers — that's fine, the
        // RwLock above remains the snapshot source for backend_status polls.
        let _ = self.status_tx.send(status);
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

    async fn move_message(
        &self,
        from: MailboxFolder,
        to: MailboxFolder,
        id: &MessageId,
    ) -> Result<(), BackendError> {
        self.mailbox.move_to(from, to, id)
    }

    async fn list_user_folders(
        &self,
    ) -> Result<Vec<crate::user_folders::UserFolder>, BackendError> {
        Ok(self.mailbox.list_user_folders())
    }

    async fn create_user_folder(
        &self,
        display_name: &str,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        self.mailbox.create_user_folder(display_name)
    }

    async fn delete_user_folder(
        &self,
        slug: &str,
        on_messages: crate::native_mailbox::DeleteAction,
    ) -> Result<(), BackendError> {
        self.mailbox.delete_user_folder(slug, on_messages)
    }

    async fn rename_user_folder(
        &self,
        slug: &str,
        new_display_name: &str,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        self.mailbox.rename_user_folder(slug, new_display_name)
    }

    async fn list_user_messages(
        &self,
        slug: &str,
    ) -> Result<Vec<MessageMeta>, BackendError> {
        self.mailbox.list_user(slug)
    }

    async fn read_user_message(
        &self,
        slug: &str,
        id: &MessageId,
    ) -> Result<MessageBody, BackendError> {
        self.mailbox.read_user(slug, id)
    }

    async fn move_between_folders(
        &self,
        from: crate::native_mailbox::FolderRef,
        to: crate::native_mailbox::FolderRef,
        id: &MessageId,
    ) -> Result<(), BackendError> {
        self.mailbox.move_between(from, to, id)
    }

    async fn send_message(
        &self,
        msg: OutboundMessage,
    ) -> Result<MessageId, BackendError> {
        let callsign = self
            .live_config()
            .identity
            .callsign
            .ok_or_else(|| BackendError::NotConfigured("identity.callsign".into()))?;
        // The trait carries an RFC 3339 date; fall back to now if unparseable.
        let unix_secs = parse_rfc3339_secs(&msg.date).unwrap_or_else(now_unix_secs);
        let to: Vec<&str> = msg.to.iter().map(String::as_str).collect();
        let cc: Vec<&str> = msg.cc.iter().map(String::as_str).collect();
        let message = compose::compose_message_with_files(
            &callsign,
            &to,
            &cc,
            &msg.subject,
            &msg.body,
            &msg.attachments,
            unix_secs,
        )
        .map_err(|e| BackendError::MessageRejected(e.to_string()))?;
        let id = self.mailbox.store(MailboxFolder::Outbox, &message.to_bytes())?;
        Ok(id)
    }

    async fn connect(&self, transport: TransportConfig) -> Result<Session, BackendError> {
        // Dispatch to per-transport paths.
        if let TransportConfig::Packet { link, ssid, role } = transport {
            return self.packet_connect_inner(link, ssid, role).await;
        }
        let mode = match transport {
            TransportConfig::Cms { mode } => mode,
            _ => return Err(BackendError::NotImplemented),
        };

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

        let config = self.live_config();
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
        let wire = self.wire.clone();
        let abort_handle = self.abort_handle.clone();
        let aborting = self.aborting.clone();
        let position = self.position.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            native_connect(&config, &mailbox, mode, &*progress, &*wire, &abort_handle, &aborting, position.as_deref())
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
                    // tuxlink-3o0: the peer is the host actually dialed (the
                    // operator's configured host, or the TUXLINK_CMS_HOST override)
                    // — no longer a hardcoded const.
                    peer: resolve_cms_host(&self.live_config()),
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

    /// Refresh the live config the connect + send paths read (tuxlink-ka7 /
    /// tuxlink-p5u). Called by the config-writing UI commands after they persist, so
    /// the next connect honors a host/transport/packet-param change restart-free.
    /// Recovers a poisoned lock rather than panicking — a failed write must not wedge
    /// the backend.
    fn set_config(&self, config: Config) {
        match self.config.write() {
            Ok(mut slot) => *slot = config,
            Err(poisoned) => *poisoned.into_inner() = config,
        }
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

impl NativeBackend {
    /// Packet-transport connect path (Task 5/6): resolve the endpoint, open the
    /// KISS link, connect/answer, and run the exchange. Wired here from the
    /// `WinlinkBackend::connect` dispatch above.
    async fn packet_connect_inner(
        &self,
        link: KissLinkConfig,
        ssid: u8,
        role: PacketRole,
    ) -> Result<Session, BackendError> {
        // Single-flight guard (same as the CMS arm).
        if self
            .connect_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackendError::BackendUnavailable {
                reason: "a connection is already in progress".to_string(),
                source: None,
            });
        }
        let _guard = ConnectGuard {
            in_progress: self.connect_in_progress.clone(),
            handle: self.abort_handle.clone(),
        };

        self.aborting.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.abort_handle.lock() {
            *slot = None;
        }

        let base = self
            .live_config()
            .identity
            .callsign
            .ok_or_else(|| BackendError::NotConfigured("identity.callsign".into()))?;
        // Decide the armed-state status before `role` is moved into resolve
        // (tuxlink-orj): Listen → Listening (armed), DialTo → Connecting (dial).
        let initial_status = initial_packet_status(&role, ssid);
        let resolved = resolve_packet_endpoint(&base, ssid, role)?;

        let config = self.live_config();
        let mailbox = self.mailbox.clone();
        let progress = self.progress.clone();
        let wire = self.wire.clone();
        let abort_handle = self.abort_handle.clone();
        let aborting = self.aborting.clone();
        let allowlist_override = self.packet_allowlist_override.clone();

        self.set_status(initial_status);

        let outcome = tokio::task::spawn_blocking(move || {
            native_packet_connect(
                &config,
                &mailbox,
                link,
                resolved,
                &*progress,
                &*wire,
                &abort_handle,
                aborting,
                allowlist_override,
            )
        })
        .await
        .map_err(|e| BackendError::Internal {
            msg: format!("packet connect task failed: {e}"),
            source: None,
        })?;

        match abort_aware_outcome(outcome, self.aborting.load(Ordering::SeqCst)) {
            Ok(()) => {
                self.set_status(BackendStatus::Connected {
                    transport: format!("Packet-{ssid}"),
                    peer: "packet".to_string(),
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
}

// ============================================================================
// Packet-transport functions (native_packet_exchange + native_packet_connect)
// Stubs — fully implemented in Tasks 5 and 6.
// ============================================================================

/// Run one B2F exchange over an already-connected AX.25 stream. By-value
/// ownership: the stream is consumed + dropped on return (DISC fires from
/// `Ax25Stream::drop`). Generic over `Read + Write` so it is fully
/// unit-tested with an in-memory `FakeAx25Stream` — no network, no RF.
///
/// Session-identity context for a packet exchange. Groups the per-session
/// identity parameters (`base_mycall`, `targetcall`, `password`, `role`,
/// `locator`) to keep `native_packet_exchange` under the clippy
/// `too_many_arguments` threshold (7).
struct PacketConnectCtx<'a> {
    /// B2F identity call (base callsign, no SSID; spec §4.4).
    base_mycall: &'a str,
    /// Peer callsign (gateway or P2P peer).
    targetcall: &'a str,
    /// Winlink password for gateway secure-login (None for P2P).
    password: Option<String>,
    /// Exchange role: Dial (slave) for DialTo, Answer (master) for Listen.
    role: ExchangeRole,
    /// Grid locator at configured broadcast precision.
    locator: &'a str,
}

/// Streams whose `read()` returns `Ok(0)` for "no data yet" rather than EOF (the
/// `Ax25Stream` defect-J contract) expose closed-ness here so [`BlockingB2fStream`]
/// can tell a transient idle read from a genuine end-of-link.
trait MaybeClosed {
    fn is_closed(&self) -> bool;
}
impl MaybeClosed for crate::winlink::ax25::Ax25Stream {
    fn is_closed(&self) -> bool {
        crate::winlink::ax25::Ax25Stream::is_closed(self)
    }
}

/// Adapts an `Ax25Stream` to the `std::io::Read` EOF contract for the B2F layer.
///
/// `Ax25Stream::read` returns `Ok(0)` for "no data buffered yet" (defect-J), but
/// `BufReader`/`read_until` — which `wire::read_line` is built on — treat `Ok(0)`
/// as **EOF**. So on a real RF link, the first inter-frame gap longer than the
/// link poll window would abort the handshake/exchange as `ConnectionClosed`.
/// This adapter blocks until ≥1 byte arrives, the link genuinely closes (`Ok(0)`
/// while `is_closed()`, e.g. an inbound DISC), or an error — making `Ok(0)` mean
/// EOF as the contract requires. Found by a Codex adversarial round (2026-05-22);
/// the localhost TCP-relay e2e test masked it because bytes arrive instantly.
struct BlockingB2fStream<S>(S);

impl<S: std::io::Read + MaybeClosed> std::io::Read for BlockingB2fStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let n = self.0.read(buf)?;
            // n > 0: data. n == 0 && closed: genuine EOF. n == 0 && open: no data
            // yet — loop. `Ax25Stream::read` naps a poll interval when idle, so
            // this is a poll, not a busy-spin.
            if n > 0 || self.0.is_closed() {
                return Ok(n);
            }
        }
    }
}

impl<S: std::io::Write> std::io::Write for BlockingB2fStream<S> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// Identity split (spec §4.4): `base_mycall` is the B2F call (no SSID); the
/// SSID rode the AX.25 link address in the `connect`/`answer` call that
/// produced `stream`. `locator` is the operator's grid reduced to the
/// configured broadcast precision (pass `cms_locator(config)`, already exists).
fn native_packet_exchange<S: std::io::Read + std::io::Write + Send + 'static>(
    stream: S,
    ctx: PacketConnectCtx<'_>,
    mailbox: &Mailbox,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
) -> Result<(), BackendError> {
    let PacketConnectCtx { base_mycall, targetcall, password, role, locator } = ctx;
    // Split the owned stream into simultaneous read + write halves via a shared
    // Arc<Mutex> (the same pattern as telnet's shared-socket approach). The
    // exchange is strictly turn-based so the lock is never contended.
    use std::sync::{Arc, Mutex};
    trait RW: std::io::Read + std::io::Write + Send {}
    impl<T: std::io::Read + std::io::Write + Send> RW for T {}

    let shared: Arc<Mutex<Box<dyn RW>>> = Arc::new(Mutex::new(Box::new(stream)));

    struct ReadHalf(Arc<Mutex<Box<dyn RW>>>);
    struct WriteHalf(Arc<Mutex<Box<dyn RW>>>);
    impl std::io::Read for ReadHalf {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            self.0.lock().expect("ax25 lock").read(b)
        }
    }
    impl std::io::Write for WriteHalf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("ax25 lock").write(b)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().expect("ax25 lock").flush()
        }
    }
    let mut reader = std::io::BufReader::new(ReadHalf(shared.clone()));
    let mut writer = WriteHalf(shared.clone());

    // Build outbox proposals (mirrors native_connect).
    let mut outbound = Vec::new();
    for meta in mailbox.list(MailboxFolder::Outbox)? {
        let body = mailbox.read(MailboxFolder::Outbox, &meta.id)?;
        if let Ok(message) = Message::from_bytes(&body.raw_rfc5322) {
            if let Some((proposal, compressed)) = message.to_proposal() {
                let title = message.header("Subject").unwrap_or_default().to_string();
                outbound.push(session::OutboundMessage { proposal, title, compressed });
            }
        }
    }

    let exchange_config = session::ExchangeConfig {
        mycall: base_mycall.to_string(), // BASE call — no SSID in B2F identity
        targetcall: targetcall.to_string(),
        locator: locator.to_string(), // config-derived locator (controller directive)
        password,
    };

    progress("AX.25 connected; negotiating messages…");
    let result = session::run_exchange_with_role(
        &mut reader,
        &mut writer,
        role,
        &exchange_config,
        outbound,
        |proposals| proposals.iter().map(|_| Answer::Accept { resume_offset: 0 }).collect(),
        Some(wire_log),
    )
    .map_err(|e| BackendError::TransportFailed { reason: format!("{e:?}"), source: None })?;

    // P1.4 (Codex post-impl review): file accepted messages FIRST, then surface
    // any rejection error. The prior ordering returned early on rejections, leaving
    // successfully-sent MIDs in the Outbox where they would be re-offered on the
    // next connection (duplicate send). Moving them to Sent is idempotent even
    // when `result.sent` is empty (all-rejected batch); the error still surfaces.
    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &MessageId(mid.clone()))?;
    }
    if !result.rejected.is_empty() {
        return Err(BackendError::MessageRejected(format!(
            "CMS rejected mid(s): {}",
            result.rejected.join(", ")
        )));
    }
    // `shared` drops here → stream drops → DISC fires (Ax25Stream::drop).
    Ok(())
}

/// Open the KISS link, connect (dial) or answer (listen), and run the exchange.
/// Per RADIO-1, the agent never runs this against a real KISS modem — tests
/// exercise `native_packet_exchange` with `FakeAx25Stream` only.
#[allow(clippy::too_many_arguments)]
fn native_packet_connect(
    config: &Config,
    mailbox: &Arc<Mailbox>,
    link: KissLinkConfig,
    resolved: ResolvedPacket,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    abort_handle: &Mutex<Option<TcpStream>>,
    aborting: Arc<AtomicBool>,
    allowlist_override: Option<crate::winlink::listener::AllowedStations>,
) -> Result<(), BackendError> {
    let params = config.packet.params.clone().into_params();
    let locator = cms_locator(config);
    let base = resolved.base_mycall.clone();

    progress("Opening KISS link…");
    // Open the KISS link with an abort handle (tuxlink-9z2 pattern, mirroring
    // native_connect's register_socket). The TCP arm yields a try_clone'd TcpStream
    // the operator's abort() can `.shutdown()`; shutting it makes the link's read
    // return 0 (FIN), which recv_frame maps to ConnectionAborted, unwinding a blocked
    // answer()/connect() poll loop. The SERIAL arm has no socket, so it wraps the link
    // in AbortableByteLink keyed on the SAME `aborting` flag: abort() sets the flag and
    // the next serial read returns ConnectionAborted, unwinding the loop (tuxlink-nj1).
    let (bytelink, abort_socket) =
        crate::winlink::ax25::connect_link_with_abort(&link, aborting.clone())
            .map_err(|e| BackendError::TransportFailed { reason: format!("KISS link: {e}"), source: None })?;
    if let Some(sock) = abort_socket {
        // Check `aborting` INSIDE the abort_handle lock (mirrors native_connect /
        // Codex #2): abort() sets `aborting` then locks to take the socket, so doing
        // the check + store under the same lock means whichever side acquires it
        // first, the socket still ends up shut down if an abort has already fired —
        // no TOCTOU window. If an abort landed during the (un-abortable) TCP-connect
        // window, shut the socket down now so answer()/connect() fails fast.
        if let Ok(mut slot) = abort_handle.lock() {
            if aborting.load(Ordering::SeqCst) {
                let _ = sock.shutdown(Shutdown::Both);
            } else {
                *slot = Some(sock);
            }
        }
    }

    // Controller directive L: push KISS TNC params before connect/answer.
    // The straightforward approach is to call kiss_param inside the link before
    // handing it to ax25::connect/answer. However, `connect_link` returns
    // Box<dyn ByteLink> with no kiss_param accessor on the trait surface (P2's
    // ByteLink is bare Read+Write). Pushing params through the `Ax25Params`
    // passed to `connect`/`answer` is the P2 design — `connect` calls
    // `kiss_param` internally on the link for txdelay/persistence/slot_time
    // (per datalink.rs connect implementation). So no separate param-push is
    // needed here; the P2 `connect`/`answer` call owns it. Follow-up filed as
    // bd issue if P2 does NOT push params in answer() (see Task 6 commit body).

    match resolved.dial {
        Some((target, digis)) => {
            progress(&format!("Connecting to {}…", target.call));
            let stream = crate::winlink::ax25::connect(
                bytelink,
                resolved.link_mycall,
                target.clone(),
                &digis,
                &params,
            )
            .map_err(|e| BackendError::TransportFailed {
                reason: format!("AX.25 connect: {e}"),
                source: None,
            })?;
            // P1.3 (Codex post-impl review): read_password is deferred until AFTER
            // the KISS link is established. The prior placement (before connect_link)
            // caused the OS-keyring migration to run even when the link failed — e.g.
            // when unit tests intentionally use a closed loopback port, the keyring
            // write still fired. Deferring until link-up means a failed connect_arq
            // never touches the operator's keyring, and Listen arming (password: None)
            // never triggers it at all. Option (a) per the Codex review.
            let password = crate::winlink::credentials::read_password(&base)
                .ok()
                .filter(|p| !p.is_empty());
            native_packet_exchange(
                BlockingB2fStream(stream),
                PacketConnectCtx {
                    base_mycall: &base,
                    targetcall: &target.call,
                    password,
                    role: ExchangeRole::Dial,
                    locator: &locator,
                },
                mailbox,
                progress,
                wire_log,
            )
        }
        None => {
            // ── Listener-arms gate (tuxlink-inde) — armed BEFORE answer()
            //
            // Codex review 2026-06-03 [P2 — arm-time]: the original code
            // created `arms` AFTER `ax25::answer()` returned, so the TTL
            // gate was effectively a no-op (the arms record was always
            // freshly-minted at peer-receipt time, never compared against
            // the operator's true arm moment). The TTL check now meaningfully
            // expires the listener if a SABM arrives more than DEFAULT_TTL
            // after the operator armed: a peer that lands past the consent
            // window gets RejectExpired rather than silent accept.
            //
            // Reject path: drop the stream. Ax25Stream::drop fires DISC because
            // the link is established (the UA we just sent armed Drop teardown
            // via tuxlink-2y4). Reject events append to the shared forensics
            // log alongside the arm record.
            //
            // The full architecture (multi-peer continuous-armed listener with
            // shared arms record across multiple SABMs in one armed window)
            // is the follow-up; current model is one-arm one-answer cycle.
            use crate::winlink::listener::packet_gate::{
                gate_inbound_peer_now, listener_forensics_log_path, peer_id_from_ax25,
                reject_reason, ListenerRejectEvent,
            };
            use crate::winlink::listener::{
                packet_gate, AllowedStations, ListenerArmsRecord, ListenerDecision, TransportKind,
            };

            // Codex review 2026-06-03 [P2 — load-error visibility]: the
            // previous code silently substituted AllowedStations::default()
            // (allow_all=FALSE, empty list) on a corrupt-or-unreadable
            // allowlist file. The operator saw a normal "allowlist" reject
            // and couldn't tell whether the gate was working as configured
            // OR the allowlist had been wiped. We now (a) surface the load
            // error verbatim via progress(), and (b) use a distinct
            // "allowlist-load-error" reject reason so the forensics log +
            // session log clearly distinguish "configured allowlist denied
            // this peer" from "couldn't load the allowlist; failing closed."
            let mut load_failed_reason: Option<String> = None;
            let allowed = if let Some(injected) = allowlist_override.clone() {
                // Test injection (tuxlink-inde): bypasses the disk-file lookup.
                // Production never sets this; `bootstrap`/UI relies on the file.
                injected
            } else {
                match AllowedStations::load_from(&packet_gate::packet_allowed_stations_path()) {
                    Ok(a) => a,
                    Err(e) => {
                        let reason_str = format!("{e}");
                        progress(&format!(
                            "Packet allowlist load failed: {reason_str}. Failing closed (reject all inbound until repaired)."
                        ));
                        load_failed_reason = Some(reason_str);
                        AllowedStations::default()
                    }
                }
            };
            let arms = ListenerArmsRecord::arm_default(TransportKind::Packet);

            progress("Listening for an inbound peer…");
            let (peer, stream) = crate::winlink::ax25::answer(
                bytelink,
                resolved.link_mycall,
                &params,
            )
            .map_err(|e| BackendError::TransportFailed {
                reason: format!("AX.25 answer: {e}"),
                source: None,
            })?;
            progress(&format!("Answered {}.", peer.call));

            let peer_id = peer_id_from_ax25(peer.clone());
            let decision = gate_inbound_peer_now(&peer_id, &allowed, &arms);

            if decision != ListenerDecision::Accept {
                // If the gate rejected with "allowlist" AND we know the load
                // failed, upgrade the reject reason to a distinct
                // "allowlist-load-error" so the operator can distinguish.
                let reason: &str = match (&decision, &load_failed_reason) {
                    (ListenerDecision::RejectAllowlist, Some(_)) => "allowlist-load-error",
                    _ => reject_reason(&decision).unwrap_or("unknown"),
                };
                let log_path = listener_forensics_log_path();
                let event =
                    ListenerRejectEvent::new(TransportKind::Packet, reason, &peer_id);
                let _ = event.append_to_log(&log_path);

                let msg = format!(
                    "Rejected inbound from {} (reason: {}). Dropping link.",
                    peer.call, reason,
                );
                progress(&msg);

                // Drop the stream → Ax25Stream::drop sends DISC + best-effort
                // awaits UA/DM.
                drop(stream);
                return Err(BackendError::AuthFailed {
                    reason: format!(
                        "listener gate rejected inbound peer {} ({})",
                        peer.call, reason
                    ),
                });
            }

            // Listen (Answer role) does not need a password — peers do not challenge.
            // password: None is intentional; no read_password call here.
            native_packet_exchange(
                BlockingB2fStream(stream),
                PacketConnectCtx {
                    base_mycall: &base,
                    targetcall: &peer.call,
                    password: None,
                    role: ExchangeRole::Answer,
                    locator: &locator,
                },
                mailbox,
                progress,
                wire_log,
            )
        }
    }
}

/// The `BackendStatus` a packet connection STARTS in, by role (tuxlink-orj).
/// `Listen` is armed-but-idle → `Listening`; `DialTo` is an active dial →
/// `Connecting`. Pure (no I/O) so the role→status decision is unit-tested
/// without a KISS link. Set before `spawn_blocking`, it persists for the whole
/// armed wait (the ribbon polls `status()`), so an armed Listen reads honestly
/// as "Listening · Packet 1200" instead of a misleading "Connecting".
fn initial_packet_status(role: &PacketRole, ssid: u8) -> BackendStatus {
    let transport = format!("Packet-{ssid}");
    match role {
        PacketRole::Listen => BackendStatus::Listening { transport },
        PacketRole::DialTo { .. } => BackendStatus::Connecting { transport },
    }
}

/// Resolve the CMS host to dial (tuxlink-3o0). Precedence: the `TUXLINK_CMS_HOST`
/// env var wins if set (the dev escape hatch, mirroring `bin/native_cms_probe`);
/// otherwise the operator's configured `config.connect.host` is used (set via the
/// inline SettingsPanel's `config_set_connect`). This replaces the former
/// hardcoded `CMS_HOST` const fallback — the default now lives in
/// `config::default_cms_host` and reaches here through the persisted config.
fn resolve_cms_host(config: &Config) -> String {
    std::env::var("TUXLINK_CMS_HOST").unwrap_or_else(|_| config.connect.host.clone())
}

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

/// The on-air locator: delegates to [`crate::position::effective_broadcast_locator`],
/// which is the single source of truth for the on-air grid (honoring both precision
/// AND the `gps_state` privacy control). This thin wrapper exists only for callers
/// that already hold a `Config` reference and an optional arbiter in the
/// winlink_backend context.
///
/// GPS-derived positions go on air ONLY when `gps_state == BroadcastAtPrecision`;
/// under `Off` or `LocalUiOnly` the on-air locator falls back to the stored
/// config grid. A hand-set Manual grid broadcasts regardless of `gps_state`.
///
/// Currently only consumed by the in-module tests; production `native_connect`
/// calls `effective_broadcast_locator` directly. Scoped to `cfg(test)` so non-test
/// builds don't flag it as dead code. If a non-test caller appears later, drop
/// the gate.
#[cfg(test)]
fn resolve_locator(config: &Config, position: Option<&crate::position::PositionArbiter>) -> String {
    crate::position::effective_broadcast_locator(config, position)
}

/// Run one CMS exchange (blocking): build the outbox into proposals, connect over
/// the chosen transport, accept all offered messages, then file what arrived into
/// the inbox and move what was sent into the sent folder.
//
// native_connect coordinates a multi-faceted connect flow (config + mailbox +
// transport + progress/wire-log callbacks + abort plumbing + position arbiter);
// refactoring to fewer args would require introducing a builder/options struct
// that's not justified for v0.2. Tracked separately if it ever becomes load-bearing.
#[allow(clippy::too_many_arguments)]
fn native_connect(
    config: &Config,
    mailbox: &Mailbox,
    mode: CmsTransport,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
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
    // tuxlink-686 / Codex P1-A: resolve the on-air locator via the single shared
    // helper that honors BOTH precision (tuxlink-882) AND the gps_state privacy
    // control. GPS grids go on air only when gps_state == BroadcastAtPrecision;
    // Off/LocalUiOnly fall back to the config grid. Manual broadcasts regardless.
    let locator = crate::position::effective_broadcast_locator(config, position);

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

    // P1.3 (Codex post-impl review): defer read_password until after all config
    // validation and outbox-building steps have succeeded. Placing it here — just
    // before ExchangeConfig is built — ensures the OS-keyring migration only runs
    // when we are actually about to open a socket. Tests that fail in the preceding
    // steps (no callsign, mailbox errors) never touch the keyring. Option (a) per
    // the Codex review; the telnet path builds ExchangeConfig inline so "after link
    // open" translates to "after outbox build but before connect_and_exchange".
    let password = crate::winlink::credentials::read_password(&callsign)
        .ok()
        .filter(|p| !p.is_empty());

    let exchange_config = session::ExchangeConfig {
        mycall: callsign,
        targetcall: telnet::CMS_TARGET_CALL.to_string(),
        locator,
        password,
    };

    // The CMS host comes from the operator's configured `config.connect.host`
    // (tuxlink-3o0, set in the inline SettingsPanel); `TUXLINK_CMS_HOST` still
    // overrides it as a dev escape hatch. See `resolve_cms_host`.
    let host = resolve_cms_host(config);

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
        wire_log,
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

    // P1.4 (Codex post-impl review): file accepted messages FIRST, then surface
    // any rejection error. The prior ordering returned early on rejections, leaving
    // successfully-sent MIDs in the Outbox where they would be re-offered on the
    // next connection (duplicate send). Moving them to Sent is idempotent even
    // when `result.sent` is empty (all-rejected batch); the error still surfaces.
    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &MessageId(mid.clone()))?;
    }
    if !result.rejected.is_empty() {
        return Err(BackendError::MessageRejected(format!(
            "CMS rejected mid(s): {}",
            result.rejected.join(", ")
        )));
    }
    Ok(())
}

/// Run a B2F mail exchange over an already-`connect_arq`'d ARDOP transport
/// (tuxlink-ytg). The transport was spawned + ARQ-connected by
/// `modem_ardop_connect`; this function consumes it for the duration of the
/// exchange and returns it so the caller can `disconnect()` + drop it under its
/// own state machine (the Tauri command in `modem_commands.rs` resets
/// `ModemSession` after this returns).
///
/// Mirrors `native_connect`'s mailbox plumbing: builds outbound from
/// `mailbox`'s Outbox folder, runs the exchange in `Dial` role (slave/IRS —
/// the operator's send/receive against a CMS or peer), files received messages
/// into Inbox, moves sent ones from Outbox to Sent.
///
/// The transport surface is `Box<dyn ModemTransport>` so any future modem
/// (Dire Wolf, tuxmodem) that implements the same trait flows through this
/// path unchanged.
///
/// # RADIO-1
///
/// The caller MUST have consumed a per-invocation consent token before
/// invoking this function. This function does NO consent gating of its own —
/// the gate is upstream at the Tauri command boundary, where it can refuse
/// I/O / state mutation pre-gate.
pub fn run_ardop_b2f_exchange(
    transport: &mut dyn crate::winlink::modem::ModemTransport,
    target: &str,
    config: &Config,
    mailbox: &Mailbox,
    position: Option<&crate::position::PositionArbiter>,
) -> Result<(), BackendError> {
    use crate::winlink::modem::ardop::b2f;

    let callsign = config
        .identity
        .callsign
        .clone()
        .ok_or_else(|| BackendError::NotConfigured("identity.callsign".into()))?
        .trim()
        .to_uppercase();
    let locator = crate::position::effective_broadcast_locator(config, position);
    // The ARDOP B2F path connects to a CMS gateway OR a peer. Both speak the
    // same B2F protocol; only the password differs (gateway dial may carry a
    // ;PQ challenge — a peer never does). Pull the keyring entry for the
    // gateway path; if the peer never challenges, the secret is simply unused.
    // Note: this reuses the existing `tuxlink-pat` keyring entry — the cred
    // store is shared with the legacy Pat flow (per ADR 0011 cred refactor).
    let password = keyring::Entry::new("tuxlink-pat", &callsign)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|p| !p.is_empty());

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
        targetcall: target.to_string(),
        locator,
        password,
    };

    let result = b2f::run_b2f_exchange(
        transport,
        ExchangeRole::Dial,
        &exchange_config,
        outbound,
        |proposals| {
            proposals
                .iter()
                .map(|_| Answer::Accept { resume_offset: 0 })
                .collect()
        },
    )
    .map_err(|e| BackendError::TransportFailed {
        reason: format!("{e}"),
        source: None,
    })?;

    // File received messages into the inbox; move delivered ones to sent.
    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(
            MailboxFolder::Outbox,
            MailboxFolder::Sent,
            &MessageId(mid.clone()),
        )?;
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

/// Format the current wall-clock instant as an RFC 3339 / ISO-8601 UTC string
/// (`YYYY-MM-DDTHH:MM:SSZ`). Minimal epoch-based formatter. Mirrors the manual
/// formatter in `ui_commands.rs` (`format_unix_ts`) and `wizard.rs`; precision
/// is whole seconds.
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
/// `ui_commands::days_to_ymd`.
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
impl NativeBackend {
    /// In-process stub for unit tests that exercise `BackendState::install`
    /// lifecycle without touching real telnet or a real mailbox. Uses the
    /// shared `native_test_config()` helper; mailbox root is a tempdir.
    ///
    /// The tempdir is Box::leak'd so it lives for the test process's lifetime
    /// without requiring the caller to hold a TempDir handle. Tests are
    /// short-lived processes; the OS reclaims the allocation on exit.
    pub fn test_fixture() -> Self {
        let tempdir = tempfile::tempdir().unwrap();
        let leaked_path = Box::leak(Box::new(tempdir)).path().to_path_buf();
        Self::new(crate::test_helpers::native_test_config(), leaked_path)
    }
}

#[cfg(test)]
mod native_read_state_tests {
    use super::*;
    use crate::config::{
        CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PacketConfig,
        PositionPrecision, PositionSource, PrivacyConfig, CONFIG_SCHEMA_VERSION,
    };
    use crate::native_mailbox::Mailbox;
    use crate::winlink::compose::compose_message;
    use tempfile::tempdir;

    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn offline_config() -> Config {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig { connect_to_cms: false, transport: CmsTransport::Telnet, host: crate::config::default_cms_host() },
            identity: IdentityConfig { callsign: None, identifier: None, grid: None },
            privacy: PrivacyConfig {
                gps_state: GpsState::Off,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
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

    // Codex P1-A retrofit: arbiter source=Gps with no fix; gps_state=Off.
    // Old behavior (pre-P1-A): arbiter authoritative when present → return "".
    // New behavior: gps_state=Off + source=Gps → fall back to config grid regardless
    // of whether the arbiter has a fix. The GPS grid must NEVER go on air under Off.
    // cfg_with_grid uses offline_config() which has gps_state=Off.
    #[test]
    fn resolve_locator_arbiter_gps_no_fix_with_gps_off_falls_back_to_config_grid() {
        let cfg = cfg_with_grid("CN87ux"); // config has a grid; gps_state=Off
        // Arbiter with GPS source but no fix yet.
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None, // no manual grid fallback either
            PositionPrecision::FourCharGrid,
        );
        // gps_state=Off: must return config grid (precision-reduced), not "".
        assert_eq!(
            resolve_locator(&cfg, Some(&arbiter)),
            "CN87",
            "gps_state=Off with no fix: must fall back to config grid, never broadcast GPS"
        );
    }

    // Complementary: arbiter source=Gps, BroadcastAtPrecision, NO fix yet → "".
    // With BroadcastAtPrecision, we go through the arbiter path; arbiter has no
    // position → broadcast_grid() returns None → unwrap_or_default() → "".
    #[test]
    fn resolve_locator_arbiter_gps_no_fix_with_broadcast_at_precision_returns_empty() {
        let mut cfg = cfg_with_grid("CN87ux");
        cfg.privacy.gps_state = GpsState::BroadcastAtPrecision;
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        // BroadcastAtPrecision + no fix: arbiter path taken; arbiter has nothing → "".
        assert_eq!(
            resolve_locator(&cfg, Some(&arbiter)),
            "",
            "BroadcastAtPrecision with no GPS fix: arbiter returns empty (no fallback to config)"
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

    // ========================================================================
    // Codex P1-A: gps_state privacy gating — GPS grid must NEVER go on air
    // when gps_state is Off or LocalUiOnly. These tests cover resolve_locator
    // (which now delegates to effective_broadcast_locator in position/mod.rs).
    // ========================================================================

    fn cfg_with_grid_and_gps_state(grid: &str, gps_state: GpsState) -> Config {
        let mut cfg = offline_config();
        cfg.identity.grid = Some(grid.to_string());
        cfg.privacy.gps_state = gps_state;
        cfg.privacy.position_precision = PositionPrecision::FourCharGrid;
        cfg
    }

    // source=Gps + gps_state=Off + config.grid=Some("DM33") + GPS fix "CN87ux"
    // → result is the CONFIG grid ("DM33"), NOT "CN87".
    #[test]
    fn resolve_locator_gps_off_never_broadcasts_gps_grid() {
        let cfg = cfg_with_grid_and_gps_state("DM33", GpsState::Off);
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(crate::position::Fix::test("CN87ux"));
        let locator = resolve_locator(&cfg, Some(&arbiter));
        assert_eq!(
            locator, "DM33",
            "gps_state=Off: GPS fix must NOT go on air (got {locator}; expected DM33)"
        );
    }

    // source=Gps + gps_state=LocalUiOnly → config grid (no GPS on air).
    #[test]
    fn resolve_locator_gps_local_ui_only_never_broadcasts_gps_grid() {
        let cfg = cfg_with_grid_and_gps_state("DM33", GpsState::LocalUiOnly);
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(crate::position::Fix::test("CN87ux"));
        let locator = resolve_locator(&cfg, Some(&arbiter));
        assert_eq!(
            locator, "DM33",
            "gps_state=LocalUiOnly: GPS fix must NOT go on air (got {locator}; expected DM33)"
        );
    }

    // source=Gps + gps_state=BroadcastAtPrecision → the arbiter's GPS grid ("CN87").
    #[test]
    fn resolve_locator_gps_broadcast_at_precision_sends_gps_grid() {
        let cfg = cfg_with_grid_and_gps_state("DM33", GpsState::BroadcastAtPrecision);
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(crate::position::Fix::test("CN87ux"));
        let locator = resolve_locator(&cfg, Some(&arbiter));
        assert_eq!(
            locator, "CN87",
            "gps_state=BroadcastAtPrecision: live GPS grid must go on air (got {locator})"
        );
    }

    // source=Manual + gps_state=Off → arbiter's manual grid (broadcasts regardless).
    #[test]
    fn resolve_locator_manual_broadcasts_regardless_of_gps_state() {
        for gps_state in [GpsState::Off, GpsState::LocalUiOnly, GpsState::BroadcastAtPrecision] {
            let cfg = cfg_with_grid_and_gps_state("DM33", gps_state);
            let arbiter = crate::position::PositionArbiter::new(
                PositionSource::Manual,
                Some("CN87ux".into()),
                PositionPrecision::FourCharGrid,
            );
            let locator = resolve_locator(&cfg, Some(&arbiter));
            assert_eq!(
                locator, "CN87",
                "Manual source must broadcast regardless of gps_state={gps_state:?} (got {locator})"
            );
        }
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

    // tuxlink-3o0: the host resolver. Absent the TUXLINK_CMS_HOST env override,
    // the operator's configured `config.connect.host` is the dial target — the
    // default-host const is gone; the value now flows from persisted config.
    //
    // NOTE: this test deliberately does NOT read/set the TUXLINK_CMS_HOST env var
    // (process-global; would race under parallel `cargo test`). It asserts the
    // no-override branch by building a config whose host differs from any plausible
    // env value AND skipping the assertion if the env override happens to be set in
    // this process (the override-wins branch is documented, not unit-asserted, for
    // the same race reason — mirrors `resolve_cms_endpoint`'s env-free unit tests).
    #[test]
    fn resolve_cms_host_uses_configured_host_when_no_env_override() {
        if std::env::var("TUXLINK_CMS_HOST").is_ok() {
            // An override is set in this process; the config-branch is not exercised
            // here. Don't fight process-global env under parallel tests.
            return;
        }
        let mut cfg = offline_config_with_callsign();
        cfg.connect.host = "example.invalid".to_string();
        assert_eq!(
            resolve_cms_host(&cfg),
            "example.invalid",
            "with no TUXLINK_CMS_HOST override, the configured host is the dial target"
        );
    }

    // tuxlink-3o0 — THE KEY connect-exercise (operator's hard requirement). NOT a
    // shell mock: a real `TcpListener` on an ephemeral 127.0.0.1 port is dialed
    // through the SAME production code path the app uses —
    //   host      ← resolve_cms_host(&config)   (sourced from config.connect.host)
    //   transport ← resolve_cms_endpoint(Telnet) (yields Plaintext)
    //   dial      ← telnet::connect_and_exchange (the real socket open)
    // and the listener's accept() proves the dial physically connected. This proves
    // host + port + transport flow from config → a real socket.
    //
    // SAFETY (RADIO-1 / live-CMS): the target is a 127.0.0.1 listener we bind in
    // this test — NEVER a real or remote CMS. The dial host is taken from
    // `resolve_cms_host`, which we point at "127.0.0.1" via the config; the port is
    // the listener's own ephemeral port (NOT 8772/8773), so even a misconfigured
    // resolver cannot reach a real CMS from here. The fake server speaks just enough
    // of the telnet login + B2F handshake (then FQ) to let the client complete and
    // return cleanly, mirroring `telnet::tests::connects_to_a_local_mock_and_runs_an_exchange`.
    #[test]
    fn config_host_and_transport_dial_a_real_local_socket() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::time::Duration;

        // Skip if a dev override is set in this process — it would redirect the dial
        // away from our local listener (process-global env; don't fight it here).
        if std::env::var("TUXLINK_CMS_HOST").is_ok() {
            return;
        }

        // A local fake CMS on 127.0.0.1 — not the live CMS, not RF. It accepts the
        // dial, answers the telnet login, sends a B2F handshake + immediate quit (FQ),
        // then drains the client's writes until EOF so we never close mid-exchange.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let bound = listener.local_addr().unwrap();
        let (connected_tx, connected_rx) = std::sync::mpsc::channel::<std::net::SocketAddr>();
        let server = std::thread::spawn(move || {
            let (mut sock, peer) = listener.accept().unwrap();
            // Signal the test that the dial physically connected (the proof point).
            let _ = connected_tx.send(peer);
            sock.write_all(b"Callsign :\rPassword :\r[WL2K-5.0-B2FHM$]\rCMS>\rFQ\r")
                .unwrap();
            let mut buf = [0u8; 256];
            while let Ok(n) = sock.read(&mut buf) {
                if n == 0 {
                    break;
                }
            }
        });

        // Build a config whose CMS host is the loopback listener (transport Telnet =
        // plaintext, so no TLS handshake is attempted against the fake server).
        let mut cfg = offline_config_with_callsign();
        cfg.connect.host = "127.0.0.1".to_string();
        cfg.connect.transport = CmsTransport::Telnet;

        // Resolve host + transport EXACTLY as native_connect does. The host MUST come
        // from the config (no env override, guarded above); the port is the listener's
        // ephemeral port (the test's stand-in for the resolve_cms_endpoint default).
        let host = resolve_cms_host(&cfg);
        assert_eq!(host, "127.0.0.1", "dial host must be sourced from config.connect.host");
        let (_default_port, transport) = resolve_cms_endpoint(cfg.connect.transport, false, None);
        assert_eq!(
            transport,
            telnet::Transport::Plaintext,
            "Telnet transport must resolve to Plaintext"
        );

        let exchange_config = session::ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: telnet::CMS_TARGET_CALL.to_string(),
            locator: "CN87".into(),
            password: None,
        };
        let result = telnet::connect_and_exchange(
            &host,
            bound.port(),
            transport,
            &exchange_config,
            vec![],
            &|_| {},
            &|_| {},
            &|_| {},
            |_| vec![],
        )
        .expect("dial to the local listener should connect and complete a clean exchange");

        // The listener accepted a connection → the dial physically connected.
        let connected_peer = connected_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the local listener should have accepted the dial");
        assert_eq!(
            connected_peer.ip().to_string(),
            "127.0.0.1",
            "the connection must originate from loopback (never a real CMS)"
        );
        // The exchange ran to completion against the fake server (nothing to send/recv).
        assert!(result.received.is_empty());
        assert!(result.sent.is_empty());
        server.join().unwrap();
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

    // =========================================================================
    // Task 4: resolve_packet_endpoint tests (spec §4.4 identity split)
    // =========================================================================

    #[test]
    fn resolve_packet_endpoint_dial_builds_ssidd_link_addr_and_base_b2f_call() {
        // Identity split (spec §4.4): the AX.25 link addr carries the SSID; the B2F
        // identity is the BASE call. Dial role → ExchangeRole::Dial + a target.
        let resolved = resolve_packet_endpoint(
            "N7CPZ",
            7,
            PacketRole::DialTo { call: "W7AUX".into(), path: vec!["RELAY-1".into()] },
        )
        .unwrap();
        assert_eq!(resolved.link_mycall, Address { call: "N7CPZ".into(), ssid: 7 });
        assert_eq!(resolved.base_mycall, "N7CPZ");
        assert_eq!(resolved.role, ExchangeRole::Dial);
        let (target, digis) = resolved.dial.unwrap();
        assert_eq!(target, Address { call: "W7AUX".into(), ssid: 0 });
        assert_eq!(digis, vec![Address { call: "RELAY".into(), ssid: 1 }]);
    }

    #[test]
    fn resolve_packet_endpoint_listen_yields_answer_role_and_no_target() {
        let resolved = resolve_packet_endpoint("N7CPZ", 7, PacketRole::Listen).unwrap();
        assert_eq!(resolved.link_mycall, Address { call: "N7CPZ".into(), ssid: 7 });
        assert_eq!(resolved.base_mycall, "N7CPZ");
        assert_eq!(resolved.role, ExchangeRole::Answer);
        assert!(resolved.dial.is_none());
    }

    #[test]
    fn resolve_packet_endpoint_rejects_more_than_two_digipeaters() {
        let err = resolve_packet_endpoint(
            "N7CPZ",
            0,
            PacketRole::DialTo {
                call: "W7AUX".into(),
                path: vec!["A-1".into(), "B-2".into(), "C-3".into()],
            },
        )
        .unwrap_err();
        assert!(matches!(err, BackendError::NotConfigured(_)));
    }

    // =========================================================================
    // Task 5: native_packet_exchange tests
    // FakeAx25Stream: reads from inbound Cursor, writes into a shared Vec.
    // =========================================================================

    struct FakeAx25Stream {
        inbound: std::io::Cursor<Vec<u8>>,
        outbound: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    }
    impl std::io::Read for FakeAx25Stream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.inbound.read(buf)
        }
    }
    impl std::io::Write for FakeAx25Stream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.outbound.lock().expect("fake outbound").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    #[test]
    fn native_packet_exchange_dials_a_gateway_with_secure_login() {
        use crate::winlink::secure::secure_login_response;
        // A scripted gateway: speaks first, challenges, then quits (empty mailbox).
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 12345678\rCMS>\r");
        server.extend_from_slice(b"FF\r");
        let outbound_spy = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = FakeAx25Stream {
            inbound: std::io::Cursor::new(server),
            outbound: outbound_spy.clone(),
        };

        let mailbox = Mailbox::new(tempdir().unwrap().path());
        let result = native_packet_exchange(
            stream,
            PacketConnectCtx {
                base_mycall: "N7CPZ",   // base B2F call (NO ssid)
                targetcall: "W7AUX",    // target call (gateway)
                password: Some("MYPASS".into()),
                role: ExchangeRole::Dial,
                locator: "CN87",        // controller directive: pass cms_locator
            },
            &mailbox,
            &|_| {},
            &|_| {},
        );
        assert!(result.is_ok(), "gateway dial must succeed, got {result:?}");

        // The secure-login token must appear in the written bytes.
        let token = secure_login_response("12345678", "MYPASS");
        let written = outbound_spy.lock().unwrap();
        assert!(
            written.windows(token.len()).any(|w| w == token.as_bytes()),
            "the secure-login token must appear in our handshake; wrote {:?}",
            String::from_utf8_lossy(&written)
        );
    }

    #[test]
    fn native_packet_exchange_answers_a_peer_and_receives_a_message() {
        use crate::winlink::message::Message as WMessage;
        use crate::winlink::proposal::batch_checksum_line;
        use crate::winlink::transfer;

        let mut peer = Vec::new();
        peer.extend_from_slice(b";FW: W7AUX\r[RMS-1.0-B2FHM$]\rW7AUX>\r");
        let mut msg = WMessage::new();
        msg.set_header("Mid", "PEERMSG00009");
        msg.set_header("Subject", "P2P");
        msg.set_body(b"hello from the field\r\n".to_vec());
        let (proposal, compressed) = msg.to_proposal().unwrap();
        peer.extend_from_slice(proposal.line().as_bytes());
        peer.push(b'\r');
        peer.extend_from_slice(batch_checksum_line(&[proposal]).as_bytes());
        peer.push(b'\r');
        peer.extend_from_slice(&transfer::frame_block("P2P", 0, &compressed));
        peer.extend_from_slice(b"FQ\r");

        let outbound_spy = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = FakeAx25Stream {
            inbound: std::io::Cursor::new(peer),
            outbound: outbound_spy.clone(),
        };

        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let result = native_packet_exchange(
            stream,
            PacketConnectCtx {
                base_mycall: "N7CPZ",
                targetcall: "W7AUX",
                password: None,
                role: ExchangeRole::Answer,
                locator: "CN87",
            },
            &mailbox,
            &|_| {},
            &|_| {},
        );
        assert!(result.is_ok(), "answer exchange must succeed, got {result:?}");

        // The received peer message was filed into the inbox.
        let inbox = mailbox.list(MailboxFolder::Inbox).unwrap();
        assert!(
            inbox.iter().any(|m| m.id.0 == "PEERMSG00009"),
            "PEERMSG00009 must be in the inbox; got {inbox:?}"
        );
    }

    // =========================================================================
    // Task 4.3: FS-reject MIDs map to BackendError::MessageRejected
    // =========================================================================

    /// When the CMS sends `FS N` for our proposal, `ExchangeResult.rejected`
    /// contains the MID. The caller (`native_packet_exchange`) must convert that
    /// into `BackendError::MessageRejected` instead of silently succeeding.
    #[test]
    fn fs_reject_for_our_mid_maps_to_message_rejected_error() {
        use crate::winlink::message::Message as WMessage;

        // Build an outbox message so native_packet_exchange has something to
        // propose to the gateway.
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let mut msg = WMessage::new();
        msg.set_header("Mid", "REJECTME0001");
        msg.set_header("Subject", "FS-reject test");
        msg.set_body(b"Should be rejected by the gateway.\r\n".to_vec());
        mailbox
            .store(MailboxFolder::Outbox, &msg.to_bytes())
            .expect("store to outbox");

        // Scripted gateway (Dial role: gateway speaks first, no challenge):
        //   1. CMS handshake
        //   2. FS N  — reject our one proposal
        //   3. FF    — gateway has nothing to offer us
        // After FS N our remaining queue is empty and remote_no_messages=true,
        // so our next send_turn emits FQ and breaks the loop.
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r");
        server.extend_from_slice(b"FS N\r");
        server.extend_from_slice(b"FF\r");

        let outbound_spy = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = FakeAx25Stream {
            inbound: std::io::Cursor::new(server),
            outbound: outbound_spy.clone(),
        };

        let result = native_packet_exchange(
            stream,
            PacketConnectCtx {
                base_mycall: "N7CPZ",
                targetcall: "W7AUX",
                password: None,
                role: ExchangeRole::Dial,
                locator: "CN87",
            },
            &mailbox,
            &|_| {},
            &|_| {},
        );

        match result {
            Err(BackendError::MessageRejected(msg)) => {
                assert!(
                    msg.contains("REJECTME0001"),
                    "MessageRejected must contain the MID; got: {msg:?}"
                );
            }
            other => panic!("expected BackendError::MessageRejected, got {other:?}"),
        }
    }

    /// P1.4 (Codex post-impl review): in a mixed FS batch where one MID is
    /// accepted (FS Y) and another is rejected (FS N), the accepted MID must be
    /// moved to the Sent folder BEFORE `BackendError::MessageRejected` is returned.
    /// Without the fix, the early-return left accepted messages in the Outbox and
    /// they would be re-offered on the next connection (duplicate send).
    ///
    /// `fs::read_dir` enumeration order is not guaranteed, so we cannot assume
    /// which MID lands in `sent` vs `rejected`. Instead, the test asserts:
    ///   - exactly one MID ends up in `result.rejected` (the MessageRejected error)
    ///   - exactly one MID ends up in `Sent`
    ///   - they are different MIDs
    ///   - neither the sent MID nor the rejected MID remains in `Outbox`
    #[test]
    fn mixed_fs_batch_moves_accepted_mid_to_sent_before_returning_rejection_error() {
        use crate::winlink::message::Message as WMessage;

        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        // Two outbox messages. FS YN accepts whichever is enumerated first and
        // rejects whichever is enumerated second. We don't control the order.
        let mut msg1 = WMessage::new();
        msg1.set_header("Mid", "MIXED00000A");
        msg1.set_header("Subject", "Msg A");
        msg1.set_body(b"Body A.\r\n".to_vec());
        mailbox.store(MailboxFolder::Outbox, &msg1.to_bytes()).expect("store msg A");

        let mut msg2 = WMessage::new();
        msg2.set_header("Mid", "MIXED00000B");
        msg2.set_header("Subject", "Msg B");
        msg2.set_body(b"Body B.\r\n".to_vec());
        mailbox.store(MailboxFolder::Outbox, &msg2.to_bytes()).expect("store msg B");

        // Scripted gateway (Dial role): `FS YN` — first proposal accepted, second rejected.
        // Filesystem enumeration order determines which MID is "first"; both orderings
        // are valid inputs for this test — the property we check holds regardless.
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r");
        server.extend_from_slice(b"FS YN\r");
        server.extend_from_slice(b"FF\r");

        let outbound_spy = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = FakeAx25Stream {
            inbound: std::io::Cursor::new(server),
            outbound: outbound_spy.clone(),
        };

        let result = native_packet_exchange(
            stream,
            PacketConnectCtx {
                base_mycall: "N7CPZ",
                targetcall: "W7AUX",
                password: None,
                role: ExchangeRole::Dial,
                locator: "CN87",
            },
            &mailbox,
            &|_| {},
            &|_| {},
        );

        // Must return Err(MessageRejected) containing exactly one MID.
        let rejected_mid = match result {
            Err(BackendError::MessageRejected(ref msg)) => {
                // Extract the one rejected MID from the error string.
                let candidates = ["MIXED00000A", "MIXED00000B"];
                let found: Vec<&str> = candidates.iter().copied()
                    .filter(|m| msg.contains(m))
                    .collect();
                assert_eq!(found.len(), 1,
                    "MessageRejected must name exactly one of our two MIDs; got: {msg:?}");
                found[0].to_string()
            }
            other => panic!("expected BackendError::MessageRejected, got {other:?}"),
        };

        let accepted_mid = if rejected_mid == "MIXED00000A" { "MIXED00000B" } else { "MIXED00000A" };

        // The accepted MID must be in Sent — NOT left in Outbox.
        let sent = mailbox.list(MailboxFolder::Sent).unwrap();
        assert!(
            sent.iter().any(|m| m.id.0 == accepted_mid),
            "accepted MID ({accepted_mid}) must be in Sent folder; sent: {sent:?}"
        );
        let outbox = mailbox.list(MailboxFolder::Outbox).unwrap();
        assert!(
            !outbox.iter().any(|m| m.id.0 == accepted_mid),
            "accepted MID ({accepted_mid}) must NOT remain in Outbox; outbox: {outbox:?}"
        );

        // The rejected MID must NOT be in Sent.
        assert!(
            !sent.iter().any(|m| m.id.0 == rejected_mid),
            "rejected MID ({rejected_mid}) must NOT be in Sent folder; sent: {sent:?}"
        );
    }

    // =========================================================================
    // Task 6: packet lifecycle branch selection + no-link fast-fail
    // =========================================================================

    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn offline_config_with_callsign() -> Config {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig { connect_to_cms: true, transport: CmsTransport::Telnet, host: crate::config::default_cms_host() },
            identity: IdentityConfig { callsign: Some("N7CPZ".into()), identifier: None, grid: None },
            privacy: PrivacyConfig {
                gps_state: GpsState::Off,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
        }
    }

    #[tokio::test]
    async fn connect_packet_with_no_reachable_link_is_transport_failed() {
        // A NativeBackend with a callsign set but a KISS link that no listener is on.
        // connect_link fails fast (connection refused) → TransportFailed.
        // Per RADIO-1: we use a definitely-closed loopback port (bind then drop).
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // nothing listening → connection refused

        let backend = NativeBackend::new(offline_config_with_callsign(), tempdir().unwrap().path());
        let err = backend
            .connect(TransportConfig::Packet {
                link: KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() },
                ssid: 7,
                role: PacketRole::DialTo { call: "W7AUX".into(), path: vec![] },
            })
            .await
            .unwrap_err();
        assert!(
            matches!(err, BackendError::TransportFailed { .. }),
            "expected TransportFailed, got {err:?}"
        );
    }

    // tuxlink-ka7 / tuxlink-p5u: a config change via `set_config` must reach the
    // LIVE backend so the NEXT connect honors it WITHOUT an app restart. Regression
    // guard for the "selector host/transport (and packet params) only apply after
    // restart" bug — the backend cached `config` at construction and the connect
    // path read that stale snapshot. Proven via the shared callsign gate (BOTH
    // native_connect and packet_connect_inner reject a missing callsign FIRST):
    // start with NO callsign, refresh to one WITH a callsign, then connect — a
    // stale snapshot fails NotConfigured(callsign); a live snapshot gets PAST that
    // gate and fails at link-open (TransportFailed). No RF, no real CMS (RADIO-1):
    // a closed loopback port refuses the connect fast.
    #[tokio::test]
    async fn set_config_refreshes_the_live_config_used_by_connect() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // nothing listening → connection refused

        // Construct with NO callsign (the stale snapshot the bug would freeze in).
        let backend = NativeBackend::new(offline_config(), tempdir().unwrap().path());
        // Operator picks a callsign in the UI → config_set_* persists + refreshes.
        backend.set_config(config_with_call("N7CPZ"));

        let err = backend
            .connect(TransportConfig::Packet {
                link: KissLinkConfig::Tcp { host: addr.ip().to_string(), port: addr.port() },
                ssid: 7,
                role: PacketRole::DialTo { call: "W7AUX".into(), path: vec![] },
            })
            .await
            .unwrap_err();

        assert!(
            !matches!(&err, BackendError::NotConfigured(field) if field.contains("callsign")),
            "connect must read the LIVE config (callsign set via set_config), not the \
             construction-time snapshot; got {err:?}"
        );
        assert!(
            matches!(err, BackendError::TransportFailed { .. }),
            "with a live callsign, connect should reach link-open and fail \
             TransportFailed; got {err:?}"
        );
    }

    #[test]
    fn packet_dial_selects_dial_role_and_listen_selects_answer_role() {
        assert_eq!(
            resolve_packet_endpoint("N7CPZ", 7, PacketRole::DialTo { call: "W7AUX".into(), path: vec![] })
                .unwrap()
                .role,
            ExchangeRole::Dial
        );
        assert_eq!(
            resolve_packet_endpoint("N7CPZ", 7, PacketRole::Listen).unwrap().role,
            ExchangeRole::Answer
        );
    }

    // tuxlink-orj: arming Listen must report Listening (armed, waiting for an
    // inbound call), NOT Connecting (which implies an active dial). This is the
    // honest-state fix — the prior code set Connecting for both roles, so the UI
    // refused to trust it and hard-coded "not connected".
    #[test]
    fn listen_role_initial_status_is_listening_not_connecting() {
        assert!(matches!(
            initial_packet_status(&PacketRole::Listen, 7),
            BackendStatus::Listening { transport } if transport == "Packet-7"
        ));
    }

    #[test]
    fn dial_role_initial_status_is_connecting() {
        assert!(matches!(
            initial_packet_status(
                &PacketRole::DialTo { call: "W7AUX".into(), path: vec![] },
                3
            ),
            BackendStatus::Connecting { transport } if transport == "Packet-3"
        ));
    }

    // =========================================================================
    // tuxlink-3wh: REAL end-to-end integration chain (no mocks, no RF).
    //
    // Two production NativeBackend instances connect to EACH OTHER over a real
    // TCP socket pair. One runs Listen (Answer role = FBB master), the other
    // DialTo (Dial role = slave/dialer). Every layer is the shipping code:
    // connect_link (real TcpStream) -> KISS framing -> AX.25 SABM/UA connect ->
    // Ax25Stream ARQ -> B2F run_exchange_with_role. The only non-tuxlink piece
    // is `kiss_wire`, a transparent byte relay that stands in for the
    // TNC->RF->TNC path (the TNC is transparent to AX.25 frames above the KISS
    // boundary, and RADIO-1 bars us from running the RF PHY anyway). 127.0.0.1
    // only; nothing is transmitted.
    // =========================================================================

    /// A transparent KISS byte-wire: accepts the two backends' TCP connections
    /// and cross-pipes their bytes, exactly as a TNC+RF+TNC link would carry the
    /// AX.25 frames between two hosts. Returns the address both peers dial.
    fn spawn_kiss_wire() -> std::net::SocketAddr {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let peer_a = match listener.accept() {
                Ok((s, _)) => s,
                Err(_) => return,
            };
            let peer_b = match listener.accept() {
                Ok((s, _)) => s,
                Err(_) => return,
            };
            let a_rd = peer_a.try_clone().unwrap();
            let mut a_wr = peer_a;
            let b_rd = peer_b.try_clone().unwrap();
            let mut b_wr = peer_b;
            let t1 = std::thread::spawn(move || {
                let mut r = a_rd;
                let _ = std::io::copy(&mut r, &mut b_wr);
            });
            let t2 = std::thread::spawn(move || {
                let mut r = b_rd;
                let _ = std::io::copy(&mut r, &mut a_wr);
            });
            let _ = t1.join();
            let _ = t2.join();
        });
        addr
    }

    fn config_with_call(call: &str) -> Config {
        let mut cfg = offline_config();
        cfg.identity.callsign = Some(call.to_string());
        cfg.identity.grid = Some("CN87".to_string());
        cfg
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn packet_two_real_peers_complete_a_connect_and_b2f_over_tcp_kiss() {
        let wire = spawn_kiss_wire();

        // Dialer (N7CPZ-7) has one outbound message; answerer (W7AUX-7) listens.
        let dialer_dir = tempdir().unwrap();
        let answerer_dir = tempdir().unwrap();
        let seed = Mailbox::new(dialer_dir.path());
        let raw =
            compose_message("N7CPZ", &["W7AUX"], &[], "AX25-E2E", "hello over packet", 1_716_200_000)
                .to_bytes();
        seed.store(MailboxFolder::Outbox, &raw).unwrap();

        let dialer = NativeBackend::new(config_with_call("N7CPZ"), dialer_dir.path());
        // The answerer's listener gate (tuxlink-inde) defaults to "reject all"
        // — fresh tuxlink with no operator-curated allowlist rejects every
        // inbound peer. For this happy-path E2E test we inject an
        // allow_all=TRUE list so the dialer's N7CPZ-7 SABM is accepted.
        let answerer = NativeBackend::new(config_with_call("W7AUX"), answerer_dir.path())
            .with_packet_allowlist(
                crate::winlink::listener::AllowedStations::new().with_allow_all(true),
            );

        let listen = TransportConfig::Packet {
            link: KissLinkConfig::Tcp { host: wire.ip().to_string(), port: wire.port() },
            ssid: 7,
            role: PacketRole::Listen,
        };
        let dial = TransportConfig::Packet {
            link: KissLinkConfig::Tcp { host: wire.ip().to_string(), port: wire.port() },
            ssid: 7,
            role: PacketRole::DialTo { call: "W7AUX-7".into(), path: vec![] },
        };

        // Watchdog: a handshake/connect deadlock must fail the test, not hang cargo.
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            tokio::join!(answerer.connect(listen), dialer.connect(dial))
        })
        .await;

        let (ans_res, dial_res) =
            outcome.expect("end-to-end packet exchange timed out (connect/handshake deadlock?)");
        ans_res.expect("answerer (Listen/Answer role) connect+exchange failed");
        dial_res.expect("dialer (DialTo/Dial role) connect+exchange failed");

        // The dialer's outbound message must have crossed the real TCP+KISS+AX.25
        // wire into the answerer's inbox (proves the full chain ran).
        let inbox = Mailbox::new(answerer_dir.path()).list(MailboxFolder::Inbox).unwrap();
        assert_eq!(
            inbox.len(),
            1,
            "answerer inbox should hold the one message that crossed the wire; got {inbox:?}"
        );
        // ...and the dialer must have filed it as Sent (proves the proposal was acked).
        let sent = Mailbox::new(dialer_dir.path()).list(MailboxFolder::Sent).unwrap();
        assert_eq!(sent.len(), 1, "dialer Sent should hold the acked message; got {sent:?}");
    }

    // A reader that mimics Ax25Stream's defect-J behaviour: it returns Ok(0) for
    // "no data yet" `idle` times (link open), then delivers `payload` once, then
    // reports closed so a further Ok(0) is a genuine EOF.
    struct IdleThenData {
        idle_left: usize,
        payload: Vec<u8>,
        delivered: bool,
    }
    impl std::io::Read for IdleThenData {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.idle_left > 0 {
                self.idle_left -= 1;
                return Ok(0); // no data yet — link still open
            }
            if !self.delivered {
                self.delivered = true;
                let n = buf.len().min(self.payload.len());
                buf[..n].copy_from_slice(&self.payload[..n]);
                return Ok(n);
            }
            Ok(0) // no more data; is_closed() is now true → genuine EOF
        }
    }
    impl MaybeClosed for IdleThenData {
        fn is_closed(&self) -> bool {
            self.delivered
        }
    }

    #[test]
    fn blocking_b2f_stream_loops_past_transient_ok0_then_reports_eof() {
        // Regression for the Codex 2026-05-22 BLOCKER: a transient Ok(0) from
        // Ax25Stream (no data yet, link open) must NOT be read as EOF by the B2F
        // BufReader; the adapter loops until real data, then surfaces a closed-link
        // Ok(0) as a genuine EOF.
        let mut s = BlockingB2fStream(IdleThenData {
            idle_left: 3,
            payload: b"FF\r".to_vec(),
            delivered: false,
        });
        let mut buf = [0u8; 8];
        let n = std::io::Read::read(&mut s, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"FF\r", "must block through transient Ok(0), not EOF early");
        let n2 = std::io::Read::read(&mut s, &mut buf).unwrap();
        assert_eq!(n2, 0, "Ok(0) while the link is closed must surface as a real EOF");
    }
}
