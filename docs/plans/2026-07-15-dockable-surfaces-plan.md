# Dockable Surfaces Implementation Plan (Routines plan 6/6)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pop the Routines, Tac Map, and APRS Chat surfaces into their own OS windows and dock them back, per the adversarially-hardened spec.

**Architecture:** A backend-owned `DockRegistry` (config-persisted, event-broadcast) is the single source of truth for where each surface lives; a generic `PoppedSurfaceHost` route renders any surface in a secondary webview; AppShell renders visual-pathway affordances for popped surfaces. A per-surface continuity token travels with every transition.

**Tech Stack:** Tauri 2.x (Rust backend), React 18 + TypeScript (Vite), WebKitGTK, vitest, cargo test.

**Canonical spec:** `docs/superpowers/specs/2026-07-15-dockable-surfaces-design.md` (commit `af062a83` or later). Section references below (`spec §N`) mean THAT document; `parent §12` means `docs/superpowers/specs/2026-07-13-routines-design.md` §12. **Read the spec section named in your task before starting the task.**

## Global Constraints

- **TDD preamble (EVERY task):** BEFORE starting work: (1) read `.claude/skills/test-driven-development/` or invoke /test-driven-development if available, else follow strict red-green; (2) read `docs/pitfalls/testing-pitfalls.md`. Write failing test → implement → verify green.
- **Completion check (EVERY task):** BEFORE marking complete: (1) review your tests against `docs/pitfalls/testing-pitfalls.md`; (2) verify error paths + edge cases are tested; (3) run the task's test commands and confirm green.
- **Review loop (after every logical group, marked below):** review the batch from multiple perspectives, minimum three rounds; if round 3 still finds substantive issues, continue until clean. Then update your private journal and continue.
- **This Pi does not finish cold cargo builds.** Write Rust + tests; verify frontend locally (`pnpm vitest run <file>`, `pnpm typecheck`); Rust compile/test verification happens on the PR's CI (both arches). Do NOT run `cargo build`/`cargo test` locally. Clippy traps to avoid (CI runs `--all-targets -D warnings`, MSRV 1.75): no `Result::inspect_err` (1.76+), no `format!` in `expect()`, derive instead of manual impls where possible, no unused imports.
- **Commits:** conventional type + scope, trailer block (replace moniker with the session's):

  ```
  Agent: <session-moniker>
  Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
  ```
- **Naming:** the word "workflow" NEVER appears in UI text, code symbols, or schema keys (parent spec naming rule). The feature vocabulary is pop out / dock back / surface.
- **Wire contract is law (spec §3 table):** Rust `SurfaceId::{Routines,TacMap,AprsChat}` ⇄ wire `"routines" | "tac_map" | "aprs_chat"` ⇄ labels `pop-routines | pop-tacmap | pop-aprschat` ⇄ routes `/pop/routines | /pop/tacmap | /pop/aprschat` ⇄ titles `Routines — Tuxlink | Tac Map — Tuxlink | APRS Chat — Tuxlink`. The label/route forms drop the underscore — copy from this table, never derive.
- **Do NOT:** add features beyond the spec (no always-on-top, no focus-flash, no drag-to-dock); relitigate spec decisions; use `close()` on pop windows (always `destroy()`); clone capability files from `help.json`/`logging.json`; add a dirty-guard prompt to the designer (spec §7 dropped it deliberately).
- **Sequencing:** Tasks 8–10 MUST run sequentially — all three edit `src/shell/AppShell.tsx` and `src/dock/surfaceRegistry.tsx`. Task 5 may run in parallel with Task 3. Nothing else in this plan is parallel-safe.
- **vitest invoke-mock teardown (applies to EVERY task writing invoke-mocked tests):** the harness calls invoke mocks with NO arguments at cleanup — mock implementations must tolerate `undefined` cmd. House mock pattern to copy: `src/aprs/useEnvStations.test.ts:11-39` (there is NO shared `setupTauriMocks` helper; each file rolls the `vi.mock` pair).
- **Main-side dock-back token rule (Tasks 8–10):** a main-window ⇤ action always invokes `surface_dock_back(surface, { foreground: true, state: null })`. Main cannot supply the popped window's live state (the backend `destroy()` path never runs a close-intent round-trip for command-initiated dock-back); state loss on main-side dock-back is accepted — Routines falls back to the dashboard. The `{ foreground, state }` envelope is ALWAYS present on every dock-back context, from either window.

## File structure (created files)

| File | Responsibility |
|---|---|
| `src-tauri/src/dock/mod.rs` | Module root: re-exports, SurfaceId/DockMode/DockSnapshot types, wire-contract helpers |
| `src-tauri/src/dock/registry.rs` | DockRegistry state + pure transition core + persist/emit orchestration |
| `src-tauri/src/dock/commands.rs` | Tauri commands (`surface_pop_out`, `surface_dock_back`, `surface_focus`, `dock_state_get`, `shell_mounted`) + close-intent + crash wiring + consent host resolution + park notification |
| `src-tauri/src/secondary_window.rs` | Shared `open_secondary_window` helper + `ClosePolicy` (factored from the 4 existing `*_window.rs`) |
| `src-tauri/capabilities/pop-routines.json`, `pop-tacmap.json`, `pop-aprschat.json` | Per-label capability files (fresh, NOT cloned from help/logging) |
| `src/dock/dockState.ts` | TS wire-type mirror, `useDockState()` (listen-first), `consentHostWindow`, invoke wrappers |
| `src/dock/PoppedSurfaceHost.tsx` + `PopTitleBar.tsx` + `surfaceRegistry.tsx` + `strips.tsx` | The popped-window shell |
| `src/dock/dock-wire-fixture.json` + `src/dock/dockParity.test.ts` | Cross-language wire-shape parity fixture (spec §10, k61j class); Task 6 also adds the Rust-side fixture assertion to `src-tauri/src/dock/mod.rs` tests |
| `dev/measure-webview-marginal-memory.py` | Recreated memory harness (tracked this time — spec §10) |

---

### Task 1: Rust dock core — types, wire contract, pure transition

**Files:**
- Create: `src-tauri/src/dock/mod.rs`, `src-tauri/src/dock/registry.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod dock;` beside the other module declarations near the top)
- Test: inline `#[cfg(test)]` in both new files

**Interfaces (Produces — later tasks rely on these exact names):**
```rust
pub enum SurfaceId { Routines, TacMap, AprsChat }        // serde: "routines"|"tac_map"|"aprs_chat"
pub enum DockMode { Docked, Popped }                      // serde: "docked"|"popped"
impl SurfaceId {
    pub fn window_label(self) -> &'static str;            // "pop-routines"|"pop-tacmap"|"pop-aprschat"
    pub fn route(self) -> &'static str;                    // "/pop/routines"|"/pop/tacmap"|"/pop/aprschat"
    pub fn title(self) -> &'static str;                    // "Routines — Tuxlink"|"Tac Map — Tuxlink"|"APRS Chat — Tuxlink"
    pub fn from_window_label(label: &str) -> Option<SurfaceId>;
    pub const ALL: [SurfaceId; 3];
}
pub struct DockSurfaces { pub routines: DockMode, pub tac_map: DockMode, pub aprs_chat: DockMode }  // Default = all Docked
impl DockSurfaces { pub fn get(&self, s: SurfaceId) -> DockMode; pub fn set(&mut self, s: SurfaceId, m: DockMode); }
pub struct DockSnapshot { pub surfaces: DockSurfaces, pub context: DockContext }
pub struct DockContext { pub routines: Option<serde_json::Value>, pub tac_map: Option<serde_json::Value>, pub aprs_chat: Option<serde_json::Value> }
pub fn apply_transition(surfaces: &mut DockSurfaces, surface: SurfaceId, target: DockMode) -> bool  // true iff effective (state changed)
pub fn consent_host_window(routines_mode: DockMode) -> &'static str  // "main" | "pop-routines"
```

