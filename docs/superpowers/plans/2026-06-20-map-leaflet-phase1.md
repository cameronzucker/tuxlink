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

## Adversarial review dispositions (5 rounds, cross-provider — 2 Codex + 3 Claude)

Round 1 (Codex, broad) findings were folded in earlier (PNG deferral, `{url}` form [SUPERSEDED by R2], test isolation, per-station filter, attribution). Rounds 2–5 below; **all accepted**. The seam-breaking P0s (R2) are why this plan was NOT executed past Task 2.

**Round 2 — Codex, seam under packaged CSP (the EmComm-critical path):**
- **P0 [SEAM BROKEN] — string `url` does not use PMTiles.** protomaps-leaflet picks `PmtilesSource` only when the URL pathname ends `.pmtiles`; `tile://pmtiles/world` → `ZxySource` (no Range; parses the whole archive as one MVT tile) → blank. → **Task 2 redesigned: pass a `new PMTiles('tile://pmtiles/<id>')` INSTANCE** (the `.d.ts` types `url: PMTiles | string`), bypassing the pathname heuristic.
- **P0 [OVERZOOM] — no `maxDataZoom` → requests z7+ the z0–6 overview lacks → blank above ~z8.** → pass `maxDataZoom: 6` for the overview; thread each pack's real maxzoom.
- **P0 [PACK BG MASK] — a flavored pack layer paints its `backgroundColor` per tile; a pack's empty tiles outside coverage mask the overview.** (MapLibre dropped pack backgrounds for this exact reason — `basemapStyle.ts:176–198`.) → packs use explicit `paintRules` + `labelRules:[]` + NO `backgroundColor`/`flavor`; only the overview carries the flavor (one global background).
- **P1 [ETag] — length-only ETag → stale JS tile cache on same-size pack re-download.** Out of frontend scope → filed **bd `tuxlink-jrXX`** (ETag fingerprint). Mitigation in-plan: if the backend later exposes a pack generation, incorporate it into the base-layer rebuild key.
- **P2 — seam unit test is mocked (cannot prove real fetch).** Accepted: real seam proven in Task 7 (grim + observe a real `tile://` Range request in the WebKit network panel — not just "a grid appears").

**Round 3 — Claude, RF-honesty + parity (5 P0s; the hard-won fixes the one-line idiom table endangered):**
- **P0 — uncertainty radius `×√2` is LOAD-BEARING** (circumscribes the ambiguity box; the inscribed circle under-claims). Plan must forbid the "drop the √2" reading; test asserts radius `= ambiguityRadiusMeters(level)*Math.SQRT2`.
- **P0 — no `whenSheetsReady` re-bake → pins render BLANK on load forever** (a data-URL icon never re-decodes; worse than MapLibre, which recovered on `styledata`). Add a sheets-ready re-bake that recomputes each marker's icon and `setIcon`s it; test it.
- **P0 — `renderSymbolBitmap` returns `ImageData`, not a canvas; `ImageData.toDataURL` does not exist.** Add an explicit `spriteDataUrl(table,code,overlay,grey): string` helper in `aprsSprites.ts` (putImageData→toDataURL; returns `''` in jsdom). Keep sprite-IDENTITY assertions at the pure-helper level.
- **P0 — WX badge must plot at `cellCenter` (ni5b), not the raw low corner** for ambiguous WX stations. Badge position `= cellCenter({lat,lon,ambiguity})`, identical to the pin; test it.
- **P0 — per-station filter bundle must conditionally include the uncertainty circle (ambiguity>0 only) + badge**; filter removes the WHOLE bundle atomically. Test an AMBIGUOUS WEATHER station: pin+label+circle+badge all leave together.
- **P1s** — popup body derived from live `byCall` (not a stale closure); diff-based marker reconciliation (stable marker identity across `positions` re-renders — do NOT rebuild all markers each render); staleness affects ONLY the pin icon (grey **desaturate** variant, NOT bundle-opacity — that would dim label+disc, a semantic change); pre-bake BOTH colour + grey data-URLs.
- **P2s** — route icons through the `ensureSymbolImage` fallback/brand-logo branch (`FALLBACK_ID`→`renderFallbackBitmap`); keep identity tests pure (jsdom `toDataURL` is blank).

