# Bug Hunt Report — Offline Vector Map Performance RCA (holistic)

**Agent:** raven-poplar-clover
**Date:** 2026-06-15
**Method:** code-bug-hunter-holistic — read all primary files, then reason about where CPU
time and main-thread blocking go when the map is open + panned on a software rasterizer (Pi 5,
llvmpipe, WebKitGTK, DMA-BUF off, all WebGL CPU-rasterized).

## Scope

Frontend `src/map/` (MapLibreMap, mapHooks, basemapStyle, darkStyle, tuxlinkFlavor,
useBasemapFlavor, MaidenheadGridLayer, GridPicker, offlineMaps, useDownloadProgress, projection)
+ consumers (StationFinderMap, LocationMap, PositionMapWidget) + backend tile seam
(`src-tauri/src/basemap/{mod,commands}.rs`, `tiles/{serve,fetch,cache}.rs`, lib.rs custom-scheme
handler) + design doc (the 45 fps forecast's provenance).

**Framing the symptom.** The 45 fps forecast came from the ndi4 R4 spike: front-end only, mocked
Tauri data, a trivial scene with NO real PMTiles decode, NO markers, NO pack compositing
(confirmed: design doc AMENDMENT block — "baked-GL-dark = 45 fps / 21 ms = the light baseline").
The real app adds, on the same CPU-bound paint budget: full `@protomaps/basemaps` layer + label
sets, per-frame overlay recomputation in every consumer, a per-pmtiles-fetch IPC round-trip with
no tile-level caching, and a redundant second full `setStyle`. The findings below are ranked by
expected contribution to the gap between 45 fps and "sluggish."

NOTE on `tiles/{serve,fetch,cache}.rs`: this is the **retired LAN-raster** path
(`tile://localhost/{z}/{x}/{y}`), not the vector basemap. `tileSource` was removed (A5) and no
map consumer mounts a raster source, so this code is **dormant** in the shipped vector map and
contributes ~0 to the symptom. Reviewed for correctness; nothing live found. The hot vector path
is `basemap/mod.rs::read_range` via the lib.rs `tile://pmtiles/...` branch.

---

## Bugs / performance defects

### 1. Redundant second full `setStyle` after every mount (the packs round-trip)
**Location:** `src/map/MapLibreMap.tsx:110-124` (fetchPacks effect) + `:219-228` (style rebuild effect)
**Severity:** significant
**Evidence:** The map is constructed with `style: buildBasemapStyle(flavorRef.current)` — overview
only, no packs (`:132`). Immediately on mount, the `fetchPacks` effect (`:119`) invokes
`basemap_list_packs` and `setPacks(...)`. Even when **zero packs** are installed, the backend
returns `{ packs: [] }`, but `setPacks([])` replaces the initial `[]` state with a **new array
identity**, re-running the style-rebuild effect (`:220`, dep `packs`). The guard there compares a
string key: construct-time seeds `styleKeyRef = "${flavor}|"` (`:223`) and the new key is
`"${flavor}|"` (empty pack list) — so for the **zero-pack case the guard correctly suppresses**
the second setStyle. BUT the moment **any** pack is installed, or whenever the
`BASEMAP_PACKS_CHANGED_EVENT` fires, `setStyle(buildBasemapStyle(flavor, packs))` runs a **full
style teardown + rebuild**.
**Impact:** `setStyle` (without `{ diff: true }`, which MapLibre cannot meaningfully diff across a
fully-regenerated layer array) drops every source + layer and re-parses + re-lays-out the entire
style. On a software rasterizer this is a multi-hundred-ms main-thread stall, and it fires a
`styledata` storm that every overlay hook (`useMapOverlay`/`usePushData` in all consumers) reacts
to — re-`addSource`/`addLayer` + re-`setData` for every overlay (finding 4). The comment at
`:108` calls the second setStyle by design, but compositing packs by *regenerating the whole
style* (rather than `addSource`+`addLayer` for just the pack) is the expensive way to do it.
**Fix direction:** Composite an installed pack by adding only its source + clamped layers via the
existing owned-hook primitives (or `map.addSource`/`addLayer`) instead of `setStyle`-ing the whole
world. Reserve `setStyle` for the flavor (light↔dark) swap, which genuinely changes every layer's
paint. At minimum, skip the rebuild effect entirely when `packs` is empty AND unchanged (compare
sorted ids, which the key already does — but avoid the `setPacks([])` identity churn by early-
returning in `fetchPacks` when the list is empty and state is already empty).

### 2. Full style is rebuilt from scratch (re-running `@protomaps/basemaps` `layers()`) on every flavor/pack change, and the dark bake walks every color each time
**Location:** `src/map/basemapStyle.ts:111-163` (`buildBasemapStyle`) + `darkStyle.ts:98-111` (`bakeDarkColors`) + `MapLibreMap.tsx:132,227`
**Severity:** significant
**Evidence:** `buildBasemapStyle` calls `layers(BASEMAP_SOURCE_ID, tuxlinkFlavor(), {lang:'en'})`
(`:127`) — the full Protomaps basemap layer generator (dozens of layers) — on **every** call: once
at construction (`MapLibreMap.tsx:132`) and again on every `setStyle` (`:227`). For dark mode each
call additionally runs `bakeDarkColors` over **every layer's every `*-color` paint value**,
including recursing into data-driven expression arrays (`darkStyle.ts:80-89`, `transformColorValue`
maps over arrays). Per pack, it does this **again** for the pack's full layer set (`basemapStyle.ts:146`).
**Impact:** Style *construction* cost is not per-frame, but it lands as a synchronous main-thread
spike at exactly the worst moments (mount, theme toggle, pack install). Combined with finding 1,
every pack change pays: regenerate world layers + bake them + regenerate+bake N pack layer sets +
full `setStyle` parse/layout. The per-pack layer multiplication also **inflates the live layer
count** the rasterizer must paint every frame (finding 6).
**Fix direction:** Memoize the baked world style (it only depends on `flavor`). Build pack layers
once per pack and cache by id. Decouple "add a pack" from "rebuild the style."

### 3. PMTiles served per-Range with NO server-side or HTTP caching → every tile decode is an IPC round-trip + blocking file read on the worker pool, repeatedly for the same bytes
**Location:** `src-tauri/src/lib.rs:262` (`spawn_blocking` per request) + `basemap/mod.rs:81-92` (`read_at`, fresh `vec![0u8; n]` per read) + lib.rs response headers (`:264-303`)
**Severity:** significant
**Evidence:** Each `tile://pmtiles/<id>` Range request spawns a `spawn_blocking` task, clones the
`Arc<PmtilesArchive>`, allocates a fresh `Vec` for the body, `pread`s, and responds. The response
sets `Accept-Ranges`, `ETag`, `Content-Range` but **no `Cache-Control`**. The `pmtiles` JS client
caches directory entries in-memory per `PMTiles` instance, but: (a) MapLibre re-requests tile
ranges as you pan back over previously-seen tiles, and with no HTTP cache + a custom scheme, the
webview cannot serve a 304/memory-cache hit for the raw byte ranges — every revisit re-crosses the
IPC boundary and re-reads from disk; (b) `maxTileCacheSize` is left at the MapLibre default, but the
decoded-tile cache doesn't cover the pmtiles byte-range fetches themselves. On llvmpipe the decode
(gzip-decompress of each vector tile happens on the **main thread** in the pmtiles/MapLibre worker,
see finding 5) dominates, and re-fetching forces re-decode.
**Impact:** Panning is the worst case — a viewport refill re-requests a ring of tiles, each a
fresh IPC + blocking read + JS-side gunzip + geometry tessellation. There is no layer between
MapLibre and disk that says "you already have these bytes." Magnitude: tens of round-trips per pan
gesture, each with allocation + syscall + cross-thread hop overhead on top of the unavoidable decode.
**Fix direction:** Add `Cache-Control: max-age=...` (the archive is immutable for a session; the
ETag already keys it) so the webview's network cache short-circuits repeat range reads. Consider a
small LRU of recently-served `(id, start, len)` byte ranges in the Rust seam to skip the `pread`
+ alloc on repeats. Confirm MapLibre's `maxTileCacheSize` is large enough that pan-back does not
evict + re-decode.

### 4. Every overlay consumer re-creates its GeoJSON FeatureCollection and calls `setData` on a churning dependency, and re-subscribes `styledata` on each data change
**Location:** `StationFinderMap.tsx:110-127,142-145` ; `LocationMap.tsx:116-135` ; `PositionMapWidget.tsx:92-106` ; `MaidenheadGridLayer.tsx:117-140` ; `GridPicker.tsx:216-233`
**Severity:** significant (StationFinderMap), minor-to-significant (others)
**Evidence (StationFinderMap):** `fc = useMemo(buildStationFC(stations, tiers, selectedKey), [stations, tiers, selectedKey])`
(`:142`). `tiers` is a `Map` passed in as a prop; the producer (`useReachabilityMap`) very likely
rebuilds this Map on each reachability recompute, so its **identity changes** even when contents are
stable, busting the memo and rebuilding the whole FeatureCollection (iterating every station,
`gridToLatLon` per station). Then `usePushData` (`:115-126`) has `data` in its dep array, so on each
new `fc` it **tears down and re-adds the `styledata` listener** and calls `src.setData(fc)`. `setData`
replaces the entire source → MapLibre re-parses + re-tessellates all features.
**Evidence (MaidenheadGridLayer):** `effBounds` is recomputed by calling `map.getBounds()` **during
render** (`:107-114`) producing a fresh object every render; the geojson memo (`:117`) keys on the
bound primitives so it's stable-ish, but a `setTick` bump on every `moveend` (`:98-105`) forces a
full re-render + `gridLines` recompute + `gridToGeoJSON` + `setData` **on every pan**. On a software
rasterizer the grid's line+label layers (with `text-allow-overlap:true`, `text-ignore-placement:true`
— `:54-55`) are re-tessellated and repainted each moveend.
**Impact:** Combined, a single pan triggers, per mounted overlay: a React re-render, a FC rebuild, a
`setData` (full source replace + re-tessellation), and listener churn. With StationFinderMap's pin
layer this is O(stations) every reachability tick AND on every selection change (a click rebuilds
the entire FC just to flip one `selected` flag). The Maidenhead grid re-tessellates lines + labels
every moveend.
**Fix direction:** (a) Make `tiers`/`stations` identity stable upstream, or build the FC from a
stable key. (b) Drop `data` from `usePushData`'s dep array — subscribe `styledata` once and call
`setData` via a ref to the latest FC. (c) For selection, use `setFeatureState` + a `feature-state`
driven paint expression instead of rebuilding the whole FC to flip `selected`. (d) For the grid,
debounce/coarsen the moveend recompute and reconsider `text-allow-overlap`/`text-ignore-placement`
(forcing every label to render is costly on llvmpipe).

