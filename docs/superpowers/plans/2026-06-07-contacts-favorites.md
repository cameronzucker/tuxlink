# Contacts + Favorites Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Contacts address book (multi-address, callsign-primary, with distribution groups) that powers Compose To/Cc, and a per-radio-mode Favorites/Recents system with an honest, time-of-day-bucketed empirical connection record.

**Architecture:** Two decoupled features sharing a JSON-store idiom. Rust owns two app-data-dir JSON stores (`contacts.json`, `stations.json`) behind `Arc<Mutex<…>>` managed state + `#[tauri::command]`s; the React frontend reads them via TanStack-Query hooks (no zustand/redux) and renders inline surfaces (a sidebar "Address" pseudo-folder + a Compose autocomplete + per-mode radio-dock tabs). Group expansion to member callsigns happens **frontend, at send time only**. Favorites distance is **derived** (haversine over operator grid + gateway grid), never stored.

**Tech Stack:** Rust (serde, thiserror, chrono, tempfile), Tauri v2 commands, React + TypeScript, TanStack Query, `@radix-ui/react-tabs` (first use), react-virtuoso, vitest + @testing-library, Rust `#[cfg(test)]` + tempdir.

**Design source:** `docs/design/2026-06-07-contacts-favorites-design.md`. **Codebase map:** `dev/scratch/_cf-codebase-map.md` (read it — every mirror file + integration line below comes from it).

**bd issues:** `tuxlink-raez` (Contacts) + `tuxlink-egmp` (Favorites, depends on raez). Branch `bd-tuxlink-raez/contacts-favorites`.

---

## Locked decisions (consistency contract — same names everywhere)

**Serde convention:** snake_case (codebase has NO `rename_all`). Frontend DTO fields are snake_case (`created_at`, `contact_id`, `ts_local`). The design doc's camelCase is illustrative only.

**Open-item defaults (proposed → Codex adrev to converge; documented, not operator-gated):**
- **ToD buckets** (local hour): `dawn` 05–07, `day` 08–16, `dusk` 17–19, `night` 20–04.
- **Hint threshold:** show a ToD hint only when a bucket has **≥3 attempts** AND that bucket's `reached` fraction is the max across buckets. Never below 3 — never over-claim from thin data.
- **Recents cap:** N=**10** non-starred entries per mode (oldest dropped on overflow).
- **Group members:** store `contact_id` when added from a contact (edits propagate); `callsign` (raw literal) when typed.

**Data models (LOCKED — do not rename across tasks):**

```rust
// src-tauri/src/contacts/store.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: String, pub name: String, pub callsign: String,
    pub email: Option<String>, pub tactical: Option<String>, pub notes: Option<String>,
    pub created_at: String, pub updated_at: String,   // RFC3339 UTC
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GroupMember { Contact { contact_id: String }, Raw { callsign: String } }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub id: String, pub name: String, pub members: Vec<GroupMember>,
    pub created_at: String, pub updated_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ContactsFile { pub schema_version: u32, pub contacts: Vec<Contact>, pub groups: Vec<Group> }
```

```rust
// src-tauri/src/favorites/store.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favorite {
    pub id: String, pub mode: String, pub gateway: String,
    pub freq: Option<String>, pub port: Option<u16>, pub band: Option<String>,
    pub grid: Option<String>, pub note: Option<String>, pub starred: bool,
    pub created_at: String, pub updated_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionAttempt {
    pub unit_id: String, pub ts_local: String,    // ISO8601 + offset — NEVER converted to UTC
    pub freq: Option<String>, pub outcome: String, // "reached" | "failed"
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct StationsFile { pub schema_version: u32, pub favorites: Vec<Favorite>, pub log: Vec<ConnectionAttempt> }
```

