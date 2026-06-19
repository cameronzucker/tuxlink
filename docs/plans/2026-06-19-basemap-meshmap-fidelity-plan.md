# Basemap MeshMap-fidelity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the dark basemap match MeshMap's quality (sophisticated, tactical, polished, lightweight), and the light basemap a restrained daylight-legible map, by feeding the existing invert bake a restrained OSM-Carto-class source instead of the punched-up one.

**Architecture:** MeshMap's dark = `invert(1) hue-rotate(180deg) brightness(1.33)` over OSM Carto raster. [`src/map/darkStyle.ts`](../../src/map/darkStyle.ts) already bakes that exact transform GL-natively, and `@protomaps/basemaps` stock layers already supply cased roads, boundaries, buildings, and labels. The drift is entirely the bold color overrides in [`src/map/tuxlinkFlavor.ts`](../../src/map/tuxlinkFlavor.ts). The realization (Option A in the spec) is therefore: set `tuxlinkFlavor` to OSM-Carto-class LIGHT values — which (a) IS the daylight light map and (b) inverts via the existing `darkStyle` to the MeshMap-class dark, by construction. Then tune generalization/onset to OSM Carto, ensure boundaries render, and confirm fidelity with grim on the real app.

**Tech Stack:** TypeScript, MapLibre GL v8 style spec, `@protomaps/basemaps` (`layers()` + `namedFlavor`), Protomaps PMTiles (offline), Vitest, grim (WebKitGTK capture).

## Global Constraints

