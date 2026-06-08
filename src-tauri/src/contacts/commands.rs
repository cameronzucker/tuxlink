//! Contacts tauri commands + id/timestamp stamping — Task A2.
//!
//! Plan: docs/superpowers/plans/2026-06-07-contacts-favorites.md → "Locked
//! decisions" + "### Task A2". The [`store`](super::store) layer takes
//! fully-formed `Contact`/`Group` values; id + RFC3339-UTC timestamp STAMPING
//! is THIS (command) layer's job.
//!
//! **Stamping contract:**
//! - A NEW entity (empty `id`) gets a fresh uuid-v4 `id` and
//!   `created_at == updated_at == now`.
//! - An UPDATE (non-empty `id`) PRESERVES the caller-supplied `created_at`,
//!   keeps the `id`, and sets `updated_at = now`.
//!
//! The pure helpers ([`stamp_contact`]/[`stamp_group`]) take an injected `now`
//! string and a `new_id` factory so they are deterministically testable without
//! a Tauri harness; the thin `#[tauri::command]` wrappers below call them with
//! the real wall clock + a uuid-v4 factory.
//!
//! **Cross-window invalidation (H9):** every mutating command emits the
//! app-level Tauri event [`CONTACTS_CHANGED_EVENT`] (`contacts:changed`) AFTER a
//! successful flush, so a separate webview window (e.g. an open Compose) can
//! invalidate its `useContacts` cache. The frontend listener lands in Task A4.

use std::sync::{Arc, Mutex};

use super::store::{Contact, ContactsError, ContactsFile, ContactsStore, Group};

/// App-level Tauri event emitted on every contacts mutation so other webview
/// windows can invalidate their cached contacts (H9). Payload is `()`.
pub const CONTACTS_CHANGED_EVENT: &str = "contacts:changed";

/// Mint a fresh uuid-v4 string id (mirrors `search/saved.rs:93`,
/// `forms/draft_library.rs:208`).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Stamp a contact for persistence. A NEW contact (empty `id`) gets a fresh id
/// (via `new_id`) and `created_at == updated_at == now`; an UPDATE preserves the
/// existing `id` + `created_at` and sets `updated_at = now`.
///
/// Pure + deterministic: `now` and `new_id` are injected so this is unit-tested
/// without a Tauri harness.
pub fn stamp_contact(mut c: Contact, now: &str, new_id: impl FnOnce() -> String) -> Contact {
    if c.id.trim().is_empty() {
        c.id = new_id();
        c.created_at = now.to_string();
    }
    // created_at is preserved on update (caller supplies the original).
    c.updated_at = now.to_string();
    c
}

/// Stamp a group for persistence. Same new-vs-update semantics as
/// [`stamp_contact`].
pub fn stamp_group(mut g: Group, now: &str, new_id: impl FnOnce() -> String) -> Group {
    if g.id.trim().is_empty() {
        g.id = new_id();
        g.created_at = now.to_string();
    }
    g.updated_at = now.to_string();
    g
}

/// The current wall-clock instant as an RFC3339 UTC string (e.g.
/// `2026-06-08T12:34:56+00:00`).
fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Emit the cross-window `contacts:changed` event (H9). Best-effort: a failed
/// emit must NOT fail the mutation (the on-disk write already succeeded).
fn emit_changed(app: &tauri::AppHandle) {
    use tauri::Emitter as _;
    let _ = app.emit(CONTACTS_CHANGED_EVENT, ());
}

/// Read the whole contacts file (contacts + groups + schema_version).
#[tauri::command]
pub fn contacts_read(
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
) -> Result<ContactsFile, ContactsError> {
    let store = svc.lock().expect("contacts store mutex poisoned");
    Ok(store.file().clone())
}

/// Insert or update a contact. Stamps id (if empty) + timestamps, persists,
/// emits `contacts:changed`, and returns the STORED contact (so the caller
/// learns the assigned id + timestamps).
#[tauri::command]
pub fn contact_upsert(
    contact: Contact,
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    app: tauri::AppHandle,
) -> Result<Contact, ContactsError> {
    let stamped = stamp_contact(contact, &now_utc(), new_id);
    {
        let mut store = svc.lock().expect("contacts store mutex poisoned");
        store.contact_upsert(stamped.clone())?;
    }
    emit_changed(&app);
    Ok(stamped)
}

/// Delete a contact by id (no-op if absent). Persists + emits `contacts:changed`.
#[tauri::command]
pub fn contact_delete(
    id: String,
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    app: tauri::AppHandle,
) -> Result<(), ContactsError> {
    {
        let mut store = svc.lock().expect("contacts store mutex poisoned");
        store.contact_delete(&id)?;
    }
    emit_changed(&app);
    Ok(())
}

