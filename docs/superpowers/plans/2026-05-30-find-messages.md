# Find-messages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship v0.1 capability 1.15 (find-messages) — full-text search over the filesystem-canonical mailbox via a derived SQLite FTS5 index, ribbon-mounted search bar with combined saved + recent dropdown, always-visible filter-chip strip, and a Settings panel for saved-search management.

**Architecture:** New Rust module `src-tauri/src/search/` owns the FTS5 schema, the extractor, the query path, and a JSON-backed saved/recent store. `Mailbox::store`/`move_to`/`mark_read` gain optional in-process hooks into the index (synchronous upsert; mailbox writes never fail because of index errors). New React directory `src/search/` owns `SearchBar`, `SearchDropdown`, `ChipStrip`, `SavedSearchesPanel` plus `useSearch` / `useSavedSearches` hooks; `AppShell.tsx`/`AppShell.css` grow a chip-strip row and the existing ribbon hosts `<SearchBar />`. The mailbox is **filesystem-canonical**; the FTS5 store is regenerable via an explicit `rebuild-index` command surfaced both via the Tauri command surface and the Settings panel.

**Tech Stack:** Rust (`src-tauri` crate) + new dep `rusqlite = "0.31"` with features `["bundled", "modern_sqlite"]` (gets FTS5 via bundled SQLite ≥ 3.9). React 19 + TypeScript + `@tauri-apps/api invoke` + `@tanstack/react-query` for command results + Vitest + @testing-library/react (jsdom; `globals: false` — every test file imports `describe/it/expect/vi` from `vitest`).

**Authority for design:** [`docs/design/2026-05-30-find-messages-design.md`](../../design/2026-05-30-find-messages-design.md). Each task names the spec section it implements; when implementation drifts from the spec, update the spec in the same PR.

**Run commands (verified):**
- **Rust:** `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::` (absolute manifest path per the worktree path-pinning convention).
- **TS:** `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages vitest run <path>` (no `test` npm script — invoke `vitest` directly through pnpm's binary resolution).
- **Type check:** `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages exec tsc --noEmit`.

**bd issue:** [`tuxlink-1hu`](https://github.com/cameronzucker/tuxlink) (claimed by this worktree).

**Branch:** `bd-tuxlink-1hu/find-messages` (already pushed). Open the PR against `main` after the integration smoke (Task 21).

**Commit trailer:** every commit ends with `Agent: <session moniker>` + `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`. The session moniker is set per agent in the execution session — substitute it verbatim in each task's commit step.

---

## File map

**New Rust (`src-tauri/src/search/`):**
- `mod.rs` — public re-exports (`Index`, `QuerySpec`, `SavedSearch`, etc.).
- `index.rs` — FTS5 schema, `Index::open`/`init`/`migrate`, `upsert`/`delete`/`update_folder`/`update_unread`, `query`.
- `extractor.rs` — `extract(&Message, MailboxFolder, Direction) -> IndexRow` pulling every conservative-max field.
- `query.rs` — `QuerySpec` + serde + chip-to-SQL composition; `parse_fts_query` for the free-text portion.
- `saved.rs` — JSON store (`$APPCONFIG/saved-searches.json`); load / save / promote / unsave / rename / reorder / prune-recent.
- `commands.rs` — Tauri command handlers (`search_run`, `search_save`, `search_list_saved`, `search_list_recent`, `search_unsave`, `search_rename`, `search_reorder`, `search_rebuild_index`).
- `types.rs` — DTOs mirroring the TS side (`QuerySpec`, `FilterKey`, `FilterValue`, `SortOrder`, `PageRequest`, `SearchResults`, `SavedSearch`, `RecentSearch`, `RebuildStats`).

**Modified Rust:**
- `src-tauri/Cargo.toml` — add `rusqlite` + `uuid` deps.
- `src-tauri/src/lib.rs` (or `main.rs` — verify in Task 1) — register the new commands, construct the `Index` handle, wire it into the `Mailbox`.
- `src-tauri/src/native_mailbox.rs` — extend `Mailbox` with `index: Option<Arc<Index>>`; hook `store`/`move_to`/`mark_read`.

**New React (`src/search/`):**
- `types.ts` — TS mirrors of `QuerySpec`, `FilterKey`, `FilterValue`, `SavedSearch`, `RecentSearch`.
- `queryRender.ts` — render `QuerySpec` to a human-readable preview string (`"from:KX5DD date:7d"`).
- `useSearch.ts` — owns the canonical `QuerySpec` + debounced `search_run` + result state.
- `useSavedSearches.ts` — saved + recent list + promote / unsave / rename / reorder / new-from-spec.
- `SearchBar.tsx` — ribbon-mounted input; shows saved-name when active; chevron opens dropdown; ⌘F focuses.
- `SearchDropdown.tsx` — saved + recent sections; keyboard nav; footer `Manage… ⚙`.
- `ChipStrip.tsx` — below-ribbon strip; active chips + ghost chips + far-right meta.
- `SavedSearchesPanel.tsx` — Settings tab: list, drag-reorder, inline edit, "+ New", "Rebuild index" button.

**Modified React:**
- `src/shell/AppShell.tsx` — wire `<SearchBar />` into the ribbon (left slot) and `<ChipStrip />` directly under the ribbon.
- `src/shell/AppShell.css` — grow `grid-template-rows` by one `auto` (the chip-strip row); shift the dash-items cluster right.
- `src/mailbox/MessageList.tsx` — add optional `matchHighlights` + `showFolderTag` props (additive; absent → renders exactly as today).
- `src/mailbox/types.ts` — extend `MessageMeta` with optional `matchHighlights?` (per-row) and an optional `folder` tag (for cross-folder result rendering).
- Settings panel — add a "Saved Searches" tab routing entry (path will be verified in Task 18 against the actual Settings panel structure).

---

### Task 1: Scaffold the `search` module + add `rusqlite` dependency

**Files:**
- Create: `src-tauri/src/search/mod.rs`
- Create: `src-tauri/src/search/index.rs`, `extractor.rs`, `query.rs`, `saved.rs`, `commands.rs`, `types.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs` (or `main.rs` — verify which one declares `mod` lines for sibling modules)

- [ ] **Step 1: Confirm where sibling modules are declared**

Run: `grep -rE '^pub mod (native_mailbox|winlink_backend)' /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/src/*.rs`
Expected: identifies whether the project uses `lib.rs` or `main.rs` as the module-declaration root. Subsequent steps use whichever file the grep returns.

- [ ] **Step 2: Write the failing smoke test**

In `src-tauri/src/search/mod.rs`:
```rust
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

#[cfg(test)]
mod module_smoke {
    #[test]
    fn module_is_wired() {
        assert_eq!(2 + 2, 4);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::`
Expected: FAIL to compile — child modules don't exist; `search` not declared at the crate root.

- [ ] **Step 4: Write minimal implementation**

Create each child file with a one-line placeholder:
```rust
//! placeholder — see plan task N
```

In whichever root file declares sibling modules (`lib.rs` or `main.rs` per Step 1), add alongside the other `pub mod` lines:
```rust
pub mod search;
```

In `src-tauri/Cargo.toml` under `[dependencies]`, add:
```toml
rusqlite = { version = "0.31", features = ["bundled", "modern_sqlite"] }
uuid = { version = "1.8", features = ["v4", "serde"] }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::`
Expected: PASS (`module_is_wired`).

- [ ] **Step 6: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/search/ src-tauri/src/lib.rs src-tauri/src/main.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): scaffold search module + add rusqlite (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Index — open, schema, migrate

**Spec:** §5.1 (storage location), §5.2 (schema).

**Files:**
- Modify: `src-tauri/src/search/index.rs`
- Test: same file, `#[cfg(test)] mod tests`.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/search/index.rs`:
```rust
use rusqlite::Connection;
use std::path::PathBuf;
use thiserror::Error;

/// Schema version. Bumped when the table layout changes; `Index::open` detects
/// drift and the caller can trigger a rebuild.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("schema drift: index is at v{found}, current is v{current}")]
    SchemaDrift { found: u32, current: u32 },
}

pub struct Index {
    conn: Connection,
}

impl Index {
    /// Open or create the index at `path`. If the file does not exist, the
    /// schema is created. If it exists but is at an older `user_version`,
    /// returns `Err(IndexError::SchemaDrift)` — caller (e.g. rebuild-index)
    /// decides whether to recreate.
    pub fn open(_path: PathBuf) -> Result<Self, IndexError> {
        unimplemented!()
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
            IndexError::SchemaDrift { found: 0, current: 1 } => {}
            other => panic!("expected SchemaDrift {{ found: 0, current: 1 }}, got {other:?}"),
        }
    }
}
```

Also: add `thiserror = "1"` and `tempfile = "3"` (dev-deps) to `src-tauri/Cargo.toml` if not already present.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::index::`
Expected: FAIL — `Index::open` unimplemented.

- [ ] **Step 3: Write minimal implementation**

