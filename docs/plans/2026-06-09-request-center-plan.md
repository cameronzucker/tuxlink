# Request Center Implementation Plan (tuxlink-eymu)

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Every task** begins with the TDD preamble and ends with the completion check below. After every task group, run the review loop.

**Goal:** Replace the cramped `CatalogRequestPanel` modal and the separate `GribRequestPanel` with one full-screen, request-first **Request Center**: location-aware request cards, search + 3-pane catalog browse, a demoted GRIB form, and a unified request basket that dispatches per rail (CMS inquiry vs Saildocs).

**Architecture:** A new viewport-covering overlay component (`src/request/RequestCenter.tsx`), mounted via the existing AppShell boolean-flag + lazy/Suspense pattern (NO new route). It reuses the existing dispatch commands (`catalog_send_inquiry`, `grib_send_request`) and catalog data (`catalog_list`). A small pure geo layer resolves the operator's grid to a US state and a sea-area to back the location-aware weather cards. The old two panels are deleted once parity is reached.

**Tech Stack:** React + TypeScript (vitest + @testing-library/react), Tauri `invoke`, existing Rust catalog/grib commands (unchanged).

---

## Decisions log (operator-aligned, 2026-06-09 — gulch-ivy-kite)

These were decided from the approved mock v4 + a catalog-data audit. Subagents: do NOT relitigate; implement as stated.

1. **Full-screen = overlay**, not a route. Reuse AppShell's `{flag && <Suspense>…}` pattern. New id `menu:message:request_center`.
2. **Absorb both** old panels: `CatalogRequestPanel` → the 3-pane "Browse full catalog by category"; `GribRequestPanel` → the demoted GRIB "More:" form inside the center. Delete both old files in Task 12 after parity.
3. **Menu:** one "Request Center…" entry; a "GRIB File Request…" entry remains but deep-links into the Request Center's GRIB form. Update `menuModel.ts` + the `EXPECTED_IDS` contract test in the SAME change.
4. **Dispatch rails unchanged:** CMS inquiry = `catalog_send_inquiry(filenames: string[])`; Saildocs = `grib_send_request(request: GribRequest)`. Both already route to the outbox. Do not reinvent.
5. **Geo layer = grid→state + grid→sea-area only** (state/zone + sea-area is what the catalog supports). No airport/office resolution.
6. **Verified card set** (only these are backable; do NOT invent others):
   - Weather → **State forecast** (grid→state→`WX_US_<ST>`, pick best "State Forecast" entry at runtime), **Marine forecast** (grid→sea-area→ open the resolved `WX_*` category in browse).
   - Propagation & space (national, one-click) → **Propagation forecast** = `PROP_3DAY`, **Solar-terrestrial** = `PROP_WWV`, **Aurora tonight** = `AUR_TONIGHT`.
   - Nearby stations → **Public gateway lists** = `WL2K_RMS` (`PUB_ARDOP`/`PUB_PACKET`/`PUB_VARA` etc., by mode), **Winlink info & how-to** = `INQUIRIES` (`WL2K_HELP`).
7. **DROPPED for v1 (unbackable — document, do NOT fake):** nearby-airport METARs (catalog has zero US METARs), hazardous-weather outlook (only TN has entries). "Gateways near you" is reframed to "Public gateway lists (by mode)" — nationwide, not geo-filtered.
8. **Basket:** unified; each item tagged `rail: 'cms' | 'saildocs'`. "Send all" → ONE `catalog_send_inquiry` for all cms filenames + ONE `grib_send_request` per saildocs item; success summarizes per rail.

**Out of scope / fast-follows (documented, not built):** METAR-by-station parameterized form; request draft/history persistence (keep the straight-to-outbox flow); grid→NWS-office hazardous resolution.

---

## TDD preamble (prepend to EVERY task)

```
BEFORE starting work:
1. Invoke /test-driven-development (or read .claude/skills/test-driven-development/).
2. Read docs/pitfalls/testing-pitfalls.md and docs/pitfalls/implementation-pitfalls.md.
Follow TDD: write the failing test → run it red → implement minimally → run green. Commit each task.
Conventions: vitest + @testing-library/react; mock Tauri via vi.mock('@tauri-apps/api/core'); see src/catalog/CatalogRequestPanel.test.tsx for the pattern.
```

## Completion check (append to EVERY task)