- [ ] **Step 1: Write the failing tests** (`src-tauri/src/dock/mod.rs` bottom `#[cfg(test)] mod tests`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Wire contract, spec §3 table. Shape test per the standing serde-enum rule:
    /// explicit rename + assert the exact wire strings — the label/route forms
    /// drop the underscore and CANNOT be derived.
    #[test]
    fn surface_id_wire_contract() {
        assert_eq!(serde_json::to_string(&SurfaceId::Routines).unwrap(), "\"routines\"");
        assert_eq!(serde_json::to_string(&SurfaceId::TacMap).unwrap(), "\"tac_map\"");
        assert_eq!(serde_json::to_string(&SurfaceId::AprsChat).unwrap(), "\"aprs_chat\"");
        assert_eq!(SurfaceId::TacMap.window_label(), "pop-tacmap");
        assert_eq!(SurfaceId::TacMap.route(), "/pop/tacmap");
        assert_eq!(SurfaceId::AprsChat.window_label(), "pop-aprschat");
        assert_eq!(SurfaceId::Routines.title(), "Routines — Tuxlink");
        for s in SurfaceId::ALL {
            assert_eq!(SurfaceId::from_window_label(s.window_label()), Some(s));
        }
        assert_eq!(SurfaceId::from_window_label("main"), None);
        assert_eq!(serde_json::to_string(&DockMode::Popped).unwrap(), "\"popped\"");
        // Round-trip: the TS side sends these exact strings as invoke args.
        assert_eq!(serde_json::from_str::<SurfaceId>("\"tac_map\"").unwrap(), SurfaceId::TacMap);
    }

    /// DockSnapshot JSON shape — the dock:changed payload / dock_state_get return
    /// (spec §3 JSON literal). Full snapshot, never deltas.
    #[test]
    fn snapshot_json_shape() {
        let mut snap = DockSnapshot::default();
        snap.surfaces.set(SurfaceId::Routines, DockMode::Popped);
        snap.context.routines = Some(serde_json::json!({"view": "designer"}));
        let v: serde_json::Value = serde_json::to_value(&snap).unwrap();
        assert_eq!(v["surfaces"]["routines"], "popped");
        assert_eq!(v["surfaces"]["tac_map"], "docked");
        assert_eq!(v["surfaces"]["aprs_chat"], "docked");
        assert_eq!(v["context"]["routines"]["view"], "designer");
        assert!(v["context"]["tac_map"].is_null());
    }

    /// Transition core (spec §3): effective vs no-op. No-op MUST return false so
    /// callers suppress persist+emit (double dock-back safety).
    #[test]
    fn transition_effectiveness() {
        let mut s = DockSurfaces::default();
        assert!(apply_transition(&mut s, SurfaceId::TacMap, DockMode::Popped));
        assert_eq!(s.get(SurfaceId::TacMap), DockMode::Popped);
        assert!(!apply_transition(&mut s, SurfaceId::TacMap, DockMode::Popped)); // no-op
        assert!(apply_transition(&mut s, SurfaceId::TacMap, DockMode::Docked));
        assert!(!apply_transition(&mut s, SurfaceId::TacMap, DockMode::Docked)); // double dock-back
        // Other surfaces untouched throughout.
        assert_eq!(s.get(SurfaceId::Routines), DockMode::Docked);
    }

    /// Consent host resolution (spec §6) — Rust is canonical; TS mirrors via the
    /// parity fixture (Task 6).
    #[test]
    fn consent_host_resolution() {
        assert_eq!(consent_host_window(DockMode::Docked), "main");
        assert_eq!(consent_host_window(DockMode::Popped), "pop-routines");
    }
}
```

- [ ] **Step 2: Verify the tests fail to compile** — the types don't exist yet. (No local cargo: eyeball that every asserted symbol is currently absent — `grep -rn "SurfaceId" src-tauri/src/` returns nothing.)

- [ ] **Step 3: Implement `src-tauri/src/dock/mod.rs`**

```rust
//! Dockable surfaces — shell capability (Routines plan 6/6, bd tuxlink-dmwte).
//! Spec: docs/superpowers/specs/2026-07-15-dockable-surfaces-design.md §3.
//! The wire-contract table in spec §3 is NORMATIVE; the strings below are
//! copied from it, never derived (label/route drop the underscore).

pub mod registry;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceId {
    Routines,
    TacMap,
    AprsChat,
}

impl SurfaceId {
    pub const ALL: [SurfaceId; 3] = [SurfaceId::Routines, SurfaceId::TacMap, SurfaceId::AprsChat];

    pub fn window_label(self) -> &'static str {
        match self {
            SurfaceId::Routines => "pop-routines",
            SurfaceId::TacMap => "pop-tacmap",
            SurfaceId::AprsChat => "pop-aprschat",
        }
    }

    pub fn route(self) -> &'static str {
        match self {
            SurfaceId::Routines => "/pop/routines",
            SurfaceId::TacMap => "/pop/tacmap",
            SurfaceId::AprsChat => "/pop/aprschat",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            SurfaceId::Routines => "Routines — Tuxlink",
            SurfaceId::TacMap => "Tac Map — Tuxlink",
            SurfaceId::AprsChat => "APRS Chat — Tuxlink",
        }
    }

    pub fn from_window_label(label: &str) -> Option<SurfaceId> {
        SurfaceId::ALL.into_iter().find(|s| s.window_label() == label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DockMode {
    #[default]
    Docked,
    Popped,
}

/// The persisted half of the snapshot — this exact shape is the config `dock`
/// section (spec §3 JSON literal; Task 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DockSurfaces {
    #[serde(default)]
    pub routines: DockMode,
    #[serde(default)]
    pub tac_map: DockMode,
    #[serde(default)]
    pub aprs_chat: DockMode,
}

impl DockSurfaces {
    pub fn get(&self, s: SurfaceId) -> DockMode {
        match s {
            SurfaceId::Routines => self.routines,
            SurfaceId::TacMap => self.tac_map,
            SurfaceId::AprsChat => self.aprs_chat,
        }
    }
    pub fn set(&mut self, s: SurfaceId, m: DockMode) {
        match s {
            SurfaceId::Routines => self.routines = m,
            SurfaceId::TacMap => self.tac_map = m,
            SurfaceId::AprsChat => self.aprs_chat = m,
        }
    }
}

/// Runtime-only continuity tokens (spec §7) — never persisted to config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DockContext {
    pub routines: Option<serde_json::Value>,
    pub tac_map: Option<serde_json::Value>,
    pub aprs_chat: Option<serde_json::Value>,
}

impl DockContext {
    pub fn set(&mut self, s: SurfaceId, v: Option<serde_json::Value>) {
        match s {
            SurfaceId::Routines => self.routines = v,
            SurfaceId::TacMap => self.tac_map = v,
            SurfaceId::AprsChat => self.aprs_chat = v,
        }
    }
}

/// The `dock:changed` payload AND the `dock_state_get` return — always the full
/// snapshot (spec §3: windows replace wholesale; a missed event self-heals).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DockSnapshot {
    pub surfaces: DockSurfaces,
    pub context: DockContext,
}

/// Pure transition core (spec §3). Returns true iff the transition was
/// EFFECTIVE (state changed); callers suppress persist + emit on false.
pub fn apply_transition(surfaces: &mut DockSurfaces, surface: SurfaceId, target: DockMode) -> bool {
    if surfaces.get(surface) == target {
        return false;
    }
    surfaces.set(surface, target);
    true
}

/// Consent host resolution (spec §6). Rust is CANONICAL; the TS mirror in
/// src/dock/dockState.ts must match via the shared parity fixture (Task 6).
pub fn consent_host_window(routines_mode: DockMode) -> &'static str {
    match routines_mode {
        DockMode::Docked => "main",
        DockMode::Popped => "pop-routines",
    }
}
```

Add `mod dock;` in `src-tauri/src/lib.rs` beside the existing `mod` declarations (grep `mod stations_window;` and add adjacent). Leave `registry.rs` as an empty `//! Task 4 fills this.` stub file so the module compiles.

