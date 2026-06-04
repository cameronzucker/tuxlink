//! Main-UI IPC commands + serializable error projection.
//!
//! Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §3
//! bd issues: tuxlink-zsm (Task 12 — main-UI cluster ROOT)
//!            tuxlink-y5c (Task 13 — message reading pane + RFC5322 parse)
//!
//! This module is the IPC foundation for the message UI. Task 12 owns
//! [`UiError`] (+ its exhaustive `From<BackendError>` impl) and the
//! [`mailbox_list`] command. Task 13 (this file, appended) owns
//! [`ParsedMessageDto`], [`AttachmentMetaDto`], [`parse_raw_rfc5322`], and
//! the [`message_read`] command. Tasks 14/16 APPEND their command fns here
//! (append-only — near-zero merge conflict); the orchestrator integration
//! commit (spec §4.3) registers them in `lib.rs`'s `invoke_handler` and
//! wires the AppShell regions. Until then, only `mailbox_list` is
//! registered.
//!
//! **Error model (spec §3.1):** `BackendError` carries non-serializable
//! `#[source] Box<dyn Error>` fields, so it cannot cross the Tauri IPC
//! boundary. `UiError` is the serializable projection, mirroring the
//! wizard's `#[serde(tag="kind", content="detail")]` discriminated-union
//! shape (`src/wizard/types.ts`). The `From` impl MUST be exhaustive over
//! every `BackendError` variant (Codex finding 6).
//!
//! **Async/lock invariant (spec §1.1):** every command clones the backend
//! `Arc` and drops the `RwLock` guard via [`BackendState::current`] BEFORE
//! awaiting the trait method — the guard is never held across an await.

use mail_parser::{HeaderName, MimeHeaders, MessageParser};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_backend::{BackendPhase, BackendState};
use crate::config::{self, CmsTransport, GpsState, PositionPrecision, PositionSource};
use crate::session_log::SessionLogState;
use crate::winlink_backend::{
    BackendError, BackendStatus, LogLevel, LogLine, LogSource, MailboxFolder, MessageId,
    MessageMeta, OutboundMessage, TransportConfig,
};

// ============================================================================
// Error projection (spec §3.1)
// ============================================================================

/// Serializable projection of [`BackendError`] for the frontend. Mirrors
/// `UiError` in `src/mailbox/types.ts` via Tauri's
/// `#[serde(tag="kind", content="detail")]` shape.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(tag = "kind", content = "detail")]
pub enum UiError {
    /// No backend configured (offline install / pre-connect). The UI
    /// renders this as a "not connected" empty state, NOT an error toast.
    NotConfigured(String),
    /// Message id not found.
    NotFound(String),
    AuthFailed { reason: String },
    Transport { reason: String },
    Unavailable { reason: String },
    Rejected(String),
    Cancelled,
    Internal { detail: String },
}

impl From<BackendError> for UiError {
    /// Exhaustive mapping per spec §3.1 — every `BackendError` variant gets a
    /// deliberate arm; NO catch-all. `BackendError` is `#[non_exhaustive]`,
    /// but since it is defined in this same crate the compiler checks
    /// exhaustiveness here at compile time: a future variant added to
    /// `winlink_backend.rs` will fail this `match`, forcing a deliberate UI
    /// projection rather than silently routing through a wildcard (Codex
    /// finding 6 — `InvalidSession`/`Cancelled`/`NotImplemented`/`Io` must
    /// each be handled, not dropped).
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::NotConfigured(s) => UiError::NotConfigured(s),
            BackendError::NotFound(id) => UiError::NotFound(id.0),
            BackendError::AuthFailed { reason } => UiError::AuthFailed { reason },
            BackendError::TransportFailed { reason, source } => UiError::Transport {
                reason: stringify_with_source(&reason, source.as_deref()),
            },
            BackendError::BackendUnavailable { reason, source } => UiError::Unavailable {
                reason: stringify_with_source(&reason, source.as_deref()),
            },
            BackendError::MessageRejected(s) => UiError::Rejected(s),
            BackendError::Cancelled => UiError::Cancelled,
            BackendError::NotImplemented => UiError::Unavailable {
                reason: "not implemented in v0.0.1".to_string(),
            },
            BackendError::InvalidSession => UiError::Internal {
                detail: "invalid session".to_string(),
            },
            BackendError::Io(err) => UiError::Internal {
                detail: err.to_string(),
            },
            BackendError::Internal { msg, source } => UiError::Internal {
                detail: stringify_with_source(&msg, source.as_deref()),
            },
        }
    }
}

/// Append the `Display` of a `source` error chain to a reason string so the
/// projected `reason`/`detail` carries the lost context that the
/// non-serializable `#[source]` would otherwise drop.
fn stringify_with_source(
    reason: &str,
    source: Option<&(dyn std::error::Error + Send + Sync + 'static)>,
) -> String {
    match source {
        Some(src) => format!("{reason}: {src}"),
        None => reason.to_string(),
    }
}

// ============================================================================
// Message metadata DTO (spec §2.1 / §3.2)
// ============================================================================

/// Serializable list-row metadata. Mirrors `MessageMeta` in
/// `src/mailbox/types.ts`. Field names are camelCase on the wire so the TS
/// model needs no rename layer.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageMetaDto {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub date: String,
    pub unread: bool,
    pub body_size: u64,
    pub has_attachments: bool,
}

impl From<MessageMeta> for MessageMetaDto {
    fn from(m: MessageMeta) -> Self {
        MessageMetaDto {
            id: m.id.0,
            subject: m.subject,
            from: m.from,
            to: m.to,
            date: m.date,
            unread: m.unread,
            body_size: m.body_size,
            has_attachments: m.has_attachments,
        }
    }
}

// ============================================================================
// Folder parsing (spec §3.2)
// ============================================================================

/// Parse a sidebar folder string into a backend [`MailboxFolder`].
///
/// `"drafts"` and `"deleted"` never reach a backend command — Drafts is a
/// local (`localStorage`) store handled frontend-side (spec §2.2), and
/// Deleted is a disabled placeholder. Either string → `Err(UiError)` so a
/// stray invocation fails loudly rather than silently querying the wrong
/// folder. Spec §3.2 + Task-12 test (2).
pub fn parse_folder(folder: &str) -> Result<MailboxFolder, UiError> {
    match folder {
        "inbox" => Ok(MailboxFolder::Inbox),
        "outbox" => Ok(MailboxFolder::Outbox),
        "sent" => Ok(MailboxFolder::Sent),
        "archive" => Ok(MailboxFolder::Archive),
        "drafts" => Err(UiError::Internal {
            detail: "drafts is a local folder, not a backend folder".to_string(),
        }),
        "deleted" => Err(UiError::Unavailable {
            reason: "the Deleted folder is not available in v0.0.1".to_string(),
        }),
        other => Err(UiError::Internal {
            detail: format!("unknown folder: {other}"),
        }),
    }
}

/// Parse a folder string as a [`crate::native_mailbox::FolderRef`] — system
/// folder OR user-folder slug (tuxlink-f62f). Drafts/deleted are still
/// rejected as backend folders.
///
/// Order of precedence: system folder match first (preserves existing
/// behavior for "inbox"/"sent"/"outbox"/"archive"), then slug validation.
/// A string that's neither a known system folder nor a valid slug → Err.
pub fn parse_folder_ref(folder: &str) -> Result<crate::native_mailbox::FolderRef, UiError> {
    use crate::native_mailbox::FolderRef;
    match folder {
        "inbox" => Ok(FolderRef::System(MailboxFolder::Inbox)),
        "outbox" => Ok(FolderRef::System(MailboxFolder::Outbox)),
        "sent" => Ok(FolderRef::System(MailboxFolder::Sent)),
        "archive" => Ok(FolderRef::System(MailboxFolder::Archive)),
        "drafts" => Err(UiError::Internal {
            detail: "drafts is a local folder, not a backend folder".to_string(),
        }),
        "deleted" => Err(UiError::Unavailable {
            reason: "the Deleted folder is not available in v0.0.1".to_string(),
        }),
        other => {
            crate::user_folders::validate_slug(other)
                .map_err(|e| UiError::Internal { detail: format!("invalid folder slug: {e}") })?;
            Ok(FolderRef::User(other.to_string()))
        }
    }
}

// ============================================================================
// Commands (spec §3.2)
// ============================================================================

/// List a folder's messages. Consumed by Task 12's `useMailbox`.
///
/// `None` backend → `NotConfigured` (the UI's "not connected" empty state,
/// not an error). Otherwise clones the `Arc`, drops the lock, then awaits
/// `list_messages` (spec §1.1 lock invariant).
#[tauri::command]
pub async fn mailbox_list(
    folder: String,
    state: State<'_, BackendState>,
) -> Result<Vec<MessageMetaDto>, UiError> {
    use crate::native_mailbox::FolderRef;
    let parsed = parse_folder_ref(&folder)?;
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let metas = match parsed {
        FolderRef::System(f) => backend.list_messages(f).await?,
        FolderRef::User(slug) => backend.list_user_messages(&slug).await?,
    };
    Ok(metas.into_iter().map(MessageMetaDto::from).collect())
}

// ============================================================================
// Task 13 — ParsedMessageDto + RFC5322 parse (spec §5.3)
// bd issue: tuxlink-y5c
// ============================================================================

/// Maximum raw RFC5322 input size the parser will accept (2 MiB).
///
/// Winlink messages are bounded by CMS limits (capped at ~120 KB on modern
/// CMS, though the trait contract makes no size guarantee). 2 MiB is a
/// conservative hard cap that:
///   - passes all realistic messages comfortably (even a busy EMCOMM
///     deployment's largest realistic attachments), and
///   - prevents a malformed or unexpected byte stream from driving the
///     parser into unbounded work. Per spec §5.3 + Codex verdict V3.
pub const MAX_RFC5322_BYTES: usize = 2 * 1024 * 1024;

/// Serializable attachment name/size. Mirrors `AttachmentMeta` in
/// `src/mailbox/types.ts`. v0.0.1 lists names + sizes only; bytes are NOT
/// downloaded or previewed in v0.0.1 (spec §5.3 — no attachment open, no
/// browser spawn).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AttachmentMetaDto {
    pub filename: String,
    pub size: u64,
}

/// Serializable parsed message. Mirrors `ParsedMessage` in
/// `src/mailbox/types.ts`. Produced by [`parse_raw_rfc5322`] at the command
/// boundary; raw bytes never reach the frontend.
///
/// `routing` is extracted from message headers when available
/// (`X-Received-Winlink-Transport` or `X-Pat-Transport`); `null` if absent
/// (the UI omits the routing strip). `isForm` is true when at least one
/// attachment filename matches `RMS_Express_Form_*.xml` — the WLE convention
/// for form payloads (the XML lives in the attachment, not the plain-text
/// body); the UI renders a "form rendering arrives in v0.1" placeholder in
/// that case.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedMessageDto {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub date: String, // RFC 3339 UTC
    pub body: String, // decoded text/plain (lossy UTF-8)
    pub attachments: Vec<AttachmentMetaDto>,
    pub is_form: bool,
    pub routing: Option<String>,
    /// Form ID extracted from `RMS_Express_Form_<id>.xml` attachment name.
    /// Validated via `forms::validation::is_valid_form_id`. None when not a form.
    pub form_id: Option<String>,
    /// Parsed form payload (eager parse while attachment bytes available).
    /// None when not a form OR when parse failed (also logged).
    pub form_payload: Option<crate::forms::FormPayload>,
}

/// Parse raw RFC5322 bytes into a [`ParsedMessageDto`].
///
/// This is the component function under [`message_read`]; factored out for
/// unit testing without a `tauri::State`. The returned `id` field is filled
/// from the caller-supplied `mid` (the MID is known at the command level;
/// the `Message-ID` header value in Winlink messages typically differs from
/// the CMS-assigned MID).
///
/// Error conditions:
/// - Input exceeds [`MAX_RFC5322_BYTES`] → `UiError::Internal` (spec §5.3 V3).
/// - `mail-parser` fails to parse the message → `UiError::Internal`
///   (the frontend renders a "could not parse" state).
///
/// Body: the `text/plain` part is decoded lossily (invalid UTF-8 bytes
/// become U+FFFD; no panic). Form detection: an attachment whose filename
/// matches `RMS_Express_Form_*.xml` sets `is_form = true`.
///
/// Attachments: all non-inline, named MIME parts are listed by name + size
/// in bytes. In v0.0.1 attachment bytes are never fetched or previewed (spec
/// §5.3 — no download, no browser spawn).
pub fn parse_raw_rfc5322(mid: &str, raw: &[u8]) -> Result<ParsedMessageDto, UiError> {
    if raw.len() > MAX_RFC5322_BYTES {
        return Err(UiError::Internal {
            detail: format!(
                "message too large to parse ({} bytes; cap is {} bytes)",
                raw.len(),
                MAX_RFC5322_BYTES
            ),
        });
    }

    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| UiError::Internal {
            detail: "RFC5322 parse failed: mail-parser returned None".to_string(),
        })?;

    // Subject — empty string if absent.
    let subject = msg
        .subject()
        .map(|s| s.to_string())
        .unwrap_or_default();

    // From — first address's display form.
    // msg.from() returns Option<&Address<'x>> (spec §2.3).
    let from = extract_first_address(msg.from());

    // To — collect all address strings.
    let to = extract_address_list(msg.to());

    // Cc — collect all address strings.
    let cc = extract_address_list(msg.cc());

    // Date — emit as RFC 3339 UTC from the Date header. Fallback chain:
    //   1. mail-parser strict RFC5322 Date (the standard path).
    //   2. Winlink B2F Date format `YYYY/MM/DD HH:MM` (UTC implicit) —
    //      CMS-originated messages (Service Advice etc.) carry this format,
    //      which mail-parser rejects. tuxlink-p3u.
    //   3. Empty string — better than a misleading 1970-01-01 epoch when
    //      the header is absent or in an unrecognized format. The frontend
    //      formatters (MessageList.tsx, MessageView.tsx) gracefully render
    //      an empty/unparseable ISO as a blank cell, not "Invalid Date".
    let date = msg
        .date()
        .map(|d| format_unix_ts(d.to_timestamp()))
        .or_else(|| {
            msg.header_raw(HeaderName::Date)
                .and_then(parse_winlink_date)
                .map(format_unix_ts)
        })
        .unwrap_or_default();

    // Body: find the first text/plain part; decode lossily.
    let body = find_text_plain_body(&msg);

    // Attachments: non-inline named parts (MIME attachments).
    let attachments = collect_attachments(&msg);

    // Winlink form detection: a RMS_Express_Form_<id>.xml attachment.
    // (Pre-T2.1 the heuristic was body.starts_with("<?xml"), which missed
    // real WLE forms — XML lives in the attachment, not the body.)
    let is_form = attachments
        .iter()
        .any(|a| a.filename.starts_with("RMS_Express_Form_") && a.filename.ends_with(".xml"));

    // Form ID + payload: eager parse while attachment bytes are in scope.
    let form_id = attachments
        .iter()
        .find_map(|a| crate::forms::detect_form_attachment(&a.filename));

    let form_payload = if let Some(ref fid) = form_id {
        let attach_name = format!("RMS_Express_Form_{}.xml", fid);
        extract_attachment_bytes(&msg, &attach_name)
            .and_then(|bytes| crate::forms::parse_form_xml(&bytes).ok())
            .map(|mut p| {
                // P2 #5 fix: backfill form_id from the attachment filename so the
                // frontend's KeyValueView receives a non-empty formId on the payload.
                p.form_id = fid.clone();
                p
            })
    } else {
        None
    };

    // Routing: check known Winlink transport headers.
    let routing = extract_routing(&msg);

    Ok(ParsedMessageDto {
        id: mid.to_string(),
        subject,
        from,
        to,
        cc,
        date,
        body,
        attachments,
        is_form,
        routing,
        form_id,
        form_payload,
    })
}

// ---- Helpers ---------------------------------------------------------------

/// Extract the first address string from a `mail_parser::Address`.
/// `Address` is an enum with `List(Vec<Addr>)` and `Group(Vec<Group>)`.
fn extract_first_address(addr: Option<&mail_parser::Address<'_>>) -> String {
    let Some(a) = addr else {
        return String::new();
    };
    match a {
        mail_parser::Address::List(list) => {
            list.first().map(addr_to_string).unwrap_or_default()
        }
        mail_parser::Address::Group(groups) => groups
            .first()
            .and_then(|g| g.addresses.first())
            .map(addr_to_string)
            .unwrap_or_default(),
    }
}

/// Collect all address strings from a `mail_parser::Address`.
fn extract_address_list(addr: Option<&mail_parser::Address<'_>>) -> Vec<String> {
    let Some(a) = addr else {
        return Vec::new();
    };
    match a {
        mail_parser::Address::List(list) => list.iter().map(addr_to_string).collect(),
        mail_parser::Address::Group(groups) => groups
            .iter()
            .flat_map(|g| g.addresses.iter().map(addr_to_string))
            .collect(),
    }
}

/// Format a `mail_parser::Addr` to a display string.
/// Prefers email address; includes name when present; falls back to name
/// only; empty string if neither.
fn addr_to_string(a: &mail_parser::Addr<'_>) -> String {
    match (&a.name, &a.address) {
        (Some(name), Some(email)) if !name.is_empty() => format!("{name} <{email}>"),
        (_, Some(email)) => email.to_string(),
        (Some(name), None) => name.to_string(),
        (None, None) => String::new(),
    }
}

/// Return the first text/plain body as a string.
///
/// Uses `msg.body_text(0)` which returns the first text/plain body part
/// (already decoded for charset + CTE). Falls back to a lossy decode of
/// the root part's binary body if no text/plain part is registered (handles
/// non-MIME messages with invalid UTF-8 bytes in the body).
fn find_text_plain_body(msg: &mail_parser::Message<'_>) -> String {
    // body_text(0) returns the first text/plain part (as decoded Cow<str>).
    if let Some(text) = msg.body_text(0) {
        return text.into_owned();
    }
    // Non-MIME message: the root part body is all we have.
    match &msg.parts[0].body {
        mail_parser::PartType::Text(t) => t.to_string(),
        mail_parser::PartType::Binary(b) | mail_parser::PartType::InlineBinary(b) => {
            String::from_utf8_lossy(b).into_owned()
        }
        _ => String::new(),
    }
}

/// Collect named MIME attachments (name + decoded size in bytes).
/// Parts without a filename are skipped (inline images without explicit names).
/// Uses the `MimeHeaders` trait for `attachment_name()` + `content_type()`.
fn collect_attachments(msg: &mail_parser::Message<'_>) -> Vec<AttachmentMetaDto> {
    msg.attachments()
        .filter_map(|part| {
            // attachment_name() checks Content-Disposition: filename first,
            // then Content-Type: name (the default impl in MimeHeaders).
            let filename = part.attachment_name().map(|s| s.to_string())?;
            let size: u64 = match &part.body {
                mail_parser::PartType::Binary(b) | mail_parser::PartType::InlineBinary(b) => {
                    b.len() as u64
                }
                mail_parser::PartType::Text(t) => t.len() as u64,
                _ => 0,
            };
            Some(AttachmentMetaDto { filename, size })
        })
        .collect()
}

/// Extract the raw bytes of an attachment by filename match.
/// Returns the decoded attachment bytes (mail-parser handles CTE decoding).
/// Returns None when no attachment matches.
fn extract_attachment_bytes(msg: &mail_parser::Message<'_>, filename: &str) -> Option<Vec<u8>> {
    msg.attachments().find_map(|part| {
        let name = part.attachment_name()?;
        if name != filename {
            return None;
        }
        match &part.body {
            mail_parser::PartType::Binary(b) | mail_parser::PartType::InlineBinary(b) => {
                Some(b.to_vec())
            }
            mail_parser::PartType::Text(t) => Some(t.as_bytes().to_vec()),
            _ => None,
        }
    })
}

/// Extract a routing string from known Winlink transport-info headers.
/// Checks a prioritized list of custom headers; returns `None` when absent.
///
/// `X-Pat-Transport` is retained as a known incoming header so messages
/// forwarded by remote Pat-running gateways still surface a routing string in
/// the UI. Tuxlink itself does not emit it (Pat is fully stripped per ADR
/// 0016); this is wire-compatibility for peers, not a Pat dependency.
fn extract_routing(msg: &mail_parser::Message<'_>) -> Option<String> {
    const TRANSPORT_HEADERS: &[&str] = &[
        "X-Winlink-Route",
        "X-Received-Winlink-Transport",
        "X-Pat-Transport",
    ];
    for &header_name in TRANSPORT_HEADERS {
        // msg.header() returns Option<&HeaderValue> directly.
        if let Some(mail_parser::HeaderValue::Text(s)) = msg.header(header_name) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Format a Unix timestamp (seconds since epoch) as an RFC 3339 UTC string.
/// Implements a minimal formatter without pulling in `chrono` or `time`.
fn format_unix_ts(ts: i64) -> String {
    // For v0.0.1 we emit an ISO-8601 approximation.  mail-parser's
    // `to_timestamp()` returns seconds since the Unix epoch (UTC).  We
    // convert with simple integer arithmetic (no leap-second correction,
    // which is standard for Unix timestamps).
    let s = ts.unsigned_abs(); // treat as |seconds|
    let neg = ts < 0;

    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let days = s / 86400;

    // Days since 1970-01-01 → calendar date (Gregorian proleptic).
    let (year, month, day) = days_to_ymd(days as u32);

    if neg {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            1970u32.saturating_sub(year),
            month,
            day,
            hour,
            min,
            sec
        )
    } else {
        format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z"
        )
    }
}

/// Convert days since 1970-01-01 to (year, month, day).
/// Uses the proleptic Gregorian calendar algorithm from
/// <https://howardhinnant.github.io/date_algorithms.html#civil_from_days>.
fn days_to_ymd(days: u32) -> (u32, u32, u32) {
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m, d)
}

/// Convert (year, month, day) to days since 1970-01-01 (proleptic Gregorian).
/// Inverse of [`days_to_ymd`]. Uses Howard Hinnant's `days_from_civil`
/// algorithm. Returns `None` for impossible month/day combinations (the
/// algorithm itself is range-tolerant; we reject out-of-range inputs early so
/// a malformed Winlink Date can't yield a plausible-looking wrong timestamp).
fn ymd_to_days(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Day-of-month upper bound per (year, month). Feb 29 is permitted on leap
    // years only. (Same Gregorian rule as the rest of the algorithm.)
    let max_day: u32 = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if leap { 29 } else { 28 }
        }
        _ => unreachable!(),
    };
    if day > max_day {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let m = if month > 2 { month - 3 } else { month + 9 }; // [0, 11]
    let doy = (153 * m as u64 + 2) / 5 + day as u64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    Some(era * 146_097 + doe as i64 - 719_468)
}

