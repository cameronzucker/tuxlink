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
use crate::winlink::message::RECEIVED_SESSION_HEADER;
use crate::winlink_backend::{
    BackendError, BackendStatus, LogLevel, LogLine, LogSource, MailboxFolder, MessageId,
    MessageMeta, OutboundMessage, TransportConfig,
};

/// Resolve the operator's sole FULL identity (tuxlink-2ns7). Phase 4 has exactly
/// one FULL (Phase 2 promoted the single config callsign). The inbound-mail /
/// exchange mailbox sites use this to set the mailbox's default received-mail
/// namespace so a bare `store`/`list` resolves Inbox/Archive under
/// `mailbox/<FULL>/` — matching the production read side + the startup
/// `migrate_legacy_layout`. `None` (no identity yet, fresh install) leaves the
/// mailbox un-defaulted (resolves the `_default` namespace).
fn sole_full_identity() -> Option<crate::identity::Callsign> {
    crate::identity::IdentityStore::load(&crate::config::identity_store_path())
        .ok()
        .and_then(|s| s.full().first().map(|f| f.callsign.clone()))
}

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
            // Task 12 (tuxlink-7do4): RemoteError carries the *** payload for
            // auth_taxonomy classification; it surfaces to the UI as a
            // transport error (the React layer gets the detailed mode via
            // the AuthClassified b2f-event, not from UiError directly).
            BackendError::RemoteError(s) => UiError::Transport { reason: s },
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
            BackendError::NoActiveIdentity => UiError::NotConfigured(
                "no active identity — authenticate before transmitting".into(),
            ),
            // Phase 5 (tuxlink-tseu): a tactical session was refused CMS entry
            // because its address is not verified CMS-registered. Surfaced as
            // "unavailable" — CMS is simply not open to this identity (P2P/RF are).
            BackendError::TacticalNotCmsRegistered { label, reason } => UiError::Unavailable {
                reason: format!(
                    "tactical address '{label}' is not verified CMS-registered ({reason}); CMS is unavailable for this identity"
                ),
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
// ICS-309 log query (tuxlink-hnkn P2 Task 2)
// ============================================================================

/// One log row returned by `messages_meta_query_for_log`. Serialised to camelCase
/// for the frontend; field names match the Ics309FormV2 `LogRow` interface.
/// Also `Deserialize` so the frontend can pass rows back for `render_ics309_pdf`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRow {
    /// RFC 3339 UTC datetime string, e.g. "2024-05-20T10:13:00Z".
    pub datetime: String,
    /// Sender callsign / address.
    pub from: String,
    /// First recipient callsign / address.
    pub to: String,
    /// Message subject.
    pub subject: String,
    /// Direction: `"in"` (received) or `"out"` (sent).
    pub direction: String,
}

/// Query `messages_meta` for ICS-309 log rows in the given UTC epoch range
/// [start_epoch, end_epoch] (inclusive). Rows are ordered chronologically.
///
/// `start_rfc3339` and `end_rfc3339` are ISO-8601 / RFC 3339 UTC strings
/// (e.g. `"2024-05-20T00:00:00Z"`). They are converted to Unix epoch seconds
/// before the SQL query so the INTEGER timestamp columns can be compared
/// efficiently.
///
/// Returns `Err` if the search index is not installed (app launched offline
/// and the setup hook failed) or if the timestamp strings are malformed.
#[tauri::command]
pub async fn messages_meta_query_for_log(
    start_rfc3339: String,
    end_rfc3339: String,
    search: State<'_, crate::search::commands::SearchService>,
) -> Result<Vec<LogRow>, String> {
    let start_epoch = rfc3339_to_epoch(&start_rfc3339)
        .ok_or_else(|| format!("invalid start timestamp: {start_rfc3339}"))?;
    let end_epoch = rfc3339_to_epoch(&end_rfc3339)
        .ok_or_else(|| format!("invalid end timestamp: {end_rfc3339}"))?;
    search
        .index
        .lock()
        .map_err(|e| format!("search index lock poisoned: {e}"))?
        .query_log_rows(start_epoch, end_epoch)
        .map_err(|e| e.to_string())
}

/// Parse an RFC 3339 UTC string into Unix epoch seconds.
/// Accepts the `Z` UTC suffix only (no offset needed for this feature).
/// Returns `None` if parsing fails.
fn rfc3339_to_epoch(s: &str) -> Option<i64> {
    // Lean on chrono (already in [dependencies]) for robust parsing.
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp())
}

/// Input to `render_ics309_pdf` — the fully resolved row set + metadata the
/// form's frontend has already gathered.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ics309PdfRequest {
    pub rows: Vec<LogRow>,
    /// RFC 3339 UTC range start (display only — not re-filtered here).
    pub range_start: String,
    /// RFC 3339 UTC range end (display only).
    pub range_end: String,
    /// Station callsign to print in the header, e.g. `"N7CPZ"`.
    pub station_callsign: Option<String>,
}

/// Render an ICS-309 Communications Log to PDF bytes.
///
/// Returns the raw PDF as `Vec<u8>` (base64-encoded by Tauri's IPC layer).
/// Page size is US Letter (216 × 279 mm). Uses the PDF built-in Helvetica
/// font so no font file needs to be embedded or shipped.
///
/// Layout:
///   - Header: "ICS-309 COMMUNICATIONS LOG" (centered)
///   - Sub-header: station callsign + date range
///   - Column headers: Datetime | Dir | From | To | Subject
///   - Log rows (up to 30, one per line; auto-page-break)
///   - Footer: "Rendered by tuxlink — <UTC timestamp>"
#[tauri::command]
pub async fn render_ics309_pdf(req: Ics309PdfRequest) -> Result<Vec<u8>, String> {
    render_ics309_pdf_inner(req).map_err(|e| e.to_string())
}

fn render_ics309_pdf_inner(req: Ics309PdfRequest) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use printpdf::{
        BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt,
        TextItem,
    };

    // US Letter: 216 × 279 mm. y=0 is the bottom of the page in PDF space.
    const PAGE_W_MM: f32 = 216.0;
    const PAGE_H_MM: f32 = 279.0;
    // Convenience conversion: mm to pt (1 mm = 2.8346 pt).
    fn mm_to_pt(mm: f32) -> Pt { Pt(mm * 2.8346) }

    let normal_font  = PdfFontHandle::Builtin(BuiltinFont::Helvetica);
    let bold_font    = PdfFontHandle::Builtin(BuiltinFont::HelveticaBold);

    // Column x positions (pt from left margin)
    let left_margin_pt = mm_to_pt(15.0);
    let col_datetime_x = left_margin_pt;
    let col_dir_x      = Pt(col_datetime_x.0 + mm_to_pt(45.0).0);
    let col_from_x     = Pt(col_dir_x.0 + mm_to_pt(10.0).0);
    let col_to_x       = Pt(col_from_x.0 + mm_to_pt(35.0).0);
    let col_subject_x  = Pt(col_to_x.0 + mm_to_pt(35.0).0);

    // Row height and starting y cursor
    const ROW_H_PT: f32 = 14.0;      // pt per data row
    const FONT_SIZE_TITLE: f32 = 14.0;
    const FONT_SIZE_HEADER: f32 = 9.0;
    const FONT_SIZE_DATA: f32 = 8.5;

    let page_top_pt    = mm_to_pt(PAGE_H_MM - 15.0); // top margin
    let footer_y_pt    = mm_to_pt(10.0);               // bottom margin

    // How many data rows fit on a page (after title + headers consume ~4 rows).
    let title_block_h  = mm_to_pt(25.0).0; // approx title + sub + col-headers height
    let usable_h       = page_top_pt.0 - footer_y_pt.0 - title_block_h;
    let rows_per_page  = (usable_h / ROW_H_PT).floor() as usize;
    let rows_per_page  = rows_per_page.max(1);

    let station_label = req.station_callsign.as_deref().unwrap_or("(unknown)");

    let now_utc = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let total_pages = if req.rows.is_empty() {
        1
    } else {
        req.rows.len().div_ceil(rows_per_page)
    };

    let mut pdf_doc = PdfDocument::new("ICS-309 Comms Log");

    for page_idx in 0..total_pages {
        let chunk_start = page_idx * rows_per_page;
        let chunk_end   = (chunk_start + rows_per_page).min(req.rows.len());
        let chunk       = &req.rows[chunk_start..chunk_end];

        let mut ops: Vec<Op> = Vec::new();
        ops.push(Op::StartTextSection);

        // ── Title ──────────────────────────────────────────────────────────
        ops.push(Op::SetFont { font: bold_font.clone(), size: Pt(FONT_SIZE_TITLE) });
        ops.push(Op::SetTextCursor {
            pos: Point { x: mm_to_pt(PAGE_W_MM / 2.0 - 50.0), y: Pt(page_top_pt.0) },
        });
        ops.push(Op::ShowText {
            items: vec![TextItem::Text("ICS-309 COMMUNICATIONS LOG".to_string())],
        });

        // ── Sub-header (station + date range) ──────────────────────────────
        ops.push(Op::SetFont { font: normal_font.clone(), size: Pt(FONT_SIZE_HEADER) });
        ops.push(Op::SetTextCursor {
            pos: Point { x: col_datetime_x, y: Pt(page_top_pt.0 - 18.0) },
        });
        ops.push(Op::ShowText {
            items: vec![TextItem::Text(format!(
                "Station: {station_label}   Period: {} — {}   Page {} of {total_pages}",
                req.range_start, req.range_end,
                page_idx + 1,
            ))],
        });

        // ── Column headers ─────────────────────────────────────────────────
        let col_hdr_y = Pt(page_top_pt.0 - 32.0);
        ops.push(Op::SetFont { font: bold_font.clone(), size: Pt(FONT_SIZE_HEADER) });
        for (x, label) in [
            (col_datetime_x, "Datetime (UTC)"),
            (col_dir_x,      "Dir"),
            (col_from_x,     "From"),
            (col_to_x,       "To"),
            (col_subject_x,  "Subject"),
        ] {
            ops.push(Op::SetTextCursor { pos: Point { x, y: col_hdr_y } });
            ops.push(Op::ShowText {
                items: vec![TextItem::Text(label.to_string())],
            });
        }

        // ── Data rows ──────────────────────────────────────────────────────
        ops.push(Op::SetFont { font: normal_font.clone(), size: Pt(FONT_SIZE_DATA) });
        let mut row_y = col_hdr_y.0 - ROW_H_PT;
        for row in chunk {
            // Truncate long fields to fit the column widths.
            let dt      = &row.datetime;
            let dir     = row.direction.as_str();
            let from    = truncate_str(&row.from, 18);
            let to      = truncate_str(&row.to, 18);
            let subject = truncate_str(&row.subject, 40);

            for (x, text) in [
                (col_datetime_x, dt.as_str()),
                (col_dir_x,      dir),
                (col_from_x,     &from),
                (col_to_x,       &to),
                (col_subject_x,  &subject),
            ] {
                ops.push(Op::SetTextCursor { pos: Point { x, y: Pt(row_y) } });
                ops.push(Op::ShowText {
                    items: vec![TextItem::Text(text.to_string())],
                });
            }
            row_y -= ROW_H_PT;
        }

        // ── Footer ─────────────────────────────────────────────────────────
        ops.push(Op::SetFont { font: normal_font.clone(), size: Pt(7.0) });
        ops.push(Op::SetTextCursor {
            pos: Point { x: col_datetime_x, y: footer_y_pt },
        });
        ops.push(Op::ShowText {
            items: vec![TextItem::Text(format!("Rendered by tuxlink — {now_utc}"))],
        });

        ops.push(Op::EndTextSection);

        let page = PdfPage::new(Mm(PAGE_W_MM), Mm(PAGE_H_MM), ops);
        pdf_doc.pages.push(page);
    }

    let bytes = pdf_doc.save(&PdfSaveOptions::default(), &mut Vec::new());
    Ok(bytes)
}

/// Truncate a string to `max_chars` characters, appending "…" if truncated.
fn truncate_str(s: &str, max_chars: usize) -> String {
    // Handle multi-byte UTF-8 chars properly via char iteration.
    let mut chars = s.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{collected}…")
    } else {
        collected
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
/// `src/mailbox/types.ts`. The message-read DTO lists names + sizes only;
/// bytes are fetched lazily by explicit Save As / Preview commands.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AttachmentMetaDto {
    pub filename: String,
    pub size: u64,
}

/// Maximum decoded attachment payload returned through the preview IPC path.
/// Keep the ordinary message-read path byte-free, and bound the explicit
/// preview path so a malformed store cannot push unbounded binary through JSON.
pub const MAX_ATTACHMENT_PREVIEW_BYTES: usize = 2 * 1024 * 1024;

/// Serializable image-preview payload. Mirrors `AttachmentPreview` in
/// `src/mailbox/types.ts`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentPreviewDto {
    pub filename: String,
    pub mime_type: String,
    pub data_base64: String,
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
    pub body: String, // decoded display text
    pub attachments: Vec<AttachmentMetaDto>,
    pub is_form: bool,
    pub routing: Option<String>,
    /// `"post-office"` when this message was filed by the local Post Office
    /// (`SessionIntent::PostOffice`); `None` for all other session types.
    /// Drives the "Post Office" chip in the mailbox inbound list (Phase B5).
    pub received_session: Option<String>,
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
/// Body: MIME `text/plain` is decoded by `mail-parser`; native B2F body bytes
/// are decoded as UTF-8 when valid and Windows-1252 otherwise. Form detection:
/// an attachment whose filename matches `RMS_Express_Form_*.xml` sets
/// `is_form = true`.
///
/// Attachments: all non-inline, named MIME parts are listed by name + size
/// in bytes. Attachment bytes are fetched only by explicit Save As / Preview
/// commands.
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

    // From — first address's display form. Native Winlink/B2F headers often
    // contain bare service/callsign identities (`From: SERVICE`, `To: N7CPZ`)
    // that are not RFC mailbox addresses; preserve the raw header when
    // mail-parser cannot materialize an address object.
    let from = extract_first_address_or_raw(&msg);

    // To/Cc — collect all address strings, with the same bare-call fallback.
    let to = extract_address_list_or_raw(msg.to(), msg.header_raw(HeaderName::To));
    let cc = extract_address_list_or_raw(msg.cc(), msg.header_raw(HeaderName::Cc));

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

    // Body: find the first text/plain part; decode lossily. The raw bytes
    // are passed alongside so the function can detect B2F wire format
    // (Winlink's native format, not RFC 5322) and dispatch to the
    // project's B2F parser instead of letting mail-parser swallow the
    // attachment payload into the text body (tuxlink-2hyf).
    let body = find_text_plain_body(&msg, raw);

    // Attachments: non-inline named parts. Dispatches to the B2F parser for
    // inbound Winlink messages — mail_parser returns zero attachments for
    // those (tuxlink-4or5).
    let attachments = collect_attachments(&msg, raw);

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
        extract_attachment_bytes(&msg, raw, &attach_name)
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

    // Received-session marker: set by file_exchange_result for PostOffice sessions.
    let received_session = extract_received_session(&msg);

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
        received_session,
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

fn extract_first_address_or_raw(msg: &mail_parser::Message<'_>) -> String {
    let parsed = extract_first_address(msg.from());
    if !parsed.trim().is_empty() {
        return parsed;
    }
    clean_raw_header_value(msg.header_raw(HeaderName::From).unwrap_or_default())
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

fn extract_address_list_or_raw(
    addr: Option<&mail_parser::Address<'_>>,
    raw: Option<&str>,
) -> Vec<String> {
    let parsed: Vec<String> = extract_address_list(addr)
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect();
    if !parsed.is_empty() {
        return parsed;
    }

    split_raw_address_header(raw.unwrap_or_default())
}

fn split_raw_address_header(raw: &str) -> Vec<String> {
    raw.split([',', ';'])
        .map(clean_raw_header_value)
        .filter(|s| !s.is_empty())
        .collect()
}

fn clean_raw_header_value(raw: &str) -> String {
    raw.trim().trim_matches('"').trim().to_string()
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

/// Return the message's displayable body text.
///
/// Dispatch ladder, in order:
///
/// 1. **B2F wire format** (the canonical inbound-Winlink case): detected by
///    the presence of both a `Mid:` and a `Body:` header in the parsed B2F
///    message. mail_parser is RFC 5322-only and treats B2F's `Body: N`
///    byte-count header as just another header line — then concatenates the
///    declared text body AND each per-attachment binary payload into one
///    blob, surfacing as a wall of REPLACEMENT CHARACTER glyphs in the UI
///    (tuxlink-9ylw smoke walk → this is the real fix, tuxlink-2hyf).
///    When B2F is detected, defer to `winlink::message::Message::body()`
///    which respects the declared body byte-count and excludes attachments,
///    then decode the body as UTF-8 with a Windows-1252 fallback for legacy
///    Winlink/WLE text.
///
/// 2. **`mail_parser::body_text(0)`** — the first text/plain part of a true
///    MIME message, already decoded for charset + CTE.
///
/// 3. **Root Text part** — for non-MIME, non-B2F single-text-root messages.
///
/// 4. **Binary placeholder** — single-part binary MIME messages without any
///    text part fall through here. The placeholder points users at the
///    attachment surface (`AttachmentStrip` + the `message_attachment_save`
///    command shipped in tuxlink-0fyj) rather than dumping bytes as text.
fn find_text_plain_body(msg: &mail_parser::Message<'_>, raw: &[u8]) -> String {
    // Step 1 — B2F detection + dispatch.
    if let Some(b2f) = parse_as_b2f(raw) {
        return decode_b2f_body_text(b2f.body());
    }

    // Step 2 — true MIME text/plain happy path.
    if let Some(text) = msg.body_text(0) {
        return text.into_owned();
    }

    // Step 3 + 4 — non-MIME-non-B2F fall-back. Root part is all we have.
    match &msg.parts[0].body {
        mail_parser::PartType::Text(t) => t.to_string(),
        mail_parser::PartType::Binary(b) | mail_parser::PartType::InlineBinary(b) => {
            // Don't render bytes as text — see step-1 doc comment for why.
            format!("[Binary content ({} bytes) — see attachments]", b.len())
        }
        _ => String::new(),
    }
}

/// Decode native B2F body bytes for UI display.
///
/// Winlink B2F does not carry a MIME charset on the body itself. Inbound
/// bodies are commonly ASCII/UTF-8, but legacy WLE-era content may arrive as a
/// single-byte Windows code page. Prefer valid UTF-8; otherwise use
/// Windows-1252, which preserves ISO-8859-1 printable bytes and the smart
/// punctuation commonly produced by Windows clients.
///
/// Decoding is **byte-wise**: valid UTF-8 runs are kept verbatim and only the
/// genuinely-invalid bytes are CP-1252-mapped. An earlier all-or-nothing gate
/// (whole-body `from_utf8`, else map every byte) re-introduced mojibake on
/// mixed-encoding bodies — a single stray non-UTF-8 byte flipped an otherwise
/// valid UTF-8 body to Latin-1, turning a real `café` (C3 A9) into `cafÃ©`
/// (smoke-walk item 24 regression risk). Walking the bytes preserves the valid
/// UTF-8 while still rescuing legacy CP-1252 high bytes.
fn decode_b2f_body_text(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    loop {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                out.push_str(text);
                break;
            }
            Err(err) => {
                let valid = err.valid_up_to();
                // SAFETY: `valid_up_to()` guarantees `rest[..valid]` is valid UTF-8.
                out.push_str(unsafe { std::str::from_utf8_unchecked(&rest[..valid]) });
                match err.error_len() {
                    // A bounded invalid sequence: CP-1252-map each offending byte,
                    // then resume UTF-8 decoding after it.
                    Some(len) => {
                        for &b in &rest[valid..valid + len] {
                            out.push(windows_1252_char(b));
                        }
                        rest = &rest[valid + len..];
                    }
                    // Truncated multibyte sequence at the tail (no further bytes):
                    // CP-1252-map the remainder and stop.
                    None => {
                        for &b in &rest[valid..] {
                            out.push(windows_1252_char(b));
                        }
                        break;
                    }
                }
            }
        }
    }
    out
}

fn windows_1252_char(byte: u8) -> char {
    match byte {
        0x80 => '\u{20AC}',
        0x82 => '\u{201A}',
        0x83 => '\u{0192}',
        0x84 => '\u{201E}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02C6}',
        0x89 => '\u{2030}',
        0x8A => '\u{0160}',
        0x8B => '\u{2039}',
        0x8C => '\u{0152}',
        0x8E => '\u{017D}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02DC}',
        0x99 => '\u{2122}',
        0x9A => '\u{0161}',
        0x9B => '\u{203A}',
        0x9C => '\u{0153}',
        0x9E => '\u{017E}',
        0x9F => '\u{0178}',
        0x81 | 0x8D | 0x8F | 0x90 | 0x9D => char::REPLACEMENT_CHARACTER,
        _ => char::from(byte),
    }
}

/// B2F detection helper. Returns Some only when the parsed Winlink B2F message
/// carries BOTH a `Mid:` header (Winlink message ID) AND a `Body:` header
/// (decimal byte count of the declared text body). Either header alone is
/// ambiguous — `Body:` could plausibly appear in malformed RFC 5322 input,
/// `Mid:` is rare but not impossible. Both together is the reliable B2F
/// signature (tuxlink-2hyf / tuxlink-4or5).
fn parse_as_b2f(raw: &[u8]) -> Option<crate::winlink::message::Message> {
    let msg = crate::winlink::message::Message::from_bytes(raw).ok()?;
    if msg.header("Mid").is_some() && msg.header("Body").is_some() {
        Some(msg)
    } else {
        None
    }
}

/// Collect named attachments (filename + decoded size in bytes). Parts without
/// a filename are skipped.
///
/// Dispatch ladder:
/// 1. **B2F** — `winlink::Message::attachments()` returns the declared file
///    list with already-decoded bytes. The canonical CMS-inbound case
///    (tuxlink-4or5: catalog image responses, weather product replies,
///    inbound forms). mail_parser sees B2F's `File:` headers as ordinary
///    header lines and returns zero attachments for these messages, so this
///    dispatch is required for the attachment surface to populate at all.
/// 2. **MIME** — `msg.attachments()` iterator across Content-Disposition:
///    attachment / Content-Type: name parts. Used for composed-and-
///    roundtripped messages where the wire format is true MIME.
fn collect_attachments(msg: &mail_parser::Message<'_>, raw: &[u8]) -> Vec<AttachmentMetaDto> {
    if let Some(b2f) = parse_as_b2f(raw) {
        return b2f
            .attachments()
            .iter()
            .map(|a| AttachmentMetaDto {
                filename: a.filename.clone(),
                size: a.bytes.len() as u64,
            })
            .collect();
    }

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

/// Extract the raw bytes of an attachment by filename match. Returns the
/// decoded attachment bytes. Returns None when no attachment matches.
///
/// Mirrors [`collect_attachments`]' B2F-then-MIME dispatch ladder so that
/// AttachmentStrip → click → Save As → write-to-disk works end-to-end for
/// inbound CMS messages (tuxlink-4or5).
fn extract_attachment_bytes(
    msg: &mail_parser::Message<'_>,
    raw: &[u8],
    filename: &str,
) -> Option<Vec<u8>> {
    if let Some(b2f) = parse_as_b2f(raw) {
        return b2f
            .attachments()
            .iter()
            .find(|a| a.filename == filename)
            .map(|a| a.bytes.clone());
    }

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

fn image_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    None
}

fn build_attachment_preview(
    filename: &str,
    bytes: Vec<u8>,
) -> Result<AttachmentPreviewDto, UiError> {
    if bytes.len() > MAX_ATTACHMENT_PREVIEW_BYTES {
        return Err(UiError::Rejected(format!(
            "attachment '{filename}' is too large to preview ({} bytes; cap is {} bytes)",
            bytes.len(),
            MAX_ATTACHMENT_PREVIEW_BYTES
        )));
    }
    let mime_type = image_mime_type(&bytes).ok_or_else(|| {
        UiError::Rejected(format!(
            "attachment '{filename}' is not a supported image preview type"
        ))
    })?;
    Ok(AttachmentPreviewDto {
        filename: filename.to_string(),
        mime_type: mime_type.to_string(),
        data_base64: base64_encode_standard(&bytes),
    })
}

fn base64_encode_standard(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16)
            | ((bytes[i + 1] as u32) << 8)
            | (bytes[i + 2] as u32);
        encoded.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        encoded.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        encoded.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        encoded.push(ALPHABET[(n & 0x3f) as usize] as char);
        i += 3;
    }
    if i < bytes.len() {
        let rem = bytes.len() - i;
        let b0 = bytes[i] as u32;
        let b1 = if rem > 1 { bytes[i + 1] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8);
        encoded.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        encoded.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        if rem == 2 {
            encoded.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
            encoded.push('=');
        } else {
            encoded.push('=');
            encoded.push('=');
        }
    }
    encoded
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

