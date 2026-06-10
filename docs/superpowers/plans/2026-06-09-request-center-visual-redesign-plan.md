# Request Center Visual Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-skin the shipped Request Center to the approved Direction-C mock — distinctive request-first cards, real line icons, purposeful use of the amber accent, true 1200×820 proportions — without changing any structure, behavior, testid, or dispatch.

**Architecture:** A presentation-only change to `RequestCenter` + `CatalogBrowse` + `GribForm` (markup + CSS), a new shared inline-SVG icon set, and one small `sections.ts` data addition (a section `kind` so the home heroes the location section vs. chips the rest). The geo / catalogMap / basket / dispatch / menu / AppShell layers are untouched.

**Tech Stack:** React + TypeScript, the app's CSS-token design system (`src/App.css`), vitest + @testing-library/react.

---

## Source of truth + binding rules (read before any task)

- **Pixel reference:** [`docs/design/mockups/2026-06-09-request-center-redesign-final.html`](../../design/mockups/2026-06-09-request-center-redesign-final.html) — Direction C at 1200×820. Translate its CSS into the component stylesheets using the app's `--tokens` (NOT the mock's hardcoded hexes; the mock's `:root` mirrors the tokens, so map `#f59f3c`→`var(--accent)`, `#141c23`→`var(--surface)`, etc.).
- **Spec:** [`docs/superpowers/specs/2026-06-09-request-center-visual-redesign-design.md`](../specs/2026-06-09-request-center-visual-redesign-design.md).
- **HARD CONSTRAINT — preserve every testid + accessible name.** The 123 `src/request/` tests + `src/shell/chrome` contract tests MUST stay green. Before editing a component, `grep -n 'data-testid\|aria-label' <file>` and keep each one. If a test asserts structure that genuinely must change, update the test minimally and note it — but default to preserving.
- **WebKitGTK:** no fixed pixel heights on layout containers (flex + `min-height:0` + `overflow-y:auto`); the GRIB map viewport is the one bounded exception. Content column caps at ~840px (`.content-inner` max-width) so a maximized window doesn't stretch; basket pinned right.
- **Voice:** formal/declarative present-indicative; no first person; no temporal hedging.
- **Per task:** run the affected existing test file(s) — they must stay GREEN (this is the regression gate in place of TDD-first) — plus `pnpm run typecheck` → 0. Reap only your own vitest workers. Commit. Conventional commit + the `Agent:` + `Co-Authored-By:` trailers.

## File structure

- **Create:** `src/request/icons.tsx` (shared inline-SVG icon set) — IF the app has no existing icon convention (Task 1 decides).
- **Modify:** `src/request/sections.ts` (+ `sections.test.ts` if present / `catalogMap` consumers) — add section `kind`. `src/request/RequestCenter.tsx` + `.css` (header, home hero/chips, basket rail). `src/request/CatalogBrowse.tsx` + `.css` (3-pane, search results). `src/request/GribForm.tsx` + `.css` (form, param chips). Possibly a tiny `src/request/usStateName.ts` (USPS→name map for the chip).

---

## Task 1: Shared line-icon set

**Files:** read the codebase for an existing icon approach; Create `src/request/icons.tsx` only if none exists.

- [ ] Search for an existing icon convention: `grep -rl "lucide\|<svg" src/shell src/mailbox src/radio | head` and check `package.json` for `lucide-react` or similar. If the app already has a shared icon component/library, REUSE it (note which) and skip creating `icons.tsx` — adapt later tasks to that API.
- [ ] If none: create `src/request/icons.tsx` exporting a single `Icon` component: `export function Icon({ name, size = 18, className }: { name: IconName; size?: number; className?: string })` rendering an inline `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round" width={size} height={size}>` with a `<path>` per name. Names + paths come from the mock's `<symbol>` defs: `pin, search, close, plus, arrow, check, radio, weather, wave, prop, sun, aurora, tower, info, list, map, basket, trash`. `IconName` is the string-literal union of those.
- [ ] **Test (`src/request/icons.test.tsx`):** renders `<Icon name="weather"/>` → an `<svg>` is in the DOM with the given size; an unknown name is a TypeScript error (compile-time) — assert one known icon renders a `path`. Keep it light; icons are static.
- [ ] Run `pnpm exec vitest run src/request/icons.test.tsx` → green; `pnpm run typecheck` → 0. Commit `feat(request): shared line-icon set for the Request Center redesign`.

