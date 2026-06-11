# Phase 7: UI — Top-bar Switcher + Inline Unlock — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement each task below in order. Steps use checkbox (`- [ ]`) syntax. Every step pairs a RED test with the GREEN implementation that satisfies it — write the test, watch it fail, then write the code. Do NOT batch tests-then-impl across a task boundary.

**Goal:** Surface the Phase 1–6 identity stack to the operator. (a) A Tauri command surface — `identity_list` / `identity_add_full` / `identity_add_tactical` / `identity_remove` / `identity_switch` / `identity_active` — projecting the in-backend `IdentityStore` + active `SessionIdentity` to the frontend with **no secrets in any DTO**. (b) The switcher UI: a dropdown **anchored to the existing top-status-bar callsign control** (`.dash-callsign-row` / `data-testid="ribbon-callsign"`) — one click opens it; it lists FULL identities with TACTICAL labels nested under their parent, a lock glyph where authentication is needed, and a per-tactical CMS ✓/blocked badge; selecting a FULL identity that needs auth reveals an **inline unlock field within the same dropdown** (no popup window). The closed-state footprint is unchanged. (c) A mailbox identity filter. (d) Listener identity badges in the connections/radio panel.

**Architecture:** The Tauri command layer is a thin, serializable projection of the Phase 1–6 state — it never returns an `IdentityHandle` (non-`Serialize` by construction) or any keyring secret. `identity_switch(address, credential)` calls `IdentityService::authenticate` behind the backend boundary and sets the active `SessionIdentity`; the frontend only ever sees the resulting `ActiveIdentityDto` ({ mycall, address_as, is_tactical }). The React switcher mirrors the established **inline-edit-in-the-ribbon** pattern from `GridEdit.tsx` (a closed display chip that swaps to an inline editing surface on click, Esc-cancels, no popup window) and the **SSID `<select>`** pattern already living in `.dash-callsign-row`.

**Tech stack:** Rust (Tauri backend, `#[tauri::command]` async fns returning `Result<_, UiError>`, `serde` DTOs), React/TS frontend (vitest + React Testing Library + `tsc --noEmit`), `@tauri-apps/api/core` `invoke`, `@tanstack/react-query` for cache invalidation.

**Spec:** [`docs/superpowers/specs/2026-06-10-multiple-tactical-callsigns-design.md`](../specs/2026-06-10-multiple-tactical-callsigns-design.md) (esp. the UI section).
**Master plan + canonical interface contract:** [`docs/superpowers/plans/2026-06-10-tactical-callsigns-master-plan.md`](2026-06-10-tactical-callsigns-master-plan.md). The Rust type names (`IdentityStore`, `FullIdentity`, `TacticalIdentity`, `TacticalCmsState`, `IdentityHandle`, `SessionIdentity`, `IdentityService`, `IdentityError`, `Address`, `Callsign`) and the command names in this plan are reproduced **verbatim** from that contract — do not rename.
**bd issue:** tuxlink-noa0 (Phase 7). Depends on Phases 3/4/5/6 (all merged before this starts).

**Project rules in force for this plan:**
- Frontend tests: `npx vitest run <file>` (scoped — never a bare `npx vitest run` full sweep) + `npx tsc --noEmit`. **REAP vitest workers after every run**: `pkill -f vitest` (zombies leak ~8.5 GB; verify `pgrep -f vitest` is empty).
- Rust tests: `cargo test --manifest-path src-tauri/Cargo.toml <name>`.
- Pre-push gate parity: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` (re-run until exit 0; `--all-targets` hides later-target lints behind earlier ones) AND the full `npx vitest run` once before push (scoped runs miss far-away contract/snapshot tests).
- Commit trailers: every commit ends with `Agent: sandbar-raven-fox` + the `Co-Authored-By:` trailer.
- UI smoke is via **WebKitGTK / grim**, NOT Chromium (Chromium is not a WebKitGTK proxy — it clips what WebKit fits). Smoke is opportunistic/post-merge, **not a pre-merge gate** (CI on both arches is the merge gate).
- This is a UI/command-surface phase: no RF transmission, no live-CMS run. `identity_switch` exercises the keyring through `IdentityService` (Phase 1), validated with the test keyring backend — no network, no radio.

---

## File Structure

### New files (frontend)
- `src/shell/identityTypes.ts` — TS mirror of the Rust DTOs (`IdentityListDto`, `IdentityRowDto`, `TacticalRowDto`, `ActiveIdentityDto`, `TacticalCmsBadge`), plus the `parseIdentityError` helper mirroring the existing `parseUiError` shape.
- `src/shell/useIdentities.ts` — react-query hooks: `useIdentityList()` (queryKey `['identity_list']`), `useActiveIdentity()` (queryKey `['identity_active']`), and a `useIdentitySwitch()` mutation that invalidates both on success.
- `src/shell/useIdentities.test.ts` — hook tests (mocked `invoke`).
- `src/shell/IdentitySwitcher.tsx` — the dropdown anchored to `.dash-callsign-row`; renders the closed chip (active call + SSID, unchanged footprint) and the open list (FULL rows, nested TACTICAL rows, lock glyph, CMS badge, inline unlock field).
- `src/shell/IdentitySwitcher.test.tsx` — switcher behavior tests.
- `src/shell/IdentitySwitcher.css` — dropdown + row + lock-glyph + CMS-badge + inline-unlock styles (imported by the component; the closed-chip rules stay in `AppShell.css` alongside `.dash-callsign-row`).
- `src/mailbox/identityFilter.ts` — pure predicate `messageMatchesIdentity(msg, filter)` + the filter-option derivation.
- `src/mailbox/identityFilter.test.ts` — predicate unit tests.

### New files (backend)
- `src-tauri/src/ui_commands_identity.rs` — the 6 identity commands + their DTOs (kept in a sibling module so the 8000-line `ui_commands.rs` does not grow further; `pub mod ui_commands_identity;` added to `lib.rs`). DTOs: `IdentityListDto`, `IdentityRowDto`, `TacticalRowDto`, `ActiveIdentityDto`, `TacticalCmsBadge`.
- `src-tauri/src/ui_commands_identity.rs` carries its own `#[cfg(test)] mod tests` (DTO projection + command tests against a test `IdentityService` / `IdentityStore`).