```typescript
// frontend mirrors (snake_case) — src/contacts/types.ts, src/favorites/types.ts
export interface Contact { id: string; name: string; callsign: string; email?: string; tactical?: string; notes?: string; created_at: string; updated_at: string }
export type GroupMember = { type: 'contact'; contact_id: string } | { type: 'raw'; callsign: string }
export interface Group { id: string; name: string; members: GroupMember[]; created_at: string; updated_at: string }
export type RadioMode = 'vara-hf' | 'vara-fm' | 'ardop-hf' | 'packet' | 'telnet'
export interface Favorite { id: string; mode: RadioMode; gateway: string; freq?: string; port?: number; band?: string; grid?: string; note?: string; starred: boolean; created_at: string; updated_at: string }
export interface ConnectionAttempt { unit_id: string; ts_local: string; freq?: string; outcome: 'reached' | 'failed' }
```

**Store conventions (both stores):** `schema_version: 1`; `#[serde(deny_unknown_fields)]`; `.tmp`→`rename` atomic write (mirror `user_folders.rs:182-192`, NOT the heavier `config.rs` fsync ceremony); `open()` degrades to a default empty store + `eprintln!` on read error (NEVER blocks startup); `app_data_dir` resolved INSIDE commands via `app: AppHandle`; managed as `Arc<Mutex<…>>`.

**RADIO-1 hard constraint (Part B):** quick-connect is PRE-FILL ONLY. It sets the existing connect-form state; the operator's click on Start IS the Part 97 consent gate. No auto-TX, no bypassing the in-process busy guard, no consent modal (it was removed). A test MUST assert quick-connect does not invoke any connect command directly.

---

# PART 0 — Shared scaffolding

### Task 0: Branch hygiene + module skeleton

**Files:**
- Verify: `bash scripts/install-githooks.sh` (activate commit-msg/pre-push hooks)
- Create (empty modules wired into the tree): `src-tauri/src/contacts/mod.rs`, `src-tauri/src/favorites/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod contacts; mod favorites;` near other `mod` decls)

- [ ] **Step 1:** `bash scripts/install-githooks.sh`; confirm "hooks installed". Confirm you are in worktree `worktrees/bd-tuxlink-raez-contacts-favorites` on branch `bd-tuxlink-raez/contacts-favorites` (`git -C . rev-parse --abbrev-ref HEAD`).
- [ ] **Step 2:** Create `src-tauri/src/contacts/mod.rs` with `pub mod store; pub mod commands;` and `src-tauri/src/favorites/mod.rs` likewise. (`store.rs`/`commands.rs` created in later tasks; create empty stubs so the crate compiles, OR add the `pub mod` lines in the task that creates each file — pick one and keep it consistent.)
- [ ] **Step 3:** Add `mod contacts;` and `mod favorites;` to `src-tauri/src/lib.rs` beside the existing `mod` declarations.
- [ ] **Step 4:** `cargo build --manifest-path src-tauri/Cargo.toml` → compiles (empty modules).
- [ ] **Step 5:** Commit: `chore(contacts,favorites): scaffold modules`.

---

# PART A — CONTACTS (`tuxlink-raez`)

### Task A1: Rust contacts store + CRUD

**Files:**
- Create: `src-tauri/src/contacts/store.rs`
- Mirror: `src-tauri/src/search/saved.rs` (open/flush/CRUD + `#[cfg(test)]`), `src-tauri/src/user_folders.rs:182-192` (`.tmp`→rename), `src-tauri/src/ui_commands.rs:48-49` (error enum projection).
- Test: in-file `#[cfg(test)]`.

- [ ] **Step 1 — failing tests.** Write `#[cfg(test)]` tests using `tempfile::tempdir()`:
  - `open_missing_returns_empty` — `open()` on a nonexistent path yields `schema_version:1`, empty vecs.
  - `upsert_then_reopen_persists` — `contact_upsert` a Contact, drop store, reopen, assert it's present.
  - `upsert_existing_updates_in_place` — upsert same `id` twice; len stays 1; fields updated; `created_at` preserved, `updated_at` changes.
  - `delete_removes` — `contact_delete(id)`; reopen; gone.
  - `group_upsert_delete_roundtrip` — same for groups incl. a `GroupMember::Contact` and a `GroupMember::Raw`.
  - `deny_unknown_fields_rejects_garbage` — writing a JSON file with an extra top-level key fails to parse (store degrades to empty per open() contract — assert it does NOT panic).
  - `atomic_write_leaves_no_tmp` — after a flush, no `*.tmp` remains in the dir.
