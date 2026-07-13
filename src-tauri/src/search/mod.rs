//! v0.1 find-messages: FTS5-indexed search over the filesystem-canonical
//! mailbox. The mailbox is canonical (see `native_mailbox.rs`); this module
//! maintains a derived `search.db` regenerable from disk via the
//! `rebuild-index` command.
//!
//! See `docs/design/2026-05-30-find-messages-design.md` for the full design.

pub mod commands;
pub mod docs_bundle;   // tuxlink-0gsy (spec §9)
pub mod docs_index;    // tuxlink-0gsy (spec §9)
pub mod extractor;
pub mod index;
pub mod query;
pub mod saved;
pub mod types;

#[cfg(test)]
mod docs_registry_test;
#[cfg(test)]
mod docs_eval_test;

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::search::commands::{CommandError, SearchService};
use crate::search::index::{Index, IndexError};
use crate::search::saved::SavedStore;

/// Build a `SearchService` rooted at the given data directory.
///
/// - Index: `<data_dir>/search.db`
/// - Saved searches: `<data_dir>/saved-searches.json`
///
/// Called once at app startup via the `.setup()` hook; the resulting
/// `SearchService` is registered as managed state via `.manage(...)`.
///
/// **Schema drift recovery (tuxlink-15mm).** If `search.db` exists at a stale
/// `user_version`, `Index::open` returns `IndexError::SchemaDrift`. The index
/// is regenerable from the mailbox source, so we delete the stale .db (+ its
/// WAL/SHM siblings) and reopen with the current schema. Without this the
/// `SearchService` was never installed into Tauri's managed state, which
/// silently wedged both search AND the rebuild-index command that would have
/// recovered (catch-22: rebuild needed `State<SearchService>` too). The index
/// is empty after recovery; the operator clicks Rebuild Index from the UI to
/// repopulate from the mbox source.
pub fn build_service(data_dir: &Path) -> Result<SearchService, CommandError> {
    let db_path = data_dir.join("search.db");
    let index = match Index::open(db_path.clone()) {
        Ok(idx) => idx,
        Err(IndexError::SchemaDrift { found, current }) => {
            eprintln!(
                "search: schema drift v{found} → v{current} at {}, recreating empty index \
                 (operator should run Rebuild Index to repopulate from mbox)",
                db_path.display()
            );
            let _ = std::fs::remove_file(&db_path);
            let _ = std::fs::remove_file(data_dir.join("search.db-wal"));
            let _ = std::fs::remove_file(data_dir.join("search.db-shm"));
            Index::open(db_path).map_err(CommandError::from)?
        }
        Err(other) => return Err(other.into()),
    };
    // Repopulate the docs_fts table whenever the bundled topic set drifts
    // from what's in the existing index. The previous "populate only when
    // empty" gate produced a stale-slug regression after PR #347: the
    // operator's existing index was non-empty with pre-IA-restructure
    // slugs (e.g. `01-getting-started`); new builds skipped repopulation
    // and search returned dead slugs that no TOPICS entry matched. The
    // sidebar appeared to work (the hits rendered) but every click fell
    // through to `getTopicBySlug(deadSlug)` → undefined → TOPICS[0].
    //
    // Drift check: compare the slug set in the index against the slug set
    // bundled into this binary. Mismatch → DELETE + INSERT. Identical →
    // no-op. The compare cost is one query + a hash-set diff; in the
    // mismatch path the wipe+repopulate is ~32 small inserts inside one
    // transaction. Either way the cost is sub-millisecond at startup.
    let bundled_slugs: std::collections::HashSet<&str> =
        crate::search::docs_bundle::BUNDLED_TOPICS
            .iter()
            .map(|t| t.slug)
            .collect();
    let indexed_slugs = index.docs_slugs().map_err(CommandError::from)?;
    let indexed_slugs_set: std::collections::HashSet<&str> =
        indexed_slugs.iter().map(String::as_str).collect();
    if bundled_slugs != indexed_slugs_set {
        index
            .populate_docs(crate::search::docs_bundle::BUNDLED_TOPICS)
            .map_err(CommandError::from)?;
    }

    let saved = Mutex::new(
        SavedStore::open(data_dir.join("saved-searches.json")).map_err(CommandError::from)?,
    );
    Ok(SearchService {
        index: Arc::new(Mutex::new(index)),
        saved,
        now_unix: || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        },
    })
}

#[cfg(test)]
mod module_smoke {
    use super::*;
    use rusqlite::Connection;
    use tempfile::tempdir;

    #[test]
    fn module_is_wired() {
        assert_eq!(2 + 2, 4);
    }

    /// Regression for tuxlink-15mm: an on-disk search.db at a stale
    /// user_version used to wedge `build_service` (and therefore the entire
    /// SearchService managed state) with `IndexError::SchemaDrift`. The
    /// recovery path must delete the stale file and return a built service so
    /// the rebuild-index command can be invoked.
    #[test]
    fn build_service_recovers_from_schema_drift() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("search.db");
        // Plant a pre-existing .db at user_version=1 (v1 ↔ pre-tuxlink-g4dj).
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.pragma_update(None, "user_version", 1u32).unwrap();
        }
        // Sanity: Index::open alone fails with SchemaDrift on this file.
        assert!(matches!(
            Index::open(db_path.clone()).unwrap_err(),
            IndexError::SchemaDrift { found: 1, .. }
        ));
        // build_service must paper over the drift and return a working service.
        let svc = build_service(dir.path()).expect("build_service after drift");
        // Service is queryable (empty index, no error).
        let count = svc.index.lock().unwrap().count().unwrap();
        assert_eq!(count, 0);
    }

    /// Regression for tuxlink-15mm: after recovery, the WAL/SHM siblings of the
    /// stale .db are no longer holding a stale schema. Plant all three; the
    /// recovery must remove them so the fresh schema doesn't get poisoned by a
    /// crash-recovery replay against the old WAL.
    #[test]
    fn build_service_clears_wal_shm_siblings_on_drift() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("search.db");
        let wal_path = dir.path().join("search.db-wal");
        let shm_path = dir.path().join("search.db-shm");
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.pragma_update(None, "user_version", 1u32).unwrap();
        }
        // Plant placeholder WAL + SHM files alongside.
        std::fs::write(&wal_path, b"stale-wal").unwrap();
        std::fs::write(&shm_path, b"stale-shm").unwrap();

        let _svc = build_service(dir.path()).expect("build_service after drift");

        // The stale WAL + SHM placeholders are gone (we wrote text bytes; if
        // they still existed we'd see them on disk regardless of SQLite mode).
        assert!(!wal_path.exists(), "WAL sibling must be removed on recovery");
        assert!(!shm_path.exists(), "SHM sibling must be removed on recovery");
    }
}