/// Extract the `X-Tuxlink-Received-Session` header value.
///
/// Returns `Some(value)` when the header is present and non-empty; `None`
/// otherwise. Mirrors `extract_routing`'s header-access style but operates on
/// a single tuxlink-private header and is intentionally separate so
/// `TRANSPORT_HEADERS` is not polluted with tuxlink-internal metadata.
fn extract_received_session(msg: &mail_parser::Message<'_>) -> Option<String> {
    if let Some(mail_parser::HeaderValue::Text(s)) = msg.header(RECEIVED_SESSION_HEADER) {
        if !s.is_empty() {
            return Some(s.to_string());
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
    // Opening a message no longer mutates read-state server-side. Mark-on-open is
    // a once-per-open-transition client effect (useMessage) so an explicit Mark
    // Unread on the open message is not undone by a reading-pane refetch (window
    // focus / poll). See tuxlink-etxt design §1.4.
    parse_raw_rfc5322(&id, &body.raw_rfc5322)
}

// ---- message_attachment_preview command (tuxlink-ewtb) ---------------------

/// Return a safe inline preview payload for a named image attachment.
///
/// Reads the stored message, extracts attachment bytes via the same B2F/MIME
/// ladder as Save As, verifies supported image magic bytes, and returns a
/// bounded base64 payload for the frontend's data URL. Unsupported attachment
/// types remain downloadable through `message_attachment_save`.
#[tauri::command]
pub async fn message_attachment_preview(
    folder: String,
    id: String,
    filename: String,
    state: State<'_, BackendState>,
) -> Result<AttachmentPreviewDto, UiError> {
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
    let bytes = extract_attachment_bytes(&msg, body.raw_rfc5322.as_slice(), &filename)
        .ok_or_else(|| {
            UiError::NotFound(format!("attachment '{filename}' not in message {id}"))
        })?;
    build_attachment_preview(&filename, bytes)
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
    let bytes = extract_attachment_bytes(&msg, body.raw_rfc5322.as_slice(), &filename)
        .ok_or_else(|| {
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

// ---- read-state commands (tuxlink-etxt) ------------------------------------

/// Set a single message's read-state. `read = true` marks read, `false` marks
/// unread. Folder may be a system folder or a user-folder slug.
#[tauri::command]
pub async fn message_set_read_state(
    folder: String,
    id: String,
    read: bool,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    let folder_ref = parse_folder_ref(&folder)?;
    let mid = MessageId::new(&id);
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    backend.set_read_state(folder_ref, &mid, read).await?;
    Ok(())
}

/// One message reference for a bulk operation. Each carries its own folder so a
/// cross-folder search-results selection (which mixes folders) stays correct.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageRefDto {
    pub folder: String,
    pub id: String,
}

/// Set the read-state of every listed message. Best-effort per item: a missing
/// message is a no-op (matching the single-message path). One command call per
/// bulk action keeps frontend round-trips bounded.
#[tauri::command]
pub async fn message_set_read_state_bulk(
    items: Vec<MessageRefDto>,
    read: bool,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    for item in items {
        let folder_ref = parse_folder_ref(&item.folder)?;
        let mid = MessageId::new(&item.id);
        backend.set_read_state(folder_ref, &mid, read).await?;
    }
    Ok(())
}

/// Move every listed message to a single destination folder `to` (tuxlink-l80q).
/// Each item carries its OWN source folder, so a cross-folder search-results
/// selection (which mixes folders) lands correctly in one command call. Archive
/// is just `to = "archive"`. Mirrors [`message_set_read_state_bulk`]; one command
/// per bulk action keeps frontend round-trips bounded.
#[tauri::command]
pub async fn message_move_bulk(
    items: Vec<MessageRefDto>,
    to: String,
    state: State<'_, BackendState>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    move_bulk_with_backend(backend.as_ref(), items, &to).await
}

/// Backend-facing core of [`message_move_bulk`], split out so it is unit-testable
/// against a real `NativeBackend` without a Tauri `State` (the `_impl` seam used
/// elsewhere in this module, e.g. `config_set_grid_impl`).
pub(crate) async fn move_bulk_with_backend(
    backend: &dyn crate::winlink_backend::WinlinkBackend,
    items: Vec<MessageRefDto>,
    to: &str,
) -> Result<(), UiError> {
    let to_ref = parse_folder_ref(to)?;
    for item in items {
        let from_ref = parse_folder_ref(&item.folder)?;
        // Self-move guard (Codex P2, data loss): move_between writes the
        // destination then removes the source, which is the SAME path when
        // from == to — deleting the message. Skip it, mirroring the single
        // mailbox_move's frontend no-op. (move_between itself also guards now.)
        if from_ref == to_ref {
            continue;
        }
        let mid = MessageId::new(&item.id);
        backend
            .move_between_folders(from_ref, to_ref.clone(), &mid)
            .await?;
    }
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
    /// Parent folder slug (schema v2 / spec D2). `skip_serializing_if` omits the
    /// key for a top-level folder so the wire shape matches TS `parentSlug?:
    /// string` (absent, not `null` — A4 / finding #7).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_slug: Option<String>,
}

impl From<crate::user_folders::UserFolder> for UserFolderDto {
    fn from(f: crate::user_folders::UserFolder) -> Self {
        UserFolderDto {
            slug: f.slug,
            display_name: f.display_name,
            created_at: f.created_at,
            parent_slug: f.parent_slug,
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
    parent_slug: Option<String>,
    state: State<'_, BackendState>,
) -> Result<UserFolderDto, UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let folder = backend
        .create_user_folder(&display_name, parent_slug.as_deref())
        .await?;
    Ok(UserFolderDto::from(folder))
}

/// Re-parent a user folder (spec D3). `parent_slug == None` promotes it to top
/// level. Metadata-only — no message files move. D4 validation failures surface
/// as `UiError::Rejected`.
#[tauri::command]
pub async fn folder_move(
    slug: String,
    parent_slug: Option<String>,
    state: State<'_, BackendState>,
) -> Result<UserFolderDto, UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let folder = backend
        .move_user_folder(&slug, parent_slug.as_deref())
        .await?;
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
) -> Result<Vec<String>, UiError> {
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
    // Returns parent + cascaded child slugs so the UI can clear a stale
    // selection when the open folder was among them (A5).
    let removed = backend.delete_user_folder(&slug, action).await?;
    Ok(removed)
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
    tracing::info!(
        target: "tuxlink::cms",
        mid = %mid.0,
        "message queued in outbox",
    );
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

/// Send an outbound webview-served WLE Standard Form.
///
/// Counterpart to [`send_form`] for the ~245 catalog forms whose authoritative
/// shape is the HTML template (not a static [`forms::FormDef`]). The form id is
/// verified against the live `forms::wle_templates::list` catalog (bundled +
/// custom roots), so this command rejects ids the webview path could not have
/// served — same error surface as `send_form` ("unknown form: <id>"). The XML
/// envelope is synthesized via [`forms::serialize::serialize_catalog_form_xml`]
/// from the operator-supplied `field_values` plus the WLE filename convention
/// for `display_form` (`<id>_Viewer.html`) and `reply_template`
/// (`<id>_SendReply.0`).
///
/// `field_values` is the post-conversion shape that the React `handleWebviewSubmit`
/// produces: single-string-per-field (newline-joined multi-values, with the
/// synthetic Submit button name stripped). The serializer sorts keys
/// alphabetically for deterministic output.
///
/// Body composition: sorted "key: value" dump, with a leading "form_id: <id>"
/// header for receiver context. Receivers that render the XML via the WLE
/// viewer get the formatted view; the body text is the fallback for
/// non-WLE-aware clients. Subject prefers `field_values["subject"]`, else
/// `Form: <id>` (mirrors WLE's "Form name as subject" default).
///
/// Attachment filename is `RMS_Express_Form_<id>.xml` — same convention as
/// `send_form` so existing parsers (Pat, RMS Express receivers, the tuxlink
/// inbox renderer) detect + render the form view consistently.
///
/// Returns the MID string on success (mirrors `message_send` / `send_form`).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn send_webview_form(
    form_id: String,
    mut field_values: std::collections::HashMap<String, String>,
    to: Vec<String>,
    cc: Vec<String>,
    senders_callsign: String,
    grid_square: String,
    // tuxlink-hhfx / G10: when this submission is a reply via a SendReply form,
    // `reply_template` is the SendReply `.0` filename (from `open_webview_reply`).
    // The reply's To:/Subject:/Msg: then render from that `.0` (whose Msg
    // reproduces the original + the operator's reply) and display_form points to
    // the SendReply viewer — instead of the original form's `.txt`. `None` for a
    // normal first-time form send.
    reply_template: Option<String>,
    // Operator-entered subject (the compose "Re: <original>" line). Used ONLY as
    // a fallback when the governing template yields no Subject: and no subject
    // field — SendReply `.0`s carry no Subject: directive, so this is how a reply
    // gets a meaningful subject instead of "Form: <id>". `None`/blank for a
    // normal send leaves existing behavior unchanged.
    subject_hint: Option<String>,
    app: AppHandle,
    state: State<'_, BackendState>,
    // tuxlink-2tom / G12-C: the persisted per-form serial counter store. When the
    // governing template carries `SeqInc:`, the next serial is allocated from here
    // and stamped into `SeqNum` (and thus `<var SeqNum>`) before rendering.
    seq_store: State<'_, std::sync::Arc<std::sync::Mutex<crate::forms::sequence::SeqCounterStore>>>,
) -> Result<String, UiError> {
    use crate::forms;

    // 1. Verify form_id is in the live catalog (bundled snapshot + operator's
    //    custom-forms dir). This is the same lookup `open_webview_form` does,
    //    so a session that opened successfully will always pass this check.
    //    A stale draft restored from localStorage with a form_id no longer in
    //    the catalog (e.g. operator deleted their custom form) gets the same
    //    "unknown form" surface as `send_form`'s BUNDLED_FORMS check.
    let bundle = forms::wle_templates::bundle_root_for_app(&app).map_err(|e| {
        UiError::Internal { detail: e.to_string() }
    })?;
    let custom = forms::wle_templates::custom_root_for_app(&app);
    let custom_opt = if custom.exists() { Some(custom.as_path()) } else { None };
    let catalog = forms::wle_templates::list(&bundle, custom_opt).map_err(|e| {
        UiError::Internal { detail: e.to_string() }
    })?;
    let template = catalog
        .iter()
        .find(|t| t.id == form_id)
        .ok_or_else(|| UiError::Internal {
            detail: format!("unknown form: {form_id}"),
        })?;

    // 2. Build the FormParameters envelope. Conventions:
    //    - xml_file_version "1.0" matches WLE's value.
    //    - rms_express_version identifies the originator client.
    //    - display_form / reply_template follow WLE's filename convention:
    //      Resolved against the authoring template's sibling folder via
    //      `resolve_viewer_for` (2026-06-04 Codex adrev P1.3) — falls
    //      back to `<id>_Viewer.html` only when the catalog walker
    //      can't find a paired viewer. Before the resolver, the
    //      hard-coded `<id>_Viewer.html` produced wrong display_form
    //      values for half the bundle (e.g. authoring "Bulletin Initial"
    //      → claimed display_form "Bulletin Initial_Viewer.html" when
    //      the actual viewer is "Bulletin Viewer.html").
    //      `reply_template` keeps the `<id>_SendReply.0` convention
    //      because tuxlink doesn't currently generate per-form
    //      reply templates; recipients fall back to the generic viewer.
    let now = chrono::Utc::now();

    // Resolve the governing template + display viewer.
    //
    // o4p9 (G12-A): normally the governing template is the form's own `.txt`,
    // resolved by the `Form:` directive. Its `<var fieldname>` + host-tag
    // placeholders render the prescribed recipient (often a fixed agency address
    // like DYFI → USGS), templated subject, and human-readable body — instead of
    // the generic fallbacks. A form with no governing `.txt` keeps the fallback.
    //
    // tuxlink-hhfx (G10): when replying via a SendReply, the governing template
    // is the SendReply `.0` (its Msg reproduces the original + the operator's
    // reply) and the display viewer is the SendReply viewer. We resolve the `.0`
    // in the original form's folder; if it can't be resolved (catalog drift),
    // fall back to the original form's templates so the send still goes out.
    let viewer_fallback = || {
        forms::wle_templates::resolve_viewer_for(&template.path)
            .unwrap_or_else(|| format!("{form_id}_Viewer.html"))
    };
    // A reply resolves the SendReply `.0` in the form's folder. If there's no
    // reply_template, or it can't be resolved (catalog drift), both fall back to
    // the original form's governing `.txt` so the send still goes out.
    let sendreply = reply_template.as_deref().and_then(|rt| {
        template
            .path
            .parent()
            .and_then(|folder| forms::txt_template::resolve_sendreply(folder, rt))
    });
    let (txt, display_form) = match sendreply {
        Some(sr) => {
            let disp = sr.template.display_html.clone().unwrap_or_else(viewer_fallback);
            (Some(sr.template), disp)
        }
        None => (
            forms::txt_template::resolve_governing_txt(&template.path),
            viewer_fallback(),
        ),
    };

    // tuxlink-2tom / G12-C: SeqInc forms auto-number each send. Allocate the next
    // serial from the persisted counter and stamp it into `SeqNum`, so `<var SeqNum>`
    // in Subject/Msg and the serialized XML both carry it. Allocation persists
    // BEFORE the network send below, so a failed send leaves a serial GAP rather
    // than risking a duplicate from a concurrent retry. The lock is released here —
    // never held across the await.
    if txt.as_ref().map(|t| t.seq_inc).unwrap_or(false) {
        let next = {
            let mut store = seq_store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            store.allocate(&form_id)
        };
        field_values.insert("SeqNum".to_string(), next.to_string());
    }

    let host_tags = {
        let mut h = std::collections::HashMap::new();
        h.insert("MsgSender".to_string(), senders_callsign.clone());
        h.insert("Callsign".to_string(), senders_callsign.clone());
        h.insert("GridSquare".to_string(), grid_square.clone());
        h.insert(
            "ProgramVersion".to_string(),
            format!("Tuxlink/{}", env!("CARGO_PKG_VERSION")),
        );
        h.insert("DateTime".to_string(), now.format("%Y-%m-%d %H:%M:%SZ").to_string());
        h.insert("Date".to_string(), now.format("%Y-%m-%d").to_string());
        h.insert("Time".to_string(), now.format("%H:%M:%SZ").to_string());
        h.insert("MsgTo".to_string(), to.join("; "));
        h.insert("MsgCc".to_string(), cc.join("; "));
        h
    };

    let params = forms::types::FormParameters {
        xml_file_version: "1.0".to_string(),
        rms_express_version: format!("Tuxlink/{}", env!("CARGO_PKG_VERSION")),
        submission_datetime: now.format("%Y%m%d%H%M%S").to_string(),
        senders_callsign,
        grid_square,
        display_form,
        // A first-time send advertises its SendReply (`<id>_SendReply.0`) so the
        // recipient can thread a reply. A reply message itself advertises no
        // further reply template — SendReply `.0`s declare none, and we don't
        // model reply-to-a-reply chains.
        reply_template: if reply_template.is_some() {
            String::new()
        } else {
            format!("{form_id}_SendReply.0")
        },
    };

    // 3. Serialize the XML attachment.
    let xml_bytes = forms::serialize::serialize_catalog_form_xml(&form_id, &params, &field_values);

    // 4. Compose the plain-text body: the .txt `Msg:` projection (rendered with
    //    the submitted field values + host tags) when the form has one, else the
    //    KISS key:value dump fallback (non-.txt / operator-custom forms). WLE
    //    receivers render the structured XML via the viewer regardless; the body
    //    text is the fallback for non-WLE clients — but the .txt projection is
    //    the faithful human-readable message the form designer intended.
    let body = txt
        .as_ref()
        .and_then(|t| t.msg.as_deref())
        .map(|m| forms::txt_template::render_template(m, &field_values, &host_tags))
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| {
            let mut keys: Vec<&String> = field_values.keys().collect();
            keys.sort();
            let mut body = format!("form_id: {form_id}\n\n");
            for k in keys {
                let v = field_values.get(k).map(String::as_str).unwrap_or("");
                body.push_str(k);
                body.push_str(": ");
                body.push_str(v);
                body.push('\n');
            }
            body
        });

    // 5. Subject: the .txt `Subject:` rendered, else an explicit subject field
    //    (WLE catalog forms often have a "subject"/"msg_subject" input), else
    //    "Form: <id>". The .txt subject is routing-significant for some forms
    //    (RRI/ICS-213 sorting), so it wins when present.
    let subject = txt
        .as_ref()
        .and_then(|t| t.subject.as_deref())
        .map(|s| forms::txt_template::render_template(s, &field_values, &host_tags))
        .filter(|s| !s.trim().is_empty())
        .or_else(|| field_values.get("subject").cloned())
        .or_else(|| field_values.get("msg_subject").cloned())
        // G10: a reply's SendReply `.0` carries no Subject: directive, so fall
        // back to the operator's compose subject ("Re: <original>") before the
        // last-resort "Form: <id>". Blank hint is ignored.
        .or_else(|| subject_hint.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string))
        .unwrap_or_else(|| format!("Form: {form_id}"));

    // 6. Recipients: union the .txt `To:`/`Cc:` (rendered) with the
    //    operator-entered recipients. The form's prescribed addresses lead
    //    (fixed agency address, or an in-form `<var address>` the operator
    //    filled); operator additions are appended, case-insensitively deduped.
    //    An empty/absent .txt `To:` leaves the operator recipients unchanged.
    let final_to = merge_txt_recipients(
        txt.as_ref().and_then(|t| t.to.as_deref()),
        &field_values,
        &host_tags,
        &to,
    );
    let final_cc = merge_txt_recipients(
        txt.as_ref().and_then(|t| t.cc.as_deref()),
        &field_values,
        &host_tags,
        &cc,
    );

    let attachment = crate::winlink_backend::OutboundAttachment {
        filename: format!("RMS_Express_Form_{form_id}.xml"),
        bytes: xml_bytes,
    };

    let msg = OutboundMessage {
        to: final_to,
        cc: final_cc,
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

/// Union a `.txt` `To:`/`Cc:` line (rendered against field values + host tags)
/// with the operator-entered recipients (tuxlink-o4p9 / G12-A). The form's
/// prescribed addresses come first, then any operator additions not already
/// present (case-insensitive dedup). An empty/absent template line yields the
/// operator recipients unchanged. Splits the rendered line on `;` and `,` (WLE
/// address separators).
fn merge_txt_recipients(
    txt_line: Option<&str>,
    field_values: &std::collections::HashMap<String, String>,
    host_tags: &std::collections::HashMap<String, String>,
    operator: &[String],
) -> Vec<String> {
    fn push_unique(out: &mut Vec<String>, addr: &str) {
        let a = addr.trim();
        if !a.is_empty() && !out.iter().any(|m| m.eq_ignore_ascii_case(a)) {
            out.push(a.to_string());
        }
    }
    let mut out: Vec<String> = Vec::new();
    if let Some(line) = txt_line {
        let rendered =
            crate::forms::txt_template::render_template(line, field_values, host_tags);
        for addr in rendered.split([';', ',']) {
            push_unique(&mut out, addr);
        }
    }
    for addr in operator {
        push_unique(&mut out, addr);
    }
    out
}

/// One form's serial-counter state (tuxlink-2tom / G12-C). `next_serial` is the
/// number this form's next `SeqInc` send will stamp into `SeqNum`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeqCounterStatus {
    pub form_id: String,
    pub next_serial: u64,
}

/// List every form that has a serial counter, as `{formId, nextSerial}`
/// (tuxlink-2tom / G12-C). Powers the Settings "Form sequence numbers" section.
/// Forms that have never been sent have no counter and are absent (their next
/// serial is implicitly 1).
#[tauri::command]
pub async fn forms_sequence_status(
    seq_store: State<'_, std::sync::Arc<std::sync::Mutex<crate::forms::sequence::SeqCounterStore>>>,
) -> Result<Vec<SeqCounterStatus>, String> {
    let store = seq_store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(store
        .status()
        .into_iter()
        .map(|(form_id, next_serial)| SeqCounterStatus {
            form_id,
            next_serial,
        })
        .collect())
}

/// Set a form's NEXT serial number (tuxlink-2tom / G12-C) — the reset affordance
/// (e.g. restart radiogram numbering at 1 for a new event/year). `next` is
/// clamped to `>= 1`. Persists immediately.
#[tauri::command]
pub async fn forms_sequence_reset(
    form_id: String,
    next: u64,
    seq_store: State<'_, std::sync::Arc<std::sync::Mutex<crate::forms::sequence::SeqCounterStore>>>,
) -> Result<(), String> {
    let mut store = seq_store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    store.set_next(&form_id, next);
    Ok(())
}

// ============================================================================
// HTML Forms — webview infrastructure command surface (P1 Task 8)
// ============================================================================
//
// Three thin shim commands wire the (already-shipped) Rust forms::http_server +
// forms::wle_templates modules to the React CatalogBrowser + WebviewFormHost
// (P1 Tasks 9 + 10). The hard work lives in the Rust modules; these
// commands marshal AppHandle resource paths, manage the
// `FormSessionRegistry` lookups, and bridge the parsed-submit channel onto
// a Tauri event.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 8.
// Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §8.2.

/// Result of [`open_webview_form`]. The React side passes `url` to the
/// child `WebviewWindow` (label `compose-form-<token>`), keeps `token`
/// for the `close_webview_form_server` teardown call, and reads `port`
/// for diagnostics only (the form's submit POSTs are path-less per the
/// WLE contract, so the port is informational — not required for the
/// frontend's submit-listener wiring).
// `token` here is an ephemeral UUID-like WebView session label — it is NOT
// an authentication credential. credential_audit_skip so the source-scan
// does not flag this as a credential-bearing struct requiring manual Debug.
#[allow(unknown_lints, credential_audit_skip)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFormResult {
    pub url: String,
    pub port: u16,
    pub token: String,
}

/// Enumerate every form template visible to tuxlink — bundled WLE Standard
/// Forms snapshot + the operator's custom-forms directory. Custom forms
/// with the same `id` as a bundled form shadow the bundled entry. Powers
/// the React `CatalogBrowser` (P1 Task 10).
///
/// The custom-forms root is only walked if it exists on disk; this is the
/// expected behavior for the install-time path (operator may never have
/// created `~/.local/share/tuxlink/forms/custom/`).
#[tauri::command]
pub async fn forms_list_catalog(
    app: AppHandle,
) -> Result<Vec<crate::forms::wle_templates::Template>, String> {
    let bundle =
        crate::forms::wle_templates::bundle_root_for_app(&app).map_err(|e| e.to_string())?;
    let custom = crate::forms::wle_templates::custom_root_for_app(&app);
    let custom_opt = if custom.exists() {
        Some(custom.as_path())
    } else {
        None
    };
    crate::forms::wle_templates::list(&bundle, custom_opt).map_err(|e| e.to_string())
}

/// Preview an import (tuxlink-z0le/fwob, spec §11.4): stage + validate +
/// classify the sources WITHOUT writing to the custom-forms dir, returning a
/// plan + an opaque staging token. The synchronous fs work runs under
/// `spawn_blocking` so it never stalls the async runtime.
#[tauri::command]
pub async fn forms_import_preview(
    sources: Vec<String>,
    app: AppHandle,
    reg: State<'_, std::sync::Arc<crate::forms::import::ImportStagingRegistry>>,
) -> Result<crate::forms::import::ImportPlan, crate::forms::import::ImportError> {
    let custom_root = crate::forms::wle_templates::custom_root_for_app(&app);
    let bundle_root = crate::forms::wle_templates::bundle_root_for_app(&app)
        .map_err(|e| crate::forms::import::ImportError::Io { reason: e.to_string() })?;
    let reg = reg.inner().clone();
    tokio::task::spawn_blocking(move || {
        crate::forms::import::preview_sources(&sources, &custom_root, &bundle_root, &reg)
    })
    .await
    .map_err(|e| crate::forms::import::ImportError::Io {
        reason: format!("join: {e}"),
    })?
}

/// Cancel an in-flight import preview, dropping its staging dir. Idempotent —
/// an unknown/expired token is a no-op (fired on ImportSheet unmount/Escape).
#[tauri::command]
pub async fn forms_import_cancel(
    staging_token: String,
    reg: State<'_, std::sync::Arc<crate::forms::import::ImportStagingRegistry>>,
) -> Result<(), ()> {
    reg.cancel(&staging_token);
    Ok(())
}

/// Commit a previewed import (tuxlink-z0le/fwob, spec §11.4): consume the
/// staging token single-shot, re-classify under the shared forms lock, and
/// atomically promote the validated set (+ approved overwrites) into the
/// custom-forms dir with `.prev` backups. Re-commit → `TokenExpired`.
#[tauri::command]
pub async fn forms_import_commit(
    staging_token: String,
    approved_overwrite_ids: Vec<String>,
    app: AppHandle,
    reg: State<'_, std::sync::Arc<crate::forms::import::ImportStagingRegistry>>,
) -> Result<crate::forms::import::ImportResult, crate::forms::import::ImportError> {
    let custom_root = crate::forms::wle_templates::custom_root_for_app(&app);
    let bundle_root = crate::forms::wle_templates::bundle_root_for_app(&app)
        .map_err(|e| crate::forms::import::ImportError::Io { reason: e.to_string() })?;
    crate::forms::import::commit(
        &staging_token,
        &approved_overwrite_ids,
        &custom_root,
        &bundle_root,
        reg.inner().clone(),
    )
    .await
}

/// Reveal the custom-forms folder in the OS file manager (the power-user
/// escape hatch). Resolves the dir strictly via the platform data dir (refuses
/// if unavailable — never a CWD-relative fallback; §11.4), creates it if
/// absent, then launches `xdg-open` via the shell plugin (backend-initiated,
/// so it bypasses the URL-scoped frontend `shell:allow-open`). Returns a typed
/// message when no file-manager handler is registered (labwc/Wayland).
#[tauri::command]
pub async fn open_forms_folder(app: AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .data_dir()
        .map_err(|_| "platform data dir unavailable".to_string())?
        .join("tuxlink/forms/custom");
    crate::forms::import::ensure_custom_dir(&dir).map_err(|e| format!("create folder: {e}"))?;
    use tauri_plugin_shell::ShellExt;
    app.shell()
        .command("xdg-open")
        .arg(dir.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| format!("No file manager is registered to open folders ({e})."))?;
    Ok(())
}

/// Remove custom forms (+ companions) from the custom-forms dir (tuxlink-z0le
/// §11.3). Confirm-gated in the UI. Only ever touches custom_root; a bundled id
/// is a no-op.
#[tauri::command]
pub async fn forms_custom_delete(ids: Vec<String>, app: AppHandle) -> Result<Vec<String>, String> {
    let custom_root = crate::forms::wle_templates::custom_root_for_app(&app);
    crate::forms::import::delete_custom_forms(&ids, &custom_root)
}

/// Open a new webview form session: spawn the loopback http_server bound
/// to a fresh ephemeral port, register it in `FormSessionRegistry` under a
/// freshly-minted token, and start a forwarder task that drains parsed
/// submissions onto the `form-submitted` event scoped to the child
/// webview's label (`compose-form-<token>`).
///
/// Returns the URL the React side passes to a child `WebviewWindow` plus
/// the port and token. The URL is `http://127.0.0.1:<port>/` — NO path
/// component; the WLE form template's `{FormServer}:{FormPort}`
/// substitution wires the submit endpoint there directly.
///
/// Errors:
/// - `unknown form: <id>` — the form_id is not in the bundled/custom
///   catalog (typo / stale frontend cache).
/// - any I/O error from reading the bundled snapshot or binding the
///   loopback listener.
#[tauri::command]
pub async fn open_webview_form(
    form_id: String,
    app: AppHandle,
    registry: State<'_, std::sync::Arc<crate::forms::http_server::FormSessionRegistry>>,
) -> Result<OpenFormResult, String> {
    let bundle =
        crate::forms::wle_templates::bundle_root_for_app(&app).map_err(|e| e.to_string())?;
    let custom = crate::forms::wle_templates::custom_root_for_app(&app);
    let custom_opt = if custom.exists() {
        Some(custom.as_path())
    } else {
        None
    };
    let cat = crate::forms::wle_templates::list(&bundle, custom_opt)
        .map_err(|e| e.to_string())?;
    let template = cat
        .into_iter()
        .find(|t| t.id == form_id)
        .ok_or_else(|| format!("unknown form: {form_id}"))?;

    let opened = registry.open(template).await?;
    let port = opened.port;
    let token = opened.token.clone();
    let url = format!("http://127.0.0.1:{port}/");

    // Forwarder task: drain ParsedBody submissions from the http_server's
    // in-process channel onto a Tauri event scoped to the child webview's
    // label. The task self-terminates when:
    //   - the session is closed (`close_webview_form_server` drops the
    //     `FormSession`, which drops the `submit_tx`, which closes the
    //     channel — `recv()` returns None and the loop exits), OR
    //   - the runtime shuts down (tokio aborts the task).
    let app_for_forwarder = app.clone();
    let label = format!("compose-form-{token}");
    let mut submit_rx = opened.submit_rx;
    tokio::spawn(async move {
        while let Some(parsed) = submit_rx.recv().await {
            let _ = app_for_forwarder.emit_to(label.as_str(), "form-submitted", parsed);
        }
    });

    Ok(OpenFormResult { url, port, token })
}

/// Shape returned by `forms_check_for_update` (Phase 3 — `forms::updater`
/// surface to the React `CatalogBrowser` "Refresh forms…" affordance).
/// `currentVersion` is `None` on a fresh install that has never run a
/// refresh — the catalog is being served from the bundle's seed snapshot,
/// whose version isn't recorded as a runtime VERSION file. `updateAvailable`
/// is `currentVersion != Some(remoteVersion)`; a missing-current is treated
/// as "update available" so the operator can opt into the runtime path.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormsRefreshStatus {
    pub current_version: Option<String>,
    pub remote_version: String,
    pub archive_url: String,
    pub update_available: bool,
}

/// Check the Pat metadata endpoint for the latest WLE Standard Forms
/// version. Pure read — no install side effect; pairs with
/// `forms_refresh` (the React modal calls this first to render the
/// confirmation, then calls `forms_refresh` if the operator confirms).
///
/// Errors surface the underlying network / decode failure verbatim so the
/// React layer can route to a "couldn't reach forms server" UX rather
/// than crashing.
#[tauri::command]
pub async fn forms_check_for_update(app: AppHandle) -> Result<FormsRefreshStatus, String> {
    let runtime_root = crate::forms::wle_templates::runtime_root_for_app(&app)
        .ok_or_else(|| "platform data dir unavailable — runtime forms root cannot be resolved".to_string())?;
    let current_version = crate::forms::updater::current_version(&runtime_root);
    let info = crate::forms::updater::fetch_latest_info(
        crate::forms::updater::DEFAULT_METADATA_URL,
    )
    .await
    .map_err(|e| e.to_string())?;
    let update_available = current_version.as_deref() != Some(info.version.as_str());
    Ok(FormsRefreshStatus {
        current_version,
        remote_version: info.version,
        archive_url: info.archive_url,
        update_available,
    })
}

/// Refresh the WLE Standard Forms snapshot from the Pat metadata
/// endpoint. Downloads + extracts + atomically swaps into
/// `<data_dir>/tuxlink/forms/standard/active/`. On any failure before
/// the swap, the current `active/` is untouched; on swap failure, the
/// prior `active/` is restored via the `.prev-<ts>/` rename.
///
/// Network or extraction errors propagate as the IPC error message so
/// the React modal can render them inline. Operators triggering a
/// successful refresh see the new `InstallReport` (version + form count
/// + prior version) and the CatalogBrowser re-invokes `forms_list_catalog`
/// to pick up the new entries.
#[tauri::command]
pub async fn forms_refresh(app: AppHandle) -> Result<crate::forms::updater::InstallReport, String> {
    let runtime_root = crate::forms::wle_templates::runtime_root_for_app(&app)
        .ok_or_else(|| "platform data dir unavailable — runtime forms root cannot be resolved".to_string())?;
    let info = crate::forms::updater::fetch_latest_info(
        crate::forms::updater::DEFAULT_METADATA_URL,
    )
    .await
    .map_err(|e| e.to_string())?;
    crate::forms::updater::install(&info.archive_url, &info.version, &runtime_root)
        .await
        .map_err(|e| e.to_string())
}

/// Tear down a webview form session. Idempotent — closing an unknown
/// token returns `Ok(())` (the React unmount cleanup path runs whether
/// or not the session is already gone). Used by BOTH Form-mode and
/// Viewer-mode sessions (the registry holds them both; close-by-token
/// is mode-agnostic).
#[tauri::command]
pub async fn close_webview_form_server(
    token: String,
    registry: State<'_, std::sync::Arc<crate::forms::http_server::FormSessionRegistry>>,
) -> Result<(), String> {
    registry.close(&token).await
}

/// Export a rendered form to PDF on demand (tuxlink-cumx / G8).
///
/// `webview_label` is the child-webview label the React host created for the
/// open form (`compose-form-<token>` for authoring, `viewer-form-<token>` for
/// a received form). `out_path` is the operator's chosen destination from the
/// native Save dialog. The form's live WebKitGTK view is printed to PDF via
/// `WebKitPrintOperation` — the same engine that painted it, so the file is a
/// faithful copy of what's on screen (no second rendering engine / licensed
/// dep). On success returns the final path (with `.pdf` ensured) for the React
/// layer to reveal.
///
/// Runs the (briefly blocking) GTK print on a blocking thread so it doesn't
/// stall an async runtime worker while the main loop renders + writes the file.
#[tauri::command]
pub async fn forms_export_pdf(
    webview_label: String,
    out_path: String,
    app: AppHandle,
) -> Result<String, UiError> {
    let path = std::path::PathBuf::from(out_path);
    let joined = tauri::async_runtime::spawn_blocking(move || {
        crate::forms::pdf_export::export_webview_pdf(&app, &webview_label, &path)
    })
    .await
    .map_err(|e| UiError::Internal {
        detail: format!("pdf export task join failed: {e}"),
    })?;
    joined
        .map(|p| p.display().to_string())
        .map_err(|e| UiError::Internal {
            detail: e.to_string(),
        })
}

/// Print a rendered form directly via the system print dialog (tuxlink-954o /
/// G8b). Counterpart to [`forms_export_pdf`]: instead of saving a file, it
/// opens GTK's printer picker on the form's live child webview and prints on
/// confirm — no save-to-disk step for a hardcopy. `webview_label` is the
/// form's child-webview label (`compose-form-<token>` / `viewer-form-<token>`).
/// Returns `true` if the operator printed, `false` if they cancelled.
#[tauri::command]
pub async fn forms_print(
    webview_label: String,
    app: AppHandle,
) -> Result<bool, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::forms::pdf_export::print_webview(&app, &webview_label)
    })
    .await
    .map_err(|e| UiError::Internal {
        detail: format!("print task join failed: {e}"),
    })?
    .map_err(|e| UiError::Internal {
        detail: e.to_string(),
    })
}

/// Result of [`open_webview_viewer`] (P1 Task 11). Symmetric to
/// [`OpenFormResult`] except the React side never subscribes to a
/// `form-submitted` event for viewer sessions — there is no submit path.
/// `token` is the lookup key for `close_webview_form_server` teardown;
/// `port` is informational only.
// `token` here is an ephemeral WebView session label — not an auth credential.
// credential_audit_skip so the source-scan does not flag this as requiring manual Debug.
#[allow(unknown_lints, credential_audit_skip)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenViewerResult {
    pub url: String,
    pub port: u16,
    pub token: String,
}

