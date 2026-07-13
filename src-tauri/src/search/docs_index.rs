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

/// A whole document, returned by `read_doc`. This is the payload `docs_search`'s
/// 12-token `snippet()` cannot carry.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocBody {
    pub slug: String,
    pub title: String,
    pub body: String,
}

/// Which corpus a topic came from. All three are indexed into `docs_fts` and are
/// searchable/readable by the agent; only `UserGuide` also renders in the in-app
/// Help sidebar (which discovers files itself via `import.meta.glob` in
/// `src/help/topics.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocSource {
    /// `docs/user-guide/` — Tuxlink's operator manual. Also in the Help sidebar.
    UserGuide,
    /// `docs/knowledge/` — agent-only reference about OTHER Winlink clients.
    Knowledge,
    /// `docs/mcp-knowledge/` — playbooks and specs, also served as MCP resources.
    McpKnowledge,
}

/// A bundled topic, supplied by the caller (the frontend has a typed
/// registry; the Rust side accepts the tuple at populate time).
#[derive(Debug, Clone)]
pub struct DocTopic<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub markdown: &'a str,
    pub source: DocSource,
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

    /// Return the full indexed body for `slug`, or `None` when the slug is not
    /// in `docs_fts`.
    ///
    /// The body column already holds the whole extracted document — `search_docs`
    /// simply never exposed more than a `snippet()` of it. This is the read half
    /// of the search-then-read pair (P0 tuxlink-0mudm).
    pub fn read_doc(&self, slug: &str) -> Result<Option<DocBody>, IndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT slug, title, body FROM docs_fts WHERE slug = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query_map([slug], |row| {
            Ok(DocBody {
                slug: row.get(0)?,
                title: row.get(1)?,
                body: row.get(2)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
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
            DocTopic { slug: "01-getting-started", title: "Getting started", markdown: "# Getting started\nWelcome to Tuxlink.", source: DocSource::UserGuide },
            DocTopic { slug: "02-connections", title: "Connections", markdown: "# Connections\nARDOP is HF digital.", source: DocSource::UserGuide },
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
            DocTopic { slug: "01", title: "x", markdown: "anything", source: DocSource::UserGuide },
        ]).unwrap();
        assert!(idx.search_docs("").unwrap().is_empty());
        assert!(idx.search_docs("   ").unwrap().is_empty());
    }

    #[test]
    fn populate_replaces_previous_content() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "old", title: "Old", markdown: "ARDOP digital", source: DocSource::UserGuide },
        ]).unwrap();
        idx.populate_docs(&[
            DocTopic { slug: "new", title: "New", markdown: "VARA digital", source: DocSource::UserGuide },
        ]).unwrap();
        assert!(idx.search_docs("ARDOP").unwrap().is_empty());
        assert_eq!(idx.search_docs("VARA").unwrap().len(), 1);
    }

    #[test]
    fn docs_slugs_returns_the_indexed_slug_set() {
        let (_dir, idx) = fresh();
        assert!(idx.docs_slugs().unwrap().is_empty());
        idx.populate_docs(&[
            DocTopic { slug: "01-foo", title: "Foo", markdown: "x", source: DocSource::UserGuide },
            DocTopic { slug: "02-bar", title: "Bar", markdown: "y", source: DocSource::UserGuide },
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
            DocTopic { slug: "01-getting-started", title: "Getting started", markdown: "ARDOP", source: DocSource::UserGuide },
        ]).unwrap();
        assert_eq!(idx.docs_slugs().unwrap(), vec!["01-getting-started".to_string()]);
        idx.populate_docs(&[
            DocTopic { slug: "01-what-is-tuxlink", title: "What is Tuxlink", markdown: "ARDOP", source: DocSource::UserGuide },
        ]).unwrap();
        assert_eq!(idx.docs_slugs().unwrap(), vec!["01-what-is-tuxlink".to_string()]);
    }

    #[test]
    fn read_doc_returns_full_body_not_a_snippet() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[DocTopic {
            slug: "pat-winlink",
            title: "Pat Winlink",
            markdown: "# Pat Winlink\nConnect via a digipeater: ax25:///DIGI/TARGET is the form.",
            source: DocSource::Knowledge,
        }])
        .unwrap();

        let doc = idx.read_doc("pat-winlink").unwrap().expect("slug is present");
        assert_eq!(doc.slug, "pat-winlink");
        assert_eq!(doc.title, "Pat Winlink");
        // The whole body, not a 12-token snippet() window.
        assert!(doc.body.contains("ax25:///DIGI/TARGET"));
        assert!(doc.body.contains("Connect via a digipeater"));
        assert!(!doc.body.contains("<mark>"));
    }

    #[test]
    fn read_doc_unknown_slug_returns_none() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[DocTopic {
            slug: "01-foo",
            title: "Foo",
            markdown: "x",
            source: DocSource::UserGuide,
        }])
        .unwrap();
        assert!(idx.read_doc("no-such-slug").unwrap().is_none());
    }

}
