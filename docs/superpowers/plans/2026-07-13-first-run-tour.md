# First-Run Tour + Spatial Hint System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Post-wizard 5-stop spotlight tour + one-time first-open tips + an Elmer `point_at` tool, all driven by one hint engine (bd tuxlink-10bkw; spec `docs/superpowers/specs/2026-07-13-first-run-tour-design.md`).

**Architecture:** Frontend `HintProvider` (React context in the main window's shell) owns hint state, persistence, scheduling, and keyboard policy; `HintOverlay` renders a 4-panel dark overlay + click-blocking hole cover + popover (NO SVG masks — WebKitGTK). Backend: `OnboardingConfig` section (schema 6→7 with upgrade-cohort sentinel), `config_set_onboarding` setter, and a `point_at` MCP tool that emits `onboarding:point-at` to the main webview and awaits a frontend ack via a keyed oneshot map (new pattern for this codebase — fully specified in Task 3).

**Tech Stack:** Rust (serde, tokio oneshot, rmcp `#[tool]`), React 18 + TypeScript, vitest + @testing-library/react, WebKitGTK render harness.

## Global Constraints

- CONFIG_SCHEMA_VERSION bumps 6→7 (config.rs golden-set test WILL fail until updated — that is the TDD signal, not an obstacle).
- No new frontend dependencies. No SVG masks or `mix-blend-mode` in the overlay.
- The engine never navigates, never opens panels, never fires actions; the spotlight hole is click-blocked.
- ESC = skip = `tour_completed` set. App-quit mid-tour leaves it unset (offer re-appears once).
- Elmer-fired hints never mutate `tips_seen`.
- Overlay z-index: 1200 (existing top tier is 1100 — StationFinderPanel/AprsPositionsMap; tour must sit above them).
- Tauri v2 command args: Rust `snake_case` params are invoked with `camelCase` keys from JS — mirror the casing of an existing `config_set_privacy` call site when writing invoke calls.
- Rust does NOT compile on this Pi (main crate). Write Rust test-first, push, let CI verify (memory: no-cold-cargo). Frontend: `pnpm typecheck` + scoped `pnpm vitest run <paths>` locally; CI runs the full suite + `clippy --all-targets`.
- Commits: conventional type + scope, `Agent: dune-willow-clover` trailer + `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` (heredoc commit messages; commit from inside the worktree `worktrees/bd-tuxlink-10bkw-first-run-tour`).

---

### Task 1: Rust — OnboardingConfig section, schema 7, upgrade-cohort sentinel

**Files:**
- Modify: `src-tauri/src/config.rs` (CONFIG_SCHEMA_VERSION at ~line 22; Config struct ~219-359; custom Deserialize impl ~428-451; golden test ~1892)
- Modify: `src-tauri/src/wizard.rs` (two `Config { ... }` literals: `persist_cms_impl` ~183, offline path ~432)
- Modify: `src-tauri/src/ui_commands.rs` (three test-fixture `Config` literals at ~10538, ~10894, ~12473 — grep `wizard_completed: true` to find all)

**Interfaces:**
- Produces: `pub struct OnboardingConfig { pub tour_completed: bool, pub tips_seen: Vec<String> }`; `Config.onboarding: Option<OnboardingConfig>` (always `Some` after load — `normalize_onboarding()` guarantees it); sentinel value `tips_seen == vec!["*"]` meaning all-tips-seen (upgrade cohort).

- [ ] **Step 1: Write the failing tests** (append to the `#[cfg(test)]` module in config.rs)

```rust
#[test]
fn onboarding_defaults_and_roundtrip() {
    let cfg: Config = serde_json::from_str(&config_json(CONFIG_SCHEMA_VERSION, ""))
        .expect("minimal config deserializes");
    let ob = cfg.onboarding.as_ref().expect("normalize_onboarding fills None");
    assert!(!ob.tour_completed);
    assert!(ob.tips_seen.is_empty(), "fresh profile: no sentinel");
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(json.contains("\"onboarding\""), "always serialized once Some");
}

#[test]
fn pre_v7_wizard_completed_seeds_tip_sentinel() {
    // A v6 file (no onboarding key) from an operator who finished the wizard:
    // migration must seed the all-seen sentinel so months-old surfaces don't tip.
    let raw = config_json(6, "").replace("\"wizard_completed\": false", "\"wizard_completed\": true");
    let cfg: Config = serde_json::from_str(&raw).expect("v6 loads additively");
    let ob = cfg.onboarding.as_ref().unwrap();
    assert!(!ob.tour_completed, "tour offer still shows once for upgraders");
    assert_eq!(ob.tips_seen, vec!["*".to_string()], "sentinel = all tips seen");
}

#[test]
fn pre_v7_wizard_not_completed_gets_clean_default() {
    let raw = config_json(6, "");
    let cfg: Config = serde_json::from_str(&raw).expect("v6 loads additively");
    let ob = cfg.onboarding.as_ref().unwrap();
    assert!(ob.tips_seen.is_empty(), "pre-wizard profile: no sentinel");
}
```

Note: inspect the existing `config_json(version, extra)` test helper first and match its actual shape — if `wizard_completed` isn't in its output as `false`, adapt the replace (or build the JSON explicitly like neighboring tests do).

- [ ] **Step 2: Run the tests to verify they fail**

No local cargo for the main crate. Instead verify failure statically: the field doesn't exist, so this is a compile failure — acceptable evidence. Proceed. (CI is the arbiter; the golden-set test failing on "onboarding" is expected until Step 3 updates it.)

- [ ] **Step 3: Implement**

3a. Bump the constant and extend its doc comment (config.rs ~22):

```rust
/// Bumped 6 → 7 (tuxlink-10bkw): added the always-serialized top-level
/// `onboarding` section (first-run tour + first-open tips). A pre-v7 file
/// lacks the key; load normalizes it to Some, seeding the tips_seen ["*"]
/// sentinel when wizard_completed is already true (upgrade cohort must not
/// get first-open tips on surfaces they have used for months).
pub const CONFIG_SCHEMA_VERSION: u32 = 7;
```

3b. Section struct (place near PrivacyConfig, follow its conventions):

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingConfig {
    /// True once the operator finished, skipped (incl. ESC), or declined the
    /// first-run tour. Help → "Replay tour" is the only reset path (it does
    /// NOT clear this flag; replay just runs the tour again).
    #[serde(default)]
    pub tour_completed: bool,
    /// Tip IDs already shown once. The single value ["*"] is the upgrade-
    /// cohort sentinel: treat every tip as seen (IDs live in the frontend
    /// registry; Rust never enumerates them — that is the point).
    #[serde(default)]
    pub tips_seen: Vec<String>,
}
```

3c. Config field (in the struct, after `wwv_offair`):

```rust
    /// First-run tour / hint state (tuxlink-10bkw). Option so load can
    /// distinguish "absent on disk" (pre-v7 file → normalize_onboarding
    /// seeds the upgrade-cohort sentinel) from a present default section.
    /// Always Some after deserialize; wizard-persist writes Some(default).
    #[serde(default)]
    pub onboarding: Option<OnboardingConfig>,
```

3d. Normalization in the existing custom Deserialize impl (config.rs ~428, alongside `migrate_rig_from_legacy_ardop`):

```rust
        let mut config = Config::deserialize(deserializer)?;
        config.migrate_rig_from_legacy_ardop();
        config.normalize_onboarding();
        Ok(config)
```

and the method next to `migrate_rig_from_legacy_ardop`:

```rust
    /// tuxlink-10bkw: a file written before v7 has no `onboarding` key. If the
    /// wizard was already completed there, the operator predates the tips
    /// system — seed the ["*"] sentinel so first-open tips never fire on
    /// surfaces they have used for months (the tour OFFER still shows once).
    fn normalize_onboarding(&mut self) {
        if self.onboarding.is_none() {
            self.onboarding = Some(if self.wizard_completed {
                OnboardingConfig { tour_completed: false, tips_seen: vec!["*".to_string()] }
            } else {
                OnboardingConfig::default()
            });
        }
    }
```

3e. Golden-set test: add `"onboarding"` to the `expected` vec in `config_schema_version_tracks_field_set` (~1892).

3f. `src-tauri/src/wizard.rs`: both `Config { ... }` literals gain `onboarding: Some(OnboardingConfig::default()),` (import it). `src-tauri/src/ui_commands.rs` test fixtures: same field (grep `wizard_completed: true` for all literals; also grep `Config {` in any other test module — the compiler finds the rest).

- [ ] **Step 4: Verify** — `cargo check` is unavailable locally; run `pnpm typecheck` (unaffected, sanity) and rely on the commit + CI. Re-read your diff once against 3a-3f.

- [ ] **Step 5: Commit** — `feat(config): onboarding section, schema v7, upgrade-cohort tip sentinel (tuxlink-10bkw)`

### Task 2: Rust — config_set_onboarding + ConfigViewDto fields

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (setter near config_set_privacy ~7288; ConfigViewDto ~3464; `From<&Config>` ~3529)
- Modify: `src-tauri/src/lib.rs` (add to `tauri::generate_handler![...]` ~1972)

**Interfaces:**
- Consumes: Task 1's `OnboardingConfig`, `Config.onboarding`.
- Produces: command `config_set_onboarding(tour_completed: bool, tips_seen: Vec<String>)`; `ConfigViewDto` gains `pub onboarding_tour_completed: bool` and `pub onboarding_tips_seen: Vec<String>`.

- [ ] **Step 1: Failing test** (ui_commands.rs test module — follow the neighboring DTO tests' fixture pattern):

```rust
#[test]
fn config_view_dto_carries_onboarding() {
    let mut cfg = test_config(); // whichever existing fixture builder the module uses
    cfg.onboarding = Some(config::OnboardingConfig {
        tour_completed: true,
        tips_seen: vec!["find-a-station".into()],
    });
    let dto = ConfigViewDto::from(&cfg);
    assert!(dto.onboarding_tour_completed);
    assert_eq!(dto.onboarding_tips_seen, vec!["find-a-station".to_string()]);
}
```

- [ ] **Step 2: Implement the DTO fields** — add to ConfigViewDto and to the `From` impl:

```rust
            onboarding_tour_completed: c
                .onboarding
                .as_ref()
                .map(|o| o.tour_completed)
                .unwrap_or(false),
            onboarding_tips_seen: c
                .onboarding
                .as_ref()
                .map(|o| o.tips_seen.clone())
                .unwrap_or_default(),
```

- [ ] **Step 3: Implement the setter** (mirror config_set_privacy exactly — read, mutate, atomic write; onboarding is backend-irrelevant so NO `backend.set_config` refresh):

```rust
/// tuxlink-10bkw: persist tour/tip state. Whole-section set (single-operator
/// app; same last-write-wins posture as the other config_set_* commands).
#[tauri::command]
pub async fn config_set_onboarding(
    tour_completed: bool,
    tips_seen: Vec<String>,
) -> Result<(), UiError> {
    let mut cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    cfg.onboarding = Some(config::OnboardingConfig { tour_completed, tips_seen });
    config::write_config_atomic(&cfg).map_err(|e| UiError::Internal { detail: e.to_string() })?;
    Ok(())
}
```

- [ ] **Step 4: Register** in lib.rs `generate_handler![` (fully-pathed like neighbors: `crate::ui_commands::config_set_onboarding` — match however the other config_set_* entries are pathed).

- [ ] **Step 5: Commit** — `feat(config): config_set_onboarding setter + onboarding fields on ConfigViewDto`

### Task 3: Rust — point_at MCP tool, event, keyed-oneshot ack

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs` (new trait after StatusPort ~691)
- Modify: `src-tauri/tuxlink-mcp-core/src/lib.rs` (McpState gains `pub ui_hint: Arc<dyn UiHintPort>` — grep EVERY McpState construction incl. tests/shim and add the field)
- Modify: `src-tauri/tuxlink-mcp-core/src/router.rs` (tool + params struct)
- Create: `src-tauri/src/onboarding_bridge.rs` (pending-ack state + ack command + emit)
- Modify: `src-tauri/src/mcp_ports.rs` (MonolithUiHintPort adapter)
- Modify: `src-tauri/src/lib.rs` (`mod onboarding_bridge;`, `.manage(...)`, register `onboarding_point_at_ack`, add `ui_hint` to the McpState construction)

**Interfaces:**
- Produces: tool `point_at { anchor_id: String }` returning `{ outcome: "shown" } | structured error`; event `onboarding:point-at` payload `{ request_id: u64, anchor_id: String }`; command `onboarding_point_at_ack(request_id: u64, outcome: String, valid_ids: Option<Vec<String>>)`; frontend ack outcomes: `"shown" | "unknown-anchor" | "anchor-unmounted" | "overlay-busy"`.
- Spec deviation (recorded): anchor validation is FRONTEND-side via the ack (the registry is the single source of truth); the tool relays outcome. The build-time catalog doc for Elmer's knowledge tier is deferred TO THE KNOWLEDGE-TIER feature (bd dep), per spec's degraded mode.

- [ ] **Step 1: Port trait** (ports.rs, mirror StatusPort conventions):

```rust
/// UI spatial-help port (tuxlink-10bkw). point_at NEVER navigates, opens
/// panels, or fires actions — it asks the main webview to spotlight a
/// registered anchor and reports honestly whether that happened.
#[async_trait]
pub trait UiHintPort: Send + Sync {
    /// Ok(()) iff the hint is actually visible. Err carries the outcome:
    /// unknown-anchor (with the valid-ID list), anchor-unmounted (with the
    /// registry's "how to open this surface" line), overlay-busy, timeout.
    async fn point_at(&self, anchor_id: &str) -> Result<(), PortError>;
}
```

- [ ] **Step 2: Router tool** (router.rs, mirror p2p_peer_password_status):

```rust
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PointAtParams {
    /// Anchor ID from the onboarding registries (e.g. "ribbon-connect",
    /// "mailbox", "contacts", "radio-dock", "elmer"). Unknown IDs error
    /// with the valid list.
    pub anchor_id: String,
}
```

```rust
    #[tool(
        name = "point_at",
        description = "Spotlight a UI element in the main window so the operator can see where it is. Never clicks, navigates, or transmits — display only. Errors list valid anchor IDs when the ID is unknown."
    )]
    pub async fn point_at(
        &self,
        params: Parameters<PointAtParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(PointAtParams { anchor_id }) = params;
        self.state.ui_hint.point_at(&anchor_id).await.map_err(port_err)?;
        Ok(CallToolResult::success(vec![ContentBlock::json(serde_json::json!({
            "outcome": "shown", "anchor_id": anchor_id
        }))?]))
    }
```

- [ ] **Step 3: The bridge** (new file `src-tauri/src/onboarding_bridge.rs` — complete):

```rust
//! tuxlink-10bkw: backend→webview point-at bridge. New pattern for this
//! codebase: a keyed pending-ack map (request_id → oneshot) so the MCP tool
//! can await the frontend's honest outcome instead of fire-and-forget
//! (fire-and-forget makes Elmer confidently wrong — spec §Elmer point-at).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::oneshot;

pub const POINT_AT_EVENT: &str = "onboarding:point-at";
pub const ACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, Clone, serde::Serialize)]
pub struct PointAtRequest {
    pub request_id: u64,
    pub anchor_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PointAtAck {
    pub outcome: String, // "shown" | "unknown-anchor" | "anchor-unmounted" | "overlay-busy"
    pub valid_ids: Option<Vec<String>>,
    pub open_hint: Option<String>, // registry's "how to open this surface" line
}

#[derive(Default)]
pub struct PointAtPending {
    next_id: AtomicU64,
    waiting: Mutex<HashMap<u64, oneshot::Sender<PointAtAck>>>,
}

impl PointAtPending {
    pub fn register(&self) -> (u64, oneshot::Receiver<PointAtAck>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.waiting.lock().expect("point-at map poisoned").insert(id, tx);
        (id, rx)
    }
    pub fn resolve(&self, id: u64, ack: PointAtAck) -> bool {
        match self.waiting.lock().expect("point-at map poisoned").remove(&id) {
            Some(tx) => tx.send(ack).is_ok(),
            None => false, // late ack after timeout cleanup — ignored
        }
    }
    pub fn forget(&self, id: u64) {
        self.waiting.lock().expect("point-at map poisoned").remove(&id);
    }
}

/// Frontend ack for a point-at request. Late acks (post-timeout) are no-ops.
#[tauri::command]
pub fn onboarding_point_at_ack(
    request_id: u64,
    outcome: String,
    valid_ids: Option<Vec<String>>,
    open_hint: Option<String>,
    pending: tauri::State<'_, std::sync::Arc<PointAtPending>>,
) -> Result<(), ()> {
    pending.resolve(request_id, PointAtAck { outcome, valid_ids, open_hint });
    Ok(())
}
```

Unit tests in the same file (these DO run in CI): register→resolve roundtrip delivers the ack; resolve of an unknown id returns false; forget prevents a later resolve.

- [ ] **Step 4: Adapter** (mcp_ports.rs, mirror MonolithStatusPort):

```rust
pub struct MonolithUiHintPort {
    app: AppHandle,
}
impl MonolithUiHintPort {
    pub fn new(app: AppHandle) -> Self { Self { app } }
}

#[async_trait]
impl UiHintPort for MonolithUiHintPort {
    async fn point_at(&self, anchor_id: &str) -> Result<(), PortError> {
        use tauri::Emitter as _;
        let pending = self.app.state::<std::sync::Arc<crate::onboarding_bridge::PointAtPending>>();
        let (id, rx) = pending.register();
        let req = crate::onboarding_bridge::PointAtRequest {
            request_id: id,
            anchor_id: anchor_id.to_string(),
        };
        if self.app.emit(crate::onboarding_bridge::POINT_AT_EVENT, &req).is_err() {
            pending.forget(id);
            return Err(PortError::Internal("point-at emit failed".into()));
        }
        match tokio::time::timeout(crate::onboarding_bridge::ACK_TIMEOUT, rx).await {
            Ok(Ok(ack)) if ack.outcome == "shown" => Ok(()),
            Ok(Ok(ack)) => {
                let mut msg = format!("point_at not shown: {}", ack.outcome);
                if let Some(ids) = ack.valid_ids {
                    msg.push_str(&format!("; valid anchor ids: {}", ids.join(", ")));
                }
                if let Some(h) = ack.open_hint {
                    msg.push_str(&format!("; to make it visible: {h}"));
                }
                Err(PortError::Internal(msg))
            }
            Ok(Err(_)) | Err(_) => {
                pending.forget(id);
                Err(PortError::Internal(
                    "point_at timed out — main window did not confirm the hint (window closed/minimized, or overlay unresponsive)".into(),
                ))
            }
        }
    }
}
```

(Match `PortError`'s actual variant/constructor — check how other adapters build it, incl. `redact_err` usage.)

- [ ] **Step 5: Wire** — lib.rs: `mod onboarding_bridge;`; `.manage(std::sync::Arc::new(crate::onboarding_bridge::PointAtPending::default()))`; add `onboarding_point_at_ack` to generate_handler; add `ui_hint: Arc::new(MonolithUiHintPort::new(app_handle.clone()))` wherever McpState is constructed (grep `McpState {` — main app AND the external shim binary AND mcp-core tests; every construction site must gain the field or the workspace won't compile).

- [ ] **Step 6: Commit** — `feat(mcp): point_at tool — spotlight UI anchors from Elmer with honest ack semantics`

### Task 4: Frontend — types, registries, pure scheduling logic

**Files:**
- Create: `src/onboarding/types.ts`, `src/onboarding/tourRegistry.ts`, `src/onboarding/hintRegistry.ts`, `src/onboarding/tipLogic.ts`
- Test: `src/onboarding/tipLogic.test.ts`, `src/onboarding/registries.test.ts`

**Interfaces (produced — later tasks depend on these exact names):**

```ts
// types.ts
export type HintFallback = 'skip' | 'center';
export interface HintEntry {
  id: string;                    // anchor id == entry id
  anchor: string;                // data-tour-anchor attribute value (same as id)
  title: string;
  body: string;
  requiredPanelState?: string;   // key into the probe registry; absent = always ok
  fallback: HintFallback;
  openHint?: string;             // "how to open this surface" — point_at unmounted error
}
export type PanelStateProbe = () => boolean;
```

- [ ] **Step 1: Failing tests for tip logic** (`tipLogic.test.ts`):

```ts
import { describe, it, expect } from 'vitest';
import { isTipSeen, markTipSeen } from './tipLogic';

describe('tip sentinel logic', () => {
  it('empty list: nothing seen', () => {
    expect(isTipSeen([], 'find-a-station')).toBe(false);
  });
  it('listed tip is seen', () => {
    expect(isTipSeen(['find-a-station'], 'find-a-station')).toBe(true);
  });
  it('["*"] sentinel: everything seen (upgrade cohort)', () => {
    expect(isTipSeen(['*'], 'anything')).toBe(true);
  });
  it('markTipSeen is idempotent and preserves the sentinel', () => {
    expect(markTipSeen(['*'], 'x')).toEqual(['*']);
    expect(markTipSeen(['a'], 'a')).toEqual(['a']);
    expect(markTipSeen(['a'], 'b')).toEqual(['a', 'b']);
  });
});
```

- [ ] **Step 2: Run** `pnpm vitest run src/onboarding/tipLogic.test.ts` → FAIL (module missing).

- [ ] **Step 3: Implement** `tipLogic.ts`:

```ts
export function isTipSeen(tipsSeen: readonly string[], id: string): boolean {
  return tipsSeen.includes('*') || tipsSeen.includes(id);
}
export function markTipSeen(tipsSeen: readonly string[], id: string): string[] {
  if (isTipSeen(tipsSeen, id)) return [...tipsSeen];
  return [...tipsSeen, id];
}
```

- [ ] **Step 4: Registries.** `tourRegistry.ts` — the 5 stops, anchors chosen from scouted reality (fallbacks are DESIGNED behavior, not errors):

```ts
import type { HintEntry } from './types';

/** Order matters — this IS the tour. Anchors are placed in Task 6. */
export const TOUR_STOPS: HintEntry[] = [
  {
    id: 'ribbon-connect', anchor: 'ribbon-connect',
    title: 'Connect',
    body: 'One click runs your last-configured session — dial, exchange mail, disconnect. Nothing transmits until you click it.',
    fallback: 'center',
    openHint: 'The Connect button lives at the right end of the status ribbon.',
  },
  {
    id: 'mailbox', anchor: 'mailbox',
    title: 'Mailbox',
    body: 'Messages land here after a connect. Folders on the left; unread counts on the ribbon.',
    requiredPanelState: 'mailbox-visible', fallback: 'skip',
    openHint: 'Select any mail folder in the left sidebar.',
  },
  {
    id: 'contacts', anchor: 'contacts',
    title: 'Contacts',
    body: 'The one address surface: everyone you know, star Favorites, and stations you have heard.',
    fallback: 'center',
    openHint: 'Open the Contacts folder in the left sidebar.',
  },
  {
    id: 'radio-dock', anchor: 'radio-dock',
    title: 'Radio dock',
    body: 'When you start a radio mode (ARDOP, VARA, packet), its panel docks here — arming a listener, session status, and the dial all live in it.',
    requiredPanelState: 'radio-dock-open', fallback: 'center',
    openHint: 'Pick a radio mode from the ribbon to open its dock panel.',
  },
  {
    id: 'elmer', anchor: 'elmer',
    title: 'Elmer',
    body: 'Your built-in assistant. Ask it anything about the app or the hobby — try: where do I connect?',
    fallback: 'center',
    openHint: 'Elmer opens from its button on the status ribbon.',
  },
];
```

`hintRegistry.ts` — 4 tips (`find-a-station`, `aprs`, `settings`, `compose`), same shape, each with `fallback: 'skip'` and real copy (write it — one sentence of what the surface does, one of what to try first). `registries.test.ts`: IDs unique across BOTH registries; every entry has non-empty title/body; no entry id contains `*`.

- [ ] **Step 5: Run both test files** → PASS. **Commit** — `feat(onboarding): registries + tip sentinel logic (5 stops, 4 tips)`

### Task 5: Frontend — HintProvider (state machine, persistence, scheduling, keyboard)

**Files:**
- Create: `src/onboarding/HintProvider.tsx`
- Test: `src/onboarding/HintProvider.test.tsx`

**Interfaces (produced):**

```ts
interface HintContextValue {
  active: null | { kind: 'offer' } | { kind: 'tour'; stepIndex: number } | { kind: 'single'; entry: HintEntry; source: 'tip' | 'point-at' };
  startTour(): void;            // offer accept + Help replay
  advance(): void; back(): void;
  skipTour(): void;             // ESC / Skip — sets tour_completed
  declineOffer(): void;         // sets tour_completed
  dismissSingle(): void;        // tip: marks seen; point-at: no mutation
  requestFirstOpenTip(id: string): void;   // consumed by useFirstOpenTip
  registerProbe(key: string, probe: PanelStateProbe): () => void;
  overlayActive: boolean;
}
export function useHints(): HintContextValue;
export function useFirstOpenTip(id: string): void;  // fires requestFirstOpenTip on mount
```

Behavior contract (each bullet = one test):
1. On mount, reads `config_read`; when `onboarding_tour_completed === false` → `active = { kind: 'offer' }`. When true → `active = null`.
2. `declineOffer()` / `skipTour()` / finishing the last stop → invoke `config_set_onboarding` with `tourCompleted: true` and the CURRENT tipsSeen (arg casing: mirror a real `config_set_privacy` call site); on invoke rejection, state still advances (session flag) and the error goes to `console.error` + structured log if a helper exists.
3. `requestFirstOpenTip(id)`: shows iff `!isTipSeen(tipsSeen, id)` AND `active === null` (suppressed-not-consumed: if busy, do nothing — stays eligible).
4. `dismissSingle()` for a tip persists `markTipSeen(...)`; for point-at, persists nothing.
5. Capture-phase `window` keydown registered ONCE in a provider-mount effect with `{ capture: true }`: when `overlayActive`, swallow everything except the tour keys (ESC → skip/dismiss; ArrowRight/Enter → advance; ArrowLeft → back) — `preventDefault()` + `stopPropagation()`. When inactive: no-op passthrough. (Grep first: `grep -rn "addEventListener('keydown'" src/ | grep -i capture` — if any window-level capture listener exists, flag it in the PR body; scouts found none.)
6. `listen('onboarding:point-at', ...)` (canonical mounted/unlisten cleanup pattern from `useInboundSelection.ts`): on event, look up the id across BOTH registries → unknown: ack `{ outcome: 'unknown-anchor', validIds: [...] }`; known but probe false or anchor element missing: ack `{ outcome: 'anchor-unmounted', openHint }`; overlay busy with a TOUR (offer/tour kinds): ack `{ outcome: 'overlay-busy' }` (a point-at may replace an earlier point-at/tip); else show `{ kind: 'single', source: 'point-at' }` and ack `{ outcome: 'shown' }`. Ack via `invoke('onboarding_point_at_ack', { requestId, outcome, validIds, openHint })`.

- [ ] **Step 1: Write the failing tests** — one `it` per numbered bullet above. Use the ContactsPanel.test.tsx conventions verbatim: `vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }))`, `vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))`, fresh QueryClient per render, `beforeEach` `vi.mocked(invoke).mockReset()`. For bullet 6, capture the listener callback: `vi.mocked(listen).mockImplementation(async (evt, cb) => { captured = cb; return () => {}; })` then invoke `captured({ payload: { request_id: 1, anchor_id: 'nope' } })` and assert the ack invoke.
- [ ] **Step 2: Run → FAIL.** `pnpm vitest run src/onboarding/HintProvider.test.tsx`
- [ ] **Step 3: Implement HintProvider.tsx** to the contract. Keep it ONE file, ~250 lines; reducer for the state machine; `useRef` for probes map.
- [ ] **Step 4: Run → PASS**, then `pnpm typecheck`.
- [ ] **Step 5: Commit** — `feat(onboarding): HintProvider — offer/tour/single state machine, point-at ack, capture-phase key policy`

### Task 6: Frontend — HintOverlay (geometry, a11y) + CSS + anchors + probes + shell wiring

**Files:**
- Create: `src/onboarding/HintOverlay.tsx`, `src/onboarding/HintOverlay.css`, `src/onboarding/OfferCard.tsx`
- Test: `src/onboarding/HintOverlay.test.tsx`
- Modify: `src/shell/AppShell.tsx` (mount provider + overlay + offer; register probes; menu handler)
- Modify: `src/shell/DashboardRibbon.tsx` (`data-tour-anchor="ribbon-connect"` on the connect Button ~598; `data-tour-anchor="elmer"` on the elmer launcher ~277)
- Modify: `src/mailbox/MessageList.tsx` (`data-tour-anchor="mailbox"` on rows-pane ~486)
- Modify: `src/mailbox/FolderSidebar.tsx` (`data-tour-anchor="contacts"` on the desktop `folder-contacts` button ~871)
- Modify: `src/shell/RadioDrawer.tsx` (`data-tour-anchor="radio-dock"` on root ~48)
- Modify: `src/shell/chrome/menuModel.ts` (Help submenu: `{ id: 'menu:help:replay_tour', label: 'Replay tour' }` after `menu:help:docs`)
- Modify: `src/shell/chrome/dispatchMenuAction.ts` (`replayTour` handler + case)
- Modify first-open surfaces: `src/catalog/StationFinderPanel.tsx`, the APRS dock surface component, `src/shell/SettingsPanel.tsx`, and the mailbox compose-affordance host — each adds one `useFirstOpenTip('<id>')` call.

**Overlay implementation requirements (each is a test or a render-harness fixture):**
- Four `position:fixed` divs computed from `getBoundingClientRect()` + a fifth transparent div exactly over the hole with `pointer-events:auto` (click-block). Background `color-mix(in srgb, var(--bg) 78%, transparent)`; z-index 1200 (panels), 1201 (blocker), 1202 (popover).
- Tracking: `window resize` + `scroll` (capture, so ancestor scrolls are caught: `window.addEventListener('scroll', reposition, true)`) + `ResizeObserver` on the target + one `requestAnimationFrame` retry after mount for late layout.
- Popover: `role="dialog"` `aria-modal="false"` `aria-labelledby`/`aria-describedby`; focus trapped (Tab cycles its buttons), previous focus restored on close; `aria-live="polite"` region announcing "Step N of 5: {title}". Collision: flip above/below, clamp horizontally to viewport with 8px margin.
- Tour mode: title, body, "Step N of 5", Back (hidden on 1), Next / "Finish" (last), "Skip tour". Single-hint mode: title, body, "Got it".
- `fallback: 'center'` renders the popover centered, no panels, no blocker.
- OfferCard: fixed bottom-right, `role="status"`, [Start tour] [No thanks], never steals focus.
- CSS: tokens only (`--surface`, `--border-strong`, `--text`, `--accent`, `--radius-panel`, `--type-body`…), both themes get render fixtures.

**Component test (HintOverlay.test.tsx):** render a fake anchor `<button data-tour-anchor="x">`, drive a minimal provider harness; assert: 5 overlay divs present with blocker over the hole rect; dialog a11y attributes; focus lands on Next and returns after dismiss; center-fallback renders no panels; clicking the blocker does not click the anchored button (spy stays uncalled).

**AppShell wiring:** `<HintProvider>` wraps `<AppShellInner />` inside `AppShell()` next to `Ft8ListenerProvider` (main window only — App.tsx already routes other windows away). Probes registered in an AppShellInner effect: `mailbox-visible` → `selectedFolder !== 'contacts' && selectedFolder !== 'favorites'`; `radio-dock-open` → `radioPanelMode !== null || aprsOpen`. Menu: model entry + dispatch case + `replayTour: () => hints.startTour()` in the handlers memo. The parity test for menu ids (`menuModel.test.ts` + Rust `menu_event_ids`) will fail if the Rust side needs the id added — follow whatever that test's failure output says (grep `menu_event_ids` in src-tauri if it fails).

- [ ] Steps: failing overlay test → implement overlay+CSS → pass → wire shell + anchors + menu (typecheck + run `src/shell` and `src/onboarding` scoped vitest; the menu parity test tells you if Rust needs the id) → commit `feat(onboarding): spotlight overlay, offer card, anchors, probes, Replay-tour menu`. Split into two commits if the shell wiring diff gets large (overlay first, wiring second).

### Task 7: Render-harness fixtures + WebKitGTK renders (OPERATOR GATE)

**Files:**
- Create: `dev/render-harness/onboarding.html` (or extend harness.tsx route pattern — follow `?view=finder` precedent from the QA-r3 session)
- Fixtures: offer card; stop 1 (ribbon-connect spotlight); stop 4 center-fallback; a first-open tip; point-at single-hint — each in default dark AND light, 1366×800 + one compact 1024×768.

Render via `WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe python3 dev/render-harness/snapshot.py <url> <png> 1366 800 2500`. Copy PNGs to `dev/scratch/tour-renders/` (absolute path in the approval request). **STOP: operator approves renders before the PR is marked ready** (render-first discipline; the operator is in-session today — ask via AskUserQuestion with the PNG paths).

### Task 8: Verification + PR + Codex adrev

- [ ] `pnpm typecheck` && `pnpm vitest run src/onboarding src/shell src/mailbox` (scoped; CI runs full + clippy --all-targets).
- [ ] Push branch, open PR titled `[dune-willow-clover] feat(onboarding): first-run tour + spatial hint system (tuxlink-10bkw)`; body: spec pointer, the two recorded spec deviations (frontend-side validation; catalog deferred to knowledge tier WITH bd dep edge `bd dep add tuxlink-10bkw <knowledge-tier-id>` — look it up: `bd list | grep -i knowledge`), render PNGs, CI = the compile gate for Rust.
- [ ] Codex adversarial round (build-robust-features requirement): stdin-pattern `codex review` against the diff, attack angles: config migration correctness (v6→v7, sentinel), ack-map races (timeout vs late ack), overlay event-swallowing regressions (does the capture handler eat typing in app inputs when inactive?), focus-trap escapes. Transcript to `dev/adversarial/2026-07-13-first-run-tour-codex.md` (gitignored); findings + dispositions into the PR body. Apply P1/P2 fixes before merge-on-green.

---

## Self-Review (performed at write time)

- Spec coverage: offer card ✓ (T6), 5 stops ✓ (T4/T6), tips ✓ (T4/T5/T6), ESC-as-skip ✓ (T5.5), config real shape ✓ (T1/T2), sentinel ✓ (T1), point_at + ack ✓ (T3/T5), a11y ✓ (T6), theme fixtures ✓ (T7), Replay tour ✓ (T6), z-index above 1100 ✓, click-blocking hole ✓ (T6 test), no-navigation invariant ✓ (T3 doc + T5.6), Compose separate-window scoping ✓ (tip anchors on main-window affordance).
- Spec deviations recorded: (1) anchor validation frontend-side via ack — the registry is the only source of truth, no Rust duplication; (2) build-time Elmer catalog doc deferred to the knowledge-tier feature with a bd dep edge — spec's own degraded mode makes point_at fully usable meanwhile. Both are operator-visible in the PR body.
- Type consistency: `HintEntry`/`PanelStateProbe` (T4) consumed by T5/T6; ack outcome strings identical in T3 Rust and T5.6; `onboarding_tour_completed`/`onboarding_tips_seen` DTO names consistent T2→T5.1.
- Known softness (flagged, not hidden): exact fixture-builder names in Rust test modules (`test_config()`, `config_json`) must be matched to what exists; the menu-id parity test may require a Rust-side list update — both are discovery steps named in their tasks with the grep to run.
