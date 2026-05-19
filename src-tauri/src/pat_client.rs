use serde::Deserialize;

/// Mailbox folder selector. `#[non_exhaustive]` per tuxlink-z5f v2 P1 #5 —
/// future folders (Drafts, Spam, custom) added without breaking exhaustive
/// matches at call sites. `Copy + Clone + Debug` so the trait re-export
/// carries useful semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MailboxFolder { Inbox, Sent, Outbox, Archive }

impl MailboxFolder {
    fn as_path(&self) -> &'static str {
        match self {
            MailboxFolder::Inbox => "in",
            MailboxFolder::Sent => "sent",
            MailboxFolder::Outbox => "out",
            MailboxFolder::Archive => "archive",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub mid: String,
    pub subject: String,
    pub from: String,
    pub date: String,
    pub unread: bool,
    pub body_size: u64,
}

#[derive(Debug, Deserialize)]
struct PatMessageDto {
    #[serde(rename = "MID")] mid: String,
    #[serde(rename = "Subject")] subject: String,
    #[serde(rename = "From")] from: PatAddr,
    #[serde(rename = "Date")] date: String,
    #[serde(rename = "Unread", default)] unread: bool,
    #[serde(rename = "BodySize", default)] body_size: u64,
}

#[derive(Debug, Deserialize)]
struct PatAddr { #[serde(rename = "Addr")] addr: String }

impl From<PatMessageDto> for Message {
    fn from(d: PatMessageDto) -> Self {
        Message {
            mid: d.mid, subject: d.subject, from: d.from.addr,
            date: d.date, unread: d.unread, body_size: d.body_size,
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum PatClientError {
    Http(reqwest::Error),
    Status(u16),
}

impl std::fmt::Display for PatClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatClientError::Http(e) => write!(f, "HTTP error: {}", e),
            PatClientError::Status(s) => write!(f, "Pat returned status {}", s),
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
}

impl PatClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build().expect("reqwest build");
        PatClient { base_url: base_url.into(), http }
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
        let resp = self.http.get(&url).send().await.map_err(PatClientError::Http)?;
        if !resp.status().is_success() {
            return Err(PatClientError::Status(resp.status().as_u16()));
        }
        let bytes = resp.bytes().await.map_err(PatClientError::Http)?;
        Ok(bytes.to_vec())
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