```
BEFORE marking complete:
1. Re-review tests against docs/pitfalls/testing-pitfalls.md (error paths? edge cases? production mount path where relevant?).
2. Run the affected vitest file(s) green. Before pushing the branch, run clippy --all-targets -D warnings + the full `pnpm vitest run` (CI verify is stricter than a scoped run — see memory scoped-vitest-misses-contract-tests).
3. Reap vitest workers after sweeps: pgrep -fc '[v]itest'; pkill -9 -f '[v]itest' (avoid the self-match — see memory vitest-worker-zombies).
```

## Review loop (after EACH task group below)

```
Review the batch from multiple perspectives (correctness, test quality, scope creep, WebKitGTK layout-fit). Minimum 3 rounds; if the 3rd still finds substantive issues, keep going. Then continue.
```

---

## File structure

**Create:**
- `src/request/RequestCenter.tsx` — the overlay shell (header, search row, body = content | basket).
- `src/request/RequestCenter.css` — design-token styles (dark; constrain like the mock; verify in WebKitGTK, not Chromium).
- `src/request/sections.ts` — the curated request-first card definitions + their catalog mappings (pure data + resolver calls).
- `src/request/geo.ts` — `gridToLatLon`, `latLonToUsState`, `latLonToSeaArea` (pure).
- `src/request/geo-data.ts` (or bundled JSON) — simplified US-state polygons + sea-area buckets (sourced, public-domain; license noted in-file).
- `src/request/basket.ts` — `BasketItem` type + `useRequestBasket` hook (add/remove/clear, rail tagging) + `dispatchBasket`.
- `src/request/CatalogBrowse.tsx` — 3-pane (category nav | items | basket) browse, absorbing CatalogRequestPanel behavior.
- `src/request/GribForm.tsx` — the GRIB form, reusing GribRequestPanel's form logic/validation.
- Test files alongside each (`*.test.tsx` / `*.test.ts`).

**Modify:**
- `src/shell/chrome/menuModel.ts` (+ `menuModel.test.ts` EXPECTED_IDS), `src/shell/chrome/dispatchMenuAction.ts`, `src/shell/AppShell.tsx` (state flag + handler + lazy/Suspense mount; remove old open paths).

**Delete (Task 12, after parity):**
- `src/catalog/CatalogRequestPanel.tsx` (+ `.css`, `.test.tsx`), `src/grib/GribRequestPanel.tsx` (+ `.test.tsx`). KEEP `src/catalog/useCatalog.ts`, `src/catalog/types.ts`, `src/grib/useGrib.ts`, `src/grib/types.ts` (the data/dispatch layers are reused).

---

## Group A — Foundational (no UI). Parallelizable after Task A0.

### Task A0: Confirm the reused interfaces (read-only, no commit)
**Files:** read `src/catalog/types.ts`, `src/catalog/useCatalog.ts`, `src/grib/types.ts`, `src/grib/useGrib.ts`, `src/shell/chrome/menuModel.ts`, `src/shell/chrome/menuModel.test.ts`, `src/shell/AppShell.tsx` (catalog/grib mount region), `src/catalog/CatalogRequestPanel.tsx`, `src/grib/GribRequestPanel.tsx`.
- [ ] Record the exact `CatalogEntry`, `GribRequest`, `useCatalog`/`sendCatalogInquiry`, `sendGribRequest` signatures and the `EXPECTED_IDS` array into your working notes. These are the contracts every later task depends on. (No code change.)

### Task A1: `gridToLatLon`
**Files:** Create `src/request/geo.ts`, `src/request/geo.test.ts`. (Check first whether a Maidenhead→lat/lon util already exists — grep `maidenhead`, `gridToLat`; if present, re-export it instead of duplicating.)
- [ ] **Test:** `gridToLatLon('CN87')` ≈ `{ lat: 47.5, lon: -123.0 }` (within 1°); `gridToLatLon('EM26')` in central US; invalid grid → `null`. Write concrete asserts with `toBeCloseTo`.
- [ ] Run red → implement standard Maidenhead center decode (2-char and 4-char) → run green → commit.

### Task A2: `latLonToUsState`
**Files:** `src/request/geo.ts`, `src/request/geo-data.ts` (bundle a simplified US-state polygon GeoJSON — source: US Census cartographic boundary, simplified to ≤~50 pts/state; note source+license in-file), `src/request/geo.test.ts`.
- [ ] **Test (incl. border cases):** Seattle `(47.6,-122.3)`→`'WA'`; Portland `(45.5,-122.7)`→`'OR'`; a Gulf point→correct state; an ocean point→`null`. Use ray-casting point-in-polygon (no new dep).
- [ ] Run red → implement PIP over bundled polygons, return the 2-letter USPS code that maps to a `WX_US_<ST>` catalog category → green → commit.
- [ ] Edge: document that ambiguous border points resolve to whichever polygon contains the centroid; acceptable for a home-QTH grid.

