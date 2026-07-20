//! `data.docs_search` — spec compat-tree rank 4 (routines-round2). A local
//! full-text search over the bundled in-app documentation, so a routine can
//! surface the operator manual (or ground an Elmer step) from within a run.
//!
//! Unlike `data.read` (a params-selected read source) this is a NEW ACTION with
//! its own param (`query`). It is INERT: local SQLite FTS5 only — it never
//! transmits, never touches the rig, never writes config, never reaches the
//! network (all four descriptor flags are `false`).
//!
//! It reuses the EXACT MCP `docs_search` path: the same
//! [`Index::search_docs`](crate::search::index::Index::search_docs)
//! (raw-then-OR fallback, `<mark>…</mark>` snippet format, 30-hit cap), via the
//! [`super::DocsSearchService`] seam — so the hits are byte-identical to the MCP
//! tool by construction (pinned against `tuxlink_mcp_core::ports::DocsHitDto`).
//! Like the MCP port ([`crate::mcp_ports::MonolithSearchPort::docs`]) it does
//! NOT apply the Help-window `retain_user_guide` filter: a routine sees every
//! indexed corpus (user-guide + knowledge + mcp-knowledge), same as the tool.
//!
//! Output: `{"hits":[{"title","slug","snippet"}, …]}`. Zero hits is a NORMAL
//! result (`{"hits":[]}`), never an error. An empty/missing `query` is invalid
//! params, rejected BEFORE any search runs.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::{Action, ActionDescriptor, OutputSpec, ParamSpec, ValueType};
use tuxlink_routines::error::StepError;

use super::DocsSearchService;

const DATA_DOCS_SEARCH: &str = "data.docs_search";

/// Shape-true dry-run output for `data.docs_search` (D6): a well-shaped
/// zero-hit result — a dry run never touches the FTS index, and empty hits is a
/// valid, non-error search outcome.
fn docs_search_dry_run_shape(_params: &Value) -> Value {
    json!({"hits": [], "dry_run": true})
}

/// `data.docs_search` params. `query` is REQUIRED and must be non-empty: an
/// absent field is a serde error (invalid params) and an empty/whitespace-only
/// string is rejected explicitly (a blank query is an author mistake, not a
/// request for "every document").
#[derive(Debug, Deserialize)]
struct DocsSearchParams {
    query: String,
}

/// `data.docs_search` — local FTS5 search over the bundled documentation.
/// All descriptor flags `false` (local read-only index; no radio, no transmit,
/// no config write, no network).
pub struct DocsSearch {
    docs_search: Arc<dyn DocsSearchService>,
}

impl DocsSearch {
    pub fn new(docs_search: Arc<dyn DocsSearchService>) -> Self {
        Self { docs_search }
    }
}