### Existing files changed
- `src-tauri/src/lib.rs` — `pub mod ui_commands_identity;` (near line 22, with the other `pub mod ui_commands;`); register the 6 commands in `generate_handler!` (near line 506, beside `config_read`).
- `src/shell/DashboardRibbon.tsx` — replace the bare `.dash-callsign-row` inner markup (lines 134–173) with `<IdentitySwitcher>`, threading new optional props (`identities`, `activeIdentity`, `onSwitchIdentity`); keep the SSID `<select>` and the no-handler fallback intact.
- `src/shell/AppShell.css` — extend the `.dash-callsign-row` block (lines 207–259) so the switcher's closed chip is pixel-identical to today's; the open dropdown's own rules live in `IdentitySwitcher.css`.
- `src/shell/AppShell.tsx` — wire `useIdentityList` / `useActiveIdentity` / `useIdentitySwitch` and pass them into `DashboardRibbon`; thread the mailbox identity-filter state into `MessageList`; pass the active/armed identity badge into the radio panel.
- `src/mailbox/MessageList.tsx` — add the identity-filter control to the list toolbar (beside `MessageListSortControl`, line ~440) and filter `sortedMessages` through `messageMatchesIdentity`.
- `src/mailbox/types.ts` — add an optional `identity?: string` field to `MessageMeta` (the identity a message was sent/received as; absent → matches the "All identities" filter only).
- `src/radio/sections/ListenSection.css` + the listener arm surface (`src/radio/sections/ListenArmButton.tsx` / `useListenerState.ts`) — render a per-listener identity badge (the bound `mycall` / tactical label captured at arm time).

---

## SESSION 1 — Tauri commands + DTOs (Rust)

> Session-1 deliverable: the 6 commands registered + green + clippy-clean, with no frontend changes. Stop at the `## SESSION BREAK` marker. The React work is Session 2.

### Task 1 — Identity DTOs (no secrets) + projection from Phase-1 types

**Files:**
- `src-tauri/src/ui_commands_identity.rs` (NEW — DTOs + `From`/projection fns + `#[cfg(test)] mod tests`)
- `src-tauri/src/lib.rs` (anchor: `pub mod ui_commands;` ~line 22 — add `pub mod ui_commands_identity;`)

DTO shapes (snake_case wire fields to match the TS mirror; secrets NEVER appear):

```rust
// src-tauri/src/ui_commands_identity.rs
use serde::Serialize;
use crate::identity::{IdentityStore, FullIdentity, TacticalIdentity, TacticalCmsState, Address, SessionIdentity};
use crate::ui_commands::UiError;

/// Per-tactical CMS reachability, projected from `TacticalCmsState`. No timestamps
/// leak to the wire beyond a coarse "checked" flag the UI needs for the badge.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(tag = "kind")]
pub enum TacticalCmsBadge {
    Unknown,           // never checked / offline-uncached → UI shows a neutral "?" (CMS blocked, fail-closed)
    Registered,        // CMS ✓ — tactical may use CMS modes
    NotRegistered,     // CMS blocked — explicit
}

impl From<&TacticalCmsState> for TacticalCmsBadge {
    fn from(s: &TacticalCmsState) -> Self {
        match s {
            TacticalCmsState::Unknown => TacticalCmsBadge::Unknown,
            TacticalCmsState::Registered { .. } => TacticalCmsBadge::Registered,
            TacticalCmsState::NotRegistered { .. } => TacticalCmsBadge::NotRegistered,
        }
    }
}

/// A tactical row nested under its parent FULL identity.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct TacticalRowDto {
    pub label: String,
    pub parent: String,        // parent callsign as string
    pub cms: TacticalCmsBadge,
}

/// A FULL identity row. `needs_auth` is true whenever this identity is NOT the
/// currently-authenticated active principal (switching to it requires the keyring
/// credential). NO secret, NO keyring value — just the boolean gate.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct IdentityRowDto {
    pub callsign: String,
    pub label: Option<String>,
    pub has_cms_account: bool,
    pub cms_registered: bool,
    pub needs_auth: bool,
    pub tacticals: Vec<TacticalRowDto>,
}

/// The whole list: FULL identities (each carrying its nested tacticals) + the
/// persisted "last selected" hint (NOT authority — display-only).
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct IdentityListDto {
    pub identities: Vec<IdentityRowDto>,
    pub last_selected: Option<String>,   // Address rendered as a string; UI pre-highlights this row
}

/// The active SessionIdentity projected for the closed-state chip + header.
/// `mycall` is ALWAYS the Part-97 station ID (handle.full_callsign); `address_as`
/// is the Winlink From (full callsign OR tactical label).
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ActiveIdentityDto {
    pub mycall: String,
    pub address_as: String,
    pub is_tactical: bool,
}
```

**TDD steps:**

