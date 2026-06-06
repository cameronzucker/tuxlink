//! FormDraftLibrary — per-form-id slot library backed by a dedicated SQLite database.
//!
//! bd tuxlink-hnkn P2 Task 4 (backend)
//!
//! ## Schema-home decision: Option B (sibling SQLite store)
//!
//! The plan offered three options for where to host the `form_draft_slots`
//! table:
//!
//! - **Option A:** Bump `Index::SCHEMA_VERSION` to v4 and embed the table
//!   inside the search index.
//! - **Option B:** Create a dedicated `form_draft_library.db` with its own
//!   `user_version` pragma, alongside (but independent of) the search index.
//! - **Option C:** Reuse `Index`'s `Connection` with a separately-managed
//!   `user_version` key via a custom pragma.
//!
//! **Option B is chosen.** Rationale: form-draft slots are operator-authored
//! state (named templates for recurring nets, events, etc.) that must survive
//! a search-index rebuild. Under Option A, `tauri_search_rebuild_index` wipes
//! and recreates `search.db`, taking the operator's saved slots with it — the
//! UX failure mode (silently deleted named templates) is unacceptable. Option C
//! is a hack: SQLite's `user_version` pragma is a single integer, not a
//! per-table versioning key; there is no standard mechanism to manage two
//! independent schema versions in one connection file. Option B imposes a
//! single additional file on disk (`<app_data>/native-mbox/form_draft_library.db`)
//! and gives the draft library its own, independently-versioned schema with no
//! coupling to the search index lifecycle.
//!
//! The database is initialised lazily on first use via [`DraftLibrary::open`]
//! and held behind an `Arc<Mutex<Connection>>` so the struct is `Send + Sync`
//! and can be registered with Tauri's `.manage()`.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Schema versioning
// ---------------------------------------------------------------------------

/// `user_version` for `form_draft_library.db`.
///
/// v1 (tuxlink-hnkn P2 Task 4): initial schema — `form_draft_slots` table +
/// index on `form_id`.
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single named draft slot for a specific form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FormDraftSlot {
    pub slot_id: String,
    pub form_id: String,
    pub label: String,
    /// The saved field-values map, stored as an arbitrary JSON value
    /// (typically an object mapping field-id → value, but the store is
    /// opaque to the schema — callers own the shape).
    pub payload: serde_json::Value,
    /// RFC 3339 UTC timestamp.
    pub created_at: String,
    /// RFC 3339 UTC timestamp.
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum DraftLibraryError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("schema drift: db is at v{found}, current is v{current}")]
    SchemaDrift { found: u32, current: u32 },
    #[error("lock poisoned")]
    LockPoisoned,
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Managed state for the form-draft slot library.
///
/// Constructed once at app startup and registered via `.manage(Arc::new(...))`.
/// The inner `Connection` is wrapped in a `Mutex` because `rusqlite::Connection`
/// is not `Sync`. All public methods take `&self` and lock internally.
pub struct DraftLibrary {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for DraftLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DraftLibrary").finish_non_exhaustive()
    }
}