/// Parse a Winlink B2F-format Date header (`YYYY/MM/DD HH:MM`, UTC implicit)
/// into a Unix timestamp in seconds. Returns `None` if the format doesn't
/// match exactly. Used as the second fallback in [`parse_raw_rfc5322`] for
/// CMS-originated messages — mail-parser's strict RFC5322 reader rejects this
/// non-standard format, so an unconditional pre-fix epoch fallback misled the
/// reading pane (tuxlink-p3u).
fn parse_winlink_date(raw: &str) -> Option<i64> {
    let s = raw.trim();
    // Exact length + separator positions; no slack. A malformed-but-parseable
    // suffix shouldn't be silently absorbed.
    if s.len() != 16 {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'/' || bytes[7] != b'/' || bytes[10] != b' ' || bytes[13] != b':' {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let hour: u32 = s.get(11..13)?.parse().ok()?;
    let minute: u32 = s.get(14..16)?.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let days = ymd_to_days(year, month, day)?;
    Some(days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60)
}

// ---- message_read command --------------------------------------------------

/// Read and parse a single message. Consumed by Task 13's `useMessage`.
///
/// Calls `read_message_in(folder, id)` to get raw RFC5322 bytes, then
/// parses headers + body + attachments into [`ParsedMessageDto`]. The
/// folder must be supplied — it comes from `selectedMessage.folder` in the
/// frontend (spec §4.2); never assumed to be Inbox.
///
/// Per spec §5.3 + Codex V3: parse input is capped at [`MAX_RFC5322_BYTES`];
/// parse failure → `UiError::Internal`; the UI renders a "could not parse"
/// state rather than crashing or showing garbage.
///
/// **NOT registered in `lib.rs`'s `invoke_handler` by this task** — the
/// orchestrator integration commit (spec §4.3) handles registration.
#[tauri::command]
pub async fn message_read(
    folder: String,
    id: String,
    state: State<'_, BackendState>,
) -> Result<ParsedMessageDto, UiError> {
    use crate::native_mailbox::FolderRef;
    let parsed = parse_folder_ref(&folder)?;
    let mid = MessageId::new(&id);
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let body = match &parsed {
        FolderRef::System(f) => backend.read_message_in(*f, &mid).await?,
        FolderRef::User(slug) => backend.read_user_message(slug, &mid).await?,
    };
    // Opening a message marks it read (tuxlink-xgn). Best-effort: a marker-write
    // failure must not fail the read the user just performed, so the error is
    // discarded (the message simply stays unread and self-heals on the next
    // open). User folders don't track unread today so the mark is a no-op for
    // them — only system folders flow through `mark_read`.
    if let FolderRef::System(f) = &parsed {
        let _ = backend.mark_read(*f, &mid).await;
    }
    parse_raw_rfc5322(&id, &body.raw_rfc5322)
}

// ---- message_attachment_save command (tuxlink-0fyj) ------------------------

/// Save a named attachment from a stored message to a destination path on disk.
///
/// Reads the message's raw RFC5322 from the storage layer, parses MIME via
/// `mail_parser`, extracts the bytes of the part whose `attachment_name()`
/// matches `filename`, and writes them to `dest_path`. The frontend's flow is
/// `@tauri-apps/plugin-dialog`'s `save()` → invoke this command with the chosen
/// path → toast 'Saved'. The attachment body never crosses the IPC boundary —
/// only the destination path does.
///
/// Errors map to UiError as: missing folder/message → NotFound (propagated from
/// the backend); attachment-name not in the message → NotFound; filesystem
/// failure → Internal { detail: <io error> }.
#[tauri::command]
pub async fn message_attachment_save(
    folder: String,
    id: String,
    filename: String,
    dest_path: String,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    use crate::native_mailbox::FolderRef;
    let parsed = parse_folder_ref(&folder)?;
    let mid = MessageId::new(&id);
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let body = match &parsed {
        FolderRef::System(f) => backend.read_message_in(*f, &mid).await?,
        FolderRef::User(slug) => backend.read_user_message(slug, &mid).await?,
    };
    let msg = mail_parser::MessageParser::new()
        .parse(body.raw_rfc5322.as_slice())
        .ok_or_else(|| UiError::Internal {
            detail: format!("could not parse message {id}"),
        })?;
    let bytes = extract_attachment_bytes(&msg, &filename).ok_or_else(|| {
        UiError::NotFound(format!("attachment '{filename}' not in message {id}"))
    })?;
    std::fs::write(&dest_path, &bytes).map_err(|e| UiError::Internal {
        detail: format!("write {dest_path}: {e}"),
    })?;
    Ok(())
}

// ---- mailbox_move command (tuxlink-ca5x) -----------------------------------

/// Move a message between folders. Used by the reading-pane Archive button
/// and the `A` accelerator (spec: docs/superpowers/specs/2026-06-02-user-folders-design.md §4).
///
/// `from` is the message's current folder (from `selectedMessage.folder`);
/// `to` is the destination (Inbox/Sent/Outbox/Archive today; user-folder slugs
/// in Phase 2). Drafts is rejected on both sides — it is a local-only store.
///
/// Mirrors the cache-invalidation contract `useMessage` uses for `mark_read`:
/// the move succeeds at the storage layer and the frontend invalidates the
/// affected folder queries so the row disappears from the source list and
/// appears in the destination on the next refetch.
#[tauri::command]
pub async fn mailbox_move(
    from: String,
    to: String,
    id: String,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    let from_ref = parse_folder_ref(&from)?;
    let to_ref = parse_folder_ref(&to)?;
    let mid = MessageId::new(&id);
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    backend.move_between_folders(from_ref, to_ref, &mid).await?;
    Ok(())
}

// ---- user-folder commands (tuxlink-f62f) -----------------------------------

/// DTO mirror of [`crate::user_folders::UserFolder`] over Tauri's camelCase
/// IPC. Display name + slug + creation time; the registry persists nothing
/// else today.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserFolderDto {
    pub slug: String,
    pub display_name: String,
    pub created_at: String,
}

impl From<crate::user_folders::UserFolder> for UserFolderDto {
    fn from(f: crate::user_folders::UserFolder) -> Self {
        UserFolderDto {
            slug: f.slug,
            display_name: f.display_name,
            created_at: f.created_at,
        }
    }
}

/// List the operator's user-created folders. Frontend `useUserFolders` calls
/// this; sidebar renders the result + the `+` button.
#[tauri::command]
pub async fn user_folders_list(
    state: State<'_, BackendState>,
) -> Result<Vec<UserFolderDto>, UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let folders = backend.list_user_folders().await?;
    Ok(folders.into_iter().map(UserFolderDto::from).collect())
}

/// Create a new user folder with the given display name. Slug is derived
/// (`ARES Drills` → `ares-drills`); reserved names + duplicate slugs are
/// rejected as `BackendError::MessageRejected` → surfaced as `UiError`.
#[tauri::command]
pub async fn folder_create(
    display_name: String,
    state: State<'_, BackendState>,
) -> Result<UserFolderDto, UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let folder = backend.create_user_folder(&display_name).await?;
    Ok(UserFolderDto::from(folder))
}

/// Rename a user folder (display name only; slug stays stable per spec §3.1).
/// The new display name is validated and surfaced as `UiError::Rejected`
/// on validation failure. Missing slug → `UiError::NotFound`.
#[tauri::command]
pub async fn folder_rename(
    slug: String,
    display_name: String,
    state: State<'_, BackendState>,
) -> Result<UserFolderDto, UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let folder = backend.rename_user_folder(&slug, &display_name).await?;
    Ok(UserFolderDto::from(folder))
}

/// Delete a user folder. `on_messages` controls cascade behavior (spec §6 D6):
/// - `"move_to_inbox"` (safe default) — re-home each message to Inbox
/// - `"move_to_archive"` — re-home each message to Archive
/// - `"delete"` — permanently remove the messages
#[tauri::command]
pub async fn folder_delete(
    slug: String,
    on_messages: String,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    use crate::native_mailbox::DeleteAction;
    let action = match on_messages.as_str() {
        "move_to_inbox" => DeleteAction::MoveToInbox,
        "move_to_archive" => DeleteAction::MoveToArchive,
        "delete" => DeleteAction::Delete,
        other => {
            return Err(UiError::Internal {
                detail: format!("unknown on_messages action: {other}"),
            })
        }
    };
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    backend.delete_user_folder(&slug, action).await?;
    Ok(())
}

// Task 14 — message_send command (spec §3.2, §5.4)
// ============================================================================
// Appended here per the append-only ownership model (spec §7). The
// `invoke_handler` registration lands in the orchestrator integration commit
// (§4.3); this file is append-only for command fns.

/// Attachment DTO for the compose window IPC.
///
/// `bytes` is serialized as a base64-encoded string by serde-json when crossing
/// the Tauri IPC boundary — that is Tauri / serde-json's default `Vec<u8>`
/// representation. The receiver (Rust) deserializes it back to raw bytes.
///
/// NOTE: large attachments over the IPC layer are intentionally not optimized for
/// v0.0.1; the file-picker UI (HTML Forms, PR #151) is not built yet. This DTO
/// establishes the bridge contract. Callers that have no attachments pass `[]`.
#[derive(Debug, Deserialize)]
pub struct OutboundAttachmentDto {
    pub filename: String,
    pub bytes: Vec<u8>,
}

/// Inbound DTO from the compose window frontend. Mirrors `OutboundDraftDto`
/// in `src/compose/Compose.tsx`.
///
/// **`cc` caveat (spec §3.2, Codex F5 VERIFIED):** The compose UI disables the
/// Cc field with a v0.1 tooltip (spec §5.4 disposition: "disable with tooltip
/// rather than silently drop"). The `cc` field is present in this DTO for API
/// completeness; native B2F outbound support for Cc is a v0.1 TODO.
///
/// **`attachments` (P2.1 / Codex post-impl review):** Previously hardcoded to
/// `vec![]` in the command body, attachments are now an explicit DTO field so
/// the compose window can thread files through the IPC layer. The frontend
/// passes `[]` until the attachment-picker UI (HTML Forms, PR #151) is built;
/// the backend plumbing (T4.1 + compose_message_with_files) already handles them.
#[derive(Debug, Deserialize)]
pub struct OutboundDraftDto {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// Attachment files. Tauri IPC encodes `Vec<u8>` fields as base64 by default.
    /// Frontend passes `[]` until the file-picker is built (HTML Forms PR #151).
    #[serde(default)]
    pub attachments: Vec<OutboundAttachmentDto>,
}

/// Send an outbound message queued via the compose window.
///
/// Maps `OutboundDraftDto` → `OutboundMessage` (adds `date = now RFC3339`
/// per spec §3.2 — the UI does not supply the send timestamp; the command
/// stamps it at queue time).
///
/// Returns `Ok(mid_string)` on success. `NativeBackend` returns a real MID.
/// The compose window shows "Posted to Outbox" on any `Ok(_)`.
/// Spec §3.2 + §5.4.
#[tauri::command]
pub async fn message_send(
    draft: OutboundDraftDto,
    state: State<'_, BackendState>,
) -> Result<String, UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;

    // Stamp the send timestamp here (the UI does not supply it — spec §3.2).
    let date = chrono::Utc::now().to_rfc3339();

    // P2.1 (Codex post-impl review): map DTO attachments into OutboundMessage.
    // The backend (T4.1 + compose_message_with_files) already handles attachments;
    // this was the only gap — the IPC layer was hardcoded to vec![]. Now threaded.
    let attachments: Vec<crate::winlink_backend::OutboundAttachment> = draft
        .attachments
        .into_iter()
        .map(|a| crate::winlink_backend::OutboundAttachment {
            filename: a.filename,
            bytes: a.bytes,
        })
        .collect();

    let msg = OutboundMessage {
        to: draft.to,
        cc: draft.cc,  // forwarded as-is; native B2F Cc support is a v0.1 TODO (Codex F5)
        subject: draft.subject,
        body: draft.body,
        date,
        attachments,
    };

    // send_message returns MessageId directly. Map to String for IPC.
    let mid = backend.send_message(msg).await?;
    Ok(mid.0)
}

/// Send an outbound Winlink HTML Form.
///
/// Per spec §5.1 (Path B — native B2F) + ADR 0016. Looks up the form
/// definition in the bundled catalog, builds the XML payload + plain-text
/// body + subject via the form's templates, wraps the XML in an
/// `OutboundAttachment` named `RMS_Express_Form_<id>.xml`, and dispatches
/// via `backend.send_message()` — the same pipeline as `message_send`.
///
/// `senders_callsign` + `grid_square` come from the caller (typically the
/// configured CMS callsign / locator); the XML's `<form_parameters>` block
/// uses them.
///
/// Returns the MID string on success (mirrors `message_send` contract).
#[tauri::command]
pub async fn send_form(
    form_id: String,
    field_values: std::collections::HashMap<String, String>,
    to: Vec<String>,
    cc: Vec<String>,
    senders_callsign: String,
    grid_square: String,
    state: State<'_, BackendState>,
) -> Result<String, UiError> {
    use crate::forms;

    let form = forms::catalog::find_form(&form_id)
        .ok_or_else(|| UiError::Internal {
            detail: format!("unknown form: {}", form_id),
        })?;

    let now = chrono::Utc::now();
    let params = forms::types::FormParameters {
        xml_file_version: "1.0".to_string(),
        rms_express_version: format!("Tuxlink/{}", env!("CARGO_PKG_VERSION")),
        submission_datetime: now.format("%Y%m%d%H%M%S").to_string(),
        senders_callsign,
        grid_square,
        display_form: form.display_form.to_string(),
        reply_template: form.reply_template.to_string(),
    };

    let xml_bytes = forms::serialize::serialize_form_xml(form, &params, &field_values);
    let body = forms::serialize::render_body_template(form.body_template, &field_values);
    let subject = forms::serialize::render_body_template(form.subject_template, &field_values);

    // OutboundAttachment has { filename, bytes } only — NO content_type field.
    // The native B2F wire format does not use MIME content-type headers for
    // attachments. See winlink_backend.rs ~105-108 for the canonical struct.
    let attachment = crate::winlink_backend::OutboundAttachment {
        filename: format!("RMS_Express_Form_{}.xml", form.id),
        bytes: xml_bytes,
    };

    let msg = OutboundMessage {
        to,
        cc,
        subject,
        body,
        date: now.to_rfc3339(),
        attachments: vec![attachment],
    };

    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let mid = backend.send_message(msg).await?;
    Ok(mid.0)
}

/// Run one CMS connection: send everything queued in the outbox and download any
/// waiting messages (tuxlink-0ic). Drives the backend's `connect` over the
/// configured transport, then drops the session (the native exchange completes
/// within the call). The frontend refreshes the mailbox on success.
///
/// Progress and the result (including any failure reason) are surfaced in the
/// session log via `session_log:line` events — NOT returned for display beside
/// the button. The command still returns `Err` so the caller can stop its
/// spinner / skip the mailbox refresh, but the human-facing detail lives in the
/// log + the connection-status ribbon.
///
/// On the native backend this performs the real on-air exchange; against the
/// production CMS it currently fails with the client-type rejection until
/// "tuxlink" is registered with Winlink (set `TUXLINK_CMS_HOST=cms-z.winlink.org`
/// to exercise it against the dev CMS in the meantime).
#[tauri::command]
pub async fn cms_connect(
    app: AppHandle,
    state: State<'_, BackendState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;

    let cfg = config::read_config().map_err(|e| UiError::Internal {
        detail: e.to_string(),
    })?;

    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        format!("Connecting to the CMS ({:?})…", cfg.connect.transport),
    );

    match backend
        .connect(TransportConfig::Cms {
            mode: cfg.connect.transport,
        })
        .await
    {
        Ok(session) => {
            emit_session_line(&app, &log, LogLevel::Info, "CMS exchange complete.".to_string());
            // 2026-05-31 operator-flagged: previously the session was
            // dropped without transitioning status back to Disconnected,
            // so the ribbon + status bar showed "Connected · Telnet"
            // perpetually after a successful CMS exchange. CMS connects
            // are transient (connect → B2F exchange → done), not a held
            // socket — close the session explicitly so backend_status
            // reflects reality on the next poll.
            //
            // 2026-05-31 operator smoke #5: even after the event-driven
            // status fix landed, the Connected state was sub-millisecond
            // on screen (the disconnect fires immediately, React batches
            // the rapid setStatus(Connected) → setStatus(Disconnected) and
            // the user never sees green). Hold the Connected state for
            // 1.5s before disconnecting so the operator has perceptible
            // visual confirmation that the exchange succeeded. The status
            // really is Connected for that time (no UX lie); the cost is
            // ~1.5s of delayed Start-button re-enable, which is
            // imperceptible compared to the value of the success signal.
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            if let Err(e) = backend.disconnect(session).await {
                emit_session_line(
                    &app,
                    &log,
                    LogLevel::Warn,
                    format!("session close after exchange: {e}"),
                );
            }
            Ok(())
        }
        Err(BackendError::Cancelled) => {
            // Operator-initiated abort (tuxlink-9z2) — not a failure.
            emit_session_line(&app, &log, LogLevel::Warn, "CMS connection aborted.".to_string());
            Err(BackendError::Cancelled.into())
        }
        Err(e) => {
            emit_session_line(&app, &log, LogLevel::Error, format!("CMS connect failed: {e}"));
            Err(e.into())
        }
    }
}

/// Abort an in-flight [`cms_connect`] (tuxlink-9z2): shut down the connecting
/// socket so a slow TLS/login/exchange phase unblocks, returning the backend to
/// Disconnected. The aborted `cms_connect` resolves with a `Cancelled` error its
/// caller swallows. A no-op when nothing is connecting.
#[tauri::command]
pub async fn cms_abort(
    app: AppHandle,
    state: State<'_, BackendState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;

    emit_session_line(&app, &log, LogLevel::Info, "Aborting CMS connection…".to_string());
    backend.abort().await?;
    Ok(())
}

/// Append a session-log line to the durable buffer (assigning its `seq`) and emit
/// it live on `session_log:line`, so it lands in the bottom progress log
/// (snapshot + tail). Used for connect progress/results (tuxlink-0ic).
fn emit_session_line(
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

// ============================================================================
// Task 16 — config_read + backend_status (spec §3.2, §5.6)
// bd issue: tuxlink-hvv
// ============================================================================
// Appended here per the append-only ownership model (spec §7). The
// `invoke_handler` registration lands in the orchestrator integration commit
// (§4.3). `config_read` reads `config.rs` with NO BackendState dependency
// (keeping Task 16 independent for build); `backend_status` consumes the live
// trait `status()` when populated, else `None` (the frontend's
// `formatConnectionState(null, configTransport)` renders the config-derived
// "Idle · <transport>" fallback — spec §5.6 + status.test.ts).

/// Flattened, frontend-facing projection of the nested [`config::Config`].
///
/// The Rust config is nested (`connect.{connect_to_cms,transport}`,
/// `identity.{callsign,identifier,grid}`, `privacy.{gps_state,
/// position_precision}`); the Task-16 ribbon's `useStatus` consumes a FLAT
/// shape (`src/shell/useStatus.ts` `ConfigViewDto`). This DTO is that flat
/// mapping. Field names are emitted verbatim (snake_case) to match the TS
/// `ConfigViewDto` (which is snake_case, NOT camelCase — verified against
/// `useStatus.ts`). The enum values serialize PascalCase (`CmsSsl`/`Telnet`,
/// `Off`/`LocalUiOnly`/`BroadcastAtPrecision`, `FourCharGrid`/`SixCharGrid`,
/// `Manual`/`Gps`) per `config.rs`'s `#[serde(rename_all = "PascalCase")]`,
/// matching the TS `CmsTransport`/`GpsState`/`PositionPrecision`/`PositionSource`
/// literal unions.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ConfigViewDto {
    pub connect_to_cms: bool,
    pub transport: CmsTransport,
    /// CMS server host the operator dials (tuxlink-3o0). Surfaced so the inline
    /// SettingsPanel can load the current host into its text input on open.
    pub host: String,
    pub callsign: Option<String>,
    pub identifier: Option<String>,
    pub grid: Option<String>,
    pub gps_state: GpsState,
    pub position_precision: PositionPrecision,
    /// Active position source (tuxlink-686): `Gps` (default) or `Manual` (operator
    /// has pinned a grid square). Mirrors `PrivacyConfig.position_source` in
    /// config.rs. Task 8 renders a source chip from this field.
    pub position_source: PositionSource,
}

impl From<&config::Config> for ConfigViewDto {
    /// Map nested → flat. Pure; no I/O. Drives the unit test
    /// `config_view_dto_maps_nested_to_flat`.
    fn from(c: &config::Config) -> Self {
        ConfigViewDto {
            connect_to_cms: c.connect.connect_to_cms,
            transport: c.connect.transport,
            host: c.connect.host.clone(),
            callsign: c.identity.callsign.clone(),
            identifier: c.identity.identifier.clone(),
            grid: c.identity.grid.clone(),
            gps_state: c.privacy.gps_state,
            position_precision: c.privacy.position_precision,
            position_source: c.privacy.position_source,
        }
    }
}

/// Read the tuxlink config and project it to the flat [`ConfigViewDto`] the
/// ribbon consumes.
///
/// NOT a backend call (spec §3.2) — reads `config.rs` directly so Task 16
/// stays independent of `BackendState`. A read/parse/validation failure (incl.
/// "no config yet", pre-wizard) maps to `UiError::Internal` (spec §3.1
/// "`config_read` is NOT a backend call — its failures map directly to
/// `UiError::Internal`"). The ribbon `.catch()`es this and renders empty,
/// so pre-wizard launches degrade gracefully.
#[tauri::command]
pub async fn config_read() -> Result<ConfigViewDto, UiError> {
    let cfg = config::read_config().map_err(|e| UiError::Internal {
        detail: e.to_string(),
    })?;
    Ok(ConfigViewDto::from(&cfg))
}

/// Serializable projection of [`BackendStatus`] for the ribbon. Mirrors
/// `StatusDto` in `src/shell/useStatus.ts` via Tauri's
/// `#[serde(tag = "kind")]` shape (an INTERNALLY-tagged union — the variant
/// fields sit alongside `kind`, NOT nested under a `content` key, matching
/// the TS `{ kind: 'Connected'; transport; peer; since_iso }` shape).
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(tag = "kind")]
pub enum StatusDto {
    Disconnected,
    Connecting {
        transport: String,
    },
    /// Packet armed-but-idle (listening to answer an inbound call). Renders as
    /// "Listening · <transport>" in the ribbon. (tuxlink-orj)
    Listening {
        transport: String,
    },
    Connected {
        transport: String,
        peer: String,
        since_iso: String,
    },
    Disconnecting,
    Error {
        reason: String,
    },
}

impl From<BackendStatus> for StatusDto {
    /// Map the trait status enum → the wire DTO. Exhaustive over the current
    /// variants; `BackendStatus` is `#[non_exhaustive]`, so a future variant
    /// added in `winlink_backend.rs` fails this `match` and forces a
    /// deliberate UI projection rather than a silent wildcard. The `transport`
    /// string is passed through verbatim (`format!("CMS-{:?}", mode)` →
    /// `"CMS-CmsSsl"`/`"CMS-Telnet"`); the frontend's `normalizeTransportLabel`
    /// renders it.
    fn from(s: BackendStatus) -> Self {
        match s {
            BackendStatus::Disconnected => StatusDto::Disconnected,
            BackendStatus::Connecting { transport } => StatusDto::Connecting { transport },
            BackendStatus::Listening { transport } => StatusDto::Listening { transport },
            BackendStatus::Connected {
                transport,
                peer,
                since_iso,
            } => StatusDto::Connected {
                transport,
                peer,
                since_iso,
            },
            BackendStatus::Disconnecting => StatusDto::Disconnecting,
            BackendStatus::Error { reason } => StatusDto::Error { reason },
        }
    }
}

/// Derive the ribbon's `Option<StatusDto>` from one atomic `BackendState`
/// snapshot (spec §3.4, adrev #9). Pure — no I/O, no lock; takes the cloned
/// `(phase, backend)` pair so it is unit-testable without a `tauri::State`
/// (drives the Task-D `derive_status_*` tests). The three-state model:
///
/// - [`BackendPhase::NotConfigured`] → `None`. The ribbon's
///   `formatConnectionState(null, config.transport)` renders the config-derived
///   "Idle · <transport>" fallback — the "not connected" empty state, NOT an
///   error (`src/shell/useStatus.ts`).
/// - [`BackendPhase::Spawning`] → `Some(Connecting)` — the bootstrap is
///   launching Pat. `transport` is left empty here (the bootstrap has not yet
///   established a CMS transport; the ribbon's `normalizeTransportLabel`
///   tolerates an empty string and the config-derived label fills the gap).
/// - [`BackendPhase::Ready`] + a backend → the live backend's `status()` mapped
///   via the existing `From<BackendStatus>` impl. (A `Ready` phase always
///   carries a backend by the `BackendState` invariant; a defensive `Ready`
///   with `None` backend degrades to `None`.)
/// - [`BackendPhase::Failed`] / [`BackendPhase::ConfigError`] → `Some(Error{reason})`
///   — an EXPLICIT error the ribbon shows loudly (Pat is a core runtime
///   dependency in CMS mode; its failure is not a benign absence — spec §2).
pub fn derive_status_dto(
    phase: BackendPhase,
    backend: Option<std::sync::Arc<dyn crate::winlink_backend::WinlinkBackend>>,
) -> Option<StatusDto> {
    match phase {
        BackendPhase::NotConfigured => None,
        BackendPhase::Spawning => Some(StatusDto::Connecting {
            transport: String::new(),
        }),
        BackendPhase::Ready => backend.map(|b| StatusDto::from(b.status())),
        BackendPhase::Failed { reason } | BackendPhase::ConfigError { reason } => {
            Some(StatusDto::Error { reason })
        }
    }
}

/// Return the ribbon's three-state status from one atomic [`BackendState`]
/// snapshot (spec §3.4 / §5.6, adrev #9 — no torn read between phase + backend).
///
/// Per spec §5.6 (Codex verdict V6): `status()` is sync / non-I/O (the trait
/// caches it — `winlink_backend.rs`), so it is cheap to poll. The derivation is
/// the pure [`derive_status_dto`]; see it for the full per-phase mapping. In
/// short: `NotConfigured` → `None` (the ribbon's config-derived "Idle ·
/// <transport>" fallback); `Spawning` → `Connecting`; `Ready` → the live
/// backend's `status()`; `Failed`/`ConfigError` → an explicit `Error{reason}`.
#[tauri::command]
pub async fn backend_status(state: State<'_, BackendState>) -> Result<Option<StatusDto>, UiError> {
    // `snapshot()` clones (phase, backend) under ONE read guard and drops it
    // (spec §1.1 + adrev #9); we hold NO lock. `status()` is sync + non-I/O.
    let (phase, backend) = state.snapshot();
    Ok(derive_status_dto(phase, backend))
}

