# Elmer Knowledge Tier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Elmer a `docs_read(slug)` tool and an agent-only Winlink-client corpus, so it answers Pat and Winlink Express questions from real documentation instead of fabricating.

**Architecture:** One SQLite FTS5 index fed by three source directories (`docs/user-guide/`, new `docs/knowledge/`, and `docs/mcp-knowledge/`). `docs_search` stays the locator; a new `docs_read` serves the `body` column already stored in `docs_fts` as the destination. The Elmer system prompt gains the grounding clause from P0 `tuxlink-0mudm`. A registry-drift test and a retrieval eval set prevent silent regression.

**Tech Stack:** Rust 2021 (MSRV **1.75**), `rusqlite` + FTS5, `rmcp` `#[tool]` macros, Tauri 2.x. Frontend untouched.

**Spec:** [`docs/superpowers/specs/2026-07-13-elmer-knowledge-tier-design.md`](../specs/2026-07-13-elmer-knowledge-tier-design.md)

**Issues:** closes `tuxlink-aib3n`; closes the tool + prompt substance of P0 `tuxlink-0mudm`.

## Global Constraints

- **Worktree:** `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-aib3n-elmer-winlink-docs`, branch `bd-tuxlink-aib3n/elmer-winlink-docs`. Pin absolute paths in shell commands; the shell cwd silently reverts to the main checkout.
- **Commits:** every commit needs an `Agent: <moniker>` trailer on its own line, supplied **inline via heredoc** (`git commit -F - <<'EOF' … EOF`). A `-F file` hides the trailer from the commit-discipline hook and is rejected. Run `git` as a **bare** command from a shell already cd'd into the worktree; `cd X && git commit` is denied by the main-checkout-race hook.
- **MSRV is 1.75.** Clippy denies `incompatible_msrv`. Do not use APIs stabilized in 1.76+ (e.g. `Result::inspect_err`).
- **This Pi cannot finish a cold `cargo` build or test.** Do **not** attempt `cargo build`/`cargo test`/`cargo clippy` locally — write the Rust and its tests, push, and let CI compile and run them. Local verification is limited to `pnpm vitest run` (fast) and `pnpm lint:docs`.
- **CI gates** (amd64 + arm64): `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` and `cargo test --manifest-path src-tauri/Cargo.toml --locked`. Clippy is `-D warnings`: no unused imports, no needless clones.
- **`--no-verify` is banned.** A fresh worktree needs `pnpm install` before the pre-push `lint:docs` hook can run. (Already done in this worktree.)
- **Accuracy bar:** every RF/connect syntax claim in a knowledge doc is verbatim from `pat`'s own `--help` output on this machine. Never from memory, and never from the bd issue text (which is wrong — see Task 6).
- **`wire-walk` is a hard gate** before any "done"/"shipped" claim or closing the issue (Task 9).

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src-tauri/src/search/docs_index.rs` | `docs_fts` schema, populate, search | Add `DocSource` enum, `source` field on `DocTopic`, `read_doc(slug)` |
| `src-tauri/src/search/docs_bundle.rs` | Compile-time registry of indexed docs | Add `source` to all entries; register the orphan + 2 new + 10 mcp-knowledge |
| `src-tauri/src/search/docs_registry_test.rs` | **new** — filesystem-vs-registry drift guard | Create |
| `src-tauri/src/search/mod.rs` | Wires the above | Add `mod docs_registry_test` (cfg(test)) |
| `src-tauri/tuxlink-mcp-core/src/ports.rs` | Port traits + DTOs | `DocsHitDto.path` → `slug`; add `DocBodyDto`; add `SearchPort::doc` |
| `src-tauri/src/mcp_ports.rs` | Monolith port impls | Implement `SearchPort::doc`; update `docs()` for the rename |
| `src-tauri/tuxlink-mcp-core/src/router.rs` | MCP tool registrations | Add `SlugParams` + `#[tool] docs_read`; rewrite `docs_search` description |
| `src-tauri/tuxlink-mcp-core/src/lib.rs` | Test mock `SearchPort` | Implement `doc`; update for rename |
| `src-tauri/tuxlink-mcp-testserver/src/{mocks,fixture}.rs` | Test-server mock `SearchPort` | Implement `doc`; update for rename |
| `src-tauri/tuxlink-agent-frontend/src/provider.rs` | `ELMER_SYSTEM_PROMPT` | Add the grounding clause |
| `docs/knowledge/pat-winlink.md` | **new** — agent-only Pat reference | Create |
| `docs/knowledge/winlink-express.md` | **new** — agent-only WLE reference | Create |

Frontend is **not** touched: `src/help/topics.ts` globs only `docs/user-guide/`, so the new corpora stay out of the Help sidebar with no code change.

---

### Task 1: Registry completeness + drift guard

Fixes the orphaned `36-off-air-space-weather.md` and adds the test that would have caught it. Ships first because every later task depends on the registry shape.

**Files:**
- Modify: `src-tauri/src/search/docs_index.rs` (add `DocSource`, extend `DocTopic`)
- Modify: `src-tauri/src/search/docs_bundle.rs` (all entries + the orphan)
- Create: `src-tauri/src/search/docs_registry_test.rs`
- Modify: `src-tauri/src/search/mod.rs`

**Interfaces:**
- Produces: `DocSource::{UserGuide, Knowledge, McpKnowledge}` and `DocTopic { slug, title, markdown, source }`. Tasks 2, 6, 7 rely on these exact names.

- [ ] **Step 1: Write the failing drift test**

Create `src-tauri/src/search/docs_registry_test.rs`:

