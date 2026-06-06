//! Tauri command handlers. Each one accepts a Tauri `State<SearchService>` (or
//! equivalent) and a serde-friendly DTO. Tests exercise the underlying service
//! methods directly — the Tauri wrapper is a one-line forward.

use crate::search::index::{Index, IndexError};
use crate::search::saved::{SavedSearch, SavedStore, SavedError, RecentSearch};
use crate::search::types::{
    MessageMetaDto, QuerySpec, RebuildStats, SearchResults,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Service struct held in Tauri's managed state. Wraps the Index + SavedStore.
/// `rusqlite::Connection` is `Send` but not `Sync`; the outer `Mutex` on
/// `index` satisfies `T: Sync` required by `tauri::State<T>`. The saved-store
/// `Mutex` guards single-writer access to the JSON file.
pub struct SearchService {
    pub index: Arc<Mutex<Index>>,
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
        let hits = self.index.lock().unwrap().query(&spec)?;
        let items: Vec<MessageMetaDto> = hits.into_iter().map(hit_to_dto).collect();
        let total_matches = items.len() as u32;
        let query_ms = started.elapsed().as_millis() as u32;
        tracing::debug!(
            target: "tuxlink::search",
            total_matches,
            query_ms,
            "search query executed",
        );
        // Recent history is NOT recorded here. Each debounced keystroke would
        // commit (e.g., typing "service" leaves s/se/ser/serv/servi/servic in
        // recent). Frontend calls tauri_search_record_recent on explicit
        // commit (Enter key) only.
        Ok(SearchResults {
            items,
            total_matches,
            query_ms,
            effective_spec: spec,
        })
    }

    pub fn list_saved(&self) -> Vec<SavedSearch> {
        self.saved.lock().unwrap().saved().to_vec()
    }

    pub fn list_recent(&self) -> Vec<RecentSearch> {
        self.saved.lock().unwrap().recent().to_vec()
    }

    /// Record `spec` as a completed search in the recent-history list.
    /// Called from the UI on explicit commit (Enter) — NOT per debounced
    /// query, which would log every keystroke.
    pub fn record_recent(&self, spec: QuerySpec) -> Result<(), CommandError> {
        // Don't record an empty spec (`Enter` on an already-cleared box).
        let empty = spec.free_text.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true)
            && spec.filters.is_empty();
        if empty { return Ok(()); }
        let now = (self.now_unix)();
        self.saved.lock().unwrap().record_recent(spec, now)?;
        Ok(())
    }

    /// Wipe the recent-history list; saved searches are untouched.
    pub fn clear_recent(&self) -> Result<(), CommandError> {
        self.saved.lock().unwrap().clear_recent()?;
        Ok(())
    }

    pub fn save(&self, name: String, spec: QuerySpec) -> Result<SavedSearch, CommandError> {
        let now = (self.now_unix)();
        Ok(self.saved.lock().unwrap().save(&name, spec, now)?)
    }

    pub fn unsave(&self, id: String) -> Result<(), CommandError> {
        Ok(self.saved.lock().unwrap().unsave(&id)?)
    }

    /// Promote a recent search to saved: removes from recent, creates a saved
    /// entry, and returns it — atomically. Prevents the duplicate shown when
    /// `save` is called without removing the matching recent (Codex adrev fix,
    /// find-messages P2).
    pub fn promote_recent(&self, name: String, spec: QuerySpec) -> Result<SavedSearch, CommandError> {
        let now = (self.now_unix)();
        Ok(self.saved.lock().unwrap().promote_recent(&name, &spec, now)?)
    }

    pub fn rename(&self, id: String, name: String) -> Result<(), CommandError> {
        Ok(self.saved.lock().unwrap().rename(&id, &name)?)
    }

    pub fn reorder(&self, ordered_ids: Vec<String>) -> Result<(), CommandError> {
        Ok(self.saved.lock().unwrap().reorder(&ordered_ids)?)
    }

    /// Delete + recreate the search.db, then re-walk every folder of the
    /// supplied mailbox calling `Index::upsert` per message. Returns stats.
    ///
    /// The in-place approach: we lock the Mutex, replace the `Index` inside it
    /// (dropping the old Connection, opening a fresh one against the recreated
    /// file), then walk and upsert into the new index — all under one lock, so
    /// any concurrent reader waits rather than seeing a half-empty index.
    pub fn rebuild_index(&self, mailbox_root: PathBuf) -> Result<RebuildStats, CommandError> {
        use crate::native_mailbox::Mailbox;
        use crate::winlink_backend::MailboxFolder;

        let started = Instant::now();
        tracing::info!(
            target: "tuxlink::search",
            mailbox_root = %mailbox_root.display(),
            "search index rebuild started",
        );

        // Delete existing index files.
        let db = mailbox_root.join("search.db");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(mailbox_root.join("search.db-wal"));
        let _ = std::fs::remove_file(mailbox_root.join("search.db-shm"));

        // Re-open: fresh schema inside the existing Mutex so the SearchService
        // handle stays valid for the runtime.
        let mut locked = self.index.lock().unwrap();
        *locked = Index::open(db).map_err(CommandError::from)?;

        // Re-walk every folder.
        let mbox = Mailbox::new(&mailbox_root);
        let mut count = 0u32;
        for folder in [
            MailboxFolder::Inbox,
            MailboxFolder::Outbox,
            MailboxFolder::Sent,
            MailboxFolder::Archive,
        ] {
            let metas = mbox
                .list(folder)
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            for meta in metas {
                let body = mbox
                    .read(folder, &meta.id)
                    .map_err(|e| CommandError::Internal(e.to_string()))?;
                if let Ok(msg) = crate::winlink::message::Message::from_bytes(&body.raw_rfc5322) {
                    let direction = match folder {
                        MailboxFolder::Sent | MailboxFolder::Outbox => {
                            crate::search::extractor::Direction::Sent
                        }
                        _ => crate::search::extractor::Direction::Received,
                    };
                    let row = crate::search::extractor::extract(
                        &msg,
                        folder,
                        direction,
                        meta.unread,
                        None,
                    );
                    locked.upsert(&row).map_err(CommandError::from)?;
                    count += 1;
                }
            }
        }

        let elapsed_ms = started.elapsed().as_millis() as u32;
        tracing::info!(
            target: "tuxlink::search",
            messages_indexed = count,
            elapsed_ms,
            "search index rebuild completed",
        );
        Ok(RebuildStats {
            messages_indexed: count,
            elapsed_ms,
        })
    }
}

