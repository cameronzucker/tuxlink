# Winlink Link Layer on the APRS Map — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a toggleable "Winlink links" layer to the existing APRS theater-of-ops map that plots recently-called Winlink gateways (recency-windowed, diamond icons) and animates the live ARDOP connection as a truthful, protocol-aware RF-path arc.

**Architecture:** A new Rust command surfaces recently-called gateways (with their stored grid) from the favorites store. The frontend plots them as CSP-safe `divIcon` diamonds in a Leaflet `LayerGroup` (mirroring `OperatorPin`), and animates the *live* link on a sibling Canvas2D layer that copies `DigipeatPathLayer`'s proven canvas/rAF/projection pattern but draws a truthful-now grammar driven by the 4 Hz `modem:status` event. A single persisted boolean toggles both the diamonds and the arc. Every visual piece is split into a **pure, unit-tested core** (mapping inputs→draw state) and a **thin imperative shell** (Leaflet/Canvas DOM) that is grim-smoked, mirroring `digipeatAnim.ts` vs `DigipeatPathLayer.tsx`.

**Tech Stack:** Rust + Tauri 2.x (backend command), React 18 + TypeScript + Leaflet (frontend), Vitest (TS unit tests), `cargo test` (Rust unit tests, compiled by CI — this Pi does not finish a cold cargo build, per CLAUDE.md).

**Design source:** `docs/design/2026-06-22-winlink-map-layer-design.md` (APPROVED, copied into this branch). bd issue: tuxlink-s1o1.

## Adversarial-review dispositions (Codex `gpt-5.5` xhigh + self-verify, 2026-06-22)