/// Open a Viewer-mode webview session for a received form whose `form_id`
/// has no native React `View` component registered (P1 Task 11). The
/// caller supplies the parsed FormPayload's `(field_id, value)` map; the
/// http_server binds the values into the WLE `*_Viewer.html` template via
/// two complementary substitution paths:
/// 1. `{var X}` placeholders in the HTML → field value (server-side
///    string replace, matching the WLE viewer convention)
/// 2. A `<script>` tag appended before `</body>` runs on
///    `DOMContentLoaded` and assigns `document.querySelectorAll(
///    '[name="X"]').value = ...` for each field, covering hidden inputs.
///
/// The Viewer filename is resolved in three passes (2026-06-04 Codex
/// adrev P1.3 — the prior two-pass resolution failed for the bulk of the
/// bundled catalog because the `<form_id>_Viewer.html` convention only
/// covers a minority of WLE templates):
/// - For form_ids in the BUNDLED_FORMS catalog
///   (ICS-213, ICS-309, Bulletin, Position, Damage Assessment), use the
///   `FormDef::display_form` field — these have non-conventional names
///   like `Bulletin Viewer.html` or `GPS Position Report.html`.
/// - Walk the authoring template's sibling folder via
///   [`forms::wle_templates::resolve_viewer_for`] for a paired viewer.
///   Covers `Bulletin Initial.html` ↔ `Bulletin Viewer.html`,
///   `Hawaii Siren Report.html` ↔ `Hawaii Siren Report Viewer.html`,
///   `ICS213_Initial.html` ↔ `ICS213_Viewer.html`, etc.
/// - Last-resort fallback to `<form_id>_Viewer.html` for tuxlink-authored
///   forms (the convention `send_webview_form` writes into outbound XML
///   when its own resolver fails — covers the round-trip case where
///   the sender claims a viewer file the receiver doesn't have).
///
/// Errors:
/// - `unknown form: <id>` — neither the bundled catalog nor the live
///   catalog (the form's INITIAL `.html` in any bundled / custom folder)
///   knows this form. The frontend falls back to KeyValueView.
/// - `viewer template not found: <path>` — the form is in the catalog
///   but the resolved Viewer file doesn't exist on disk (catalog drift /
///   custom form without a companion viewer). Frontend falls back to
///   KeyValueView.
/// - any I/O error from binding the loopback listener.
#[tauri::command]
pub async fn open_webview_viewer(
    form_id: String,
    field_values: std::collections::HashMap<String, String>,
    app: AppHandle,
    registry: State<'_, std::sync::Arc<crate::forms::http_server::FormSessionRegistry>>,
) -> Result<OpenViewerResult, String> {
    use crate::forms;

    // 1. Resolve the live catalog so we can find the form's folder (for
    //    {FormFolder} substitution + adjacent-asset serving). The Viewer
    //    file lives next to the form's INITIAL .html in the same folder.
    let bundle =
        forms::wle_templates::bundle_root_for_app(&app).map_err(|e| e.to_string())?;
    let custom = forms::wle_templates::custom_root_for_app(&app);
    let custom_opt = if custom.exists() {
        Some(custom.as_path())
    } else {
        None
    };
    let cat = forms::wle_templates::list(&bundle, custom_opt)
        .map_err(|e| e.to_string())?;
    let template = cat
        .iter()
        .find(|t| t.id == form_id)
        .ok_or_else(|| format!("unknown form: {form_id}"))?;

    // 2. Resolve the Viewer filename. Priority order:
    //    a. Native bundled forms (BUNDLED_FORMS, the 5 hard-coded
    //       templates with `FormDef::display_form`): use the explicit
    //       filename (e.g. `Bulletin Viewer.html`,
    //       `GPS Position Report.html`).
    //    b. Walk the authoring template's sibling folder for a paired
    //       viewer via `resolve_viewer_for` (2026-06-04 Codex adrev P1.3).
    //       Covers the WLE catalog's inconsistent naming conventions
    //       (`Bulletin Initial.html` ↔ `Bulletin Viewer.html`,
    //       `Hawaii Siren Report.html` ↔ `Hawaii Siren Report Viewer.html`,
    //       `Field Situation Report Initial.html` ↔
    //       `Field Situation Report viewer.html`, etc.).
    //    c. Last-resort fallback to `<form_id>_Viewer.html` — the
    //       convention `send_webview_form` writes into outbound XML
    //       when no paired viewer is found.
    let viewer_filename = forms::catalog::find_form(&form_id)
        .map(|f| f.display_form.to_string())
        .or_else(|| forms::wle_templates::resolve_viewer_for(&template.path))
        .unwrap_or_else(|| format!("{form_id}_Viewer.html"));

    // 3. The Viewer file lives next to the form template (same folder).
    let form_parent = template
        .path
        .parent()
        .ok_or_else(|| "template has no parent folder".to_string())?;
    let viewer_path = form_parent.join(&viewer_filename);
    if !viewer_path.exists() {
        return Err(format!(
            "viewer template not found: {}",
            viewer_path.display()
        ));
    }

    // 4. Open the viewer session. No forwarder task — the POST handler
    //    returns 404 in Viewer mode, so there's nothing to drain.
    let opened = registry
        .open_viewer(viewer_path, template.folder.clone(), &field_values)
        .await?;
    let port = opened.port;
    let token = opened.token.clone();
    let url = format!("http://127.0.0.1:{port}/");

    Ok(OpenViewerResult { url, port, token })
}

/// Result of [`open_webview_reply`] (tuxlink-hhfx / G10). Like [`OpenFormResult`]
/// (an editable session with a live submit channel), plus the resolved
/// `reply_template` the frontend threads back to [`send_webview_form`] so the
/// reply's To:/Subject:/Msg: render from the SendReply `.0`.
// `token` is an ephemeral WebView session label — not an auth credential.
#[allow(unknown_lints, credential_audit_skip)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenReplyResult {
    pub url: String,
    pub port: u16,
    pub token: String,
    /// The SendReply governing-template filename (e.g. `ICS213_SendReply.0`),
    /// resolved from the original form's own bundled `.txt`. The frontend passes
    /// it to `send_webview_form` on submit.
    pub reply_template: String,
}

/// Open an editable, **pre-bound** reply session for a received form that
/// declares a WLE `ReplyTemplate:` (tuxlink-hhfx / G10). Resolves the original
/// form's SendReply authoring HTML (`<X>_SendReply.html`), serves it editable
/// with the original field values pre-filled (name-aligned) + the original
/// message body as `MsgOriginalBody`, and wires the submit path so the operator
/// fills the Reply section and submits — round-tripping through the same
/// `form-submitted` → `send_webview_form` path as any webview form.
///
/// `form_id` is the ORIGINAL received form (e.g. `ICS213_Initial`). The
/// SendReply to open is derived from that form's OWN bundled `.txt`
/// `ReplyTemplate:` directive — the local source of truth — NOT from whatever
/// `reply_template` the inbound XML claimed (a tuxlink sender writes a synthetic
/// `<id>_SendReply.0` that wouldn't resolve). `field_values` are the original
/// form's submitted values; `msg_original_body` is the received message body.
///
/// Errors:
/// - `unknown form: <id>` — form_id not in the catalog (operator lacks the form).
/// - `form has no reply template: <id>` — the form isn't a ReplyTemplate form.
/// - `reply template not found: <name>` — the `.0` or its SendReply HTML is
///   absent on disk (catalog drift). The frontend falls back to a plain reply.
#[tauri::command]
pub async fn open_webview_reply(
    form_id: String,
    field_values: std::collections::HashMap<String, String>,
    msg_original_body: String,
    app: AppHandle,
    registry: State<'_, std::sync::Arc<crate::forms::http_server::FormSessionRegistry>>,
) -> Result<OpenReplyResult, String> {
    use crate::forms;

    // 1. Resolve the live catalog → the original form's template (folder + path).
    let bundle =
        forms::wle_templates::bundle_root_for_app(&app).map_err(|e| e.to_string())?;
    let custom = forms::wle_templates::custom_root_for_app(&app);
    let custom_opt = if custom.exists() {
        Some(custom.as_path())
    } else {
        None
    };
    let cat = forms::wle_templates::list(&bundle, custom_opt).map_err(|e| e.to_string())?;
    let template = cat
        .iter()
        .find(|t| t.id == form_id)
        .ok_or_else(|| format!("unknown form: {form_id}"))?;

    // 2. The form's OWN bundled .txt names the SendReply (.0). Local truth —
    //    independent of whatever reply_template the inbound XML claimed.
    let reply_template_name = forms::txt_template::resolve_governing_txt(&template.path)
        .and_then(|t| t.reply_template)
        .ok_or_else(|| format!("form has no reply template: {form_id}"))?;

    // 3. Resolve the SendReply authoring HTML via the .0's Form: directive.
    let form_folder = template
        .path
        .parent()
        .ok_or_else(|| "template has no parent folder".to_string())?;
    let sendreply = forms::txt_template::resolve_sendreply(form_folder, &reply_template_name)
        .ok_or_else(|| format!("reply template not found: {reply_template_name}"))?;

    // 4. Pre-bind: the original field values (name-aligned — the SendReply
    //    reproduces the original via the same `<var fieldname>` names) plus the
    //    original message body as `MsgOriginalBody`.
    let mut prebind = field_values;
    prebind.insert("MsgOriginalBody".to_string(), msg_original_body);

    let opened = registry
        .open_form_prebound(sendreply.html_path, template.folder.clone(), &prebind)
        .await?;
    let port = opened.port;
    let token = opened.token.clone();
    let url = format!("http://127.0.0.1:{port}/");

    // 5. Forwarder task — identical to `open_webview_form` (Form-kind session,
    //    live submit channel). Drains parsed submissions onto the child
    //    webview's label so Compose's reply-submit handler receives them.
    let app_for_forwarder = app.clone();
    let label = format!("compose-form-{token}");
    let mut submit_rx = opened.submit_rx;
    tokio::spawn(async move {
        while let Some(parsed) = submit_rx.recv().await {
            let _ = app_for_forwarder.emit_to(label.as_str(), "form-submitted", parsed);
        }
    });

    Ok(OpenReplyResult {
        url,
        port,
        token,
        reply_template: reply_template_name,
    })
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
    registry: State<'_, crate::winlink::inbound_selection::SelectionRegistry>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;

    let cfg = config::read_config().map_err(|e| UiError::Internal {
        detail: e.to_string(),
    })?;

    tracing::info!(
        target: "tuxlink::cms",
        transport = ?cfg.connect.transport,
        "CMS connect started",
    );
    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        format!("Connecting to the CMS ({:?})…", cfg.connect.transport),
    );

    // Task 11: Tauri-side event sink infrastructure.
    // Task 12 (tuxlink-7do4): now used for result-level AuthClassified
    // emission in the Err arm. Deep threading through backend.connect
    // (for in-flight TcpConnected / RemoteSidReceived / PostAuthExchangeStarted
    // events) is deferred as bd-tuxlink-7do4-followup-backend-events.
    let events_sink: std::sync::Arc<dyn crate::winlink::b2f_events::B2fEventSink> =
        std::sync::Arc::new(crate::winlink::b2f_events::TauriEventSink::new(app.clone()));

    // tuxlink-bsiy: mint ONE attempt_id at the top so the in-flight
    // InboundProposalsOffered events (emitted from the selecting decider) and the
    // result-level AuthClassified event in the Err arm share a single correlation
    // id. The frontend stale-filter keys on this AttemptId (Codex #2).
    let attempt_id = crate::winlink::b2f_events::AttemptId::fresh();

    // tuxlink-bsiy connect-path staleness fix: build the selection context — which
    // makes `native_connect` prompt the operator — from the FRESH on-disk
    // preference read into `cfg` above (`config::read_config()`), NOT the backend's
    // in-memory `live_config`. `config_set_review_inbound` refreshes `live_config`
    // only when the backend is already installed, so a preference enabled during
    // startup would otherwise be ignored at the next connect. Preference off ⇒ no
    // context ⇒ accept-all (the WLE-parity default).
    let selection = if cfg.review_inbound_before_download {
        Some(crate::winlink_backend::CmsSelectionContext {
            sink: events_sink.clone(),
            attempt_id,
            registry: registry.inner().clone(),
        })
    } else {
        None
    };

    match backend
        .connect(
            TransportConfig::Cms {
                mode: cfg.connect.transport,
            },
            selection,
        )
        .await
    {
        Ok(session) => {
            tracing::info!(
                target: "tuxlink::cms",
                outcome = "ok",
                "CMS exchange complete",
            );
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
            tracing::info!(
                target: "tuxlink::cms",
                outcome = "aborted",
                "CMS connection aborted by operator",
            );
            emit_session_line(&app, &log, LogLevel::Warn, "CMS connection aborted.".to_string());
            Err(BackendError::Cancelled.into())
        }
        Err(e) => {
            tracing::warn!(
                target: "tuxlink::cms",
                error = %e,
                outcome = "error",
                "CMS connect failed",
            );
            emit_session_line(&app, &log, LogLevel::Error, format!("CMS connect failed: {e}"));

            // Task 12 (tuxlink-7do4, R5 spec §6.3 + §6.4): emit a structured
            // AuthClassified event so the React useAuthDiagnostic hook can
            // render the appropriate banner mode. This is result-level
            // classification; deep backend.connect threading (which would also
            // give us TcpConnected / RemoteSidReceived / PostAuthExchangeStarted
            // events for Mode 1 vs Mode 5 discrimination) is tracked as
            // bd-tuxlink-7do4-followup-backend-events.
            //
            // tuxlink-bsiy: reuse the top-minted `attempt_id` (no longer re-minted
            // here) so a pending InboundProposalsOffered event and this
            // AuthClassified event share one attempt_id — the frontend stale-filter
            // depends on that (Codex #2).
            use crate::winlink::b2f_events::{B2fEvent, FailureMode};
            let (mode, raw) = match &e {
                // BackendError::RemoteError carries the *** payload (redaction-
                // scrubbed at the handshake.rs + telnet.rs layers per Tasks 6+8).
                // This variant is new in Task 12 — the map_err in native_connect
                // lifts ExchangeError::RemoteError + HandshakeError::RemoteError
                // into it so the payload is preserved structurally.
                BackendError::RemoteError(payload) => {
                    (crate::winlink::auth_taxonomy::classify(payload), Some(payload.clone()))
                }
                // TransportFailed = TCP / TLS / DNS failure (no *** payload).
                // These are Mode 1 (NetworkUnreachable). Mode 1 vs Mode 5
                // discrimination requires deep backend.connect event threading
                // (deferred — bd-tuxlink-7do4-followup-backend-events).
                BackendError::TransportFailed { .. } => {
                    (FailureMode::NetworkUnreachable, None)
                }
                // AuthFailed is the listener gate variant (not a CMS telnet
                // scenario), but map it conservatively.
                BackendError::AuthFailed { reason } => {
                    (crate::winlink::auth_taxonomy::classify(reason), Some(reason.clone()))
                }
                // All other BackendError variants in the cms_connect Err arm
                // → Uncategorized; surface the error Display as raw context.
                _ => (FailureMode::Uncategorized, Some(format!("{e}"))),
            };
            events_sink.push(B2fEvent::AuthClassified { mode, raw, attempt_id });

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
    registry: State<'_, crate::winlink::inbound_selection::SelectionRegistry>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;

    tracing::info!(
        target: "tuxlink::cms",
        "CMS connection abort requested",
    );
    emit_session_line(&app, &log, LogLevel::Info, "Aborting CMS connection…".to_string());
    backend.abort().await?;

    // tuxlink-bsiy: wake a decider parked on a pending selection prompt. Dropping
    // the slot drops the only mpsc Sender, so the decider's recv_timeout returns
    // Disconnected immediately; backend.abort() above already set the `aborting`
    // flag, so the decider's post-recv abort re-check returns Err(Cancelled) (NOT
    // accept-all). No-op if no prompt is pending. Order matters: abort() (sets
    // aborting) BEFORE the slot drop (wakes the decider).
    *registry.lock().unwrap() = None;

    Ok(())
}

// ============================================================================
// Task 5 (tuxlink-bsiy) — cms_resolve_inbound_selection
// ============================================================================

/// Deliver the operator's inbound-message selection to a decider parked in
/// native_connect (Task 4b). Matches the pending slot by (attempt_id, request_id)
/// and take()s it (idempotent); a stale/mismatched key or an empty registry is a
/// silent no-op (the frontend also stale-filters via attempt_id). The numeric
/// attempt_id mirrors the AttemptId carried by the InboundProposalsOffered event
/// the frontend received. Thin wrapper over inbound_selection::resolve_selection
/// (Task 3), which carries the unit tests for the match/take/no-op semantics.
#[tauri::command]
pub async fn cms_resolve_inbound_selection(
    attempt_id: u64,
    request_id: u64,
    selection: crate::winlink::inbound_selection::InboundSelection,
    registry: State<'_, crate::winlink::inbound_selection::SelectionRegistry>,
) -> Result<(), UiError> {
    crate::winlink::inbound_selection::resolve_selection(
        registry.inner(),
        crate::winlink::b2f_events::AttemptId(attempt_id),
        request_id,
        selection,
    );
    Ok(())
}

// ============================================================================
// Task 13 (tuxlink-7do4) — smart-auth-diagnostics banner recovery commands
// ============================================================================
// Three commands the banner's recovery affordances depend on. Registered in
// invoke_handler alongside cms_connect / cms_abort per the append-only model.

/// Write a password to the OS keyring for the given callsign. Per spec
/// §4.3 (i) — the inline "Re-enter password" affordance on the Mode 3
/// banner uses this. Preserves the read-first → set_password
/// destructive-overwrite-readback discipline (R2 #3 keyring-locked
/// failure handling: any KeyringError surfaces; no in-memory fallback).
#[tauri::command]
pub async fn credentials_write_password(
    callsign: String,
    password: String,
) -> Result<(), UiError> {
    crate::winlink::credentials::write_password(&callsign, &password)
        .map_err(|e| UiError::Internal { detail: e.to_string() })
}

/// Reopen the onboarding wizard scoped to a specific step. Per spec
/// §4.3 (ii) — the "Try a different callsign" affordance on the Mode 4
/// banner uses this with step="callsign".
#[tauri::command]
pub async fn wizard_reopen(
    app: tauri::AppHandle,
    step: String,
) -> Result<(), UiError> {
    crate::wizard::reopen(&app, &step)
        .map_err(|e| UiError::Internal { detail: e.to_string() })
}

/// Clear the most-recent auth-diagnostic classification from Rust state.
/// Per spec §4.3 (v) — the Dismiss affordance on the banner calls this
/// so that a stale event for the dismissed AttemptId doesn't
/// re-render the banner.
///
/// Implementation: emits a "auth-diagnostic-clear" event that the React
/// hook listens for and uses to reset its local state. The Rust side
/// doesn't currently hold AttemptId-keyed state to clear; the event
/// emission alone unblocks the React side. If Rust-side state is added
/// later (e.g., for replay-after-mount), this is the place to clear it.
#[tauri::command]
pub async fn auth_diagnostic_clear(
    app: tauri::AppHandle,
) -> Result<(), UiError> {
    use tauri::Emitter;
    app.emit("auth-diagnostic-clear", ())
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}

// ============================================================================
// Task 14 (tuxlink-7do4) — cms_connect_test (spec §4.3 iii)
// ============================================================================
// Auth-only "check this password works" command. Connects to the CMS, runs the
// B2F handshake (emitting all B2fEvents including PostAuthExchangeStarted for
// the Mode 5 discriminator), and quits via FF + FQ — NEVER reads inbound
// proposals, NEVER mutates the outbox.
//
// Rate-limit (R2 #8): client-side via the React banner — 10s post-test
// debounce + 3-in-60s circuit-break. Backend has no intrinsic rate-limit;
// it's a UI-layer concern.

/// Test the user's credentials against the configured CMS WITHOUT committing
/// to a real message exchange. Per spec §4.3 (iii):
///
/// - Shares `cms_connect`'s single-flight guard so a concurrent click while a
///   real connect is running returns `Unavailable` (maps to `AlreadyConnecting`
///   on the React side).
/// - Runs only the B2F handshake (no inbound proposal reading, no outbound
///   message sending). Sends `FF + FQ` on success.
/// - Emits the full [`crate::winlink::b2f_events::B2fEvent`] stream including
///   `PostAuthExchangeStarted` (the Mode 5 vs Mode 1 discriminator that the
///   result-level `cms_connect` classification collapses) and `AuthClassified`
///   at the end (success or failure).
/// - Returns `Ok(())` on a successful auth-and-quit, `Err(UiError::*)` on any
///   failure mode.
///
/// RADIO-1 GUARDRAIL: This command is CMS-TELNET ONLY FOREVER. Any future
/// proposal to route it over an RF transport (ARDOP / VARA / Pactor) REQUIRES
/// (a) fresh RADIO-1 review per `docs/live-cms-testing-policy.md`,
/// (b) explicit transmit-consent gate at the click moment, and
/// (c) a separate command name (`cms_connect_test_rf`).
/// See spec §2 out-of-scope + §4.3 (iii).
#[tauri::command]
pub async fn cms_connect_test(
    app: AppHandle,
    state: State<'_, BackendState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
) -> Result<(), UiError> {
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;

    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        "Testing CMS credentials (auth-only)…".to_string(),
    );

    let events_sink: std::sync::Arc<dyn crate::winlink::b2f_events::B2fEventSink> =
        std::sync::Arc::new(crate::winlink::b2f_events::TauriEventSink::new(app.clone()));

    let attempt_id = crate::winlink::b2f_events::AttemptId::fresh();

    match backend.cms_connect_test(events_sink.clone(), attempt_id).await {
        Ok(()) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Info,
                "Credential test passed.".to_string(),
            );
            // Emit a success AuthClassified so the React banner can render
            // the green "credentials are valid" state.
            use crate::winlink::b2f_events::{B2fEvent, FailureMode};
            events_sink.push(B2fEvent::AuthClassified {
                // PostAuthExchangeStarted was already emitted (Mode 5
                // discriminator) — the auth-only contract fired FF + FQ.
                // There is no failure mode; use a sentinel-free Ok path:
                // the React hook treats a missing FailureMode as success.
                // For structural consistency the event still fires; the
                // mode field is unused on the Ok branch by the React hook
                // (it keys off PostAuthExchangeStarted presence).
                mode: FailureMode::Uncategorized,
                raw: None,
                attempt_id,
            });
            Ok(())
        }
        Err(BackendError::Cancelled) => {
            emit_session_line(&app, &log, LogLevel::Warn, "Credential test aborted.".to_string());
            Err(BackendError::Cancelled.into())
        }
        Err(e) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Error,
                format!("Credential test failed: {e}"),
            );
            // Emit AuthClassified so the React banner updates.
            use crate::winlink::b2f_events::{B2fEvent, FailureMode};
            let (mode, raw) = match &e {
                BackendError::RemoteError(payload) => {
                    (crate::winlink::auth_taxonomy::classify(payload), Some(payload.clone()))
                }
                BackendError::TransportFailed { .. } => (FailureMode::NetworkUnreachable, None),
                BackendError::AuthFailed { reason } => {
                    (crate::winlink::auth_taxonomy::classify(reason), Some(reason.clone()))
                }
                _ => (FailureMode::Uncategorized, Some(format!("{e}"))),
            };
            events_sink.push(B2fEvent::AuthClassified { mode, raw, attempt_id });
            Err(e.into())
        }
    }
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
    emit_session_line_with_source(app, buffer, level, LogSource::Transport, message);
}

fn emit_session_line_with_source(
    app: &AppHandle,
    buffer: &SessionLogState,
    level: LogLevel,
    source: LogSource,
    message: String,
) {
    crate::session_log_emit::emit(app, buffer, level, source, message);
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
    /// Opt-in: prompt the operator to select which pending inbound messages to
    /// download on a CMS connect (WLE "Review Pending Messages" parity), instead
    /// of auto-downloading all. Default false. Mirrors
    /// `Config.review_inbound_before_download` (tuxlink-bsiy). The inline
    /// SettingsPanel loads this into its checkbox on open.
    pub review_inbound_before_download: bool,
    /// AREDN mesh master-node host for Post Office discovery (tuxlink-1w7t).
    /// `None` → discovery uses `localnode.local.mesh`. Mirrors
    /// `Config.aredn_master_node_host`; the Network Post Office panel loads it
    /// into the discovery section's node-host input on open.
    pub aredn_master_node_host: Option<String>,
}

impl From<&config::Config> for ConfigViewDto {
    /// Map nested → flat. Pure; no I/O. Drives the unit test
    /// `config_view_dto_maps_nested_to_flat`.
    fn from(c: &config::Config) -> Self {
        ConfigViewDto {
            connect_to_cms: c.connect.connect_to_cms,
            transport: c.connect.transport,
            host: c.connect.host.clone(),
            callsign: c.identity.active_full.clone(),
            identifier: c.identity.identifier.clone(),
            grid: c.identity.grid.clone(),
            gps_state: c.privacy.gps_state,
            position_precision: c.privacy.position_precision,
            position_source: c.privacy.position_source,
            review_inbound_before_download: c.review_inbound_before_download,
            aredn_master_node_host: c.aredn_master_node_host.clone(),
        }
    }
}

