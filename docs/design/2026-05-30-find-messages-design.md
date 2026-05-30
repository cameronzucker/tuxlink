# Find-messages — design spec

> **Status:** design, pending operator review → `writing-plans`.
> **Date:** 2026-05-30 · **Agent:** finch-gulch-lichen · **Brainstorm:** continued from `fen-alder-bog` (this branch's first commit captures the paused state at Q4).
> **Capability:** v0.1 message-action **1.15 — Find messages** (per [`2026-05-29-winlink-express-feature-inventory.md`](2026-05-29-winlink-express-feature-inventory.md) §1.15).
> **Relates to:** [`tuxlink-1hu`](https://github.com/cameronzucker/tuxlink/issues) (this work). Coexists with — does not modify — the filesystem-backed mailbox ([`src-tauri/src/native_mailbox.rs`](../../src-tauri/src/native_mailbox.rs)).

## 1. Goal & context

Tuxlink is greenfield, so v0.1's find-messages is not bounded to RMS Express's
grep-headers floor (`F3` → in-folder substring scan against `From`/`To`/`Subject`).
Ship the floor plus structured-filter chips and saved searches, indexed by a
**derived** SQLite FTS5 store alongside the canonical filesystem mailbox.

Key constraint: the mailbox is **filesystem-canonical** by deliberate design
([`native_mailbox.rs:9-12`](../../src-tauri/src/native_mailbox.rs#L9-L12) — *"The on-disk format is deliberately simple (raw message bytes per file) and is ours, not Pat's"*).
Find-messages **must not migrate the mailbox into sqlite**. The FTS5 store is a
companion subsystem: regenerable from disk, hooked into the existing mailbox
write methods for incremental sync, and rebuildable on demand via a CLI/UI
command.

## 2. Settled decisions (brainstorm Q1–Q7)

| # | Question | Decision |
|---|---|---|
| Q1 | Scope ambition | Floor (`F3` parity) + structured filter chips + saved searches. |
| Q2 | Index scope | Headers + body + form-payload fields. Attachment text (PDF/CSV) **deferred to v0.5+** (multi-week parser-security surface). |
| Q3 | Storage backend | SQLite FTS5 in a derived `search.db` alongside the mailbox root. Tantivy is the v0.5 swap candidate if fuzzy / faceting earn their keep. |
| Q4 | Extraction surface | Conservative-max: index every field that's a plausible future filter. Re-index is cheap at v0.1 mailbox scale, so over-extracting now removes future re-index friction. |
| Q5 | Saved-search model | Pure query spec (`{free_text, filter_state}`). Each open **re-runs** against current corpus — no result snapshot. Archival served separately by an export-results action (deferred). |
| Q6 | UI placement (Option E) | Ribbon-mounted search bar + combined saved/recent dropdown + always-visible filter-chip strip below the ribbon + folders-only sidebar. Visual companion render archived in [`.superpowers/brainstorm/3079710-1780180290/content/q6-v3-search-dropdown.html`](../../.superpowers/brainstorm/3079710-1780180290/content/q6-v3-search-dropdown.html) (gitignored — see §11). |
| Q7 | Index sync strategy | In-process hooks in `Mailbox::store`, `move_to`, `mark_read` (synchronous index upsert) + an explicit `rebuild-index` command for migration / repair. No filesystem watcher (tuxlink is the sole writer). |

## 3. UI structure (Option E)

### 3.1 Ribbon-mounted search bar

The dashboard ribbon ([`AppShell.css:45-97`](../../src/shell/AppShell.css#L45-L97)) currently holds `Callsign · Grid · UTC · CMS · Connect` left-justified with substantial right-side negative space. We rebalance: a 420 px search input takes the leftmost slot; the existing dash-items cluster right-justified.

The input shows two modes:

- **Free-typed query** — raw text rendered in the input; placeholder when empty.
- **Active saved search** — saved-search **name** displayed (e.g. `★ Storm Net 5/30`) with a small query-preview alongside (`"damage" from:KX5DD date:7d`). The leading ★ is filled and clickable: click un-saves back to a one-off query.

Affordances:

- `⌘F` (Tauri menu binding) focuses the search input from anywhere in the shell.
- Trailing chevron `▾` opens the dropdown (§3.3).
- Esc clears the input and collapses the dropdown.

### 3.2 Filter chip strip (always visible)

A new full-width strip lives directly under the ribbon, above the panes. It renders:

- Active filter chips (filled, with `×` to remove) — orange accent.
- Ghost-chip "+ FIELD" affordances for inactive filters — click to add.
- Far-right meta: `<N matches · <ms> · ★ <saved-search-name>` (the saved-search name doubles as the "you're currently in a saved search" indicator).

The strip is **static-positioned** to avoid layout reflow when filters toggle. When empty (no query, no filters), it renders only the ghost-chip "+ Add filter" hints and a "no active filter" placeholder — the strip's height never changes between empty and populated states. The full chip vocabulary lives in §4.

### 3.3 Search dropdown (saved + recent, combined)

Opens on focus, `⌘F`, or chevron click. Two sections:

1. **Saved** — pinned starred entries, manually ordered (drag-reorder in Settings) or pre-created in Settings. Each row: ★ (filled), user-supplied name, raw query preview, last-run hint.
2. **Recent** — last `N=20` runs not promoted to Saved, chronological newest-first. Each row: ☆ (empty), raw query string, run-timestamp. Click ☆ → promotes to Saved with a `name` prompt (defaults to first chip + free-text).

Footer: keyboard hints (`↑↓ navigate · ⏎ run · ⌥⏎ open in new pane`) + a `Manage… ⚙` link → opens Settings → Saved Searches panel (§3.5).

Keyboard semantics:

- `↑/↓` navigate, `⏎` runs the focused row, `⌥⏎` opens it in a new pane (deferred to v0.2 — placeholder keystroke documented in v0.1 so users can discover it later).
- Typing while the dropdown is open filters the dropdown's saved+recent list (sub-search) and concurrently treats the input as a new query — the row labelled "Run as new search: `<text>`" appears as the active row when no saved/recent matches.

### 3.4 Folders sidebar — unchanged

The sidebar reverts to folders-only (the prior `Saved Searches` section sketched in mock B/D is dropped). Folder layout, counts, and selection behavior are untouched from [`FolderSidebar.tsx`](../../src/mailbox/FolderSidebar.tsx).

### 3.5 Settings → Saved Searches panel

A new tab in the existing Settings overlay/panel for **pre-create / rename / delete / reorder**. Layout: list with drag-handles; per-row inline-edit of name, query-spec preview, and a `Run now` button. "+ New saved search" expands an inline editor:

```
Name:        ___________________
Free text:   ___________________
Filters:     [+ Add chip]
```

The same per-row UI is reachable from the dropdown's `Manage… ⚙`.

## 4. Filter chip vocabulary

v0.1 ships with the **conservative-max** extraction surface (Q4) so chips are
purely a UI question — adding/removing chips later doesn't require re-indexing.
v0.1 surface (alphabetical):

| Chip | Source field | Notes |
|---|---|---|
| `DATE:<range>` | `date_received` (Inbox/Archive) or `date_sent` (Sent/Outbox) | UI exposes presets (`today / 1d / 7d / 30d / all`) and a custom `from-to`. |
| `FOLDER:<name>` | `folder` | Default `FOLDER:All`. Removing yields all folders. |
| `FORM-TYPE:<id>` | `form_type` | Discrete dropdown: `ICS-213 / ICS-309 / Position / Bulletin / DamageAssessment / <other>`. |
| `FROM:<addr>` | `from` | Free-text match; autocompletes from index. |
| `HAS-ATTACH` | `attachment_count > 0` | Boolean toggle. |
| `HAS-FORM` | `form_type IS NOT NULL` | Boolean toggle. Independent of `FORM-TYPE`. |
| `READ-STATE:<unread\|read>` | `unread` | Tri-state via repeat-click (unread → read → off). |
| `TO:<addr>` | `to` | Same UX as `FROM`. |
| `TRANSPORT:<id>` | `transport_used` | Discrete: `telnet / packet / vara-hf / vara-fm / ardop`. |

The free-text input itself maps to the FTS5 `MATCH` clause against the
`subject_body_form` virtual column (a concatenation of subject, body text, and
form field values — see §5.2). Filter chips translate to `WHERE` clauses on
indexed columns (non-FTS). Both are applied via the same query path.

## 5. Index schema (FTS5)

### 5.1 Storage location

`search.db` lives **alongside** the mailbox root. Concretely, if the mailbox is
at `$DATA_DIR/mail/`, the index is at `$DATA_DIR/search.db`. This keeps it
ignorable as derived state (a `search.db.lock`, `search.db-wal`, `search.db-shm`
sit beside it) and trivially deletable as the "nuke the index" repair gesture
that pairs with the `rebuild-index` command.

### 5.2 Schema

A single FTS5 virtual table plus a metadata-shadow table:

```sql
-- FTS5 virtual table — full-text-indexed columns
CREATE VIRTUAL TABLE messages_fts USING fts5 (
    mid           UNINDEXED,    -- message id (primary key, also in shadow)
    folder        UNINDEXED,    -- 'inbox' | 'outbox' | 'sent' | 'archive'
    subject,                    -- FTS-indexed
    body,                       -- FTS-indexed (decoded text/plain)
    form_field_values,          -- FTS-indexed (concatenated form payload values)
    tokenize = 'porter unicode61 remove_diacritics 2'
);

-- Shadow table — structured fields used by filter chips and sort
CREATE TABLE messages_meta (
    mid              TEXT PRIMARY KEY,
    folder           TEXT NOT NULL,
    from_addr        TEXT,
    to_addrs         TEXT,        -- JSON array
    cc_addrs         TEXT,        -- JSON array
    date_sent        INTEGER,     -- Unix epoch seconds, UTC
    date_received    INTEGER,     -- Unix epoch seconds, UTC
    unread           INTEGER NOT NULL DEFAULT 0,    -- 0/1
    form_type        TEXT,        -- e.g. 'ICS-213'; NULL if not a form
    has_attachments  INTEGER NOT NULL DEFAULT 0,    -- 0/1
    attachment_count INTEGER NOT NULL DEFAULT 0,
    transport_used   TEXT,        -- 'telnet' | 'packet' | ...
    direction        TEXT NOT NULL,    -- 'sent' | 'received'
    message_size     INTEGER NOT NULL,
    routing_path     TEXT,        -- e.g. 'via CMS-SSL'; freeform
    indexed_at       INTEGER NOT NULL    -- when this row was last upserted
);

CREATE INDEX idx_meta_date_recv ON messages_meta(date_received);
CREATE INDEX idx_meta_date_sent ON messages_meta(date_sent);
CREATE INDEX idx_meta_from      ON messages_meta(from_addr);
CREATE INDEX idx_meta_form_type ON messages_meta(form_type);
CREATE INDEX idx_meta_folder    ON messages_meta(folder);

-- Schema-version pragma so rebuild-index can detect drift
PRAGMA user_version = 1;
```

The FTS5 row and the meta row share a `mid` primary key. Search execution
joins them:

```sql
SELECT m.*
  FROM messages_fts AS f
  JOIN messages_meta AS m ON m.mid = f.mid
 WHERE messages_fts MATCH :fts_query        -- free-text query (NULL → omit clause)
   AND m.folder = :folder                   -- if FOLDER chip set (other than 'All')
   AND m.from_addr LIKE :from_glob          -- if FROM chip set
   AND m.date_received >= :date_from        -- if DATE range
   AND m.form_type = :form_type             -- if FORM-TYPE chip set
   -- … additional WHERE clauses per active chip
 ORDER BY COALESCE(m.date_received, m.date_sent) DESC
 LIMIT :page_size OFFSET :page_offset;
```

When no free-text is present, the FTS join is skipped entirely (pure metadata
filter, served from `messages_meta` alone).

## 6. Backend (Rust) — `src-tauri/src/search/`

### 6.1 Module layout

```
src-tauri/src/search/
    mod.rs           pub use surfaces
    index.rs         FTS5 schema, open + init + migrate, upsert, delete, query exec
    extractor.rs     Message → IndexRow + MetaRow
    query.rs         QuerySpec parsing + serialization, chip → SQL composition
    saved.rs         saved + recent JSON store (load, save, promote, prune)
    commands.rs      Tauri command handlers
```

### 6.2 Mailbox hooks (Q7)

`Mailbox::store`, `Mailbox::move_to`, and `Mailbox::mark_read` are extended
to call into a shared `Index` handle synchronously after the filesystem
mutation completes successfully:

```rust
impl Mailbox {
    pub fn store(&self, folder: MailboxFolder, raw: &[u8]) -> Result<MessageId, BackendError> {
        // existing filesystem write...
        let mid = self.write_to_disk(folder, raw)?;
        if let Some(index) = self.index.as_ref() {
            let parsed = Message::from_bytes(raw)?;
            let row = extractor::extract(&parsed, folder, /* direction = */ from_folder(folder));
            index.upsert(&row)?;            // best-effort; logs on failure (§8)
        }
        Ok(mid)
    }
    // move_to: index.update_folder(mid, new_folder)
    // mark_read: index.update_unread(mid, false)
}
```

The `Option<&Index>` lets tests construct a `Mailbox` without an index attached
(existing test bodies untouched) and lets `rebuild-index` open the mailbox in
read-only mode without re-triggering write hooks.

### 6.3 `rebuild-index` command

Two surfaces, same code path:

- **CLI**: `tuxlink search rebuild-index` (added to the existing Tauri sidecar
  CLI surface — see [`src-tauri/src/main.rs`](../../src-tauri/src/main.rs) for the
  current sidecar entry pattern).
- **UI**: a button in Settings → Saved Searches → "Maintenance" section
  ("Rebuild search index — runs in <N> seconds at current mailbox size").

The rebuild deletes `search.db` (and `-wal` / `-shm`), recreates the schema,
and re-walks every folder calling `Index::upsert` per message. v0.1 mailbox
sizes are small (operator-scale, not server-scale), so a single-pass synchronous
rebuild is correct; chunking is deferred to v0.5+ when corpus size justifies
the complexity.

### 6.4 Tauri command surface

```rust
#[tauri::command] fn search_run(spec: QuerySpec) -> Result<SearchResults, UiError>;
#[tauri::command] fn search_list_saved() -> Result<Vec<SavedSearch>, UiError>;
#[tauri::command] fn search_list_recent() -> Result<Vec<RecentSearch>, UiError>;
#[tauri::command] fn search_save(name: String, spec: QuerySpec) -> Result<SavedSearch, UiError>;
#[tauri::command] fn search_unsave(id: SavedSearchId) -> Result<(), UiError>;
#[tauri::command] fn search_rename(id: SavedSearchId, name: String) -> Result<(), UiError>;
#[tauri::command] fn search_reorder(ordered_ids: Vec<SavedSearchId>) -> Result<(), UiError>;
#[tauri::command] fn search_rebuild_index() -> Result<RebuildStats, UiError>;
```

`QuerySpec` and `SearchResults` shapes:

```rust
pub struct QuerySpec {
    pub free_text: Option<String>,             // FTS5 MATCH expression (or None)
    pub filters: BTreeMap<FilterKey, FilterValue>,    // chip state
    pub sort: SortOrder,                       // default: date_desc
    pub page: PageRequest,                     // page_size + offset
}

pub enum FilterKey {
    Folder, From, To, DateRange, FormType, HasForm, HasAttach, ReadState, Transport,
}

pub struct SearchResults {
    pub items: Vec<MessageMetaDto>,            // existing DTO, same shape MessageList consumes
    pub total_matches: u32,
    pub query_ms: u32,
    pub effective_spec: QuerySpec,             // server-canonicalized
}
```

`MessageMetaDto` is the **same shape** [`MessageList`](../../src/mailbox/MessageList.tsx)
already renders from [`useMailbox`](../../src/mailbox/useMailbox.ts) — search results plug into the existing
list rendering without a parallel row component.

## 7. Frontend (React) — `src/search/`

### 7.1 Components & hooks

```
src/search/
    SearchBar.tsx          ribbon-mounted input; renders saved-name + chevron; opens SearchDropdown
    SearchDropdown.tsx     saved + recent two-section list; keyboard nav
    ChipStrip.tsx          below-ribbon strip; renders active + ghost chips + meta
    SavedSearchesPanel.tsx Settings tab: list + drag-reorder + per-row inline edit
    useSearch.ts           QuerySpec state, debounced search_run, results
    useSavedSearches.ts    saved + recent list, save / unsave / promote / rename
    queryRender.ts         render QuerySpec → human-readable preview ("from:X date:7d")
    types.ts               QuerySpec, FilterKey/FilterValue, SavedSearch, RecentSearch (mirror Rust)
```

### 7.2 Shell wiring

[`AppShell.tsx`](../../src/shell/AppShell.tsx) is the only existing file that grows:

- Ribbon JSX gets a leading `<SearchBar />` (replaces the leading dash-item position).
- Below the ribbon, before `.panes`, a new `<ChipStrip />` row is added.
- The grid template (`AppShell.css:24-26` — `auto auto auto 1fr auto auto`) grows by one `auto` row for the chip strip: `auto auto auto auto 1fr auto auto`. **Verify the row-1fr invariant** documented at [`AppShell.css:18-22`](../../src/shell/AppShell.css#L18-L22) — `panes` must remain the `1fr` row after the change.

[`MessageList.tsx`](../../src/mailbox/MessageList.tsx) gains two additive props:

- `matchHighlights?: { mid: string; ranges: HighlightRange[] }[]` — optional per-row term-highlight info; rendered via `<mark>` in the `.subject` and `.preview` cells. When absent (no search active), rows render exactly as today.
- `showFolderTag?: boolean` — renders a small folder badge inline-left of the subject when the search is cross-folder.

### 7.3 State model

`useSearch` owns the canonical `QuerySpec` in shell state. The chip strip and
the search bar are both controlled inputs against this state. Changes debounce
(`150 ms`) before issuing a `search_run` Tauri call. Results replace
`useMailbox`'s folder listing **when a search is active**; clearing the search
(empty input + all chips removed + no saved search active) reverts to the
folder-scoped `useMailbox` view (no flash — the transition is a state swap, not
a remount).

### 7.4 Persistence of saved + recent

The Rust `saved.rs` module persists `saved` + `recent` to a single JSON file
under the app config dir (Tauri's `$APPCONFIG/saved-searches.json`). Schema:

```json
{
  "version": 1,
  "saved": [
    {
      "id": "uuid",
      "name": "Storm Net 5/30",
      "spec": { "free_text": "damage", "filters": {...}, "sort": "date_desc" },
      "created_at": 1717084800,
      "last_used_at": 1717091400,
      "order": 0
    }
  ],
  "recent": [
    {
      "spec": { ... },
      "ran_at": 1717092100
    }
  ]
}
```

`recent` is capped at 20 entries (oldest pruned). Promoting a recent → saved
removes it from `recent` and inserts into `saved`.

## 8. Error handling

| Failure | Surface | Behavior |
|---|---|---|
| FTS5 syntax error in free-text | UI | Chip strip meta-text becomes `"⚠ Invalid query: <reason>"` in `--accent`; no toast. Input is not cleared. |
| Index upsert fails on `Mailbox::store` | Backend log + UI dashboard | The filesystem write **still succeeds** (mailbox is canonical). Index drift is recorded via a `index_drift_count` metric exposed in Settings → Saved Searches → Maintenance. Operator can hit `Rebuild` to reconcile. The store call itself returns `Ok` — find-messages **never** breaks mailbox writes. |
| `search.db` missing / corrupt at startup | UI prompt | Settings → Saved Searches panel shows a "Rebuild needed" banner; search bar is disabled (chevron + greyed input with placeholder "Search index not ready — Rebuild in Settings"). |
| Schema version drift on open | Automatic | Open path detects `pragma user_version < SCHEMA_VERSION` and triggers `rebuild-index` automatically with a non-blocking UI toast. |
| `saved-searches.json` malformed | UI prompt | Backend logs the parse error; UI surfaces "Saved searches file is malformed — Reset?" in Settings. Search bar still works for free-typed queries. |
| Mailbox folder mutation occurs while a search is mid-flight | UI | The synchronous `Mailbox::store`-hook write completes before the next query; debouncing handles the rest. No cross-pane race because the chip strip's `effective_spec` echo matches what the backend ran. |

## 9. Testing

### 9.1 Rust (`src-tauri/src/search/`)

- **`extractor.rs`** — exhaustive table-test per `MessageMeta` field: every conservative-max column extracts correctly from a fixture `Message` (including form-payload fields for `ICS-213`, `ICS-309`, position, bulletin, damage-assessment forms).
- **`index.rs`** — schema-create idempotence; `upsert` insert vs update; `delete` semantics; `update_folder` mirrors `move_to`; `update_unread` mirrors `mark_read`; schema-version drift triggers rebuild.
- **`query.rs`** — `QuerySpec` round-trip serde; chip → SQL composition with every chip toggled independently; combinations (`FROM` + `DATE` + `FORM-TYPE`); free-text-only; chips-only.
- **`saved.rs`** — load empty → seed; promote recent → saved; rename; unsave; reorder; recent cap-at-20; corrupt-file → structured error.
- **Integration** (`tests/search_integration.rs`) — full roundtrip: `Mailbox::store(N messages) → search_run(spec) → assert results match expected mids`. Covers every chip and 3 representative free-text queries.
- **Mailbox-hook regression** — existing `native_mailbox.rs` tests still pass with the new `Option<&Index>` wiring; a new test asserts `Mailbox::store` with `Some(index)` upserts into the index.
- **Rebuild-index** — populate mailbox, delete `search.db`, run `rebuild-index`, assert search results identical to pre-deletion run.

### 9.2 React (`src/search/`)

- **`SearchBar.test.tsx`** — renders placeholder when empty; shows saved-name + ★ when a saved search is active; ⌘F focuses input; chevron toggles dropdown; click on filled star unsaves.
- **`SearchDropdown.test.tsx`** — saved section above recent; ☆ promotion flow; keyboard nav `↑/↓/⏎/Esc`; "Run as new search" appears when no saved/recent matches typed input; footer `Manage…` opens Settings panel.
- **`ChipStrip.test.tsx`** — active chip renders with `×`; clicking `×` removes the chip; ghost chip click opens chip-add UI; meta-text shows match count + ms; empty state shows ghost chips only with stable height.
- **`useSearch.test.ts`** — debounced search_run; clearing query reverts to folder view; FTS5 syntax error surfaces in chip-strip meta.
- **`useSavedSearches.test.ts`** — promote / unsave / rename / reorder; recent cap-at-20; persists through Tauri command bindings.
- **`SavedSearchesPanel.test.tsx`** — list renders; per-row inline edit; "+ New saved search" expand → save; drag-reorder; "Rebuild search index" button.

### 9.3 End-to-end smoke (operator-runnable)

- Open Settings → Saved Searches → "+ New saved search" → enter `name: Storm Net Test`, `free text: damage`, `+ FROM:KX5DD`. Save.
- Close Settings → click `★ Storm Net Test` in the search dropdown.
- Assert: rows pane shows only matching messages; chip strip meta says `★ Storm Net Test`; matched terms highlighted.
- Send a new message in the outbox matching the query; observe the row appears in the active saved-search view after the index hook fires.

## 10. Out of scope / v0.2+

The following are **explicitly deferred** so v0.1 stays bounded:

- **Attachment text indexing** (PDFs / CSVs / .docx). Multi-week parser-security surface; ConvertX-style sandboxing is the right shape but is a separate ADR / spec.
- **Live match-counts** on starred searches in the dropdown (ambient signal lost when moving from sidebar to dropdown — see §11 known-unknown).
- **Result snapshots / archival saved searches** (the static-vs-dynamic variant rejected at Q5). Replace with an explicit "Export results to .mbox / .csv / .json" action.
- **`⌥⏎ open in new pane`** — keyboard binding is documented in the dropdown footer but the underlying pane-management is v0.2 scope (multi-pane search comparison).
- **Cross-call-sign federation** (search across multiple `~/.tuxlink/` data dirs). Single-account / single-data-dir only in v0.1.
- **Fuzzy match / typo-tolerance**. FTS5 `porter` stemming covers prefix/plural variants; true fuzzy is the Tantivy-swap motivator in v0.5.
- **Regex queries**. Power-user feature; not a net-control workflow.
- **Search-result sort by anything other than date**. Relevance-ranked FTS5 score is available but not surfaced as a UI sort option in v0.1 — date-desc is what operators want during ops.

## 11. Known-unknowns recorded in the spec

| Topic | Question | Likely placement |
|---|---|---|
| Live match-counts on starred saved searches | When v0.2 adds live polling per-saved-search, where does the count surface? | Three candidates: (a) badge on the search bar, (b) notification chip in the chip strip, (c) status-bar indicator. Defer to v0.2 design pass; do not pre-commit. |
| FTS5 vs Tantivy at v0.5+ | If fuzzy-match and faceting become must-haves, swap to Tantivy? | Engine-swap should preserve `QuerySpec` semantics; `index.rs` already isolates schema behind a trait-shaped interface so the swap doesn't propagate. |
| Saved searches across data-dirs / call-signs | Are saved searches per-call-sign? Per-data-dir? | Stick with per-data-dir until call-sign-switching ships (out of v0.1 scope). |

## 12. Visual companion artifact

Three iterative mockups (Options A/B/C → D → E) were rendered during this
brainstorm and saved under
[`.superpowers/brainstorm/3079710-1780180290/content/`](../../.superpowers/brainstorm/3079710-1780180290/content/).
That directory is `.gitignore`d ([`.gitignore:22`](../../.gitignore#L22) — `.superpowers/`)
per the visual-companion convention; the renders are reference material, not
project artifacts. If a future operator needs to see them, regenerate from the
spec or rebuild the companion server via `superpowers:brainstorming`.