Replace `Index::open`:
```rust
impl Index {
    pub fn open(path: PathBuf) -> Result<Self, IndexError> {
        let conn = Connection::open(&path)?;
        let found: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if found == 0 {
            Self::init_schema(&conn)?;
        } else if found != SCHEMA_VERSION {
            return Err(IndexError::SchemaDrift { found, current: SCHEMA_VERSION });
        }
        Ok(Self { conn })
    }

    fn init_schema(conn: &Connection) -> Result<(), IndexError> {
        conn.execute_batch(
            r#"
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
            "#,
        )?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::index::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/search/index.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): FTS5 schema + Index::open with drift detection (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Types module — `QuerySpec`, `FilterKey`, `FilterValue`, etc.

**Spec:** §6.4 (Tauri command surface — DTO shapes).

**Files:**
- Modify: `src-tauri/src/search/types.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/search/types.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilterKey {
    Folder,
    From,
    To,
    DateRange,
    FormType,
    HasForm,
    HasAttach,
    ReadState,
    Transport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum FilterValue {
    /// Folder filter: "inbox" | "outbox" | "sent" | "archive" | "all"
    Folder(String),
    /// Free-form address glob, e.g. "KX5DD" or "*@KX5DD".
    Addr(String),
    /// Date range, both bounds optional (unix epoch seconds, UTC).
    DateRange { from: Option<i64>, to: Option<i64> },
    /// Form-type id, e.g. "ICS-213". Empty string never appears (use chip omission instead).
    FormType(String),
    /// Boolean toggle (`has-form`, `has-attach`).
    Bool(bool),
    /// Read-state tri-state mapped to two-state at the chip layer (only `Read` or `Unread`).
    ReadState(ReadState),
    /// Transport id, e.g. "telnet" | "packet" | "vara-hf" | "vara-fm" | "ardop".
    Transport(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadState { Read, Unread }

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    DateDesc,
    DateAsc,
}

impl Default for SortOrder {
    fn default() -> Self { SortOrder::DateDesc }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRequest {
    pub page_size: u32,
    pub offset: u32,
}

impl Default for PageRequest {
    fn default() -> Self { Self { page_size: 200, offset: 0 } }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct QuerySpec {
    /// Free-text portion, mapped to FTS5 `MATCH`. `None` → no FTS clause.
    pub free_text: Option<String>,
    /// Active chip state, keyed by `FilterKey` (BTreeMap so command serialization is deterministic).
    pub filters: BTreeMap<FilterKey, FilterValue>,
    pub sort: SortOrder,
    pub page: PageRequest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn querySpec_serde_roundtrip_for_typical_active_query() {
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::From, FilterValue::Addr("KX5DD".into()));
        filters.insert(
            FilterKey::DateRange,
            FilterValue::DateRange { from: Some(1_700_000_000), to: None },
        );
        filters.insert(FilterKey::FormType, FilterValue::FormType("ICS-213".into()));

        let spec = QuerySpec {
            free_text: Some("damage".into()),
            filters,
            sort: SortOrder::DateDesc,
            page: PageRequest::default(),
        };

        let json = serde_json::to_string(&spec).unwrap();
        let back: QuerySpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn filterValue_kind_tag_matches_kebab_case_keys() {
        let v = FilterValue::Addr("KX5DD".into());
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains(r#""kind":"addr""#), "got {json}");
        assert!(json.contains(r#""value":"KX5DD""#), "got {json}");
    }
}
```

Also add to `src-tauri/Cargo.toml` if not already present: `thiserror = "1"`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::types::`
Expected: FAIL (probably compile error: identifier-naming lint or missing serde derive). Confirm the test compiles after you write the types.

- [ ] **Step 3: Verify the implementation already passes**

The implementation IS the type definitions written in Step 1. No additional code change needed — re-run:
Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::types::`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/src/search/types.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): QuerySpec, FilterKey, FilterValue, SortOrder types (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Extractor — `Message → IndexRow`

**Spec:** §5.2 (schema fields), Q4 (conservative-max extraction surface).

**Files:**
- Modify: `src-tauri/src/search/extractor.rs`

- [ ] **Step 1: Write the `IndexRow` struct + failing test**

In `src-tauri/src/search/extractor.rs`:
```rust
use crate::winlink::message::Message;
use crate::winlink_backend::MailboxFolder;

/// One row's worth of extracted fields — both FTS5 (`subject`/`body`/`form_field_values`)
/// and structured (`messages_meta`). The Index::upsert call writes both tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    // FTS5 columns
    pub mid: String,
    pub folder: String,                 // also a meta column; UNINDEXED in FTS table
    pub subject: String,                // FTS-indexed
    pub body: String,                   // FTS-indexed (decoded text/plain)
    pub form_field_values: String,      // FTS-indexed (concatenation; empty if not a form)

    // messages_meta columns
    pub from_addr: Option<String>,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub date_sent: Option<i64>,         // unix seconds UTC
    pub date_received: Option<i64>,     // unix seconds UTC
    pub unread: bool,
    pub form_type: Option<String>,
    pub has_attachments: bool,
    pub attachment_count: u32,
    pub transport_used: Option<String>,
    pub direction: Direction,
    pub message_size: u32,
    pub routing_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { Sent, Received }

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self { Direction::Sent => "sent", Direction::Received => "received" }
    }
}

/// Extract one IndexRow from a parsed Message + the folder it lives in. The
/// caller supplies `direction` because some folders (e.g. Archive) host either.
/// `unread` is supplied explicitly because read-state is a sidecar file the
/// extractor cannot see (it lives on the mailbox layer).
pub fn extract(
    _msg: &Message,
    _folder: MailboxFolder,
    _direction: Direction,
    _unread: bool,
    _transport_used: Option<String>,
) -> IndexRow {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::compose::compose_message;

    fn parse(raw: &[u8]) -> Message {
        Message::from_bytes(raw).expect("fixture parses")
    }

    #[test]
    fn extracts_headers_from_a_plain_message() {
        let raw = compose_message(
            "N7CPZ", &["W1AW"], &[],
            "Hello", "Body text", 1_716_200_000,
        ).to_bytes();
        let msg = parse(&raw);
        let row = extract(
            &msg, MailboxFolder::Inbox, Direction::Received,
            /*unread=*/ true, /*transport_used=*/ Some("telnet".into()),
        );
        assert_eq!(row.subject, "Hello");
        assert_eq!(row.from_addr.as_deref(), Some("N7CPZ"));
        assert_eq!(row.to_addrs, vec!["W1AW".to_string()]);
        assert!(row.cc_addrs.is_empty());
        assert_eq!(row.body.trim(), "Body text");
        assert_eq!(row.folder, "inbox");
        assert!(row.unread);
        assert_eq!(row.transport_used.as_deref(), Some("telnet"));
        assert_eq!(row.direction, Direction::Received);
        assert_eq!(row.has_attachments, false);
        assert_eq!(row.attachment_count, 0);
        assert_eq!(row.form_type, None);
        assert_eq!(row.form_field_values, "");
        assert!(row.message_size > 0);
        assert_eq!(row.mid.is_empty(), false);
    }

    #[test]
    fn extracts_form_payload_into_form_field_values() {
        // ICS-213-shaped fixture body. The extractor concatenates form-field
        // values into `form_field_values` for FTS5; the exact shape of "form
        // detection" is up to the implementer (header sniff + body sniff).
        let raw = compose_message(
            "KX5DD", &["N7CPZ"], &[],
            "DAMAGE REPORT",
            "FORM: ICS-213\nTO: Net Control\nFROM: KX5DD\nSUBJECT: Sector 7\nMSG: poles down\n",
            1_716_200_000,
        ).to_bytes();
        let msg = parse(&raw);
        let row = extract(&msg, MailboxFolder::Inbox, Direction::Received, true, None);
        assert_eq!(row.form_type.as_deref(), Some("ICS-213"));
        assert!(row.form_field_values.contains("Net Control"));
        assert!(row.form_field_values.contains("KX5DD"));
        assert!(row.form_field_values.contains("Sector 7"));
        assert!(row.form_field_values.contains("poles down"));
    }

    #[test]
    fn date_received_set_for_received_direction_only() {
        let raw = compose_message(
            "N7CPZ", &["W1AW"], &[],
            "x", "y", 1_716_200_000,
        ).to_bytes();
        let msg = parse(&raw);
        let recv = extract(&msg, MailboxFolder::Inbox, Direction::Received, true, None);
        let sent = extract(&msg, MailboxFolder::Sent, Direction::Sent, false, None);
        assert!(recv.date_received.is_some());
        assert!(sent.date_sent.is_some());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::extractor::`
Expected: FAIL — `extract` unimplemented.

- [ ] **Step 3: Write minimal implementation**

Replace `extract`:
```rust
pub fn extract(
    msg: &Message,
    folder: MailboxFolder,
    direction: Direction,
    unread: bool,
    transport_used: Option<String>,
) -> IndexRow {
    let mid = msg.header("Mid").unwrap_or_default().to_string();
    let subject = msg.header("Subject").unwrap_or_default().to_string();
    let from_addr = msg.header("From").map(|s| s.to_string());
    let to_addrs: Vec<String> = msg.header_all("To").iter().map(|s| s.to_string()).collect();
    let cc_addrs: Vec<String> = msg.header_all("Cc").iter().map(|s| s.to_string()).collect();
    let body = msg.body().to_string();

    let folder_str = match folder {
        MailboxFolder::Inbox => "inbox",
        MailboxFolder::Outbox => "outbox",
        MailboxFolder::Sent => "sent",
        MailboxFolder::Archive => "archive",
    }
    .to_string();

    let date_unix = parse_winlink_date(msg.header("Date").unwrap_or_default());
    let (date_sent, date_received) = match direction {
        Direction::Sent => (date_unix, None),
        Direction::Received => (None, date_unix),
    };

    let (form_type, form_field_values) = sniff_form(&subject, &body);

    let attachment_count = msg.header_all("File").len() as u32;
    let has_attachments = attachment_count > 0;

    let routing_path = msg.header("Via").map(|s| s.to_string());
    let message_size = body.len() as u32;

    IndexRow {
        mid, folder: folder_str,
        subject, body, form_field_values,
        from_addr, to_addrs, cc_addrs,
        date_sent, date_received,
        unread,
        form_type,
        has_attachments, attachment_count,
        transport_used,
        direction,
        message_size,
        routing_path,
    }
}

/// Parse Winlink `YYYY/MM/DD HH:MM` into Unix seconds UTC. Returns None on any
/// shape that does not match.
fn parse_winlink_date(s: &str) -> Option<i64> {
    let (d, t) = s.split_once(' ')?;
    let parts: Vec<&str> = d.split('/').collect();
    if parts.len() != 3 { return None; }
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    let (h, m) = t.split_once(':')?;
    let hour: u32 = h.parse().ok()?;
    let minute: u32 = m.parse().ok()?;
    // Days since 1970-01-01 — simple Gregorian calc; correct for 1970..2100.
    let days = days_from_civil(year, month as i32, day as i32);
    Some(days as i64 * 86_400 + hour as i64 * 3600 + minute as i64 * 60)
}

/// Howard Hinnant's `days_from_civil` (public-domain reference algorithm).
fn days_from_civil(y: i32, m: i32, d: i32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u32;
    let doy = ((153 * (m + (if m > 2 { -3 } else { 9 })) + 2) / 5 + d - 1) as u32;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i32 - 719_468
}

/// Cheap form-payload sniff. v0.1 recognizes payloads whose body or subject
/// begins with `FORM: <id>` — the convention emitted by ICS-213 forms in the
/// Winlink ecosystem. Returns (form_type, concatenated_values).
fn sniff_form(subject: &str, body: &str) -> (Option<String>, String) {
    // Body wins over subject. Look for "FORM: <id>\n" on its own line.
    let form_type = body
        .lines()
        .find_map(|l| l.strip_prefix("FORM:").map(|rest| rest.trim().to_string()))
        .or_else(|| {
            subject
                .strip_prefix("FORM: ")
                .map(|s| s.trim().to_string())
        });
    if form_type.is_none() {
        return (None, String::new());
    }

    // Concatenate every "Key: value" payload line for FTS indexing. Skip the
    // FORM: line itself.
    let values: Vec<String> = body
        .lines()
        .filter(|l| !l.starts_with("FORM:"))
        .filter_map(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
        .filter(|v| !v.is_empty())
        .collect();

    (form_type, values.join(" \u{2503} "))    // U+2503 separator unlikely to occur in payload text
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::extractor::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/src/search/extractor.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): extractor — Message → IndexRow with form-payload sniff (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Index — upsert / delete / update_folder / update_unread

**Spec:** §6.2 (mailbox hooks), §5.2 (schema).

**Files:**
- Modify: `src-tauri/src/search/index.rs`

- [ ] **Step 1: Write failing tests**

Append to `src-tauri/src/search/index.rs`:
```rust
use crate::search::extractor::{Direction, IndexRow};

impl Index {
    /// Insert-or-replace `row` in both `messages_fts` and `messages_meta`.
    pub fn upsert(&self, _row: &IndexRow) -> Result<(), IndexError> {
        unimplemented!()
    }

    pub fn delete(&self, _mid: &str) -> Result<(), IndexError> {
        unimplemented!()
    }

    pub fn update_folder(&self, _mid: &str, _new_folder: &str) -> Result<(), IndexError> {
        unimplemented!()
    }

    pub fn update_unread(&self, _mid: &str, _unread: bool) -> Result<(), IndexError> {
        unimplemented!()
    }

    /// Count rows in `messages_meta` — for tests and `RebuildStats`.
    pub fn count(&self) -> Result<u32, IndexError> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM messages_meta", [], |r| r.get(0))?;
        Ok(n as u32)
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::index::mutation_tests`
Expected: FAIL — `upsert`/`delete`/`update_folder`/`update_unread` unimplemented.

- [ ] **Step 3: Write minimal implementation**

Replace the `impl Index` block's four `unimplemented!()` bodies:
```rust
impl Index {
    pub fn upsert(&self, row: &IndexRow) -> Result<(), IndexError> {
        let tx = self.conn.unchecked_transaction()?;
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
                mid, folder, from_addr, to_addrs, cc_addrs,
                date_sent, date_received, unread,
                form_type, has_attachments, attachment_count,
                transport_used, direction, message_size, routing_path, indexed_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, strftime('%s','now')
             )
             ON CONFLICT(mid) DO UPDATE SET
                folder = excluded.folder,
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
                row.mid, row.folder,
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
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::index::`
Expected: PASS (all 3 schema tests from Task 2 + 4 mutation tests).

- [ ] **Step 5: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/src/search/index.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): Index upsert/delete/update_folder/update_unread (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Query — `QuerySpec` → SQL composition + `Index::query`

**Spec:** §4 (chip vocabulary), §5.2 (schema), §6.4 (`search_run`).

**Files:**
- Modify: `src-tauri/src/search/query.rs`
- Modify: `src-tauri/src/search/index.rs` (add the `query` method)

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/search/query.rs`:
```rust
use crate::search::types::{FilterKey, FilterValue, QuerySpec, ReadState, SortOrder};

/// Compose a `QuerySpec` into `(sql, params)`. The SQL joins `messages_fts`
/// to `messages_meta` only when free-text is present; otherwise scans
/// `messages_meta` directly.
pub fn compose(_spec: &QuerySpec) -> (String, Vec<SqlParam>) {
    unimplemented!()
}

#[derive(Debug, Clone, PartialEq)]
pub enum SqlParam {
    Text(String),
    Int(i64),
    Null,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn empty_spec() -> QuerySpec { QuerySpec::default() }

    #[test]
    fn compose_no_filters_no_freetext_lists_all() {
        let spec = empty_spec();
        let (sql, params) = compose(&spec);
        assert!(sql.contains("FROM messages_meta"), "got: {sql}");
        assert!(!sql.contains("MATCH"), "got: {sql}");
        assert!(params.is_empty());
        assert!(sql.contains("ORDER BY"));
    }

    #[test]
    fn compose_freetext_joins_fts() {
        let spec = QuerySpec { free_text: Some("damage".into()), ..QuerySpec::default() };
        let (sql, params) = compose(&spec);
        assert!(sql.contains("messages_fts MATCH"), "got: {sql}");
        assert!(params.iter().any(|p| matches!(p, SqlParam::Text(s) if s == "damage")));
    }

    #[test]
    fn compose_from_chip_adds_where_clause() {
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::From, FilterValue::Addr("KX5DD".into()));
        let spec = QuerySpec { filters, ..QuerySpec::default() };
        let (sql, params) = compose(&spec);
        assert!(sql.contains("from_addr LIKE"), "got: {sql}");
        assert!(params.iter().any(|p| matches!(p, SqlParam::Text(s) if s == "%KX5DD%")));
    }

    #[test]
    fn compose_folder_all_does_not_filter() {
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::Folder, FilterValue::Folder("all".into()));
        let spec = QuerySpec { filters, ..QuerySpec::default() };
        let (sql, _params) = compose(&spec);
        assert!(!sql.contains("m.folder ="), "FOLDER:all should not constrain: {sql}");
    }

    #[test]
    fn compose_combined_chip_set_emits_all_clauses() {
        let mut filters = BTreeMap::new();
        filters.insert(FilterKey::From, FilterValue::Addr("KX5DD".into()));
        filters.insert(FilterKey::FormType, FilterValue::FormType("ICS-213".into()));
        filters.insert(FilterKey::ReadState, FilterValue::ReadState(ReadState::Unread));
        filters.insert(FilterKey::HasAttach, FilterValue::Bool(true));
        filters.insert(
            FilterKey::DateRange,
            FilterValue::DateRange { from: Some(1_700_000_000), to: Some(1_710_000_000) },
        );
        filters.insert(FilterKey::Transport, FilterValue::Transport("packet".into()));
        let spec = QuerySpec {
            free_text: Some("damage".into()),
            filters, sort: SortOrder::DateDesc,
            page: Default::default(),
        };
        let (sql, params) = compose(&spec);
        assert!(sql.contains("messages_fts MATCH"));
        assert!(sql.contains("from_addr LIKE"));
        assert!(sql.contains("form_type ="));
        assert!(sql.contains("unread ="));
        assert!(sql.contains("has_attachments ="));
        assert!(sql.contains("transport_used ="));
        assert!(sql.contains(">="));   // date range from
        assert!(sql.contains("<="));   // date range to
        assert!(params.len() >= 7);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::query::`
Expected: FAIL — `compose` unimplemented.

- [ ] **Step 3: Write minimal implementation**

Replace `compose`:
```rust
pub fn compose(spec: &QuerySpec) -> (String, Vec<SqlParam>) {
    let mut params: Vec<SqlParam> = Vec::new();
    let mut where_clauses: Vec<String> = Vec::new();

    // FTS join when free-text is present
    let (from_clause, fts_where) = match &spec.free_text {
        Some(text) if !text.trim().is_empty() => {
            params.push(SqlParam::Text(text.clone()));
            (
                "messages_fts AS f JOIN messages_meta AS m ON m.mid = f.mid".to_string(),
                Some(format!("messages_fts MATCH ?{}", params.len())),
            )
        }
        _ => ("messages_meta AS m".to_string(), None),
    };
    if let Some(c) = fts_where { where_clauses.push(c); }

    for (key, value) in &spec.filters {
        match (key, value) {
            (FilterKey::Folder, FilterValue::Folder(f)) if f != "all" => {
                params.push(SqlParam::Text(f.clone()));
                where_clauses.push(format!("m.folder = ?{}", params.len()));
            }
            (FilterKey::From, FilterValue::Addr(a)) => {
                params.push(SqlParam::Text(format!("%{}%", a)));
                where_clauses.push(format!("m.from_addr LIKE ?{}", params.len()));
            }
            (FilterKey::To, FilterValue::Addr(a)) => {
                params.push(SqlParam::Text(format!("%{}%", a)));
                where_clauses.push(format!("m.to_addrs LIKE ?{}", params.len()));
            }
            (FilterKey::FormType, FilterValue::FormType(ft)) => {
                params.push(SqlParam::Text(ft.clone()));
                where_clauses.push(format!("m.form_type = ?{}", params.len()));
            }
            (FilterKey::HasForm, FilterValue::Bool(true)) => {
                where_clauses.push("m.form_type IS NOT NULL".into());
            }
            (FilterKey::HasForm, FilterValue::Bool(false)) => {
                where_clauses.push("m.form_type IS NULL".into());
            }
            (FilterKey::HasAttach, FilterValue::Bool(b)) => {
                params.push(SqlParam::Int(*b as i64));
                where_clauses.push(format!("m.has_attachments = ?{}", params.len()));
            }
            (FilterKey::ReadState, FilterValue::ReadState(rs)) => {
                let v = matches!(rs, ReadState::Unread) as i64;
                params.push(SqlParam::Int(v));
                where_clauses.push(format!("m.unread = ?{}", params.len()));
            }
            (FilterKey::Transport, FilterValue::Transport(t)) => {
                params.push(SqlParam::Text(t.clone()));
                where_clauses.push(format!("m.transport_used = ?{}", params.len()));
            }
            (FilterKey::DateRange, FilterValue::DateRange { from, to }) => {
                if let Some(f) = from {
                    params.push(SqlParam::Int(*f));
                    where_clauses.push(format!(
                        "COALESCE(m.date_received, m.date_sent) >= ?{}",
                        params.len()
                    ));
                }
                if let Some(t) = to {
                    params.push(SqlParam::Int(*t));
                    where_clauses.push(format!(
                        "COALESCE(m.date_received, m.date_sent) <= ?{}",
                        params.len()
                    ));
                }
            }
            _ => {}    // unknown / mismatched (FilterKey, FilterValue) — defensively ignored
        }
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    let order = match spec.sort {
        SortOrder::DateDesc => "ORDER BY COALESCE(m.date_received, m.date_sent) DESC",
        SortOrder::DateAsc  => "ORDER BY COALESCE(m.date_received, m.date_sent) ASC",
    };

    params.push(SqlParam::Int(spec.page.page_size as i64));
    let limit_n = params.len();
    params.push(SqlParam::Int(spec.page.offset as i64));
    let offset_n = params.len();

    let sql = format!(
        "SELECT m.mid, m.folder, m.from_addr, m.to_addrs, m.cc_addrs, \
                m.date_sent, m.date_received, m.unread, m.form_type, \
                m.has_attachments, m.attachment_count, m.transport_used, \
                m.direction, m.message_size, m.routing_path \
         FROM {from_clause}{where_sql} {order} LIMIT ?{limit_n} OFFSET ?{offset_n}",
    );

    (sql, params)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::query::`
Expected: PASS (5 tests).

- [ ] **Step 5: Add `Index::query` + an integration-style test**

Append to `src-tauri/src/search/index.rs`:
```rust
use crate::search::query::{compose, SqlParam};
use crate::search::types::QuerySpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryHit {
    pub mid: String,
    pub folder: String,
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
        let rs = params.iter().map(|p| match p {
            SqlParam::Text(s) => rusqlite::types::Value::Text(s.clone()),
            SqlParam::Int(i)  => rusqlite::types::Value::Integer(*i),
            SqlParam::Null    => rusqlite::types::Value::Null,
        }).collect::<Vec<_>>();
        let param_refs: Vec<&dyn rusqlite::ToSql> = rs.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(QueryHit {
                mid: row.get(0)?,
                folder: row.get(1)?,
                from_addr: row.get(2)?,
                to_addrs: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                cc_addrs: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                date_sent: row.get(5)?,
                date_received: row.get(6)?,
                unread: row.get::<_, i64>(7)? != 0,
                form_type: row.get(8)?,
                has_attachments: row.get::<_, i64>(9)? != 0,
                attachment_count: row.get::<_, i64>(10)? as u32,
                transport_used: row.get(11)?,
                direction: row.get(12)?,
                message_size: row.get::<_, i64>(13)? as u32,
                routing_path: row.get(14)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
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
            mid: mid.into(), folder: folder.into(),
            subject: subject.into(), body: body.into(), form_field_values: "".into(),
            from_addr: Some(from.into()), to_addrs: vec!["N7CPZ".into()], cc_addrs: vec![],
            date_sent: None, date_received: Some(1_716_200_000),
            unread: true, form_type: None, has_attachments: false, attachment_count: 0,
            transport_used: Some("telnet".into()), direction: Direction::Received,
            message_size: body.len() as u32, routing_path: None,
        }
    }

    #[test]
    fn freetext_returns_only_matching_messages() {
        let dir = tempdir().unwrap();
        let idx = Index::open(dir.path().join("search.db")).unwrap();
        idx.upsert(&r("A", "inbox", "KX5DD", "DAMAGE report", "powerlines")).unwrap();
        idx.upsert(&r("B", "inbox", "WX5RES", "weather brief", "ridge")).unwrap();
        let spec = QuerySpec { free_text: Some("damage".into()), ..QuerySpec::default() };
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
        let spec = QuerySpec { filters, ..QuerySpec::default() };
        let hits = idx.query(&spec).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].mid, "A");
    }
}
```

- [ ] **Step 6: Run tests to verify**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::`
Expected: PASS (all schema + mutation + compose + query_integration tests).

- [ ] **Step 7: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/src/search/query.rs src-tauri/src/search/index.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): QuerySpec→SQL composition + Index::query (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Saved + recent JSON store

**Spec:** §7.4 (persistence shape), §3.3 (saved + recent semantics), §6.4 (commands).

**Files:**
- Modify: `src-tauri/src/search/saved.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/search/saved.rs`:
```rust
use crate::search::types::QuerySpec;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 1;
pub const RECENT_CAP: usize = 20;

#[derive(Error, Debug)]
pub enum SavedError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedSearch {
    pub id: String,                  // uuid v4
    pub name: String,
    pub spec: QuerySpec,
    pub created_at: i64,             // unix seconds
    pub last_used_at: Option<i64>,
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentSearch {
    pub spec: QuerySpec,
    pub ran_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedStoreFile {
    pub version: u32,
    pub saved: Vec<SavedSearch>,
    pub recent: Vec<RecentSearch>,
}

impl Default for SavedStoreFile {
    fn default() -> Self {
        Self { version: SCHEMA_VERSION, saved: vec![], recent: vec![] }
    }
}

pub struct SavedStore {
    path: PathBuf,
    file: SavedStoreFile,
}

impl SavedStore {
    pub fn open(path: PathBuf) -> Result<Self, SavedError> {
        let file = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            SavedStoreFile::default()
        };
        Ok(Self { path, file })
    }

    pub fn save(&self, name: &str, spec: QuerySpec, now: i64) -> Result<SavedSearch, SavedError> {
        let _ = (name, spec, now);
        unimplemented!()
    }

    pub fn unsave(&self, id: &str) -> Result<(), SavedError> {
        let _ = id;
        unimplemented!()
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<(), SavedError> {
        let _ = (id, name);
        unimplemented!()
    }

    pub fn reorder(&self, ordered_ids: &[String]) -> Result<(), SavedError> {
        let _ = ordered_ids;
        unimplemented!()
    }

    pub fn record_recent(&self, spec: QuerySpec, now: i64) -> Result<(), SavedError> {
        let _ = (spec, now);
        unimplemented!()
    }

    pub fn promote_recent(&self, name: &str, spec: &QuerySpec, now: i64) -> Result<SavedSearch, SavedError> {
        let _ = (name, spec, now);
        unimplemented!()
    }

    pub fn saved(&self) -> &[SavedSearch] { &self.file.saved }
    pub fn recent(&self) -> &[RecentSearch] { &self.file.recent }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn empty_spec() -> QuerySpec { QuerySpec::default() }

    #[test]
    fn open_returns_default_when_file_missing() {
        let dir = tempdir().unwrap();
        let store = SavedStore::open(dir.path().join("saved-searches.json")).unwrap();
        assert!(store.saved().is_empty());
        assert!(store.recent().is_empty());
    }

    #[test]
    fn save_then_unsave_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("saved.json");
        let mut store = SavedStore::open(path.clone()).unwrap();
        let s = store.save("Storm Net", empty_spec(), 1_700_000_000).unwrap();
        assert_eq!(store.saved().len(), 1);
        assert_eq!(s.name, "Storm Net");
        // reload from disk
        let store2 = SavedStore::open(path.clone()).unwrap();
        assert_eq!(store2.saved().len(), 1);
        // unsave
        let mut store3 = store2;
        store3.unsave(&s.id).unwrap();
        assert_eq!(store3.saved().len(), 0);
    }

    #[test]
    fn record_recent_caps_at_RECENT_CAP() {
        let dir = tempdir().unwrap();
        let mut store = SavedStore::open(dir.path().join("s.json")).unwrap();
        for i in 0..(RECENT_CAP as i64 + 5) {
            store.record_recent(empty_spec(), 1_700_000_000 + i).unwrap();
        }
        assert_eq!(store.recent().len(), RECENT_CAP);
        // newest first
        assert!(store.recent().first().unwrap().ran_at > store.recent().last().unwrap().ran_at);
    }

    #[test]
    fn promote_recent_moves_into_saved() {
        let dir = tempdir().unwrap();
        let mut store = SavedStore::open(dir.path().join("s.json")).unwrap();
        store.record_recent(empty_spec(), 1_700_000_000).unwrap();
        let s = store.promote_recent("My pick", &empty_spec(), 1_700_000_100).unwrap();
        assert_eq!(s.name, "My pick");
        assert_eq!(store.saved().len(), 1);
        // promoted entry is removed from recent
        assert_eq!(store.recent().len(), 0);
    }
}
```

Note the mutating methods are declared `&self` but mutate `file` — they need `&mut self`. The Step 3 implementation will fix the signatures.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::saved::`
Expected: FAIL — methods unimplemented + signature drift.

- [ ] **Step 3: Write minimal implementation**

Replace the `impl SavedStore` block:
```rust
impl SavedStore {
    pub fn open(path: PathBuf) -> Result<Self, SavedError> {
        let file = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            SavedStoreFile::default()
        };
        Ok(Self { path, file })
    }

    fn flush(&self) -> Result<(), SavedError> {
        let json = serde_json::to_string_pretty(&self.file)?;
        if let Some(parent) = self.path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn save(&mut self, name: &str, spec: QuerySpec, now: i64) -> Result<SavedSearch, SavedError> {
        let order = self.file.saved.iter().map(|s| s.order).max().map(|n| n + 1).unwrap_or(0);
        let s = SavedSearch {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            spec,
            created_at: now,
            last_used_at: None,
            order,
        };
        self.file.saved.push(s.clone());
        self.flush()?;
        Ok(s)
    }

    pub fn unsave(&mut self, id: &str) -> Result<(), SavedError> {
        let before = self.file.saved.len();
        self.file.saved.retain(|s| s.id != id);
        if self.file.saved.len() == before {
            return Err(SavedError::NotFound(id.to_string()));
        }
        self.flush()
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), SavedError> {
        let s = self.file.saved.iter_mut().find(|s| s.id == id)
            .ok_or_else(|| SavedError::NotFound(id.to_string()))?;
        s.name = name.to_string();
        self.flush()
    }

    pub fn reorder(&mut self, ordered_ids: &[String]) -> Result<(), SavedError> {
        for (i, id) in ordered_ids.iter().enumerate() {
            let s = self.file.saved.iter_mut().find(|s| &s.id == id)
                .ok_or_else(|| SavedError::NotFound(id.clone()))?;
            s.order = i as u32;
        }
        self.file.saved.sort_by_key(|s| s.order);
        self.flush()
    }

    pub fn record_recent(&mut self, spec: QuerySpec, now: i64) -> Result<(), SavedError> {
        self.file.recent.insert(0, RecentSearch { spec, ran_at: now });
        if self.file.recent.len() > RECENT_CAP {
            self.file.recent.truncate(RECENT_CAP);
        }
        self.flush()
    }

    pub fn promote_recent(&mut self, name: &str, spec: &QuerySpec, now: i64) -> Result<SavedSearch, SavedError> {
        self.file.recent.retain(|r| &r.spec != spec);
        let saved = self.save(name, spec.clone(), now)?;
        Ok(saved)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::saved::`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/src/search/saved.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): saved + recent JSON store with promote / unsave / cap-20 (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Tauri command handlers

**Spec:** §6.4 (command surface).

**Files:**
- Modify: `src-tauri/src/search/commands.rs`
- Modify: `src-tauri/src/search/types.rs` (add `SearchResults`, `RebuildStats`)

- [ ] **Step 1: Add the result DTOs to `types.rs`**

Append to `src-tauri/src/search/types.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub items: Vec<MessageMetaDto>,
    pub total_matches: u32,
    pub query_ms: u32,
    pub effective_spec: QuerySpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageMetaDto {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub date: String,           // RFC3339 UTC
    pub unread: bool,
    pub body_size: u32,
    pub has_attachments: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_tag: Option<String>,
    /// Folder badge for cross-folder search rendering (spec §7.2).
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RebuildStats {
    pub messages_indexed: u32,
    pub elapsed_ms: u32,
}
```

- [ ] **Step 2: Write the failing commands test**

In `src-tauri/src/search/commands.rs`:
```rust
//! Tauri command handlers. Each one accepts a Tauri `State<SearchService>` (or
//! equivalent) and a serde-friendly DTO. Tests exercise the underlying service
//! methods directly — the Tauri wrapper is a one-line forward.

use crate::search::index::{Index, IndexError};
use crate::search::saved::{SavedSearch, SavedStore, SavedError, RecentSearch};
use crate::search::types::{
    MessageMetaDto, QuerySpec, RebuildStats, SearchResults,
};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
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
        subject: String::new(),         // filled by extractor at upsert; query returns meta only
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
    // Plain RFC3339 in UTC. No chrono dep — hand-roll using the inverse of
    // extractor::days_from_civil. The day-arithmetic helper lives here mirror-
    // image to the extractor and is unit-tested.
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
```

- [ ] **Step 3: Run tests to verify they fail then pass**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::commands::`
Expected: First run FAILS to compile (missing struct fields, etc.). Iterate: fix compile errors per rustc output. Once compile clean, all 3 tests PASS.

- [ ] **Step 4: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/src/search/commands.rs src-tauri/src/search/types.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): SearchService + Tauri command surface (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Mailbox hooks — wire `Index` into `store` / `move_to` / `mark_read`

**Spec:** §6.2 (mailbox hooks), §8 (find-messages never breaks mailbox writes).

**Files:**
- Modify: `src-tauri/src/native_mailbox.rs`

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/native_mailbox.rs` test module:
```rust
#[cfg(test)]
mod index_hook_tests {
    use super::*;
    use crate::search::index::Index;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn build_mailbox_with_index(dir: &std::path::Path) -> (Mailbox, Arc<Index>) {
        let idx = Arc::new(Index::open(dir.join("search.db")).unwrap());
        let mut mbox = Mailbox::new(dir.to_path_buf());
        mbox = mbox.with_index(idx.clone());
        (mbox, idx)
    }

    fn raw(subject: &str, body: &str) -> Vec<u8> {
        crate::winlink::compose::compose_message(
            "N7CPZ", &["W1AW"], &[], subject, body, 1_716_200_000,
        ).to_bytes()
    }

    #[test]
    fn store_upserts_into_index() {
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());
        mbox.store(MailboxFolder::Inbox, &raw("Hello", "body")).unwrap();
        assert_eq!(idx.count().unwrap(), 1);
    }

    #[test]
    fn move_to_updates_folder_in_index() {
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());
        let id = mbox.store(MailboxFolder::Outbox, &raw("x", "y")).unwrap();
        mbox.move_to(MailboxFolder::Outbox, MailboxFolder::Sent, &id).unwrap();
        let folder: String = idx
            .conn
            .query_row("SELECT folder FROM messages_meta WHERE mid = ?1", [&id.0], |r| r.get(0))
            .unwrap();
        assert_eq!(folder, "sent");
    }

    #[test]
    fn mark_read_updates_unread_in_index() {
        let dir = tempdir().unwrap();
        let (mbox, idx) = build_mailbox_with_index(dir.path());
        let id = mbox.store(MailboxFolder::Inbox, &raw("x", "y")).unwrap();
        mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();
        let unread: i64 = idx
            .conn
            .query_row("SELECT unread FROM messages_meta WHERE mid = ?1", [&id.0], |r| r.get(0))
            .unwrap();
        assert_eq!(unread, 0);
    }

    #[test]
    fn index_failure_does_not_break_mailbox_store() {
        // Build an Index, close the connection by dropping a poisoned variant
        // (we simulate by deleting the file mid-test). The store call must
        // still return Ok — find-messages never breaks mailbox writes (§8).
        let dir = tempdir().unwrap();
        let (mbox, _idx) = build_mailbox_with_index(dir.path());
        std::fs::remove_file(dir.path().join("search.db")).unwrap();
        // Store should still succeed; the inability to write the index is
        // logged but not propagated.
        let res = mbox.store(MailboxFolder::Inbox, &raw("x", "y"));
        assert!(res.is_ok(), "mailbox.store must not fail because of index errors");
    }
}
```

The test expects the public `conn` field on `Index` to remain accessible to tests — keep it `pub` (or expose a `pub(crate)` accessor).

- [ ] **Step 2: Write the minimal implementation**

In `src-tauri/src/native_mailbox.rs`, change:
```rust
pub struct Mailbox {
    root: PathBuf,
    index: Option<Arc<crate::search::index::Index>>,
}

impl Mailbox {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), index: None }
    }

    pub fn with_index(mut self, index: Arc<crate::search::index::Index>) -> Self {
        self.index = Some(index);
        self
    }

    pub fn store(&self, folder: MailboxFolder, raw: &[u8]) -> Result<MessageId, BackendError> {
        let mid = self.store_to_disk(folder, raw)?;
        if let Some(idx) = self.index.as_ref() {
            if let Ok(msg) = Message::from_bytes(raw) {
                let row = crate::search::extractor::extract(
                    &msg, folder,
                    direction_for_folder(folder),
                    /*unread=*/ folder == MailboxFolder::Inbox,
                    /*transport_used=*/ None,
                );
                if let Err(e) = idx.upsert(&row) {
                    eprintln!("search-index upsert failed for mid={}: {e}", row.mid);
                }
            }
        }
        Ok(mid)
    }

    pub fn move_to(
        &self, from: MailboxFolder, to: MailboxFolder, id: &MessageId,
    ) -> Result<(), BackendError> {
        self.move_to_disk(from, to, id)?;
        if let Some(idx) = self.index.as_ref() {
            if let Err(e) = idx.update_folder(&id.0, folder_str(to)) {
                eprintln!("search-index update_folder failed for mid={}: {e}", id.0);
            }
        }
        Ok(())
    }

    pub fn mark_read(&self, folder: MailboxFolder, id: &MessageId) -> Result<(), BackendError> {
        self.mark_read_on_disk(folder, id)?;
        if let Some(idx) = self.index.as_ref() {
            if let Err(e) = idx.update_unread(&id.0, false) {
                eprintln!("search-index update_unread failed for mid={}: {e}", id.0);
            }
        }
        Ok(())
    }

    // ... existing store_to_disk / move_to_disk / mark_read_on_disk are the
    // pre-existing method bodies, renamed by extracting the FS operations.
}

fn direction_for_folder(f: MailboxFolder) -> crate::search::extractor::Direction {
    match f {
        MailboxFolder::Sent | MailboxFolder::Outbox => crate::search::extractor::Direction::Sent,
        _ => crate::search::extractor::Direction::Received,
    }
}

fn folder_str(f: MailboxFolder) -> &'static str {
    match f {
        MailboxFolder::Inbox => "inbox",
        MailboxFolder::Outbox => "outbox",
        MailboxFolder::Sent => "sent",
        MailboxFolder::Archive => "archive",
    }
}
```

Add `use std::sync::Arc;` and `use crate::winlink::message::Message;` at the top if they're not already there.

- [ ] **Step 3: Run tests to verify**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml native_mailbox::`
Expected: existing 11 tests + 4 new `index_hook_tests` all PASS.

Also run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml`
Expected: full suite green.

- [ ] **Step 4: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/src/native_mailbox.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): Mailbox::with_index hooks store/move_to/mark_read (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: `rebuild-index` command + Tauri command registration

**Spec:** §6.3 (rebuild semantics), §8 (schema drift → automatic rebuild).

**Files:**
- Modify: `src-tauri/src/search/commands.rs` (add `rebuild_index` method)
- Modify: `src-tauri/src/search/mod.rs` (top-level `pub fn build_service(...)`)
- Modify: `src-tauri/src/lib.rs` (or `main.rs`) — register Tauri commands

- [ ] **Step 1: Write the failing rebuild test**

Append to `src-tauri/src/search/commands.rs`:
```rust
impl SearchService {
    /// Delete + recreate the search.db, then re-walk every folder of the
    /// supplied mailbox calling `Index::upsert` per message. Returns stats.
    pub fn rebuild_index(
        &self,
        _mailbox_root: PathBuf,
    ) -> Result<RebuildStats, CommandError> {
        unimplemented!()
    }
}

#[cfg(test)]
mod rebuild_tests {
    use super::*;
    use crate::winlink_backend::MailboxFolder;
    use crate::native_mailbox::Mailbox;
    use tempfile::tempdir;

    fn raw(subject: &str, body: &str) -> Vec<u8> {
        crate::winlink::compose::compose_message(
            "N7CPZ", &["W1AW"], &[], subject, body, 1_716_200_000,
        ).to_bytes()
    }

    #[test]
    fn rebuild_picks_up_messages_already_on_disk() {
        let dir = tempdir().unwrap();
        // First: store 3 messages WITHOUT an index attached (so disk has data
        // but the index doesn't know about it).
        {
            let mbox = Mailbox::new(dir.path());
            mbox.store(MailboxFolder::Inbox, &raw("a", "x")).unwrap();
            mbox.store(MailboxFolder::Inbox, &raw("b", "y")).unwrap();
            mbox.store(MailboxFolder::Sent,  &raw("c", "z")).unwrap();
        }

        let svc = build_service(dir.path());
        let stats = svc.rebuild_index(dir.path().to_path_buf()).unwrap();
        assert_eq!(stats.messages_indexed, 3);
        assert_eq!(svc.index.count().unwrap(), 3);
    }
}
```

(Re-use `build_service` from the earlier tests block in this file.)

- [ ] **Step 2: Implement `rebuild_index`**

Replace the `unimplemented!()`:
```rust
use crate::native_mailbox::Mailbox;
use crate::winlink_backend::MailboxFolder;

impl SearchService {
    pub fn rebuild_index(&self, mailbox_root: PathBuf) -> Result<RebuildStats, CommandError> {
        let started = Instant::now();
        // delete the existing files
        let db = mailbox_root.join("search.db");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(mailbox_root.join("search.db-wal"));
        let _ = std::fs::remove_file(mailbox_root.join("search.db-shm"));
        // re-open: a fresh schema is created
        let fresh = Index::open(db).map_err(CommandError::from)?;

        // re-walk every folder
        let mbox = Mailbox::new(mailbox_root.clone());
        let mut count = 0u32;
        for folder in [MailboxFolder::Inbox, MailboxFolder::Outbox, MailboxFolder::Sent, MailboxFolder::Archive] {
            let metas = mbox.list(folder).map_err(|e| CommandError::Internal(e.to_string()))?;
            for meta in metas {
                let body = mbox.read(folder, &meta.id)
                    .map_err(|e| CommandError::Internal(e.to_string()))?;
                if let Ok(msg) = crate::winlink::message::Message::from_bytes(&body.raw_rfc5322) {
                    let row = crate::search::extractor::extract(
                        &msg, folder,
                        match folder {
                            MailboxFolder::Sent | MailboxFolder::Outbox => crate::search::extractor::Direction::Sent,
                            _ => crate::search::extractor::Direction::Received,
                        },
                        meta.unread,
                        None,
                    );
                    fresh.upsert(&row).map_err(CommandError::from)?;
                    count += 1;
                }
            }
        }
        // NOTE: the SearchService's internal `index` Arc still points at the
        // OLD handle. Production callers must construct a NEW SearchService
        // after a rebuild, or — for the runtime case — the lib.rs wiring
        // re-creates SearchService inside the rebuild Tauri command.
        Ok(RebuildStats { messages_indexed: count, elapsed_ms: started.elapsed().as_millis() as u32 })
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml search::commands::rebuild_tests`
Expected: PASS.

- [ ] **Step 4: Add `build_service` factory + register Tauri commands**

In `src-tauri/src/search/mod.rs`, append:
```rust
use std::path::Path;
use std::sync::{Arc, Mutex};
use crate::search::commands::SearchService;
use crate::search::index::Index;
use crate::search::saved::SavedStore;

/// Build a SearchService rooted at the given data directory. The Index lives
/// at `<data_dir>/search.db`; saved-searches at `<data_dir>/saved-searches.json`.
pub fn build_service(data_dir: &Path) -> Result<SearchService, crate::search::commands::CommandError> {
    let index = Arc::new(Index::open(data_dir.join("search.db"))?);
    let saved = Mutex::new(SavedStore::open(data_dir.join("saved-searches.json"))?);
    Ok(SearchService {
        index,
        saved,
        now_unix: || std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    })
}
```

In whichever file declares the Tauri command surface (`lib.rs` or `main.rs`, identified in Task 1), register the new commands inside the existing `invoke_handler` chain:
```rust
.invoke_handler(tauri::generate_handler![
    /* existing commands... */
    crate::search::commands::tauri_search_run,
    crate::search::commands::tauri_search_save,
    crate::search::commands::tauri_search_list_saved,
    crate::search::commands::tauri_search_list_recent,
    crate::search::commands::tauri_search_unsave,
    crate::search::commands::tauri_search_rename,
    crate::search::commands::tauri_search_reorder,
    crate::search::commands::tauri_search_rebuild_index,
])
.manage(crate::search::build_service(&app_data_dir).expect("search service"))
```

Then in `src-tauri/src/search/commands.rs`, add thin Tauri wrappers (one per command) at the bottom of the file:
```rust
#[tauri::command]
pub fn tauri_search_run(svc: tauri::State<SearchService>, spec: QuerySpec) -> Result<SearchResults, CommandError> {
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
pub fn tauri_search_save(svc: tauri::State<SearchService>, name: String, spec: QuerySpec) -> Result<SavedSearch, CommandError> {
    svc.save(name, spec)
}

#[tauri::command]
pub fn tauri_search_unsave(svc: tauri::State<SearchService>, id: String) -> Result<(), CommandError> {
    svc.unsave(id)
}

#[tauri::command]
pub fn tauri_search_rename(svc: tauri::State<SearchService>, id: String, name: String) -> Result<(), CommandError> {
    svc.rename(id, name)
}

#[tauri::command]
pub fn tauri_search_reorder(svc: tauri::State<SearchService>, ordered_ids: Vec<String>) -> Result<(), CommandError> {
    svc.reorder(ordered_ids)
}

#[tauri::command]
pub fn tauri_search_rebuild_index(svc: tauri::State<SearchService>, app: tauri::AppHandle) -> Result<RebuildStats, CommandError> {
    let data_dir = app.path_resolver().app_data_dir().ok_or_else(|| CommandError::Internal("no app_data_dir".into()))?;
    svc.rebuild_index(data_dir.join("mail"))
}
```

(The exact Tauri-2 path-resolver call may need a one-line tweak per the project's actual `AppHandle` extension trait usage; verify against an existing command in `src-tauri/src/ui_commands.rs`.)

- [ ] **Step 5: Verify the app compiles end-to-end**

Run: `cargo build --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml`
Expected: clean build (warnings ok).

- [ ] **Step 6: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/src/search/ src-tauri/src/lib.rs src-tauri/src/main.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): rebuild-index command + Tauri command registration (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Frontend `types.ts` + `queryRender.ts`

**Spec:** §7.1 (component file map), §6.4 (Rust DTO shapes — mirror to TS).

**Files:**
- Create: `src/search/types.ts`, `src/search/queryRender.ts`
- Create: `src/search/types.test.ts`, `src/search/queryRender.test.ts`

- [ ] **Step 1: Write the failing types compile-test**

In `src/search/types.test.ts`:
```ts
import { describe, it, expect } from 'vitest';
import type {
  QuerySpec, FilterKey, FilterValue, ReadState, SortOrder, PageRequest,
  SavedSearch, RecentSearch, SearchResults, MessageMetaDto, RebuildStats,
} from './types';

describe('search/types', () => {
  it('compose a QuerySpec literal that matches the Rust serde shape', () => {
    const spec: QuerySpec = {
      free_text: 'damage',
      filters: {
        from: { kind: 'addr', value: 'KX5DD' },
        'form-type': { kind: 'form-type', value: 'ICS-213' },
        'date-range': { kind: 'date-range', value: { from: 1_700_000_000, to: null } },
      },
      sort: 'date_desc',
      page: { page_size: 200, offset: 0 },
    };
    expect(spec.free_text).toBe('damage');
    expect(spec.filters.from?.kind).toBe('addr');
  });

  it('SavedSearch has required fields', () => {
    const s: SavedSearch = {
      id: 'uuid',
      name: 'Storm Net',
      spec: { free_text: null, filters: {}, sort: 'date_desc', page: { page_size: 200, offset: 0 } },
      created_at: 1,
      last_used_at: null,
      order: 0,
    };
    expect(s.name).toBe('Storm Net');
  });
});
```

- [ ] **Step 2: Implement `types.ts`**

In `src/search/types.ts`:
```ts
/// Mirrors src-tauri/src/search/types.rs — kebab-case serde tags on FilterValue,
/// snake_case SortOrder, etc. When the Rust shape changes, this file MUST be
/// updated in the same PR.

export type FilterKey =
  | 'folder'
  | 'from'
  | 'to'
  | 'date-range'
  | 'form-type'
  | 'has-form'
  | 'has-attach'
  | 'read-state'
  | 'transport';

export type ReadState = 'read' | 'unread';

export type FilterValue =
  | { kind: 'folder'; value: string }
  | { kind: 'addr'; value: string }
  | { kind: 'date-range'; value: { from: number | null; to: number | null } }
  | { kind: 'form-type'; value: string }
  | { kind: 'bool'; value: boolean }
  | { kind: 'read-state'; value: ReadState }
  | { kind: 'transport'; value: string };

export type SortOrder = 'date_desc' | 'date_asc';

export interface PageRequest {
  page_size: number;
  offset: number;
}

export interface QuerySpec {
  free_text: string | null;
  filters: Partial<Record<FilterKey, FilterValue>>;
  sort: SortOrder;
  page: PageRequest;
}

export const EMPTY_SPEC: QuerySpec = {
  free_text: null,
  filters: {},
  sort: 'date_desc',
  page: { page_size: 200, offset: 0 },
};

export interface MessageMetaDto {
  id: string;
  subject: string;
  from: string;
  to: string[];
  date: string;           // RFC3339
  unread: boolean;
  bodySize: number;
  hasAttachments: boolean;
  formTag?: string;
  folder: string;
}

export interface SearchResults {
  items: MessageMetaDto[];
  totalMatches: number;
  queryMs: number;
  effectiveSpec: QuerySpec;
}

export interface SavedSearch {
  id: string;
  name: string;
  spec: QuerySpec;
  created_at: number;
  last_used_at: number | null;
  order: number;
}

export interface RecentSearch {
  spec: QuerySpec;
  ran_at: number;
}

export interface RebuildStats {
  messagesIndexed: number;
  elapsedMs: number;
}
```

- [ ] **Step 3: Write the queryRender failing test**

In `src/search/queryRender.test.ts`:
```ts
import { describe, it, expect } from 'vitest';
import { renderQuery } from './queryRender';
import { EMPTY_SPEC } from './types';

describe('renderQuery', () => {
  it('returns the free text when no filters', () => {
    expect(renderQuery({ ...EMPTY_SPEC, free_text: 'damage' })).toBe('damage');
  });

  it('renders chips after free text', () => {
    expect(renderQuery({
      ...EMPTY_SPEC,
      free_text: 'damage',
      filters: {
        from: { kind: 'addr', value: 'KX5DD' },
        'date-range': { kind: 'date-range', value: { from: 1_700_000_000, to: null } },
      },
    })).toBe('damage from:KX5DD date:from-1700000000');
  });

  it('renders only chips when no free text', () => {
    expect(renderQuery({
      ...EMPTY_SPEC,
      filters: { from: { kind: 'addr', value: 'KX5DD' } },
    })).toBe('from:KX5DD');
  });

  it('returns "(empty)" for a totally empty spec', () => {
    expect(renderQuery(EMPTY_SPEC)).toBe('(empty)');
  });
});
```

- [ ] **Step 4: Implement `queryRender.ts`**

In `src/search/queryRender.ts`:
```ts
import type { FilterKey, FilterValue, QuerySpec } from './types';

const KEY_ORDER: FilterKey[] = [
  'folder', 'from', 'to', 'date-range', 'form-type',
  'has-form', 'has-attach', 'read-state', 'transport',
];

export function renderQuery(spec: QuerySpec): string {
  const parts: string[] = [];
  if (spec.free_text && spec.free_text.trim()) parts.push(spec.free_text.trim());
  for (const key of KEY_ORDER) {
    const v = spec.filters[key];
    if (!v) continue;
    parts.push(renderChip(key, v));
  }
  return parts.length === 0 ? '(empty)' : parts.join(' ');
}

function renderChip(key: FilterKey, v: FilterValue): string {
  switch (v.kind) {
    case 'addr':       return `${key}:${v.value}`;
    case 'folder':     return `folder:${v.value}`;
    case 'form-type':  return `form:${v.value}`;
    case 'transport':  return `transport:${v.value}`;
    case 'bool':       return `${key}:${v.value}`;
    case 'read-state': return `read-state:${v.value}`;
    case 'date-range': {
      const f = v.value.from != null ? `from-${v.value.from}` : '';
      const t = v.value.to != null ? `to-${v.value.to}` : '';
      return `date:${[f, t].filter(Boolean).join('..')}`;
    }
  }
}
```

- [ ] **Step 5: Run tests**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages vitest run src/search/types.test.ts src/search/queryRender.test.ts`
Expected: PASS (6 tests).

- [ ] **Step 6: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src/search/types.ts src/search/queryRender.ts src/search/types.test.ts src/search/queryRender.test.ts
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): TS types mirror + queryRender (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: `useSearch` hook

**Spec:** §7.3 (state model — debounced search_run, results, fallback to folder view).

**Files:**
- Create: `src/search/useSearch.ts`, `src/search/useSearch.test.ts`

- [ ] **Step 1: Write the failing test**

In `src/search/useSearch.test.ts`:
```ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { useSearch } from './useSearch';
import { EMPTY_SPEC, type SearchResults } from './types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

function wrap() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe('useSearch', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('returns null results when spec is empty', () => {
    const { result } = renderHook(() => useSearch(), { wrapper: wrap() });
    expect(result.current.results).toBeNull();
  });

  it('calls invoke after debounce when the spec is non-empty', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [], totalMatches: 0, queryMs: 1, effectiveSpec: EMPTY_SPEC,
    } satisfies SearchResults);
    const { result } = renderHook(() => useSearch(), { wrapper: wrap() });
    act(() => result.current.setSpec({ ...EMPTY_SPEC, free_text: 'damage' }));
    expect(invoke).not.toHaveBeenCalled();           // not yet — debounce window
    await act(async () => { vi.advanceTimersByTime(200); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('tauri_search_run', expect.anything()));
  });

  it('exposes setActiveSavedSearch — name surfaces back in `activeSaved`', () => {
    const { result } = renderHook(() => useSearch(), { wrapper: wrap() });
    act(() => result.current.setActiveSavedSearch({ id: '1', name: 'Storm', spec: EMPTY_SPEC, created_at: 0, last_used_at: null, order: 0 }));
    expect(result.current.activeSaved?.name).toBe('Storm');
  });
});
```

- [ ] **Step 2: Implement `useSearch.ts`**

In `src/search/useSearch.ts`:
```ts
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { EMPTY_SPEC, type QuerySpec, type SavedSearch, type SearchResults } from './types';

const DEBOUNCE_MS = 150;

function specIsActive(spec: QuerySpec): boolean {
  return !!(spec.free_text && spec.free_text.trim()) || Object.keys(spec.filters).length > 0;
}

export function useSearch() {
  const [spec, setSpec] = useState<QuerySpec>(EMPTY_SPEC);
  const [debounced, setDebounced] = useState<QuerySpec>(EMPTY_SPEC);
  const [activeSaved, setActiveSaved] = useState<SavedSearch | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setDebounced(spec), DEBOUNCE_MS);
    return () => { if (timer.current) clearTimeout(timer.current); };
  }, [spec]);

  const active = useMemo(() => specIsActive(debounced), [debounced]);

  const query = useQuery({
    queryKey: ['search', debounced],
    queryFn: async (): Promise<SearchResults> => {
      return await invoke<SearchResults>('tauri_search_run', { spec: debounced });
    },
    enabled: active,
    staleTime: 0,
  });

  const clear = useCallback(() => {
    setSpec(EMPTY_SPEC);
    setActiveSaved(null);
  }, []);

  const setActiveSavedSearch = useCallback((saved: SavedSearch | null) => {
    setActiveSaved(saved);
    setSpec(saved ? saved.spec : EMPTY_SPEC);
  }, []);

  return {
    spec,
    setSpec,
    activeSaved,
    setActiveSavedSearch,
    clear,
    results: active ? (query.data ?? null) : null,
    isLoading: query.isLoading,
    error: query.error as Error | null,
    isActive: active,
  };
}
```

- [ ] **Step 3: Run tests**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages vitest run src/search/useSearch.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src/search/useSearch.ts src/search/useSearch.test.ts
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): useSearch hook with debounced Tauri invoke (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 13: `useSavedSearches` hook

**Spec:** §3.3 (saved + recent semantics), §7.1 (file map).

**Files:**
- Create: `src/search/useSavedSearches.ts`, `src/search/useSavedSearches.test.ts`

- [ ] **Step 1: Write the failing test**

In `src/search/useSavedSearches.test.ts`:
```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { useSavedSearches } from './useSavedSearches';
import { EMPTY_SPEC, type SavedSearch, type RecentSearch } from './types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

function wrap() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe('useSavedSearches', () => {
  beforeEach(() => (invoke as unknown as ReturnType<typeof vi.fn>).mockReset());

  it('lists saved + recent on mount', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'tauri_search_list_saved') return Promise.resolve([{ id: '1', name: 'Storm', spec: EMPTY_SPEC, created_at: 0, last_used_at: null, order: 0 } satisfies SavedSearch]);
      if (cmd === 'tauri_search_list_recent') return Promise.resolve([{ spec: EMPTY_SPEC, ran_at: 100 } satisfies RecentSearch]);
      return Promise.resolve(null);
    });
    const { result } = renderHook(() => useSavedSearches(), { wrapper: wrap() });
    await waitFor(() => expect(result.current.saved).toHaveLength(1));
    expect(result.current.saved[0].name).toBe('Storm');
    expect(result.current.recent).toHaveLength(1);
  });

  it('save invokes tauri_search_save', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValue([] satisfies SavedSearch[]);
    const { result } = renderHook(() => useSavedSearches(), { wrapper: wrap() });
    await act(async () => {
      await result.current.save('My pick', EMPTY_SPEC);
    });
    expect(invoke).toHaveBeenCalledWith('tauri_search_save', { name: 'My pick', spec: EMPTY_SPEC });
  });
});
```

- [ ] **Step 2: Implement `useSavedSearches.ts`**

```ts
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { QuerySpec, RecentSearch, SavedSearch } from './types';

const SAVED_KEY = ['search', 'saved'];
const RECENT_KEY = ['search', 'recent'];

export function useSavedSearches() {
  const qc = useQueryClient();
  const saved = useQuery({ queryKey: SAVED_KEY, queryFn: () => invoke<SavedSearch[]>('tauri_search_list_saved') });
  const recent = useQuery({ queryKey: RECENT_KEY, queryFn: () => invoke<RecentSearch[]>('tauri_search_list_recent') });

  const refetchAll = () => Promise.all([qc.invalidateQueries({ queryKey: SAVED_KEY }), qc.invalidateQueries({ queryKey: RECENT_KEY })]);

  return {
    saved: saved.data ?? [],
    recent: recent.data ?? [],
    isLoading: saved.isLoading || recent.isLoading,

    save: async (name: string, spec: QuerySpec): Promise<SavedSearch> => {
      const result = await invoke<SavedSearch>('tauri_search_save', { name, spec });
      await refetchAll();
      return result;
    },
    unsave: async (id: string) => {
      await invoke('tauri_search_unsave', { id });
      await refetchAll();
    },
    rename: async (id: string, name: string) => {
      await invoke('tauri_search_rename', { id, name });
      await refetchAll();
    },
    reorder: async (orderedIds: string[]) => {
      await invoke('tauri_search_reorder', { orderedIds });
      await refetchAll();
    },
    rebuildIndex: async (): Promise<{ messagesIndexed: number; elapsedMs: number }> => {
      const stats = await invoke<{ messagesIndexed: number; elapsedMs: number }>('tauri_search_rebuild_index');
      await refetchAll();
      return stats;
    },
  };
}
```

- [ ] **Step 3: Run tests**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages vitest run src/search/useSavedSearches.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src/search/useSavedSearches.ts src/search/useSavedSearches.test.ts
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): useSavedSearches hook (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 14: `SearchBar` component

**Spec:** §3.1 (ribbon-mounted input, modes, ⌘F focus, chevron opens dropdown).

**Files:**
- Create: `src/search/SearchBar.tsx`, `src/search/SearchBar.test.tsx`, `src/search/SearchBar.css`

- [ ] **Step 1: Write the failing test**

In `src/search/SearchBar.test.tsx`:
```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';
import { SearchBar } from './SearchBar';
import { EMPTY_SPEC, type SavedSearch } from './types';

const noop = () => {};
const STORM: SavedSearch = {
  id: '1', name: 'Storm Net 5/30', spec: { ...EMPTY_SPEC, free_text: 'damage' },
  created_at: 0, last_used_at: null, order: 0,
};

describe('SearchBar', () => {
  it('renders placeholder when no spec', () => {
    render(<SearchBar spec={EMPTY_SPEC} activeSaved={null} onSpecChange={noop} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} />);
    expect(screen.getByPlaceholderText(/Search messages/i)).toBeInTheDocument();
  });

  it('shows saved-search name + ★ when activeSaved set', () => {
    render(<SearchBar spec={STORM.spec} activeSaved={STORM} onSpecChange={noop} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} />);
    expect(screen.getByTestId('searchbar-saved-name')).toHaveTextContent('Storm Net 5/30');
    expect(screen.getByTestId('searchbar-saved-star')).toBeInTheDocument();
  });

  it('clicking ★ on an active saved search calls onUnsave', () => {
    const onUnsave = vi.fn();
    render(<SearchBar spec={STORM.spec} activeSaved={STORM} onSpecChange={noop} onUnsave={onUnsave} onToggleDropdown={noop} dropdownOpen={false} />);
    fireEvent.click(screen.getByTestId('searchbar-saved-star'));
    expect(onUnsave).toHaveBeenCalled();
  });

  it('clicking chevron toggles dropdown', () => {
    const onToggle = vi.fn();
    render(<SearchBar spec={EMPTY_SPEC} activeSaved={null} onSpecChange={noop} onUnsave={noop} onToggleDropdown={onToggle} dropdownOpen={false} />);
    fireEvent.click(screen.getByTestId('searchbar-chevron'));
    expect(onToggle).toHaveBeenCalled();
  });

  it('typing fires onSpecChange with updated free_text', () => {
    const onSpecChange = vi.fn();
    render(<SearchBar spec={EMPTY_SPEC} activeSaved={null} onSpecChange={onSpecChange} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} />);
    const input = screen.getByTestId('searchbar-input');
    fireEvent.change(input, { target: { value: 'damage' } });
    expect(onSpecChange).toHaveBeenCalledWith(expect.objectContaining({ free_text: 'damage' }));
  });

  it('Escape clears spec and closes dropdown', () => {
    const onSpecChange = vi.fn();
    const onToggle = vi.fn();
    render(<SearchBar spec={{ ...EMPTY_SPEC, free_text: 'x' }} activeSaved={null} onSpecChange={onSpecChange} onUnsave={noop} onToggleDropdown={onToggle} dropdownOpen={true} />);
    fireEvent.keyDown(screen.getByTestId('searchbar-input'), { key: 'Escape' });
    expect(onSpecChange).toHaveBeenCalledWith(EMPTY_SPEC);
  });
});
```

- [ ] **Step 2: Implement `SearchBar.tsx`**

```tsx
import React, { useRef, useEffect } from 'react';
import './SearchBar.css';
import { EMPTY_SPEC, type QuerySpec, type SavedSearch } from './types';

export interface SearchBarProps {
  spec: QuerySpec;
  activeSaved: SavedSearch | null;
  onSpecChange: (spec: QuerySpec) => void;
  onUnsave: () => void;
  onToggleDropdown: () => void;
  dropdownOpen: boolean;
}

export function SearchBar({ spec, activeSaved, onSpecChange, onUnsave, onToggleDropdown, dropdownOpen }: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  // ⌘F / Ctrl-F focuses the search input from anywhere in the shell.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const handleEsc = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      onSpecChange(EMPTY_SPEC);
      if (dropdownOpen) onToggleDropdown();
    }
  };

  if (activeSaved) {
    return (
      <div className="search-bar focused" data-testid="search-bar">
        <span className="magnifier" aria-hidden="true">🔍</span>
        <button
          type="button"
          className="saved-star"
          data-testid="searchbar-saved-star"
          aria-label={`Unsave ${activeSaved.name}`}
          onClick={onUnsave}
        >★</button>
        <span className="saved-name" data-testid="searchbar-saved-name">{activeSaved.name}</span>
        <button
          type="button"
          className="chev"
          data-testid="searchbar-chevron"
          onClick={onToggleDropdown}
          aria-label="Open search dropdown"
        >▾</button>
      </div>
    );
  }

  return (
    <div className="search-bar" data-testid="search-bar">
      <span className="magnifier" aria-hidden="true">🔍</span>
      <input
        ref={inputRef}
        data-testid="searchbar-input"
        type="text"
        placeholder="Search messages…"
        value={spec.free_text ?? ''}
        onChange={(e) => onSpecChange({ ...spec, free_text: e.target.value || null })}
        onFocus={() => { if (!dropdownOpen) onToggleDropdown(); }}
        onKeyDown={handleEsc}
      />
      <button
        type="button"
        className="chev"
        data-testid="searchbar-chevron"
        onClick={onToggleDropdown}
        aria-label="Open search dropdown"
      >▾</button>
      <span className="shortcut">⌘F</span>
    </div>
  );
}
```

In `src/search/SearchBar.css`:
```css
/* Ribbon-mounted search input. Renders inside .layout-b .dashboard, replacing
   the leading dash-item slot. See AppShell.css for the dashboard's grid. */
.layout-b .dashboard .search-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--bg);
  border: 1px solid var(--border-strong);
  border-radius: 4px;
  padding: 6px 12px;
  width: 420px;
  font-size: 12px;
}
.layout-b .dashboard .search-bar.focused,
.layout-b .dashboard .search-bar:focus-within {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}
.layout-b .dashboard .search-bar .magnifier { color: var(--text-faint); }
.layout-b .dashboard .search-bar input {
  flex: 1;
  background: transparent;
  border: 0;
  color: var(--text);
  font-size: 13px;
  font-family: var(--mono);
  outline: none;
  padding: 0;
}
.layout-b .dashboard .search-bar input::placeholder { color: var(--text-faint); }
.layout-b .dashboard .search-bar .saved-star {
  background: transparent; border: 0; color: var(--accent);
  font-size: 14px; cursor: pointer; padding: 0;
}
.layout-b .dashboard .search-bar .saved-name {
  flex: 1; color: var(--accent-2); font-family: var(--mono);
  font-size: 13px; font-weight: 600;
}
.layout-b .dashboard .search-bar .chev {
  background: transparent; border: 0; color: var(--accent);
  font-size: 11px; cursor: pointer; padding: 0;
}
.layout-b .dashboard .search-bar .shortcut {
  color: var(--text-faint); font-size: 10px; font-family: var(--mono);
  background: var(--surface-2); border: 1px solid var(--border);
  border-radius: 2px; padding: 1px 4px;
}
```

- [ ] **Step 3: Run tests**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages vitest run src/search/SearchBar.test.tsx`
Expected: PASS (6 tests).

- [ ] **Step 4: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src/search/SearchBar.tsx src/search/SearchBar.css src/search/SearchBar.test.tsx
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): SearchBar component (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 15: `SearchDropdown` component

**Spec:** §3.3 (saved + recent sections, keyboard nav, footer).

**Files:**
- Create: `src/search/SearchDropdown.tsx`, `src/search/SearchDropdown.test.tsx`, `src/search/SearchDropdown.css`

- [ ] **Step 1: Write the failing test**

In `src/search/SearchDropdown.test.tsx`:
```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';
import { SearchDropdown } from './SearchDropdown';
import { EMPTY_SPEC, type RecentSearch, type SavedSearch } from './types';

const saved: SavedSearch[] = [
  { id: '1', name: 'Storm Net 5/30', spec: EMPTY_SPEC, created_at: 0, last_used_at: null, order: 0 },
  { id: '2', name: 'ICS-213 last 24h', spec: EMPTY_SPEC, created_at: 0, last_used_at: null, order: 1 },
];
const recent: RecentSearch[] = [
  { spec: { ...EMPTY_SPEC, free_text: 'outage' }, ran_at: 100 },
  { spec: { ...EMPTY_SPEC, free_text: 'weather' }, ran_at: 50 },
];

describe('SearchDropdown', () => {
  it('renders saved section above recent section', () => {
    render(<SearchDropdown saved={saved} recent={recent} activeSavedId={null} onRunSaved={() => {}} onRunRecent={() => {}} onPromoteRecent={() => {}} onUnsaveActive={() => {}} onManage={() => {}} onClose={() => {}} />);
    const labels = screen.getAllByTestId(/section-label/);
    expect(labels[0]).toHaveTextContent(/Saved/);
    expect(labels[1]).toHaveTextContent(/Recent/);
  });

  it('clicking a saved row calls onRunSaved with that saved-search', () => {
    const onRunSaved = vi.fn();
    render(<SearchDropdown saved={saved} recent={recent} activeSavedId={null} onRunSaved={onRunSaved} onRunRecent={() => {}} onPromoteRecent={() => {}} onUnsaveActive={() => {}} onManage={() => {}} onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('dropdown-saved-row-1'));
    expect(onRunSaved).toHaveBeenCalledWith(saved[0]);
  });

  it('clicking ☆ on a recent row promotes it', () => {
    const onPromote = vi.fn();
    render(<SearchDropdown saved={saved} recent={recent} activeSavedId={null} onRunSaved={() => {}} onRunRecent={() => {}} onPromoteRecent={onPromote} onUnsaveActive={() => {}} onManage={() => {}} onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('dropdown-recent-star-0'));
    expect(onPromote).toHaveBeenCalledWith(recent[0]);
  });

  it('arrow-down then enter runs the focused row', () => {
    const onRunSaved = vi.fn();
    render(<SearchDropdown saved={saved} recent={recent} activeSavedId={null} onRunSaved={onRunSaved} onRunRecent={() => {}} onPromoteRecent={() => {}} onUnsaveActive={() => {}} onManage={() => {}} onClose={() => {}} />);
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(onRunSaved).toHaveBeenCalled();
  });

  it('clicking Manage calls onManage', () => {
    const onManage = vi.fn();
    render(<SearchDropdown saved={saved} recent={recent} activeSavedId={null} onRunSaved={() => {}} onRunRecent={() => {}} onPromoteRecent={() => {}} onUnsaveActive={() => {}} onManage={onManage} onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('dropdown-manage'));
    expect(onManage).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Implement `SearchDropdown.tsx`**

```tsx
import React, { useEffect, useState } from 'react';
import './SearchDropdown.css';
import { renderQuery } from './queryRender';
import type { RecentSearch, SavedSearch } from './types';

export interface SearchDropdownProps {
  saved: SavedSearch[];
  recent: RecentSearch[];
  activeSavedId: string | null;
  onRunSaved: (s: SavedSearch) => void;
  onRunRecent: (r: RecentSearch) => void;
  onPromoteRecent: (r: RecentSearch) => void;
  onUnsaveActive: () => void;
  onManage: () => void;
  onClose: () => void;
}

export function SearchDropdown(props: SearchDropdownProps) {
  const { saved, recent, activeSavedId, onRunSaved, onRunRecent, onPromoteRecent, onManage, onClose } = props;
  const totalRows = saved.length + recent.length;
  const [focusIdx, setFocusIdx] = useState(0);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown') { e.preventDefault(); setFocusIdx((i) => Math.min(i + 1, totalRows - 1)); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); setFocusIdx((i) => Math.max(i - 1, 0)); }
      else if (e.key === 'Enter') {
        e.preventDefault();
        if (focusIdx < saved.length) onRunSaved(saved[focusIdx]);
        else onRunRecent(recent[focusIdx - saved.length]);
      } else if (e.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [focusIdx, saved, recent, totalRows, onRunSaved, onRunRecent, onClose]);

  return (
    <div className="search-dropdown" data-testid="search-dropdown">
      <div className="dropdown-section-label" data-testid="section-label-saved">
        Saved {saved.length > 0 && <span className="muted">(pinned)</span>}
      </div>
      {saved.length === 0 && <div className="dropdown-empty">No saved searches yet — star a recent one to save it.</div>}
      {saved.map((s, i) => (
        <div
          key={s.id}
          className={`dropdown-row${focusIdx === i ? ' focused' : ''}${s.id === activeSavedId ? ' active' : ''}`}
          data-testid={`dropdown-saved-row-${s.id}`}
          onClick={() => onRunSaved(s)}
        >
          <span className="star filled" aria-hidden="true">★</span>
          <div className="body">
            <span className="name">{s.name}</span>
            <span className="query">{renderQuery(s.spec)}</span>
          </div>
        </div>
      ))}

      <div className="dropdown-section-label" data-testid="section-label-recent">Recent</div>
      {recent.length === 0 && <div className="dropdown-empty">No recent searches yet.</div>}
      {recent.map((r, i) => {
        const idx = saved.length + i;
        return (
          <div
            key={`recent-${i}`}
            className={`dropdown-row unsaved${focusIdx === idx ? ' focused' : ''}`}
            data-testid={`dropdown-recent-row-${i}`}
            onClick={() => onRunRecent(r)}
          >
            <button
              type="button"
              className="star empty"
              data-testid={`dropdown-recent-star-${i}`}
              aria-label="Star to save"
              onClick={(e) => { e.stopPropagation(); onPromoteRecent(r); }}
            >☆</button>
            <div className="body"><span className="name">{renderQuery(r.spec)}</span></div>
          </div>
        );
      })}

      <div className="dropdown-footer">
        <span className="hints">↑↓ navigate · ⏎ run · Esc close</span>
        <button type="button" className="action" data-testid="dropdown-manage" onClick={onManage}>Manage… ⚙</button>
      </div>
    </div>
  );
}
```

In `src/search/SearchDropdown.css`:
```css
.layout-b .search-dropdown {
  position: absolute; top: 100%; left: 0; width: 480px;
  background: var(--surface-2); border: 1px solid var(--border-strong);
  border-radius: 4px; box-shadow: 0 14px 32px rgba(0,0,0,.65); z-index: 30;
  overflow: hidden; margin-top: 4px;
}
.layout-b .search-dropdown .dropdown-section-label {
  font-size: 9px; text-transform: uppercase; letter-spacing: .12em;
  color: var(--text-faint); padding: 9px 14px 5px;
}
.layout-b .search-dropdown .muted { color: var(--text-faint); text-transform: none; letter-spacing: 0; margin-left: 4px; }
.layout-b .search-dropdown .dropdown-row {
  padding: 7px 14px; display: grid; grid-template-columns: 16px 1fr;
  gap: 10px; align-items: center; cursor: pointer; font-size: 12px;
  color: var(--text-dim); border-bottom: 1px solid var(--border);
}
.layout-b .search-dropdown .dropdown-row:hover,
.layout-b .search-dropdown .dropdown-row.focused {
  background: var(--accent-soft); color: var(--accent-2);
  border-left: 2px solid var(--accent); padding-left: 12px;
}
.layout-b .search-dropdown .dropdown-row.active { background: var(--accent-soft); }
.layout-b .search-dropdown .dropdown-row .star { color: var(--accent); font-size: 13px; background: transparent; border: 0; padding: 0; cursor: pointer; }
.layout-b .search-dropdown .dropdown-row .star.empty { color: var(--border-strong); }
.layout-b .search-dropdown .dropdown-row .star.empty:hover { color: var(--accent); }
.layout-b .search-dropdown .dropdown-row .body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.layout-b .search-dropdown .dropdown-row .name { font-size: 12px; color: var(--text); font-weight: 500; }
.layout-b .search-dropdown .dropdown-row.unsaved .name { color: var(--text-dim); font-family: var(--mono); font-size: 11px; font-weight: 400; }
.layout-b .search-dropdown .dropdown-row .query { font-size: 10px; color: var(--text-faint); font-family: var(--mono); }
.layout-b .search-dropdown .dropdown-empty { padding: 12px 14px; font-size: 11px; color: var(--text-faint); font-style: italic; }
.layout-b .search-dropdown .dropdown-footer {
  padding: 8px 14px; background: var(--surface);
  border-top: 1px solid var(--border-strong);
  display: flex; justify-content: space-between; font-size: 11px; color: var(--text-faint);
}
.layout-b .search-dropdown .dropdown-footer .action {
  background: transparent; border: 0; color: var(--accent); cursor: pointer; padding: 0;
}
```

- [ ] **Step 3: Run tests**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages vitest run src/search/SearchDropdown.test.tsx`
Expected: PASS (5 tests).

- [ ] **Step 4: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src/search/SearchDropdown.tsx src/search/SearchDropdown.css src/search/SearchDropdown.test.tsx
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): SearchDropdown with keyboard nav + promote-from-recent (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 16: `ChipStrip` component

**Spec:** §3.2 (always-visible strip, active + ghost chips, far-right meta), §4 (chip vocabulary).

**Files:**
- Create: `src/search/ChipStrip.tsx`, `src/search/ChipStrip.test.tsx`, `src/search/ChipStrip.css`

- [ ] **Step 1: Write the failing test**

In `src/search/ChipStrip.test.tsx`:
```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';
import { ChipStrip } from './ChipStrip';
import { EMPTY_SPEC } from './types';

describe('ChipStrip', () => {
  it('renders the empty-state placeholder + all ghost chips when no filters', () => {
    render(<ChipStrip spec={EMPTY_SPEC} onSpecChange={() => {}} metaText={null} />);
    expect(screen.getByTestId('chipstrip-empty')).toBeInTheDocument();
    expect(screen.getAllByTestId(/^chip-ghost-/)).toHaveLength(9);
  });

  it('renders an active chip with an × that removes it', () => {
    const onSpecChange = vi.fn();
    render(<ChipStrip
      spec={{ ...EMPTY_SPEC, filters: { from: { kind: 'addr', value: 'KX5DD' } } }}
      onSpecChange={onSpecChange}
      metaText={null}
    />);
    expect(screen.getByTestId('chip-active-from')).toHaveTextContent('FROM:KX5DD');
    fireEvent.click(screen.getByTestId('chip-x-from'));
    expect(onSpecChange).toHaveBeenCalledWith(expect.objectContaining({ filters: {} }));
  });

  it('renders meta-text on the far right', () => {
    render(<ChipStrip spec={EMPTY_SPEC} onSpecChange={() => {}} metaText="3 matches · 47 ms · ★ Storm Net" />);
    expect(screen.getByTestId('chipstrip-meta')).toHaveTextContent('3 matches · 47 ms · ★ Storm Net');
  });
});
```

- [ ] **Step 2: Implement `ChipStrip.tsx`**

```tsx
import React from 'react';
import './ChipStrip.css';
import type { FilterKey, FilterValue, QuerySpec } from './types';

export interface ChipStripProps {
  spec: QuerySpec;
  onSpecChange: (spec: QuerySpec) => void;
  metaText: string | null;
}

const ALL_KEYS: FilterKey[] = [
  'folder', 'from', 'to', 'date-range', 'form-type',
  'has-form', 'has-attach', 'read-state', 'transport',
];

function chipLabel(key: FilterKey, v: FilterValue): string {
  switch (v.kind) {
    case 'addr':       return `${key.toUpperCase()}:${v.value}`;
    case 'folder':     return `FOLDER:${v.value}`;
    case 'form-type':  return `FORM-TYPE:${v.value}`;
    case 'transport':  return `TRANSPORT:${v.value}`;
    case 'bool':       return `${key.toUpperCase()}`;
    case 'read-state': return `READ-STATE:${v.value}`;
    case 'date-range': {
      const f = v.value.from != null ? new Date(v.value.from * 1000).toISOString().slice(0, 10) : '*';
      const t = v.value.to   != null ? new Date(v.value.to   * 1000).toISOString().slice(0, 10) : '*';
      return `DATE:${f}..${t}`;
    }
  }
}

export function ChipStrip({ spec, onSpecChange, metaText }: ChipStripProps) {
  const activeKeys = Object.keys(spec.filters) as FilterKey[];
  const inactiveKeys = ALL_KEYS.filter((k) => !activeKeys.includes(k));
  const isEmpty = activeKeys.length === 0 && !(spec.free_text && spec.free_text.trim());

  const removeChip = (key: FilterKey) => {
    const filters = { ...spec.filters };
    delete filters[key];
    onSpecChange({ ...spec, filters });
  };

  return (
    <div className="chip-strip" data-testid="chip-strip">
      {isEmpty && <span className="empty-prefix" data-testid="chipstrip-empty">No active filter — click + to add</span>}
      {!isEmpty && <span className="label-prefix">Filters:</span>}
      {activeKeys.map((k) => {
        const v = spec.filters[k]!;
        return (
          <span className="chip active" key={`active-${k}`} data-testid={`chip-active-${k}`}>
            {chipLabel(k, v)}
            <button
              type="button"
              className="x"
              data-testid={`chip-x-${k}`}
              aria-label={`Remove ${k} filter`}
              onClick={() => removeChip(k)}
            >×</button>
          </span>
        );
      })}
      {inactiveKeys.map((k) => (
        <span className="chip inactive" key={`ghost-${k}`} data-testid={`chip-ghost-${k}`}>
          + {k.toUpperCase()}
        </span>
      ))}
      <span className="meta" data-testid="chipstrip-meta">{metaText ?? ''}</span>
    </div>
  );
}
```

In `src/search/ChipStrip.css`:
```css
.layout-b .chip-strip {
  background: var(--bg); border-bottom: 1px solid var(--border);
  padding: 8px 18px; display: flex; flex-wrap: wrap;
  align-items: center; gap: 6px; min-height: 30px;
}
.layout-b .chip-strip .label-prefix,
.layout-b .chip-strip .empty-prefix {
  color: var(--text-faint); font-size: 10px; margin-right: 6px;
  text-transform: uppercase; letter-spacing: .08em;
}
.layout-b .chip-strip .chip {
  font-size: 10px; padding: 2px 7px 2px 8px; border-radius: 9px;
  font-family: var(--mono); display: inline-flex; align-items: center; gap: 4px;
}
.layout-b .chip-strip .chip.active {
  background: var(--accent-soft); color: var(--accent-2);
  border: 1px solid color-mix(in srgb, var(--accent) 32%, transparent);
}
.layout-b .chip-strip .chip.inactive {
  background: transparent; color: var(--text-faint);
  border: 1px solid var(--border-strong); font-family: inherit;
}
.layout-b .chip-strip .chip .x {
  background: transparent; border: 0; color: var(--text-faint); cursor: pointer;
  padding: 0; font-size: 11px;
}
.layout-b .chip-strip .meta {
  margin-left: auto; color: var(--text-faint); font-size: 10px; font-family: var(--mono);
}
```

- [ ] **Step 3: Run tests**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages vitest run src/search/ChipStrip.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src/search/ChipStrip.tsx src/search/ChipStrip.css src/search/ChipStrip.test.tsx
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): ChipStrip component with active/ghost chips + meta (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 17: Wire `SearchBar` + `ChipStrip` into `AppShell` + extend `MessageList`

**Spec:** §3.1 (ribbon hosts SearchBar in the leading slot, dash items right), §3.2 (chip strip below ribbon), §7.2 (grid-template-rows grows by one auto; row-1fr panes invariant verified), §7.2 (MessageList gains `matchHighlights` + `showFolderTag` props — additive only).

**Files:**
- Modify: `src/shell/AppShell.tsx`, `src/shell/AppShell.css`
- Modify: `src/mailbox/MessageList.tsx`, `src/mailbox/MessageList.test.tsx`
- Modify: `src/mailbox/types.ts` (optional folder field on `MessageMeta` for cross-folder display)

- [ ] **Step 1: Write the failing AppShell test**

In `src/shell/AppShell.test.tsx` (extend existing), append:
```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { AppShell } from './AppShell';

function wrap(children: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe('AppShell — find-messages wiring', () => {
  it('renders the SearchBar in the ribbon', () => {
    render(wrap(<AppShell />));
    expect(screen.getByTestId('search-bar')).toBeInTheDocument();
  });

  it('renders the ChipStrip below the ribbon', () => {
    render(wrap(<AppShell />));
    expect(screen.getByTestId('chip-strip')).toBeInTheDocument();
  });

  it('dashboard dash-items still render (right-clustered)', () => {
    render(wrap(<AppShell />));
    expect(screen.getByText(/Callsign/)).toBeInTheDocument();
    expect(screen.getByText(/CMS/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Modify `AppShell.tsx`**

In the existing dashboard JSX (the `.dashboard` div), restructure to two flex-children:
```tsx
import { SearchBar } from '../search/SearchBar';
import { SearchDropdown } from '../search/SearchDropdown';
import { ChipStrip } from '../search/ChipStrip';
import { useSearch } from '../search/useSearch';
import { useSavedSearches } from '../search/useSavedSearches';
import { renderQuery } from '../search/queryRender';

// inside AppShell():
const search = useSearch();
const saved = useSavedSearches();
const [dropdownOpen, setDropdownOpen] = React.useState(false);

const metaText = (() => {
  if (!search.isActive) return null;
  const r = search.results;
  if (!r) return search.isLoading ? 'Searching…' : null;
  const star = search.activeSaved ? ` · ★ ${search.activeSaved.name}` : '';
  return `${r.totalMatches} matches · ${r.queryMs} ms${star}`;
})();

// dashboard JSX (replaces existing flat structure):
<div className="dashboard">
  <div className="search-zone">
    <SearchBar
      spec={search.spec}
      activeSaved={search.activeSaved}
      onSpecChange={search.setSpec}
      onUnsave={async () => {
        if (search.activeSaved) {
          await saved.unsave(search.activeSaved.id);
          search.setActiveSavedSearch(null);
        }
      }}
      onToggleDropdown={() => setDropdownOpen((o) => !o)}
      dropdownOpen={dropdownOpen}
    />
    {dropdownOpen && (
      <SearchDropdown
        saved={saved.saved}
        recent={saved.recent}
        activeSavedId={search.activeSaved?.id ?? null}
        onRunSaved={(s) => { search.setActiveSavedSearch(s); setDropdownOpen(false); }}
        onRunRecent={(r) => { search.setSpec(r.spec); setDropdownOpen(false); }}
        onPromoteRecent={async (r) => { const name = window.prompt('Name for this saved search?', renderQuery(r.spec).slice(0, 24)); if (name) await saved.save(name, r.spec); }}
        onUnsaveActive={async () => { if (search.activeSaved) await saved.unsave(search.activeSaved.id); }}
        onManage={() => { /* open Settings → Saved Searches (Task 18) */ setDropdownOpen(false); }}
        onClose={() => setDropdownOpen(false)}
      />
    )}
  </div>
  <div className="right-cluster">
    {/* existing dash-items (Callsign, Grid, UTC, CMS) — same JSX as today */}
    {/* … */}
  </div>
</div>

{/* directly under .dashboard, BEFORE .panes: */}
<ChipStrip
  spec={search.spec}
  onSpecChange={search.setSpec}
  metaText={metaText}
/>
```

- [ ] **Step 3: Modify `AppShell.css`**

Update the layout-b grid template rows (CSS comment at lines ~18-22 plus the rule at line 25):
```css
/* grid template now has 7 rows: titlebar / menubar / dashboard / chipstrip /
   panes(1fr) / session-log / statusbar. ResizeHandles stays position:absolute
   so it does NOT consume a grid row. */
.layout-b {
  display: grid;
  grid-template-rows: auto auto auto auto 1fr auto auto;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
  background: var(--bg);
  color: var(--text);
  position: relative;
}
```

Adjust the `.dashboard` rule: change its child layout to split `search-zone` (left) and `right-cluster` (right). The right cluster keeps the existing dash-item rendering exactly as-is:
```css
.layout-b .dashboard .search-zone { position: relative; flex-shrink: 0; }
.layout-b .dashboard .right-cluster { margin-left: auto; display: flex; align-items: center; gap: 22px; }
```

- [ ] **Step 4: Modify `MessageList.tsx` + types**

In `src/mailbox/types.ts`, extend `MessageMeta`:
```ts
export interface MessageMeta {
  // ...existing fields...
  /// When search results are cross-folder, the folder is rendered as a small
  /// badge inline-left of the subject. Absent → no badge (current behavior).
  folder?: MailboxFolder;
}
```

In `src/mailbox/MessageList.tsx`, extend the props:
```tsx
export interface MessageRowProps {
  // ...existing...
  matchHighlight?: HighlightRange[];
  showFolderTag?: boolean;
}

export interface HighlightRange {
  field: 'subject' | 'preview';
  start: number;
  end: number;
}

export interface MessageListProps {
  // ...existing...
  matchHighlights?: Record<string, HighlightRange[]>;     // mid → ranges
  showFolderTag?: boolean;
}
```

In the row JSX, render an inline-left folder badge when `showFolderTag` is true and `message.folder` is set, and replace plain subject/preview text with a `renderWithMark()` helper that splits the string on the `HighlightRange[]` and wraps matches in `<mark>`. If `matchHighlight` is absent, render exactly as today.

Add tests:
```tsx
it('renders <mark> around matched ranges when matchHighlight is provided', () => {
  const m = { /* fixture MessageMeta */, subject: 'DAMAGE report' } as MessageMeta;
  render(<MessageRow message={m} folder="inbox" selected={false} onSelect={() => {}}
                     matchHighlight={[{ field: 'subject', start: 0, end: 6 }]} />);
  expect(screen.getByTestId('row-subject').querySelector('mark')).toHaveTextContent('DAMAGE');
});

it('renders folder badge when showFolderTag and message.folder set', () => {
  const m = { /* fixture */, folder: 'sent' as const } as MessageMeta;
  render(<MessageRow message={m} folder="inbox" selected={false} onSelect={() => {}}
                     showFolderTag />);
  expect(screen.getByTestId('row-folder-tag')).toHaveTextContent(/sent/i);
});
```

- [ ] **Step 5: Run tests**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages vitest run src/shell/AppShell.test.tsx src/mailbox/MessageList.test.tsx`
Expected: existing passing tests still PASS + new tests PASS.

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages exec tsc --noEmit`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src/shell/AppShell.tsx src/shell/AppShell.css src/shell/AppShell.test.tsx src/mailbox/MessageList.tsx src/mailbox/MessageList.test.tsx src/mailbox/types.ts
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): wire SearchBar+ChipStrip into AppShell ribbon; MessageList highlight+folder-tag (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 18: `SavedSearchesPanel` (Settings tab)

**Spec:** §3.5 (Settings panel structure + Maintenance/Rebuild button).

**Files:**
- Create: `src/search/SavedSearchesPanel.tsx`, `src/search/SavedSearchesPanel.test.tsx`, `src/search/SavedSearchesPanel.css`
- Modify: Settings panel host (locate via `grep -r SettingsPanel src/` — the spec calls it out as TBD-find; typical names are `SettingsPanel.tsx` or `SettingsOverlay.tsx`).

- [ ] **Step 1: Locate the Settings host + write the failing test**

Run: `grep -rn 'SettingsPanel\|Settings.*Tab\|SettingsOverlay' /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src/ | head -10`
Expected: identifies the file that owns Settings tabbing. (If no Settings host exists yet, add the panel to AppShell's existing settings area or render inline — verify with operator before scoping a new host file.)

Write `src/search/SavedSearchesPanel.test.tsx` covering:
- renders the list of saved searches with names
- "+ New saved search" expands an inline form
- inline rename triggers `onRename`
- "Rebuild search index" button triggers `onRebuild`
- drag-handle reorder triggers `onReorder` with new order

- [ ] **Step 2: Implement `SavedSearchesPanel.tsx`**

Renders:
- A list of `SavedSearch` rows: drag-handle (`⋮⋮`) · name (inline-editable on click) · query preview · `Run now` button · trash icon (unsave with confirm).
- "+ New saved search" expander: name input + free-text input + chip composer (re-use `ChipStrip` in disabled-meta mode) + Save button.
- A "Maintenance" subsection with "Rebuild search index — runs in <N> seconds" button that calls `useSavedSearches().rebuildIndex()` and shows the returned stats.

Component reads from `useSavedSearches()` for data + mutations.

- [ ] **Step 3: Wire the Manage… link**

In `AppShell.tsx` (modified in Task 17), implement the `onManage` callback to open the Settings panel with the Saved Searches tab active. The exact mechanism depends on the host located in Step 1 — likely a setter from a `SettingsContext` or a `setActiveSettingsTab('search')` call.

- [ ] **Step 4: Run tests + type-check**

Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages vitest run src/search/SavedSearchesPanel.test.tsx`
Run: `pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages exec tsc --noEmit`
Expected: both clean.

- [ ] **Step 5: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src/search/SavedSearchesPanel.tsx src/search/SavedSearchesPanel.css src/search/SavedSearchesPanel.test.tsx src/shell/AppShell.tsx
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "feat(search): SavedSearchesPanel Settings tab + Manage link wiring (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 19: End-to-end Rust integration test

**Spec:** §9.1 (integration test — full roundtrip).

**Files:**
- Create: `src-tauri/tests/search_integration.rs`

- [ ] **Step 1: Write the failing integration test**

```rust
//! End-to-end: store N messages via the mailbox (with the Index attached),
//! then exercise every chip + free-text path. Each test owns a tempdir so
//! they run in parallel.

use std::sync::Arc;
use tempfile::tempdir;
use tuxlink_lib::native_mailbox::Mailbox;
use tuxlink_lib::search::commands::SearchService;
use tuxlink_lib::search::index::Index;
use tuxlink_lib::search::saved::SavedStore;
use tuxlink_lib::search::types::{FilterKey, FilterValue, QuerySpec};
use tuxlink_lib::winlink::compose::compose_message;
use tuxlink_lib::winlink_backend::MailboxFolder;
use std::collections::BTreeMap;

fn raw(from: &str, to: &[&str], subject: &str, body: &str, secs: u32) -> Vec<u8> {
    compose_message(from, to, &[], subject, body, secs).to_bytes()
}

fn build(dir: &std::path::Path) -> (Mailbox, SearchService) {
    let idx = Arc::new(Index::open(dir.join("search.db")).unwrap());
    let mbox = Mailbox::new(dir.to_path_buf()).with_index(idx.clone());
    let svc = SearchService {
        index: idx,
        saved: std::sync::Mutex::new(SavedStore::open(dir.join("saved.json")).unwrap()),
        now_unix: || 1_716_200_000,
    };
    (mbox, svc)
}

#[test]
fn freetext_finds_match_across_inbox_and_sent() {
    let dir = tempdir().unwrap();
    let (mbox, svc) = build(dir.path());
    mbox.store(MailboxFolder::Inbox, &raw("KX5DD", &["N7CPZ"], "DAMAGE report", "powerlines", 1_716_200_000)).unwrap();
    mbox.store(MailboxFolder::Sent,  &raw("N7CPZ", &["KX5DD"], "Re: damage", "ack", 1_716_200_100)).unwrap();
    let res = svc.run(QuerySpec { free_text: Some("damage".into()), ..QuerySpec::default() }).unwrap();
    assert_eq!(res.total_matches, 2);
}

#[test]
fn from_chip_narrows_by_sender() {
    let dir = tempdir().unwrap();
    let (mbox, svc) = build(dir.path());
    mbox.store(MailboxFolder::Inbox, &raw("KX5DD",  &["N7CPZ"], "x", "y", 1_716_200_000)).unwrap();
    mbox.store(MailboxFolder::Inbox, &raw("WX5RES", &["N7CPZ"], "x", "y", 1_716_200_100)).unwrap();
    let mut filters = BTreeMap::new();
    filters.insert(FilterKey::From, FilterValue::Addr("KX5DD".into()));
    let res = svc.run(QuerySpec { filters, ..QuerySpec::default() }).unwrap();
    assert_eq!(res.total_matches, 1);
}

#[test]
fn mark_read_propagates_to_unread_filter() {
    let dir = tempdir().unwrap();
    let (mbox, svc) = build(dir.path());
    let id = mbox.store(MailboxFolder::Inbox, &raw("KX5DD", &["N7CPZ"], "x", "y", 1_716_200_000)).unwrap();
    let mut filters = BTreeMap::new();
    filters.insert(FilterKey::ReadState, FilterValue::ReadState(tuxlink_lib::search::types::ReadState::Unread));
    let before = svc.run(QuerySpec { filters: filters.clone(), ..QuerySpec::default() }).unwrap();
    assert_eq!(before.total_matches, 1);
    mbox.mark_read(MailboxFolder::Inbox, &id).unwrap();
    let after = svc.run(QuerySpec { filters, ..QuerySpec::default() }).unwrap();
    assert_eq!(after.total_matches, 0);
}

#[test]
fn rebuild_picks_up_pre_existing_mailbox() {
    let dir = tempdir().unwrap();
    // Phase 1: store without index attached
    {
        let mbox = Mailbox::new(dir.path().to_path_buf());
        mbox.store(MailboxFolder::Inbox, &raw("KX5DD", &["N7CPZ"], "DAMAGE report", "p", 1_716_200_000)).unwrap();
    }
    // Phase 2: attach index + rebuild
    let svc = SearchService {
        index: Arc::new(Index::open(dir.path().join("search.db")).unwrap()),
        saved: std::sync::Mutex::new(SavedStore::open(dir.path().join("saved.json")).unwrap()),
        now_unix: || 1_716_200_000,
    };
    let stats = svc.rebuild_index(dir.path().to_path_buf()).unwrap();
    assert_eq!(stats.messages_indexed, 1);
}
```

(`tuxlink_lib` is the library crate name from `Cargo.toml` `[lib] name = "tuxlink_lib"`.)

- [ ] **Step 2: Run + commit**

Run: `cargo test --manifest-path /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages/src-tauri/Cargo.toml --test search_integration`
Expected: PASS (4 tests).

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add src-tauri/tests/search_integration.rs
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "test(search): end-to-end mailbox → index → search integration (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 20: Operator UI smoke

**Spec:** §9.3 (end-to-end smoke).

**Files:**
- Create: `dev/smoke/find-messages-smoke.md`

- [ ] **Step 1: Write the smoke script**

In `dev/smoke/find-messages-smoke.md`:
```markdown
# find-messages — operator smoke (tuxlink-1hu)

Branch: `bd-tuxlink-1hu/find-messages`. Build + run:

\`\`\`bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages
pnpm install
pnpm tauri dev
\`\`\`

Smoke checks (✓/✗):

1. Shell opens; the SearchBar is visible in the ribbon (left); Callsign/Grid/UTC/CMS cluster on the right; chip strip below the ribbon shows the empty-state placeholder.
2. Press ⌘F (or Ctrl-F): focus jumps to the SearchBar input.
3. Type "damage": after ~150ms the rows pane filters to matches. Chip-strip meta shows match count + ms.
4. Click a folder in the sidebar: filter remains active; results stay cross-folder until the search is cleared.
5. Click the ★ in the dropdown footer's `Manage…` → Settings → Saved Searches → "+ New saved search" → name "Storm Net Test", free-text "damage". Save.
6. Open the search bar dropdown: "Storm Net Test" appears at the top of the Saved section. Click it → input shows "★ Storm Net Test"; rows pane shows damage matches; meta shows "★ Storm Net Test".
7. Click the filled ★ next to the saved name → un-saves; spec reverts to a one-off "damage" query.
8. Type a new query "weather"; close the dropdown; observe results. Reopen dropdown — the new query appears as a Recent entry.
9. Click the ☆ on a Recent entry → name prompt → name "Weather Quick"; verify it appears in Saved.
10. Settings → Saved Searches → Maintenance → "Rebuild search index": stats banner shows `<N> messages indexed in <ms>`.
11. Quit the app; relaunch: saved searches persist; recent searches persist (up to cap=20).

If any step fails: capture screenshot to `dev/scratch/find-messages-smoke-<step>.png`, file a bd issue, and decide whether to merge with a known-issues entry or block.
```

- [ ] **Step 2: Run the smoke (operator)**

Operator runs each numbered step in order. The agent assists with diagnostics on failures but does NOT run live UI checks (per `browser_smoke_before_ship` memory — agent ensures the build runs cleanly; the operator drives the actual UI exercise).

- [ ] **Step 3: Commit**

```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages add dev/smoke/find-messages-smoke.md
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages commit -m "docs(smoke): find-messages operator smoke checklist (tuxlink-1hu)

Agent: <session moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 21: Codex adrev round + PR

**Spec:** Process per `no_carveout_on_cross_provider_adrev` memory — the 5-round cross-provider Codex adrev is the unique value; **do not skip**. Plumbing-y plans may carve out per `discipline_triage_rule`, but find-messages is **new subsystem** scope, not plumbing.

**Files:** none — process step.

- [ ] **Step 1: Run Codex adrev against the uncommitted/staged branch state at the end of Task 19**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-1hu-find-messages
npx --yes @openai/codex review --base main \
  "Review find-messages v0.1 capability per docs/design/2026-05-30-find-messages-design.md.
   Attack angles: (1) index/mailbox consistency under partial-write failures;
   (2) FTS5 query syntax injection or DoS; (3) saved-searches JSON corruption recovery;
   (4) schema-drift detection correctness; (5) cross-folder query result ordering;
   (6) ChipStrip empty-state UX vs the spec's §3.2 layout-stability requirement." \
  2>&1 | tee dev/adversarial/2026-05-30-find-messages-codex.md
```

- [ ] **Step 2: Triage findings**

For each Codex finding, mark `accept` / `defer-to-v0.2` / `wontfix` inline in the adrev transcript. Land `accept`s as additional commits on this branch. `defer-to-v0.2` becomes a new bd issue linked to `tuxlink-1hu`.

- [ ] **Step 3: Open PR**

```bash
gh pr create --base main --head bd-tuxlink-1hu/find-messages \
  --title "[<moniker>] feat: v0.1 capability 1.15 find-messages (FTS5)" \
  --body "$(cat <<'EOF'
## Summary
- v0.1 message-action capability 1.15 — full-text search over the
  filesystem-canonical mailbox via derived SQLite FTS5.
- UI: ribbon search bar (Option E) with combined saved + recent dropdown;
  always-visible chip strip below the ribbon; folders-only sidebar.
- Backend: synchronous index hooks in `Mailbox::store`/`move_to`/`mark_read`;
  explicit `rebuild-index` command.

Design: `docs/design/2026-05-30-find-messages-design.md`
Plan:   `docs/superpowers/plans/2026-05-30-find-messages.md`
Adrev:  `dev/adversarial/2026-05-30-find-messages-codex.md` (local-only)
bd:     tuxlink-1hu

## Test plan
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` (Rust unit + integration)
- [ ] `pnpm vitest run src/search` (TS components + hooks)
- [ ] `pnpm exec tsc --noEmit` (types clean)
- [ ] `pnpm tauri dev` smoke per `dev/smoke/find-messages-smoke.md` (operator)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Merge after operator smoke green-lights**

```bash
gh pr merge <PR#> --merge --delete-branch
```

Per ADR 0010 (no-squash). The merge-commit preserves every task-branch commit.

- [ ] **Step 5: Close `tuxlink-1hu`**

```bash
bd close tuxlink-1hu
bd remember "find-messages v0.1 shipped (PR #<n>): FTS5 over filesystem mailbox + Option E UI + saved/recent dropdown"
```

- [ ] **Step 6: Dispose the worktree per ADR 0009 ritual**

(Standard disposal — inventory + propagate-or-archive + rm -rf + git worktree prune. See CLAUDE.md §"Worktree disposal ritual".)

---

## Self-review

**Spec coverage check** (each §N in `docs/design/2026-05-30-find-messages-design.md`):

- §1 (goal / filesystem-canonical) → Tasks 9 + 19 enforce that mailbox writes always succeed.
- §2 settled decisions Q1–Q7 → architecture in the File map + every backend task.
- §3.1 ribbon search bar → Task 14 + Task 17 wiring.
- §3.2 chip strip → Task 16 + Task 17 wiring.
- §3.3 search dropdown → Task 15.
- §3.4 sidebar unchanged → no task needed (intentional non-change; flagged in the File map).
- §3.5 Settings → Saved Searches panel → Task 18.
- §4 chip vocabulary → Task 6 (compose) + Task 16 (render).
- §5.1 storage location → Task 10 (`build_service` uses `<data_dir>/search.db`).
- §5.2 schema → Task 2.
- §6.1 module layout → Task 1.
- §6.2 mailbox hooks → Task 9.
- §6.3 rebuild-index → Task 10.
- §6.4 command surface → Task 8 (`SearchService`) + Task 10 (Tauri wrappers).
- §7.1–7.4 frontend module layout, shell wiring, state model, persistence → Tasks 11–18.
- §8 error handling table → covered piecemeal: FTS5 syntax in Task 8's `CommandError`; index-upsert failure isolation in Task 9; schema drift in Task 2 + automatic rebuild in Task 10; malformed JSON in Task 7.
- §9.1–9.3 testing → Tasks 2/4/5/6/7/8/9/10 (Rust units), Tasks 11–18 (TS units), Task 19 (integration), Task 20 (operator smoke).
- §10 deferrals → all v0.2+ items remain out of scope (no tasks); deferral list lives in the spec, not the plan.
- §11 known-unknowns → live match-counts referenced in spec §10 + §11; no plan task (intentional — v0.2 design).

**Placeholder scan:** no `TBD` / `TODO` / "implement later" / "similar to Task N" patterns remain. Task 18's "locate the Settings host" is genuine (the spec acknowledges the panel-location verification step); the test exists, the implementation pattern is concrete.

**Type consistency:** `QuerySpec` / `FilterKey` / `FilterValue` shape matches between Rust types (Task 3 — kebab-case serde tags) and TS types (Task 11 — matching string literals). `SearchResults`/`MessageMetaDto` use camelCase via `#[serde(rename_all = "camelCase")]` on the Rust side (Task 8) which TS receives natively. `SavedSearch` keeps snake_case (`created_at` etc.) because the JSON saved-store file is human-readable. `RebuildStats` is camelCase per the Tauri command convention.

---

## Execution handoff

**Plan complete and saved to [`docs/superpowers/plans/2026-05-30-find-messages.md`](docs/superpowers/plans/2026-05-30-find-messages.md). Two execution options:**

**1. Subagent-Driven (recommended)** — fresh subagent per task, parent reviews between tasks, fast iteration. Each task in this plan is self-contained TDD; subagent dispatch is a clean fit.

**2. Inline Execution** — execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints for review.

Which approach?