Transcript: `dev/adversarial/2026-06-22-winlink-plan-codex.md` (gitignored; noisy — Codex spent its budget reading source). Concrete findings, all reconciled against real code and fixed below:
- **VERIFIED CORRECT:** `gridToLatLon` returns `LatLon { lat; lon }` (`src/forms/position/maidenhead.ts:11-13`) — Task 4's `ll.lat/ll.lon` is right; Leaflet still wants `.lng` (Task 7).
- **FIXED (Task 1 test construction):** `FavoritesStore` has **NO `Default`** (`store.rs:740` guards against it) and `file` is **private** (read via `fn file()/favorites()/log()`). The original `FavoritesStore::default()` + `store.file.push(..)` would not compile. Construct via `FavoritesStore::open(tempdir path)`. Because `record_attempt` stamps `ts_local = now` (can't inject a historical timestamp), the recency-window test MUST use the **JSON-fixture-open idiom**: write a `stations.json` with favorites + log carrying controlled `ts_local`, then `FavoritesStore::open(path)` — mirror the existing `unknown_top_level_field_tolerated` test in `store.rs`. The `recent_gateways` method body itself may read `self.file.favorites`/`self.file.log` (valid inside the impl) or the `self.favorites()/self.log()` accessors.
- **FIXED (design doc was absent on this branch):** copied `docs/design/2026-06-22-winlink-map-layer-design.md` into this worktree branch (it was committed on `recover-handoffs`, not here).

## Global Constraints

- **Base:** worktree off `origin/main` (`64ce6390`, = v0.74.1). The reuse surface (LeafletMap, AprsPositionsMap, DigipeatPathLayer, StationFinderMap, useModemStatus, favorites store) is present on this base — verified.
- **Rust MSRV 1.75** (`src-tauri/Cargo.toml`). Clippy denies `incompatible_msrv` — do NOT use APIs stabilized in 1.76+ (e.g. `Result::inspect_err`). Use pre-1.76 idioms.
- **Rust CI gate:** `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` and `cargo test --manifest-path src-tauri/Cargo.toml --locked`. Clippy traps to pre-empt: `io::Error::other` over `Error::new(ErrorKind::Other, _)`, `.is_some_and(..)` over `.map_or(false, ..)`, no `is_none_or` (>MSRV). **Do not cold-build cargo locally** — write code + tests, rely on CI (open draft PR early).
- **TS gates:** `pnpm typecheck`, `pnpm vitest run`, `pnpm build`. Run `pnpm install` first in a fresh worktree.
- **Serde conventions (TWO in this codebase, do not mix them up):** the favorites/`Favorite` struct serializes **snake_case (no `rename_all`)** → the new command's response DTO fields are snake_case on the wire. The `ModemStatus` struct serializes **camelCase** (`#[serde(rename_all = "camelCase")]`) → its TS mirror uses camelCase. Command *arguments* are auto-camelCased by Tauri regardless.
- **CSP (production):** a Leaflet `divIcon`'s `html` is parsed, and the production Tauri CSP nonces `style-src` making `'unsafe-inline'` inert → **parsed inline `style="..."` attributes are dropped**. NEVER size/color a divIcon via inline `style`; use a CSS **class** (or presentational `width`/`height` attrs for `<img>`). This caused the v0.74.1 huge-sprite bug.
- **RADIO-1 (ADR 0018):** this feature is a read-only consumer of `modem:status` telemetry; it touches NO transmit path. No airtime caps / TOT / consent modals to add.
- **Truthful-now grammar:** animate ONLY real telemetry. NO ack/retry visuals (not exposed to frontend → would be fabricated). Those are deferred (tuxlink-g8h9).
- **Commit discipline:** conventional commits; every commit body ends with `Agent: esker-oak-butte` + the `Co-Authored-By:` trailer (commit-msg hook enforces the moniker).

---

## File Structure

| File | Create/Modify | Responsibility |
|---|---|---|
| `src-tauri/src/favorites/store.rs` | Modify | Add `recent_gateways(within_hours)` store method (pure, unit-tested) |
| `src-tauri/src/contacts/commands.rs` | Modify | Add `contacts_recent_gateways` Tauri command + `RecentGatewayPin` DTO |
| `src-tauri/src/lib.rs` | Modify | Register the new command in `generate_handler!` |
| `src/winlink/recentGateways.ts` | Create | TS mirror type `RecentGatewayPin` + `useRecentGateways` hook |
| `src/winlink/winlinkPins.ts` | Create | **Pure** recency→tier/style mapping (unit-tested) |
| `src/winlink/WinlinkGatewayLayer.tsx` | Create | Diamond `divIcon` markers in a LayerGroup (mirrors `OperatorPin`) |
| `src/winlink/WinlinkGatewayLayer.css` | Create | CSP-safe diamond pin classes (tier colors) |
| `src/winlink/winlinkLinkAnim.ts` | Create | **Pure** `modem:status`→link-draw-state mapping (unit-tested) |
| `src/winlink/WinlinkLinkLayer.tsx` | Create | Sibling Canvas2D arc (copies DigipeatPathLayer shell) |
| `src/winlink/useWinlinkLayerToggle.ts` | Create | Persisted on/off boolean for the layer |
| `src/aprs/AprsLayersPanel.tsx` | Modify | Add a "Winlink links" overlay toggle row |
| `src/aprs/AprsPositionsMap.tsx` | Modify | Mount the two Winlink layers + wire the toggle + hover popup |

**Mount point:** `AprsPositionsMap.tsx` return block (~line 598-615) already composes `<AprsLayersPanel>`, `<LeafletMap>` with `<OperatorPin>`, `<WxSitrepControl>`, `<DigipeatPathLayer>`, the markers, `<LeafletRecenterControl>`. The two new layers mount as siblings inside `<LeafletMap>`, gated by the toggle boolean.

---

## Task 1: Rust store method `recent_gateways(within_hours)`

**Files:**
- Modify: `src-tauri/src/favorites/store.rs` (add method on `FavoritesStore` near `attempts_for_gateway` ~line 433; add `#[cfg(test)]` tests at the file's test module)

**Interfaces:**
- Consumes: `FavoritesStore.file.favorites: Vec<Favorite>` (each has `gateway: String`, `grid: Option<String>`, `id: String`), `FavoritesStore.file.log: Vec<ConnectionAttempt>` (`unit_id: String`, `ts_local: String` RFC3339-offset, `outcome: String` `"reached"|"failed"`).
- Produces: `pub fn recent_gateways(&self, within_hours: u32, now: chrono::DateTime<chrono::FixedOffset>) -> Vec<RecentGateway>` where
  ```rust
  pub struct RecentGateway {
      pub gateway: String,
      pub grid: Option<String>,
      pub last_attempt_at: String, // RFC3339, the most-recent attempt in window
      pub outcome: String,         // outcome of that most-recent attempt
  }
  ```
  Semantics: for each favorite, find its attempts (via `unit_id`→favorite `id`, gateway match — reuse the `attempts_for_gateway` join logic), keep the single most-recent attempt whose `ts_local` is within `within_hours` of `now`; emit one `RecentGateway` per gateway that has such an attempt. `grid` passes through from the favorite (may be `None`; the frontend drops `None`). `now` is injected (not `Local::now()`) so the test is deterministic.

- [ ] **Step 1: Write the failing test** (add to the `#[cfg(test)]` mod in `store.rs`). **Construction (corrected — see dispositions):** `FavoritesStore` has no `Default` and a private `file`; build via `FavoritesStore::open(<tempdir>/stations.json)` from a **JSON fixture** carrying controlled `ts_local` (because `record_attempt` stamps `now`). Mirror the existing `unknown_top_level_field_tolerated` test for the fixture-write+open shape. First READ that test + the `StationsFile`/`Favorite`/`ConnectionAttempt` serde field names in `store.rs` to get the exact JSON keys (snake_case: `unit_id`, `ts_local`, `outcome`, `gateway`, `grid`, `schema_version`, etc.). Sketch:

```rust
#[test]
fn recent_gateways_returns_in_window_most_recent_with_grid() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-06-22T12:00:00-07:00").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stations.json");
    // Write a fixture with: W6DRZ (grid CM97) two attempts — newest 11:30 (in 6h),
    // older 09:00 failed; AI6BX (grid CM98) one attempt at 04:00 (OUTSIDE 6h).
    // Match the EXACT serde shape of StationsFile (read store.rs first).
    std::fs::write(&path, r#"{ "schema_version": 1, "favorites": [ /* ...W6DRZ id=u1 grid=CM97, AI6BX id=u2 grid=CM98... */ ], "log": [ /* ...attempts with unit_id+ts_local+outcome... */ ] }"#).unwrap();
    let store = FavoritesStore::open(path);

    let got = store.recent_gateways(6, now);
    assert_eq!(got.len(), 1, "only W6DRZ is within the 6h window");
    assert_eq!(got[0].gateway, "W6DRZ");
    assert_eq!(got[0].last_attempt_at, "2026-06-22T11:30:00-07:00");
    assert_eq!(got[0].outcome, "reached", "most-recent attempt's outcome");
    assert_eq!(got[0].grid.as_deref(), Some("CM97"));
}
```
(Fill the JSON arrays with the real field names from `store.rs`. If `tempfile` isn't a dev-dependency, check `src-tauri/Cargo.toml` — the existing tests use `tempdir()`, so it is.)

- [ ] **Step 2: Run test to verify it fails** — `cargo test --manifest-path src-tauri/Cargo.toml recent_gateways -- --nocapture` Expected: FAIL (method not found). **If the Pi cannot finish the cold build, skip running and rely on CI; mark this step done with a note "deferred to CI" — do NOT block.**

- [ ] **Step 3: Write minimal implementation** (define `RecentGateway` near `ConnectionAttempt`; add the method)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentGateway {
    pub gateway: String,
    pub grid: Option<String>,
    pub last_attempt_at: String,
    pub outcome: String,
}

impl FavoritesStore {
    /// Gateways with at least one connection attempt within `within_hours` of
    /// `now`, each carrying its most-recent in-window attempt + the favorite's
    /// stored grid. `now` is injected for deterministic tests.
    pub fn recent_gateways(
        &self,
        within_hours: u32,
        now: chrono::DateTime<chrono::FixedOffset>,
    ) -> Vec<RecentGateway> {
        use std::collections::HashMap;
        let cutoff = now - chrono::Duration::hours(within_hours as i64);
        // unit_id -> (gateway, grid)
        let units: HashMap<&str, (&str, Option<&str>)> = self
            .file
            .favorites
            .iter()
            .map(|f| (f.id.as_str(), (f.gateway.as_str(), f.grid.as_deref())))
            .collect();
        // gateway -> most-recent in-window attempt
        let mut best: HashMap<&str, (&ConnectionAttempt, chrono::DateTime<chrono::FixedOffset>, Option<&str>)> = HashMap::new();
        for a in &self.file.log {
            let Some((gw, grid)) = units.get(a.unit_id.as_str()).copied() else { continue };
            let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&a.ts_local) else { continue };
            if ts < cutoff { continue; }
            match best.get(gw) {
                Some((_, prev_ts, _)) if *prev_ts >= ts => {}
                _ => { best.insert(gw, (a, ts, grid)); }
            }
        }
        let mut out: Vec<RecentGateway> = best
            .into_iter()
            .map(|(gw, (a, _ts, grid))| RecentGateway {
                gateway: gw.to_string(),
                grid: grid.map(|g| g.to_string()),
                last_attempt_at: a.ts_local.clone(),
                outcome: a.outcome.clone(),
            })
            .collect();
        out.sort_by(|x, y| y.last_attempt_at.cmp(&x.last_attempt_at)); // newest first, stable
        out
    }
}
```

- [ ] **Step 4: Run test to verify it passes** — same command as Step 2 (or defer to CI per the note).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/favorites/store.rs
git commit -m "feat(winlink-map): recent_gateways store query (tuxlink-s1o1)

Returns gateways called within a recency window with their stored grid +
most-recent in-window outcome. now injected for deterministic tests.

Agent: esker-oak-butte
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

**BEFORE marking complete:** review tests against `docs/pitfalls/testing-pitfalls.md`; confirm edge cases covered: empty log, attempt exactly at cutoff (boundary), favorite with `grid: None` still returned, multiple favorites same gateway. Add a test for the `grid: None` passthrough and the boundary case.

---

## Task 2: Tauri command `contacts_recent_gateways`

**Files:**
- Modify: `src-tauri/src/contacts/commands.rs` (add command + re-export `RecentGateway` as the response item, mirroring `contacts_connection_record` at ~line 315)
- Modify: `src-tauri/src/lib.rs` (register in `generate_handler!` next to `contacts_connection_record` ~line 1454)

**Interfaces:**
- Consumes: `FavoritesStore.recent_gateways` (Task 1); `tauri::State<Arc<Mutex<FavoritesStore>>>`.
- Produces: command `contacts_recent_gateways(within_hours: u32) -> Result<Vec<RecentGateway>, ContactsError>`. Arg `within_hours` arrives camelCased from JS as `{ withinHours }`. Response items serialize snake_case (`gateway`, `grid`, `last_attempt_at`, `outcome`).

- [ ] **Step 1: Write the failing test** (in `commands.rs` test mod — exercise the command body via a constructed `FavoritesStore` if the existing tests do; otherwise unit-test is covered by Task 1 and this step asserts registration compiles — write a thin test that builds the DTO list through `recent_gateways` and checks JSON shape with `serde_json::to_value`)

```rust
#[test]
fn recent_gateways_serializes_snake_case() {
    let rg = crate::favorites::store::RecentGateway {
        gateway: "W6DRZ".into(), grid: Some("CM97".into()),
        last_attempt_at: "2026-06-22T11:30:00-07:00".into(), outcome: "reached".into(),
    };
    let v = serde_json::to_value(&rg).unwrap();
    assert!(v.get("last_attempt_at").is_some(), "snake_case on the wire");
    assert!(v.get("lastAttemptAt").is_none());
}
```

- [ ] **Step 2: Run/fail** — `cargo test ... recent_gateways_serializes_snake_case` (defer to CI if cold-build won't finish).

- [ ] **Step 3: Implement the command**

```rust
#[tauri::command]
pub fn contacts_recent_gateways(
    within_hours: u32,
    favorites: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<Vec<crate::favorites::store::RecentGateway>, ContactsError> {
    let store = favorites.lock().expect("favorites store mutex poisoned");
    let now = chrono::Local::now().fixed_offset();
    Ok(store.recent_gateways(within_hours, now))
}
```

Then register in `src-tauri/src/lib.rs` after `crate::contacts::commands::contacts_connection_record,`:
```rust
    crate::contacts::commands::contacts_recent_gateways,
```

- [ ] **Step 4: Pass** (or defer to CI).
- [ ] **Step 5: Commit** `feat(winlink-map): contacts_recent_gateways command (tuxlink-s1o1)` (+ trailers).

**BEFORE marking complete:** verify the command is in `generate_handler!` (grep `contacts_recent_gateways` in lib.rs returns the registration line). A command not in the handler is invisible to the frontend — the recurring "registration ≠ reachable" trap.

---

## Task 3: TS mirror type + `useRecentGateways` hook

**Files:**
- Create: `src/winlink/recentGateways.ts`
- Test: `src/winlink/recentGateways.test.ts`

**Interfaces:**
- Consumes: command `contacts_recent_gateways` (Task 2), `@tanstack/react-query` `useQuery`, `invoke`.
- Produces:
  ```ts
  export interface RecentGatewayPin { gateway: string; grid?: string; last_attempt_at: string; outcome: 'reached' | 'failed'; }
  export function useRecentGateways(withinHours: number): { gateways: RecentGatewayPin[]; isLoading: boolean };
  export const recentGatewaysKey: (withinHours: number) => readonly unknown[];
  ```

- [ ] **Step 1: Write the failing test** (mirror `useContactConnectionRecord` test style; mock `@tauri-apps/api/core` `invoke`)

```ts
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { vi, describe, it, expect } from 'vitest';
const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));
import { useRecentGateways } from './recentGateways';

it('queries contacts_recent_gateways with withinHours and returns rows', async () => {
  invokeMock.mockResolvedValue([
    { gateway: 'W6DRZ', grid: 'CM97', last_attempt_at: '2026-06-22T11:30:00-07:00', outcome: 'reached' },
  ]);
  const qc = new QueryClient();
  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
  const { result } = renderHook(() => useRecentGateways(6), { wrapper });
  await waitFor(() => expect(result.current.gateways.length).toBe(1));
  expect(invokeMock).toHaveBeenCalledWith('contacts_recent_gateways', { withinHours: 6 });
  expect(result.current.gateways[0].gateway).toBe('W6DRZ');
});
```

- [ ] **Step 2: Run/fail** — `pnpm vitest run src/winlink/recentGateways.test.ts` Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `src/winlink/recentGateways.ts`:

```ts
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';

export interface RecentGatewayPin {
  gateway: string;
  grid?: string;
  last_attempt_at: string;
  outcome: 'reached' | 'failed';
}

export const recentGatewaysKey = (withinHours: number) =>
  ['winlink', 'recent_gateways', withinHours] as const;

export function useRecentGateways(withinHours: number): { gateways: RecentGatewayPin[]; isLoading: boolean } {
  const query = useQuery({
    queryKey: recentGatewaysKey(withinHours),
    queryFn: () => invoke<RecentGatewayPin[]>('contacts_recent_gateways', { withinHours }),
    refetchInterval: 60_000, // recency window ages; refresh gently
  });
  return { gateways: query.data ?? [], isLoading: query.isLoading };
}
```

- [ ] **Step 4: Pass** — `pnpm vitest run src/winlink/recentGateways.test.ts` Expected: PASS.
- [ ] **Step 5: Commit** `feat(winlink-map): useRecentGateways hook + RecentGatewayPin type (tuxlink-s1o1)`.

---

## Task 4: Pure pin-style mapping `winlinkPins.ts`

**Files:**
- Create: `src/winlink/winlinkPins.ts`
- Test: `src/winlink/winlinkPins.test.ts`

**Interfaces:**
- Consumes: `RecentGatewayPin` (Task 3), `gridToLatLon` (`../forms/position/maidenhead`).
- Produces:
  ```ts
  export interface WinlinkPin { gateway: string; lat: number; lon: number; tierClass: string; isLive: boolean; }
  // Pure: drops grid-less gateways; computes a CSS tier class from outcome + age + live state.
  export function toWinlinkPins(rows: RecentGatewayPin[], opts: { livePeer: string | null; nowMs: number }): WinlinkPin[];
  ```
  `tierClass` ∈ `'winlink-pin--live' | 'winlink-pin--reached' | 'winlink-pin--failed' | 'winlink-pin--stale'`. Live peer (matches `livePeer`, case-insensitive, SSID-stripped) → `--live`. Else `failed` outcome → `--failed`; `reached` within 1h → `--reached`; older → `--stale`. Grid-less rows are dropped (no map position — honest).

- [ ] **Step 1: Write the failing test**

```ts
import { describe, it, expect } from 'vitest';
import { toWinlinkPins } from './winlinkPins';
const NOW = Date.parse('2026-06-22T12:00:00-07:00');
const row = (o: object) => ({ gateway: 'W6DRZ', grid: 'CM97', last_attempt_at: '2026-06-22T11:30:00-07:00', outcome: 'reached', ...o });

it('drops grid-less gateways', () => {
  const pins = toWinlinkPins([row({ grid: undefined })], { livePeer: null, nowMs: NOW });
  expect(pins).toEqual([]);
});
it('marks the live peer regardless of outcome/age', () => {
  const pins = toWinlinkPins([row({ outcome: 'failed' })], { livePeer: 'w6drz-1', nowMs: NOW });
  expect(pins[0].tierClass).toBe('winlink-pin--live');
});
it('reached within 1h is --reached; failed is --failed', () => {
  expect(toWinlinkPins([row({})], { livePeer: null, nowMs: NOW })[0].tierClass).toBe('winlink-pin--reached');
  expect(toWinlinkPins([row({ outcome: 'failed' })], { livePeer: null, nowMs: NOW })[0].tierClass).toBe('winlink-pin--failed');
});
```

- [ ] **Step 2: Run/fail** — `pnpm vitest run src/winlink/winlinkPins.test.ts`.
- [ ] **Step 3: Implement** (use `gridToLatLon`; SSID-strip = take chars before `-`, upper-case):

```ts
import { gridToLatLon } from '../forms/position/maidenhead';
import type { RecentGatewayPin } from './recentGateways';

export interface WinlinkPin { gateway: string; lat: number; lon: number; tierClass: string; isLive: boolean; }
const base = (call: string) => call.split('-')[0]!.toUpperCase();

export function toWinlinkPins(
  rows: RecentGatewayPin[],
  opts: { livePeer: string | null; nowMs: number },
): WinlinkPin[] {
  const live = opts.livePeer ? base(opts.livePeer) : null;
  const out: WinlinkPin[] = [];
  for (const r of rows) {
    if (!r.grid) continue;
    const ll = gridToLatLon(r.grid);
    if (!ll) continue;
    const isLive = live !== null && base(r.gateway) === live;
    let tierClass: string;
    if (isLive) tierClass = 'winlink-pin--live';
    else if (r.outcome === 'failed') tierClass = 'winlink-pin--failed';
    else {
      const ageMs = opts.nowMs - Date.parse(r.last_attempt_at);
      tierClass = ageMs <= 3_600_000 ? 'winlink-pin--reached' : 'winlink-pin--stale';
    }
    out.push({ gateway: r.gateway, lat: ll.lat, lon: ll.lon, tierClass, isLive });
  }
  return out;
}
```

- [ ] **Step 4: Pass.** - [ ] **Step 5: Commit** `feat(winlink-map): pure WinlinkPin tier/position mapping (tuxlink-s1o1)`.

**BEFORE marking complete:** confirm `gridToLatLon` returns `{ lat, lon }` (NOT `{ lat, lng }`) — read `src/forms/position/maidenhead.ts` to verify the property name before relying on it.

---

## Task 5: `WinlinkGatewayLayer` (diamond markers) + CSS

**Files:**
- Create: `src/winlink/WinlinkGatewayLayer.tsx`, `src/winlink/WinlinkGatewayLayer.css`

**Interfaces:**
- Consumes: `WinlinkPin[]` (Task 4), `useLeafletMap` (`../map/LeafletMapContext`), `useLeafletLayerGroup` (`../map/leafletHooks`), Leaflet `L`.
- Produces: `export function WinlinkGatewayLayer({ pins, onSelect }: { pins: WinlinkPin[]; onSelect: (gateway: string) => void }): null` — renders nothing into React; imperatively manages markers in a LayerGroup.

This is the imperative shell (grim-smoked, like `OperatorPin`). Mirror `OperatorPin` (`AprsPositionsMap.tsx:519-546`) exactly for lifecycle.

- [ ] **Step 1: Write the CSS (CSP-safe — class drives the diamond, NOT inline style)** `WinlinkGatewayLayer.css`:

```css
/* Diamond = rotated square; color by tier. divIcon html carries NO inline style (CSP). */
.winlink-pin { width: 14px; height: 14px; transform: rotate(45deg); border: 2px solid #eafff2; box-sizing: border-box; }
.winlink-pin--live    { width: 18px; height: 18px; background: #46d07f; box-shadow: 0 0 0 6px rgba(70,208,127,.18); }
.winlink-pin--reached { background: #46d07f; }
.winlink-pin--failed  { background: #e0913a; border-color: #e0913a; }
.winlink-pin--stale   { background: rgba(70,208,127,.45); border-color: rgba(234,255,242,.5); }
.winlink-pin-label { font: 10px ui-monospace, Menlo, monospace; color: #bfe6cd; white-space: nowrap; }
```

- [ ] **Step 2: Implement the layer** (divIcon html uses ONLY classes; click → onSelect):

```tsx
import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { useLeafletMap } from '../map/LeafletMapContext';
import { useLeafletLayerGroup } from '../map/leafletHooks';
import type { WinlinkPin } from './winlinkPins';
import './WinlinkGatewayLayer.css';

export function WinlinkGatewayLayer({ pins, onSelect }: { pins: WinlinkPin[]; onSelect: (gateway: string) => void }): null {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  useEffect(() => {
    if (!group) return;
    group.clearLayers();
    for (const p of pins) {
      const icon = L.divIcon({
        className: 'winlink-pin-icon',
        html: `<div class="winlink-pin ${p.tierClass}"></div>`,
        iconSize: [18, 18], iconAnchor: [9, 9],
      });
      const m = L.marker([p.lat, p.lon], { icon, keyboard: false });
      m.on('click', () => onSelectRef.current(p.gateway));
      group.addLayer(m);
    }
    return () => { group.clearLayers(); };
  }, [group, pins]);

  return null;
}
```

- [ ] **Step 3: Commit** `feat(winlink-map): WinlinkGatewayLayer diamond markers (CSP-safe) (tuxlink-s1o1)`.

**Note:** no jsdom unit test (Leaflet markers + divIcon are a grim-smoke surface, per the DigipeatPathLayer precedent). The pure mapping (Task 4) carries the unit coverage. Acceptance = operator grim-smoke (Task 9).

---

## Task 6: Pure animation grammar `winlinkLinkAnim.ts`

**Files:**
- Create: `src/winlink/winlinkLinkAnim.ts`, `src/winlink/winlinkLinkAnim.test.ts`

**Interfaces:**
- Consumes: `ModemStatus` (`../modem/types`), the previous tick's byte counters.
- Produces: a pure reducer mapping a `modem:status` snapshot → a `LinkDrawState` the canvas renders. NO DOM, NO canvas — fully unit-testable.
  ```ts
  export type LinkPhase = 'idle' | 'connecting' | 'data-out' | 'data-in' | 'busy' | 'error' | 'closing';
  export interface LinkDrawState {
    phase: LinkPhase;
    /** 0..1 intensity of the data comet (from throughput), 0 when not data phase. */
    flow: number;
    /** arc tint 0..1 from quality/snDb (1 = great link). */
    quality: number;
    /** true while an arc should be drawn at all (connecting..closing). */
    active: boolean;
  }
  export function linkDrawState(s: Pick<ModemStatus, 'state' | 'arqFlags' | 'throughputBps' | 'quality' | 'snDb'>): LinkDrawState;
  ```
  Mapping (truthful-now; NO ack/retry): `state==='connecting'`→`connecting`; `arqFlags.busy`→`busy`; `state==='connected-iss'`→`data-out`; `state==='connected-irs'`→`data-in`; `state==='error'`→`error`; `state==='disconnecting'`→`closing`; `idle/stopped/...`→`idle` (active=false). `flow = clamp(throughputBps / 4000, 0, 1)` for data phases else 0. `quality = clamp((quality ?? snrToQuality(snDb)) / 100, 0, 1)`, default 0.6 when both null.

- [ ] **Step 1: Write the failing test**

```ts
import { describe, it, expect } from 'vitest';
import { linkDrawState } from './winlinkLinkAnim';
const base = { state: 'idle' as const, arqFlags: { busy: false, rx: false, tx: false }, throughputBps: null, quality: null, snDb: null };
it('idle → inactive', () => expect(linkDrawState(base).active).toBe(false));
it('connecting → connecting+active', () => {
  const d = linkDrawState({ ...base, state: 'connecting' });
  expect(d.phase).toBe('connecting'); expect(d.active).toBe(true);
});
it('connected-iss with throughput → data-out with flow', () => {
  const d = linkDrawState({ ...base, state: 'connected-iss', throughputBps: 2000 });
  expect(d.phase).toBe('data-out'); expect(d.flow).toBeCloseTo(0.5, 1);
});
it('busy overrides data direction', () => {
  const d = linkDrawState({ ...base, state: 'connected-iss', arqFlags: { busy: true, rx: false, tx: false } });
  expect(d.phase).toBe('busy');
});
it('error → error phase, still active (for the flash)', () => {
  const d = linkDrawState({ ...base, state: 'error' });
  expect(d.phase).toBe('error'); expect(d.active).toBe(true);
});
```

- [ ] **Step 2: Run/fail** — `pnpm vitest run src/winlink/winlinkLinkAnim.test.ts`.
- [ ] **Step 3: Implement** (busy checked before direction; clamp helper inline):

```ts
import type { ModemStatus } from '../modem/types';
export type LinkPhase = 'idle' | 'connecting' | 'data-out' | 'data-in' | 'busy' | 'error' | 'closing';
export interface LinkDrawState { phase: LinkPhase; flow: number; quality: number; active: boolean; }
const clamp01 = (n: number) => (n < 0 ? 0 : n > 1 ? 1 : n);

export function linkDrawState(
  s: Pick<ModemStatus, 'state' | 'arqFlags' | 'throughputBps' | 'quality' | 'snDb'>,
): LinkDrawState {
  const q = s.quality != null ? s.quality / 100 : s.snDb != null ? clamp01((s.snDb + 10) / 30) : 0.6;
  const flowOf = () => clamp01((s.throughputBps ?? 0) / 4000);
  let phase: LinkPhase = 'idle';
  let flow = 0;
  if (s.state === 'connecting') phase = 'connecting';
  else if (s.arqFlags.busy && (s.state === 'connected-iss' || s.state === 'connected-irs')) phase = 'busy';
  else if (s.state === 'connected-iss') { phase = 'data-out'; flow = flowOf(); }
  else if (s.state === 'connected-irs') { phase = 'data-in'; flow = flowOf(); }
  else if (s.state === 'error') phase = 'error';
  else if (s.state === 'disconnecting') phase = 'closing';
  const active = phase !== 'idle';
  return { phase, flow, quality: clamp01(q), active };
}
```

- [ ] **Step 4: Pass.** - [ ] **Step 5: Commit** `feat(winlink-map): pure truthful-now link animation grammar (tuxlink-s1o1)`.

---

## Task 7: `WinlinkLinkLayer` canvas shell

**Files:**
- Create: `src/winlink/WinlinkLinkLayer.tsx`

**Interfaces:**
- Consumes: `linkDrawState` (Task 6), `useModemStatus` (`../modem/useModemStatus`), `useLeafletMap`, the live peer's `{lat,lon}` (resolved by the parent from the pin set), Leaflet `map.latLngToContainerPoint`.
- Produces: `export function WinlinkLinkLayer({ origin, peer }: { origin: {lat:number;lon:number} | null; peer: {lat:number;lon:number} | null }): null`. When both endpoints exist and `linkDrawState(...).active`, draws an animated curved arc on its OWN canvas (z-index 451 — one above DigipeatPathLayer's 450). Copies DigipeatPathLayer's shell: canvas created once per map, bounded rAF that stops when inactive, `safe()` wrapper, resize-to-container. Colors: connecting=dashed `#8fb3ff`; data-out comet `#5ce08a` origin→peer; data-in comet `#7fd0ff` peer→origin; busy = amber `#d9b13a` dashed shimmer; error = `#e0683a` flash. Arc tint alpha scales with `quality`.

This is an imperative canvas shell — grim-smoke acceptance, no jsdom test (Canvas2D absent in jsdom, per DigipeatPathLayer's own note). The unit coverage lives in Task 6.

- [ ] **Step 1: Implement** the component by copying the canvas-lifecycle structure of `src/aprs/DigipeatPathLayer.tsx` (the canvas-create effect with `[map]` deps; the rAF loop with `safe()` + resize + `clearRect`; stop when inactive). Replace the trace-drawing body with: compute `const d = linkDrawState(status)`; if `!d.active || !origin || !peer` clear+stop; else draw the quadratic arc (`origin`→`peer`, perpendicular bow) and a moving packet per `d.phase` (use `performance.now()` for the parametric position; bidirectional per phase). Use `map.latLngToContainerPoint({ lat, lng })` (note `.lng`, not `.lon`). Subscribe to live status via `useModemStatus()` inside the component; the rAF reads the latest via a ref.

  Reference the mock for exact packet shapes: `dev/scratch/2026-06-22-winlink-link-animation-mock.html` (the `comet`, `pingPulse`, `busyShimmer` functions are the visual target; port them to container-point coords).

- [ ] **Step 2: Commit** `feat(winlink-map): WinlinkLinkLayer animated truthful-now arc (tuxlink-s1o1)`.

**Do NOT** add ack/retry packets — not in the truthful-now grammar (would be fabricated). **Do NOT** draw when `peer` has no position (honest: no endpoint → no arc).

---

## Task 8: Layer toggle + recency window + mount + hover popup

**Files:**
- Create: `src/winlink/useWinlinkLayerToggle.ts`
- Modify: `src/aprs/AprsLayersPanel.tsx` (add an overlay toggle row), `src/aprs/AprsPositionsMap.tsx` (mount the layers, wire toggle + window + popup)

**Interfaces:**
- Consumes: all prior tasks. `usePersistedViewport`/`usePersistedBucketFilter` show the persistence idiom (localStorage key under `tuxlink:`).
- Produces: `useWinlinkLayerToggle(): { on: boolean; toggle: () => void; withinHours: number; setWithinHours: (h: number) => void }` (persisted under `tuxlink:winlink-layer`). `AprsLayersPanel` gains optional props `winlinkOn?: boolean; onToggleWinlink?: () => void` rendering one extra row (data-testid `winlink-layer-toggle`) — purely additive, existing bucket behavior unchanged.

- [ ] **Step 1: Write the toggle hook + a vitest** for persistence (mock `localStorage`; assert default off, toggle on persists, withinHours default 6).
- [ ] **Step 2: Implement** `useWinlinkLayerToggle.ts` (mirror an existing `usePersisted*` hook's localStorage read/write).
- [ ] **Step 3: Extend `AprsLayersPanel`** — add, below the bucket rows, an overlay row gated on `onToggleWinlink != null`:

```tsx
{onToggleWinlink && (
  <label className="aprs-layers-panel__row" data-testid="winlink-layer-toggle">
    <input type="checkbox" checked={!!winlinkOn} onChange={onToggleWinlink} />
    <span className="aprs-layers-panel__name">
      <span className="aprs-layers-panel__glyph" aria-hidden="true">◆</span> Winlink links
    </span>
  </label>
)}
```

- [ ] **Step 4: Mount in `AprsPositionsMap`** — in the component body call `useWinlinkLayerToggle()`, `useRecentGateways(withinHours)`, `useModemStatus()`; compute `const pins = useMemo(() => on ? toWinlinkPins(gateways, { livePeer: status.peer, nowMs: Date.now() }) : [], [on, gateways, status.peer])`; track a selected gateway state for the popup. Pass `winlinkOn`/`onToggleWinlink` to `<AprsLayersPanel>`. Inside `<LeafletMap>`, when `on`, render:
  ```tsx
  <WinlinkGatewayLayer pins={pins} onSelect={setSelectedGateway} />
  <WinlinkLinkLayer origin={me} peer={livePeerLatLon} />
  ```
  where `livePeerLatLon` is the `WinlinkPin` with `isLive` (its `{lat,lon}`) or null. Both render only when `on` (toggle gates BOTH the diamonds and the arc — the design's explicit requirement).

- [ ] **Step 5: Hover/click popup** — on `setSelectedGateway`, render a Leaflet popup (or reuse the existing popup component the heard-station map uses) bound to that pin's latlon, populated by `useContactConnectionRecord(selectedGateway)` (attempts + tod hint). Close on map click / `×`.

- [ ] **Step 6: Commit** `feat(winlink-map): mount Winlink layer, toggle gates diamonds+arc, hover history (tuxlink-s1o1)`.

**BEFORE marking complete:** verify toggling the layer OFF removes BOTH the diamonds AND tears down the arc canvas (the arc lives on its own canvas, not the LayerGroup — if you conditionally render `<WinlinkLinkLayer>` only when `on`, its unmount cleanup removes the canvas; confirm that path). Verify a connected peer with NO grid draws no arc (no endpoint).

---

## Task 9: Integration, gates, wire-walk, draft PR

- [ ] **Step 1: Run the full gate suite** — `pnpm install` (if not yet), `pnpm typecheck`, `pnpm vitest run`, `pnpm build`. All green. (Rust: push and let CI run `cargo clippy`/`cargo test` — do not cold-build locally.)
- [ ] **Step 2: Review batch from multiple perspectives — minimum 3 rounds** per the BRF review-loop mandate. Check each task against `docs/pitfalls/testing-pitfalls.md` and `docs/pitfalls/implementation-pitfalls.md`. Keep going past 3 if substantive findings remain.
- [ ] **Step 3: Open a DRAFT PR early** (`gh pr create --draft --base main --head bd-tuxlink-s1o1/winlink-map-layer`) so CI compiles the Rust. Title: `[esker-oak-butte] feat: Winlink link layer on the APRS map (tuxlink-s1o1)`.
- [ ] **Step 4: WIRE-WALK GATE (hard gate, per CLAUDE.md `.claude/skills/wire-walk/`):** the OPERATOR supplies the key user flows greenfield (do NOT draft them yourself — anchoring launders blind spots). Trace each flow verbatim to `file:line`. Any broken primary flow = NOT shipped. Likely flows the operator may name: "toggle Winlink links on → see my recently-called gateways as diamonds"; "connect to a gateway → watch the live arc animate"; "hover a gateway → see my connection history"; "change the recency window → set of diamonds updates."
- [ ] **Step 5: Operator grim-smoke** of the canvas + markers (the un-unit-testable surface): diamonds render CSP-safe (not oversized — the v0.74.1 bug class), arc animates during a real ARDOP connection, two trace types stay visually distinct from APRS digipeat. Per CLAUDE.md this is opportunistic/post-merge-friendly, not a hard pre-merge gate.

---

## Self-Review (run before handoff)

- **Spec coverage:** design doc's 4 decisions → Tasks: toggle-layer-on-APRS (Task 8 ✓), recency-windowed gateways (Tasks 1/3/8 ✓), truthful-now ARDOP grammar (Tasks 6/7 ✓), no new chrome (no dock added ✓). Edge cases from the design's "live-arc edge cases": no-position peer (Task 7/8 ✓), toggle-off teardown (Task 8 ✓), live-peer-outside-window → live peer always pinned (Task 4 `--live` regardless of age ✓), clean-disconnect vs error (Task 6 `closing` vs `error` ✓).
- **Type consistency:** `RecentGatewayPin` (Task 3) consumed by Tasks 4/8; `WinlinkPin` (Task 4) by Tasks 5/8; `LinkDrawState`/`linkDrawState` (Task 6) by Task 7. Snake_case DTO (Rust) ↔ snake_case TS interface (Task 3) — consistent. `gridToLatLon` returns `.lon` (verify in Task 4); Leaflet wants `.lng` (Task 7 note).
- **Deferred (NOT in scope):** ack/retry frame visuals (tuxlink-g8h9); VARA live animation (tuxlink-5q31).