// ── Tauri command wrappers ───────────────────────────────────────────────────

#[tauri::command]
pub fn tauri_search_run(
    svc: tauri::State<SearchService>,
    spec: QuerySpec,
) -> Result<SearchResults, CommandError> {
    svc.run(spec)
}

#[tauri::command]
pub fn tauri_search_list_saved(svc: tauri::State<SearchService>) -> Vec<SavedSearch> {
    svc.list_saved()
}

#[tauri::command]
pub fn tauri_search_list_recent(svc: tauri::State<SearchService>) -> Vec<RecentSearch> {
    svc.list_recent()
}

#[tauri::command]
pub fn tauri_search_save(
    svc: tauri::State<SearchService>,
    name: String,
    spec: QuerySpec,
) -> Result<SavedSearch, CommandError> {
    svc.save(name, spec)
}

#[tauri::command]
pub fn tauri_search_unsave(
    svc: tauri::State<SearchService>,
    id: String,
) -> Result<(), CommandError> {
    svc.unsave(id)
}

#[tauri::command]
pub fn tauri_search_promote_recent(
    svc: tauri::State<SearchService>,
    name: String,
    spec: QuerySpec,
) -> Result<SavedSearch, CommandError> {
    svc.promote_recent(name, spec)
}

#[tauri::command]
pub fn tauri_search_rename(
    svc: tauri::State<SearchService>,
    id: String,
    name: String,
) -> Result<(), CommandError> {
    svc.rename(id, name)
}

#[tauri::command]
pub fn tauri_search_reorder(
    svc: tauri::State<SearchService>,
    ordered_ids: Vec<String>,
) -> Result<(), CommandError> {
    svc.reorder(ordered_ids)
}

#[tauri::command]
pub fn tauri_search_record_recent(
    svc: tauri::State<SearchService>,
    spec: QuerySpec,
) -> Result<(), CommandError> {
    svc.record_recent(spec)
}

#[tauri::command]
pub fn tauri_search_clear_recent(
    svc: tauri::State<SearchService>,
) -> Result<(), CommandError> {
    svc.clear_recent()
}

#[tauri::command]
pub fn tauri_search_rebuild_index(
    svc: tauri::State<SearchService>,
    app: tauri::AppHandle,
) -> Result<RebuildStats, CommandError> {
    use tauri::Manager as _;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| CommandError::Internal(format!("no app_data_dir: {e}")))?;
    svc.rebuild_index(data_dir.join("native-mbox"))
}