- [ ] **Step 2 — run, expect FAIL** (`ContactsStore` undefined): `cargo test --manifest-path src-tauri/Cargo.toml -p <crate> contacts::store`.
- [ ] **Step 3 — implement.** `pub struct ContactsStore { path: PathBuf, file: ContactsFile }` with:
  - `pub fn open(path: PathBuf) -> Self` — read+parse; on any error `eprintln!` + `ContactsFile { schema_version: 1, ..Default::default() }`. (open is infallible by design — degrade, never block.)
  - `fn flush(&self) -> Result<(), ContactsError>` — `serde_json::to_string_pretty`, write to `path.with_extension("tmp")`, `std::fs::rename(tmp, &self.path)`. `create_dir_all(parent)` first.
  - `pub fn contacts(&self) -> &[Contact]`, `pub fn groups(&self) -> &[Group]`.
  - `pub fn contact_upsert(&mut self, c: Contact) -> Result<(), ContactsError>` — replace by `id` or push; set timestamps (caller passes them or store stamps — stamp in the command layer, see A2); flush.
  - `pub fn contact_delete(&mut self, id: &str) -> Result<(), ContactsError>`; `group_upsert`/`group_delete` analogous.
  - `ContactsError` enum (`thiserror` + `#[serde(tag="kind",content="detail")]`): `Io(String)`, `Serde(String)`.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(contacts): JSON store + CRUD with atomic writes`.

### Task A2: Rust contacts commands + registration

**Files:**
- Create: `src-tauri/src/contacts/commands.rs`
- Modify: `src-tauri/src/lib.rs:318` (invoke_handler), `src-tauri/src/lib.rs` setup block (~198-232) (manage state)
- Mirror: `src-tauri/src/search/commands.rs:284-295` (command signature + `app.path().app_data_dir()`).
- Test: in-file `#[cfg(test)]` for the timestamp-stamping helper (commands themselves are thin; test the pure helpers).

- [ ] **Step 1 — failing test** for `stamp_new`/`stamp_update` helpers and a `new_id()` (uuid v4 string) — assert `created_at`==`updated_at` on new, `created_at` preserved on update, ids are unique + non-empty.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** commands (each `#[tauri::command]`, `svc: State<Arc<Mutex<ContactsStore>>>`, `app: AppHandle` only where the store needs (re)opening — prefer managed state so app_data_dir is resolved once at setup; if a command must resolve it, use `app.path().app_data_dir()?`):
  - `contacts_read() -> Result<ContactsFile, ContactsError>`
  - `contact_upsert(contact: Contact) -> Result<Contact, ContactsError>` — stamp timestamps + id if empty, persist, return the stored contact.
  - `contact_delete(id: String) -> Result<(), ContactsError>`
  - `group_upsert(group: Group) -> Result<Group, ContactsError>`, `group_delete(id: String)`
  - (`contacts_suggestions` is Task A3.)
  - Register all in `lib.rs:318` `generate_handler![…]` in a **commented `// contacts` section** (coordination: the Catalog agent appends adjacent — keep your block labeled). Add `.manage(Arc::new(Mutex::new(ContactsStore::open(app_data_dir.join("contacts.json")))))` in setup, guarded so a failure logs and continues.
- [ ] **Step 4 — run, expect PASS;** `cargo build` the whole crate.
- [ ] **Step 5 — commit:** `feat(contacts): tauri commands + state registration`.

### Task A3: contacts_suggestions (suggest-from-history)