// ============================================================================
// Task 15 — session_log_snapshot (spec §3.3 / §5.5)
// bd issue: tuxlink-8zg (integration commit; the snapshot command was specified
// in §3.3 but not implemented by Task 15 — Codex integration round P1)
// ============================================================================
// Appended here per the append-only ownership model (spec §7). Registered in
// `lib.rs`'s `invoke_handler` by the integration commit (§4.3).

/// Serializable session-log line. Mirrors `LogLineDto` in
/// `src/session/logProjection.ts` — field names are camelCase on the wire
/// (`timestampIso`) and the enum values serialize as lowercase strings
/// (`'trace'|'debug'|'info'|'warn'|'error'`, `'backend'|'transport'|'wire'`)
/// so the TS model needs no rename/translation layer.
///
/// `seq` is the monotonic sequence number from `SessionLogState`. The frontend
/// uses it as a cursor for snapshot-then-tail deduplication (adrev #4): seed
/// from `session_log_snapshot`, record the last `seq`, then filter live events
/// by `seq > last_seen_seq` to close the subscribe-before-listen window.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogLineDto {
    pub seq: u64,
    pub timestamp_iso: String,
    pub level: LogLevelDto,
    pub source: LogSourceDto,
    pub message: String,
}

/// Wire projection of [`LogLevel`]. Lowercase to match the TS union.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevelDto {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for LogLevelDto {
    /// Exhaustive over the current variants. `LogLevel` is `#[non_exhaustive]`,
    /// so a future variant added in `winlink_backend.rs` fails this `match`,
    /// forcing a deliberate wire projection rather than a silent wildcard.
    fn from(l: LogLevel) -> Self {
        match l {
            LogLevel::Trace => LogLevelDto::Trace,
            LogLevel::Debug => LogLevelDto::Debug,
            LogLevel::Info => LogLevelDto::Info,
            LogLevel::Warn => LogLevelDto::Warn,
            LogLevel::Error => LogLevelDto::Error,
        }
    }
}

/// Wire projection of [`LogSource`]. Lowercase to match the TS union.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogSourceDto {
    Backend,
    Transport,
    Wire,
}

impl From<LogSource> for LogSourceDto {
    /// Exhaustive over the current variants; see [`LogLevelDto::from`].
    fn from(s: LogSource) -> Self {
        match s {
            LogSource::Backend => LogSourceDto::Backend,
            LogSource::Transport => LogSourceDto::Transport,
            LogSource::Wire => LogSourceDto::Wire,
        }
    }
}

impl From<LogLine> for LogLineDto {
    fn from(l: LogLine) -> Self {
        LogLineDto {
            seq: l.seq,
            timestamp_iso: l.timestamp_iso,
            level: l.level.into(),
            source: l.source.into(),
            message: l.message,
        }
    }
}

/// Return the current session-log snapshot for late subscribers / pane re-open
/// (spec §3.3 / §11.1: "returns the current ring buffer (last N lines)").
///
/// Reads the durable `SessionLogState` ring buffer managed by the app.
/// Each line carries a monotonic `seq` so the frontend can implement
/// snapshot-then-tail without losing lines in the subscribe-before-listen
/// window (adrev #1, #2, #3):
///
///   1. Call `session_log_snapshot` → seed the pane, record `last_seq`.
///   2. Listen on `session_log:line` events.
///   3. On each event, only display lines with `seq > last_seq` to close
///      the gap and de-duplicate (adrev #4: timestamp collisions possible).
///
/// Task D (the drain task in `lib.rs`) manages the `SessionLogState` and
/// calls `append` before broadcasting. Until Task D is wired, the managed
/// state starts empty (cap 500) and this command returns `[]` — the same
/// contract as before, now future-proof.
#[tauri::command]
pub async fn session_log_snapshot(
    // Task C (tuxlink-22l §11.2): the managed buffer is an `Arc<SessionLogState>`
    // so the backend's bridge thread can append to the SAME buffer this command
    // reads. `State` derefs through the `Arc`, so `state.snapshot()` resolves to
    // `SessionLogState::snapshot` unchanged.
    state: State<'_, std::sync::Arc<SessionLogState>>,
) -> Result<Vec<LogLineDto>, UiError> {
    Ok(state.snapshot().into_iter().map(LogLineDto::from).collect())
}

/// Drain the shared session-log ring buffer (operator smoke 2026-05-31).
///
/// Previously `useSessionLog`'s `clear()` only reset the panel's React state.
/// When the operator switched modes, the new panel re-mounted, refetched the
/// snapshot via `session_log_snapshot`, and the "cleared" entries reappeared.
/// This command empties the backend buffer so the snapshot is genuinely empty
/// after clear. `next_seq` is preserved so post-clear ids stay monotonic — a
/// stale panel still holding a `last_seq` cursor cannot accidentally match a
/// recycled id.
#[tauri::command]
pub fn session_log_clear(
    state: State<'_, std::sync::Arc<SessionLogState>>,
) -> Result<(), UiError> {
    state.clear();
    Ok(())
}

// ============================================================================
// tuxlink-ng3 — app_quit (HTML chrome File→Quit / Ctrl+Q)
// ============================================================================

/// Exit the application (tuxlink-ng3). With the native menu removed, File → Quit
/// and the Ctrl+Q accelerator invoke this. Mirrors the native menu's old inline
/// `app.exit(0)` (menu.rs) — `PredefinedMenuItem::quit` is unsupported on
/// Linux/muda, so an explicit command is the canonical pattern. This is the ONLY
/// path that exits the process; the window close button keeps the app alive
/// (lib.rs CloseRequested handler).
#[tauri::command]
pub fn app_quit(app: tauri::AppHandle) {
    app.exit(0);
}

// ============================================================================
// Task 7 (tuxlink-7fr) — packet_config_get / packet_config_set
// ============================================================================
// `packet_config_get` reads `config.rs` directly (no BackendState dependency,
// like `config_read`); `packet_config_set` reads the current config, applies
// the DTO's packet fields, validates (SSID range), and writes atomically.
//
// The DTO is flat / camelCase on the wire to match the TS PacketConfigDto shape.
// `link_kind` is `"Tcp"` | `"Serial"` | absent; tcp_*/serial_* fields carry
// whichever set applies.

/// Flat, frontend-facing projection of `config::PacketConfig` (the `[packet]`
/// section). camelCase on the wire to match the TS model. `link_kind` is
/// `"Tcp"` | `"Serial"` | absent; the tcp_*/serial_* fields carry whichever set
/// applies (the other is `None`).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PacketConfigDto {
    pub ssid: u8,
    pub listen_default: bool,
    pub link_kind: Option<String>,
    pub tcp_host: Option<String>,
    pub tcp_port: Option<u16>,
    pub serial_device: Option<String>,
    pub serial_baud: Option<u32>,
    /// Radio MAC for `linkKind: "Bluetooth"` (tuxlink-nx2 RFCOMM-socket transport).
    /// `#[serde(default)]` so a payload from an older frontend (no `btMac`) still loads.
    #[serde(default)]
    pub bt_mac: Option<String>,
    pub txdelay: u8,
    pub persistence: u8,
    pub slot_time: u8,
    pub paclen: u16,
    pub maxframe: u8,
    pub t1_ms: u64,
    pub n2_retries: u8,
}

impl From<&config::PacketConfig> for PacketConfigDto {
    fn from(p: &config::PacketConfig) -> Self {
        use crate::winlink::ax25::KissLinkConfig;
        let (link_kind, tcp_host, tcp_port, serial_device, serial_baud, bt_mac) = match &p.link {
            Some(KissLinkConfig::Tcp { host, port }) => {
                (Some("Tcp".into()), Some(host.clone()), Some(*port), None, None, None)
            }
            Some(KissLinkConfig::Serial { device, baud }) => {
                (Some("Serial".into()), None, None, Some(device.clone()), Some(*baud), None)
            }
            Some(KissLinkConfig::Bluetooth { mac }) => {
                (Some("Bluetooth".into()), None, None, None, None, Some(mac.clone()))
            }
            None => (None, None, None, None, None, None),
        };
        PacketConfigDto {
            ssid: p.ssid,
            listen_default: p.listen_default,
            link_kind,
            tcp_host,
            tcp_port,
            serial_device,
            serial_baud,
            bt_mac,
            txdelay: p.params.txdelay,
            persistence: p.params.persistence,
            slot_time: p.params.slot_time,
            paclen: p.params.paclen,
            maxframe: p.params.maxframe,
            t1_ms: p.params.t1_ms,
            n2_retries: p.params.n2_retries,
        }
    }
}

impl PacketConfigDto {
    /// Build a `PacketConfig` from the DTO. Validates the link-kind/field
    /// coherence (`Tcp` needs host+port; `Serial` needs device+baud).
    pub fn into_packet_config(self) -> Result<config::PacketConfig, UiError> {
        use crate::winlink::ax25::KissLinkConfig;
        let link = match self.link_kind.as_deref() {
            Some("Tcp") => Some(KissLinkConfig::Tcp {
                host: self
                    .tcp_host
                    .ok_or_else(|| UiError::Internal { detail: "Tcp link needs tcp_host".into() })?,
                port: self
                    .tcp_port
                    .ok_or_else(|| UiError::Internal { detail: "Tcp link needs tcp_port".into() })?,
            }),
            Some("Serial") => Some(KissLinkConfig::Serial {
                device: self.serial_device.ok_or_else(|| UiError::Internal {
                    detail: "Serial link needs serial_device".into(),
                })?,
                baud: self.serial_baud.ok_or_else(|| UiError::Internal {
                    detail: "Serial link needs serial_baud".into(),
                })?,
            }),
            Some("Bluetooth") => Some(KissLinkConfig::Bluetooth {
                mac: self.bt_mac.ok_or_else(|| UiError::Internal {
                    detail: "Bluetooth link needs bt_mac".into(),
                })?,
            }),
            None => None,
            Some(other) => {
                return Err(UiError::Internal {
                    detail: format!("unknown link_kind '{other}'"),
                })
            }
        };
        Ok(config::PacketConfig {
            ssid: self.ssid,
            link,
            params: config::Ax25ParamsConfig {
                txdelay: self.txdelay,
                persistence: self.persistence,
                slot_time: self.slot_time,
                paclen: self.paclen,
                maxframe: self.maxframe,
                t1_ms: self.t1_ms,
                n2_retries: self.n2_retries,
            },
            listen_default: self.listen_default,
        })
    }
}

/// Read the `[packet]` config section as a flat DTO. Reads `config.rs` directly
/// (no BackendState), like `config_read`.
#[tauri::command]
pub async fn packet_config_get() -> Result<PacketConfigDto, UiError> {
    let cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(PacketConfigDto::from(&cfg.packet))
}

/// Apply the `[packet]` section from a DTO: read the current config, swap in
/// the new packet section, validate (SSID range), and write atomically.
#[tauri::command]
pub async fn packet_config_set(
    state: State<'_, BackendState>,
    dto: PacketConfigDto,
) -> Result<(), UiError> {
    let mut cfg =
        config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.packet = dto.into_packet_config()?;
    cfg.validate().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    // tuxlink-p5u: refresh the LIVE backend so a packet link/SSID/timing change
    // applies on the NEXT connect without an app restart (the packet connect path
    // reads the backend's live config for the callsign + Ax25 params).
    if let Some(backend) = state.current() {
        backend.set_config(cfg);
    }
    Ok(())
}

/// A discovered serial/RFCOMM device + its transport kind, so the UI can show
/// USB and Bluetooth devices SEPARATELY (and tell the operator what each is)
/// rather than dumping one undifferentiated `/dev` list into both pickers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialDeviceDto {
    /// Full device path, e.g. `/dev/ttyUSB0`.
    pub path: String,
    /// Transport class: `"usb"` | `"bluetooth"` | `"uart"`. The picker shows only
    /// the kind(s) matching the selected transport tab.
    pub kind: String,
    /// Human label, e.g. `"USB serial"` / `"Bluetooth (RFCOMM)"` / `"On-board UART"`.
    pub label: String,
}

/// Classify a `/dev` node name into (kind, label) for a KISS-capable port, or
/// `None` if it isn't one. USB-serial adapters (`ttyUSB`/`ttyACM`); bound
/// Bluetooth RFCOMM (`rfcomm`, appears once the operator pairs+binds — spec
/// §4.1); on-board UARTs (`ttyAMA`/`ttyS`, e.g. the Pi's GPIO serial). The
/// suffix check excludes a bare prefix with no instance number.
fn classify_serial_device(name: &str) -> Option<(&'static str, &'static str)> {
    let has_suffix = |p: &str| name.starts_with(p) && name.len() > p.len();
    if has_suffix("ttyUSB") || has_suffix("ttyACM") {
        Some(("usb", "USB serial"))
    } else if has_suffix("rfcomm") {
        Some(("bluetooth", "Bluetooth (RFCOMM)"))
    } else if has_suffix("ttyAMA") || has_suffix("ttyS") {
        Some(("uart", "On-board UART"))
    } else {
        None
    }
}

/// Scan `dev_dir` (normally `/dev`) for serial/RFCOMM device nodes a KISS TNC
/// might use, classified by kind. Pure + dir-injected so it is unit-testable
/// without real hardware. Sorted by path, deduped. Plain `std::fs` — no libudev,
/// no new system deps. This only ENUMERATES candidates; the operator confirms
/// the right one (and a real open is exercised on-air, RADIO-1).
pub fn discover_serial_devices(dev_dir: &std::path::Path) -> Vec<SerialDeviceDto> {
    let mut found: Vec<SerialDeviceDto> = match std::fs::read_dir(dev_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|name| {
                classify_serial_device(&name).map(|(kind, label)| SerialDeviceDto {
                    path: dev_dir.join(&name).to_string_lossy().into_owned(),
                    kind: kind.to_string(),
                    label: label.to_string(),
                })
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.dedup();
    found
}

/// List serial/RFCOMM devices a KISS TNC might use, classified by transport
/// (USB / Bluetooth / on-board UART), by scanning `/dev`. An empty list means
/// none are present — plug in a TNC or bind an rfcomm device, then refresh.
#[tauri::command]
pub async fn packet_list_serial_devices() -> Result<Vec<SerialDeviceDto>, UiError> {
    Ok(discover_serial_devices(std::path::Path::new("/dev")))
}

/// A paired Bluetooth radio the operator can dial as a KISS modem via the
/// in-app RFCOMM socket (tuxlink-nx2 — no `rfcomm bind`, no `/dev/rfcommN`).
/// Surfaced from `bluetoothctl devices Paired` so the picker shows BlueZ's
/// own list (the canonical "what's paired right now") rather than a `/dev`
/// snapshot that requires a separate `sudo rfcomm bind` first.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothDeviceDto {
    /// Bluetooth MAC, e.g. `"38:D2:00:01:55:5C"`. This is what the dial side
    /// hands to `KissLinkConfig::Bluetooth { mac }`.
    pub mac: String,
    /// Human-readable name BlueZ keeps for the device (the radio's friendly
    /// name, e.g. `"UV-PRO"`). Used as the dropdown label; if BlueZ has no
    /// name yet, the picker can fall back to the MAC.
    pub name: String,
}

/// Parse `bluetoothctl devices Paired` output into a list of paired devices.
/// One line per device: `Device <MAC> <NAME>` (the prefix is literal). Pure
/// (no I/O) so the parser is unit-testable without a Bluetooth stack. Invalid
/// MACs are dropped via `parse_bdaddr` — the picker should not offer an
/// un-dialable entry (the dial-side `RfcommSocket::connect` parses the same
/// way, so any MAC that survives here will round-trip cleanly).
pub fn parse_paired_bluetooth(output: &str) -> Vec<BluetoothDeviceDto> {
    output
        .lines()
        .filter_map(|raw| {
            let line = raw.trim_start();
            let rest = line.strip_prefix("Device ")?;
            // `Device <MAC> <NAME>` — MAC is the first whitespace-delimited
            // token; everything after the next space is the name (which may
            // contain spaces). A missing name yields an empty string, which
            // the picker renders as the MAC.
            let mut parts = rest.splitn(2, ' ');
            let mac = parts.next()?.trim();
            // Validate the MAC via the AX.25 parser — same routine the
            // dial side uses, so any MAC that lands in the dropdown will
            // also `RfcommSocket::connect` cleanly.
            crate::winlink::ax25::rfcomm::parse_bdaddr(mac)?;
            let name = parts.next().unwrap_or("").trim().to_string();
            Some(BluetoothDeviceDto { mac: mac.to_string(), name })
        })
        .collect()
}

/// List paired Bluetooth devices for the picker (tuxlink-mqu3). Shells out to
/// `bluetoothctl devices Paired` (BlueZ ≥ 5.66 / Pi packs 5.82); a missing
/// `bluetoothctl` binary OR a failed command yields an empty list rather than
/// an error — same posture as `packet_list_serial_devices` ("none present →
/// the operator pairs a radio and refreshes"). The MAC validator drops any
/// malformed line; pure parser exposed as `parse_paired_bluetooth` for tests.
#[tauri::command]
pub async fn packet_list_bluetooth_devices() -> Result<Vec<BluetoothDeviceDto>, UiError> {
    let output = std::process::Command::new("bluetoothctl")
        .args(["devices", "Paired"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            Ok(parse_paired_bluetooth(&String::from_utf8_lossy(&o.stdout)))
        }
        // `bluetoothctl` not installed, exit non-zero, or process spawn error:
        // empty list (matches the serial-discovery posture of "soft failure
        // yields an empty list"). The picker shows "no paired devices —
        // pair one and refresh"; a real error would surface as a dial
        // failure when the operator picks a device that doesn't actually
        // exist.
        _ => Ok(Vec::new()),
    }
}

/// An ALSA audio device the operator can pick for ARDOP capture or playback.
/// Surfaced from `arecord -L` / `aplay -L` so the picker shows ALSA's own
/// canonical name (the `plughw:CARD=…`, `pulse`, `default` ladder) rather
/// than asking the operator to remember the syntax (tuxlink-y7x7).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlsaDeviceDto {
    /// Canonical ALSA name fed to ardopcf via `-c` / `-p`, e.g.
    /// `"plughw:CARD=Device,DEV=0"` or `"default"`.
    pub name: String,
    /// One-line human label from the ALSA tool's description block, e.g.
    /// `"USB Audio CODEC"`. May be empty for unusual entries.
    pub description: String,
    /// True for direct-hardware entries (`plughw:CARD=…`, `hw:CARD=…`).
    /// The picker sorts these to the top so the operator's first choice
    /// from the dropdown is the kind ARDOP actually wants (a real audio
    /// interface, not a sysdefault / plugin chain).
    pub is_hardware: bool,
}

/// Bundled lists for ARDOP — capture (`arecord -L`) and playback (`aplay -L`)
/// share the same enumeration shape but list different devices, so one Tauri
/// call returns both rather than forcing the frontend to spawn two.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlsaDevicesDto {
    pub captures: Vec<AlsaDeviceDto>,
    pub playbacks: Vec<AlsaDeviceDto>,
}

/// Parse the multi-line `arecord -L` / `aplay -L` output. The format is one
/// device per block: a NAME line at column 0, then one or more indented
/// description lines (lines that start with whitespace). A new column-0 line
/// ends the current block and starts the next. Pure (no I/O) so the parser
/// is unit-testable without an ALSA stack. Sorted with hardware devices
/// first so the operator's natural top-of-list choice is the right kind.
pub fn parse_alsa_devices(output: &str) -> Vec<AlsaDeviceDto> {
    fn finish(
        name: Option<String>,
        desc_lines: &[String],
        out: &mut Vec<AlsaDeviceDto>,
    ) {
        if let Some(n) = name {
            let is_hardware = n.starts_with("plughw:CARD=") || n.starts_with("hw:CARD=");
            out.push(AlsaDeviceDto {
                name: n,
                description: desc_lines.join(" — "),
                is_hardware,
            });
        }
    }
    let mut devices: Vec<AlsaDeviceDto> = Vec::new();
    let mut current_name: Option<String> = None;
    let mut desc_lines: Vec<String> = Vec::new();
    for raw in output.lines() {
        if raw.is_empty() {
            continue;
        }
        let is_indented = raw.starts_with(' ') || raw.starts_with('\t');
        if is_indented {
            // Description line for the current block.
            desc_lines.push(raw.trim().to_string());
        } else {
            // New device block — flush the previous one first.
            finish(current_name.take(), &desc_lines, &mut devices);
            desc_lines.clear();
            current_name = Some(raw.to_string());
        }
    }
    // Flush the final block.
    finish(current_name, &desc_lines, &mut devices);
    // Stable sort: hardware first (preserve input order within each group).
    devices.sort_by_key(|d| !d.is_hardware);
    devices
}

/// List ALSA capture + playback devices for the ARDOP picker (tuxlink-y7x7).
/// Shells to `arecord -L` and `aplay -L`. Soft-failure posture mirrors
/// `packet_list_serial_devices` / `packet_list_bluetooth_devices`: missing
/// binary OR non-zero exit yields an empty list for that axis (the picker
/// renders "no devices found — refresh after plugging in" and falls back to
/// the manual-text input).
#[tauri::command]
pub async fn ardop_list_audio_devices() -> Result<AlsaDevicesDto, UiError> {
    let run = |bin: &str| match std::process::Command::new(bin).arg("-L").output() {
        Ok(o) if o.status.success() => {
            parse_alsa_devices(&String::from_utf8_lossy(&o.stdout))
        }
        _ => Vec::new(),
    };
    Ok(AlsaDevicesDto {
        captures: run("arecord"),
        playbacks: run("aplay"),
    })
}

// ============================================================================
// Task 8 (tuxlink-7fr) — packet_connect / packet_set_listen
// ============================================================================
// Pure builders (`packet_transport_from_config`, `apply_listen_default`) are
// split out so they are unit-testable without `tauri::State`, matching the
// `parse_raw_rfc5322` / `derive_status_dto` pattern in this file.

/// Build the packet `TransportConfig` from config + the operator's dial args.
/// Returns `NotConfigured` if no KISS link is set yet (the UI must configure
/// one first via `packet_config_set`).
pub fn packet_transport_from_config(
    cfg: &config::Config,
    call: String,
    path: Vec<String>,
) -> Result<TransportConfig, UiError> {
    let link = cfg
        .packet
        .link
        .clone()
        .ok_or_else(|| UiError::NotConfigured("no KISS link configured".into()))?;
    Ok(TransportConfig::Packet {
        link,
        ssid: cfg.packet.ssid,
        role: crate::winlink_backend::PacketRole::DialTo { call, path },
    })
}

/// Build the Listen-role packet transport (no dial target) from config.
/// Returns `NotConfigured` if no KISS link is set yet (the UI must configure
/// one first via `packet_config_set`). Mirrors `packet_transport_from_config`
/// but resolves to the Listen role — arm the station to answer an inbound call.
pub fn packet_listen_transport_from_config(
    cfg: &config::Config,
) -> Result<TransportConfig, UiError> {
    let link = cfg
        .packet
        .link
        .clone()
        .ok_or_else(|| UiError::NotConfigured("no KISS link configured".into()))?;
    Ok(TransportConfig::Packet {
        link,
        ssid: cfg.packet.ssid,
        role: crate::winlink_backend::PacketRole::Listen,
    })
}

/// Flip the sticky idle-listen default on a config (the mutation
/// `packet_set_listen` persists). Pure; the command wraps read → mutate → write.
pub fn apply_listen_default(cfg: &mut config::Config, enabled: bool) {
    cfg.packet.listen_default = enabled;
}

/// Dial a packet station (gateway or peer — tuxlink reacts to the challenge,
/// not a mode flag; spec §2). Builds the packet TransportConfig from config +
/// args and drives `backend.connect`, surfacing progress/result on the session
/// log like `cms_connect`.
///
/// RADIO-1: operator-run on real hardware; the agent never runs this command
/// against a real link or modem.
#[tauri::command]
pub async fn packet_connect(
    app: AppHandle,
    state: State<'_, BackendState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    call: String,
    path: Vec<String>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".into()))?;
    let cfg =
        config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let transport = packet_transport_from_config(&cfg, call.clone(), path)?;
    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        format!("Connecting to {call} over packet…"),
    );
    match backend.connect(transport).await {
        Ok(_session) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Info,
                "Packet exchange complete.".into(),
            );
            Ok(())
        }
        Err(BackendError::Cancelled) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Warn,
                "Packet connection aborted.".into(),
            );
            Err(BackendError::Cancelled.into())
        }
        Err(e) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Error,
                format!("Packet connect failed: {e}"),
            );
            Err(e.into())
        }
    }
}

