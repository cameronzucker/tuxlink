# Offline-First Map Foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Source of truth:** [`docs/design/2026-06-08-offline-map-foundation-approach.md`](../design/2026-06-08-offline-map-foundation-approach.md) (design + BINDING adversarial corrections C1–C15) and the locked spec [`docs/design/2026-06-07-map-pin-grid-design.md`](../design/2026-06-07-map-pin-grid-design.md). This plan operationalizes them; if they conflict, the approach-doc corrections win.

**Goal:** An offline-first map location picker (pin + GRIB-box modes) that renders a bundled static world map with NO network, reuses the existing Maidenhead converter, and retrofits the shipped compose widget + CSP so the app never ingests public OpenStreetMap tiles.

**Architecture:** Leaflet/react-leaflet v5 with `crs={L.CRS.EPSG4326}` (equirectangular → linear pixel↔lat/lon) and a bundled `<ImageOverlay>` world PNG served from `'self'`. All projection / grid-geometry / GRIB-bbox math is extracted into **pure functions** unit-tested in jsdom; Leaflet components are tested at the module-mock boundary for shape only; real map rendering/click/drag correctness is verified **only** via grim on real Tauri WebKitGTK. Bundled-only scope: zero tile-server affordance (the opt-in permitted server + Rust gatekeeper is split to `tuxlink-dyop`, NOT this PR).

**Tech Stack:** React 18 + TypeScript, react-leaflet ^5.0.0 / leaflet ^1.9.4 (already installed), Vitest+jsdom, Tauri 2.x (Rust backend untouched by this PR except CSP config). Vendored Natural Earth PNG (public domain).

---

## ⛔ Non-negotiables (every task obeys)