```rust
//! Guards the filesystem-vs-registry boundary.
//!
//! `BUNDLED_TOPICS` is hand-maintained while `src/help/topics.ts` auto-globs via
//! `import.meta.glob`. That asymmetry let docs/user-guide/36-off-air-space-weather.md
//! exist on disk, render in the sidebar, and be absent from the FTS index — so
//! `docs_search` could not find it. A test comparing the registry against ITSELF
//! (`len == TOPICS.len()`) can never catch that. This one crosses the boundary.

use crate::search::docs_bundle::BUNDLED_TOPICS;
use crate::search::docs_index::DocSource;
use std::collections::HashSet;
use std::path::PathBuf;

/// Repo root, derived from the crate manifest dir (`src-tauri/`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn dir_for(source: DocSource) -> PathBuf {
    let sub = match source {
        DocSource::UserGuide => "docs/user-guide",
        DocSource::Knowledge => "docs/knowledge",
        DocSource::McpKnowledge => "docs/mcp-knowledge",
    };
    repo_root().join(sub)
}

/// Every `.md` on disk in every indexed source dir must be registered.
#[test]
fn every_markdown_file_on_disk_is_registered() {
    let registered: HashSet<String> = BUNDLED_TOPICS
        .iter()
        .map(|t| t.slug.to_string())
        .collect();

    let mut missing: Vec<String> = Vec::new();

    for source in [DocSource::UserGuide, DocSource::Knowledge, DocSource::McpKnowledge] {
        let dir = dir_for(source);
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("readable dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("utf-8 file stem")
                .to_string();
            if !registered.contains(&stem) {
                missing.push(format!("{}/{stem}.md", dir.display()));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "these markdown files exist on disk but are NOT in BUNDLED_TOPICS, so they are \
         absent from docs_fts and unfindable by docs_search/docs_read:\n  {}",
        missing.join("\n  ")
    );
}

/// Slugs are the retrieval key; duplicates would make `docs_read` ambiguous.
#[test]
fn registered_slugs_are_unique() {
    let mut seen = HashSet::new();
    for t in BUNDLED_TOPICS {
        assert!(seen.insert(t.slug), "duplicate slug in BUNDLED_TOPICS: {}", t.slug);
    }
}
```

- [ ] **Step 2: Wire the test module**

In `src-tauri/src/search/mod.rs`, add alongside the existing `mod` declarations:

```rust
#[cfg(test)]
mod docs_registry_test;
```

- [ ] **Step 3: Add `DocSource` and extend `DocTopic`**

In `src-tauri/src/search/docs_index.rs`, replace the `DocTopic` struct (currently lines 21-26) with:

```rust
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
```

- [ ] **Step 4: Add `source` to every existing entry and register the orphan**

In `src-tauri/src/search/docs_bundle.rs`, add `source: DocSource::UserGuide,` to each of the 35 existing `DocTopic` literals, extend the import, and append the missing topic.

Change the import line:

```rust
use crate::search::docs_index::{DocSource, DocTopic};
```

Each existing entry becomes (shown for the first; apply the same to all 35):

```rust
    DocTopic {
        slug: "01-what-is-tuxlink",
        title: "What is Tuxlink?",
        markdown: include_str!("../../../docs/user-guide/01-what-is-tuxlink.md"),
        source: DocSource::UserGuide,
    },
```

Append after the `35-agent-mcp` entry, before the closing `];`:

```rust
    // Was on disk but unregistered until 2026-07-13 — the drift this file's
    // companion test now guards against.
    DocTopic {
        slug: "36-off-air-space-weather",
        title: "Off-air space weather",
        markdown: include_str!("../../../docs/user-guide/36-off-air-space-weather.md"),
        source: DocSource::UserGuide,
    },
```

Also fix the stale module doc-comment at the top of the file:

```rust
//! Compile-time bundle of the indexed documentation corpora, used by
//! build_service to populate docs_fts (tuxlink-0gsy / spec §9.1).
//!
//! Three sources are indexed: docs/user-guide/ (also the Help sidebar),
//! docs/knowledge/ (agent-only, other Winlink clients), and docs/mcp-knowledge/
//! (playbooks; also served as MCP resources). All three are searchable via
//! docs_search and readable via docs_read.
//!
//! Adding a topic: include_str! it below + extend BUNDLED_TOPICS. The test in
//! docs_registry_test.rs FAILS if a .md exists on disk and is not registered here.
//!
//! Path resolution: include_str! is relative to THIS file. From
//! src-tauri/src/search/docs_bundle.rs, `../../../docs/...` reaches the repo root.
```

- [ ] **Step 5: Verify the title matches the document**

Read the first heading of the orphan to confirm the registered title is right:

Run: `head -1 /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-aib3n-elmer-winlink-docs/docs/user-guide/36-off-air-space-weather.md`

If the `# H1` differs from "Off-air space weather", use the document's actual H1 as the `title`. Titles are an FTS-indexed column, so a wrong title degrades retrieval.

- [ ] **Step 6: Commit**

Bare git, from a shell already cd'd into the worktree:

```bash
git add src-tauri/src/search/docs_index.rs src-tauri/src/search/docs_bundle.rs \
        src-tauri/src/search/docs_registry_test.rs src-tauri/src/search/mod.rs
git commit -F - <<'EOF'
fix(search): register orphaned space-weather topic + guard registry drift

docs/user-guide/36-off-air-space-weather.md was on disk but absent from
BUNDLED_TOPICS, so it was never inserted into docs_fts and could not be found by
docs_search or the in-app help search — while still rendering in the sidebar,
because src/help/topics.ts auto-globs while the Rust registry is hand-maintained.

Adds DocSource so a topic declares which corpus it came from, and a test that
walks the source dirs and fails when a .md on disk is unregistered. The existing
list_resources_returns_full_catalog test compares the registry against itself
(len == CATALOG.len()) and is structurally incapable of catching this.

Agent: sumac-magnolia-fen
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 2: `Index::read_doc` — the storage half of the destination

**Files:**
- Modify: `src-tauri/src/search/docs_index.rs`

**Interfaces:**
- Consumes: `DocTopic`, `DocSource` (Task 1).
- Produces: `pub struct DocBody { pub slug: String, pub title: String, pub body: String }` and `Index::read_doc(&self, slug: &str) -> Result<Option<DocBody>, IndexError>`. Task 3 calls this.

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block in `src-tauri/src/search/docs_index.rs`:

```rust
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

    #[test]
    fn docs_slugs_lists_every_slug_for_the_not_found_hint() {
        let (_dir, idx) = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "a", title: "A", markdown: "x", source: DocSource::UserGuide },
            DocTopic { slug: "b", title: "B", markdown: "y", source: DocSource::Knowledge },
        ])
        .unwrap();
        let mut slugs = idx.docs_slugs().unwrap();
        slugs.sort();
        assert_eq!(slugs, vec!["a".to_string(), "b".to_string()]);
    }