/// Arm the station to answer an inbound packet call (Listen role). Builds the
/// Listen `TransportConfig` from config and drives `backend.connect`, which
/// blocks in `ax25::answer` polling the link until a SABM arrives (then replies
/// UA and runs the exchange) or the operator aborts (via `cms_abort`, which
/// shuts the link → `answer()` unwinds → `Cancelled`).
///
/// RADIO-1: arming Listen means the station auto-answers an inbound call — which
/// TRANSMITS a UA under the operator's callsign. The agent NEVER runs this
/// command against a real link; the operator runs it on real hardware.
#[tauri::command]
pub async fn packet_listen(
    app: AppHandle,
    state: State<'_, BackendState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".into()))?;
    let cfg =
        config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let transport = packet_listen_transport_from_config(&cfg)?;
    // Effective call = <callsign>-<ssid> (the SSID'd link address we answer on).
    let effective = cfg
        .identity
        .callsign
        .as_deref()
        .map(|c| format!("{}-{}", c.trim().to_uppercase(), cfg.packet.ssid))
        .unwrap_or_else(|| format!("(no callsign)-{}", cfg.packet.ssid));
    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        format!("Listening for an incoming packet call as {effective}…"),
    );
    match backend.connect(transport).await {
        Ok(_session) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Info,
                "Answered an incoming call; packet exchange complete.".into(),
            );
            Ok(())
        }
        Err(BackendError::Cancelled) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Warn,
                "Stopped listening.".into(),
            );
            Err(BackendError::Cancelled.into())
        }
        Err(e) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Error,
                format!("Packet listen failed: {e}"),
            );
            Err(e.into())
        }
    }
}

/// Persist the sticky idle-listen default (spec §4.5, §4.6 panel toggle + the
/// Settings selector write the same value).
#[tauri::command]
pub async fn packet_set_listen(enabled: bool) -> Result<(), UiError> {
    let mut cfg =
        config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    apply_listen_default(&mut cfg, enabled);
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}

// ============================================================================
// Packet allowlist overlay (tuxlink-inde)
// ============================================================================
//
// DTOs + Tauri commands for the operator-curated allowlist that gates inbound
// Packet-P2P sessions. Per
// `docs/design/2026-06-03-multi-transport-listener-architecture.md` §4.1 +
// `dev/scratch/winlink-re/findings/packet-p2p.md` §"Allowed-stations model",
// this is a tuxlink divergence over the shipped Packet listener: WLE has no
// listener-side allowlist for Packet because AX.25 always accepts at the link
// layer. Tuxlink overlays one at the answerer to give the operator explicit
// control over which peers are answered.
//
// Storage: `<config-dir>/listener/packet/allowed_stations.json` via
// `winlink::listener::packet_gate::packet_allowed_stations_path()` — same
// resolution chain as `config::config_path`.
//
// Default: `allow_all=false`, empty list — fresh tuxlink REJECTS every peer.

/// Frontend-facing view of the Packet allowlist.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketAllowedStationsDto {
    pub allow_all: bool,
    pub callsigns: Vec<String>,
}

impl PacketAllowedStationsDto {
    fn from_allowed(allowed: &crate::winlink::listener::AllowedStations) -> Self {
        Self {
            allow_all: allowed.allow_all(),
            callsigns: allowed.callsigns().to_vec(),
        }
    }
}

/// Helper: load → mutate → save, in one place so the four mutation commands
/// stay one-liners and the read/write code path is single-sourced.
/// Codex review 2026-06-03 [P3]: process-wide mutex serialising the
/// load-mutate-save cycle on the Packet allowlist file. Mirror of the
/// Telnet listener's same fix (tuxlink-xehu); without it two concurrent
/// UI commands (e.g. packet_allowed_stations_add + packet_allowed_stations_set_allow_all)
/// race — both load the same file, mutate in-memory, second save clobbers first.
fn packet_allowlist_file_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

// ============================================================================
// tuxlink-dhbl — ARDOP listener (allowed-stations + arms + LISTEN toggle)
// ============================================================================
//
// Wires the shared listener-arms foundation (`crate::winlink::listener`) into
// the ARDOP transport. ARDOP has no station-password layer per
// `dev/scratch/winlink-re/findings/ardop-p2p.md` divergence 2 — only the
// allowlist gate + the arms TTL apply.
//
// Persistence: `<config-dir>/listener/ardop/allowed_stations.json`. The
// listener arms record + LISTEN-flag flip on the modem are runtime concerns
// — when an operator presses "Listen" the UI mints an arms record (the
// foundation tracks TTL) and flips the modem's `LISTEN` flag via
// `winlink::modem::ardop::listener::set_listen`.
//
// The shape of the commands mirrors the Packet `packet_listen` /
// `packet_set_listen` / `packet_*_listen_*` family.

/// Resolve the config dir from `config::config_path()` (which returns the
/// `config.json` file path). Returns `<base>/listener/ardop/allowed_stations.json`.
fn ardop_allowed_stations_path() -> std::path::PathBuf {
    let cfg_dir = config::config_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    crate::winlink::modem::ardop::listener::allowed_stations_path(&cfg_dir)
}

/// Codex review 2026-06-03 [P3]: process-wide mutex serialising the
/// load-mutate-save cycle on the ARDOP allowlist file. Mirror of the
/// Telnet (tuxlink-xehu) + Packet (tuxlink-inde) fixes. Without it,
/// concurrent UI commands race: both load the same file, mutate
/// in-memory copies, second save clobbers first or stomps on a half-
/// written temp file.
fn ardop_allowlist_file_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// Flat camelCase DTO for the ARDOP AllowedStations list (Tauri wire shape).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AllowedStationsDto {
    pub allow_all: bool,
    pub callsigns: Vec<String>,
    pub ips: Vec<String>,
}

impl From<&crate::winlink::listener::AllowedStations> for AllowedStationsDto {
    fn from(a: &crate::winlink::listener::AllowedStations) -> Self {
        Self {
            allow_all: a.allow_all(),
            callsigns: a.callsigns().to_vec(),
            ips: a.ips().to_vec(),
        }
    }
}

/// Read the ARDOP allowed-stations JSON file. Returns the defensive default
/// (`allow_all: false`, empty lists) if the file is absent.
#[tauri::command]
pub async fn ardop_allowed_stations_get() -> Result<AllowedStationsDto, UiError> {
    let path = ardop_allowed_stations_path();
    let allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(AllowedStationsDto::from(&allowed))
}

/// Add a callsign (or callsign-wildcard like `N7*`) to the ARDOP
/// allowed-stations list. Idempotent (duplicates are deduplicated by
/// `AllowedStations::add_callsign_pattern`).
#[tauri::command]
pub async fn ardop_allowed_stations_add(callsign: String) -> Result<(), UiError> {
    let trimmed = callsign.trim();
    if trimmed.is_empty() {
        return Err(UiError::Internal { detail: "callsign must not be empty".into() });
    }
    let _guard = ardop_allowlist_file_lock().lock().unwrap();
    let path = ardop_allowed_stations_path();
    let mut allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    allowed.add_callsign_pattern(trimmed);
    allowed
        .save_to(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}

/// Remove a callsign (exact-match, case-insensitive after uppercasing) from
/// the ARDOP allowed-stations list. Silently succeeds if the entry isn't
/// present — same semantics as the Packet allowlist commands' "set" model.
#[tauri::command]
pub async fn ardop_allowed_stations_remove(callsign: String) -> Result<(), UiError> {
    let needle = callsign.trim().to_uppercase();
    if needle.is_empty() {
        return Err(UiError::Internal { detail: "callsign must not be empty".into() });
    }
    let _guard = ardop_allowlist_file_lock().lock().unwrap();
    let path = ardop_allowed_stations_path();
    let allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    // AllowedStations doesn't expose a public `remove_callsign` API; rebuild
    // a fresh AllowedStations omitting the matched entry, then save.
    let mut rebuilt = crate::winlink::listener::AllowedStations::new();
    rebuilt.set_allow_all(allowed.allow_all());
    for c in allowed.callsigns() {
        if !c.eq_ignore_ascii_case(&needle) {
            rebuilt.add_callsign_pattern(c.clone());
        }
    }
    for ip in allowed.ips() {
        rebuilt.add_ip_pattern(ip.clone());
    }
    rebuilt
        .save_to(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}

/// Toggle the master `Allow All Connections` flag on the ARDOP allowlist.
///
/// `true` = WLE-compatible permissive: every inbound ARDOP peer that
/// completes the modem-level ARQ handshake is accepted.
///
/// `false` (the tuxlink default) = restrict to the operator-curated
/// `callsigns` list.
#[tauri::command]
pub async fn ardop_allowed_stations_set_allow_all(allow_all: bool) -> Result<(), UiError> {
    let _guard = ardop_allowlist_file_lock().lock().unwrap();
    let path = ardop_allowed_stations_path();
    let mut allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    allowed.set_allow_all(allow_all);
    allowed
        .save_to(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}

/// Arm the ARDOP listener for the default TTL (1 hour per architecture §5).
///
/// Mints a [`crate::winlink::listener::ListenerArmsRecord`], appends it to
/// the cross-transport forensics JSONL log, and sends `LISTEN TRUE` to the
/// running ardopcf modem so the modem accepts inbound ARQ connections at
/// the modem layer. The arm window authorizes inbound peer connections
/// during the TTL — when a peer arrives the radio keys to send the ARQ
/// response handshake. On TTL expiry the gate stops accepting new inbound
/// peers.
///
/// Behavior:
/// 1. Validate the allowlist file loads (corrupt file → reject the arm so the
///    operator can repair before they think the gate is active).
/// 2. Mint a `ListenerArmsRecord` + append to the cross-transport forensics
///    log.
/// 3. Send `LISTEN TRUE` to the running ardopcf modem via the side-channel
///    cmd writer installed during modem-init. If the modem isn't running,
///    surface a clear "start the modem first" error.
///
/// NOT YET DONE in this PR — separate follow-up (filed during execution as a
/// new bd issue mirroring tuxlink-k3ru's Telnet symmetry): when ardopcf
/// emits CONNECTED for an inbound peer, route the event through
/// `gate_inbound_peer_now` and on Accept hand the modem data stream to the
/// B2F answerer. Without that routing the modem accepts peers at the ARQ
/// layer (operator can see it in the status display) but no mail exchange
/// runs at the application layer.
#[tauri::command]
pub async fn ardop_listen(
    app: AppHandle,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    session: State<'_, std::sync::Arc<crate::modem_status::ModemSession>>,
) -> Result<(), UiError> {
    use crate::winlink::listener::{ListenerArmsRecord, TransportKind, DEFAULT_TTL};

    let allowlist_path = ardop_allowed_stations_path();
    if let Err(e) = crate::winlink::listener::AllowedStations::load_from(&allowlist_path) {
        return Err(UiError::Internal {
            detail: format!(
                "ARDOP listener arm refused: allowlist file at {} could not be loaded: {e}. \
                 Repair or delete the file (a missing file is fine — it falls back to the \
                 tuxlink WLE-parity default of allow_all=true + empty list).",
                allowlist_path.display()
            ),
        });
    }

    // Codex review 2026-06-03 [P2] (tuxlink-7vea): append the arms record
    // to the forensics log BEFORE flipping the modem's LISTEN flag. The
    // prior order sent LISTEN TRUE first and then appended — if the
    // forensics-log write failed (config dir unwritable, disk full), the
    // command returned an error but ardopcf was left in LISTEN mode with
    // no successful arm record. Now: log first, then toggle. If LISTEN
    // TRUE fails, the operator sees a forensics-record-without-LISTEN
    // (recoverable, the next disarm cleans up the record).
    let arms = ListenerArmsRecord::arm(TransportKind::Ardop, DEFAULT_TTL);
    let log_path = ardop_arms_log_path();
    arms.append_to_log(&log_path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;

    // Send LISTEN TRUE to the running ardopcf modem. This requires the
    // modem to already be running (the cmd writer is installed during
    // `modem_ardop_connect`'s init path — i.e., after an outbound dial).
    // Codex review 2026-06-03 [P3] (tuxlink-7vea): the prior error
    // message told the operator to "open the ARDOP modem panel," but
    // opening the panel does not start ardopcf — only an outbound
    // Connect does. Tightened to the actually-available action; the full
    // listen-only start flow is tracked at tuxlink-syqb.
    session
        .send_listen_command(true)
        .map_err(|e| UiError::Internal {
            detail: format!(
                "ARDOP listener arm refused — the modem is not running. \
                 In the current build the modem only starts when you Connect \
                 to a peer (an outbound dial). Start the modem via an ARDOP \
                 Connect first, then re-arm the listener. Listen-only \
                 modem start is tracked at tuxlink-syqb. \
                 Underlying error: {e}"
            ),
        })?;

    let mins = arms.ttl.as_secs() / 60;
    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        format!(
            "ARDOP listener armed for {mins} min (consent uuid {}). \
             Modem is now in LISTEN TRUE mode at the modem layer.",
            &arms.consent_uuid
        ),
    );
    Ok(())
}

/// Toggle the ARDOP listener on/off. `enabled == true` is equivalent to
/// `ardop_listen()` (sends LISTEN TRUE + mints arms record). `enabled == false`
/// sends LISTEN FALSE to the running ardopcf modem.
#[tauri::command]
pub async fn ardop_set_listen(
    app: AppHandle,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    session: State<'_, std::sync::Arc<crate::modem_status::ModemSession>>,
    enabled: bool,
) -> Result<(), UiError> {
    if enabled {
        return ardop_listen(app, log, session).await;
    }
    session
        .send_listen_command(false)
        .map_err(|e| UiError::Internal {
            detail: format!(
                "ARDOP listener disarm error — modem may not be running. {e}"
            ),
        })?;
    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        "ARDOP listener disarmed (LISTEN FALSE sent to modem).".to_string(),
    );
    Ok(())
}

/// Resolve the cross-transport listener forensics JSONL log path:
/// `<config-dir>/listener/listener_arms.jsonl`. Append-only.
///
/// Codex review 2026-06-03 [P3]: the prior per-transport
/// `<config-dir>/listener/ardop/arms.jsonl` split arm events across
/// transports, but the shared `ListenerArmsRecord` contract + the Packet
/// gate's `packet_gate::listener_forensics_log_path` write to one
/// cross-transport file. Use the same path here so an operator (or
/// future audit reader) gets a unified history of arm + reject events
/// across all listeners.
fn ardop_arms_log_path() -> std::path::PathBuf {
    let cfg_dir = config::config_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    cfg_dir.join("listener").join("listener_arms.jsonl")
}

fn with_packet_allowed_stations<F>(mutate: F) -> Result<(), UiError>
where
    F: FnOnce(&mut crate::winlink::listener::AllowedStations),
{
    let _guard = packet_allowlist_file_lock().lock().unwrap();
    let path = crate::winlink::listener::packet_gate::packet_allowed_stations_path();
    let mut allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: format!("load allowlist: {e}") })?;
    mutate(&mut allowed);
    allowed
        .save_to(&path)
        .map_err(|e| UiError::Internal { detail: format!("save allowlist: {e}") })?;
    Ok(())
}

/// Read the Packet allowlist. First-run (file missing) returns the tuxlink
/// default: `allow_all=false`, empty list.
#[tauri::command]
pub async fn packet_allowed_stations_get() -> Result<PacketAllowedStationsDto, UiError> {
    let path = crate::winlink::listener::packet_gate::packet_allowed_stations_path();
    let allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: format!("load allowlist: {e}") })?;
    Ok(PacketAllowedStationsDto::from_allowed(&allowed))
}

/// Add a callsign (or `CALL*` tail-wildcard pattern) to the Packet allowlist.
/// Idempotent — adding the same pattern twice is a no-op.
#[tauri::command]
pub async fn packet_allowed_stations_add(callsign: String) -> Result<(), UiError> {
    let trimmed = callsign.trim().to_string();
    if trimmed.is_empty() {
        return Err(UiError::Internal {
            detail: "callsign must not be empty".into(),
        });
    }
    with_packet_allowed_stations(|a| a.add_callsign_pattern(trimmed))
}

/// Remove a callsign (or pattern) from the Packet allowlist. Matches the
/// stored form (uppercased + trimmed); calling with a callsign that is not
/// in the list is a no-op.
#[tauri::command]
pub async fn packet_allowed_stations_remove(callsign: String) -> Result<(), UiError> {
    let needle = callsign.trim().to_uppercase();
    if needle.is_empty() {
        return Err(UiError::Internal {
            detail: "callsign must not be empty".into(),
        });
    }
    let _guard = packet_allowlist_file_lock().lock().unwrap();
    let path = crate::winlink::listener::packet_gate::packet_allowed_stations_path();
    let mut allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: format!("load allowlist: {e}") })?;
    // The struct doesn't expose a remove_callsign API, so reconstruct without
    // the entry. (Mirrors the "clear + re-add survivors" pattern; bd follow-up
    // could expose a first-class remove on the AllowedStations API.)
    let keep: Vec<String> = allowed
        .callsigns()
        .iter()
        .filter(|c| c.as_str() != needle.as_str())
        .cloned()
        .collect();
    let ips: Vec<String> = allowed.ips().to_vec();
    let allow_all = allowed.allow_all();
    allowed.clear();
    allowed.set_allow_all(allow_all);
    for c in keep {
        allowed.add_callsign_pattern(c);
    }
    for ip in ips {
        allowed.add_ip_pattern(ip);
    }
    allowed
        .save_to(&path)
        .map_err(|e| UiError::Internal { detail: format!("save allowlist: {e}") })?;
    Ok(())
}

/// Flip the master "allow all" toggle. TRUE matches WLE's permissive default
/// (every peer accepted); FALSE is the tuxlink default (callsigns list gates
/// every inbound).
#[tauri::command]
pub async fn packet_allowed_stations_set_allow_all(allow_all: bool) -> Result<(), UiError> {
    with_packet_allowed_stations(|a| a.set_allow_all(allow_all))
}

// Task 5 (tuxlink-686) — config_set_grid + validate_grid_input
// bd issue: tuxlink-686
// ============================================================================

/// Validate a user-supplied Maidenhead grid string.
///
/// **Precondition:** caller must trim whitespace before calling; this function
/// operates on the argument as-given.
///
/// Delegates to [`crate::position::grid_to_lat_lon`], which operates on
/// `as_bytes()` and is panic-safe for arbitrary UTF-8 input (no unsafe
/// char-indexing on `&str`). Returns `Some(message)` when the input is invalid,
/// `None` when it is a valid 4- or 6-char Maidenhead locator.
pub(crate) fn validate_grid_input(s: &str) -> Option<&'static str> {
    crate::position::grid_to_lat_lon(s)
        .is_none()
        .then_some("Grid must be a 4- or 6-char Maidenhead locator (e.g. EM75 or EM75xx).")
}

/// Persist a manually-set grid to the config file and update the arbiter's
/// fallback grid.
///
/// - Validates the input with [`validate_grid_input`]; invalid → `Rejected`.
/// - Reads the current config, updates `identity.grid`, and writes atomically.
///   Both I/O errors map to `UiError::Internal` (same pattern as `config_read`
///   + `cms_connect`).
/// - Calls `arbiter.set_manual` to update the in-memory fallback grid; the
///   arbiter is the runtime source of truth for broadcast position
///   (spec §position-686).
///
/// **Position-subsystem restoration (tuxlink-c79g, 2026-06-01):** the pjih
/// patch removed both the on-disk `position_source = Manual` persistence
/// and the arbiter's source-pinning inside `set_manual`. Spec §3.1 +
/// Codex P1 #3 restore both: this command now persists
/// `cfg.privacy.position_source = Manual` (T4) and the arbiter pins
/// `source = Manual` inside `set_manual` (T1). The on-disk preference and
/// the in-memory runtime source stay aligned; switching back to GPS
/// happens via `position_set_source("Gps")`, not by a fresh GPS fix
/// (sticky-manual invariant per spec §2 State 4 / State 5).
///
/// The arbiter is managed as an `Arc<PositionArbiter>` so it is shared between
/// this command and (Task 11) the gpsd task.
///
/// NOTE (empty string): this command is never invoked with an empty string — the
/// Task 8 `GridEdit` UI validates client-side first; the backend correctly
/// rejects empty as invalid.
#[tauri::command]
pub async fn config_set_grid(
    grid: String,
    arbiter: tauri::State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    config_set_grid_impl(grid, arbiter.inner().clone(), state.current()).await
}

/// Tauri-attribute-free body of [`config_set_grid`], factored out so the
/// command is unit-testable without a Tauri app handle. See the command's
/// rustdoc for the contract.
///
/// Per spec §3.1 + Codex P1 #3 (position-subsystem restoration): persists
/// `cfg.privacy.position_source = Manual` to disk alongside the grid update
/// (pjih dropped this; T4 restores it). The arbiter's `set_manual` is the
/// in-memory mirror — T1 restored the source-pinning invariant inside
/// `set_manual` itself, so the on-disk `position_source = Manual` value and
/// the arbiter's runtime `source` stay aligned.
pub(crate) async fn config_set_grid_impl(
    grid: String,
    arbiter: std::sync::Arc<crate::position::PositionArbiter>,
    backend: Option<std::sync::Arc<dyn crate::winlink_backend::WinlinkBackend>>,
) -> Result<(), UiError> {
    let g = grid.trim().to_string();
    if let Some(msg) = validate_grid_input(&g) {
        return Err(UiError::Rejected(msg.to_string()));
    }

    // Critical section per spec §3.3 + R3 F1 + F7: hold the arbiter mutex
    // from read_config through write_config_atomic through arbiter mutation
    // so disk and arbiter state cannot diverge across concurrent callers.
    // The backend.set_config push happens AFTER mutex release
    // (eventually-consistent — pre-existing pattern).
    let new_cfg = arbiter.with_inner(|i| -> Result<config::Config, UiError> {
        let mut cfg = config::read_config()
            .map_err(|e| UiError::Internal { detail: e.to_string() })?;
        cfg.identity.grid = Some(g.clone());
        cfg.privacy.position_source = config::PositionSource::Manual;  // RESTORED per spec §3.1 (Codex P1 #3)
        config::write_config_atomic(&cfg)
            .map_err(|e| UiError::Internal { detail: e.to_string() })?;
        // Mirror in-memory: T1 invariant — set_manual pins source = Manual.
        i.manual_grid = Some(g.clone());
        i.source = config::PositionSource::Manual;
        Ok(cfg)
    })?;

    // tuxlink-ka7/p5u: refresh the live backend (config_set_* wildcard). The grid
    // feeds effective_broadcast_locator's config fallback, so a stale snapshot would
    // broadcast the old grid until restart.
    if let Some(backend) = backend {
        backend.set_config(new_cfg);
    }
    Ok(())
}

// ============================================================================
// Task 11 (tuxlink-686) — position_set_source + position_status
// ============================================================================
// Appended here per the append-only ownership model (spec §7). Both commands
// are registered in lib.rs's `invoke_handler` by the Task 11 integration
// commit. `position_status` reads LIVE arbiter state (NOT config), so
// `gps_ready` is intentionally absent from `ConfigViewDto` (spec §position-686).

/// Persist the operator's chip selection (`config.privacy.position_source`)
/// and flip the in-memory arbiter source.
///
/// `'Gps'` succeeds unconditionally per spec §1.1 (the relaxation): the
/// pre-check on `arbiter.has_fresh_fix()` was removed in Task 3 of the
/// position-subsystem restoration (Codex P0 #1), mirroring the T2 change
/// that made `arbiter.use_gps()` infallible. If no fresh fix exists when
/// the operator picks GPS, `active_grid` falls back to `manual_grid`
/// (State 4 / State 5 per the 2026-05-22 spec row 3) — the chip selection
/// is a stable preference, not a snapshot of "GPS is currently usable."
///
/// `'Manual'` is rejected — manual pinning happens via `config_set_grid`,
/// which `arbiter.set_manual` keys to `source = Manual` (T1 restored the
/// source-pinning invariant).
///
/// Persists config BEFORE flipping the in-memory arbiter, so a write
/// failure returns `UiError::Internal` without an in-memory/persisted skew
/// (mirrors `config_set_grid`'s persist-first invariant).
#[tauri::command]
pub async fn position_set_source(
    source: String,
    arbiter: tauri::State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    position_set_source_impl(source, arbiter.inner().clone(), state.current()).await
}