**Files:**
- Modify: `src-tauri/src/contacts/commands.rs` (+ a pure `derive_suggestions` fn in `store.rs` or a `suggest.rs`)
- Reference: `src-tauri/src/winlink_backend.rs` (mailbox read access for From/To correspondents)
- Test: in-file `#[cfg(test)]` over a fixture correspondent list.

- [ ] **Step 1 — failing test** for `derive_suggestions(correspondents: &[(callsign, count)], existing: &[Contact]) -> Vec<Suggestion>`: excludes callsigns already in contacts (case-insensitive); returns `Suggestion { callsign, message_count }`; sorted by count desc; never auto-creates.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** the pure fn, then `contacts_suggestions(app) -> Result<Vec<Suggestion>, ContactsError>` that reads mailbox From/To via the backend, tallies per-callsign counts, and calls `derive_suggestions`. Register in lib.rs.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(contacts): suggest-from-history derivation`.

### Task A4: useContacts hook + types

**Files:**
- Create: `src/contacts/types.ts` (see Locked decisions), `src/contacts/useContacts.ts`
- Mirror: `src/search/useSavedSearches.ts` (TanStack-Query + invoke + invalidate).
- Test: `src/contacts/useContacts.test.ts` (renderHook + QueryClient wrapper, mirror `src/mailbox/useMailbox.test.ts:27-30`).

- [ ] **Step 1 — failing test:** mock `@tauri-apps/api/core` `invoke` at file top; `useContacts` returns `{ contacts, groups, isLoading, upsertContact, deleteContact, upsertGroup, deleteGroup }`; calling `upsertContact` invokes `contact_upsert` then invalidates `['contacts']`.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** the hook: `useQuery({ queryKey: ['contacts'], queryFn: () => invoke('contacts_read') })` → split `.contacts`/`.groups`; mutations `await invoke(...)` then `qc.invalidateQueries({ queryKey: ['contacts'] })`; errors `.catch(()=>{})` (non-blocking, no error state — Cross-cutting §1).
- [ ] **Step 4 — run, expect PASS** (`pkill -9 -f vitest` after).
- [ ] **Step 5 — commit:** `feat(contacts): useContacts hook`.

### Task A5: RecipientInput (chips + autocomplete)

**Files:**
- Create: `src/contacts/RecipientInput.tsx`, `src/contacts/recipients.ts` (pure match/format helpers)
- Mirror: `src/search/ChipStrip.tsx:31` (chips), `src/search/SearchDropdown.tsx:54-61` (↑↓/Enter/Esc). **NO native `<select>`** (renders disabled on WebKitGTK).
- Test: `src/contacts/RecipientInput.test.tsx`, `src/contacts/recipients.test.ts`.

- [ ] **Step 1 — failing tests:**
  - `recipients.test.ts`: `matchRecipients(query, contacts, groups)` matches name/callsign/email substrings (case-insensitive), groups included, returns ordered list; empty query → empty.
  - `RecipientInput.test.tsx`: typing filters the dropdown; ArrowDown+Enter adds a chip; a typed raw callsign + Enter (no match) adds a raw chip (passthrough); a group selection renders as ONE chip labeled `name · <memberCount>`; Backspace on empty input removes last chip; Esc closes dropdown.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement.** Controlled component: `value: string` (semicolon string), `onChange`. Internally parse to chips for display (`splitAddrs`-compatible), keep group chips distinguished. Dropdown uses the SearchDropdown keyboard pattern. Group chip is non-editing (membership managed in the Contacts surface).
- [ ] **Step 4 — run, expect PASS** (reap vitest).
- [ ] **Step 5 — commit:** `feat(contacts): recipient chip+autocomplete input`.

### Task A6: Compose integration + group expansion at send

**Files:**
- Modify: `src/compose/Compose.tsx` (To/Cc inputs ~735-764; the THREE send paths: `handleSend` L391/L414, `send_form`, `send_webview_form`)
- Modify: `src/compose/useDraft.ts` (add `expandGroupsAndDedup` beside `splitAddrs` L137)
- Test: `src/compose/useDraft.test.ts` (expansion+dedup), `src/compose/Compose.test.tsx` (send path uses expansion).

- [ ] **Step 1 — failing tests** for `expandGroupsAndDedup(recipients: string[], contacts: Contact[], groups: Group[]): string[]`:
  - a group token expands to its member callsigns (resolving `contact_id`→callsign, raw passthrough);
  - dedup is case-insensitive for bare callsigns (`W6ABC`==`w6abc`), keeps first occurrence;
  - a raw callsign not matching any group/contact passes through unchanged;
  - email-form recipients preserved (not normalized here — backend `normalize_address` handles wire form);
  - an empty/unknown group token resolves to nothing (no crash).
  Plus `Compose.test.tsx`: a draft with a group in To, on Send, invokes `message_send` with the EXPANDED member callsigns (assert via the invoke mock), and group expansion is NOT applied on autosave.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** `expandGroupsAndDedup` in `useDraft.ts`; wrap all three send paths: `to: expandGroupsAndDedup(splitAddrs(to), contacts, groups)` (same for cc). Replace the To/Cc `<input>`s with `RecipientInput` (keep `to`/`cc` state as semicolon strings for autosave). Compose is a separate window → it gets its own `useContacts` instance.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(compose): contacts autocomplete + group expansion at send`.