```

Note: the pre-existing tests in this module construct `DocTopic` without `source`. Task 1 added the field, so **every existing `DocTopic { … }` literal in this test module must gain a `source:`** — use `DocSource::UserGuide` for all of them. Without that the crate does not compile.

- [ ] **Step 2: Add `DocBody` and `read_doc`**

In `src-tauri/src/search/docs_index.rs`, add the struct beside `DocsHit`:

```rust
/// A whole document, returned by `read_doc`. This is the payload `docs_search`'s
/// 12-token `snippet()` cannot carry.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocBody {
    pub slug: String,
    pub title: String,
    pub body: String,
}
```

And add the method inside `impl Index`, after `search_docs`:

```rust
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
```

> **FTS5 note:** `docs_fts` is a virtual table, so a `WHERE slug = ?1` equality
> filter is a linear scan rather than an index seek. At ~48 documents that is
> irrelevant; do not add an auxiliary table for it.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/search/docs_index.rs
git commit -F - <<'EOF'
feat(search): Index::read_doc — return a whole document, not a snippet

docs_fts already stores the full extracted body; search_docs only ever exposed a
12-token snippet() window of it. read_doc(slug) returns the whole thing, which is
the storage half of the search-then-read pair P0 tuxlink-0mudm calls for.

Agent: sumac-magnolia-fen
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 3: Port layer — `SearchPort::doc` + the `path`→`slug` rename

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs`
- Modify: `src-tauri/src/mcp_ports.rs`
- Modify: `src-tauri/tuxlink-mcp-core/src/lib.rs` (test mock, ~line 310)
- Modify: `src-tauri/tuxlink-mcp-testserver/src/mocks.rs` (~line 171)
- Modify: `src-tauri/tuxlink-mcp-testserver/src/fixture.rs` (~line 121)

**Interfaces:**
- Consumes: `Index::read_doc` → `Option<DocBody>` (Task 2).
- Produces: `DocBodyDto { slug, title, body }`, `DocsHitDto { title, slug, snippet }` (field renamed from `path`), and `SearchPort::doc(&self, slug: &str) -> Result<Option<DocBodyDto>, PortError>`. Task 4 calls this.

**Why the rename:** `DocsHitDto` currently exposes the slug under the key `path`. If `docs_search` returns `path` and `docs_read` takes `slug`, a small tool-reliant model has to infer they are the same string. `DocsHitDto` has no `serde(rename_all)` and no frontend consumer — only the two test mocks below — so the rename is contained.

- [ ] **Step 1: Rename the field and add the DTO**

In `src-tauri/tuxlink-mcp-core/src/ports.rs`, replace `DocsHitDto` (lines 111-116) and add `DocBodyDto`:

```rust
/// One in-app documentation search hit. `slug` is the key `docs_read` takes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocsHitDto {
    pub title: String,
    pub slug: String,
    pub snippet: String,
}

/// A whole documentation page, returned by `docs_read`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocBodyDto {
    pub slug: String,
    pub title: String,
    pub body: String,
}
```

- [ ] **Step 2: Extend the `SearchPort` trait**

In the same file, in `pub trait SearchPort` (line 722), add after the `docs` method:

```rust
    /// Read one documentation page in full, by the `slug` returned from `docs`.
    /// `Ok(None)` means the slug is unknown. App-owned content; does not taint.
    async fn doc(&self, slug: &str) -> Result<Option<DocBodyDto>, PortError>;
```

- [ ] **Step 3: Implement it on `MonolithSearchPort`**

In `src-tauri/src/mcp_ports.rs`, update the existing `docs` mapping for the rename and add `doc` after it:

```rust
    async fn docs(&self, query: &str) -> Result<Vec<DocsHitDto>, PortError> {
        let svc = self
            .app
            .try_state::<crate::search::commands::SearchService>()
            .ok_or_else(|| PortError::Unavailable("search index unavailable".to_string()))?;
        // Mirror `docs_search`: lock the shared index and run the docs FTS path.
        let hits = svc
            .index
            .lock()
            .map_err(|e| PortError::Internal(format!("docs index poisoned: {e}")))?
            .search_docs(query)
            .map_err(|e| PortError::Internal(redact_err(format!("{e:?}"))))?;
        Ok(hits
            .into_iter()
            .map(|h| DocsHitDto {
                title: h.title,
                slug: h.slug,
                snippet: h.snippet,
            })
            .collect())
    }

    async fn doc(&self, slug: &str) -> Result<Option<DocBodyDto>, PortError> {
        let svc = self
            .app
            .try_state::<crate::search::commands::SearchService>()
            .ok_or_else(|| PortError::Unavailable("search index unavailable".to_string()))?;
        let found = svc
            .index
            .lock()
            .map_err(|e| PortError::Internal(format!("docs index poisoned: {e}")))?
            .read_doc(slug)
            .map_err(|e| PortError::Internal(redact_err(format!("{e:?}"))))?;
        Ok(found.map(|d| DocBodyDto {
            slug: d.slug,
            title: d.title,
            body: d.body,
        }))
    }
```

Add `DocBodyDto` to the `tuxlink_mcp_core::ports` import list at the top of `mcp_ports.rs`.

- [ ] **Step 4: Update the two test mocks**

`SearchPort` gains a required method, so every implementor must provide it or the crate will not compile.

In `src-tauri/tuxlink-mcp-core/src/lib.rs` (~line 310), the mock's `docs` returns a `DocsHitDto` literal — change `path:` to `slug:` — and add:

```rust
        async fn doc(&self, slug: &str) -> Result<Option<DocBodyDto>, PortError> {
            Ok(Some(DocBodyDto {
                slug: slug.to_string(),
                title: "Test doc".to_string(),
                body: "Full body text.".to_string(),
            }))
        }
```

Do the same in `src-tauri/tuxlink-mcp-testserver/src/mocks.rs` (~line 171). Add `DocBodyDto` to both files' import lists. In `src-tauri/tuxlink-mcp-testserver/src/fixture.rs`, the `pub docs: Vec<DocsHitDto>` field (~line 121) needs any literal constructing it updated for `path` → `slug`; grep the crate for `path:` near `DocsHitDto` to find them all.

- [ ] **Step 5: Find every remaining `DocsHitDto` construction**

Run:

```bash
grep -rn "DocsHitDto" /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-aib3n-elmer-winlink-docs/src-tauri --include=*.rs
```

Every construction site must use `slug:`. CI clippy is `-D warnings` and a missed site is a hard compile error, so this grep is the cheap way to catch them before pushing (you cannot compile locally).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/tuxlink-mcp-core/src/ports.rs src-tauri/src/mcp_ports.rs \
        src-tauri/tuxlink-mcp-core/src/lib.rs \
        src-tauri/tuxlink-mcp-testserver/src/mocks.rs \
        src-tauri/tuxlink-mcp-testserver/src/fixture.rs
