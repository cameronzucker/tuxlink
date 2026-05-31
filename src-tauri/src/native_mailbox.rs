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
use std::sync::{Arc, Mutex};

use crate::winlink::message::Message;
use crate::winlink_backend::{BackendError, MailboxFolder, MessageBody, MessageId, MessageMeta};

/// A native message store rooted at a directory.
pub struct Mailbox {
    root: PathBuf,
    /// Search index, wrapped in a Mutex because `rusqlite::Connection` is not
    /// `Sync`. `Mailbox` itself must be `Sync` (it is held as `Arc<Mailbox>`
    /// inside `NativeBackend: Send + Sync`). The Mutex makes every index call
    /// exclusive, which is fine — index operations are fast and infrequent.
    index: Option<Arc<Mutex<crate::search::index::Index>>>,
}

impl Mailbox {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), index: None }
    }

    /// Attach a search index. After each successful filesystem write, the
    /// mailbox dispatches a best-effort index update. Index errors are logged
    /// but never propagated — the filesystem write is canonical (spec §8).
    pub fn with_index(mut self, index: Arc<Mutex<crate::search::index::Index>>) -> Self {
        self.index = Some(index);
        self
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

        // Best-effort index hook — filesystem write already succeeded above.
        // Index errors are logged but never propagated (spec §8).
        if let Some(idx) = self.index.as_ref() {
            let row = crate::search::extractor::extract(
                &msg,
                folder,
                direction_for_folder(folder),
                /*unread=*/ folder == MailboxFolder::Inbox,
                /*transport_used=*/ None,
            );
            match idx.lock() {
                Ok(guard) => {
                    if let Err(e) = guard.upsert(&row) {
                        eprintln!("search-index upsert failed for mid={}: {e}", row.mid);
                    }
                }
                Err(e) => eprintln!("search-index lock poisoned during upsert: {e}"),
            }
        }

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
                let mut meta = meta_from_message(&msg);
                // Unread is a received-mail concept: only the Inbox surfaces it
                // (the Mock B sidebar shows Sent as a total, not an unread
                // count). A message is unread until a `<mid>.read` sidecar marks
                // it read.
                meta.unread =
                    folder == MailboxFolder::Inbox && !path.with_extension("read").exists();
                metas.push(meta);
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
        // Carry the read-marker so read-state follows the message and no orphan
        // marker is left behind in the source folder.
        let src_marker = self.folder_dir(from).join(format!("{}.read", id.0));
        if src_marker.exists() {
            fs::write(dst_dir.join(format!("{}.read", id.0)), [])?;
            fs::remove_file(&src_marker)?;
        }

        // Best-effort index hook — filesystem move already succeeded above.
        if let Some(idx) = self.index.as_ref() {
            match idx.lock() {
                Ok(guard) => {
                    if let Err(e) = guard.update_folder(&id.0, folder_str(to)) {
                        eprintln!("search-index update_folder failed for mid={}: {e}", id.0);
                    }
                }
                Err(e) => eprintln!("search-index lock poisoned during update_folder: {e}"),
            }
        }

        Ok(())
    }

    /// Mark a message read by dropping an empty `<mid>.read` sidecar next to its
    /// `<mid>.b2f`. Tolerant: a message with no file on disk is a no-op (it may
    /// have been moved or removed between the list view and the open), never an
    /// error. Read-state is only *surfaced* for the Inbox (see [`Mailbox::list`]),
    /// but the marker is written for whatever folder is given so it can travel
    /// with the message in [`Mailbox::move_to`].
    pub fn mark_read(&self, folder: MailboxFolder, id: &MessageId) -> Result<(), BackendError> {
        let dir = self.folder_dir(folder);
        if !dir.join(format!("{}.b2f", id.0)).exists() {
            return Ok(());
        }
        fs::write(dir.join(format!("{}.read", id.0)), [])?;

        // Best-effort index hook — filesystem write already succeeded above.
        if let Some(idx) = self.index.as_ref() {
            match idx.lock() {
                Ok(guard) => {
                    if let Err(e) = guard.update_unread(&id.0, false) {
                        eprintln!("search-index update_unread failed for mid={}: {e}", id.0);
                    }
                }
                Err(e) => eprintln!("search-index lock poisoned during update_unread: {e}"),
            }
        }

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

fn direction_for_folder(f: MailboxFolder) -> crate::search::extractor::Direction {
    match f {
        MailboxFolder::Sent | MailboxFolder::Outbox => crate::search::extractor::Direction::Sent,
        _ => crate::search::extractor::Direction::Received,
    }
}

fn folder_str(f: MailboxFolder) -> &'static str {
    match f {
        MailboxFolder::Inbox => "inbox",
        MailboxFolder::Outbox => "outbox",
        MailboxFolder::Sent => "sent",
        MailboxFolder::Archive => "archive",
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
        // Placeholder; the real value is set by `list`, which is the only
        // caller and knows the folder + read-marker state (tuxlink-xgn).
        unread: false,
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

    // ---- read/unread state (tuxlink-xgn) -----------------------------------

    #[test]
    fn an_inbox_message_is_unread_until_marked() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        mbox.store(MailboxFolder::Inbox, &raw("Hello", "Body text")).unwrap();

        let metas = mbox.list(MailboxFolder::Inbox).unwrap();
        assert!(metas[0].unread, "a freshly stored inbox message should be unread");
    }

    #[test]
    fn non_inbox_folders_never_report_unread() {
        // Unread is a received-mail (Inbox) concept; the Mock B sidebar shows
        // Sent as a total, not an unread count. Sent/Outbox/Archive must always
        // report unread = false even with no read-marker on disk.
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        mbox.store(MailboxFolder::Sent, &raw("S", "x")).unwrap();
        mbox.store(MailboxFolder::Outbox, &raw("O", "y")).unwrap();

        assert!(!mbox.list(MailboxFolder::Sent).unwrap()[0].unread);
        assert!(!mbox.list(MailboxFolder::Outbox).unwrap()[0].unread);
    }

    #[test]
    fn marking_an_inbox_message_read_flips_it() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let id = mbox.store(MailboxFolder::Inbox, &raw("Hello", "Body text")).unwrap();
        assert!(mbox.list(MailboxFolder::Inbox).unwrap()[0].unread);

        mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();

        assert!(!mbox.list(MailboxFolder::Inbox).unwrap()[0].unread);
    }

    #[test]
    fn read_state_persists_across_mailbox_instances() {
        let dir = tempdir().unwrap();
        let id = {
            let mbox = Mailbox::new(dir.path());
            let id = mbox.store(MailboxFolder::Inbox, &raw("Hello", "Body text")).unwrap();
            mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();
            id
        };
        // A fresh Mailbox over the same root must still see the message as read.
        let reopened = Mailbox::new(dir.path());
        let metas = reopened.list(MailboxFolder::Inbox).unwrap();
        assert_eq!(metas[0].id, id);
        assert!(!metas[0].unread, "read state must persist on disk");
    }

    #[test]
    fn marking_a_missing_message_read_is_not_an_error() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        // No such message; marking read is a tolerant no-op (the message may
        // have been moved/removed between list and open).
        mbox.mark_read(MailboxFolder::Inbox, &MessageId::new("NOPE")).unwrap();
    }

    #[test]
    fn moving_a_message_carries_its_read_marker() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let id = mbox.store(MailboxFolder::Inbox, &raw("Hello", "x")).unwrap();
        mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();

        mbox.move_to(MailboxFolder::Inbox, MailboxFolder::Archive, &id).unwrap();

        // The marker follows the message; no orphan is left in the source.
        assert!(
            !dir.path().join("inbox").join(format!("{}.read", id.0)).exists(),
            "source read-marker should not be orphaned"
        );
        assert!(
            dir.path().join("archive").join(format!("{}.read", id.0)).exists(),
            "read-marker should travel with the message"
        );
    }
}

