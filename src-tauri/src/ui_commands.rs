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
//! `Arc` and drops the `RwLock` guard via [`AppBackend::current`] BEFORE
//! awaiting the trait method — the guard is never held across an await.

use mail_parser::{MimeHeaders, MessageParser};
use serde::Serialize;
use tauri::State;

use crate::app_backend::AppBackend;
use crate::winlink_backend::{BackendError, MailboxFolder, MessageId, MessageMeta};

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
    state: State<'_, AppBackend>,
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
#[derive(Debug, Serialize, Clone, PartialEq)]
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
/// (the UI omits the routing strip). `isForm` is true when the text/plain
/// body starts with `<?xml` — the heuristic for a Winlink form payload; the
/// UI renders a "form rendering arrives in v0.1" placeholder in that case.
#[derive(Debug, Serialize, Clone, PartialEq)]
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
/// become U+FFFD; no panic). Form detection: body starting with `<?xml`
/// sets `is_form = true`.
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

    // Date — emit as RFC 3339 UTC from the Date header. If the date is
    // absent or unparseable, fall back to the Unix epoch string so the
    // frontend always receives a valid ISO-8601 string (never panics, never
    // empty). mail-parser gives us a DateTime struct with a timestamp().
    let date = msg
        .date()
        .map(|d| {
            let ts = d.to_timestamp();
            // Format as RFC 3339 UTC: YYYY-MM-DDTHH:MM:SSZ
            format_unix_ts(ts)
        })
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

    // Body: find the first text/plain part; decode lossily.
    let body = find_text_plain_body(&msg);

    // Winlink form detection: text/plain body starting with `<?xml`.
    let is_form = body.trim_start().starts_with("<?xml");

    // Attachments: non-inline named parts (MIME attachments).
    let attachments = collect_attachments(&msg);

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

/// Extract a routing string from known Winlink / Pat transport headers.
/// Checks a prioritized list of custom headers; returns `None` when absent.
fn extract_routing(msg: &mail_parser::Message<'_>) -> Option<String> {
    // Known Winlink / Pat transport-info headers (order of preference).
    const TRANSPORT_HEADERS: &[&str] = &[
        "X-Winlink-Route",
        "X-Received-Winlink-Transport",
        "X-Pat-Transport",
    ];
    for &header_name in TRANSPORT_HEADERS {
        if let Some(hv) = msg.header(header_name) {
            // msg.header() returns Option<&HeaderValue> directly.
            if let mail_parser::HeaderValue::Text(s) = hv {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
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
    state: State<'_, AppBackend>,
) -> Result<ParsedMessageDto, UiError> {
    let parsed_folder = parse_folder(&folder)?;
    let mid = MessageId::new(&id);
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let body = backend.read_message_in(parsed_folder, &mid).await?;
    parse_raw_rfc5322(&id, &body.raw_rfc5322)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink_backend::MessageId;

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
}
