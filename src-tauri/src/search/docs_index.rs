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

/// Reduce free text to a safe FTS5 expression: bare alphanumeric tokens, each
/// quoted as an FTS5 string literal, joined by `OR`.
///
/// Quoting is what makes this safe. Inside `"..."` FTS5 treats the content as a
/// literal term, so a token that would otherwise be an operator (`AND`, `OR`,
/// `NOT`, `NEAR`) or a column reference is inert. Splitting on non-alphanumerics
/// means `-`, `.`, `?`, `*`, `:` and `"` never reach the parser at all — so
/// `pat-winlink` becomes `"pat" OR "winlink"` instead of an error, and "ax.25"
/// becomes `"ax" OR "25"`.
///
/// Single-character tokens are dropped. They carry no retrieval signal and, ORed
/// into the expression, actively hurt: the `t` left behind by "won't" and the `I`
/// from "I'm" match a huge fraction of the corpus and drag unrelated documents up
/// the BM25 ranking. Measured against the real 48-document corpus, dropping them
/// moves the ARDOP playbook from rank 24 to rank 5 for "ARDOP won't connect", with
/// no regression on the other evals. Two-character tokens are kept — "25" from
/// "ax.25" is meaningful.
///
/// Returns `None` when the input holds no usable token at all.
fn fts5_or_query(query: &str) -> Option<String> {
    let tokens: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 2)
        .map(|t| format!("\"{}\"", t.to_lowercase()))
        .collect();
    if tokens.is_empty() {
        return None;
    }
    Some(tokens.join(" OR "))
}