git commit -F - <<'EOF'
feat(mcp): SearchPort::doc + rename DocsHitDto.path to slug

Adds the port method behind docs_read. Also renames DocsHitDto.path to slug: the
field always held a slug, and docs_search returning "path" while docs_read takes
"slug" forces a small tool-reliant model to infer they are the same key. The DTO
has no serde rename_all and no frontend consumer, so the rename is contained to
the two test mocks.

Agent: sumac-magnolia-fen
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 4: The `docs_read` MCP tool + self-describing tool descriptions

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/router.rs` (tool at ~313; params struct at ~1113)

**Interfaces:**
- Consumes: `SearchPort::doc` (Task 3).
- Produces: MCP tools `docs_search` and `docs_read`.

**Design constraint from the spec:** the target model (Qwen3-Coder-Next) has no domain knowledge and treats tools as its only information source. The two-step protocol must be legible **in the tool descriptions**, because tool schemas are the one context the runner always presents.

- [ ] **Step 1: Add the params struct**

In `src-tauri/tuxlink-mcp-core/src/router.rs`, beside `QueryParams` (~line 1113):

```rust
/// `{ "slug": "pat-winlink" }` — input for `docs_read`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SlugParams {
    /// The `slug` of a documentation page, exactly as returned by `docs_search`.
    pub slug: String,
}
```

- [ ] **Step 2: Rewrite `docs_search`'s description and add `docs_read`**

Replace the existing `docs_search` tool block (~line 312) with both tools:

```rust
    #[tool(
        name = "docs_search",
        description = "Search the documentation by keyword. Covers Tuxlink's own user guide, \
                       troubleshooting playbooks, and reference material on OTHER Winlink clients \
                       (Pat, Winlink Express). Returns ranked hits as {title, slug, snippet}. \
                       IMPORTANT: the snippet is only a short fragment around the keyword match \
                       and is NOT enough to answer from. To read a page, call docs_read with its \
                       slug. App-owned content; does not taint. Read-only."
    )]
    pub async fn docs_search(
        &self,
        params: Parameters<QueryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(QueryParams { query }) = params;
        let dto = self.state.search.docs(&query).await.map_err(port_err)?;
        Ok(CallToolResult::success(vec![ContentBlock::json(dto)?]))
    }

    #[tool(
        name = "docs_read",
        description = "Read one documentation page IN FULL, given the slug from a docs_search hit. \
                       This is how you get the actual text — command syntax, connection strings, \
                       configuration steps — that docs_search's snippet only hints at. Use \
                       docs_search first to find the slug, then docs_read to read the page, then \
                       answer FROM the page. If the slug is unknown, the result lists the valid \
                       slugs. App-owned content; does not taint. Read-only."
    )]
    pub async fn docs_read(
        &self,
        params: Parameters<SlugParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(SlugParams { slug }) = params;
        match self.state.search.doc(&slug).await.map_err(port_err)? {
            Some(doc) => Ok(CallToolResult::success(vec![ContentBlock::json(doc)?])),
            None => {
                // Steer, don't derail: a wrong slug guess should tell the model what
                // it CAN read rather than surfacing as a tool error mid-turn.
                let hits = self.state.search.docs(&slug).await.map_err(port_err)?;
                let available: Vec<String> = hits.into_iter().map(|h| h.slug).collect();
                Ok(CallToolResult::success(vec![ContentBlock::json(
                    serde_json::json!({
                        "error": "unknown slug",
                        "requested": slug,
                        "hint": "Call docs_search first and pass a slug from its hits.",
                        "closest_slugs": available,
                    }),
                )?]))
            }
        }
    }
```

> The not-found branch reuses `docs` as a "did you mean" — the requested slug is
> itself a reasonable FTS query, so a near-miss like `pat_winlink` still surfaces
> `pat-winlink`. If it returns nothing, `closest_slugs` is `[]`, which is honest.

- [ ] **Step 3: Confirm `serde_json` is in scope**

Run:

```bash
grep -n "serde_json" /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-aib3n-elmer-winlink-docs/src-tauri/tuxlink-mcp-core/Cargo.toml
```

Expected: `serde_json` appears as a dependency. If it does not, add it to `[dependencies]` and **regenerate `Cargo.lock`** — `--locked` in CI fails on a stale lockfile.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tuxlink-mcp-core/src/router.rs
git commit -F - <<'EOF'
feat(mcp): docs_read tool — give docs_search a destination

docs_search returned a 12-token snippet() window and nothing could turn its slug
into the document, so the search tool was a locator with no destination and the
model answered product questions from nothing (P0 tuxlink-0mudm).

Both tool descriptions now spell out the search-then-read protocol. The consuming
model is small and carries no domain knowledge, so the workflow has to live in the
tool schemas — the one context the runner always presents — not only in the system
prompt. An unknown slug returns a steering hint with candidate slugs rather than an
error, so a wrong guess does not derail the turn.

Agent: sumac-magnolia-fen
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 5: The grounding clause in `ELMER_SYSTEM_PROMPT`

**Files:**
- Modify: `src-tauri/tuxlink-agent-frontend/src/provider.rs` (const at line 829)

**Interfaces:**
- Consumes: the tool names `docs_search` / `docs_read` (Task 4).

`ELMER_SYSTEM_PROMPT` currently never mentions documentation. This is the prompt half of P0 `tuxlink-0mudm`.

- [ ] **Step 1: Write the failing test**

Add to the test module in `provider.rs` (or create one if absent — check with `grep -n "mod tests" src-tauri/tuxlink-agent-frontend/src/provider.rs`):

```rust
    #[test]
    fn system_prompt_directs_the_model_to_the_docs_tools() {
        // P0 tuxlink-0mudm: the model invented Tuxlink's own credential storage
        // because nothing told it to look anything up. Both tools must be named.
        assert!(ELMER_SYSTEM_PROMPT.contains("docs_search"));
        assert!(ELMER_SYSTEM_PROMPT.contains("docs_read"));
    }
```

- [ ] **Step 2: Add the clause**

In `src-tauri/tuxlink-agent-frontend/src/provider.rs`, insert this paragraph immediately **before** the final `Be concise and practical.";` line of the const. Keep the existing trailing-backslash line-continuation style exactly:

