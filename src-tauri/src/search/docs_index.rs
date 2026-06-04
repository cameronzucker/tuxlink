//! Docs-side FTS5 surface (tuxlink-0gsy / spec §9).
//!
//! Owns the `docs_fts` virtual table created in `index.rs::init_schema`,
//! populates it once at first launch from the bundled user-guide markdown,
//! and exposes a `search_docs(query) -> Vec<DocsHit>` query.

use crate::search::extractor::extract_markdown;
use crate::search::index::{Index, IndexError};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocsHit {
    pub slug: String,
    pub title: String,
    pub snippet: String,  // FTS5 snippet() output, may contain <mark>...</mark>
}

/// A bundled topic, supplied by the caller (the frontend has a typed
/// registry; the Rust side accepts the trio at populate time).
#[derive(Debug, Clone)]
pub struct DocTopic<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub markdown: &'a str,
}

impl Index {
    /// Return true if `docs_fts` is empty.
    pub fn docs_is_empty(&self) -> Result<bool, IndexError> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM docs_fts",
            [],
            |row| row.get(0),
        )?;
        Ok(count == 0)
    }

    /// Return the set of slugs currently in `docs_fts`. The caller compares
    /// this against the bundled-topics slug set to decide whether the index
    /// needs to be repopulated after a docs-bundle change (e.g. the PR #347
    /// IA restructure renamed every slug; the old slugs would otherwise
    /// stay in the index and produce dead search hits).
    pub fn docs_slugs(&self) -> Result<Vec<String>, IndexError> {
        let mut stmt = self.conn.prepare("SELECT slug FROM docs_fts")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(IndexError::from)
    }

    /// Populate `docs_fts` from `topics`. Wipes the table first so re-calls
    /// (e.g. after a schema-drift recovery) start from a clean state.
    pub fn populate_docs(&self, topics: &[DocTopic<'_>]) -> Result<(), IndexError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM docs_fts", [])?;
        for t in topics {
            let body_text = extract_markdown(t.markdown);
            tx.execute(
                "INSERT INTO docs_fts (slug, title, body) VALUES (?1, ?2, ?3)",
                rusqlite::params![t.slug, t.title, body_text],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Run a free-text query against `docs_fts`. Returns hits ordered by
    /// BM25 rank (best first) with FTS5 snippet() output for the matching
    /// body fragment.
    ///
    /// The `query` is passed through to FTS5 MATCH unchanged after rejecting
    /// the empty string. Operators get FTS5's column-scoping and prefix
    /// syntax for free.
    pub fn search_docs(&self, query: &str) -> Result<Vec<DocsHit>, IndexError> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let mut stmt = self.conn.prepare(
            "SELECT slug, title, snippet(docs_fts, 2, '<mark>', '</mark>', '…', 12) \
             FROM docs_fts \
             WHERE docs_fts MATCH ?1 \
             ORDER BY rank \
             LIMIT 30",
        )?;
        let rows = stmt.query_map([query], |row| {
            Ok(DocsHit {
                slug: row.get(0)?,
                title: row.get(1)?,
                snippet: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(IndexError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{tempdir, TempDir};

    /// Returns `(TempDir, Index)` — the TempDir handle MUST be kept alive
    /// for the duration of the test. Dropping it before the Index would
    /// unlink the underlying file, and SQLite would then fail subsequent
    /// writes with `SQLITE_READONLY_DBMOVED` (the actual symptom that
    /// surfaced when this helper originally returned just the Index).
    fn fresh() -> (TempDir, Index) {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        (dir, idx)
    }

    #[test]
    fn docs_is_empty_on_fresh_index() {
        let (_dir, idx) = fresh();
        assert!(idx.docs_is_empty().unwrap());
    }

    #[test]
    fn populate_then_search_returns_hits() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "01-getting-started", title: "Getting started", markdown: "# Getting started\nWelcome to Tuxlink." },
            DocTopic { slug: "02-connections", title: "Connections", markdown: "# Connections\nARDOP is HF digital." },
        ]).unwrap();

        assert!(!idx.docs_is_empty().unwrap());
        let hits = idx.search_docs("ARDOP").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slug, "02-connections");
        assert!(hits[0].snippet.contains("ARDOP"));
    }

    #[test]
    fn empty_query_returns_no_hits() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "01", title: "x", markdown: "anything" },
        ]).unwrap();
        assert!(idx.search_docs("").unwrap().is_empty());
        assert!(idx.search_docs("   ").unwrap().is_empty());
    }

    #[test]
    fn populate_replaces_previous_content() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "old", title: "Old", markdown: "ARDOP digital" },
        ]).unwrap();
        idx.populate_docs(&[
            DocTopic { slug: "new", title: "New", markdown: "VARA digital" },
        ]).unwrap();
        assert!(idx.search_docs("ARDOP").unwrap().is_empty());
        assert_eq!(idx.search_docs("VARA").unwrap().len(), 1);
    }

    #[test]
    fn docs_slugs_returns_the_indexed_slug_set() {
        let (_dir, idx) = fresh();
        assert!(idx.docs_slugs().unwrap().is_empty());
        idx.populate_docs(&[
            DocTopic { slug: "01-foo", title: "Foo", markdown: "x" },
            DocTopic { slug: "02-bar", title: "Bar", markdown: "y" },
        ]).unwrap();
        let mut slugs = idx.docs_slugs().unwrap();
        slugs.sort();
        assert_eq!(slugs, vec!["01-foo".to_string(), "02-bar".to_string()]);
    }

    #[test]
    fn docs_slugs_reflects_repopulation_with_renamed_slugs() {
        // Regression for the PR #347 search-stale-slug failure: an existing
        // populated index gets repopulated with the new bundle's slugs, and
        // a subsequent search returns ONLY the new slugs.
        let (_dir, idx) = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "01-getting-started", title: "Getting started", markdown: "ARDOP" },
        ]).unwrap();
        assert_eq!(idx.docs_slugs().unwrap(), vec!["01-getting-started".to_string()]);
        idx.populate_docs(&[
            DocTopic { slug: "01-what-is-tuxlink", title: "What is Tuxlink", markdown: "ARDOP" },
        ]).unwrap();
        assert_eq!(idx.docs_slugs().unwrap(), vec!["01-what-is-tuxlink".to_string()]);
    }
}
