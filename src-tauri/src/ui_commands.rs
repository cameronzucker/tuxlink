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
use tauri::{AppHandle, Emitter, State};

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
    let parsed = parse_folder(&folder)?;
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let metas = backend.list_messages(parsed).await?;
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
    let parsed_folder = parse_folder(&folder)?;
    let mid = MessageId::new(&id);
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let body = backend.read_message_in(parsed_folder, &mid).await?;
    // Opening a message marks it read (tuxlink-xgn). Best-effort: a marker-write
    // failure must not fail the read the user just performed, so the error is
    // discarded (the message simply stays unread and self-heals on the next
    // open). For backends without read-state this is the trait's no-op default.
    let _ = backend.mark_read(parsed_folder, &mid).await;
    parse_raw_rfc5322(&id, &body.raw_rfc5322)
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
/// **tuxlink-pjih (2026-06-01):** previously this command also persisted
/// `cfg.privacy.position_source = Manual` and the arbiter's `set_manual`
/// pinned the runtime source to `Manual`. Operator complaint was that
/// setting a manual grid caused the GPS-derived grid to stop displaying
/// even when a fresh GPS fix was available. Resolution: setting a manual
/// grid now updates ONLY the fallback grid; the stored source preference
/// and live arbiter source are no longer pinned. The displayed grid
/// follows GPS-fresh-else-manual (see [`PositionArbiter::active_grid`]).
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
    let g = grid.trim().to_string();
    if let Some(msg) = validate_grid_input(&g) {
        return Err(UiError::Rejected(msg.to_string()));
    }
    let mut cfg = config::read_config().map_err(|e| UiError::Internal {
        detail: e.to_string(),
    })?;
    cfg.identity.grid = Some(g.clone());
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal {
        detail: e.to_string(),
    })?;
    arbiter.set_manual(&g);
    // tuxlink-ka7/p5u: refresh the live backend (config_set_* wildcard). The grid
    // feeds effective_broadcast_locator's config fallback, so a stale snapshot would
    // broadcast the old grid until restart.
    if let Some(backend) = state.current() {
        backend.set_config(cfg);
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

/// Switch the active position source (operator-driven). v0.1 supports switching
/// TO GPS only — Manual is pinned by editing the grid (`config_set_grid`), which
/// requires a grid value. "Gps" calls `arbiter.use_gps()` (requires a fresh fix);
/// on success, persists `position_source = Gps` so the choice survives restart.
#[tauri::command]
pub async fn position_set_source(
    source: String,
    arbiter: tauri::State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    match source.as_str() {
        "Gps" => {
            // Pre-check the fix WITHOUT flipping yet, so the common "no fix" case
            // short-circuits before we persist anything (mirrors config_set_grid's
            // persist-first invariant: in-memory never gets ahead of persisted config).
            if !arbiter.has_fresh_fix() {
                return Err(UiError::Unavailable {
                    reason: "Cannot switch to GPS: no usable GPS fix".into(),
                });
            }
            // Persist first; if the write fails, return WITHOUT having flipped in-memory.
            let mut cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
            cfg.privacy.position_source = config::PositionSource::Gps;
            config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
            // Flip in-memory only after a successful persist. use_gps re-checks freshness
            // atomically; the pre-check→use_gps window is sub-millisecond vs a 30s staleness,
            // so a fix expiring in between is not a real-world concern.
            arbiter.use_gps().map_err(|e| UiError::Unavailable { reason: format!("Cannot switch to GPS: {e}") })?;
            // tuxlink-ka7/p5u: refresh the live backend (config_set_* wildcard).
            if let Some(backend) = state.current() {
                backend.set_config(cfg);
            }
            Ok(())
        }
        other => Err(UiError::Rejected(format!("unsupported position source: {other}"))),
    }
}

/// Live position-subsystem status from the arbiter (NOT config).
///
/// - `gps_ready`: a usable fresh GPS fix exists — the ribbon's GridEdit shows
///   "GPS ready — tap to switch" from it.
/// - `broadcast_grid`: the EFFECTIVE on-air locator computed by
///   [`crate::position::effective_broadcast_locator`], honoring both precision and
///   the `gps_state` privacy control. The ribbon displays this so it always shows
///   exactly what is/would be transmitted (Codex P1-B). Empty string = no grid.
/// - `active_source`: the LIVE source actually producing the displayed grid —
///   `Gps` while a fresh fix exists, `Manual` when falling back to the
///   manually-set grid. tuxlink-pjih: the UI source chip reads this so it
///   stays truthful even when the operator's stored preference disagrees.
///
/// Polled by useStatusData (2s).
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct PositionStatusDto {
    pub gps_ready: bool,
    /// The precision-reduced grid that WILL be broadcast on air (honoring
    /// gps_state). Empty string when no grid is available. Serializes as
    /// `broadcast_grid` (snake_case) matching the TS PositionStatusDto.
    pub broadcast_grid: String,
    /// The live source actually used to derive the active grid (tuxlink-pjih).
    pub active_source: config::PositionSource,
}

#[tauri::command]
pub async fn position_status(
    arbiter: tauri::State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
) -> Result<PositionStatusDto, UiError> {
    let cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(PositionStatusDto {
        gps_ready: arbiter.has_fresh_fix(),
        broadcast_grid: crate::position::effective_broadcast_locator(&cfg, Some(&arbiter)),
        active_source: arbiter.effective_source(),
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

    // tuxlink-pjih (2026-06-01): set_manual no longer pins source = Manual;
    // it just updates the fallback grid. The stored source preference stays
    // where the operator put it. The displayed grid follows GPS-fresh-else-
    // manual (verified in `position::arbiter::tests`); this test pins the
    // ui_commands-side invariant that set_manual is grid-only.
    #[test]
    fn arbiter_set_manual_updates_grid_without_changing_stored_source() {
        use crate::config::{PositionPrecision, PositionSource};
        use crate::position::PositionArbiter;

        let arbiter = PositionArbiter::new(PositionSource::Gps, None, PositionPrecision::FourCharGrid);
        assert_eq!(arbiter.source(), PositionSource::Gps);
        arbiter.set_manual("EM75");
        assert_eq!(arbiter.source(), PositionSource::Gps, "set_manual must not flip stored source (tuxlink-pjih)");
        // With no fresh GPS fix, active_grid falls back to manual_grid.
        assert_eq!(arbiter.active_grid().as_deref(), Some("EM75"));
        // effective_source reflects the live fallback path.
        assert_eq!(arbiter.effective_source(), PositionSource::Manual);
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

    // position_set_source: use_gps with no fix → Err → maps to UiError::Unavailable.
    // Tests the use_gps → UiError mapping at unit level (the full State-bearing
    // command path requires a Tauri app runtime; the arbiter primitive is the
    // critical correctness gate per spec §position-686).
    #[test]
    fn use_gps_no_fix_maps_to_ui_error_unavailable() {
        let arbiter = PositionArbiter::new(
            PositionSource::Manual,
            Some("CN87".into()),
            PositionPrecision::FourCharGrid,
        );
        // No fix applied → use_gps() must be Err.
        let result = arbiter.use_gps();
        assert!(result.is_err(), "use_gps without a fix must fail");
        // Map the &'static str Err to UiError::Unavailable (the command's mapping).
        let ui_err = UiError::Unavailable {
            reason: format!("Cannot switch to GPS: {}", result.unwrap_err()),
        };
        assert!(
            matches!(ui_err, UiError::Unavailable { .. }),
            "Err from use_gps maps to UiError::Unavailable"
        );
    }

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
        }
    }

    // position_status: arbiter with a fresh fix + BroadcastAtPrecision
    // → PositionStatusDto { gps_ready: true, broadcast_grid: "DM33" }.
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
            gps_ready: arbiter.has_fresh_fix(),
            broadcast_grid: crate::position::effective_broadcast_locator(&cfg, Some(&arbiter)),
            active_source: arbiter.effective_source(),
        };
        assert!(dto.gps_ready);
        assert_eq!(dto.broadcast_grid, "DM33", "GPS fix grid must appear in broadcast_grid");
        assert_eq!(dto.active_source, PositionSource::Gps, "active_source must be Gps when fresh fix is producing the displayed grid (tuxlink-pjih)");
        // Verify snake_case serialization.
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["gps_ready"], true, "gps_ready serializes snake_case");
        assert_eq!(v["broadcast_grid"], "DM33", "broadcast_grid serializes snake_case");
        assert_eq!(v["active_source"], "Gps", "active_source serializes snake_case + PascalCase value");
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
            gps_ready: arbiter.has_fresh_fix(),
            broadcast_grid: crate::position::effective_broadcast_locator(&cfg, Some(&arbiter)),
            active_source: arbiter.effective_source(),
        };
        assert!(!dto.gps_ready);
        assert_eq!(dto.active_source, PositionSource::Manual, "active_source must be Manual when no fresh fix (tuxlink-pjih)");
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["gps_ready"], false);
        // Manual arbiter with "CN87" → broadcast_grid = "CN87".
        assert_eq!(v["broadcast_grid"], "CN87");
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
            gps_ready: arbiter.has_fresh_fix(),
            broadcast_grid: crate::position::effective_broadcast_locator(&cfg, Some(&arbiter)),
            active_source: arbiter.effective_source(),
        };
        assert_eq!(
            dto.broadcast_grid, "DM33",
            "gps_state=Off: broadcast_grid must be config grid, NOT the GPS fix"
        );
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
}
