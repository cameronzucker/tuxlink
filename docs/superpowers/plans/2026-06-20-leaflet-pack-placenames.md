# Leaflet Pack Placenames Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make an installed region pack render its own placenames so it becomes the detailed local map within its coverage on the Leaflet engine.

**Architecture:** In `buildBaseLayers` (basemapLeaflet.ts) each region-pack layer currently passes `labelRules: []`. Change it to the flavor's real label rules (`pmLabelRules(namedFlavor(flavor), 'en')`) — the same source the overview uses. The pack's already-drawn opaque `earth` polygon occludes the overview's labels beneath it, so the pack's own labels become the only land labels in coverage (no double-labeling, no new clipping logic). The overview layer is unchanged and fills outside coverage.

**Tech Stack:** TypeScript, React, Vite, Vitest, the vendored `protomaps-leaflet` 5.1.0 bundle, `@protomaps/basemaps` flavors, Leaflet.

## Global Constraints

- Frontend tooling is `pnpm`. Run frontend tests with `pnpm vitest run`.
- Commit messages MUST carry `Agent: glade-gulch-fern` and the `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` trailers (commit-msg hook enforces the Agent trailer).
- Conventional commit types; this is a user-facing behavior change → `feat(map): …`.
- No Rust, no download-pipeline, no `LeafletMap.tsx` changes — those hops are engine-agnostic and already correct.
- `.github/RELEASE_FREEZE` stays in place; do not touch it.
- Branch is `bd-tuxlink-c973/pack-labels` (already created off post-#846 main). Worktree: `worktrees/bd-tuxlink-c973-placenames-packs`.

---

### Task 1: Pack layers carry the flavor's label rules

**Files:**
- Modify: `src/map/basemapLeaflet.ts` (the `buildBaseLayers` pack-layer block, ~lines 131–150, plus the import on line 43)
- Test: `src/map/basemapLeaflet.test.ts` (the hoisted `pmLabelRulesSpy` mock + the pack test, ~lines 13 and 54–69)

**Interfaces:**
- Consumes: the vendored `protomaps-leaflet` export `labelRules(t: Flavor, lang: string): LabelRule[]` (imported aliased as `pmLabelRules`), and `namedFlavor(flavor)` from `@protomaps/basemaps` (already imported in basemapLeaflet.ts).
- Produces: each pack `leafletLayer` options object now has `labelRules` set to `pmLabelRules(namedFlavor(flavor), 'en')` instead of `[]`. No exported signature changes — `buildBaseLayers(flavor, packs)` is unchanged.

- [ ] **Step 1: Update the test mock and pack assertion to require the flavor's label rules (failing test)**

In `src/map/basemapLeaflet.test.ts`, change the hoisted `pmLabelRulesSpy` to return a non-empty sentinel so the wiring is observable (it currently returns `[]`, which would mask the change):

```typescript
// in the vi.hoisted(() => ({ ... })) block, replace the pmLabelRulesSpy line:
pmLabelRulesSpy: vi.fn(() => [{ dataLayer: 'places', symbolizer: {} }]),
```

Then replace the pack `labelRules` assertion (currently `expect(p.labelRules).toEqual([])`) in the pack test with assertions that the pack carries the flavor's label rules:

```typescript
// pack now owns its placenames in-coverage; labels come from the SAME flavor as
// its paint rules (the opaque earth occludes the overview's labels beneath, so
// no double-labeling — see 2026-06-20-leaflet-pack-placenames-design.md).
expect(p.labelRules).toEqual([{ dataLayer: 'places', symbolizer: {} }]);
expect(pmLabelRulesSpy).toHaveBeenCalledWith({ __flavor: 'dark' }, 'en');
```

Also add `pmLabelRulesSpy` to the `vi.hoisted` destructuring return and to the `beforeEach` `mockClear()` list, and reference it in the test body. The destructure at the top already lists the other spies; add `pmLabelRulesSpy` there too. Add to `beforeEach`:

```typescript
pmLabelRulesSpy.mockClear();
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm vitest run src/map/basemapLeaflet.test.ts`
Expected: FAIL — the pack test asserts `labelRules` equals `[{ dataLayer: 'places', symbolizer: {} }]` but the code still passes `[]`, and `pmLabelRulesSpy` was never called.

- [ ] **Step 3: Wire the flavor's label rules into pack layers**

In `src/map/basemapLeaflet.ts`, add the import alias on line 43:

```typescript
import { leafletLayer, paintRules as pmPaintRules, labelRules as pmLabelRules } from '../vendor/protomaps-leaflet';
```

In the pack `.map(...)` block inside `buildBaseLayers`, replace `labelRules: []` (line ~140) and update the preceding comment:

```typescript
        // Explicit paint rules from the SAME flavor, but NO `flavor`/`backgroundColor`
        // (so empty pack tiles never mask the overview, R2 P0#3). The pack DOES carry
        // the flavor's label rules: in coverage its opaque `earth` occludes the
        // overview's labels beneath it, so the pack's own z6-13 placenames are the
        // only land labels — the pack is the detailed local map, no double-labeling
        // (tuxlink-c973; 2026-06-20-leaflet-pack-placenames-design.md).
        paintRules: pmPaintRules(namedFlavor(flavor)),
        labelRules: pmLabelRules(namedFlavor(flavor), 'en'),
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm vitest run src/map/basemapLeaflet.test.ts`
Expected: PASS — pack `labelRules` now equals the sentinel and `pmLabelRules` was called with `namedFlavor('dark')` and `'en'`.

- [ ] **Step 5: Run the full map suite + typecheck (no regressions)**

Run: `pnpm vitest run src/map/ && pnpm typecheck`
Expected: PASS — all map tests green (including `basemapLeaflet.labelcap.test.ts` from #846), no type errors. The label-cap test is unaffected; the overview cap is still 256.

- [ ] **Step 6: Commit**

```bash
git add src/map/basemapLeaflet.ts src/map/basemapLeaflet.test.ts
git commit -m "$(cat <<'EOF'
feat(map): region packs render their own placenames on Leaflet (tuxlink-c973)

Pack layers now carry the flavor's label rules instead of labelRules:[], so an
installed pack is the detailed local map within its coverage — its own z6-13
placenames (city/neighborhood/landmark). The pack's opaque earth already
occludes the overview's labels beneath it, so pack labels are the only land
labels: no double-labeling, no new clipping. Overview unchanged, fills outside
coverage. Implements the 2026-06-20-leaflet-pack-placenames design.

Agent: glade-gulch-fern
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Acceptance (post-implementation, not a code task)

Automated gates (merge gate): `pnpm typecheck`, `pnpm vitest run`, `pnpm build` — all green; open the PR and let CI run the full matrix.

**Operator end-to-end smoke** (validates the never-exercised Leaflet download path + this change together): operator downloads one small region pack and confirms, inside coverage, the map renders as the detailed local map with placenames (city/neighborhood/landmark), flavor colors, no double labels, smooth zoom. If large-water labels double up, build the reserved fallback (cap the overview's label rules to `maxzoom 6`) — out of scope unless smoke shows it. If zoom is heavy, reduce label density rather than change architecture.

## Self-Review

- **Spec coverage:** the spec's single decision (pack = detailed local map via pack-owned label rules) → Task 1. No-double-label reasoning → encoded in the comment + relies on existing earth occlusion (no code needed). Download verification → Acceptance smoke. Reserved fallback + perf → Acceptance notes. No spec requirement is left without a task.
- **Placeholder scan:** none — every step shows exact code/commands.
- **Type consistency:** `pmLabelRules` (alias of the vendored `labelRules`) is used consistently; the sentinel `[{ dataLayer: 'places', symbolizer: {} }]` matches the `LabelRule` shape (`dataLayer` + `symbolizer`); `namedFlavor('dark')` returns `{ __flavor: 'dark' }` per the existing mock, matching the `toHaveBeenCalledWith` assertion.
