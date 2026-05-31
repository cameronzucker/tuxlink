//! v0.1 find-messages: FTS5-indexed search over the filesystem-canonical
//! mailbox. The mailbox is canonical (see `native_mailbox.rs`); this module
//! maintains a derived `search.db` regenerable from disk via the
//! `rebuild-index` command.
//!
//! See `docs/design/2026-05-30-find-messages-design.md` for the full design.

pub mod commands;
pub mod extractor;
pub mod index;
pub mod query;
pub mod saved;
pub mod types;

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::search::commands::{CommandError, SearchService};
use crate::search::index::Index;
use crate::search::saved::SavedStore;

/// Build a `SearchService` rooted at the given data directory.
///
/// - Index: `<data_dir>/search.db`
/// - Saved searches: `<data_dir>/saved-searches.json`
///
/// Called once at app startup via the `.setup()` hook; the resulting
/// `SearchService` is registered as managed state via `.manage(...)`.
pub fn build_service(data_dir: &Path) -> Result<SearchService, CommandError> {
    let index = Arc::new(Mutex::new(
        Index::open(data_dir.join("search.db")).map_err(CommandError::from)?,
    ));
    let saved = Mutex::new(
        SavedStore::open(data_dir.join("saved-searches.json")).map_err(CommandError::from)?,
    );
    Ok(SearchService {
        index,
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
    #[test]
    fn module_is_wired() {
        assert_eq!(2 + 2, 4);
    }
}
