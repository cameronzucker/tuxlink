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
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::user_folders::{self, UserFolder};
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
    ///
    /// Result is sorted newest-first (descending by [`MessageMeta::date`]),
    /// with [`MessageMeta::id`] ascending as a deterministic tiebreaker for
    /// messages whose dates collide at the minute-resolution Winlink stores.
    /// Operator-load-bearing default per tuxlink-mjc8: without an explicit
    /// sort, `fs::read_dir` yields filesystem-hash order — effectively random
    /// to the operator — and search-but-no-default-order makes the inbox
    /// unusable. Messages whose date doesn't parse as RFC 3339 sort to the
    /// bottom of the list rather than anchoring to the 1970 epoch.
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
        metas.sort_by(|a, b| {
            let ka = sort_key_from_rfc3339(&a.date);
            let kb = sort_key_from_rfc3339(&b.date);
            // Newest first: `Some(later).cmp(&Some(earlier)) == Greater`, so
            // `kb.cmp(&ka)` returns Greater when `b` is older → `a` sorts
            // first. `Option::None` is less than any `Some(_)`, so an
            // unparseable date falls to the bottom of the newest-first list.
            // Id ascending breaks ties deterministically (Winlink Date headers
            // are minute-resolution, so a single batched receive can collide).
            kb.cmp(&ka).then_with(|| a.id.0.cmp(&b.id.0))
        });
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

    // ========================================================================
    // User folders (tuxlink-f62f — Phase 2 of the user-folders work).
    //
    // Spec: docs/superpowers/specs/2026-06-02-user-folders-design.md §3.1.
    // System folders (Inbox/Sent/Outbox/Archive) live in the closed
    // `MailboxFolder` enum and use `folder_dir`; user folders are open-set
    // string-keyed slugs that live alongside, under `<root>/<slug>/`. The
    // `.folders.json` registry at the mailbox root tracks display names +
    // creation times.
    // ========================================================================

    /// List the user folders as recorded in `<root>/.folders.json`, sorted by
    /// creation time ascending (so first-created sticks to the top). Missing
    /// registry → empty list (first-launch path is normal).
    pub fn list_user_folders(&self) -> Vec<UserFolder> {
        let mut reg = user_folders::load_registry(&self.root);
        reg.folders.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        reg.folders
    }

    /// Create a new user folder. Validates the display name, derives a slug,
    /// rejects collisions with system folder names + existing user folder
    /// slugs, then creates the on-disk directory + persists the registry.
    /// Returns the newly created `UserFolder` so the caller can echo back to
    /// the UI (no extra round-trip).
    pub fn create_user_folder(&self, display_name: &str) -> Result<UserFolder, BackendError> {
        let display = display_name.trim();
        user_folders::validate_display_name(display)
            .map_err(BackendError::MessageRejected)?;
        let slug = user_folders::slug_from_display(display);
        user_folders::validate_slug(&slug).map_err(BackendError::MessageRejected)?;

        let mut reg = user_folders::load_registry(&self.root);
        for existing in &reg.folders {
            if existing.slug == slug {
                return Err(BackendError::MessageRejected(format!(
                    "a folder with that name already exists ('{slug}')"
                )));
            }
        }

        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let folder = UserFolder {
            slug: slug.clone(),
            display_name: display.to_string(),
            created_at: now,
        };

        // Create the directory FIRST — if the FS write fails we don't poison
        // the registry with a folder whose dir doesn't exist.
        let dir = user_folders::folder_dir(&self.root, &slug);
        fs::create_dir_all(&dir)?;

        reg.folders.push(folder.clone());
        user_folders::save_registry(&self.root, &reg)?;
        Ok(folder)
    }

    /// Rename a user folder. Only the display name changes — the slug stays
    /// stable so messages don't have to move on disk (spec §3.1). Validates
    /// the new display name + reserved-name list. Missing slug → `NotFound`.
    pub fn rename_user_folder(
        &self,
        slug: &str,
        new_display_name: &str,
    ) -> Result<UserFolder, BackendError> {
        let display = new_display_name.trim();
        user_folders::validate_display_name(display)
            .map_err(BackendError::MessageRejected)?;
        let mut reg = user_folders::load_registry(&self.root);
        let folder = reg
            .folders
            .iter_mut()
            .find(|f| f.slug == slug)
            .ok_or_else(|| crate::winlink_backend::BackendError::NotFound(
                crate::winlink_backend::MessageId(slug.into()),
            ))?;
        folder.display_name = display.to_string();
        let renamed = folder.clone();
        user_folders::save_registry(&self.root, &reg)?;
        Ok(renamed)
    }

    /// Delete a user folder. `on_messages` controls what happens to messages
    /// inside (spec §6 D6):
    /// - `MoveToInbox` (safe default) — re-home each `.b2f` to the inbox dir
    /// - `MoveToArchive` — re-home each `.b2f` to the archive dir
    /// - `Delete` — remove the directory and its contents
    ///
    /// Either way, the folder directory is removed and the registry entry
    /// erased on success. Missing folder → no-op-safe Ok.
    pub fn delete_user_folder(
        &self,
        slug: &str,
        on_messages: DeleteAction,
    ) -> Result<(), BackendError> {
        let dir = user_folders::folder_dir(&self.root, slug);
        let mut reg = user_folders::load_registry(&self.root);
        let in_registry = reg.folders.iter().any(|f| f.slug == slug);

        if dir.exists() {
            match on_messages {
                DeleteAction::Delete => {
                    fs::remove_dir_all(&dir)?;
                }
                DeleteAction::MoveToInbox | DeleteAction::MoveToArchive => {
                    let dst_dir = self.folder_dir(match on_messages {
                        DeleteAction::MoveToInbox => MailboxFolder::Inbox,
                        DeleteAction::MoveToArchive => MailboxFolder::Archive,
                        DeleteAction::Delete => unreachable!(),
                    });
                    fs::create_dir_all(&dst_dir)?;
                    for entry in fs::read_dir(&dir)? {
                        let path = entry?.path();
                        if let Some(name) = path.file_name() {
                            let dst = dst_dir.join(name);
                            fs::rename(&path, &dst)?;
                        }
                    }
                    fs::remove_dir_all(&dir)?;
                }
            }
        }

        if in_registry {
            reg.folders.retain(|f| f.slug != slug);
            user_folders::save_registry(&self.root, &reg)?;
        }
        Ok(())
    }

    /// List messages in a user folder. Mirrors [`Mailbox::list`]'s sort order
    /// (newest first, id ascending as tiebreaker). User folders don't track
    /// unread state today; every message reports `unread: false`.
    pub fn list_user(&self, slug: &str) -> Result<Vec<MessageMeta>, BackendError> {
        let dir = user_folders::folder_dir(&self.root, slug);
        Self::list_dir(&dir, /*surface_unread=*/ false)
    }

    /// Read a raw message from a user folder. Returns `NotFound` if the slug
    /// or mid is unknown.
    pub fn read_user(&self, slug: &str, id: &MessageId) -> Result<MessageBody, BackendError> {
        let path = user_folders::folder_dir(&self.root, slug).join(format!("{}.b2f", id.0));
        let raw = fs::read(&path).map_err(|_| BackendError::NotFound(id.clone()))?;
        Ok(MessageBody { id: id.clone(), raw_rfc5322: raw })
    }

    /// Move a message between any two folders, where each side is a folder
    /// reference (system or user). The MVP move primitive — spec §4.4.
    ///
    /// Both source and destination are validated (system folders by the
    /// enum, user folders by the registry membership check). Source missing
    /// → no-op-safe Ok (matches `Mailbox::move_to`). Read-marker travels
    /// with the message.
    pub fn move_between(
        &self,
        from: FolderRef,
        to: FolderRef,
        id: &MessageId,
    ) -> Result<(), BackendError> {
        let src_dir = self.resolve_dir(&from);
        let dst_dir = self.resolve_dir(&to);
        let src = src_dir.join(format!("{}.b2f", id.0));
        let raw = match fs::read(&src) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        fs::create_dir_all(&dst_dir)?;
        fs::write(dst_dir.join(format!("{}.b2f", id.0)), raw)?;
        fs::remove_file(&src)?;
        // Carry the read-marker if present.
        let src_marker = src_dir.join(format!("{}.read", id.0));
        if src_marker.exists() {
            fs::write(dst_dir.join(format!("{}.read", id.0)), [])?;
            fs::remove_file(&src_marker)?;
        }

        // Best-effort search-index update. The destination folder string is
        // either the system-folder dir name (so `update_folder` matches the
        // existing index column convention) or the user-folder slug.
        if let Some(idx) = self.index.as_ref() {
            let dst_str = match &to {
                FolderRef::System(f) => folder_str(*f).to_string(),
                FolderRef::User(slug) => slug.clone(),
            };
            match idx.lock() {
                Ok(guard) => {
                    if let Err(e) = guard.update_folder(&id.0, &dst_str) {
                        eprintln!("search-index update_folder failed for mid={}: {e}", id.0);
                    }
                }
                Err(e) => eprintln!("search-index lock poisoned during update_folder: {e}"),
            }
        }
        Ok(())
    }

    fn resolve_dir(&self, r: &FolderRef) -> PathBuf {
        match r {
            FolderRef::System(f) => self.folder_dir(*f),
            FolderRef::User(slug) => user_folders::folder_dir(&self.root, slug),
        }
    }

    /// Shared list-dir helper used by both system and user folder listing.
    /// Returns metadatas sorted newest-first with id ascending as tiebreaker.
    /// `surface_unread` controls whether a missing `.read` sidecar marks the
    /// message unread — only the inbox surfaces this today (spec §2.1).
    fn list_dir(dir: &Path, surface_unread: bool) -> Result<Vec<MessageMeta>, BackendError> {
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut metas = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("b2f") {
                continue;
            }
            let raw = fs::read(&path)?;
            if let Ok(msg) = Message::from_bytes(&raw) {
                let mut meta = meta_from_message(&msg);
                meta.unread = surface_unread && !path.with_extension("read").exists();
                metas.push(meta);
            }
        }
        metas.sort_by(|a, b| {
            let ka = sort_key_from_rfc3339(&a.date);
            let kb = sort_key_from_rfc3339(&b.date);
            kb.cmp(&ka).then_with(|| a.id.0.cmp(&b.id.0))
        });
        Ok(metas)
    }
}