- [ ] Create `src-tauri/src/ui_commands_identity.rs` with the DTO definitions above and the `From<&TacticalCmsState> for TacticalCmsBadge` impl. Add `pub mod ui_commands_identity;` to `lib.rs` directly beneath `pub mod ui_commands;` (~line 22).
- [ ] Write a `#[cfg(test)] mod tests` with `tactical_cms_badge_projects_from_state`: assert each `TacticalCmsState` variant maps to the matching `TacticalCmsBadge` and that `Registered { checked_unix: 123 }` and `Registered { checked_unix: 999 }` both project to `TacticalCmsBadge::Registered` (timestamp is dropped — no leak).
- [ ] Write `identity_list_dto_serializes_without_secrets`: build an `IdentityListDto` and `serde_json::to_string` it; assert the JSON contains `"callsign"`, `"needs_auth"`, `"tacticals"`, `"cms"` and assert it does NOT contain any of `"secret"`, `"credential"`, `"password"`, `"keyring"`, `"handle"`.
- [ ] Write a projection fn `pub fn project_list(store: &IdentityStore, active: Option<&SessionIdentity>) -> IdentityListDto`. For each FULL identity: `needs_auth = active.map(|s| s.mycall().as_str() != full.callsign.as_str()).unwrap_or(true)` (no active session ⇒ everything needs auth). Nest each `TacticalIdentity` whose `parent == full.callsign` as a `TacticalRowDto`. `last_selected = store.last_selected().map(render_address)`.
- [ ] Write `project_list_sets_needs_auth_for_non_active`: a store with FULL `W1ABC` + FULL `W7XYZ`, active session authenticated as `W1ABC`; assert the `W1ABC` row has `needs_auth == false` and `W7XYZ` has `needs_auth == true`. With `active = None`, both are `true`.
- [ ] Write `project_list_nests_tacticals_under_parent`: store with FULL `W1ABC` and TACTICAL `EOC-3` (parent `W1ABC`); assert the `W1ABC` row's `tacticals` has one entry `{ label: "EOC-3", parent: "W1ABC", cms: Unknown }` and the `W7XYZ` row's `tacticals` is empty.
- [ ] Run: `cargo test --manifest-path src-tauri/Cargo.toml ui_commands_identity::tests` → all green. Then `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → exit 0.
- [ ] **Commit** `feat(identity): Phase 7 identity DTOs (no secrets) + list projection` ending `Agent: sandbar-raven-fox`.

### Task 2 — `identity_list` + `identity_active` (read commands)

**Files:**
- `src-tauri/src/ui_commands_identity.rs` (append the two command fns + tests)

These read the Tauri-managed backend state: the persisted `IdentityStore` + the in-memory `Option<SessionIdentity>` active default (master plan §"Active-session backend state"). Mirror the `config_read` / `backend_status` `State<…>` access pattern in `ui_commands.rs` (~lines 2711–2826). Failures map to `UiError` (reuse the existing enum; identity I/O → `UiError::Internal { detail }`).

```rust
#[tauri::command]
pub async fn identity_list(state: State<'_, AppIdentityState>) -> Result<IdentityListDto, UiError> { /* lock store + active, project_list */ }

#[tauri::command]
pub async fn identity_active(state: State<'_, AppIdentityState>) -> Result<Option<ActiveIdentityDto>, UiError> {
    // Some(ActiveIdentityDto { mycall: s.mycall().as_str().into(),
    //   address_as: render_address(s.address_as()), is_tactical: matches!(s.address_as(), Address::Tactical(_)) })
    // None when no SessionIdentity is active (pre-auth / fresh launch — re-auth required, Phase 6).
}
```

> `AppIdentityState` is the managed-state wrapper holding `Arc<Mutex<IdentityStore>>` + `Arc<Mutex<Option<SessionIdentity>>>`. If Phases 3/6 already introduced this managed state, reuse it verbatim and DELETE this note; if not, define it here next to the commands and register it via `.manage(...)` in `lib.rs` `run()` (anchor: the existing `.manage(...)` chain near the `generate_handler!` call). Confirm by `grep -n "AppIdentityState\|SessionIdentity" src-tauri/src/*.rs` before writing — do not duplicate an existing managed state.

**TDD steps:**

- [ ] `grep -n "AppIdentityState\|\.manage(" src-tauri/src/lib.rs src-tauri/src/winlink_backend.rs` to determine whether the active-session managed state already exists (Phase 3/6). Record the finding inline in the module doc-comment.
- [ ] Write `identity_active_none_when_no_session`: with the active `Option<SessionIdentity>` = `None`, the projection helper backing `identity_active` returns `None`. (Test the inner helper, not the `#[tauri::command]` async wrapper — Tauri `State` is awkward to construct in unit tests; factor the body into `fn active_dto(active: Option<&SessionIdentity>) -> Option<ActiveIdentityDto>` and test that.)
- [ ] Write `identity_active_full`: an active `SessionIdentity::full(handle_for("W1ABC"))` → `Some({ mycall: "W1ABC", address_as: "W1ABC", is_tactical: false })`. Build the `IdentityHandle` via the Phase-1 test constructor (`IdentityService::authenticate` against the test keyring backend — the only legal way to mint a handle).
- [ ] Write `identity_active_tactical`: active `SessionIdentity::tactical(handle_for("W1ABC"), "EOC-3")` (where `EOC-3` is registered under `W1ABC` in the store) → `Some({ mycall: "W1ABC", address_as: "EOC-3", is_tactical: true })`. Asserts `mycall` is the FULL callsign (Part-97 station ID) even though `address_as` is the tactical label.
- [ ] Write `identity_list_inner_returns_store_projection`: factor the command body into `fn list_dto(store: &IdentityStore, active: Option<&SessionIdentity>) -> IdentityListDto` (= `project_list`) and assert it round-trips a store with 2 FULL + 1 TACTICAL.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml ui_commands_identity` → green. `cargo clippy … --all-targets -- -D warnings` → exit 0.
- [ ] **Commit** `feat(identity): identity_list + identity_active read commands` ending `Agent: sandbar-raven-fox`.

### Task 3 — `identity_add_full` / `identity_add_tactical` / `identity_remove` (mutations)

**Files:**
- `src-tauri/src/ui_commands_identity.rs` (append the three command fns + tests)

```rust
#[tauri::command]
pub async fn identity_add_full(
    state: State<'_, AppIdentityState>,
    callsign: String, label: Option<String>, has_cms_account: bool, activation_secret: String,
) -> Result<(), UiError> {
    // 1. Callsign::parse(&callsign)?            -> InvalidCallsign => UiError::Rejected
    // 2. store.add_full(FullIdentity { ... })?  -> persists list (NO secret in the list)
    // 3. service.set_activation_secret(&cs, &activation_secret)?  -> secret to keyring ONLY
    // 4. store.save()?
}

#[tauri::command]
pub async fn identity_add_tactical(state: State<'_, AppIdentityState>, label: String, parent: String) -> Result<(), UiError> {
    // Callsign::parse(parent)? ; store.add_tactical(TacticalIdentity { label, parent, cms: Unknown })?  -> ParentNotFound => UiError::Rejected ; save
}

#[tauri::command]
pub async fn identity_remove(state: State<'_, AppIdentityState>, address: String) -> Result<(), UiError> {
    // parse address (Full vs Tactical) ; store.remove(&addr)? -> RemoveHasTacticals => UiError::Rejected("…has tactical labels…") ; clear_activation_secret on FULL removal ; save
}
```

Map `IdentityError` → `UiError`: `InvalidCallsign`/`InvalidTactical`/`ParentNotFound`/`RemoveHasTacticals`/`CredentialMismatch`/`NoSecretSet` → `UiError::Rejected(msg)`; `Keyring`/`Io` → `UiError::Internal { detail }`; `UnknownIdentity` → `UiError::NotFound`. Add a `impl From<IdentityError> for UiError` so every command uses `?`.

**TDD steps:**

- [ ] Write `add_full_persists_identity_not_secret`: factor the body into `fn do_add_full(store, service, callsign, label, has_cms_account, secret) -> Result<(), IdentityError>`. Call it; assert the store now has a FULL `W1ABC`, assert `serde_json::to_string(&store).unwrap()` does NOT contain the secret string, and assert `service.authenticate(&cs, secret)` succeeds (secret landed in keyring).
- [ ] Write `add_full_rejects_bad_callsign`: `do_add_full` with `"bad call"` (whitespace) → `Err(IdentityError::InvalidCallsign(_))`, and the `From<IdentityError> for UiError` maps it to `UiError::Rejected`.
- [ ] Write `add_tactical_requires_known_parent`: `do_add_tactical` with parent `"W9ZZZ"` not in the store → `Err(IdentityError::ParentNotFound)` → `UiError::Rejected`. With parent `"W1ABC"` present → `Ok` and the tactical is nested under it on the next `project_list`.
- [ ] Write `remove_full_with_tacticals_rejected`: store with FULL `W1ABC` + TACTICAL `EOC-3`; `do_remove(Address::Full("W1ABC"))` → `Err(RemoveHasTacticals)` → `UiError::Rejected`. After removing `EOC-3` first, removing `W1ABC` succeeds AND clears its activation secret (`service.authenticate` afterward → `Err(NoSecretSet)`).
- [ ] Write `identity_error_maps_to_ui_error`: table-test every `IdentityError` variant → expected `UiError` variant.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml ui_commands_identity` → green. `cargo clippy … --all-targets -- -D warnings` → exit 0.
- [ ] **Commit** `feat(identity): identity add/remove commands with keyring-only secrets` ending `Agent: sandbar-raven-fox`.

### Task 4 — `identity_switch` (authenticate + set active)

**Files:**
- `src-tauri/src/ui_commands_identity.rs` (append + tests)

```rust
#[tauri::command]
pub async fn identity_switch(
    state: State<'_, AppIdentityState>,
    address: String, credential: String,
) -> Result<(), UiError> {
    // parse address. The FULL callsign to authenticate is:
    //   Address::Full(cs)      -> cs
    //   Address::Tactical(lbl) -> the parent FULL of `lbl` (lookup in store; ParentNotFound => Rejected)
    // handle = service.authenticate(&full, &credential)?   // CredentialMismatch/NoSecretSet => UiError::Rejected
    // session = match address {
    //     Full     => SessionIdentity::full(handle),
    //     Tactical => SessionIdentity::tactical(handle, label)?,   // err unless label registered under handle.full_callsign
    // };
    // *active.lock() = Some(session);          // in-memory ONLY — never persisted
    // store.set_last_selected(addr); store.save();   // persist the DISPLAY hint, not the session
}
```

> The credential is taken by value, used for `authenticate`, and dropped at end of scope. Do NOT log it, do NOT echo it in any error message, do NOT store it anywhere. `CredentialMismatch` maps to `UiError::Rejected("Credential did not match.")` — a generic message (no "wrong password for W7XYZ" that confirms the callsign exists beyond what `identity_list` already shows).

**TDD steps:**

- [ ] Write `switch_full_authenticates_and_sets_active`: factor `fn do_switch(store, service, active, address, credential) -> Result<(), IdentityError>`. Add FULL `W7XYZ` with secret `"hunter2"`; `do_switch(Address::Full("W7XYZ"), "hunter2")` → `Ok`; assert `active` is now `Some` with `mycall == "W7XYZ"` and `store.last_selected() == Some(Address::Full("W7XYZ"))`.
- [ ] Write `switch_wrong_credential_rejected_and_active_unchanged`: pre-set active to `W1ABC`; `do_switch(Address::Full("W7XYZ"), "wrong")` → `Err(CredentialMismatch)`; assert `active` STILL holds `W1ABC` (a failed switch must not clear the prior session) and `last_selected` is unchanged.
- [ ] Write `switch_tactical_authenticates_parent`: TACTICAL `EOC-3` under `W1ABC` (secret `"pw"`); `do_switch(Address::Tactical("EOC-3"), "pw")` → `Ok`; assert active `mycall == "W1ABC"`, `address_as == Tactical("EOC-3")`, `is_tactical`.
- [ ] Write `switch_tactical_unregistered_label_rejected`: `do_switch(Address::Tactical("GHOST"), "pw")` where `GHOST` is not in the store → `Err(ParentNotFound)` (no parent to authenticate). And a label whose parent secret is wrong → `Err(CredentialMismatch)`.
- [ ] Write `switch_does_not_persist_session`: after a successful `do_switch`, assert `serde_json::to_string(&store)` contains `last_selected` but NOT `mycall`/`handle`/any session field (the session is in-memory only).
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml ui_commands_identity` → green. `cargo clippy … --all-targets -- -D warnings` → exit 0.
- [ ] **Commit** `feat(identity): identity_switch — authenticate + set active session` ending `Agent: sandbar-raven-fox`.

### Task 5 — Register the 6 commands + managed state in `generate_handler!`

**Files:**
- `src-tauri/src/lib.rs` (anchors: the `.manage(...)` chain in `run()`; the `generate_handler!` list near `crate::ui_commands::config_read,` ~line 506)

**TDD steps:**

- [ ] Register the managed `AppIdentityState` via `.manage(AppIdentityState::new(...))` in `run()` IF Task 2's grep showed it is not already managed by Phase 3/6. (If already managed, skip — do not double-`.manage` the same type; Tauri panics on duplicate-typed managed state.)
- [ ] Add the six registrations to `generate_handler!` beside `config_read`, each on its own line with a trailing comment:
  ```rust
  crate::ui_commands_identity::identity_list,        // Phase 7 (tuxlink-noa0)
  crate::ui_commands_identity::identity_active,      // Phase 7 (tuxlink-noa0)
  crate::ui_commands_identity::identity_add_full,    // Phase 7 (tuxlink-noa0)
  crate::ui_commands_identity::identity_add_tactical,// Phase 7 (tuxlink-noa0)
  crate::ui_commands_identity::identity_remove,      // Phase 7 (tuxlink-noa0)
  crate::ui_commands_identity::identity_switch,      // Phase 7 (tuxlink-noa0)
  ```
- [ ] Build the whole backend to prove the `generate_handler!` macro accepts the command signatures (the macro fails to compile if a command's args aren't `Deserialize` or its return isn't `Serialize`): `cargo build --manifest-path src-tauri/Cargo.toml` → exit 0. This is the contract test that the DTOs are wire-legal.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml` (full backend suite, scoped to the crate) → green. `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → exit 0.
- [ ] **Commit** `feat(identity): register Phase 7 identity command surface` ending `Agent: sandbar-raven-fox`.

### Session-1 gate (before the break)

- [ ] `cargo test --manifest-path src-tauri/Cargo.toml ui_commands_identity` → all green.
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → exit 0 (re-run until clean; `--all-targets` reveals test-target lints).
- [ ] `cargo build --manifest-path src-tauri/Cargo.toml` → exit 0.
- [ ] Push the Session-1 commits (never hold a green unit of work locally). Update bd: `bd update tuxlink-noa0` note "Session 1 (commands+DTOs) green + pushed; Session 2 = React UI."

---

## SESSION BREAK

**Session 1 is DONE when:** all six Tauri commands + their DTOs exist, compile, are registered in `generate_handler!`, the `ui_commands_identity` test module is green, clippy `--all-targets` is clean, and the commits are pushed. No frontend file has changed yet.

**Resume Session 2 with:** a fresh session that READS this plan from the `## SESSION 2` marker, confirms the Session-1 commands are merged/available (`grep -n identity_list src-tauri/src/lib.rs`), and starts at Task 6. The frontend has zero dependency on un-pushed Session-1 work — Session 2 invokes the commands by name through `@tauri-apps/api/core`.

---

## SESSION 2 — React ribbon switcher + inline unlock + mailbox filter + listener badges

> Mirror the established **inline-edit-in-the-ribbon** pattern (`GridEdit.tsx`): a closed display surface that swaps to an inline editing surface on click, Esc-cancels, NEVER opens a popup window (project rule: inline UI, no window clutter). The closed switcher chip MUST be pixel-identical to today's `.dash-callsign-row` (callsign text + SSID `<select>`), so the top bar's footprint is unchanged when closed.

### Task 6 — TS DTO mirror + `useIdentities` hooks

**Files:**
- `src/shell/identityTypes.ts` (NEW)
- `src/shell/useIdentities.ts` (NEW)
- `src/shell/useIdentities.test.ts` (NEW)

TS mirror (snake_case fields to match the Rust wire DTOs — verified against the `ConfigViewDto` convention that the TS mirror uses the SAME snake_case the Rust `#[derive(Serialize)]` emits):

```ts
// src/shell/identityTypes.ts
export type TacticalCmsBadge = { kind: 'Unknown' } | { kind: 'Registered' } | { kind: 'NotRegistered' };
export interface TacticalRowDto { label: string; parent: string; cms: TacticalCmsBadge; }
export interface IdentityRowDto {
  callsign: string; label: string | null; has_cms_account: boolean;
  cms_registered: boolean; needs_auth: boolean; tacticals: TacticalRowDto[];
}
export interface IdentityListDto { identities: IdentityRowDto[]; last_selected: string | null; }
export interface ActiveIdentityDto { mycall: string; address_as: string; is_tactical: boolean; }
```

**TDD steps:**

- [ ] Write `useIdentities.test.ts` with a mocked `@tauri-apps/api/core` `invoke` (follow the mock pattern in `useMailbox.test.ts` / `usePacketConfig.test.tsx`). Test `useIdentityList` calls `invoke('identity_list')` and exposes `data.identities`. RED first (hooks don't exist).
- [ ] Implement `useIdentities.ts`: `useIdentityList()` → `useQuery({ queryKey: ['identity_list'], queryFn: () => invoke<IdentityListDto>('identity_list') })`; `useActiveIdentity()` → `useQuery({ queryKey: ['identity_active'], queryFn: () => invoke<ActiveIdentityDto | null>('identity_active') })`; `useIdentitySwitch()` → `useMutation({ mutationFn: ({address, credential}) => invoke('identity_switch', { address, credential }), onSuccess: () => { qc.invalidateQueries(['identity_list']); qc.invalidateQueries(['identity_active']); } })`.
- [ ] Test `useIdentitySwitch` invalidates both queries on success and surfaces the `UiError` (parsed via a `parseIdentityError` reusing the mailbox `parseUiError` shape) on reject.
- [ ] Run `npx vitest run src/shell/useIdentities.test.ts` → green. **REAP**: `pkill -f vitest; pgrep -f vitest` (must be empty). `npx tsc --noEmit` → exit 0.
- [ ] **Commit** `feat(identity): TS DTO mirror + useIdentities react-query hooks` ending `Agent: sandbar-raven-fox`.

### Task 7 — `IdentitySwitcher` closed chip (footprint-unchanged)

**Files:**
- `src/shell/IdentitySwitcher.tsx` (NEW)
- `src/shell/IdentitySwitcher.test.tsx` (NEW)
- `src/shell/IdentitySwitcher.css` (NEW)

The component renders, when closed, exactly what `.dash-callsign-row` renders today: the bare callsign text chip (`data-testid="ribbon-callsign-text"`) + the SSID `<select>` (`data-testid="ribbon-ssid-select"`), wrapped in the existing `.dash-callsign-row` (`data-testid="ribbon-callsign"`). The callsign text becomes a `<button data-testid="identity-switcher-trigger">` that opens the dropdown on click. The SSID select keeps its own click behavior (it must NOT open the identity dropdown — `e.stopPropagation()` on the select, or render it as a sibling of the trigger button so its clicks never bubble to the trigger).

Props:
```ts
export interface IdentitySwitcherProps {
  active: ActiveIdentityDto | null;     // closed-chip label source (mycall); null pre-auth → render the config-callsign fallback
  list: IdentityListDto | null;         // dropdown contents; null while loading → dropdown shows a spinner row
  ssid?: number;
  onSsidChange?: (n: number) => void;
  onSwitch: (address: string, credential: string) => Promise<void>;  // throws UiError on reject
}
```

**TDD steps:**

- [ ] Write `IdentitySwitcher.test.tsx` `closed_chip_matches_ribbon_callsign`: render with `active = { mycall: 'W1ABC', address_as: 'W1ABC', is_tactical: false }`, `ssid=3`, `onSsidChange` provided; assert `getByTestId('ribbon-callsign')` exists, `ribbon-callsign-text` shows `W1ABC`, `ribbon-ssid-select` shows `-3`, and the dropdown list (`identity-switcher-list`) is NOT in the document (closed by default). RED first.
- [ ] Implement the closed render: copy the `.dash-callsign-row` markup from `DashboardRibbon.tsx` (lines 134–173) verbatim, but make the callsign text a `<button className="dash-callsign-text" data-testid="identity-switcher-trigger">`. Keep the no-`onSsidChange` fallback (`<span className="dash-callsign-text">`).
- [ ] Test `tactical_active_shows_address_as_with_parent_subscript`: `active = { mycall: 'W1ABC', address_as: 'EOC-3', is_tactical: true }` → the closed chip shows `EOC-3` as the primary label with `W1ABC` as a small "as" parent indicator (`data-testid="identity-active-parent"`), because the operator must see they're operating as a tactical riding under W1ABC. (Part-97 mycall is still W1ABC; surface both.)
- [ ] Test `null_active_renders_config_fallback`: `active = null` → render the plain callsign text from `ssid`-less fallback (pre-auth launch shows the last-selected hint or em-dash, never a stale authenticated call).
- [ ] Add `IdentitySwitcher.css` closed-chip rules (or confirm the existing `.dash-callsign-row` rules in `AppShell.css` cover them — the trigger button must reset `appearance`/`background`/`border` so it reads as text, not a button, matching the SSID-select de-buttoning at AppShell.css:219–236).
- [ ] Run `npx vitest run src/shell/IdentitySwitcher.test.tsx` → green. **REAP** `pkill -f vitest`. `npx tsc --noEmit` → exit 0.
- [ ] **Commit** `feat(identity): IdentitySwitcher closed chip (footprint unchanged)` ending `Agent: sandbar-raven-fox`.

### Task 8 — Open dropdown: FULL rows, nested tacticals, lock glyph, CMS badge

**Files:**
- `src/shell/IdentitySwitcher.tsx` (extend)
- `src/shell/IdentitySwitcher.test.tsx` (extend)
- `src/shell/IdentitySwitcher.css` (extend — dropdown panel, anchored to the chip)

One click on the trigger opens `identity-switcher-list`, a dropdown **anchored to** `.dash-callsign-row` (absolute-positioned below the chip; the chip is the positioning context, like `GridEdit`'s `dash-grid-edit-container`). The list renders each FULL identity as a row, with its tacticals nested directly beneath (indented). A FULL row with `needs_auth` shows a lock glyph (`🔒` wrapped `aria-hidden`, with an `aria-label="locked"` on the row); a tactical row shows a CMS badge from `cms.kind`: `Registered` → `✓ CMS` (`data-testid="cms-badge-ok"`), `NotRegistered` → `⊘ CMS` (`data-testid="cms-badge-blocked"`), `Unknown` → `? CMS` (`data-testid="cms-badge-unknown"`, treated as blocked/fail-closed). The `last_selected` row gets `aria-current="true"`.

**TDD steps:**

- [ ] Write `opening_lists_full_with_nested_tacticals`: a `list` with FULL `W1ABC` (tacticals `[EOC-3 Registered]`) + FULL `W7XYZ` (needs_auth, no tacticals); click `identity-switcher-trigger`; assert `identity-switcher-list` appears, contains a `W1ABC` row and an indented `EOC-3` row (assert DOM nesting/order: `EOC-3` follows `W1ABC` and precedes `W7XYZ`), and a `W7XYZ` row. RED first.
- [ ] Implement the open state with a `useState(false)` `open` toggle (mirror `GridEdit`'s `editing` state). Render the anchored `<div className="identity-switcher-list" role="listbox" data-testid="identity-switcher-list">` only when `open`. Esc closes it (keydown handler like `GridEdit.handleKeyDown`); a click outside closes it (a `mousedown` document listener added on open, mirror any existing overlay-dismiss in the codebase or use a simple `onBlur` within a `tabIndex` container).
- [ ] Test `locked_full_shows_lock_glyph`: `W7XYZ` row has `aria-label` containing "locked" and renders the lock glyph element; `W1ABC` (the active, `needs_auth=false`) does NOT.
- [ ] Test `tactical_cms_badges_render`: a Registered tactical shows `cms-badge-ok`; a NotRegistered tactical shows `cms-badge-blocked`; an Unknown tactical shows `cms-badge-unknown`. Assert the blocked/unknown badges carry a `title` explaining CMS modes are unavailable for that tactical until registration is verified.
- [ ] Test `last_selected_row_marked_current`: `list.last_selected = 'W7XYZ'` → that row has `aria-current="true"`.
- [ ] Test `selecting_full_without_auth_switches_immediately`: click the active `W1ABC` row (or any row with `needs_auth=false`) → calls `onSwitch('W1ABC', '')` (no credential needed when re-selecting the already-authenticated active) OR closes if it's already active — assert the no-auth path does not reveal the unlock field.
- [ ] Add `IdentitySwitcher.css` for `.identity-switcher-list` (absolute, anchored, `z-index` above the ribbon, dark surface matching `.dash-callsign-select option` background), `.identity-row`, `.identity-row--tactical` (indent), `.identity-lock`, `.cms-badge` variants.
- [ ] Run `npx vitest run src/shell/IdentitySwitcher.test.tsx` → green. **REAP** `pkill -f vitest`. `npx tsc --noEmit` → exit 0.
- [ ] **Commit** `feat(identity): switcher dropdown — nested tacticals, lock glyph, CMS badges` ending `Agent: sandbar-raven-fox`.

### Task 9 — Inline unlock field within the open dropdown

**Files:**
- `src/shell/IdentitySwitcher.tsx` (extend)
- `src/shell/IdentitySwitcher.test.tsx` (extend)
- `src/shell/IdentitySwitcher.css` (extend)

Selecting a FULL (or tactical) row that needs auth does NOT switch immediately — it reveals an inline unlock field WITHIN the same open dropdown (no popup): a `<label>Unlock W7XYZ</label>` + `<input type="password" data-testid="identity-unlock-input">` + `<button data-testid="identity-unlock-submit">Unlock</button>`. Enter submits; Esc cancels back to the list. On submit, call `onSwitch(address, credential)`. On reject (`UiError`), show an inline error (`data-testid="identity-unlock-error"` `role="alert"`) and KEEP the field open so the operator retries (mirror `GridEdit.finishEdit`'s catch-and-stay-in-edit). On success, the dropdown closes and the parent invalidates the active-identity query (chip updates).

> This is access-control credential entry, distinct from the removed TX-consent modal — RADIO-1 governs transmission, not identity switching. Do NOT add a consent modal here.

**TDD steps:**

- [ ] Write `selecting_locked_full_reveals_inline_unlock`: open the dropdown, click the `W7XYZ` row (`needs_auth`); assert `identity-unlock-input` appears WITHIN `identity-switcher-list` (assert it is a descendant of the list node — no separate window/portal-to-body) and is labeled `Unlock W7XYZ`. RED first.
- [ ] Implement: a `useState<string | null>(null)` `unlockingAddress`; clicking a needs-auth row sets it; the row's slot renders the inline form when `unlockingAddress === row.address`. `onSwitch` is awaited; success clears `unlockingAddress` + `open`.
- [ ] Test `unlock_submit_calls_onSwitch_with_credential`: type `hunter2`, click Unlock → `onSwitch` called with `('W7XYZ', 'hunter2')`. Enter-key submits identically.
- [ ] Test `unlock_reject_shows_error_and_stays_open`: `onSwitch` rejects with a `UiError` `Rejected` → `identity-unlock-error` shows the message, the input is still present, the dropdown is still open (operator retries). Assert the password value is NOT cleared on the visible DOM by an error (the operator can correct a typo) — but is held only in component state, never in the list DTO or a query cache.
- [ ] Test `unlock_esc_cancels_back_to_list`: Esc in the unlock input clears `unlockingAddress` (back to the row list) without closing the whole dropdown.
- [ ] Test `unlock_success_closes_and_invalidates`: `onSwitch` resolves → `open` is false and `identity-switcher-list` is gone (the parent's `onSwitch` is `useIdentitySwitch().mutateAsync`, which invalidates `identity_active`; assert via a spy that `onSwitch` resolved and the component closed).
- [ ] Add `IdentitySwitcher.css` for the inline unlock row (`.identity-unlock`, the input + submit, the error). No `position: fixed`, no portal — it lives in the dropdown flow.
- [ ] Run `npx vitest run src/shell/IdentitySwitcher.test.tsx` → green. **REAP** `pkill -f vitest`. `npx tsc --noEmit` → exit 0.
- [ ] **Commit** `feat(identity): inline unlock field within the switcher dropdown` ending `Agent: sandbar-raven-fox`.

### Task 10 — Wire `IdentitySwitcher` into `DashboardRibbon` + `AppShell`

**Files:**
- `src/shell/DashboardRibbon.tsx` (replace the `.dash-callsign-row` inner block, lines 134–173)
- `src/shell/DashboardRibbon.test.tsx` (extend)
- `src/shell/AppShell.tsx` (wire the hooks)
- `src/shell/AppShell.css` (closed-chip parity, lines 207–259)

**TDD steps:**

- [ ] Extend `DashboardRibbon.test.tsx`: with new optional props `identities` / `activeIdentity` / `onSwitchIdentity` provided, the ribbon renders `<IdentitySwitcher>` in place of the bare chip; WITHOUT them (prop-free legacy consumers + existing tests), the ribbon renders the current bare `.dash-callsign-row` exactly as before (back-compat — the existing ribbon tests must still pass unchanged). RED for the new branch first.
- [ ] Implement: add `identities?: IdentityListDto | null`, `activeIdentity?: ActiveIdentityDto | null`, `onSwitchIdentity?: (address: string, credential: string) => Promise<void>` to `DashboardRibbonProps`. In the `.dash-callsign-row` slot, render `<IdentitySwitcher>` when `onSwitchIdentity` is provided; else keep the existing inline markup (the fallback branch). Thread `ssid` + `onSsidChange` through to the switcher.
- [ ] Run the FULL existing `DashboardRibbon.test.tsx` to prove no regression: `npx vitest run src/shell/DashboardRibbon.test.tsx` → green. **REAP**.
- [ ] Wire `AppShell.tsx`: call `useIdentityList()` + `useActiveIdentity()` + `useIdentitySwitch()`; pass `identities={list.data}` `activeIdentity={active.data}` `onSwitchIdentity={(a,c) => switchMut.mutateAsync({address:a, credential:c})}` into `DashboardRibbon`.
- [ ] Confirm `AppShell.css` closed-chip rules give the switcher chip identical metrics to today (no layout shift): the trigger `<button>` must reset to text appearance (`background:transparent; border:0; padding:0; font: inherit; color: var(--accent-2)`). Add a `.dash-callsign-text` button-reset rule if needed.
- [ ] Run `npx vitest run src/shell/DashboardRibbon.test.tsx src/shell/AppShell.test.tsx` → green. **REAP**. `npx tsc --noEmit` → exit 0.
- [ ] **Commit** `feat(identity): mount IdentitySwitcher in the dashboard ribbon` ending `Agent: sandbar-raven-fox`.

### Task 11 — Mailbox identity filter

**Files:**
- `src/mailbox/types.ts` (add `identity?: string` to `MessageMeta` + the matching Rust `MessageMetaDto` field in `ui_commands.rs` if not already present from Phase 4)
- `src/mailbox/identityFilter.ts` (NEW — pure predicate)
- `src/mailbox/identityFilter.test.ts` (NEW)
- `src/mailbox/MessageList.tsx` (toolbar control beside `MessageListSortControl` ~line 440; filter `sortedMessages`)
- `src/mailbox/MessageList.test.tsx` (extend)

The filter is "All identities" | one entry per FULL identity | one per tactical. `messageMatchesIdentity(msg, filter)` returns true when `filter === null` (All) or `msg.identity === filter`. Messages with no `identity` tag match only "All".

> Phase 4 added the per-identity tag on the message store + the `MessageMetaDto.identity` field. CONFIRM with `grep -n "identity" src-tauri/src/ui_commands.rs src/mailbox/types.ts` — if Phase 4 already added the TS field, skip that edit; only add the filter UI.

**TDD steps:**

- [ ] Write `identityFilter.test.ts`: `messageMatchesIdentity({identity:'W1ABC'}, null) === true`; `(…'W1ABC', 'W1ABC') === true`; `(…'W1ABC', 'W7XYZ') === false`; `({identity: undefined}, 'W1ABC') === false`; `({identity: undefined}, null) === true`. RED first.
- [ ] Implement `identityFilter.ts`: the predicate + `deriveIdentityFilterOptions(list: IdentityListDto)` → `[{ value: null, label: 'All identities' }, ...full callsigns, ...tactical labels]`.
- [ ] Run `npx vitest run src/mailbox/identityFilter.test.ts` → green. **REAP**.
- [ ] Add `identity?: string` to `MessageMeta` in `types.ts` (if not present from Phase 4) and to the dev fixture so the filter has data to exercise.
- [ ] Extend `MessageList.test.tsx`: render with an `identityFilter` prop + `onIdentityFilterChange` and a mixed-identity message set; selecting `W7XYZ` in the filter control hides `W1ABC` rows. RED first.
- [ ] Implement: add `identityFilter?: string | null` + `onIdentityFilterChange?` + `identityFilterOptions?` to `MessageListProps`; render an identity-filter `<select data-testid="mailbox-identity-filter">` in the toolbar beside `MessageListSortControl` (line ~440), shown only when `identityFilterOptions` is provided; filter `sortedMessages` through `messageMatchesIdentity` before passing to Virtuoso.
- [ ] Wire `AppShell.tsx`: hold the filter state, derive options from `useIdentityList`, pass into `MessageList`.
- [ ] Run `npx vitest run src/mailbox/identityFilter.test.ts src/mailbox/MessageList.test.tsx` → green. **REAP**. `npx tsc --noEmit` → exit 0.
- [ ] **Commit** `feat(identity): mailbox identity filter` ending `Agent: sandbar-raven-fox`.

### Task 12 — Listener identity badges in the connections/radio panel

**Files:**
- `src/radio/sections/useListenerState.ts` (surface the bound identity captured at arm time)
- `src/radio/sections/ListenArmButton.tsx` (render the badge) + `ListenSection.css`
- a co-located `*.test.tsx` (extend or new `ListenArmButton.test.tsx`)

Per spec + master-plan resolved decision 4: each armed listener shows the identity it answers as (the `mycall` / tactical label captured at arm time). Switching the active identity must NOT mutate the badge of an already-armed listener (Phase 6 enforces the backend invariant; the UI just renders the listener's own bound identity, never the global active one).

> The listener's bound identity comes from the listener-state surface Phase 6 added (the armed listener captured its `SessionIdentity`). CONFIRM the field name with `grep -rn "identity\|mycall\|answering\|bound" src/radio/sections/useListenerState.ts` and the Phase-6 listener DTO. If Phase 6 exposed it as e.g. `boundIdentity` / `answeringAs`, use that exact name; this plan calls it `answeringAs` pending that grep.

**TDD steps:**

- [ ] `grep -rn "identity\|mycall\|answeringAs\|boundIdentity" src/radio/ src-tauri/src/winlink_backend.rs` to find the Phase-6 listener-identity field name. Record it inline.
- [ ] Write a listener-badge test: an armed listener with `answeringAs = 'EOC-3'` renders a badge `data-testid="listener-identity-badge"` showing `EOC-3`; a listener bound to a FULL `W1ABC` shows `W1ABC`. RED first.
- [ ] Implement: render `<span className="listener-identity-badge" data-testid="listener-identity-badge">` in `ListenArmButton.tsx` (or the listener row) when the listener is armed and carries a bound identity. Style it in `ListenSection.css` (small monospace pill, matching the dashboard's badge idiom).
- [ ] Write `active_switch_does_not_change_armed_badge`: render with an armed listener `answeringAs='W1ABC'` and a DIFFERENT active identity prop `W7XYZ`; assert the badge still shows `W1ABC` (the badge reads the listener's own bound identity, not the active one).
- [ ] Run `npx vitest run src/radio/sections/ListenArmButton.test.tsx` (or the co-located test) → green. **REAP**. `npx tsc --noEmit` → exit 0.
- [ ] **Commit** `feat(identity): listener identity badges in the radio panel` ending `Agent: sandbar-raven-fox`.

### Session-2 gate (before push)

- [ ] `npx tsc --noEmit` → exit 0.
- [ ] Full frontend sweep (scoped runs miss far-away contract/snapshot tests): `npx vitest run` → all green. **REAP IMMEDIATELY**: `pkill -f vitest; pgrep -f vitest` (must be empty; the full sweep spawns the most workers).
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → exit 0 (Task 11 may have touched `ui_commands.rs` for the `MessageMetaDto.identity` field).
- [ ] **WebKitGTK / grim smoke (post-merge, NOT a merge gate):** launch a fresh `tauri dev`, open the dashboard, click the callsign chip → confirm the dropdown anchors under the chip (not clipped at the ribbon edge), the nested tacticals indent, the lock glyph + CMS badges render, and selecting a locked FULL reveals the inline unlock WITHIN the dropdown (no separate window). Capture via `grim` (NOT Chromium — Chromium clips what WebKit fits). This validates anchoring/clipping that jsdom cannot. Mind the :1420 strictPort single-instance rule — only one worktree's `tauri dev` runs machine-wide. File any visual issue as a fast-follow bd issue and fix-forward; do not block the merge on it.
- [ ] Push the Session-2 commits.

---

## Self-review

- **Spec coverage:** (a) command surface → Tasks 1–5 (all six canonical command names: `identity_list`, `identity_add_full`, `identity_add_tactical`, `identity_remove`, `identity_switch`, `identity_active`, with `needs_auth` + CMS-badge flags and NO secrets in any DTO — asserted by `identity_list_dto_serializes_without_secrets` + `add_full_persists_identity_not_secret`). (b) switcher anchored to `.dash-callsign-row`/`ribbon-callsign`, one-click open, nested tacticals, lock glyph, per-tactical CMS badge, inline unlock within the dropdown, closed-footprint unchanged → Tasks 6–10. (c) mailbox identity filter → Task 11. (d) listener identity badges → Task 12. Every Phase-7-scope item (a)–(d) is assigned.
- **Canonical names verbatim:** `IdentityStore`, `FullIdentity`, `TacticalIdentity`, `TacticalCmsState`, `SessionIdentity`, `IdentityService`, `IdentityError`, `Address`, `Callsign`, and the six command names match the master-plan contract exactly. `SessionIdentity::mycall()` (Part-97 station ID) is surfaced distinctly from `address_as()` in `ActiveIdentityDto` — the spec's "biggest risk" (conflating principal + mail address) is kept separated on the wire.
- **No-secrets invariant:** secrets only ever cross `identity_add_full` (in) and `identity_switch` (in) as inbound credential strings; no DTO returns one; `IdentityHandle` is non-`Serialize` by Phase-1 construction so it cannot appear in a command return; a serialization test fences the list DTO. The credential string in `identity_switch` is dropped at scope end, never logged, never put in an error message.
- **Inline-unlock = no popup:** Task 9 asserts the unlock input is a DESCENDANT of the dropdown list node (no portal-to-body, no `position:fixed` window) — mirrors `GridEdit`'s in-flow inline-edit and honors the inline-UI / no-window-clutter rule. It is access-control entry, explicitly NOT a RADIO-1 TX-consent modal.
- **Footprint unchanged:** Task 7 + Task 10 assert the closed chip is the existing `.dash-callsign-row` markup (callsign text + SSID select), and the SSID select keeps its independent click. Legacy prop-free `DashboardRibbon` consumers/tests keep the bare-chip fallback (back-compat asserted).
- **Session break placed correctly:** the `## SESSION BREAK` marker sits after Tasks 1–5 (commands + DTOs + tests green + registered + pushed) and before the React work (Tasks 6–12), per the LARGE-phase split directive. Session 2 has zero dependency on un-pushed Session-1 state (it invokes commands by name).
- **Dependency confirmations, not assumptions:** Tasks 2, 11, 12 each begin with a `grep` to confirm whether the managed active-session state (Phase 3/6), the `MessageMetaDto.identity` tag (Phase 4), and the listener bound-identity field (Phase 6) already exist — avoiding duplicate `.manage` (Tauri panics) or a duplicated field.
- **Gates honored:** every Rust task ends with `cargo clippy --all-targets -D warnings`; every frontend task runs scoped vitest + REAP + `tsc --noEmit`; both session gates run the broad sweep that scoped runs miss; WebKitGTK/grim smoke is opportunistic/post-merge, not a merge gate (CI both arches gates the merge).
- **Risks / watch:** (1) the dropdown anchoring + clipping at the ribbon's right edge is the most likely jsdom-invisible bug — the grim smoke specifically targets it. (2) If Phase 6 did not expose a listener bound-identity field, Task 12's grep surfaces it as blocked → file a bd issue against Phase 6 rather than inventing a field. (3) `AppIdentityState` double-`.manage` is the sharpest backend footgun — Task 2's grep + Task 5's conditional guard against it.