/// Fingerprint of the topic bundle compiled into this binary, computed exactly
/// the way [`Index::docs_content_fingerprint`] computes the index's — same
/// fields, same order, and crucially the same BODY representation.
///
/// `populate_docs` stores `extract_markdown(t.markdown)`, not the raw file, so
/// this must extract too. Hashing the raw markdown here would mismatch the index
/// on every single startup and repopulate forever — a silent, permanent
/// write-on-every-launch.
pub fn bundled_docs_fingerprint(topics: &[DocTopic<'_>]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut rows: Vec<(String, String, String)> = topics
        .iter()
        .map(|t| {
            (
                t.slug.to_string(),
                t.title.to_string(),
                extract_markdown(t.markdown),
            )
        })
        .collect();
    rows.sort();

    let mut h = DefaultHasher::new();
    for (slug, title, body) in &rows {
        slug.hash(&mut h);
        title.hash(&mut h);
        body.hash(&mut h);
    }
    h.finish()
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

    /// Fingerprint of what is actually IN `docs_fts` right now: every row's
    /// slug, title and body, in slug order.
    ///
    /// Compare against [`bundled_docs_fingerprint`] to decide whether the index
    /// needs repopulating. Comparing slug SETS — which is what this used to do —
    /// only notices topics being added, renamed or removed. It does not notice a
    /// topic's BODY changing, so a correction to an existing page shipped inside
    /// the new binary and the operator's `docs_fts` kept serving the old text
    /// forever.
    ///
    /// That was survivable when the only thing exposed was a 12-token snippet.
    /// It is not now: `docs_read` hands whole documents to the model as ground
    /// truth, so a stale body means Elmer keeps quoting a connect string we have
    /// already fixed — on air.
    ///
    /// Deliberately NOT a stored fingerprint. A stored value is a third piece of
    /// state that can itself drift and would need migrating; comparing the two
    /// live sources (the index vs the bundle compiled into this binary) is
    /// self-validating. Cost is one scan of ~50 short documents at startup.
    pub fn docs_content_fingerprint(&self) -> Result<u64, IndexError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut rows: Vec<(String, String, String)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT slug, title, body FROM docs_fts")?;
            let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        // FTS5 has no inherent row order; sort so the fingerprint is stable.
        rows.sort();

        let mut h = DefaultHasher::new();
        for (slug, title, body) in &rows {
            slug.hash(&mut h);
            title.hash(&mut h);
            body.hash(&mut h);
        }
        Ok(h.finish())
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
    ///
    /// Natural-language queries are handled by a fallback, because FTS5's MATCH
    /// argument is a QUERY LANGUAGE, not a search string. Passing a question
    /// through raw fails two ways:
    ///
    /// - **Syntax errors.** `-`, `.`, `?`, `"`, `*`, `:` and the bare words
    ///   `AND`/`OR`/`NOT`/`NEAR` are all operators. "How do I connect?" is a
    ///   syntax error near `?`; the KJ4UYO question is a syntax error near `.`
    ///   (from "ax.25"); even passing a slug like `pat-winlink` errors, because
    ///   `-` means NOT.
    /// - **Implicit AND.** Bare terms are ANDed, so one absent word returns zero
    ///   rows — indistinguishable, to a caller, from "no documentation on this".
    ///
    /// Both matter more than they look: the callers are the in-app help search box
    /// (a human typing a question) and the `docs_search` MCP tool (a small model
    /// handing over the operator's question verbatim). A model that gets an error
    /// or an empty list has nothing to ground on, and fabricates — which is the
    /// exact failure this whole retrieval path exists to prevent (P0 tuxlink-0mudm).
    ///
    /// So: try the query as written first, preserving FTS5 syntax for anyone who
    /// means it. If that errors OR returns nothing, retry with the query reduced to
    /// bare tokens joined by OR. BM25 then ranks documents matching more of the
    /// distinctive terms first, which is what a natural-language query wants. The
    /// fallback can only add results, never remove them.
    pub fn search_docs(&self, query: &str) -> Result<Vec<DocsHit>, IndexError> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        // Attempt 1: the query as written. A deliberate FTS5 expression works here.
        match self.match_docs(query) {
            Ok(hits) if !hits.is_empty() => return Ok(hits),
            Ok(_) => {}  // parsed, but matched nothing — fall through and broaden
            Err(_) => {} // not a valid FTS5 expression — almost certainly prose
        }
        // Attempt 2: treat it as prose.
        match fts5_or_query(query) {
            Some(relaxed) => self.match_docs(&relaxed),
            // Nothing tokenizable (e.g. "???") — no hits rather than an error.
            None => Ok(vec![]),
        }
    }

    /// Run one `MATCH` against `docs_fts`. `expr` must already be valid FTS5.
    fn match_docs(&self, expr: &str) -> Result<Vec<DocsHit>, IndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT slug, title, snippet(docs_fts, 2, '<mark>', '</mark>', '…', 12) \
             FROM docs_fts \
             WHERE docs_fts MATCH ?1 \
             ORDER BY rank \
             LIMIT 30",
        )?;
        let rows = stmt.query_map([expr], |row| {
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

    /// The whole point of tuxlink-cr0wz: an edit to a doc's BODY must trigger a
    /// repopulate. The old slug-set comparison could not see this — same slugs,
    /// different text — so a corrected page never reached an existing install.
    #[test]
    fn content_fingerprint_changes_when_only_a_body_changes() {
        let (_dir, idx) = fresh();
        let before = [DocTopic {
            slug: "pat-winlink",
            title: "Pat Winlink",
            markdown: "# Pat Winlink\nHops are separated by commas.",
            source: DocSource::Knowledge,
        }];
        idx.populate_docs(&before).unwrap();

        // Same slug, same title — only the text is corrected.
        let after = [DocTopic {
            slug: "pat-winlink",
            title: "Pat Winlink",
            markdown: "# Pat Winlink\nHops are separated by slashes.",
            source: DocSource::Knowledge,
        }];

        assert_eq!(
            idx.docs_content_fingerprint().unwrap(),
            bundled_docs_fingerprint(&before),
            "a freshly populated index must match the bundle it came from"
        );
        assert_ne!(
            idx.docs_content_fingerprint().unwrap(),
            bundled_docs_fingerprint(&after),
            "a body-only correction must be detected — this is the bug: the old \
             slug-set check saw no drift and the operator kept the wrong text"
        );
    }

    /// The trap on the other side. `populate_docs` stores
    /// `extract_markdown(markdown)`, so the bundle fingerprint must extract too.
    /// If it hashed the RAW markdown it would never match the index, and every
    /// single launch would wipe and repopulate — forever, silently.
    #[test]
    fn fingerprints_agree_after_populate_so_startup_is_idempotent() {
        let (_dir, idx) = fresh();
        let topics = [
            DocTopic {
                slug: "01-a",
                title: "A",
                markdown: "# A\nSome **bold** text and `code` and a [link](http://x).",
                source: DocSource::UserGuide,
            },
            DocTopic {
                slug: "02-b",
                title: "B",
                markdown: "# B\n```\nax25:///DIGI/TARGET\n```\n",
                source: DocSource::Knowledge,
            },
        ];
        idx.populate_docs(&topics).unwrap();

        assert_eq!(
            idx.docs_content_fingerprint().unwrap(),
            bundled_docs_fingerprint(&topics),
            "fingerprints must agree right after populate, or the app repopulates \
             the docs index on EVERY launch"
        );

        // And repopulating with the same bundle keeps it stable.
        idx.populate_docs(&topics).unwrap();
        assert_eq!(
            idx.docs_content_fingerprint().unwrap(),
            bundled_docs_fingerprint(&topics)
        );
    }

    /// Slug drift (the original PR #347 bug) must still be caught.
    #[test]
    fn content_fingerprint_still_catches_slug_drift() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[DocTopic {
            slug: "01-getting-started",
            title: "Getting started",
            markdown: "ARDOP",
            source: DocSource::UserGuide,
        }])
        .unwrap();
        let renamed = [DocTopic {
            slug: "01-what-is-tuxlink",
            title: "Getting started",
            markdown: "ARDOP",
            source: DocSource::UserGuide,
        }];
        assert_ne!(
            idx.docs_content_fingerprint().unwrap(),
            bundled_docs_fingerprint(&renamed)
        );
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

    /// FTS5's MATCH argument is a query language. A question containing `?`, `.`,
    /// or `-` is a SYNTAX ERROR, not a miss — which surfaced to the model as a tool
    /// failure and left it with nothing to ground on.
    #[test]
    fn natural_language_question_does_not_error_and_still_matches() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[DocTopic {
            slug: "pat-winlink",
            title: "Pat Winlink",
            markdown: "# Pat Winlink\nConnect via a digipeater: ax25:///DIGI/TARGET.",
            source: DocSource::Knowledge,
        }])
        .unwrap();

        // The motivating question (KJ4UYO), verbatim. Raw, this is an FTS5 syntax
        // error near "." (from "ax.25") — it must still find the document.
        let hits = idx
            .search_docs("What is the syntax for Pat Winlink in EmComm Tools in ax.25 to connect via a digipeater?")
            .expect("a question must never surface as an FTS5 error");
        assert!(hits.iter().any(|h| h.slug == "pat-winlink"));
    }

    /// `-` is the NOT operator, so a bare slug is an FTS5 error. A model that pastes
    /// a slug back in as a query must not get an error.
    #[test]
    fn a_slug_as_a_query_does_not_error() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[DocTopic {
            slug: "pat-winlink",
            title: "Pat Winlink",
            markdown: "# Pat Winlink\nPat is a Winlink client.",
            source: DocSource::Knowledge,
        }])
        .unwrap();
        let hits = idx.search_docs("pat-winlink").expect("a slug must not error");
        assert!(hits.iter().any(|h| h.slug == "pat-winlink"));
    }

    /// Bare terms are ANDed by FTS5, so one absent word returns zero rows — which a
    /// caller cannot distinguish from "there is no documentation on this".
    #[test]
    fn one_absent_term_does_not_zero_out_the_result() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[DocTopic {
            slug: "playbook-ardop",
            title: "Playbook: ARDOP will not connect",
            markdown: "# Playbook: ARDOP will not connect\nCheck the audio device.",
            source: DocSource::McpKnowledge,
        }])
        .unwrap();
        // "troubleshooting" appears nowhere in the doc. Under implicit AND this
        // returns nothing; the OR fallback still finds it.
        let hits = idx.search_docs("ardop connect troubleshooting").unwrap();
        assert!(hits.iter().any(|h| h.slug == "playbook-ardop"));
    }

    /// A deliberate FTS5 expression still works — the fallback only engages when the
    /// query as written errors or matches nothing, so it can never remove results.
    #[test]
    fn explicit_fts5_syntax_is_preserved() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "a", title: "A", markdown: "ardop digital mode", source: DocSource::UserGuide },
            DocTopic { slug: "b", title: "B", markdown: "vara digital mode", source: DocSource::UserGuide },
        ])
        .unwrap();
        // Explicit AND: only "a" has both terms. If the fallback fired, "b" would
        // also come back (it has "digital").
        let hits = idx.search_docs("ardop AND digital").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slug, "a");
    }

    #[test]
    fn query_with_no_usable_token_returns_no_hits_not_an_error() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[DocTopic {
            slug: "a", title: "A", markdown: "x", source: DocSource::UserGuide,
        }])
        .unwrap();
        assert!(idx.search_docs("???").unwrap().is_empty());
        assert!(idx.search_docs("- . ?").unwrap().is_empty());
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