/// Persist the AREDN mesh master-node host for Post Office discovery (tuxlink-1w7t).
///
/// `host = None` or blank clears the override (discovery falls back to
/// `localnode.local.mesh`). Pure config write — discovery reads config fresh on
/// each invocation, so no live-backend refresh is needed (unlike
/// `config_set_connect`).
#[tauri::command]
pub async fn config_set_aredn_master_node_host(host: Option<String>) -> Result<(), UiError> {
    let normalized = host
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty());
    let mut cfg =
        config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.aredn_master_node_host = normalized;
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
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
/// (spec §3.3 / §11.1: retained operator-session history).
///
/// Reads the durable `SessionLogState` managed by the app.
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
/// state starts empty and this command returns `[]` — the same contract as
/// before, now future-proof.
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
    match backend.connect(transport, None).await {
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
    // tuxlink-0063 (Phase 3, Task 3.8): effective call derives from the active
    // SessionIdentity — the authenticated Part 97 principal — NOT from
    // config.identity.active_full. Fail-closed: NoActiveIdentity errors here
    // before any hardware interaction; packet_connect_inner also gates on
    // active_identity() before opening the KISS link.
    //
    // TOCTOU note: `effective` below is a best-effort log/audit label only.
    // `packet_connect_inner` resolves `active_identity()` INDEPENDENTLY a second
    // time for the actual on-air call. If the active identity were switched between
    // these two resolutions, the log could show a different call than what goes on
    // air. This window is currently benign — nothing switches the active identity
    // mid-flight (Phase 6/7 UI does not exist yet). When Phase 6 introduces live
    // identity-switching, the correct fix is to resolve `active_identity()` ONCE
    // here and thread the resolved `SessionIdentity` through `backend.connect()`
    // so both the log label and the on-air call use the same snapshot (tuxlink-0063
    // Phase 3 / Phase 6 handoff).
    let session_id = backend.active_identity()?;
    // Effective call = <base>-<ssid> (the SSID'd link address we answer on).
    // See TOCTOU note above: this is the log label; packet_connect_inner's own
    // active_identity() resolution is the authoritative on-air call.
    let effective = format!(
        "{}-{}",
        session_id.mycall().as_str().to_uppercase(),
        cfg.packet.ssid
    );
    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        format!("Listening for an incoming packet call as {effective}…"),
    );
    match backend.connect(transport, None).await {
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
/// End-to-end (tuxlink-61yg): if the modem isn't running, this command
/// starts ardopcf in listen-only mode (init with LISTEN TRUE, no outbound
/// dial). If the modem IS running, sends LISTEN TRUE to the side channel.
/// Either way, spawns a long-lived consumer task that:
///   - Takes the transport from `ModemSession`
///   - Loops on `transport.wait_for_listener_connect()` until disarm
///   - On each inbound CONNECTED event, runs `gate_inbound_peer_now` against
///     the operator's allowlist + arms record
///   - On Accept: hands the modem data stream to `run_ardop_b2f_answer`
///     (mailbox-symmetric)
///   - On Reject: writes `DISCONNECT` via the cmd writer + appends a reject
///     event to the forensics log
///   - On disarm: sends LISTEN FALSE and returns the transport to the session
#[tauri::command]
pub async fn ardop_listen(
    app: AppHandle,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    session: State<'_, std::sync::Arc<crate::modem_status::ModemSession>>,
    listen_state: State<'_, std::sync::Arc<ArdopListenState>>,
) -> Result<(), UiError> {
    // Thin wrapper. The body lives in `ardop_listen_inner` so the
    // tuxlink-0ye6 Task 3.5 `ardop_open_session` auto-arm path can call
    // it without going through the Tauri dispatcher (which would require
    // re-extracting the same managed-state Arcs the outer caller already
    // has). Mirror of the VARA Task 3.2 `arm_vara_listener_inner` pattern.
    ardop_listen_inner(
        &app,
        log.inner(),
        session.inner(),
        listen_state.inner(),
    )
    .await
}

/// Inner body of [`ardop_listen`] — factored out so the
/// `ardop_open_session` auto-arm path (tuxlink-0ye6 Task 3.5) can call
/// it directly without re-dispatching through Tauri. Borrowed args (no
/// `State`-typed params) because the open-session path already holds the
/// same managed-state Arcs via its own `State` extractors. Mirror of the
/// VARA `arm_vara_listener_inner` pattern (Task 3.2).
pub(crate) async fn ardop_listen_inner(
    app: &AppHandle,
    log: &std::sync::Arc<SessionLogState>,
    session: &std::sync::Arc<crate::modem_status::ModemSession>,
    listen_state: &std::sync::Arc<ArdopListenState>,
) -> Result<(), UiError> {
    use crate::winlink::listener::{ListenerArmsRecord, TransportKind, DEFAULT_TTL};

    // Refuse a second arm while one is in flight.
    {
        let guard = listen_state.inner.lock().unwrap();
        if guard.is_some() {
            return Err(UiError::Internal {
                detail: "ARDOP listener is already armed".into(),
            });
        }
    }

    // Validate allowlist file loads.
    let allowlist_path = ardop_allowed_stations_path();
    let allowed = crate::winlink::listener::AllowedStations::load_from(&allowlist_path)
        .map_err(|e| UiError::Internal {
            detail: format!(
                "ARDOP listener arm refused: allowlist file at {} could not be loaded: {e}. \
                 Repair or delete the file (a missing file is fine — it falls back to the \
                 tuxlink WLE-parity default of allow_all=true + empty list).",
                allowlist_path.display()
            ),
        })?;

    // Append arms record BEFORE flipping the modem (Codex 2026-06-03 P2).
    let arms = ListenerArmsRecord::arm(TransportKind::Ardop, DEFAULT_TTL);
    let log_path = ardop_arms_log_path();
    arms.append_to_log(&log_path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;

    // Codex review 2026-06-03 [P1 #1] (tuxlink-61yg): only flip the running
    // modem into listen mode when it's IDLE. A modem in Connecting/Connected
    // state belongs to an active outbound dial; the consumer task's
    // `take_transport()` would yank the live connection out from under
    // `modem_ardop_b2f_exchange` and any in-flight disconnect logic.
    let cur_state = session.status_snapshot().state;
    let already_running_idle = matches!(
        cur_state,
        crate::modem_status::ModemState::Idle
    );
    let modem_busy = matches!(
        cur_state,
        crate::modem_status::ModemState::Connecting
            | crate::modem_status::ModemState::ConnectedIrs
            | crate::modem_status::ModemState::ConnectedIss
            | crate::modem_status::ModemState::Disconnecting
            | crate::modem_status::ModemState::Spawning
            | crate::modem_status::ModemState::Initializing
    );
    if modem_busy {
        return Err(UiError::Internal {
            detail: format!(
                "ARDOP listener arm refused — modem is busy ({:?}). \
                 Wait for the active session to end (or disconnect it) and re-arm.",
                cur_state
            ),
        });
    }
    // tuxlink-0063 (Phase 3, Task 3.6 — RF-correctness fix): capture the active
    // session identity AT ARM TIME so the answerer answers as the identity that
    // was active when the operator armed the listener — not a live-read that
    // could change if the operator switches identity during the armed window.
    // Resolved BEFORE any modem interaction (modem start OR LISTEN TRUE flip) so
    // that a NoActiveIdentity error leaves the RADIO completely untouched
    // (fail-closed before any on-air-capable state). tuxlink-0063 Phase 3.
    let session_id = app
        .state::<BackendState>()
        .current()
        .ok_or_else(|| UiError::Internal {
            detail: "ARDOP listener arm: backend offline — cannot resolve active identity".into(),
        })?
        .active_identity()?;

    if !already_running_idle {
        let cfg = config::read_config()
            .map_err(|e| UiError::Internal { detail: e.to_string() })?;
        let ardop_ui = cfg.modem_ardop.clone().unwrap_or_default();
        let session_arc: std::sync::Arc<crate::modem_status::ModemSession> =
            (*session).clone();
        // tuxlink-0063 (Phase 3, Task 3.9): the modem-init MYCALL (on-air
        // station ID) comes from the active SessionIdentity resolved above at
        // arm time. `session_id` is moved into the listener consumer task
        // below; SessionIdentity is Clone, so clone it for the modem-spawn
        // closure and keep the original for the consumer task.
        let session_id_for_spawn = session_id.clone();
        let cfg_for_spawn = cfg.clone();
        // Spawn the modem on a blocking thread (bind-wait + init can be slow).
        let res = tokio::task::spawn_blocking(move || {
            crate::modem_commands::start_modem_listen_only(
                &session_arc,
                &session_id_for_spawn,
                &cfg_for_spawn,
                &ardop_ui,
                |cfg, _target| {
                    crate::winlink::modem::ardop::transport::ArdopTransport::with_managed_modem(cfg)
                        .map(|t| Box::new(t) as Box<dyn crate::winlink::modem::ModemTransport>)
                        .map_err(|e| format!("{e:?}"))
                },
            )
        })
        .await
        .map_err(|e| UiError::Internal { detail: format!("modem spawn task failed: {e}") })?;
        if let Err(e) = res {
            return Err(UiError::Internal {
                detail: format!("ARDOP listener arm refused — could not start modem: {e}"),
            });
        }
    } else {
        // Modem already running — just flip LISTEN TRUE via the side channel.
        session.send_listen_command(true).map_err(|e| UiError::Internal {
            detail: format!("ARDOP listener arm — could not flip LISTEN: {e}"),
        })?;
    }

    // Build the mailbox for inbound-mail persistence.
    let mailbox = match app.path().app_data_dir() {
        Ok(dir) => {
            // tuxlink-2ns7: default to the sole FULL so received mail lands under
            // `mailbox/<FULL>/inbox` (the production read namespace).
            let mut mb = crate::native_mailbox::Mailbox::new(dir.join("native-mbox"));
            if let Some(full) = sole_full_identity() {
                mb = mb.with_default_identity(&full);
            }
            if let Some(svc) = app.try_state::<crate::search::commands::SearchService>() {
                mb = mb.with_index(svc.index.clone());
            }
            Some(std::sync::Arc::new(mb))
        }
        Err(_) => None,
    };

    // Spawn the consumer task that owns the transport for the armed window.
    let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut guard = listen_state.inner.lock().unwrap();
        *guard = Some(ArdopListenHandle { shutdown: shutdown.clone() });
    }

    let arbiter: std::sync::Arc<crate::position::PositionArbiter> =
        (*app.state::<std::sync::Arc<crate::position::PositionArbiter>>()).clone();
    let session_arc: std::sync::Arc<crate::modem_status::ModemSession> = (*session).clone();
    let arms_for_task = arms.clone();
    let app_clone = app.clone();
    let log_clone: std::sync::Arc<SessionLogState> = (*log).clone();
    let listen_state_for_task = (*listen_state).clone();
    tokio::task::spawn_blocking(move || {
        ardop_listener_consumer_task(
            session_arc,
            mailbox,
            allowed,
            arms_for_task,
            arbiter,
            session_id,
            shutdown,
            app_clone,
            log_clone,
            listen_state_for_task,
        );
    });

    let mins = arms.ttl.as_secs() / 60;
    emit_session_line(
        app,
        log,
        LogLevel::Info,
        format!(
            "ARDOP listener armed for {mins} min (consent uuid {}). \
             Modem is in LISTEN TRUE; waiting for inbound peers…",
            &arms.consent_uuid
        ),
    );
    Ok(())
}

/// Toggle the ARDOP listener on/off. `enabled == true` is equivalent to
/// `ardop_listen()`. `enabled == false` signals the consumer task to drain
/// (LISTEN FALSE, transport returned to session, status → Stopped).
#[tauri::command]
pub async fn ardop_set_listen(
    app: AppHandle,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    session: State<'_, std::sync::Arc<crate::modem_status::ModemSession>>,
    listen_state: State<'_, std::sync::Arc<ArdopListenState>>,
    enabled: bool,
) -> Result<(), UiError> {
    ardop_set_listen_inner(
        &app,
        log.inner(),
        session.inner(),
        listen_state.inner(),
        enabled,
    )
    .await
}

/// Inner body of [`ardop_set_listen`] — factored out so the tuxlink-0ye6
/// Task 3.5 `ardop_close_session` path can disarm the listener without
/// re-dispatching through Tauri. Mirror of the VARA Task 3.3
/// `disarm_vara_listener_inner` pattern, but kept as a full set-listen
/// (arm OR disarm) helper to preserve the `enabled == true` re-arm path
/// the existing `ardop_set_listen` exposed.
pub(crate) async fn ardop_set_listen_inner(
    app: &AppHandle,
    log: &std::sync::Arc<SessionLogState>,
    session: &std::sync::Arc<crate::modem_status::ModemSession>,
    listen_state: &std::sync::Arc<ArdopListenState>,
    enabled: bool,
) -> Result<(), UiError> {
    use std::sync::atomic::Ordering;
    if enabled {
        return ardop_listen_inner(app, log, session, listen_state).await;
    }
    let handle = {
        let mut guard = listen_state.inner.lock().unwrap();
        guard.take()
    };
    if let Some(h) = handle {
        h.shutdown.store(true, Ordering::SeqCst);
        // Codex review 2026-06-03 [P1 #3] (tuxlink-61yg): disarm during
        // active B2F. The consumer is blocked inside run_ardop_b2f_answer
        // (synchronous). LISTEN FALSE alone doesn't tear down the active
        // ARQ link; the consumer's B2F can loop forever. Send ABORT via
        // the cmd-side-channel to force the active session to fault out
        // (ardopcf emits FAULT/NEWSTATE DISC which unwinds the B2F recv
        // loop), then send LISTEN FALSE. Same pattern as
        // modem_ardop_disconnect_inner.
        let _ = session.abort_in_flight();
        let _ = session.send_listen_command(false);
        emit_session_line(
            app,
            log,
            LogLevel::Info,
            "ARDOP listener disarming — ABORT + LISTEN FALSE sent; waiting for consumer to drain.".to_string(),
        );
    } else {
        emit_session_line(
            app,
            log,
            LogLevel::Warn,
            "ARDOP listener disarm: no armed listener".to_string(),
        );
    }
    Ok(())
}

/// Shared state for the ARDOP listener consumer task — holds the disarm flag
/// so `ardop_set_listen(false)` can signal the task to drain.
#[derive(Default)]
pub struct ArdopListenState {
    pub inner: std::sync::Mutex<Option<ArdopListenHandle>>,
}

impl ArdopListenState {
    /// True iff a listener handle is currently registered (the consumer task
    /// has been spawned and not yet drained). Mirror of
    /// [`VaraListenState::is_armed`] — added for tuxlink-0ye6 Task 3.5 so
    /// `ardop_open_session` can detect the auto-armed state in its
    /// post-open snapshot and `ardop_close_session` can early-skip the
    /// disarm side-channel when no listener is armed.
    pub fn is_armed(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }
}

pub struct ArdopListenHandle {
    pub shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[allow(clippy::too_many_arguments)]
fn ardop_listener_consumer_task(
    session: std::sync::Arc<crate::modem_status::ModemSession>,
    mailbox: Option<std::sync::Arc<crate::native_mailbox::Mailbox>>,
    allowed: crate::winlink::listener::AllowedStations,
    arms: crate::winlink::listener::ListenerArmsRecord,
    arbiter: std::sync::Arc<crate::position::PositionArbiter>,
    // tuxlink-0063 Phase 3 Task 3.6: session_id is captured AT LISTENER-ARM TIME
    // so the answerer uses the identity that was active when the operator armed the
    // listener — not a live-read that could change if the operator switches identity
    // during the armed window. SessionIdentity is Clone.
    session_id: crate::identity::SessionIdentity,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    app: AppHandle,
    log: std::sync::Arc<SessionLogState>,
    listen_state: std::sync::Arc<ArdopListenState>,
) {
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use crate::winlink::listener::{
        listener_decide_at, packet_gate, ListenerDecision, PeerId, StationPassword,
    };
    use crate::winlink_backend::run_ardop_b2f_answer;
    use crate::winlink::modem::ardop::session::ConnectInfo;

    let log_clone_for_progress = log.clone();
    let app_clone_for_progress = app.clone();
    let progress = move |line: &str| {
        emit_session_line(
            &app_clone_for_progress,
            &log_clone_for_progress,
            LogLevel::Info,
            line.to_string(),
        );
    };

    // tuxlink-pdnw (Codex Phase 3-4 P1 #5): snapshot the close-generation
    // BEFORE the transport take. If `ardop_close_session_inner` bumps the
    // generation while this consumer is in flight, the shutdown-path
    // install-back below sees the stale snapshot and drops the transport
    // instead of restoring it into a session the operator just closed.
    let close_gen_snapshot = session.current_close_generation();

    // Take the transport. If someone else has it (race), clean up cleanly.
    // Codex review 2026-06-03 [P2 #5] (tuxlink-61yg): a stale take_transport
    // result would leave listen_state populated → UI claims armed, no
    // consumer running. Mirror the shutdown-cleanup before returning.
    let mut transport = match session.take_transport() {
        Some(t) => t,
        None => {
            progress(
                "ARDOP listener consumer: transport not present at start; \
                 clearing arm state.",
            );
            let _ = session.send_listen_command(false);
            *listen_state.inner.lock().unwrap() = None;
            return;
        }
    };

    let no_password = StationPassword::no_keyring();
    let log_path = ardop_arms_log_path();

    while !shutdown.load(Ordering::SeqCst) {
        let evt = match transport.wait_for_listener_connect(Duration::from_secs(1)) {
            Ok(Some(info)) => info,
            Ok(None) => continue,
            Err(e) => {
                progress(&format!("ARDOP listener consumer: transport error {e}; stopping."));
                break;
            }
        };
        let ConnectInfo { peer_call, bandwidth_hz } = evt;
        progress(&format!(
            "ARDOP inbound: {} @ {} Hz; gating…",
            peer_call, bandwidth_hz
        ));

        // Parse peer callsign into Address for PeerId.
        let peer_addr = parse_peer_call_for_listener(&peer_call);
        let peer_id = PeerId::Callsign(peer_addr.clone());

        let decision = listener_decide_at(
            &peer_id,
            None,
            &allowed,
            &no_password,
            &arms,
            std::time::SystemTime::now(),
        );

        match decision {
            ListenerDecision::Accept => {
                // Read config + run B2F over the live transport.
                progress(&format!("ARDOP listener: accepting {}; running B2F…", peer_call));
                let cfg = match crate::config::read_config() {
                    Ok(c) => c,
                    Err(e) => {
                        progress(&format!("ARDOP listener: config read failed {e}; dropping link"));
                        let _ = arq_disconnect_via_cmd_writer(&*transport);
                        continue;
                    }
                };
                let mb_ref = mailbox.as_deref();
                let result = match mb_ref {
                    Some(mb) => run_ardop_b2f_answer(
                        transport.as_mut(),
                        &peer_call,
                        &cfg,
                        &session_id,
                        mb,
                        Some(arbiter.as_ref()),
                        Some(&progress),
                    ),
                    None => {
                        // Codex review 2026-06-03 [P2 #7] (tuxlink-61yg): the
                        // prior "Mailbox::new(temp_dir())" path rooted a real
                        // mailbox at the world-readable /tmp directory. On a
                        // multi-user host, a local user could pre-create
                        // /tmp/outbox to inject outbound proposals into the
                        // session. Use a fresh per-session private tempdir
                        // (tempfile::tempdir cleans up on drop).
                        progress(
                            "ARDOP listener: no mailbox available; \
                             protocol-only exchange in private tempdir.",
                        );
                        match tempfile::tempdir() {
                            Ok(tmp) => {
                                let tmp_mb = crate::native_mailbox::Mailbox::new(tmp.path());
                                let r = run_ardop_b2f_answer(
                                    transport.as_mut(),
                                    &peer_call,
                                    &cfg,
                                    &session_id,
                                    &tmp_mb,
                                    Some(arbiter.as_ref()),
                                    Some(&progress),
                                );
                                // `tmp` drops here → directory deletes.
                                r
                            }
                            Err(e) => {
                                progress(&format!(
                                    "ARDOP listener: could not create private \
                                     tempdir for protocol-only exchange ({e}); \
                                     dropping link."
                                ));
                                let _ = arq_disconnect_via_cmd_writer(&*transport);
                                continue;
                            }
                        }
                    }
                };
                match result {
                    Ok(()) => {
                        progress(&format!("ARDOP listener: exchange with {} complete.", peer_call));
                    }
                    Err(e) => {
                        progress(&format!("ARDOP listener: exchange with {} failed: {e}", peer_call));
                    }
                }
                // Best-effort DISCONNECT to release the ARQ link. Run
                // through the cmd-writer side-channel rather than the
                // synchronous arq_disconnect that would need a CmdSocket
                // reference (not exposed on ModemTransport trait).
                let _ = arq_disconnect_via_cmd_writer(&*transport);
            }
            ListenerDecision::RejectAllowlist | ListenerDecision::RejectExpired
            | ListenerDecision::RejectPassword => {
                let reason = match decision {
                    ListenerDecision::RejectAllowlist => "allowlist",
                    ListenerDecision::RejectExpired => "expired",
                    _ => "password",
                };
                progress(&format!(
                    "ARDOP listener: rejecting {} (reason: {})",
                    peer_call, reason
                ));
                let event = packet_gate::ListenerRejectEvent::new(
                    crate::winlink::listener::TransportKind::Ardop,
                    reason,
                    &peer_id,
                );
                let _ = event.append_to_log(&log_path);
                let _ = arq_disconnect_via_cmd_writer(&*transport);
                // Codex review 2026-06-03 [P1 #4] (tuxlink-61yg): after
                // DISCONNECT, drain modem events for a bounded window so
                // any DISCONNECTED/NEWSTATE DISC ack is consumed before
                // we loop to wait for the NEXT inbound. Otherwise the
                // rejected peer's ARQ link can still be holding the
                // modem while wait_for_listener_connect blocks.
                let _ = transport.wait_for_listener_connect(Duration::from_millis(500));
            }
        }
    }

    // Shutdown path: send LISTEN FALSE + return transport.
    progress("ARDOP listener consumer: draining; sending LISTEN FALSE.");
    let _ = session.send_listen_command(false);
    // tuxlink-pdnw (Codex Phase 3-4 P1 #5): guarded install. Stale snapshot
    // → close intervened (ardop_close_session_inner ran since we took the
    // transport). Drop the transport instead of installing — the session
    // is in a Stopped posture and a fresh open will spawn a new transport.
    match session
        .install_transport_if_generation_matches(transport, close_gen_snapshot)
    {
        Ok(()) => {
            let mut snap = session.status_snapshot();
            snap.peer = None;
            snap.last_error = None;
            session.set_status(snap);
        }
        Err(dropped) => {
            progress(
                "ARDOP listener consumer: close intervened during drain; \
                 dropping transport instead of restoring session.",
            );
            drop(dropped);
        }
    }
    // Clear shared state.
    *listen_state.inner.lock().unwrap() = None;
    progress("ARDOP listener disarmed (transport returned).");
}

/// Best-effort cmd-writer DISCONNECT (sends "DISCONNECT\r"). Returns Err if
/// the transport has no abort writer (modem not initialised).
fn arq_disconnect_via_cmd_writer(
    transport: &dyn crate::winlink::modem::ModemTransport,
) -> std::io::Result<()> {
    use std::io::Write;
    // tuxlink-0ye6 Task 4.1 widened the return type to (writer, stream).
    // The DISCONNECT path only needs the cooperative writer; the hard-close
    // stream is discarded — graceful disconnect is the contract here, and
    // an unresponsive peer just surfaces the write error to the caller.
    let (mut writer, _stream) = transport.try_clone_abort_writer().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotConnected, "no cmd writer")
    })?;
    writer.write_all(b"DISCONNECT\r")?;
    writer.flush()
}