1. **`vitest green ≠ map-correct` (C1).** jsdom CANNOT render Leaflet (it's mocked at the module boundary — see `src/compose/PositionMapWidget.test.tsx:1-20`). Test pure math in vitest; test components at the mock boundary for *shape only*; verify real projection/click/drag/layout **only** via grim on WebKitGTK. Any test that claims to prove projection correctness through the Leaflet mock is theater — do not write it.
2. **No RF/transmit path** anywhere here (GRIB requests queue to the outbox; the map is local). RADIO-1 does NOT gate (verified: no `command::Send`/transmit call is added). Agents may run `pnpm vitest`, `pnpm tauri dev`, and grim.
3. **Branch from `origin/main`; work in this worktree** (`worktrees/bd-tuxlink-z9u4-offline-map-foundation`, already at `origin/main` HEAD). NEVER the 639-behind recovery checkout. Pin `pnpm -C <worktree>` / `cargo --manifest-path <worktree>/src-tauri/Cargo.toml`.
4. **CSP stays `'self'` for tiles.** No task may add an external `img-src`/`connect-src` tile host. The post-remediation CSP is EXACTLY (C5): `default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:; style-src 'self' 'unsafe-inline'`.
5. **grim, not Chromium** (`feedback_chromium_not_webkitgtk_proxy`). Restart `pnpm tauri dev` to load frontend changes — Ctrl+R is a no-op in the webview.
6. **Reuse, don't rebuild:** the Maidenhead converter `src/forms/position/maidenhead.ts` (`gridToLatLon`, `latLonToGrid`) is the ONE converter. Do not add a second.

## File structure (created / modified)

```
src/map/                                  ← NEW subsystem
  assets/world-equirect-2048.png          ← Task 0 vendored PD asset
  assets/world-equirect.CREDITS.md        ← Task 0 provenance
  leafletIconFix.ts                        ← Task 4 (C8) shared marker-icon side-effect
  projection.ts + projection.test.ts       ← Task 1 pure EPSG4326 px↔latlon/bbox
  gridGeometry.ts + gridGeometry.test.ts   ← Task 2 pure Maidenhead overlay geometry
  gribRegion.ts + gribRegion.test.ts       ← Task 3 pure signed-bbox→GRIB region
  BaseMap.tsx + BaseMap.test.tsx           ← Task 4 shared map substrate
  MaidenheadOverlay.tsx + .test.tsx        ← Task 5 grid overlay component
  GridMapPicker.tsx + .test.tsx            ← Task 6 pin + box-drag picker
src/grib/GribRequestPanel.tsx (+ test)     ← Task 7 wire box mode (tuxlink-mxmx / item 21)
src/compose/PositionMapWidget.tsx (+ test) ← Task 8 retrofit to BaseMap (tuxlink-714t)
src/compose/positionMapCsp.test.ts         ← Task 8 INVERT (C4)
src-tauri/tauri.conf.json                  ← Task 8 CSP revert (C5)
docs/design/2026-06-07-map-pin-grid-design.md ← Task 9 appended Correction (C13)
```

## Task sequencing (C11 — record bd dep edges; respect ordering)

```
Task 0 (asset) ─┐
Task 1 (projection.ts) ──┐
Task 2 (gridGeometry.ts) ─┤ pure fns — fully parallel, no Leaflet
Task 3 (gribRegion.ts) ──┘
        ↓ (Task 4 needs the asset + projection.ts)
Task 4 (<BaseMap> — FREEZE its prop contract here)
        ↓ (everything below consumes BaseMap)
Task 5 (overlay) · Task 6 (picker) · Task 8 (remediation)  ← parallel AFTER Task 4
        ↓ (Task 7 needs GridMapPicker box mode + gribRegion.ts)
Task 7 (GRIB wiring)
Task 9 (grounding correction + docs) — any time
```
**Subagent rule:** Task 4's `BaseMapProps` interface is the frozen contract. Tasks 5/6/8 must not change it; if one needs a new prop, it stops and coordinates. Two parallel worktrees → `bd dep add` edges already recorded (`mxmx`,`urbv`,`714t`,`dyop` → `z9u4`).

---

## Task 0: Vendor the bundled world-map asset (C12)

**Files:**
- Create: `src/map/assets/world-equirect-2048.png` (binary)
- Create: `src/map/assets/world-equirect.CREDITS.md`

- [ ] **Step 1 — Obtain the asset.** Download Natural Earth **"Natural Earth II with Shaded Relief, Water, and Drainages", 1:50m** raster (public domain — Natural Earth places all versions in the public domain, no attribution required). Source: `https://www.naturalearthdata.com/downloads/50m-raster-data/50m-natural-earth-2/`. It is a full-globe **equirectangular (plate carrée)** raster covering exactly `[-180,180]×[-90,90]` with NO crop — this is required for the EPSG4326 overlay bounds to align.
- [ ] **Step 2 — Resize + optimize to a fixed target.** Produce a `2048×1024` PNG (exact 2:1 plate-carrée aspect), then optimize:
  ```bash
  cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-z9u4-offline-map-foundation
  # from the downloaded NE2 tif/png named $SRC:
  convert "$SRC" -resize 2048x1024! src/map/assets/world-equirect-2048.png
  oxipng -o4 --strip safe src/map/assets/world-equirect-2048.png   # or: pngquant --quality=70-90
  ls -l src/map/assets/world-equirect-2048.png   # expect < 600 KB; hard ceiling 1.5 MB
  sha256sum src/map/assets/world-equirect-2048.png
  ```
  If > 1.5 MB after optimization, reduce palette (`pngquant --quality=60-85`) — do NOT drop below 2048 px wide (precision floor).
- [ ] **Step 3 — Record provenance** in `src/map/assets/world-equirect.CREDITS.md`: exact NE product name + version + download URL, the `convert`/`oxipng` commands, output dimensions `2048×1024`, byte size, and the sha256. Credit text: "Map data: Natural Earth (public domain, naturalearthdata.com)." (Attribution optional for PD, included as good practice.)
- [ ] **Step 4 — Verify the import emits a file URL** (mirrors `src/assets/tuxlink-icon.png` ← `src/help/HelpTitleBar.tsx:16`). Vite's default `assetsInlineLimit` is 4096 bytes, so a >600 KB PNG emits a hashed file served from `'self'` (matches `img-src 'self'`). Do not shrink it under 4 KB.
- [ ] **Step 5 — Commit.**
  ```bash
  git add src/map/assets/world-equirect-2048.png src/map/assets/world-equirect.CREDITS.md
  git commit -m "feat(map): vendor public-domain equirectangular world map asset"
  ```

---

## Task 1: Pure EPSG4326 projection math (`projection.ts`) — TDD

**Why pure:** Under `L.CRS.EPSG4326` (plate carrée) pixel↔lat/lon is LINEAR, so the load-bearing projection is a pure function testable in jsdom WITHOUT Leaflet (C1). The Leaflet component only *calls* this; the mock would replace `map.mouseEventToLatLng`, so we must not rely on it for correctness.

**Files:** Create `src/map/projection.ts`, `src/map/projection.test.ts`.

- [ ] **Step 1 — Failing test.**
```ts
// src/map/projection.test.ts
import { describe, expect, it } from 'vitest';
import { pixelToLatLon, latLonToPixel, clampLatLon, WORLD_BOUNDS } from './projection';

describe('EPSG4326 projection (plate carrée, linear)', () => {
  const W = 2048, H = 1024;
  it('maps image corners to world corners', () => {
    expect(pixelToLatLon(0, 0, W, H)).toEqual({ lat: 90, lon: -180 });      // top-left
    expect(pixelToLatLon(W, H, W, H)).toEqual({ lat: -90, lon: 180 });      // bottom-right
  });
  it('maps image center to (0,0)', () => {
    expect(pixelToLatLon(W / 2, H / 2, W, H)).toEqual({ lat: 0, lon: 0 });
  });
  it('round-trips pixel→latlon→pixel', () => {
    const px = 512, py = 300;
    const { lat, lon } = pixelToLatLon(px, py, W, H);
    const back = latLonToPixel(lat, lon, W, H);
    expect(back.x).toBeCloseTo(px, 6);
    expect(back.y).toBeCloseTo(py, 6);
  });
  it('clamps out-of-range coordinates to the world rectangle', () => {
    expect(clampLatLon(95, 200)).toEqual({ lat: 90, lon: 180 });
    expect(clampLatLon(-95, -200)).toEqual({ lat: -90, lon: -180 });
  });
  it('exposes WORLD_BOUNDS as [[south,west],[north,east]] for ImageOverlay/maxBounds', () => {
    expect(WORLD_BOUNDS).toEqual([[-90, -180], [90, 180]]);
  });
});
```
- [ ] **Step 2 — Run, verify FAIL** (`pnpm -C <wt> exec vitest run src/map/projection.test.ts` → "Cannot find module './projection'").
- [ ] **Step 3 — Implement.**
```ts
// src/map/projection.ts
/** EPSG4326 plate-carrée: lon is linear in x over [-180,180]; lat is linear in y over [90,-90]. */
export interface LatLon { lat: number; lon: number; }
export const WORLD_BOUNDS: [[number, number], [number, number]] = [[-90, -180], [90, 180]];

export function pixelToLatLon(px: number, py: number, width: number, height: number): LatLon {
  const lon = (px / width) * 360 - 180;
  const lat = 90 - (py / height) * 180;
  return clampLatLon(lat, lon);
}
export function latLonToPixel(lat: number, lon: number, width: number, height: number): { x: number; y: number } {
  return { x: ((lon + 180) / 360) * width, y: ((90 - lat) / 180) * height };
}
export function clampLatLon(lat: number, lon: number): LatLon {
  return { lat: Math.min(90, Math.max(-90, lat)), lon: Math.min(180, Math.max(-180, lon)) };
}
```
- [ ] **Step 4 — Run, verify PASS.**

**Type note:** `src/map/projection.ts` owns the map-subsystem `LatLon`. It is intentionally structurally identical to (and assignment-compatible with) `src/forms/position/maidenhead.ts`'s `LatLon` — do NOT cross-import; keep each module's `LatLon` local to avoid a coupling edge between subsystems (structural typing makes them interchangeable at call sites).

- [ ] **Step 5 — Commit** (`feat(map): pure EPSG4326 projection helpers`).

---

## Task 2: Pure Maidenhead grid-overlay geometry (`gridGeometry.ts`) — TDD

**Files:** Create `src/map/gridGeometry.ts`, `src/map/gridGeometry.test.ts`. Pure geometry (no Leaflet): given visible bounds + a "level" (field 20°×10° / square 2°×1°), return the lat/lon lines + cell labels to draw. The component (Task 5) renders these as SVG `<Polyline>`/labels.

- [ ] **Step 1 — Failing test** asserting: field-level vertical lines at lon −180,−160,…,180 (step 20) and horizontal at lat −90,−80,…,90 (step 10). **Label derivation:** a field cell's label is the 2-char field of its CENTER. Define cell center precisely: a field cell whose SW corner is at (lonLine `L`, latLine `M`) has center `(M+5, L+10)`; its label is `latLonToGrid(M+5, L+10).slice(0,2)`. Concrete check: the field cell containing the origin has SW corner (−10,−10) [the lines at or below 0], center `(−5+5,−10+10)`→ actually the cell straddling 0 has SW corner at lat line 0-step... so assert directly: `latLonToGrid(5,10).slice(0,2) === 'JJ'` (NOT 'JI' — both field chars are 'J' at ~(5,10)). Square cells use `.slice(0,4)`. Use the real `latLonToGrid` from `../forms/position/maidenhead` (pure, jsdom-safe). Add ONE boundary-cell label assertion (e.g. a cell near 179.9E) so the `maidenhead.ts` lon-clamp (179.999) binning is covered. Cover: world window → field-level lines; a zoomed window (lon [−2,2], lat [−1,1]) → square-level lines at step 2/1.
```ts
// src/map/gridGeometry.test.ts (shape)
import { describe, expect, it } from 'vitest';
import { gridLines, GridLevel } from './gridGeometry';
describe('maidenhead overlay geometry', () => {
  it('world view → field lines at 20°/10° spacing', () => {
    const g = gridLines({ south: -90, west: -180, north: 90, east: 180 }, GridLevel.Field);
    expect(g.lonLines).toContain(-180); expect(g.lonLines).toContain(0); expect(g.lonLines).toContain(160);
    expect(g.latLines).toContain(-90); expect(g.latLines).toContain(0); expect(g.latLines).toContain(80);
  });
  it('clips lines to the visible window', () => {
    const g = gridLines({ south: -1, west: -2, north: 1, east: 2 }, GridLevel.Square);
    expect(Math.min(...g.lonLines)).toBeGreaterThanOrEqual(-2);
    expect(Math.max(...g.lonLines)).toBeLessThanOrEqual(2);
  });
});
```
- [ ] **Step 2 — Verify FAIL. Step 3 — Implement** `gridLines(bounds, level)` returning `{ lonLines: number[]; latLines: number[]; labels: {lat,lon,text}[] }` using a step table (`Field`: 20/10, `Square`: 2/1) and `Math.ceil/floor` to clip to the window; labels via the first 2 (field) or 4 (square) chars of `latLonToGrid(cellCenterLat, cellCenterLon)`. **Step 4 — Verify PASS. Step 5 — Commit** (`feat(map): pure maidenhead overlay geometry`).

---

## Task 3: Pure signed-bbox → GRIB region (`gribRegion.ts`) — TDD (C3 — correctness-critical)

**Why:** A map drag yields TWO signed decimal corners in arbitrary order; `GribRequest` needs whole-degree `{degrees,dir}` fields, ordered, non-degenerate (Rust `composer.rs` does NOT reorder and rejects equal ranges). The default `40N,60N,140W,120W` fixes the convention: **lat0 = south, lat1 = north, lon0 = west, lon1 = east.**

**Files:** Create `src/map/gribRegion.ts`, `src/map/gribRegion.test.ts`. (`Latitude`/`Longitude` types come from `../grib/types`.)

- [ ] **Step 1 — Failing test** (covers the convention + every edge the adrev named):
```ts
// src/map/gribRegion.test.ts
import { describe, expect, it } from 'vitest';
import { signedToLatitude, signedToLongitude, signedBboxToGribRegion } from './gribRegion';

describe('signed coord → {degrees,dir}', () => {
  it('hemispheres', () => {
    expect(signedToLatitude(33.7)).toEqual({ degrees: 34, dir: 'N' });   // round to whole deg
    expect(signedToLatitude(-33.2)).toEqual({ degrees: 33, dir: 'S' });
    expect(signedToLongitude(-118.4)).toEqual({ degrees: 118, dir: 'W' });
    expect(signedToLongitude(118.6)).toEqual({ degrees: 119, dir: 'E' });
  });
  it('zero is canonical N / E (not S / W)', () => {
    expect(signedToLatitude(0)).toEqual({ degrees: 0, dir: 'N' });
    expect(signedToLongitude(0)).toEqual({ degrees: 0, dir: 'E' });
  });
});
describe('signedBboxToGribRegion (two signed corners → ordered, whole-degree, non-degenerate)', () => {
  it('normalizes corner order to south/north + west/east and floors/ceils OUTWARD', () => {
    // dragged NE→SW: cornerA = (60.2N,120.9W) cornerB = (40.8N,140.1W)
    const r = signedBboxToGribRegion({ lat: 60.2, lon: -120.9 }, { lat: 40.8, lon: -140.1 });
    expect(r.lat0).toEqual({ degrees: 40, dir: 'N' });   // south, floor outward
    expect(r.lat1).toEqual({ degrees: 61, dir: 'N' });   // north, ceil outward
    expect(r.lon0).toEqual({ degrees: 141, dir: 'W' });  // west (more-negative), expand outward
    expect(r.lon1).toEqual({ degrees: 120, dir: 'W' });  // east
  });
  it('expands a sub-degree drag so the region is never degenerate', () => {
    const r = signedBboxToGribRegion({ lat: 40.2, lon: -120.2 }, { lat: 40.6, lon: -120.6 });
    expect(r.lat0.degrees).toBeLessThan(r.lat1.degrees);   // not equal → composer accepts
    expect(r.lon0).toEqual({ degrees: 121, dir: 'W' });
    expect(r.lon1).toEqual({ degrees: 120, dir: 'W' });
  });
  it('handles equator/prime-meridian spanning boxes', () => {
    const r = signedBboxToGribRegion({ lat: -5.3, lon: -3.1 }, { lat: 5.3, lon: 3.1 });
    expect(r.lat0).toEqual({ degrees: 6, dir: 'S' });
    expect(r.lat1).toEqual({ degrees: 6, dir: 'N' });
    expect(r.lon0).toEqual({ degrees: 4, dir: 'W' });
    expect(r.lon1).toEqual({ degrees: 4, dir: 'E' });
  });
});
```
- [ ] **Step 1b — Add the boundary/degeneracy failing tests** the bbox path needs (composer.rs rejects `degrees>90`/`>180` and rejects equal ranges):
```ts
  it('clamps a near-pole / near-antimeridian box to ≤90 / ≤180 (composer rejects over-range)', () => {
    const r = signedBboxToGribRegion({ lat: 89.6, lon: 179.6 }, { lat: 88.2, lon: 178.1 });
    expect(r.lat1.degrees).toBeLessThanOrEqual(90);   // ceil(89.6)=90, clamped, OK
    expect(r.lon1.degrees).toBeLessThanOrEqual(180);
  });
  it('never emits a degenerate (equal) range — even for an integer-aligned/zero drag', () => {
    const r = signedBboxToGribRegion({ lat: 40, lon: -120 }, { lat: 40, lon: -120 });
    expect(r.lat0.degrees).not.toEqual(r.lat1.degrees);   // floor(40)==ceil(40) would collapse → guard must expand
    expect(r.lon0.degrees).not.toEqual(r.lon1.degrees);
  });
```
- [ ] **Step 2 — Verify FAIL. Step 3 — Implement.** `signedToLatitude`/`signedToLongitude`: `abs + Math.round`, dir from sign with **0 → N / E** (not S/W), then clamp degrees to 90/180. `signedBboxToGribRegion(a, b)`:
  1. `south = Math.max(-90, Math.floor(Math.min(latA,latB)))`, `north = Math.min(90, Math.ceil(Math.max(latA,latB)))`;
     `west = Math.max(-180, Math.floor(Math.min(lonA,lonB)))`, `east = Math.min(180, Math.ceil(Math.max(lonA,lonB)))`. (Clamp is load-bearing — composer.rs:148,158 reject over-range.)
  2. **Degeneracy guard** (integer-aligned/zero drag makes floor==ceil): `if (north === south) north = Math.min(90, south + 1); if (east === west) east = Math.min(180, west + 1);` and if that hits the clamp ceiling, expand the OTHER edge down instead (`south -= 1` / `west -= 1`) so the range is always ≥1° and in-bounds.
  3. Convert each signed integer edge to `{degrees,dir}` via the same abs+hemisphere mapping (`Math.round` is a no-op on integers, so reusing `signedToLatitude`/`signedToLongitude` is exact). Result: `lat0`=south, `lat1`=north, `lon0`=west, `lon1`=east.
  **Note:** the bbox path uses per-edge `Math.floor`/`Math.ceil` (outward), NOT `Math.round`; `Math.round` is only the single-pin display path. Test both paths.
- [ ] **Step 4 — Verify PASS. Step 5 — Commit** (`feat(map): pure signed-bbox→GRIB region normalizer`).

---

## ↳ Review loop after Tasks 0–3 (pure layer)

After the pure layer is green, do **≥3 review rounds** (keep going past 3 if substantive issues remain): (1) re-read `docs/pitfalls/testing-pitfalls.md` §3 (error paths), §4 (negative property), §7 (honest doubles, no network) — are equator/pole/antimeridian/degenerate paths covered? (2) Type-consistency: do `Latitude`/`Longitude` usages match `src/grib/types.ts` exactly? (3) Does any "pure" function secretly touch Leaflet/DOM (it must not)? Update your private journal, then continue.

---

## Canonical react-leaflet test mock — `src/map/testMapMock.ts` (Tasks 4/5/6/8 reuse VERBATIM)

The existing `PositionMapWidget.test.tsx` mock only wires a `click` handler and a
`useMap()` returning `{on, off}` — it CANNOT drive box-drag or deliver a
projected `latlng`. To remove that ambiguity (plan-review blockers), Task 4
creates ONE canonical mock that every map component test imports. Real
projection/drag correctness is grim-only (C1); this mock proves only *shape +
wiring*.

- [ ] **Create `src/map/testMapMock.ts`** (exported as a `vi.mock` factory + a
  `fireMapEvent` helper). It must:
  - render `MapContainer` as a `<div data-testid="leaflet-map">` that **spreads
    every received prop onto `data-*`** (so `boxZoom`, `crs`, `maxBounds` are
    assertable AND no React unknown-prop warning leaks — testing-pitfalls §1);
  - render `ImageOverlay` as `<div data-testid="image-overlay" data-bounds={JSON.stringify(bounds)} />`;
    `Marker`/`Rectangle`/`Polyline`/`Tooltip` as `data-testid` divs that consume
    their props;
  - implement `useMapEvents(handlers)` by storing the passed handler map in a
    module-level registry, and `useMap()` returning a fake map:
    `{ on: (e,h)=>reg.set(e,h), off: (e)=>reg.delete(e), dragging: { disable: vi.fn(), enable: vi.fn() }, mouseEventToLatLng: (pt)=>({ lat: pt.clientY, lng: pt.clientX }) }`;
  - export `fireMapEvent(type, { lat, lng })` that looks up the registry handler
    and calls it with `{ latlng: { lat, lng } }` (so a test can fire
    `click`/`mousedown`/`mousemove`/`mouseup`).
- [ ] **Box-drag testability decision (resolves blocker):** the box-drag *gesture*
  (dragging.disable → temp Rectangle → mouseup) is NOT meaningfully unit-testable
  in jsdom. Tasks 6's shape test asserts ONLY: (a) `boxZoom={false}` reached
  MapContainer, (b) firing `mousedown`+`mouseup` via `fireMapEvent` invokes
  `onBoxChange` with the two latlngs, (c) `dragging.disable`/`enable` were called.
  The live rubber-band preview + no-pan-during-drag + click-suppression are
  **grim-gated only** — state this in Task 6 so no agent writes projection theater.

## C15 decision (grid-square rectangle precision) — per-consumer for this PR

The grid-square `<Rectangle>` geometry stays **per-consumer**: `PositionMapWidget`
keeps its existing `is6Char`-length-based rectangle (`:114-122`); `GridMapPicker`
pin mode draws its own rectangle from the held grid. It is NOT centralized into
`<BaseMap>`/`<MaidenheadOverlay>` in this PR (the overlay draws the Maidenhead
LATTICE, not the selected-cell highlight). Revisit centralization only if a third
consumer appears.

## Task 4: `<BaseMap>` shared substrate + marker-icon fix (C6, C8) — FREEZE the contract

**Files:** Create `src/map/BaseMap.tsx`, `src/map/BaseMap.test.tsx`, `src/map/leafletIconFix.ts`.

- [ ] **Step 1 — Move the marker-icon fix (C8)** into `src/map/leafletIconFix.ts` as a side-effect module (copy the `delete L.Icon.Default.prototype._getIconUrl` + `mergeOptions({iconUrl,iconRetinaUrl,shadowUrl})` block currently at `src/compose/PositionMapWidget.tsx:29-38`). `BaseMap` imports it for side effect.
- [ ] **Step 2 — Implement `<BaseMap>`** wrapping react-leaflet `<MapContainer crs={L.CRS.EPSG4326} maxBounds={WORLD_BOUNDS} maxBoundsViscosity={1.0} minZoom={0} maxZoom={4} zoomSnap={0.5} worldCopyJump={false} attributionControl={false}>` with a single `<ImageOverlay url={worldEquirectPng} bounds={WORLD_BOUNDS} />` (import the Task-0 PNG). Props (**FROZEN CONTRACT** — `BaseMapProps`): `{ children?: ReactNode; onMapClick?: (latlon: LatLon) => void; initialCenter?: LatLon; initialZoom?: number }`. Expose click via a `useMapEvents({ click })` child that calls `onMapClick(clampLatLon(e.latlng.lat, e.latlng.lng))`. `maxZoom={4}` caps zoom so you can't zoom past the 2048px native resolution into illusory precision (C6).
- [ ] **Step 3 — Component-shape test ONLY, via the canonical mock.** First create `src/map/testMapMock.ts` (see the "Canonical react-leaflet test mock" section above) and `vi.mock('react-leaflet', () => canonicalMock)`. Assert: `<MapContainer>` rendered with `data-crs` reflecting EPSG4326 and `data-maxbounds` set; `<ImageOverlay>` rendered with `data-bounds` === `[[-90,-180],[90,180]]`; firing `fireMapEvent('click', {lat:0,lng:0})` invokes `onMapClick({lat:0,lon:0})`. **Header-comment the file:** "shape-only; real projection/render verified via grim (C1) — do NOT assert projection arithmetic through the mock (that's `projection.test.ts`)."
- [ ] **Step 4 — grim gate (REQUIRED, not optional).** `mkdir -p dev/scratch` first (it does not exist yet). Restart `pnpm tauri dev`; mount BaseMap on a scratch route or via the GridMapPicker harness in Task 6; `grim dev/scratch/2026-06-08-basemap-render.png` the window; confirm the world PNG renders, panning is bounded to the world (no grey void), and a click logs a plausible lat/lon. Record provenance (worktree, branch, commit SHA, "branch-local tauri dev") per the project's verification-provenance rule.
- [ ] **Step 5 — Commit** (`feat(map): BaseMap offline EPSG4326 substrate + shared leaflet icon fix`). Do NOT touch PositionMapWidget yet (Task 8).

---

## Task 5: `<MaidenheadOverlay>` component — TDD shape + grim

**Files:** `src/map/MaidenheadOverlay.tsx`, `.test.tsx`. Consumes `gridLines()` (Task 2). Renders SVG `<Polyline>`s (react-leaflet) for lat/lon lines + `<Tooltip>`/`<Marker>` `DivIcon` labels; a `visible` prop toggles it (default on); chooses `GridLevel` from current zoom.

- [ ] **Step 1 — Shape test** (mock react-leaflet): given a fixed bounds+level, assert it renders the expected count of `<Polyline>` elements and that `visible={false}` renders none. The line *geometry* is already proven pure in Task 2 — do NOT re-assert coordinates through the mock. **Step 2 — FAIL. Step 3 — Implement. Step 4 — PASS. Step 5 — grim:** confirm the grid lines + field labels draw over the world map and the toggle hides them. **Step 6 — Commit** (`feat(map): toggleable maidenhead grid overlay`).

---

## Task 6: `<GridMapPicker>` — pin + box-drag modes (C2) — TDD shape + grim

**Files:** `src/map/GridMapPicker.tsx`, `.test.tsx`. Composes `<BaseMap>` + `<MaidenheadOverlay>` + mode-specific interaction. Props: `{ mode: 'pin' | 'box'; grid?: string; onGridChange?: (grid4or6: string) => void; onBoxChange?: (a: LatLon, b: LatLon) => void; gridOverlay?: boolean }`.

- [ ] **Step 1 — Box-drag mechanics (C2) — implement carefully.** Attach events with **`useMapEvents({ mousedown, mousemove, mouseup, click })`** (auto-cleans on unmount — do NOT use bare `map.on` without a `useEffect` cleanup). In `mode==='box'`: `mousedown` → `map.dragging.disable()` + record start `latlng`; `mousemove` (while dragging) → update a state-held temp `<Rectangle>`; `mouseup` → `onBoxChange(start, end)` + `map.dragging.enable()` + set a `draggedRef.current=true`. The next `click` (Leaflet fires one after a short drag) is suppressed when `draggedRef.current` then reset to false (prevents pin double-fire). Set `boxZoom={false}` on `<BaseMap>`'s MapContainer. In `mode==='pin'`: `click` → `onGridChange(latLonToGrid(lat,lon).slice(0,4))` (4-char broadcast default; `.slice(0,4)` is correct — Maidenhead is hierarchical; finer precision needs the opt-in server, out of scope/`tuxlink-dyop`). Pin-click no-ops while a box drag is in progress (`mode==='box'`).
- [ ] **Step 2 — Shape test via the canonical mock** (`src/map/testMapMock.ts` + `fireMapEvent`). Assert: (a) `boxZoom={false}` reached MapContainer (`data-boxzoom="false"`); (b) pin mode: `fireMapEvent('click',{lat:33.6,lng:-118.2})` → `onGridChange` called with a **4-char** grid (`expect(g).toHaveLength(4)`); (c) box mode: `fireMapEvent('mousedown',cornerA)` then `fireMapEvent('mouseup',cornerB)` → `onBoxChange(cornerA,cornerB)` called AND `map.dragging.disable`/`enable` were each called once. The live rubber-band preview, no-pan-during-drag, and click-suppression are **grim-gated only** — do NOT attempt to assert them in jsdom (theater). **Header-comment: shape-only.** **Step 3 — FAIL → Step 4 — implement → Step 5 — PASS.**
- [ ] **Step 6 — grim gate (REQUIRED):** restart tauri dev; in box mode, drag a rectangle → confirm the live preview rectangle tracks the cursor, the map does NOT pan during drag, and release fires a plausible bbox; in pin mode, click → marker + 4-char readout; confirm no double-fire after a drag. Screenshot to `dev/scratch/`.
- [ ] **Step 7 — Commit** (`feat(map): GridMapPicker pin + box-drag modes`).

---

## ↳ Review loop after Tasks 4–6 (component layer) — ≥3 rounds

(1) Does any component test assert projection/coordinate correctness through the mock (forbidden — C1)? (2) Is `BaseMapProps` still the frozen contract Tasks 5/6 agreed to? (3) Re-read `testing-pitfalls.md` §7 (honest doubles) + §1 (pristine output — no Leaflet console warnings leaking). (4) Did every component task complete its grim gate with a saved screenshot + provenance? Journal + continue.

---

## Task 7: Wire GRIB box mode into `GribRequestPanel` (`tuxlink-mxmx` / item 21) — TDD

**Files:** Modify `src/grib/GribRequestPanel.tsx`; add to `src/grib/GribRequestPanel.test.tsx`.

- [ ] **Step 1 — Failing test (pure-ish, jsdom):** mount `GribRequestPanel`; simulate `GridMapPicker` `onBoxChange(a,b)` (the picker is mocked at the module boundary — assert the wiring, not Leaflet); assert the four region fields update to `signedBboxToGribRegion(a,b)` values (whole-degree, ordered); AND assert the manual `LatField`/`LonField` inputs (`data-testid` `grib-lat0`…`grib-lon1`) **remain present and editable** (C9 — accessibility hard criterion); AND a keyboard edit of `grib-lat0-deg` still updates state.
- [ ] **Step 2 — FAIL. Step 3 — Implement:** add `<GridMapPicker mode="box" onBoxChange={(a,b) => { const r = signedBboxToGribRegion(a,b); setLat('lat0', r.lat0.degrees, r.lat0.dir); setLat('lat1', r.lat1.degrees, r.lat1.dir); setLon('lon0', r.lon0.degrees, r.lon0.dir); setLon('lon1', r.lon1.degrees, r.lon1.dir); }} />` above the Region section. Keep `LatField`/`LonField` exactly as-is (map is an aid). NO change to `useGrib`/composer.rs — `signedBboxToGribRegion` already produces the whole-degree ordered `{degrees,dir}` the existing setters expect. **Step 4 — PASS.**
- [ ] **Step 5 — grim gate:** restart tauri dev; open GRIB request; drag a region box; confirm the four fields populate sanely and the GRIB body preview (or queued outbox message) shows the right `lat,lat,lon,lon`. NO send (RADIO-1 not engaged, but no transmit needed to verify). Screenshot.
- [ ] **Step 6 — Commit** (`feat(grib): map-based region selection (item 21, tuxlink-mxmx)`).

---

## Task 8: Remediate compose OSM ingestion (`tuxlink-714t`) — TDD (C4, C5, C7)

**Files:** Modify `src/compose/PositionMapWidget.tsx`, `src/compose/PositionMapWidget.test.tsx`, `src/compose/positionMapCsp.test.ts`, `src-tauri/tauri.conf.json`.

- [ ] **Step 1 — Invert the CSP guard test (C4) — write it FAILing first.** Replace `src/compose/positionMapCsp.test.ts` body with the never-OSM contract:
```ts
import { describe, expect, it } from 'vitest';
import tauriConfig from '../../src-tauri/tauri.conf.json';
function directiveTokens(csp: string, directive: string): string[] {
  const m = csp.split(';').map(p => p.trim()).find(p => p.startsWith(`${directive} `));
  return m?.split(/\s+/).slice(1) ?? [];
}
describe('Position map CSP — offline-first, never public OSM', () => {
  const csp = tauriConfig.app.security.csp;
  const imgSrc = directiveTokens(csp, 'img-src');
  const connectSrc = directiveTokens(csp, 'connect-src');
  it('forbids any OpenStreetMap tile host in img-src and connect-src', () => {
    for (const tok of [...imgSrc, ...connectSrc]) expect(tok).not.toContain('openstreetmap');
  });
  it('preserves the load-bearing retain-list', () => {
    expect(imgSrc).toEqual(expect.arrayContaining(["'self'", 'data:']));        // data: = dropdown SVGs
    expect(connectSrc).toEqual(expect.arrayContaining(["'self'", 'http://127.0.0.1:*'])); // WLE forms server
  });
});
```
  Run it → FAIL (CSP still has OSM).
- [ ] **Step 2 — Revert the CSP (C5).** In `src-tauri/tauri.conf.json` set `app.security.csp` to EXACTLY: `default-src 'self'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:; style-src 'self' 'unsafe-inline'`. Re-run Step 1's test → PASS.
- [ ] **Step 3 — Retrofit `PositionMapWidget.tsx` (C7 — remove the FULL online apparatus).** Replace the `<TileLayer ...osm...>` + `MapContainer` body with `<BaseMap onMapClick={({lat,lon}) => onGridChange(latLonToGrid(lat,lon))}>` keeping the existing `<Marker>` + grid-square `<Rectangle>`. **PRESERVE 6-char semantics:** this widget passes the FULL 6-char `latLonToGrid(lat,lon)` (NOT `.slice(0,4)`) — the 4-char default is a GridMapPicker-only concern; PositionMapWidget does NOT consume GridMapPicker and its existing per-message 6-char position-report contract is unchanged. DELETE: `isOnline` state, the `online`/`offline` window-listener effect, `MapInteractor`'s `onTileError`+`isOnline` props + the `tileerror` effect, `handleTileError`, and the OSM `<TileLayer>`. Remove the now-unused `navigator.onLine` import path. Move the marker-icon fix import to `leafletIconFix` (already in BaseMap; delete the local copy). The file's header comment must be rewritten to describe the offline-only behavior.
- [ ] **Step 4 — Update `PositionMapWidget.test.tsx`.** Drop the `osm-tile-layer` / `navigator.onLine` tests (tests 3 & 5 in the current file header). Keep/adjust: renders BaseMap, marker at grid lat/lon, click fires `onGridChange` **with a 6-char grid (`expect(grid).toHaveLength(6)` — preserve the existing length-6 contract at `:188`)**. Add a **negative assertion**: no `TileLayer`/no external tile URL appears in the rendered tree (genuinely valuable — C1/D-P2). Header-comment: shape-only; real render via grim.
- [ ] **Step 4b — C9 manual-path guard (compose).** The compose-side manual grid path is the `<input type="text">` inside `PositionFormV2.tsx:193` (NOT `GridEdit.tsx`, which is the dashboard ribbon — approach-doc C9 wording corrected here). It is the PARENT of PositionMapWidget and is untouched by this task, so it is retained automatically; add/keep a `PositionFormV2` test asserting the manual grid input remains present + editable when the map is mounted (accessibility — map is an aid, never the only path).
- [ ] **Step 5 — grim gate:** restart tauri dev; open Compose → Position form; confirm the map shows the bundled world (NOT OSM tiles), click still sets the grid, and `PositionFormV2` is not regressed. Screenshot. Also run the no-OSM source scan: `grep -rn "openstreetmap\|tile\." src/ src-tauri/tauri.conf.json` → expect ZERO matches outside CREDITS/tests.
- [ ] **Step 6 — Commit** (`fix(compose)!: remove public-OSM tiles; use bundled offline map (tuxlink-714t)` — `!` because the CSP/behavior change is user-visible; `BREAKING CHANGE:` footer: "Position map now uses a bundled offline world map instead of online OpenStreetMap tiles.").

---

## Task 9: Correct the locked-spec grounding (C13) + bd/docs

**Files:** Modify `docs/design/2026-06-07-map-pin-grid-design.md`; update `dev/implementation-log.md` if present.

- [ ] **Step 1 — Append (do NOT rewrite) a dated block** after the spec's Grounding section: `## Correction (2026-06-08, agent moss-basalt-hawk)` stating that `leaflet`+`react-leaflet`+the Maidenhead converter+the OSM CSP allowance all existed on `origin/main` ~2 days before the brainstorm (the brainstorm read a 639-behind checkout); the offline-first/never-OSM POSTURE stands (operator-re-affirmed) and was delivered bundled-only via `tuxlink-z9u4`, with the gatekeeper/opt-in server split to `tuxlink-dyop`. Leave the original Grounding text visible.
- [ ] **Step 2 — Commit** (`docs(design): correct map-pin grounding errata; record bundled-only delivery`).

---

## Final verification (before PR) + execution handoff

- [ ] Full frontend suite green: `pnpm -C <wt> exec vitest run` (watch for the C5 CSP test + the inverted guard + all `src/map/*` pure tests). Reap vitest zombies after (`pkill -9 -f vitest`; `feedback_vitest_worker_zombies`).
- [ ] `pnpm -C <wt> typecheck` clean; `pnpm -C <wt> lint:docs` clean (pre-push gate).
- [ ] **`pnpm -C <wt> build` (tsc + vite production build) PASS** — this is a CI `verify`-job gate (`.github/workflows/ci.yml`) and is the single most likely break in THIS PR: it catches the new `world-equirect-2048.png` asset-import / vite-only failures that vitest + typecheck miss.
- [ ] **CI-parity gate** (`feedback_scoped_vitest_misses_contract_tests`): the converged `verify` runs `cargo clippy --all-targets -D warnings` + full vitest — run clippy (`cargo clippy --all-targets --manifest-path <wt>/src-tauri/Cargo.toml -- -D warnings`, re-run till exit 0) even though this PR is mostly frontend (the `tauri.conf.json` change can trip a Rust rebuild).
- [ ] **grim walk** on real WebKitGTK for every map surface (BaseMap, overlay, GridMapPicker pin+box, GRIB region, compose Position) — Chromium is NOT acceptable. Save screenshots + provenance.
- [ ] No-OSM proof: `grep -rn "openstreetmap" src/ src-tauri/` returns only CREDITS + the inverted test.
- [ ] Close `tuxlink-z9u4` + `tuxlink-mxmx` (item 21) + `tuxlink-714t` on merge; `tuxlink-urbv` (item 18, needs `9xy1`) + `tuxlink-dyop` (gatekeeper) remain open as follow-ups.

### Recommended execution approach

**`/executing-plans` in THIS worktree, fresh next session** (option 2). Rationale: (a) the design+adrev consumed this session's context — a fresh session executes the pure→component→integration ladder with full budget; (b) tasks are mostly sequential through the BaseMap contract (C11), so the batch-with-checkpoints model fits better than wide subagent fan-out; (c) the grim gates need an interactive tauri-dev loop the operator can watch. Use subagent-driven-development only for the three independent pure-function tasks (1/2/3) if parallel speed is wanted — they share no files.