### Task A3: `latLonToSeaArea`
**Files:** `src/request/geo.ts`, `geo.test.ts`.
- [ ] **Test:** Pacific NW coast → `'WX_EASTPAC'`; SoCal → `'WX_EASTPAC'`/`'WX_PACIFIC'` (pick one, assert it); Gulf → `'WX_CAR_GULF'`; NE coast → `'WX_ATLANTIC'`; interior (no coast within ~300 mi) → `null`.
- [ ] Run red → implement a coarse longitude/latitude bucket mapping to a sea-area catalog category constant → green → commit.

### Task A4: catalog mapping helpers
**Files:** Create `src/request/catalogMap.ts`, `catalogMap.test.ts`. Depends on A0 (`CatalogEntry`).
- [ ] **Test:** `bestStateForecast(entries, 'WA')` returns the `WX_US_WA` entry whose description matches `/state forecast/i`, preferring a non-tabular `*_FOR_*` filename over `*_TAB_*`, **falling back to a tabular entry if that's all that exists**, and returning `null` if the state has none. Include a no-state-forecast case: `bestStateForecast(entries, 'AK')` → `null` → the card hides. `NATIONAL = { propagation:'PROP_3DAY', solar:'PROP_WWV', aurora:'AUR_TONIGHT', winlinkInfo:'INQUIRIES' }`; `gatewayListFilenames(entries)` returns the `WL2K_RMS` `PUB_*` filenames present.
- [ ] **Coverage reality (Codex-verified 2026-06-09 — fallback is PRIMARY, not defensive):** of 51 `WX_US_<ST>` buckets, **14 have no state-forecast entry** (AK VT SD NM MD NH MA KS DE PR KY SC RI IL → card must hide), **28 are tabular-only** (card resolves to a `*_TAB_*` entry), and only **9** have a clean `*_FOR_*`. The "State forecast" card therefore commonly shows a tabular forecast or is absent. Do NOT assume a clean state forecast exists.
- [ ] Run red → implement pure helpers over `CatalogEntry[]` → green → commit.

**→ Run the review loop for Group A.**

---

## Group B — Basket + dispatch (no full UI yet)

### Task B1: `BasketItem` + `useRequestBasket`
**Files:** Create `src/request/basket.ts`, `basket.test.ts`.
- [ ] **Test:** `BasketItem = { id:string; label:string; rail:'cms'; filename:string } | { id:string; label:string; rail:'saildocs'; request:GribRequest }`. Hook supports add (dedupe by id), remove, clear; exposes `cmsFilenames` and `saildocsItems` selectors.
- [ ] Run red → implement → green → commit.

### Task B2: `dispatchBasket` — per-rail send
**Files:** `src/request/basket.ts`, `basket.test.ts`. Reuses `sendCatalogInquiry`, `sendGribRequest`.
- [ ] **Test (dual-rail, the key one):** given a basket with 2 cms items + 1 saildocs item, `dispatchBasket` calls `catalog_send_inquiry` EXACTLY once with both cms filenames (order preserved) AND `grib_send_request` once for the saildocs item; returns a per-rail summary `{ cms:{ sent:2, mid }, saildocs:[{ mid }] }`. Mock `invoke`. Also test cms-only and saildocs-only baskets (the absent rail is not called).
- [ ] Run red → implement → green → commit.

**→ Run the review loop for Group B.**

---

## Group C — Request Center shell + sections

### Task C1: RequestCenter overlay shell
**Files:** Create `src/request/RequestCenter.tsx`, `RequestCenter.css`, `RequestCenter.test.tsx`. Props: `{ onClose: () => void; initialView?: 'home' | 'browse' | 'grib' }`.
- [ ] **Test:** renders `role="dialog"` with `aria-label="Request Center"`, a header with the `data-testid="request-center-location"` chip and a Close button (calls `onClose`), a search input (`data-testid="request-search"`), and a body with content + `data-testid="request-basket"` regions. ESC calls `onClose`. Loads grid via `invoke('config_read')` (mock returns `{ grid:'CN87' }`) and shows "Near CN87".
- [ ] Run red → implement the shell (model overlay structure on CatalogBuilderPanel: fixed backdrop, no backdrop-click-close, design tokens) → green → commit.
- [ ] WebKitGTK note: use auto/min-height, not fixed pixel heights (memory chromium-not-webkitgtk-proxy).