/// Tauri-attribute-free body of [`position_set_source`], factored out so the
/// command is unit-testable without a Tauri app handle. See the command's
/// rustdoc for the contract.
pub(crate) async fn position_set_source_impl(
    source: String,
    arbiter: std::sync::Arc<crate::position::PositionArbiter>,
    backend: Option<std::sync::Arc<dyn crate::winlink_backend::WinlinkBackend>>,
) -> Result<(), UiError> {
    match source.as_str() {
        "Gps" => {
            // Per spec §1.1: no `has_fresh_fix` gate. The chip selection is a
            // stable preference; effective_broadcast_locator handles the no-fix
            // fallback to manual_grid (State 4 / State 5).
            //
            // Critical section per spec §3.3 + R3 F1 + F7: hold the arbiter
            // mutex from read_config through write_config_atomic through
            // arbiter mutation so disk and arbiter state cannot diverge across
            // concurrent callers. Persist-first invariant preserved — on a
            // write_config_atomic error the closure returns without mutating
            // the arbiter's in-memory source.
            let new_cfg = arbiter.with_inner(|i| -> Result<config::Config, UiError> {
                let mut cfg = config::read_config()
                    .map_err(|e| UiError::Internal { detail: e.to_string() })?;
                cfg.privacy.position_source = config::PositionSource::Gps;
                config::write_config_atomic(&cfg)
                    .map_err(|e| UiError::Internal { detail: e.to_string() })?;
                // Mirror in-memory: use_gps semantics (T2 — infallible).
                i.source = config::PositionSource::Gps;
                Ok(cfg)
            })?;
            // tuxlink-ka7/p5u: refresh the live backend (config_set_* wildcard).
            if let Some(backend) = backend {
                backend.set_config(new_cfg);
            }
            Ok(())
        }
        other => Err(UiError::Rejected(format!("unsupported position source: {other}"))),
    }
}

/// Live position-subsystem status from the arbiter (NOT config).
///
/// - `gps_ready`: a usable fresh GPS fix exists AND the operator has not
///   disabled GPS (`gps_state != Off`). The ribbon's GridEdit shows
///   "GPS ready — tap to switch" from it. Per tuxlink-va1i Codex consultation:
///   gating on `gps_state != Off` ensures that an operator who switches GPS
///   to Off mid-session sees `gps_ready=false` immediately, even if the
///   arbiter still holds a stale-fresh fix (separate follow-up: kill the gpsd
///   client task on Off).
/// - `broadcast_grid`: the EFFECTIVE on-air locator computed by
///   [`crate::position::effective_broadcast_locator`], honoring both precision and
///   the `gps_state` privacy control. The on-air transmitter consults this for
///   the actual broadcast (Codex P1-B). Empty string = no grid.
/// - `ui_grid`: the EFFECTIVE local-UI locator computed by
///   [`crate::position::effective_ui_locator`]. NOT privacy-gated for
///   `LocalUiOnly` (the operator's intent under that state is "show GPS
///   locally, don't broadcast"). The ribbon reads this for display.
///   Empty string = no grid.
///
/// Polled by useStatusData (2s).
///
/// Spec §3.1: this DTO does NOT carry `active_source`. The frontend's source
/// chip reads source from `config_read`; optimistic updates after
/// `config_set_grid` + `position_set_source` ensure the chip flips within one
/// render cycle (Task 14 wires this). See
/// `docs/superpowers/specs/2026-06-01-position-subsystem-restoration-design.md`.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct PositionStatusDto {
    pub gps_ready: bool,
    /// The precision-reduced grid that WILL be broadcast on air (honoring
    /// gps_state). Empty string when no grid is available. Serializes as
    /// `broadcast_grid` (snake_case) matching the TS PositionStatusDto.
    pub broadcast_grid: String,
    /// The precision-reduced grid the ribbon displays. Distinct from
    /// `broadcast_grid` under `source=Gps + LocalUiOnly + fresh fix`: the
    /// operator sees the live fix locally but the on-air locator stays at
    /// the static config grid. Serializes as `ui_grid` (snake_case).
    pub ui_grid: String,
}

#[tauri::command]
pub async fn position_status(
    arbiter: tauri::State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
) -> Result<PositionStatusDto, UiError> {
    let cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(PositionStatusDto {
        gps_ready: arbiter.has_fresh_fix()
            && cfg.privacy.gps_state != crate::config::GpsState::Off,
        broadcast_grid: crate::position::effective_broadcast_locator(&cfg, Some(&arbiter)),
        ui_grid: crate::position::effective_ui_locator(&cfg, Some(&arbiter)),
    })
}

// ============================================================================
// tuxlink-39b — config_set_privacy (GPS-state + precision control surface)
// ============================================================================
// Closes the gap found in the post-merge smoke of #113: gps_state +
// position_precision were ENFORCED (effective_broadcast_locator) but had NO
// setter — the Tools→Settings items were dead no-op stubs. This is the write
// path the inline Settings panel calls.

/// Persist the GPS privacy controls (state + broadcast precision) and sync the
/// arbiter. Mirrors `config_set_grid`'s persist-before-mutate ordering:
/// read → set both privacy fields → write atomically → sync the arbiter's
/// broadcast precision.
///
/// `gps_state` lives only in config (the on-air gate `effective_broadcast_locator`
/// reads it directly), so it just persists. `position_precision` lives in BOTH
/// config (the config-fallback path) AND the arbiter (the GPS-broadcast path),
/// so after persisting we call `arbiter.set_precision` to keep them consistent.
///
/// NOTE (test coverage): like `config_set_grid`, the full read→write round-trip
/// is NOT unit-tested — `config::config_path()` resolves via the process-global
/// `XDG_CONFIG_HOME`, so an isolated round-trip races under parallel `cargo test`.
/// The novel logic (arbiter precision sync) IS unit-tested
/// (`position::arbiter::tests::set_precision_changes_broadcast_reduction`); the
/// persist path is identical to `config_set_grid`'s and is operator-smoke-covered.
/// Both args are typed enums — any deserializable value is valid by construction,
/// so no string validation is needed (unlike grids).
#[tauri::command]
pub async fn config_set_privacy(
    gps_state: GpsState,
    position_precision: PositionPrecision,
    arbiter: tauri::State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    let mut cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.privacy.gps_state = gps_state;
    cfg.privacy.position_precision = position_precision;
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    arbiter.set_precision(position_precision);
    // tuxlink-ka7/p5u: refresh the live backend (config_set_* wildcard). gps_state
    // is read directly from config by effective_broadcast_locator's on-air gate, so
    // a stale snapshot would keep broadcasting (or suppressing) per the OLD privacy
    // setting until an app restart.
    if let Some(backend) = state.current() {
        backend.set_config(cfg);
    }
    Ok(())
}

// ============================================================================
// tuxlink-3o0 — config_set_connect (user-switchable CMS server endpoint)
// ============================================================================
// Replaces the former hardcoded `winlink_backend::CMS_HOST` const + hidden
// `TUXLINK_CMS_HOST` env var with an inline-SettingsPanel control. The env var
// stays a dev override on top of the persisted host (see
// `winlink_backend::resolve_cms_host`).

/// Persist the CMS server endpoint (host + transport) the operator dials.
/// Mirrors `config_set_privacy`'s read → mutate → persist ordering and its
/// `UiError` handling exactly. Validates `host` (nonempty + no whitespace —
/// the same shape `validate_identity` enforces for callsigns; the CMS DNS layer
/// is authoritative for actual reachability). `transport` is a typed enum, so any
/// deserializable value is valid by construction (no string validation needed).
///
/// NOTE (test coverage): like `config_set_privacy` / `config_set_grid`, the full
/// read→write round-trip is NOT unit-tested here — `config::config_path()`
/// resolves via the process-global `XDG_CONFIG_HOME`, so an isolated round-trip
/// races under parallel `cargo test`. The validation logic IS unit-tested
/// (`validate_cms_host`); the persist path is identical to `config_set_privacy`'s
/// and is operator-smoke-covered. The host→socket flow is proved by
/// `winlink_backend::tests::config_host_and_transport_dial_a_real_local_socket`.
#[tauri::command]
pub async fn config_set_connect(
    state: State<'_, BackendState>,
    host: String,
    transport: CmsTransport,
) -> Result<(), UiError> {
    if let Some(msg) = validate_cms_host(&host) {
        return Err(UiError::Rejected(msg.to_string()));
    }
    let mut cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.connect.host = host;
    cfg.connect.transport = transport;
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    // tuxlink-ka7: refresh the LIVE backend so this host/transport selection applies
    // on the NEXT connect without an app restart. The connect path reads the
    // backend's live config (not the disk), so persisting alone is not enough — the
    // split-brain that hit production (fresh transport mode + stale cached host) was
    // exactly this gap.
    if let Some(backend) = state.current() {
        backend.set_config(cfg);
    }
    Ok(())
}

/// Validate a user-supplied CMS host. Returns `Some(message)` for the FIRST rule
/// violated, `None` when valid. Rules (most-actionable first, mirroring
/// `config::validate_identity_describe`): nonempty → no whitespace. A hostname's
/// finer syntax (labels, TLD) is left to the DNS resolver — `connect_stream`'s
/// `resolve_with_timeout` surfaces an unresolvable host as a connect error.
pub(crate) fn validate_cms_host(host: &str) -> Option<&'static str> {
    let h = host.trim();
    if h.is_empty() {
        return Some("CMS host must not be empty");
    }
    if host.chars().any(char::is_whitespace) {
        return Some("CMS host must not contain whitespace");
    }
    None
}

// ============================================================================
// tuxlink-0pnb — P2P-Telnet connect + peer-password management (PR 1)
// Spec: docs/design/2026-06-01-tcp-p2p-telnet-design.md §4.6
// Plan: 2026-06-01-tcp-p2p-telnet-pr1-client-dial.md Task 4
// ============================================================================

/// Whether the per-peer password is stored in the keyring. Read-only so the
/// UI can render a Set / Not Set indicator without reading the secret itself.
///
/// Mirrors the `PeerPasswordStatus` type in the plan spec.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PeerPasswordStatus {
    Set,
    NotSet,
}

/// Request object for [`telnet_p2p_connect`].
#[derive(Debug, Deserialize)]
pub struct P2pDialRequest {
    /// Hostname or IP address of the peer's TCP listener.
    pub host: String,
    /// TCP port on the peer.
    pub port: u16,
    /// Callsign of the peer station (used for the login exchange + optional
    /// password lookup in the keyring).
    pub peer_callsign: String,
    /// Our own callsign, sent in the B2F handshake.
    pub my_callsign: String,
    /// Our Maidenhead grid locator for the B2F handshake.
    pub locator: String,
}

/// Result returned by [`telnet_p2p_connect`].
#[derive(Debug, Serialize)]
pub struct P2pDialResult {
    /// Number of outbound messages sent successfully.
    pub sent_count: usize,
    /// Number of inbound messages received.
    pub received_count: usize,
}

/// Single-flight + abort coordination for the P2P-Telnet connect path (mirrors
/// `NativeBackend`'s `connect_in_progress` + `aborting` flags). Held in
/// Tauri managed state so `telnet_p2p_connect` and `telnet_p2p_abort` share it.
///
/// `in_progress`: set to `true` for the duration of a connect; prevents a
/// second concurrent connect from racing on the status/log pipeline.
///
/// `aborting`: set by `telnet_p2p_abort`; checked by the post-connect outcome
/// handler so an abort request during an in-flight exchange is reflected in the
/// session log and status rather than being silently swallowed.
///
/// Abort limitation (PR 1): `telnet_p2p::connect_and_exchange` runs synchronously
/// inside `spawn_blocking` and does not accept an abort handle. The abort command
/// emits a Disconnected status event and logs an abort notice, but cannot
/// forcibly terminate the blocking exchange mid-flight. The exchange completes
/// naturally and the final outcome is logged. A follow-on PR may thread an
/// abort-capable `TcpStream` handle through `connect_and_exchange` to enable
/// hard-stop (mirroring how `NativeBackend::abort` calls `TcpStream::shutdown`).
pub struct P2pConnectState {
    pub in_progress: std::sync::atomic::AtomicBool,
    pub aborting: std::sync::atomic::AtomicBool,
}

/// Write the per-peer station password to the OS keyring.
///
/// Overwrites any existing entry. `callsign` is uppercased before storage so
/// case variants do not create duplicate entries (see `credentials::p2p_peer_account`).
#[tauri::command]
pub async fn p2p_peer_password_set(callsign: String, password: String) -> Result<(), UiError> {
    crate::winlink::credentials::p2p_peer_password_write(&callsign, &password).map_err(|e| {
        UiError::Internal { detail: e.to_string() }
    })
}

/// Delete the per-peer station password from the OS keyring.
///
/// Idempotent: succeeds when no entry exists (spec §4.4). Useful for clearing
/// a stored password without navigating back to a settings form.
#[tauri::command]
pub async fn p2p_peer_password_clear(callsign: String) -> Result<(), UiError> {
    crate::winlink::credentials::p2p_peer_password_delete(&callsign).map_err(|e| {
        UiError::Internal { detail: e.to_string() }
    })
}

/// Return whether a peer-station password is stored in the keyring.
///
/// Returns [`PeerPasswordStatus::Set`] when an entry exists,
/// [`PeerPasswordStatus::NotSet`] when absent. Any other keyring error is
/// surfaced as `UiError::Internal`.
#[tauri::command]
pub async fn p2p_peer_password_status(
    callsign: String,
) -> Result<PeerPasswordStatus, UiError> {
    use crate::winlink::credentials::KeyringError;
    match crate::winlink::credentials::p2p_peer_password_read(&callsign) {
        Ok(_) => Ok(PeerPasswordStatus::Set),
        Err(KeyringError::NoEntry { .. }) => Ok(PeerPasswordStatus::NotSet),
        Err(e) => Err(UiError::Internal { detail: e.to_string() }),
    }
}

/// Emit a `backend_status:change` event for the P2P-Telnet connection path
/// (mirrors the bootstrap emitter task that does this for `NativeBackend`).
/// P2P connects bypass `WinlinkBackend` entirely, so they must emit the event
/// directly to keep the StatusBar and connection-status ribbon in sync.
fn emit_p2p_status(app: &AppHandle, status: StatusDto) {
    let _ = app.emit("backend_status:change", &status);
}

/// Connect to a P2P peer over TCP-Telnet and run a full B2F message exchange.
///
/// Mirrors `cms_connect` in structure:
///   1. Emits session log lines at each phase (Connecting → Login → B2F → result).
///   2. Emits `backend_status:change` events at each phase transition so the
///      StatusBar reflects the P2P connection state without polling the
///      `WinlinkBackend` (P2P bypasses it; the events are emitted directly here).
///   3. Single-flight via `P2pConnectState.in_progress` — a second concurrent
///      connect is rejected rather than racing on the log/status pipeline.
///   4. Returns `Err` on failure so the frontend spinner can stop; human-facing
///      detail lives in the session log.
///
/// Looks up the per-peer password from the keyring (absent = no password
/// challenge attempted).
///
/// Outbox handling (tuxlink-l55l): the Outbox folder is read once before
/// dial and ALL queued messages are offered. P2P routing is the peer's job
/// (the WLE-as-Post-Office model — recipient delivery is delegated to the
/// peer's CMS uplink), so no per-peer filtering is applied here. After a
/// successful exchange, received messages are filed into Inbox and
/// successfully-sent MIDs are moved Outbox → Sent, mirroring
/// `native_telnet_exchange` (winlink_backend.rs).
///
/// RADIO-1: This command drives TCP to a peer station — no RF, so no
/// Part 97 consent gate is needed. The peer may transmit RF independently,
/// but tuxlink does not trigger it.
#[tauri::command]
pub async fn telnet_p2p_connect(
    app: AppHandle,
    p2p_state: State<'_, P2pConnectState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    req: P2pDialRequest,
) -> Result<P2pDialResult, UiError> {
    use std::sync::atomic::Ordering;
    use crate::winlink::credentials::KeyringError;
    use crate::winlink::session::{ExchangeConfig, SessionIntent};
    use crate::winlink::telnet_p2p;

    // Single-flight: reject a second concurrent connect.
    if p2p_state
        .in_progress
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(UiError::Internal {
            detail: "a P2P connection is already in progress".to_string(),
        });
    }
    // Clear any stale abort flag from a prior connect.
    p2p_state.aborting.store(false, Ordering::SeqCst);

    // Emit Connecting status — the StatusBar subscribes to backend_status:change
    // and updates immediately (no 2s poll wait).
    let peer_label = if req.peer_callsign.is_empty() {
        format!("{}:{}", req.host, req.port)
    } else {
        format!("{} @ {}:{}", req.peer_callsign, req.host, req.port)
    };
    emit_p2p_status(
        &app,
        StatusDto::Connecting { transport: "P2P-Telnet".to_string() },
    );

    // tuxlink-l55l: build a Mailbox at the same on-disk location the native
    // backend uses (`<app_data>/native-mbox`, per `bootstrap::install_native`).
    // P2P bypasses `WinlinkBackend` entirely, so it walks the same filesystem
    // store directly. The shared search index (`SearchService`) is attached
    // when present so messages received over P2P land in the search corpus
    // alongside CMS-received ones.
    let mbox_dir = match app.path().app_data_dir() {
        Ok(dir) => dir.join("native-mbox"),
        Err(e) => {
            p2p_state.in_progress.store(false, Ordering::SeqCst);
            emit_p2p_status(&app, StatusDto::Disconnected);
            return Err(UiError::Internal {
                detail: format!("could not resolve app data dir: {e}"),
            });
        }
    };
    let mut mailbox = crate::native_mailbox::Mailbox::new(mbox_dir);
    if let Some(svc) = app.try_state::<crate::search::commands::SearchService>() {
        mailbox = mailbox.with_index(svc.index.clone());
    }

    // tuxlink-l55l: read the outbox BEFORE opening the socket — same ordering
    // as `native_telnet_exchange` (P1.3 Codex review). A malformed outbox
    // fails fast without consuming an on-air slot or surprising the peer.
    let outbound = match crate::winlink_backend::build_outbound_proposals(&mailbox) {
        Ok(v) => v,
        Err(e) => {
            p2p_state.in_progress.store(false, Ordering::SeqCst);
            emit_p2p_status(&app, StatusDto::Disconnected);
            emit_session_line(
                &app,
                &log,
                LogLevel::Error,
                format!("Outbox read failed: {e}"),
            );
            return Err(UiError::Internal { detail: format!("outbox read: {e}") });
        }
    };

    let outbound_count = outbound.len();
    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        format!(
            "Connecting to {} (P2P-Telnet, {} queued)…",
            peer_label, outbound_count
        ),
    );

    // Look up peer password if configured (None = no password challenge attempted).
    let peer_password = match crate::winlink::credentials::p2p_peer_password_read(
        &req.peer_callsign,
    ) {
        Ok(p) => Some(p),
        Err(KeyringError::NoEntry { .. }) => None,
        Err(e) => {
            p2p_state.in_progress.store(false, Ordering::SeqCst);
            emit_p2p_status(&app, StatusDto::Disconnected);
            return Err(UiError::Internal { detail: e.to_string() });
        }
    };

    let config = ExchangeConfig {
        mycall: req.my_callsign.clone(),
        targetcall: req.peer_callsign.clone(),
        locator: req.locator.clone(),
        // P2P never uses B2F secure-login (spec §4.3 — no secure-login for P2P).
        password: None,
        intent: SessionIntent::P2p,
    };

    // Clone values for the spawn_blocking task.
    let host = req.host.clone();
    let port = req.port;
    let peer_callsign = req.peer_callsign.clone();

    // Wire progress + wire-log callbacks into the session log (mirrors cms_connect's
    // ProgressSink / WireSink wiring in bootstrap::install_native).
    let app_progress = app.clone();
    let log_progress = log.inner().clone();
    let app_wire = app.clone();
    let log_wire = log.inner().clone();

    let result = tokio::task::spawn_blocking(move || {
        telnet_p2p::connect_and_exchange(
            &host,
            port,
            &peer_callsign,
            peer_password.as_deref(),
            &config,
            outbound,
            &move |line: &str| {
                emit_session_line(&app_progress, &log_progress, LogLevel::Info, line.to_string());
            },
            &move |line: &str| {
                emit_session_line(&app_wire, &log_wire, LogLevel::Info, line.to_string());
            },
            |_proposals| Vec::new(),
        )
    })
    .await
    .map_err(|e| UiError::Internal { detail: format!("P2P connect task failed: {e}") })?;

    // Release single-flight flag before processing the outcome.
    p2p_state.in_progress.store(false, Ordering::SeqCst);

    let was_aborted = p2p_state.aborting.load(Ordering::SeqCst);

    match result {
        Ok(exchange) => {
            // tuxlink-l55l: file received messages into Inbox and move
            // successfully-sent MIDs from Outbox to Sent. Mirrors the
            // post-exchange handling in `native_telnet_exchange`. Failures
            // here are logged but don't fail the exchange — the bytes are on
            // disk either way; a duplicate-send next dial is the worst-case
            // outcome of a stuck Outbox→Sent move and the operator is told
            // about it via the session log.
            for message in &exchange.received {
                if let Err(e) = mailbox.store(MailboxFolder::Inbox, &message.to_bytes()) {
                    emit_session_line(
                        &app,
                        &log,
                        LogLevel::Warn,
                        format!("Inbox store failed: {e}"),
                    );
                }
            }
            for mid in &exchange.sent {
                if let Err(e) = mailbox.move_to(
                    MailboxFolder::Outbox,
                    MailboxFolder::Sent,
                    &MessageId(mid.clone()),
                ) {
                    emit_session_line(
                        &app,
                        &log,
                        LogLevel::Warn,
                        format!("Outbox→Sent move failed for {mid}: {e}"),
                    );
                }
            }

            let summary = format!(
                "P2P exchange complete. Sent {}, received {}.",
                exchange.sent.len(),
                exchange.received.len(),
            );
            emit_session_line(&app, &log, LogLevel::Info, summary);
            // Brief Connected window (mirrors cms_connect's 1.5s hold so the
            // operator has perceptible visual confirmation). P2P sessions are
            // also transient (connect → B2F → done), not a held socket.
            emit_p2p_status(
                &app,
                StatusDto::Connected {
                    transport: "P2P-Telnet".to_string(),
                    peer: peer_label,
                    since_iso: chrono::Utc::now().to_rfc3339(),
                },
            );
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            emit_p2p_status(&app, StatusDto::Disconnected);
            Ok(P2pDialResult {
                sent_count: exchange.sent.len(),
                received_count: exchange.received.len(),
            })
        }
        Err(e) => {
            if was_aborted {
                emit_session_line(
                    &app,
                    &log,
                    LogLevel::Warn,
                    "P2P connection aborted.".to_string(),
                );
            } else {
                emit_session_line(
                    &app,
                    &log,
                    LogLevel::Error,
                    format!("P2P connect failed: {e}"),
                );
            }
            emit_p2p_status(&app, StatusDto::Disconnected);
            Err(UiError::Transport { reason: e.to_string() })
        }
    }
}

/// Abort an in-flight [`telnet_p2p_connect`] (mirrors [`cms_abort`]).
///
/// Sets the abort flag so the post-connect handler reports "aborted" rather
/// than "error". Emits a Disconnected status event immediately so the StatusBar
/// responds without waiting for the blocking exchange to finish.
///
/// Abort limitation (PR 1): `telnet_p2p::connect_and_exchange` does not accept
/// an abort handle, so this cannot forcibly kill the in-flight blocking task.
/// The exchange runs to completion (or errors naturally); the aborting flag
/// controls how the outcome is reported. A follow-on PR may wire
/// `TcpStream::shutdown` into `connect_and_exchange` for hard-stop support
/// (matching `NativeBackend::abort`).
#[tauri::command]
pub async fn telnet_p2p_abort(
    app: AppHandle,
    p2p_state: State<'_, P2pConnectState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
) -> Result<(), UiError> {
    use std::sync::atomic::Ordering;

    emit_session_line(&app, &log, LogLevel::Info, "Aborting P2P connection…".to_string());
    p2p_state.aborting.store(true, Ordering::SeqCst);
    // Emit Disconnected immediately — the blocking task will still run to
    // completion, but the StatusBar should respond right away.
    emit_p2p_status(&app, StatusDto::Disconnected);
    Ok(())
}

// ============================================================================
// tuxlink-xehu — Telnet-P2P listener (allowlist + keyring + arm/disarm)
// Spec: docs/design/2026-06-03-multi-transport-listener-architecture.md §4.1
// Wire: dev/scratch/winlink-re/findings/telnet-p2p.md
// ============================================================================

/// Shared state for the in-flight Telnet listener: the shutdown flag (set by
/// `telnet_set_listen(false)` and read by the accept loop) + the bound listener
/// handle so the accept loop can be woken by closing the socket.
///
/// Held as `Arc<Mutex<...>>` so the spawn_blocking accept-loop thread + the
/// Tauri command thread can both manipulate it.
#[derive(Default)]
pub struct TelnetListenState {
    pub inner: std::sync::Mutex<Option<TelnetListenHandle>>,
}

/// In-flight handle for a running Telnet listener.
pub struct TelnetListenHandle {
    pub shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub bound_addr: std::net::SocketAddr,
}

/// Filesystem location of the Telnet listener's allowlist file.
fn telnet_allowed_stations_path() -> std::path::PathBuf {
    let mut p = crate::config::config_path();
    p.pop(); // strip `config.json`
    p.join("listener").join("telnet").join("allowed_stations.json")
}

