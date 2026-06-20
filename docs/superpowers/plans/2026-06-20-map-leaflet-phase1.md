# Map engine migration — Phase 1: Leaflet substrate + AprsPositionsMap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up a Leaflet + protomaps-leaflet map substrate that renders the project's existing vector PMTiles over the real `tile://` Rust seam, and migrate the first (most demanding) consumer — `AprsPositionsMap` — onto it, behind the active `.github/RELEASE_FREEZE`.

**Architecture:** Strangler-fig. A NEW Leaflet substrate (`LeafletMap` + `LeafletMapContext` + Leaflet overlay hooks) is introduced ALONGSIDE the untouched MapLibre substrate. Only `AprsPositionsMap` switches to it this phase; the other four consumers (`StationFinderMap`, `LocationMap`, the compose position picker, `GridPicker`/`MaidenheadGridLayer`) keep importing `MapLibreMap` until their own phases. Both `maplibre-gl` and `leaflet` are bundled during the migration window — exactly the half-migrated state `.github/RELEASE_FREEZE` exists to keep out of a release. The MapLibre substrate (`MapLibreMap`, `mapHooks`, `MapContext`, `basemapStyle`/`darkStyle`/`tuxlinkFlavor`) is DELETED in the final phase when the last consumer migrates — NOT in this one.

**Tech Stack:** Leaflet 1.9.x (npm), protomaps-leaflet 5.x (VENDORED — see Global Constraints), pmtiles (existing npm dep), React 18 + TypeScript, Vitest/jsdom, Tauri 2.x + WebKitGTK.

## Global Constraints

- **Do NOT touch the MapLibre substrate or the four un-migrated consumers.** This phase adds a parallel substrate and rewires ONLY `AprsPositionsMap`. `MapLibreMap.tsx`, `mapHooks.ts`, `MapContext.ts`, `basemapStyle.ts`, `darkStyle.ts`, `tuxlinkFlavor.ts`, `StationFinderMap.tsx`, `LocationMap.tsx`, `PositionPickerOverlay.tsx`, `PositionMapWidget.tsx`, `GridPicker.tsx`, `MaidenheadGridLayer.tsx` stay byte-for-byte unchanged.
- **Do NOT rebuild any map UI in MapLibre.** (Decision: `docs/design/2026-06-20-map-engine-leaflet-decision.md`.)
- **Reuse, do not duplicate, the backend.** The `tile://` PMTiles seam, region-pack download/manifest, and offline-maps settings UI are untouched (~8,000 LOC backend stays). protomaps-leaflet reads the same PMTiles.
- **MSRV / no backend changes**: this phase is frontend-only. No Rust edits.
- **Dark mode = protomaps-leaflet `flavor: 'dark'`** (paint-rule colors), NOT a CSS filter and NOT the `tuxlinkFlavor` bake-invert.
- **CSP is fixed** (`src-tauri/tauri.conf.json`): `default-src 'self'; connect-src 'self' http://127.0.0.1:* tile:; img-src 'self' data: tile:; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'`. The Leaflet stack MUST render under this exact CSP — no CSP edits permitted without operator sign-off (a CSP loosening is a security-surface change). If protomaps-leaflet needs `'unsafe-eval'` or a non-`blob:` worker, STOP and escalate.
- **Vendoring posture (decided, see Task 1):** `leaflet` and `pmtiles` are npm deps (actively maintained). `protomaps-leaflet` is VENDORED into `src/vendor/protomaps-leaflet/` (ESM dist + a `PROVENANCE.md`) per ADR-0011 fork-and-own, because it is in upstream maintenance mode and on the offline/EmComm-critical path. Imports resolve to the vendored copy, never the npm registry.
- **RF-honesty (carried from the current AprsPositionsMap):** every pin is a real decoded fix; ambiguous fixes render as an uncertainty region, never a false-exact pin; stale fixes dim and surface their age; the operator "you" pin is visually distinct and never counted as a heard station. The migration MUST preserve every one of these behaviors.
- **Commit discipline:** conventional commits, `Agent: sage-fox-mesa` + `Co-Authored-By:` trailers on every commit. Branch `bd-tuxlink-6kdw/map-leaflet-phase1` (already created, off main, freeze committed as its base).
- **Validation reality:** jsdom has NO real layout/canvas sizing; Leaflet partially works in jsdom (DOM-based) but map sizing, tile loads, and real paint do NOT. Unit tests cover pure logic + DOM structure + Leaflet layer bookkeeping with a sized-container shim; the actual vector render is validated ONLY in the real Tauri app via `grim` (Task 7). `get_snapshot`/event-metrics cannot verify render — but unlike MapLibre/WebGL, Leaflet's Canvas2D output IS grim-verifiable.

