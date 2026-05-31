//! Tauri command handlers. Each one accepts a Tauri `State<SearchService>` (or
//! equivalent) and a serde-friendly DTO. Tests exercise the underlying service
//! methods directly — the Tauri wrapper is a one-line forward.

use crate::search::index::{Index, IndexError};
use crate::search::saved::{SavedSearch, SavedStore, SavedError, RecentSearch};
use crate::search::types::{
    MessageMetaDto, QuerySpec, SearchResults,
};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Service struct held in Tauri's managed state. Wraps the Index + SavedStore.
/// The `Mutex` guards single-writer access to the JSON saved-store (the index
/// is internally synchronized by SQLite).
pub struct SearchService {
    pub index: Arc<Index>,
    pub saved: Mutex<SavedStore>,
    pub now_unix: fn() -> i64,
}

#[derive(thiserror::Error, Debug, serde::Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "PascalCase")]
pub enum CommandError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl From<IndexError> for CommandError {
    fn from(e: IndexError) -> Self {
        match e {
            IndexError::SchemaDrift { .. } => CommandError::Internal(e.to_string()),
            IndexError::Sqlite(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("fts5") => {
                CommandError::InvalidQuery(msg)
            }
            other => CommandError::Internal(other.to_string()),
        }
    }
}

impl From<SavedError> for CommandError {
    fn from(e: SavedError) -> Self { CommandError::Internal(e.to_string()) }
}

impl SearchService {
    pub fn run(&self, spec: QuerySpec) -> Result<SearchResults, CommandError> {
        let started = Instant::now();
        let hits = self.index.query(&spec)?;
        let items: Vec<MessageMetaDto> = hits.into_iter().map(hit_to_dto).collect();
        let total_matches = items.len() as u32;
        let now = (self.now_unix)();
        self.saved.lock().unwrap().record_recent(spec.clone(), now)?;
        Ok(SearchResults {
            items,
            total_matches,
            query_ms: started.elapsed().as_millis() as u32,
            effective_spec: spec,
        })
    }

    pub fn list_saved(&self) -> Vec<SavedSearch> {
        self.saved.lock().unwrap().saved().to_vec()
    }

    pub fn list_recent(&self) -> Vec<RecentSearch> {
        self.saved.lock().unwrap().recent().to_vec()
    }

    pub fn save(&self, name: String, spec: QuerySpec) -> Result<SavedSearch, CommandError> {
        let now = (self.now_unix)();
        Ok(self.saved.lock().unwrap().save(&name, spec, now)?)
    }

    pub fn unsave(&self, id: String) -> Result<(), CommandError> {
        Ok(self.saved.lock().unwrap().unsave(&id)?)
    }

    pub fn rename(&self, id: String, name: String) -> Result<(), CommandError> {
        Ok(self.saved.lock().unwrap().rename(&id, &name)?)
    }

    pub fn reorder(&self, ordered_ids: Vec<String>) -> Result<(), CommandError> {
        Ok(self.saved.lock().unwrap().reorder(&ordered_ids)?)
    }
}

fn hit_to_dto(h: crate::search::index::QueryHit) -> MessageMetaDto {
    MessageMetaDto {
        id: h.mid,
        // subject not stored in messages_meta in v0.1 (plan gap, see bd issue);
        // search results render with empty subject until backfilled.
        subject: String::new(),
        from: h.from_addr.unwrap_or_default(),
        to: h.to_addrs,
        date: unix_to_rfc3339(h.date_received.or(h.date_sent).unwrap_or(0)),
        unread: h.unread,
        body_size: h.message_size,
        has_attachments: h.has_attachments,
        form_tag: h.form_type,
        folder: h.folder,
    }
}

fn unix_to_rfc3339(unix: i64) -> String {
    civil_from_days_and_seconds(unix)
}

fn civil_from_days_and_seconds(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let seconds_of_day = unix.rem_euclid(86_400) as u32;
    let (y, m, d) = civil_from_days(days as i32);
    let h = seconds_of_day / 3600;
    let mi = (seconds_of_day % 3600) / 60;
    let s = seconds_of_day % 60;
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn civil_from_days(z: i32) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::extractor::{Direction, IndexRow};
    use tempfile::tempdir;

    fn fixed_now() -> i64 { 1_716_200_000 }

    fn build_service(dir: &std::path::Path) -> SearchService {
        let index = Arc::new(Index::open(dir.join("search.db")).unwrap());
        let saved = Mutex::new(SavedStore::open(dir.join("saved.json")).unwrap());
        SearchService { index, saved, now_unix: fixed_now }
    }

    fn fixture_row(mid: &str, subject: &str, body: &str) -> IndexRow {
        IndexRow {
            mid: mid.into(), folder: "inbox".into(),
            subject: subject.into(), body: body.into(), form_field_values: "".into(),
            from_addr: Some("KX5DD".into()), to_addrs: vec!["N7CPZ".into()], cc_addrs: vec![],
            date_sent: None, date_received: Some(1_716_200_000),
            unread: true, form_type: None, has_attachments: false, attachment_count: 0,
            transport_used: Some("telnet".into()), direction: Direction::Received,
            message_size: body.len() as u32, routing_path: None,
        }
    }

    #[test]
    fn run_returns_results_and_records_recent() {
        let dir = tempdir().unwrap();
        let svc = build_service(dir.path());
        svc.index.upsert(&fixture_row("A", "damage report", "powerlines down")).unwrap();
        let spec = QuerySpec { free_text: Some("damage".into()), ..QuerySpec::default() };
        let res = svc.run(spec.clone()).unwrap();
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.items[0].id, "A");
        // recent was recorded
        let rec = svc.list_recent();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].spec, spec);
    }

    #[test]
    fn save_then_list_saved_returns_the_entry() {
        let dir = tempdir().unwrap();
        let svc = build_service(dir.path());
        let s = svc.save("Storm Net".into(), QuerySpec::default()).unwrap();
        let listed = svc.list_saved();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, s.id);
    }

    #[test]
    fn civil_roundtrip_unix_to_rfc3339() {
        assert_eq!(unix_to_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(unix_to_rfc3339(1_716_200_000), "2024-05-20T10:13:20Z");
    }
}
