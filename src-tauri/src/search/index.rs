use rusqlite::Connection;
use std::path::PathBuf;
use thiserror::Error;

/// Schema version. Bumped when the table layout changes; `Index::open` detects
/// drift and the caller can trigger a rebuild.
///
/// v1 → v2 (tuxlink-g4dj): add `subject` column to `messages_meta`. Subject
/// already existed in `messages_fts` for free-text search; the column on
/// `messages_meta` is what `hit_to_dto` reads at query time so result rows
/// render with a non-empty subject. Existing v1 indices return SchemaDrift
/// from `Index::open`; the operator runs `tauri_search_rebuild_index` to
/// recreate from the mbox source.
///
/// v2 → v3 (tuxlink-0gsy): add `docs_fts` virtual table for user-guide
/// search. Existing v2 indices return SchemaDrift; the mod.rs build_service
/// recovery path recreates fresh and the docs table is repopulated on first
/// launch from the bundled docs/user-guide/*.md.
pub const SCHEMA_VERSION: u32 = 3;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("schema drift: index is at v{found}, current is v{current}")]
    SchemaDrift { found: u32, current: u32 },
}

pub struct Index {
    pub(crate) conn: Connection,
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
                subject          TEXT NOT NULL DEFAULT '',
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

            -- tuxlink-0gsy (spec §9.1): docs_fts holds bundled user-guide
            -- topics for help-window search. Populated by build_service on
            -- first launch from search/docs_bundle.rs's compiled-in markdown.
            CREATE VIRTUAL TABLE docs_fts USING fts5 (
                slug              UNINDEXED,
                title,
                body,
                tokenize = 'porter unicode61 remove_diacritics 2'
            );

            COMMIT;
            "#,
        )?;
        // PRAGMA user_version is set outside the transaction: some SQLite
        // versions reject schema-version pragmas inside an explicit transaction.
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }
}

use crate::search::extractor::IndexRow;