### 5. PMTiles per-tile gzip decompression runs in JS (no native decode), on the CPU-bound substrate
**Location:** `basemap/mod.rs:38-39` (serves raw octet-stream, "decoded by the JS client") + `basemapStyle.ts:11-17` (pmtiles JS Protocol)
**Severity:** significant (inherent cost, but unmitigated)
**Evidence:** By design the Rust seam serves **raw** PMTiles bytes ("zero content decoding here:
PMTiles internal compression is decoded by the JS client" — mod.rs:38, lib.rs:219). Each vector
tile inside a PMTiles archive is individually gzip-compressed; the `pmtiles` JS lib gunzips each
tile in the webview before handing the MVT to MapLibre, which then parses + tessellates it. On a
device with a working GPU this is fine; here MapLibre's worker rendering is itself CPU-bound, and the
gunzip + MVT parse + tessellation all compete for the same cores as the rasterizer.
**Impact:** Every newly-visible tile pays gunzip + parse + tessellation in JS before it can paint.
At maxZoom=14 with overzoom (finding 7) the tessellated geometry per painted tile is large. This is
the single biggest *unavoidable-by-the-current-architecture* per-tile cost and the main reason the
trivial-scene 45 fps does not survive real tiles.
**Fix direction:** This is partly inherent, but levers exist: (a) the cache levers in finding 3
prevent re-decode; (b) reducing the painted layer/label count (finding 6) reduces tessellation work;
(c) ensure MapLibre is using its web worker for tile decode (default) so gunzip is at least off the
**UI** thread — verify the WebKitGTK build actually spawns the worker (some packaged WebKitGTK
configs fall back to main-thread); if it cannot, decoding pmtiles tiles in Rust (decompress + maybe
pre-tessellate) becomes worth it despite the "zero decoding" design note.

### 6. Full Protomaps layer + label set is painted every frame; pack compositing multiplies it
**Location:** `basemapStyle.ts:127` (world layers) + `:139-153` (per-pack layer append) + `MaidenheadGridLayer.tsx:45-58` (always-overlap labels)
**Severity:** significant
**Evidence:** `layers(...)` emits the complete Protomaps basemap layer list (roads, casings, land
use, water, buildings, boundaries, plus many label/symbol layers). Every installed pack appends a
**second full copy** of that layer set (minus background), clamped to z≥6 (`:146-152`). A software
rasterizer's per-frame budget is dominated by (layer count × features painted × overdraw). Label
layers are the most expensive (glyph layout + collision detection); the Maidenhead grid forces
`text-allow-overlap`/`text-ignore-placement` true, disabling MapLibre's collision-skip optimization.
**Impact:** With one pack installed the live style paints ~2× the layer set inside the pack's bbox.
Label-heavy styles on llvmpipe are exactly the workload that turns 45 fps (geometry-light trivial
scene) into single-digit fps. Casings (each road drawn twice) double road overdraw.
**Fix direction:** Curate a tuxlink-specific reduced layer set for the EmComm use case (drop POI
labels, minor landuse, building detail at low z) rather than the full Protomaps set. Cap label
density. Avoid painting the world overview layers where a pack fully covers the viewport (the design
chose overlap-not-disjoint to avoid blanking — but at z≥pack-min you're painting both the overview
AND the pack in the same viewport; consider clamping the overview's `maxzoom` where a pack exists).

### 7. maxZoom=14 with overview overzoomed from z6 → heavy overzoom tessellation outside pack coverage
**Location:** `MapLibreMap.tsx:43` (`MAP_MAX_ZOOM=14`) + `basemapStyle.ts:104-109,126` (overview left unclamped, overzoomed past z6)
**Severity:** minor-to-significant
**Evidence:** The bundled world overview is z0–6 but the map allows zoom to 14. Outside any
installed pack, MapLibre overzooms the z6 overview tile up to 8 zoom levels. Overzoom re-renders
the same coarse geometry at progressively larger scale — the geometry isn't more detailed but the
tiles are repainted/re-scaled and the z6 tile's features are tessellated for a much larger screen
extent.
**Impact:** Zooming in over un-downloaded regions keeps the rasterizer busy painting stretched
coarse geometry with no visual benefit. Less severe than the label cost but a steady drain when the
operator zooms past z6 without a pack.
**Fix direction:** Consider clamping the overview source `maxzoom` to ~7–8 so MapLibre stops
requesting/overzooming it aggressively, and/or lowering `MAP_MAX_ZOOM` where no pack covers the view.

### 8. `useBasemapFlavor` MutationObserver watches `style` attribute mutations on `<html>` → fires on unrelated style changes
**Location:** `src/map/useBasemapFlavor.ts:32-41`
**Severity:** minor
**Evidence:** The observer subscribes to `attributeFilter: ['data-theme', 'style']` on
`document.documentElement`. Any code that writes an inline style on `<html>` (scroll-lock, a CSS
custom property update, a vh fix, etc.) fires `update()` → `setFlavor(resolveBasemapFlavor())`.
`resolveBasemapFlavor` returns the same value, so React bails on the state set — but the observer
callback + resolve run on each such mutation.
**Impact:** Small, but it can couple unrelated `<html>` style writes to a flavor recompute; if the
returned flavor object/string ever differed it would trigger the **full setStyle** path (finding 1/2).
Low magnitude; flagged for the interaction risk.
**Fix direction:** Narrow to `['data-theme']` plus a dedicated `color-scheme` data attribute, or
diff the resolved value inside the callback before any further work.

### 9. `MaidenheadGridLayer` calls `map.getBounds()` / `map.getZoom()` during render
**Location:** `src/map/MaidenheadGridLayer.tsx:107-115`
**Severity:** minor
**Evidence:** `effBounds` and `effLevel` are computed by calling live map methods inside the render
body (not in an effect/memo). React may render a component multiple times per commit; each render
calls into MapLibre's transform. Coupled with the `setTick` moveend bump (`:98-105`), every pan
forces a render that reads the transform and recomputes the lattice.
**Impact:** Minor CPU, but it makes the grid recompute correlate 1:1 with pan frames. Combined with
finding 4 this is part of the per-pan overlay tax.
**Fix direction:** Read bounds/zoom inside the `moveend` handler and store in state, rather than
during render.

---

## Design Concerns (risk patterns, not line-bugs)

- **The forecast substrate ≠ the production substrate.** The 45 fps number is a trivial-scene,
  mocked-data, front-end-only measurement (design doc AMENDMENT). It validated that *baked-dark
  costs nothing vs light* — a narrow claim — and was over-generalized into a whole-app fps forecast.
  Any future "it'll be fast enough" claim for the map must be measured with real tiles + markers +
  a pack installed, via grim on the Pi, not on `dev/render-harness`.

- **`setStyle` as the pack-compositing mechanism is structurally expensive.** Findings 1+2+4
  compound: one pack install = full style regen + bake + parse + layout + a `styledata` storm that
  re-adds + re-pushes every overlay in every mounted consumer. The owned-hook lifecycle layer was
  built precisely to add/remove sources+layers incrementally; pack compositing should use it instead
  of nuking the style.

- **No caching layer anywhere between MapLibre and disk for pmtiles bytes** (no `Cache-Control`, no
  Rust-side range LRU). On a CPU rasterizer where re-decode is the dominant cost, the absence of any
  "you already have this" short-circuit makes pan-back re-pay the full decode pipeline.

- **Full Protomaps layer/label set on a software rasterizer.** The style was adopted wholesale; for
  an EmComm field map a curated, label-light layer set would cut per-frame paint substantially. Label
  collision is the classic llvmpipe killer and the grid layer explicitly disables the collision
  optimization.

- **`MAX_RESPONSE_BYTES` (16 MiB) is not a perf problem in normal operation** — the pmtiles client
  only requests small bounded ranges, so it never truncates a legitimate read; it is a correct OOM
  guard. Flagged only to confirm it was reviewed and is NOT contributing to the symptom.

- **The retired raster path (`tiles/`) is dead weight, not a perf bug.** It is well-guarded and
  correct; it simply isn't on the vector map's hot path. No action needed for perf.

## Ranking summary (by expected impact on the symptom)

1. Finding 5 — JS-side per-tile gunzip + parse + tessellation on CPU-bound cores (inherent + unmitigated)
2. Finding 6 — full label-heavy layer set painted every frame, doubled per pack
3. Finding 3 — no byte/HTTP cache → repeat IPC + disk + re-decode on pan-back
4. Finding 4 — per-pan/per-tick overlay FC rebuild + full `setData` + listener churn (worst in StationFinderMap)
5. Finding 1 + 2 — redundant full `setStyle` + style regen + dark bake on pack change
6. Finding 7 — overzoom of z6 overview to z14 outside pack coverage
7. Findings 8, 9 — minor recompute coupling
