use serde::Deserialize;

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

pub struct PatClient {
    base_url: String,
    http: reqwest::blocking::Client,
}

impl PatClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build().expect("reqwest build");
        PatClient { base_url: base_url.into(), http }
    }

    pub fn list(&self, folder: MailboxFolder) -> Result<Vec<Message>, PatClientError> {
        let url = format!("{}/api/mailbox/{}", self.base_url, folder.as_path());
        let resp = self.http.get(&url).send().map_err(PatClientError::Http)?;
        if !resp.status().is_success() {
            return Err(PatClientError::Status(resp.status().as_u16()));
        }
        let dtos: Vec<PatMessageDto> = resp.json().map_err(PatClientError::Http)?;
        Ok(dtos.into_iter().map(Message::from).collect())
    }

    pub fn send(&self, to: &[&str], subject: &str, body: &str) -> Result<String, PatClientError> {
        #[derive(serde::Serialize)]
        struct Out<'a> {
            #[serde(rename = "To")] to: Vec<Addr<'a>>,
            #[serde(rename = "Subject")] subject: &'a str,
            #[serde(rename = "Body")] body: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Addr<'a> { #[serde(rename = "Addr")] addr: &'a str }
        #[derive(Deserialize)]
        struct Resp { #[serde(rename = "MID")] mid: String }

        let msg = Out {
            to: to.iter().map(|a| Addr { addr: a }).collect(),
            subject, body,
        };
        let url = format!("{}/api/mailbox/out", self.base_url);
        let resp = self.http.post(&url).json(&msg).send().map_err(PatClientError::Http)?;
        if !resp.status().is_success() {
            return Err(PatClientError::Status(resp.status().as_u16()));
        }
        let body: Resp = resp.json().map_err(PatClientError::Http)?;
        Ok(body.mid)
    }
}