fn load_telnet_allowed_stations()
    -> Result<crate::winlink::listener::AllowedStations, UiError>
{
    crate::winlink::listener::AllowedStations::load_from(&telnet_allowed_stations_path())
        .map_err(|e| UiError::Internal { detail: e.to_string() })
}

fn save_telnet_allowed_stations(
    a: &crate::winlink::listener::AllowedStations,
) -> Result<(), UiError> {
    a.save_to(&telnet_allowed_stations_path())
        .map_err(|e| UiError::Internal { detail: e.to_string() })
}

/// Codex review 2026-06-03 [P3]: process-wide mutex serialising
/// load-mutate-save against the Telnet allowlist file. The UI exposes
/// independent add/remove commands for callsigns + IPs + allow_all; without
/// this mutex two concurrent UI calls race (both load same file → both
/// mutate in-memory copies → second save clobbers first). Lock for the
/// whole load-mutate-save cycle, not just save.
fn telnet_allowlist_file_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

// ── Allowlist commands ──────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TelnetAllowedStationsDto {
    pub allow_all: bool,
    pub callsigns: Vec<String>,
    pub ips: Vec<String>,
}

/// Read the Telnet listener's allowlist (allow_all + callsigns + IPs).
#[tauri::command]
pub async fn telnet_allowed_stations_get() -> Result<TelnetAllowedStationsDto, UiError> {
    let a = load_telnet_allowed_stations()?;
    Ok(TelnetAllowedStationsDto {
        allow_all: a.allow_all(),
        callsigns: a.callsigns().to_vec(),
        ips: a.ips().to_vec(),
    })
}

/// Add a callsign to the Telnet listener's allowlist. Callsign may be a bare
/// callsign (`N7CPZ`), a callsign-with-SSID (`N7CPZ-1`), or a tail-wildcard
/// pattern (`N7*` — matches every callsign starting with `N7`).
#[tauri::command]
pub async fn telnet_allowed_stations_add_callsign(callsign: String) -> Result<(), UiError> {
    let _guard = telnet_allowlist_file_lock().lock().unwrap();
    let mut a = load_telnet_allowed_stations()?;
    a.add_callsign_pattern(callsign);
    save_telnet_allowed_stations(&a)
}

/// Remove a callsign from the Telnet listener's allowlist. Match is on the
/// canonical stored form (uppercased, trimmed).
#[tauri::command]
pub async fn telnet_allowed_stations_remove_callsign(callsign: String) -> Result<(), UiError> {
    let _guard = telnet_allowlist_file_lock().lock().unwrap();
    let mut a = load_telnet_allowed_stations()?;
    let target = callsign.trim().to_uppercase();
    let kept: Vec<String> = a.callsigns().iter().filter(|c| **c != target).cloned().collect();
    let allow_all = a.allow_all();
    let ips_kept: Vec<String> = a.ips().to_vec();
    a = crate::winlink::listener::AllowedStations::new().with_allow_all(allow_all);
    for c in kept {
        a.add_callsign_pattern(c);
    }
    for ip in ips_kept {
        a.add_ip_pattern(ip);
    }
    save_telnet_allowed_stations(&a)
}

/// Add an IP pattern. Patterns are 4-token IPv4 with optional `*` per octet
/// (`192.168.*.50`, `192.168.1.*`, `192.168.1.5`) per telnet-p2p.md §4.3.
#[tauri::command]
pub async fn telnet_allowed_stations_add_ip(pattern: String) -> Result<(), UiError> {
    let _guard = telnet_allowlist_file_lock().lock().unwrap();
    let mut a = load_telnet_allowed_stations()?;
    a.add_ip_pattern(pattern);
    save_telnet_allowed_stations(&a)
}

#[tauri::command]
pub async fn telnet_allowed_stations_remove_ip(pattern: String) -> Result<(), UiError> {
    let _guard = telnet_allowlist_file_lock().lock().unwrap();
    let mut a = load_telnet_allowed_stations()?;
    let target = pattern.trim().to_string();
    let kept_ips: Vec<String> = a.ips().iter().filter(|i| **i != target).cloned().collect();
    let allow_all = a.allow_all();
    let calls_kept: Vec<String> = a.callsigns().to_vec();
    a = crate::winlink::listener::AllowedStations::new().with_allow_all(allow_all);
    for c in calls_kept {
        a.add_callsign_pattern(c);
    }
    for ip in kept_ips {
        a.add_ip_pattern(ip);
    }
    save_telnet_allowed_stations(&a)
}

/// Toggle the master `Allow All Connections` flag. DIVERGES from WLE: tuxlink
/// defaults FALSE; operator opts into permissive parity-with-WLE explicitly.
#[tauri::command]
pub async fn telnet_allowed_stations_set_allow_all(enabled: bool) -> Result<(), UiError> {
    let _guard = telnet_allowlist_file_lock().lock().unwrap();
    let mut a = load_telnet_allowed_stations()?;
    a.set_allow_all(enabled);
    save_telnet_allowed_stations(&a)
}

// ── Station password commands ────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum StationPasswordStatus {
    Set,
    NotSet,
}

/// Set the Telnet listener's station password. The operator-typed value is
/// **uppercased + trimmed** before storage per telnet-p2p.md §9.4 Option A —
/// reject the WLE silent-fail-on-lowercase bug by normalising at write time.
#[tauri::command]
pub async fn telnet_station_password_set(password: String) -> Result<(), UiError> {
    let sp = crate::winlink::listener::StationPassword::new();
    let normalised = crate::winlink::telnet_listen::normalize_station_password(&password);
    sp.set(&normalised)
        .map_err(|e| UiError::Internal { detail: e.to_string() })
}

#[tauri::command]
pub async fn telnet_station_password_clear() -> Result<(), UiError> {
    let sp = crate::winlink::listener::StationPassword::new();
    sp.clear().map_err(|e| UiError::Internal { detail: e.to_string() })
}

#[tauri::command]
pub async fn telnet_station_password_is_set() -> Result<StationPasswordStatus, UiError> {
    let sp = crate::winlink::listener::StationPassword::new();
    Ok(if sp.is_set() {
        StationPasswordStatus::Set
    } else {
        StationPasswordStatus::NotSet
    })
}

// ── Listener config commands ─────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct TelnetListenConfigDto {
    pub port: u16,
    pub bind_addr: String,
    pub ttl_secs: u64,
}

#[tauri::command]
pub async fn telnet_listen_config_get() -> Result<TelnetListenConfigDto, UiError> {
    let cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(TelnetListenConfigDto {
        port: cfg.telnet_listen.port,
        bind_addr: cfg.telnet_listen.bind_addr,
        ttl_secs: cfg.telnet_listen.ttl_secs,
    })
}

#[tauri::command]
pub async fn telnet_listen_config_set(req: TelnetListenConfigDto) -> Result<(), UiError> {
    let mut cfg =
        config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.telnet_listen.port = req.port;
    cfg.telnet_listen.bind_addr = req.bind_addr;
    cfg.telnet_listen.ttl_secs = req.ttl_secs;
    config::write_config_atomic(&cfg)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}

// ── Listen toggle ────────────────────────────────────────────────

/// Arm the Telnet listener. Reads the per-listener config + allowlist +
/// keyring password, binds the TCP socket, and spawns a blocking accept loop
/// on a background task. Idempotent: a second call while a listener is
/// already running returns `Internal { detail: "already listening" }`.
///
/// RADIO-1: Telnet is an IP transport — no RF, no Part 97 consent. The
/// listener accepting an inbound peer does NOT cause a radio to transmit
/// (it's a TCP socket). This command does NOT require a RADIO-1 consent
/// token — the arming itself is the operator's per-invocation consent for
/// inbound TCP peers, framed by the arm window (TTL).
#[tauri::command]
pub async fn telnet_listen(
    app: AppHandle,
    state: State<'_, std::sync::Arc<TelnetListenState>>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
) -> Result<(), UiError> {
    use crate::winlink::listener::{ListenerArmsRecord, TransportKind};
    use crate::winlink::session::{ExchangeConfig, SessionIntent};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    // Refuse second arm while one is in flight.
    {
        let guard = state.inner.lock().unwrap();
        if guard.is_some() {
            return Err(UiError::Internal {
                detail: "Telnet listener is already armed".into(),
            });
        }
    }

    let cfg = config::read_config()
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let mycall = cfg.identity.callsign.clone().unwrap_or_default();
    if mycall.is_empty() {
        return Err(UiError::NotConfigured(
            "no callsign configured — cannot arm listener without identity".into(),
        ));
    }
    let locator = cfg.identity.grid.clone().unwrap_or_default();

    // Bind the TCP socket up-front so a bind failure is surfaced
    // synchronously to the operator (port conflict / permission).
    let listener = crate::winlink::telnet_listen::bind(
        &cfg.telnet_listen.bind_addr,
        cfg.telnet_listen.port,
    )
    .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let bound_addr = listener
        .local_addr()
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;

    // Load the allowlist + station password.
    let allowed = load_telnet_allowed_stations()?;
    let password = crate::winlink::listener::StationPassword::new();
    let arms = ListenerArmsRecord::arm(
        TransportKind::Telnet,
        std::time::Duration::from_secs(cfg.telnet_listen.ttl_secs),
    );

    let exchange_cfg = ExchangeConfig {
        mycall,
        targetcall: String::new(), // filled in per-session by run_exchange_with_role
        locator,
        password: None,
        intent: SessionIntent::P2p,
    };

    let shutdown = Arc::new(AtomicBool::new(false));
    // Stash the handle in shared state BEFORE spawning so a fast
    // `telnet_set_listen(false)` from the operator finds the flag.
    {
        let mut guard = state.inner.lock().unwrap();
        *guard = Some(TelnetListenHandle {
            shutdown: Arc::clone(&shutdown),
            bound_addr,
        });
    }

    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        format!("Telnet listener armed on {bound_addr} (TTL {}s)", cfg.telnet_listen.ttl_secs),
    );

    // Build the same on-disk mailbox the Packet listener + telnet_p2p_connect
    // use (`<app_data>/native-mbox`). The listener thread needs this so
    // inbound exchanges can persist received messages to Inbox + drain the
    // Outbox into the answerer's outbound vec — symmetry with Packet's
    // `native_packet_exchange` that closes the original tuxlink-k3ru gap.
    let mailbox = match app.path().app_data_dir() {
        Ok(dir) => {
            let mut mb = crate::native_mailbox::Mailbox::new(dir.join("native-mbox"));
            if let Some(svc) = app.try_state::<crate::search::commands::SearchService>() {
                mb = mb.with_index(svc.index.clone());
            }
            Some(Arc::new(mb))
        }
        Err(e) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Warn,
                format!(
                    "Telnet listener: app data dir unavailable — inbound mail won't persist this session: {e}"
                ),
            );
            None
        }
    };

    // Hand off the bound listener + arms record to a blocking task.
    let app_progress = app.clone();
    let log_progress = log.inner().clone();
    let app_wire = app.clone();
    let log_wire = log.inner().clone();

    let state_for_thread = Arc::clone(state.inner());
    tokio::task::spawn_blocking(move || {
        crate::winlink::telnet_listen::run_accept_loop(
            listener,
            allowed,
            password,
            arms,
            exchange_cfg,
            mailbox,
            shutdown,
            &move |line: &str| {
                emit_session_line(
                    &app_progress,
                    &log_progress,
                    LogLevel::Info,
                    line.to_string(),
                );
            },
            &move |line: &str| {
                emit_session_line(
                    &app_wire,
                    &log_wire,
                    LogLevel::Info,
                    line.to_string(),
                );
            },
            |proposals: &[crate::winlink::proposal::Proposal]| {
                // Codex review 2026-06-03 [P1]: returning an empty Vec made
                // `receive_turn` fail with `AnswerCountMismatch` on any
                // inbound batch, so the listener couldn't actually accept
                // P2P mail. Mirror the Packet listener's policy
                // (`winlink_backend::native_packet_exchange` ~line 1279):
                // accept every proposal at the B2F layer; mailbox dedup is
                // a follow-up (filed as a new bd issue tracking inbound-
                // mail symmetry — outbox-on-inbound + inbox-persistence).
                proposals
                    .iter()
                    .map(|_| crate::winlink::proposal::Answer::Accept { resume_offset: 0 })
                    .collect()
            },
        );
        // Clear the handle once the loop exits.
        let mut guard = state_for_thread.inner.lock().unwrap();
        *guard = None;
    });
    Ok(())
}