impl Index {
    /// Insert-or-replace `row` in both `messages_fts` and `messages_meta`.
    pub fn upsert(&self, row: &IndexRow) -> Result<(), IndexError> {
        let tx = self.conn.unchecked_transaction()?;
        // FTS5 does not support ON CONFLICT — use DELETE + INSERT for the FTS side.
        tx.execute(
            "DELETE FROM messages_fts WHERE mid = ?1",
            rusqlite::params![row.mid],
        )?;
        tx.execute(
            "INSERT INTO messages_fts (mid, folder, subject, body, form_field_values)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![row.mid, row.folder, row.subject, row.body, row.form_field_values],
        )?;
        tx.execute(
            "INSERT INTO messages_meta (
                mid, folder, subject, from_addr, to_addrs, cc_addrs,
                date_sent, date_received, unread,
                form_type, has_attachments, attachment_count,
                transport_used, direction, message_size, routing_path, indexed_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9,
                ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, strftime('%s','now')
             )
             ON CONFLICT(mid) DO UPDATE SET
                folder = excluded.folder,
                subject = excluded.subject,
                from_addr = excluded.from_addr,
                to_addrs = excluded.to_addrs,
                cc_addrs = excluded.cc_addrs,
                date_sent = excluded.date_sent,
                date_received = excluded.date_received,
                unread = excluded.unread,
                form_type = excluded.form_type,
                has_attachments = excluded.has_attachments,
                attachment_count = excluded.attachment_count,
                transport_used = excluded.transport_used,
                direction = excluded.direction,
                message_size = excluded.message_size,
                routing_path = excluded.routing_path,
                indexed_at = excluded.indexed_at",
            rusqlite::params![
                row.mid, row.folder, row.subject,
                row.from_addr,
                serde_json::to_string(&row.to_addrs).unwrap(),
                serde_json::to_string(&row.cc_addrs).unwrap(),
                row.date_sent, row.date_received,
                row.unread as i64,
                row.form_type,
                row.has_attachments as i64, row.attachment_count,
                row.transport_used,
                row.direction.as_str(),
                row.message_size,
                row.routing_path,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete(&self, mid: &str) -> Result<(), IndexError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM messages_fts WHERE mid = ?1", rusqlite::params![mid])?;
        tx.execute("DELETE FROM messages_meta WHERE mid = ?1", rusqlite::params![mid])?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_folder(&self, mid: &str, new_folder: &str) -> Result<(), IndexError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE messages_fts SET folder = ?2 WHERE mid = ?1",
            rusqlite::params![mid, new_folder],
        )?;
        tx.execute(
            "UPDATE messages_meta SET folder = ?2 WHERE mid = ?1",
            rusqlite::params![mid, new_folder],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_unread(&self, mid: &str, unread: bool) -> Result<(), IndexError> {
        self.conn.execute(
            "UPDATE messages_meta SET unread = ?2 WHERE mid = ?1",
            rusqlite::params![mid, unread as i64],
        )?;
        Ok(())
    }

    /// Count rows in `messages_meta` — for tests and `RebuildStats`.
    pub fn count(&self) -> Result<u32, IndexError> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM messages_meta", [], |r| r.get(0))?;
        Ok(n as u32)
    }

    /// Return sent + received messages in the half-open interval [start_epoch, end_epoch).
    /// Results are ordered by the effective timestamp (date_sent for outbound, date_received
    /// for inbound) ascending — matching ICS-309's chronological log convention.
    ///
    /// `start_epoch` and `end_epoch` are Unix seconds UTC.
    ///
    /// `direction` column stores `"sent"` (outbound) or `"received"` (inbound).
    /// The returned `LogRow::direction` is `"out"` | `"in"`.
    pub fn query_log_rows(
        &self,
        start_epoch: i64,
        end_epoch: i64,
    ) -> Result<Vec<crate::ui_commands::LogRow>, IndexError> {
        // Use CASE to pick the appropriate timestamp column per row direction.
        // `effective_ts` is used both for filtering and for ORDER BY.
        let mut stmt = self.conn.prepare(
            "SELECT
                mid,
                CASE direction WHEN 'sent' THEN date_sent ELSE date_received END AS effective_ts,
                from_addr,
                to_addrs,
                subject,
                direction
             FROM messages_meta
             WHERE direction IN ('sent', 'received')
               AND CASE direction WHEN 'sent' THEN date_sent ELSE date_received END
                   BETWEEN ?1 AND ?2
             ORDER BY effective_ts ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![start_epoch, end_epoch], |r| {
            let ts_epoch: i64 = r.get(1)?;
            let from_addr: Option<String> = r.get(2)?;
            let to_addrs_json: String = r.get(3).unwrap_or_else(|_| "[]".to_string());
            let subject: String = r.get(4)?;
            let direction_raw: String = r.get(5)?;

            // Decode first recipient from the JSON array stored in to_addrs.
            let first_to: String = serde_json::from_str::<Vec<String>>(&to_addrs_json)
                .ok()
                .and_then(|v| v.into_iter().next())
                .unwrap_or_default();

            // Format epoch as RFC 3339 UTC datetime for display.
            let datetime = epoch_to_rfc3339(ts_epoch);

            Ok(crate::ui_commands::LogRow {
                datetime,
                from: from_addr.unwrap_or_default(),
                to: first_to,
                subject,
                direction: if direction_raw == "sent" {
                    "out".to_string()
                } else {
                    "in".to_string()
                },
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(IndexError::Sqlite)
    }
}

/// Convert a Unix epoch (seconds UTC) to an RFC 3339 UTC string.
/// e.g. 1716199980 → "2024-05-20T10:13:00Z"
fn epoch_to_rfc3339(epoch: i64) -> String {
    // Simple Gregorian calendar conversion — no external crate dependency.
    // Uses the inverse of Howard Hinnant's days_from_civil algorithm.
    let secs = epoch % 86_400;
    let days = epoch / 86_400;
    let (y, m, d) = civil_from_days(days);
    let hour = secs / 3600;
    let min = (secs % 3600) / 60;
    let sec = secs % 60;
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Inverse of Howard Hinnant's `days_from_civil` — convert days-since-1970 to (y, m, d).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

use crate::search::query::{compose, SqlParam};
use crate::search::types::QuerySpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryHit {
    pub mid: String,
    pub folder: String,
    pub subject: String,
    pub from_addr: Option<String>,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub date_sent: Option<i64>,
    pub date_received: Option<i64>,
    pub unread: bool,
    pub form_type: Option<String>,
    pub has_attachments: bool,
    pub attachment_count: u32,
    pub transport_used: Option<String>,
    pub direction: String,
    pub message_size: u32,
    pub routing_path: Option<String>,
}

impl Index {
    pub fn query(&self, spec: &QuerySpec) -> Result<Vec<QueryHit>, IndexError> {
        let (sql, params) = compose(spec);
        let mut stmt = self.conn.prepare(&sql)?;
        let rs = params
            .iter()
            .map(|p| match p {
                SqlParam::Text(s) => rusqlite::types::Value::Text(s.clone()),
                SqlParam::Int(i) => rusqlite::types::Value::Integer(*i),
                SqlParam::Null => rusqlite::types::Value::Null,
            })
            .collect::<Vec<_>>();
        let param_refs: Vec<&dyn rusqlite::ToSql> =
            rs.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(QueryHit {
                    mid: row.get(0)?,
                    folder: row.get(1)?,
                    subject: row.get(2)?,
                    from_addr: row.get(3)?,
                    to_addrs: serde_json::from_str(&row.get::<_, String>(4)?)
                        .unwrap_or_default(),
                    cc_addrs: serde_json::from_str(&row.get::<_, String>(5)?)
                        .unwrap_or_default(),
                    date_sent: row.get(6)?,
                    date_received: row.get(7)?,
                    unread: row.get::<_, i64>(8)? != 0,
                    form_type: row.get(9)?,
                    has_attachments: row.get::<_, i64>(10)? != 0,
                    attachment_count: row.get::<_, i64>(11)? as u32,
                    transport_used: row.get(12)?,
                    direction: row.get(13)?,
                    message_size: row.get::<_, i64>(14)? as u32,
                    routing_path: row.get(15)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod query_integration {
    use super::*;
    use crate::search::extractor::Direction;
    use crate::search::types::{FilterKey, FilterValue, QuerySpec};
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn r(mid: &str, folder: &str, from: &str, subject: &str, body: &str) -> IndexRow {
        IndexRow {
            mid: mid.into(),
            folder: folder.into(),
            subject: subject.into(),
            body: body.into(),
            form_field_values: "".into(),
            from_addr: Some(from.into()),
            to_addrs: vec!["N7CPZ".into()],
            cc_addrs: vec![],
            date_sent: None,
            date_received: Some(1_716_200_000),
            unread: true,
            form_type: None,
            has_attachments: false,
            attachment_count: 0,
            transport_used: Some("telnet".into()),
            direction: Direction::Received,
            message_size: body.len() as u32,
            routing_path: None,
        }
    }

    #[test]
    fn freetext_returns_only_matching_messages() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&r("A", "inbox", "KX5DD", "DAMAGE report", "powerlines"))
            .unwrap();
        idx.upsert(&r("B", "inbox", "WX5RES", "weather brief", "ridge"))
            .unwrap();
        let spec = QuerySpec {
            free_text: Some("damage".into()),
            ..QuerySpec::default()
        };
        let hits = idx.query(&spec).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].mid, "A");
    }

    #[test]
    fn from_chip_narrows_results() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&r("A", "inbox", "KX5DD", "x", "y")).unwrap();
        idx.upsert(&r("B", "inbox", "WX5RES", "x", "y")).unwrap();
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::From, FilterValue::Addr("KX5DD".into()));
        let spec = QuerySpec {
            filters,
            ..QuerySpec::default()
        };
        let hits = idx.query(&spec).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].mid, "A");
    }

    /// tuxlink-g4dj: subject populated on the IndexRow round-trips through
    /// messages_meta and surfaces in QueryHit.subject (not empty).
    #[test]
    fn subject_round_trips_through_messages_meta() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&r("MID-A", "inbox", "KX5DD", "DAMAGE REPORT - SHELBY", "body"))
            .unwrap();
        let hits = idx.query(&QuerySpec::default()).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].mid, "MID-A");
        // Pre-g4dj this assertion failed (hit_to_dto would have surfaced "").
        assert_eq!(hits[0].subject, "DAMAGE REPORT - SHELBY");
    }
}

