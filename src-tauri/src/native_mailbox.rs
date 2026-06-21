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

/// Validate a message id (`Mid`) before it is ever used to build a filesystem
/// path. SECURITY (pre-public review 2026-06-16, tuxlink-5lbm): an INBOUND
/// Winlink message's `Mid` header is attacker-controlled, and the store/read/
/// move paths interpolate it into `dir.join(format!("{mid}.b2f"))`. Without this
/// guard a crafted `Mid` such as `../../../x` — or an ABSOLUTE path like
/// `/home/user/.config/autostart/x`, which `Path::join` substitutes for the base
/// entirely — escapes the mailbox directory, yielding an arbitrary-file-write
/// primitive (the message body `raw` is attacker-controlled too). Real Winlink
/// ids are short alphanumeric tokens (our `generate_mid` emits 12 base32 chars),
/// so a strict allowlist costs nothing legitimate while making a path separator,
/// `..`, NUL, or any other traversal byte impossible.
fn validate_mid(mid: &str) -> Result<(), String> {
    if mid.is_empty() || mid.len() > 64 {
        return Err(format!("message id length {} out of range (1..=64)", mid.len()));
    }
    if !mid
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(format!("message id {mid:?} contains a disallowed character"));
    }
    Ok(())
}

/// A native message store rooted at a directory.
pub struct Mailbox {
    root: PathBuf,
    /// Search index, wrapped in a Mutex because `rusqlite::Connection` is not
    /// `Sync`. `Mailbox` itself must be `Sync` (it is held as `Arc<Mailbox>`
    /// inside `NativeBackend: Send + Sync`). The Mutex makes every index call
    /// exclusive, which is fine — index operations are fast and infrequent.
    index: Option<Arc<Mutex<crate::search::index::Index>>>,
    /// Serializes registry read-modify-write across create / rename / move /
    /// delete (tuxlink-ka3z A2). Tauri commands can run concurrently, so two
    /// folder mutations could otherwise interleave their load→mutate→save and
    /// strand an orphaned child (Codex finding #3). The lock is held only for
    /// the brief registry critical section.
    registry_lock: Arc<Mutex<()>>,
    /// Default received-mail namespace for the un-namespaced (`None`) path
    /// (tuxlink-2ns7). Production constructs the mailbox with the operator's
    /// sole FULL identity via [`Mailbox::with_default_identity`], so a bare
    /// `list`/`store`/`read` resolves Inbox/Archive + user folders under
    /// `mailbox/<FULL>/` — the SAME subtree [`Mailbox::migrate_legacy_layout`]
    /// re-homes legacy mail into, so the move and the read never split (the
    /// invariant tuxlink-ej7a's heal was a stopgap for). `None` (tests / a
    /// pre-identity install) falls back to the literal `_default` segment.
    /// Sent/Outbox ignore this — they are always shared at the root.
    default_ns: Option<IdentityNamespace>,
}