**Round 4 — Claude, strangler-fig + lifecycle:**
- **P1 — use Leaflet NATIVE `maxBounds` + `maxBoundsViscosity`, NOT a ported moveend snap-back clamp.** The maplibre-5.24.0 maxBounds crash (tuxlink-rwo6) that forced the manual clamp DOES NOT apply to Leaflet; the moveend clamp is also weaker (Leaflet CRS wraps `getCenter()`). → Task 4 sets `maxBounds` to the world rectangle; drop the moveend pan-clamp port.
- **P1 — base-layer-swap effect must gate on the `map` STATE, not an `instanceRef`,** so it can never act on a torn-down instance (StrictMode).
- **P2s** — single attribution source of truth (confirm whether Protomaps credit is license-required — see open question); pin `@protomaps/basemaps` EXACTLY (the vendored layer hard-imports its `namedFlavor`); set explicit `zIndex` per base layer (overview low, packs high) so compositing is not add-order-fragile; `whenReady` fires synchronously (overlays get a non-null map a render earlier — Task 5 overlays must not assume null-first); persisted fractional zoom is snapped by Leaflet `zoomSnap:1` (cosmetic). Cleared concerns: no leaflet/maplibre CSS collision; the `maplibregl.addProtocol('pmtiles')` global never runs on a Leaflet path; `tuxlink:map-viewport:aprs` format is read-compatible. **lastKnownPacks transient-failure latent bug → filed bd `tuxlink-kepz`.**