- [ ] **Step 4: Local sanity** — `grep` your symbols compile-plausibly; run `cargo check --manifest-path src-tauri/Cargo.toml --offline 2>/dev/null || true` (it may not finish on the Pi — that's fine; CI is the gate).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/dock/ src-tauri/src/lib.rs
git commit -m "feat(dock): SurfaceId/DockMode wire contract + pure transition core (spec §3, tuxlink-dmwte task 1)"
```

---

### Task 2: Config v8 — the `dock` section

**Files:**
- Modify: `src-tauri/src/config.rs` (`CONFIG_SCHEMA_VERSION` at line ~28; `pub struct Config` at line ~230; the `config_schema_version_tracks_field_set` test near the bottom)
- Test: inline in `config.rs`

**Interfaces:**
- Consumes: `crate::dock::DockSurfaces` (Task 1).
- Produces: `Config.dock: DockSurfaces` — always-serialized, `#[serde(default)]`.

- [ ] **Step 1: Write the failing tests** (add to config.rs's test module)

**`Config` does NOT implement `Default`** — every config test builds its base via the existing `config_json(<version>, "")` string fixture (see `config_schema_version_tracks_field_set` at config.rs ~1936 and `pre_v7_wizard_completed_seeds_tip_sentinel` ~3157 for the shape). Use that pattern:

```rust
#[test]
fn v7_config_loads_with_default_dock_section() {
    // A v7 fixture naturally lacks "dock"; serde default must fill it all-Docked.
    let cfg: Config = serde_json::from_str(&config_json(7, ""))
        .expect("v7 file must load additively");
    assert_eq!(cfg.dock, crate::dock::DockSurfaces::default());
    assert_eq!(detect_schema_action(7), SchemaAction::MigrateAdditive);
    assert_eq!(detect_schema_action(8), SchemaAction::Current);
}

#[test]
fn dock_section_persists_popped_state() {
    let mut cfg: Config = serde_json::from_str(&config_json(CONFIG_SCHEMA_VERSION, "")).unwrap();
    cfg.dock.set(crate::dock::SurfaceId::TacMap, crate::dock::DockMode::Popped);
    let v: serde_json::Value = serde_json::to_value(&cfg).unwrap();
    // Spec §3 JSON literal: {"routines":"docked","tac_map":"popped","aprs_chat":"docked"}
    assert_eq!(v["dock"]["tac_map"], "popped");
    assert_eq!(v["dock"]["routines"], "docked");
}
```

- [ ] **Step 2: Note the guard that MUST fail first** — `config_schema_version_tracks_field_set` will fail the moment the field is added without the version bump; that failing state is your red.

- [ ] **Step 3: Implement** — in `config.rs`:
  1. `pub const CONFIG_SCHEMA_VERSION: u32 = 8;` and extend the doc comment: `/// Bumped 7 → 8 (tuxlink-dmwte): added the always-serialized top-level 'dock' section (dockable-surfaces popped/docked persistence, spec §3). Runtime context tokens are NOT persisted.`
  2. Add to `pub struct Config` (after the `onboarding` field, following the `rig`/`onboarding` precedent):

```rust
    /// Dockable-surfaces layout (tuxlink-dmwte, spec §3): which surfaces are
    /// popped into their own OS windows. Always-serialized; `#[serde(default)]`
    /// migrates pre-8 files (absent → all docked). Geometry is NOT here —
    /// tauri-plugin-window-state owns it per window label.
    #[serde(default)]
    pub dock: crate::dock::DockSurfaces,
```

  3. Update `config_schema_version_tracks_field_set`'s expected field list/hash per its own instructions (read the test — it documents how to re-stamp).

- [ ] **Step 4: Verify** — tests as in Step 1 read green by inspection; CI runs them for real.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "feat(config): schema v8 — dock section persists popped/docked per surface (spec §3, tuxlink-dmwte task 2)"
```

---

### Task 3: Shared secondary-window helper + close policy + pop capabilities

**Files:**
- Create: `src-tauri/src/secondary_window.rs`; `src-tauri/capabilities/pop-routines.json`, `pop-tacmap.json`, `pop-aprschat.json`
- Modify: `src-tauri/src/lib.rs` (`mod secondary_window;`), `src-tauri/src/help_window.rs`, `logging_window.rs`, `stations_window.rs`, `compose_window.rs` (each becomes a thin caller of the helper — **behavior must not change**)
- Test: inline in `secondary_window.rs`

**Interfaces (Produces):**
```rust
pub enum ClosePolicy { CloseSelf, CommandRouted, DockBack }   // spec §3, adrev R3-F7
pub struct SecondaryWindowSpec {
    pub label: String,            // window label ("help", "compose-<id>", "pop-tacmap", …)
    pub route: String,            // WebviewUrl::App path
    pub title: String,
    pub inner_size: (f64, f64),
    pub min_inner_size: (f64, f64),
    pub decorations: bool,
    pub close_policy: ClosePolicy, // recorded for the lib.rs on_window_event dispatch (Task 4)
}
pub fn caller_is_authorized(caller_label: &str) -> bool       // == "main" (single shared copy)
pub fn open_secondary_window(app: &AppHandle, caller_label: &str, spec: &SecondaryWindowSpec) -> Result<(), String>
```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_main_is_authorized() {
        assert!(caller_is_authorized("main"));
        for bad in ["help", "stations", "compose-x", "pop-routines", ""] {
            assert!(!caller_is_authorized(bad), "{bad} must not spawn windows");
        }
    }

    /// The pop windows' spec constants (spec §3: sizes; §3 wire table: labels/
    /// routes/titles; decorations always false = custom chrome).
    #[test]
    fn pop_specs_match_wire_contract() {
        use crate::dock::SurfaceId;
        let map = pop_window_spec(SurfaceId::TacMap);
        assert_eq!(map.label, "pop-tacmap");
        assert_eq!(map.route, "/pop/tacmap");
        assert_eq!(map.inner_size, (1100.0, 750.0));
        assert!(matches!(map.close_policy, ClosePolicy::DockBack));
        assert!(!map.decorations);
        let routines = pop_window_spec(SurfaceId::Routines);
        assert_eq!(routines.inner_size, (960.0, 680.0));
        let chat = pop_window_spec(SurfaceId::AprsChat);
        assert_eq!(chat.inner_size, (440.0, 640.0));
    }
}
```

- [ ] **Step 2: Implement the helper.** Transplant the get-or-focus + builder + `WindowLabelAlreadyExists`/`WebviewLabelAlreadyExists` race-guard body from `stations_window.rs:38-71` verbatim into `open_secondary_window`, parameterized by the spec struct (apply `.decorations(spec.decorations)` and the title/sizes). Add `pub fn pop_window_spec(surface: crate::dock::SurfaceId) -> SecondaryWindowSpec` with the constants from Step 1 (sizes from spec §3: TacMap 1100×750, Routines 960×680, AprsChat 440×640; min sizes 420×360 like stations).

- [ ] **Step 3: Migrate the four existing windows.** Each `*_window_open` keeps its `#[tauri::command]` signature, its label constant, and its docstring, but its body becomes: authorization check via the shared `caller_is_authorized`, then `open_secondary_window(&app, caller.label(), &SPEC)`. Close policies: help/logging/stations `CloseSelf`; compose `CommandRouted` (its `compose_close_self` + monitor-height clamp code stays untouched in `compose_window.rs`). **Do not change any label, route, size, or decoration value** — copy each window's current constants into its spec. Keep each file's existing tests passing (the `caller_is_authorized` tests in `stations_window.rs` may now delegate to the shared fn — update imports, keep assertions).

- [ ] **Step 4: Write the three capability files.** Model on the STRUCTURE of `stations.json` (read it), NOT help/logging (they grant `core:window:allow-close` — the opposite close semantics; spec §3). Content for `pop-tacmap.json` (repeat per label, changing `identifier` and `windows`):

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "pop-tacmap",
  "description": "Popped Tac Map window (tuxlink-dmwte): events + custom-chrome window ops. NO allow-close — close routes through surface_dock_back (spec §3).",
  "windows": ["pop-tacmap"],
  "permissions": [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "core:event:allow-emit",
    "core:window:allow-start-dragging",
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-start-resize-dragging"
  ]
}
```

  (If the existing custom-chrome windows' capability files use different permission identifiers for the window ops — drag, resize, minimize, maximize-toggle, and the `is-maximized` query a maximize-toggle typically needs — copy THEIR exact identifiers from `help.json`; the ONLY grant you must never copy from it is `core:window:allow-close`. The `core:event:allow-emit` line is LOAD-BEARING: the snapshot handshakes emit from the client window — `stations.json` shipped without it and its handshake silently never fires; see the bd issue filed 2026-07-15 citing this plan.)

- [ ] **Step 5: Run what's runnable** — `pnpm typecheck` unaffected; eyeball Rust. Commit:

```bash
git add src-tauri/src/secondary_window.rs src-tauri/src/lib.rs src-tauri/src/*_window.rs src-tauri/capabilities/pop-*.json
git commit -m "refactor(windows): shared open_secondary_window helper + ClosePolicy; migrate 4 windows; pop-* capabilities (spec §3, tuxlink-dmwte task 3)"
```

**REVIEW LOOP: Tasks 1–3 form the first logical group — run the 3-round multi-perspective review now.**

---

### Task 4: Dock registry, commands, events, close-intent, crash wiring, restoration

**Files:**
- Create: fill `src-tauri/src/dock/registry.rs`, create `src-tauri/src/dock/commands.rs`
- Modify: `src-tauri/src/lib.rs` — register commands in the `invoke_handler` list (near `stations_window_open`, line ~3022); add the `pop-*` branch in `on_window_event` (the handler at line ~2849); call restoration setup where managed state is built.
- Test: inline `#[cfg(test)]` (pure parts) — the Tauri-handle parts are integration-verified via the live pass; keep them thin.

**Interfaces (Produces):**
```rust
// registry.rs
pub struct DockRegistry(Mutex<DockSnapshot>);              // Tauri managed state
impl DockRegistry {
    pub fn new(persisted: DockSurfaces) -> Self;
    pub fn snapshot(&self) -> DockSnapshot;
    /// The one transition path (spec §3): mutate → persist best-effort → emit
    /// ALWAYS on effective transition; returns false for no-ops (no emit).
    pub fn transition(&self, app: &AppHandle, surface: SurfaceId, target: DockMode,
                      context: Option<serde_json::Value>) -> bool;
}
// commands.rs — all #[tauri::command]
pub fn surface_pop_out(app, caller: WebviewWindow, registry: State<DockRegistry>,
                       surface: SurfaceId, context: Option<serde_json::Value>) -> Result<(), String>;
pub fn surface_dock_back(app, registry: State<DockRegistry>,
                         surface: SurfaceId, context: Option<serde_json::Value>) -> Result<(), String>;
pub fn surface_focus(app, surface: SurfaceId) -> Result<(), String>;
pub fn dock_state_get(registry: State<DockRegistry>) -> DockSnapshot;
pub fn shell_mounted(app, registry: State<DockRegistry>) -> Result<(), String>;  // idempotent restoration trigger
// registry.rs — the idempotence guard Step 1's test constructs:
#[derive(Default)] pub struct RestorationGate(std::sync::atomic::AtomicBool);
impl RestorationGate { pub fn arm(&self) -> bool; }  // true exactly once
```
Events emitted: `dock:changed` (payload `DockSnapshot`, broadcast); `dock:close-intent` (emitted TO the specific pop window's webview — payload `{ "surface": <wire string> }`).

**Behavior requirements (each maps to a spec clause — implement ALL):**
1. `surface_pop_out` (spec §3): caller must be `main` OR the surface's own pop label (re-pop is focus). If window exists live → `show`+`set_focus`, return Ok (NO transition — pop-out on live Popped is a no-op). Else spawn via `open_secondary_window(pop_window_spec(surface))`; only on Ok run `registry.transition(surface, Popped, context)`.
2. `surface_dock_back` (spec §3): callable from `main` or the surface's own pop window. Run `transition(surface, Docked, context)`; if effective, then `destroy()` the window (never `close()` — loop; get the window by label, ignore "not found"). No-op (already Docked) → skip destroy AND emit.
3. `transition` persist path: update `Config.dock` via `update_config(mutate)` at `config.rs:1066` (the Mutex-gated read→mutate→write surface over `write_config_atomic` at `config.rs:982`; usage precedent in `ui_commands.rs`). Hold the `DockRegistry` mutex across the persist so registry mutation + persist are one critical section. On write Err, `tracing::warn!` AND surface a session-log warning via `crate::session_log_emit::emit` (declared `lib.rs:25`, usage precedent `ui_commands.rs:3435`), but STILL emit `dock:changed` (registry is authoritative — spec §3).
4. Close-intent round-trip (spec §3): in lib.rs `on_window_event`, add a branch BEFORE the main-window arm: if `SurfaceId::from_window_label(window.label())` is Some → `api.prevent_close()`; emit `dock:close-intent` to that window; spawn a task that waits 1500 ms and then, if the surface is still Popped, calls the dock-back transition with `context: None` (the webview's own `surface_dock_back` invoke with its token normally lands first and the timeout finds Docked → no-op).
5. Crash wiring (spec §3): after each pop-window spawn, connect the WebKitGTK `web-process-terminated` signal → route into the same dock-back transition (`context: None`). **The in-tree precedent is `src-tauri/src/forms/pdf_export.rs:96-101`** — `window.with_webview(|platform| { let wv = platform.inner(); … })` yields the `webkit2gtk::WebView`; no cfg attribute needed (webkit2gtk is a Linux-target dep, Cargo.toml:187-189, `webkit2gtk = "=2.0.2"`, and wry 0.55.1 already unifies the `v2_40` feature set, so `WebViewExt::connect_web_process_terminated` is available with NO Cargo.toml or Cargo.lock change). **If the signal still proves unreachable, STOP and escalate to the operator — do not improvise a fallback (spec §3).**
6. `shell_mounted` (spec §3): first call (guard with a `OnceLock`/`AtomicBool` in managed state) walks `SurfaceId::ALL`; for each persisted `Popped`, spawn its window (get-or-focus). Later calls no-op. Never called during wizard (frontend gates it — Task 8).
7. Park notification (spec §6): the park emission site is `RoutinesEvent::AwaitingConsent` in `src-tauri/src/routines/consent.rs:141-150` (grep `AwaitingConsent`, case-sensitive — lowercase `awaitingConsent` only hits a test assertion). Add: resolve `consent_host_window(registry.snapshot().surfaces.routines)`; if that window exists and is not focused (`is_focused().unwrap_or(false)`), fire a `tauri_plugin_notification` notification ("Routine awaiting transmit consent — <routine>") and call `request_user_attention(Some(UserAttentionType::Critical))` (X11 polish; Wayland no-op accepted).
8. `surface_focus` (spec §5 — the feature's most load-bearing call): get the window by label; `unminimize()`, then `show()`, then `set_focus()`, in that order. On Wayland, Tauri's `set_focus` maps to `gtk_window_present`, whose cross-toplevel activation depends on xdg-activation; labwc supports the protocol. The §13 (Task 13) live pass verifies pathway-click → raise/unminimize on labwc AND X11 as a hard gate; **if `set_focus` does not raise on labwc, STOP and escalate — the pathway principle depends on this call, and a workaround is a spec-level decision.** Window absent (stale pathway) → Ok(()) no-op (the dock:changed reconcile heals the UI).

- [ ] **Step 1: Write the failing pure tests** — transition emit-suppression + context lifecycle in `registry.rs` (factor the mutate+context bookkeeping into a pure fn `apply_with_context(snap: &mut DockSnapshot, surface, target, context) -> bool` so it's testable without an AppHandle):

```rust
#[test]
fn context_stored_on_effective_transition_and_cleared_on_next() {
    let mut snap = DockSnapshot::default();
    assert!(apply_with_context(&mut snap, SurfaceId::Routines, DockMode::Popped,
        Some(serde_json::json!({"view":"designer"}))));
    assert_eq!(snap.context.routines.as_ref().unwrap()["view"], "designer");
    // Next transition of the same surface REPLACES the token (spec §3).
    assert!(apply_with_context(&mut snap, SurfaceId::Routines, DockMode::Docked, None));
    assert!(snap.context.routines.is_none());
    // No-op transition: state, context, and return all unchanged.
    assert!(!apply_with_context(&mut snap, SurfaceId::Routines, DockMode::Docked,
        Some(serde_json::json!({"x":1}))));
    assert!(snap.context.routines.is_none());
}

#[test]
fn shell_mounted_is_idempotent() {
    let gate = RestorationGate::default();
    assert!(gate.arm());   // first call: proceed
    assert!(!gate.arm());  // every later call: no-op
}
```

- [ ] **Step 2: Implement** registry + commands per the behavior list. Keep AppHandle-touching code thin (spawn/destroy/emit one-liners around the pure core).
- [ ] **Step 3: Wire lib.rs** — `.manage(DockRegistry::new(config.dock))` beside the other managed state; the five commands into `invoke_handler`; the `on_window_event` pop branch.
- [ ] **Step 4: `pnpm typecheck` unaffected; eyeball; commit:**

```bash
git add src-tauri/src/dock/ src-tauri/src/lib.rs src-tauri/src/routines/
git commit -m "feat(dock): registry + commands + close-intent + crash signal + shell_mounted restoration + park notification (spec §3/§6, tuxlink-dmwte task 4)"
```

---

### Task 5: `aprs-message:sent` — the own-send echo

**Files:**
- Modify: `src-tauri/src/winlink/aprs/engine.rs` only. NOTE the real topology (do not go hunting elsewhere): `fn aprs_send` is a thin `#[tauri::command]` in `ui_commands.rs:4332` delegating to `AprsState::send` (`engine.rs:809`), which pushes `TxCommand::Send/Broadcast` into the driver channel; the `EventSink` (whose Tauri impl `TauriEventSink::emit_message` is at `engine.rs:936`) lives in the driver loop. **Mechanism (decided — do not choose another):** extend the `EventSink` trait with `emit_sent(dto: SentMsgDto)`; call it from the driver loop at the point it dequeues the Send/Broadcast command (the message is accepted and its tracking id exists there). Ordering vs the `aprs_send` invoke return is irrelevant: the frontend dedupes by msgid in both arrival orders (Task 10 tests both).
- Test: extend the mock `EventSink` at `engine.rs:983` (the existing test double — there are NO event-channel string tests to copy; the mock sink is the pattern) to record `emit_sent` calls; assert emission with the minted msgid, text, and `addressee` normalization on the driver-loop dequeue path.

**Interfaces (Produces):** event `aprs-message:sent`, payload (spec §7):

```rust
#[derive(Debug, Clone, Serialize)]
pub struct SentMsgDto {
    pub msgid: String,        // the tracking id aprs_send already returns
    pub addressee: String,    // "" = broadcast (matches InboundMsgDto convention)
    pub text: String,
    pub at_ms: u64,           // backend clock at acceptance
}
```

- [ ] **Step 1: Failing test** — using the mock `EventSink` (engine.rs:983 pattern), drive a Send command through the driver loop and assert one `emit_sent` with the minted msgid + text + `addressee` normalization (`None`/empty recipient → `""`).
- [ ] **Step 2: Implement** — add `emit_sent` to the `EventSink` trait (default impl NOT allowed — every sink implements it explicitly so the mock records it); `TauriEventSink::emit_sent` does `app.emit("aprs-message:sent", dto)` (broadcast, same as `aprs-message:new`); call it in the driver loop on Send/Broadcast dequeue.
- [ ] **Step 3: Commit:**

```bash
git add src-tauri/src/winlink/aprs/
git commit -m "feat(aprs): aprs-message:sent own-send echo at aprs_send acceptance (spec §7, tuxlink-dmwte task 5)"
```

**REVIEW LOOP: Tasks 4–5 are the second logical group — run the 3-round review now.**

---

### Task 6: Frontend dock state — types, routing, guards, parity

**Files:**
- Create: `src/dock/dockState.ts`, `src/dock/dockState.test.ts`, `src/dock/dockParity.test.ts`
- Modify: `src/routing.ts` (append), `src/App.tsx` (route branch + guard predicate)
- Test: `src/routing.test.ts` (extend — it exists; follow its `parseComposeRoute` test shape), the two new test files

**Interfaces (Produces — Tasks 7–10 rely on these exact names):**
```typescript
// src/dock/dockState.ts
export type SurfaceId = 'routines' | 'tac_map' | 'aprs_chat';
export type DockMode = 'docked' | 'popped';
export interface DockSurfaces { routines: DockMode; tac_map: DockMode; aprs_chat: DockMode }
export interface DockSnapshot { surfaces: DockSurfaces; context: Record<SurfaceId, unknown | null> }
export const SURFACE_WINDOW_LABEL: Record<SurfaceId, string>;  // copy spec §3 table, never derive
export function consentHostWindow(s: DockSurfaces): 'main' | 'pop-routines';  // MIRRORS Rust (parity fixture)
export function useDockState(): DockSnapshot | null;           // null until first read; listen-FIRST discipline
export function popOut(surface: SurfaceId, context?: unknown): Promise<void>;
export function dockBack(surface: SurfaceId, context?: unknown): Promise<void>;
export function focusSurface(surface: SurfaceId): Promise<void>;
// src/routing.ts
export function parsePopRoute(pathname: string): SurfaceId | null;   // '/pop/routines'|'/pop/tacmap'|'/pop/aprschat'
export function isSecondaryWindow(pathname: string): boolean;        // compose|help|logging|stations|pop-*
```

- [ ] **Step 1: Write the failing tests.** In `src/routing.test.ts` (follow the existing describe blocks):

```typescript
describe('parsePopRoute', () => {
  it('maps the three pop routes to surface ids (spec §3 table — underscore dropped in route, kept in id)', () => {
    expect(parsePopRoute('/pop/routines')).toBe('routines');
    expect(parsePopRoute('/pop/tacmap')).toBe('tac_map');
    expect(parsePopRoute('/pop/aprschat')).toBe('aprs_chat');
    expect(parsePopRoute('/pop/tacmap/')).toBe('tac_map');       // trailing slash tolerated
    expect(parsePopRoute('/pop/tac_map')).toBeNull();            // the id form is NOT a route
    expect(parsePopRoute('/pop')).toBeNull();
    expect(parsePopRoute('/')).toBeNull();
  });
});

describe('isSecondaryWindow', () => {
  it('covers all five secondary kinds (adrev Codex-9: pop windows must not run main-only side effects)', () => {
    for (const p of ['/compose/d1', '/help', '/logging', '/stations', '/pop/routines', '/pop/tacmap', '/pop/aprschat']) {
      expect(isSecondaryWindow(p)).toBe(true);
    }
    expect(isSecondaryWindow('/')).toBe(false);
  });
});
```

  In `src/dock/dockState.test.ts` — `consentHostWindow` both branches; `useDockState` listen-before-get ordering (mock `@tauri-apps/api/event` `listen` and `@tauri-apps/api/core` `invoke` per the project's standard mock pattern — REMEMBER the vitest teardown rule: invoke mocks get called with NO args at cleanup, so mock implementations must tolerate `undefined` cmd): assert `listen('dock:changed', …)` is awaited BEFORE `invoke('dock_state_get')` fires (spec §5 — capture call order in an array).

  In `src/dock/dockParity.test.ts` — the cross-language fixture (spec §10): commit a shared fixture file `src/dock/dock-wire-fixture.json` containing two snapshot variants keyed `routinesDocked` / `routinesPopped` (JSON per the spec §3 literal). BOTH sides assert against it. **TS side:** parse each variant into `DockSnapshot`; assert `consentHostWindow(routinesPopped.surfaces) === 'pop-routines'` and `=== 'main'` for the docked variant. **Rust side — THIS task adds it** (Task 1 deliberately does not): a test in `src-tauri/src/dock/mod.rs` that `include_str!`s the fixture (relative path `../../../src/dock/dock-wire-fixture.json`), deserializes each variant into `DockSnapshot`, re-serializes, and asserts `serde_json::Value` equality against the parsed original (Value equality, NOT string comparison — whitespace/key order must not matter). This task's Files and commit therefore include `src-tauri/src/dock/mod.rs`.

- [ ] **Step 2: Run to verify fail** — `pnpm vitest run src/routing.test.ts src/dock/` → FAIL (symbols missing).
- [ ] **Step 3: Implement.** `parsePopRoute` via a literal map `{ routines: 'routines', tacmap: 'tac_map', aprschat: 'aprs_chat' }` on `/^\/pop\/([a-z]+)\/?$/`. `isSecondaryWindow` composes the four existing parsers + `parsePopRoute`. `useDockState`: effect that (a) registers `listen<DockSnapshot>('dock:changed', set)`, (b) then `invoke('dock_state_get')` and set, (c) re-reads via one more `dock_state_get` after the listener resolves (spec §5 reconcile — closes the get-then-subscribe gap). In `App.tsx`: add the lazy `PoppedSurfaceHost` branch mirroring the stations branch; replace the hand-rolled secondary-window conditions in the first-paint suppression (~line 66) and wizard-probe guard (~line 78) with `isSecondaryWindow(pathname)` — behavior for the four existing kinds unchanged (the predicate is a superset).
- [ ] **Step 4: `pnpm vitest run src/routing.test.ts src/dock/ && pnpm typecheck`** → PASS.
- [ ] **Step 5: Commit:**

```bash
git add src/routing.ts src/routing.test.ts src/App.tsx src/dock/ src-tauri/src/dock/mod.rs
git commit -m "feat(dock): frontend wire mirror + useDockState (listen-first) + pop routes + isSecondaryWindow guard + two-sided parity fixture (spec §3/§5/§10, tuxlink-dmwte task 6)"
```

---

### Task 7: PoppedSurfaceHost — title bar, registry, strips, theme, keyboard

**Files:**
- Create: `src/dock/PoppedSurfaceHost.tsx`, `src/dock/PopTitleBar.tsx`, `src/dock/surfaceRegistry.tsx`, `src/dock/strips.tsx`, `src/dock/PoppedSurfaceHost.css`, `src/dock/PoppedSurfaceHost.test.tsx`
- Test: the new test file

**Interfaces:**
- Consumes: `parsePopRoute` output (App.tsx passes the `SurfaceId`), `dockBack` (Task 6), the surface components (`RoutinesSurface`, `AprsPositionsMap`, `AprsChatPanel` + `AprsConnectStrip`), `useAprsChat`/`useAprsPositions`/`useRoutines` hooks.
- Produces (registry entry shape Tasks 8–10 extend):

```typescript
export interface SurfaceComponentProps {
  /** The continuity token's `state` half from dock_state_get, null when absent. */
  context: unknown | null;
  /** The surface registers a live state-collector; PoppedSurfaceHost stores it in
   *  a ref and calls it at every dock-back path (⇤, ✕, Ctrl+W, close-intent) to
   *  build the outgoing token's `state`. Surfaces with no internal state to carry
   *  (tac_map, aprs_chat) never call it — the host's ref stays null. */
  registerGetContext: (fn: () => unknown | null) => void;
}
export interface SurfaceRegistryEntry {
  id: SurfaceId;
  title: string;                                     // from the spec §3 table
  Component: React.ComponentType<SurfaceComponentProps>;
  StatusStrip: React.ComponentType;                  // chrome option B (spec §4)
}
export const SURFACE_REGISTRY: Record<SurfaceId, SurfaceRegistryEntry>;
// Deliberate deviation from spec §4's registry sketch: NO defaultSize field here —
// first-spawn sizes live Rust-side in pop_window_spec (Task 3). Do not "restore" it.
```

**Behavior requirements:**
1. `PopTitleBar` (model on `src/help/HelpTitleBar.tsx` for drag-region + `getCurrentWindow()` mechanics — but its ✕ calls `win.close()`, which pop windows must NEVER do; every close path here goes through `dockBack`): left → **⇤ Dock back** button (`aria-label="Dock back into main window"`) → collect `state` from the registered getContext ref → `dockBack(surface, { foreground: true, state })`. Center: static title (drag region). Right: minimize / maximize / **✕** — ✕ → `dockBack(surface, { foreground: false, state })`. Every dock-back context is the `{ foreground, state }` envelope (Global Constraints), from every path. All controls tab-reachable `<button>`s with `aria-label`s (spec §4).
2. **Close-intent listener:** on mount, `listen('dock:close-intent', …)`; assert `payload.surface` equals this host's surface before acting (belt-and-braces — a broadcast-emitting backend bug must not dock every window back), then the ✕ path (collect state, `dockBack` with `foreground: false`). This is how a WM close carries state out before the backend's 1.5 s timeout fires (spec §3).
3. **Ctrl+W** → the ✕ path (spec §4). `keydown` listener on window, `e.ctrlKey && e.key === 'w'`, preventDefault.
4. **Theme (spec §4, adrev R5-F9):** on mount, apply the stored scheme exactly as the main window does at boot (the apply fn in `src/shell/colorScheme.ts`); add a `window.addEventListener('storage', …)` that re-applies on changes to **BOTH keys** — `tuxlink.colorScheme` AND `tuxlink.customTheme` (colorScheme.ts:57-58; the custom-theme token re-injection is explicitly required by spec §4 — scheme-key-only leaves popped windows stale on custom-theme edits).
5. **Strips (spec §4 — never duplicate a vital the surface already shows):** in `strips.tsx`: `RoutinesStrip` (parked count from `useParkedRuns().parked.length`, running count + next fire from `useRoutines()` — grep its return shape), `TacMapStrip` (last-packet age, ticking 1 s interval, from `useAprsPositions()`'s newest position timestamp; plotted-station count ONLY after checking the map's filter bar — grep `stationBuckets`, whose chips may already show counts — omit if duplicated), `ChatStrip` (last-heard callsign + unread placeholder from `useAprsChat().heardStations[0]`). Keep each ≤ ~30 lines; dark-theme styles in the CSS file per the design tokens used by `StatusBar` (grep its class names for token vars).
6. Consent modal mounts here for Routines only (Task 8 wires the gating prop).

- [ ] **Step 1: Failing tests** (`PoppedSurfaceHost.test.tsx`, jsdom + the standard tauri mocks):

Mock pattern: copy `src/aprs/useEnvStations.test.ts:11-39`'s per-file `vi.mock('@tauri-apps/api/event', …)` + `vi.mock('@tauri-apps/api/core', …)` pair (there is NO shared mock helper in this repo); remember the teardown rule from Global Constraints (invoke mocks called with no args at cleanup).

```typescript
it('renders title bar with dock-back, min, max, close — all labeled buttons (spec §4)', () => {
  render(<PoppedSurfaceHost surface="tac_map" />);
  expect(screen.getByRole('button', { name: /dock back into main window/i })).toBeInTheDocument();
  expect(screen.getByText('Tac Map — Tuxlink')).toBeInTheDocument();
});

