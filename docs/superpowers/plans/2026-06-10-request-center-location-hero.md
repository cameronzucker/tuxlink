# Request Center — location-aware "For your location" hero Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**bd:** tuxlink-96lu · branch `bd-tuxlink-loc/location-hero` · worktree `worktrees/bd-tuxlink-loc-location-hero/`
**Spec (locked):** [docs/superpowers/specs/2026-06-10-request-center-location-hero-design.md](../specs/2026-06-10-request-center-location-hero-design.md)
**Mock:** [docs/design/mockups/2026-06-10-request-center-location-hero.html](../../design/mockups/2026-06-10-request-center-location-hero.html) · render `dev/scratch/location-hero-mock/02-corrected-1920.png`

**Goal:** Replace the Request Center's coarse State+Marine "For your location" pair with the complete set of genuinely-local products that resolve **and apply** for the operator's Maidenhead grid — the exact NWS public forecast zone (primary), the tightest-scoped regional radar, and the sea-area marine forecast (coastal only).

**Architecture:** Extend the existing pure geo layer (`src/request/geo.ts`, which already does ray-casting point-in-polygon over a bundled simplified GeoJSON for US states). Add three committed data assets produced by a committed generation script: a simplified+pruned NWS public-zone GeoJSON, a **vetted** NWS-zone-id → Winlink-catalog-filename mapping, and a curated radar-region bbox table. New pure resolvers (`gridToNwsZone`, `gridToRadarRegion`) feed a rewritten location section in `buildSections`; the UI grows a primary zone card plus a supporting radar/marine grid. Mapping completeness is enforced by a test (Definition of Done #5) — no fuzzy auto-match ships unreviewed.

**Tech Stack:** TypeScript, React 18, Vitest, Vite; Node generation script run with `pnpm tsx`; data from the public-domain NWS public-zone dataset (`api.weather.gov/zones?type=public&area=<ST>&include_geometry=true`) + the bundled Winlink catalog (`src-tauri/resources/catalog/winlink-queries.txt`).

---

## Pre-flight (read before Task 1)

- **All commands run from the worktree:** `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-loc-location-hero/`. `node_modules` is a symlink — already present. Do **not** `cd` into the main checkout.
- **Test commands (verify in this exact form):**
  - Scoped vitest: `pnpm vitest run src/request/<file>` (single file) — fast, use during TDD.
  - Full request suite (the DoD #6 gate): `pnpm vitest run src/request`
  - Typecheck: `pnpm tsc --noEmit`
  - CI parity (run before any push, per memory `scoped_vitest_misses_contract_tests`): `pnpm exec eslint . && pnpm vitest run` (CI also runs cargo clippy — no Rust changes here, so clippy is N/A, but eslint + full vitest are not).
- **Render gate (CRITICAL — non-negotiable, cost two regressions last session):** every UI-affecting task ends by rendering in real WebKitGTK at **1920×1080** with grid **`CN87uo`** (6-char — 4-char hid a geo-collapse bug) and reading the PNG. Command in Task 12; memory ref `reference_webkitgtk_render_harness`.
- **Commit discipline:** every commit carries `Agent: <moniker>` + `Co-Authored-By:` trailers (the `.githooks/commit-msg` hook rejects a missing moniker). Conventional commit types. Push the instant a unit is committed + green (memory `never_hold_a_push`) — do not batch pushes to session end.
- **Geometry technique already exists:** `latLonToUsState` + `pointInPolygonWithHoles` + `pointInRing` in `geo.ts:106-158`. `gridToNwsZone` is the same algorithm over a finer polygon set — reuse the ring helpers, do not reimplement ray-casting.

---

## Data-asset contract (locks the shapes used across tasks)

Three committed assets under `src/request/`. All three are produced/refreshed by `scripts/build-request-geo.ts` and committed as reviewed artifacts.

**1. `src/request/nws-zones.geo.json`** — simplified GeoJSON `FeatureCollection`, pruned to catalog states. Each feature:
```json
{ "type": "Feature",
  "properties": { "id": "WAZ558", "name": "Seattle and Vicinity", "state": "WA" },
  "geometry": { "type": "Polygon" | "MultiPolygon", "coordinates": [...] } }
```

**2. `src/request/nws-zone-to-catalog.json`** — vetted map, NWS zone id → catalog filename:
```json
{ "_source": { "dataset": "api.weather.gov/zones?type=public", "fetched": "2026-06-10", "zoneCount": 3471 },
  "map": { "WAZ558": "WA_ZON_SEA", "WAZ029": "WA_ZON_BLUF", "...": "..." } }
```
Values are real catalog filenames (`CatalogEntry.filename`). The completeness test reads `map`'s **values**.

**3. `src/request/nws-zone-unmapped.json`** — catalog zone-forecast filenames intentionally NOT mapped to a single NWS zone, each with a reason:
```json
{ "unmapped": {
    "WA_FOR_EAST": "multi-zone regional (Eastern Washington spans many NWS zones)",
    "WA_FOR_WA":   "state-level forecast, not a single public zone",
    "WA_TAB_NW":   "tabular multi-zone product" } }
```

**4. `src/request/radar-regions.json`** — curated bbox table for `WX_US_RAD`:
```json
{ "_source": "NWS local-radar sector extents; see scripts/build-request-geo.ts header",
  "regions": [
    { "filename": "US.RAD.PSND", "name": "Puget Sound & SJDF", "bbox": [-124.9, 46.9, -121.4, 49.0] },
    { "filename": "US.RAD.NWWA", "name": "W Washington & NW Oregon", "bbox": [-124.9, 45.2, -120.8, 49.0] },
    { "filename": "US.RAD.PNW",  "name": "Pacific Northwest", "bbox": [-125.0, 41.9, -116.5, 49.1] }
  ] }
```
`bbox` is `[west, south, east, north]` in decimal degrees (lon negative in W hemisphere).

**Completeness-test target rule (used by Tasks 4 & 7):** a catalog entry is a *mappable zone forecast* iff its category matches `^WX_US_[A-Z]{2}$` for a real US state/territory USPS code (the catalog's two-letter `WX_US_<ST>` categories — `WX_US_RAD`, `WX_US_COAST`, `WX_US_GUAM`, `WX_US_SELCTY`, `WX_US_SAMOA`, `WX_US_OUTDR` are excluded as non-two-letter or non-state) **and** its `description` matches `/zone forecast/i`. Every such filename MUST be a value in `nws-zone-to-catalog.json#map` OR a key in `nws-zone-unmapped.json#unmapped`. (Territories with two-letter codes that DO have zone forecasts — `PR`, `HI`, `AK` — are included; `GUAM`/`SAMOA` are excluded by the two-letter rule and live in Browse.)

---

## File structure

| File | Create/Modify | Responsibility |
|---|---|---|
| `scripts/build-request-geo.ts` | Create | Fetch NWS zones, simplify+prune geometry, auto-match zone↔catalog, scaffold radar table, emit all 4 assets + unresolved report. |
| `scripts/build-request-geo.md` | Create | Provenance + regeneration runbook (mirrors `scripts/build-us-states-geojson.md`). |
| `src/request/nws-zones.geo.json` | Create | Bundled simplified NWS public-zone polygons (catalog states). |
| `src/request/nws-zone-to-catalog.json` | Create | Vetted zone-id → catalog-filename map. |
| `src/request/nws-zone-unmapped.json` | Create | Explicit unmapped-by-design list. |
| `src/request/radar-regions.json` | Create | Curated radar-region bbox table. |
| `src/request/geo.ts` | Modify | Add `gridToNwsZone`, `gridToRadarRegion`, `NwsZone`/`RadarRegion` types. |
| `src/request/geo.test.ts` | Modify | Tests for the two new resolvers. |
| `src/request/catalogMap.ts` | Modify | Add `zoneForecastEntry(entries, zoneId)` + radar entry resolver. |
| `src/request/catalogMap.test.ts` | Modify | Mapping-completeness test + radar-completeness test + resolver tests. |
| `src/request/sections.ts` | Modify | Rewrite location section: zone primary + radar + marine, adaptive. |
| `src/request/sections.test.ts` | Modify | Adaptive-set tests (coastal/inland/non-US). |
| `src/request/RequestCenter.tsx` | Modify | Primary zone card + supporting `locgrid`; render card `meta`. |
| `src/request/RequestCenter.css` | Modify | `.zone`, `.zmeta`, `.locgrid`, `.fmeta` from the mock. |
| `src/request/RequestCenter.test.tsx` | Modify | Hero structure assertions (zone card primary, meta lines). |
| `src/request/RequestCenter.app.test.tsx` | Modify | App-level mount renders the resolved hero end-to-end. |
| `dev/render-harness/harness.tsx` | Modify | Canned catalog includes real zone + radar entries so the hero resolves. |

---

## Task 1: Generation script — fetch NWS public zones

**Files:**
- Create: `scripts/build-request-geo.ts`
- Create: `scripts/build-request-geo.md`

This task only fetches + caches; simplification and mapping come in later tasks. The script is incremental and idempotent (cache to a gitignored scratch dir so re-runs don't re-hit the network).

- [ ] **Step 1: Determine the catalog state set**

Add a helper that reads `src-tauri/resources/catalog/winlink-queries.txt` and returns the distinct two-letter `WX_US_<ST>` categories that contain at least one `/zone forecast/i` entry. Expected set (verify at runtime — do not hardcode blindly): `AK AL AR AZ CA CO CT DE FL GA HI IA ID IL IN KS KY LA MA MD ME MI MN MO MS MT NC ND NE NH NJ NM NV NY OH OK OR PA PR RI SC SD TN TX UT VA VT WA WI WV WY` (plus any the catalog adds). The two-letter rule excludes `GUAM`/`SAMOA`/`SELCTY`/`OUTDR`/`COAST`/`RAD`.

- [ ] **Step 2: Fetch zones per state with a polite client**

For each catalog state, GET `https://api.weather.gov/zones?type=public&area=<ST>&include_geometry=true` with header `User-Agent: tuxlink-dev (cameronzucker@gmail.com)` (NWS requires a UA), 200 ms between requests, retry-once on non-200. Write each raw response to `dev/scratch/request-geo/raw/<ST>.json`. Record the max `effectiveDate` seen as the dataset version.

```ts
const UA = 'tuxlink-dev (cameronzucker@gmail.com)';
async function fetchState(st: string): Promise<unknown> {
  const url = `https://api.weather.gov/zones?type=public&area=${st}&include_geometry=true`;
  for (let attempt = 0; attempt < 2; attempt++) {
    const res = await fetch(url, { headers: { 'User-Agent': UA, Accept: 'application/geo+json' } });
    if (res.ok) return res.json();
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error(`NWS fetch failed for ${st}`);
}
```

- [ ] **Step 3: Write the provenance doc**

`scripts/build-request-geo.md` documents: the source dataset + URL, the `pnpm tsx scripts/build-request-geo.ts` invocation, the gitignored raw cache location, what each emitted asset is, and that re-running refreshes the data when the catalog or NWS zones change. Mirror the tone of `scripts/build-us-states-geojson.md`.

- [ ] **Step 4: Run the fetch and verify the cache**

Run: `pnpm tsx scripts/build-request-geo.ts --fetch-only`
Expected: `dev/scratch/request-geo/raw/WA.json` exists and contains `"id": "WAZ558"` and a non-null `geometry`. Verify: `ls dev/scratch/request-geo/raw | wc -l` equals the catalog-state count.

> **If the shell has no outbound network to api.weather.gov:** the GIS shapefile bulk source (`https://www.weather.gov/source/gis/Shapefiles/WSOM/z_<MMYY>.zip`, public domain) is the documented fallback — convert with `mapshaper`/`ogr2ogr` to the same per-state JSON shape, then continue. Note the substitution in `build-request-geo.md`. (Connectivity confirmed during planning, so the primary path should work.)

- [ ] **Step 5: Commit**

```bash
git add scripts/build-request-geo.ts scripts/build-request-geo.md .gitignore
git commit -m "feat(request): NWS public-zone fetch for location hero geo data (tuxlink-96lu)"
```
(Add `dev/scratch/request-geo/` to `.gitignore` if not already covered by `dev/scratch/`.)

---

## Task 2: Simplify + prune → `nws-zones.geo.json`

**Files:**
- Modify: `scripts/build-request-geo.ts`
- Create: `src/request/nws-zones.geo.json`

- [ ] **Step 1: Simplify geometry**

Add a simplification pass (Douglas–Peucker; use `@turf/simplify` if already a dep, else a vendored ~30-line DP implementation — check `package.json` first). Target tolerance ~0.005° (matches the `us-states.geo.json` visual fidelity). Drop the `@id`/`@context`/date properties; keep only `{ id, name, state }`.

- [ ] **Step 2: Emit the pruned FeatureCollection**

Write `src/request/nws-zones.geo.json` as a `FeatureCollection` of all fetched zones (already pruned to catalog states by Task 1). Pretty-print with 0 decimals beyond 4 places to bound file size.

- [ ] **Step 3: Verify bundle size + a known point**

Run: `pnpm tsx scripts/build-request-geo.ts` then `ls -lh src/request/nws-zones.geo.json`
Expected: a single file, ideally < 2 MB (if larger, raise the simplify tolerance to 0.01° and re-run). Spot-check: the feature with `id: "WAZ558"` exists and its name is `"Seattle and Vicinity"`.

- [ ] **Step 4: Commit**

```bash
git add scripts/build-request-geo.ts src/request/nws-zones.geo.json
git commit -m "feat(request): bundle simplified NWS public-zone geometry (tuxlink-96lu)"
```

---

## Task 3: Auto-match zone↔catalog → candidate map + unmapped + unresolved report

**Files:**
- Modify: `scripts/build-request-geo.ts`
- Create: `src/request/nws-zone-to-catalog.json`
- Create: `src/request/nws-zone-unmapped.json`

- [ ] **Step 1: Build the catalog zone-forecast index**

From the catalog, collect every *mappable zone forecast* per the completeness-test target rule (category `^WX_US_[A-Z]{2}$`, description `/zone forecast/i`). For each, compute a normalised key: lowercase, strip a trailing `" zone forecast"`, collapse whitespace, drop punctuation.

- [ ] **Step 2: Auto-match against NWS zone names**

For each NWS zone (from the fetched data, grouped by state), normalise its `name` the same way and match against the catalog index **within the same state**. Three outcomes:
  - **Exact normalised match** → write to `map` (zone-id → catalog filename). Example: catalog `WA_ZON_BLUF` "Foothills of the Blue Mountains of Washington" ↔ NWS `WAZ029` "Foothills of the Blue Mountains of Washington".
  - **Catalog description starts `"zone forecast for"`** (regional, e.g. `WA_FOR_EAST` "Zone Forecast for Eastern Washington") → write filename to `nws-zone-unmapped.json#unmapped` with reason `"multi-zone regional"`.
  - **No exact match + not a regional** (abbreviated description, e.g. `WA_ZON_CAKCF` "F-hills & Valleys of cent King County Cascades") → append to `dev/scratch/request-geo/unresolved.txt` as `<state> <filename> | <description> | candidates: <top-3 fuzzy NWS names+ids>`. Do **not** auto-write a guess into `map`.

```ts
function normalise(s: string): string {
  return s.toLowerCase().replace(/\s+zone forecast\s*$/,'').replace(/[^a-z0-9 ]/g,'').replace(/\s+/g,' ').trim();
}
```

- [ ] **Step 3: Emit candidate assets + report**

Write `nws-zone-to-catalog.json` (with `_source` metadata + `map`), `nws-zone-unmapped.json` (regionals so far), and `dev/scratch/request-geo/unresolved.txt`. Log a summary: `mapped=N  unmapped=M  unresolved=K`.

- [ ] **Step 4: Run + inspect the split**

Run: `pnpm tsx scripts/build-request-geo.ts`
Expected: `unresolved.txt` is non-empty (the abbreviated tail — e.g. several WA `WA_ZON_CA*` entries). The map has the bulk of clean matches. No guesses in the map. Verify `WA_ZON_BLUF` is a value in the map and `WA_FOR_EAST` is a key in unmapped.

- [ ] **Step 5: Commit (candidate state)**

```bash
git add scripts/build-request-geo.ts src/request/nws-zone-to-catalog.json src/request/nws-zone-unmapped.json
git commit -m "feat(request): auto-matched zone↔catalog map + regional unmapped list (tuxlink-96lu)"
```

---

## Task 4: Mapping-completeness test (RED) — Definition of Done #5

**Files:**
- Modify: `src/request/catalogMap.test.ts`
- Test target: the real bundled catalog + the two JSON assets.

This test is the guardrail that makes the hand-resolution in Task 5 *finished* rather than *guessed at*. Write it before resolving the tail so it stays RED until coverage is complete.

- [ ] **Step 1: Write the failing completeness test**

```ts
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import zoneMap from './nws-zone-to-catalog.json';
import unmapped from './nws-zone-unmapped.json';

// Parse the real bundled catalog the same way the Rust parser does (pipe-delimited).
function loadCatalogZoneForecasts(): { category: string; filename: string; description: string }[] {
  const txt = readFileSync(
    resolve(__dirname, '../../src-tauri/resources/catalog/winlink-queries.txt'),
    'utf8',
  );
  return txt
    .split('\n')
    .map((l) => l.replace(/^﻿/, '').trim())
    .filter(Boolean)
    .map((l) => { const [category, filename, description] = l.split('|'); return { category, filename, description }; })
    .filter((e) => /^WX_US_[A-Z]{2}$/.test(e.category) && /zone forecast/i.test(e.description ?? ''));
}

describe('NWS zone mapping completeness (DoD #5)', () => {
  it('every catalog zone-forecast filename is mapped or explicitly unmapped-by-design', () => {
    const mappedFilenames = new Set(Object.values((zoneMap as { map: Record<string,string> }).map));
    const unmappedFilenames = new Set(Object.keys((unmapped as { unmapped: Record<string,string> }).unmapped));
    const missing = loadCatalogZoneForecasts()
      .map((e) => e.filename)
      .filter((f) => !mappedFilenames.has(f) && !unmappedFilenames.has(f));
    expect(missing, `Unresolved catalog zone forecasts:\n${missing.join('\n')}`).toEqual([]);
  });
});
```

- [ ] **Step 2: Run — verify it FAILS with the unresolved tail listed**

Run: `pnpm vitest run src/request/catalogMap.test.ts`
Expected: FAIL; the assertion message lists exactly the filenames in `unresolved.txt` (e.g. `WA_ZON_CAKCF`, `WA_ZON_CAPKF`, …).

- [ ] **Step 3: Commit the RED test**

```bash
git add src/request/catalogMap.test.ts
git commit -m "test(request): mapping-completeness gate for zone↔catalog (DoD #5) (tuxlink-96lu)"
```

---

## Task 5: Hand-resolve the unresolved tail → completeness GREEN

**Files:**
- Modify: `src/request/nws-zone-to-catalog.json` (add resolved entries to `map`)
- Modify: `src/request/nws-zone-unmapped.json` (add genuinely-unmappable entries)

This is the **heavy, vetted** part (memory: *alpha = vettedness*). Work state-by-state through `dev/scratch/request-geo/unresolved.txt`. **Batch by state** so each batch is reviewable. For each unresolved filename:

1. Read its catalog description (abbreviated).
2. Read the candidate NWS zone names+ids for that state (the report's `candidates:` field, and/or re-derive from `dev/scratch/request-geo/raw/<ST>.json`).
3. Decide:
   - **Confident single-zone match** → add `"<NWSID>": "<filename>"` to `map`. Example: `WA_ZON_CAKCF` "F-hills & Valleys of cent King County Cascades" → NWS "Western Columbia River Gorge"? **No** — resolve against the actual WA zone list; the correct match is the King-County-Cascades foothills zone id. Verify the zone id exists in `nws-zones.geo.json` before adding.
   - **Genuinely multi-zone / no single NWS public zone** → add `"<filename>": "<reason>"` to `nws-zone-unmapped.json#unmapped`.
4. Never add a zone id that is not present in `nws-zones.geo.json` (a mapping to absent geometry resolves to null at runtime → silent omission, defeating the vetting).

- [ ] **Step 1: Resolve state-by-state, re-running the gate after each batch**

After each state's batch: `pnpm vitest run src/request/catalogMap.test.ts -t completeness` — the `missing` list shrinks. Commit per state or per few states:
```bash
git add src/request/nws-zone-to-catalog.json src/request/nws-zone-unmapped.json
git commit -m "feat(request): hand-resolve <ST> zone mappings (tuxlink-96lu)"
```

- [ ] **Step 2: Add a referential-integrity test (map ids exist in geometry)**

```ts
import zonesGeo from './nws-zones.geo.json';
it('every mapped NWS zone id exists in the bundled geometry', () => {
  const geoIds = new Set((zonesGeo as { features: { properties: { id: string } }[] }).features.map((f) => f.properties.id));
  const orphan = Object.keys((zoneMap as { map: Record<string,string> }).map).filter((id) => !geoIds.has(id));
  expect(orphan, `Mapped zone ids absent from geometry:\n${orphan.join('\n')}`).toEqual([]);
});
```
Run: `pnpm vitest run src/request/catalogMap.test.ts` → both tests PASS.

- [ ] **Step 3: Verify the full completeness gate is GREEN**

Run: `pnpm vitest run src/request/catalogMap.test.ts`
Expected: PASS, all zone forecasts accounted for.

- [ ] **Step 4: Final commit for the batch**

```bash
git add src/request/nws-zone-to-catalog.json src/request/nws-zone-unmapped.json src/request/catalogMap.test.ts
git commit -m "feat(request): complete vetted zone↔catalog mapping; completeness gate green (tuxlink-96lu)"
```

---

## Task 6: `gridToNwsZone` resolver

**Files:**
- Modify: `src/request/geo.ts`
- Modify: `src/request/geo.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
import { gridToLatLon, gridToNwsZone } from './geo';

describe('gridToNwsZone', () => {
  it('resolves Seattle (CN87uo) to its NWS public zone', () => {
    const { lat, lon } = gridToLatLon('CN87uo')!;
    const zone = gridToNwsZone(lat, lon);
    expect(zone?.id).toBe('WAZ558');
    expect(zone?.name).toBe('Seattle and Vicinity');
  });
  it('returns null for an ocean/non-US point', () => {
    expect(gridToNwsZone(0, -150)).toBeNull();
  });
});
```
(If the simplified geometry places `CN87uo`'s center in a neighbouring zone, assert the actually-correct zone id from `nws-zones.geo.json` — verify against the bundled data, not the mock.)

- [ ] **Step 2: Run — verify FAIL**

Run: `pnpm vitest run src/request/geo.test.ts -t gridToNwsZone`
Expected: FAIL with "gridToNwsZone is not a function".

- [ ] **Step 3: Implement `gridToNwsZone`**

Reuse the existing `pointInPolygonWithHoles` helper. Add to `geo.ts`:
```ts
import nwsZonesGeoJson from './nws-zones.geo.json';

export interface NwsZone { id: string; name: string; state: string; }

interface ZoneFeature {
  properties: { id: string; name: string; state: string };
  geometry:
    | { type: 'Polygon'; coordinates: number[][][] }
    | { type: 'MultiPolygon'; coordinates: number[][][][] };
}
const ZONE_FEATURES = (nwsZonesGeoJson as { features: ZoneFeature[] }).features;

/** Point-in-polygon over bundled NWS public-zone geometry. Returns the zone
 *  covering the point, or null for ocean / non-US / a gap in the bundled set.
 *  Same ray-casting technique as latLonToUsState. */
export function gridToNwsZone(lat: number, lon: number): NwsZone | null {
  for (const f of ZONE_FEATURES) {
    const polys = f.geometry.type === 'Polygon' ? [f.geometry.coordinates] : f.geometry.coordinates;
    for (const poly of polys) {
      if (pointInPolygonWithHoles(lon, lat, poly)) return f.properties;
    }
  }
  return null;
}
```

- [ ] **Step 4: Run — verify PASS**

Run: `pnpm vitest run src/request/geo.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/request/geo.ts src/request/geo.test.ts
git commit -m "feat(request): gridToNwsZone point-in-polygon resolver (tuxlink-96lu)"
```

---

## Task 7: Radar-region table + completeness gate

**Files:**
- Modify: `scripts/build-request-geo.ts` (scaffold the table)
- Create: `src/request/radar-regions.json`
- Modify: `src/request/catalogMap.test.ts` (radar completeness)

Radar regions (`WX_US_RAD`, 161 entries) have no public geometry API by these codes; bboxes are curated from the region name + known NWS local-radar sector extents. **Alpha vettedness applies** (memory `no_operator_decision_punts_on_polish`): cover all 161, enforced by a completeness test — don't ship a radar card that only resolves for Seattle.

- [ ] **Step 1: Scaffold all 161 region rows from the catalog**

Extend the script to emit `radar-regions.json` with one `{ filename, name, bbox: null }` row per `WX_US_RAD` catalog entry (name = catalog description with the `SNAPSHOT CURRENT RADAR U.S. ` prefix stripped). Preserve any already-curated bboxes on re-run (merge by filename — never clobber a filled bbox with null).

- [ ] **Step 2: Write the radar-completeness test (RED)**

```ts
import radar from './radar-regions.json';
describe('radar-region coverage', () => {
  it('every WX_US_RAD catalog filename has a curated bbox', () => {
    const radarFilenames = loadCatalog().filter((e) => e.category === 'WX_US_RAD').map((e) => e.filename);
    const byName = new Map((radar as { regions: { filename: string; bbox: number[] | null }[] }).regions.map((r) => [r.filename, r.bbox]));
    const missing = radarFilenames.filter((f) => { const b = byName.get(f); return !b || b.length !== 4; });
    expect(missing, `Radar regions missing a bbox:\n${missing.join('\n')}`).toEqual([]);
  });
});
```
Run: `pnpm vitest run src/request/catalogMap.test.ts -t radar` → FAIL listing all 161.

- [ ] **Step 3: Curate bboxes, batched by region cluster, until GREEN**

Fill `bbox: [west, south, east, north]` for each region from its name. Work in clusters (PNW, California, Gulf, Northeast, …). Sources: the region name's geography + state extents (the bundled `us-states.geo.json` gives per-state bbox as a sanity bound). Examples in the data contract above (PSND/NWWA/PNW). Where a region nests inside another (PSND ⊂ NWWA ⊂ PNW), the smaller bbox just needs to be tighter — the resolver (Task 8) picks smallest-area. Commit per cluster:
```bash
git add src/request/radar-regions.json
git commit -m "feat(request): curate radar bboxes for <cluster> (tuxlink-96lu)"
```

- [ ] **Step 4: Verify GREEN**

Run: `pnpm vitest run src/request/catalogMap.test.ts -t radar` → PASS (all 161 have valid 4-tuple bboxes).

- [ ] **Step 5: Commit**

```bash
git add scripts/build-request-geo.ts src/request/radar-regions.json src/request/catalogMap.test.ts
git commit -m "feat(request): complete radar-region bbox table + coverage gate (tuxlink-96lu)"
```

---

## Task 8: `gridToRadarRegion` resolver (smallest containing region)

**Files:**
- Modify: `src/request/geo.ts`
- Modify: `src/request/geo.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
import { gridToLatLon, gridToRadarRegion } from './geo';
describe('gridToRadarRegion', () => {
  it('resolves Seattle to the tightest region (Puget Sound, not PNW)', () => {
    const { lat, lon } = gridToLatLon('CN87uo')!;
    expect(gridToRadarRegion(lat, lon)?.filename).toBe('US.RAD.PSND');
  });
  it('returns null for a point in no curated region (mid-ocean)', () => {
    expect(gridToRadarRegion(0, -150)).toBeNull();
  });
});
```

- [ ] **Step 2: Run — verify FAIL**

Run: `pnpm vitest run src/request/geo.test.ts -t gridToRadarRegion` → FAIL.

- [ ] **Step 3: Implement**

```ts
import radarRegionsJson from './radar-regions.json';
export interface RadarRegion { filename: string; name: string; bbox: [number, number, number, number]; }
const RADAR_REGIONS = (radarRegionsJson as { regions: RadarRegion[] }).regions
  .filter((r) => Array.isArray(r.bbox) && r.bbox.length === 4);

/** Smallest-area radar region whose bbox contains the point; null if none. */
export function gridToRadarRegion(lat: number, lon: number): RadarRegion | null {
  let best: RadarRegion | null = null;
  let bestArea = Infinity;
  for (const r of RADAR_REGIONS) {
    const [w, s, e, n] = r.bbox;
    if (lon >= w && lon <= e && lat >= s && lat <= n) {
      const area = (e - w) * (n - s);
      if (area < bestArea) { bestArea = area; best = r; }
    }
  }
  return best;
}
```

- [ ] **Step 4: Run — verify PASS**

Run: `pnpm vitest run src/request/geo.test.ts` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src/request/geo.ts src/request/geo.test.ts
git commit -m "feat(request): gridToRadarRegion smallest-containing-region resolver (tuxlink-96lu)"
```

---

## Task 9: Catalog resolvers for zone + radar entries

**Files:**
- Modify: `src/request/catalogMap.ts`
- Modify: `src/request/catalogMap.test.ts`

- [ ] **Step 1: Write the failing tests**

```ts
import { zoneForecastEntry, radarEntry } from './catalogMap';
const ENTRIES = [
  { category: 'WX_US_WA', filename: 'WA_ZON_SEA', description: 'Seattle and Vicinity Zone Forecast', size_bytes: 2500 },
  { category: 'WX_US_RAD', filename: 'US.RAD.PSND', description: 'SNAPSHOT CURRENT RADAR U.S. PUGET SOUND & SJDF', size_bytes: 20799 },
];
it('zoneForecastEntry maps an NWS zone id to its catalog entry', () => {
  expect(zoneForecastEntry(ENTRIES, 'WAZ558')?.filename).toBe('WA_ZON_SEA');
});
it('zoneForecastEntry returns null for an unmapped zone', () => {
  expect(zoneForecastEntry(ENTRIES, 'WAZ999')).toBeNull();
});
it('radarEntry returns the catalog entry for a region filename', () => {
  expect(radarEntry(ENTRIES, 'US.RAD.PSND')?.filename).toBe('US.RAD.PSND');
});
```

- [ ] **Step 2: Run — verify FAIL**

Run: `pnpm vitest run src/request/catalogMap.test.ts -t zoneForecastEntry` → FAIL.

- [ ] **Step 3: Implement**

```ts
import zoneMap from './nws-zone-to-catalog.json';
const ZONE_MAP = (zoneMap as { map: Record<string, string> }).map;

/** Resolve an NWS zone id → the backing catalog entry (via the vetted map),
 *  or null if the zone is unmapped or its filename isn't in the loaded catalog. */
export function zoneForecastEntry(entries: CatalogEntry[], zoneId: string): CatalogEntry | null {
  const filename = ZONE_MAP[zoneId];
  if (!filename) return null;
  return entries.find((e) => e.filename === filename) ?? null;
}

/** Resolve a radar region filename → its catalog entry, or null if absent. */
export function radarEntry(entries: CatalogEntry[], filename: string): CatalogEntry | null {
  return entries.find((e) => e.category === 'WX_US_RAD' && e.filename === filename) ?? null;
}
```

- [ ] **Step 4: Run — verify PASS**

Run: `pnpm vitest run src/request/catalogMap.test.ts` → PASS (all, including completeness gates).

- [ ] **Step 5: Commit**

```bash
git add src/request/catalogMap.ts src/request/catalogMap.test.ts
git commit -m "feat(request): zone + radar catalog-entry resolvers (tuxlink-96lu)"
```

---

## Task 10: Rewrite the location section in `buildSections`

**Files:**
- Modify: `src/request/sections.ts`
- Modify: `src/request/sections.test.ts`

The location section becomes zone-primary + radar + marine, adaptive. Add an optional `meta` field to `RequestCard` (the mono zone-id/filename line, DoD #4) and a `primary` flag for the zone card.

- [ ] **Step 1: Write the failing tests**

```ts
import { buildSections } from './sections';
const CAT = [
  { category: 'WX_US_WA', filename: 'WA_ZON_SEA', description: 'Seattle and Vicinity Zone Forecast', size_bytes: 2500 },
  { category: 'WX_US_RAD', filename: 'US.RAD.PSND', description: 'SNAPSHOT CURRENT RADAR U.S. PUGET SOUND & SJDF', size_bytes: 20799 },
  { category: 'PROPAGATION', filename: 'PROP_3DAY', description: '3-day', size_bytes: 1 },
];
it('coastal grid → location section has zone (primary) + radar + marine', () => {
  const loc = buildSections(CAT, 'CN87uo').find((s) => s.kind === 'location')!;
  expect(loc.cards.map((c) => c.id)).toEqual(['loc-zone-forecast', 'loc-radar', 'loc-marine']);
  expect(loc.cards[0].primary).toBe(true);
  expect(loc.cards[0].action).toEqual({ kind: 'addCms', filename: 'WA_ZON_SEA' });
  expect(loc.cards[0].meta).toContain('WAZ558');
});
it('inland grid (Denver DM79) → zone + radar, no marine', () => {
  // requires CO zone + a CO radar region present in the bundled assets + CAT
  const loc = buildSections(CAT_CO, 'DM79').find((s) => s.kind === 'location');
  expect(loc?.cards.some((c) => c.id === 'loc-marine')).toBe(false);
});
it('non-US grid → no location section', () => {
  expect(buildSections(CAT, 'IO91').find((s) => s.kind === 'location')).toBeUndefined();
});
```

- [ ] **Step 2: Run — verify FAIL**

Run: `pnpm vitest run src/request/sections.test.ts` → FAIL.

- [ ] **Step 3: Extend the card type + rewrite the section**

In `sections.ts` add to `RequestCard`: `meta?: string;` and `primary?: boolean;`. Replace the Weather block (`sections.ts:61-90`) with:
```ts
import { gridToLatLon, latLonToSeaArea, gridToNwsZone, gridToRadarRegion } from './geo';
import { NATIONAL, zoneForecastEntry, radarEntry } from './catalogMap';

// --- For your location (geo-derived) ---------------------------------------
const locationCards: RequestCard[] = [];
const latLon = grid ? gridToLatLon(grid) : null;
if (latLon) {
  const zone = gridToNwsZone(latLon.lat, latLon.lon);
  if (zone) {
    const entry = zoneForecastEntry(entries, zone.id);
    if (entry) {
      locationCards.push({
        id: 'loc-zone-forecast',
        label: zone.name,
        description: 'Your NWS public forecast zone — the local text forecast for your grid. Returns text.',
        meta: `${zone.id} · ${entry.filename}`,
        primary: true,
        action: { kind: 'addCms', filename: entry.filename },
      });
    }
  }

  const radar = gridToRadarRegion(latLon.lat, latLon.lon);
  if (radar) {
    const entry = radarEntry(entries, radar.filename);
    if (entry) {
      locationCards.push({
        id: 'loc-radar',
        label: 'Regional radar',
        description: 'Current precipitation radar snapshot for your area. Returns an image.',
        meta: `${radar.name} · ${radar.filename}`,
        action: { kind: 'addCms', filename: entry.filename },
      });
    }
  }

  const seaArea = latLonToSeaArea(latLon.lat, latLon.lon);
  if (seaArea) {
    locationCards.push({
      id: 'loc-marine',
      label: 'Marine forecast',
      description: 'Wind, wave and sea-state forecasts for your offshore sea area. Returns text.',
      meta: seaArea,
      action: { kind: 'openBrowse', category: seaArea },
    });
  }
}
if (locationCards.length > 0) {
  sections.push({ id: 'weather', title: 'For your location', kind: 'location', cards: locationCards });
}
```
Keep the national `propagation` + `nearby` sections unchanged.

- [ ] **Step 4: Run — verify PASS**

Run: `pnpm vitest run src/request/sections.test.ts` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src/request/sections.ts src/request/sections.test.ts
git commit -m "feat(request): adaptive zone/radar/marine location section (tuxlink-96lu)"
```

---

## Task 11: UI — primary zone card + supporting grid + CSS + harness catalog

**Files:**
- Modify: `src/request/RequestCenter.tsx`
- Modify: `src/request/RequestCenter.css`
- Modify: `src/request/RequestCenter.test.tsx`
- Modify: `dev/render-harness/harness.tsx`

- [ ] **Step 1: Write the failing structural test**

```ts
// in RequestCenter.test.tsx — render with a WA coastal grid + zone/radar catalog
it('renders the zone forecast as the primary hero card with its meta line', async () => {
  // ... mount RequestCenter with grid CN87uo + catalog incl. WA_ZON_SEA, US.RAD.PSND
  expect(await screen.findByTestId('request-card-loc-zone-forecast')).toHaveClass('zone');
  expect(screen.getByText('WAZ558 · WA_ZON_SEA')).toBeInTheDocument();
  expect(screen.getByTestId('request-card-loc-radar')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run — verify FAIL**

Run: `pnpm vitest run src/request/RequestCenter.test.tsx -t "primary hero card"` → FAIL.

- [ ] **Step 3: Render the primary zone card + supporting grid**

In the `kind === 'location'` block (`RequestCenter.tsx:305-373`), split cards: the `card.primary` card renders as the `.zone` markup (icon tile `.zi` + `.zn` name + `.zd` description + `.zmeta` mono line + `.za` Add control); the rest render in a `.locgrid` as `.feat` cards extended with a `.fmeta` mono line (`{card.meta}`). Match the mock's class names exactly (`zone/zi/zn/zd/zmeta/za`, `locgrid/feat/fi/fn/fd/fmeta/fa`). Render `card.meta` in both card types (the current feat card omits it — that's the DoD #4 gap).

- [ ] **Step 4: Add the CSS from the mock**

Port `.zone`, `.zi`, `.zn`, `.zd`, `.zmeta`, `.za`, `.locgrid`, `.feat .fmeta` rules from the mock (`docs/design/mockups/2026-06-10-request-center-location-hero.html` lines 53-68) into `RequestCenter.css`, using the existing CSS variables. Keep the amber-edged hero treatment.

- [ ] **Step 5: Update the harness canned catalog**

In `harness.tsx`, replace the placeholder `WA_ZONE`/`WA_FCST` entries with real resolvable data so the hero renders in WebKitGTK: add `{ category: 'WX_US_WA', filename: 'WA_ZON_SEA', description: 'Seattle and Vicinity Zone Forecast', size_bytes: 2500 }` and `{ category: 'WX_US_RAD', filename: 'US.RAD.PSND', description: 'SNAPSHOT CURRENT RADAR U.S. PUGET SOUND & SJDF', size_bytes: 20799 }` plus the existing EASTPAC marine entries.

- [ ] **Step 6: Run tests + typecheck**

Run: `pnpm vitest run src/request/RequestCenter.test.tsx && pnpm tsc --noEmit` → PASS.

- [ ] **Step 7: Commit**

```bash
git add src/request/RequestCenter.tsx src/request/RequestCenter.css src/request/RequestCenter.test.tsx dev/render-harness/harness.tsx
git commit -m "feat(request): primary zone card + supporting radar/marine grid UI (tuxlink-96lu)"
```

---

## Task 12: CRITICAL — WebKitGTK render gate (1920×1080, CN87uo)

**Files:** none (verification only). **Do not skip — cost two regressions last session.**

- [ ] **Step 1: Start the worktree dev server**

Run (background): `pnpm dev` in the worktree. Confirm Vite is on `:1420`. (Memory `worktree_dev_port_collision`: only one `tauri dev`/Vite binds :1420 machine-wide — ensure no other worktree's server is running, or this renders the wrong build.)

- [ ] **Step 2: Render the coastal case at full resolution**

Run:
```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
  "http://localhost:1420/dev/render-harness/harness.html?grid=CN87uo&view=home" \
  dev/scratch/location-hero-mock/build-coastal-1920.png 1920 1080 2500
```
Then **Read** the PNG. Verify against the mock: amber-edged hero, "For your location · CN87uo · Washington", the **Seattle and Vicinity** primary zone card with `WAZ558 · WA_ZON_SEA` mono line + Add control, the radar card (`Puget Sound & SJDF · US.RAD.PSND`), the marine card, then the national chips. No clipped/overflowing/mis-centered elements (the failure class from PR #559/#564).

- [ ] **Step 3: Render an inland case (no marine)**

Repeat with `?grid=DM79` (Denver) → output `build-inland-1920.png`, Read it, confirm zone + radar but **no** marine card, and the layout still holds with two supporting cards.

- [ ] **Step 4: If any defect appears, fix in source and re-render**

Iterate Task 11 ↔ Task 12 until both renders match the mock. **Restart `pnpm dev` after source changes** (memory `chromium_not_webkitgtk_proxy`: Ctrl+R is a no-op for some changes). Commit any fixes:
```bash
git add -A && git commit -m "fix(request): WebKitGTK render-gate corrections for location hero (tuxlink-96lu)"
```

- [ ] **Step 5: Keep the PNGs as evidence**

Leave `build-coastal-1920.png` + `build-inland-1920.png` in `dev/scratch/` (gitignored) so the operator can open them in VS Code without a 30-min compile.

---

## Task 13: App-level mount test + full-suite green + finalize

**Files:**
- Modify: `src/request/RequestCenter.app.test.tsx`

- [ ] **Step 1: Add an App-level test that exercises the production mount path**

Per memory `test_production_mount_path_not_just_units`: assert the hero renders end-to-end when `RequestCenter` is mounted as production mounts it (config_read → grid → catalog_list → buildSections → hero), not just with hand-injected props.
```ts
it('renders the resolved location hero from config + catalog (production path)', async () => {
  // mock config_read → { grid: 'CN87uo' }, catalog_list → [WA_ZON_SEA, US.RAD.PSND, EASTPAC...]
  // mount <RequestCenter/> with no section props; await the hero
  expect(await screen.findByTestId('request-card-loc-zone-forecast')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the full request suite (DoD #6)**

Run: `pnpm vitest run src/request`
Expected: PASS — including the pre-existing tests (the location-section rename from "Weather" to "For your location" may need updates in `RequestCenter.test.tsx`/`sections.test.ts`; fix any that assert the old labels).

- [ ] **Step 3: CI-parity gate before push**

Run: `pnpm exec eslint . && pnpm vitest run`
Expected: both clean (memory `scoped_vitest_misses_contract_tests`: scoped vitest can pass while a far-away contract test or eslint idiom-lint fails CI).

- [ ] **Step 4: Commit + push**

```bash
git add src/request/RequestCenter.app.test.tsx src/request/*.test.tsx
git commit -m "test(request): app-level mount renders resolved location hero (tuxlink-96lu)"
git push -u origin bd-tuxlink-loc/location-hero
```

- [ ] **Step 5: Open the PR**

```bash
gh pr create --title "[gully-cardinal-gulch] feat(request): location-aware 'For your location' hero (tuxlink-96lu)" \
  --body "<summary + DoD checklist + render-gate PNGs noted as local; per propagation contract, summarise any pitfalls>"
```
Mark ready (not draft — memory `no_draft_pr_parking`).

---

## Self-Review (against the spec)

**Spec coverage:**
- DoD #1 (exact NWS zone primary card) → Tasks 6, 9, 10, 11.
- DoD #2 (tightest radar) → Tasks 7, 8, 10.
- DoD #3 (marine only when coastal) → Task 10 (reuses `latLonToSeaArea`).
- DoD #4 (each card states CMS return + target + filename) → Task 10 (`meta` field) + Task 11 (renders meta).
- DoD #5 (vetted mapping, completeness test) → Tasks 3, 4, 5 (the completeness gate).
- DoD #6 (reachable in real build, suite green) → Tasks 12 (render gate), 13 (app-level + full suite).
- Scope OUT (METAR, buoy/NAVTEX/offshore/sat/fax, non-US, travel) → none added; marine stays sea-area only; non-US grid → no section (Task 10 test).
- Geo architecture (bundle geometry, point-in-polygon, filename map, radar table, generation script) → Tasks 1, 2, 3, 7.
- Data provenance (committed script records dataset version) → Tasks 1 (`_source`/effectiveDate), 3.
- Browse reveal scales to in-state zone count ("Browse all WA local forecasts · 68 zones") → **covered in Task 11** (the reveal label already exists in the shipped UI; update it to use the in-state zone count if not already dynamic — verify during Task 11 Step 3 and extend if static).

**Type consistency:** `NwsZone {id,name,state}`, `RadarRegion {filename,name,bbox}`, `RequestCard.meta?`/`.primary?`, `zoneForecastEntry`/`radarEntry`/`gridToNwsZone`/`gridToRadarRegion` — names consistent across Tasks 6–11.

**Placeholder scan:** the heavy data tasks (5, 7) are inherently batched manual vetting; their *gate* (completeness test) is concrete and the *procedure* is shown with worked examples (WA_ZON_BLUF auto, WA_ZON_CAKCF hand, WA_FOR_EAST unmapped). No "TODO/handle edge cases" left in code steps.