```rust
\
Documentation and product knowledge: you do NOT reliably know how Tuxlink works, \
and you do NOT reliably know the details of other Winlink clients (Pat, Winlink \
Express) — none of it is in your training data, so anything you recall about them \
is likely invented. For ANY question about how something works, how to configure \
it, what a setting does, command or connection syntax, or troubleshooting steps — \
for Tuxlink OR for another Winlink client — call docs_search to find the relevant \
page, then call docs_read on that page's slug to read it in full, and answer FROM \
the page. docs_search's snippet is only a fragment; never answer from the snippet \
alone. Say which client you are describing, since Tuxlink, Pat, and Winlink Express \
differ. If docs_read shows the documentation does not cover the question, say you \
do not know and say what the docs DO cover — do not guess. \
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tuxlink-agent-frontend/src/provider.rs
git commit -F - <<'EOF'
feat(elmer): system prompt directs product/config questions to the docs tools

ELMER_SYSTEM_PROMPT never mentioned documentation, so the model answered "where
does Tuxlink store my password?" from nothing and invented a plausible-but-wrong
dotfile — the exact symptom recorded in P0 tuxlink-0mudm. Adds the grounding
clause: search, then read, then answer from the page; name the client; decline
when uncovered.

Per the issue's own scope-correction note, grounded honesty is delivered by tool
plus prompt, NOT by a trained refusal reflex.

Agent: sumac-magnolia-fen
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 6: The agent-only Winlink-client corpus

**Files:**
- Create: `docs/knowledge/pat-winlink.md`
- Create: `docs/knowledge/winlink-express.md`
- Modify: `src-tauri/src/search/docs_bundle.rs` (register the 2 new + the 10 mcp-knowledge docs)

**Interfaces:**
- Consumes: `DocSource::{Knowledge, McpKnowledge}` (Task 1).
- Produces: slugs `pat-winlink`, `winlink-express`, plus the 10 mcp-knowledge slugs. Task 7's evals assert these.

**ACCURACY GATE — read before writing a single line.** The bd issue `tuxlink-aib3n`
records Pat's digipeater syntax as `ax25:///DIGI1,DIGI2/TARGET`. **That is wrong.**
Hops are separated by `/`, not commas. The issue text is not a source. Re-derive
every syntax claim from the binary on this machine:

```bash
pat connect --help
pat --help
```

Verified output (`pat v1.0.0`, 2026-07-13) — the grammar and the digipeater line:

```
Usage:
  connect 'alias' or 'transport://[host][/digi]/targetcall[?params...]'

  connect ax25:///LA1B-10              Connect to the RMS Gateway LA1B-10 using AX.25 engine as per configuration.
  connect ax25+linux://tmd710/LA1B-10  Connect to LA1B-10 using Linux kernel's AX.25 stack on axport 'tmd710'.
  connect ax25:///LA1B/LA5NTA          Peer-to-peer connection with LA5NTA via LA1B digipeater.
```

- [ ] **Step 1: Resolve the two open accuracy questions**

1. **Multi-hop.** Pat's help *states* "if the path has multiple hops (e.g. AX.25), they are separated by `/`" but only *exemplifies* one digi. Confirm 2+ digis against Pat's source (`https://github.com/la5nta/pat`, the connect-URL parser) before asserting it. If it cannot be confirmed, document the single-digi form as verified and describe multi-hop as following from the stated grammar — do not silently present it as verified.
2. **EmComm Tools.** The motivating question was asked in ETC terms. Confirm ETC ships Pat stock (no wrapper altering the connect string). If unconfirmed, say so in the doc rather than implying it was checked.

Use `WebFetch`/`WebSearch` or the Hamexandria DB (`~/Code/library-of-hamexandria`, `uv run ham-search`). **Do not guess.** An operator keys these strings on the air.

- [ ] **Step 2: Write `docs/knowledge/pat-winlink.md`**

Constraints: `docs_read` returns the **whole** document into a bounded context, so keep it tight (target under ~150 lines), syntax-forward, keyword-dense near the terms an operator would type. First line must be an `# H1` (the extractor and the registry title both key off it).