#[async_trait]
impl Action for DocsSearch {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            writes_config: false,
            name: DATA_DOCS_SEARCH,
            label: "Search app docs",
            description:
                "Full-text search the bundled in-app documentation (local, read-only).",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
            example_params: Some(r#"{"query":"find stations"}"#),
            allowed_values: None,
            params: &[ParamSpec {
                key: "query",
                ty: ValueType::String,
                required: true,
                description: "Full-text query over the bundled in-app documentation",
                allowed: None,
                example: r#""space weather""#,
            }],
            outputs: &[OutputSpec {
                key: "hits",
                ty: ValueType::ObjectList,
                description: "Matching doc pages (slug, title, snippet)",
                nullable: false,
            }],
            dry_run_shape: Some(docs_search_dry_run_shape),
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: DocsSearchParams =
            serde_json::from_value(params).map_err(|e| StepError::Action {
                action: DATA_DOCS_SEARCH.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        // An empty/whitespace-only query is an author mistake, not a request for
        // "every document" — reject as invalid params BEFORE any search. (The
        // FTS layer treats an empty query as zero hits, so a passthrough would
        // silently hide the mistake behind an empty result.)
        if parsed.query.trim().is_empty() {
            return Err(StepError::Action {
                action: DATA_DOCS_SEARCH.to_string(),
                cause: "invalid params: query must be a non-empty string".to_string(),
            });
        }

        // SAME `search_docs` the MCP `docs_search` tool runs (raw-then-OR
        // fallback, `<mark>` snippet, 30-cap), via the seam.
        let hits = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.docs_search.search_docs(&parsed.query) => res,
        }
        .map_err(|cause| StepError::Action {
            action: DATA_DOCS_SEARCH.to_string(),
            cause,
        })?;

        // Project each hit to the spec'd `{title, slug, snippet}` shape. Zero
        // hits yields `{"hits":[]}` — a normal result, never an error.
        let hits_json: Vec<Value> = hits
            .into_iter()
            .map(|h| {
                json!({
                    "title": h.title,
                    "slug": h.slug,
                    "snippet": h.snippet,
                })
            })
            .collect();

        Ok(json!({ "hits": hits_json }))
    }
}

// ============================================================================
// Real seam adapter — MonolithDocsSearchService. Mirrors the MCP `docs_search`
// tool's index path (`crate::mcp_ports::MonolithSearchPort::docs`): resolve the
// OPTIONALLY-managed `SearchService` via `try_state` (a build_service failure at
// startup leaves it unmanaged — degrade to an error rather than panic), lock the
// shared index, and run the SAME `search_docs`. No `retain_user_guide` filter —
// a routine, like the agent tool, sees every indexed corpus.
// ============================================================================

pub struct MonolithDocsSearchService {
    app: AppHandle,
}

impl MonolithDocsSearchService {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl DocsSearchService for MonolithDocsSearchService {
    async fn search_docs(
        &self,
        query: &str,
    ) -> Result<Vec<crate::search::docs_index::DocsHit>, String> {
        use tauri::Manager;
        let svc = self
            .app
            .try_state::<crate::search::commands::SearchService>()
            .ok_or_else(|| "search index unavailable".to_string())?;
        let hits = svc
            .index
            .lock()
            .map_err(|e| format!("docs index poisoned: {e}"))?
            .search_docs(query)
            .map_err(|e| format!("{e:?}"))?;
        Ok(hits)
    }
}

// ============================================================================
// Tests — trait fake, no hardware/tauri. The pin test drives a REAL, populated
// in-memory `Index` behind the seam so the action's output-shaping is exercised
// against the SAME `Index::search_docs` the MCP `docs_search` tool calls.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::docs_index::{DocSource, DocTopic, DocsHit};
    use crate::search::index::Index;
    use tuxlink_mcp_core::ports::DocsHitDto;

    // ---- FakeDocsSearchService --------------------------------------------
    // Panics if `search_docs` is called when a test didn't expect it (the
    // invalid-params tests rely on this: the reject happens BEFORE any search).

    type SearchFn = dyn Fn(&str) -> Result<Vec<DocsHit>, String> + Send + Sync;

    struct FakeDocsSearchService {
        search: Box<SearchFn>,
    }

    impl Default for FakeDocsSearchService {
        fn default() -> Self {
            Self {
                search: Box::new(|_| panic!("search_docs not expected in this test")),
            }
        }
    }

    impl FakeDocsSearchService {
        fn with_search(
            mut self,
            f: impl Fn(&str) -> Result<Vec<DocsHit>, String> + Send + Sync + 'static,
        ) -> Self {
            self.search = Box::new(f);
            self
        }
    }

    #[async_trait]
    impl DocsSearchService for FakeDocsSearchService {
        async fn search_docs(&self, query: &str) -> Result<Vec<DocsHit>, String> {
            (self.search)(query)
        }
    }

    fn action(fake: FakeDocsSearchService) -> DocsSearch {
        DocsSearch::new(Arc::new(fake))
    }