---

## File Structure

New files (the parallel Leaflet substrate):
- `src/map/LeafletMap.tsx` — the substrate component; preserves the `MapLibreMapProps` public contract.
- `src/map/LeafletMapContext.ts` — React context publishing `L.Map | null`.
- `src/map/leafletHooks.ts` — Leaflet-native overlay lifecycle primitive(s).
- `src/map/basemapLeaflet.ts` — builds the protomaps-leaflet base layer(s) (overview + packs) over the `tile://` seam; the seam crux.
- `src/vendor/protomaps-leaflet/` — vendored dist + `PROVENANCE.md`.
- Tests: `src/map/LeafletMap.test.tsx`, `src/map/leafletHooks.test.tsx`, `src/map/basemapLeaflet.test.ts`.

Modified files:
- `package.json` — add `leaflet` + `@types/leaflet`; (pmtiles already present).
- `src/aprs/AprsPositionsMap.tsx` — rewired to the Leaflet substrate.
- `src/aprs/AprsPositionsMap.test.tsx` — rewritten against the Leaflet substrate.
- `src/aprs/AprsPositionsMap.css` — only if Leaflet markers need positioning tweaks; keep diff minimal.
- `vite.config.ts` / `tsconfig*.json` — only if the vendored import path needs an alias (prefer a relative import to avoid alias config).

Untouched (listed to make the boundary explicit): everything under "Do NOT touch" in Global Constraints.

---

### Task 1: Dependencies + vendor protomaps-leaflet

**Files:**
- Modify: `package.json`
- Create: `src/vendor/protomaps-leaflet/` (vendored dist), `src/vendor/protomaps-leaflet/PROVENANCE.md`
- Create: `src/vendor/protomaps-leaflet/index.d.ts` (or reuse upstream `.d.ts` if shipped)

**Interfaces:**
- Produces: an import path `../vendor/protomaps-leaflet` exporting `leafletLayer(options)` and the protomaps-leaflet runtime; `leaflet` importable as `import L from 'leaflet'`; `import 'leaflet/dist/leaflet.css'`.