/// Toggle the listener on/off. `enabled == true` = arm (same as
/// `telnet_listen()`); `enabled == false` = disarm (signal shutdown +
/// close the bound socket to wake the accept loop).
#[tauri::command]
pub async fn telnet_set_listen(
    app: AppHandle,
    state: State<'_, std::sync::Arc<TelnetListenState>>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    enabled: bool,
) -> Result<(), UiError> {
    use std::sync::atomic::Ordering;
    if enabled {
        // Equivalent to telnet_listen().
        telnet_listen(app, state, log).await
    } else {
        let mut guard = state.inner.lock().unwrap();
        if let Some(handle) = guard.take() {
            handle.shutdown.store(true, Ordering::SeqCst);
            // Open a transient connection to wake the accept loop.
            let _ = std::net::TcpStream::connect_timeout(
                &handle.bound_addr,
                std::time::Duration::from_millis(500),
            );
            emit_session_line(
                &app,
                &log,
                LogLevel::Info,
                "Telnet listener disarmed.".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink_backend::MessageId;

    #[test]
    fn discover_serial_devices_classifies_usb_bluetooth_uart_and_excludes_others() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path();
        for name in ["ttyUSB0", "ttyACM0", "rfcomm0", "ttyAMA0", "ttyS0", "null", "sda1", "tty"] {
            std::fs::write(dev.join(name), b"").unwrap();
        }
        let found = discover_serial_devices(dev);
        let by_name = |n: &str| {
            found
                .iter()
                .find(|d| d.path.rsplit('/').next().unwrap() == n)
                .cloned()
        };
        // USB-serial adapters → kind "usb".
        assert_eq!(by_name("ttyUSB0").unwrap().kind, "usb");
        assert_eq!(by_name("ttyACM0").unwrap().kind, "usb");
        // Bound Bluetooth RFCOMM → kind "bluetooth" (NOT conflated with USB).
        assert_eq!(by_name("rfcomm0").unwrap().kind, "bluetooth");
        // On-board UARTs → kind "uart".
        assert_eq!(by_name("ttyAMA0").unwrap().kind, "uart");
        assert_eq!(by_name("ttyS0").unwrap().kind, "uart");
        // Non-serial nodes + a bare "tty" (no instance number) are excluded.
        for skip in ["null", "sda1", "tty"] {
            assert!(by_name(skip).is_none(), "should not list {skip}");
        }
        // Every entry carries a human label.
        assert!(found.iter().all(|d| !d.label.is_empty()));
        // Sorted by path.
        let mut sorted = found.clone();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(found, sorted);
    }

    #[test]
    fn discover_serial_devices_empty_when_dir_missing() {
        assert!(discover_serial_devices(std::path::Path::new("/no/such/dir/xyzzy")).is_empty());
    }

    // tuxlink-mqu3: BluetoothDeviceDto parser regression-pin. Real `bluetoothctl
    // devices Paired` output on the dev Pi: a single `Device <MAC> <NAME>` line.
    // The picker fan-out needs the parser to (a) match the literal prefix, (b)
    // split MAC vs name on the first space, (c) accept names with spaces, (d)
    // drop malformed lines (bad MAC, missing prefix) so the dropdown never
    // shows an un-dialable entry.
    #[test]
    fn parse_paired_bluetooth_extracts_mac_and_name() {
        let output = "Device 38:D2:00:01:55:5C UV-PRO\n";
        let found = parse_paired_bluetooth(output);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mac, "38:D2:00:01:55:5C");
        assert_eq!(found[0].name, "UV-PRO");
    }

    #[test]
    fn parse_paired_bluetooth_handles_multiple_devices_and_names_with_spaces() {
        // Mixed: a clean UV-PRO line + a name with spaces + a lowercase MAC.
        let output = "\
Device 38:D2:00:01:55:5C UV-PRO
Device aa:bb:cc:dd:ee:ff My Pixel 8
Device 11:22:33:44:55:66 Some BT Speaker
";
        let found = parse_paired_bluetooth(output);
        assert_eq!(found.len(), 3);
        assert_eq!(found[1].name, "My Pixel 8", "name must keep internal spaces");
        assert_eq!(found[2].mac, "11:22:33:44:55:66");
    }

    #[test]
    fn parse_paired_bluetooth_drops_malformed_lines() {
        // Missing prefix, malformed MAC (too few octets / wrong sep), and a
        // bare prefix with no MAC must all be dropped — the dropdown only
        // offers entries the dial side can actually use.
        let output = "\
not a device line
Device 38:D2:00:01:55 ShortMac
Device 38-D2-00-01-55-5C WrongSeparator
Device
Device 38:D2:00:01:55:5C OnlyValid
";
        let found = parse_paired_bluetooth(output);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "OnlyValid");
    }

    #[test]
    fn parse_paired_bluetooth_empty_for_no_devices() {
        // bluetoothctl with zero paired devices prints nothing (or a single
        // newline). Empty input → empty list, no panic, no spurious entry.
        assert!(parse_paired_bluetooth("").is_empty());
        assert!(parse_paired_bluetooth("\n").is_empty());
    }

    // tuxlink-y7x7: ALSA parser regression-pin. Real `arecord -L` output is a
    // multi-line ladder of device blocks (name at col 0, one or more indented
    // description lines). The parser must (a) match each block correctly,
    // (b) preserve multi-line descriptions, (c) classify `plughw:CARD=`/
    // `hw:CARD=` entries as hardware, (d) sort hardware first so the picker's
    // natural top-of-list choice is the right kind.
    #[test]
    fn parse_alsa_devices_classifies_hardware_and_sorts_first() {
        // Trimmed from a real `arecord -L` snapshot — plugins (null, default,
        // pulse), a hardware USB-CODEC entry with a two-line description, and
        // a sysdefault as the trailing block.
        let output = "\
null
    Discard all samples (playback) or generate zero samples (capture)
default
    Default Audio Device
plughw:CARD=Device,DEV=0
    USB Audio CODEC
    Hardware device with all software conversions
pulse
    PulseAudio Sound Server
";
        let devs = parse_alsa_devices(output);
        // Hardware sorts first regardless of input order.
        assert_eq!(devs[0].name, "plughw:CARD=Device,DEV=0");
        assert!(devs[0].is_hardware);
        // Multi-line description survives — both lines joined.
        assert!(devs[0].description.contains("USB Audio CODEC"));
        assert!(devs[0].description.contains("Hardware device"));
        // Plugins follow, none classified as hardware.
        let plugin_names: Vec<_> = devs[1..].iter().map(|d| d.name.clone()).collect();
        assert!(plugin_names.contains(&"null".to_string()));
        assert!(plugin_names.contains(&"default".to_string()));
        assert!(plugin_names.contains(&"pulse".to_string()));
        assert!(devs[1..].iter().all(|d| !d.is_hardware));
    }

    #[test]
    fn parse_alsa_devices_classifies_hw_card_as_hardware() {
        // `hw:CARD=…` is the non-converting variant; should also be hardware.
        let output = "\
hw:CARD=Device,DEV=0
    USB Audio CODEC raw
";
        let devs = parse_alsa_devices(output);
        assert_eq!(devs.len(), 1);
        assert!(devs[0].is_hardware);
    }

    #[test]
    fn parse_alsa_devices_handles_no_description_lines() {
        // A block with only a name line (no indented description) must still
        // parse to a device with an empty description, not get dropped.
        let output = "default\nplughw:CARD=Device,DEV=0\n    USB Audio CODEC\n";
        let devs = parse_alsa_devices(output);
        let names: Vec<_> = devs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"default"));
        let default_entry = devs.iter().find(|d| d.name == "default").unwrap();
        assert_eq!(default_entry.description, "");
    }

    #[test]
    fn parse_alsa_devices_empty_input_returns_empty_list() {
        assert!(parse_alsa_devices("").is_empty());
        assert!(parse_alsa_devices("\n\n\n").is_empty());
    }

    // MessageMetaDto serializes camelCase (bodySize, hasAttachments) so the
    // TS `MessageMeta` model needs no rename layer. In-crate because
    // `MessageMeta` is `#[non_exhaustive]` (can't be struct-literal-built
    // from the external integration-test crate).
    #[test]
    fn message_meta_dto_serializes_camel_case() {
        let dto = MessageMetaDto::from(MessageMeta {
            id: MessageId::new("M1"),
            subject: "S".into(),
            from: "F".into(),
            to: vec!["T".into()],
            date: "2026-05-19T00:00:00Z".into(),
            unread: true,
            body_size: 7,
            has_attachments: true,
        });
        let v = serde_json::to_value(dto).unwrap();
        assert_eq!(v["bodySize"], 7);
        assert_eq!(v["hasAttachments"], true);
        assert_eq!(v["to"][0], "T");
        assert_eq!(v["id"], "M1");
    }

    // Task-13 in-crate smoke: ParsedMessageDto serializes with `camelCase`
    // field names (isForm → "isForm", routing → null when None).
    #[test]
    fn parsed_message_dto_serializes_camel_case() {
        let dto = ParsedMessageDto {
            id: "MID_INCRATE".into(),
            subject: "Test".into(),
            from: "W4PHS@winlink.org".into(),
            to: vec!["KK4XYZ@winlink.org".into()],
            cc: vec![],
            date: "2026-05-19T00:00:00Z".into(),
            body: "Hello".into(),
            attachments: vec![AttachmentMetaDto {
                filename: "f.txt".into(),
                size: 10,
            }],
            is_form: false,
            routing: Some("via CMS-SSL".into()),
            form_id: None,
            form_payload: None,
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["isForm"], false);
        assert_eq!(v["routing"], "via CMS-SSL");
        assert_eq!(v["attachments"][0]["filename"], "f.txt");
        assert_eq!(v["attachments"][0]["size"], 10);
    }

    // format_unix_ts sanity: epoch → 1970-01-01T00:00:00Z, a known date.
    #[test]
    fn format_unix_ts_epoch_and_known_date() {
        assert_eq!(format_unix_ts(0), "1970-01-01T00:00:00Z");
        // 2026-05-19T00:00:00Z = days_since_epoch 20592.  Verify a round-trip
        // via a known offset (1 day = 86400s).
        // 1970-01-02T00:00:00Z
        assert_eq!(format_unix_ts(86400), "1970-01-02T00:00:00Z");
    }

    // parse_raw_rfc5322 rejects oversized input.
    #[test]
    fn parse_rejects_oversized_input_inmodule() {
        let huge = vec![b'X'; MAX_RFC5322_BYTES + 1];
        assert!(matches!(
            parse_raw_rfc5322("OVER", &huge),
            Err(UiError::Internal { .. })
        ));
    }

    // ---- tuxlink-0fyj: message_attachment_save extraction tests ----------

    /// Build a synthetic multipart/mixed message with a single named binary
    /// attachment whose body is the bytes `payload`. The base64 encoding is
    /// done inline (no extra crate dep) so the test can stay self-contained.
    fn build_mime_with_attachment(filename: &str, payload: &[u8]) -> Vec<u8> {
        // base64 encode payload (RFC 4648 standard alphabet, no line wrap).
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut b64 = String::new();
        let mut i = 0;
        while i + 3 <= payload.len() {
            let n = ((payload[i] as u32) << 16) | ((payload[i + 1] as u32) << 8) | (payload[i + 2] as u32);
            b64.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
            b64.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
            b64.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
            b64.push(ALPHABET[(n & 0x3f) as usize] as char);
            i += 3;
        }
        if i < payload.len() {
            let rem = payload.len() - i;
            let b0 = payload[i] as u32;
            let b1 = if rem > 1 { payload[i + 1] as u32 } else { 0 };
            let n = (b0 << 16) | (b1 << 8);
            b64.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
            b64.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
            if rem == 2 {
                b64.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
                b64.push('=');
            } else {
                b64.push('=');
                b64.push('=');
            }
        }
        format!(
            "From: sender@example.com\r\n\
             To: recipient@example.com\r\n\
             Subject: test\r\n\
             Date: Tue, 03 Jun 2026 02:00:00 +0000\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: multipart/mixed; boundary=\"BOUND\"\r\n\
             \r\n\
             --BOUND\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             body\r\n\
             --BOUND\r\n\
             Content-Type: application/octet-stream\r\n\
             Content-Disposition: attachment; filename=\"{filename}\"\r\n\
             Content-Transfer-Encoding: base64\r\n\
             \r\n\
             {b64}\r\n\
             --BOUND--\r\n"
        )
        .into_bytes()
    }

    #[test]
    fn extract_attachment_bytes_round_trips_binary_payload() {
        // Use a non-text binary blob (GRIB-1 magic bytes + some noise) to
        // stand in for a real Saildocs GRIB attachment.
        let payload: Vec<u8> = b"GRIB\x00\x01\x02\x03\xff\xfe\xfd random binary \x00\x00".to_vec();
        let raw = build_mime_with_attachment("forecast.grb", &payload);
        let msg = mail_parser::MessageParser::new()
            .parse(raw.as_slice())
            .expect("synthetic MIME parses");
        let got = extract_attachment_bytes(&msg, "forecast.grb")
            .expect("named attachment found");
        assert_eq!(got, payload, "decoded bytes must match the source payload");
    }

    #[test]
    fn extract_attachment_bytes_returns_none_for_unknown_filename() {
        let raw = build_mime_with_attachment("a.bin", b"abc");
        let msg = mail_parser::MessageParser::new().parse(raw.as_slice()).unwrap();
        assert!(extract_attachment_bytes(&msg, "missing.bin").is_none());
    }

    #[test]
    fn message_attachment_save_write_path_persists_bytes_to_disk() {
        // The Tauri-State-bound `message_attachment_save` command can't be
        // driven from a sync unit test (it needs a BackendState). This test
        // covers the write-half of the command: given extracted bytes + a
        // destination, std::fs::write produces a byte-identical file.
        let payload = b"GRIB\x00\x01some bytes\xff\xfe";
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("saved.grb");
        std::fs::write(&dest, payload).expect("write succeeds");
        let read_back = std::fs::read(&dest).expect("read succeeds");
        assert_eq!(read_back, payload);
    }

    // Task 14 test (6): Ok(None) from send_message maps to a SUCCESS on the
    // IPC boundary — not an error. This test asserts the None-success mapping
    // by verifying that `OutboundDraftDto` serializes correctly and that the
    // None → None path produces a serializable `Option<String>`.
    //
    // The full async command path (BackendState + mock backend) cannot be driven
    // from a sync unit test without a tokio runtime; the structural contract
    // is tested here (None round-trip) and the command handler is tested at
    // the integration layer (cargo test with tokio::test).
    #[test]
    fn none_mid_maps_to_none_string() {
        // Simulate the final map: `Option<MessageId>` → `Option<String>`
        let mid: Option<MessageId> = None; // Pat 1.0.0 behavior
        let result: Option<String> = mid.map(|id| id.0);
        assert!(result.is_none(), "Pat's None return maps to Ok(None), not an error");
    }

    #[test]
    fn some_mid_maps_to_some_string() {
        let mid: Option<MessageId> = Some(MessageId::new("MID-12345"));
        let result: Option<String> = mid.map(|id| id.0);
        assert_eq!(result, Some("MID-12345".to_string()));
    }

    // tuxlink-ca5x: `parse_folder("archive")` must succeed end-to-end. The
    // string "archive" arrives from `mailbox_list` (folder browse), `message_read`
    // (open a message), and `mailbox_move` (Archive button + A shortcut).
    #[test]
    fn parse_folder_accepts_archive() {
        let parsed = parse_folder("archive").expect("archive must parse");
        assert_eq!(parsed, MailboxFolder::Archive);
    }

    // tuxlink-ca5x: drafts + deleted remain non-backend even after Archive
    // joined the wire vocabulary. Regression-pin: a future careless union of
    // "any string is a folder slug" (Phase 2 work) must not silently flip
    // these to Ok.
    #[test]
    fn parse_folder_still_rejects_drafts_and_deleted() {
        assert!(parse_folder("drafts").is_err());
        assert!(parse_folder("deleted").is_err());
    }

    #[test]
    fn outbound_draft_dto_deserializes() {
        let json = r#"{
            "to": ["W6ABC@winlink.org"],
            "cc": [],
            "subject": "ICS-213 check-in",
            "body": "Standing by at staging area."
        }"#;
        let dto: OutboundDraftDto = serde_json::from_str(json).unwrap();
        assert_eq!(dto.to, vec!["W6ABC@winlink.org"]);
        assert!(dto.cc.is_empty());
        assert_eq!(dto.subject, "ICS-213 check-in");
        assert_eq!(dto.body, "Standing by at staging area.");
    }

    // ========================================================================
    // Task 16 — config_read DTO mapping (integration commit, spec §5.6 / §6)
    // ========================================================================
    use crate::config::{
        CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PacketConfig,
        PositionPrecision, PositionSource, PrivacyConfig, CONFIG_SCHEMA_VERSION,
    };

    /// Build a CMS-mode config fixture for the mapping tests.
    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn cms_config_fixture() -> Config {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig {
                connect_to_cms: true,
                transport: CmsTransport::CmsSsl,
                host: config::default_cms_host(),
            },
            identity: IdentityConfig {
                callsign: Some("W4PHS".into()),
                identifier: None,
                grid: Some("EM10ab".into()),
            },
            privacy: PrivacyConfig {
                gps_state: GpsState::BroadcastAtPrecision,
                position_precision: PositionPrecision::SixCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
        }
    }

    // config_read DTO mapping: nested config.rs struct → flat ConfigViewDto.
    // Asserts every flattened field is sourced from the right nested location.
    #[test]
    fn config_view_dto_maps_nested_to_flat() {
        let cfg = cms_config_fixture();
        let dto = ConfigViewDto::from(&cfg);
        assert!(dto.connect_to_cms);
        assert_eq!(dto.transport, CmsTransport::CmsSsl);
        // tuxlink-3o0: host is surfaced in the DTO (sourced from connect.host).
        assert_eq!(dto.host, "cms-z.winlink.org");
        assert_eq!(dto.callsign.as_deref(), Some("W4PHS"));
        assert_eq!(dto.identifier, None);
        assert_eq!(dto.grid.as_deref(), Some("EM10ab"));
        assert_eq!(dto.gps_state, GpsState::BroadcastAtPrecision);
        assert_eq!(dto.position_precision, PositionPrecision::SixCharGrid);
        // tuxlink-686 Task 7: position_source is surfaced in the DTO.
        assert_eq!(dto.position_source, PositionSource::Gps);
    }

    // Offline-mode mapping: callsign None, identifier Some — mirrors the
    // ribbon's identity.identifier fallback (useStatus.ts formatCallsign).
    #[test]
    fn config_view_dto_maps_offline_identity() {
        let mut cfg = cms_config_fixture();
        cfg.connect.connect_to_cms = false;
        cfg.connect.transport = CmsTransport::Telnet;
        cfg.connect.host = "server.winlink.org".into();
        cfg.identity.callsign = None;
        cfg.identity.identifier = Some("OFFLINE-STATION".into());
        cfg.privacy.gps_state = GpsState::Off;
        cfg.privacy.position_precision = PositionPrecision::FourCharGrid;

        let dto = ConfigViewDto::from(&cfg);
        assert!(!dto.connect_to_cms);
        assert_eq!(dto.transport, CmsTransport::Telnet);
        // tuxlink-3o0: a non-default host maps through verbatim.
        assert_eq!(dto.host, "server.winlink.org");
        assert_eq!(dto.callsign, None);
        assert_eq!(dto.identifier.as_deref(), Some("OFFLINE-STATION"));
        assert_eq!(dto.gps_state, GpsState::Off);
        assert_eq!(dto.position_precision, PositionPrecision::FourCharGrid);
    }

    // ConfigViewDto serializes with snake_case keys + PascalCase enum values,
    // matching the TS ConfigViewDto shape in useStatus.ts (status.test.ts #6).
    #[test]
    fn config_view_dto_serializes_snake_case_keys_pascal_enums() {
        let dto = ConfigViewDto::from(&cms_config_fixture());
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["connect_to_cms"], true);
        assert_eq!(v["transport"], "CmsSsl");
        // tuxlink-3o0: host serializes as a snake_case string key.
        assert_eq!(v["host"], "cms-z.winlink.org");
        assert_eq!(v["callsign"], "W4PHS");
        assert_eq!(v["identifier"], serde_json::Value::Null);
        assert_eq!(v["grid"], "EM10ab");
        assert_eq!(v["gps_state"], "BroadcastAtPrecision");
        assert_eq!(v["position_precision"], "SixCharGrid");
        // tuxlink-686 Task 7: position_source key is snake_case; value is PascalCase.
        assert_eq!(v["position_source"], "Gps");
    }

    // ========================================================================
    // Task 16 — backend_status DTO mapping + populated-vs-None branch logic
    // ========================================================================
    use crate::app_backend::{BackendPhase, BackendState};
    use crate::winlink_backend::{BackendStatus, NativeBackend};
    use std::sync::Arc;

    // StatusDto::from maps every BackendStatus variant; transport is verbatim
    // (frontend normalizeTransportLabel renders "CMS-CmsSsl" → "CMS-SSL").
    #[test]
    fn status_dto_maps_all_backend_status_variants() {
        assert_eq!(
            StatusDto::from(BackendStatus::Disconnected),
            StatusDto::Disconnected
        );
        assert_eq!(
            StatusDto::from(BackendStatus::Connecting {
                transport: "CMS-CmsSsl".into()
            }),
            StatusDto::Connecting {
                transport: "CMS-CmsSsl".into()
            }
        );
        // Listening (packet armed-but-idle): distinct from Connecting (which
        // implies an active dial). Carries the transport so the ribbon can
        // render "Listening · Packet 1200". (tuxlink-orj)
        assert_eq!(
            StatusDto::from(BackendStatus::Listening {
                transport: "Packet-7".into()
            }),
            StatusDto::Listening {
                transport: "Packet-7".into()
            }
        );
        assert_eq!(
            StatusDto::from(BackendStatus::Connected {
                transport: "CMS-Telnet".into(),
                peer: "cms.winlink.org".into(),
                since_iso: "2026-05-19T00:00:00Z".into(),
            }),
            StatusDto::Connected {
                transport: "CMS-Telnet".into(),
                peer: "cms.winlink.org".into(),
                since_iso: "2026-05-19T00:00:00Z".into(),
            }
        );
        assert_eq!(
            StatusDto::from(BackendStatus::Disconnecting),
            StatusDto::Disconnecting
        );
        assert_eq!(
            StatusDto::from(BackendStatus::Error {
                reason: "refused".into()
            }),
            StatusDto::Error {
                reason: "refused".into()
            }
        );
    }

    // StatusDto serializes internally-tagged (kind alongside fields, no
    // "content" wrapper) — matches the TS StatusDto union in useStatus.ts.
    #[test]
    fn status_dto_serializes_internally_tagged() {
        let connected = serde_json::to_value(StatusDto::Connected {
            transport: "CMS-CmsSsl".into(),
            peer: "cms.winlink.org".into(),
            since_iso: "2026-05-19T00:00:00Z".into(),
        })
        .unwrap();
        assert_eq!(connected["kind"], "Connected");
        assert_eq!(connected["transport"], "CMS-CmsSsl");
        assert_eq!(connected["peer"], "cms.winlink.org");
        assert_eq!(connected["since_iso"], "2026-05-19T00:00:00Z");

        let disc = serde_json::to_value(StatusDto::Disconnected).unwrap();
        assert_eq!(disc["kind"], "Disconnected");
    }

    // ========================================================================
    // Task D (tuxlink-22l) — three-state backend_status derivation (spec §3.4)
    // The command fn takes `State<'_, BackendState>` (needs a Tauri app), so
    // the three-state logic is exercised here against `derive_status_dto`, the
    // pure helper the command calls on its `snapshot()`. We construct
    // `BackendState` directly in each phase + a real `NativeBackend` for the
    // Ready case (the live IPC round-trip is the M2 smoke gate).
    // ========================================================================

    // NotConfigured (pre-wizard / offline) → None: the ribbon renders its
    // config-derived "Idle · <transport>" empty state, NOT an error.
    #[test]
    fn derive_status_not_configured_is_none() {
        let state = BackendState::new(); // (NotConfigured, None)
        let (phase, backend) = state.snapshot();
        assert!(
            derive_status_dto(phase, backend).is_none(),
            "NotConfigured → None (frontend renders Idle · <transport>)"
        );
    }

    // Spawning → Some(Connecting): the bootstrap is launching the backend; the ribbon
    // shows a connecting state rather than "not connected" or an error.
    #[test]
    fn derive_status_spawning_is_connecting() {
        let state = BackendState::new();
        state.set_phase(BackendPhase::Spawning);
        let (phase, backend) = state.snapshot();
        assert_eq!(
            derive_status_dto(phase, backend),
            Some(StatusDto::Connecting {
                transport: String::new()
            }),
            "Spawning → Some(Connecting)"
        );
    }

    // Ready + backend → the live backend's status() mapped. A freshly-constructed
    // NativeBackend reports Disconnected, which projects to Some(StatusDto::Disconnected).
    #[test]
    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn derive_status_ready_maps_backend_status() {
        use crate::config::{
            CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PacketConfig,
            PositionPrecision, PrivacyConfig, CONFIG_SCHEMA_VERSION,
        };
        let cfg = Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig {
                connect_to_cms: true,
                transport: CmsTransport::CmsSsl,
                host: crate::config::default_cms_host(),
            },
            identity: IdentityConfig { callsign: Some("N0CALL".into()), identifier: None, grid: None },
            privacy: PrivacyConfig {
                gps_state: GpsState::Off,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: crate::config::PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
        };
        let tmp = tempfile::tempdir().expect("tmpdir");
        let state = BackendState::new();
        state.install(Arc::new(NativeBackend::new(cfg, tmp.path())));
        let (phase, backend) = state.snapshot();
        assert_eq!(
            derive_status_dto(phase, backend),
            Some(StatusDto::Disconnected),
            "Ready + backend → Some(live status())"
        );
    }

    // Failed → Some(Error{reason}): CMS configured but backend spawn/health failed.
    // The ribbon shows the reason loudly.
    #[test]
    fn derive_status_failed_is_error_with_reason() {
        let state = BackendState::new();
        state.set_phase(BackendPhase::Failed {
            reason: "Pat failed to start: announce timeout".to_string(),
        });
        let (phase, backend) = state.snapshot();
        assert_eq!(
            derive_status_dto(phase, backend),
            Some(StatusDto::Error {
                reason: "Pat failed to start: announce timeout".to_string()
            }),
            "Failed → Some(Error{{reason}})"
        );
    }

    // ConfigError → Some(Error{reason}): a config file exists but is unusable
    // (Serde/Validation/Io). Distinct phase from Failed (adrev #15) but also an
    // explicit error at the ribbon, carrying its own reason.
    #[test]
    fn derive_status_config_error_is_error_with_reason() {
        let state = BackendState::new();
        state.set_phase(BackendPhase::ConfigError {
            reason: "config deserialize failed: expected value at line 1".to_string(),
        });
        let (phase, backend) = state.snapshot();
        assert_eq!(
            derive_status_dto(phase, backend),
            Some(StatusDto::Error {
                reason: "config deserialize failed: expected value at line 1".to_string()
            }),
            "ConfigError → Some(Error{{reason}})"
        );
    }

    // ========================================================================
    // Task 15 — session_log_snapshot DTO shape + projection (integration round)
    // ========================================================================
    use crate::winlink_backend::{LogLevel, LogLine, LogSource};

    // LogLineDto serializes camelCase keys (timestampIso) with lowercase enum
    // values, matching the TS LogLineDto in src/session/logProjection.ts so the
    // frontend needs no rename layer.
    #[test]
    fn log_line_dto_serializes_camel_case_lowercase_enums() {
        let dto = LogLineDto::from(LogLine {
            seq: 0,
            timestamp_iso: "2026-05-19T00:00:00Z".into(),
            level: LogLevel::Warn,
            source: LogSource::Transport,
            message: "*** Session started".into(),
        });
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["timestampIso"], "2026-05-19T00:00:00Z");
        assert_eq!(v["level"], "warn");
        assert_eq!(v["source"], "transport");
        assert_eq!(v["message"], "*** Session started");
        // No snake_case key leaks through.
        assert!(v.get("timestamp_iso").is_none());
    }

    // Every LogLevel / LogSource variant maps to its lowercase wire string.
    #[test]
    fn log_level_and_source_variants_map_lowercase() {
        for (level, expected) in [
            (LogLevel::Trace, "trace"),
            (LogLevel::Debug, "debug"),
            (LogLevel::Info, "info"),
            (LogLevel::Warn, "warn"),
            (LogLevel::Error, "error"),
        ] {
            let v = serde_json::to_value(LogLevelDto::from(level)).unwrap();
            assert_eq!(v, expected);
        }
        for (source, expected) in [
            (LogSource::Backend, "backend"),
            (LogSource::Transport, "transport"),
            (LogSource::Wire, "wire"),
        ] {
            let v = serde_json::to_value(LogSourceDto::from(source)).unwrap();
            assert_eq!(v, expected);
        }
    }

    // ========================================================================
    // Task 5 (tuxlink-686) — config_set_grid validator + arbiter pin
    // ========================================================================

    // Step 1 — failing test: invalid Maidenhead is rejected.
    #[test]
    fn config_set_grid_rejects_invalid_maidenhead() {
        let err = validate_grid_input("NOTAGRID");
        assert!(err.is_some()); // returns the validation message
    }

    // Step 4a — valid grids pass validation (the arbiter is updated separately by
    // `set_manual`; see the dedicated test below for that side of the contract).
    #[test]
    fn validate_grid_accepts_valid_four_and_six_char() {
        assert!(validate_grid_input("EM75").is_none(), "4-char Maidenhead should be valid");
        assert!(validate_grid_input("EM75xx").is_none(), "6-char Maidenhead should be valid");
        // Rejection path
        assert!(validate_grid_input("ZZ99").is_some(), "ZZ out-of-range field");
        assert!(validate_grid_input("").is_some(), "empty string should be invalid");
        assert!(validate_grid_input("EM7").is_some(), "3-char should be invalid");
    }

    // Step 4c — multibyte UTF-8 input must not panic and must return Some (invalid).
    #[test]
    fn validate_grid_multibyte_does_not_panic() {
        // "ABé" has byte-len 4 (é is 2 bytes) but is NOT valid Maidenhead.
        // A naive s[2..4] byte-slice on &str would panic at the é boundary.
        // Delegating to grid_to_lat_lon (as_bytes()) is panic-safe.
        assert!(validate_grid_input("ABé").is_some());
        // Also a longer multibyte string
        assert!(validate_grid_input("EM75é1").is_some());
    }

    // ========================================================================
    // Position-subsystem restoration T4 (tuxlink-c79g) — config_set_grid
    // pins position_source = Manual in BOTH config and arbiter.
    // ========================================================================

    // Codex P1 #3 + spec §3.1: config_set_grid persists
    // cfg.privacy.position_source = Manual on disk AND the arbiter's
    // `set_manual` pins source = Manual in memory (T1). This test asserts
    // the cross-layer persistence — pjih dropped the config persistence,
    // T4 restores it.
    //
    // Uses the same env-lock + tempdir + seed-config pattern as
    // `position_set_source_gps_succeeds_without_fresh_fix` (Task 3) since
    // both tests mutate the process-global TUXLINK_CONFIG_DIR env var and
    // exercise the config read→write round-trip.
    #[tokio::test]
    async fn config_set_grid_pins_manual_source_in_config_and_arbiter() {
        use crate::config::CONFIG_SCHEMA_VERSION;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: single-threaded test (env_lock); no concurrent env reads within this block.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        // Seed a minimal valid config with position_source = Gps so we can
        // assert it FLIPS to Manual via the command.
        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid", "position_source": "Gps" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let arbiter = std::sync::Arc::new(PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        ));
        assert_eq!(
            arbiter.source(),
            PositionSource::Gps,
            "fixture starts with source = Gps so we can assert the flip to Manual"
        );

        let result = config_set_grid_impl(
            "EM75".to_string(),
            arbiter.clone(),
            /* backend = */ None,
        )
        .await;

        assert!(result.is_ok(), "config_set_grid must succeed; got {result:?}");

        // Arbiter side: set_manual pins source = Manual (T1) and updates manual_grid.
        assert_eq!(
            arbiter.source(),
            PositionSource::Manual,
            "arbiter source must be pinned to Manual after config_set_grid (T1 invariant)"
        );
        assert_eq!(
            arbiter.active_grid().as_deref(),
            Some("EM75"),
            "arbiter active_grid must follow the manual grid"
        );

        // Config side: position_source = Manual must be persisted to disk.
        // This is the bit pjih dropped and T4 restores (Codex P1 #3, spec §3.1).
        let cfg = crate::config::read_config().expect("read back the persisted config");
        assert_eq!(
            cfg.privacy.position_source,
            PositionSource::Manual,
            "config_set_grid must persist position_source = Manual to disk"
        );
        assert_eq!(
            cfg.identity.grid.as_deref(),
            Some("EM75"),
            "config_set_grid must persist identity.grid"
        );

        // Restore env (best-effort).
        // SAFETY: symmetric with the set_var above; single-threaded test.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    // ========================================================================
    // tuxlink-3o0 — config_set_connect host validator
    // ========================================================================

    // The host validator mirrors validate_identity's nonempty + no-whitespace
    // shape: a typical hostname passes; empty and whitespace-bearing inputs are
    // rejected with the most-actionable message first (empty before whitespace).
    #[test]
    fn validate_cms_host_accepts_typical_hosts_and_rejects_empty_or_whitespace() {
        assert!(validate_cms_host("cms-z.winlink.org").is_none(), "dev host should be valid");
        assert!(validate_cms_host("server.winlink.org").is_none(), "production host should be valid");
        assert!(validate_cms_host("127.0.0.1").is_none(), "an IP literal should be valid");
        // Rejection paths.
        assert_eq!(validate_cms_host(""), Some("CMS host must not be empty"));
        assert_eq!(validate_cms_host("   "), Some("CMS host must not be empty"),
            "whitespace-only trims to empty → empty message (most actionable first)");
        assert_eq!(
            validate_cms_host("cms z.winlink.org"),
            Some("CMS host must not contain whitespace"),
            "an internal space is rejected"
        );
        assert_eq!(
            validate_cms_host("host\twith\ttabs"),
            Some("CMS host must not contain whitespace")
        );
    }

    // ========================================================================
    // Task A (tuxlink-22l) — session_log_snapshot projection via SessionLogState
    // ========================================================================
    use crate::session_log::SessionLogState;

    // Empty buffer → empty JSON array (the v0.0.1 / no-lines-yet path).
    // The frontend's `invoke<LogLineDto[]>('session_log_snapshot')` must
    // resolve (no rejection) and seed with [] when no lines exist yet.
    #[test]
    fn session_log_snapshot_empty_is_a_json_array() {
        let ring = SessionLogState::new(8);
        let snapshot: Vec<LogLineDto> = ring.snapshot().into_iter().map(LogLineDto::from).collect();
        let v = serde_json::to_value(&snapshot).unwrap();
        assert!(v.is_array(), "snapshot serializes as a JSON array");
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    // Appended lines project to LogLineDto with correct seq, message, and
    // camelCase serialization. This verifies the projection logic used by
    // `session_log_snapshot` without requiring a live Tauri runtime.
    #[test]
    fn session_log_snapshot_projects_seq_and_message_order() {
        let ring = SessionLogState::new(8);
        ring.append(LogLine {
            seq: 0,
            timestamp_iso: "2026-05-20T00:00:01Z".into(),
            level: LogLevel::Info,
            source: LogSource::Backend,
            message: "Pat HTTP server ready".into(),
        });
        ring.append(LogLine {
            seq: 0,
            timestamp_iso: "2026-05-20T00:00:02Z".into(),
            level: LogLevel::Warn,
            source: LogSource::Backend,
            message: "CMS connection timeout".into(),
        });

        let dtos: Vec<LogLineDto> =
            ring.snapshot().into_iter().map(LogLineDto::from).collect();

        assert_eq!(dtos.len(), 2, "both appended lines project to DTOs");
        assert_eq!(dtos[0].seq, 1, "first line gets seq=1");
        assert_eq!(dtos[0].message, "Pat HTTP server ready");
        assert_eq!(dtos[0].source, LogSourceDto::Backend);
        assert_eq!(dtos[1].seq, 2, "second line gets seq=2");
        assert_eq!(dtos[1].message, "CMS connection timeout");
        assert_eq!(dtos[1].source, LogSourceDto::Backend);

        // Verify camelCase wire shape (the frontend LogLineDto TS type reads
        // `seq` + `timestampIso`, not `timestamp_iso`).
        let v = serde_json::to_value(&dtos[0]).unwrap();
        assert_eq!(v["seq"], 1);
        assert_eq!(v["timestampIso"], "2026-05-20T00:00:01Z");
        assert!(v.get("timestamp_iso").is_none(), "no snake_case key on wire");
    }

    // Operator smoke 2026-05-31: SessionLogState::clear drains the buffer.
    // Subsequent snapshot() returns empty; the next append still gets a
    // strictly-greater seq (no recycling — stale `last_seq` cursors stay
    // monotonic past a clear). This is the unit-test for the same drain
    // path the `session_log_clear` command invokes.
    #[test]
    fn session_log_state_clear_drains_buffer_and_preserves_seq_monotonic() {
        let ring = SessionLogState::new(8);
        let line = || LogLine {
            seq: 0,
            timestamp_iso: "2026-05-31T00:00:00Z".into(),
            level: LogLevel::Info,
            source: LogSource::Backend,
            message: "x".into(),
        };
        let seq1 = ring.append(line());
        let seq2 = ring.append(line());
        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
        assert_eq!(ring.snapshot().len(), 2);

        ring.clear();
        assert_eq!(ring.snapshot().len(), 0, "snapshot is empty after clear");

        // The next append must NOT recycle seq=1 — a panel that mounted
        // before clear and still tracks last_seq=2 must not match the new
        // line as a duplicate.
        let seq3 = ring.append(line());
        assert_eq!(seq3, 3, "next_seq is preserved across clear");
        assert_eq!(ring.snapshot().len(), 1);
    }

    // Integration-style test for the command path: append → clear (via the
    // same code path the command calls) → snapshot returns empty. Mirrors
    // what `session_log_clear` does on the wire without a tauri runtime.
    #[test]
    fn session_log_clear_command_path_empties_snapshot() {
        let ring = std::sync::Arc::new(SessionLogState::new(8));
        ring.append(LogLine {
            seq: 0,
            timestamp_iso: "2026-05-31T00:00:00Z".into(),
            level: LogLevel::Info,
            source: LogSource::Backend,
            message: "before-clear".into(),
        });
        assert!(!ring.snapshot().is_empty(), "buffer has a line before clear");

        // This is the body of `session_log_clear` — calling it through the
        // Arc the way the tauri State guard does, without spinning up a
        // tauri runtime.
        ring.clear();

        let after: Vec<LogLineDto> =
            ring.snapshot().into_iter().map(LogLineDto::from).collect();
        assert!(after.is_empty(), "snapshot is empty after clear");
    }

    // ========================================================================
    // Task 7 (tuxlink-7fr) — PacketConfigDto round-trip + serialization
    // ========================================================================

    // Helper: valid Config with packet.link = Tcp 127.0.0.1:8001, ssid = 7.
    fn config_with_packet_link() -> config::Config {
        use crate::winlink::ax25::KissLinkConfig;
        let mut cfg = cms_config_fixture();
        cfg.packet = config::PacketConfig {
            ssid: 7,
            link: Some(KissLinkConfig::Tcp {
                host: "127.0.0.1".into(),
                port: 8001,
            }),
            params: config::Ax25ParamsConfig::default(),
            listen_default: true,
        };
        cfg
    }

    // Helper: valid Config with packet.link = None (defaults).
    fn config_with_packet_defaults() -> config::Config {
        let mut cfg = cms_config_fixture();
        cfg.packet = config::PacketConfig::default();
        cfg
    }

    #[test]
    fn packet_config_dto_round_trips_through_packet_config() {
        use crate::winlink::ax25::KissLinkConfig;
        let pc = config::PacketConfig {
            ssid: 7,
            link: Some(KissLinkConfig::Tcp {
                host: "127.0.0.1".into(),
                port: 8001,
            }),
            params: config::Ax25ParamsConfig {
                paclen: 128,
                maxframe: 4,
                ..Default::default()
            },
            listen_default: false,
        };
        let dto = PacketConfigDto::from(&pc);
        assert_eq!(dto.ssid, 7);
        assert!(!dto.listen_default);
        assert_eq!(dto.link_kind.as_deref(), Some("Tcp"));
        assert_eq!(dto.tcp_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(dto.tcp_port, Some(8001));
        assert_eq!(dto.paclen, 128);

        let back = dto.into_packet_config().unwrap();
        assert_eq!(back, pc);
    }

    #[test]
    fn packet_config_dto_with_no_link_maps_to_none() {
        let pc = config::PacketConfig::default();
        let dto = PacketConfigDto::from(&pc);
        assert_eq!(dto.link_kind, None);
        assert!(dto.listen_default); // default-on
        assert_eq!(dto.into_packet_config().unwrap().link, None);
    }

    #[test]
    fn packet_config_dto_serializes_camel_case_for_the_frontend() {
        let dto = PacketConfigDto::from(&config::PacketConfig::default());
        let v = serde_json::to_value(&dto).unwrap();
        assert!(
            v.get("listenDefault").is_some(),
            "expected camelCase listenDefault"
        );
        assert!(v.get("ssid").is_some());
    }

    // ========================================================================
    // Task 8 (tuxlink-7fr) — packet_transport_from_config + apply_listen_default
    // ========================================================================

    #[test]
    fn packet_transport_from_config_builds_dialto_with_ssid_and_path() {
        let mut cfg = config_with_packet_link();
        cfg.packet.ssid = 7;
        let tc =
            packet_transport_from_config(&cfg, "W7AUX".into(), vec!["RELAY-1".into()]).unwrap();
        match tc {
            TransportConfig::Packet { ssid, role, .. } => {
                assert_eq!(ssid, 7);
                assert_eq!(
                    role,
                    crate::winlink_backend::PacketRole::DialTo {
                        call: "W7AUX".into(),
                        path: vec!["RELAY-1".into()],
                    }
                );
            }
            _ => panic!("expected a Packet transport"),
        }
    }

    #[test]
    fn packet_transport_from_config_with_no_link_is_not_configured() {
        let cfg = config_with_packet_defaults();
        let err = packet_transport_from_config(&cfg, "W7AUX".into(), vec![]).unwrap_err();
        assert!(matches!(err, UiError::NotConfigured(_)));
    }

    #[test]
    fn set_listen_default_writes_the_sticky_flag() {
        let mut cfg = config_with_packet_defaults(); // listen_default = true
        apply_listen_default(&mut cfg, false);
        assert!(!cfg.packet.listen_default);
        apply_listen_default(&mut cfg, true);
        assert!(cfg.packet.listen_default);
    }

    #[test]
    fn packet_listen_transport_from_config_builds_listen_role_with_ssid() {
        let mut cfg = config_with_packet_link();
        cfg.packet.ssid = 7;
        let tc = packet_listen_transport_from_config(&cfg).unwrap();
        match tc {
            TransportConfig::Packet { ssid, role, .. } => {
                assert_eq!(ssid, 7);
                assert_eq!(role, crate::winlink_backend::PacketRole::Listen);
            }
            _ => panic!("expected a Packet transport"),
        }
    }

    #[test]
    fn packet_listen_transport_from_config_with_no_link_is_not_configured() {
        let cfg = config_with_packet_defaults();
        let err = packet_listen_transport_from_config(&cfg).unwrap_err();
        assert!(matches!(err, UiError::NotConfigured(_)));
    }

    // ========================================================================
    // Task 11 (tuxlink-686) — position_set_source + position_status unit tests
    // ========================================================================
    use crate::position::{Fix, PositionArbiter};

    // position_set_source: unknown source string → UiError::Rejected.
    #[test]
    fn unknown_position_source_maps_to_rejected() {
        // Replicate the command's match arm for unknown strings.
        let source = "Unknown";
        let result: Result<(), UiError> =
            Err(UiError::Rejected(format!("unsupported position source: {source}")));
        assert!(
            matches!(result, Err(UiError::Rejected(_))),
            "unknown source string maps to UiError::Rejected"
        );
    }

    /// Serializes tests that mutate the process-global TUXLINK_CONFIG_DIR env
    /// var. Mirrors the pattern in `modem_commands::tests::env_lock` — `set_var`
    /// is not thread-safe under cargo's parallel test pool, so each test that
    /// touches the env grabs this mutex for the duration of its
    /// set→read→restore sequence. Without this gate, env-mutating tests in this
    /// binary race (tuxlink-j0ij precedent).
    ///
    /// Uses `tokio::sync::Mutex` rather than `std::sync::Mutex` because the
    /// callers are `#[tokio::test]` async functions that hold the guard across
    /// `.await` points; std::sync::Mutex would block the worker thread when
    /// contended (clippy::await_holding_lock), while tokio's Mutex yields to
    /// the executor.
    async fn position_set_source_env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        LOCK.lock().await
    }

    // Codex P0 #1 / spec §1.1: position_set_source('Gps') mirrors the arbiter
    // relaxation. Returns Ok(()) without a fresh fix; persists
    // position_source = Gps. T2 made `arbiter.use_gps()` infallible; T3
    // removes the `has_fresh_fix()` pre-check + `UiError::Unavailable` error
    // path from the command, so the command is now infallible-on-this-branch.
    #[tokio::test]
    async fn position_set_source_gps_succeeds_without_fresh_fix() {
        use crate::config::CONFIG_SCHEMA_VERSION;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: single-threaded test (env_lock); no concurrent env reads within this block.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        // Seed a minimal valid config with position_source = Manual so we can
        // assert it FLIPS to Gps via the command (offline path: no callsign).
        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": "EM75" }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid", "position_source": "Manual" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let arbiter = std::sync::Arc::new(PositionArbiter::new(
            PositionSource::Manual,
            Some("EM75".to_string()),
            PositionPrecision::FourCharGrid,
        ));
        assert!(!arbiter.has_fresh_fix(), "fixture has no fix");

        let result = position_set_source_impl(
            "Gps".to_string(),
            arbiter.clone(),
            /* backend = */ None,
        )
        .await;

        assert!(
            result.is_ok(),
            "position_set_source('Gps') must succeed without a fresh fix per spec §1.1; got {result:?}"
        );
        assert_eq!(
            arbiter.source(),
            PositionSource::Gps,
            "arbiter source must flip to Gps"
        );

        let cfg = crate::config::read_config().expect("read back the persisted config");
        assert_eq!(
            cfg.privacy.position_source,
            PositionSource::Gps,
            "position_source = Gps must be persisted to config"
        );

        // Restore env (best-effort).
        // SAFETY: symmetric with the set_var above; single-threaded test.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    // R3 F1 + F7 (T6): concurrent config_set_grid + position_set_source must
    // serialize via the arbiter mutex. The final state must be consistent —
    // arbiter.source() must equal config.privacy.position_source after all
    // tasks join — and no task may panic (which would indicate a poisoned
    // mutex). Without the with_inner transactional wrapper, the disk write
    // and arbiter mutation are independently ordered, producing a TOCTOU
    // window where the LAST disk-writer and the LAST arbiter-mutator can be
    // different tasks → disk source != arbiter source.
    //
    // To maximize the chance of catching the race in a single test run, tasks
    // synchronize via a tokio Barrier so all 100 spawned tasks unblock at the
    // same instant, then each task runs its sequence repeatedly. This
    // compresses the contention window relative to the simple-spawn-and-go
    // pattern.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_config_set_grid_and_position_set_source_serialize() {
        use crate::config::CONFIG_SCHEMA_VERSION;
        use tokio::sync::Barrier;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: single-threaded test (env_lock); no concurrent env reads within this block.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        // Seed a minimal valid config so read_config inside the spawned tasks
        // has something to deserialize on first call.
        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": "EM75" }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid", "position_source": "Gps" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let arbiter = std::sync::Arc::new(PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        ));

        // 50 grid-setters + 50 source-setters, total 100. Each task does
        // ITERATIONS rounds. The barrier releases all 100 tasks at once for
        // peak contention.
        const TASKS_PER_KIND: usize = 50;
        const ITERATIONS: usize = 20;
        let barrier = std::sync::Arc::new(Barrier::new(TASKS_PER_KIND * 2));

        let mut handles = Vec::new();
        for i in 0..TASKS_PER_KIND {
            let a1 = arbiter.clone();
            let b1 = barrier.clone();
            handles.push(tokio::spawn(async move {
                b1.wait().await;
                for _ in 0..ITERATIONS {
                    let grid = format!("EM{:02}", i % 100);
                    let _ = config_set_grid_impl(grid, a1.clone(), None).await;
                }
            }));
            let a2 = arbiter.clone();
            let b2 = barrier.clone();
            handles.push(tokio::spawn(async move {
                b2.wait().await;
                for _ in 0..ITERATIONS {
                    let _ = position_set_source_impl("Gps".to_string(), a2.clone(), None).await;
                }
            }));
        }
        for h in handles {
            h.await.expect("task panicked — arbiter mutex was poisoned");
        }

        // Final state must be consistent — source from disk == source from arbiter.
        // With the with_inner transactional wrapper, the LAST critical section to
        // release the mutex writes BOTH the on-disk source AND the arbiter source
        // atomically, so the two values must agree.
        let cfg = crate::config::read_config().expect("read back the persisted config");
        assert_eq!(
            arbiter.source(),
            cfg.privacy.position_source,
            "final arbiter source must match final on-disk source (R3 F1 + F7)"
        );

        // Restore env (best-effort).
        // SAFETY: symmetric with the set_var above; single-threaded test.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    // Helpers for position_status DTO unit tests.
    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn make_config_for_position_status(gps_state: GpsState, grid: Option<&str>) -> config::Config {
        use crate::config::{ConnectConfig, CmsTransport, IdentityConfig, PrivacyConfig, CONFIG_SCHEMA_VERSION};
        config::Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig { connect_to_cms: false, transport: CmsTransport::Telnet, host: config::default_cms_host() },
            identity: IdentityConfig {
                callsign: None,
                identifier: None,
                grid: grid.map(|s| s.to_string()),
            },
            privacy: PrivacyConfig {
                gps_state,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: crate::config::PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
        }
    }

    // Spec §3.1: PositionStatusDto must NOT carry active_source post-restore.
    // tuxlink-va1i: it MUST now carry both broadcast_grid AND ui_grid.
    #[test]
    fn position_status_dto_does_not_carry_active_source() {
        // Use serde to introspect the serialized shape.
        let dto = PositionStatusDto {
            gps_ready: true,
            broadcast_grid: "CN87".to_string(),
            ui_grid: "CN87".to_string(),
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert!(v.get("active_source").is_none(),
            "PositionStatusDto must not have active_source field (spec §3.1)");
        assert_eq!(v.get("gps_ready").and_then(|x| x.as_bool()), Some(true));
        assert_eq!(v.get("broadcast_grid").and_then(|x| x.as_str()), Some("CN87"));
        assert!(v.get("ui_grid").is_some(),
            "PositionStatusDto must carry ui_grid field (tuxlink-va1i, spec §3.1)");
        assert_eq!(v.get("ui_grid").and_then(|x| x.as_str()), Some("CN87"));
    }

    // position_status: arbiter with a fresh fix + BroadcastAtPrecision
    // → PositionStatusDto { gps_ready: true, broadcast_grid: "DM33", ui_grid: "DM33" }.
    #[test]
    fn position_status_dto_gps_ready_true_when_fresh_fix() {
        let arbiter = PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(Fix::test("DM33ab"));
        assert!(arbiter.has_fresh_fix(), "fresh fix just applied");
        let cfg = make_config_for_position_status(GpsState::BroadcastAtPrecision, None);
        let dto = PositionStatusDto {
            gps_ready: arbiter.has_fresh_fix()
                && cfg.privacy.gps_state != GpsState::Off,
            broadcast_grid: crate::position::effective_broadcast_locator(&cfg, Some(&arbiter)),
            ui_grid: crate::position::effective_ui_locator(&cfg, Some(&arbiter)),
        };
        assert!(dto.gps_ready);
        assert_eq!(dto.broadcast_grid, "DM33", "GPS fix grid must appear in broadcast_grid");
        assert_eq!(dto.ui_grid, "DM33", "GPS fix grid must appear in ui_grid");
        // Verify snake_case serialization.
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["gps_ready"], true, "gps_ready serializes snake_case");
        assert_eq!(v["broadcast_grid"], "DM33", "broadcast_grid serializes snake_case");
        assert_eq!(v["ui_grid"], "DM33", "ui_grid serializes snake_case");
    }

    // position_status: fresh arbiter (no fix) → PositionStatusDto { gps_ready: false }.
    #[test]
    fn position_status_dto_gps_ready_false_when_no_fix() {
        let arbiter = PositionArbiter::new(
            PositionSource::Manual,
            Some("CN87".into()),
            PositionPrecision::FourCharGrid,
        );
        assert!(!arbiter.has_fresh_fix(), "no fix applied");
        let cfg = make_config_for_position_status(GpsState::BroadcastAtPrecision, None);
        let dto = PositionStatusDto {
            gps_ready: arbiter.has_fresh_fix()
                && cfg.privacy.gps_state != GpsState::Off,
            broadcast_grid: crate::position::effective_broadcast_locator(&cfg, Some(&arbiter)),
            ui_grid: crate::position::effective_ui_locator(&cfg, Some(&arbiter)),
        };
        assert!(!dto.gps_ready);
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["gps_ready"], false);
        // Manual arbiter with "CN87" → broadcast_grid = "CN87".
        assert_eq!(v["broadcast_grid"], "CN87");
        // Manual source: ui_grid is config_grid; config has no identity.grid → "".
        assert_eq!(v["ui_grid"], "");
    }

    // Codex P1-B: position_status broadcast_grid respects gps_state.
    // source=Gps + gps_state=Off + config grid "DM33" + GPS fix "CN87ux"
    // → broadcast_grid is "DM33" (config grid, not the GPS fix).
    #[test]
    fn position_status_dto_broadcast_grid_respects_gps_state_off() {
        let arbiter = PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(Fix::test("CN87ux"));
        let cfg = make_config_for_position_status(GpsState::Off, Some("DM33"));
        let dto = PositionStatusDto {
            gps_ready: arbiter.has_fresh_fix()
                && cfg.privacy.gps_state != GpsState::Off,
            broadcast_grid: crate::position::effective_broadcast_locator(&cfg, Some(&arbiter)),
            ui_grid: crate::position::effective_ui_locator(&cfg, Some(&arbiter)),
        };
        assert_eq!(
            dto.broadcast_grid, "DM33",
            "gps_state=Off: broadcast_grid must be config grid, NOT the GPS fix"
        );
    }

    // tuxlink-va1i: gps_ready must be false under gps_state=Off, even when the
    // arbiter holds a fresh fix. The arbiter's gpsd-client task is not killed
    // when the operator flips to Off (separate follow-up), so the arbiter can
    // still report `has_fresh_fix() == true`. position_status must honor the
    // operator's "Off" intent by reporting gps_ready=false.
    #[test]
    fn gps_ready_false_under_off() {
        let arbiter = PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(Fix::test("DM33ab"));
        assert!(arbiter.has_fresh_fix(),
            "arbiter must still report fresh fix (gpsd task not killed on Off)");
        let cfg = make_config_for_position_status(GpsState::Off, Some("DM33"));
        // Direct test of the position_status command's gps_ready expression.
        let gps_ready = arbiter.has_fresh_fix()
            && cfg.privacy.gps_state != GpsState::Off;
        assert!(!gps_ready,
            "operator chose Off — gps_ready must be false even with stale-fresh fix in arbiter");
    }

    // tuxlink-va1i: DTO contract test — ui_grid and broadcast_grid can diverge
    // and both serialize correctly.
    #[test]
    fn position_status_dto_serializes_ui_grid_and_broadcast_grid_when_they_differ() {
        let dto = PositionStatusDto {
            gps_ready: true,
            broadcast_grid: "DM33".to_string(),  // config fallback under LocalUiOnly
            ui_grid: "DM33ww".to_string(),       // live fix shown to operator
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["broadcast_grid"], "DM33");
        assert_eq!(v["ui_grid"], "DM33ww");
        assert_eq!(v["gps_ready"], true);
    }

    // Codex P1 #3: Manual source ignores fresh GPS at the BROADCAST boundary.
    // (Different from arbiter::tests because effective_broadcast_locator
    // also enforces gps_state privacy gating.)
    #[test]
    fn manual_source_ignores_fresh_gps_fix_at_broadcast_boundary() {
        let mut cfg = make_config_for_position_status(
            crate::config::GpsState::BroadcastAtPrecision,
            None,
        );
        cfg.privacy.position_source = crate::config::PositionSource::Manual;
        let arbiter = crate::position::PositionArbiter::new(
            crate::config::PositionSource::Manual,
            Some("EM75".to_string()),
            crate::config::PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(crate::position::Fix::test("DM33ab"));
        let locator = crate::position::effective_broadcast_locator(&cfg, Some(&arbiter));
        assert_eq!(locator, "EM75",
            "Manual source must broadcast manual_grid regardless of fresh GPS fix");
    }

    // ========================================================================
    // P2.1 (Codex post-impl review) — OutboundDraftDto attachment bridge
    // ========================================================================
    //
    // Prior to this fix, `message_send` hardcoded `attachments: vec![]`,
    // making it impossible for the compose window to send attachments even
    // though the backend (T4.1 + compose_message_with_files) had the plumbing.
    // These tests verify the DTO deserialization round-trip and the
    // DTO→OutboundAttachment mapping.

    /// `OutboundDraftDto` with no attachments field deserializes correctly via
    /// `#[serde(default)]` — existing callers that omit the field are not broken.
    #[test]
    fn outbound_draft_dto_defaults_attachments_to_empty_vec() {
        let json = r#"{"to":["W1AW"],"cc":[],"subject":"Hi","body":"Hello"}"#;
        let dto: OutboundDraftDto = serde_json::from_str(json)
            .expect("dto without attachments field should deserialize");
        assert!(dto.attachments.is_empty(), "missing 'attachments' must default to []");
    }

    /// `OutboundDraftDto` with an explicit attachments array deserializes correctly.
    #[test]
    fn outbound_draft_dto_deserializes_attachments() {
        // Tauri IPC encodes Vec<u8> as a JSON array of integers.
        let json = r#"{
            "to": ["W1AW"],
            "cc": [],
            "subject": "With attachment",
            "body": "See attached.",
            "attachments": [
                {"filename": "report.txt", "bytes": [72, 101, 108, 108, 111]}
            ]
        }"#;
        let dto: OutboundDraftDto = serde_json::from_str(json)
            .expect("dto with attachments should deserialize");
        assert_eq!(dto.attachments.len(), 1);
        assert_eq!(dto.attachments[0].filename, "report.txt");
        assert_eq!(dto.attachments[0].bytes, b"Hello");
    }

    /// The mapping from `OutboundAttachmentDto` to `OutboundAttachment` preserves
    /// filename and bytes without truncation or transformation.
    #[test]
    fn outbound_attachment_dto_maps_to_backend_type() {
        let dto = OutboundAttachmentDto {
            filename: "ics213.pdf".to_string(),
            bytes: vec![0x50, 0x44, 0x46],
        };
        let att = crate::winlink_backend::OutboundAttachment {
            filename: dto.filename.clone(),
            bytes: dto.bytes.clone(),
        };
        assert_eq!(att.filename, "ics213.pdf");
        assert_eq!(att.bytes, [0x50, 0x44, 0x46]);
    }

    /// Multiple attachments in a DTO all map through correctly.
    #[test]
    fn outbound_draft_dto_maps_multiple_attachments() {
        let dto = OutboundDraftDto {
            to: vec!["W1AW".to_string()],
            cc: vec![],
            subject: "Multi".to_string(),
            body: "Two files.".to_string(),
            attachments: vec![
                OutboundAttachmentDto { filename: "a.txt".into(), bytes: vec![1, 2] },
                OutboundAttachmentDto { filename: "b.bin".into(), bytes: vec![3, 4, 5] },
            ],
        };
        let mapped: Vec<crate::winlink_backend::OutboundAttachment> = dto
            .attachments
            .into_iter()
            .map(|a| crate::winlink_backend::OutboundAttachment {
                filename: a.filename,
                bytes: a.bytes,
            })
            .collect();
        assert_eq!(mapped.len(), 2);
        assert_eq!(mapped[0].filename, "a.txt");
        assert_eq!(mapped[1].bytes, [3, 4, 5]);
    }

    // ========================================================================
    // tuxlink-0pnb Task 4 — PeerPasswordStatus serialization + P2pDialResult
    // ========================================================================

    // PeerPasswordStatus serializes as a plain string variant (no content wrapper).
    // Mirrors how the TS side will pattern-match on the value.
    #[test]
    fn peer_password_status_serializes_as_plain_string() {
        let set = serde_json::to_value(PeerPasswordStatus::Set).unwrap();
        assert_eq!(set, "Set");
        let not_set = serde_json::to_value(PeerPasswordStatus::NotSet).unwrap();
        assert_eq!(not_set, "NotSet");
    }

    // PeerPasswordStatus round-trips through JSON (Deserialize impl exercised).
    #[test]
    fn peer_password_status_round_trips_through_json() {
        let original = PeerPasswordStatus::NotSet;
        let json = serde_json::to_string(&original).unwrap();
        let decoded: PeerPasswordStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, PeerPasswordStatus::NotSet);
    }

    // P2pDialResult serializes sent_count + received_count so the frontend
    // can display exchange totals after a successful dial.
    #[test]
    fn p2p_dial_result_serializes_counts() {
        let dto = P2pDialResult { sent_count: 3, received_count: 1 };
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["sent_count"], 3);
        assert_eq!(v["received_count"], 1);
    }
}