it('✕ and Ctrl+W invoke surface_dock_back with the {foreground:false} envelope, never window.close (spec §4)', async () => {
  render(<PoppedSurfaceHost surface="tac_map" />);
  fireEvent.keyDown(window, { key: 'w', ctrlKey: true });
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('surface_dock_back',
    expect.objectContaining({ surface: 'tac_map', context: expect.objectContaining({ foreground: false }) })));
});

it('storage event re-applies the color scheme — both tuxlink.colorScheme and tuxlink.customTheme keys (spec §4, adrev R5-F9)', () => { /* set localStorage, dispatch StorageEvent per key, assert documentElement dataset.theme / injected tokens */ });
```

- [ ] **Step 2: `pnpm vitest run src/dock/PoppedSurfaceHost.test.tsx`** → FAIL.
- [ ] **Step 3: Implement** per the behavior list. Registry entries for `tac_map`/`aprs_chat` complete here (their components take no token: `Component: () => <AprsPositionsMapPopped/>` etc. — Tac Map's popped wrapper mounts `useAprsPositions()` + `useEnvStations({snapshotRole:'client'})` and feeds `AprsPositionsMap` the same props AppShell does at line ~2120, grep the exact prop list; chat's wrapper composes `AprsConnectStrip` above `AprsChatPanel`, mirroring AppShell's dock composition at ~2313–2334). Routines entry renders `RoutinesSurface` with `view` from the token's `state` (default dashboard) — Task 8 finishes its wiring.
- [ ] **Step 4: Tests green + `pnpm typecheck`.**
- [ ] **Step 5: Commit:**

```bash
git add src/dock/
git commit -m "feat(dock): PoppedSurfaceHost — labeled chrome, Ctrl+W, close-intent token flush, theme storage listener, mini strips (spec §4, tuxlink-dmwte task 7)"
```

**REVIEW LOOP: Tasks 6–7 are the third logical group — run the 3-round review now.**

---

### Task 8: Routines end-to-end — affordances, token, menu, consent split

**Files:**
- Modify: `src/shell/AppShell.tsx` (state cluster ~508–590; render slots ~2036–2044; ConsentGate mount ~2455; menu handlers ~1711), `src/shell/chrome/menuModel.ts` (Routines menu ~66–69 — static id addition only), `src/shell/chrome/MenuBar.tsx` (the dynamic parts — see item 3), `src/shell/chrome/dispatchMenuAction.ts`, `src/routines/RoutinesSurface.tsx` (dashboard header ↗), `src/routines/designer/RoutineDesigner.tsx` (designer header ↗ + `initialDraft` seed prop + getContext wiring — the designer header lives HERE, not in RoutinesSurface.tsx), `src/routines/ConsentGate.tsx`, `src/dock/surfaceRegistry.tsx`
- Test: `src/routines/ConsentGate.test.tsx` (exists — extend), `src/shell/chrome/menuModel.test.ts` (**golden `EXPECTED_IDS` list at lines 6–33 MUST be updated** — adding `menu:routines:dockback` fails it until you do; this is the documented scoped-vitest contract-test trap), `src/shell/AppShell.dock.test.tsx` (new), existing AppShell tests must stay green

**Interfaces:**
- Consumes: `useDockState`, `popOut`, `dockBack`, `focusSurface`, `consentHostWindow` (Task 6); registry entry (Task 7).
- Produces: `ConsentGateProps` gains `renderModal?: boolean` (default true — existing callers unchanged); the Routines continuity token `state` shape, defined once here: `{ view: RoutinesView; draft?: RoutineDef }` (`RoutinesView` from `RoutinesSurface.tsx:30-32`; `RoutineDef` the existing definition type `routinesApi.ts` exports); `RoutineDesignerProps` gains `initialDraft?: RoutineDef` (when present, the designer seeds from it and skips its `routines_get` fetch).

**Behavior requirements (spec §5/§6/§7):**
1. **↗ affordances:** the dashboard header (`RoutinesSurface.tsx`) and the designer header (`RoutineDesigner.tsx` — NOT RoutinesSurface) each get a text-labeled "↗ Pop out" button → `popOut('routines', { foreground: true, state: { view: <current RoutinesView>, draft: <designer's current draft, designer only> } })`. Designer draft plumbing (this is real work, not a grep): `RoutineDesigner` holds its draft in internal state; add the `initialDraft?: RoutineDef` prop (seed + skip fetch when present) and surface the live draft upward for token collection — inside the popped host via `registerGetContext` (Task 7's contract: the routines registry Component wires `registerGetContext(() => ({ view: currentView, draft: currentDraft }))`); inline in AppShell via a ref callback prop on `RoutinesSurface` mirroring the same shape, used by the ↗ click handler.
2. **AppShell dock subscription:** `useDockState()`; while `surfaces.routines === 'popped'`, force `routinesView = null` (pane returns to mailbox) and ignore `menu:routines:open` pane-swap — instead `focusSurface('routines')`.
3. **Menu (spec §5):** `MENU_TREE` in `menuModel.ts` is STATIC data with no dynamic-label affordance — the dynamic parts are MenuBar-level, exactly like the existing `badges` special case (`MenuBar.tsx:65-103`, `badges?: { routines?: number }` prop, hardcoded render at ~103). Do: (a) add `{ id: 'menu:routines:dockback', label: 'Dock Routines back' }` statically to `MENU_TREE` and update the golden `EXPECTED_IDS` in `menuModel.test.ts`; (b) give MenuBar a `dockPopped?: boolean` prop — while true, render the Routines top-level label as "Routines ↗" and show the dockback item; while false, hide it; (c) `dispatchMenuAction` handler for `menu:routines:dockback` → `dockBack('routines', { foreground: true, state: null })` per the Global Constraints main-side dock-back rule (main cannot supply the popped window's state; Routines falls back to the dashboard — accepted).
4. **`menu:routines:new` while popped (spec §5):** `focusSurface('routines')` + emit surface-scoped event `dock:intent` payload `{ surface: 'routines', intent: 'new_routine' }` (plain frontend `emit` — cross-window). The popped host listens and forwards to `RoutinesSurface`'s existing new-routine entry point (grep how the dashboard's New Routine button navigates).
5. **⇤ foreground on main:** when a `dock:changed` arrives flipping routines popped→docked AND the arriving snapshot's `context.routines?.foreground === true`, set `routinesView` to the token's `state.view` (fall back `'dashboard'`); when `foreground: false`, leave the current pane alone (availability semantics).
6. **ConsentGate split (spec §6):** AppShell renders `<ConsentGate renderModal={consentHostWindow(dock.surfaces) === 'main'} onParkedChange={…} reopenSignal={…}/>`; `PoppedSurfaceHost` (routines entry) renders `<ConsentGate renderModal={true}/>` (it only mounts when routines is popped, and then it IS the host). In `ConsentGate.tsx`: `if (!renderModal) return null;` placed AFTER the hooks (the data hook + onParkedChange mirroring must keep running in main for the badge).
7. **Badge/StatusBar click routing (spec §5):** the StatusBar consent item's onClick (AppShell ~2372) and any badge click: `consentHostWindow === 'main'` → bump `reopenSignal` (existing); else → `focusSurface('routines')`.
8. **Journal-seeded park duration (spec §6):** in `ConsentGate.tsx`'s launch-recovery path, extend `recoverParkedStepId` to also return the journal entry's timestamp (the `step_intent` entry carries one — grep the journal entry shape in `routinesApi.ts`) and use it for `parkedAtMs` instead of `Date.now()`; live-event parks keep `Date.now()` (the event is the park moment).
9. **`shell_mounted` (spec §3):** AppShell's mount effect invokes `shell_mounted` once (it is NOT in the wizard tree, so wizard gating is structural).
10. **Quit-prompt wording (spec §6, adrev R4-F11):** the graceful-quit prompt ("N routines running — stop them and exit?", from the parent-spec engine work — grep `running — stop` / the quit-confirm implementation across src/ and src-tauri/ to locate it) must name awaiting-consent runs distinctly: e.g. "1 routine running, 1 waiting for transmit consent — stop them and exit?". Update its test alongside.

- [ ] **Step 1: Failing tests.** Extend `ConsentGate.test.tsx`: `renderModal={false}` renders no modal but still fires `onParkedChange` with the parked list; `renderModal` default keeps all existing tests passing untouched. New `AppShell.dock.test.tsx` (mock `useDockState` module): routines-popped renders mailbox (no `RoutinesSurface`), menu wiring calls `focusSurface`; a `dock:changed` docked-arrival with `foreground: true, state: {view: designer-ish}` sets the pane; with `foreground: false` does not.
- [ ] **Step 2: Run** → FAIL. **Step 3: Implement.** **Step 4:** `pnpm vitest run src/routines/ src/shell/AppShell.dock.test.tsx && pnpm typecheck` → PASS, plus the full `pnpm vitest run` once (AppShell is heavily tested — nothing may regress).
- [ ] **Step 5: Commit:**

```bash
git add src/shell/ src/routines/ src/dock/
git commit -m "feat(routines): pop-out e2e — affordances, continuity token, menu verbs, ConsentGate split + badge routing + journal-seeded duration (spec §5/§6/§7, tuxlink-dmwte task 8)"
```

---

### Task 9: Tac Map wiring + positions snapshot handshake

**Files:**
- Modify: `src/shell/AppShell.tsx` (map slot ~2114–2148 + the map toggle control), `src/aprs/useAprsPositions.ts`, `src/dock/surfaceRegistry.tsx` (finish the tac_map entry from Task 7)
- Test: `src/aprs/useAprsPositions.test.ts` (extend existing or create following `useEnvStations`' test), `src/shell/AppShell.dock.test.tsx` (extend)

**Behavior requirements (spec §5/§7):**
1. ↗ in the map header controls → `popOut('tac_map', { foreground: true, state: null })` (viewport continuity is `usePersistedViewport`, not the token).
2. While popped: the inline map never renders regardless of `aprsMapOpen`; the toggle control renders "Tac Map ↗ — in window" → `focusSurface('tac_map')`, with an adjacent "⇤ dock back" action → `dockBack('tac_map', { foreground: true, state: null })` (Global Constraints rule). ⇤-foreground arrival (`context.tac_map?.foreground === true` on the docked flip) sets `aprsOpen = true` AND `aprsMapOpen = true` (the inline placement's two preconditions — spec §5); a `foreground: false` arrival (✕ from the popped window) changes neither.
3. `useAprsPositions` gains `snapshotRole?: 'host' | 'client'` copied from `useEnvStations.ts:38-131` — **with the spec §7 retry amendment:** the client re-emits `SNAPSHOT_REQUEST` every 250 ms until the first reply arrives or 3 s elapses (a `setInterval` cleared on reply/timeout/unmount). Event names: `aprs-positions:request-snapshot` / `aprs-positions:snapshot`. AppShell's existing `useAprsPositions()` call (~line 500) becomes `useAprsPositions({ snapshotRole: 'host' })`; the popped wrapper uses `'client'`.

- [ ] **Step 1: Failing tests** — the retry path is the load-bearing one (spec §10): with fake timers, client mounts, host listener registers 600 ms later (delayed mock), assert ≥2 request emissions and successful seeding; assert retries stop after reply; assert 3 s gives up cleanly. Follow `useEnvStations`' existing test file for the listen/emit mock pattern.
- [ ] **Step 2–4: red → implement → green** (`pnpm vitest run src/aprs/ && pnpm typecheck`).
- [ ] **Step 5: Commit:**

```bash
git add src/aprs/useAprsPositions.ts src/aprs/*.test.ts src/shell/AppShell.tsx src/dock/surfaceRegistry.tsx
git commit -m "feat(tacmap): pop-out wiring + positions snapshot handshake with 250ms/3s retry (spec §5/§7, tuxlink-dmwte task 9)"
```

---

### Task 10: APRS Chat wiring — echo consumption, handshake, dock-aware flows

**Files:**
- Modify: `src/aprs/useAprsChat.ts`, `src/shell/AppShell.tsx` (APRS dock tab ~2313–2334 + the dock-opening flows ~878–932), `src/dock/surfaceRegistry.tsx` (finish aprs_chat entry)
- Test: `src/aprs/useAprsChat.test.ts` (exists — grep it; extend)

**Behavior requirements (spec §5/§7):**
1. **Echo consumption:** subscribe `aprs-message:sent` (payload `{ msgid, addressee, text, at_ms }`). Append as `direction: 'out'`, `from: 'me'`, `to: addressee === '' ? null : addressee`, `state: 'sent'`, `at: at_ms` — **deduped by msgid** (the sending window already appended optimistically in `send`; `setMessages(prev => prev.some(m => m.msgid === payload.msgid) ? prev : [...prev, msg])`). The local optimistic append in `send` stays EXACTLY as is (RF-honesty comment) — the invariant is "reconstructible from events alone," not "events are the only writer" (spec §7).
2. **Snapshot handshake** with the same 250 ms/3 s retry as Task 9, events `aprs-chat:request-snapshot` / `aprs-chat:snapshot`, payload = the full `ChannelMessage[]`; client merges by dedupe on `id` keeping newer `state`. `snapshotRole` option, AppShell = host, popped wrapper = client.
3. **Tab placeholder + ⇤:** while popped, the APRS dock tab content is a placeholder div — text "APRS Chat ↗ — in its own window", subtext "click to focus" → `focusSurface('aprs_chat')`; plus a small "⇤ dock back" link → `dockBack('aprs_chat', { foreground: true, state: null })`. **⇤-foreground arrival** (`context.aprs_chat?.foreground === true` on the docked flip): `setAprsOpen(true)` + `setDockTab('aprs')` (spec §5 — ⇤ activates the tab); `foreground: false` arrival changes neither. Other tabs untouched.
4. **Dock-opening flows (spec §5, adrev R4-F9):** every AppShell path that programmatically opens the dock to reach the APRS strip (the StatusBar listening-switch first-run path and the connect-failure retry path, ~878–932) checks dock state first: popped → `focusSurface('aprs_chat')` instead of opening the dock.

- [ ] **Step 1: Failing tests:** own-send echo dedupe (send in this instance → 1 message despite echo arriving; echo without local append → 1 message with `at` from `at_ms`); delivery-state event applies to an echo-appended message; snapshot retry (as Task 9).
- [ ] **Step 2–4: red → implement → green** (`pnpm vitest run src/aprs/ && pnpm typecheck`, then full `pnpm vitest run`).
- [ ] **Step 5: Commit:**

```bash
git add src/aprs/ src/shell/AppShell.tsx src/dock/surfaceRegistry.tsx
git commit -m "feat(aprschat): pop-out wiring — sent-echo dedupe, snapshot handshake, placeholder + dock-aware flows (spec §5/§7, tuxlink-dmwte task 10)"
```

**REVIEW LOOP: Tasks 8–10 are the fourth logical group — run the 3-round review now.**

---

### Task 11: Render-harness fixtures + WebKitGTK smoke

**Files:**
- Modify: `dev/render-harness/` (read its README.md FIRST — it documents the fixture pattern from the plan-5 smoke, `?view=routines` family)
- Create: fixture families `?view=pop-routines | pop-tacmap | pop-aprschat`, the three vacated-slot main-shell states, and the three docked-state headers showing the ↗ affordance (spec §10 — the affordance is a flex-crush candidate)

**Requirements:** run the harness on the real WebKitGTK engine per the README (the plan-5 lesson: dashboard trigger clipping, 72px control columns, `ch`-unit font-metric traps were ALL invisible to jsdom). Fixture realism rules from the plan-5 smoke: run ids use the real `run-<unixsecs>-<NNNN>` shape; snapshots carry the def. Capture PNGs; fix every render defect found (that is the point of the task, not a follow-up); repeat until clean.

- [ ] **Step 1:** fixtures per family. **Step 2:** WebKitGTK render + PNG review. **Step 3:** fix defects, re-render. **Step 4:** commit fixtures + fixes:

```bash
git add dev/render-harness/ src/
git commit -m "test(render-harness): pop-window + vacated-slot + ↗-affordance fixtures; WebKitGTK smoke fixes (spec §10, tuxlink-dmwte task 11)"
```

---

### Task 12: Memory harness + docs

**Files:**
- Create: `dev/measure-webview-marginal-memory.py` (tracked — the parent spec's `dev/scratch` copy is gone; spec §10). PSS via `/proc/<pid>/smaps_rollup` per the parent §12 measurement note: launch the app, snapshot PSS, pop each surface, snapshot after each, print the marginal deltas. ~80 lines, stdlib only.
- Modify: `docs/user-guide/` — add a "Pop-out windows" section to the appropriate page (grep the guide's structure; likely beside the Routines page added in plan 5): what ↗/⇤/✕ do (⇤ brings it back in front; ✕ puts it away without disturbing your mailbox), that layouts persist, the memory cost in plain terms (parent §12 docs note: state the ~30 MiB class number without defensiveness; update with Task 13's measured map number).
- Modify: `dev/implementation-log.md` — top entry for this feature.

- [ ] Commit:

```bash
git add dev/measure-webview-marginal-memory.py docs/user-guide/ dev/implementation-log.md
git commit -m "docs(dock): user-guide pop-out section + tracked memory harness + implementation log (tuxlink-dmwte task 12)"
```

---

### Task 13: Integration gates — wire-walk, CI, PR, operator live pass

- [ ] **Step 1: Full local gates:** `pnpm typecheck && pnpm vitest run && pnpm build`. All green.
- [ ] **Step 2: Wire-walk (HARD GATE — CLAUDE.md):** invoke the `wire-walk` skill at the integration boundary. The operator supplies flows greenfield; expect them to cover the spec §11-era wire-walk table (pop ×3, dock-back both semantics, find-from-main, consent-while-popped, quit/relaunch, crash recovery, chat continuity). Every flow traces to `file:line` or the feature is NOT done.
- [ ] **Step 3: Push + PR** (draft first; CI compiles the Rust — this Pi does not). PR body: spec pointer, adrev summary pointer (spec §11), the parent-§12 AMD-1/AMD-2 note, test evidence, and the live-pass checklist below. Verify CI green **by head SHA** on both arches before marking ready.
- [ ] **Step 4: Operator live multi-window pass (operator-run, from spec §10):** pop all three; consent-parking dry-run routine; modal placement; badge-click routing both dock states; desktop notification incl. daemon presence check; pathway-click focus/raise/unminimize on labwc AND X11; dock-back mid-park; ⇤-vs-✕ presentation difference; quit/relaunch restoration; live theme change; main-to-tray consent discovery; pop→dock→re-pop churn ×3 surfaces; memory re-measure (`dev/measure-webview-marginal-memory.py`) with the map number recorded into the user guide. **Dry-run only; no transmission.** Fix-forward per house policy; CI-green is the merge gate, the live pass validates on the converged build.
- [ ] **Step 5:** `bd close tuxlink-dmwte` after merge; worktree disposal per ADR 0009; handoff.

---

## Plan self-review record

- **Spec coverage:** §3 registry/wire/commands/focus → T1/T2/T4; §3 helper/capabilities → T3; §3 restoration/logout/crash → T4; §4 host/chrome/strips/theme → T7; §5 pathways/menu/badge/dock-aware flows → T8–T10; §6 consent split/notification/duration/quit-prompt wording → T4/T8; §7 token/echo/handshakes/map → T5/T8–T10; §8 covered by T4 behaviors + T13 live pass; §10 tests distributed per task + T11/T13; §12 sequencing honored (mechanism → Routines → map → chat).
- **Review round 1–2 amendments applied (2026-07-15):** main-side dock-back token rule promoted to Global Constraints (was self-contradictory across T8–T10); `surface_focus` behavior added to T4; T5 rewritten onto the real EventSink topology; T2 tests moved off the nonexistent `Config::default()` onto the `config_json` fixture; `registerGetContext` contract added to T7 + designer draft plumbing to T8; MenuBar/menuModel golden-test work made explicit; chat ⇤-arrival clause added; theme listener covers both localStorage keys; capability emit-grant marked load-bearing (stations.json bug filed as bd issue); crash-wiring precedent corrected to `forms/pdf_export.rs`.
- **Known intentional simplification:** the ⇤/✕ presentation split rides inside the continuity token (`foreground` flag) rather than a separate command parameter — one wire field, AppShell interprets. This IS the spec §5 behavior; implementers must not add a second mechanism.
- **Type consistency check:** `SurfaceId` strings, label/route table, `DockSnapshot` shape, token envelope `{foreground, state}`, `SentMsgDto` fields, and `renderModal` prop are each defined once above and referenced by those exact names in later tasks.