impl Mailbox {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            index: None,
            registry_lock: Arc::new(Mutex::new(())),
            default_ns: None,
        }
    }

    /// Set the default received-mail namespace to a FULL identity (tuxlink-2ns7).
    /// Production wires this from the operator's sole FULL (`config.identity`)
    /// so bare `list`/`store`/`read` resolve Inbox/Archive + user folders under
    /// `mailbox/<FULL>/` — matching where [`Mailbox::migrate_legacy_layout`]
    /// re-homes legacy mail. Uses the NORMALIZED `Callsign::as_str()` so the
    /// read namespace and the migration target dir-name agree (tuxlink-21w3).
    /// A `Callsign` always parses as a valid namespace segment, so the `.ok()`
    /// fallback to `None` (→ `_default`) is unreachable in practice.
    pub fn with_default_identity(mut self, full: &crate::identity::Callsign) -> Self {
        self.default_ns = IdentityNamespace::parse(full.as_str()).ok();
        self
    }

    /// Acquire the registry critical-section lock, recovering from a poisoned
    /// mutex (a panic in another holder must not wedge folder operations — the
    /// guarded data is the on-disk registry, re-read fresh under the lock).
    fn lock_registry(&self) -> std::sync::MutexGuard<'_, ()> {
        self.registry_lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Attach a search index. After each successful filesystem write, the
    /// mailbox dispatches a best-effort index update. Index errors are logged
    /// but never propagated — the filesystem write is canonical (spec §8).
    pub fn with_index(mut self, index: Arc<Mutex<crate::search::index::Index>>) -> Self {
        self.index = Some(index);
        self
    }

    /// Store a raw Winlink message in a folder, keyed by its message id (taken
    /// from the `Mid` header). Returns that id. Delegates to [`Mailbox::store_ns`]
    /// with the un-namespaced (`_default`) namespace so existing callers route
    /// through the per-FULL layout's default subtree.
    pub fn store(&self, folder: MailboxFolder, raw: &[u8]) -> Result<MessageId, BackendError> {
        self.store_ns(None, folder, raw)
    }

    /// Namespace-aware store (Phase 4, tuxlink-2ns7). Received-mail folders
    /// (Inbox/Archive) resolve under the per-FULL namespace root; Sent/Outbox
    /// stay shared. The index `unread` seed + the `identity_tag` are threaded
    /// per the namespace.
    pub fn store_ns(
        &self,
        ns: Option<&IdentityNamespace>,
        folder: MailboxFolder,
        raw: &[u8],
    ) -> Result<MessageId, BackendError> {
        let msg = Message::from_bytes(raw)
            .map_err(|_| BackendError::MessageRejected("stored bytes are not a message".into()))?;
        let mid = msg
            .header("Mid")
            .ok_or_else(|| BackendError::MessageRejected("message has no Mid".into()))?
            .to_string();
        // SECURITY (tuxlink-5lbm): reject an attacker-controlled inbound `Mid`
        // before it is used as a filename — see `validate_mid`. This is the
        // remotely-reachable sink (a received message → arbitrary file write).
        validate_mid(&mid).map_err(BackendError::MessageRejected)?;

        let dir = self.folder_dir_ns(ns, folder);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(format!("{mid}.b2f")), raw)?;

        // The identity tag for this message: for received mail it is the
        // namespace callsign (the FULL whose inbox it was delivered into); for
        // Sent/Outbox it is the FULL the message was authored under. Resolve
        // through the effective namespace (explicit arg, else the mailbox's
        // default identity) so a production bare `store` on an
        // identity-defaulted mailbox still tags Sent/Outbox correctly
        // (tuxlink-2ns7).
        let identity_tag: Option<String> =
            ns.or(self.default_ns.as_ref()).map(|n| n.as_str().to_string());

        // tuxlink-2ns7: Sent/Outbox stay in the SHARED store, so a message's
        // owning identity must be persisted on-disk as a `<mid>.identity`
        // sidecar (canonical, like `<mid>.read`). Received mail (Inbox/Archive)
        // is already namespaced by directory, so it needs no sidecar. Only
        // write the sidecar when a namespace is present (an un-namespaced
        // store leaves the message untagged — drained for any identity).
        if matches!(folder, MailboxFolder::Sent | MailboxFolder::Outbox) {
            if let Some(tag) = identity_tag.as_deref() {
                fs::write(dir.join(format!("{mid}.identity")), tag.as_bytes())?;
            }
        }

        // Best-effort index hook — filesystem write already succeeded above.
        // Index errors are logged but never propagated (spec §8).
        if let Some(idx) = self.index.as_ref() {
            let row = crate::search::extractor::extract(
                &msg,
                folder,
                direction_for_folder(folder),
                // tuxlink-mzm4: seed `unread` with the SAME predicate list() uses
                // (received-mail folders = Inbox | Archive), not Inbox-only. A
                // message stored directly into Archive surfaces as unread in
                // list() (no .read sidecar yet), so the index must agree or a
                // search filtered by unread silently drops it. Sent/Outbox stay
                // read (operator-authored). A fresh store has no .read sidecar,
                // so list()'s extra `!<mid>.read exists` clause is implicitly
                // true here and the predicates align.
                /*unread=*/ matches!(folder, MailboxFolder::Inbox | MailboxFolder::Archive),
                /*transport_used=*/ None,
                /*identity_tag=*/ identity_tag.clone(),
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
        self.list_ns(None, folder)
    }

    /// Namespace-aware list (Phase 4, tuxlink-2ns7). A copy of [`Mailbox::list`]'s
    /// body that resolves the folder dir via `folder_dir_ns`. The unread
    /// predicate is byte-for-byte identical to `list` — `matches!(folder,
    /// Inbox | Archive) && !<mid>.read exists` — so namespacing never changes
    /// what counts as unread (tuxlink-mzm4 invariant, now per-FULL).
    pub fn list_ns(
        &self,
        ns: Option<&IdentityNamespace>,
        folder: MailboxFolder,
    ) -> Result<Vec<MessageMeta>, BackendError> {
        let dir = self.folder_dir_ns(ns, folder);
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
                // Unread is a received-mail concept: the Inbox and Archive (which
                // holds received mail) surface it. Sent/Outbox are the
                // operator's own messages. A message is unread until a
                // `<mid>.read` sidecar marks it read.
                meta.unread =
                    matches!(folder, MailboxFolder::Inbox | MailboxFolder::Archive)
                        && !path.with_extension("read").exists();
                // Identity tag for the mailbox filter (Phase 7, tuxlink-noa0):
                // received mail (Inbox/Archive) belongs to the FULL namespace it
                // was listed from; the shared Sent/Outbox carry a per-message
                // `<mid>.identity` sidecar (the sent/queued-as identity).
                meta.identity = match folder {
                    MailboxFolder::Sent | MailboxFolder::Outbox => {
                        read_identity_sidecar(&path)
                    }
                    _ => ns
                        .or(self.default_ns.as_ref())
                        .map(|n| n.as_str().to_string()),
                };
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
        self.read_ns(None, folder, id)
    }

    /// Namespace-aware read (Phase 4, tuxlink-2ns7). Resolves the folder dir via
    /// `folder_dir_ns` so received-mail reads hit the per-FULL subtree.
    pub fn read_ns(
        &self,
        ns: Option<&IdentityNamespace>,
        folder: MailboxFolder,
        id: &MessageId,
    ) -> Result<MessageBody, BackendError> {
        validate_mid(&id.0).map_err(BackendError::MessageRejected)?;
        let path = self.folder_dir_ns(ns, folder).join(format!("{}.b2f", id.0));
        let raw = fs::read(&path).map_err(|_| BackendError::NotFound(id.clone()))?;
        Ok(MessageBody {
            id: id.clone(),
            raw_rfc5322: raw,
        })
    }

    /// Read a message's `<mid>.identity` tag, if present (tuxlink-2ns7).
    /// `None` = untagged (legacy / pre-Phase-4) — callers treat untagged as
    /// "matches any active identity" so a legacy draft is never stranded by the
    /// identity drain filter. Sent/Outbox resolve to the shared root; the tag
    /// lives next to the `<mid>.b2f` there.
    pub fn read_identity_tag(&self, folder: MailboxFolder, id: &MessageId) -> Option<String> {
        if validate_mid(&id.0).is_err() {
            return None;
        }
        let p = self.folder_dir(folder).join(format!("{}.identity", id.0));
        std::fs::read_to_string(p)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Move a message from one folder to another (e.g. outbox → sent once it has
    /// been delivered). No-op-safe if the source file is missing.
    pub fn move_to(
        &self,
        from: MailboxFolder,
        to: MailboxFolder,
        id: &MessageId,
    ) -> Result<(), BackendError> {
        validate_mid(&id.0).map_err(BackendError::MessageRejected)?;
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
        // marker is left behind in the source folder. When the source is Sent
        // or Outbox (Fix 4: operator-authored mail), also pre-write the .read
        // sidecar at the destination so a moved message does not surface as
        // unread in Archive or a user folder.
        let src_marker = self.folder_dir(from).join(format!("{}.read", id.0));
        let source_is_sent_or_outbox =
            matches!(from, MailboxFolder::Sent | MailboxFolder::Outbox);
        if src_marker.exists() {
            fs::write(dst_dir.join(format!("{}.read", id.0)), [])?;
            fs::remove_file(&src_marker)?;
        } else if source_is_sent_or_outbox {
            fs::write(dst_dir.join(format!("{}.read", id.0)), [])?;
        }

        // Carry the identity tag (tuxlink-2ns7): the send-time Outbox→Sent move
        // must keep the `<mid>.identity` sidecar so the sent copy still records
        // which FULL authored it. Mirrors the `<mid>.read` carry above.
        let src_id_marker = self.folder_dir(from).join(format!("{}.identity", id.0));
        if src_id_marker.exists() {
            let tag = fs::read(&src_id_marker)?;
            fs::write(dst_dir.join(format!("{}.identity", id.0)), tag)?;
            fs::remove_file(&src_id_marker)?;
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

    /// Delete a message: move it into the shared `Deleted` (Trash) folder and
    /// record where it came from in a `<mid>.trash` sidecar so Restore can
    /// return it and auto-purge can expire it (tuxlink-wl7n).
    ///
    /// `from` is the origin folder; `origin_full` is the origin identity FULL
    /// for per-identity origins (Inbox/Archive/user folders) and `None` for the
    /// shared folders (Sent/Outbox). `now_rfc3339` is injected (not read from
    /// `chrono::Utc::now()` here) so unit tests are deterministic; the command
    /// layer passes `chrono::Utc::now().to_rfc3339()`.
    pub fn delete_message(
        &self,
        from: MailboxFolder,
        id: &MessageId,
        origin_full: Option<&str>,
        now_rfc3339: &str,
    ) -> Result<(), BackendError> {
        self.move_to(from, MailboxFolder::Deleted, id)?;
        let meta = TrashMeta {
            origin: from.as_path().to_string(),
            origin_full: origin_full.map(str::to_string),
            deleted_at: now_rfc3339.to_string(),
        };
        write_trash_sidecar(&self.folder_dir(MailboxFolder::Deleted), &id.0, &meta)?;
        Ok(())
    }

    /// Restore a deleted message from the shared `Deleted` (Trash) folder back
    /// to its recorded origin (tuxlink-wl7n).
    ///
    /// Reads the `<mid>.trash` sidecar to recover the origin folder + origin
    /// identity, moves the message back there, and removes the sidecar. A
    /// missing/corrupt sidecar, or an `origin` that is neither a known system
    /// folder nor a user-folder slug that resolves, falls back to the active
    /// identity's Inbox so a recoverable message is never stranded in Trash.
    ///
    /// The move routes through [`Mailbox::move_between_ns`] with the recorded
    /// `origin_full` as the destination namespace: the source (`Deleted`) is a
    /// shared folder that resolves the same regardless of namespace, while the
    /// destination (Inbox/Archive/user folder) lands in that identity's
    /// per-FULL subtree. Shared origins (Sent/Outbox) carry no `origin_full`
    /// and resolve to their shared paths.
    pub fn restore_message(&self, id: &MessageId) -> Result<(), BackendError> {
        let deleted_dir = self.folder_dir(MailboxFolder::Deleted);
        let meta = read_trash_sidecar(&deleted_dir, &id.0);

        // Resolve the destination namespace from the recorded origin identity.
        // An un-parseable / absent FULL falls back to the default namespace
        // (matches `for_identity`'s `_default` fallback).
        let ns = meta
            .as_ref()
            .and_then(|m| m.origin_full.as_deref())
            .and_then(|full| IdentityNamespace::parse(full).ok());

        // Resolve the destination folder reference from the recorded origin
        // slug. A system folder ("in"|"sent"|"out"|"archive") maps via
        // `parse_origin_folder`; any other non-empty slug is treated as a user
        // folder. A missing sidecar, an empty/`"deleted"` origin, or any value
        // that cannot route falls back to Inbox so the message returns
        // somewhere reachable rather than stranding in Trash.
        let to = match meta.as_ref() {
            Some(m) => match parse_origin_folder(&m.origin) {
                Some(folder) => FolderRef::System(folder),
                // Non-system slug: a user folder, unless it is empty or the
                // degenerate `"deleted"` self-reference — both → Inbox.
                None if m.origin.is_empty() || m.origin == "deleted" => {
                    FolderRef::System(MailboxFolder::Inbox)
                }
                None => FolderRef::User(m.origin.clone()),
            },
            None => FolderRef::System(MailboxFolder::Inbox),
        };

        self.move_between_ns(ns.as_ref(), FolderRef::System(MailboxFolder::Deleted), to, id)?;

        // Drop the now-stale sidecar (ignore NotFound: a bare-message restore
        // with no sidecar reaches here with nothing to remove).
        match fs::remove_file(deleted_dir.join(format!("{}.trash", id.0))) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    /// Permanently delete a message from the shared `Deleted` (Trash) folder
    /// (tuxlink-wl7n). Unlinks the `<mid>.b2f` plus every carried sidecar
    /// (`.read`, `.identity`, `.trash`) — each ignore-NotFound — and drops the
    /// search-index row. Irreversible; the command layer gates it behind a
    /// confirm.
    pub fn purge_message(&self, id: &MessageId) -> Result<(), BackendError> {
        let dir = self.folder_dir(MailboxFolder::Deleted);
        for ext in ["b2f", "read", "identity", "trash"] {
            match fs::remove_file(dir.join(format!("{}.{ext}", id.0))) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        self.index_delete(&id.0);
        Ok(())
    }

    /// Permanently purge every message in the `Deleted` (Trash) folder
    /// (tuxlink-wl7n). Enumerates each `<mid>.b2f`, purges it via
    /// [`Mailbox::purge_message`], and returns the count purged. A non-existent
    /// Deleted dir is an empty trash (0 purged).
    pub fn empty_trash(&self) -> Result<usize, BackendError> {
        let dir = self.folder_dir(MailboxFolder::Deleted);
        let mut count = 0usize;
        for mid in trash_message_ids(&dir) {
            self.purge_message(&MessageId(mid))?;
            count += 1;
        }
        Ok(count)
    }

    /// Auto-purge sweep: permanently purge every message in `Deleted` whose
    /// `<mid>.trash` `deleted_at` is at least `retention_days` old relative to
    /// `now` (tuxlink-wl7n). `now` is injected for deterministic tests; the
    /// command/scheduler layer supplies `chrono::Utc::now()`. A message with no
    /// readable sidecar (or an unparseable timestamp) is kept, never auto-
    /// purged. Returns the count purged.
    pub fn purge_expired(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        retention_days: i64,
    ) -> Result<usize, BackendError> {
        let dir = self.folder_dir(MailboxFolder::Deleted);
        let mut count = 0usize;
        for mid in trash_message_ids(&dir) {
            let expired = read_trash_sidecar(&dir, &mid)
                .map(|m| trash_is_expired(&m.deleted_at, now, retention_days))
                .unwrap_or(false);
            if expired {
                self.purge_message(&MessageId(mid))?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Set a message's read-state by adding (`read = true`) or removing
    /// (`read = false`) the `<mid>.read` sidecar next to its `<mid>.b2f`.
    /// Folder-ref aware: works for system folders AND user-folder slugs via
    /// `resolve_dir`. Tolerant: a message with no file on disk is a no-op
    /// (it may have been moved/removed between the list view and the action),
    /// and removing an absent marker is not an error.
    pub fn set_read_state(
        &self,
        folder: &FolderRef,
        id: &MessageId,
        read: bool,
    ) -> Result<(), BackendError> {
        self.set_read_state_ns(None, folder, id, read)
    }

    /// Namespace-aware read-state setter (Phase 4, tuxlink-2ns7). Resolves the
    /// sidecar dir via `resolve_dir_ns` so received-mail read-state lands in the
    /// per-FULL subtree. Same tolerance + index-hook semantics as
    /// [`Mailbox::set_read_state`].
    pub fn set_read_state_ns(
        &self,
        ns: Option<&IdentityNamespace>,
        folder: &FolderRef,
        id: &MessageId,
        read: bool,
    ) -> Result<(), BackendError> {
        let dir = self.resolve_dir_ns(ns, folder);
        if !dir.join(format!("{}.b2f", id.0)).exists() {
            return Ok(());
        }
        let marker = dir.join(format!("{}.read", id.0));
        if read {
            fs::write(&marker, [])?;
        } else {
            match fs::remove_file(&marker) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        // Best-effort index hook — filesystem write already succeeded above.
        if let Some(idx) = self.index.as_ref() {
            match idx.lock() {
                Ok(guard) => {
                    if let Err(e) = guard.update_unread(&id.0, !read) {
                        eprintln!("search-index update_unread failed for mid={}: {e}", id.0);
                    }
                }
                Err(e) => eprintln!("search-index lock poisoned during update_unread: {e}"),
            }
        }
        Ok(())
    }

    /// Mark a message read. Thin wrapper over [`Mailbox::set_read_state`] kept for
    /// existing call sites. System-folder convenience signature.
    pub fn mark_read(&self, folder: MailboxFolder, id: &MessageId) -> Result<(), BackendError> {
        self.set_read_state(&FolderRef::System(folder), id, true)
    }

    /// Store a received message for `parent_full`, optionally addressed to a
    /// tactical label riding under that FULL. Phase 4 (tuxlink-2ns7): tactical
    /// mail lands in the parent FULL's inbox. `tuxlink-73nl` hooks per-tactical-
    /// folder routing HERE — it will inspect `addressed_to_tactical` and choose a
    /// user-folder destination instead of Inbox. Until then the label is recorded
    /// only via the identity tag and the message goes to Inbox.
    pub fn route_received(
        &self,
        parent_full: &str,
        addressed_to_tactical: Option<&str>,
        raw: &[u8],
    ) -> Result<MessageId, BackendError> {
        // tuxlink-73nl SEAM: branch on `addressed_to_tactical` to a per-tactical
        // user folder. Phase 4 deliberately ignores it for the destination and
        // always targets the parent FULL's Inbox.
        let _ = addressed_to_tactical;
        self.for_identity(parent_full).store(MailboxFolder::Inbox, raw)
    }

    /// One-time migration of a pre-Phase-4 *flat* mailbox to the per-FULL layout
    /// (Phase 4, tuxlink-2ns7). The legacy install kept `inbox/`, `sent/`,
    /// `outbox/`, `archive/`, `<user-slug>/`, and `.folders.json` directly at the
    /// mailbox root. This re-homes the received-mail folders + user folders under
    /// `mailbox/<DEFAULT_FULL>/`, tags the shared Sent/Outbox messages with a
    /// `<mid>.identity` sidecar naming the default FULL, and leaves the search
    /// index alone.
    ///
    /// `default_full` is the single migrated FULL identity (Phase 2's "existing
    /// `identity.callsign` becomes the one FULL identity"). Its NORMALIZED string
    /// (`Callsign::as_str`) is used for BOTH the namespace path segment AND the
    /// Sent/Outbox sidecars, so the target directory name equals what
    /// `for_identity(default_full.as_str())` resolves to — otherwise the read
    /// side would not find the moved mail (tuxlink-21w3: per-FULL dir case must
    /// match the normalized stored Callsign).
    ///
    /// Idempotent at the granularity of each sub-step: every move runs only when
    /// its source dir exists AND its destination does not, and each Sent/Outbox
    /// sidecar is written only when absent. This lets the migration run safely on
    /// EVERY launch regardless of partial prior state — there is no wholesale
    /// short-circuit, so a half-completed prior run (or a fresh old-scheme dir
    /// left by the now-removed `heal_misplaced_inbox`) is finished on the next run.
    ///
    /// The search index (`search.db`) is intentionally untouched. The v3→v4
    /// schema bump already forces the operator's existing
    /// `tauri_search_rebuild_index` path to run, which re-extracts every message
    /// from the new `mailbox/<CALLSIGN>/` paths + `.identity` sidecars — so no
    /// bespoke index-migration code is needed here.
    pub fn migrate_legacy_layout(
        &self,
        default_full: &crate::identity::Callsign,
    ) -> Result<(), BackendError> {
        // Use the NORMALIZED Callsign string for the namespace segment so the
        // migrated dir name matches what `for_identity(default_full.as_str())`
        // resolves to (tuxlink-21w3).
        let ns = IdentityNamespace::parse(default_full.as_str())?;
        let per_full = self.received_root(Some(&ns));

        fs::create_dir_all(&per_full)?;

        // 1. Re-home the received-mail folders (inbox + archive). Two legacy
        //    sources are possible per folder, in precedence order:
        //      (a) the FLAT scheme `<root>/<name>` (pre-Phase-4 default), and
        //      (b) the OLD per-FULL scheme `<root>/<CALLSIGN>/<name>` that the
        //          now-removed `bootstrap::heal_misplaced_inbox` used to bounce
        //          mail back to flat from. Removing that heal means this migration
        //          must absorb its job and forward-migrate the old-scheme dir.
        //    For each folder we move only when the NEW destination does NOT
        //    already exist (per-step idempotency). When BOTH a flat and an
        //    old-scheme source exist (shouldn't happen — the heal guaranteed a
        //    single inbox), prefer the flat one and leave the old-scheme dir
        //    untouched so no data is clobbered.
        let old_scheme_root = per_full_root(&self.root, default_full.as_str());
        for name in ["inbox", "archive"] {
            let dst = per_full.join(name);
            if dst.exists() {
                // Already migrated (or born per-FULL) for this folder — skip.
                continue;
            }
            let flat = self.root.join(name);
            let old_scheme = old_scheme_root.join(name);
            if flat.is_dir() {
                fs::rename(&flat, &dst)?;
                // If an old-scheme dir also exists we deliberately do NOT clobber
                // it: the flat one wins and the old-scheme dir is left in place.
                // (tracing not imported in this module; left as a code comment.)
            } else if old_scheme.is_dir() {
                fs::rename(&old_scheme, &dst)?;
            }
        }

        // 2. Re-home legacy top-level user folders. The legacy registry lives at
        //    the FLAT root's `.folders.json`; move each recorded slug dir under
        //    the per-FULL root, then move the registry file itself so the
        //    per-FULL `list_user_folders_ns` finds it. Each move is individually
        //    idempotent: it runs only when the source dir exists AND the
        //    destination does not, so a re-run never clobbers an already-migrated
        //    folder.
        let legacy_registry = user_folders::load_registry(&self.root);
        for folder in &legacy_registry.folders {
            let legacy_slug_dir = user_folders::folder_dir(&self.root, &folder.slug);
            let dst_slug_dir = user_folders::folder_dir(&per_full, &folder.slug);
            if legacy_slug_dir.is_dir() && !dst_slug_dir.exists() {
                fs::rename(&legacy_slug_dir, &dst_slug_dir)?;
            }
        }
        let legacy_registry_path = self.root.join(".folders.json");
        let dst_registry_path = per_full.join(".folders.json");
        if legacy_registry_path.is_file() && !dst_registry_path.exists() {
            fs::rename(&legacy_registry_path, &dst_registry_path)?;
        }

        // 3. Tag the shared Sent/Outbox messages. These STAY at `<root>/sent` and
        //    `<root>/outbox`; each `<mid>.b2f` that lacks a `<mid>.identity`
        //    sidecar gets one naming the default FULL (the normalized string, to
        //    match the read side's tag comparisons — tuxlink-21w3).
        for name in ["sent", "outbox"] {
            let dir = self.root.join(name);
            if !dir.is_dir() {
                continue;
            }
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("b2f") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let sidecar = dir.join(format!("{stem}.identity"));
                if !sidecar.exists() {
                    fs::write(&sidecar, default_full.as_str().as_bytes())?;
                }
            }
        }

        Ok(())
    }

    /// Root for received-mail folders + user folders for a given namespace
    /// (Phase 4, tuxlink-2ns7). `None` (the un-namespaced default) resolves to
    /// `<root>/mailbox/_default` so even the default path is uniform with the
    /// per-FULL layout. Sent/Outbox never use this root — they stay shared at
    /// `<root>/sent` and `<root>/outbox` (see `folder_dir_ns`).
    fn received_root(&self, ns: Option<&IdentityNamespace>) -> PathBuf {
        // The explicit `ns` arg wins (per-FULL callers like `for_identity` and
        // `migrate_legacy_layout`); otherwise fall back to the mailbox's
        // construction-time default identity (production's sole FULL), then to
        // the literal `_default` (tests / a pre-identity install) (tuxlink-2ns7).
        let eff = ns.or(self.default_ns.as_ref());
        let seg = eff.map(|n| n.as_str()).unwrap_or("_default");
        self.root.join("mailbox").join(seg)
    }

    /// Namespace-aware folder directory resolver. The single load-bearing rule:
    /// **Sent/Outbox use the shared root; Inbox/Archive use the per-FULL root.**
    fn folder_dir_ns(&self, ns: Option<&IdentityNamespace>, folder: MailboxFolder) -> PathBuf {
        match folder {
            // Shared, never namespaced.
            MailboxFolder::Sent => self.root.join("sent"),
            MailboxFolder::Outbox => self.root.join("outbox"),
            // Trash is a single shared folder (like Sent/Outbox), NOT per-FULL:
            // a delete from any identity's Inbox/Archive lands in one common
            // bin, and Empty Trash / auto-purge operate on the whole store
            // (tuxlink-wl7n).
            MailboxFolder::Deleted => self.root.join("deleted"),
            // Per-FULL received mail.
            MailboxFolder::Inbox => self.received_root(ns).join("inbox"),
            MailboxFolder::Archive => self.received_root(ns).join("archive"),
        }
    }

    /// The default (un-namespaced) folder directory. Delegates to the `None`
    /// namespace form so every existing caller routes through the `_default`
    /// namespace (received mail under `mailbox/_default/`, Sent/Outbox shared).
    fn folder_dir(&self, folder: MailboxFolder) -> PathBuf {
        self.folder_dir_ns(None, folder)
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
        self.list_user_folders_ns(None)
    }

    /// Namespace-aware user-folder listing (Phase 4, tuxlink-2ns7). The registry
    /// is per-FULL: it lives at `received_root(ns)/.folders.json`, so a folder
    /// under `W1ABC` is independent of one under `W7XYZ`.
    pub fn list_user_folders_ns(&self, ns: Option<&IdentityNamespace>) -> Vec<UserFolder> {
        let mut reg = user_folders::load_registry(&self.received_root(ns));
        reg.folders.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        reg.folders
    }

    /// Create a new user folder. Validates the display name, derives a slug,
    /// rejects collisions with system folder names + existing user folder
    /// slugs, then creates the on-disk directory + persists the registry.
    /// Returns the newly created `UserFolder` so the caller can echo back to
    /// the UI (no extra round-trip).
    pub fn create_user_folder(
        &self,
        display_name: &str,
        parent_slug: Option<&str>,
    ) -> Result<UserFolder, BackendError> {
        self.create_user_folder_ns(None, display_name, parent_slug)
    }

    /// Namespace-aware user-folder creation (Phase 4, tuxlink-2ns7). The
    /// registry + on-disk dir live under the per-FULL `received_root(ns)`; the
    /// validation, slug derivation, collision, and depth-cap logic are unchanged
    /// — only the registry/dir root is per-FULL.
    pub fn create_user_folder_ns(
        &self,
        ns: Option<&IdentityNamespace>,
        display_name: &str,
        parent_slug: Option<&str>,
    ) -> Result<UserFolder, BackendError> {
        let root = self.received_root(ns);
        let display = display_name.trim();
        user_folders::validate_display_name(display)
            .map_err(BackendError::MessageRejected)?;
        let slug = user_folders::slug_from_display(display);
        user_folders::validate_slug(&slug).map_err(BackendError::MessageRejected)?;

        let _guard = self.lock_registry();
        let mut reg = user_folders::load_registry(&root);
        for existing in &reg.folders {
            if existing.slug == slug {
                return Err(BackendError::MessageRejected(format!(
                    "a folder with that name already exists ('{slug}')"
                )));
            }
        }
        // Validate the parent (spec D4): must be an existing top-level folder so
        // the new child lands at depth 2, never deeper.
        if let Some(parent) = parent_slug {
            user_folders::validate_create_parent(&reg, parent)
                .map_err(BackendError::MessageRejected)?;
        }

        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let folder = UserFolder {
            slug: slug.clone(),
            display_name: display.to_string(),
            created_at: now,
            parent_slug: parent_slug.map(|s| s.to_string()),
        };

        // Create the directory FIRST — if the FS write fails we don't poison
        // the registry with a folder whose dir doesn't exist.
        let dir = user_folders::folder_dir(&root, &slug);
        fs::create_dir_all(&dir)?;

        reg.folders.push(folder.clone());
        user_folders::save_registry(&root, &reg)?;
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
        // Phase 4 (tuxlink-2ns7): the user-folder registry is per-FULL, living
        // under `received_root`. The default (un-namespaced) path resolves to the
        // `_default` namespace, matching `create_user_folder` / `list_user_folders`.
        let root = self.received_root(None);
        let _guard = self.lock_registry();
        let mut reg = user_folders::load_registry(&root);
        let folder = reg
            .folders
            .iter_mut()
            .find(|f| f.slug == slug)
            .ok_or_else(|| crate::winlink_backend::BackendError::NotFound(
                crate::winlink_backend::MessageId(slug.into()),
            ))?;
        folder.display_name = display.to_string();
        let renamed = folder.clone();
        user_folders::save_registry(&root, &reg)?;
        Ok(renamed)
    }

    /// Delete a user folder, cascading to its direct subfolders (spec D6, A1).
    /// `on_messages` controls the disposition of EVERY message in the parent and
    /// its children:
    /// - `MoveToInbox`/`MoveToArchive` — re-home each message to that system
    ///   folder. PREFLIGHTED: if any message would overwrite an existing file at
    ///   the destination, or two affected messages share a filename, the whole
    ///   operation is REFUSED (no partial work, no silent overwrite — data loss
    ///   is the only irreversible consequence). Search-index rows are re-pointed.
    /// - `Delete` — remove each message permanently and drop its index row.
    ///
    /// Returns the slugs actually removed from the registry (parent + children)
    /// so the UI can clear a stale selection (A5). Held under the registry lock
    /// (A2). Missing folder → `Ok(empty)`.
    ///
    /// Failure contract: the preflight turns any destination/merge collision into
    /// a clean up-front rejection. A filesystem error during the commit phase
    /// returns the error after logging; full transactional rollback is out of
    /// scope for the single-process desktop store — the preflight is the guard.
    pub fn delete_user_folder(
        &self,
        slug: &str,
        on_messages: DeleteAction,
    ) -> Result<Vec<String>, BackendError> {
        // Phase 4 (tuxlink-2ns7): per-FULL registry + user-folder dirs under
        // `received_root`. The default path uses the `_default` namespace, which
        // matches the system-folder dirs (`folder_dir` is also `_default`-namespaced).
        let root = self.received_root(None);
        let _guard = self.lock_registry();
        let mut reg = user_folders::load_registry(&root);

        // Affected folders: target + its direct children (depth-capped → leaves).
        let mut affected = user_folders::children_slugs(&reg, slug);
        affected.push(slug.to_string());

        let move_dst = match on_messages {
            DeleteAction::MoveToInbox => Some(MailboxFolder::Inbox),
            DeleteAction::MoveToArchive => Some(MailboxFolder::Archive),
            DeleteAction::Delete => None,
        };

        // PREFLIGHT (move modes): refuse rather than clobber (finding #1).
        if let Some(sys) = move_dst {
            let dst_dir = self.folder_dir(sys);
            let mut seen = std::collections::HashSet::new();
            for s in &affected {
                let dir = user_folders::folder_dir(&root, s);
                if !dir.exists() {
                    continue;
                }
                for entry in fs::read_dir(&dir)? {
                    let name = match entry?.path().file_name() {
                        Some(n) => n.to_owned(),
                        None => continue,
                    };
                    if dst_dir.join(&name).exists() {
                        return Err(BackendError::MessageRejected(format!(
                            "cannot delete: a message named '{}' already exists in the destination folder",
                            name.to_string_lossy()
                        )));
                    }
                    if !seen.insert(name.clone()) {
                        return Err(BackendError::MessageRejected(format!(
                            "cannot delete: two subfolders both contain a message named '{}'",
                            name.to_string_lossy()
                        )));
                    }
                }
            }
        }

        // Registry-present affected slugs (return value + retain target).
        let removed: Vec<String> = affected
            .iter()
            .filter(|s| reg.folders.iter().any(|f| &f.slug == *s))
            .cloned()
            .collect();

        // COMMIT.
        for s in &affected {
            let dir = user_folders::folder_dir(&root, s);
            if !dir.exists() {
                continue;
            }
            match move_dst {
                None => {
                    for entry in fs::read_dir(&dir)? {
                        let path = entry?.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("b2f") {
                            if let Some(mid) = path.file_stem().and_then(|st| st.to_str()) {
                                self.index_delete(mid);
                            }
                        }
                    }
                    fs::remove_dir_all(&dir)?;
                }
                Some(sys) => {
                    let dst_dir = self.folder_dir(sys);
                    fs::create_dir_all(&dst_dir)?;
                    for entry in fs::read_dir(&dir)? {
                        let path = entry?.path();
                        if let Some(name) = path.file_name() {
                            fs::rename(&path, dst_dir.join(name))?;
                            if path.extension().and_then(|e| e.to_str()) == Some("b2f") {
                                if let Some(mid) = path.file_stem().and_then(|st| st.to_str()) {
                                    self.index_set_folder(mid, folder_str(sys));
                                }
                            }
                        }
                    }
                    fs::remove_dir_all(&dir)?;
                }
            }
        }

        // Drop all affected slugs from the registry in a single save.
        let affected_set: std::collections::HashSet<&str> =
            affected.iter().map(|s| s.as_str()).collect();
        reg.folders.retain(|f| !affected_set.contains(f.slug.as_str()));
        user_folders::save_registry(&root, &reg)?;

        Ok(removed)
    }

    /// Re-parent a user folder by editing its `parent_slug` in the registry
    /// (spec D3). `new_parent == None` promotes it to top level. METADATA ONLY —
    /// folder directories stay flat at `root/<slug>`, so no message file moves
    /// regardless of how many messages the folder holds. Validates against the
    /// D4 rule set; held under the registry lock (A2).
    pub fn move_user_folder(
        &self,
        slug: &str,
        new_parent: Option<&str>,
    ) -> Result<UserFolder, BackendError> {
        // Phase 4 (tuxlink-2ns7): per-FULL registry under `received_root`; the
        // default path uses the `_default` namespace.
        let root = self.received_root(None);
        let _guard = self.lock_registry();
        let mut reg = user_folders::load_registry(&root);
        user_folders::validate_reparent(&reg, slug, new_parent)
            .map_err(BackendError::MessageRejected)?;
        // validate_reparent guarantees the folder exists.
        let folder = reg.folders.iter_mut().find(|f| f.slug == slug).unwrap();
        folder.parent_slug = new_parent.map(|s| s.to_string());
        let updated = folder.clone();
        user_folders::save_registry(&root, &reg)?;
        Ok(updated)
    }

    /// Best-effort search-index row delete (mirrors `move_between`'s logging).
    fn index_delete(&self, mid: &str) {
        if let Some(idx) = self.index.as_ref() {
            match idx.lock() {
                Ok(guard) => {
                    if let Err(e) = guard.delete(mid) {
                        eprintln!("search-index delete failed for mid={mid}: {e}");
                    }
                }
                Err(e) => eprintln!("search-index lock poisoned during delete: {e}"),
            }
        }
    }

    /// Best-effort search-index folder re-point (mirrors `move_between`).
    fn index_set_folder(&self, mid: &str, folder: &str) {
        if let Some(idx) = self.index.as_ref() {
            match idx.lock() {
                Ok(guard) => {
                    if let Err(e) = guard.update_folder(mid, folder) {
                        eprintln!("search-index update_folder failed for mid={mid}: {e}");
                    }
                }
                Err(e) => eprintln!("search-index lock poisoned during update_folder: {e}"),
            }
        }
    }

    /// List messages in a user folder. Mirrors [`Mailbox::list`]'s sort order
    /// (newest first, id ascending as tiebreaker). User folders hold received
    /// mail; unread state is surfaced from the `<mid>.read` sidecar.
    pub fn list_user(&self, slug: &str) -> Result<Vec<MessageMeta>, BackendError> {
        self.list_user_ns(None, slug)
    }

    /// Namespace-aware user-folder listing (Phase 4, tuxlink-2ns7). Resolves the
    /// user-folder dir under the per-FULL `received_root(ns)`. Same sort order +
    /// unread-surfacing as [`Mailbox::list_user`].
    pub fn list_user_ns(
        &self,
        ns: Option<&IdentityNamespace>,
        slug: &str,
    ) -> Result<Vec<MessageMeta>, BackendError> {
        let dir = user_folders::folder_dir(&self.received_root(ns), slug);
        let mut metas = Self::list_dir(&dir, /*surface_unread=*/ true)?;
        // User folders hold received mail under a FULL namespace; stamp each row
        // with that FULL so the Phase-7 mailbox identity filter matches them
        // (the same namespace `list_user_ns` resolved the dir from).
        if let Some(id) = ns.or(self.default_ns.as_ref()).map(|n| n.as_str().to_string()) {
            for m in &mut metas {
                m.identity = Some(id.clone());
            }
        }
        Ok(metas)
    }

    /// Read a raw message from a user folder. Returns `NotFound` if the slug
    /// or mid is unknown.
    pub fn read_user(&self, slug: &str, id: &MessageId) -> Result<MessageBody, BackendError> {
        validate_mid(&id.0).map_err(BackendError::MessageRejected)?;
        // Phase 4 (tuxlink-2ns7): user folders live under `received_root`; the
        // default path resolves to the `_default` namespace.
        let path =
            user_folders::folder_dir(&self.received_root(None), slug).join(format!("{}.b2f", id.0));
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
        self.move_between_ns(None, from, to, id)
    }

    /// Namespace-aware move (Phase 4, tuxlink-2ns7). Both source and destination
    /// resolve through `resolve_dir_ns`, so a move within a per-FULL namespace
    /// (e.g. Inbox → a per-FULL user folder) stays inside that FULL's subtree.
    /// Same self-move guard, read-marker carry, and index-hook semantics as
    /// [`Mailbox::move_between`].
    pub fn move_between_ns(
        &self,
        ns: Option<&IdentityNamespace>,
        from: FolderRef,
        to: FolderRef,
        id: &MessageId,
    ) -> Result<(), BackendError> {
        // Self-move guard (tuxlink-l80q / Codex P2, data loss): for from == to,
        // src and dst are the same path — the write-then-remove sequence below
        // would delete the message. A self-move is semantically a no-op.
        if from == to {
            return Ok(());
        }
        validate_mid(&id.0).map_err(BackendError::MessageRejected)?;
        let src_dir = self.resolve_dir_ns(ns, &from);
        let dst_dir = self.resolve_dir_ns(ns, &to);
        let src = src_dir.join(format!("{}.b2f", id.0));
        let raw = match fs::read(&src) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        fs::create_dir_all(&dst_dir)?;
        fs::write(dst_dir.join(format!("{}.b2f", id.0)), raw)?;
        fs::remove_file(&src)?;
        // Carry the read-marker if present; and pre-write one at the destination
        // when the source is Sent or Outbox (Fix 4: messages composed by the
        // operator are already "read" — they must not surface as unread after
        // being moved into Archive or a user folder, where the absence of a
        // .read sidecar would otherwise mark them unread).
        let src_marker = src_dir.join(format!("{}.read", id.0));
        let source_is_sent_or_outbox = matches!(
            &from,
            FolderRef::System(MailboxFolder::Sent) | FolderRef::System(MailboxFolder::Outbox)
        );
        if src_marker.exists() {
            fs::write(dst_dir.join(format!("{}.read", id.0)), [])?;
            fs::remove_file(&src_marker)?;
        } else if source_is_sent_or_outbox {
            // No existing marker, but this is an operator-authored message:
            // pre-write the .read sidecar at the destination so it surfaces as read.
            fs::write(dst_dir.join(format!("{}.read", id.0)), [])?;
        }

        // Carry the identity tag (tuxlink-2ns7): keep the `<mid>.identity`
        // sidecar alongside the message so a move (e.g. Outbox→Sent at send
        // time, or Inbox→user-folder) preserves which FULL owns it. Mirrors the
        // `<mid>.read` carry above.
        let src_id_marker = src_dir.join(format!("{}.identity", id.0));
        if src_id_marker.exists() {
            let tag = fs::read(&src_id_marker)?;
            fs::write(dst_dir.join(format!("{}.identity", id.0)), tag)?;
            fs::remove_file(&src_id_marker)?;
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

    /// Namespace-aware folder-ref resolver (Phase 4, tuxlink-2ns7). System
    /// folders route through `folder_dir_ns` (Inbox/Archive per-FULL, Sent/
    /// Outbox shared); user folders resolve under the per-FULL `received_root`.
    fn resolve_dir_ns(&self, ns: Option<&IdentityNamespace>, r: &FolderRef) -> PathBuf {
        match r {
            FolderRef::System(f) => self.folder_dir_ns(ns, *f),
            FolderRef::User(slug) => user_folders::folder_dir(&self.received_root(ns), slug),
        }
    }

    /// Shared list-dir helper used by both system and user folder listing.
    /// Returns metadatas sorted newest-first with id ascending as tiebreaker.
    /// `surface_unread` controls whether a missing `.read` sidecar marks the
    /// message unread. Its sole caller (`list_user`) passes `true` — received
    /// mail in user folders surfaces unread (tuxlink-etxt); `list` surfaces
    /// Inbox + Archive directly (spec §2.1).
    ///
    /// Called only for user folders; system folders compute unread in `list` directly.
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
                // `meta.identity` is stamped by the caller: user folders hold
                // received mail under a FULL namespace (no per-message sidecar),
                // so `list_user_ns` sets it from the resolved namespace.
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

/// Selects which per-FULL subtree received-mail folders resolve into (Phase 4,
/// tuxlink-2ns7). Sent + Outbox ignore this (always shared root paths). A
/// validated single, safe path segment.
#[derive(Debug, Clone)]
pub struct IdentityNamespace(String);

impl IdentityNamespace {
    /// Build from a FULL callsign string. Rejects anything that is not a single
    /// safe path segment (defense in depth over `Callsign::parse`): no path
    /// separators, no `.`/`..`, nonempty.
    pub fn parse(callsign: &str) -> Result<Self, BackendError> {
        let c = callsign.trim();
        if c.is_empty()
            || c == "."
            || c == ".."
            || c.contains('/')
            || c.contains('\\')
            || c.contains('\0')
        {
            return Err(BackendError::MessageRejected(format!(
                "invalid identity namespace segment: {callsign:?}"
            )));
        }
        Ok(Self(c.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Mailbox {
    /// A view of this mailbox scoped to a FULL callsign's received-mail subtree
    /// (Phase 4, tuxlink-2ns7). Received-mail folders (Inbox/Archive) + user
    /// folders resolve under `mailbox/<CALLSIGN>/`; Sent/Outbox stay shared. An
    /// un-parseable callsign falls back to the literal `_default` namespace so
    /// the view never fails to construct (the path-segment guard already rejects
    /// the genuinely dangerous inputs).
    pub fn for_identity(&self, full_callsign: &str) -> ScopedMailbox<'_> {
        let ns = IdentityNamespace::parse(full_callsign)
            .unwrap_or_else(|_| IdentityNamespace("_default".to_string()));
        ScopedMailbox { inner: self, ns }
    }
}

/// A namespace-scoped view of a [`Mailbox`] (Phase 4, tuxlink-2ns7). Carries a
/// borrow of the underlying mailbox + the active received-mail namespace, and
/// delegates each per-FULL operation to the mailbox's `*_ns` helpers.
pub struct ScopedMailbox<'a> {
    inner: &'a Mailbox,
    ns: IdentityNamespace,
}

impl<'a> ScopedMailbox<'a> {
    pub fn store(&self, folder: MailboxFolder, raw: &[u8]) -> Result<MessageId, BackendError> {
        self.inner.store_ns(Some(&self.ns), folder, raw)
    }

    pub fn list(&self, folder: MailboxFolder) -> Result<Vec<MessageMeta>, BackendError> {
        self.inner.list_ns(Some(&self.ns), folder)
    }

    pub fn read(&self, folder: MailboxFolder, id: &MessageId) -> Result<MessageBody, BackendError> {
        self.inner.read_ns(Some(&self.ns), folder, id)
    }

    pub fn set_read_state(
        &self,
        folder: &FolderRef,
        id: &MessageId,
        read: bool,
    ) -> Result<(), BackendError> {
        self.inner.set_read_state_ns(Some(&self.ns), folder, id, read)
    }

    pub fn list_user(&self, slug: &str) -> Result<Vec<MessageMeta>, BackendError> {
        self.inner.list_user_ns(Some(&self.ns), slug)
    }

    pub fn list_user_folders(&self) -> Vec<UserFolder> {
        self.inner.list_user_folders_ns(Some(&self.ns))
    }

    pub fn create_user_folder(
        &self,
        display_name: &str,
        parent_slug: Option<&str>,
    ) -> Result<UserFolder, BackendError> {
        self.inner.create_user_folder_ns(Some(&self.ns), display_name, parent_slug)
    }

    pub fn move_between(
        &self,
        from: FolderRef,
        to: FolderRef,
        id: &MessageId,
    ) -> Result<(), BackendError> {
        self.inner.move_between_ns(Some(&self.ns), from, to, id)
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
        MailboxFolder::Deleted => "deleted",
    }
}

/// The per-FULL mailbox root for `callsign` (Phase 4 namespacing target):
/// `<root>/<CALLSIGN>/` — the inbox for that FULL lives at `<root>/<CALLSIGN>/inbox`.
pub fn per_full_root(root: &std::path::Path, callsign: &str) -> std::path::PathBuf {
    root.join(callsign)
}

/// Write the default identity-tag sidecar for a message in a SHARED folder
/// (Sent/Outbox stay shared per spec; the tag records which FULL owns it).
/// Non-shared folders (Inbox/Archive) are a no-op.
pub fn tag_identity(root: &std::path::Path, folder: MailboxFolder, id: &MessageId, callsign: &str)
    -> std::io::Result<()> {
    let dir = match folder {
        MailboxFolder::Sent => "sent",
        MailboxFolder::Outbox => "outbox",
        _ => return Ok(()),
    };
    let folder_path = root.join(dir);
    std::fs::create_dir_all(&folder_path)?;
    std::fs::write(folder_path.join(format!("{}.identity", id.0)), callsign.as_bytes())
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
        // Placeholder; populated in the listing loops from the `<mid>.identity`
        // sidecar, which they can resolve from the `.b2f` path (Phase 7).
        identity: None,
    }
}

/// Read a message's Phase-4 `<mid>.identity` sidecar given its `.b2f` path.
/// Returns the trimmed tag, or `None` when absent/empty (untagged → the
/// mailbox identity filter treats it as "All identities" only). Used by the
/// listing loops (Phase 7, tuxlink-noa0) to surface the identity onto the
/// list-row DTO without a per-message folder/namespace lookup.
fn read_identity_sidecar(b2f_path: &Path) -> Option<String> {
    fs::read_to_string(b2f_path.with_extension("identity"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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

// ============================================================================
// Trash (Deleted folder) sidecar — origin + identity + deletion timestamp.
//
// A delete moves the `<mid>.b2f` into the shared `Deleted` folder and writes a
// `<mid>.trash` sidecar recording where it came from (so Restore can return it)
// and when it was deleted (so auto-purge can expire it). tuxlink-wl7n.
// ============================================================================

/// Sidecar recording a deleted message's origin folder, origin identity, and
/// deletion time, written as `<mid>.trash` alongside the message in the shared
/// `Deleted` folder.
///
/// - `origin` — the origin folder's [`MailboxFolder::as_path`] value
///   (`"in"|"sent"|"out"|"archive"`) or a user-folder slug.
/// - `origin_full` — the origin identity FULL when the origin was per-identity
///   (Inbox/Archive/user folder); `None` for shared origins (Sent/Outbox).
/// - `deleted_at` — RFC 3339 UTC timestamp of the deletion.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TrashMeta {
    pub origin: String,
    #[serde(default)]
    pub origin_full: Option<String>,
    pub deleted_at: String,
}

/// Write a `<mid>.trash` sidecar (pretty JSON) into `dir` (the `Deleted` folder).
fn write_trash_sidecar(dir: &Path, mid: &str, m: &TrashMeta) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(m)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(dir.join(format!("{mid}.trash")), json)
}

/// Read a `<mid>.trash` sidecar from `dir`. `None` on a missing file or
/// unparseable JSON (a corrupt sidecar must never strand a recovery flow).
/// Consumed by the tests here and by `restore_message`/`purge_expired`
/// (tuxlink-wl7n).
fn read_trash_sidecar(dir: &Path, mid: &str) -> Option<TrashMeta> {
    let raw = fs::read(dir.join(format!("{mid}.trash"))).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Map a `TrashMeta::origin` slug onto a system [`MailboxFolder`]. The slug is
/// the origin folder's [`MailboxFolder::as_path`] value. Returns `None` for a
/// user-folder slug (which routes through the user-folder move path) or any
/// unknown value. tuxlink-wl7n.
fn parse_origin_folder(origin: &str) -> Option<MailboxFolder> {
    match origin {
        "in" => Some(MailboxFolder::Inbox),
        "sent" => Some(MailboxFolder::Sent),
        "out" => Some(MailboxFolder::Outbox),
        "archive" => Some(MailboxFolder::Archive),
        _ => None, // user-folder slug or unknown
    }
}

/// Decide whether a deleted message is eligible for auto-purge: `true` iff
/// `now - deleted_at >= retention_days` (inclusive boundary). An unparseable
/// `deleted_at_rfc3339` returns `false` so a corrupt timestamp is never
/// auto-purged. tuxlink-wl7n.
pub fn trash_is_expired(
    deleted_at_rfc3339: &str,
    now: chrono::DateTime<chrono::Utc>,
    retention_days: i64,
) -> bool {
    match chrono::DateTime::parse_from_rfc3339(deleted_at_rfc3339) {
        Ok(parsed) => {
            let parsed_utc = parsed.with_timezone(&chrono::Utc);
            now - parsed_utc >= chrono::Duration::days(retention_days)
        }
        Err(_) => false,
    }
}

/// Enumerate the message ids (the `<mid>` stems of every `<mid>.b2f`) in a
/// `Deleted` (Trash) directory. A missing directory yields an empty list (an
/// empty trash). tuxlink-wl7n.
fn trash_message_ids(dir: &Path) -> Vec<String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut ids = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) == Some("b2f") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::compose::compose_message;
    use tempfile::tempdir;

    /// A `Mailbox` over a fresh temp directory for the delete/trash tests. The
    /// directory is intentionally leaked (`TempDir::keep`) so the path outlives
    /// this helper — `Mailbox` does not own the `TempDir` guard, so returning a
    /// bare `Mailbox` over a `tempdir()` would delete the dir on drop. Mirrors
    /// the `Mailbox::new(dir.path())` construction the other tests use, minus
    /// the in-scope guard.
    fn test_mailbox() -> Mailbox {
        let path = tempdir().unwrap().keep();
        Mailbox::new(path)
    }

    /// Place a bare `<mid>.b2f` directly in `folder`'s directory under the
    /// chosen `id`. Writes raw bytes (the trash/delete tests assert on file
    /// presence + the `.trash` sidecar, never parse the body), so this can use
    /// a caller-controlled MID instead of one derived from message content the
    /// way `store` does.
    fn store_test_message(mb: &Mailbox, folder: MailboxFolder, id: &MessageId) {
        let dir = mb.folder_dir(folder);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{}.b2f", id.0)), b"raw test message").unwrap();
    }

    #[test]
    fn trash_meta_round_trips_through_sidecar() {
        let dir = tempdir().unwrap();
        let m = TrashMeta {
            origin: "inbox".into(),
            origin_full: Some("N0CALL".into()),
            deleted_at: "2026-06-20T18:30:00Z".into(),
        };
        write_trash_sidecar(dir.path(), "ABCD1234", &m).unwrap();
        assert_eq!(read_trash_sidecar(dir.path(), "ABCD1234"), Some(m));
        // Shared-folder origin: no identity.
        let m2 = TrashMeta {
            origin: "sent".into(),
            origin_full: None,
            deleted_at: "2026-06-20T18:31:00Z".into(),
        };
        write_trash_sidecar(dir.path(), "EF", &m2).unwrap();
        assert_eq!(read_trash_sidecar(dir.path(), "EF"), Some(m2));
        // Missing / garbage → None.
        assert_eq!(read_trash_sidecar(dir.path(), "NOPE"), None);
        fs::write(dir.path().join("BAD.trash"), b"{not json").unwrap();
        assert_eq!(read_trash_sidecar(dir.path(), "BAD"), None);
    }

    #[test]
    fn delete_message_moves_to_trash_and_writes_sidecar() {
        let mb = test_mailbox();
        let id = MessageId::new("MID01");
        store_test_message(&mb, MailboxFolder::Inbox, &id);
        mb.delete_message(MailboxFolder::Inbox, &id, Some("N0CALL"), "2026-06-20T00:00:00Z")
            .unwrap();
        // Gone from Inbox, present in Deleted.
        assert!(!mb
            .folder_dir(MailboxFolder::Inbox)
            .join("MID01.b2f")
            .exists());
        assert!(mb
            .folder_dir(MailboxFolder::Deleted)
            .join("MID01.b2f")
            .exists());
        // Sidecar records origin.
        let meta = read_trash_sidecar(&mb.folder_dir(MailboxFolder::Deleted), "MID01").unwrap();
        assert_eq!(meta.origin, "in");
        assert_eq!(meta.origin_full.as_deref(), Some("N0CALL"));
        assert_eq!(meta.deleted_at, "2026-06-20T00:00:00Z");
    }

    #[test]
    fn restore_returns_message_to_recorded_origin() {
        let mb = test_mailbox();
        let id = MessageId::new("MID02");
        store_test_message(&mb, MailboxFolder::Archive, &id);
        mb.delete_message(MailboxFolder::Archive, &id, None, "2026-06-20T00:00:00Z")
            .unwrap();
        mb.restore_message(&id).unwrap();
        assert!(mb
            .folder_dir(MailboxFolder::Archive)
            .join("MID02.b2f")
            .exists());
        assert!(!mb
            .folder_dir(MailboxFolder::Deleted)
            .join("MID02.b2f")
            .exists());
        assert!(!mb
            .folder_dir(MailboxFolder::Deleted)
            .join("MID02.trash")
            .exists());
    }

    #[test]
    fn restore_without_sidecar_falls_back_to_inbox() {
        let mb = test_mailbox();
        let id = MessageId::new("MID03");
        // Put a bare message in Deleted with no .trash sidecar.
        store_test_message(&mb, MailboxFolder::Deleted, &id);
        mb.restore_message(&id).unwrap();
        assert!(mb
            .folder_dir(MailboxFolder::Inbox)
            .join("MID03.b2f")
            .exists());
        assert!(!mb
            .folder_dir(MailboxFolder::Deleted)
            .join("MID03.b2f")
            .exists());
    }

    #[test]
    fn restore_to_identity_namespaced_origin() {
        let mb = test_mailbox();
        let id = MessageId::new("MIDNS");
        // Delete from a specific identity's Inbox (origin_full recorded).
        store_test_message(&mb, MailboxFolder::Deleted, &id);
        let meta = TrashMeta {
            origin: "in".into(),
            origin_full: Some("W1AW".into()),
            deleted_at: "2026-06-20T00:00:00Z".into(),
        };
        write_trash_sidecar(&mb.folder_dir(MailboxFolder::Deleted), "MIDNS", &meta).unwrap();
        mb.restore_message(&id).unwrap();
        // Lands in W1AW's namespaced inbox, not the _default inbox.
        let ns = IdentityNamespace::parse("W1AW").unwrap();
        assert!(mb
            .folder_dir_ns(Some(&ns), MailboxFolder::Inbox)
            .join("MIDNS.b2f")
            .exists());
        // NOT in the _default inbox.
        assert!(!mb
            .folder_dir(MailboxFolder::Inbox)
            .join("MIDNS.b2f")
            .exists());
        assert!(!mb
            .folder_dir(MailboxFolder::Deleted)
            .join("MIDNS.b2f")
            .exists());
    }

    #[test]
    fn purge_removes_message_and_all_sidecars() {
        let mb = test_mailbox();
        let id = MessageId::new("MID04");
        store_test_message(&mb, MailboxFolder::Inbox, &id);
        mb.delete_message(MailboxFolder::Inbox, &id, None, "2026-06-20T00:00:00Z")
            .unwrap();
        mb.purge_message(&id).unwrap();
        let d = mb.folder_dir(MailboxFolder::Deleted);
        assert!(!d.join("MID04.b2f").exists());
        assert!(!d.join("MID04.trash").exists());
    }

    #[test]
    fn empty_trash_purges_all_and_counts() {
        let mb = test_mailbox();
        for n in ["A1", "B2", "C3"] {
            let id = MessageId::new(n);
            store_test_message(&mb, MailboxFolder::Inbox, &id);
            mb.delete_message(MailboxFolder::Inbox, &id, None, "2026-06-20T00:00:00Z")
                .unwrap();
        }
        assert_eq!(mb.empty_trash().unwrap(), 3);
        let remaining = std::fs::read_dir(mb.folder_dir(MailboxFolder::Deleted))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |x| x == "b2f"))
            .count();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn trash_is_expired_respects_retention_window() {
        use chrono::{TimeZone, Utc};
        let now = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        // Exactly 30 days → expired (inclusive boundary).
        assert!(trash_is_expired("2026-06-20T00:00:00Z", now, 30));
        assert!(trash_is_expired("2026-05-01T00:00:00Z", now, 30));
        // 10 days → kept.
        assert!(!trash_is_expired("2026-07-10T00:00:00Z", now, 30));
        // Unparseable → keep (never auto-purge a corrupt timestamp).
        assert!(!trash_is_expired("garbage", now, 30));
    }

    #[test]
    fn purge_expired_only_purges_aged_items() {
        use chrono::{TimeZone, Utc};
        let mb = test_mailbox();
        // Old item (40 days before `now`).
        let old = MessageId::new("OLD1");
        store_test_message(&mb, MailboxFolder::Inbox, &old);
        mb.delete_message(MailboxFolder::Inbox, &old, None, "2026-06-10T00:00:00Z")
            .unwrap();
        // Fresh item (5 days before `now`).
        let fresh = MessageId::new("FRESH1");
        store_test_message(&mb, MailboxFolder::Inbox, &fresh);
        mb.delete_message(MailboxFolder::Inbox, &fresh, None, "2026-07-15T00:00:00Z")
            .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        assert_eq!(mb.purge_expired(now, 30).unwrap(), 1);
        let d = mb.folder_dir(MailboxFolder::Deleted);
        assert!(!d.join("OLD1.b2f").exists());
        assert!(d.join("FRESH1.b2f").exists());
    }

    fn raw(subject: &str, body: &str) -> Vec<u8> {
        compose_message("N7CPZ", &["W1AW"], &[], subject, body, 1_716_200_000).to_bytes()
    }

    /// An inbound message with an attacker-chosen `Mid` (the path-traversal vector).
    fn raw_with_mid(subject: &str, mid: &str) -> Vec<u8> {
        let mut msg = compose_message("N7CPZ", &["W1AW"], &[], subject, "x", 1_716_200_000);
        msg.set_header("Mid", mid);
        msg.to_bytes()
    }

    // ---- SECURITY: inbound `Mid` path-traversal guard (tuxlink-5lbm) ----

    #[test]
    fn validate_mid_accepts_real_ids_and_rejects_traversal() {
        assert!(validate_mid("LIHHQU663POB").is_ok()); // our generate_mid form
        assert!(validate_mid("INMID000001").is_ok());
        assert!(validate_mid("a-b_C9").is_ok());
        assert!(validate_mid("").is_err());
        assert!(validate_mid("../../etc/passwd").is_err());
        assert!(validate_mid("/abs/path").is_err());
        assert!(validate_mid("a/b").is_err());
        assert!(validate_mid("a..b").is_err()); // '.' is disallowed outright
        assert!(validate_mid(&"x".repeat(65)).is_err());
    }

    #[test]
    fn store_rejects_a_traversal_mid_and_writes_nothing_outside() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());

        // Relative `..` escape: a crafted received message must be rejected.
        let r = mbox.store(MailboxFolder::Inbox, &raw_with_mid("Evil", "../../../../pwned"));
        assert!(matches!(r, Err(BackendError::MessageRejected(_))));
        assert!(!dir.path().parent().unwrap().join("pwned.b2f").exists());

        // Absolute-path Mid: `Path::join` replaces the base dir entirely.
        let abs = dir.path().join("ABSPWNED");
        let r2 = mbox.store(MailboxFolder::Inbox, &raw_with_mid("Evil2", abs.to_str().unwrap()));
        assert!(matches!(r2, Err(BackendError::MessageRejected(_))));
        assert!(!dir.path().join("ABSPWNED.b2f").exists());

        // And nothing legitimately landed in the inbox.
        assert!(mbox.list(MailboxFolder::Inbox).unwrap().is_empty());
    }

    #[test]
    fn store_still_accepts_a_normal_inbound_message() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let id = mbox.store(MailboxFolder::Inbox, &raw("Hello", "Body")).unwrap();
        assert!(validate_mid(&id.0).is_ok());
        assert_eq!(mbox.list(MailboxFolder::Inbox).unwrap().len(), 1);
    }

    #[test]
    fn read_and_move_reject_a_traversal_message_id() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        // A traversal id arriving via the frontend IPC boundary must be rejected.
        let evil_id = MessageId("../../../../etc/shadow".to_string());
        assert!(matches!(
            mbox.read(MailboxFolder::Inbox, &evil_id),
            Err(BackendError::MessageRejected(_))
        ));
        assert!(matches!(
            mbox.move_to(MailboxFolder::Inbox, MailboxFolder::Archive, &evil_id),
            Err(BackendError::MessageRejected(_))
        ));
        assert!(mbox.read_identity_tag(MailboxFolder::Inbox, &evil_id).is_none());
    }

    fn raw_at(subject: &str, body: &str, ts: u64) -> Vec<u8> {
        compose_message("N7CPZ", &["W1AW"], &[], subject, body, ts).to_bytes()
    }

    /// The on-disk root the un-namespaced `Mailbox` methods resolve received-mail
    /// folders + user folders into after the Phase-4 namespace refactor
    /// (tuxlink-2ns7): `<root>/mailbox/_default`. Sent/Outbox stay at `<root>`.
    fn default_root(p: &std::path::Path) -> std::path::PathBuf {
        p.join("mailbox").join("_default")
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

    // tuxlink-2ns7 Task 5: the send-time Outbox→Sent move must carry the
    // <mid>.identity sidecar so the sent copy retains its authoring identity
    // and no orphan tag is left in Outbox.
    #[test]
    fn send_time_move_outbox_to_sent_keeps_identity_tag() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let id = mbox.for_identity("W1ABC").store(MailboxFolder::Outbox, &raw("Out", "x")).unwrap();
        assert_eq!(mbox.read_identity_tag(MailboxFolder::Outbox, &id).as_deref(), Some("W1ABC"));

        mbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &id).unwrap();

        assert!(mbox.read_identity_tag(MailboxFolder::Outbox, &id).is_none(), "no orphan tag in outbox");
        assert_eq!(
            mbox.read_identity_tag(MailboxFolder::Sent, &id).as_deref(),
            Some("W1ABC"),
            "the identity tag travels with the message into Sent"
        );
    }

    // Phase 7 (tuxlink-noa0): the stored `<mid>.identity` sidecar must surface
    // onto the list-row MessageMeta so the mailbox identity filter has data to
    // act on. Untagged messages list with `identity == None` (match "All" only).
    #[test]
    fn list_surfaces_the_identity_tag_onto_meta() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        // Distinct timestamps → distinct MIDs (the MID derives from
        // sender/recipient/time, NOT subject/body, so `raw()` alone would
        // collide both messages onto one MID in the shared Outbox).
        let tagged = mbox
            .for_identity("W1ABC")
            .store(MailboxFolder::Outbox, &raw_at("Tagged", "x", 1_716_200_000))
            .unwrap();
        // A bare store (no identity) leaves the Outbox message untagged.
        let untagged = mbox
            .store(MailboxFolder::Outbox, &raw_at("Untagged", "y", 1_716_300_000))
            .unwrap();
        assert_ne!(tagged, untagged, "test needs two distinct MIDs");

        let metas = mbox.list(MailboxFolder::Outbox).unwrap();
        let tag_of = |id: &MessageId| {
            metas
                .iter()
                .find(|m| &m.id == id)
                .unwrap()
                .identity
                .clone()
        };
        assert_eq!(tag_of(&tagged).as_deref(), Some("W1ABC"));
        assert_eq!(tag_of(&untagged), None);
    }

    // Phase 7 (tuxlink-noa0): received mail (Inbox/Archive + user folders) has no
    // per-message sidecar — it is namespaced by FULL — so its list-row identity
    // must be stamped from the namespace it was listed from, else the mailbox
    // filter would empty the Inbox for any concrete-identity selection.
    #[test]
    fn list_stamps_received_mail_identity_from_namespace() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let scoped = mbox.for_identity("W1ABC");
        let id = scoped.store(MailboxFolder::Inbox, &raw("In", "x")).unwrap();

        // Listed under its own namespace → tagged with that FULL.
        let metas = scoped.list(MailboxFolder::Inbox).unwrap();
        assert_eq!(
            metas.iter().find(|m| m.id == id).unwrap().identity.as_deref(),
            Some("W1ABC"),
            "received mail lists tagged with its FULL namespace"
        );

        // A default-identity mailbox stamps the un-namespaced (None) Inbox too.
        let mbox2 = Mailbox::new(dir.path())
            .with_default_identity(&crate::identity::Callsign::parse("W1ABC").unwrap());
        let metas2 = mbox2.list(MailboxFolder::Inbox).unwrap();
        assert_eq!(
            metas2.iter().find(|m| m.id == id).unwrap().identity.as_deref(),
            Some("W1ABC"),
            "the default namespace stamps the bare `list` path"
        );
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
    fn sent_and_outbox_never_report_unread() {
        // Unread is a received-mail concept; the Mock B sidebar shows Sent as
        // a total, not an unread count. Sent/Outbox must always report unread =
        // false even with no read-marker on disk. (Archive, also a non-inbox
        // system folder, DOES surface unread — see `archive_messages_surface_unread`.)
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        mbox.store(MailboxFolder::Sent, &raw("S", "x")).unwrap();
        mbox.store(MailboxFolder::Outbox, &raw("O", "y")).unwrap();

        assert!(!mbox.list(MailboxFolder::Sent).unwrap()[0].unread);
        assert!(!mbox.list(MailboxFolder::Outbox).unwrap()[0].unread);
    }

    #[test]
    fn archive_messages_surface_unread() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let id = mbox.store(MailboxFolder::Archive, &raw("A", "x")).unwrap();
        assert!(mbox.list(MailboxFolder::Archive).unwrap()[0].unread, "archived received mail surfaces unread");

        mbox.set_read_state(&FolderRef::System(MailboxFolder::Archive), &id, true).unwrap();
        assert!(!mbox.list(MailboxFolder::Archive).unwrap()[0].unread);
    }

    #[test]
    fn user_folder_messages_surface_unread() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let uf = mbox.create_user_folder("Skywarn", None).unwrap();
        let id = mbox.store(MailboxFolder::Inbox, &raw("Net", "x")).unwrap();
        mbox.move_between(FolderRef::System(MailboxFolder::Inbox), FolderRef::User(uf.slug.clone()), &id).unwrap();

        assert!(mbox.list_user(&uf.slug).unwrap()[0].unread, "received mail in a user folder surfaces unread");
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
    fn mark_unread_removes_the_marker() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let id = mbox.store(MailboxFolder::Inbox, &raw("Hello", "Body")).unwrap();
        mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();
        assert!(!mbox.list(MailboxFolder::Inbox).unwrap()[0].unread);

        mbox.set_read_state(&FolderRef::System(MailboxFolder::Inbox), &id, false).unwrap();

        assert!(mbox.list(MailboxFolder::Inbox).unwrap()[0].unread, "mark unread must re-surface as unread");
    }

    #[test]
    fn set_read_state_on_missing_message_is_not_an_error() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let r = mbox.set_read_state(&FolderRef::System(MailboxFolder::Inbox), &MessageId::new("NOPE"), false);
        assert!(r.is_ok());
    }

    #[test]
    fn set_read_state_works_on_a_user_folder() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let uf = mbox.create_user_folder("Net Traffic", None).unwrap();
        let id = mbox.store(MailboxFolder::Inbox, &raw("Net", "x")).unwrap();
        mbox.move_between(FolderRef::System(MailboxFolder::Inbox), FolderRef::User(uf.slug.clone()), &id).unwrap();

        mbox.set_read_state(&FolderRef::User(uf.slug.clone()), &id, true).unwrap();

        // Directly verify the sidecar was written to the user folder directory.
        // This is the most precise check of set_read_state's FolderRef::User arm:
        // it fails if that arm is broken, independent of list_user's surfacing.
        let sidecar = crate::user_folders::folder_dir(&default_root(dir.path()), &uf.slug)
            .join(format!("{}.read", id.0));
        assert!(
            sidecar.exists(),
            "set_read_state(FolderRef::User) must write the <mid>.read sidecar at {sidecar:?}"
        );
        // And confirm list_user surfaces the now-read state: with the sidecar
        // written and surface_unread=true (tuxlink-etxt), the message reports read.
        assert!(!mbox.list_user(&uf.slug).unwrap()[0].unread, "a read message in a user folder must surface unread=false");
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
            !default_root(dir.path()).join("inbox").join(format!("{}.read", id.0)).exists(),
            "source read-marker should not be orphaned"
        );
        assert!(
            default_root(dir.path()).join("archive").join(format!("{}.read", id.0)).exists(),
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

    // Fix 4 (Codex P2): a Sent message moved to Archive must NOT surface as
    // unread — the operator wrote it, so it is inherently read. Without this
    // fix, the absence of a .read sidecar in the Archive dir caused it to
    // appear unread (Archive surfaces unread for received mail).
    #[test]
    fn sent_message_moved_to_archive_is_not_unread() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let id = mbox.store(MailboxFolder::Sent, &raw("Sent msg", "body")).unwrap();

        // Confirm Sent itself never reports unread (established behaviour).
        assert!(!mbox.list(MailboxFolder::Sent).unwrap()[0].unread);

        // Move to Archive via move_to (system → system).
        mbox.move_to(MailboxFolder::Sent, MailboxFolder::Archive, &id).unwrap();

        // After the move the message must NOT surface as unread.
        let archived = mbox.list(MailboxFolder::Archive).unwrap();
        assert_eq!(archived.len(), 1, "moved message must appear in Archive");
        assert!(
            !archived[0].unread,
            "a Sent message moved to Archive must not surface as unread (Fix 4)"
        );
    }

    // Fix 4 (cont.): same contract via move_between, covering the Outbox case
    // and the FolderRef::System path taken by mailbox_move for system folders.
    #[test]
    fn outbox_message_moved_to_archive_via_move_between_is_not_unread() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        let id = mbox.store(MailboxFolder::Outbox, &raw("Queued msg", "body")).unwrap();

        mbox.move_between(
            FolderRef::System(MailboxFolder::Outbox),
            FolderRef::System(MailboxFolder::Archive),
            &id,
        ).unwrap();

        let archived = mbox.list(MailboxFolder::Archive).unwrap();
        assert_eq!(archived.len(), 1);
        assert!(
            !archived[0].unread,
            "an Outbox message moved to Archive must not surface as unread (Fix 4)"
        );
    }

    // ========================================================================
    // Phase 4 (tuxlink-2ns7): per-FULL received-mail namespace.
    // ========================================================================

    // Task 1: storing to W1ABC's inbox and W7XYZ's inbox keeps them separate.
    #[test]
    fn per_full_inbox_namespaces_are_independent() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());

        // Store one message into each FULL's inbox via the namespace selector.
        // NOTE (heron-beaver-gorge, plan↔source reconciliation): `generate_mid`
        // keys on `(unix_secs, callsign)` only — NOT subject/body — so two
        // fixtures composed with the same fixed timestamp by `raw()` collide on
        // MID. The plan's `assert_ne!(a.0, x.0)` therefore needs distinct
        // timestamps to hold; use `raw_at` with timestamps a minute apart.
        let a = mbox
            .for_identity("W1ABC")
            .store(MailboxFolder::Inbox, &raw_at("For Alpha", "a", 1_716_200_000))
            .unwrap();
        let x = mbox
            .for_identity("W7XYZ")
            .store(MailboxFolder::Inbox, &raw_at("For Xray", "x", 1_716_200_060))
            .unwrap();
        assert_ne!(a.0, x.0, "fixtures must carry distinct MIDs");

        // Each inbox sees only its own message.
        let alpha = mbox.for_identity("W1ABC").list(MailboxFolder::Inbox).unwrap();
        let xray = mbox.for_identity("W7XYZ").list(MailboxFolder::Inbox).unwrap();
        assert_eq!(alpha.len(), 1);
        assert_eq!(alpha[0].subject, "For Alpha");
        assert_eq!(xray.len(), 1);
        assert_eq!(xray[0].subject, "For Xray");

        // On-disk paths are namespaced under mailbox/<CALLSIGN>/inbox.
        assert!(dir.path().join("mailbox/W1ABC/inbox").join(format!("{}.b2f", a.0)).exists());
        assert!(dir.path().join("mailbox/W7XYZ/inbox").join(format!("{}.b2f", x.0)).exists());
        // And NOT at the legacy flat path.
        assert!(!dir.path().join("inbox").join(format!("{}.b2f", a.0)).exists());
    }

    // Task 2: unread accounting is per-FULL and matches list().
    #[test]
    fn unread_is_per_full_and_matches_list_predicate() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());

        // W1ABC inbox: unread until marked; W7XYZ inbox untouched.
        let id = mbox.for_identity("W1ABC").store(MailboxFolder::Inbox, &raw("Net", "x")).unwrap();
        assert!(
            mbox.for_identity("W1ABC").list(MailboxFolder::Inbox).unwrap()[0].unread,
            "fresh per-FULL inbox message is unread"
        );
        // W7XYZ's inbox is empty — namespaces don't bleed.
        assert!(mbox.for_identity("W7XYZ").list(MailboxFolder::Inbox).unwrap().is_empty());

        // Marking read in W1ABC's namespace flips only W1ABC.
        mbox.for_identity("W1ABC")
            .set_read_state(&FolderRef::System(MailboxFolder::Inbox), &id, true)
            .unwrap();
        assert!(!mbox.for_identity("W1ABC").list(MailboxFolder::Inbox).unwrap()[0].unread);
    }

    // Task 3: user folders are per-FULL; "Skywarn" under W1ABC is independent of W7XYZ.
    #[test]
    fn user_folders_are_per_full() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());

        let a = mbox.for_identity("W1ABC").create_user_folder("Skywarn", None).unwrap();
        assert_eq!(a.slug, "skywarn");
        // W7XYZ sees no folders — its registry is separate.
        assert!(mbox.for_identity("W7XYZ").list_user_folders().is_empty());
        // W1ABC sees exactly its one folder.
        let listed = mbox.for_identity("W1ABC").list_user_folders();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].slug, "skywarn");

        // The registries live in distinct per-FULL roots.
        assert!(dir.path().join("mailbox/W1ABC/.folders.json").exists());
        assert!(!dir.path().join("mailbox/W7XYZ/.folders.json").exists());
    }

    // Task 3: tactical mail lands in the parent FULL's inbox via the routing seam.
    #[test]
    fn tactical_mail_routes_into_parent_full_inbox() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());

        // A message addressed to tactical label "AIDSTATION-1" whose parent FULL is
        // W1ABC must land in W1ABC's inbox (tuxlink-73nl will later route it into a
        // per-tactical folder; this phase only guarantees the parent-FULL landing).
        let id = mbox
            .route_received("W1ABC", Some("AIDSTATION-1"), &raw("Tactical traffic", "x"))
            .unwrap();
        let inbox = mbox.for_identity("W1ABC").list(MailboxFolder::Inbox).unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].id, id);
        // It is NOT in W7XYZ's inbox.
        assert!(mbox.for_identity("W7XYZ").list(MailboxFolder::Inbox).unwrap().is_empty());
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

    /// The on-disk root the un-namespaced `Mailbox` methods resolve received-mail
    /// folders + user folders into after the Phase-4 namespace refactor
    /// (tuxlink-2ns7): `<root>/mailbox/_default`. Sent/Outbox stay at `<root>`.
    fn default_root(p: &std::path::Path) -> std::path::PathBuf {
        p.join("mailbox").join("_default")
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

    // tuxlink-mzm4: store() must seed the search-index `unread` column with the
    // SAME predicate list() uses — matches!(folder, Inbox | Archive) — not
    // Inbox-only. A message stored DIRECTLY into Archive surfaces as unread in
    // list() (no .read sidecar yet), but the old Inbox-only seed recorded it
    // read in the index, so a search filtered by unread silently dropped it.
    // This pins index↔list agreement at store time. (Fails against the old
    // `folder == MailboxFolder::Inbox` seed: the Archive case recorded 0.)
    #[test]
    fn store_seeds_index_unread_matching_list_predicate() {
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());

        let index_unread = |mid: &str| -> i64 {
            idx.lock()
                .unwrap()
                .conn
                .query_row("SELECT unread FROM messages_meta WHERE mid = ?1", [mid], |r| r.get(0))
                .unwrap()
        };

        // Inbox: unread in both list() and the index (established behaviour).
        let inbox = mbox.store(MailboxFolder::Inbox, &raw("In", "x")).unwrap();
        assert!(mbox.list(MailboxFolder::Inbox).unwrap()[0].unread);
        assert_eq!(index_unread(&inbox.0), 1, "Inbox: index unread must match list()");

        // Archive (the bug): list() surfaces a freshly-stored Archive message as
        // unread (no .read sidecar), so the index must agree.
        let archived = mbox.store(MailboxFolder::Archive, &raw("Arch", "y")).unwrap();
        assert!(
            mbox.list(MailboxFolder::Archive).unwrap()[0].unread,
            "list() surfaces a freshly-stored Archive message as unread"
        );
        assert_eq!(
            index_unread(&archived.0),
            1,
            "Archive: index unread must match list() (tuxlink-mzm4)"
        );

        // Sent: never unread in list() nor the index (control — predicate excludes it).
        let sent = mbox.store(MailboxFolder::Sent, &raw("Sent", "z")).unwrap();
        assert!(!mbox.list(MailboxFolder::Sent).unwrap()[0].unread);
        assert_eq!(index_unread(&sent.0), 0, "Sent: not unread in index");
    }

    // tuxlink-2ns7 Task 4: storing to Sent under a FULL writes the <mid>.identity
    // sidecar next to the shared Sent b2f AND tags the search index.
    #[test]
    fn sent_store_tags_identity_sidecar_and_index() {
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());
        let id = mbox.for_identity("W1ABC").store(MailboxFolder::Sent, &raw("Sent", "x")).unwrap();

        // On-disk identity sidecar next to the shared Sent b2f.
        let sidecar = dir.path().join("sent").join(format!("{}.identity", id.0));
        assert!(sidecar.exists(), "Sent message must carry a <mid>.identity sidecar");
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap().trim(), "W1ABC");

        // Index tag.
        let tag: Option<String> = idx
            .lock().unwrap().conn
            .query_row("SELECT identity_tag FROM messages_meta WHERE mid = ?1", [&id.0], |r| r.get(0))
            .unwrap();
        assert_eq!(tag.as_deref(), Some("W1ABC"));
    }

    // tuxlink-2ns7 Task 6: a pre-Phase-4 flat mailbox migrates to the per-FULL
    // layout — Inbox + Archive re-home under mailbox/<DEFAULT_FULL>/, the legacy
    // flat dirs are gone, and the shared Sent/Outbox messages are tagged with the
    // default FULL via a <mid>.identity sidecar.
    #[test]
    fn migrate_legacy_flat_layout_to_per_full() {
        let dir = tempdir().unwrap();
        // Seed a pre-Phase-4 flat layout directly on disk.
        let mbox = Mailbox::new(dir.path());
        // store() (un-namespaced) writes to mailbox/_default; for the migration
        // test we want the TRUE legacy flat path, so write raw into <root>/inbox
        // etc.
        for (folder, subj) in [("inbox", "In A"), ("archive", "Arch A")] {
            let raw = raw(subj, "x");
            let mid = crate::winlink::message::Message::from_bytes(&raw)
                .unwrap()
                .header("Mid")
                .unwrap()
                .to_string();
            let d = dir.path().join(folder);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{mid}.b2f")), &raw).unwrap();
        }
        let sent_raw = raw("Sent A", "s");
        let sent_mid = crate::winlink::message::Message::from_bytes(&sent_raw)
            .unwrap()
            .header("Mid")
            .unwrap()
            .to_string();
        std::fs::create_dir_all(dir.path().join("sent")).unwrap();
        std::fs::write(dir.path().join("sent").join(format!("{sent_mid}.b2f")), &sent_raw).unwrap();

        // Migrate, naming the default FULL.
        let full = crate::identity::Callsign::parse("N7CPZ").unwrap();
        mbox.migrate_legacy_layout(&full).unwrap();

        // Inbox + Archive now live under mailbox/N7CPZ/.
        assert_eq!(mbox.for_identity("N7CPZ").list(MailboxFolder::Inbox).unwrap().len(), 1);
        assert_eq!(mbox.for_identity("N7CPZ").list(MailboxFolder::Archive).unwrap().len(), 1);
        // Legacy flat dirs are gone.
        assert!(!dir.path().join("inbox").exists());
        assert!(!dir.path().join("archive").exists());
        // Sent stays shared but is now tagged with the default FULL.
        assert_eq!(
            mbox.read_identity_tag(MailboxFolder::Sent, &MessageId(sent_mid)).as_deref(),
            Some("N7CPZ"),
        );
        assert!(dir.path().join("sent").exists(), "Sent stays at the shared root");
    }

    // tuxlink-2ns7 Phase 4 wiring: the migration runs on EVERY launch, so a
    // second run must be a no-op (idempotent at each sub-step). Seed a flat
    // layout, migrate once, then migrate again and assert the inbox count is
    // unchanged and the second run is Ok.
    #[test]
    fn migrate_legacy_layout_is_idempotent() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        // Seed a flat inbox with one message.
        let raw_msg = raw("In A", "x");
        let mid = crate::winlink::message::Message::from_bytes(&raw_msg)
            .unwrap()
            .header("Mid")
            .unwrap()
            .to_string();
        let d = dir.path().join("inbox");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("{mid}.b2f")), &raw_msg).unwrap();

        let full = crate::identity::Callsign::parse("N7CPZ").unwrap();
        mbox.migrate_legacy_layout(&full).unwrap();
        let after_first = mbox.for_identity("N7CPZ").list(MailboxFolder::Inbox).unwrap().len();
        assert_eq!(after_first, 1);

        // Second run: no-op, still Ok, count unchanged.
        mbox.migrate_legacy_layout(&full).unwrap();
        let after_second = mbox.for_identity("N7CPZ").list(MailboxFolder::Inbox).unwrap().len();
        assert_eq!(after_second, 1, "a second migration run must not change the inbox count");
    }

    // tuxlink-2ns7 Phase 4 wiring: the migration absorbs the now-removed
    // `heal_misplaced_inbox`'s job — it forward-migrates the OLD per-FULL scheme
    // `<root>/<CALLSIGN>/inbox` (via `per_full_root`) into the new
    // `mailbox/<CALLSIGN>/inbox` location.
    #[test]
    fn migrate_legacy_layout_rescues_old_scheme() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path());
        // Seed a message under the OLD per-FULL scheme: <root>/<CALLSIGN>/inbox.
        let raw_msg = raw("Old scheme", "x");
        let mid = crate::winlink::message::Message::from_bytes(&raw_msg)
            .unwrap()
            .header("Mid")
            .unwrap()
            .to_string();
        let old_inbox = per_full_root(dir.path(), "N7CPZ").join("inbox");
        std::fs::create_dir_all(&old_inbox).unwrap();
        std::fs::write(old_inbox.join(format!("{mid}.b2f")), &raw_msg).unwrap();

        let full = crate::identity::Callsign::parse("N7CPZ").unwrap();
        mbox.migrate_legacy_layout(&full).unwrap();

        // It lands in the NEW location and is listable via the scoped view.
        assert!(
            dir.path().join("mailbox/N7CPZ/inbox").join(format!("{mid}.b2f")).exists(),
            "old-scheme message must land under mailbox/<CALLSIGN>/inbox"
        );
        let inbox = mbox.for_identity("N7CPZ").list(MailboxFolder::Inbox).unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].id.0, mid);
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
        assert!(default_root(dir.path()).join("inbox").join(format!("{}.read", id.0)).exists());

        mbox.move_to(MailboxFolder::Inbox, MailboxFolder::Archive, &id).unwrap();

        // The b2f file lives in archive/ and is gone from inbox/.
        assert!(default_root(dir.path()).join("archive").join(format!("{}.b2f", id.0)).exists());
        assert!(!default_root(dir.path()).join("inbox").join(format!("{}.b2f", id.0)).exists());
        // The read marker traveled with the message — no orphan in inbox/.
        assert!(default_root(dir.path()).join("archive").join(format!("{}.read", id.0)).exists());
        assert!(!default_root(dir.path()).join("inbox").join(format!("{}.read", id.0)).exists());

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
        let ares = mbox.create_user_folder("ARES Drills", None).unwrap();
        assert_eq!(ares.slug, "ares-drills");
        assert_eq!(ares.display_name, "ARES Drills");
        let prep = mbox.create_user_folder("Disaster Prep", None).unwrap();
        assert_eq!(prep.slug, "disaster-prep");

        // Listed in creation order.
        let list = mbox.list_user_folders();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].slug, "ares-drills");
        assert_eq!(list[1].slug, "disaster-prep");

        // The on-disk directories exist (under the _default namespace root).
        assert!(default_root(dir.path()).join("ares-drills").is_dir());
        assert!(default_root(dir.path()).join("disaster-prep").is_dir());
        // The registry file exists.
        assert!(default_root(dir.path()).join(".folders.json").exists());

        // Delete with Delete cascade (no messages inside; safe).
        mbox.delete_user_folder("ares-drills", DeleteAction::Delete).unwrap();
        let after = mbox.list_user_folders();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].slug, "disaster-prep");
        assert!(!default_root(dir.path()).join("ares-drills").exists());
    }

    #[test]
    fn create_rejects_reserved_names_and_duplicates() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());

        // Reserved system names (case-insensitive).
        assert!(mbox.create_user_folder("Inbox", None).is_err());
        assert!(mbox.create_user_folder("ARCHIVE", None).is_err());

        // First create OK, duplicate rejected.
        mbox.create_user_folder("ARES Drills", None).unwrap();
        assert!(mbox.create_user_folder("ARES Drills", None).is_err());
        // Same slug from a different display would also collide.
        assert!(mbox.create_user_folder("ares drills", None).is_err());
    }

    #[test]
    fn move_between_inbox_and_user_folder_relocates_message() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let id = mbox.store(MailboxFolder::Inbox, &raw("hello", "body")).unwrap();
        let _ = mbox.create_user_folder("ARES Drills", None).unwrap();

        // Inbox → user folder.
        mbox.move_between(
            FolderRef::System(MailboxFolder::Inbox),
            FolderRef::User("ares-drills".into()),
            &id,
        )
        .unwrap();
        assert!(default_root(dir.path()).join("ares-drills").join(format!("{}.b2f", id.0)).exists());
        assert!(!default_root(dir.path()).join("inbox").join(format!("{}.b2f", id.0)).exists());

        // User folder → Archive.
        mbox.move_between(
            FolderRef::User("ares-drills".into()),
            FolderRef::System(MailboxFolder::Archive),
            &id,
        )
        .unwrap();
        assert!(default_root(dir.path()).join("archive").join(format!("{}.b2f", id.0)).exists());
        assert!(!default_root(dir.path()).join("ares-drills").join(format!("{}.b2f", id.0)).exists());
    }

    #[test]
    fn delete_user_folder_with_move_to_inbox_relocates_messages() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let _ = mbox.create_user_folder("ARES Drills", None).unwrap();
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
        assert!(default_root(dir.path()).join("inbox").join(format!("{}.b2f", id.0)).exists());
        assert!(!default_root(dir.path()).join("ares-drills").exists());
        assert!(mbox.list_user_folders().is_empty());
    }

    #[test]
    fn delete_user_folder_with_delete_cascade_removes_messages() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let _ = mbox.create_user_folder("ARES Drills", None).unwrap();
        let id = mbox.store(MailboxFolder::Inbox, &raw("hello", "body")).unwrap();
        mbox.move_between(
            FolderRef::System(MailboxFolder::Inbox),
            FolderRef::User("ares-drills".into()),
            &id,
        )
        .unwrap();

        mbox.delete_user_folder("ares-drills", DeleteAction::Delete).unwrap();
        assert!(!default_root(dir.path()).join("ares-drills").exists());
        assert!(!default_root(dir.path()).join("inbox").join(format!("{}.b2f", id.0)).exists());
        assert!(mbox.list_user_folders().is_empty());
    }

    // tuxlink-ejph: renaming a user folder updates display_name but NOT the
    // slug — the on-disk dir name stays the same so messages don't have to
    // move. Subsequent list_user_folders() reflects the new name.
    #[test]
    fn rename_user_folder_updates_display_name_only() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let f = mbox.create_user_folder("ARES Drills", None).unwrap();
        assert_eq!(f.slug, "ares-drills");

        let renamed = mbox.rename_user_folder("ares-drills", "June Drills").unwrap();
        assert_eq!(renamed.slug, "ares-drills", "slug must stay stable");
        assert_eq!(renamed.display_name, "June Drills");

        // The on-disk directory still uses the original slug (no churn).
        assert!(default_root(dir.path()).join("ares-drills").is_dir());

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
        mbox.create_user_folder("ARES Drills", None).unwrap();

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

    // ---- Nested folders (tuxlink-ka3z): re-parent + cascade delete ----

    // Seeds a `.b2f` under the `_default` namespace root so it lines up with the
    // Phase-4 namespaced `folder_dir` / user-folder paths (tuxlink-2ns7). `root`
    // is the mailbox root (tempdir); the file lands at
    // `<root>/mailbox/_default/<folder>/<name>`.
    fn seed_b2f(root: &std::path::Path, folder: &str, name: &str) {
        let dir = default_root(root).join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), b"raw").unwrap();
    }

    #[test]
    fn move_user_folder_reparents_without_touching_disk() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let nets = mbox.create_user_folder("Nets", None).unwrap();
        let weather = mbox.create_user_folder("Weather", None).unwrap();
        seed_b2f(dir.path(), &weather.slug, "M1.b2f");

        mbox.move_user_folder(&weather.slug, Some(&nets.slug)).unwrap();

        let reg = user_folders::load_registry(&default_root(dir.path()));
        let moved = reg.folders.iter().find(|f| f.slug == weather.slug).unwrap();
        assert_eq!(moved.parent_slug.as_deref(), Some("nets"));
        // Metadata-only: the message file never left the weather dir.
        assert!(default_root(dir.path()).join(&weather.slug).join("M1.b2f").exists());
    }

    #[test]
    fn move_user_folder_rejects_invalid_reparent_and_promotes() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let nets = mbox.create_user_folder("Nets", None).unwrap();
        let ares = mbox.create_user_folder("ARES", Some(&nets.slug)).unwrap();
        let weather = mbox.create_user_folder("Weather", None).unwrap();
        // weather under ares (a subfolder) violates the 2-level cap.
        assert!(mbox.move_user_folder(&weather.slug, Some(&ares.slug)).is_err());
        // promoting ares to top level is fine.
        mbox.move_user_folder(&ares.slug, None).unwrap();
        let reg = user_folders::load_registry(&default_root(dir.path()));
        assert_eq!(reg.folders.iter().find(|f| f.slug == ares.slug).unwrap().parent_slug, None);
    }

    #[test]
    fn delete_parent_cascades_children_move_to_inbox_and_returns_slugs() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let nets = mbox.create_user_folder("Nets", None).unwrap();
        let ares = mbox.create_user_folder("ARES", Some(&nets.slug)).unwrap();
        seed_b2f(dir.path(), &nets.slug, "P1.b2f");
        seed_b2f(dir.path(), &ares.slug, "C1.b2f");

        let removed = mbox.delete_user_folder(&nets.slug, DeleteAction::MoveToInbox).unwrap();

        // Both folders gone from registry + disk; both messages in Inbox.
        assert!(user_folders::load_registry(&default_root(dir.path())).folders.is_empty());
        assert!(default_root(dir.path()).join("inbox").join("P1.b2f").exists());
        assert!(default_root(dir.path()).join("inbox").join("C1.b2f").exists());
        assert!(!default_root(dir.path()).join(&ares.slug).exists());
        // A5: returns parent + child so the UI can clear a stale selection.
        let mut got = removed;
        got.sort();
        assert_eq!(got, vec!["ares".to_string(), "nets".to_string()]);
    }

    #[test]
    fn delete_parent_cascades_children_delete_mode() {
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let nets = mbox.create_user_folder("Nets", None).unwrap();
        let ares = mbox.create_user_folder("ARES", Some(&nets.slug)).unwrap();
        seed_b2f(dir.path(), &ares.slug, "C1.b2f");

        mbox.delete_user_folder(&nets.slug, DeleteAction::Delete).unwrap();
        assert!(user_folders::load_registry(&default_root(dir.path())).folders.is_empty());
        assert!(!default_root(dir.path()).join(&ares.slug).exists());
        assert!(!default_root(dir.path()).join(&nets.slug).exists());
    }

    #[test]
    fn delete_cascade_refuses_on_destination_collision() {
        // P0 finding #1: a MID already in Inbox must NOT be silently overwritten.
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let nets = mbox.create_user_folder("Nets", None).unwrap();
        seed_b2f(dir.path(), "inbox", "M1.b2f"); // pre-existing in destination
        seed_b2f(dir.path(), &nets.slug, "M1.b2f"); // collides

        let err = mbox.delete_user_folder(&nets.slug, DeleteAction::MoveToInbox);
        assert!(err.is_err(), "must refuse rather than overwrite");
        // Nothing moved: both files still in place, folder still present.
        assert!(default_root(dir.path()).join("inbox").join("M1.b2f").exists());
        assert!(default_root(dir.path()).join(&nets.slug).join("M1.b2f").exists());
        assert!(!user_folders::load_registry(&default_root(dir.path())).folders.is_empty());
    }

    #[test]
    fn delete_cascade_refuses_on_child_vs_child_collision() {
        // P0 finding #1: two affected folders sharing a filename would merge.
        let dir = tempdir().unwrap();
        let mbox = Mailbox::new(dir.path().to_path_buf());
        let nets = mbox.create_user_folder("Nets", None).unwrap();
        let ares = mbox.create_user_folder("ARES", Some(&nets.slug)).unwrap();
        seed_b2f(dir.path(), &nets.slug, "DUP.b2f");
        seed_b2f(dir.path(), &ares.slug, "DUP.b2f");

        assert!(mbox.delete_user_folder(&nets.slug, DeleteAction::MoveToInbox).is_err());
        // Refused before any move: both folders + files intact.
        assert!(default_root(dir.path()).join(&nets.slug).join("DUP.b2f").exists());
        assert!(default_root(dir.path()).join(&ares.slug).join("DUP.b2f").exists());
    }

    #[test]
    fn delete_cascade_updates_search_index() {
        // A1 step (e): with an index attached, a cascaded permanent-delete drops
        // the row. Uses a real message so the index has a row keyed by Mid.
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());
        let nets = mbox.create_user_folder("Nets", None).unwrap();
        let ares = mbox.create_user_folder("ARES", Some(&nets.slug)).unwrap();
        let id = mbox.store(MailboxFolder::Inbox, &raw("netlog", "body")).unwrap();
        mbox.move_between(
            FolderRef::System(MailboxFolder::Inbox),
            FolderRef::User(ares.slug.clone()),
            &id,
        )
        .unwrap();
        assert_eq!(idx.lock().unwrap().count().unwrap(), 1, "message indexed after store");

        // Deleting the parent cascades the child's message out of the index.
        mbox.delete_user_folder(&nets.slug, DeleteAction::Delete).unwrap();
        assert_eq!(
            idx.lock().unwrap().count().unwrap(),
            0,
            "permanently-deleted cascaded message must be gone from the search index"
        );
    }
}