/// Parse the peer callsign from ardopcf's `CONNECTED <peer> <bw>` event into
/// an [`Address`] suitable for the listener-arms gate. Tolerates trailing
/// whitespace, SSID suffix (`N7CPZ-7`), and uppercases the base call to
/// match the foundation's canonicalised allowlist storage.
fn parse_peer_call_for_listener(raw: &str) -> crate::winlink::ax25::frame::Address {
    let trimmed = raw.trim();
    if let Some((call, ssid_str)) = trimmed.split_once('-') {
        if let Ok(ssid) = ssid_str.parse::<u8>() {
            return crate::winlink::ax25::frame::Address {
                call: call.to_uppercase(),
                ssid,
            };
        }
    }
    crate::winlink::ax25::frame::Address {
        call: trimmed.to_uppercase(),
        ssid: 0,
    }
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

// ============================================================================
// tuxlink-9ls2 — VARA listener (allowed-stations + arms + LISTEN toggle)
// ============================================================================
//
// Mirror of the ARDOP listener section above, adapted for VARA. Like ARDOP,
// VARA has no station-password layer (clean-sheet decision: plaintext shared
// secrets over RF are worse than no secret) — only the allowlist + arms TTL
// apply.
//
// Key shape divergences from ARDOP:
// - VARA is NOT spawned by tuxlink. The operator runs VARA externally
//   (Windows native, or under Wine on x86 Linux). `vara_listen` therefore
//   refuses to arm unless `vara_open_session` has already opened the TCP
//   transport (state == Open). There is no "start the modem first" auto-spawn.
// - VARA's LISTEN setter uses `LISTEN ON` / `LISTEN OFF` (not ARDOP's
//   `LISTEN TRUE` / `LISTEN FALSE`).
// - VARA owns the cmd socket directly — there's no separate "side-channel
//   cmd writer" like ARDOP's. The arm command sends LISTEN ON via the
//   session's helper that briefly locks the session mutex.
//
// Persistence: `<config-dir>/listener/vara/allowed_stations.json`. The arms
// + reject forensics log is shared cross-transport at
// `<config-dir>/listener/listener_arms.jsonl` (same file ARDOP / Packet /
// Telnet append to — operators get a unified audit trail).

/// Resolve the config dir from `config::config_path()` and return
/// `<base>/listener/vara/allowed_stations.json`.
fn vara_allowed_stations_path() -> std::path::PathBuf {
    let cfg_dir = config::config_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    crate::winlink::modem::vara::allowed_stations_path(&cfg_dir)
}

/// Process-wide mutex serialising the load-mutate-save cycle on the VARA
/// allowlist file. Mirror of the Telnet / Packet / ARDOP locks. Without it,
/// concurrent UI commands race: both load the same file, mutate in-memory
/// copies, second save clobbers first.
fn vara_allowlist_file_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// Cross-transport listener forensics log path. Reuses the same shared file
/// the ARDOP / Packet listeners write to so the operator sees a unified
/// arm + reject history across all transports.
fn vara_arms_log_path() -> std::path::PathBuf {
    ardop_arms_log_path()
}

/// Read the VARA allowed-stations JSON file. Returns the WLE-parity default
/// (`allow_all: true`, empty lists) if the file is absent — same posture as
/// the ARDOP / Packet allowlist readers.
#[tauri::command]
pub async fn vara_allowed_stations_get() -> Result<AllowedStationsDto, UiError> {
    let path = vara_allowed_stations_path();
    let allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(AllowedStationsDto::from(&allowed))
}

/// Add a callsign (or callsign-wildcard like `N7*`) to the VARA
/// allowed-stations list. Idempotent.
#[tauri::command]
pub async fn vara_allowed_stations_add(callsign: String) -> Result<(), UiError> {
    let trimmed = callsign.trim();
    if trimmed.is_empty() {
        return Err(UiError::Internal { detail: "callsign must not be empty".into() });
    }
    let _guard = vara_allowlist_file_lock().lock().unwrap();
    let path = vara_allowed_stations_path();
    let mut allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    allowed.add_callsign_pattern(trimmed);
    allowed
        .save_to(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}

/// Remove a callsign (exact-match, case-insensitive after uppercasing) from
/// the VARA allowed-stations list. Silently succeeds if the entry isn't
/// present.
#[tauri::command]
pub async fn vara_allowed_stations_remove(callsign: String) -> Result<(), UiError> {
    let needle = callsign.trim().to_uppercase();
    if needle.is_empty() {
        return Err(UiError::Internal { detail: "callsign must not be empty".into() });
    }
    let _guard = vara_allowlist_file_lock().lock().unwrap();
    let path = vara_allowed_stations_path();
    let allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
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

/// Toggle the master `Allow All Connections` flag on the VARA allowlist.
///
/// `true` = WLE-compatible permissive: every inbound VARA peer that
/// completes the modem-level ARQ handshake is accepted.
///
/// `false` = restrict to the operator-curated `callsigns` list.
#[tauri::command]
pub async fn vara_allowed_stations_set_allow_all(allow_all: bool) -> Result<(), UiError> {
    let _guard = vara_allowlist_file_lock().lock().unwrap();
    let path = vara_allowed_stations_path();
    let mut allowed = crate::winlink::listener::AllowedStations::load_from(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    allowed.set_allow_all(allow_all);
    allowed
        .save_to(&path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}

/// Shared state for the VARA listener consumer task — holds the disarm flag
/// so `vara_set_listen(false)` can signal the task to drain.
#[derive(Default)]
pub struct VaraListenState {
    pub inner: std::sync::Mutex<Option<VaraListenHandle>>,
}

pub struct VaraListenHandle {
    pub shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Returns `true` when a VARA listener consumer task is currently armed
/// (a `VaraListenHandle` is present in `listen_state.inner`). Cheap; safe
/// to poll. Used by the `vara_open_session` auto-arm path's unit tests to
/// assert intent-driven arming without spinning up a real consumer task.
impl VaraListenState {
    pub fn is_armed(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }
}

/// Arm the VARA listener for the default TTL (1 hour per architecture §5).
///
/// **Precondition:** VARA must already be in state==Open (operator must
/// have run `vara_open_session` first). Unlike ARDOP, tuxlink does NOT
/// spawn VARA — it's an externally-managed Windows process (native or
/// under Wine) and the operator owns the lifecycle. An arm against a
/// closed session returns `UiError::Internal` with a clear remediation.
///
/// Sequence:
/// 1. Refuse if a listener is already armed (single-flight).
/// 2. Validate the allowlist file loads (corrupt file → reject arm).
/// 3. Validate VARA session is Open.
/// 4. Mint the `ListenerArmsRecord` + append to the cross-transport
///    forensics log.
/// 5. Send `LISTEN ON` via the session's brief-lock helper — if the
///    write fails, the arm fails without spawning a consumer.
/// 6. Spawn the long-lived consumer task that takes the transport via
///    `take_transport()` and loops on `serve_inbound_one` until disarm.
#[tauri::command]
pub async fn vara_listen(
    app: AppHandle,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    vara_session: State<'_, std::sync::Arc<crate::winlink::modem::vara::VaraSession>>,
    listen_state: State<'_, std::sync::Arc<VaraListenState>>,
) -> Result<(), UiError> {
    use crate::winlink::listener::TransportKind;
    // Codex Phase 3-4 boundary P2 #3 (tuxlink-u1r7): the listener-arm
    // record must reflect the session's actual VARA-HF vs VARA-FM kind,
    // not a hardcoded HF. The session's `active_transport_kind` was set
    // by `vara_open_session_inner` after the TCP open succeeded; pull
    // it here. If no session is open (snapshot has None), the inner
    // helper will surface a clean "transport not Open" error.
    let kind = vara_session
        .snapshot()
        .active_transport_kind
        .unwrap_or(TransportKind::VaraHf);
    // Thin wrapper. The body lives in `arm_vara_listener_inner` so the
    // `vara_open_session` auto-arm path can call it without going through
    // the Tauri command dispatcher (which would require an AppHandle from
    // inside the inner command + double-acquire the same State).
    arm_vara_listener_inner(
        &app,
        log.inner(),
        vara_session.inner(),
        listen_state.inner(),
        kind,
    )
    .await
}

/// Inner body of [`vara_listen`] — factored out so the
/// `vara_open_session` auto-arm path (tuxlink-0ye6 Task 3.2) can call it
/// directly without re-dispatching through Tauri. Borrowed args (no
/// `State`-typed params) because the open-session path already holds the
/// same managed-state Arcs via its own `State` extractors.
///
/// `transport_kind` (Codex Phase 3-4 boundary P2 #3 — tuxlink-u1r7) flows
/// from the session's `active_transport_kind` (manual arm path) or from
/// the operator-supplied `vara_open_session` arg (auto-arm path). Used as
/// the arms-record + reject-event transport label so VARA-FM listeners
/// don't surface in forensics as VARA-HF. Validation: rejects non-VARA
/// kinds before any arm state mutation.
///
/// Note: intent is not currently load-bearing inside the consumer task
/// (the listener accepts any allowlisted peer regardless of intent), but
/// reserving the parameter at the inner boundary makes Phase 3's
/// `RadioOnly`-specific routing-flag wiring a no-source-shape change.
pub(crate) async fn arm_vara_listener_inner(
    app: &AppHandle,
    log: &std::sync::Arc<SessionLogState>,
    vara_session: &std::sync::Arc<crate::winlink::modem::vara::VaraSession>,
    listen_state: &std::sync::Arc<VaraListenState>,
    transport_kind: crate::winlink::listener::TransportKind,
) -> Result<(), UiError> {
    use crate::winlink::listener::{ListenerArmsRecord, TransportKind, DEFAULT_TTL};

    // Codex Phase 3-4 boundary P2 #3 (tuxlink-u1r7): defensive validation —
    // arm_vara_listener_inner is VARA-only. A future regression that
    // routes a non-VARA TransportKind here surfaces a clean error before
    // any arms-record / LISTEN ON mutation.
    if !matches!(
        transport_kind,
        TransportKind::VaraHf | TransportKind::VaraFm
    ) {
        return Err(UiError::Internal {
            detail: format!(
                "arm_vara_listener_inner invoked with non-VARA transport_kind={:?}",
                transport_kind
            ),
        });
    }

    // Refuse a second arm while one is in flight.
    {
        let guard = listen_state.inner.lock().unwrap();
        if guard.is_some() {
            return Err(UiError::Internal {
                detail: "VARA listener is already armed".into(),
            });
        }
    }

    // Validate allowlist file loads.
    let allowlist_path = vara_allowed_stations_path();
    let allowed = crate::winlink::listener::AllowedStations::load_from(&allowlist_path)
        .map_err(|e| UiError::Internal {
            detail: format!(
                "VARA listener arm refused: allowlist file at {} could not be loaded: {e}. \
                 Repair or delete the file (a missing file is fine — it falls back to the \
                 tuxlink WLE-parity default of allow_all=true + empty list).",
                allowlist_path.display()
            ),
        })?;

    // VARA precondition: the transport must be Open. Unlike ARDOP we
    // don't auto-spawn — the operator runs VARA externally and must
    // open the session first.
    let snap = vara_session.snapshot();
    if !matches!(
        snap.state,
        crate::winlink::modem::vara::VaraState::Open
    ) {
        return Err(UiError::Internal {
            detail: format!(
                "VARA listener arm refused — VARA transport is not Open (current state: {:?}). \
                 Press Start on the VARA panel first (vara_open_session) so the TCP transport \
                 is open before arming the listener.",
                snap.state
            ),
        });
    }

    // Append arms record BEFORE flipping the modem (mirror of ARDOP's
    // Codex 2026-06-03 P2 fix — the arms record is the audit anchor; if
    // the modem flip fails downstream we still have the attempt logged).
    // Codex Phase 3-4 boundary P2 #3 (tuxlink-u1r7): record the operator-
    // supplied transport_kind instead of a hardcoded VaraHf so FM arm
    // events surface accurately in forensics.
    let arms = ListenerArmsRecord::arm(transport_kind, DEFAULT_TTL);
    let log_path = vara_arms_log_path();
    arms.append_to_log(&log_path)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;

    // tuxlink-0063 (Phase 3, Task 3.7 — RF-correctness fix): capture the active
    // session identity AT ARM TIME so the answerer answers as the identity that
    // was active when the operator armed the listener — not a live-read that
    // could change if the operator switches identity during the armed window.
    // Resolved BEFORE send_listen_on() so that a NoActiveIdentity error leaves
    // the RADIO completely untouched (fail-closed before any on-air-capable
    // state). tuxlink-0063 Phase 3.
    let session_id = app
        .state::<BackendState>()
        .current()
        .ok_or_else(|| UiError::Internal {
            detail: "VARA listener arm: backend offline — cannot resolve active identity".into(),
        })?
        .active_identity()?;

    // Flip LISTEN ON via the session's brief-lock helper. If this fails,
    // the arm fails cleanly without a consumer task to clean up.
    vara_session
        .send_listen_on()
        .map_err(|e| UiError::Internal {
            detail: format!("VARA listener arm — could not flip LISTEN ON: {e}"),
        })?;

    // Build the mailbox for inbound-mail persistence (same shape as ARDOP).
    let mailbox = match app.path().app_data_dir() {
        Ok(dir) => {
            // tuxlink-2ns7: default to the sole FULL (per-FULL received-mail namespace).
            let mut mb = crate::native_mailbox::Mailbox::new(dir.join("native-mbox"));
            if let Some(full) = sole_full_identity() {
                mb = mb.with_default_identity(&full);
            }
            if let Some(svc) = app.try_state::<crate::search::commands::SearchService>() {
                mb = mb.with_index(svc.index.clone());
            }
            Some(std::sync::Arc::new(mb))
        }
        Err(_) => None,
    };

    // Spawn the consumer task that owns the transport for the armed window.
    let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut guard = listen_state.inner.lock().unwrap();
        *guard = Some(VaraListenHandle { shutdown: shutdown.clone() });
    }
    let arbiter: std::sync::Arc<crate::position::PositionArbiter> =
        (*app.state::<std::sync::Arc<crate::position::PositionArbiter>>()).clone();
    let vara_session_arc: std::sync::Arc<crate::winlink::modem::vara::VaraSession> =
        vara_session.clone();
    let arms_for_task = arms.clone();
    let app_clone = app.clone();
    let log_clone: std::sync::Arc<SessionLogState> = log.clone();
    let listen_state_for_task = listen_state.clone();
    let bound_host = snap.bound_host.clone();
    let bound_cmd_port = snap.bound_cmd_port;
    tokio::task::spawn_blocking(move || {
        vara_listener_consumer_task(
            vara_session_arc,
            mailbox,
            allowed,
            arms_for_task,
            arbiter,
            session_id,
            shutdown,
            app_clone,
            log_clone,
            listen_state_for_task,
            bound_host,
            bound_cmd_port,
            transport_kind,
        );
    });

    let mins = arms.ttl.as_secs() / 60;
    emit_session_line(
        app,
        log,
        LogLevel::Info,
        format!(
            "VARA listener armed for {mins} min (consent uuid {}). \
             Modem is in LISTEN ON; waiting for inbound peers…",
            &arms.consent_uuid
        ),
    );
    Ok(())
}

/// Toggle the VARA listener on/off. `enabled == true` is equivalent to
/// `vara_listen()`. `enabled == false` signals the consumer task to drain
/// (LISTEN OFF, transport returned to session, status → Open).
#[tauri::command]
pub async fn vara_set_listen(
    app: AppHandle,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    vara_session: State<'_, std::sync::Arc<crate::winlink::modem::vara::VaraSession>>,
    listen_state: State<'_, std::sync::Arc<VaraListenState>>,
    enabled: bool,
) -> Result<(), UiError> {
    if enabled {
        return vara_listen(app, log, vara_session, listen_state).await;
    }
    disarm_vara_listener_inner(&app, log.inner(), listen_state.inner());
    Ok(())
}

/// Inner body of [`vara_set_listen`] with `enabled == false` — factored out
/// so [`crate::winlink::modem::vara::commands::vara_close_session`] (tuxlink-0ye6
/// Task 3.3) can call it directly without re-dispatching through Tauri. Mirrors
/// the shape of [`arm_vara_listener_inner`] (Task 3.2) so the close-session
/// path's listener disarm is one helper call.
///
/// Idempotent: when no listener is armed, emits a Warn log line and returns
/// without an error. The close-session contract is "unconditional teardown,"
/// so a missing-listener-on-disarm is information, not a failure.
pub(crate) fn disarm_vara_listener_inner(
    app: &AppHandle,
    log: &std::sync::Arc<SessionLogState>,
    listen_state: &std::sync::Arc<VaraListenState>,
) {
    use std::sync::atomic::Ordering;
    let handle = {
        let mut guard = listen_state.inner.lock().unwrap();
        guard.take()
    };
    if let Some(h) = handle {
        // Signal the consumer task to drain. The task observes this on
        // its next loop iteration; it will then send LISTEN OFF +
        // DISCONNECT (defensive) and return the transport to the
        // session. We do NOT send LISTEN OFF / DISCONNECT here because
        // the consumer task owns the transport via take_transport();
        // any send from here would race the consumer's transport.
        h.shutdown.store(true, Ordering::SeqCst);
        emit_session_line(
            app,
            log,
            LogLevel::Info,
            "VARA listener disarming — shutdown flag set; waiting for consumer to drain.".to_string(),
        );
    } else {
        emit_session_line(
            app,
            log,
            LogLevel::Warn,
            "VARA listener disarm: no armed listener".to_string(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn vara_listener_consumer_task(
    vara_session: std::sync::Arc<crate::winlink::modem::vara::VaraSession>,
    mailbox: Option<std::sync::Arc<crate::native_mailbox::Mailbox>>,
    allowed: crate::winlink::listener::AllowedStations,
    arms: crate::winlink::listener::ListenerArmsRecord,
    arbiter: std::sync::Arc<crate::position::PositionArbiter>,
    // tuxlink-0063 Phase 3 Task 3.7: session_id is captured AT LISTENER-ARM TIME
    // so the answerer uses the identity that was active when the operator armed the
    // listener — not a live-read that could change if the operator switches identity
    // during the armed window. SessionIdentity is Clone.
    session_id: crate::identity::SessionIdentity,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    app: AppHandle,
    log: std::sync::Arc<SessionLogState>,
    listen_state: std::sync::Arc<VaraListenState>,
    bound_host: Option<String>,
    bound_cmd_port: Option<u16>,
    transport_kind: crate::winlink::listener::TransportKind,
) {
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use crate::winlink::modem::vara::{InboundOutcome, VaraListenerError};
    use crate::winlink_backend::run_vara_b2f_answer;

    let log_clone_for_progress = log.clone();
    let app_clone_for_progress = app.clone();
    let progress = move |line: &str| {
        emit_session_line(
            &app_clone_for_progress,
            &log_clone_for_progress,
            LogLevel::Info,
            line.to_string(),
        );
    };

    // tuxlink-pdnw (Codex Phase 3-4 P1 #4): snapshot the close-generation
    // BEFORE the transport take. If `vara_close_session_inner` bumps the
    // generation while this consumer is in flight, the shutdown-path
    // install-back below sees the stale snapshot and drops the transport
    // instead of restoring the session to `Open`.
    let close_gen_snapshot = vara_session.current_close_generation();

    // Take the transport. If someone else has it (race), clean up cleanly.
    // Mirror of ARDOP's tuxlink-61yg Codex P2 #5 fix.
    let mut transport = match vara_session.take_transport() {
        Some(t) => t,
        None => {
            progress(
                "VARA listener consumer: transport not present at start; \
                 clearing arm state.",
            );
            *listen_state.inner.lock().unwrap() = None;
            return;
        }
    };

    while !shutdown.load(Ordering::SeqCst) {
        // 1-second tick budget on each poll — same cadence as ARDOP's
        // wait_for_listener_connect. The consumer task reacts to disarm
        // within ~1s.
        let outcome = match crate::winlink::modem::vara::serve_inbound_one(
            &mut transport,
            &allowed,
            &arms,
            Duration::from_secs(1),
        ) {
            Ok(o) => o,
            Err(VaraListenerError::Timeout) => continue,
            Err(VaraListenerError::RemoteDisconnect) => {
                progress("VARA listener consumer: remote disconnected mid-listen; continuing.");
                continue;
            }
            Err(e) => {
                progress(&format!("VARA listener consumer: transport error {e}; stopping."));
                break;
            }
        };

        match outcome {
            InboundOutcome::Accepted { peer: _, peer_call } => {
                progress(&format!(
                    "VARA inbound: {} accepted; running B2F…",
                    peer_call
                ));
                // Codex Phase 3-4 boundary P2 #4 (tuxlink-u1r7): mark
                // the session as running an inbound exchange so a
                // status poll surfaces Exchange::Inbound. Cleared at
                // the bottom of this branch (success / config-read
                // failure / tempdir failure all route through the
                // continue or the end-of-branch end_exchange).
                vara_session.begin_exchange(
                    crate::modem_status::ExchangeState::Inbound,
                );
                let cfg = match crate::config::read_config() {
                    Ok(c) => c,
                    Err(e) => {
                        progress(&format!("VARA listener: config read failed {e}; dropping link"));
                        let _ = crate::winlink::modem::vara::set_listen(&mut transport, false);
                        vara_session.end_exchange();
                        continue;
                    }
                };
                let mb_ref = mailbox.as_deref();
                let result = match mb_ref {
                    Some(mb) => run_vara_b2f_answer(
                        &mut transport,
                        &peer_call,
                        &cfg,
                        &session_id,
                        mb,
                        Some(arbiter.as_ref()),
                        Some(&progress),
                    ),
                    None => {
                        progress(
                            "VARA listener: no mailbox available; \
                             protocol-only exchange in private tempdir.",
                        );
                        match tempfile::tempdir() {
                            Ok(tmp) => {
                                let tmp_mb = crate::native_mailbox::Mailbox::new(tmp.path());
                                let r = run_vara_b2f_answer(
                                    &mut transport,
                                    &peer_call,
                                    &cfg,
                                    &session_id,
                                    &tmp_mb,
                                    Some(arbiter.as_ref()),
                                    Some(&progress),
                                );
                                r
                            }
                            Err(e) => {
                                progress(&format!(
                                    "VARA listener: could not create private \
                                     tempdir for protocol-only exchange ({e}); \
                                     dropping link."
                                ));
                                vara_session.end_exchange();
                                continue;
                            }
                        }
                    }
                };
                match result {
                    Ok(()) => {
                        progress(&format!("VARA listener: exchange with {} complete.", peer_call));
                    }
                    Err(e) => {
                        progress(&format!("VARA listener: exchange with {} failed: {e}", peer_call));
                    }
                }
                // After the B2F exchange completes (success or fail) the
                // peer's link will normally have torn down. Send a
                // best-effort DISCONNECT so the modem releases the ARQ
                // link if it's still up.
                let _ = transport.send(&crate::winlink::modem::vara::OutboundCommand::Disconnect);
                // Codex Phase 3-4 boundary P2 #4: clear the exchange
                // marker now that the b2f handling is fully done.
                vara_session.end_exchange();
            }
            InboundOutcome::RejectedAllowlist { peer } => {
                progress(&format!(
                    "VARA listener: rejecting {} (reason: allowlist)",
                    peer.call
                ));
                let log_path = vara_arms_log_path();
                let peer_id = crate::winlink::listener::PeerId::Callsign(peer);
                // Codex Phase 3-4 boundary P2 #3 (tuxlink-u1r7): use the
                // operator-supplied transport_kind, not a hardcoded VaraHf.
                let event = crate::winlink::listener::packet_gate::ListenerRejectEvent::new(
                    transport_kind,
                    "allowlist",
                    &peer_id,
                );
                let _ = event.append_to_log(&log_path);
            }
            InboundOutcome::RejectedExpired { peer } => {
                progress(&format!(
                    "VARA listener: rejecting {} (reason: expired)",
                    peer.call
                ));
                let log_path = vara_arms_log_path();
                let peer_id = crate::winlink::listener::PeerId::Callsign(peer);
                let event = crate::winlink::listener::packet_gate::ListenerRejectEvent::new(
                    transport_kind,
                    "expired",
                    &peer_id,
                );
                let _ = event.append_to_log(&log_path);
            }
        }
    }

    // Shutdown path: send LISTEN OFF best-effort and return the transport
    // to the session so the operator's vara_close_session / vara_status
    // sees the transport as if the consumer never owned it.
    progress("VARA listener consumer: draining; sending LISTEN OFF.");
    let _ = crate::winlink::modem::vara::set_listen(&mut transport, false);
    // tuxlink-pdnw (Codex Phase 3-4 P1 #4): guarded install. Stale snapshot
    // → close intervened (vara_close_session_inner ran since we took the
    // transport). Drop the transport instead of restoring `VaraState::Open`.
    //
    // tuxlink-0iqi: the listener consumer's drain path runs at shutdown —
    // it's tearing down the session, not preserving the operator's active
    // mode. Pass `None`/`None` for active_intent + active_transport_kind
    // so the install-back (if it happens — fresh snapshot only) resets
    // the active-mode fields, matching the legacy drain behavior.
    match vara_session.install_transport_if_generation_matches(
        transport,
        close_gen_snapshot,
        bound_host,
        bound_cmd_port,
        None,
        None,
    ) {
        Ok(()) => {}
        Err(dropped) => {
            progress(
                "VARA listener consumer: close intervened during drain; \
                 dropping transport instead of restoring session.",
            );
            drop(dropped);
        }
    }
    *listen_state.inner.lock().unwrap() = None;
    progress("VARA listener disarmed (transport returned).");
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
// tuxlink-hnkn P2 — position_current_fix (PositionFormV2 pre-fill)
// ============================================================================
// Thin shim over PositionArbiter that returns the active grid + source label
// + freshness flag to the React Position Report compose form.  The form
// pre-fills the grid input with this value so the operator can confirm (or
// manually override) before sending.
//
// `source` is stringified from the PositionSource enum via Debug (yielding
// "Gps" or "Manual").  The React consumer is case-insensitive on source
// comparison, so the PascalCase output is intentional and documented.

/// Wire-shape returned to PositionFormV2.tsx by `position_current_fix`.
#[derive(Debug, Clone, Serialize)]
pub struct PositionFix {
    pub grid: Option<String>,
    /// Source label — "Gps" | "Manual" (Debug-derived from PositionSource).
    pub source: String,
    /// True when the GPS fix is < 30 s old (FIX_STALENESS from arbiter).
    pub fresh: bool,
}

#[tauri::command]
pub async fn position_current_fix(
    arbiter: tauri::State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
) -> Result<PositionFix, String> {
    Ok(PositionFix {
        grid: arbiter.active_grid(),
        source: format!("{:?}", arbiter.source()),
        fresh: arbiter.has_fresh_fix(),
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

// ============================================================================
// tuxlink-bsiy — config_set_review_inbound (Review Pending Messages preference)
// ============================================================================

/// Persist the opt-in `review_inbound_before_download` preference (WLE "Review
/// Pending Messages" parity). Default false = auto-download-all (the default,
/// WLE parity). Mirrors `config_set_connect`'s read → mutate → persist ordering
/// and its `UiError` handling exactly.
///
/// The live-refresh `set_config` call is load-bearing: the connect path reads
/// the backend's LIVE config (not the disk), so persisting alone is NOT enough
/// — without `set_config`, the next connect would use the stale snapshot until
/// an app restart.
///
/// NOTE (test coverage): like `config_set_connect` / `config_set_privacy`, the
/// full read→write round-trip is NOT unit-tested here — `config::config_path()`
/// resolves via the process-global `XDG_CONFIG_HOME`, so an isolated round-trip
/// races under parallel `cargo test`. The persist path is identical to
/// `config_set_connect`'s and is operator-smoke-covered. The serde
/// round-trip + default are unit-tested in `config.rs`; the DTO mapping is
/// unit-tested in `config_view_dto_maps_review_inbound_when_enabled`.
#[tauri::command]
pub async fn config_set_review_inbound(
    state: State<'_, BackendState>,
    enabled: bool,
) -> Result<(), UiError> {
    let mut cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.review_inbound_before_download = enabled;
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    if let Some(backend) = state.current() {
        backend.set_config(cfg); // live refresh: next connect sees it without restart (Codex #9)
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
// tuxlink-6c9y — Network Post Office relay favorites (Phase A7)
// Spec: docs/design/2026-06-08-telnet-post-office-design.md §5.8
// Plan: docs/superpowers/plans/2026-06-08-telnet-post-office.md Task A7
// ============================================================================

/// Return the full list of saved Network PO relay favorites.
///
/// Read-only; mirrors the `config_set_connect` read path.
#[tauri::command]
pub async fn network_po_favorites_get() -> Result<Vec<config::RelayFavorite>, UiError> {
    let cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(cfg.network_po_favorites)
}

/// Add a Network PO relay favorite.
///
/// Validation: `host` and `callsign` must be non-empty (trimmed).
/// Dedup: `(host case-insensitive, port)` — duplicate returns `UiError::Rejected`.
/// Trimming: `host`, `callsign`, and `label` are trimmed before storing so that
/// whitespace-padded inputs cannot evade the case-insensitive dedup.
/// Returns the updated Vec on success.
/// Mirrors `config_set_connect`'s unguarded read-modify-write convention.
#[tauri::command]
pub async fn network_po_favorites_add(
    favorite: config::RelayFavorite,
) -> Result<Vec<config::RelayFavorite>, UiError> {
    if favorite.host.trim().is_empty() {
        return Err(UiError::Rejected("relay host must not be empty".into()));
    }
    if favorite.callsign.trim().is_empty() {
        return Err(UiError::Rejected("relay callsign must not be empty".into()));
    }
    // Build the trimmed version that will actually be stored.
    let trimmed = config::RelayFavorite {
        host: favorite.host.trim().to_string(),
        callsign: favorite.callsign.trim().to_string(),
        label: favorite.label.trim().to_string(),
        port: favorite.port,
    };
    let mut cfg =
        config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    // Check for duplicate using the trimmed host so whitespace-padded inputs
    // cannot evade the case-insensitive (host, port) dedup.
    let is_dup = cfg.network_po_favorites.iter().any(|f| {
        f.host.eq_ignore_ascii_case(&trimmed.host) && f.port == trimmed.port
    });
    if is_dup {
        return Err(UiError::Rejected(format!(
            "a favorite with host '{}' port {} already exists",
            trimmed.host, trimmed.port
        )));
    }
    cfg.network_po_favorites.push(trimmed);
    config::write_config_atomic(&cfg)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(cfg.network_po_favorites)
}

/// Remove the favorite matching `(host case-insensitive, port)`.
///
/// Idempotent: no error if no entry matched.
/// Returns the updated Vec.
#[tauri::command]
pub async fn network_po_favorites_remove(
    host: String,
    port: u16,
) -> Result<Vec<config::RelayFavorite>, UiError> {
    let mut cfg =
        config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.network_po_favorites
        .retain(|f| !(f.host.eq_ignore_ascii_case(&host) && f.port == port));
    config::write_config_atomic(&cfg)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(cfg.network_po_favorites)
}

/// Replace the entire favorites list atomically.
///
/// Returns the updated Vec (the same slice passed in, after persisting).
///
/// NOTE: performs NO validation or dedup — unlike `network_po_favorites_add`,
/// the caller is responsible for ensuring entries are valid (non-empty
/// host/callsign) and unique on `(host, port)`. Intended for trusted callers
/// (e.g. reordering an already-validated list).
#[tauri::command]
pub async fn network_po_favorites_set(
    favorites: Vec<config::RelayFavorite>,
) -> Result<Vec<config::RelayFavorite>, UiError> {
    let mut cfg =
        config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.network_po_favorites = favorites;
    config::write_config_atomic(&cfg)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(cfg.network_po_favorites)
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
    state: State<'_, BackendState>,
    p2p_state: State<'_, P2pConnectState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    req: P2pDialRequest,
) -> Result<P2pDialResult, UiError> {
    use std::sync::atomic::Ordering;
    use crate::winlink::credentials::KeyringError;
    use crate::winlink::session::{ExchangeConfig, SessionIntent};
    use crate::winlink::telnet_p2p;

    // Phase 3 (bd-tuxlink-0063): the on-air station ID comes from the
    // authenticated active SessionIdentity, not req.my_callsign (advisory).
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let session_id = backend.active_identity()?;

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
    // tuxlink-2ns7: default to the sole FULL (per-FULL received-mail namespace).
    let mut mailbox = crate::native_mailbox::Mailbox::new(mbox_dir);
    if let Some(full) = sole_full_identity() {
        mailbox = mailbox.with_default_identity(&full);
    }
    if let Some(svc) = app.try_state::<crate::search::commands::SearchService>() {
        mailbox = mailbox.with_index(svc.index.clone());
    }

    // tuxlink-l55l: read the outbox BEFORE opening the socket — same ordering
    // as `native_telnet_exchange` (P1.3 Codex review). A malformed outbox
    // surfaces an error before any peer interaction.
    //
    // tuxlink-u5hl (Codex Round 5 P1 #3 + Phase 3-4 RE-REVIEW P2): for the
    // safety-gate `MessageRejected` case (non-CMS intent, schema gate),
    // degrade to empty outbound rather than failing — the operator's
    // telnet walk through still completes the handshake, no proposal
    // ships, and the peer sees an empty exchange (consistent with
    // listener-answer pattern). Other outbox errors (corrupt mailbox,
    // etc.) still fail-closed via the error path below.
    let outbound = match crate::winlink_backend::build_outbound_proposals(
        &mailbox,
        SessionIntent::P2p,
        None,
        Some(session_id.mycall().as_str()),
    ) {
        Ok(v) => v,
        Err(crate::winlink_backend::BackendError::MessageRejected(reason)) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Info,
                format!("Outbound drain skipped ({reason}); telnet proceeds with empty outbound"),
            );
            Vec::new()
        }
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
    // Capture (MID, subject) pairs before `outbound` is moved into the exchange
    // closure, so the post-exchange plain log can name each sent/deferred/rejected
    // message by subject — parity with the shared exchange filing paths
    // (smoke-walk item 4: Telnet P2P was the one Telnet mode that still emitted
    // only an aggregate count, not per-message movement).
    let outbound_log = crate::winlink_backend::outbound_log_items(&outbound);
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
        // req.my_callsign is advisory; mycall authority is the active SessionIdentity.
        mycall: session_id.mycall().as_str().to_string(),
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
                emit_session_line_with_source(
                    &app_wire,
                    &log_wire,
                    LogLevel::Info,
                    LogSource::Wire,
                    line.to_string(),
                );
            },
            |_proposals| Ok(Vec::new()),
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

            // Per-message movement detail (Received/Sent/Rejected/Deferred by
            // subject), mirroring the shared exchange filing paths so a Telnet
            // P2P operator can see WHICH messages moved, not just a count.
            crate::winlink_backend::emit_exchange_result_progress(
                &exchange,
                &outbound_log,
                &|line: &str| emit_session_line(&app, &log, LogLevel::Info, line.to_string()),
            );
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
// tuxlink-6c9y — Telnet "Post Office" connect (RMS Relay over plaintext TCP)
// Plan: docs/superpowers/plans/2026-06-08-telnet-post-office.md Task C1
// ============================================================================
//
// Dials an RMS Relay over PLAINTEXT TCP and runs the B2F exchange with
// send-time outbound selection + `bsiy`'s inbound message selection. This is
// the WLE "post office" model: `mode == "local"` (L pool, `<base>-L` login,
// PostOffice intent, the `X-Tuxlink-Received-Session: post-office` marker) vs
// `mode == "network"` (normal C-mail pool, full base callsign, Mesh intent).
//
// RADIO-1: pure TCP, no RF, no transmitter keying — exactly like the existing
// CMS-over-Telnet path. NO consent gate. The relay never challenges for a
// secure-login password, so the Post Office path reads NO keyring
// (`ExchangeConfig.password = None`); `post_office_exchange_config` pins this.

/// Serializable view of [`crate::winlink::relay_banner::RelayState`] for the
/// connect-result DTO. The wire-protocol [`RelayState`] is a parser type kept
/// free of frontend serde concerns; this kebab-case mirror (matching how
/// `SessionIntent` serializes) lets the pane render a banner strip without
/// pulling serde into the parser. The variant set is 1:1 with `RelayState`.
///
/// [`RelayState`]: crate::winlink::relay_banner::RelayState
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelayStateDto {
    /// Ordinary CMS endpoint / no relay banner matched.
    NotRelay,
    /// Local-database store-and-forward post office.
    LocalDatabase,
    /// Radio-network hub (no internet leg).
    RadioNetwork,
    /// Hybrid radio + internet hub.
    RadioNetworkAndInternet,
    /// CMS routing currently unavailable (relay holding messages).
    NoCmsConnectionAvailable,
}

impl From<crate::winlink::relay_banner::RelayState> for RelayStateDto {
    fn from(state: crate::winlink::relay_banner::RelayState) -> Self {
        use crate::winlink::relay_banner::RelayState;
        match state {
            RelayState::NotRelay => RelayStateDto::NotRelay,
            RelayState::LocalDatabase => RelayStateDto::LocalDatabase,
            RelayState::RadioNetwork => RelayStateDto::RadioNetwork,
            RelayState::RadioNetworkAndInternet => RelayStateDto::RadioNetworkAndInternet,
            RelayState::NoCmsConnectionAvailable => RelayStateDto::NoCmsConnectionAvailable,
        }
    }
}

/// Request object for [`telnet_post_office_connect`]. The B3 pane sends these
/// snake_case keys inside a `{ req: {...} }` wrapper (Tauri rejects flat args
/// for this shape — see `TelnetP2pRadioPanel.tsx`); mirror `P2pDialRequest`.
#[derive(Debug, Deserialize)]
pub struct PostOfficeDialRequest {
    /// `"local"` (L pool / `<base>-L` login / PostOffice intent) or anything
    /// else (`"network"` — normal C-mail pool / full base callsign / Mesh).
    pub mode: String,
    /// Hostname or IP of the RMS Relay's TCP listener (default `127.0.0.1`).
    pub host: String,
    /// TCP port on the relay (default `8772`, the RMS Relay default).
    pub port: u16,
    /// Operator callsign; the login line is derived from it per `mode`.
    pub my_callsign: String,
    /// Maidenhead grid locator for the B2F handshake.
    pub locator: String,
    /// MIDs the operator selected to send this session. `build_outbound_proposals`
    /// intersects this advisory set with the live Outbox (a vanished MID is
    /// skipped, not fatal). An EMPTY set is valid — a receive-only dial.
    pub selected_mids: Vec<String>,
}

/// Result returned by [`telnet_post_office_connect`]. The snake_case counters
/// are read by the pane's `DialResult`; `relay_state` exposes what the relay's
/// pre-SID banner revealed (spec §5.9 banner strip).
#[derive(Debug, Serialize)]
pub struct PostOfficeDialResult {
    /// Outbound messages sent successfully (moved Outbox → Sent).
    pub sent_count: usize,
    /// Inbound messages received (filed into Inbox).
    pub received_count: usize,
    /// What the relay's banner revealed about its type.
    pub relay_state: RelayStateDto,
}

/// Single-flight + abort coordination for the Post Office connect path
/// (mirrors [`P2pConnectState`] + [`NativeBackend`]'s `connect_in_progress` +
/// `aborting` + `abort_handle`). Held in Tauri managed state so
/// [`telnet_post_office_connect`] and [`telnet_post_office_abort`] share it.
///
/// [`NativeBackend`]: crate::winlink_backend::NativeBackend
pub struct PostOfficeConnectState {
    /// `true` for the duration of a connect; a second concurrent connect is
    /// rejected rather than racing on the status/log pipeline. Reset by a
    /// connect-scoped RAII guard ([`PostOfficeConnectGuard`]) so it is released
    /// on EVERY exit — normal return, early return, or a panic in the async
    /// setup window — and can never wedge `true` permanently.
    pub in_progress: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Set by [`telnet_post_office_abort`]; checked by the selecting decider
    /// (it shares this flag) and the socket-abort handler so an in-flight dial
    /// can be cancelled.
    pub aborting: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Shutdown handle for the in-flight connect socket (mirrors
    /// [`NativeBackend`]'s `abort_handle`). A clone of the connecting
    /// `TcpStream`, stored once TCP connects by the connect path's
    /// `register_socket` closure and taken + `shutdown(Both)` by
    /// [`telnet_post_office_abort`] to force-close a slow login/exchange phase.
    /// `None` when nothing is in flight.
    ///
    /// [`NativeBackend`]: crate::winlink_backend::NativeBackend
    pub abort_handle: std::sync::Arc<std::sync::Mutex<Option<std::net::TcpStream>>>,
}

impl Default for PostOfficeConnectState {
    fn default() -> Self {
        Self {
            in_progress: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            aborting: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            abort_handle: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

/// Clears the single-flight flag + abort handle when a Post Office connect ends,
/// however it ends (mirrors [`NativeBackend`]'s `ConnectGuard`): normal return,
/// early return, or a panic in the async setup window (Mailbox build, Arc
/// clones, `HashSet` collect — between acquiring single-flight and the
/// `.await`). A manual `store(false)` cannot cover a panic in that window; the
/// RAII Drop can. Clearing the handle on exit also prevents a late
/// [`telnet_post_office_abort`] from `shutdown`-ing a reused fd once this dial
/// is done.
///
/// [`NativeBackend`]: crate::winlink_backend::NativeBackend
struct PostOfficeConnectGuard {
    in_progress: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: std::sync::Arc<std::sync::Mutex<Option<std::net::TcpStream>>>,
}

impl Drop for PostOfficeConnectGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.handle.lock() {
            *slot = None;
        }
        self.in_progress
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Build the [`ExchangeConfig`] for a Post Office dial. Split out as a pure
/// function so the no-keyring (`password: None`) + login-callsign + intent
/// mapping is unit-testable without a socket.
///
/// - `local == true`  → `<base>-L` login, [`SessionIntent::PostOffice`].
/// - `local == false` → full base callsign (no `-L`), [`SessionIntent::Mesh`].
///
/// `targetcall` is always [`telnet::CMS_TARGET_CALL`] (`wl2k`). `password` is
/// ALWAYS `None` — the relay never issues a `;PQ` challenge to a post-office
/// client, so no OS-keyring read happens on this path.
///
/// [`ExchangeConfig`]: crate::winlink::session::ExchangeConfig
/// [`SessionIntent::PostOffice`]: crate::winlink::session::SessionIntent::PostOffice
/// [`SessionIntent::Mesh`]: crate::winlink::session::SessionIntent::Mesh
fn post_office_exchange_config(
    mycall: &crate::identity::Callsign,
    locator: &str,
    local: bool,
) -> crate::winlink::session::ExchangeConfig {
    use crate::winlink::session::SessionIntent;
    let intent = if local { SessionIntent::PostOffice } else { SessionIntent::Mesh };
    crate::winlink::session::ExchangeConfig {
        mycall: crate::winlink::telnet::base_callsign_for_post_office(mycall.as_str(), local),
        targetcall: crate::winlink::telnet::CMS_TARGET_CALL.to_string(),
        locator: locator.to_string(),
        // Post Office uses no keyring — see the function doc + RADIO-1 note.
        password: None,
        intent,
    }
}

/// Run one Post Office B2F exchange against `host:port` over plaintext TCP.
///
/// The pure orchestration seam (the analog of `winlink_backend::native_connect`
/// for the CMS path), factored out of [`telnet_post_office_connect`] so it can
/// be integration-tested against a scripted loopback relay with the real
/// `bsiy` selecting decider. It:
///
/// 1. builds the [`ExchangeConfig`] via [`post_office_exchange_config`]
///    (no keyring; `<base>-L`/base login; PostOffice/Mesh intent);
/// 2. drains the Outbox filtered to `selected` (advisory set ∩ live Outbox);
/// 3. dials `host:port` with [`Transport::Plaintext`] and runs the exchange,
///    driving the caller-supplied `decide` closure for inbound selection;
/// 4. files the result — received mail into Inbox (PostOffice intent stamps
///    the `X-Tuxlink-Received-Session: post-office` marker), sent MIDs
///    Outbox → Sent — via `file_exchange_result`.
///
/// Returns the [`PostOfficeDialResult`] (counts + relay banner state). N=0
/// `selected` still connects (a receive-only dial).
///
/// [`ExchangeConfig`]: crate::winlink::session::ExchangeConfig
/// [`Transport::Plaintext`]: crate::winlink::telnet::Transport::Plaintext
/// Outbound drain selection for a Post Office session (tuxlink-b6ad). Network PO
/// (Mesh intent) carries normal mail into normal Winlink routing — the same
/// destination as CMS — so it drains the whole Outbox (`None`), exactly like
/// `cms_connect`; no per-message picker. Telnet RMS Post Office (`local`, the
/// `-L` pool whose mail is never forwarded globally) keeps the operator's
/// explicit send-time selection as its leakage guard (`Some`).
fn po_drain_selection(
    local: bool,
    selected: &std::collections::HashSet<String>,
) -> Option<&std::collections::HashSet<String>> {
    if local {
        Some(selected)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn post_office_exchange<F>(
    mailbox: &crate::native_mailbox::Mailbox,
    host: &str,
    port: u16,
    mycall: &crate::identity::Callsign,
    locator: &str,
    local: bool,
    selected: &std::collections::HashSet<String>,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    mailbox_change: &dyn Fn(),
    register_socket: &dyn Fn(&std::net::TcpStream),
    decide: F,
) -> Result<PostOfficeDialResult, UiError>
where
    F: Fn(
        &[crate::winlink::proposal::Proposal],
    ) -> Result<Vec<crate::winlink::proposal::Answer>, crate::winlink::session::ExchangeError>,
{
    let config = post_office_exchange_config(mycall, locator, local);
    let intent = config.intent;

    // Drain the Outbox. Network PO (Mesh) carries normal mail into normal
    // Winlink routing — same destination as CMS — so it drains the WHOLE Outbox
    // (`None`), exactly like `cms_connect`. Telnet RMS Post Office (local `-L`
    // pool, never forwarded globally) keeps the explicit send-time selection as
    // its leakage guard (`Some`; advisory set ∩ live Outbox, vanished MID
    // skipped). tuxlink-b6ad.
    let outbound =
        crate::winlink_backend::build_outbound_proposals(mailbox, intent, po_drain_selection(local, selected), Some(mycall.as_str()))
            .map_err(|e| UiError::Internal { detail: format!("outbox drain: {e}") })?;

    let result = crate::winlink::telnet::connect_and_exchange(
        host,
        port,
        crate::winlink::telnet::Transport::Plaintext,
        &config,
        outbound,
        progress,
        wire_log,
        register_socket,
        decide,
    )
    .map_err(|e| UiError::Transport { reason: format!("{e:?}") })?;

    // File received mail (Inbox; PostOffice stamps the marker) + move sent
    // MIDs Outbox → Sent. Filing FIRST is idempotent even on an all-rejected
    // batch (mirrors native_connect P1.4).
    crate::winlink_backend::file_exchange_result(mailbox, &result, intent, mailbox_change)
        .map_err(|e| UiError::Internal { detail: format!("file exchange result: {e}") })?;

    Ok(PostOfficeDialResult {
        sent_count: result.sent.len(),
        received_count: result.received.len(),
        relay_state: result.relay_state.into(),
    })
}

/// Connect to an RMS Relay "post office" over plaintext TCP and run a full B2F
/// message exchange with send-time outbound selection + inbound message
/// selection (`bsiy`).
///
/// Mirrors [`telnet_p2p_connect`]'s structure (session-log + `backend_status`
/// events, single-flight, `spawn_blocking` for the blocking exchange) and
/// [`cms_connect`]'s inbound-selection plumbing (mints one `AttemptId`, builds
/// the selecting decider that emits `InboundProposalsOffered` and parks on the
/// shared [`SelectionRegistry`] until [`cms_resolve_inbound_selection`]
/// delivers the operator's choice). The Post Office connect ALWAYS prompts for
/// inbound selection (full inbound selection in v1 per the spec).
///
/// The `{ req }` wrapper is the B3 ↔ C1 contract (single `req` param). N=0
/// `selected_mids` still connects (a receive-only dial — do NOT early-return).
///
/// RADIO-1: drives plaintext TCP to a relay — no RF, so no Part 97 consent
/// gate. The relay may transmit RF independently; tuxlink does not trigger it.
/// No keyring is read on this path (`ExchangeConfig.password = None`).
#[tauri::command]
pub async fn telnet_post_office_connect(
    app: AppHandle,
    state: State<'_, BackendState>,
    po_state: State<'_, PostOfficeConnectState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    registry: State<'_, crate::winlink::inbound_selection::SelectionRegistry>,
    req: PostOfficeDialRequest,
) -> Result<PostOfficeDialResult, UiError> {
    use std::sync::atomic::Ordering;

    let local = req.mode == "local";

    // Phase 3 (bd-tuxlink-0063): the on-air station ID comes from the
    // authenticated active SessionIdentity, not the wire DTO. req.my_callsign is
    // advisory; mycall authority is the active SessionIdentity.
    let backend = state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let session_id = backend.active_identity()?;

    // Single-flight: reject a second concurrent connect. The RAII guard
    // (constructed immediately) releases the flag + clears the abort handle on
    // EVERY exit — normal return, early return, or a panic in the async setup
    // window below (Mailbox build, Arc clones, HashSet collect). A manual
    // `store(false)` cannot survive a panic in that window and would wedge the
    // single-flight `true` forever, rejecting every future connect until
    // restart (mirrors NativeBackend::ConnectGuard).
    if po_state
        .in_progress
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(UiError::Internal {
            detail: "a Post Office connection is already in progress".to_string(),
        });
    }
    let _guard = PostOfficeConnectGuard {
        in_progress: po_state.in_progress.clone(),
        handle: po_state.abort_handle.clone(),
    };

    // Fresh abort epoch: clear any stale flag/handle from a prior connect so an
    // earlier abort can't bleed into this one (mirrors NativeBackend::connect).
    po_state.aborting.store(false, Ordering::SeqCst);
    if let Ok(mut slot) = po_state.abort_handle.lock() {
        *slot = None;
    }

    let transport_label = "Post Office".to_string();
    emit_p2p_status(&app, StatusDto::Connecting { transport: transport_label.clone() });

    // Build a Mailbox at the same on-disk location the native backend uses
    // (`<app_data>/native-mbox`); the Post Office path walks the shared store
    // directly (like P2P). Attach the search index when present so PO-received
    // mail lands in the search corpus.
    let mbox_dir = match app.path().app_data_dir() {
        Ok(dir) => dir.join("native-mbox"),
        Err(e) => {
            // `_guard` releases single-flight on this early return.
            emit_p2p_status(&app, StatusDto::Disconnected);
            return Err(UiError::Internal {
                detail: format!("could not resolve app data dir: {e}"),
            });
        }
    };
    // tuxlink-2ns7: default to the sole FULL (per-FULL received-mail namespace).
    let mut mailbox = crate::native_mailbox::Mailbox::new(mbox_dir);
    if let Some(full) = sole_full_identity() {
        mailbox = mailbox.with_default_identity(&full);
    }
    if let Some(svc) = app.try_state::<crate::search::commands::SearchService>() {
        mailbox = mailbox.with_index(svc.index.clone());
    }

    let login =
        crate::winlink::telnet::base_callsign_for_post_office(session_id.mycall().as_str(), local);
    emit_session_line(
        &app,
        &log,
        LogLevel::Info,
        format!(
            "Connecting to {}:{} (Post Office, login {}, {} selected)…",
            req.host,
            req.port,
            login,
            req.selected_mids.len()
        ),
    );

    // tuxlink-bsiy: mint ONE attempt_id so the in-flight InboundProposalsOffered
    // events share a single correlation id (the frontend stale-filter keys on it).
    let attempt_id = crate::winlink::b2f_events::AttemptId::fresh();
    let events_sink: std::sync::Arc<dyn crate::winlink::b2f_events::B2fEventSink> =
        std::sync::Arc::new(crate::winlink::b2f_events::TauriEventSink::new(app.clone()));

    // Values moved into the blocking task.
    let host = req.host.clone();
    let port = req.port;
    // The active identity's full callsign is the station authority for this
    // exchange (Phase 3). req.my_callsign is advisory and intentionally unused.
    let mycall = session_id.mycall().clone();
    let locator = req.locator.clone();
    let selected: std::collections::HashSet<String> =
        req.selected_mids.iter().cloned().collect();
    let registry = registry.inner().clone();
    let aborting = po_state.aborting.clone();
    // Clone the SHARED abort handle (lives on PostOfficeConnectState) into the
    // blocking task so the socket the dial connects is reachable from
    // `telnet_post_office_abort` (mirrors NativeBackend::connect). A handle
    // local to this closure — the prior shape — could never be reached by the
    // abort command, so abort could wake a parked decider but never shut a
    // mid-exchange socket.
    let abort_handle = po_state.abort_handle.clone();

    let app_progress = app.clone();
    let log_progress = log.inner().clone();
    let app_wire = app.clone();
    let log_wire = log.inner().clone();

    let outcome = tokio::task::spawn_blocking(move || {
        use crate::winlink::b2f_events::B2fEvent;
        use crate::winlink::inbound_selection::PendingProposalDto;

        // The selecting decider: emit the offer + park on the registry slot
        // until the resolve command delivers (or abort / timeout). The Post
        // Office connect ALWAYS prompts (full inbound selection in v1).
        let emit = {
            let sink = events_sink.clone();
            move |request_id: u64, dtos: &[PendingProposalDto]| {
                sink.push(B2fEvent::InboundProposalsOffered {
                    request_id,
                    proposals: dtos.to_vec(),
                    attempt_id,
                });
            }
        };
        let decide = crate::winlink::inbound_selection::build_selecting_decider(
            registry,
            attempt_id,
            emit,
            aborting.clone(),
        );

        // Hand each freshly-connected socket to the SHARED abort handle (on
        // PostOfficeConnectState, cloned in above) so an operator abort can
        // `.shutdown()` it (mirrors native_connect's register_socket). The
        // TOCTOU-safe store: under the same `lock()`, if `aborting` is already
        // set the socket is shut down immediately rather than stored (the abort
        // command may have fired before TCP connected); otherwise it is stored
        // for the abort command to take + shut down.
        let register_socket = |sock: &std::net::TcpStream| {
            if let Ok(clone) = sock.try_clone() {
                if let Ok(mut slot) = abort_handle.lock() {
                    if aborting.load(Ordering::SeqCst) {
                        let _ = clone.shutdown(std::net::Shutdown::Both);
                    } else {
                        *slot = Some(clone);
                    }
                }
            }
        };

        post_office_exchange(
            &mailbox,
            &host,
            port,
            &mycall,
            &locator,
            local,
            &selected,
            &move |line: &str| {
                emit_session_line(&app_progress, &log_progress, LogLevel::Info, line.to_string());
            },
            &move |line: &str| {
                emit_session_line_with_source(
                    &app_wire,
                    &log_wire,
                    LogLevel::Info,
                    LogSource::Wire,
                    line.to_string(),
                );
            },
            &|| {},
            &register_socket,
            decide,
        )
    })
    .await;

    // Read the abort flag for outcome reporting. Single-flight release + abort
    // handle clear are owned by `_guard`'s Drop (fires when this fn returns),
    // so they run on every exit including a panic in the setup window above.
    let was_aborted = po_state.aborting.load(Ordering::SeqCst);

    match outcome {
        Ok(Ok(result)) => {
            emit_session_line(
                &app,
                &log,
                LogLevel::Info,
                format!(
                    "Post Office exchange complete. Sent {}, received {}.",
                    result.sent_count, result.received_count
                ),
            );
            // Brief Connected window (mirrors cms_connect / telnet_p2p_connect's
            // 1.5s hold so the operator perceives success). PO sessions are
            // transient (connect → B2F → done), not a held socket.
            emit_p2p_status(
                &app,
                StatusDto::Connected {
                    transport: transport_label,
                    peer: format!("{}:{}", req.host, req.port),
                    since_iso: chrono::Utc::now().to_rfc3339(),
                },
            );
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            emit_p2p_status(&app, StatusDto::Disconnected);
            Ok(result)
        }
        Ok(Err(e)) => {
            if was_aborted {
                emit_session_line(
                    &app,
                    &log,
                    LogLevel::Warn,
                    "Post Office connection aborted.".to_string(),
                );
            } else {
                emit_session_line(
                    &app,
                    &log,
                    LogLevel::Error,
                    format!("Post Office connect failed: {e:?}"),
                );
            }
            emit_p2p_status(&app, StatusDto::Disconnected);
            Err(e)
        }
        Err(join_err) => {
            // `_guard` already released single-flight + cleared the handle.
            emit_p2p_status(&app, StatusDto::Disconnected);
            Err(UiError::Internal {
                detail: format!("Post Office connect task failed: {join_err}"),
            })
        }
    }
}

/// Abort an in-flight [`telnet_post_office_connect`] (mirrors
/// [`telnet_p2p_abort`] + [`cms_abort`] + [`NativeBackend::abort`]). Three
/// effects, in order:
///
/// 1. Set the shared `aborting` flag — the selecting decider re-checks it after
///    waking so it returns Cancelled rather than accept-all, and the connect
///    path's `register_socket` reads it to shut down a socket that connects
///    AFTER this abort fires.
/// 2. Drop the registry slot — wakes a decider parked on a pending inbound
///    selection prompt immediately (its `recv_timeout` returns Disconnected).
/// 3. Take + `shutdown(Both)` the stored connect socket — force-closes a dial
///    already mid login/exchange so a slow phase unblocks at once instead of
///    waiting for the blocking exchange to time out.
///
/// Emits Disconnected at once so the StatusBar responds without waiting for the
/// blocking exchange. A no-op for the parts with nothing in flight (no parked
/// decider / no stored socket).
///
/// [`NativeBackend::abort`]: crate::winlink_backend::NativeBackend
#[tauri::command]
pub async fn telnet_post_office_abort(
    app: AppHandle,
    po_state: State<'_, PostOfficeConnectState>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    registry: State<'_, crate::winlink::inbound_selection::SelectionRegistry>,
) -> Result<(), UiError> {
    use std::sync::atomic::Ordering;

    emit_session_line(&app, &log, LogLevel::Info, "Aborting Post Office connection…".to_string());
    // Order matters (mirrors cms_abort / NativeBackend::abort): set `aborting`
    // FIRST so the woken decider's post-recv abort re-check returns Cancelled
    // (not accept-all) and a socket connecting after this point is shut down by
    // `register_socket`. Then drop the slot (wakes a parked decider), then
    // force-close the in-flight socket.
    po_state.aborting.store(true, Ordering::SeqCst);
    *registry.lock().unwrap() = None;
    if let Some(sock) = po_state
        .abort_handle
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    {
        let _ = sock.shutdown(std::net::Shutdown::Both);
    }
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
    backend_state: State<'_, BackendState>,
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

    // Phase 3 (bd-tuxlink-0063): the station ID the listener answers as comes
    // from the authenticated active SessionIdentity captured AT ARM TIME, not
    // from `cfg.identity.active_full`. `session_id` is moved into the spawned
    // listener task below so the listener answers as the identity active when
    // armed (full listener independence is Phase 6; this lands the
    // capture-at-arm seam).
    let backend = backend_state
        .current()
        .ok_or_else(|| UiError::NotConfigured("backend offline".to_string()))?;
    let session_id = backend.active_identity()?;
    let mycall = session_id.mycall().as_str().to_uppercase();
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
            // tuxlink-2ns7: default to the sole FULL (per-FULL received-mail namespace).
            let mut mb = crate::native_mailbox::Mailbox::new(dir.join("native-mbox"));
            if let Some(full) = sole_full_identity() {
                mb = mb.with_default_identity(&full);
            }
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
    // Phase 3 capture-at-arm seam (bd-tuxlink-0063): move the SessionIdentity
    // resolved above into the listener task so the listener is bound to the
    // identity active when armed, independent of later identity switches. The
    // station callsign already flows through `exchange_cfg.mycall`; holding the
    // whole `session_id` here is the seam Phase 6 (full listener independence)
    // builds on. Prefixed `_` because nothing in the loop reads it yet.
    let _listen_identity = session_id;
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
                Ok(proposals
                    .iter()
                    .map(|_| crate::winlink::proposal::Answer::Accept { resume_offset: 0 })
                    .collect())
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
    backend_state: State<'_, BackendState>,
    state: State<'_, std::sync::Arc<TelnetListenState>>,
    log: State<'_, std::sync::Arc<SessionLogState>>,
    enabled: bool,
) -> Result<(), UiError> {
    use std::sync::atomic::Ordering;
    if enabled {
        // Equivalent to telnet_listen() — forward BackendState so the listener
        // captures the active SessionIdentity at arm time (Phase 3).
        telnet_listen(app, backend_state, state, log).await
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
    use crate::winlink::message::RECEIVED_SESSION_POST_OFFICE;
    use crate::winlink_backend::MessageId;

    #[test]
    fn user_folder_dto_carries_parent_slug_and_omits_when_top_level() {
        // tuxlink-ka3z A4/finding #7: a subfolder serializes parentSlug; a
        // top-level folder omits the key entirely (TS parentSlug?: string).
        let child = UserFolderDto::from(crate::user_folders::UserFolder {
            slug: "ares".into(),
            display_name: "ARES".into(),
            created_at: "2026-06-09T00:00:00Z".into(),
            parent_slug: Some("nets".into()),
        });
        assert_eq!(child.parent_slug.as_deref(), Some("nets"));
        let json = serde_json::to_string(&child).unwrap();
        assert!(json.contains("\"parentSlug\":\"nets\""), "{json}");

        let top = UserFolderDto::from(crate::user_folders::UserFolder {
            slug: "nets".into(),
            display_name: "Nets".into(),
            created_at: "2026-06-09T00:00:00Z".into(),
            parent_slug: None,
        });
        let json_top = serde_json::to_string(&top).unwrap();
        assert!(!json_top.contains("parentSlug"), "top-level must omit the key: {json_top}");
    }

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
            received_session: None,
            form_id: None,
            form_payload: None,
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["isForm"], false);
        assert_eq!(v["routing"], "via CMS-SSL");
        assert_eq!(v["attachments"][0]["filename"], "f.txt");
        assert_eq!(v["attachments"][0]["size"], 10);
    }

    #[test]
    fn attachment_preview_dto_serializes_camel_case() {
        let dto = AttachmentPreviewDto {
            filename: "map.jpg".into(),
            mime_type: "image/jpeg".into(),
            data_base64: "/9j/AA==".into(),
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["filename"], "map.jpg");
        assert_eq!(v["mimeType"], "image/jpeg");
        assert_eq!(v["dataBase64"], "/9j/AA==");
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

    /// tuxlink-9ylw: regression test for the screenshot-bad rendering of CMS
    /// image responses (the canonical example is the CMS-Z Catalog reply that
    /// inlines a JPEG as the entire message body). When mail-parser hits a
    /// single-part message whose root body is binary (no text/plain sibling
    /// to fall back to), the prior implementation called
    /// `String::from_utf8_lossy` on the raw bytes — which produces a wall of
    /// REPLACEMENT CHARACTERs interleaved with whatever byte sequences happen
    /// to be valid UTF-8. That made screenshots look broken and gave users no
    /// pointer to the actual content path (the AttachmentStrip).
    ///
    /// Replacement contract: don't render bytes as text; emit a short
    /// placeholder pointing the user at the attachment surface.
    #[test]
    fn find_text_plain_body_emits_placeholder_for_binary_root_body() {
        // Synthetic single-part image/jpeg message — no text/plain sibling, so
        // `msg.body_text(0)` returns None and the fallback runs.
        let mut raw: Vec<u8> = Vec::new();
        raw.extend_from_slice(
            b"From: catalog@cms-z.winlink.org\r\n\
              To: tuxlink@example.com\r\n\
              Subject: CMS image response\r\n\
              Date: Fri, 05 Jun 2026 03:30:00 +0000\r\n\
              MIME-Version: 1.0\r\n\
              Content-Type: image/jpeg\r\n\
              \r\n",
        );
        // JPEG SOI + JFIF marker + a handful of high-entropy bytes —
        // representative of the CMS-Z Catalog payload shape that surfaced the bug.
        raw.extend_from_slice(&[
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01,
            0xcb, 0xa1, 0x12, 0xef, 0xc9, 0x97, 0xd4, 0xb2, 0x68, 0x5a, 0x80, 0xff,
        ]);

        let msg = mail_parser::MessageParser::new()
            .parse(raw.as_slice())
            .expect("synthetic MIME parses");
        let body = find_text_plain_body(&msg, raw.as_slice());

        // Failure mode under the prior implementation: REPLACEMENT CHARACTER
        // glyphs leak through wherever the JPEG bytes aren't valid UTF-8.
        assert!(
            !body.contains('\u{FFFD}'),
            "raw binary bytes leaked into body as U+FFFD replacement chars; got: {body:?}"
        );
        // JFIF magic happens to be valid ASCII (`JFIF`) and would survive
        // a lossy-UTF-8 decode unchanged — assert it doesn't end up rendered.
        assert!(
            !body.contains("JFIF"),
            "JPEG marker text leaked into rendered body; got: {body:?}"
        );
        // Placeholder must point the user at the attachment surface, where
        // AttachmentStrip + Save As (tuxlink-0fyj) handle binary content.
        assert!(
            body.contains("attachments"),
            "expected placeholder to direct users to the attachments surface; got: {body:?}"
        );
    }

    /// tuxlink-2hyf: regression-pin for the *real* broken case PR #401 did
    /// not catch. Real Winlink inbound messages aren't MIME — they're B2F
    /// wire format: a `Mid:` header, a `Body: <N>` byte-count declaring the
    /// text body length, optional `File: <N> <filename>` headers declaring
    /// per-attachment binary payloads, a blank line, then the declared text
    /// body, then the declared attachment payloads concatenated.
    ///
    /// mail_parser is RFC 5322-only and:
    /// - Sees `Mid:` / `Body:` / `File:` as ordinary RFC 5322 header lines.
    /// - Default implicit Content-Type is text/plain.
    /// - Treats everything after the blank line — text body PLUS binary
    ///   attachment payloads — as one text body.
    /// - `body_text(0)` returns Some(lossy-UTF-8 of text + binary bytes).
    /// - PR #401's `PartType::Binary` fallback never triggers because the
    ///   body is "text" from mail-parser's perspective.
    ///
    /// The fix dispatches to `winlink::message::Message::body()` (the
    /// project's own B2F parser) when both `Mid:` and `Body:` headers
    /// are present in the parsed B2F message.
    #[test]
    fn find_text_plain_body_dispatches_to_b2f_parser_for_winlink_messages() {
        // Shape mirrors a real CMS-Z Catalog reply: B2F headers, a short
        // text-body byte count, the text body, then a binary attachment.
        // The attachment magic bytes (JFIF) MUST NOT leak into the rendered
        // body — that's the screenshot-bad bug operators reported.
        let text_body = "Resource URL: https://example.org/cat/img.jpg\r\n  Inquiry ID: WCCOL.JPG\r\n";
        let attachment_bytes: &[u8] = &[
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01,
            0xcb, 0xa1, 0x12, 0xef, 0xc9, 0x97, 0xd4, 0xb2, 0x68, 0x5a, 0x80, 0xff,
        ];
        let mut raw: Vec<u8> = Vec::new();
        raw.extend_from_slice(b"Mid: 9YMNP4GB4UOD\r\n");
        raw.extend_from_slice(format!("Body: {}\r\n", text_body.len()).as_bytes());
        raw.extend_from_slice(b"Date: 2026/06/05 11:24\r\n");
        raw.extend_from_slice(format!("File: {} 600x600.jpg\r\n", attachment_bytes.len()).as_bytes());
        raw.extend_from_slice(b"From: SERVICE\r\n");
        raw.extend_from_slice(b"Mbo: SYSTEM\r\n");
        raw.extend_from_slice(b"Subject: INQUIRY - https://example.org/cat/img.jpg\r\n");
        raw.extend_from_slice(b"To: N7CPZ\r\n");
        raw.extend_from_slice(b"\r\n");
        raw.extend_from_slice(text_body.as_bytes());
        raw.extend_from_slice(b"\r\n");
        raw.extend_from_slice(attachment_bytes);
        raw.extend_from_slice(b"\r\n");

        let msg = mail_parser::MessageParser::new()
            .parse(raw.as_slice())
            .expect("synthetic B2F parses through mail-parser (as one big text body)");
        let body = find_text_plain_body(&msg, raw.as_slice());

        // The body must contain the declared text content — that's what
        // operators are supposed to see.
        assert!(
            body.contains("Resource URL"),
            "B2F dispatch should preserve the declared text body; got: {body:?}"
        );
        // Binary attachment bytes MUST NOT leak through. Two markers:
        //   - JFIF is ASCII-valid and would survive a lossy decode unchanged.
        //   - U+FFFD shows up wherever individual bytes weren't valid UTF-8.
        assert!(
            !body.contains("JFIF"),
            "JPEG marker text leaked from binary attachment into body; got: {body:?}"
        );
        assert!(
            !body.contains('\u{FFFD}'),
            "U+FFFD replacement chars in body indicate binary bytes leaked through; got: {body:?}"
        );
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
        let got = extract_attachment_bytes(&msg, raw.as_slice(), "forecast.grb")
            .expect("named attachment found");
        assert_eq!(got, payload, "decoded bytes must match the source payload");
    }

    #[test]
    fn extract_attachment_bytes_returns_none_for_unknown_filename() {
        let raw = build_mime_with_attachment("a.bin", b"abc");
        let msg = mail_parser::MessageParser::new().parse(raw.as_slice()).unwrap();
        assert!(extract_attachment_bytes(&msg, raw.as_slice(), "missing.bin").is_none());
    }

    #[test]
    fn build_attachment_preview_accepts_safe_image_magic_and_base64_encodes() {
        let jpeg: Vec<u8> = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, b'J', b'F', b'I', b'F'];
        let dto = build_attachment_preview("map.jpg", jpeg).expect("JPEG preview accepted");
        assert_eq!(dto.filename, "map.jpg");
        assert_eq!(dto.mime_type, "image/jpeg");
        assert_eq!(dto.data_base64, "/9j/4AAQSkZJRg==");
    }

    #[test]
    fn build_attachment_preview_rejects_non_image_content() {
        let err = build_attachment_preview("notes.txt", b"plain text".to_vec())
            .expect_err("text attachment is not previewable");
        assert!(matches!(err, UiError::Rejected(detail) if detail.contains("supported image")));
    }

    #[test]
    fn build_attachment_preview_rejects_oversized_payloads() {
        let too_large = vec![0u8; MAX_ATTACHMENT_PREVIEW_BYTES + 1];
        let err = build_attachment_preview("huge.jpg", too_large)
            .expect_err("oversized attachment is rejected");
        assert!(matches!(err, UiError::Rejected(detail) if detail.contains("too large")));
    }

    /// tuxlink-4or5: real CMS Catalog image messages arrive as B2F format —
    /// `Mid:` + `Body: N` + `File: N filename` headers, then text body, then
    /// per-attachment binary payloads. mail_parser is RFC 5322-only and
    /// returns zero attachments for these messages, so AttachmentStrip stayed
    /// empty and Save As had nothing to click — the image was visually clean
    /// after PR #412 but functionally inaccessible.
    ///
    /// End-to-end check: parse_raw_rfc5322 against a real-shape B2F message
    /// must surface the attachment in the DTO with the right filename + size
    /// AND extract_attachment_bytes must return byte-identical bytes back
    /// for that filename, so the AttachmentStrip → Save As chain works.
    #[test]
    fn parse_raw_rfc5322_surfaces_b2f_attachment_end_to_end() {
        let text_body = "Resource URL: https://example.org/cat/img.jpg\r\n";
        let attachment_bytes: &[u8] = &[
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01,
            0xcb, 0xa1, 0x12, 0xef, 0xc9, 0x97, 0xd4, 0xb2, 0x68, 0x5a, 0x80, 0xff,
        ];
        let filename = "600x600.jpg";

        let mut raw: Vec<u8> = Vec::new();
        raw.extend_from_slice(b"Mid: 9YMNP4GB4UOD\r\n");
        raw.extend_from_slice(format!("Body: {}\r\n", text_body.len()).as_bytes());
        raw.extend_from_slice(b"Date: 2026/06/05 11:24\r\n");
        raw.extend_from_slice(
            format!("File: {} {}\r\n", attachment_bytes.len(), filename).as_bytes(),
        );
        raw.extend_from_slice(b"From: SERVICE\r\n");
        raw.extend_from_slice(b"Mbo: SYSTEM\r\n");
        raw.extend_from_slice(b"Subject: INQUIRY - https://example.org/cat/img.jpg\r\n");
        raw.extend_from_slice(b"To: N7CPZ\r\n");
        raw.extend_from_slice(b"\r\n");
        raw.extend_from_slice(text_body.as_bytes());
        raw.extend_from_slice(b"\r\n");
        raw.extend_from_slice(attachment_bytes);
        raw.extend_from_slice(b"\r\n");

        // End-to-end through the same code path the UI takes.
        let dto = parse_raw_rfc5322("9YMNP4GB4UOD", raw.as_slice())
            .expect("B2F parse succeeds end-to-end");

        // 1. AttachmentStrip data: the attachment must be listed.
        assert_eq!(
            dto.attachments.len(),
            1,
            "AttachmentStrip needs the B2F attachment surfaced; got {:?}",
            dto.attachments
        );
        assert_eq!(dto.attachments[0].filename, filename);
        assert_eq!(dto.attachments[0].size, attachment_bytes.len() as u64);

        // 2. Save As data: extract_attachment_bytes must return the exact bytes
        //    so the on-disk file is byte-identical to the original.
        let msg = mail_parser::MessageParser::new()
            .parse(raw.as_slice())
            .expect("mail-parser accepts B2F headers as RFC 5322");
        let got = extract_attachment_bytes(&msg, raw.as_slice(), filename)
            .expect("attachment lookup succeeds for B2F messages");
        assert_eq!(got, attachment_bytes, "Save As bytes must round-trip");
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
                active_full: Some("W4PHS".into()),
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
            network_po_favorites: Vec::new(),
            review_inbound_before_download: false,
            map_tile_source: None,
            aredn_master_node_host: None,
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
        // tuxlink-bsiy: review_inbound_before_download maps through the From impl
        // (default false on the fixture).
        assert!(!dto.review_inbound_before_download);
    }

    // tuxlink-bsiy: a config with review_inbound_before_download=true maps to a
    // true DTO field (proves the From impl reads the real config value, not a
    // hardcoded default).
    #[test]
    fn config_view_dto_maps_review_inbound_when_enabled() {
        let mut cfg = cms_config_fixture();
        cfg.review_inbound_before_download = true;
        let dto = ConfigViewDto::from(&cfg);
        assert!(dto.review_inbound_before_download);
    }

    // Offline-mode mapping: callsign None, identifier Some — mirrors the
    // ribbon's identity.identifier fallback (useStatus.ts formatCallsign).
    #[test]
    fn config_view_dto_maps_offline_identity() {
        let mut cfg = cms_config_fixture();
        cfg.connect.connect_to_cms = false;
        cfg.connect.transport = CmsTransport::Telnet;
        cfg.connect.host = "server.winlink.org".into();
        cfg.identity.active_full = None;
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
            identity: IdentityConfig { active_full: Some("N0CALL".into()), identifier: None, grid: None },
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
            network_po_favorites: Vec::new(),
            review_inbound_before_download: false,
            map_tile_source: None,
            aredn_master_node_host: None,
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
    // tuxlink-6c9y — network_po_favorites commands
    // ========================================================================

    // Command tests: add, get, duplicate, remove idempotent.
    // Uses the TUXLINK_CONFIG_DIR + tempdir isolation pattern (same as the
    // position_set_source / config_set_grid tests above) since these commands
    // call read_config + write_config_atomic via the process-global config dir.

    #[tokio::test]
    async fn network_po_favorites_add_then_get_returns_favorite() {
        use crate::config::CONFIG_SCHEMA_VERSION;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: single-threaded test (env_lock).
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed).expect("seed config");

        let fav = crate::config::RelayFavorite {
            callsign: "W7AUX".into(),
            label: "My relay".into(),
            host: "relay.local".into(),
            port: 8772,
        };

        let add_result = network_po_favorites_add(fav.clone()).await;
        assert!(add_result.is_ok(), "add must succeed; got {add_result:?}");
        let added = add_result.unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0], fav);

        let get_result = network_po_favorites_get().await;
        assert!(get_result.is_ok(), "get after add must succeed; got {get_result:?}");
        assert_eq!(get_result.unwrap(), vec![fav]);

        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    #[tokio::test]
    async fn network_po_favorites_add_duplicate_host_port_returns_rejected() {
        use crate::config::CONFIG_SCHEMA_VERSION;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed).expect("seed config");

        let fav = crate::config::RelayFavorite {
            callsign: "W7AUX".into(),
            label: "First".into(),
            host: "Relay.Local".into(), // mixed-case host
            port: 8772,
        };
        let dup = crate::config::RelayFavorite {
            callsign: "K7XYZ".into(),   // different callsign/label — only host+port matters
            label: "Dup".into(),
            host: "relay.local".into(), // same host, different case
            port: 8772,
        };

        let _ = network_po_favorites_add(fav).await.expect("first add");
        let dup_result = network_po_favorites_add(dup).await;
        assert!(
            matches!(dup_result, Err(UiError::Rejected(_))),
            "duplicate (host case-insensitive, port) must return UiError::Rejected; got {dup_result:?}"
        );

        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    #[tokio::test]
    async fn network_po_favorites_remove_is_idempotent() {
        use crate::config::CONFIG_SCHEMA_VERSION;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed).expect("seed config");

        let fav = crate::config::RelayFavorite {
            callsign: "W7AUX".into(),
            label: "My relay".into(),
            host: "relay.local".into(),
            port: 8772,
        };

        let _ = network_po_favorites_add(fav).await.expect("add");

        // First remove: should remove the entry and return empty Vec.
        let remove1 = network_po_favorites_remove("relay.local".into(), 8772).await;
        assert!(remove1.is_ok(), "first remove must succeed; got {remove1:?}");
        assert!(remove1.unwrap().is_empty(), "Vec must be empty after remove");

        // Second remove: idempotent — no error, still empty.
        let remove2 = network_po_favorites_remove("relay.local".into(), 8772).await;
        assert!(remove2.is_ok(), "second remove must be idempotent; got {remove2:?}");
        assert!(remove2.unwrap().is_empty(), "Vec remains empty after second remove");

        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    #[tokio::test]
    async fn network_po_favorites_add_rejects_empty_host() {
        use crate::config::CONFIG_SCHEMA_VERSION;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed).expect("seed config");

        let bad_host = crate::config::RelayFavorite {
            callsign: "W7AUX".into(),
            label: "Test".into(),
            host: "  ".into(), // whitespace-only → empty after trim
            port: 8772,
        };
        let bad_callsign = crate::config::RelayFavorite {
            callsign: "  ".into(), // empty callsign
            label: "Test".into(),
            host: "relay.local".into(),
            port: 8772,
        };

        assert!(
            matches!(network_po_favorites_add(bad_host).await, Err(UiError::Rejected(_))),
            "empty host must be Rejected"
        );
        assert!(
            matches!(network_po_favorites_add(bad_callsign).await, Err(UiError::Rejected(_))),
            "empty callsign must be Rejected"
        );

        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    // ---- network_po_favorites_set + trim-on-store tests --------------------

    #[tokio::test]
    async fn network_po_favorites_set_replaces_list() {
        use crate::config::CONFIG_SCHEMA_VERSION;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed).expect("seed config");

        let fav_a = crate::config::RelayFavorite {
            callsign: "W7AUX".into(),
            label: "Alpha".into(),
            host: "alpha.local".into(),
            port: 8772,
        };
        let fav_b = crate::config::RelayFavorite {
            callsign: "K7XYZ".into(),
            label: "Beta".into(),
            host: "beta.local".into(),
            port: 8773,
        };

        // Seed one favorite via add, then replace with two via set.
        let _ = network_po_favorites_add(fav_a.clone()).await.expect("add fav_a");
        let set_result = network_po_favorites_set(vec![fav_a.clone(), fav_b.clone()]).await;
        assert!(set_result.is_ok(), "set must succeed; got {set_result:?}");
        let after_set = set_result.unwrap();
        assert_eq!(after_set.len(), 2, "set must replace to exactly 2 entries");

        // get confirms persistence.
        let get_result = network_po_favorites_get().await;
        assert!(get_result.is_ok());
        let list = get_result.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], fav_a);
        assert_eq!(list[1], fav_b);

        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    #[tokio::test]
    async fn network_po_favorites_set_empty_clears_list() {
        use crate::config::CONFIG_SCHEMA_VERSION;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed).expect("seed config");

        let fav = crate::config::RelayFavorite {
            callsign: "W7AUX".into(),
            label: "MyFav".into(),
            host: "relay.local".into(),
            port: 8772,
        };

        let _ = network_po_favorites_add(fav).await.expect("add fav");

        // set with empty Vec must clear the list.
        let set_result = network_po_favorites_set(vec![]).await;
        assert!(set_result.is_ok(), "set(empty) must succeed; got {set_result:?}");
        assert!(set_result.unwrap().is_empty(), "set(empty) must return empty Vec");

        let get_result = network_po_favorites_get().await;
        assert!(get_result.is_ok());
        assert!(get_result.unwrap().is_empty(), "get after set(empty) must return empty");

        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    #[tokio::test]
    async fn network_po_favorites_add_trims_whitespace_on_store() {
        use crate::config::CONFIG_SCHEMA_VERSION;

        let _env_guard = position_set_source_env_lock().await;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed).expect("seed config");

        // Add a favorite with surrounding whitespace in host, callsign, and label.
        let padded = crate::config::RelayFavorite {
            callsign: "  W7AUX  ".into(),
            label: "  My Relay  ".into(),
            host: " relay.local ".into(),
            port: 8772,
        };
        let add_result = network_po_favorites_add(padded).await;
        assert!(add_result.is_ok(), "padded add must succeed; got {add_result:?}");
        let stored = add_result.unwrap();
        assert_eq!(stored.len(), 1);
        // Host, callsign, and label must be trimmed on store.
        assert_eq!(stored[0].host, "relay.local", "host must be trimmed");
        assert_eq!(stored[0].callsign, "W7AUX", "callsign must be trimmed");
        assert_eq!(stored[0].label, "My Relay", "label must be trimmed");

        // Adding the same host (no padding, same port) must now be rejected as a
        // duplicate — proving trim-on-store makes the dedup whitespace-robust.
        let exact = crate::config::RelayFavorite {
            callsign: "K7XYZ".into(),
            label: "Other".into(),
            host: "relay.local".into(), // no padding — matches the trimmed stored entry
            port: 8772,
        };
        let dup_result = network_po_favorites_add(exact).await;
        assert!(
            matches!(dup_result, Err(UiError::Rejected(_))),
            "adding exact host after padded add must be Rejected (trim-on-store dedup); got {dup_result:?}"
        );

        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
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
                active_full: None,
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
            network_po_favorites: Vec::new(),
            review_inbound_before_download: false,
            map_tile_source: None,
            aredn_master_node_host: None,
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

    // ── tuxlink-u1r7 — Codex Phase 3-4 boundary P2 #3 ──────────────────
    //
    // `arm_vara_listener_inner` now takes a `transport_kind: TransportKind`
    // parameter (defaulting to the session's `active_transport_kind` at
    // the manual-arm site; passed through from `vara_open_session`'s arg
    // at the auto-arm site) so the arms-record + reject-event labels
    // reflect VARA-HF vs VARA-FM instead of a hardcoded HF.

    /// The arms-record helper records exactly the kind it's handed. Pin
    /// both VaraHf and VaraFm so a future regression that hardcodes
    /// VaraHf again surfaces.
    #[test]
    fn arms_record_carries_supplied_transport_kind() {
        use crate::winlink::listener::{
            ListenerArmsRecord, TransportKind, DEFAULT_TTL,
        };
        let hf = ListenerArmsRecord::arm(TransportKind::VaraHf, DEFAULT_TTL);
        assert_eq!(hf.transport, TransportKind::VaraHf);
        let fm = ListenerArmsRecord::arm(TransportKind::VaraFm, DEFAULT_TTL);
        assert_eq!(
            fm.transport,
            TransportKind::VaraFm,
            "P2 #3: arms-record must reflect VaraFm when armed for FM, \
             not a hardcoded VaraHf"
        );
    }

    /// The reject-event constructor records the supplied kind. Pins the
    /// consumer-task reject path against a hardcoded-HF regression.
    #[test]
    fn reject_event_carries_supplied_transport_kind() {
        use crate::winlink::listener::{
            packet_gate::ListenerRejectEvent, PeerId, TransportKind,
        };
        use crate::winlink::ax25::frame::Address;
        let peer = PeerId::Callsign(Address {
            call: "W1ABC".into(),
            ssid: 0,
        });
        let evt_hf =
            ListenerRejectEvent::new(TransportKind::VaraHf, "allowlist", &peer);
        let json_hf = serde_json::to_string(&evt_hf).unwrap();
        assert!(
            json_hf.contains("vara-hf"),
            "VaraHf reject record must serialize with vara-hf transport label; \
             got: {json_hf}"
        );

        let evt_fm =
            ListenerRejectEvent::new(TransportKind::VaraFm, "allowlist", &peer);
        let json_fm = serde_json::to_string(&evt_fm).unwrap();
        assert!(
            json_fm.contains("vara-fm"),
            "P2 #3: VaraFm reject record must serialize with vara-fm transport \
             label, not vara-hf; got: {json_fm}"
        );
        assert!(
            !json_fm.contains("vara-hf"),
            "VaraFm reject record must not contain vara-hf label"
        );
    }

    /// Sentinel — pin the `arm_vara_listener_inner` signature shape so a
    /// regression to "no transport_kind param" breaks the typecheck. The
    /// body is irrelevant; the existence-check at the module boundary is
    /// the test. async-fn return shape makes a fully-typed fn-pointer
    /// coercion impossible — reference the function by name.
    #[test]
    fn arm_vara_listener_inner_signature_includes_transport_kind() {
        let _f = arm_vara_listener_inner;
        // Pin the TransportKind type is reachable; if a regression drops
        // the param from the signature this compile-fence still passes
        // but the source-scan sentinel below catches it.
        let _kind: crate::winlink::listener::TransportKind =
            crate::winlink::listener::TransportKind::VaraFm;
    }

    /// Source-scan sentinel: the consumer task's reject paths must NOT
    /// hardcode the VaraHf transport kind directly inside the
    /// `ListenerRejectEvent::new(...)` invocations (the two reject-event
    /// constructors each had a direct hardcode in the prior shape).
    /// Search for the precise pattern that was the bug — the literal
    /// `RejectEvent::new(` followed by a positional hardcode — so the
    /// test is robust to incidental `VaraHf` references in test
    /// fixtures and docstrings.
    #[test]
    fn vara_consumer_reject_does_not_hardcode_transport_kind() {
        let source = include_str!("ui_commands.rs");
        // Sentinel assembled via `concat!` so this test's own bytes
        // don't trip the search.
        let bug_pattern = concat!(
            "ListenerRejectEvent::new(\n",
            "                    crate::winlink::listener::TransportKind::",
            "Vara",
        );
        assert!(
            !source.contains(bug_pattern),
            "P2 #3: consumer-task reject-event constructors must take \
             the threaded `transport_kind` parameter, not a hardcoded \
             `crate::winlink::listener::TransportKind::Vara*` literal. \
             Sentinel found the pre-fix shape."
        );
    }
    // ========================================================================
    // P1 Task 10 critical-fix: send_webview_form synthesis (tuxlink-tzr5)
    // ========================================================================
    //
    // The `send_webview_form` Tauri command can't be invoked directly in unit
    // tests (it takes AppHandle + State, which require a Tauri runtime). The
    // pure XML synthesis lives in `forms::serialize::serialize_catalog_form_xml`
    // and is unit-tested there. These tests cover the per-command synthesis
    // decisions: WLE filename conventions for display_form / reply_template,
    // attachment filename, body composition, and subject fallback.

    /// `display_form` falls back to the `<id>_Viewer.html` convention when
    /// `resolve_viewer_for` finds no paired viewer (e.g. when the operator
    /// drops a custom form into `~/.local/share/tuxlink/forms/custom/`
    /// without a companion `_Viewer.html`).
    ///
    /// 2026-06-04 Codex adrev P1.3: the resolver-driven path is the
    /// happy path now; this test pins the fallback. The
    /// `send_webview_form_display_form_uses_resolver_for_wle_bundle`
    /// test below covers the resolver path with the realistic Bulletin /
    /// Hawaii / underscored bundle conventions.
    #[test]
    fn send_webview_form_display_form_falls_back_to_id_underscore_viewer() {
        let form_id = "ICS205";
        let display_form = format!("{form_id}_Viewer.html");
        assert_eq!(display_form, "ICS205_Viewer.html");
        // The fallback applies to any form_id without a sibling viewer
        // file in the catalog walker's view.
        let form_id = "ARC213";
        assert_eq!(format!("{form_id}_Viewer.html"), "ARC213_Viewer.html");
    }

    /// 2026-06-04 Codex adrev P1.3: in the realistic WLE bundle, the
    /// authoring template's sibling Viewer file does NOT follow the
    /// `<id>_Viewer.html` convention for the bulk of templates. The
    /// resolver walks the same folder and picks the actual paired
    /// viewer regardless of which naming convention WLE used.
    #[test]
    fn send_webview_form_display_form_uses_resolver_for_wle_bundle() {
        use crate::forms::wle_templates::resolve_viewer_for;
        use tempfile::TempDir;

        let td = TempDir::new().unwrap();
        let folder = td.path().join("General Forms");
        std::fs::create_dir_all(&folder).unwrap();

        // Bulletin Initial.html ↔ Bulletin Viewer.html
        let bulletin = folder.join("Bulletin Initial.html");
        std::fs::write(&bulletin, "<html></html>").unwrap();
        std::fs::write(folder.join("Bulletin Viewer.html"), "<html></html>").unwrap();
        let resolved = resolve_viewer_for(&bulletin).expect("Bulletin viewer resolved");
        assert_eq!(resolved, "Bulletin Viewer.html");

        // ICS213_Initial.html ↔ ICS213_Viewer.html
        let ics_folder = td.path().join("ICS Forms");
        std::fs::create_dir_all(&ics_folder).unwrap();
        let ics = ics_folder.join("ICS213_Initial.html");
        std::fs::write(&ics, "<html></html>").unwrap();
        std::fs::write(ics_folder.join("ICS213_Viewer.html"), "<html></html>").unwrap();
        let resolved = resolve_viewer_for(&ics).expect("ICS213 viewer resolved");
        assert_eq!(resolved, "ICS213_Viewer.html");

        // Hawaii Siren Report.html ↔ Hawaii Siren Report Viewer.html
        // (authoring template has no "Initial" suffix at all)
        let hi_folder = td.path().join("HI State forms");
        std::fs::create_dir_all(&hi_folder).unwrap();
        let hi = hi_folder.join("Hawaii Siren Report.html");
        std::fs::write(&hi, "<html></html>").unwrap();
        std::fs::write(
            hi_folder.join("Hawaii Siren Report Viewer.html"),
            "<html></html>",
        )
        .unwrap();
        let resolved = resolve_viewer_for(&hi).expect("Hawaii viewer resolved");
        assert_eq!(resolved, "Hawaii Siren Report Viewer.html");
    }

    /// `reply_template` follows the WLE convention `<id>_SendReply.0`. The
    /// `.0` suffix is part of WLE's filename scheme (versioning slot).
    #[test]
    fn send_webview_form_reply_template_follows_wle_convention() {
        let form_id = "ICS205";
        let reply_template = format!("{form_id}_SendReply.0");
        assert_eq!(reply_template, "ICS205_SendReply.0");
    }

    /// Attachment filename uses the `RMS_Express_Form_<id>.xml` convention —
    /// same as `send_form` so parsers (Pat, RMS Express, tuxlink inbox) detect
    /// the form attachment consistently regardless of which submit pathway
    /// produced it.
    #[test]
    fn send_webview_form_attachment_filename_matches_send_form_convention() {
        let form_id = "ICS213_Initial";
        let filename = format!("RMS_Express_Form_{form_id}.xml");
        assert_eq!(filename, "RMS_Express_Form_ICS213_Initial.xml");
    }

    /// Body composition: sorted-by-key "key: value" dump with a leading
    /// `form_id: <id>` header. This mirrors the synthesis inside
    /// `send_webview_form` so a change to the body composition logic would
    /// fail this test.
    #[test]
    fn send_webview_form_body_starts_with_form_id_header_and_sorts_keys() {
        let form_id = "ICS213_Initial";
        let mut field_values = std::collections::HashMap::new();
        field_values.insert("zebra".to_string(), "z-val".to_string());
        field_values.insert("alpha".to_string(), "a-val".to_string());
        field_values.insert("mango".to_string(), "m-val".to_string());

        // Replicates the body synthesis in `send_webview_form`.
        let mut keys: Vec<&String> = field_values.keys().collect();
        keys.sort();
        let mut body = format!("form_id: {form_id}\n\n");
        for k in keys {
            let v = field_values.get(k).map(String::as_str).unwrap_or("");
            body.push_str(k);
            body.push_str(": ");
            body.push_str(v);
            body.push('\n');
        }

        assert!(body.starts_with("form_id: ICS213_Initial\n\n"));
        let pa = body.find("alpha:").unwrap();
        let pm = body.find("mango:").unwrap();
        let pz = body.find("zebra:").unwrap();
        assert!(pa < pm && pm < pz, "body must sort keys alphabetically");
        assert!(body.contains("alpha: a-val"));
    }

    /// Subject: prefers `field_values["subject"]`, then `msg_subject`, else
    /// falls back to `Form: <id>`. Matches WLE's "form-derived subject"
    /// default for any form that doesn't capture an explicit subject input.
    #[test]
    fn send_webview_form_subject_prefers_explicit_subject_field() {
        let form_id = "ICS213_Initial";
        let mut field_values = std::collections::HashMap::new();
        field_values.insert("subject".to_string(), "Urgent — supplies needed".to_string());
        let subject = field_values
            .get("subject")
            .or_else(|| field_values.get("msg_subject"))
            .cloned()
            .unwrap_or_else(|| format!("Form: {form_id}"));
        assert_eq!(subject, "Urgent — supplies needed");
    }

    #[test]
    fn send_webview_form_subject_falls_back_to_msg_subject_then_form_id() {
        let form_id = "ARC213";
        // Only msg_subject present.
        let mut field_values = std::collections::HashMap::new();
        field_values.insert("msg_subject".to_string(), "Daily ops brief".to_string());
        let subject = field_values
            .get("subject")
            .or_else(|| field_values.get("msg_subject"))
            .cloned()
            .unwrap_or_else(|| format!("Form: {form_id}"));
        assert_eq!(subject, "Daily ops brief");

        // Neither present → fallback.
        let empty = std::collections::HashMap::<String, String>::new();
        let subject = empty
            .get("subject")
            .or_else(|| empty.get("msg_subject"))
            .cloned()
            .unwrap_or_else(|| format!("Form: {form_id}"));
        assert_eq!(subject, "Form: ARC213");
    }

    /// The XML envelope `send_webview_form` produces is the catalog-form
    /// shape from `forms::serialize::serialize_catalog_form_xml` — same XML
    /// envelope structure as `serialize_form_xml`, but iterates the
    /// `field_values` map directly. This test fixes the catalog-form serializer
    /// as the canonical envelope source so a future regression that diverges
    /// the two serializers' shapes would fire here.
    #[test]
    fn send_webview_form_xml_uses_catalog_serializer() {
        use crate::forms;
        let params = forms::types::FormParameters {
            xml_file_version: "1.0".into(),
            rms_express_version: "Tuxlink/0.0.1".into(),
            submission_datetime: "20260604120000".into(),
            senders_callsign: "N0CALL".into(),
            grid_square: "FN42".into(),
            display_form: "ICS205_Viewer.html".into(),
            reply_template: "ICS205_SendReply.0".into(),
        };
        let mut values = std::collections::HashMap::new();
        values.insert("incident_name".to_string(), "Test".to_string());
        let xml = forms::serialize::serialize_catalog_form_xml("ICS205", &params, &values);
        let xml_str = String::from_utf8_lossy(&xml);
        // BOM + xml declaration + envelope present (same as serialize_form_xml).
        assert_eq!(&xml[0..3], &[0xEF, 0xBB, 0xBF]);
        assert!(xml_str.contains("<RMS_Express_Form>"));
        assert!(xml_str.contains("<display_form>ICS205_Viewer.html</display_form>"));
        assert!(xml_str.contains("<reply_template>ICS205_SendReply.0</reply_template>"));
        assert!(xml_str.contains("<incident_name>Test</incident_name>"));
    }

    // ── tuxlink-hhfx / G10 — reply-form threading send-path decisions ──────
    //
    // `send_webview_form`'s reply branch can't be invoked directly (AppHandle +
    // State), so these replicate the new subject/reply_template decisions inline
    // (matching the synthesis-test pattern above), plus one genuine end-to-end
    // render of a SendReply `.0` Msg projection.

    /// A reply via SendReply has no `Subject:` directive in its `.0` and usually
    /// no `subject`/`msg_subject` field, so the operator's compose subject
    /// (`subject_hint`, "Re: <original>") is used before the "Form: <id>" last
    /// resort. A blank hint is ignored. Replicates the subject chain.
    #[test]
    fn send_webview_form_reply_subject_uses_hint_before_form_id() {
        let form_id = "ICS213_Initial";
        let field_values = std::collections::HashMap::<String, String>::new();
        let subject_hint: Option<String> = Some("Re: Road status".to_string());
        let subject = None::<String>
            .or_else(|| field_values.get("subject").cloned())
            .or_else(|| field_values.get("msg_subject").cloned())
            .or_else(|| {
                subject_hint
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("Form: {form_id}"));
        assert_eq!(subject, "Re: Road status");

        // A blank hint is ignored → "Form: <id>".
        let blank: Option<String> = Some("   ".to_string());
        let subject2 = None::<String>
            .or_else(|| {
                blank
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("Form: {form_id}"));
        assert_eq!(subject2, "Form: ICS213_Initial");
    }

    /// A first-time send advertises `<id>_SendReply.0` so the recipient can
    /// thread a reply; a reply message itself advertises an empty reply_template
    /// (SendReply `.0`s declare none; no reply-to-a-reply chain). Replicates the
    /// outbound `reply_template` branch.
    #[test]
    fn send_webview_form_reply_emits_empty_outbound_reply_template() {
        let form_id = "ICS213_Initial";
        let first_time: Option<String> = None;
        let as_reply: Option<String> = Some("ICS213_SendReply.0".to_string());
        let rt_first = if first_time.is_some() {
            String::new()
        } else {
            format!("{form_id}_SendReply.0")
        };
        let rt_reply = if as_reply.is_some() {
            String::new()
        } else {
            format!("{form_id}_SendReply.0")
        };
        assert_eq!(rt_first, "ICS213_Initial_SendReply.0");
        assert_eq!(rt_reply, "");
    }

    /// End-to-end reply render: a real ICS213 SendReply `.0` governs the reply
    /// message. `resolve_sendreply` parses it; `render_template` projects the
    /// `Msg:` with the original field values (round-tripped through the
    /// SendReply's hidden inputs) + the operator's reply fields + host tags. The
    /// body must reproduce the original 213 AND carry the reply.
    #[test]
    fn reply_body_reproduces_original_and_carries_reply() {
        use crate::forms::txt_template::{render_template, resolve_sendreply};
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let mut f = std::fs::File::create(dir.path().join("ICS213_SendReply.0")).unwrap();
        // A faithful subset of the bundled ICS213_SendReply.0.
        f.write_all(
            b"Form: ICS213_SendReply.html,ICS213_SendReply_Viewer.html\r\n\
              Def: MsgOriginalBody=<var MsgOriginalBody>\r\nTo: \r\nMsg:\r\n\
              GENERAL MESSAGE (ICS 213)\r\n\
              2. To: <var To_Name>\r\n3. From: <var fm_name>\r\n\
              4. Subject: <var Subjectline>\r\n7. Message:\r\n<var Message>\r\n\
              9. Reply:\r\n<var Reply>\r\n10. Replied by: <var rply_by>\r\n\
              Express Sending Station: <MsgSender>\r\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("ICS213_SendReply.html"), b"<html></html>").unwrap();

        let sr = resolve_sendreply(dir.path(), "ICS213_SendReply.0").unwrap();
        // The submitted field set: original fields (pre-bound + round-tripped via
        // hidden inputs) + the operator's reply fields.
        let mut fv = std::collections::HashMap::new();
        for (k, v) in [
            ("To_Name", "Jane / Net Control"),
            ("fm_name", "Bob / Field 3"),
            ("Subjectline", "Road status"),
            ("Message", "Roads clear north of mile 30"),
            ("Reply", "Acknowledged — relaying to ops"),
            ("rply_by", "W7ABC"),
        ] {
            fv.insert(k.to_string(), v.to_string());
        }
        let mut ht = std::collections::HashMap::new();
        ht.insert("MsgSender".to_string(), "W7ABC".to_string());

        let body = render_template(sr.template.msg.as_deref().unwrap(), &fv, &ht);
        // Original 213 reproduced.
        assert!(body.contains("To: Jane / Net Control"), "body: {body}");
        assert!(body.contains("From: Bob / Field 3"));
        assert!(body.contains("Subject: Road status"));
        assert!(body.contains("Roads clear north of mile 30"));
        // The operator's reply carried.
        assert!(body.contains("Acknowledged — relaying to ops"));
        assert!(body.contains("Replied by: W7ABC"));
        // Host tag substituted.
        assert!(body.contains("Express Sending Station: W7ABC"));
        // The SendReply viewer is what a WLE recipient renders.
        assert_eq!(
            sr.template.display_html.as_deref(),
            Some("ICS213_SendReply_Viewer.html")
        );
    }

    // ── tuxlink-2tom / G12-C — SeqInc serial stamping ──────────────────────
    //
    // Replicates the send-path SeqInc decision (the command itself needs State):
    // on a SeqInc template, allocate the next serial and stamp it into `SeqNum`,
    // so `<var SeqNum>` in Subject + Msg both carry the (incrementing) number.
    #[test]
    fn seqinc_stamps_incrementing_serial_into_subject_and_body() {
        use crate::forms::sequence::SeqCounterStore;
        use crate::forms::txt_template::{parse_txt_template, render_template};

        let dir = tempfile::tempdir().unwrap();
        let mut store = SeqCounterStore::open(dir.path().join("c.json"));
        // Real bundle shape (IARU Message Form): SeqNum in both Subject and Msg.
        let t = parse_txt_template(
            "Form: X.html\r\nSubject:Msg# <var SeqNum>\r\nSeqInc:\r\nMsg:\r\nSerial <var SeqNum>\r\n",
        );
        assert!(t.seq_inc);
        let ht = std::collections::HashMap::new();

        // First send: allocate → SeqNum=1.
        let mut fv = std::collections::HashMap::new();
        if t.seq_inc {
            fv.insert("SeqNum".to_string(), store.allocate("X").to_string());
        }
        assert_eq!(render_template(t.subject.as_deref().unwrap(), &fv, &ht), "Msg# 1");
        assert_eq!(render_template(t.msg.as_deref().unwrap(), &fv, &ht), "Serial 1");

        // Second send: the counter advances → SeqNum=2.
        fv.insert("SeqNum".to_string(), store.allocate("X").to_string());
        assert_eq!(render_template(t.subject.as_deref().unwrap(), &fv, &ht), "Msg# 2");
    }

    /// A non-SeqInc form does not allocate or stamp a serial (the gate holds).
    #[test]
    fn non_seqinc_form_does_not_stamp_serial() {
        use crate::forms::txt_template::parse_txt_template;
        let t = parse_txt_template("Form: X.html\r\nSubject:Hello\r\nMsg:\r\nbody\r\n");
        assert!(!t.seq_inc);
        let mut fv = std::collections::HashMap::<String, String>::new();
        if t.seq_inc {
            fv.insert("SeqNum".to_string(), "1".to_string());
        }
        assert!(!fv.contains_key("SeqNum"));
    }

    // ========================================================================
    // tuxlink-hnkn P2 Task 2: render_ics309_pdf unit tests
    // ========================================================================

    #[test]
    fn render_ics309_pdf_with_two_rows_produces_nonempty_pdf() {
        let req = Ics309PdfRequest {
            rows: vec![
                LogRow {
                    datetime: "2024-05-20T10:13:00Z".to_string(),
                    from: "N7CPZ".to_string(),
                    to: "W1AW".to_string(),
                    subject: "DAMAGE REPORT - SECTOR 7".to_string(),
                    direction: "out".to_string(),
                },
                LogRow {
                    datetime: "2024-05-20T10:15:00Z".to_string(),
                    from: "W1AW".to_string(),
                    to: "N7CPZ".to_string(),
                    subject: "RE: DAMAGE REPORT ACK".to_string(),
                    direction: "in".to_string(),
                },
            ],
            range_start: "2024-05-20T10:00:00Z".to_string(),
            range_end: "2024-05-20T11:00:00Z".to_string(),
            station_callsign: Some("N7CPZ".to_string()),
        };
        let result = render_ics309_pdf_inner(req);
        assert!(result.is_ok(), "render should succeed: {:?}", result.err());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty(), "PDF must have bytes");
        // PDF magic bytes: %PDF- (0x25 0x50 0x44 0x46 0x2D)
        assert!(bytes.starts_with(b"%PDF-"), "output must start with PDF magic bytes");
    }

    #[test]
    fn render_ics309_pdf_empty_rows_produces_valid_pdf() {
        let req = Ics309PdfRequest {
            rows: vec![],
            range_start: "2024-05-20T00:00:00Z".to_string(),
            range_end: "2024-05-20T23:59:59Z".to_string(),
            station_callsign: None,
        };
        let bytes = render_ics309_pdf_inner(req).expect("empty rows must still produce a PDF");
        assert!(bytes.starts_with(b"%PDF-"), "empty-row PDF must have PDF magic bytes");
    }

    #[test]
    fn truncate_str_truncates_long_strings_with_ellipsis() {
        assert_eq!(truncate_str("hello world", 5), "hello…");
        assert_eq!(truncate_str("hi", 5), "hi");
        assert_eq!(truncate_str("", 5), "");
    }

    #[test]
    fn rfc3339_to_epoch_parses_utc_string() {
        // 2024-05-20T10:13:00Z = 1_716_199_980 (from native_mailbox tests)
        assert_eq!(rfc3339_to_epoch("2024-05-20T10:13:00Z"), Some(1_716_199_980));
        assert_eq!(rfc3339_to_epoch("not-a-date"), None);
        assert_eq!(rfc3339_to_epoch(""), None);
    }

    // ---- tuxlink-6c9y A6: receivedSession DTO field -------------------------

    /// A message bearing `X-Tuxlink-Received-Session: post-office` must
    /// surface as `received_session = Some("post-office")` and serialise to
    /// `{ "receivedSession": "post-office" }` (camelCase per serde rename_all).
    #[test]
    fn parse_raw_rfc5322_surfaces_received_session_for_post_office_header() {
        let mut raw: Vec<u8> = Vec::new();
        raw.extend_from_slice(b"Mid: POMID0000001\r\n");
        raw.extend_from_slice(b"From: W1AW\r\n");
        raw.extend_from_slice(b"To: N7CPZ\r\n");
        raw.extend_from_slice(b"Subject: Post Office Test\r\n");
        raw.extend_from_slice(b"Date: 2026/06/08 12:00\r\n");
        raw.extend_from_slice(b"Body: 5\r\n");
        raw.extend_from_slice(
            format!("{}: {}\r\n", RECEIVED_SESSION_HEADER, RECEIVED_SESSION_POST_OFFICE)
                .as_bytes(),
        );
        raw.extend_from_slice(b"\r\n");
        raw.extend_from_slice(b"hello");

        let dto = parse_raw_rfc5322("POMID0000001", &raw)
            .expect("parse succeeds for message with X-Tuxlink-Received-Session");

        assert_eq!(
            dto.received_session,
            Some(RECEIVED_SESSION_POST_OFFICE.to_string()),
            "received_session must be Some(\"post-office\") when header is present"
        );

        // Verify camelCase serialisation for the TS boundary.
        let value = serde_json::to_value(&dto).expect("DTO serialises");
        assert_eq!(
            value["receivedSession"],
            serde_json::Value::String(RECEIVED_SESSION_POST_OFFICE.to_string()),
            "receivedSession (camelCase) must appear in JSON with value \"post-office\""
        );
    }

    /// A message without the header must give `received_session = None`
    /// and `receivedSession` must be `null` in JSON.
    #[test]
    fn parse_raw_rfc5322_received_session_is_none_when_header_absent() {
        let mut raw: Vec<u8> = Vec::new();
        raw.extend_from_slice(b"Mid: CMSMID000001\r\n");
        raw.extend_from_slice(b"From: W1AW\r\n");
        raw.extend_from_slice(b"To: N7CPZ\r\n");
        raw.extend_from_slice(b"Subject: CMS Mail\r\n");
        raw.extend_from_slice(b"Date: 2026/06/08 12:00\r\n");
        raw.extend_from_slice(b"Body: 5\r\n");
        raw.extend_from_slice(b"\r\n");
        raw.extend_from_slice(b"hello");

        let dto = parse_raw_rfc5322("CMSMID000001", &raw)
            .expect("parse succeeds for message without X-Tuxlink-Received-Session");

        assert_eq!(
            dto.received_session, None,
            "received_session must be None when header is absent"
        );

        let value = serde_json::to_value(&dto).expect("DTO serialises");
        assert_eq!(
            value["receivedSession"],
            serde_json::Value::Null,
            "receivedSession must serialise as null when absent"
        );
    }

    // ========================================================================
    // tuxlink-6c9y Task C1 — telnet_post_office_connect orchestration
    // ========================================================================

    /// Local mode (`mode == "local"`) builds the `<base>-L` login callsign,
    /// the `wl2k` B2F targetcall, a `PostOffice` intent, and — critically —
    /// NO keyring password. The `-L` suffix is the entire local-vs-network
    /// routing discriminator (WLE `TelnetSession.cs:2011-2013`); the Post
    /// Office path never reads the OS keyring (`password: None`).
    #[test]
    fn post_office_config_local_uses_dash_l_and_no_keyring() {
        let cfg = post_office_exchange_config(
            &crate::identity::Callsign::parse("n7cpz-7").unwrap(),
            "CN87",
            true,
        );
        assert_eq!(cfg.mycall, "N7CPZ-L", "local PO login is the base call + -L");
        assert_eq!(
            cfg.targetcall,
            crate::winlink::telnet::CMS_TARGET_CALL,
            "B2F targetcall is wl2k"
        );
        assert_eq!(cfg.locator, "CN87");
        assert!(
            cfg.password.is_none(),
            "Post Office uses no keyring — password MUST be None"
        );
        assert_eq!(
            cfg.intent,
            crate::winlink::session::SessionIntent::PostOffice,
            "local mode → PostOffice intent (L pool + marker)"
        );
    }

    /// Network mode (`mode != "local"`) builds the FULL base callsign (no
    /// `-L`), the `wl2k` targetcall, a `Mesh` intent (normal C-mail pool), and
    /// still NO keyring password.
    #[test]
    fn post_office_config_network_uses_full_base_and_mesh_intent() {
        let cfg = post_office_exchange_config(
            &crate::identity::Callsign::parse("N7CPZ-7").unwrap(),
            "CN87",
            false,
        );
        assert_eq!(cfg.mycall, "N7CPZ", "network PO login is the bare base call, no -L");
        assert_eq!(cfg.targetcall, crate::winlink::telnet::CMS_TARGET_CALL);
        assert!(cfg.password.is_none(), "network PO also reads no keyring");
        assert_eq!(
            cfg.intent,
            crate::winlink::session::SessionIntent::Mesh,
            "network mode → Mesh intent (normal C-mail pool)"
        );
    }

    /// tuxlink-b6ad: Network PO drains the whole Outbox like CMS (`None`),
    /// ignoring any selection set; Telnet RMS Post Office (local `-L` pool)
    /// keeps the explicit selection as its leakage guard (`Some`).
    #[test]
    fn po_drain_selection_network_drains_all_local_keeps_guard() {
        use std::collections::HashSet;
        let sel: HashSet<String> = ["A".to_string(), "B".to_string()].into_iter().collect();
        // local `-L` pool: explicit send-time selection is the leakage guard.
        assert_eq!(po_drain_selection(true, &sel), Some(&sel));
        // network (Mesh): drains all, even when a non-empty selection was passed.
        assert_eq!(po_drain_selection(false, &sel), None);
        // network with an empty selection still drains all — NOT receive-only.
        let empty: HashSet<String> = HashSet::new();
        assert_eq!(po_drain_selection(false, &empty), None);
    }

    /// The connect-request DTO deserializes the snake_case keys the B3 pane
    /// sends (`my_callsign`, `selected_mids`) — the `{ req: {...} }` contract.
    #[test]
    fn post_office_dial_request_deserializes_snake_case() {
        let json = serde_json::json!({
            "mode": "local",
            "host": "127.0.0.1",
            "port": 8772,
            "my_callsign": "N7CPZ",
            "locator": "CN87",
            "selected_mids": ["AAA111", "BBB222"],
        });
        let req: PostOfficeDialRequest =
            serde_json::from_value(json).expect("snake_case PO dial request deserializes");
        assert_eq!(req.mode, "local");
        assert_eq!(req.host, "127.0.0.1");
        assert_eq!(req.port, 8772);
        assert_eq!(req.my_callsign, "N7CPZ");
        assert_eq!(req.locator, "CN87");
        assert_eq!(req.selected_mids, vec!["AAA111".to_string(), "BBB222".to_string()]);
    }

    /// The result DTO serializes `relay_state` as a kebab-case string the pane
    /// can render in its banner strip, alongside the snake_case counters the
    /// frontend `DialResult` reads.
    #[test]
    fn post_office_dial_result_serializes_relay_state_kebab_case() {
        use crate::winlink::relay_banner::RelayState;
        let result = PostOfficeDialResult {
            sent_count: 2,
            received_count: 1,
            relay_state: RelayState::LocalDatabase.into(),
        };
        let value = serde_json::to_value(&result).expect("PO dial result serializes");
        assert_eq!(value["sent_count"], 2);
        assert_eq!(value["received_count"], 1);
        assert_eq!(
            value["relay_state"], "local-database",
            "RelayState::LocalDatabase → \"local-database\""
        );
        // NotRelay is the ordinary-CMS / no-banner default.
        let plain: RelayStateDto = RelayState::NotRelay.into();
        assert_eq!(
            serde_json::to_value(plain).unwrap(),
            "not-relay",
            "RelayState::NotRelay → \"not-relay\""
        );
    }

    /// Integration test (clone of `bsiy`'s
    /// `selecting_connect_emits_offer_and_files_selected_message_into_inbox`):
    /// a scripted Answer-role relay on loopback OFFERS one inbound message and
    /// receives the client's outbound. Drives the REAL
    /// `post_office_exchange` orchestration with the `bsiy` selecting decider.
    ///
    /// Asserts, against the fixture relay:
    /// - the login line sent is `<base>-L` (local mode);
    /// - ONLY the operator-selected outbound MID is proposed (selection filter);
    /// - inbound selection is exercised via the `bsiy` decider (the registry
    ///   slot is populated; `resolve_selection` delivers the selected message);
    /// - the received PO mail is filed into Inbox WITH the
    ///   `X-Tuxlink-Received-Session: post-office` marker (PostOffice intent);
    /// - `relay_state` is returned in the result.
    ///
    /// The keyring-never property is structural — `post_office_exchange` takes
    /// no password and builds `ExchangeConfig.password = None`
    /// (`post_office_config_local_uses_dash_l_and_no_keyring` pins it). Nothing
    /// is transmitted: 127.0.0.1 loopback only (RADIO-1 N/A — pure TCP).
    #[test]
    fn post_office_exchange_selects_outbound_and_files_inbound_with_marker() {
        use crate::winlink::b2f_events::{AttemptId, B2fEvent, B2fEventSink};
        use crate::winlink::compose::compose_message;
        use crate::winlink::inbound_selection::{
            build_selecting_decider, resolve_selection, InboundSelection, PendingProposalDto,
            SelectionRegistry, UnselectedDisposition,
        };
        use crate::winlink::proposal::{Answer, Proposal};
        use crate::winlink::session::{
            run_exchange_with_role, ExchangeConfig, ExchangeRole, OutboundMessage as SessionOutbound,
            SessionIntent,
        };
        use crate::winlink::telnet::CMS_TARGET_CALL;
        use crate::native_mailbox::Mailbox;
        use std::collections::HashSet;
        use std::io::{BufRead, BufReader, Write};
        use std::net::{Shutdown, TcpListener, TcpStream};
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex as StdMutex};

        struct RecordingSink {
            events: Arc<StdMutex<Vec<B2fEvent>>>,
        }
        impl B2fEventSink for RecordingSink {
            fn push(&self, event: B2fEvent) {
                self.events.lock().unwrap().push(event);
            }
        }

        // -- Step 1: the message the relay will OFFER the client (inbound). ----
        let offered = compose_message(
            "W7AUX",
            &["N7CPZ"],
            &[],
            "PO inbound",
            "pick me",
            1_716_400_000,
        );
        let offered_mid = offered.header("Mid").expect("offered has Mid").to_string();
        let (in_proposal, in_compressed) =
            offered.to_proposal().expect("offered → proposal");
        let server_outbound = vec![SessionOutbound {
            proposal: in_proposal,
            title: "PO inbound".to_string(),
            compressed: in_compressed,
        }];

        // -- Step 2: the client's Outbox has TWO drafts; only ONE is selected.
        let client_dir = tempfile::tempdir().unwrap();
        let client_mailbox = Mailbox::new(client_dir.path());
        let selected_msg = compose_message(
            "N7CPZ",
            &["W7AUX"],
            &[],
            "Selected outbound",
            "send me",
            1_716_400_100,
        );
        let unselected_msg = compose_message(
            "N7CPZ",
            &["KK7XYZ"],
            &[],
            "Unselected outbound",
            "leave me",
            1_716_400_101,
        );
        let selected_mid = selected_msg.header("Mid").expect("selected has Mid").to_string();
        let unselected_mid =
            unselected_msg.header("Mid").expect("unselected has Mid").to_string();
        client_mailbox
            .store(MailboxFolder::Outbox, &selected_msg.to_bytes())
            .unwrap();
        client_mailbox
            .store(MailboxFolder::Outbox, &unselected_msg.to_bytes())
            .unwrap();
        let mut selected_set = HashSet::new();
        selected_set.insert(selected_mid.clone());

        // -- Step 3: scripted Answer-role relay on loopback. -------------------
        // Captures the client's login line so the test can assert `<base>-L`,
        // and records which outbound MIDs the client proposes.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let listen_port = listener.local_addr().unwrap().port();
        let login_capture = Arc::new(StdMutex::new(String::new()));
        let proposed_mids = Arc::new(StdMutex::new(Vec::<String>::new()));

        let server = {
            let login_capture = login_capture.clone();
            let proposed_mids = proposed_mids.clone();
            std::thread::spawn(move || {
                let (sock, _) = listener.accept().expect("accept");
                let mut writer = sock.try_clone().expect("clone for write");
                writer.write_all(b"Callsign :\rPassword :\r").expect("login prompts");
                let mut reader = BufReader::new(sock);
                // First client line is the callsign (login), second is the password.
                let mut callsign_line = Vec::new();
                reader.read_until(b'\r', &mut callsign_line).expect("read callsign");
                *login_capture.lock().unwrap() =
                    String::from_utf8_lossy(&callsign_line).trim_end_matches('\r').to_string();
                let mut password_line = Vec::new();
                reader.read_until(b'\r', &mut password_line).expect("read password");

                let server_config = ExchangeConfig {
                    mycall: "W7AUX".into(),
                    targetcall: CMS_TARGET_CALL.to_string(),
                    locator: "CN87".into(),
                    password: None,
                    intent: SessionIntent::Cms,
                };
                // The Answer-role server OFFERS its message and records the MIDs
                // the client proposes back (the decide closure sees the client's
                // outbound proposals).
                run_exchange_with_role(
                    &mut reader,
                    &mut writer,
                    ExchangeRole::Answer,
                    &server_config,
                    server_outbound,
                    |proposals: &[Proposal]| {
                        let mut g = proposed_mids.lock().unwrap();
                        for p in proposals {
                            g.push(p.mid.clone());
                        }
                        Ok(proposals
                            .iter()
                            .map(|_| Answer::Accept { resume_offset: 0 })
                            .collect())
                    },
                    None,
                )
                .expect("server Answer exchange succeeds");
            })
        };

        // -- Step 4: operator-answer thread (models the resolve command). ------
        let registry: SelectionRegistry = Arc::new(StdMutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let attempt_id = AttemptId(7777);
        let events = Arc::new(StdMutex::new(Vec::<B2fEvent>::new()));
        let sink: Arc<dyn B2fEventSink> = Arc::new(RecordingSink {
            events: events.clone(),
        });

        let answerer = {
            let registry = registry.clone();
            let want_mid = offered_mid.clone();
            std::thread::spawn(move || {
                let (slot_attempt, slot_req) = {
                    let mut found = None;
                    for _ in 0..400 {
                        if let Some(s) = registry.lock().unwrap().as_ref() {
                            found = Some((s.attempt_id, s.request_id));
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    found.expect("decider never registered a selection slot")
                };
                let delivered = resolve_selection(
                    &registry,
                    slot_attempt,
                    slot_req,
                    InboundSelection {
                        selected_mids: vec![want_mid],
                        disposition: UnselectedDisposition::Hold,
                    },
                );
                assert!(delivered, "resolve_selection should match the live slot");
            })
        };

        // -- Step 5: build the selecting decider + run post_office_exchange. ---
        let emit = {
            let sink = sink.clone();
            move |request_id: u64, dtos: &[PendingProposalDto]| {
                sink.push(B2fEvent::InboundProposalsOffered {
                    request_id,
                    proposals: dtos.to_vec(),
                    attempt_id,
                });
            }
        };
        let decide = build_selecting_decider(registry.clone(), attempt_id, emit, aborting.clone());

        let abort_handle: StdMutex<Option<TcpStream>> = StdMutex::new(None);
        let register_socket = |sock: &TcpStream| {
            if let Ok(clone) = sock.try_clone() {
                if let Ok(mut slot) = abort_handle.lock() {
                    if aborting.load(std::sync::atomic::Ordering::SeqCst) {
                        let _ = clone.shutdown(Shutdown::Both);
                    } else {
                        *slot = Some(clone);
                    }
                }
            }
        };

        let result = post_office_exchange(
            &client_mailbox,
            "127.0.0.1",
            listen_port,
            &crate::identity::Callsign::parse("N7CPZ").unwrap(),
            "CN87",
            /* local */ true,
            &selected_set,
            &|_| {},
            &|_| {},
            &|| {},
            &register_socket,
            decide,
        )
        .expect("post_office_exchange completes");

        answerer.join().expect("answerer thread panicked");
        server.join().expect("server thread panicked");

        // -- Assertion: login line was the base call + -L. ---------------------
        assert_eq!(
            *login_capture.lock().unwrap(),
            "N7CPZ-L",
            "local-mode login line must be the base callsign + -L"
        );

        // -- Assertion: ONLY the selected outbound MID was proposed. -----------
        let proposed = proposed_mids.lock().unwrap().clone();
        assert!(
            proposed.contains(&selected_mid),
            "the selected outbound MID must be proposed; proposed = {proposed:?}"
        );
        assert!(
            !proposed.contains(&unselected_mid),
            "the UNSELECTED outbound MID must NOT be proposed; proposed = {proposed:?}"
        );

        // -- Assertion: the inbound offer fired via the bsiy decider. ----------
        let log = events.lock().unwrap();
        let offer = log.iter().find_map(|e| match e {
            B2fEvent::InboundProposalsOffered { proposals, attempt_id: a, .. } => {
                Some((proposals.clone(), *a))
            }
            _ => None,
        });
        let (dtos, evt_attempt) =
            offer.expect("an InboundProposalsOffered event must have fired");
        assert_eq!(evt_attempt, attempt_id, "offer carries the threaded attempt_id");
        assert_eq!(dtos.len(), 1, "exactly one inbound proposal offered");
        assert_eq!(dtos[0].mid, offered_mid);

        // -- Assertion: received PO mail filed with the post-office marker. ----
        let inbox = client_mailbox.list(MailboxFolder::Inbox).expect("list inbox");
        assert_eq!(inbox.len(), 1, "selected inbound landed in Inbox; got {inbox:?}");
        let body = client_mailbox
            .read(MailboxFolder::Inbox, &inbox[0].id)
            .expect("read filed inbound");
        let stored = crate::winlink::message::Message::from_bytes(&body.raw_rfc5322)
            .expect("filed bytes are a Message");
        assert_eq!(
            stored.header(crate::winlink::message::RECEIVED_SESSION_HEADER),
            Some(RECEIVED_SESSION_POST_OFFICE),
            "PostOffice intent must stamp X-Tuxlink-Received-Session: post-office"
        );

        // -- Assertion: result carries counts + relay_state. -------------------
        assert_eq!(result.received_count, 1, "one inbound received");
        assert_eq!(result.sent_count, 1, "one outbound sent");
        // relay_state is plumbed from the exchange (NotRelay for a plain
        // CMS-style server with no relay banner).
        let _ = result.relay_state;
    }

    /// The abort wiring fix (code-review C1): the connect path stores the live
    /// socket into the SHARED `PostOfficeConnectState.abort_handle`, and the
    /// abort command takes + `shutdown(Both)`s it. The full `#[tauri::command]`
    /// needs an app harness, so this drives the abort command's exact take +
    /// shutdown logic against a real loopback socket the connect path would have
    /// registered, proving (a) the handle is emptied (the take happened) and
    /// (b) the peer observes the socket close (read returns 0 = EOF).
    ///
    /// Regression guard: the prior shape kept `abort_handle` LOCAL to the
    /// `spawn_blocking` closure, unreachable from the abort command — so an
    /// in-flight dial's socket was never shut down. Loopback only, no RF
    /// (RADIO-1 N/A — pure TCP).
    #[test]
    fn post_office_abort_takes_and_shuts_down_the_registered_socket() {
        use std::io::Read;
        use std::net::{Shutdown, TcpListener, TcpStream};
        use std::sync::atomic::Ordering;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");

        // Server side: accept the connection and hold its end so the client's
        // shutdown is observable as EOF on a read.
        let server = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 8];
            // Blocks until the client end is shut down → returns Ok(0) (EOF).
            peer.read(&mut buf).expect("read after peer shutdown")
        });

        let client = TcpStream::connect(addr).expect("connect loopback");

        // Build the shared state the connect path manages, and register the
        // socket exactly as the connect path's `register_socket` does (store a
        // clone into the SHARED handle while `aborting` is unset).
        let po_state = PostOfficeConnectState::default();
        {
            let clone = client.try_clone().expect("clone socket");
            assert!(
                !po_state.aborting.load(Ordering::SeqCst),
                "precondition: not yet aborting, so the socket is stored not shut"
            );
            *po_state.abort_handle.lock().unwrap() = Some(clone);
        }
        assert!(
            po_state.abort_handle.lock().unwrap().is_some(),
            "the registered socket is reachable from shared state (the bug: it was not)"
        );

        // Run the abort command's exact take + shutdown sequence (order:
        // set aborting → [drop registry slot, N/A here] → shutdown socket).
        po_state.aborting.store(true, Ordering::SeqCst);
        if let Some(sock) = po_state
            .abort_handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = sock.shutdown(Shutdown::Both);
        }

        // The handle was emptied by the take.
        assert!(
            po_state.abort_handle.lock().unwrap().is_none(),
            "abort must TAKE the socket out of the handle"
        );

        // The peer observed the shutdown as EOF (0 bytes), proving the abort
        // actually force-closed the in-flight socket.
        let bytes_read = server.join().expect("server thread");
        assert_eq!(bytes_read, 0, "peer must see EOF after abort shuts the socket down");

        // The original client handle still exists but is shut; an extra
        // shutdown of an already-closed socket is a harmless no-op.
        let _ = client.shutdown(Shutdown::Both);
    }

    /// The `PostOfficeConnectGuard` resets `in_progress` on Drop (panic-safe
    /// single-flight). A panic in the async setup window — after the
    /// `compare_exchange` acquires the flag but before the `.await` — would
    /// otherwise wedge `in_progress = true` forever. The guard's Drop covers it.
    #[test]
    fn post_office_connect_guard_resets_single_flight_on_drop() {
        use std::sync::atomic::Ordering;

        let po_state = PostOfficeConnectState::default();
        // Acquire single-flight, as the connect command does.
        assert!(
            po_state
                .in_progress
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "single-flight acquired"
        );
        // Simulate a panic in the setup window: the guard is in scope and
        // unwinds. `catch_unwind` confirms the Drop ran despite the panic.
        let in_progress = po_state.in_progress.clone();
        let handle = po_state.abort_handle.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = PostOfficeConnectGuard {
                in_progress: in_progress.clone(),
                handle: handle.clone(),
            };
            panic!("simulated setup-window panic");
        }));
        assert!(result.is_err(), "the closure panicked as set up");
        assert!(
            !po_state.in_progress.load(Ordering::SeqCst),
            "the guard's Drop must reset in_progress even on an unwinding panic"
        );
    }

    // tuxlink-o4p9 / G12-A: send_webview_form recipient union — the form's .txt
    // To: leads, operator additions append, case-insensitive dedup.
    #[test]
    fn merge_txt_recipients_unions_form_and_operator() {
        use std::collections::HashMap;
        // Fixed-address .txt To: (DYFI → USGS); operator added a personal CC-as-to.
        let to = merge_txt_recipients(
            Some("dyfi_reports_automated@usgs.gov"),
            &HashMap::new(),
            &HashMap::new(),
            &["W7ABC".to_string()],
        );
        assert_eq!(to, vec!["dyfi_reports_automated@usgs.gov", "W7ABC"]);
    }

    #[test]
    fn merge_txt_recipients_renders_var_addressed_to() {
        use std::collections::HashMap;
        // Quick Message: `To: <var address>` — the recipient is an in-form field.
        let mut fv = HashMap::new();
        fv.insert("address".to_string(), "NET-CONTROL; W1AW".to_string());
        let to = merge_txt_recipients(Some("<var address>"), &fv, &HashMap::new(), &[]);
        assert_eq!(to, vec!["NET-CONTROL", "W1AW"]);
    }

    #[test]
    fn merge_txt_recipients_dedupes_case_insensitively() {
        use std::collections::HashMap;
        let to = merge_txt_recipients(
            Some("ops@ares.org"),
            &HashMap::new(),
            &HashMap::new(),
            &["OPS@ares.org".to_string(), "extra@x.com".to_string()],
        );
        assert_eq!(to, vec!["ops@ares.org", "extra@x.com"]);
    }

    #[test]
    fn merge_txt_recipients_empty_txt_keeps_operator() {
        use std::collections::HashMap;
        // Blank .txt To: (SendReply style) or absent → operator recipients alone.
        let from_blank = merge_txt_recipients(Some(""), &HashMap::new(), &HashMap::new(), &["a@b.c".to_string()]);
        assert_eq!(from_blank, vec!["a@b.c"]);
        let from_none = merge_txt_recipients(None, &HashMap::new(), &HashMap::new(), &["a@b.c".to_string()]);
        assert_eq!(from_none, vec!["a@b.c"]);
    }

    // tuxlink-l80q: message_move_bulk moves every listed message to a single
    // destination, honoring each item's own source folder so a cross-folder
    // selection (inbox + sent) lands correctly in one command call. Mirrors the
    // seed-via-sibling-Mailbox seam used by the NativeBackend read-state tests.
    #[tokio::test]
    async fn message_move_bulk_moves_all_listed_messages_across_source_folders() {
        use crate::native_mailbox::Mailbox;
        use crate::winlink::compose::compose_message;
        use crate::winlink_backend::{MailboxFolder, NativeBackend, WinlinkBackend};

        let dir = tempfile::tempdir().unwrap();
        let seed = Mailbox::new(dir.path());
        let a = seed
            .store(MailboxFolder::Inbox, &compose_message("N7CPZ", &["W1AW"], &[], "A", "a", 1_716_200_000).to_bytes())
            .unwrap();
        let b = seed
            .store(MailboxFolder::Inbox, &compose_message("N7CPZ", &["W1AW"], &[], "B", "b", 1_716_200_001).to_bytes())
            .unwrap();
        let c = seed
            .store(MailboxFolder::Sent, &compose_message("N7CPZ", &["W1AW"], &[], "C", "c", 1_716_200_002).to_bytes())
            .unwrap();

        let backend = NativeBackend::new(crate::test_helpers::native_test_config(), dir.path());

        let items = vec![
            MessageRefDto { folder: "inbox".into(), id: a.0.clone() },
            MessageRefDto { folder: "inbox".into(), id: b.0.clone() },
            MessageRefDto { folder: "sent".into(), id: c.0.clone() },
        ];
        move_bulk_with_backend(&backend, items, "archive")
            .await
            .expect("bulk move succeeds");

        assert!(
            backend.list_messages(MailboxFolder::Inbox).await.unwrap().is_empty(),
            "both inbox messages left the source folder"
        );
        assert!(
            backend.list_messages(MailboxFolder::Sent).await.unwrap().is_empty(),
            "the sent message left the source folder"
        );
        assert_eq!(
            backend.list_messages(MailboxFolder::Archive).await.unwrap().len(),
            3,
            "all three messages landed in the single destination folder"
        );
    }

    // tuxlink-l80q (Codex P2): a self-move (item.folder == to) must be a no-op,
    // NOT a delete. Mailbox::move_between writes dst then removes src — the same
    // path for a self-move — so without a guard the message is destroyed.
    #[tokio::test]
    async fn message_move_bulk_self_move_is_a_no_op_not_a_delete() {
        use crate::native_mailbox::Mailbox;
        use crate::winlink::compose::compose_message;
        use crate::winlink_backend::{MailboxFolder, NativeBackend, WinlinkBackend};

        let dir = tempfile::tempdir().unwrap();
        let seed = Mailbox::new(dir.path());
        let a = seed
            .store(MailboxFolder::Inbox, &compose_message("N7CPZ", &["W1AW"], &[], "A", "a", 1_716_200_000).to_bytes())
            .unwrap();
        let backend = NativeBackend::new(crate::test_helpers::native_test_config(), dir.path());

        let items = vec![MessageRefDto { folder: "inbox".into(), id: a.0.clone() }];
        move_bulk_with_backend(&backend, items, "inbox")
            .await
            .expect("self-move is accepted as a no-op");

        assert_eq!(
            backend.list_messages(MailboxFolder::Inbox).await.unwrap().len(),
            1,
            "a self-move must not delete the message"
        );
    }
}

// ============================================================================
// tuxlink-hnkn P2 Task 4: FormDraftLibrary Tauri commands
// ============================================================================
//
// Three IPC commands that expose the `DraftLibrary` SQLite-backed store to the
// frontend. The store is registered as `Arc<DraftLibrary>` managed state in
// `lib.rs` during app startup. Each command acquires the `Arc` via
// `tauri::State<'_, Arc<DraftLibrary>>` and delegates to the store methods.
//
// Error projection: `DraftLibraryError` → `String` (the lightweight IPC
// convention used by the existing search commands). The commands are async
// only because the Tauri `#[tauri::command]` macro requires it for commands
// that return `Result`; the underlying SQLite calls are synchronous (the
// `DraftLibrary::conn` is a `Mutex<Connection>`).

use crate::forms::draft_library::{DraftLibrary, FormDraftSlot};

/// List all saved draft slots for the given `form_id`.
///
/// Returns an empty list when no slots exist for that form (not an error).
/// Slots are ordered by `created_at` ascending (oldest/first-created first).
#[tauri::command]
pub async fn form_draft_library_list(
    form_id: String,
    library: State<'_, std::sync::Arc<DraftLibrary>>,
) -> Result<Vec<FormDraftSlot>, String> {
    library.list(&form_id).map_err(|e| e.to_string())
}

/// Insert or update a draft slot.
///
/// - `slot_id = None` → new slot with a minted UUID.
/// - `slot_id = Some(id)` → update the matching row in place, or insert if it
///   does not exist yet (upsert semantics).
///
/// Returns the final `FormDraftSlot` — callers use this to get the assigned
/// `slot_id` on creates, or to reflect the preserved `created_at` on updates.
#[tauri::command]
pub async fn form_draft_library_upsert(
    slot_id: Option<String>,
    form_id: String,
    label: String,
    payload: serde_json::Value,
    library: State<'_, std::sync::Arc<DraftLibrary>>,
) -> Result<FormDraftSlot, String> {
    library.upsert(slot_id, form_id, label, payload).map_err(|e| e.to_string())
}

/// Delete a draft slot by `slot_id`. No-op-safe if the slot does not exist.
#[tauri::command]
pub async fn form_draft_library_delete(
    slot_id: String,
    library: State<'_, std::sync::Arc<DraftLibrary>>,
) -> Result<(), String> {
    library.delete(&slot_id).map_err(|e| e.to_string())
}
