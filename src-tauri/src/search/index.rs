use rusqlite::Connection;
use std::path::PathBuf;
use thiserror::Error;

/// Schema version. Bumped when the table layout changes; `Index::open` detects
/// drift and the caller can trigger a rebuild.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("schema drift: index is at v{found}, current is v{current}")]
    SchemaDrift { found: u32, current: u32 },
}

pub struct Index {
    conn: Connection,
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Index").finish_non_exhaustive()
    }
}

impl Index {
    /// Open or create the index at `path`. If the file does not exist, the
    /// schema is created. If it exists but is at an older `user_version`,
    /// returns `Err(IndexError::SchemaDrift)` — caller (e.g. rebuild-index)
    /// decides whether to recreate.
    pub fn open(path: PathBuf) -> Result<Self, IndexError> {
        // Detect whether the file pre-exists before opening it. A new file has
        // no user_version (SQLite defaults to 0) and needs schema init. A
        // pre-existing file with user_version=0 is a pre-versioned database
        // (schema drift).
        let preexisted = path.exists();
        let conn = Connection::open(&path)?;
        let found: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if found == 0 && !preexisted {
            Self::init_schema(&conn)?;
        } else if found != SCHEMA_VERSION {
            return Err(IndexError::SchemaDrift { found, current: SCHEMA_VERSION });
        }
        Ok(Self { conn })
    }

    /// DDL is wrapped in a transaction so a kill between CREATE statements and
    /// the user_version pragma cannot leave the file at a partial-schema /
    /// user_version=0 state that is recoverable only via manual delete.
    fn init_schema(conn: &Connection) -> Result<(), IndexError> {
        conn.execute_batch(
            r#"
            BEGIN;

            CREATE VIRTUAL TABLE messages_fts USING fts5 (
                mid               UNINDEXED,
                folder            UNINDEXED,
                subject,
                body,
                form_field_values,
                tokenize = 'porter unicode61 remove_diacritics 2'
            );

            CREATE TABLE messages_meta (
                mid              TEXT PRIMARY KEY,
                folder           TEXT NOT NULL,
                from_addr        TEXT,
                to_addrs         TEXT,
                cc_addrs         TEXT,
                date_sent        INTEGER,
                date_received    INTEGER,
                unread           INTEGER NOT NULL DEFAULT 0,
                form_type        TEXT,
                has_attachments  INTEGER NOT NULL DEFAULT 0,
                attachment_count INTEGER NOT NULL DEFAULT 0,
                transport_used   TEXT,
                direction        TEXT NOT NULL,
                message_size     INTEGER NOT NULL,
                routing_path     TEXT,
                indexed_at       INTEGER NOT NULL
            );

            CREATE INDEX idx_meta_date_recv ON messages_meta(date_received);
            CREATE INDEX idx_meta_date_sent ON messages_meta(date_sent);
            CREATE INDEX idx_meta_from      ON messages_meta(from_addr);
            CREATE INDEX idx_meta_form_type ON messages_meta(form_type);
            CREATE INDEX idx_meta_folder    ON messages_meta(folder);

            COMMIT;
            "#,
        )?;
        // PRAGMA user_version is set outside the transaction: some SQLite
        // versions reject schema-version pragmas inside an explicit transaction.
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_creates_schema_on_first_use() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("search.db");
        let idx = Index::open(path.clone()).expect("first open creates schema");
        // tables exist
        let names: Vec<String> = idx
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','view') ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(names.iter().any(|n| n == "messages_meta"));
        assert!(names.iter().any(|n| n == "messages_fts"));
        // user_version is set
        let v: u32 = idx.conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn open_is_idempotent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("search.db");
        let _ = Index::open(path.clone()).unwrap();
        let _ = Index::open(path.clone()).unwrap();
        // no error on second open
    }

    #[test]
    fn open_detects_schema_drift() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("search.db");
        // hand-roll an old-version db
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA user_version = 0;").unwrap();
        }
        let err = Index::open(path).unwrap_err();
        match err {
            IndexError::SchemaDrift { found: 0, current: 1 } => {}
            other => panic!("expected SchemaDrift {{ found: 0, current: 1 }}, got {other:?}"),
        }
    }
}