#[cfg(test)]
mod mutation_tests {
    use super::*;
    use crate::search::extractor::{Direction, IndexRow};
    use tempfile::tempdir;

    fn fixture_row(mid: &str, folder: &str, subject: &str, body: &str) -> IndexRow {
        IndexRow {
            mid: mid.into(), folder: folder.into(),
            subject: subject.into(), body: body.into(), form_field_values: "".into(),
            from_addr: Some("KX5DD".into()), to_addrs: vec!["N7CPZ".into()], cc_addrs: vec![],
            date_sent: None, date_received: Some(1_716_200_000), unread: true,
            form_type: None, has_attachments: false, attachment_count: 0,
            transport_used: Some("telnet".into()), direction: Direction::Received,
            message_size: body.len() as u32, routing_path: None,
        }
    }

    #[test]
    fn upsert_inserts_then_replaces_by_mid() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&fixture_row("MID1", "inbox", "first", "body1")).unwrap();
        assert_eq!(idx.count().unwrap(), 1);
        // replace
        idx.upsert(&fixture_row("MID1", "inbox", "updated", "body2")).unwrap();
        assert_eq!(idx.count().unwrap(), 1);
        let subj: String = idx
            .conn
            .query_row("SELECT subject FROM messages_fts WHERE mid = 'MID1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(subj, "updated");
    }

    #[test]
    fn delete_removes_from_both_tables() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&fixture_row("MID1", "inbox", "x", "y")).unwrap();
        idx.delete("MID1").unwrap();
        assert_eq!(idx.count().unwrap(), 0);
        let fts_n: i64 = idx
            .conn
            .query_row("SELECT COUNT(*) FROM messages_fts WHERE mid = 'MID1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fts_n, 0);
    }

    #[test]
    fn update_folder_changes_folder_in_both_tables() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&fixture_row("MID1", "outbox", "x", "y")).unwrap();
        idx.update_folder("MID1", "sent").unwrap();
        let meta: String = idx
            .conn
            .query_row("SELECT folder FROM messages_meta WHERE mid = 'MID1'", [], |r| r.get(0))
            .unwrap();
        let fts: String = idx
            .conn
            .query_row("SELECT folder FROM messages_fts WHERE mid = 'MID1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(meta, "sent");
        assert_eq!(fts, "sent");
    }

    #[test]
    fn update_unread_flips_the_flag() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&fixture_row("MID1", "inbox", "x", "y")).unwrap();
        idx.update_unread("MID1", false).unwrap();
        let u: i64 = idx
            .conn
            .query_row("SELECT unread FROM messages_meta WHERE mid = 'MID1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(u, 0);
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
            IndexError::SchemaDrift { found: 0, current: SCHEMA_VERSION } => {}
            other => panic!("expected SchemaDrift {{ found: 0, current: {SCHEMA_VERSION} }}, got {other:?}"),
        }
    }
}