### Task A7: Sidebar "Address" group + Contacts item

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx:29-35` (add Address section + Contacts item w/ count), `src/shell/AppShell.tsx` (pass contacts count; route `selectedFolder==='contacts'`)
- Test: `src/mailbox/FolderSidebar.test.tsx` (section label + item + count + click→onSelect('contacts')).

- [ ] **Step 1 — failing test:** FolderSidebar renders an "Address" `section-label` and a "Contacts" nav-item with `data-testid="folder-count-contacts"` showing the passed count; clicking it calls `onSelectFolder('contacts')`.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement.** Add the Address section + Contacts item. **Coordination note for FZ-M1:** keep the item declared in/derived from the `MAILBOX_ITEMS`-style list so the icon-rail picks it up generically. Contacts count comes from `useContacts().contacts.length` (NOT the mailbox counts memo). `'contacts'` is a pseudo-folder string — do not extend the `MailboxFolder` enum.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(contacts): Address sidebar group + Contacts nav item`.

### Task A8: ContactsPanel (list/detail + suggestions)

**Files:**
- Create: `src/contacts/ContactsPanel.tsx`, `src/contacts/ContactEditor.tsx`
- Modify: `src/shell/AppShell.tsx` main-content switch (~869-929) — **insert `if (selectedFolder === 'contacts') return <ContactsPanel/>` EARLY**, before the readingPane/connection construction.
- Mirror: `src/mailbox/MessageList.tsx` (virtuoso list), `src/mailbox/devFixture.ts` (fixtures).
- Test: `src/contacts/ContactsPanel.test.tsx`.

- [ ] **Step 1 — failing tests:** list shows Groups section (avatars) ABOVE People; search filters both; selecting a row shows the detail pane (name, primary callsign, email/tactical, notes) with **New message** + **Edit** actions; the "Suggested" affordance lists suggest-from-history "+ Add" cards (from `contacts_suggestions`) annotated with the message count; **New message** drops the contact into Compose To (assert the invoke/route).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** the inline list+detail surface (~286px list column + detail pane). ContactEditor is the New/Edit form (callsign required; name/email/tactical/notes optional). "+ Add" card calls `contact_upsert`. New message routes the primary callsign into Compose.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(contacts): inline list/detail surface + suggestions`.

### Task A9: App-level mount test (Contacts)

**Files:**
- Modify/create: `src/App.test.tsx` or `src/shell/AppShell.test.tsx` — add a case using `routeInvoke` (mirror `App.test.tsx:74-82`) that mounts the real tree, routes `contacts_read`, selects the Contacts folder, and asserts ContactsPanel renders inside the production provider stack.

- [ ] **Step 1 — failing test** (selecting Contacts in the full AppShell mount renders the panel).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — make it pass** (wire any missing provider/route).
- [ ] **Step 4 — run, expect PASS;** `pkill -9 -f vitest`.
- [ ] **Step 5 — commit:** `test(contacts): App-level mount path`.

---

# PART B — FAVORITES (`tuxlink-egmp`)

### Task B1: Rust favorites store + ToD bucketing + recents trim

**Files:**
- Create: `src-tauri/src/favorites/store.rs`
- Mirror: `src-tauri/src/search/saved.rs`; same atomic-write + degrade conventions as A1.
- Test: in-file `#[cfg(test)]`.