/// Insert or update a group. Stamps id (if empty) + timestamps, persists, emits
/// `contacts:changed`, returns the STORED group.
#[tauri::command]
pub fn group_upsert(
    group: Group,
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    app: tauri::AppHandle,
) -> Result<Group, ContactsError> {
    let stamped = stamp_group(group, &now_utc(), new_id);
    {
        let mut store = svc.lock().expect("contacts store mutex poisoned");
        store.group_upsert(stamped.clone())?;
    }
    emit_changed(&app);
    Ok(stamped)
}

/// Delete a group by id (no-op if absent). Persists + emits `contacts:changed`.
#[tauri::command]
pub fn group_delete(
    id: String,
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    app: tauri::AppHandle,
) -> Result<(), ContactsError> {
    {
        let mut store = svc.lock().expect("contacts store mutex poisoned");
        store.group_delete(&id)?;
    }
    emit_changed(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contacts::store::GroupMember;

    fn blank_contact() -> Contact {
        Contact {
            id: String::new(),
            name: "Pat Example".to_string(),
            callsign: "W6ABC-7".to_string(),
            email: None,
            tactical: None,
            notes: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn blank_group() -> Group {
        Group {
            id: String::new(),
            name: "ARES Net".to_string(),
            members: vec![GroupMember::Raw {
                callsign: "KE7VAR".to_string(),
            }],
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn new_id_is_nonempty_and_unique() {
        let a = new_id();
        let b = new_id();
        assert!(!a.is_empty());
        assert!(!b.is_empty());
        assert_ne!(a, b, "uuid-v4 ids must be unique");
    }

    #[test]
    fn stamp_new_contact_sets_id_and_equal_timestamps() {
        let now = "2026-06-08T12:00:00+00:00";
        let stamped = stamp_contact(blank_contact(), now, || "fixed-id".to_string());
        assert_eq!(stamped.id, "fixed-id", "empty id must be assigned");
        assert_eq!(stamped.created_at, now);
        assert_eq!(stamped.updated_at, now);
        assert_eq!(
            stamped.created_at, stamped.updated_at,
            "new contact: created_at == updated_at"
        );
    }

    #[test]
    fn stamp_update_contact_preserves_created_at_and_id() {
        let created = "2026-06-01T08:00:00+00:00";
        let now = "2026-06-08T12:00:00+00:00";
        let mut existing = blank_contact();
        existing.id = "existing-id".to_string();
        existing.created_at = created.to_string();
        existing.updated_at = created.to_string();

        let stamped = stamp_contact(existing, now, || {
            panic!("update must NOT mint a new id")
        });
        assert_eq!(stamped.id, "existing-id", "update preserves id");
        assert_eq!(stamped.created_at, created, "update preserves created_at");
        assert_eq!(stamped.updated_at, now, "update advances updated_at");
        assert_ne!(stamped.created_at, stamped.updated_at);
    }

    #[test]
    fn stamp_new_group_sets_id_and_equal_timestamps() {
        let now = "2026-06-08T12:00:00+00:00";
        let stamped = stamp_group(blank_group(), now, || "grp-id".to_string());
        assert_eq!(stamped.id, "grp-id");
        assert_eq!(stamped.created_at, now);
        assert_eq!(stamped.updated_at, now);
    }

    #[test]
    fn stamp_update_group_preserves_created_at_and_id() {
        let created = "2026-06-01T08:00:00+00:00";
        let now = "2026-06-08T12:00:00+00:00";
        let mut existing = blank_group();
        existing.id = "grp-existing".to_string();
        existing.created_at = created.to_string();
        existing.updated_at = created.to_string();

        let stamped = stamp_group(existing, now, || panic!("update must NOT mint a new id"));
        assert_eq!(stamped.id, "grp-existing");
        assert_eq!(stamped.created_at, created);
        assert_eq!(stamped.updated_at, now);
    }

    #[test]
    fn whitespace_only_id_is_treated_as_new() {
        // A caller passing "   " (not a real id) gets a fresh id, not a
        // mistaken in-place update against a phantom record.
        let now = "2026-06-08T12:00:00+00:00";
        let mut c = blank_contact();
        c.id = "   ".to_string();
        let stamped = stamp_contact(c, now, || "minted".to_string());
        assert_eq!(stamped.id, "minted");
        assert_eq!(stamped.created_at, now);
    }
}