// ============================================================================
// tuxlink-hnkn P2 Task 2: query_log_rows unit tests
// ============================================================================
#[cfg(test)]
mod log_query_tests {
    use super::*;
    use crate::search::extractor::{Direction, IndexRow};
    use tempfile::tempdir;

    /// Seed a minimal IndexRow for a SENT message with a given epoch ts.
    fn sent_row(mid: &str, from: &str, to: &str, subject: &str, ts: i64) -> IndexRow {
        IndexRow {
            mid: mid.into(),
            folder: "sent".into(),
            subject: subject.into(),
            body: "".into(),
            form_field_values: "".into(),
            from_addr: Some(from.into()),
            to_addrs: vec![to.into()],
            cc_addrs: vec![],
            date_sent: Some(ts),
            date_received: None,
            unread: false,
            form_type: None,
            has_attachments: false,
            attachment_count: 0,
            transport_used: None,
            direction: Direction::Sent,
            message_size: 10,
            routing_path: None,
        }
    }

    /// Seed a minimal IndexRow for a RECEIVED (inbox) message.
    fn recv_row(mid: &str, from: &str, to: &str, subject: &str, ts: i64) -> IndexRow {
        IndexRow {
            mid: mid.into(),
            folder: "inbox".into(),
            subject: subject.into(),
            body: "".into(),
            form_field_values: "".into(),
            from_addr: Some(from.into()),
            to_addrs: vec![to.into()],
            cc_addrs: vec![],
            date_sent: None,
            date_received: Some(ts),
            unread: true,
            form_type: None,
            has_attachments: false,
            attachment_count: 0,
            transport_used: None,
            direction: Direction::Received,
            message_size: 10,
            routing_path: None,
        }
    }