### Task C2: request-first sections + cards
**Files:** Create `src/request/sections.ts`; modify `RequestCenter.tsx`; `RequestCenter.test.tsx`. Depends on A2/A3/A4, B1.
- [ ] **Test:** with grid `CN87`, the Weather section shows a "State forecast" card resolving to the WA state-forecast entry and a "Marine forecast" card (sea-area `WX_EASTPAC`); Propagation shows 3 national cards; Nearby stations shows gateway-lists + Winlink-info. Clicking a card's add control adds the correct `BasketItem` (cms rail, right filename). Dropped cards (METAR, hazardous) are ABSENT. Assert exact basket contents after clicks.
- [ ] Run red → implement sections from `sections.ts` (each card = label, icon, resolver→filename) → green → commit.

**→ Run the review loop for Group C.**

---

## Group D — Browse + GRIB form + search

### Task D1: 3-pane CatalogBrowse (absorb CatalogRequestPanel)
**Files:** Create `src/request/CatalogBrowse.tsx`, `CatalogBrowse.test.tsx`; mount behind the "Browse full catalog by category" reveal in `RequestCenter.tsx`. Reuses `useCatalog`.
- [ ] **Test:** loads `catalog_list` (mock fixture), renders category nav (left) → selecting a category lists its items (center) → "add" puts a cms `BasketItem`. Port the meaningful assertions from `CatalogRequestPanel.test.tsx`.
- [ ] Run red → implement 3-pane → green → commit.

### Task D2: search across all items
**Files:** `RequestCenter.tsx`, `CatalogBrowse.tsx`, tests.
- [ ] **Test:** typing in the search filters across all 1,477 by category/filename/description (case-insensitive), showing matching items add-able to the basket.
- [ ] Run red → implement client-side filter over the loaded catalog → green → commit.

### Task D3: GRIB "More:" form (absorb GribRequestPanel)
**Files:** Create `src/request/GribForm.tsx`, `GribForm.test.tsx`; reachable via the "More: GRIB by area" link and via `initialView='grib'`. Reuse GribRequestPanel's region/grid/times/params logic + `GribRequest` type + validation.
- [ ] **Test:** form builds a valid `GribRequest`; "add to basket" creates a `saildocs` `BasketItem` carrying the request (NOT an immediate send). Port GribRequestPanel's validation tests.
- [ ] Run red → implement (extract shared form logic from GribRequestPanel rather than copy-paste where clean) → green → commit.

**→ Run the review loop for Group D.**

---

## Group E — Basket UI + Send + integration

### Task E1: basket right-rail UI + Send all
**Files:** `RequestCenter.tsx`, `RequestCenter.test.tsx`. Uses B1/B2.
- [ ] **Test:** basket lists added items with remove (✕); footer summarizes counts per rail ("N requests · 1 inquiry message to the CMS"); "Send all" calls `dispatchBasket`; on success shows the per-rail summary + "Responses arrive in your Inbox after the next connect" and clears the basket. Mock invoke for both rails.
- [ ] Run red → implement → green → commit.

### Task E2: menu IA + AppShell wiring
**Files:** `src/shell/chrome/menuModel.ts`, `src/shell/chrome/menuModel.test.ts` (EXPECTED_IDS), `src/shell/chrome/dispatchMenuAction.ts`, `src/shell/AppShell.tsx`.
- [ ] **Test:** `menuModel.test.ts` EXPECTED_IDS includes `menu:message:request_center`; the GRIB entry maps to opening the center at the GRIB form. dispatchMenuAction test: `menu:message:request_center` → `openRequestCenter()`.
- [ ] Run red → add the menu entry + handler + AppShell boolean flag `requestCenterOpen` + lazy `RequestCenter` mount (Suspense fallback null); wire `openRequestCenter`/grib deep-link → green → commit.

### Task E3: App-level production-mount test
**Files:** Create `src/request/RequestCenter.app.test.tsx` (mounts via AppShell, not the unit). Per testing-pitfalls (test the production mount path).
- [ ] **Test:** render AppShell, fire the menu action that opens the Request Center, assert the dialog mounts with its providers (no missing-context crash), add a national card, "Send all" invokes the right command.
- [ ] Run red → fix any provider/wiring gaps → green → commit.

**→ Run the review loop for Group E.**

---

## Group F — Cutover + polish