- [ ] **Step 1 — failing tests:**
  - store CRUD + reopen-persist (favorites + log) like A1.
  - `favorite_star(id, true)` flips `starred`; star-to-promote keeps it past the recents cap.
  - `record_attempt_appends` — appends a `ConnectionAttempt` with the EXACT `ts_local` passed (no UTC conversion); for a non-starred recent, upserts the `(gateway,freq)` and trims non-starred for that mode to **10** (oldest dropped); starred favorites are NEVER trimmed.
  - `tod_bucket`: 06→`dawn`, 12→`day`, 18→`dusk`, 23→`night`, 02→`night`.
  - `tod_hint`: given attempts, returns `None` when the top bucket has <3 attempts; returns `Some` naming the bucket with the highest `reached` fraction once it has ≥3; ties resolved deterministically (earliest bucket).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** `FavoritesStore` (mirror A1) + pure fns `tod_bucket(hour: u8) -> &'static str` (dawn 5-7/day 8-16/dusk 17-19/night 20-4), `tod_hint(attempts: &[ConnectionAttempt]) -> Option<TodHint>` (parse local hour from `ts_local`; bucket; require ≥3 + max reached-fraction), and the recents-trim logic in the record path. `ts_local` is stored verbatim.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(favorites): JSON store, ToD buckets, recents cap`.

### Task B2: Rust favorites commands + registration

**Files:**
- Create: `src-tauri/src/favorites/commands.rs`; Modify `src-tauri/src/lib.rs:318` + setup.
- Test: in-file helper tests as needed.

- [ ] **Step 1 — failing test** for any command-layer helper (id stamping; mode validation that rejects an unknown mode string).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement:** `favorites_read() -> StationsFile`, `favorite_upsert(favorite) -> Favorite`, `favorite_delete(id)`, `favorite_star(id, starred)`, `favorite_record_attempt(attempt: ConnectionAttempt, gateway: String, mode: String) -> ()` (appends + recents upsert/trim), `favorites_recents(mode) -> Vec<Favorite>`. Register in lib.rs `// favorites` section; manage `stations.json` store.
- [ ] **Step 4 — run, expect PASS;** full `cargo build`.
- [ ] **Step 5 — commit:** `feat(favorites): tauri commands + state registration`.

### Task B3: useFavorites hook + types

**Files:**
- Create: `src/favorites/types.ts`, `src/favorites/useFavorites.ts`; Mirror `useSavedSearches.ts`.
- Test: `src/favorites/useFavorites.test.ts`.

- [ ] **Step 1 — failing test:** `useFavorites(mode)` returns mode-filtered `{ favorites, recents, isLoading, upsert, remove, star, recordAttempt }`; mutations invalidate `['favorites']`.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** (queryKey `['favorites']`; filter by `mode` in the selector; recents = `starred:false`).
- [ ] **Step 4 — run, expect PASS;** reap vitest.
- [ ] **Step 5 — commit:** `feat(favorites): useFavorites hook`.

### Task B4: haversine distance util

