use crate::winlink_backend::MailboxFolder;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Message {
    pub mid: String,
    pub subject: String,
    pub from: String,
    /// Recipient list. Added by Task 12 (tuxlink-zsm) for the list "To"
    /// column. **Graceful degradation:** Pat 1.0.0's `/api/mailbox` list
    /// endpoint does NOT include a `To` field (verified against the shipped
    /// `test_list_inbox_parses_pat_json` fixture, which has no `To`), so this
    /// defaults to an empty vec via `#[serde(default)]` + a tolerant
    /// deserializer. If a future Pat exposes recipients, `deser_addr_list`
    /// parses Pat's address-object array `[{"Addr":"..."}]` without a
    /// mapping change. Spec §2.1 + §9 item 7.
    pub to: Vec<String>,
    pub date: String,
    pub unread: bool,
    pub body_size: u64,
    /// Attachment-presence flag for the list `#` column. Added by Task 12.
    /// Pat 1.0.0's list DTO has no attachment metadata, so this defaults
    /// `false` (`#[serde(default)]`). The authoritative attachment list is
    /// materialized at read time (Task 13 RFC5322 parse). Spec §2.1.
    pub has_attachments: bool,
}

#[derive(Debug, Deserialize)]
struct PatMessageDto {
    #[serde(rename = "MID")] mid: String,
    #[serde(rename = "Subject")] subject: String,
    #[serde(rename = "From")] from: PatAddr,
    // `To` is absent from Pat 1.0.0's list DTO. `default` + a tolerant
    // deserializer means a missing field → empty vec, and a present field
    // (future Pat / other backend) → parsed recipient list. Spec §2.1.
    #[serde(rename = "To", default, deserialize_with = "deser_addr_list")] to: Vec<String>,
    #[serde(rename = "Date")] date: String,
    #[serde(rename = "Unread", default)] unread: bool,
    #[serde(rename = "BodySize", default)] body_size: u64,
    // No attachment metadata in Pat 1.0.0's list DTO. Default `false`;
    // tolerate either a bool flag or (future) a count that we coerce to a
    // presence bool. Spec §2.1.
    #[serde(rename = "Files", default, deserialize_with = "deser_has_attachments")] has_attachments: bool,
}

#[derive(Debug, Deserialize)]
struct PatAddr { #[serde(rename = "Addr")] addr: String }

/// Deserialize Pat's recipient array (`[{"Addr":"CALL@..."}]`) into a flat
/// `Vec<String>` of addresses. Tolerant: a JSON `null` yields an empty vec.
/// Pat 1.0.0 omits the field entirely, so `#[serde(default)]` handles the
/// common case and this only runs when `To` IS present.
fn deser_addr_list<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = <Option<Vec<PatAddr>>>::deserialize(d)?;
    Ok(opt.unwrap_or_default().into_iter().map(|a| a.addr).collect())
}

/// Deserialize an attachment-presence indicator. Pat 1.0.0 omits this, so
/// `#[serde(default)]` returns `false`. If a future Pat exposes a `Files`
/// array, a non-empty array → `true`; `null`/absent → `false`. Spec §2.1.
fn deser_has_attachments<'de, D>(d: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Accept either an array of files (presence = non-empty) or a bool.
    let v = <Option<serde_json::Value>>::deserialize(d)?;
    Ok(match v {
        Some(serde_json::Value::Array(a)) => !a.is_empty(),
        Some(serde_json::Value::Bool(b)) => b,
        _ => false,
    })
}

impl From<PatMessageDto> for Message {
    fn from(d: PatMessageDto) -> Self {
        Message {
            mid: d.mid, subject: d.subject, from: d.from.addr,
            to: d.to, date: d.date, unread: d.unread, body_size: d.body_size,
            has_attachments: d.has_attachments,
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum PatClientError {
    Http(reqwest::Error),
    Status(u16),
    /// The response body exceeded the read byte cap — rejected before fully
    /// buffering (tuxlink-f1a). Carries the cap for the error message.
    TooLarge { cap: usize },
}

/// Hard cap on a single message body buffered from Pat. Aligns with the parser
/// cap (`ui_commands::MAX_RFC5322_BYTES`); enforced HERE on the read side so an
/// oversized/buggy Pat response cannot force unbounded memory/network work
/// before the parser cap trips (tuxlink-f1a, Codex Task-13 finding 3).
pub const MAX_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

impl std::fmt::Display for PatClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatClientError::Http(e) => write!(f, "HTTP error: {}", e),
            PatClientError::Status(s) => write!(f, "Pat returned status {}", s),
            PatClientError::TooLarge { cap } => {
                write!(f, "message body exceeded the {}-byte read cap", cap)
            }
        }
    }
}
impl std::error::Error for PatClientError {}