#[cfg(test)]
mod index_hook_tests {
    use super::*;
    use crate::search::index::Index;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    fn build_mailbox_with_index(dir: &std::path::Path) -> (Mailbox, Arc<Mutex<Index>>) {
        let idx = Arc::new(Mutex::new(Index::open(dir.join("search.db")).unwrap()));
        let mut mbox = Mailbox::new(dir.to_path_buf());
        mbox = mbox.with_index(idx.clone());
        (mbox, idx)
    }

    fn raw(subject: &str, body: &str) -> Vec<u8> {
        crate::winlink::compose::compose_message(
            "N7CPZ", &["W1AW"], &[], subject, body, 1_716_200_000,
        ).to_bytes()
    }

    #[test]
    fn store_upserts_into_index() {
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());
        mbox.store(MailboxFolder::Inbox, &raw("Hello", "body")).unwrap();
        assert_eq!(idx.lock().unwrap().count().unwrap(), 1);
    }

    #[test]
    fn move_to_updates_folder_in_index() {
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());
        let id = mbox.store(MailboxFolder::Outbox, &raw("x", "y")).unwrap();
        mbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &id).unwrap();
        let folder: String = idx
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT folder FROM messages_meta WHERE mid = ?1", [&id.0], |r| r.get(0))
            .unwrap();
        assert_eq!(folder, "sent");
    }

    #[test]
    fn mark_read_updates_unread_in_index() {
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());
        let id = mbox.store(MailboxFolder::Inbox, &raw("x", "y")).unwrap();
        mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();
        let unread: i64 = idx
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT unread FROM messages_meta WHERE mid = ?1", [&id.0], |r| r.get(0))
            .unwrap();
        assert_eq!(unread, 0);
    }

    #[test]
    fn index_failure_does_not_break_mailbox_store() {
        // Build an Index, then delete the file mid-test so the next index
        // upsert fails. The mailbox.store call must still return Ok —
        // find-messages never breaks mailbox writes (spec §8).
        let dir = tempdir().unwrap();
        let (mbox, _idx) = build_mailbox_with_index(dir.path());
        std::fs::remove_file(dir.path().join("search.db")).unwrap();
        let res = mbox.store(MailboxFolder::Inbox, &raw("x", "y"));
        assert!(res.is_ok(), "mailbox.store must not fail because of index errors");
    }
}
