//! The native on-disk message store — Pat-independent.
//!
//! Tuxlink's native Winlink client keeps its own mailbox so it does not depend
//! on Pat at all. Each message is stored as its raw Winlink bytes in a file
//! named after its message id, under a directory per folder
//! (`inbox/`, `sent/`, `outbox/`, `archive/`). Listing a folder parses each
//! file's headers into a [`MessageMeta`]; reading returns the raw bytes.
//!
//! The on-disk format is deliberately simple (raw message bytes per file) and is
//! ours, not Pat's — Pat is removed once the native client reaches parity. A
//! one-time import of existing Pat `.b2f` messages can be layered on later; it
//! is not required for the store to work.

use std::fs;
use std::path::PathBuf;

use crate::winlink::message::Message;
use crate::winlink_backend::{BackendError, MailboxFolder, MessageBody, MessageId, MessageMeta};

/// A native message store rooted at a directory.
pub struct Mailbox {
    root: PathBuf,
}

impl Mailbox {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Store a raw Winlink message in a folder, keyed by its message id (taken
    /// from the `Mid` header). Returns that id.
    pub fn store(&self, folder: MailboxFolder, raw: &[u8]) -> Result<MessageId, BackendError> {
        let msg = Message::from_bytes(raw)
            .map_err(|_| BackendError::MessageRejected("stored bytes are not a message".into()))?;
        let mid = msg
            .header("Mid")
            .ok_or_else(|| BackendError::MessageRejected("message has no Mid".into()))?
            .to_string();

        let dir = self.folder_dir(folder);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(format!("{mid}.b2f")), raw)?;
        Ok(MessageId(mid))
    }

    /// List the messages in a folder (header-only view). A missing folder lists
    /// as empty.
    pub fn list(&self, folder: MailboxFolder) -> Result<Vec<MessageMeta>, BackendError> {
        let dir = self.folder_dir(folder);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut metas = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("b2f") {
                continue;
            }
            let raw = fs::read(&path)?;
            if let Ok(msg) = Message::from_bytes(&raw) {
                metas.push(meta_from_message(&msg));
            }
        }
        Ok(metas)
    }

    /// Read one message's raw bytes from a folder.
    pub fn read(
        &self,
        folder: MailboxFolder,
        id: &MessageId,
    ) -> Result<MessageBody, BackendError> {
        let path = self.folder_dir(folder).join(format!("{}.b2f", id.0));
        let raw = fs::read(&path).map_err(|_| BackendError::NotFound(id.clone()))?;
        Ok(MessageBody {
            id: id.clone(),
            raw_rfc5322: raw,
        })
    }

    /// Move a message from one folder to another (e.g. outbox → sent once it has
    /// been delivered). No-op-safe if the source file is missing.
    pub fn move_to(
        &self,
        from: MailboxFolder,
        to: MailboxFolder,
        id: &MessageId,
    ) -> Result<(), BackendError> {
        let src = self.folder_dir(from).join(format!("{}.b2f", id.0));
        let raw = match fs::read(&src) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let dst_dir = self.folder_dir(to);
        fs::create_dir_all(&dst_dir)?;
        fs::write(dst_dir.join(format!("{}.b2f", id.0)), raw)?;
        fs::remove_file(&src)?;
        Ok(())
    }

    fn folder_dir(&self, folder: MailboxFolder) -> PathBuf {
        let name = match folder {
            MailboxFolder::Inbox => "inbox",
            MailboxFolder::Sent => "sent",
            MailboxFolder::Outbox => "outbox",
            MailboxFolder::Archive => "archive",
        };
        self.root.join(name)
    }
}

/// Build the header-only list view from a parsed message.
fn meta_from_message(msg: &Message) -> MessageMeta {
    let body_size = msg
        .header("Body")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(msg.body().len() as u64);
    MessageMeta {
        id: MessageId(msg.header("Mid").unwrap_or_default().to_string()),
        subject: msg.header("Subject").unwrap_or_default().to_string(),
        from: msg.header("From").unwrap_or_default().to_string(),
        // Native populates recipients (Pat's list DTO can't) — one per To line.
        to: msg.header_all("To").iter().map(|s| s.to_string()).collect(),
        date: winlink_date_to_rfc3339(msg.header("Date").unwrap_or_default()),
        unread: false, // read-state tracking is a later refinement
        body_size,
        has_attachments: false, // attachment parsing is a later step
    }
}

/// Convert a Winlink date header (`YYYY/MM/DD HH:MM`) to the RFC 3339 form the
/// trait's `MessageMeta::date` expects. Anything not in that exact shape is
/// passed through unchanged.
fn winlink_date_to_rfc3339(winlink: &str) -> String {
    // "2024/05/20 10:13" -> "2024-05-20T10:13:00Z"
    match winlink.split_once(' ') {
        Some((date, time)) if date.matches('/').count() == 2 && time.contains(':') => {
            format!("{}T{}:00Z", date.replace('/', "-"), time)
        }
        _ => winlink.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::compose::compose_message;
    use tempfile::tempdir;

    fn raw(subject: &str, body: &str) -> Vec<u8> {
        compose_message("N7CPZ", &["W1AW"], &[], subject, body, 1_716_200_000).to_bytes()
    }

    #[test]
    fn stores_then_lists_and_reads_a_message() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());

        let id = mbox.store(MailboxFolder::Inbox, &raw("Hello", "Body text")).unwrap();
        assert_eq!(id.0, "LIHHQU663POB");

        let metas = mbox.list(MailboxFolder::Inbox).unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].id.0, "LIHHQU663POB");
        assert_eq!(metas[0].subject, "Hello");
        assert_eq!(metas[0].from, "N7CPZ");
        assert_eq!(metas[0].to, vec!["W1AW".to_string()]);
        assert_eq!(metas[0].date, "2024-05-20T10:13:00Z");

        let body = mbox.read(MailboxFolder::Inbox, &id).unwrap();
        assert_eq!(body.raw_rfc5322, raw("Hello", "Body text"));
    }

    #[test]
    fn listing_a_missing_folder_yields_nothing() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        assert!(mbox.list(MailboxFolder::Sent).unwrap().is_empty());
    }

    #[test]
    fn folders_are_independent() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        mbox.store(MailboxFolder::Outbox, &raw("Out", "x")).unwrap();
        assert_eq!(mbox.list(MailboxFolder::Outbox).unwrap().len(), 1);
        assert!(mbox.list(MailboxFolder::Inbox).unwrap().is_empty());
    }

    #[test]
    fn moves_a_message_between_folders() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let id = mbox.store(MailboxFolder::Outbox, &raw("Out", "x")).unwrap();

        mbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &id).unwrap();

        assert!(mbox.list(MailboxFolder::Outbox).unwrap().is_empty());
        assert_eq!(mbox.list(MailboxFolder::Sent).unwrap().len(), 1);
        // Moving a missing id is a no-op, not an error.
        mbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &id).unwrap();
    }

    #[test]
    fn reading_a_missing_message_is_not_found() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let err = mbox.read(MailboxFolder::Inbox, &MessageId::new("NOPE")).unwrap_err();
        assert!(matches!(err, BackendError::NotFound(_)));
    }
}