## Task 2: `sections.ts` — section kind (location vs national)

**Files:** Modify `src/request/sections.ts` (+ `sections.test.ts` if it exists). Depends on nothing.

- [ ] Add `kind: 'location' | 'national'` to the `RequestSection` interface. In `buildSections`, tag the Weather section (the geo-derived State/Marine cards) `kind: 'location'`; tag Propagation & space and Nearby stations `kind: 'national'`. No card-level change; card ids/labels/actions unchanged.
- [ ] **Test:** `buildSections(entries, 'CN87')` → the Weather section has `kind === 'location'`, the others `'national'`; with `grid: null` the location section is absent (geo cards omitted) and the nationals remain `'national'`. Keep all existing `sections`/`RequestCenter` assertions green.
- [ ] Run the sections + RequestCenter test files → green; typecheck → 0. Commit `feat(request): tag request sections location vs national for the home hero`.

## Task 3: RequestCenter — header + home (hero + chips + reveals)

**Files:** Modify `src/request/RequestCenter.tsx`, `src/request/RequestCenter.css`; Create `src/request/usStateName.ts`. Depends on Tasks 1, 2.

- [ ] `usStateName.ts`: `export function usStateName(usps: string): string | null` — a static USPS→full-name map (50 states + DC/PR/territories as available); returns null for unknown. Used only for the chip's "· <state>" suffix.
- [ ] **Header:** RC glyph (`Icon name="radio"`) + title; location chip = pin icon + `Near <grid>` + (when `usStateName(state)` resolves) `· <state name>`; keep the neutral "Location not set" when no grid. Search input keeps `data-testid="request-search"` + a search icon. Close keeps `data-testid="request-close"` + a close icon. Keep `data-testid="request-center-location"`. (To get the state for the chip, resolve it the same way `buildSections` does — `gridToLatLon`→`latLonToUsState`; lift that to a small memo or read it off the built sections' location section.)
- [ ] **Home render:** when `view==='home'` and entries are loaded, render: (a) the `kind:'location'` section as the **hero** — an amber-edged panel with each card as a large feat card (icon tile + label + `card.description` + the existing Add control for addCms / "Browse <area>" for openBrowse); (b) the `kind:'national'` sections as **chip grids** (icon + label + description + filename + add/→). Reuse `runAction` unchanged. PRESERVE `request-section-<id>`, `request-card-<id>`, and the add-control aria-labels. Map card→icon (weather/wave/prop/sun/aurora/tower/info) by card id; default to a sensible icon.
- [ ] **CSS:** port `.hero/.feat/.chips/.chip/.subt/.reveals/.rev` from the mock into `RequestCenter.css` using `--tokens`. Add `.content-inner { max-width: 840px }`. Bump base sizing to the mock's scale. No fixed heights on containers.
- [ ] **Reveals:** "Browse full catalog by category" + "GRIB by area" keep their existing testids/handlers (`request-browse-reveal`, the grib reveal) with the new `.rev` styling + list/map icons + the count meta.
- [ ] Run `pnpm exec vitest run src/request/RequestCenter.test.tsx src/request/sections.test.ts` → green (update only assertions that depended on the old flat-card structure, preserving testids); typecheck → 0. Commit `feat(request): re-skin RequestCenter header + request-first home (hero + chips)`.

## Task 4: Basket rail re-skin

**Files:** Modify `src/request/RequestCenter.tsx` (the `request-basket` aside) + `RequestCenter.css`. Depends on Task 1.

- [ ] Restyle the basket: header with basket icon + count chip; item rows with a per-rail icon tile (cms = success-tint + the card's icon or a generic check; saildocs = amber + map icon) + label + rail caption + a trash-icon remove; per-rail summary; gradient amber **Send all** (greyed/disabled when empty); arrival note; success/error result block (style `request-basket-result`). Add an **empty state** (dashed ring + basket icon + "Your basket is empty. Add requests from the cards or browse.") shown when `basket.isEmpty`.
- [ ] PRESERVE `data-testid`: `request-basket`, `basket-item-<id>`, `basket-remove-<id>`, `request-basket-summary`, `request-basket-send`, `request-basket-result` (+ its line/note/error children if tests target them — grep first).
- [ ] Port `.basket/.bk-*/.send/.result/.bk-empty` from the mock. No fixed heights; list scrolls within the rail.
- [ ] Run `pnpm exec vitest run src/request/RequestCenter.test.tsx` → green; typecheck → 0. Commit `feat(request): re-skin the request basket rail (icons, summary, send, empty state)`.

## Task 5: CatalogBrowse re-skin (3-pane)

**Files:** Modify `src/request/CatalogBrowse.tsx`, `src/request/CatalogBrowse.css`. Depends on Task 1.

- [ ] Restyle: crumb + Back (arrow icon); category nav (name + count, active = amber inset rail + `--accent-2` text); item rows (mono filename in `--accent-2`, description, size, Add control / "✓ Added" with check icon when in basket). Search-mode results list keeps its flat layout, restyled to match. PRESERVE `request-browse` (+ `data-category`), `catalog-browse-*`, `catalog-search-results`, `catalog-browse-back`, item/add testids — grep first.
- [ ] Port `.crumb/.browse/.nav/.nav-item/.items/.it` from the mock.
- [ ] Run `pnpm exec vitest run src/request/CatalogBrowse.test.tsx src/request/RequestCenter.test.tsx` → green; typecheck → 0. Commit `feat(request): re-skin the 3-pane catalog browse`.

## Task 6: GribForm re-skin

**Files:** Modify `src/request/GribForm.tsx`, `src/request/GribForm.css`. Depends on Task 1.

- [ ] Restyle: crumb + Back; sectioned form (Region with the **real `GridMapPicker`** in a styled bounded container — keep its existing height handling, the one allowed fixed height; lat/lon fields; grid; forecast-hours input; Parameters as toggle chips `.pchip`/`.pchip.on`; Mode/sub block when present; Subject); the primary "Add to request" button (`grib-add`) with a plus icon. PRESERVE every `grib-*` field testid + `request-grib` + `grib-add` + `grib-back` — grep first.
- [ ] Port `.grib/.gsec/.map (container only — do not restyle the Leaflet internals)/.fld/.params/.pchip/.gadd` from the mock.
- [ ] Run `pnpm exec vitest run src/request/GribForm.test.tsx src/request/RequestCenter.test.tsx` → green; typecheck → 0. Commit `feat(request): re-skin the GRIB request form`.

## Task 7: Verify, smoke, docs

**Files:** `CHANGELOG.md`; no code.

- [ ] Full `pnpm exec vitest run src/request src/shell/chrome` → all green (the regression gate for the whole re-skin). Reap workers.
- [ ] `pnpm run typecheck` → 0. (No Rust changed → clippy unaffected; CI verifies both arches.)
- [ ] **WebKitGTK grim smoke** (the verification this change most needs): if the dev port `:1420` is free, launch the app, open Message → Request Center, and `grim`-capture the home / browse / GRIB; compare against the mock for fit + proportion (memory `grim-realapp-validation`; NOT Chromium). Fix any clipping/overflow. If `:1420` is contended, flag the smoke for post-merge in the PR + handoff (the static WebKitGTK rules were followed).
- [ ] CHANGELOG Unreleased note: "Request Center — visual redesign (request-first hero + compact chips, line icons, true-window proportions)." Commit `docs(request): CHANGELOG note for the Request Center visual redesign`.

**→ After all tasks: run a review pass (correctness of the markup changes, testid preservation, WebKitGTK fit, voice). Then open the PR (base main); CI verify is the merge gate; grim smoke post-merge if not done pre-merge.**

## Self-review (author)

- Spec coverage: header ✓ (T3), home hero+chips ✓ (T2/T3), browse ✓ (T5), GRIB ✓ (T6), basket incl. empty/result ✓ (T4), icons ✓ (T1), USPS→name ✓ (T3), WebKitGTK/content-cap ✓ (T3 + per-task rule), voice ✓ (binding rule), grim smoke ✓ (T7).
- The one data change (`sections.ts` kind) is isolated in T2 with its own test; everything else is presentation.
- Testid preservation is a per-task binding rule + grep step, since the existing test suite is the regression gate (no new behavior to TDD).
- Risk: a few RequestCenter tests assert the old flat-card DOM; T3/T4 explicitly allow minimal, testid-preserving assertion updates. The grim smoke (T7) is the only check that catches real WebKitGTK fit — flagged as the priority verification.