/// What to do with messages remaining in a user folder when the folder is
/// deleted (spec §6 D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteAction {
    /// Move each message to Inbox (safe default).
    MoveToInbox,
    /// Move each message to Archive.
    MoveToArchive,
    /// Permanently delete each message.
    Delete,
}

/// A folder reference that can be either a system folder (closed enum) or a
/// user folder (open-set slug). Used by [`Mailbox::move_between`] so the move
/// primitive supports all four combinations (system↔system, system↔user,
/// user↔system, user↔user) under a single API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderRef {
    System(MailboxFolder),
    User(String),
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

/// Parse an RFC 3339 timestamp into seconds since the epoch, for use as a
/// `MessageMeta::date` sort key. `None` (unparseable) sorts to the bottom of a
/// newest-first list rather than to a default-zero "1970" anchor that would
/// pin garbage-dated messages to the bottom only by accident.
fn sort_key_from_rfc3339(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.timestamp())
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

    fn raw_at(subject: &str, body: &str, ts: u64) -> Vec<u8> {
        compose_message("N7CPZ", &["W1AW"], &[], subject, body, ts).to_bytes()
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

    // tuxlink-mjc8: operator-load-bearing default — list returns newest first
    // regardless of fs::read_dir order. Three timestamps stored in arbitrary
    // sequence; the result must be strictly descending by date.
    #[test]
    fn list_returns_messages_newest_first_by_date() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        // Three timestamps minutes apart so the Winlink minute-resolution
        // Date header carries distinct values for all three.
        let oldest = 1_716_200_000; // 2024-05-20T10:13Z → header "2024/05/20 10:13"
        let middle = oldest + 600; // +10 min
        let newest = oldest + 1200; // +20 min
        // Store out-of-order: middle, oldest, newest — so any "preserved
        // insertion order" implementation would land middle-first.
        mbox.store(MailboxFolder::Inbox, &raw_at("Middle", "m", middle)).unwrap();
        mbox.store(MailboxFolder::Inbox, &raw_at("Oldest", "o", oldest)).unwrap();
        mbox.store(MailboxFolder::Inbox, &raw_at("Newest", "n", newest)).unwrap();

        let metas = mbox.list(MailboxFolder::Inbox).unwrap();
        assert_eq!(metas.len(), 3);
        assert_eq!(metas[0].subject, "Newest", "first row must be the newest message");
        assert_eq!(metas[1].subject, "Middle");
        assert_eq!(metas[2].subject, "Oldest", "last row must be the oldest");
    }

    // tuxlink-mjc8: messages with identical (minute-resolution) Date headers
    // must sort by id ascending — deterministic across runs + filesystems.
    // Winlink Date headers carry minute resolution and `generate_mid` keys
    // on `(unix_secs, callsign)`, so three timestamps in the same minute
    // (different seconds) produce identical Date headers but distinct MIDs
    // — exactly the tie-break case the operator hits when a single CMS
    // exchange ingests multiple messages within one minute.
    #[test]
    fn list_tiebreaks_equal_dates_by_id_ascending() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let base = 1_716_200_000; // 2024-05-20 10:13:20 UTC
        let id_0 = mbox.store(MailboxFolder::Inbox, &raw_at("Sec20", "x", base)).unwrap();
        let id_1 = mbox.store(MailboxFolder::Inbox, &raw_at("Sec21", "x", base + 1)).unwrap();
        let id_2 = mbox.store(MailboxFolder::Inbox, &raw_at("Sec22", "x", base + 2)).unwrap();
        let metas = mbox.list(MailboxFolder::Inbox).unwrap();
        assert_eq!(metas.len(), 3, "three distinct MIDs from same-minute timestamps");
        // All three carry the same minute-resolution Date header
        // ("2024-05-20T10:13:00Z"), so the tiebreaker (id asc) determines
        // the displayed order.
        assert!(
            metas.iter().all(|m| m.date == "2024-05-20T10:13:00Z"),
            "fixture must collide at the minute (was: {:?})",
            metas.iter().map(|m| &m.date).collect::<Vec<_>>()
        );
        let mut expected = vec![id_0.0.clone(), id_1.0.clone(), id_2.0.clone()];
        expected.sort();
        let actual: Vec<String> = metas.iter().map(|m| m.id.0.clone()).collect();
        assert_eq!(actual, expected, "equal-date tiebreaker must be id ascending");
    }

    // tuxlink-mjc8: a message whose Date header doesn't parse as RFC 3339
    // sorts to the bottom of a newest-first list — anchoring it at the
    // 1970 epoch (the alternative Option-default) would be just as random
    // as the current OS-order bug.
    #[test]
    fn sort_key_from_rfc3339_returns_none_for_unparseable() {
        assert_eq!(sort_key_from_rfc3339(""), None);
        assert_eq!(sort_key_from_rfc3339("not a date"), None);
        assert_eq!(sort_key_from_rfc3339("2024/05/20 10:13"), None, "Winlink raw form is not RFC 3339");
        // Properly-shaped RFC 3339 parses to its epoch second.
        assert_eq!(sort_key_from_rfc3339("2024-05-20T10:13:00Z"), Some(1_716_199_980));
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

    // tuxlink-ca5x: Inbox → Archive is the canonical user-facing move (the
    // Archive button + A shortcut). The file moves to the archive directory,
    // the inbox copy is gone, the index folder column updates, and a read-marker
    // (if present) travels along so read-state isn't lost. (Note: the on-disk
    // segment name is "inbox" here, not the "in" form that surfaces via
    // winlink_backend::as_path_segment — `native_mailbox::folder_dir` uses the
    // longer form.)
    #[test]
    fn move_inbox_to_archive_relocates_message_and_marker_and_index() {
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());
        let id = mbox.store(MailboxFolder::Inbox, &raw("hello", "body")).unwrap();
        mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();
        // Sanity: marker exists in inbox before move.
        assert!(dir.path().join("inbox").join(format!("{}.read", id.0)).exists());

        mbox.move_to(MailboxFolder::Inbox, MailboxFolder::Archive, &id).unwrap();

        // The b2f file lives in archive/ and is gone from inbox/.
        assert!(dir.path().join("archive").join(format!("{}.b2f", id.0)).exists());
        assert!(!dir.path().join("inbox").join(format!("{}.b2f", id.0)).exists());
        // The read marker traveled with the message — no orphan in inbox/.
        assert!(dir.path().join("archive").join(format!("{}.read", id.0)).exists());
        assert!(!dir.path().join("inbox").join(format!("{}.read", id.0)).exists());

        // Search index reflects the new folder.
        let folder: String = idx
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT folder FROM messages_meta WHERE mid = ?1", [&id.0], |r| r.get(0))
            .unwrap();
        assert_eq!(folder, "archive");
    }

    // tuxlink-ca5x: moving a missing message is a no-op-safe Ok, not an error.
    // The Archive button race window — user clicks Archive twice quickly, the
    // second backend call would otherwise see the file already gone — closes
    // cleanly without surfacing an error.
    #[test]
    fn move_nonexistent_message_is_ok_safe() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let res = mbox.move_to(
            MailboxFolder::Inbox,
            MailboxFolder::Archive,
            &MessageId("does-not-exist".to_string()),
        );
        assert!(res.is_ok(), "moving a missing message must be a no-op-safe Ok");
    }

    // ========================================================================
    // tuxlink-f62f: user-folder lifecycle + cross-kind move integration tests.
    // ========================================================================

    #[test]
    fn user_folder_create_list_delete_roundtrip() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());

        // Empty at first launch.
        assert!(mbox.list_user_folders().is_empty());

        // Create two folders.
        let ares = mbox.create_user_folder("ARES Drills").unwrap();
        assert_eq!(ares.slug, "ares-drills");
        assert_eq!(ares.display_name, "ARES Drills");
        let prep = mbox.create_user_folder("Disaster Prep").unwrap();
        assert_eq!(prep.slug, "disaster-prep");

        // Listed in creation order.
        let list = mbox.list_user_folders();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].slug, "ares-drills");
        assert_eq!(list[1].slug, "disaster-prep");

        // The on-disk directories exist.
        assert!(dir.path().join("ares-drills").is_dir());
        assert!(dir.path().join("disaster-prep").is_dir());
        // The registry file exists.
        assert!(dir.path().join(".folders.json").exists());

        // Delete with Delete cascade (no messages inside; safe).
        mbox.delete_user_folder("ares-drills", DeleteAction::Delete).unwrap();
        let after = mbox.list_user_folders();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].slug, "disaster-prep");
        assert!(!dir.path().join("ares-drills").exists());
    }

    #[test]
    fn create_rejects_reserved_names_and_duplicates() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());

        // Reserved system names (case-insensitive).
        assert!(mbox.create_user_folder("Inbox").is_err());
        assert!(mbox.create_user_folder("ARCHIVE").is_err());

        // First create OK, duplicate rejected.
        mbox.create_user_folder("ARES Drills").unwrap();
        assert!(mbox.create_user_folder("ARES Drills").is_err());
        // Same slug from a different display would also collide.
        assert!(mbox.create_user_folder("ares drills").is_err());
    }

    #[test]
    fn move_between_inbox_and_user_folder_relocates_message() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let id = mbox.store(MailboxFolder::Inbox, &raw("hello", "body")).unwrap();
        let _ = mbox.create_user_folder("ARES Drills").unwrap();

        // Inbox → user folder.
        mbox.move_between(
            FolderRef::System(MailboxFolder::Inbox),
            FolderRef::User("ares-drills".into()),
            &id,
        )
        .unwrap();
        assert!(dir.path().join("ares-drills").join(format!("{}.b2f", id.0)).exists());
        assert!(!dir.path().join("inbox").join(format!("{}.b2f", id.0)).exists());

        // User folder → Archive.
        mbox.move_between(
            FolderRef::User("ares-drills".into()),
            FolderRef::System(MailboxFolder::Archive),
            &id,
        )
        .unwrap();
        assert!(dir.path().join("archive").join(format!("{}.b2f", id.0)).exists());
        assert!(!dir.path().join("ares-drills").join(format!("{}.b2f", id.0)).exists());
    }

    #[test]
    fn delete_user_folder_with_move_to_inbox_relocates_messages() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let _ = mbox.create_user_folder("ARES Drills").unwrap();
        // Plant a message in the user folder via the move primitive.
        let id = mbox.store(MailboxFolder::Inbox, &raw("hello", "body")).unwrap();
        mbox.move_between(
            FolderRef::System(MailboxFolder::Inbox),
            FolderRef::User("ares-drills".into()),
            &id,
        )
        .unwrap();

        // Delete with MoveToInbox cascade.
        mbox.delete_user_folder("ares-drills", DeleteAction::MoveToInbox).unwrap();

        // Message is back in the inbox; user folder is gone.
        assert!(dir.path().join("inbox").join(format!("{}.b2f", id.0)).exists());
        assert!(!dir.path().join("ares-drills").exists());
        assert!(mbox.list_user_folders().is_empty());
    }

    #[test]
    fn delete_user_folder_with_delete_cascade_removes_messages() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let _ = mbox.create_user_folder("ARES Drills").unwrap();
        let id = mbox.store(MailboxFolder::Inbox, &raw("hello", "body")).unwrap();
        mbox.move_between(
            FolderRef::System(MailboxFolder::Inbox),
            FolderRef::User("ares-drills".into()),
            &id,
        )
        .unwrap();

        mbox.delete_user_folder("ares-drills", DeleteAction::Delete).unwrap();
        assert!(!dir.path().join("ares-drills").exists());
        assert!(!dir.path().join("inbox").join(format!("{}.b2f", id.0)).exists());
        assert!(mbox.list_user_folders().is_empty());
    }

    // tuxlink-ejph: renaming a user folder updates display_name but NOT the
    // slug — the on-disk dir name stays the same so messages don't have to
    // move. Subsequent list_user_folders() reflects the new name.
    #[test]
    fn rename_user_folder_updates_display_name_only() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let f = mbox.create_user_folder("ARES Drills").unwrap();
        assert_eq!(f.slug, "ares-drills");

        let renamed = mbox.rename_user_folder("ares-drills", "June Drills").unwrap();
        assert_eq!(renamed.slug, "ares-drills", "slug must stay stable");
        assert_eq!(renamed.display_name, "June Drills");

        // The on-disk directory still uses the original slug (no churn).
        assert!(dir.path().join("ares-drills").is_dir());

        // Registry persists the new display name.
        let list = mbox.list_user_folders();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].display_name, "June Drills");
        assert_eq!(list[0].slug, "ares-drills");
    }

    #[test]
    fn rename_user_folder_rejects_reserved_names_and_missing_slug() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        mbox.create_user_folder("ARES Drills").unwrap();

        // Reserved system folder names rejected.
        assert!(mbox.rename_user_folder("ares-drills", "Inbox").is_err());

        // Unknown slug → NotFound.
        assert!(mbox.rename_user_folder("nope", "Whatever").is_err());
    }

    #[test]
    fn list_user_returns_empty_for_unknown_slug() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let metas = mbox.list_user("nope").unwrap();
        assert!(metas.is_empty());
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