**Round 5 — Claude, testability + pitfalls + completeness (empirically validated Leaflet-in-jsdom):**
- **P0 — canvas-rasterized `L.icon({iconUrl})` is HOLLOW in jsdom** (`getContext('2d')`→null, `toDataURL` unimplemented; no `canvas` pkg). → **Build pins as `L.divIcon` (HTML/DOM, fully jsdom-inspectable)** so sprite identity is assertable, OR explicitly move identity to grim. Decision: use `L.divIcon` with an `<img src=spriteDataUrl>` (DOM-inspectable; real raster proven in grim). Sprite-identity asserted via the pure `spriteDataUrl`/`spriteIdFor` helper.
- **P0 — the 10-case rewrite DROPS ~6 negative/empty RF-honesty cases** the current 30-case suite protects: WX hover card; plots-nothing-when-empty; exact-fix-no-centre-shift; operator-pin-ABSENT when no grid; WX-badge-ABSENT when no weather; recenter-hidden when no grid. → restore all as explicit cases.
- **P1s** — `flyTo({animate:false})` leaves `getZoom()===NaN` in jsdom → test the recenter via `vi.spyOn(map,'flyTo')` on args, not by reading zoom after; tighten the filter test to assert the count delta equals the FULL bundle size (not merely "shrinks"); name the composed-seam testing-pitfall (the mocked base hides the one integration that is Phase 1's point → Task 7 must observe a real `tile://` Range fetch).
- **P2s** — mandate the proven `Object.defineProperty(HTMLElement.prototype, clientWidth/Height)` shim + `afterEach` restore in every Leaflet test; Task 7 wire-walk must verify the map fills the reading-pane grid slot (no 0-height flex collapse) and that the React popup/controls stack ABOVE Leaflet's panes (z-index 200–700); `grep` for other `composeSnapshotHeader`/`exportWxSnapshot` consumers before deleting the PNG path.

**Open question for the operator (does not block planning):** is the Protomaps default credit (`Protomaps © OpenStreetMap`) required by the tile license, or is `© OpenStreetMap contributors` (the current ODbL string) sufficient? Default to the current ODbL string; revisit if the vendored license requires the Protomaps credit.

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

**Design notes (CORRECTED by Codex adrev R2 — the first cut shipped a blank map; this is the validated design):**

Confirmed vendored API (`src/vendor/protomaps-leaflet/index.d.ts`, protomaps-leaflet 5.1.0):
`leafletLayer(opts)` where `LeafletLayerOptions extends L.GridLayerOptions` has: `url?: PMTiles | string`, `sources?: Record<string, SourceOptions>`, `paintRules?: PaintRule[]`, `labelRules?: LabelRule[]`, `maxDataZoom?: number`, `flavor?: string`, `backgroundColor?: string`, `attribution?`, `lang?`, plus inherited `minZoom`/`maxZoom`/`zIndex`/`pane`. Also EXPORTED: `paintRules(flavor): PaintRule[]`, `labelRules(flavor, lang): LabelRule[]`, `PmtilesSource`. The flavor object comes from `@protomaps/basemaps`' `namedFlavor(name)`.

THREE seam rules, each fixing a proven-P0:
1. **PMTiles INSTANCE, not a string** (R2 P0#1). protomaps-leaflet chooses `PmtilesSource` ONLY when a string URL pathname ends `.pmtiles`; `tile://pmtiles/world` (pathname `/world`) would fall through to `ZxySource` (no Range, parses the archive as one MVT tile → blank). Passing `url: new PMTiles('tile://pmtiles/world')` (an instance — the `.d.ts` allows `PMTiles | string`) forces `PmtilesSource`. Import `{ PMTiles }` from `'pmtiles'` (resolves to the repo's pmtiles@4, the same lib the MapLibre path proves can Range-fetch `tile://`). NO `addProtocol` (MapLibre-only).
2. **Cap `maxDataZoom` per source** (R2 P0#2). protomaps-leaflet defaults `maxDataZoom:15` and requests data at z=displayZ−1; the overview archive is z0–6, so above ~z8 it requests z7+ data the archive lacks → blank. Pass `maxDataZoom: 6` for the overview. Each pack carries its real maxzoom (continent-na is z0–14 → `maxDataZoom: 14`); thread the pack maxzoom from `offlineMaps.ts`'s pack metadata if available, else default to 14 for now and note it.
3. **Packs carry NO background and NO labels** (R2 P0#3). A flavored layer paints its `backgroundColor` on EVERY rendered tile; a pack's empty tiles outside its coverage would mask the overview. So the OVERVIEW is the only flavored layer (one global background + labels); each PACK layer passes explicit `paintRules: pmPaintRules(namedFlavor(flavor))`, `labelRules: []` (labels owned by the overview — no duplicate glyph cost), NO `flavor`, NO `backgroundColor` → it draws only its detail geometry, transparent elsewhere. This mirrors `basemapStyle.ts:176–205` (which drops `background` + `symbol` from pack layer sets).

Composite ordering (R4 P2): set explicit `zIndex` — overview `zIndex: 1`, packs `zIndex: 2+i` — so packs paint above the overview regardless of add order. Each pack also gets `minZoom: REGION_MINZOOM` (6).

Record the confirmed signature + these three rules in a top-of-file comment.
- Dark mode: pass `flavor: 'dark'` (and `lang: 'en'`). Do NOT bake colors.
- Pack compositing mirrors `basemapStyle.ts`'s intent: overview overzooms past z6 (never blank); each pack is a separate protomaps-leaflet layer clamped to `minZoom >= 6` drawn above the overview, so detail wins inside coverage and the overview shows outside it. protomaps-leaflet layers accept Leaflet `minZoom`/`maxZoom`/`pane` options; set the pack `pane`/zIndex above the overview.

- [ ] **Step 1: Write the failing test**

```ts
// src/map/basemapLeaflet.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the seam libs so the test asserts WIRING, not render. Capture each
// leafletLayer({...}) opts object; make PMTiles instances identifiable by url.
const { leafletLayerSpy, pmPaintRulesSpy } = vi.hoisted(() => ({
  leafletLayerSpy: vi.fn((opts: Record<string, unknown>) => ({ __pm: true, opts })),
  pmPaintRulesSpy: vi.fn(() => [{ dataLayer: 'roads', symbolizer: {} }]),
}));
vi.mock('../vendor/protomaps-leaflet', () => ({
  leafletLayer: leafletLayerSpy,
  paintRules: pmPaintRulesSpy,
  labelRules: vi.fn(() => []),
}));
vi.mock('pmtiles', () => ({
  PMTiles: vi.fn().mockImplementation((url: string) => ({ __pmtiles: true, url })),
}));
vi.mock('@protomaps/basemaps', () => ({ namedFlavor: vi.fn((n: string) => ({ __flavor: n })) }));

import { buildBaseLayers, PMTILES_TILE_URL } from './basemapLeaflet';

const optsOf = (i: number) => leafletLayerSpy.mock.calls[i][0] as Record<string, any>;

beforeEach(() => { leafletLayerSpy.mockClear(); });

describe('basemapLeaflet', () => {
  it('overview: one flavored layer over a PMTiles INSTANCE of the world seam, maxDataZoom 6, zIndex 1', () => {
    const layers = buildBaseLayers('dark', []);
    expect(layers).toHaveLength(1);
    const o = optsOf(0);
    expect(o.flavor).toBe('dark');                       // overview carries the flavor (background+labels)
    expect(o.url.__pmtiles).toBe(true);                  // a PMTiles INSTANCE, not a string (R2 P0#1)
    expect(o.url.url).toBe('tile://pmtiles/world');
    expect(o.maxDataZoom).toBe(6);                       // overzoom cap (R2 P0#2)
    expect(o.zIndex).toBe(1);
  });

  it('pack: NO flavor, NO backgroundColor, explicit paintRules + empty labelRules, maxDataZoom 14, minZoom 6, higher zIndex (R2 P0#3)', () => {
    const layers = buildBaseLayers('dark', [{ id: 'continent-na' }]);
    expect(layers).toHaveLength(2);
    const p = optsOf(1);
    expect(p.url.url).toBe('tile://pmtiles/continent-na');
    expect(p.flavor).toBeUndefined();                    // packs are NOT flavored (no background mask)
    expect(p.backgroundColor).toBeUndefined();
    expect(Array.isArray(p.paintRules)).toBe(true);      // explicit paint rules from namedFlavor(flavor)
    expect(p.labelRules).toEqual([]);                    // labels owned by overview
    expect(p.maxDataZoom).toBe(14);
    expect(p.minZoom).toBe(6);
    expect(p.zIndex).toBeGreaterThan(optsOf(0).zIndex);  // packs paint above the overview
  });

  it('exposes the tile:// URL helper', () => {
    expect(PMTILES_TILE_URL('world')).toBe('tile://pmtiles/world');
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run src/map/basemapLeaflet.test.ts`
Expected: FAIL — the current committed `basemapLeaflet.ts` (string `url`, per-pack flavor) does not satisfy the corrected assertions.

- [ ] **Step 3: Re-implement `basemapLeaflet.ts`** per the corrected design notes. Overview: `leafletLayer({ url: new PMTiles(PMTILES_TILE_URL('world')), flavor, lang:'en', attribution: OSM_ATTRIBUTION, maxDataZoom: 6, zIndex: 1 })`. Each pack: `leafletLayer({ url: new PMTiles(PMTILES_TILE_URL(pack.id)), paintRules: pmPaintRules(namedFlavor(flavor)), labelRules: [], lang:'en', maxDataZoom: pack.maxZoom ?? 14, minZoom: REGION_MINZOOM, zIndex: 2 + i })`. Import `PMTiles` from `'pmtiles'`, `paintRules as pmPaintRules` from the vendored lib, `namedFlavor` from `'@protomaps/basemaps'`. Keep `PMTILES_TILE_URL`, `OSM_ATTRIBUTION`, `REGION_MINZOOM`, `BasemapFlavor`, `PackSource` (extend `PackSource` with optional `maxZoom?: number`). Return `[overview, ...packLayers]`.

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm vitest run src/map/basemapLeaflet.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit** (amends nothing — a new commit superseding the broken seam; the prior commit stays in history)

```bash
git add src/map/basemapLeaflet.ts src/map/basemapLeaflet.test.ts
git commit -m "fix(map): correct protomaps-leaflet tile:// seam wiring (tuxlink-6kdw)

PMTiles instance (not string url) so PmtilesSource is chosen and bytes are
Range-fetched; maxDataZoom caps per source so overzoom does not request absent
tiles; packs drop flavor/background/labels so empty pack tiles never mask the
overview. Fixes three P0s from the cross-provider adrev.

Agent: sage-fox-mesa
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

**Note:** also verify under the REAL vendored module (no mock) that `new PMTiles('tile://pmtiles/world')` constructs and that `buildBaseLayers` returns objects — a thin non-mocked smoke (`pnpm typecheck` + a `it.skip`-able construction probe). The true seam (real Range fetch + render) is proven in Task 7.

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
- Pan-clamp: **use Leaflet NATIVE `maxBounds` + `maxBoundsViscosity: 1.0`** set at construction to the world rectangle (`[[-MERCATOR_MAX_LAT, -180], [MERCATOR_MAX_LAT, 180]]` from `projection.ts`). Do NOT port MapLibreMap's moveend snap-back clamp (R4 P1): the maplibre-5.24.0 `maxBounds` constructor crash (tuxlink-rwo6) that forced the manual clamp DOES NOT exist in Leaflet, and the ported clamp is weaker (Leaflet's CRS wraps `getCenter()` so the raw-vs-clamped diff misfires). Native `maxBounds` is the correct, simpler tool here.
- `flyTo` on a post-construct `initialCenter` change (the async-arrival recenter): `map.flyTo([lat,lon])`; skip the construct-time center exactly like `skipConstructCenter`.
- Attribution: construct the map with `attributionControl: false` (Leaflet adds one by default → would duplicate), then add exactly one `L.control.attribution({ prefix: false })` and set `OSM_ATTRIBUTION` (Codex adrev P2). Scale: `L.control.scale({ imperial: true, metric: true })`.
- **Test isolation (Codex adrev P1):** `LeafletMap.test.tsx` MUST mock `./basemapLeaflet` so `buildBaseLayers` returns an inert layer (`L.layerGroup()`), NOT a real protomaps-leaflet `GridLayer` — a real GridLayer tries to load/render tiles to canvas, which jsdom cannot do (it will throw or hang). The real vector render is proven only in Task 7 (Tauri/grim). Tests exercise the substrate's lifecycle/props/event wiring against the inert base. Use the prototype-wide shim `Object.defineProperty(HTMLElement.prototype, 'clientWidth'/'clientHeight', { configurable:true, value })` with an `afterEach` restore (R5 P2).
- **Adrev hardening (MANDATORY):**
  - The flavor/pack base-layer swap effect MUST gate on the `map` STATE (`if (!map) return;`), not an `instanceRef`, so it never mutates a torn-down/replaced instance under StrictMode (R4 P1). Dedupe on `flavor|packIds` to skip redundant rebuilds; remove the old base layers then add `buildBaseLayers(...)` (the zIndex on each base layer comes from `basemapLeaflet`, so add-order is not load-bearing).
  - `whenReady` fires SYNCHRONOUSLY when the map is constructed with center+zoom (R4 P2) — `setMap(instance)` runs inside the construct effect before its cleanup returns. Benign, but overlays get a non-null map one render earlier than under MapLibre; do not rely on a null-first render. Keep a `removed` flag set in cleanup before `map.remove()` so a `moveend`/`whenReady` during teardown is ignored.
  - StrictMode double-invoke: construct→cleanup→construct must converge to ONE map with no "Map container is already initialized" error — null the container's `_leaflet_id` is NOT needed if `map.remove()` runs in cleanup; verify the second construct succeeds (test: mount under `<StrictMode>` or simulate double-invoke).
  - Persisted fractional zoom is snapped by Leaflet's default `zoomSnap:1` (R4 P2) — acceptable; do not change `zoomSnap` (it affects feel) unless the operator asks.
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
| Category filter | `setFilter` on layers (the drunk-map bug origin) | **Per-station layer bundle (Codex adrev P1):** each heard station owns MULTIPLE Leaflet layers (pin marker, label, WX badge, uncertainty circle, click handlers). Group each station's layers into one keyed `L.featureGroup` (or a `Map<call, L.Layer[]>`), and filter by adding/removing the WHOLE bundle from the parent `LayerGroup` when `category` changes — never filter "markers" alone (that orphans the uncertainty disc / badge). This ELIMINATES the `setFilter`-on-`styledata` self-loop failure class — do NOT reintroduce any per-frame style mutation. Test MUST assert ALL of a filtered-out station's layers disappear. |
| Staleness tick | `setFeatureState({stale})` on NOW_TICK | iterate per-station bundles, swap icon/opacity per `now - p.at > STALE_MS`. |
| PNG export | `map.getCanvas()` (WebGL) + `preserveDrawingBuffer` saga | **DEFERRED this phase (Codex adrev P0 → bd `tuxlink-a7qt`).** Naive canvas compositing is broken: protomaps-leaflet renders per-tile positioned `<canvas>` elements (not one canvas) requiring leaflet pane/tile transforms, and pins are DOM markers a canvas copy omits. Do NOT ship a broken Export PNG. For Phase 1, remove the `WxExportControl` button from `AprsPositionsMap` (and drop `exportWxSnapshot`/its tests) and leave a code comment pointing at `tuxlink-a7qt`. The Winlink-text Weather SITREP (`WxSitrepControl`) — the actually-load-bearing report path — STAYS. Note the temporary Export-PNG regression in the handoff for operator awareness. |

**Adrev hardening (MANDATORY — these are hard-won RF-honesty/identity fixes the one-line table endangers; each needs a targeted test):**
- **Pins are `L.divIcon`, NOT `L.icon`+canvas (R5 P0).** jsdom has no canvas (`getContext('2d')`→null, `toDataURL` unimplemented), so a canvas-rasterized `iconUrl` is hollow and untestable. Render the pin as a `divIcon` whose HTML is `<img class="aprs-pin" src="${spriteDataUrl}">` (DOM-inspectable; real raster proven in grim). Sprite IDENTITY is asserted via the pure helper, not the rendered pixels.
- **Add `spriteDataUrl(table, code, overlay, grey): string` to `aprsSprites.ts` (R3 P0).** `renderSymbolBitmap`/`renderFallbackBitmap` return `ImageData` (no `.toDataURL`). The helper: new canvas → `putImageData` → `toDataURL('image/png')`; returns `''` in jsdom (null ctx). It MUST route through the SAME fallback/brand-logo branch `ensureSymbolImage` uses (`id===FALLBACK_ID ? renderFallbackBitmap() : renderSymbolBitmap(...)`) so unresolved/brand symbols get the neutral dot, not a broken image (R3 P2).
- **`whenSheetsReady` re-bake is REQUIRED (R3 P0) — without it pins are BLANK forever.** Sprite sheets decode async; the first bake on mount is transparent; a data-URL icon never re-decodes (worse than MapLibre, which recovered on `styledata`). On `whenSheetsReady`, recompute each station's `spriteDataUrl` and `marker.setIcon(newDivIcon)`. Test: drive `whenSheetsReady`, assert the marker icon HTML/src changes.
- **Uncertainty circle radius keeps the `×√2` (R3 P0 — LOAD-BEARING).** `L.circle([lat,lon], { radius: ambiguityRadiusMeters(level) * Math.SQRT2, ... })`. The √2 circumscribes the ambiguity BOX; dropping it under-claims uncertainty (operator trusts precision the wire didn't carry). The note "L.circle takes meters directly" means drop `circlePolygon` ONLY — NOT the √2. Test asserts `getRadius() === ambiguityRadiusMeters(level)*Math.SQRT2`. Center on `cellCenter`.
- **WX badge plots at `cellCenter` (R3 P0 / ni5b)** — `cellCenter({lat:w.lat, lon:w.lon, ambiguity:w.ambiguity})`, identical to the pin. Test an ambiguous WX station: badge latlng === pin latlng === cellCenter (not the raw low corner).
- **Per-station bundle = conditional (R3 P0).** Build one keyed `L.featureGroup` per call containing `[pinMarker, labelMarker, uncertaintyCircle (ONLY if ambiguity>0), wxBadge (ONLY if a WxStation)]` + click handlers. Category filter adds/removes the WHOLE bundle atomically. Test an AMBIGUOUS WEATHER station: assert the filtered-out bundle's pin+label+circle+badge ALL leave the parent group together (count delta === full bundle size, not merely "shrinks").
- **Diff-based reconciliation, stable marker identity (R3 P1).** Keep a `Map<call, bundle>` across renders; on `positions` change, ADD new calls, UPDATE existing markers in place (`setLatLng`/`setIcon`), REMOVE dropped calls — do NOT rebuild all bundles each render (churn/leak/flicker; re-attaches 100s of handlers). Test: re-render with identical positions → marker instances are identity-stable. Click/hover handlers close over `call` only; popup/card body derives from a live `byCall` ref (R3 P1) so a re-beacon updates and a pruned station closes it.
- **Staleness affects ONLY the pin icon (R3 P1) — grey DESATURATE variant, not opacity.** On the NOW_TICK, swap the pin's divIcon between the colour and pre-baked grey `spriteDataUrl` (grey=true → `desaturate`). Do NOT dim the whole bundle (would change label/disc opacity — a semantic change from current, which greys only the pin). Pre-bake BOTH colour and grey data-URLs per station. Test: a stale station's pin uses the grey src; its label/disc opacity is unchanged.
- **PNG export removal is self-contained (R5 P2).** Before deleting `exportWxSnapshot`/`WxExportControl`, `grep -rn 'composeSnapshotHeader\|exportWxSnapshot' src/` — if `composeSnapshotHeader`/`wxSnapshot` has no other consumer, leave the module but drop the control + its tests; if shared, keep the module. Leave a code comment pointing at `tuxlink-a7qt`. Record the temporary Export-PNG regression in the handoff for operator awareness.

**Do NOT** add features, change copy, or alter the RF-honesty semantics. This is a faithful re-expression on a new engine. The ONLY intentional scope change this phase is the temporary removal of Export PNG (deferred to `tuxlink-a7qt`); everything else is behavior-preserving.

- [ ] **Step 1: Rewrite the test file first (TDD)** — `AprsPositionsMap.test.tsx`

Port EVERY assertion the current 30-case suite makes (R5 P0 — do not drop the negative/empty cases; they ARE the RF-honesty "never draw what wasn't heard" guarantees). Cases:
1. renders the map container (`data-testid="aprs-positions-map"`).
2. a heard station produces a marker at its decoded lat/lon (assert a marker in the positions group at the expected latlng) AND its sprite identity via the pure `spriteIdFor`/`spriteDataUrl` helper (NOT the rendered pixel — jsdom toDataURL is blank).
3. **exact (non-ambiguous) fix plots at the decoded coord with NO centre-shift** (the RF-honesty twin of case 4).
4. an ambiguous fix renders an uncertainty circle (not a sharp pin) with `getRadius() === ambiguityRadiusMeters(level)*Math.SQRT2`, centred on `cellCenter`.
5. clicking a pin opens the popup with callsign, symbol name, last-heard age; ambiguous adds the ±note; re-render with an updated fix → popup updates; re-render with the station pruned → popup closes.
6. operator grid renders the distinct "you" `circleMarker`; **AND: no operator grid → no "you" pin** (negative case).
7. WX station renders a temperature badge at `cellCenter`; clicking it calls `onFocusStation(call)`; **AND: hovering it shows the `aprs-wx-card` full reading, leaving hides it** (R5 P0 — the hover card was dropped); **AND: station with no heard weather → no badge** (negative case).
8. category filter: filtering out an AMBIGUOUS WEATHER station removes its FULL bundle (pin+label+circle+badge) — assert the parent group's layer-count delta equals the bundle size, not merely "shrinks" (R5 P1).
9. stale station (now-at > STALE_MS) → pin uses the grey (desaturated) `spriteDataUrl`; its label + uncertainty disc opacity are UNCHANGED (R3 P1 — staleness is pin-only).
10. `whenSheetsReady` re-bake: after sheets ready, the pin's icon src/HTML changes from the blank/initial bake (R3 P0).
11. diff reconciliation: re-render with identical `positions` → the same marker instances persist (identity-stable, R3 P1).
12. `ambiguityRadiusMeters` unit cases unchanged.
13. WX SITREP button disabled with zero WX stations; enabled with ≥1.
14. **plots nothing when `positions` is empty** (negative case).
15. `LeafletRecenterControl`: clicking it calls `flyTo` with the operator latlng — assert via `vi.spyOn(map,'flyTo')` on the ARGS (do NOT read `getZoom()` after — `flyTo({animate:false})` leaves zoom NaN in jsdom, R5 P1); **AND: recenter control hidden when no operator grid** (negative case).
16. viewport restore (tuxlink-dwzu): a saved viewport restores center+zoom (assert `captured.getCenter()`/`getZoom()` after `whenReady`); first-run with operator grid centers on the operator at `OPERATOR_ZOOM`; no grid + no saved view → world view.

Use the sized-container shim + `@tauri-apps/api/core` invoke mock. **Mock `../map/basemapLeaflet` so `buildBaseLayers` returns an inert `L.layerGroup()` (Codex adrev P1)** — do NOT let a real protomaps-leaflet GridLayer load tiles in jsdom. Where the old test used a MapLibre test double, replace with real Leaflet + the shim (Leaflet markers/layers are inspectable: `layerGroup.getLayers()`, `marker.getLatLng()`, `marker.getIcon()`). Drop the old `exportWxSnapshot` / Export-PNG test cases (feature deferred to `tuxlink-a7qt`).

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
- [ ] **Observe a REAL `tile://` Range request** in the WebKit network panel (R2 P2 / R5 P1) — a cached/overview-only render could mask a broken pack fetch. Confirm a 206 (or pmtiles' Range GET) to `tile://pmtiles/world` AND, with a pack installed, to `tile://pmtiles/<pack>`. "A grid appears" is NOT sufficient proof of the seam.
- [ ] **Layout (R5 P2):** confirm the Leaflet `.leaflet-container` FILLS the reading-pane grid slot (no 0-height flex/grid collapse — a classic Leaflet-in-flex bug) and that the React popup/controls (`.aprs-positions-map__popup`, `.aprs-wx-filter`, recenter) stack ABOVE Leaflet's panes (Leaflet uses z-index 200–700; controls must clear that). Adjust `AprsPositionsMap.css` minimally if needed.
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