impl DraftLibrary {
    /// Open or create the draft library at `path`.
    ///
    /// On a fresh database the schema is initialised atomically. An existing
    /// database at an older `user_version` returns `Err(SchemaDrift)`.
    pub fn open(path: PathBuf) -> Result<Self, DraftLibraryError> {
        let preexisted = path.exists();
        let conn = Connection::open(&path)?;
        let found: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if found == 0 && !preexisted {
            Self::init_schema(&conn)?;
        } else if found != SCHEMA_VERSION {
            return Err(DraftLibraryError::SchemaDrift { found, current: SCHEMA_VERSION });
        }
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// DDL wrapped in a transaction so a crash between the CREATE statements and
    /// the `user_version` pragma cannot leave the file at a partial/zero-version
    /// state.
    fn init_schema(conn: &Connection) -> Result<(), DraftLibraryError> {
        conn.execute_batch(
            r#"
            BEGIN;

            CREATE TABLE IF NOT EXISTS form_draft_slots (
                slot_id      TEXT PRIMARY KEY,
                form_id      TEXT NOT NULL,
                label        TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at   TEXT NOT NULL,
                updated_at   TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS form_draft_slots_by_form
                ON form_draft_slots(form_id);

            COMMIT;
            "#,
        )?;
        // PRAGMA user_version must be set outside the transaction on some
        // SQLite versions.
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // CRUD operations
    // -----------------------------------------------------------------------

    /// Return all slots for `form_id`, ordered by `created_at` ascending
    /// (oldest first — stable, intuitive ordering for a picker list).
    pub fn list(&self, form_id: &str) -> Result<Vec<FormDraftSlot>, DraftLibraryError> {
        let conn = self.conn.lock().map_err(|_| DraftLibraryError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT slot_id, form_id, label, payload_json, created_at, updated_at
             FROM form_draft_slots
             WHERE form_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![form_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;

        let mut slots = Vec::new();
        for row in rows {
            let (slot_id, form_id, label, payload_json, created_at, updated_at) = row?;
            let payload = serde_json::from_str(&payload_json)?;
            slots.push(FormDraftSlot { slot_id, form_id, label, payload, created_at, updated_at });
        }
        Ok(slots)
    }

    /// Insert or update a slot.
    ///
    /// - If `slot_id` is `None`, a new UUID v4 is minted and `created_at` is
    ///   set to the current UTC time.
    /// - If `slot_id` is `Some(id)` and a row with that id already exists, the
    ///   `label`, `payload_json`, and `updated_at` are updated in place;
    ///   `created_at` is preserved.
    /// - If `slot_id` is `Some(id)` and no row exists yet, a new row is
    ///   inserted with the provided id.
    pub fn upsert(
        &self,
        slot_id: Option<String>,
        form_id: String,
        label: String,
        payload: serde_json::Value,
    ) -> Result<FormDraftSlot, DraftLibraryError> {
        let conn = self.conn.lock().map_err(|_| DraftLibraryError::LockPoisoned)?;
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let payload_json = serde_json::to_string(&payload)?;

        match slot_id {
            None => {
                // New slot — mint a UUID.
                let id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO form_draft_slots
                        (slot_id, form_id, label, payload_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    rusqlite::params![id, form_id, label, payload_json, now],
                )?;
                Ok(FormDraftSlot {
                    slot_id: id,
                    form_id,
                    label,
                    payload,
                    created_at: now.clone(),
                    updated_at: now,
                })
            }
            Some(id) => {
                // Attempt an update first; if no row was changed, fall through to insert.
                let changed = conn.execute(
                    "UPDATE form_draft_slots
                     SET label = ?2, payload_json = ?3, updated_at = ?4
                     WHERE slot_id = ?1",
                    rusqlite::params![id, label, payload_json, now],
                )?;

                if changed > 0 {
                    // Fetch the preserved created_at.
                    let created_at: String = conn.query_row(
                        "SELECT created_at FROM form_draft_slots WHERE slot_id = ?1",
                        rusqlite::params![id],
                        |r| r.get(0),
                    )?;
                    Ok(FormDraftSlot {
                        slot_id: id,
                        form_id,
                        label,
                        payload,
                        created_at,
                        updated_at: now,
                    })
                } else {
                    // No existing row — insert with the caller-provided id.
                    conn.execute(
                        "INSERT INTO form_draft_slots
                            (slot_id, form_id, label, payload_json, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                        rusqlite::params![id, form_id, label, payload_json, now],
                    )?;
                    Ok(FormDraftSlot {
                        slot_id: id,
                        form_id,
                        label,
                        payload,
                        created_at: now.clone(),
                        updated_at: now,
                    })
                }
            }
        }
    }

    /// Delete a slot by id. No-op-safe if the id does not exist.
    pub fn delete(&self, slot_id: &str) -> Result<(), DraftLibraryError> {
        let conn = self.conn.lock().map_err(|_| DraftLibraryError::LockPoisoned)?;
        conn.execute(
            "DELETE FROM form_draft_slots WHERE slot_id = ?1",
            rusqlite::params![slot_id],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    /// Open a DraftLibrary in a temp directory and return BOTH so the caller's
    /// `_dir` binding keeps the directory alive for the test duration. Dropping
    /// `_dir` would unlink the directory on Linux, making the open SQLite file
    /// read-only (the kernel keeps the inode but the path is gone, and rusqlite
    /// re-opens on write).
    fn open_tmp() -> (DraftLibrary, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let lib = DraftLibrary::open(dir.path().join("form_draft_library.db")).unwrap();
        (lib, dir)
    }

    // -----------------------------------------------------------------------
    // Schema / open tests
    // -----------------------------------------------------------------------

    #[test]
    fn open_creates_schema_on_first_use() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("form_draft_library.db");
        let lib = DraftLibrary::open(path.clone()).expect("open should succeed on new file");
        let conn = lib.conn.lock().unwrap();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(names.iter().any(|n| n == "form_draft_slots"));
        let v: u32 =
            conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn open_is_idempotent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("form_draft_library.db");
        let _ = DraftLibrary::open(path.clone()).unwrap();
        let _ = DraftLibrary::open(path.clone()).unwrap(); // second open must not error
    }

    #[test]
    fn open_detects_schema_drift() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("form_draft_library.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", 0_u32).unwrap();
        }
        let err = DraftLibrary::open(path).unwrap_err();
        assert!(matches!(err, DraftLibraryError::SchemaDrift { .. }));
    }

    // -----------------------------------------------------------------------
    // upsert_creates_new_slot
    // -----------------------------------------------------------------------

    #[test]
    fn upsert_creates_new_slot() {
        let (lib, _dir) = open_tmp();
        let slot = lib
            .upsert(None, "Winlink_Check-In".into(), "Monday Night Net".into(), json!({}))
            .unwrap();
        assert!(!slot.slot_id.is_empty(), "slot_id must be assigned");
        assert_eq!(slot.form_id, "Winlink_Check-In");
        assert_eq!(slot.label, "Monday Night Net");
        assert_eq!(slot.created_at, slot.updated_at, "fresh slot: created == updated");
    }

    // -----------------------------------------------------------------------
    // upsert_with_existing_slot_id_updates
    // -----------------------------------------------------------------------

    #[test]
    fn upsert_with_existing_slot_id_updates() {
        let (lib, _dir) = open_tmp();
        let original = lib
            .upsert(None, "Winlink_Check-In".into(), "Original".into(), json!({"x": 1}))
            .unwrap();
        let id = original.slot_id.clone();
        let created_at = original.created_at.clone();

        // Give wall-clock time to advance so updated_at differs from created_at.
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let updated = lib
            .upsert(Some(id.clone()), "Winlink_Check-In".into(), "Updated".into(), json!({"x": 2}))
            .unwrap();

        assert_eq!(updated.slot_id, id);
        assert_eq!(updated.label, "Updated");
        assert_eq!(updated.payload, json!({"x": 2}));
        // created_at must be preserved; updated_at must strictly advance
        // (the preceding sleep is > 1s, so the RFC 3339 second-precision
        // timestamps must differ).
        assert_eq!(updated.created_at, created_at, "created_at must be preserved on update");
        assert!(
            updated.updated_at > created_at,
            "updated_at must strictly advance past created_at"
        );

        // Confirm only one row in DB.
        let list = lib.list("Winlink_Check-In").unwrap();
        assert_eq!(list.len(), 1);
    }

    // -----------------------------------------------------------------------
    // list_filters_by_form_id
    // -----------------------------------------------------------------------

    #[test]
    fn list_filters_by_form_id() {
        let (lib, _dir) = open_tmp();
        lib.upsert(None, "Winlink_Check-In".into(), "Slot A".into(), json!({})).unwrap();
        lib.upsert(None, "Winlink_Check-In".into(), "Slot B".into(), json!({})).unwrap();
        lib.upsert(None, "ICS213_v2".into(), "Slot C".into(), json!({})).unwrap();

        let checkin = lib.list("Winlink_Check-In").unwrap();
        assert_eq!(checkin.len(), 2);
        assert!(checkin.iter().all(|s| s.form_id == "Winlink_Check-In"));

        let ics = lib.list("ICS213_v2").unwrap();
        assert_eq!(ics.len(), 1);
        assert_eq!(ics[0].label, "Slot C");

        // Unknown form_id → empty list.
        assert!(lib.list("Unknown_Form").unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // delete_removes_slot
    // -----------------------------------------------------------------------

    #[test]
    fn delete_removes_slot() {
        let (lib, _dir) = open_tmp();
        let slot = lib
            .upsert(None, "Winlink_Check-In".into(), "To Delete".into(), json!({}))
            .unwrap();
        assert_eq!(lib.list("Winlink_Check-In").unwrap().len(), 1);

        lib.delete(&slot.slot_id).unwrap();
        assert!(lib.list("Winlink_Check-In").unwrap().is_empty());
    }

    #[test]
    fn delete_nonexistent_is_ok() {
        let (lib, _dir) = open_tmp();
        // Deleting a non-existent id must be a no-op, not an error.
        lib.delete("does-not-exist").unwrap();
    }

    // -----------------------------------------------------------------------
    // payload_json_round_trips_unicode
    // -----------------------------------------------------------------------

    #[test]
    fn payload_json_round_trips_unicode() {
        let (lib, _dir) = open_tmp();
        let complex = json!({
            "callsign": "N7CPZ",
            "comment": "73 de tuxlink — unicode: 日本語 emoji 🎙️",
            "nested": { "a": [1, 2, 3], "b": null }
        });
        let slot = lib
            .upsert(None, "Winlink_Check-In".into(), "Unicode".into(), complex.clone())
            .unwrap();
        let fetched = lib.list("Winlink_Check-In").unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].payload, complex);
        // Round-trip via the returned struct too.
        assert_eq!(slot.payload, complex);
    }

    // -----------------------------------------------------------------------
    // list order
    // -----------------------------------------------------------------------

    #[test]
    fn list_returns_slots_in_created_at_asc_order() {
        let (lib, _dir) = open_tmp();
        // Insert three slots with a tiny sleep between them so created_at differs.
        lib.upsert(None, "Winlink_Check-In".into(), "First".into(), json!({})).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        lib.upsert(None, "Winlink_Check-In".into(), "Second".into(), json!({})).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        lib.upsert(None, "Winlink_Check-In".into(), "Third".into(), json!({})).unwrap();

        let list = lib.list("Winlink_Check-In").unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].label, "First");
        assert_eq!(list[1].label, "Second");
        assert_eq!(list[2].label, "Third");
    }

    // -----------------------------------------------------------------------
    // Persistence across instances
    // -----------------------------------------------------------------------

    #[test]
    fn data_persists_across_open_calls() {
        let (lib, dir) = open_tmp();
        let slot = lib
            .upsert(None, "Winlink_Check-In".into(), "Persist".into(), json!({"x": 42}))
            .unwrap();
        drop(lib); // close the connection

        // Re-open the same file.
        let lib2 = DraftLibrary::open(dir.path().join("form_draft_library.db")).unwrap();
        let list = lib2.list("Winlink_Check-In").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].slot_id, slot.slot_id);
        assert_eq!(list[0].payload, json!({"x": 42}));
    }
}
