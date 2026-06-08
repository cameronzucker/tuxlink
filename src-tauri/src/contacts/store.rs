//! Contacts JSON store — the `contacts.json` address book backing the Contacts
//! feature (Compose autocomplete + the Contacts surface build on it).
//!
//! Plan: docs/superpowers/plans/2026-06-07-contacts-favorites.md → "Locked
//! decisions" + "Task A1". Hardened by a 5-round adversarial review; the
//! data-loss invariants below are load-bearing, not stylistic.
//!
//! **Forward-compat + data-loss policy (C1/M1):**
//! - `#[serde(default)]` additive tolerance on `ContactsFile` fields; NO
//!   `#[serde(deny_unknown_fields)]` (it would reject forward-version files and
//!   trigger data loss). Unknown future fields are silently ignored.
//! - [`ContactsStore::open`] is INFALLIBLE — it always returns a usable store.
//!   On ANY full parse/read failure it FIRST renames the unreadable file to
//!   `<name>.corrupt-<utc-ts>` (preserving the original bytes), `eprintln!`s a
//!   warning, THEN returns the default empty store. Never blocks startup; never
//!   silently overwrites a corrupt file's bytes with empty JSON. (Mirrors the
//!   DEGRADE-on-error pattern of `user_folders.rs::load_registry`, extended
//!   with corrupt-file preservation.)
//! - Hand-written `impl Default for ContactsFile` sets `schema_version:
//!   SCHEMA_VERSION` (= 1); NO `#[derive(Default)]` (it would write 0).
//!
//! **Atomic write:** serialize → write to `format!("{}.tmp", path_str)` (NOT
//! `path.with_extension("tmp")`, which would drop `.json`) → `fs::rename`;
//! `create_dir_all(parent)` first. Mirrors `user_folders.rs:182-192`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// On-disk schema version. Bumped only on a non-additive shape change.
pub const SCHEMA_VERSION: u32 = 1;

/// A single address-book contact. The callsign is the SSID-bearing primary
/// identity — NEVER strip the SSID. Timestamps are RFC3339 UTC; id/timestamp
/// STAMPING is the command layer's job (Task A2), not the store's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub callsign: String,
    pub email: Option<String>,
    pub tactical: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A distribution-group member. Stored as a `contact_id` reference when added
/// from a contact (so edits propagate), or a raw literal callsign when typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GroupMember {
    Contact { contact_id: String },
    Raw { callsign: String },
}

/// A distribution group (named set of members).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub members: Vec<GroupMember>,
    pub created_at: String,
    pub updated_at: String,
}

/// The on-disk file shape. `#[serde(default)]` on every field gives additive
/// forward-compat tolerance; there is deliberately NO `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactsFile {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub contacts: Vec<Contact>,
    #[serde(default)]
    pub groups: Vec<Group>,
}

// NO derive(Default) (M1) — hand-write Default so schema_version is 1, not 0.
impl Default for ContactsFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            contacts: vec![],
            groups: vec![],
        }
    }
}

/// Serializable error projection for the IPC boundary. Mirrors the
/// `#[serde(tag = "kind", content = "detail")]` discriminated-union shape used
/// by `ui_commands.rs::UiError` so the frontend gets a `{ kind, detail }` shape.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum ContactsError {
    #[error("io: {0}")]
    Io(String),
    #[error("serde: {0}")]
    Serde(String),
}

/// The contacts store: an in-memory [`ContactsFile`] plus the path it persists
/// to. Mutations flush eagerly. Construct via [`ContactsStore::open`].
pub struct ContactsStore {
    path: PathBuf,
    file: ContactsFile,
}

impl ContactsStore {
    /// Open the store at `path`. INFALLIBLE — always returns a usable store.
    ///
    /// - Missing file → default empty store.
    /// - Present + parseable → the parsed file.
    /// - Present + UNparseable (read error or JSON error): rename the file to
    ///   `<name>.corrupt-<utc-ts>` to PRESERVE the original bytes, `eprintln!`
    ///   a warning, then return the default empty store. The corrupt original
    ///   is never overwritten in place; a later flush writes only to `path`,
    ///   leaving the sidecar intact.
    pub fn open(path: PathBuf) -> Self {
        let file = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<ContactsFile>(&bytes) {
                Ok(parsed) => parsed,
                Err(e) => {
                    Self::quarantine_corrupt(&path, &bytes);
                    eprintln!(
                        "contacts: {} is unparseable, starting empty (original preserved): {e}",
                        path.display()
                    );
                    ContactsFile::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ContactsFile::default(),
            Err(e) => {
                // A non-NotFound read error (e.g. permission/partial). Try to
                // preserve whatever bytes we can read; if even that fails, we
                // still degrade to empty rather than blocking startup.
                if let Ok(bytes) = std::fs::read(&path) {
                    Self::quarantine_corrupt(&path, &bytes);
                }
                eprintln!(
                    "contacts: failed to read {}, starting empty: {e}",
                    path.display()
                );
                ContactsFile::default()
            }
        };
        Self { path, file }
    }

    /// Rename the unreadable file to a timestamped `.corrupt-*` sidecar,
    /// preserving the original bytes. Falls back to a copy-write if the rename
    /// itself fails (best-effort preservation; never panics).
    fn quarantine_corrupt(path: &std::path::Path, original: &[u8]) {
        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "contacts.json".to_string());
        let corrupt = path.with_file_name(format!("{name}.corrupt-{ts}"));
        if let Err(e) = std::fs::rename(path, &corrupt) {
            // Rename failed (e.g. cross-device); fall back to copying the bytes
            // out so the original is not lost when a later flush overwrites it.
            eprintln!(
                "contacts: could not rename corrupt {} → {} ({e}); copying bytes instead",
                path.display(),
                corrupt.display()
            );
            let _ = std::fs::write(&corrupt, original);
        }
    }