- Offline-first: NO online raster-tile dependency (MeshMap's raster path is unavailable to us). Verbatim from spec.
- ODbL: render "© OpenStreetMap contributors" attribution.
- Dark mode is a GL-native bake, NOT a runtime CSS filter (R4 spike: ~45fps vs ~15fps on the Pi's software GL).
- Final visual fidelity is confirmed by grim on the actual WebKitGTK app vs MeshMap at multiple zooms; Chromium / OpenFreeMap proxy tiles are NOT acceptable as the fidelity gate.
- Work happens in worktree `worktrees/bd-tuxlink-h17b-basemap-meshmap` (branch `bd-tuxlink-h17b/basemap-meshmap`, off `main`); tracking issue `tuxlink-h17b`. Commit trailers: `Agent: <moniker>`.
- Color anchors (OSM Carto LIGHT source → MeshMap dark via the bake), computed with `darkStyle`'s exact matrix at brightness 1.33:

  | Slot | OSM Carto LIGHT (source) | → baked DARK (target) |
  |---|---|---|
  | earth/land | `#f2efe9` | `#19150d` |
  | residential landuse | `#e0dfdf` | `#2b2a2a` |
  | water | `#aad3df` | `#194f5f` |
  | forest | `#add19e` | `#2b5b18` |
  | park/grass | `#cdebb0` | `#0f3700` |
  | motorway | `#e990a0` | `#d55e73` |
  | trunk | `#f9b29c` | `#a14225` |
  | primary | `#fcd6a4` | `#5d2b00` |
  | secondary | `#f7fabf` | `#101400` |
  | minor/residential | `#ffffff` | `#000000` (recedes) |
  | building | `#d9d0c9` | `#473b31` |
  | label text | `#333333` | `#e8e8e8` (light, readable) |

---

### Task 1: Prove the realization (Option A) by unit-asserting the baked palette

Rationale: resolve the open A-vs-B question with math before touching the live app. If feeding `tuxlinkFlavor` OSM-Carto values yields the computed MeshMap dark palette through the existing bake, Option A is proven and Option B (hand-built dark) is unnecessary.

**Files:**
- Test: `src/map/tuxlinkFlavor.meshmap.test.ts` (create)
- Read: `src/map/darkStyle.ts`, `src/map/tuxlinkFlavor.ts`

**Interfaces:**
- Consumes: `bakeDarkColors(layers)` from `darkStyle.ts`; `xformHex(hex)` from `darkStyle.ts`.
- Produces: confidence that `xformHex(<OSM light slot>)` equals the dark target column above.

- [ ] **Step 1: Write the failing test**

```ts
// src/map/tuxlinkFlavor.meshmap.test.ts
import { describe, it, expect } from 'vitest';
import { xformHex } from './darkStyle';

// OSM-Carto LIGHT source slots -> expected MeshMap-class dark after the bake.
const CASES: Array<[string, string]> = [
  ['#f2efe9', '#19150d'], // earth
  ['#e0dfdf', '#2b2a2a'], // residential landuse
  ['#aad3df', '#194f5f'], // water
  ['#e990a0', '#d55e73'], // motorway -> salmon
  ['#f9b29c', '#a14225'], // trunk -> rust
  ['#333333', '#e8e8e8'], // label text -> light
];

describe('OSM-Carto light source bakes to the MeshMap dark palette', () => {
  it.each(CASES)('xformHex(%s) === %s', (src, dark) => {
    expect(xformHex(src)).toBe(dark);
  });
});
```

- [ ] **Step 2: Run it**

Run: `npx vitest run src/map/tuxlinkFlavor.meshmap.test.ts`
Expected: PASS (this asserts the existing `xformHex` math; if any case is off by ±1, copy the actual `xformHex` output into the expectation — the point is to pin the source→dark mapping, not to change `darkStyle`).

- [ ] **Step 3: Record the A/B decision on the issue**

```bash
bd update tuxlink-h17b --notes "Realization A confirmed: OSM-Carto light source + existing darkStyle bake reproduces the computed MeshMap dark palette (tuxlinkFlavor.meshmap.test.ts). Proceeding with Option A (restrained source + keep invert); Option B (hand-built dark) not needed."
```

- [ ] **Step 4: Commit**

```bash
git add src/map/tuxlinkFlavor.meshmap.test.ts
git commit -m "test(map): pin OSM-Carto light -> MeshMap dark bake (realization A)

Agent: <moniker>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Restrain `tuxlinkFlavor` to an OSM-Carto-class LIGHT palette

**Files:**
- Modify: `src/map/tuxlinkFlavor.ts` (replace the `TUXLINK_FLAVOR_OVERRIDES` road/earth/water values with OSM-Carto-class values)
- Test: `src/map/tuxlinkFlavor.test.ts` (extend)

**Interfaces:**
- Consumes: `namedFlavor('light')` from `@protomaps/basemaps`.
- Produces: `tuxlinkFlavor()` returning a restrained light flavor whose road slots are warm-but-restrained (OSM-Carto values), consumed by `basemapStyle.baseLayers()`.

- [ ] **Step 1: Write the failing test** — assert the loud values are gone and restrained ones present.

```ts
// add to src/map/tuxlinkFlavor.test.ts
import { TUXLINK_FLAVOR_OVERRIDES } from './tuxlinkFlavor';
it('uses restrained OSM-Carto-class road values, not the punched-up ramp', () => {
  expect(TUXLINK_FLAVOR_OVERRIDES.highway).not.toBe('#e85d3a'); // old loud value gone
  expect(TUXLINK_FLAVOR_OVERRIDES.highway).toBe('#e990a0');     // OSM motorway
  expect(TUXLINK_FLAVOR_OVERRIDES.major).toBe('#fcd6a4');       // OSM primary (tan, not orange)
  expect(TUXLINK_FLAVOR_OVERRIDES.earth).toBe('#f2efe9');
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `npx vitest run src/map/tuxlinkFlavor.test.ts`
Expected: FAIL (current `highway` is `#e85d3a`).

- [ ] **Step 3: Replace the overrides** in `src/map/tuxlinkFlavor.ts` with OSM-Carto-class values:

```ts
export const TUXLINK_FLAVOR_OVERRIDES: Record<string, string> = {
  background: '#f2efe9',
  earth: '#f2efe9',
  water: '#aad3df',
  wood_a: '#add19e', wood_b: '#a3c995',
  park_a: '#cdebb0', park_b: '#c2e6a0',
  scrub_a: '#c8d7ab', scrub_b: '#bccf9c',
  sand: '#f5e9c6', beach: '#f5e9c6', glacier: '#e8f0f5',
  // Road network — OSM-Carto warm ramp (restrained; inverts to MeshMap salmon/rust/gold).
  highway: '#e990a0', highway_casing_early: '#d4748a', highway_casing_late: '#d4748a',
  major: '#fcd6a4', major_casing_early: '#e0b070', major_casing_late: '#e0b070',
  minor_a: '#f7fabf', minor_b: '#ffffff', minor_casing: '#cfcf9a',
  minor_service: '#ffffff', minor_service_casing: '#d6d6d6',
  link: '#fcd6a4', link_casing: '#e0b070',
  other: '#ffffff',
  buildings: '#d9d0c9', railway: '#b0a394', boundaries: '#ac46ac', pier: '#e0ddd5',
};
```

- [ ] **Step 4: Run tests** — both `tuxlinkFlavor.test.ts` and `tuxlinkFlavor.meshmap.test.ts`.

Run: `npx vitest run src/map/tuxlinkFlavor.test.ts src/map/tuxlinkFlavor.meshmap.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/map/tuxlinkFlavor.ts src/map/tuxlinkFlavor.test.ts
git commit -m "feat(map): restrain tuxlinkFlavor to OSM-Carto-class light palette

Drops the punched-up road ramp; inverting this source via darkStyle now
yields the MeshMap-class dark, and the light flavor itself is a restrained
daylight map. Root fix for the basemap drift (tuxlink-h17b).

Agent: <moniker>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: True the generalization + onset schedule to OSM Carto

Extends the existing `generalizeRoadDensity` in `basemapStyle.ts` so minor classes gate to a neighborhood zoom and arterials stay proportionate (no boldening at zoom-out). Keep highways ungated.

**Files:**
- Modify: `src/map/basemapStyle.ts` (`generalizeRoadDensity`, `MINOR_ROAD_MINZOOM`, add `MAJOR_ROAD_MINZOOM` if arterials need a floor)
- Test: `src/map/basemapStyle.test.ts` (extend)

**Interfaces:**
- Consumes: `layers()` output from `@protomaps/basemaps` (layer ids `roads_minor`, `roads_major`, `roads_highway`, `roads_*_casing`, `roads_tunnels_*`, `roads_bridges_*`).
- Produces: a layer array with minzoom floors matching OSM Carto onset (minor z12–13, arterials proportionate, highways ungated).

- [ ] **Step 1: Write the failing test**

```ts
// add to src/map/basemapStyle.test.ts
import { buildBasemapStyle } from './basemapStyle';
it('gates minor roads to a neighborhood zoom and keeps highways ungated', () => {
  const layers = buildBasemapStyle('light').layers;
  const minor = layers.find(l => l.id === 'roads_minor');
  const hwy = layers.find(l => l.id === 'roads_highway');
  expect(minor?.minzoom).toBeGreaterThanOrEqual(12);
  expect(hwy?.minzoom ?? 0).toBeLessThanOrEqual(6);
});
```

- [ ] **Step 2: Run to verify current behavior**

Run: `npx vitest run src/map/basemapStyle.test.ts`
Expected: PASS or FAIL depending on current floors; if FAIL, adjust `MINOR_ROAD_MINZOOM`/regex in Step 3.

- [ ] **Step 3: Adjust** `generalizeRoadDensity` floors in `basemapStyle.ts` so the schedule matches the spec table (minor z12, tertiary/other z11–12; arterials keep proportionate widths; highways ungated). Confirm `MINOR_ROAD_RE` matches the real protomaps ids (`roads_minor`, `roads_minor_service`, `roads_link`, `roads_other`, tunnels/bridges variants).

- [ ] **Step 4: Run tests**

Run: `npx vitest run src/map/basemapStyle.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit** (`feat(map): true road generalization/onset to OSM Carto schedule`, with trailers).

---

### Task 4: Ensure admin boundaries render (state + national)

The operator reports no borders. Protomaps `layers()` emits a `boundaries` layer; verify it survives the build and is styled visibly (light line on dark after the bake).

**Files:**
- Modify: `src/map/basemapStyle.ts` (only if boundaries are dropped/over-muted)
- Modify: `src/map/tuxlinkFlavor.ts` (`boundaries` slot — already set to `#ac46ac` in Task 2, which bakes to a visible tone)
- Test: `src/map/basemapStyle.test.ts` (extend)

**Interfaces:**
- Produces: a boundary line layer present in both `light` and `dark` styles.

- [ ] **Step 1: Write the failing test**

```ts
it('includes admin boundary layers in both flavors', () => {
  for (const flavor of ['light', 'dark'] as const) {
    const ids = buildBasemapStyle(flavor).layers.map(l => l.id);
    expect(ids.some(id => /boundar/i.test(id))).toBe(true);
  }
});
```

- [ ] **Step 2: Run** — `npx vitest run src/map/basemapStyle.test.ts`. If FAIL, the protomaps boundary layers are being filtered out; fix the assembly to keep them.

- [ ] **Step 3: Implement** the minimal fix (only if needed) to retain boundary layers.

- [ ] **Step 4: Run tests** — Expected: PASS.

- [ ] **Step 5: Commit** (`fix(map): ensure admin boundaries render in both flavors`, trailers).

---

### Task 5: Building contrast pass

Buildings should read clearly when zoomed in (z13+), with higher contrast than the canvas.

**Files:**
- Modify: `src/map/tuxlinkFlavor.ts` (`buildings` slot — `#d9d0c9` set in Task 2, bakes to `#473b31`; lighten the source if buildings read too dark)
- Test: `src/map/basemapStyle.test.ts` (assert building layer minzoom ≤ 14)

- [ ] **Step 1: Write the failing test**

```ts
it('renders buildings by z14', () => {
  const b = buildBasemapStyle('dark').layers.find(l => /building/i.test(l.id));
  expect(b).toBeTruthy();
  expect(b!.minzoom ?? 99).toBeLessThanOrEqual(14);
});
```

- [ ] **Step 2: Run** — adjust if FAIL.
- [ ] **Step 3: Implement** any needed building minzoom/contrast change.
- [ ] **Step 4: Run tests** — PASS.
- [ ] **Step 5: Commit** (trailers).

---

### Task 6: grim fidelity validation vs MeshMap (operator-run visual gate)

The automated tests pin structure + palette math; this task confirms the *look* on the real renderer. Operator-run (build + capture); agent assesses captures.

**Files:** none (validation). Captures go to `dev/scratch/` (in-workspace, per convention).

- [ ] **Step 1:** Operator builds + launches the dev app (dark mode) on a metro view matching a MeshMap reference (e.g. central LA / Phoenix).
- [ ] **Step 2:** Capture: `grim -t png dev/scratch/h17b-dark-<zoom>.png` at z7, z10, z12, z14.
- [ ] **Step 3:** Compare against MeshMap reference `/tmp/meshmap-ref.png` + the spec palette. Check: neutral gray canvas (not brown), olive greens present at zoom-out, salmon freeways / rust arterials / receding minors, borders visible, buildings legible, no orange spaghetti, roads not over-bold at zoom-out.
- [ ] **Step 4:** Repeat for light mode (daylight legibility check).
- [ ] **Step 5:** File any residual-fidelity deltas as follow-up notes on `tuxlink-h17b`; iterate Tasks 2–5 values as needed (palette tuning is cheap — re-run the unit tests, re-grim).

---

### Task 7 (follow-up, separate plan): dedicated light-mode tuning

If OSM-Carto-class light needs more bright-sun contrast for field use beyond the daylight default, spin a focused light-mode plan. Out of scope here; the dark mode is the priority and the light mode is already daylight-legible from Task 2.

---

## Self-Review

- **Spec coverage:** dark palette (Tasks 1–2), structural road rules / casing (inherited from protomaps stock + Task 3), onset schedule (Task 3), borders (Task 4), building contrast (Task 5), neutral canvas + greens (Task 2 anchors), labels readable (bake of dark source text → light, pinned in Task 1), decoupled light mode (Task 2 light flavor + Task 7), validation via grim (Task 6), A-vs-B resolved (Task 1). Covered.
- **Placeholder scan:** none — values, paths, and tests are concrete.
- **Type consistency:** uses existing exports `xformHex`, `bakeDarkColors`, `buildBasemapStyle`, `TUXLINK_FLAVOR_OVERRIDES`, `tuxlinkFlavor` — all present on `main`.
- **Note:** the structural "cased roads" look is inherited from `@protomaps/basemaps` stock casings (the drift was color, not missing casing); Task 3 only tunes density/onset, not casing structure. If grim (Task 6) shows casings missing, add a casing-presence assertion + fix as a Task-3 follow-up.