Required content:
- H1: `# Pat Winlink (third-party client — not Tuxlink)`
- One-line scope statement: this describes **Pat**, a different Winlink client.
- The connect-URL grammar, verbatim: `transport://[host][/digi]/targetcall[?params...]`
- **The host-vs-path distinction**, called out explicitly — host addresses the local TNC/modem; the path is the RF route; the last path element is the target callsign. This is the single most-confused point.
- The digipeater form, with a worked example: `pat connect "ax25:///W4ABC-1/W4XYZ-10"` — reach `W4XYZ-10` via digi `W4ABC-1`. Include Pat's own `ax25:///LA1B/LA5NTA` line verbatim as the citation.
- Multi-hop: `ax25:///DIGI1/DIGI2/TARGET` (flagged per Step 1's finding).
- The triple slash explained: `ax25://` + empty host + `/path`.
- Explicit axport form: `ax25+linux://tmd710/LA1B-10`.
- Transports table: `telnet`, `ardop`, `pactor`, `varahf`, `varafm`, `ax25`, `ax25+agwpe`, `ax25+linux`, `ax25+serial-tnc`.
- Params: `?freq=` (ardop + ax25 only), `?host=`, `?prehook=`.
- CLI vs `pat interactive` (same connect string at the `pat>` prompt); `pat http` web UI.
- Config path: `~/.config/pat/config.json`.
- EmComm Tools note (per Step 1).
- A short **"See also"** line pointing to `32-from-express-or-pat` for how Pat compares to Tuxlink.

**Do NOT** write a Pat-vs-Tuxlink comparison table. `docs/user-guide/32-from-express-or-pat.md` already owns that. Two documents making the same comparison in different words is how a BM25 corpus starts contradicting itself.

- [ ] **Step 3: Write `docs/knowledge/winlink-express.md`**

Same constraints. H1: `# Winlink Express (third-party client — not Tuxlink)`.

Required content: session types (Telnet, Packet, VARA HF/FM, ARDOP, Pactor); opening a session from the main window; channel selection and the channel list; **where the digipeater path is entered in a Packet session** (the WLE analogue of the Pat question); forms; account and password basics (callsign + Winlink password, password recovery). Same "See also" pointer to `32-from-express-or-pat`; no comparison table.

Ground it in the WLE install under Wine on R2 (`~/.wine-wle`) or WLE's own help — **not** recollection. If a detail cannot be verified, omit it or mark it explicitly unverified. A confidently wrong WLE menu path is the same failure class as a bad connect string.

- [ ] **Step 4: Register all 12 new topics**

In `src-tauri/src/search/docs_bundle.rs`, append before the closing `];`:

```rust
    // --- docs/knowledge/ — agent-only reference on OTHER Winlink clients.
    DocTopic {
        slug: "pat-winlink",
        title: "Pat Winlink (third-party client)",
        markdown: include_str!("../../../docs/knowledge/pat-winlink.md"),
        source: DocSource::Knowledge,
    },
    DocTopic {
        slug: "winlink-express",
        title: "Winlink Express (third-party client)",
        markdown: include_str!("../../../docs/knowledge/winlink-express.md"),
        source: DocSource::Knowledge,
    },
    // --- docs/mcp-knowledge/ — playbooks + specs. Previously reachable ONLY by
    // external MCP clients via the tuxlink:// resource tier, which in-app Elmer
    // never lists or reads. Indexing them here is what makes them Elmer-visible.
    DocTopic {
        slug: "playbook-ardop-wont-connect",
        title: "Playbook: ARDOP won't connect",
        markdown: include_str!("../../../docs/mcp-knowledge/playbook-ardop-wont-connect.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "playbook-cms-z-password-lag",
        title: "Playbook: CMS-Z password lag",
        markdown: include_str!("../../../docs/mcp-knowledge/playbook-cms-z-password-lag.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "audio-setup",
        title: "Audio setup",
        markdown: include_str!("../../../docs/mcp-knowledge/audio-setup.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "band-plan",
        title: "Band plan",
        markdown: include_str!("../../../docs/mcp-knowledge/band-plan.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "modem-capability-matrix",
        title: "Modem capability matrix",
        markdown: include_str!("../../../docs/mcp-knowledge/modem-capability-matrix.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "vara-wine-setup",
        title: "VARA under Wine setup",
        markdown: include_str!("../../../docs/mcp-knowledge/vara-wine-setup.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "device-uv-pro",
        title: "Device: UV-Pro",
        markdown: include_str!("../../../docs/mcp-knowledge/device-uv-pro.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "glossary-supplement",
        title: "Glossary supplement",
        markdown: include_str!("../../../docs/mcp-knowledge/glossary-supplement.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "local-agent-deployment",
        title: "Local agent deployment",
        markdown: include_str!("../../../docs/mcp-knowledge/local-agent-deployment.md"),
        source: DocSource::McpKnowledge,
    },
    DocTopic {
        slug: "agents-guide",
        title: "Agents guide",
        markdown: include_str!("../../../docs/mcp-knowledge/agents-guide.md"),
        source: DocSource::McpKnowledge,
    },
```

**Titles must match each file's actual `# H1`.** Verify before committing:

```bash
head -1 /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-aib3n-elmer-winlink-docs/docs/mcp-knowledge/*.md
```

Titles are an FTS-indexed column; a wrong title degrades retrieval.

- [ ] **Step 5: Verify the drift test now passes conceptually**

The Task 1 test walks all three dirs. After this task every `.md` in all three is registered. You cannot run `cargo test` locally — instead confirm by count:

```bash
WT=/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-aib3n-elmer-winlink-docs
echo "on disk:    $(ls $WT/docs/user-guide/*.md $WT/docs/knowledge/*.md $WT/docs/mcp-knowledge/*.md | wc -l)"
echo "registered: $(grep -c 'include_str!' $WT/src-tauri/src/search/docs_bundle.rs)"
```

Expected: the two numbers are **equal** (36 + 2 + 10 = 48). If they differ, a file is unregistered and the CI drift test will fail.

- [ ] **Step 6: Run the docs link linter**

New markdown with links must pass the pre-push hook:

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-aib3n-elmer-winlink-docs lint:docs`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add docs/knowledge/pat-winlink.md docs/knowledge/winlink-express.md \
        src-tauri/src/search/docs_bundle.rs
git commit -F - <<'EOF'
feat(docs): agent-only Pat + Winlink Express corpus; index mcp-knowledge for Elmer

Elmer's knowledge was Tuxlink-only, but in an incident the operator is often helping
someone on a DIFFERENT client. Adds docs/knowledge/ — agent-only reference indexed
into docs_fts but deliberately NOT in the Help sidebar, which keeps Tuxlink's manual
about Tuxlink (src/help/topics.ts globs user-guide only, so this needs no frontend
change).

Also indexes docs/mcp-knowledge/, which until now was reachable only over the MCP
resource tier that in-app Elmer never touches — so the ARDOP and CMS-Z playbooks were
readable by Claude Desktop and invisible to Elmer.

Pat syntax is verbatim from pat v1.0.0's connect --help on this machine. Note that
tuxlink-aib3n's own description had it wrong (ax25:///DIGI1,DIGI2/TARGET): hops are
slash-separated, not comma-separated. Writing the doc from the issue text would have
shipped a connect string that fails on the air.

Neither doc restates the Pat/WLE-vs-Tuxlink comparison; 32-from-express-or-pat owns
that, and two docs making the same comparison is how a BM25 corpus starts
contradicting itself.

Agent: sumac-magnolia-fen
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 7: Retrieval evals

**Files:**
- Create: `src-tauri/src/search/docs_eval_test.rs`
- Modify: `src-tauri/src/search/mod.rs`

**Interfaces:**
- Consumes: `BUNDLED_TOPICS`, `Index::populate_docs`, `Index::search_docs`, `Index::read_doc`.

Nothing today measures whether Elmer can *find* an answer. These assert retrievability against the real bundled corpus — no model in the loop.

- [ ] **Step 1: Write the eval tests**

Create `src-tauri/src/search/docs_eval_test.rs`:

```rust
//! Retrieval evals: can a real operator question reach the document that answers it?
//!
//! These run against the REAL bundled corpus (not fixtures) and assert only
//! retrievability — the thing that was broken. Answer quality with a model in the
//! loop stays in dev/elmer-distill/.
//!
//! Assertions are "the expected slug is among the hits", never "is rank 1". BM25
//! ordering is not a stable contract and tests that pin it are brittle.

use crate::search::docs_bundle::BUNDLED_TOPICS;
use crate::search::index::Index;
use tempfile::{tempdir, TempDir};

fn corpus() -> (TempDir, Index) {
    let dir = tempdir().unwrap();
    let idx = Index::open(dir.path().join("search.db")).unwrap();
    idx.populate_docs(BUNDLED_TOPICS).unwrap();
    (dir, idx)
}

fn slugs_for(idx: &Index, query: &str) -> Vec<String> {
    idx.search_docs(query)
        .unwrap()
        .into_iter()
        .map(|h| h.slug)
        .collect()
}

/// Eval 1 — the motivating question (KJ4UYO, verbatim in tuxlink-aib3n):
/// "What is the syntax for Pat Winlink in EmComm Tools in ax.25 to connect via
/// a digipeater?" Search must reach the doc, AND the doc must actually carry the
/// syntax — a hit whose body lacks the connect string is a hit the model cannot
/// answer from.
#[test]
fn eval_pat_ax25_digipeater_syntax_is_retrievable() {
    let (_dir, idx) = corpus();

    let hits = slugs_for(&idx, "pat ax25 digipeater connect");
    assert!(
        hits.iter().any(|s| s == "pat-winlink"),
        "docs_search could not reach pat-winlink; got {hits:?}"
    );

    let doc = idx.read_doc("pat-winlink").unwrap().expect("pat-winlink is indexed");
    assert!(
        doc.body.contains("ax25:///"),
        "pat-winlink does not contain the ax25:/// connect form — an operator would \
         get a confident answer with no syntax in it"
    );
    // The digipeater path is slash-separated. tuxlink-aib3n's description claimed
    // commas; that is wrong and must never reappear in the corpus.
    assert!(
        !doc.body.contains("ax25:///DIGI1,DIGI2"),
        "comma-separated digipeater path found — hops are separated by '/'"
    );
}

/// Eval 2 — P0 tuxlink-0mudm's original symptom: the model invented
/// "~/.config/tuxlink/tuxlink.cfg base64/mode 600" when the truth is the OS keyring.
#[test]
fn eval_credential_storage_is_retrievable() {
    let (_dir, idx) = corpus();
    let hits = slugs_for(&idx, "password credential storage keyring");
    assert!(
        hits.iter().any(|s| s == "27-settings" || s == "02-first-launch-wizard"),
        "no credential-storage doc retrievable; got {hits:?}"
    );
}

/// Eval 3 — the playbooks were invisible to in-app Elmer before this work
/// (resource-tier only). Indexing docs/mcp-knowledge is what fixes that.
#[test]
fn eval_ardop_playbook_is_retrievable() {
    let (_dir, idx) = corpus();
    let hits = slugs_for(&idx, "ardop will not connect troubleshooting");
    assert!(
        hits.iter().any(|s| s == "playbook-ardop-wont-connect"),
        "ARDOP playbook not retrievable; got {hits:?}"
    );
}

/// Eval 4 — the Winlink Express analogue of the Pat question.
#[test]
fn eval_winlink_express_packet_path_is_retrievable() {
    let (_dir, idx) = corpus();
    let hits = slugs_for(&idx, "winlink express packet session digipeater path");
    assert!(
        hits.iter().any(|s| s == "winlink-express"),
        "winlink-express not retrievable; got {hits:?}"
    );
}

/// Eval 5 — the conflation guard. "Operating Pat" and "migrating from Pat" are
/// different questions with different documents; both must be reachable.
#[test]
fn eval_migration_topic_is_distinct_from_the_operational_doc() {
    let (_dir, idx) = corpus();
    let hits = slugs_for(&idx, "switching to tuxlink from pat what changes");
    assert!(
        hits.iter().any(|s| s == "32-from-express-or-pat"),
        "migration topic not retrievable; got {hits:?}"
    );
}

/// Every registered slug is readable. Guards the populate→read round trip across
/// the whole real corpus.
#[test]
fn every_registered_slug_is_readable() {
    let (_dir, idx) = corpus();
    for t in BUNDLED_TOPICS {
        let doc = idx
            .read_doc(t.slug)
            .unwrap()
            .unwrap_or_else(|| panic!("registered slug {} is not readable", t.slug));
        assert!(!doc.body.trim().is_empty(), "{} has an empty body", t.slug);
    }
}
```

- [ ] **Step 2: Wire the module**

In `src-tauri/src/search/mod.rs`:

```rust
#[cfg(test)]
mod docs_eval_test;
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/search/docs_eval_test.rs src-tauri/src/search/mod.rs
git commit -F - <<'EOF'
test(search): retrieval evals — can an operator question reach its answer?

Nothing measured document retrieval before this: dev/elmer-distill judges tool-use
trajectories, so neither the 12-token-snippet ceiling nor the orphaned space-weather
topic would ever have failed a test.

Six evals against the REAL bundled corpus. Eval 1 is the motivating KJ4UYO question
and asserts both that pat-winlink is reachable AND that its body actually carries the
ax25:/// form — plus a negative assertion that the comma-separated path from the bd
issue never reappears. Eval 2 is P0 tuxlink-0mudm's original symptom.

Assertions are membership, not rank: BM25 ordering is not a stable contract.

Agent: sumac-magnolia-fen
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 8: Push, CI, and green build

Rust cannot be compiled on this Pi. CI is the compiler.

- [ ] **Step 1: Push the branch**

```bash
git push -u origin bd-tuxlink-aib3n/elmer-winlink-docs
```

- [ ] **Step 2: Open a draft PR**

```bash
gh pr create --draft --base main \
  --head bd-tuxlink-aib3n/elmer-winlink-docs \
  --title "[sumac-magnolia-fen] feat(elmer): knowledge tier — docs_read + Pat/WLE corpus" \
  --body "$(cat <<'EOF'
Closes `tuxlink-aib3n`. Closes the tool + prompt substance of P0 `tuxlink-0mudm`.

## The finding

Elmer could **search** documentation but not **read** it. `docs_search` returned a
12-token `snippet()` window and no tool could turn its slug into the document, so it
was a locator with no destination — and `docs/mcp-knowledge/` was exposed only over
the MCP *resource* tier, which in-app Elmer never lists or reads. The model therefore
answered product questions from nothing, which is exactly the confabulation recorded
in `tuxlink-0mudm`.

Writing new Pat/WLE docs onto that tier would have reproduced the bug with more
source material behind it.

## What changed

- **`docs_read(slug)`** — serves the `body` column already stored in `docs_fts`.
- **One index, three sources** — `docs/user-guide/` + new agent-only `docs/knowledge/`
  + `docs/mcp-knowledge/` (now Elmer-visible; the ARDOP/CMS-Z playbooks were
  Claude-Desktop-readable and Elmer-invisible).
- **Tool descriptions carry the search→read protocol**, because the consuming model
  (Qwen3-Coder-Next) has no domain knowledge and tool schemas are the one context the
  runner always presents.
- **System prompt grounding clause** (the prompt half of the P0).
- **`DocsHitDto.path` → `slug`** so `docs_search`'s output key matches `docs_read`'s
  input key.
- **Registry drift test** — `36-off-air-space-weather.md` was on disk, absent from
  `BUNDLED_TOPICS`, and therefore unsearchable. Now registered, with a test that
  crosses the filesystem/registry boundary (the old `len == CATALOG.len()` assertion
  is a tautology).
- **Six retrieval evals**, #1 being the verbatim KJ4UYO question.

## Accuracy note

`tuxlink-aib3n`'s own description gives Pat's digipeater form as
`ax25:///DIGI1,DIGI2/TARGET`. **That is wrong.** Verified against `pat v1.0.0`'s
`connect --help` on the dev Pi, hops are slash-separated:

```
connect ax25:///LA1B/LA5NTA    Peer-to-peer connection with LA5NTA via LA1B digipeater.
```

An eval asserts the comma form never reappears in the corpus.

## Verification provenance

Rust compiled and tested **by CI only** (amd64 + arm64) — this dev Pi cannot finish a
cold `cargo` build. `pnpm lint:docs` run locally. No frontend change (`topics.ts` globs
`docs/user-guide/` only, so the agent-only corpus stays out of the Help sidebar).
Not yet exercised against a live model; the operator plans a Qwen3-Coder-Next run.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Watch CI and fix by SHA**

```bash
gh pr checks --watch
```

Verify the run's `headSha` matches your pushed commit — a bare watch can report a stale run.

Expected failures to triage first, since none can be caught locally:
- **Missed `DocsHitDto` construction sites** — `path:` still used somewhere (Task 3, Step 5).
- **`DocTopic` literals missing `source:`** — especially the pre-existing tests inside `docs_index.rs`.
- **MSRV 1.75** — clippy `incompatible_msrv` on any 1.76+ API.
- **Clippy `-D warnings`** — unused imports after the rename.
- **`--locked`** — a stale `Cargo.lock` if `serde_json` had to be added (Task 4, Step 3).

Fix forward with new commits (amending pushed commits is banned). Re-check by SHA.

---

### Task 9: `wire-walk` gate, then close

`wire-walk` is a **hard gate** before any "done"/"shipped" claim or closing the issue. Registration is not a caller; CI-green is not reachability.

- [ ] **Step 1: Run the wire-walk skill**

Invoke `.claude/skills/wire-walk/`. The **operator supplies the flows greenfield** — do not draft them; anchoring launders your own blind spots.

The seam this feature can plausibly break: a tool that is *registered* on the router but never *reaches Elmer's tool list*, or a doc that is *on disk* but never *in the index*. Trace to `file:line`:
1. Operator asks Elmer the KJ4UYO question →
2. `ELMER_SYSTEM_PROMPT` tells it to search →
3. `list_tools_as_specs` includes `docs_read` →
4. `docs_search` returns slug `pat-winlink` →
5. `docs_read("pat-winlink")` → `SearchPort::doc` → `Index::read_doc` →
6. full body, containing `ax25:///`, reaches the model.

Any broken link means the feature is **not shipped**.

- [ ] **Step 2: Mark the PR ready and merge**

```bash
gh pr ready <PR#>
gh pr merge <PR#> --merge
```

No squash (ADR 0010). Do **not** pass `--auto` (it merges immediately in this repo) and avoid `--delete-branch`.

- [ ] **Step 3: Close the issues**

```bash
bd close tuxlink-aib3n
```

For `tuxlink-0mudm`, close only if wire-walk confirms the grounding path end-to-end; otherwise update it with what shipped (tool + prompt) and what remains (gold-gen product/help question family, which is `elmer-distill` work and out of scope here).

```bash
bd update tuxlink-0mudm --notes "Tool half (docs_read) + prompt half (grounding clause) shipped in PR <#>. Remaining: gold-gen product/help question family in dev/elmer-distill."
```

- [ ] **Step 4: Dispose of the worktree** per the ADR 0009 ritual (`git worktree remove` is hook-banned).

---

## Self-Review

**Spec coverage:**

| Spec requirement | Task |
|---|---|
| `docs_read(slug)` tool | 2, 3, 4 |
| One index, three source dirs | 1, 6 |
| Index `docs/mcp-knowledge/` for Elmer | 6 |
| Agent-only `docs/knowledge/`, not in sidebar | 6 (no frontend change needed) |
| Tool descriptions carry the two-step protocol | 4 |
| System-prompt grounding clause | 5 |
| `pat-winlink.md` + `winlink-express.md` | 6 |
| Cross-link `32-from-express-or-pat`, no duplicate comparison | 6 |
| Registry drift guard | 1 |
| Register orphaned `36-off-air-space-weather` | 1 |
| Retrieval evals incl. KJ4UYO #1 | 7 |
| `docs_read` unknown-slug returns steering hint | 4 |
| Read-only, non-tainting | 4 |
| Resource tier untouched | (no task — verified by omission) |

No gaps.

**Placeholder scan:** none. The two genuine unknowns (Pat multi-hop exemplification, ETC packaging) are Task 6 Step 1 verification *actions* with an explicit "do not guess" instruction and a stated fallback, not deferred decisions.

**Type consistency:** `DocSource` (Task 1) → used in Tasks 2, 6. `DocTopic.source` (Task 1) → every literal in Tasks 1, 2, 6. `DocBody{slug,title,body}` (Task 2) → `DocBodyDto{slug,title,body}` (Task 3) → `docs_read` (Task 4). `DocsHitDto.slug` (Task 3) → `SlugParams.slug` (Task 4) → eval `h.slug` (Task 7). `Index::read_doc → Result<Option<DocBody>, IndexError>` matches `SearchPort::doc → Result<Option<DocBodyDto>, PortError>`. Consistent.

One known compile hazard is called out where it bites: Task 1 adds a field to `DocTopic`, which breaks **every existing `DocTopic` literal in `docs_index.rs`'s own test module**. Task 2 Step 1 states this explicitly.