    /// Persist the in-memory file atomically: serialize → write to a sibling
    /// `<name>.tmp` → `rename` over the final path. `create_dir_all(parent)`
    /// first. Uses `format!("{}.tmp", name)` so the suffix is `contacts.json.tmp`
    /// (NOT `with_extension("tmp")`, which would drop `.json`).
    fn flush(&self) -> Result<(), ContactsError> {
        let json = serde_json::to_string_pretty(&self.file)
            .map_err(|e| ContactsError::Serde(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ContactsError::Io(e.to_string()))?;
        }
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "contacts.json".to_string());
        let tmp = self.path.with_file_name(format!("{name}.tmp"));
        std::fs::write(&tmp, json).map_err(|e| ContactsError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| ContactsError::Io(e.to_string()))?;
        Ok(())
    }

    /// All contacts (read-only view).
    pub fn contacts(&self) -> &[Contact] {
        &self.file.contacts
    }

    /// All groups (read-only view).
    pub fn groups(&self) -> &[Group] {
        &self.file.groups
    }

    /// The whole in-memory file (read-only view) — used by the `contacts_read`
    /// command (Task A2) to return the full DTO.
    pub fn file(&self) -> &ContactsFile {
        &self.file
    }

    /// Insert a contact, or replace the existing one with the same `id`. The
    /// store takes the contact as given (id/timestamp stamping is the command
    /// layer's job, Task A2). Flushes on success.
    pub fn contact_upsert(&mut self, c: Contact) -> Result<(), ContactsError> {
        match self.file.contacts.iter_mut().find(|x| x.id == c.id) {
            Some(existing) => *existing = c,
            None => self.file.contacts.push(c),
        }
        self.flush()
    }

    /// Remove a contact by id (no-op if absent). Flushes on success.
    pub fn contact_delete(&mut self, id: &str) -> Result<(), ContactsError> {
        self.file.contacts.retain(|c| c.id != id);
        self.flush()
    }

    /// Insert a group, or replace the existing one with the same `id`. Flushes.
    pub fn group_upsert(&mut self, g: Group) -> Result<(), ContactsError> {
        match self.file.groups.iter_mut().find(|x| x.id == g.id) {
            Some(existing) => *existing = g,
            None => self.file.groups.push(g),
        }
        self.flush()
    }

    /// Remove a group by id (no-op if absent). Flushes on success.
    pub fn group_delete(&mut self, id: &str) -> Result<(), ContactsError> {
        self.file.groups.retain(|g| g.id != id);
        self.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn contact(id: &str) -> Contact {
        Contact {
            id: id.to_string(),
            name: "Pat Example".to_string(),
            callsign: "W6ABC-7".to_string(),
            email: Some("w6abc@winlink.org".to_string()),
            tactical: None,
            notes: None,
            created_at: "2026-06-07T12:00:00+00:00".to_string(),
            updated_at: "2026-06-07T12:00:00+00:00".to_string(),
        }
    }

    fn group(id: &str) -> Group {
        Group {
            id: id.to_string(),
            name: "ARES Net".to_string(),
            members: vec![
                GroupMember::Contact { contact_id: "c1".to_string() },
                GroupMember::Raw { callsign: "KE7VAR".to_string() },
            ],
            created_at: "2026-06-07T12:00:00+00:00".to_string(),
            updated_at: "2026-06-07T12:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn open_missing_returns_empty() {
        let dir = tempdir().unwrap();
        let store = ContactsStore::open(dir.path().join("contacts.json"));
        assert_eq!(store.file().schema_version, SCHEMA_VERSION);
        assert!(store.contacts().is_empty());
        assert!(store.groups().is_empty());
    }

    #[test]
    fn fresh_empty_store_has_schema_version_1() {
        // M1: a brand-new store written to disk persists schema_version:1,
        // NOT 0 (guards against an accidental derive(Default)).
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        // A mutation triggers a flush, writing the file with the default version.
        store.contact_upsert(contact("c1")).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("\"schema_version\": 1"),
            "expected schema_version 1 on disk, got: {raw}"
        );
        // And reopening yields version 1.
        let reopened = ContactsStore::open(path);
        assert_eq!(reopened.file().schema_version, 1);
    }

    #[test]
    fn upsert_then_reopen_persists() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        store.contact_upsert(contact("c1")).unwrap();
        drop(store);
        let reopened = ContactsStore::open(path);
        assert_eq!(reopened.contacts().len(), 1);
        assert_eq!(reopened.contacts()[0].id, "c1");
        assert_eq!(reopened.contacts()[0].callsign, "W6ABC-7");
    }

    #[test]
    fn upsert_existing_updates_in_place() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        let mut c = contact("c1");
        store.contact_upsert(c.clone()).unwrap();
        // Upsert the SAME id with mutated fields + a changed updated_at,
        // preserved created_at.
        c.name = "Pat Renamed".to_string();
        c.updated_at = "2026-06-08T09:30:00+00:00".to_string();
        store.contact_upsert(c.clone()).unwrap();
        assert_eq!(store.contacts().len(), 1, "upsert must not duplicate");
        let stored = &store.contacts()[0];
        assert_eq!(stored.name, "Pat Renamed");
        assert_eq!(stored.created_at, "2026-06-07T12:00:00+00:00");
        assert_eq!(stored.updated_at, "2026-06-08T09:30:00+00:00");
    }

    #[test]
    fn delete_removes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        store.contact_upsert(contact("c1")).unwrap();
        store.contact_delete("c1").unwrap();
        drop(store);
        let reopened = ContactsStore::open(path);
        assert!(reopened.contacts().is_empty());
    }

    #[test]
    fn group_upsert_delete_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        store.group_upsert(group("g1")).unwrap();
        drop(store);
        let mut reopened = ContactsStore::open(path.clone());
        assert_eq!(reopened.groups().len(), 1);
        let g = &reopened.groups()[0];
        assert_eq!(g.members.len(), 2);
        assert!(matches!(g.members[0], GroupMember::Contact { .. }));
        assert!(matches!(g.members[1], GroupMember::Raw { .. }));
        // delete
        reopened.group_delete("g1").unwrap();
        drop(reopened);
        let again = ContactsStore::open(path);
        assert!(again.groups().is_empty());
    }

    #[test]
    fn unknown_top_level_field_tolerated() {
        // C1: a JSON file carrying an EXTRA top-level key (a future field) must
        // parse fine — known fields preserved, unknown ignored. This proves
        // #[serde(default)] additive tolerance AND that deny_unknown_fields is
        // ABSENT (with it, this parse would fail and trigger quarantine).
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let json = r#"{
            "schema_version": 1,
            "contacts": [
                {
                    "id": "c1", "name": "Pat", "callsign": "W6ABC-7",
                    "email": null, "tactical": null, "notes": null,
                    "created_at": "2026-06-07T12:00:00+00:00",
                    "updated_at": "2026-06-07T12:00:00+00:00"
                }
            ],
            "groups": [],
            "future_field_from_a_newer_version": {"nested": true}
        }"#;
        std::fs::write(&path, json).unwrap();
        let store = ContactsStore::open(path.clone());
        // Parsed cleanly (NOT quarantined): the contact is present.
        assert_eq!(store.contacts().len(), 1);
        assert_eq!(store.contacts()[0].id, "c1");
        // No corrupt sidecar should have been created.
        let sidecars: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
            .collect();
        assert!(
            sidecars.is_empty(),
            "tolerated unknown field must NOT trigger quarantine"
        );
    }

    #[test]
    fn open_on_corrupt_file_preserves_original_bytes() {
        // C1: garbage in contacts.json → open() returns empty AND leaves a
        // contacts.json.corrupt-<ts> sidecar holding the ORIGINAL bytes; a
        // subsequent mutate+flush must NOT have destroyed those bytes.
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let garbage = b"this is not valid json {{{ \x00\x01 broken";
        std::fs::write(&path, garbage).unwrap();

        let mut store = ContactsStore::open(path.clone());
        assert!(
            store.contacts().is_empty(),
            "corrupt file must degrade to empty store"
        );

        // A corrupt sidecar exists and holds the ORIGINAL bytes.
        let sidecar = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().contains("contacts.json.corrupt-"))
            .expect("expected a contacts.json.corrupt-<ts> sidecar");
        let preserved = std::fs::read(sidecar.path()).unwrap();
        assert_eq!(
            preserved, garbage,
            "corrupt sidecar must hold the original bytes verbatim"
        );

        // A subsequent mutate+flush writes the fresh empty file WITHOUT
        // destroying the preserved bytes.
        store.contact_upsert(contact("c1")).unwrap();
        let preserved_after = std::fs::read(sidecar.path()).unwrap();
        assert_eq!(
            preserved_after, garbage,
            "flush must not clobber the preserved corrupt bytes"
        );
        // And the live file is the new valid one.
        let reopened = ContactsStore::open(path);
        assert_eq!(reopened.contacts().len(), 1);
    }

    #[test]
    fn atomic_write_leaves_no_tmp() {
        // After a flush, no *.tmp remains in the dir (rename consumed it).
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path);
        store.contact_upsert(contact("c1")).unwrap();
        let tmps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "no .tmp file should remain after flush");
    }
}
