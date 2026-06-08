# Map-pin GPS grid (+ GRIB region) — design

> Status: **locked** (operator brainstorm 2026-06-07, agent `basalt-mesa-dahlia`, visual-companion session).
> Smoke-walk items 18 (`tuxlink-urbv`, set grid by map pin) + 21 (`tuxlink-mxmx`, GRIB region map). Brainstorm #3 of 4.

## Grounding (verified, corrects the audit)

The audit cited a `PositionMapWidget` + `src/forms/position/maidenhead.ts` as reusable prior art. **Neither exists.** There is **no map library** (`package.json` has no Leaflet/MapLibre/Mapbox), **no map widget**, and **no lat/lon↔Maidenhead converter**. Grid is set today via the `GridEdit.tsx` text input. The app CSP is `img-src 'self' data:` — **external tiles (OSM) are already forbidden**, which aligns with the offline/no-ban posture. This feature is **greenfield**.

## Operator principles (load-bearing)

- **Self-contained by default.** A bundled offline map is the **required fallback** (small files), always available with no network.
- **Never public OSM as an unwitting backend.** Public OSM is not in the source chain at all.
- **Linkable to permitted sources.** The operator may **explicitly opt in** to a permitted network tile server (e.g. Geographica / self-hosted).
- **Toggleable** Maidenhead grid overlay.
- **Precision posture preserved** (`feedback_gps_precision_reduction`): the pin resolves a precise grid, but the stored/broadcast value defaults to **4-char Maidenhead**; finer is opt-in.

## Components

### 1. `maidenhead` converter (greenfield, Rust + TS mirror)
- Bidirectional lat/lon ↔ Maidenhead at variable precision (2/4/6/8 char). Standard algorithm.
- Reused **everywhere** grids appear (GridEdit, ribbon, the picker, Favorites distance). One source of truth.
- Edge cases tested: poles, antimeridian, precision rounding.

### 2. `GridMapPicker` (React component)
- Renders the **active map source** (bundled image by default) with a **toggleable Maidenhead grid overlay** (SVG/canvas lines + field/square labels).
- **Click → pixel → lat/lon → grid**; the readout updates live (`CN85` → `CN85nq` as you zoom/refine).
- **Zoom:** swap to a coarser/finer bundled regional asset (or scale), refining achievable precision.
- **Two interactions, one map:**
  - **Pin** (item 18): single point → grid. The default mode.
  - **GRIB box** (item 21): a toggle that switches to drag-a-rectangle → bounding box `(lat0,lon0,lat1,lon1)` for the GRIB weather request.
- **"Use"** stores the grid honoring the 4-char broadcast default (finer opt-in).

### 3. Map-source chain + gatekeeper (Rust backend)
- **Bundled (offline)** — DEFAULT + required fallback. A small set of bundled map images: a low-zoom world map + a few coarse regional zooms. Bundled = CSP `'self'`, so they load with no CSP change. Size kept modest (operator: "not overly-large files") — target a few hundred KB to low-MB total.
- **Tile server (opt-in)** — the operator may configure a permitted tile-server URL (Geographica/self-hosted). **The Rust backend is the tile gatekeeper:** the webview never fetches tiles directly (CSP stays `'self'`); the backend fetches from the *configured, permitted* host and serves them to the picker. This (a) keeps the CSP locked, (b) makes "permitted source only, never silent OSM" enforceable in one place, (c) avoids the webview-hits-public-OSM failure mode entirely.
- **Public OSM** is **not** an option in the chain.

## Access points
The same `GridMapPicker` opens from:
- **Settings → Location** (set/replace the station grid).
- **First-run wizard** (Step 2 grid entry) — gives new operators who can't derive a grid a way in.
- **GRIB request dialog** (region selection, box mode).

## Data flow
```
click/drag ─► pixel ─► lat/lon (projection of active map extent) ─► maidenhead.toGrid(precision)
   │                                                                      └─► readout + "Use" → config grid
   └─(box mode)─► two corners ─► (lat0,lon0,lat1,lon1) ─► GRIB request
map source: bundled (default, 'self')  | backend tile-gatekeeper → permitted server (opt-in)
```

## Error handling
- Missing/failed configured tile server → automatic fall back to **bundled** with a non-blocking notice ("tile server unreachable — using offline map").
- The picker is never blocking: manual grid entry (`GridEdit`) remains available alongside it.

## Testing
- **Rust:** maidenhead round-trips + precision + edge cases; tile-gatekeeper (bundled default; configured-server fetch+serve; **rejects/never-fetches public OSM**; fallback-to-bundled on failure).
- **Frontend:** `GridMapPicker` — click→grid readout, overlay toggle on/off, zoom refines precision, pin vs GRIB-box modes, "Use" honors 4-char default; integration mounts in Settings/wizard/GRIB.

## Out of scope (v1)
- A full slippy/MBTiles offline tile pipeline (that's item 11b's station-map viewer — separate).
- Reverse-geocoding / place search (pin + grid only).
- Bundling high-detail regional maps (coarse is enough for grid-picking).

## Open items for the implementation plan
- Exact bundled asset set + projection (equirectangular world + which regional extents) + total size budget.
- Tile-server config surface (URL field in Settings) + the backend fetch/serve protocol + cache.
- Overlay rendering tech (SVG vs canvas) at the chosen zoom levels.