- [ ] **Step 1: Pin and add the actively-maintained deps**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-6kdw-map-leaflet-phase1
pnpm add leaflet@^1.9.4
pnpm add -D @types/leaflet@^1.9.12
# pmtiles is already a dependency (used by the MapLibre Protocol path) — confirm:
grep '"pmtiles"' package.json
```
Expected: `leaflet` + `@types/leaflet` appear in `package.json`; `pmtiles` already listed.

- [ ] **Step 2: Vendor protomaps-leaflet**

Fetch the exact version the spike validated (protomaps-leaflet@5) and copy its distributed ESM into the repo. Determine the dist shape first:

```bash
npm pack protomaps-leaflet@5 --pack-destination /tmp 2>/dev/null
tar xzf /tmp/protomaps-leaflet-*.tgz -C /tmp
ls -la /tmp/package/dist          # inspect: ESM entry, .d.ts, whether minified
cat /tmp/package/package.json | grep -E '"(version|main|module|types|exports)"'
```

Copy the ESM build + types into `src/vendor/protomaps-leaflet/`:

```bash
mkdir -p src/vendor/protomaps-leaflet
cp /tmp/package/dist/*.js  src/vendor/protomaps-leaflet/    # the ESM entry (e.g. index.js / protomaps-leaflet.js)
cp /tmp/package/dist/*.d.ts src/vendor/protomaps-leaflet/ 2>/dev/null || true
```

If only a minified bundle ships (no readable source), STILL vendor it (fork-and-own = we OWN the artifact and pin it; patchability is a bonus, not a requirement for Phase 1). Record this in PROVENANCE.

- [ ] **Step 3: Write PROVENANCE.md**

```markdown
# Vendored: protomaps-leaflet

- **Version:** <exact version from npm pack>
- **Source:** https://www.npmjs.com/package/protomaps-leaflet (npm pack, <date>)
- **License:** MIT (upstream LICENSE preserved below)
- **Why vendored:** upstream is in maintenance mode; this is on tuxlink's
  offline/EmComm-critical path (ADR-0011 fork-and-own). Pinning the artifact in-repo
  removes the registry dependency at build time and lets us patch the PMTiles source
  wiring if upstream cannot read our `tile://` custom-protocol seam.
- **Dist shape:** <ESM entry filename>, <has/has-no .d.ts>, <minified? y/n>.
- **Upstream LICENSE:**
  <paste upstream MIT LICENSE text>
```

- [ ] **Step 4: Verify the vendored import resolves + types build**

Create a throwaway probe `src/vendor/protomaps-leaflet/_probe.ts`:
```ts
import { leafletLayer } from './index'; // adjust to the actual entry filename
export const _probe = typeof leafletLayer;
```
Run:
```bash
pnpm typecheck 2>&1 | tail -20
```
Expected: no error about the vendored module. If `leafletLayer` is not the exported name or the entry differs, adjust the import to the real export (inspect the vendored file's `export` statements) and re-run. Delete `_probe.ts` after.

- [ ] **Step 5: Commit**

```bash
rm -f src/vendor/protomaps-leaflet/_probe.ts
git add package.json pnpm-lock.yaml src/vendor/protomaps-leaflet
git commit -m "build(map): add leaflet dep + vendor protomaps-leaflet (tuxlink-6kdw)

Agent: sage-fox-mesa
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: `basemapLeaflet.ts` — protomaps-leaflet base layer over the `tile://` seam (THE SEAM CRUX)

**Files:**
- Create: `src/map/basemapLeaflet.ts`
- Test: `src/map/basemapLeaflet.test.ts`

**Interfaces:**
- Consumes: vendored `leafletLayer`; `pmtiles` package (`PMTiles`, `FetchSource`); `PackSource` (re-declare locally as `{ id: string }` — do NOT import from `basemapStyle.ts`, keep the substrates independent).
- Produces:
  - `export const PMTILES_TILE_URL = (id: string) => \`tile://pmtiles/${id}\`;`
  - `export function buildBaseLayers(flavor: 'light' | 'dark', packs: { id: string }[]): L.Layer[];`
    - Returns protomaps-leaflet layer(s): the world overview (id `'world'`) as the always-present base, plus one protomaps-leaflet layer per installed pack (clamped `minZoom: 6`), drawn on top.
  - `export const OSM_ATTRIBUTION = '© OpenStreetMap contributors';`

**Design notes (read before writing):**
- The spike fed protomaps-leaflet a plain http URL (`location.origin + '/phoenix.pmtiles'`). tuxlink must instead point it at the Tauri `tile://pmtiles/world` custom protocol so the Rust 206 seam serves the bytes. Two wiring options, try in order:
  1. **Pre-built PMTiles source (preferred):** `new PMTiles(new FetchSource('tile://pmtiles/world'))` then pass to `leafletLayer({ source: <pmtiles instance>, flavor, ... })` if protomaps-leaflet@5 accepts a `source`/PMTiles option.
  2. **URL string:** if it only accepts `url`, pass `leafletLayer({ url: 'tile://pmtiles/world', flavor })` and confirm protomaps-leaflet's internal pmtiles construction issues a `fetch('tile://...', {headers:{Range}})` (which the Tauri protocol + CSP `connect-src tile:` allow).
- Inspect the vendored entry's exported `leafletLayer` signature (the `.d.ts` or the source) to pick the option. Record which worked in a top-of-file comment.
- Dark mode: pass `flavor: 'dark'` (and `lang: 'en'`). Do NOT bake colors.
- Pack compositing mirrors `basemapStyle.ts`'s intent: overview overzooms past z6 (never blank); each pack is a separate protomaps-leaflet layer clamped to `minZoom >= 6` drawn above the overview, so detail wins inside coverage and the overview shows outside it. protomaps-leaflet layers accept Leaflet `minZoom`/`maxZoom`/`pane` options; set the pack `pane`/zIndex above the overview.

- [ ] **Step 1: Write the failing test**

```ts
// src/map/basemapLeaflet.test.ts
import { describe, it, expect, vi } from 'vitest';

// Mock the vendored protomaps-leaflet so the test asserts WIRING, not render.
const leafletLayerSpy = vi.fn((opts: unknown) => ({ __pm: true, opts }));
vi.mock('../vendor/protomaps-leaflet', () => ({ leafletLayer: leafletLayerSpy }));

import { buildBaseLayers, PMTILES_TILE_URL } from './basemapLeaflet';

describe('basemapLeaflet', () => {
  it('builds a single overview layer (dark) over the tile:// world seam when no packs', () => {
    const layers = buildBaseLayers('dark', []);
    expect(layers).toHaveLength(1);
    expect(leafletLayerSpy).toHaveBeenCalledTimes(1);
    const opts = leafletLayerSpy.mock.calls[0][0] as Record<string, unknown>;
    expect(opts.flavor).toBe('dark');
    // overview is wired to the world seam (via source or url; assert the URL text appears)
    expect(JSON.stringify(opts)).toContain('tile://pmtiles/world');
  });

  it('appends one pack layer per installed pack, clamped to minZoom 6, above the overview', () => {
    const layers = buildBaseLayers('light', [{ id: 'continent-na' }]);
    expect(layers).toHaveLength(2);
    const packOpts = leafletLayerSpy.mock.calls.at(-1)![0] as Record<string, unknown>;
    expect(packOpts.minZoom ?? packOpts.minzoom).toBe(6);
    expect(JSON.stringify(packOpts)).toContain('tile://pmtiles/continent-na');
  });

  it('exposes the tile:// URL helper', () => {
    expect(PMTILES_TILE_URL('world')).toBe('tile://pmtiles/world');
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run src/map/basemapLeaflet.test.ts`
Expected: FAIL — `buildBaseLayers` not defined.

- [ ] **Step 3: Implement `basemapLeaflet.ts`**

Write the module per the design notes. Use whichever pmtiles-source wiring the vendored API supports (document it). Clamp pack layers `minZoom: 6`. Each `leafletLayer(...)` call returns an `L.Layer`. Return `[overview, ...packLayers]`.

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm vitest run src/map/basemapLeaflet.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/map/basemapLeaflet.ts src/map/basemapLeaflet.test.ts
git commit -m "feat(map): protomaps-leaflet base layer over the tile:// seam (tuxlink-6kdw)

Agent: sage-fox-mesa
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: `LeafletMapContext.ts` + `leafletHooks.ts` — context + overlay lifecycle

**Files:**
- Create: `src/map/LeafletMapContext.ts`, `src/map/leafletHooks.ts`
- Test: `src/map/leafletHooks.test.tsx`

**Interfaces:**
- Produces:
  - `LeafletMapContext.ts`: `export const LeafletMapProvider`; `export function useLeafletMap(): L.Map | null;`
  - `leafletHooks.ts`:
    - `export function useLeafletLayerGroup(map: L.Map | null): L.LayerGroup | null;` — creates one `LayerGroup`, adds it to the map on mount, removes on unmount; null-tolerant.
    - The Leaflet model has NO sources/styles/feature-state. Overlays manage their own Leaflet layers (markers/circles/polylines) inside a `LayerGroup`. There is NO `styledata` re-add problem: Leaflet's base-layer flavor swap (Task 4) does NOT clear overlay layers, so overlays need no re-add subscription. This is a deliberate simplification the engine change earns — do NOT port `useMapOverlay`'s `styledata` re-subscription.

**Design notes:**
- Keep `leafletHooks` minimal. The complex MapLibre `useMapOverlay` (idempotent add, layer-before-source teardown, `styledata` re-add) existed because `setStyle` dropped sources/layers. Leaflet has no `setStyle`; flavor swap replaces only the base tile layer. So the only shared primitive worth extracting is `useLeafletLayerGroup` (lifecycle of a container layer). Consumers add/remove their own `L.marker`/`L.circle`/`L.polygon` to it.

- [ ] **Step 1: Write the failing test**

```tsx
// src/map/leafletHooks.test.tsx
import { describe, it, expect } from 'vitest';
import { renderHook } from '@testing-library/react';
import L from 'leaflet';
import { useLeafletLayerGroup } from './leafletHooks';

function makeMap(): L.Map {
  const div = document.createElement('div');
  Object.defineProperty(div, 'clientWidth', { value: 800 });
  Object.defineProperty(div, 'clientHeight', { value: 600 });
  document.body.appendChild(div);
  return L.map(div, { center: [0, 0], zoom: 2 });
}

describe('useLeafletLayerGroup', () => {
  it('adds a layer group to the map and removes it on unmount', () => {
    const map = makeMap();
    const { result, unmount } = renderHook(() => useLeafletLayerGroup(map));
    const lg = result.current!;
    expect(map.hasLayer(lg)).toBe(true);
    unmount();
    expect(map.hasLayer(lg)).toBe(false);
  });

  it('returns null for a null map', () => {
    const { result } = renderHook(() => useLeafletLayerGroup(null));
    expect(result.current).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run src/map/leafletHooks.test.tsx`
Expected: FAIL — module/exports not defined. (If Leaflet throws in jsdom on `L.map`, add the sized-container shim shown; if it still throws on `_onResize`/`getSize`, call `map.getContainer()` sizing via the `Object.defineProperty` shim above — Leaflet reads `clientWidth/Height`.)

- [ ] **Step 3: Implement both files**

`LeafletMapContext.ts`:
```ts
import { createContext, useContext } from 'react';
import type { Map as LeafletMapInstance } from 'leaflet';
const LeafletMapContext = createContext<LeafletMapInstance | null>(null);
export const LeafletMapProvider = LeafletMapContext.Provider;
export function useLeafletMap(): LeafletMapInstance | null {
  return useContext(LeafletMapContext);
}
```

`leafletHooks.ts`:
```ts
import { useEffect, useState } from 'react';
import L from 'leaflet';
export function useLeafletLayerGroup(map: L.Map | null): L.LayerGroup | null {
  const [group, setGroup] = useState<L.LayerGroup | null>(null);
  useEffect(() => {
    if (!map) { setGroup(null); return; }
    const lg = L.layerGroup().addTo(map);
    setGroup(lg);
    return () => { map.removeLayer(lg); setGroup(null); };
  }, [map]);
  return group;
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm vitest run src/map/leafletHooks.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/map/LeafletMapContext.ts src/map/leafletHooks.ts src/map/leafletHooks.test.tsx
git commit -m "feat(map): Leaflet map context + layer-group lifecycle hook (tuxlink-6kdw)

Agent: sage-fox-mesa
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: `LeafletMap.tsx` — the substrate (preserves the MapLibreMap public contract)

**Files:**
- Create: `src/map/LeafletMap.tsx`
- Test: `src/map/LeafletMap.test.tsx`

**Interfaces:**
- Consumes: `buildBaseLayers` (Task 2), `useBasemapFlavor` (existing, reused unchanged), `clampMapCenter`/`clampLatLon`/`LatLon` (existing `projection.ts`, reused), `LeafletMapProvider` (Task 3), `BASEMAP_PACKS_CHANGED_EVENT` + `PacksList` (existing `offlineMaps.ts`, reused).
- Produces: `export interface LeafletMapProps { children?; onMapClick?; initialCenter?; initialZoom?; onZoomChange?; onViewportChange?; flavor? }` (identical shape to `MapLibreMapProps`), and `export function LeafletMap(props): JSX.Element`.

**Behaviors to preserve (current MapLibreMap → desired LeafletMap):**
- Construct once: `L.map(container, { preferCanvas: true, zoomControl: true, minZoom: 0, maxZoom: 14, center: clampMapCenter(initialCenter)→[lat,lon], zoom: initialZoom ?? 2, worldCopyJump: false })`. NOTE Leaflet uses `[lat, lon]`; `clampMapCenter` returns `[lng, lat]` — swap accordingly.
- Base layers: add `buildBaseLayers(effectiveFlavor, packs)` to the map. On flavor change OR pack change, REMOVE the old base layers and ADD freshly built ones (Leaflet has no `setStyle`; swap the layers). Dedupe so a redundant render does not rebuild (track a `flavor|packIds` key like MapLibreMap's `styleKeyRef`).
- Packs: fetch via `invoke('basemap_list_packs')` after mount, re-fetch on `BASEMAP_PACKS_CHANGED_EVENT`, cache last-known at module scope (mirror `lastKnownPacks`). Same try/catch → `[]` fallback.
- `onMapClick`: `map.on('click', e => onClick(clampLatLon(e.latlng.lat, e.latlng.lng)))`.
- `onZoomChange`: emit on `load`-equivalent (after construct) AND on `moveend`, deduped against last emitted zoom (Leaflet `map.getZoom()` is integer-by-default; still dedupe).
- `onViewportChange`: on `moveend`, emit `{ clamped center, zoom }`; skip non-finite transients (teardown).
- Pan-clamp: on `moveend`, `clampMapCenter(center)`; if changed, `map.panTo([lat,lon], { animate:false })`. Guard the re-fire (clamped center clamps to itself → no loop).
- `flyTo` on a post-construct `initialCenter` change (the async-arrival recenter): `map.flyTo([lat,lon])`; skip the construct-time center exactly like `skipConstructCenter`.
- Attribution: `L.control.attribution` with `OSM_ATTRIBUTION`. Scale: `L.control.scale({ imperial: true, metric: true })`.
- Error fallback: wrap construction in try/catch; on throw, render the SAME `data-testid="map-unavailable"` panel with text "The map could not be displayed on this system." (consumers/tests may assert it).
- Teardown: `map.remove()` in cleanup; set a `removed` flag first so a `moveend` during teardown is ignored.
- Render: `<div ref={container} style={{height:'100%',width:'100%'}}><LeafletMapProvider value={map}>{children}</LeafletMapProvider></div>`. Import `'leaflet/dist/leaflet.css'` at top.

**Design notes:**
- Set context value to the map only AFTER first layout (`map.whenReady(() => setMap(map))`) so overlays wire when the map is usable (mirrors MapLibre's `on('load')` → `setMap`).
- jsdom: tests must shim container sizing (see Task 3's `makeMap`). Where Leaflet internals (e.g. `_animateZoom`) throw in jsdom, gate behavior so the throw is caught by the error fallback OR avoid the path in tests.

- [ ] **Step 1: Write the failing tests**

Test list (write each as a `vitest` case in `LeafletMap.test.tsx`, using the sized-container render pattern + a `@tauri-apps/api/core` `invoke` mock returning `{ packs: [] }`):
1. `renders a map container and provides the map via context` — a child calling `useLeafletMap()` receives a non-null `L.Map` after `whenReady`.
2. `calls onMapClick with clamped lat/lon on map click` — fire `map.fire('click', { latlng: { lat: 33.4, lng: -112.0 } })`; assert callback got `{lat:33.4, lon:-112.0}`.
3. `emits zoom on load and dedupes repeated moveend at same zoom` — assert `onZoomChange` called once on ready, not again on a `moveend` at unchanged zoom.
4. `emits clamped viewport on moveend` — assert `onViewportChange` shape.
5. `renders the unavailable panel when construction throws` — force `L.map` to throw (mock) and assert `data-testid="map-unavailable"`.
6. `rebuilds base layers on flavor change but not on a redundant rerender` — spy `buildBaseLayers` call count.

- [ ] **Step 2: Run to verify they fail**

Run: `pnpm vitest run src/map/LeafletMap.test.tsx`
Expected: FAIL — `LeafletMap` not defined.

- [ ] **Step 3: Implement `LeafletMap.tsx`** per the behaviors above.

- [ ] **Step 4: Run to verify they pass**

Run: `pnpm vitest run src/map/LeafletMap.test.tsx`
Expected: PASS (6 tests). If a Leaflet-in-jsdom internal throws, prefer shimming the container/size over weakening the assertion; document any jsdom-specific guard in a comment.

- [ ] **Step 5: Commit**

```bash
git add src/map/LeafletMap.tsx src/map/LeafletMap.test.tsx
git commit -m "feat(map): Leaflet substrate preserving the MapLibreMap contract (tuxlink-6kdw)

Agent: sage-fox-mesa
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Migrate `AprsPositionsMap` to the Leaflet substrate

**Files:**
- Modify: `src/aprs/AprsPositionsMap.tsx` (rewrite the rendering internals; keep the public props + exported helpers `ambiguityRadiusMeters` unchanged)
- Modify: `src/aprs/AprsPositionsMap.test.tsx` (rewrite against Leaflet)
- Modify (minimal): `src/aprs/AprsPositionsMap.css` only if needed

**Interfaces:**
- Public surface UNCHANGED: `export interface AprsPositionsMapProps { positions; operatorGrid?; envStations?; onFocusStation? }`, `export function AprsPositionsMap(props)`, `export function ambiguityRadiusMeters(level)`.
- Consumes: `LeafletMap` (Task 4), `useLeafletMap` (Task 3), `useLeafletLayerGroup` (Task 3); reused unchanged: `usePersistedViewport`, `RecenterControl` (NOTE: `RecenterControl` calls `map.flyTo` via `useMapContext` → it currently reads the MapLibre context. It must instead read the Leaflet context. To avoid touching the shared `RecenterControl` used by un-migrated consumers, create a LOCAL recenter control inside AprsPositionsMap OR a `LeafletRecenterControl`. Decision: add `src/map/LeafletRecenterControl.tsx` (a Leaflet-context twin) this task; the MapLibre `RecenterControl` stays for the other consumers.), `gridToLatLon`, `lookupAprsSymbol`, `aprsSprites` helpers, `joinWxStations`/`badgeContent`, `stationCategories`, `wxSnapshot`/`wxSitrep`, `saveDraft`/`newDraftId`/`invoke`.

**Behavior mapping (MapLibre idiom → Leaflet implementation). Each must preserve the documented RF-honesty behavior:**

| Feature | Current (MapLibre) | Leaflet implementation |
|---|---|---|
| Station pins | symbol layers w/ `icon-image` sprite, two stacked (colour+grey) cross-faded by `feature-state.stale` | one `L.marker` per station in a `LayerGroup`; icon = `L.icon`/`L.divIcon` built from the SAME `aprsSprites` canvas `ImageData` (convert ImageData→`canvas.toDataURL()`→`iconUrl`). Staleness: swap to the grey icon (or set marker element opacity 0.55) — NO FC rebuild; update marker icon in place on the NOW_TICK. |
| Callsign label | `symbol` text layer | `L.divIcon` (text) marker or `marker.bindTooltip(call, {permanent:true, direction:'top'})`. |
| Ambiguity region | GeoJSON fill+line `circlePolygon` | `L.circle([lat,lon], { radius: ambiguityRadiusMeters(level)*√2, color:'#f0c24a', weight:1, dashArray:'2 2', fillColor:'#f0c24a', fillOpacity:0.12 })`. Reuse `cellCenter`. (Leaflet `L.circle` takes a METERS radius directly — drop `circlePolygon` math.) |
| Operator "you" pin | circle layer | `L.circleMarker([lat,lon], { radius:7, color:'#2f86f0', weight:3, fillColor:'#eaf3fb', fillOpacity:1 })`. |
| WX badge | `symbol` text layer (amber) | `L.marker` w/ `L.divIcon` rendering the badge text (the spike's `.wx-chip` pattern); offset above the pin. |
| Pin click → popup | `map.on('click', LAYER, ...)` + React popup div reading MapContext | per-marker `marker.on('click', () => setPopupCall(call))`; keep the existing React popup div (it reads selected fix from `byCall`). OR use `marker.bindPopup` — but keep the existing styled popup component for parity; wire its open/close to marker clicks. |
| WX badge click/hover | layer-scoped `click`/`mouseenter`/`mouseleave` | per-badge-marker `.on('click'|'mouseover'|'mouseout')`. |
| Category filter | `setFilter` on layers (the drunk-map bug origin) | add/remove the non-matching markers from the `LayerGroup` (rebuild the group's membership when `category` changes). This ELIMINATES the entire `setFilter`-on-`styledata` self-loop failure class — do NOT reintroduce any per-frame style mutation. |
| Staleness tick | `setFeatureState({stale})` on NOW_TICK | iterate markers, swap icon/opacity per `now - p.at > STALE_MS`. |
| PNG export | `map.getCanvas()` (WebGL) + `preserveDrawingBuffer` saga | Leaflet `preferCanvas:true` renders overlays to a canvas, but the BASE protomaps-leaflet tiles are a separate canvas; compositing the full map to PNG needs drawing both canvases. Implementation: query the Leaflet container's canvases (`container.querySelectorAll('canvas')`), draw each onto an output canvas in order, prepend the SITREP header strip (reuse `composeSnapshotHeader`), `toDataURL` + download. If multi-canvas compositing proves unreliable under WebKitGTK, gate Export PNG behind a follow-up bd issue rather than shipping a broken button — note in handoff. (Validate in Task 7.) |

**Do NOT** add features, change copy, or alter the RF-honesty semantics. This is a faithful re-expression on a new engine.

- [ ] **Step 1: Rewrite the test file first (TDD)** — `AprsPositionsMap.test.tsx`

Port each existing assertion to the Leaflet rendering. Minimum cases (keep every behavior the old tests covered):
1. renders the map container (`data-testid="aprs-positions-map"`).
2. a heard station produces a marker at its decoded lat/lon (assert a marker exists in the positions layer group at the expected latlng).
3. an ambiguous fix renders an uncertainty circle (not a sharp pin) sized to `ambiguityRadiusMeters(level)*√2`.
4. clicking a pin opens the popup with callsign, symbol name, last-heard age; ambiguous adds the ±note.
5. operator grid renders the distinct "you" `circleMarker`.
6. WX station renders a temperature badge; clicking it calls `onFocusStation(call)`.
7. category filter hides non-matching markers (assert layer-group membership shrinks).
8. stale station (now - at > STALE_MS) renders dimmed/grey (assert icon/opacity).
9. `ambiguityRadiusMeters` unit cases unchanged.
10. WX SITREP button disabled with zero WX stations; enabled with ≥1.

Use the sized-container shim + `@tauri-apps/api/core` invoke mock. Where the old test used a MapLibre test double, replace with real Leaflet + the shim (Leaflet markers/layers are inspectable: `layerGroup.getLayers()`, `marker.getLatLng()`, `marker.getIcon()`).

- [ ] **Step 2: Run to verify the rewritten tests fail**

Run: `pnpm vitest run src/aprs/AprsPositionsMap.test.tsx`
Expected: FAIL (component still imports MapLibre substrate / new assertions unmet).

- [ ] **Step 3: Rewrite `AprsPositionsMap.tsx`** per the behavior-mapping table. Also create `src/map/LeafletRecenterControl.tsx` (a Leaflet-context twin of `RecenterControl`: a button that calls `useLeafletMap().flyTo([lat,lon], { ... })` — mirror the existing control's markup/testid).

- [ ] **Step 4: Run the suite**

Run: `pnpm vitest run src/aprs/AprsPositionsMap.test.tsx src/map`
Expected: PASS. Then full quick suite: `pnpm vitest run` (must stay green — no other consumer changed).

- [ ] **Step 5: Typecheck + commit**

```bash
pnpm typecheck 2>&1 | tail -20   # must be clean
git add src/aprs/AprsPositionsMap.tsx src/aprs/AprsPositionsMap.test.tsx src/aprs/AprsPositionsMap.css src/map/LeafletRecenterControl.tsx
git commit -m "feat(aprs): migrate AprsPositionsMap to the Leaflet substrate (tuxlink-6kdw)

Agent: sage-fox-mesa
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Review loop (subagent-readiness + pitfalls)

- [ ] After Tasks 1–5, run a minimum of three review rounds over the batch from multiple perspectives (correctness, CSP/security surface, RF-honesty preservation, Leaflet-in-jsdom test soundness, dead MapLibre code NOT accidentally removed). Cross-check against `docs/pitfalls/implementation-pitfalls.md` and `docs/pitfalls/testing-pitfalls.md`. If the third round still finds substantive issues, keep going. Record dispositions in the handoff / PR body.
- [ ] Run full gates: `pnpm typecheck`, `pnpm vitest run`, `pnpm build`. (`cargo` is unchanged — CI compiles it; do not cold-build locally.)

---

### Task 7: Tauri-app render validation (grim) + wire-walk (HARD GATE)

This is the CRITICAL GATE from the kickoff and the handoff: PROVE the overview (z0–6) + continent-na pack (z0–14) render as protomaps-leaflet layers against the real `tile://` Rust seam + packaged CSP IN THE TAURI APP.

- [ ] **Build provenance first.** Confirm which build is running (`/proc/<pid>/cwd`, the `:1420` strictPort collision — only one `tauri dev` runs machine-wide). Launch THIS worktree's build. Always `WEBKIT_DISABLE_DMABUF_RENDERER=1` (else "TV static"); `get_snapshot`/idle-event CANNOT verify a render — use `grim` of the live foreground window (or operator eyes).
- [ ] Open the APRS Tac Chat positions map. Capture `grim`. Confirm: dark vector street grid renders (overview), pins/badges/uncertainty/operator-pin draw, pan/zoom is smooth, no CSP violation in the WebKit console (check `connect-src tile:` / `worker-src blob:` are sufficient — if a CSP error appears, STOP and escalate; do NOT loosen CSP unilaterally).
- [ ] Install/enable the `continent-na` pack (or a region pack) and confirm z6–14 detail renders over the seam.
- [ ] Test the Export PNG path; if multi-canvas compositing fails under WebKitGTK, file a follow-up bd issue and note it (do not ship a broken button silently).
- [ ] **wire-walk gate:** invoke the `wire-walk` skill (`.claude/skills/wire-walk/`). The OPERATOR supplies the key user flows greenfield (do NOT draft them). Trace each flow verbatim to code (`file:line`). Any broken primary flow ⇒ the surface is NOT shipped. Capture flows as the definition-of-done.
- [ ] Record verification provenance (worktree path, branch, commit SHA, dev vs converged, what was exercised) per the CLAUDE.md verification-provenance rule.

---

## Self-Review (run before execution)

- **Spec coverage:** Task 1 = vendoring + deps; Task 2 = tile:// seam (the issue's "render overview+pack against the real tile:// seam"); Tasks 3–4 = substrate ("replace MapLibreMap/mapHooks"); Task 5 = AprsPositionsMap FIRST; Task 6 = review loop; Task 7 = packaged-CSP + Tauri render proof + wire-walk + adrev gate. RELEASE_FREEZE already set (pre-plan). Vendoring posture decided (Task 1). Covered.
- **Strangler-fig boundary:** Global Constraints forbid touching the four un-migrated consumers and the MapLibre substrate; the LeafletRecenterControl twin (Task 5) exists specifically to avoid editing the shared `RecenterControl`. No dual-context collision.
- **Type consistency:** `buildBaseLayers(flavor, packs)` (Task 2) consumed by `LeafletMap` (Task 4); `useLeafletMap`/`useLeafletLayerGroup` (Task 3) consumed by Tasks 4–5; `LeafletMapProps` mirrors `MapLibreMapProps`.
- **Highest residual risk:** protomaps-leaflet reading the `tile://` custom protocol (Task 2 design notes give two wirings + a STOP if neither works) and PNG multi-canvas compositing (Task 5/7 gate it behind a follow-up if it fails). Both are flagged for the Codex adrev.
- **Pitfalls:** no per-frame style mutation (the drunk-map class is designed OUT by using LayerGroup membership instead of `setFilter`); CSP not loosened; build-provenance check before on-device debugging; one `:1420` at a time.