    // Base epoch: 2024-05-20T10:13:00Z = 1_716_199_980
    const BASE: i64 = 1_716_199_980;

    #[test]
    fn query_log_rows_returns_inbound_and_outbound_in_range() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();

        // Seed: 2 sent + 2 inbox messages, half in-range, half out-of-range.
        //   In-range  [BASE, BASE+3600]:
        idx.upsert(&sent_row("SENT-IN",  "N7CPZ", "W1AW",  "Sent in-range",  BASE + 60)).unwrap();
        idx.upsert(&recv_row("RECV-IN",  "W1AW",  "N7CPZ", "Recv in-range",  BASE + 120)).unwrap();
        //   Out-of-range:
        idx.upsert(&sent_row("SENT-OUT", "N7CPZ", "W1AW",  "Sent out-range", BASE - 100)).unwrap();
        idx.upsert(&recv_row("RECV-OUT", "W1AW",  "N7CPZ", "Recv out-range", BASE + 7200)).unwrap();

        let rows = idx.query_log_rows(BASE, BASE + 3600).unwrap();
        assert_eq!(rows.len(), 2, "only the 2 in-range rows should be returned");

        // Results are ASC by effective timestamp — SENT-IN (BASE+60) before RECV-IN (BASE+120).
        assert_eq!(rows[0].subject, "Sent in-range");
        assert_eq!(rows[1].subject, "Recv in-range");
    }

    #[test]
    fn query_log_rows_direction_discriminator() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&sent_row("S1", "N7CPZ", "W1AW",  "out msg", BASE)).unwrap();
        idx.upsert(&recv_row("R1", "W1AW",  "N7CPZ", "in msg",  BASE + 10)).unwrap();

        let rows = idx.query_log_rows(BASE - 1, BASE + 3600).unwrap();
        assert_eq!(rows.len(), 2);
        // "sent" direction → "out"; "received" direction → "in"
        let sent_row = rows.iter().find(|r| r.subject == "out msg").unwrap();
        let recv_row = rows.iter().find(|r| r.subject == "in msg").unwrap();
        assert_eq!(sent_row.direction, "out");
        assert_eq!(recv_row.direction, "in");
    }

    #[test]
    fn query_log_rows_empty_range_returns_empty() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&sent_row("S1", "N7CPZ", "W1AW", "msg", BASE)).unwrap();
        // Range ends before the row's timestamp.
        let rows = idx.query_log_rows(BASE - 1000, BASE - 1).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn epoch_to_rfc3339_round_trips_known_value() {
        // 1_716_199_980 = 2024-05-20T10:13:00Z (validated against chrono in
        // native_mailbox.rs tests).
        let s = epoch_to_rfc3339(1_716_199_980);
        assert_eq!(s, "2024-05-20T10:13:00Z");
    }
}