/// HTTP client wrapper for the Pat sidecar. `Clone` per tuxlink-z5f v2 P1
/// #4 — `reqwest::Client` is `Arc`-backed internally, so cloning is cheap
/// and yields handles that share the connection pool.
///
/// **Async** per tuxlink-z5f impl-phase discovery: `reqwest::blocking::Client`
/// spawns an internal tokio runtime which panics if dropped from within an
/// outer tokio runtime (`Cannot drop a runtime in a context where blocking
/// is not allowed`). Tauri command handlers are async-by-default, so async
/// is the natural fit anyway. The `WinlinkBackend::*` trait methods
/// `.await` directly without `spawn_blocking` wrappers.
#[derive(Clone)]
pub struct PatClient {
    base_url: String,
    http: reqwest::Client,
    /// Per-message read byte cap (tuxlink-f1a). Defaults to `MAX_MESSAGE_BYTES`.
    max_read_bytes: usize,
}

impl PatClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build().expect("reqwest build");
        PatClient { base_url: base_url.into(), http, max_read_bytes: MAX_MESSAGE_BYTES }
    }

    /// Override the per-message read byte cap. Primarily for tests (a tiny cap
    /// exercises the limit without a multi-MiB fixture); callers may also lower
    /// it for constrained environments (tuxlink-f1a).
    pub fn with_max_read_bytes(mut self, cap: usize) -> Self {
        self.max_read_bytes = cap;
        self
    }

    pub async fn list(&self, folder: MailboxFolder) -> Result<Vec<Message>, PatClientError> {
        let url = format!("{}/api/mailbox/{}", self.base_url, folder.as_path());
        let resp = self.http.get(&url).send().await.map_err(PatClientError::Http)?;
        if !resp.status().is_success() {
            return Err(PatClientError::Status(resp.status().as_u16()));
        }
        let dtos: Vec<PatMessageDto> = resp.json().await.map_err(PatClientError::Http)?;
        Ok(dtos.into_iter().map(Message::from).collect())
    }

    /// Fetch one message body by MID. Returns raw bytes preserving wire
    /// fidelity (per tuxlink-z5f v2 P0 #2 — MIME attachments need byte-level
    /// preservation, not lossy UTF-8 conversion at this layer). The trait's
    /// `WinlinkBackend::read_message` wraps this into a `MessageBody`.
    pub async fn read(&self, folder: MailboxFolder, mid: &str) -> Result<Vec<u8>, PatClientError> {
        let url = format!("{}/api/mailbox/{}/{}", self.base_url, folder.as_path(), mid);
        let mut resp = self.http.get(&url).send().await.map_err(PatClientError::Http)?;
        if !resp.status().is_success() {
            return Err(PatClientError::Status(resp.status().as_u16()));
        }
        // tuxlink-f1a: bound memory BEFORE buffering the whole body. First reject
        // a declared-oversize body (Content-Length), then stream chunks and abort
        // the moment the running total would exceed the cap — so a buggy/oversized
        // (or chunked, no-Content-Length) Pat response cannot force unbounded work.
        if let Some(len) = resp.content_length() {
            if len > self.max_read_bytes as u64 {
                return Err(PatClientError::TooLarge { cap: self.max_read_bytes });
            }
        }
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(PatClientError::Http)? {
            if buf.len().saturating_add(chunk.len()) > self.max_read_bytes {
                return Err(PatClientError::TooLarge { cap: self.max_read_bytes });
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }

    pub async fn send(&self, to: &[&str], subject: &str, body: &str, date: &str) -> Result<(), PatClientError> {
        let mut form = reqwest::multipart::Form::new()
            .text("subject", subject.to_string())
            .text("body", body.to_string())
            .text("date", date.to_string());
        for addr in to {
            form = form.text("to", addr.to_string());
        }
        let url = format!("{}/api/mailbox/out", self.base_url);
        let resp = self.http.post(&url).multipart(form).send().await.map_err(PatClientError::Http)?;
        if !resp.status().is_success() {
            return Err(PatClientError::Status(resp.status().as_u16()));
        }
        Ok(())
    }
}
