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

**Open-item defaults (adrev-converged; documented, not operator-gated):**
- **ToD buckets** (local hour): `dawn` 05–07, `day` 08–16, `dusk` 17–19, `night` 20–04.
- **Hint threshold (tightened — H2/Codex#9):** show a ToD hint ONLY when the argmax-`reached`-fraction bucket has **≥3 attempts** AND **≥1 (prefer ≥2) actual successes** AND is a **unique max** (strictly greater than the runner-up). Return None otherwise. NEVER name a zero-success bucket; NEVER show below 3 attempts; NEVER frame as a prediction — observed counts only.
- **Recents cap:** N=**10** non-starred entries per mode. Eviction = **least-recently-DIALED** (smallest `last_attempt_at`), NOT least-recently-created (M3). `record_attempt` bumps the recent's `last_attempt_at`; trim drops the smallest.
- **Group members:** store `contact_id` when added from a contact (edits propagate); `callsign` (raw literal) when typed.

**Forward-compat + data-loss policy (C1/M1 — applies to BOTH stores):**
- **DROP `#[serde(deny_unknown_fields)]`.** Use `#[serde(default)]` additive tolerance, mirroring `saved.rs`/`user_folders.rs`. Unknown future fields are silently ignored, never fatal.
- **On ANY full parse failure** in `open()`: rename the unreadable file to `<name>.corrupt-<utc-ts>` (preserving the original bytes) BEFORE returning the empty store, then proceed. Malformed *individual* entries are skipped, not fatal (per design §151).
- **No `#[derive(Default)]`** on `ContactsFile`/`StationsFile` — it would write `schema_version: 0`. Hand-write `Default` setting `schema_version: SCHEMA_VERSION` (const = 1), mirroring `saved.rs:48-56`.

**Operator-grid → distance policy (C4 — RESOLVED, no operator question):** Operator grid for distance comes from `invoke("position_current_fix").grid` (full-precision `active_grid`, `Option<String>`). **FORBID** `position_status`/`useStatus` `broadcast_grid`/`ui_grid` (both precision-reduced) and stale `config_read.grid`. Using the operator's OWN full-precision grid for INTERNAL distance display to that same operator IS consistent with the GPS precision-reduction policy — that policy governs **OVER-AIR BROADCAST**, not internal app use; distance is computed and shown locally, never transmitted. An implementer must not second-guess this and reach for the precision-reduced status hook.

**Data models (LOCKED — do not rename across tasks):**

```rust
// src-tauri/src/contacts/store.rs
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: String, pub name: String, pub callsign: String,  // callsign is SSID-bearing identity — NEVER strip SSID
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
// NO derive(Default) (M1) — hand-write Default so schema_version is 1, not 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactsFile {
    #[serde(default)] pub schema_version: u32,   // additive tolerance — NO deny_unknown_fields (C1)
    #[serde(default)] pub contacts: Vec<Contact>,
    #[serde(default)] pub groups: Vec<Group>,
}
impl Default for ContactsFile {
    fn default() -> Self { Self { schema_version: SCHEMA_VERSION, contacts: vec![], groups: vec![] } }
}
```

```rust
// src-tauri/src/favorites/store.rs
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favorite {
    pub id: String, pub mode: String, pub gateway: String,
    pub freq: Option<String>,            // RF dial freq — RECORD-ONLY metadata, never read back from a form (H8)
    pub transport: Option<String>,       // telnet only: "CmsSsl" | "Telnet" (H7 — replaces the old free `port`)
    pub band: Option<String>, pub grid: Option<String>, pub note: Option<String>, pub starred: bool,
    pub last_attempt_at: Option<String>, // bumped on every record_attempt — LRU-dialed eviction key (M3)
    pub created_at: String, pub updated_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionAttempt {
    pub unit_id: String,                 // stamped SERVER-SIDE (H3) — client never supplies it
    pub ts_local: String,                // ISO8601 + offset — NEVER converted to UTC (H1)
    pub freq: Option<String>, pub outcome: String, // "reached" | "failed"
}
// FavoriteDial — the record-path DTO (H3/Codex#8). Carries everything needed to upsert/find the unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoriteDial {
    pub mode: String, pub gateway: String,
    pub freq: Option<String>, pub transport: Option<String>,
    pub band: Option<String>, pub grid: Option<String>,
}
// NO derive(Default) (M1) — hand-write Default so schema_version is 1, not 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationsFile {
    #[serde(default)] pub schema_version: u32,   // additive tolerance — NO deny_unknown_fields (C1)
    #[serde(default)] pub favorites: Vec<Favorite>,
    #[serde(default)] pub log: Vec<ConnectionAttempt>,
}
impl Default for StationsFile {
    fn default() -> Self { Self { schema_version: SCHEMA_VERSION, favorites: vec![], log: vec![] } }
}
```

```typescript
// frontend mirrors (snake_case) — src/contacts/types.ts, src/favorites/types.ts
export interface Contact { id: string; name: string; callsign: string; email?: string; tactical?: string; notes?: string; created_at: string; updated_at: string }
export type GroupMember = { type: 'contact'; contact_id: string } | { type: 'raw'; callsign: string }
export interface Group { id: string; name: string; members: GroupMember[]; created_at: string; updated_at: string }
export type RadioMode = 'vara-hf' | 'vara-fm' | 'ardop-hf' | 'packet' | 'telnet'
export interface Favorite { id: string; mode: RadioMode; gateway: string; freq?: string; transport?: 'CmsSsl' | 'Telnet'; band?: string; grid?: string; note?: string; starred: boolean; last_attempt_at?: string; created_at: string; updated_at: string }
export interface ConnectionAttempt { unit_id: string; ts_local: string; freq?: string; outcome: 'reached' | 'failed' }
// FavoriteDial — what the frontend passes to favorite_record_attempt (server stamps unit_id) (H3/Codex#8)
export interface FavoriteDial { mode: RadioMode; gateway: string; freq?: string; transport?: 'CmsSsl' | 'Telnet'; band?: string; grid?: string }
```

**Store conventions (both stores):** `schema_version: 1`; **`#[serde(default)]` additive tolerance — NO `deny_unknown_fields`** (C1); `.tmp`→`rename` atomic write (mirror `user_folders.rs:182-192`, NOT the heavier `config.rs` fsync ceremony) — tmp name is `format!("{}.tmp", name)` so the suffix is `contacts.json.tmp`, NOT `with_extension("tmp")` (L1), `create_dir_all(parent)` first; `open() -> Self` is **infallible** — on a full parse failure it renames the unreadable file to `<name>.corrupt-<utc-ts>` (preserving bytes) THEN returns `Default` + `eprintln!` (NEVER blocks startup, NEVER overwrites the original silently); degrade mirror is **`user_folders.rs:load_registry`** (NOT `saved.rs::open`, which is fallible/propagates); `app_data_dir` is fallible — resolve it ONCE in the lib.rs setup match arm and reuse the resolved dir; managed as `Arc<Mutex<…>>`.

**RADIO-1 hard constraint (Part B):** quick-connect is PRE-FILL ONLY. It sets the existing connect-form state via an `onPrefill` callback (`setTarget` for ARDOP/Packet; `setHost`+transport for Telnet) — `FavoritesTabs` is NEVER passed an invoke/connect callback (Codex#3). The operator's click on Start IS the Part 97 consent gate. No auto-TX, no bypassing the in-process busy guard, no consent modal (it was removed). The consent-non-bypass test (M13) MUST assert a favorite Connect invokes **NEITHER** the connect command NOR the on-air TX command — for ARDOP, neither `modem_ardop_connect` NOR `modem_ardop_b2f_exchange`; mirror the negative assertion for `packet_connect` and `cms_connect`. Each mode (ARDOP, Packet, Telnet) gets its OWN first-class consent-non-bypass test file (Codex#2) — not a buried "mirror for Packet/Telnet" note.

---

# PART 0 — Shared scaffolding

### Task 0: Branch hygiene + module skeleton

**Files:**
- Verify: `bash scripts/install-githooks.sh` (activate commit-msg/pre-push hooks)
- Create (empty modules wired into the tree): `src-tauri/src/contacts/mod.rs`, `src-tauri/src/favorites/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod contacts; mod favorites;` near other `mod` decls)

- [ ] **Step 1:** `bash scripts/install-githooks.sh`; confirm "hooks installed". Confirm you are in worktree `worktrees/bd-tuxlink-raez-contacts-favorites` on branch `bd-tuxlink-raez/contacts-favorites` (`git -C . rev-parse --abbrev-ref HEAD`).
- [ ] **Step 2:** Create `src-tauri/src/contacts/mod.rs` with `pub mod store; pub mod commands;` and `src-tauri/src/favorites/mod.rs` likewise. **To avoid the unresolved-module build break (Codex#13): create empty stub files `store.rs` + `commands.rs` in each module dir NOW** (so `cargo build` in Step 4 succeeds with the `pub mod` lines present). Later tasks fill the stubs in place; do NOT defer the `pub mod` declarations.
- [ ] **Step 3:** Add `mod contacts;` and `mod favorites;` to `src-tauri/src/lib.rs` beside the existing `mod` declarations.
- [ ] **Step 4:** `cargo build --manifest-path src-tauri/Cargo.toml` → compiles (empty modules).
- [ ] **Step 5:** Commit: `chore(contacts,favorites): scaffold modules`.

---

# PART A — CONTACTS (`tuxlink-raez`)

### Task A1: Rust contacts store + CRUD

**Files:**
- Create: `src-tauri/src/contacts/store.rs`
- Mirror: **`src-tauri/src/user_folders.rs:load_registry`** (the canonical degrade-to-default-on-read-error pattern — NOT `saved.rs::open`, which is fallible/propagates), `src-tauri/src/search/saved.rs` (CRUD shape + `#[cfg(test)]` idiom + hand-written `Default` at saved.rs:48-56), `src-tauri/src/user_folders.rs:182-192` (`.tmp`→rename), `src-tauri/src/ui_commands.rs:48-49` (error enum projection).
- Test: in-file `#[cfg(test)]`.

- [ ] **Step 1 — failing tests.** Write `#[cfg(test)]` tests using `tempfile::tempdir()`:
  - `open_missing_returns_empty` — `open()` on a nonexistent path yields `schema_version:1`, empty vecs.
  - `fresh_empty_store_has_schema_version_1` (M1) — a brand-new store written via the same path `.manage()` uses persists `schema_version:1`, NOT 0 (guards against `derive(Default)`).
  - `upsert_then_reopen_persists` — `contact_upsert` a Contact, drop store, reopen, assert it's present.
  - `upsert_existing_updates_in_place` — upsert same `id` twice; len stays 1; fields updated; `created_at` preserved, `updated_at` changes.
  - `delete_removes` — `contact_delete(id)`; reopen; gone.
  - `group_upsert_delete_roundtrip` — same for groups incl. a `GroupMember::Contact` and a `GroupMember::Raw`.
  - `unknown_top_level_field_tolerated` (C1) — writing a JSON file with an EXTRA top-level key parses fine (known fields preserved, unknown ignored), proving `#[serde(default)]` tolerance and that `deny_unknown_fields` is absent.
  - `open_on_corrupt_file_preserves_original_bytes` (C1) — write garbage (un-parseable) to `contacts.json`; `open()` returns an empty store AND leaves a `contacts.json.corrupt-<ts>` sidecar holding the ORIGINAL bytes; a subsequent mutate+flush must NOT have destroyed those bytes.
  - `atomic_write_leaves_no_tmp` — after a flush, no `*.tmp` remains in the dir.
- [ ] **Step 2 — run, expect FAIL** (`ContactsStore` undefined): `cargo test --manifest-path src-tauri/Cargo.toml contacts::store`.
- [ ] **Step 3 — implement.** `pub struct ContactsStore { path: PathBuf, file: ContactsFile }` with:
  - `pub fn open(path: PathBuf) -> Self` (INFALLIBLE — degrade, never block): if the file is absent → `Default`. If present, read bytes and `serde_json::from_slice`. On a FULL parse failure: rename the file to `<name>.corrupt-<utc-ts>` (e.g. `format!("{}.corrupt-{}", name, Utc::now().format("%Y%m%dT%H%M%SZ"))`) to preserve the bytes, `eprintln!` the error, then return `Default`. (Mirror `user_folders.rs:load_registry`.) Never overwrite the unreadable original in place.
  - `fn flush(&self) -> Result<(), ContactsError>` — `serde_json::to_string_pretty`; `create_dir_all(parent)` first; write to a sibling tmp named `format!("{}.tmp", final_file_name)` (so `contacts.json.tmp` — NOT `with_extension("tmp")`); `std::fs::rename(tmp, &self.path)`.
  - `pub fn contacts(&self) -> &[Contact]`, `pub fn groups(&self) -> &[Group]`.
  - `pub fn contact_upsert(&mut self, c: Contact) -> Result<(), ContactsError>` — replace by `id` or push; timestamps stamped in the command layer (see A2); flush.
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
- [ ] **Step 3 — implement** commands (each `#[tauri::command]`, `svc: State<Arc<Mutex<ContactsStore>>>`). The store is always managed (open is infallible), so commands take `State`, NOT `AppHandle` — `app_data_dir` is resolved ONCE at setup, never per-command (C2):
  - `contacts_read() -> Result<ContactsFile, ContactsError>`
  - `contact_upsert(contact: Contact) -> Result<Contact, ContactsError>` — stamp timestamps + id if empty, persist, return the stored contact.
  - `contact_delete(id: String) -> Result<(), ContactsError>`
  - `group_upsert(group: Group) -> Result<Group, ContactsError>`, `group_delete(id: String)`
  - (`contacts_suggestions` is Task A3.)
  - **Cross-window invalidation (H9):** after each mutating command's flush succeeds, emit a Tauri app-level event (`app.emit("contacts:changed", ())` — mirror `usePacketConfig`'s CustomEvent/Tauri-event pattern) so every `useContacts` instance (including a separate Compose window) can invalidate. The hook subscribes in A4.
  - Register all in `lib.rs:318` `generate_handler![…]` in a **commented `// contacts` section** (coordination: the Catalog agent appends adjacent — keep your block labeled). In the **existing setup match arm where `app.path().app_data_dir()` is already resolved (lib.rs:198)** (C2 — `app_data_dir()` is itself fallible; resolve it once there and reuse the resolved dir), add `.manage(Arc::new(Mutex::new(ContactsStore::open(app_data_dir.join("contacts.json")))))`. `open()` is infallible (always returns a usable store) — there is NO "guarded so a failure logs and continues" branch; the store is unconditionally managed.
- [ ] **Step 4 — run, expect PASS;** `cargo build` the whole crate.
- [ ] **Step 5 — commit:** `feat(contacts): tauri commands + state registration`.

### Task A3: contacts_suggestions (suggest-from-history)

**Files:**
- Modify: `src-tauri/src/contacts/commands.rs` (+ a pure `derive_suggestions` fn in `store.rs` or a `suggest.rs`)
- Reference: `src-tauri/src/winlink_backend.rs` (mailbox read access for From/To correspondents)
- Test: in-file `#[cfg(test)]` over a fixture correspondent list.

- [ ] **Step 1 — failing test** for `derive_suggestions(correspondents: &[(callsign, count)], existing: &[Contact], operator_callsign: &str) -> Vec<Suggestion>`:
  - excludes callsigns already in contacts, matched on a NORMALIZED wire key (strip a trailing `@winlink.org` → bare; compare base callsign case-insensitively) — so contact `W6ABC` suppresses a suggestion for `w6abc@winlink.org` (H11);
  - excludes the operator's OWN callsign and its normalized variants (Sent/Outbox `From` is the operator) (H11);
  - returns `Suggestion { callsign, message_count }`; sorted by count desc; never auto-creates.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** the pure fn, then `contacts_suggestions(app) -> Result<Vec<Suggestion>, ContactsError>` that reads mailbox From/To via the backend, tallies per-callsign counts, reads the operator callsign from `config.identity.callsign`, and calls `derive_suggestions(..., operator_callsign)`. Register in lib.rs. **Tests:** operator's own callsign never suggested; contact `W6ABC` suppresses `w6abc@winlink.org`.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(contacts): suggest-from-history derivation`.

### Task A4: useContacts hook + types

**Files:**
- Create: `src/contacts/types.ts` (see Locked decisions), `src/contacts/useContacts.ts`
- Mirror: `src/search/useSavedSearches.ts` (TanStack-Query + invoke + invalidate).
- Test: `src/contacts/useContacts.test.ts` (renderHook + QueryClient wrapper, mirror `src/mailbox/useMailbox.test.ts:27-30`).

- [ ] **Step 1 — failing test:** mock `@tauri-apps/api/core` `invoke` AND `@tauri-apps/api/event` `listen` at file top; `useContacts` returns `{ contacts, groups, isLoading, upsertContact, deleteContact, upsertGroup, deleteGroup }`; calling `upsertContact` invokes `contact_upsert` then invalidates `['contacts']`; **a fired `contacts:changed` Tauri event triggers an invalidation of `['contacts']`** (H9 — proves cross-window propagation, e.g. a contact edited in the main window reaching an open Compose).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** the hook: `useQuery({ queryKey: ['contacts'], queryFn: () => invoke('contacts_read') })` → split `.contacts`/`.groups`; mutations `await invoke(...)` then `qc.invalidateQueries({ queryKey: ['contacts'] })`; errors `.catch(()=>{})` (non-blocking, no error state — Cross-cutting §1). **In a `useEffect`, `listen('contacts:changed', () => qc.invalidateQueries({ queryKey: ['contacts'] }))` and unlisten on cleanup** (mirror `usePacketConfig`'s cross-window listener) — this is how a separate Compose window sees a main-window edit.
- [ ] **Step 4 — run, expect PASS** (`pkill -9 -f vitest` after).
- [ ] **Step 5 — commit:** `feat(contacts): useContacts hook`.

### Task A5: RecipientInput (chips + autocomplete)

**Files:**
- Create: `src/contacts/RecipientInput.tsx`, `src/contacts/recipients.ts` (pure match/format helpers)
- Mirror: `src/search/ChipStrip.tsx:31` (chip visuals only). **DIVERGE from `src/search/SearchDropdown.tsx:54-61`** — see H10 below. **NO native `<select>`** for THIS new combobox surface (renders disabled on WebKitGTK); the no-select rule binds only NEW autocomplete/combobox surfaces, NOT existing `<select>`s elsewhere (M11).
- Test: `src/contacts/RecipientInput.test.tsx`, `src/contacts/recipients.test.ts`.

- [ ] **Step 1 — failing tests:**
  - `recipients.test.ts`: `matchRecipients(query, contacts, groups)` matches name/callsign/email substrings (case-insensitive), groups included, returns ordered list; empty query → empty. **A matching contact emits a row PER usable address (Codex#12): primary callsign + email + tactical when present** — each selectable as a distinct alternate; assert a contact with an email yields both a callsign row and an email row.
  - `resolveGroupMemberCount(group, contacts)` (M6): returns the RESOLVED member count (raw callsigns + still-resolving `contact_id`s), computed with the SAME logic as send-time expansion, so a deleted-contact member is NOT counted; chip count == expansion length.
  - `RecipientInput.test.tsx`: typing filters the dropdown; ArrowDown+Enter adds a chip; **selecting the email-alternate row adds the email-form chip, selecting the callsign row adds the callsign chip (Codex#12)**; a typed raw callsign + Enter with NO focused row commits the trimmed text as a raw chip (H10 passthrough); **Enter with no focused row and empty match still commits raw text** (H10); a group selection renders as ONE chip labeled `name · <resolvedMemberCount>` and emits the sentinel token `group:<uuid>` into the value string (H5); Backspace on empty input removes last chip; Esc closes dropdown; ↑↓ are CLAMPED (no wrap).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement.** Controlled component: `value: string` (semicolon string), `onChange`. Internally parse to chips for display (`splitAddrs`-compatible), keep group chips distinguished and rendered from contacts/groups state by their `group:<uuid>` token. **H10 — RecipientInput DIVERGES from SearchDropdown:** use an INPUT-SCOPED `onKeyDown` handler (NOT a global `window` keydown listener — two instances, To + Cc, would fight); init with no focused row; **Enter with no focused row (or no match) commits the trimmed input as a raw chip** (SearchDropdown's `focusIdx<0` early-return would swallow it); Enter with a focused row adds that recipient/alternate; ↑↓ clamped, no wrap; Esc closes. **H5 — group sentinel:** a group chip serializes as `group:<uuid>` (uuid, no spaces/`;`) in the value string so it survives an autosave round-trip and is unambiguously distinguishable from a typed recipient. Group chip is non-editing (membership managed in the Contacts surface via `GroupEditor`, Task A8b). **Codex#12 — alternates:** the dropdown surfaces a contact's primary callsign, email, and tactical as separate selectable rows.
- [ ] **Step 4 — run, expect PASS** (reap vitest).
- [ ] **Step 5 — commit:** `feat(contacts): recipient chip+autocomplete input`.

### Task A6: Compose integration + group expansion at send

**Files:**
- Modify: `src/compose/Compose.tsx` (To/Cc inputs ~735-764; the THREE send paths: `handleSend` L391/L414, `send_form`, `send_webview_form`)
- Modify: `src/compose/useDraft.ts` (add `expandGroupsAndDedup` beside `splitAddrs` L137)
- Test: `src/compose/useDraft.test.ts` (expansion+dedup), `src/compose/Compose.test.tsx` (send path uses expansion).

- [ ] **Step 1 — failing tests** for `expandGroupsAndDedup(recipients: string[], contacts: Contact[], groups: Group[]): string[]`:
  - a `group:<uuid>` sentinel token (H5) expands to its member callsigns (resolving `contact_id`→callsign, raw passthrough); everything NOT prefixed `group:` is treated as a literal recipient;
  - **wire-key dedup (H6/Codex#6):** dedup on a normalized key — trim, strip a trailing `@winlink.org`, uppercase, normalize `-0` SSID to bare — keeps first occurrence. `['W6ABC','w6abc@winlink.org']` → ONE entry; `['W6ABC','w6abc@gmail.com']` → BOTH (non-winlink SMTP preserved as distinct);
  - **SSID is identity (M5):** `['W6ABC-7','W6ABC']` → TWO entries (SSID NEVER stripped except the `-0` no-op); only the `@winlink.org` email form normalizes to the bare-with-SSID callsign;
  - a raw callsign not matching any group/contact passes through unchanged;
  - non-`@winlink.org` email recipients preserved verbatim (arbitrary SMTP);
  - **deleted-contact group member (M6):** a `GroupMember::Contact{contact_id}` whose contact was deleted resolves to nothing and drops silently from expansion (no crash); surviving members expand normally; the chip's `resolvedMemberCount` already excludes it so chip == expansion length;
  - **H5 — unresolvable group token must NOT silently vanish:** an unparseable/unknown `group:<uuid>` token is surfaced (kept as visible raw text OR blocks send) rather than dropping recipients silently — assert it does not just disappear.
  - **Cc seeded from expanded To (Codex#6):** Cc dedup is seeded with the already-expanded-and-deduped To set so a recipient in both To and Cc is not double-sent.
  Plus `Compose.test.tsx`: a draft with a `group:<uuid>` in To, on Send, invokes `message_send` with the EXPANDED member callsigns and NO `group:` token reaches Rust (assert via the invoke mock); group expansion is NOT applied on autosave; an edited group membership (after the Compose window's `useContacts` receives a `contacts:changed` event, H9) expands to the updated members.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** `expandGroupsAndDedup` in `useDraft.ts`; **fetch fresh contacts/groups state immediately before expansion at send (Codex#5)** — combined with the A4 `contacts:changed` listener this prevents a separate Compose window from expanding a stale group; wrap all three send paths (`handleSend`/`send_form`/`send_webview_form`): `to: expandGroupsAndDedup(splitAddrs(to), contacts, groups)`, then `cc: expandGroupsAndDedup(splitAddrs(cc), contacts, groups)` seeded against the expanded To. Replace the To/Cc `<input>`s with `RecipientInput` (keep `to`/`cc` state as semicolon strings for autosave). Compose is a separate window → it gets its own `useContacts` instance, which subscribes to `contacts:changed` (A4) so main-window edits propagate.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(compose): contacts autocomplete + group expansion at send`.

### Task A7: Sidebar "Address" group + Contacts item

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx:29-35` (add Address section + Contacts item w/ count), `src/shell/AppShell.tsx` (pass contacts count; route `selectedFolder==='contacts'`)
- Test: `src/mailbox/FolderSidebar.test.tsx` (section label + item + count + click→onSelect('contacts')).

- [ ] **Step 1 — failing test:** FolderSidebar renders an "Address" `section-label` and a "Contacts" nav-item with `data-testid="folder-count-contacts"` showing the passed count; clicking it calls `onSelectFolder('contacts')`.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement.** Add the Address section + Contacts item. **Coordination note for FZ-M1:** keep the item declared in/derived from the `MAILBOX_ITEMS`-style list so the icon-rail picks it up generically. Contacts count comes from `useContacts().contacts.length` (NOT the mailbox counts memo). `'contacts'` is a pseudo-folder string — do not extend the `MailboxFolder` enum. **Codex#11 — prevent a spurious mailbox query:** `AppShell` always calls `useMailbox(selectedFolder)`, and `contacts` matches user-folder slug rules → it would enable `mailbox_list({folder:'contacts'})`. Guard the mailbox hook so it is DISABLED for `selectedFolder==='contacts'` (e.g. pass an `enabled: isBackendFolder(selectedFolder)` predicate, or feed `useMailbox` an effective-folder that falls back to inbox/none for the pseudo-folder). The A9 mount test asserts NO `mailbox_list({folder:'contacts'})` invoke fires.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(contacts): Address sidebar group + Contacts nav item`.

### Task A8: ContactsPanel (list/detail + suggestions + add-from-sender)

**Files:**
- Create: `src/contacts/ContactsPanel.tsx`, `src/contacts/ContactEditor.tsx`
- Modify: `src/shell/AppShell.tsx` main-content switch (~869-929) — see M8 placement below.
- Modify: the message reading view (sender header) to add the "Add to contacts" action (G1).
- Mirror: `src/mailbox/MessageList.tsx` (virtuoso list), `src/mailbox/devFixture.ts` (fixtures).
- Test: `src/contacts/ContactsPanel.test.tsx`.

- [ ] **Step 1 — failing tests** (top-of-file `vi.mock('react-virtuoso', () => ({ Virtuoso: ({data,itemContent}) => <div>{data.map((m,i)=>itemContent(i,m))}</div> }))` — M10, else rows render empty under jsdom): list shows Groups section (avatars) ABOVE People; search filters both; selecting a row shows the detail pane (name, primary callsign, email/tactical, notes) with **New message** + **Edit** actions; the "Suggested" affordance lists suggest-from-history "+ Add" cards (from `contacts_suggestions`) annotated with the message count; **New message** drops the contact into Compose To (assert the invoke/route); **G1 — the message reading view's sender header has an "Add to contacts" action that pre-fills ContactEditor with the sender callsign and, on save, routes through `contact_upsert`** (assert the editor opens pre-filled and the invoke fires).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** the inline list+detail surface (~286px list column + detail pane). ContactEditor is the New/Edit form (callsign required; name/email/tactical/notes optional). "+ Add" card calls `contact_upsert`. New message routes the primary callsign into Compose. **M8 placement:** put the early-return right AFTER `<FolderSidebar>` inside `.panes`, wrapping BOTH `<MessageList>` AND the reading-pane region — `if (selectedFolder === 'contacts') return <ContactsPanel/>`. `<MessageList>` MUST NOT render when `selectedFolder==='contacts'` (otherwise two list columns). **G1 — Add to contacts:** wire the sender-header action to open ContactEditor pre-filled with the message sender, then `contact_upsert` on save.
- [ ] **Step 4 — run, expect PASS** (`pkill -9 -f vitest` after).
- [ ] **Step 5 — commit:** `feat(contacts): inline list/detail surface + suggestions + add-from-sender`.

### Task A8b: GroupEditor (create/edit groups + manage members)

**Files:**
- Create: `src/contacts/GroupEditor.tsx`
- Modify: `src/contacts/ContactsPanel.tsx` (new-group + edit-group entry points in the Groups section).
- Test: `src/contacts/GroupEditor.test.tsx`.

> **Codex#7 — v1 group management UI is REQUIRED.** The design (§38) ships groups in v1 with editable members; A8 only covers contact fields. Group membership is "managed in the Contacts surface" — that needs a concrete editor, not a TODO.

- [ ] **Step 1 — failing tests** (mock react-virtuoso per M10 if the member list uses it): GroupEditor renders name + member list; **Add member** adds a `GroupMember::Contact` from a contact picker OR a `GroupMember::Raw` from a typed callsign; **Remove member** drops one; a deleted-contact member is shown distinctly (e.g. "unknown contact" / greyed) rather than silently absent (M6); Save calls `group_upsert` with the assembled `members: GroupMember[]`; Cancel discards.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** GroupEditor: controlled member list, contact picker (reuse `matchRecipients`/contacts state), raw-callsign entry; deleted-contact members render with a distinct affordance. Wire new-group + edit-group buttons in ContactsPanel's Groups section.
- [ ] **Step 4 — run, expect PASS** (`pkill -9 -f vitest` after).
- [ ] **Step 5 — commit:** `feat(contacts): group editor (create/edit + member management)`.

### Task A9: App-level mount test (Contacts)

**Files:**
- Modify/create: `src/App.test.tsx` or `src/shell/AppShell.test.tsx` — add a case using `routeInvoke` (mirror `App.test.tsx:74-82`) that mounts the real tree, routes `contacts_read`, selects the Contacts folder, and asserts ContactsPanel renders inside the production provider stack.

- [ ] **Step 1 — failing test** (selecting Contacts in the full AppShell mount renders the panel). **Route ALL commands the production path fires (M9):** `contacts_read` → the seeded file, `contacts_suggestions` → `[]` (ContactsPanel fires it). Return `Promise.resolve(...)` for every routed command (never a raw value — react-query rejects undefined/non-promise reads). Add top-of-file react-virtuoso mock (M10). **Codex#11 — assert NO `mailbox_list({folder:'contacts'})` invoke fires** when the Contacts folder is selected.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — make it pass** (wire any missing provider/route).
- [ ] **Step 4 — run, expect PASS;** `pkill -9 -f vitest`.
- [ ] **Step 5 — commit:** `test(contacts): App-level mount path`.

---

# PART B — FAVORITES (`tuxlink-egmp`)

### Task B1: Rust favorites store + ToD bucketing + recents trim

**Files:**
- Create: `src-tauri/src/favorites/store.rs`
- Mirror: `src-tauri/src/user_folders.rs:load_registry` (degrade), `src-tauri/src/search/saved.rs` (CRUD shape + hand-written `Default`); SAME forward-compat + atomic-write + corrupt-file-preservation + `format!("{}.tmp", name)` conventions as A1 (see Locked "Store conventions").
- Test: in-file `#[cfg(test)]`.

- [ ] **Step 1 — failing tests:**
  - store CRUD + reopen-persist (favorites + log) like A1, including `open_on_corrupt_file_preserves_original_bytes` and `fresh_empty_store_has_schema_version_1` for `stations.json` (C1/M1).
  - `favorite_star(id, true)` flips `starred`; star-to-promote keeps it past the recents cap.
  - `record_attempt_appends` — appends a `ConnectionAttempt` with the EXACT `ts_local` passed (no UTC conversion); the unit's `last_attempt_at` is bumped; starred favorites are NEVER trimmed.
  - **`first_dial_creates_recent_and_links_attempt` (H3):** recording on a brand-new `(mode,gateway,freq|transport)` pair via `record_attempt` CREATES the recent (server assigns its id), and the appended attempt's `unit_id` equals that id; a SECOND record on the same pair reuses the same recent id (no duplicate unit). `favorites_recents(mode)` then contains the recent.
  - **`trim_evicts_least_recently_dialed` (M3):** with the cap (10) exceeded, eviction drops the recent with the SMALLEST `last_attempt_at` — NOT the smallest `created_at`. Dial an old-created entry to bump its `last_attempt_at`; overflow; assert it SURVIVES and a newer-created-but-staler-dialed entry is dropped.
  - **`trim_sweeps_orphaned_log_entries` + `delete_sweeps_log_entries` (M2):** when a non-starred recent is trimmed, its `ConnectionAttempt`s (`unit_id == dropped_id`) are removed from `log`; `favorite_delete` does the same orphan-sweep; assert no orphaned attempts remain. Also a per-unit cap (~50 most-recent attempts per `unit_id`) keeps `log` bounded.
  - `tod_bucket`: 06→`dawn`, 12→`day`, 18→`dusk`, 23→`night`, 02→`night`.
  - **`tod_hint` — offset-local hour (H1):** offset-bearing fixtures where local and UTC hours fall in DIFFERENT buckets: `2026-06-07T23:00:00-07:00` → `night` (NOT `dawn`/UTC-06), `2026-01-15T02:00:00+10:00` → `night`, `2026-06-07T06:00:00-07:00` → `dawn` (NOT `day`/UTC-13). An unparseable `ts_local` is skipped (no panic, no count).
  - **`tod_hint` — gating (H2):** returns `None` when the argmax bucket has <3 attempts; returns `None` when the argmax bucket has ≥3 attempts but ZERO successes (never name a zero-success bucket); returns `None` when there is no UNIQUE max (a tie does not produce a hint); returns `Some` only when the bucket has ≥3 attempts AND ≥1 (prefer ≥2) successes AND is a strict unique max.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** `FavoritesStore` (mirror A1) + pure fns:
  - `tod_bucket(hour: u8) -> &'static str` (dawn 5-7 / day 8-16 / dusk 17-19 / night 20-4).
  - `tod_hint(attempts: &[ConnectionAttempt]) -> Option<TodHint>` — **parse the LOCAL hour via `DateTime::parse_from_rfc3339(&a.ts_local).map(|dt| dt.hour())` on `DateTime<FixedOffset>` (already local). FORBID `.with_timezone(&Utc)` / `.naive_utc()` / `.timestamp()`** (H1 — those bucket in UTC and defeat the feature). Skip unparseable timestamps. Bucket each; require argmax-reached-fraction bucket to have ≥3 attempts AND ≥1 (prefer ≥2) successes AND be a strict unique max, else `None` (H2). Never name a zero-success bucket.
  - The record path (H3): upsert/find the `(mode,gateway,freq|transport)` recent FIRST, obtain/assign its id, bump `last_attempt_at`, stamp the attempt's `unit_id` server-side, then append. `ts_local` stored verbatim. Trim non-starred recents for that mode to **10** by least-recently-DIALED (`last_attempt_at`, M3); on trim AND on `favorite_delete`, sweep orphaned `log` entries (M2) and enforce the per-unit ~50 attempt cap.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(favorites): JSON store, ToD buckets, recents cap, log orphan-sweep`.

### Task B2: Rust favorites commands + registration

**Files:**
- Create: `src-tauri/src/favorites/commands.rs`; Modify `src-tauri/src/lib.rs:318` + setup.
- Test: in-file helper tests as needed.

- [ ] **Step 1 — failing test** for command-layer helpers: id stamping; mode validation that rejects an unknown mode string; **`upsert_preserves_starred_and_created_at` (M12)** — a stale `favorite_upsert` carrying `starred:false` over an already-starred favorite leaves `starred:true` and preserves `created_at`/`log`.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement:**
  - `favorites_read() -> StationsFile`
  - `favorite_upsert(favorite) -> Favorite` — **MERGE only operator-editable fields (M12):** `note`, `freq`, `transport`, `band`, `grid`, `gateway` (+ `name` if added) into the existing record by id; `favorite_star` and `favorite_record_attempt` are the ONLY writers of `starred`, `log`, and `last_attempt_at`. A stale whole-object upsert must NOT revert a concurrent star.
  - `favorite_delete(id)` — also sweeps orphaned `log` entries (M2).
  - `favorite_star(id, starred)`
  - **`favorite_record_attempt(dial: FavoriteDial, outcome: String, ts_local: String) -> ()` (H3/Codex#8):** the Rust path upserts/finds the `(dial.mode, dial.gateway, dial.freq|dial.transport)` recent FIRST, gets its id, stamps `unit_id` server-side, bumps `last_attempt_at`, appends the attempt (`ts_local` verbatim), then trims/sweeps (B1). Client never supplies `unit_id`. (Replaces the old `favorite_record_attempt(attempt, gateway, mode)` signature.)
  - `favorites_recents(mode) -> Vec<Favorite>`
  - Register in lib.rs `// favorites` section; manage `stations.json` store in the SAME setup match arm where `app_data_dir` was already resolved (C2) — reuse the resolved dir, do not re-resolve.
- [ ] **Step 4 — run, expect PASS;** full `cargo build`.
- [ ] **Step 5 — commit:** `feat(favorites): tauri commands + state registration`.

### Task B3: useFavorites hook + types

**Files:**
- Create: `src/favorites/types.ts`, `src/favorites/useFavorites.ts`; Mirror `useSavedSearches.ts`.
- Test: `src/favorites/useFavorites.test.ts`.

- [ ] **Step 1 — failing test:** `useFavorites(mode)` returns mode-filtered `{ favorites, recents, isLoading, upsert, remove, star, recordAttempt }`; mutations invalidate `['favorites']`. **`recordAttempt(dial: FavoriteDial, outcome: 'reached' | 'failed', ts_local: string)` (H3/Codex#8)** invokes `favorite_record_attempt` with those three args (no client `unit_id`).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** (queryKey `['favorites']`; filter by `mode` in the selector; recents = `starred:false`). `recordAttempt` forwards `{ dial, outcome, ts_local }` to the command. The OFFSET-BEARING `ts_local` is built by the caller in B6 (M4) — the hook does not synthesize it.
- [ ] **Step 4 — run, expect PASS;** reap vitest.
- [ ] **Step 5 — commit:** `feat(favorites): useFavorites hook`.

### Task B4: haversine distance util

**Files:**
- Create: `src/forms/position/distance.ts` (NEW file — minimizes `maidenhead.ts` merge conflict; export `haversineKm`)
- Mirror: `src/forms/position/maidenhead.ts:24` `gridToLatLon`.
- Test: `src/forms/position/distance.test.ts`.

- [ ] **Step 1 — failing tests:** `haversineKm(a: LatLon, b: LatLon): number` — known pair within tolerance; identical points → 0. `distanceBetweenGrids(gridA, gridB): number | null` — null if either grid is null/malformed (uses `gridToLatLon`).
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** haversine (R=6371 km) + `distanceBetweenGrids`. **Coordination: this is the shared helper the Catalog agent consumes — export `haversineKm` + `distanceBetweenGrids`.** The OPERATOR grid fed to `distanceBetweenGrids` comes from `position_current_fix.grid` (full precision) — this util is grid-agnostic, but the consuming surface (B5) must pin that source (C4); a `null` grid → `null` distance, never a crash.
- [ ] **Step 4 — run, expect PASS.**
- [ ] **Step 5 — commit:** `feat(position): shared haversine distance util`.

### Task B5: FavoritesTabs + rows + honest record rendering

**Files:**
- Create: `src/favorites/FavoritesTabs.tsx`, `src/favorites/FavoriteRow.tsx`, `src/favorites/ConnectionRecord.tsx`, `src/favorites/favorites-fixture.ts`
- Mirror: `@radix-ui/react-tabs` (consult Radix docs — FIRST use), `ChipStrip`/list idioms. **Operator grid source is `invoke("position_current_fix").grid` (full-precision `active_grid`, `Option<String>`) — FORBID `position_status`/`useStatus` `broadcast_grid`/`ui_grid` (precision-reduced) and stale `config_read.grid` (C4).** See Locked "Operator-grid → distance policy".
- Test: `src/favorites/FavoritesTabs.test.tsx`, `src/favorites/ConnectionRecord.test.tsx`.

- [ ] **Step 1 — failing tests** (top-of-file `vi.mock('react-virtuoso', …)` per M10 if rows use Virtuoso):
  - **Tabs vary by mode (M7):** for `ardop-hf`/`packet`/`telnet`, render Favorites / Recent / Manual; switching tabs shows the right list. **For VARA modes (`vara-hf`/`vara-fm`), render ONLY the Manual tab** — no Favorites/Recent tabs, NO dead Connect button. Manual shows the existing hand-entry fields (passthrough — rendered by the host panel).
  - A FavoriteRow shows star toggle, `gateway · band`, `freq · grid · distance` (distance from `distanceBetweenGrids` over `position_current_fix.grid`; **absent when operator grid is None / the fix has `grid:null` — assert no crash**, C4), the honest record line, and a Connect button.
  - A telnet FavoriteRow shows `host · transport` (CmsSsl/Telnet) — NO freq/band (H7).
  - **Distance consumes `position_current_fix` (C4):** assert the component invokes `position_current_fix` (not `position_status`) for the operator grid.
  - ConnectionRecord: renders the last few ✓/✗; **"reached 2 h ago · 21:42 local" where the wall-clock `21:42` is extracted from the `ts_local` OFFSET component (station clock), NOT the viewer's TZ (L2)** — add a test with a non-local offset asserting the shown wall-clock matches the offset, not the test-env TZ; the "ago" delta uses the absolute instant. Honest "no successful connect yet · 1 attempt failed 3 d ago" when none. Shows a ToD hint ONLY when the Rust `tod_hint` returned `Some` (≥3 attempts, ≥1 success, unique max — H2) and NEVER as a prediction.
  - star-to-promote: clicking the star on a Recent calls `favorite_star(id,true)`.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement** the components. `FavoritesTabs` accepts an `onPrefill` callback (Codex#3) — it is NEVER given a connect/invoke callback; clicking Connect calls `onPrefill(dial)`, the host panel sets its form state. Telnet rows show `host · transport`. Distance derived from `position_current_fix.grid`, never stored. Wall-clock display extracted from the `ts_local` offset (reuse/extend `formatRowDate`, MessageList.tsx:38).
- [ ] **Step 4 — run, expect PASS;** reap vitest.
- [ ] **Step 5 — commit:** `feat(favorites): per-mode tabs + honest connection record`.

### Task B6: Per-mode panel integration (ARDOP / Packet / Telnet) — RADIO-1

**Files:**
- Modify: `src/radio/modes/ArdopRadioPanel.tsx` (record around the on-air event; target input L606-617: quick-connect pre-fill via `setTarget`), `PacketRadioPanel.tsx` (`onConnect` ~L171-175), `TelnetRadioPanel.tsx` (`start()` ~L113-117 + `config_set_connect`)
- Add a tiny `src/favorites/ts-local.ts` helper (M4) for the offset-bearing timestamp.
- Test: `src/radio/modes/ArdopRadioPanel.test.tsx`, **`PacketRadioPanel.test.tsx`, `TelnetRadioPanel.test.tsx`** — each gets its OWN first-class record + consent-non-bypass test (Codex#2), not a buried "mirror" note.

> **Mounting (Codex#3):** FavoritesTabs is mounted PER MODE PANEL (ArdopRadioPanel / PacketRadioPanel / TelnetRadioPanel), NOT in the generic `RadioPanel` body — `RadioPanel` only renders children and has no `setTarget`/`setHost` setters. Each mode panel passes an `onPrefill` callback that sets ITS OWN form state. `FavoritesTabs` is never handed a connect/invoke callback.

> **What "reached" means (C3 — honesty):** `reached` is derived from the modem reaching a `connected-*` status state (the on-air link), NOT from a connect invoke merely resolving. `modem_ardop_connect` only spawns ardopcf + inits the TNC — it is NOT the on-air dial. Recording around `doConnect` would log "ardopcf started locally" as a gateway success (the opposite of the honest record). Do NOT record for pre-air busy-guard rejections ("connect already in progress") — those would bias the reached-fraction down before any RF.

- [ ] **Step 1 — failing tests:**
  - **ARDOP record (C3):** a `favorite_record_attempt` with `outcome:'reached'` fires when the modem reaches a `connected-*` state during a `modem_ardop_b2f_exchange` (on-air event), with `dial.gateway = status.peer`; `outcome:'failed'` when the exchange/connection fails. An invoke that RESOLVES but whose state NEVER reaches `connected-*` does NOT record `reached` (C3). Clicking Connect while a connect is in flight appends NO attempt (C3 — no pre-air busy-guard records).
  - **Packet/Telnet record (C3, H4):** `packet_connect`/`cms_connect` are blocking connect→B2F; record around the connect call. The REJECTED path actually fires `favorite_record_attempt` with `outcome:'failed'` — assert it (H4: Packet `onConnect` must be restructured so the failure is observable; today it is fire-and-forget `void invoke(...).catch(()=>{})`).
  - **CONSENT NON-BYPASS (critical, M13):** clicking a Favorite's Connect sets form state but invokes NEITHER the connect command NOR the on-air TX command — for ARDOP, neither `modem_ardop_connect` NOR `modem_ardop_b2f_exchange`; for Packet neither `packet_connect`; for Telnet neither `cms_connect`. Await a tick, then assert only a subsequent Start (and for ARDOP, Send/Receive) invokes them. ONE such test PER mode file (Codex#2).
  - **Prefill shape (H8):** ARDOP/Packet quick-connect pre-fills ONLY the target callsign (no freq field exists on those forms — dial freq is set on the physical radio, out of app scope). `favorite.freq` is carried into the `FavoriteDial` as record-only metadata, never read back from a form.
  - **Telnet prefill (H7):** quick-connect sets host + transport and calls `config_set_connect({host, transport})` so the next operator Start dials right; no freq.
  - **Offset-bearing ts_local (M4):** `recordAttempt` is called with a NON-`Z` offset-bearing `ts_local` (assert the string matches `±HH:MM`, not `Z`).
  - **VARA (M7):** a VARA mode renders no Favorites/Recent Connect button (Manual-only) — invokes no transport command.
- [ ] **Step 2 — run, expect FAIL.**
- [ ] **Step 3 — implement.**
  - Mount FavoritesTabs in each mode panel with an `onPrefill` callback that setStates that panel's connect form (Codex#3). Quick-connect = setState only (+ for Telnet, `config_set_connect`). VARA renders Manual-only (M7).
  - **`ts-local.ts` helper (M4):** produce an offset-bearing ISO8601 string by appending `±HH:MM` derived from `getTimezoneOffset()`. **FORBID `toISOString()`** for this field (it emits `Z`, stripping the offset and defeating the ToD-local feature).
  - **ARDOP record (C3, H4):** wire `recordAttempt(dial, 'reached', tsLocal())` to the on-air event — the status state machine reaching `connected-*` during `modem_ardop_b2f_exchange` — and `recordAttempt(dial, 'failed', tsLocal())` to the exchange failure branch (in the existing `catch`, NOT a `finally`, so busy-guard pre-air rejections don't record). `dial.gateway = status.peer`.
  - **Packet (H4):** convert `onConnect` to `async`, `await packet_connect`, record `reached` in the resolve branch and `failed` in the `catch` (NOT `finally`).
  - **Telnet (H4):** put `recordAttempt(... 'reached' ...)` in the success branch and `recordAttempt(... 'failed' ...)` in the existing `catch`; build the `FavoriteDial` with `transport` (no freq/band).
- [ ] **Step 4 — run, expect PASS;** reap vitest.
- [ ] **Step 5 — commit:** `feat(favorites): radio-dock integration + record-on-air-link (RADIO-1 pre-fill only)`.

### Task B7: App-level mount test (Favorites)

**Files:**
- Modify: `src/radio/modes/ArdopRadioPanel.test.tsx` or an AppShell-level test — mount the panel through the production provider stack; route `favorites_read` AND `favorites_recents` → `[]` (useFavorites fires both — M9) AND `position_current_fix` (B5 distance source) — each as `Promise.resolve(...)`; assert tabs render + a favorite's Connect pre-fills without transmitting (invokes NEITHER connect NOR on-air command). Add the react-virtuoso mock (M10).

- [ ] **Step 1 — failing test.** **Step 2 — FAIL. Step 3 — pass. Step 4 — PASS** (reap vitest). **Step 5 — commit:** `test(favorites): App-level mount path`.

---

## Cross-cutting wrap-up

### Task C1: design-gap note + open-items doc
- [ ] Record the **packet relay-chain favorites gap** (Favorite schema has no relay field) as a bd follow-up issue + a `bd remember` on `tuxlink-egmp`; do NOT add the field in v1 (additive later).
- [ ] Confirm the four open-item defaults are documented in this plan (done above) and call them out in the PR body as "proposed → adrev-converged".

### Task C2: Codex cross-provider adversarial review (build-robust-features requirement)
- [ ] Run a directed Codex `review -` round per CLAUDE.md "Adversarial-review pattern" (stdin custom-prompt form). Attack angles: (1) RADIO-1 — can quick-connect ever reach a connect OR on-air TX command without a Start click? (2) group expansion — any path where a `group:<uuid>` token reaches the B2F builder, or a wire-key dedup miss double-sends `W6ABC` + `w6abc@winlink.org`? (3) store durability — partial write / concurrent stale upsert clobbering a star / corrupt-file preservation / startup-block on I/O error. (4) ToD hint — can it over-claim a zero-success or non-unique-max bucket, or mis-bucket across the offset? (5) distance — precision-reduced-grid leak or None-grid crash. (6) cross-window contact staleness — can an open Compose expand a stale group after a main-window edit? Tee to `dev/adversarial/2026-06-07-contacts-favorites-codex.md`; verify `wc -l` >> 5 (not a stub). Triage findings; fix or document disposition.

### Task C3: full quality gate
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` (full) green; `cargo build` green.
- [ ] `pnpm vitest run src/contacts src/favorites src/compose src/forms/position` (narrow scope) green; `pkill -9 -f vitest` after.
- [ ] `pnpm tsc --noEmit` clean. Lint clean.
- [ ] Self-smoke via converged-style `tauri dev` from THIS worktree (after freeing :1420): grim-screenshot the Contacts surface (pre-seed `contacts.json`) + a radio panel's Favorites tab (pre-seed `stations.json`). Evidence to `dev/scratch/smoke/`.

---

## Self-Review (run before handing to build-robust-features)

**Spec coverage:** A1-A3 (store/commands/suggestions) = design A.1/A.3/A.5; A4-A6 = A.4 autocomplete (incl. email/tactical alternates) + A.2 group expansion-at-send; A7-A8 = A.4 sidebar + list/detail + add-from-sender (G1); **A8b = A.2 group management UI (GroupEditor, Codex#7)**; B1-B2 = B.1/B.3/B.5 store/ToD/commands; B3 = hook; B4 = B.1 derived distance; B5 = B.2/B.3/B.4 tabs+record; B6 = B.4 quick-connect + RADIO-1; tests per design §Testing. **Gap check:** RMS-list ingest (B.6) = forward hook, out of scope ✓. Cross-mode "all stations" = out of scope ✓. "Add to contacts" on a sender = NOW IN SCOPE (A8, G1) ✓. Packet relay-chain favorites = explicit forward gap (Task C1) ✓.

**Adversarial-review fold-in (rounds 1-4 punchlist C1-G1 + Codex 13 findings):** all confirmed findings applied at-task. CRITICAL: C1 (drop `deny_unknown_fields`, `<name>.corrupt-<ts>` preservation, manual Default), C2 (`open()->Self` infallible, `user_folders.rs:load_registry` mirror, app_data_dir resolved once at setup), C3 (record on `connected-*`/on-air `b2f_exchange`, not invoke-resolve), C4 (`position_current_fix.grid` full-precision; internal-distance-use is policy-consistent). HIGH: H1 (offset-local hour via `FixedOffset`), H2 (≥1 success + unique-max gate), H3+Codex#8 (`FavoriteDial` DTO, server-stamped `unit_id`), H4 (per-handler async, record in catch not finally), H5 (`group:<uuid>` sentinel), H6 (wire-key dedup), H7 (telnet `transport` not free port), H8 (target-only prefill, no freq form), H9 (`contacts:changed` Tauri event), H10 (RecipientInput diverges: input-scoped onKeyDown, Enter-commits-raw), H11 (self-exclusion + normalized suggest). MEDIUM: M1 (manual Default), M2 (log orphan-sweep + per-unit cap), M3 (LRU-dialed eviction), M4 (offset-bearing `ts_local`, no `toISOString()`), M5 (SSID = identity), M6 (resolved memberCount), M7 (VARA Manual-only), M8 (early-return wraps MessageList+pane), M9 (route all mount-test invokes), M10 (per-file virtuoso mock), M11 (no-select binds NEW combobox only), M12 (upsert merges editable fields only), M13 (consent test asserts neither connect nor TX command). LOW: L1 (`format!("{}.tmp",name)`), L2 (wall-clock from `ts_local` offset), L3 (pinned per-task vitest + `cargo test --manifest-path … contacts::store`). Codex extras: #2 (per-mode consent test files), #3 (`onPrefill` callback, per-mode mount not generic RadioPanel), #7 (GroupEditor task A8b), #11 (mailbox hook disabled for `contacts` pseudo-folder), #12 (email/tactical alternates), #13 (Task 0 creates stub files).

**Placeholder scan:** boilerplate CRUD is complete-by-reference to named mirror files with locked field shapes (acceptable per "follow established patterns"); novel logic (expansion/wire-key dedup, ToD bucket+gated hint, haversine, LRU-dialed recents trim + orphan-sweep, corrupt-file preservation, offset-local timestamp, consent-non-bypass test) is spelled out. No "TBD"/"handle edge cases".

**Type consistency:** `ContactsFile`/`StationsFile` (`#[serde(default)]`, hand-written `Default`, `SCHEMA_VERSION`), `GroupMember{Contact,Raw}`, `group:<uuid>` sentinel, `ConnectionAttempt.ts_local`/`unit_id` (server-stamped), `Favorite.transport`/`last_attempt_at`, `FavoriteDial`, `RadioMode`, `expandGroupsAndDedup`, `haversineKm`/`distanceBetweenGrids`, `tod_bucket`/`tod_hint`, `favorite_record_attempt(dial, outcome, ts_local)`, `contacts:changed` event — names used identically across tasks and frontend/Rust DTOs (snake_case). The old `Favorite.port` and the old `favorite_record_attempt(attempt, gateway, mode)` signature are REPLACED everywhere; no task references them.