**Files:**
- Create: `src/forms/position/distance.ts` (NEW file — minimizes `maidenhead.ts` merge conflict; export `haversineKm`)
- Mirror: `src/forms/position/maidenhead.ts:24` `gridToLatLon`.
- Test: `src/forms/position/distance.test.ts`.

- [ ] **Step 1 — failing tests:** `haversineKm(a: LatLon, b: LatLon): number` — known pair within tolerance; identical points → 0. `distanceBetweenGrids(gridA, gridB): number | null` — null if either grid is null/malformed (uses `gridToLatLon`).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** haversine (R=6371 km) + `distanceBetweenGrids`. **Coordination: this is the shared helper the Catalog agent consumes — export `haversineKm` + `distanceBetweenGrids`.**
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(position): shared haversine distance util`.

### Task B5: FavoritesTabs + rows + honest record rendering

**Files:**
- Create: `src/favorites/FavoritesTabs.tsx`, `src/favorites/FavoriteRow.tsx`, `src/favorites/ConnectionRecord.tsx`, `src/favorites/favorites-fixture.ts`
- Mirror: `@radix-ui/react-tabs` (consult Radix docs — FIRST use), `ChipStrip`/list idioms. Operator grid from an existing position/config command exposing `active_grid()` (FULL precision — NOT `broadcast_grid`).
- Test: `src/favorites/FavoritesTabs.test.tsx`, `src/favorites/ConnectionRecord.test.tsx`.

- [ ] **Step 1 — failing tests:**
  - Tabs render Favorites / Recent / Manual; switching tabs shows the right list; Manual shows the existing hand-entry fields (passthrough — rendered by the host panel).
  - A FavoriteRow shows star toggle, `gateway · band`, `freq · grid · distance` (distance from `distanceBetweenGrids`; absent when operator grid is None — assert no crash), the honest record line, and a Connect button.
  - ConnectionRecord: renders the last few ✓/✗; "reached 2 h ago · 21:42 local" when a success exists; honest "no successful connect yet · 1 attempt failed 3 d ago" when none; shows a ToD hint ONLY when supported (≥3 in a bucket) and never as a prediction.
  - star-to-promote: clicking the star on a Recent calls `favorite_star(id,true)`.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** the components. Telnet rows show `host:port` (no freq/band). Distance derived, never stored.
- [ ] **Step 4 — run, expect PASS;** reap vitest.
- [ ] **Step 5 — commit:** `feat(favorites): per-mode tabs + honest connection record`.

### Task B6: Per-mode panel integration (ARDOP / Packet / Telnet) — RADIO-1

**Files:**
- Modify: `src/radio/RadioPanel.tsx` (~59, mount FavoritesTabs as first child of `radio-panel-body`), `src/radio/modes/ArdopRadioPanel.tsx` (`doConnect` L501-513: record attempt; target input L606-617: quick-connect pre-fill), `PacketRadioPanel.tsx:175`, `TelnetRadioPanel.tsx:117`
- Test: `src/radio/modes/ArdopRadioPanel.test.tsx` (record + **consent-non-bypass**).

- [ ] **Step 1 — failing tests:**
  - on a successful `modem_ardop_connect`, `favorite_record_attempt` is invoked with `outcome:'reached'`; on rejection, with `outcome:'failed'`.
  - **CONSENT NON-BYPASS (critical):** clicking a Favorite's Connect button sets the target input state but does NOT invoke `modem_ardop_connect` (assert the connect mock was NOT called); only a subsequent Start click invokes it. (Mirror for Packet/Telnet.)
  - Telnet quick-connect pre-fills host+port (no freq).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement.** Mount FavoritesTabs in `radio-panel-body` (leave body interior ownership noted for FZ-M1 which wraps the container). Quick-connect = setState of the existing connect form only. Wrap `doConnect` to call `recordAttempt` after the invoke settles (success + failure) + recents upsert. **VARA has no RF connect form yet** — VARA favorites pre-fill is a no-op stub pointing at the future Phase-3 dial (do not wire to Start/Stop transport buttons).
- [ ] **Step 4 — run, expect PASS;** reap vitest.
- [ ] **Step 5 — commit:** `feat(favorites): radio-dock integration + record-on-connect (RADIO-1 pre-fill only)`.

### Task B7: App-level mount test (Favorites)

**Files:**
- Modify: `src/radio/modes/ArdopRadioPanel.test.tsx` or an AppShell-level test — mount the panel through the production provider stack; route `favorites_read`; assert tabs render + a favorite's Connect pre-fills without transmitting.

- [ ] **Step 1 — failing test.** **Step 2 — FAIL. Step 3 — pass. Step 4 — PASS** (reap vitest). **Step 5 — commit:** `test(favorites): App-level mount path`.

---

## Cross-cutting wrap-up

### Task C1: design-gap note + open-items doc
- [ ] Record the **packet relay-chain favorites gap** (Favorite schema has no relay field) as a bd follow-up issue + a `bd remember` on `tuxlink-egmp`; do NOT add the field in v1 (additive later).
- [ ] Confirm the four open-item defaults are documented in this plan (done above) and call them out in the PR body as "proposed → adrev-converged".

### Task C2: Codex cross-provider adversarial review (build-robust-features requirement)
- [ ] Run a directed Codex `review -` round per CLAUDE.md "Adversarial-review pattern" (stdin custom-prompt form). Attack angles: (1) RADIO-1 — can quick-connect ever reach a connect command without a Start click? (2) group expansion — any path where an unexpanded group token reaches the B2F builder, or a dedup miss double-sends? (3) store durability — partial write / concurrent mutation / startup-block on I/O error. (4) ToD hint — can it over-claim below threshold or mis-bucket across the offset? (5) distance — `broadcast_grid` leak or None-grid crash. Tee to `dev/adversarial/2026-06-07-contacts-favorites-codex.md`; verify `wc -l` >> 5 (not a stub). Triage findings; fix or document disposition.

### Task C3: full quality gate
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` (full) green; `cargo build` green.
- [ ] `pnpm vitest run src/contacts src/favorites src/compose src/forms/position` (narrow scope) green; `pkill -9 -f vitest` after.
- [ ] `pnpm tsc --noEmit` clean. Lint clean.
- [ ] Self-smoke via converged-style `tauri dev` from THIS worktree (after freeing :1420): grim-screenshot the Contacts surface (pre-seed `contacts.json`) + a radio panel's Favorites tab (pre-seed `stations.json`). Evidence to `dev/scratch/smoke/`.