    /// A fresh, populated in-memory docs index (mirrors the `docs_index.rs`
    /// test helper). The TempDir must outlive the Index.
    fn fresh_index(topics: &[DocTopic<'_>]) -> (tempfile::TempDir, Index) {
        let dir = tempfile::tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.populate_docs(topics).unwrap();
        (dir, idx)
    }

    // ---- hits map to {title,slug,snippet}, byte-identical to MCP -----------

    #[tokio::test]
    async fn hits_map_to_title_slug_snippet_matching_mcp_docs_search() {
        let topics = [
            DocTopic {
                slug: "02-connections",
                title: "Connections",
                markdown: "# Connections\nARDOP is an HF digital mode.",
                source: DocSource::UserGuide,
            },
            DocTopic {
                slug: "01-getting-started",
                title: "Getting started",
                markdown: "# Getting started\nWelcome to Tuxlink.",
                source: DocSource::UserGuide,
            },
        ];
        let (_dir, index) = fresh_index(&topics);

        // MCP path: the SAME `Index::search_docs` the `MonolithSearchPort::docs`
        // port runs, mapped to the MCP `DocsHitDto {title,slug,snippet}`.
        let mcp: Vec<DocsHitDto> = index
            .search_docs("ARDOP")
            .unwrap()
            .into_iter()
            .map(|h| DocsHitDto {
                title: h.title,
                slug: h.slug,
                snippet: h.snippet,
            })
            .collect();
        assert!(!mcp.is_empty(), "fixture must produce at least one hit");
        // The <mark> snippet format crosses through unchanged — pin it.
        assert!(
            mcp.iter().any(|h| h.snippet.contains("<mark>")),
            "the FTS5 snippet() <mark> format must survive to the DTO"
        );
        let expected = serde_json::to_value(&mcp).unwrap();

        // Action path: the SAME index behind the seam.
        let index = std::sync::Mutex::new(index);
        let out = action(FakeDocsSearchService::default().with_search(move |q| {
            index
                .lock()
                .unwrap()
                .search_docs(q)
                .map_err(|e| format!("{e:?}"))
        }))
        .execute(json!({ "query": "ARDOP" }), CancellationToken::new())
        .await
        .unwrap();

        assert_eq!(
            out["hits"], expected,
            "routine hits must be byte-identical to the MCP docs_search output"
        );
    }

    // ---- zero hits is not an error -----------------------------------------

    #[tokio::test]
    async fn zero_hits_returns_empty_hits_not_an_error() {
        let out = action(FakeDocsSearchService::default().with_search(|_| Ok(vec![])))
            .execute(
                json!({ "query": "nothingmatchesthisxyz" }),
                CancellationToken::new(),
            )
            .await
            .expect("zero hits is a normal result, never an error");
        assert_eq!(out["hits"], json!([]));
    }

    // ---- empty / missing / whitespace query is invalid params --------------

    #[tokio::test]
    async fn empty_query_is_invalid_params_without_searching() {
        // The default fake panics if search_docs is ever called — proving the
        // reject happens BEFORE any search.
        let err = action(FakeDocsSearchService::default())
            .execute(json!({ "query": "" }), CancellationToken::new())
            .await
            .expect_err("an empty query is invalid params");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.docs_search");
                assert!(cause.contains("invalid params"), "got: {cause}");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_query_is_invalid_params_without_searching() {
        let err = action(FakeDocsSearchService::default())
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("a missing query field is invalid params");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "data.docs_search");
                assert!(cause.contains("invalid params"), "got: {cause}");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn whitespace_only_query_is_invalid_params_without_searching() {
        let err = action(FakeDocsSearchService::default())
            .execute(json!({ "query": "   " }), CancellationToken::new())
            .await
            .expect_err("a whitespace-only query is invalid params");
        assert!(matches!(err, StepError::Action { .. }));
    }

    // ---- cancellation before search is prompt ------------------------------

    #[tokio::test]
    async fn pre_cancelled_token_returns_cancelled_without_searching() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = action(FakeDocsSearchService::default())
            .execute(json!({ "query": "ardop" }), cancel)
            .await
            .expect_err("a pre-cancelled token must not search");
        assert!(matches!(err, StepError::Cancelled));
    }

    // ---- descriptor flags --------------------------------------------------

    #[test]
    fn descriptor_all_flags_false() {
        let d = action(FakeDocsSearchService::default()).descriptor();
        assert_eq!(d.name, "data.docs_search");
        assert_eq!(d.label, "Search app docs");
        assert!(!d.label.is_empty() && !d.description.is_empty());
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
        assert!(!d.writes_config);
    }

    // ---- D6: authoring affordances + dry-run shape -------------------------

    #[test]
    fn descriptor_advertises_example_params_and_dry_run_shape() {
        let d = action(FakeDocsSearchService::default()).descriptor();
        assert_eq!(d.example_params, Some(r#"{"query":"find stations"}"#));
        assert!(d.dry_run_shape.is_some());
    }

    #[test]
    fn dry_run_shape_is_empty_hits_marked_dry_run() {
        let out = docs_search_dry_run_shape(&json!({"query": "anything"}));
        assert_eq!(out, json!({"hits": [], "dry_run": true}));
    }

    /// tuxlink-3nvvl: every descriptor's example_params must pass its own
    /// declared ParamSpecs — locks the registry backfill mechanically.
    #[test]
    fn descriptor_examples_pass_their_own_param_specs() {
        use tuxlink_routines::validate::params::example_self_check;
        let d = DocsSearch::new(Arc::new(FakeDocsSearchService::default())).descriptor();
        let f = example_self_check(&d);
        assert!(f.is_empty(), "{}: {f:?}", d.name);
    }

}