### Task F1: delete the old panels
**Files:** Delete `src/catalog/CatalogRequestPanel.tsx` (+`.css`,`.test.tsx`), `src/grib/GribRequestPanel.tsx` (+`.test.tsx`); remove their AppShell flags/handlers and the old `menu:message:catalog_request` open path (the menu now routes to the Request Center). Update EXPECTED_IDS accordingly.
- [ ] Grep for any remaining importers of the deleted files; fix. Run full `pnpm vitest run` green. Commit.
- [ ] Confirm no dead `menu:message:catalog_request`/`grib_request` references remain unless intentionally repointed.

### Task F2: WebKitGTK layout-fit + docs
- [ ] Verify the full-screen layout fits in WebKitGTK via grim on pandora (NOT Chromium) — memory grim-realapp-validation. Capture before/after if adjusting CSS.
- [ ] Update CHANGELOG + any user-guide catalog/GRIB references to the Request Center. AGENTS.md parity check if any rule text changed (none expected).

**→ Run the review loop for Group F. Then the feature is complete; CI verify (clippy --all-targets -D warnings + full vitest, both arches) is the merge gate.**

---

## Adrev revisions (Codex cross-provider round, 2026-06-09 — BINDING; apply within the named tasks)

1. **Card action model (C2/sections.ts):** a card's action is a tagged union — `{ kind:'addCms', filename }` OR `{ kind:'openBrowse', category }`. The **Marine** card is `openBrowse('WX_EASTPAC'|…)` (navigates the 3-pane to the resolved sea-area category; does NOT mutate the basket). Test marine-click → browse shows that category, basket unchanged.
2. **National constants are real FILENAMES, guard them (A4):** `PROP_3DAY`, `PROP_WWV`, `AUR_TONIGHT`, `INQUIRIES` are filenames. Add a test asserting each NATIONAL filename is present in the loaded catalog (fail loudly if the catalog drops one). For `openBrowse` cards keep `category` separate from any filename; basket items only ever carry real filenames.
3. **Single catalog load owner (C1):** `RequestCenter` loads `catalog_list` ONCE (via `useCatalog`) and passes `entries` to home sections, browse, and search. Test loading / empty / error states at the RequestCenter level (mock `catalog_list` rejecting).
4. **Partial-failure dispatch (B2/E1):** `dispatchBasket` runs the two rails with `Promise.allSettled`. On partial failure: KEEP the failed rail's items in the basket, clear only the succeeded rail, and surface a per-rail error ("CMS sent; Saildocs failed: <msg>"). Tests: cms-ok+saildocs-fail (saildocs items remain, error shown), both-fail (nothing cleared), both-ok (cleared).
5. **Empty basket (E1):** "Send all" is disabled when the basket is empty; test that clicking a disabled/empty Send invokes nothing.
6. **State polygons (A2):** include a reproducible sourcing note (script or documented steps) deriving simplified polygons from US Census cartographic boundaries; support `MultiPolygon` (HI, AK, island states); set a bundle size budget (≤~150 KB) and note it. Tests: coastal (Seattle/WA), border (KC metro), island (Honolulu/HI), ocean point → null.
7. **Sea-area buckets (A3):** define explicit lon/lat boundaries with precedence (Pacific vs Atlantic vs Gulf) and an inland-exclusion rule. Tests: Phoenix/Denver/Chicago → null; Miami → Atlantic-or-Gulf (assert which); Seattle → EASTPAC.
8. **Menu IDs — declare final state once (E2/F1):** ADD `menu:message:request_center`; REMOVE `menu:message:catalog_request`; KEEP `menu:message:grib_request` mapped to opening the center with `initialView='grib'`. Update `EXPECTED_IDS` in one edit. Grep to confirm zero dangling `catalog_request` refs after F1.
9. **Production/error-path tests (E3 + throughout):** E3 must drive the EXACT AppShell menu-dispatch path (not a synthetic open) and assert the dialog mounts with providers. Add invoke-failure tests for `config_read` (no grid → chip shows a neutral state, no crash), `catalog_list` (error state), and both send rails (error surfaced) BEFORE deleting the old panels in F1.

## Self-review notes (author)
- Spec coverage: all approved-mock elements map to a task (header/location chip C1; search D2; sections+cards C2; browse D1; GRIB form D3; basket+dual-rail B2/E1; menu IA E2). Dropped cards (METAR, hazardous) documented in the decisions log, not silently omitted.
- The geo dataset (A2) is the one real data-sourcing risk — isolated in its own task with border-case tests.
- Reuse over rewrite: dispatch commands, `useCatalog`, `GribRequest`/validation are reused; only UI is new.