---

## Self-Review (run before handing to build-robust-features)

**Spec coverage:** A1-A3 (store/commands/suggestions) = design A.1/A.3/A.5; A4-A6 = A.4 autocomplete + A.2 group expansion-at-send; A7-A8 = A.4 sidebar + list/detail; B1-B2 = B.1/B.3/B.5 store/ToD/commands; B3 = hook; B4 = B.1 derived distance; B5 = B.2/B.3/B.4 tabs+record; B6 = B.4 quick-connect + RADIO-1; tests per design §Testing. **Gap check:** RMS-list ingest (B.6) = forward hook, out of scope ✓. Cross-mode "all stations" = out of scope ✓.

**Placeholder scan:** boilerplate CRUD is complete-by-reference to named mirror files with locked field shapes (acceptable per "follow established patterns"); novel logic (expansion/dedup, ToD bucket+hint, haversine, recents trim, consent-non-bypass test) is spelled out. No "TBD"/"handle edge cases".

**Type consistency:** `ContactsFile`/`StationsFile`, `GroupMember{Contact,Raw}`, `ConnectionAttempt.ts_local`, `RadioMode`, `expandGroupsAndDedup`, `haversineKm`/`distanceBetweenGrids`, `tod_bucket`/`tod_hint` — names used identically across tasks and frontend/Rust DTOs (snake_case).