fn hit_to_dto(h: crate::search::index::QueryHit) -> MessageMetaDto {
    MessageMetaDto {
        id: h.mid,
        // subject is stored in messages_meta as of SCHEMA_VERSION=2
        // (tuxlink-g4dj). Pre-v2 indices fail Index::open with SchemaDrift
        // and the operator runs tauri_search_rebuild_index to recreate.
        subject: h.subject,
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
        let index = Arc::new(Mutex::new(Index::open(dir.join("search.db")).unwrap()));
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
    fn run_returns_results_and_does_not_record_recent() {
        // Bug fix follow-up: run() must NOT record_recent on every call,
        // because the UI debounces and calls run on every keystroke pause —
        // that polluted history with "s", "se", "ser", ... entries.
        let dir = tempdir().unwrap();
        let svc = build_service(dir.path());
        svc.index.lock().unwrap().upsert(&fixture_row("A", "damage report", "powerlines down")).unwrap();
        let spec = QuerySpec { free_text: Some("damage".into()), ..QuerySpec::default() };
        let res = svc.run(spec.clone()).unwrap();
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.items[0].id, "A");
        // run() must not record anything in recent — that's record_recent's job
        assert_eq!(svc.list_recent().len(), 0);
    }

    #[test]
    fn record_recent_commits_explicitly() {
        let dir = tempdir().unwrap();
        let svc = build_service(dir.path());
        let spec = QuerySpec { free_text: Some("damage".into()), ..QuerySpec::default() };
        svc.record_recent(spec.clone()).unwrap();
        let rec = svc.list_recent();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].spec, spec);
    }

    #[test]
    fn record_recent_ignores_empty_spec() {
        let dir = tempdir().unwrap();
        let svc = build_service(dir.path());
        // empty free_text, no filters → no-op (Enter on a cleared box)
        svc.record_recent(QuerySpec::default()).unwrap();
        svc.record_recent(QuerySpec { free_text: Some("   ".into()), ..QuerySpec::default() }).unwrap();
        assert_eq!(svc.list_recent().len(), 0);
    }

    #[test]
    fn record_recent_dedupes_existing_spec() {
        let dir = tempdir().unwrap();
        let svc = build_service(dir.path());
        let spec = QuerySpec { free_text: Some("damage".into()), ..QuerySpec::default() };
        svc.record_recent(spec.clone()).unwrap();
        svc.record_recent(spec.clone()).unwrap();
        // committing the same query twice should leave one entry (the new one),
        // not two
        assert_eq!(svc.list_recent().len(), 1);
    }

    #[test]
    fn clear_recent_empties_history_without_touching_saved() {
        let dir = tempdir().unwrap();
        let svc = build_service(dir.path());
        svc.save("Keep me".into(), QuerySpec { free_text: Some("net".into()), ..QuerySpec::default() }).unwrap();
        svc.record_recent(QuerySpec { free_text: Some("temp".into()), ..QuerySpec::default() }).unwrap();
        assert_eq!(svc.list_recent().len(), 1);
        assert_eq!(svc.list_saved().len(), 1);
        svc.clear_recent().unwrap();
        assert_eq!(svc.list_recent().len(), 0);
        assert_eq!(svc.list_saved().len(), 1, "saved must not be touched");
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

#[cfg(test)]
mod rebuild_tests {
    use super::*;
    use crate::native_mailbox::Mailbox;
    use crate::winlink_backend::MailboxFolder;
    use tempfile::tempdir;

    /// Each call uses a distinct unix_secs so `generate_mid` produces unique MIDs.
    fn raw(subject: &str, body: &str, unix_secs: u64) -> Vec<u8> {
        crate::winlink::compose::compose_message(
            "N7CPZ",
            &["W1AW"],
            &[],
            subject,
            body,
            unix_secs,
        )
        .to_bytes()
    }

    fn build_service_for_rebuild(dir: &std::path::Path) -> SearchService {
        crate::search::build_service(dir).expect("build_service")
    }

    #[test]
    fn rebuild_picks_up_messages_already_on_disk() {
        let dir = tempdir().unwrap();
        // Store 3 messages WITHOUT an index attached.
        // Use distinct timestamps so generate_mid yields 3 unique MIDs
        // (Mid = callsign + unix_secs — same timestamp → same Mid → upsert overwrites).
        {
            let mbox = Mailbox::new(dir.path());
            mbox.store(MailboxFolder::Inbox, &raw("a", "x", 1_716_200_001)).unwrap();
            mbox.store(MailboxFolder::Inbox, &raw("b", "y", 1_716_200_002)).unwrap();
            mbox.store(MailboxFolder::Sent, &raw("c", "z", 1_716_200_003)).unwrap();
        }

        let svc = build_service_for_rebuild(dir.path());
        let stats = svc
            .rebuild_index(dir.path().to_path_buf())
            .unwrap();
        assert_eq!(stats.messages_indexed, 3);
        assert_eq!(svc.index.lock().unwrap().count().unwrap(), 3);
    }
}

/// User-guide search command (tuxlink-0gsy / spec §9.3). Frontend
/// (useHelpSearch) debounces; this command is a thin forward to the
/// underlying Index::search_docs path.
#[tauri::command]
pub fn docs_search(
    svc: tauri::State<SearchService>,
    query: String,
) -> Result<Vec<crate::search::docs_index::DocsHit>, String> {
    svc.index
        .lock()
        .unwrap()
        .search_docs(&query)
        .map_err(|e| e.to_string())
}
