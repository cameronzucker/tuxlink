# Bug Hunt Report — Offline Vector-Map Performance RCA (multipass)

**Agent:** raven-poplar-clover
**Date:** 2026-06-15
**Method:** code-bug-hunter-multipass, all 5 passes, biased toward PERFORMANCE defects.
**Substrate under analysis:** Raspberry Pi 5, forced software GL (llvmpipe), WebKitGTK; MapLibre GL JS over PMTiles via a custom `tile://pmtiles/...` Rust 206 seam.

## Scope

Frontend `src/map/` (MapLibreMap.tsx, mapHooks.ts, basemapStyle.ts, darkStyle.ts, tuxlinkFlavor.ts, useBasemapFlavor.ts, MaidenheadGridLayer.tsx), consumers (StationFinderMap.tsx, LocationMap.tsx, PositionMapWidget.tsx), backend seam `src-tauri/src/basemap/mod.rs` + the `tile` URI-scheme handler in `src-tauri/src/lib.rs`, `src-tauri/src/tiles/serve.rs`. Design doc + build script cross-referenced. Passes performed: 1 contract, 2 cross-sibling, 3 failure-mode, 4 concurrency, 5 error-propagation.

The forecast (~45 fps) came from the ndi4 R4 spike on `dev/render-harness` — front-end only, mocked Tauri data, trivial scene with no real tile decode, no markers, no pack compositing. The real app must do all three of those things, on the CPU rasterizer, plus the defects below.

---

## Bugs (ranked by performance impact)

### P1 — Backend tile responses carry NO `Cache-Control`; every byte-range refetched over the IPC/custom-scheme boundary on every pan/zoom
**Location:** `src-tauri/src/lib.rs:262-303` (pmtiles branch response builder); confirmed absent via grep — no `CACHE_CONTROL` header anywhere in the handler.
**Severity:** critical
**Found in:** Pass 3 (failure-mode: cache miss / refetch storm) + Pass 5.
**Evidence:** The pmtiles 206 response sets `Content-Type`, `Accept-Ranges`, CORS, `Content-Length`, `ETag`, `Content-Range` — but never `Cache-Control` / `Expires`. The `pmtiles` JS `FetchSource` issues a separate Range request per directory page and per tile. With no cache directive, WebKitGTK's network layer will not serve these from HTTP cache, so the SAME header/root-directory/leaf-directory ranges are re-requested every time MapLibre needs a tile that isn't already in its in-memory tile cache — i.e. constantly during pan/zoom. Each request is a full custom-scheme round trip: webview → wry → `register_asynchronous_uri_scheme_protocol` closure → `spawn_blocking` → `read_at` (pread) → response marshalling back across the boundary.
**Why it costs on this substrate:** The archive bytes are immutable for the session (the ETag is literally `format!("\"{}\"", total_len)` — content never changes). The directory pages especially are hit on nearly every tile resolution. Re-doing the scheme round-trip + 206 framing for bytes that never change is pure overhead stacked on an already CPU-starved frame budget. The cost is per-tile-request, and panning a vector map at z6–14 requests dozens of tiles per move.
**Expected magnitude:** Large. Eliminates a multiplicative per-tile IPC + decode-setup cost; directory-page refetches alone are issued for most tile loads.
**Fix direction:** Emit `Cache-Control: public, max-age=31536000, immutable` on 200/206 pmtiles responses (the bytes are immutable per archive id; a deleted/replaced pack gets a new id or the registry 404s). This lets WebKit's HTTP cache short-circuit repeat ranges entirely.

### P2 — The `packs` fetch fires a SECOND full `setStyle` after first load, reloading the entire style + re-decoding the world overview
**Location:** `src/map/MapLibreMap.tsx:109-124` (`fetchPacks` effect) + `:219-228` (style rebuild effect); `setMap(instance)` at `:158`.
**Severity:** critical
**Found in:** Pass 1 (contract: effect-dependency contract causing redundant work) + Pass 3.
**Evidence:** The map is constructed with `buildBasemapStyle(flavorRef.current)` (overview only, no packs) at `:132`. Separately, the mount effect at `:119` calls `fetchPacks()`, which `invoke('basemap_list_packs')` and `setPacks(...)`. The style-rebuild effect at `:220` keys on `${effectiveFlavor}|${pack-ids}`. On first run `styleKeyRef.current` is seeded to `${flavor}|` (empty packs). When `basemap_list_packs` resolves with ANY installed pack, the key becomes `${flavor}|cascadia,...` ≠ the seeded key, so `map.setStyle(buildBasemapStyle(effectiveFlavor, packs))` runs — a full style teardown + rebuild moments after the initial load completes. `setStyle` drops every source and layer (forcing all overlay hooks to re-add via `styledata`) and re-initializes all basemap sources, re-fetching the world header/directories and re-decoding visible tiles.
**Why it costs on this substrate:** A full `setStyle` is one of the most expensive MapLibre operations — it discards GL programs, re-parses the style, re-creates all sources, and re-triggers tile loading. Doing it unconditionally at startup whenever a pack exists doubles the initial map-load work on the slowest possible renderer, and re-runs every overlay's add path. The comment at `:107-108` acknowledges the design ("Fetched after mount … a change drives setStyle") but the consequence is a guaranteed redundant style reload on every cold start where a pack is installed.
**Expected magnitude:** Large at startup and on every pack-list change; visible as a "loads, then reloads/flashes" stall.
**Fix direction:** Build the initial style WITH the packs already known, or gate the construct-once effect on the first `fetchPacks` resolving (construct after packs are loaded). Alternatively use `map.addSource`/`map.addLayer` to composite a newly-installed pack incrementally rather than a whole-style swap; reserve `setStyle` for the flavor (light↔dark) change it was designed for.

### P3 — PMTiles tiles are gzip-compressed and decompressed on the JS main thread, per tile, every load
**Location:** `src/map/basemapStyle.ts:30,87-89` (source URLs route through the `pmtiles` JS protocol registered at `MapLibreMap.tsx:37-38`); build script `scripts/build-basemap-bundle.sh:96` (`pmtiles extract` of a standard protomaps planet build, which is gzip tile-compressed); backend comment `src-tauri/src/basemap/mod.rs:13-16` ("PMTiles internal compression is decoded by the JS client, not here").
**Severity:** significant
**Found in:** Pass 3 (failure-mode: slow path) + Pass 4 (main-thread blocking).
**Evidence:** The Rust seam deliberately serves RAW bytes with "zero content decoding"; the `pmtiles` JS library therefore performs the gzip inflate of each tile's bytes in JS. The standard protomaps planet PMTiles uses gzip tile compression, and `pmtiles extract` preserves the source compression. MapLibre then parses the inflated vector tile (MVT protobuf) — also on the main thread in this build path. There is no Web Worker offload visible, and even if MapLibre worker-pools its MVT parse, the `pmtiles` protocol decompress runs on the protocol callback's thread (main).
**Why it costs on this substrate:** llvmpipe already consumes the CPU for rasterization; competing main-thread gzip inflate + protobuf parse for every newly-visible tile during a pan directly steals from the frame budget and shows as jank. The R4 forecast scene had NO real tile decode, so this entire cost was absent from the 45-fps number.
**Expected magnitude:** Significant and continuous during pan/zoom; the single largest item the R4 forecast omitted.
**Fix direction:** Bake the bundle with `--no-tile-compression` (serve already-inflated MVT) so the JS client skips inflate — trading ~2-3x larger archive bytes (cheap: bytes are local, disk is not tight) for zero per-tile main-thread decompress. Verify the `pmtiles` client's `tile-compression` metadata is honored. This is the highest-leverage decode fix.

### P4 — `useBasemapFlavor` MutationObserver watches `style` on `<html>`, re-resolving (and risking a full `setStyle`) on unrelated inline-style mutations
**Location:** `src/map/useBasemapFlavor.ts:32-41`.
**Severity:** significant
**Found in:** Pass 3 (failure-mode) + Pass 4 (concurrency/observer churn).
**Evidence:** The observer subscribes to `attributeFilter: ['data-theme', 'style']` on `document.documentElement`. Any code that writes ANY inline style on `<html>` (scroll-lock, CSS custom-property updates, viewport/vh shims, theme tweaks) fires `update()` → `resolveBasemapFlavor()` → `setFlavor(...)`. If the resolved string actually flips, the style-rebuild effect at `MapLibreMap.tsx:220` runs a full `map.setStyle`. Even when the flavor string is unchanged, `setFlavor` to the same value is cheap (React bails) — but the `style` filter is far broader than needed (only the `custom` theme reads `root.style.colorScheme`).
**Why it costs on this substrate:** A spurious flavor flip triggers the most expensive map op (full style reload, P2's mechanism) on the slowest renderer. Frequent `<html>` inline-style writes are common in app shells (modals, drawers). The blast radius is "occasional unexplained full map reload + overlay re-add storm."
**Expected magnitude:** Bursty/significant — zero most frames, catastrophic on the frames it fires during interaction.
**Fix direction:** Narrow the observer to `['data-theme']` and only also watch `style` when the active theme is `custom`; or memoize on the resolved string so a same-value resolution can never reach `setStyle`. The `styleKeyRef` guard already protects against same-key, so the real fix is preventing spurious string flips.

### P5 — Every overlay rebinds `setData` to the `styledata` event, and `styledata` fires repeatedly; combined with N overlays this is O(overlays × styledata-events) main-thread work per style settle
**Location:** `src/map/MaidenheadGridLayer.tsx:129-140`; `src/catalog/StationFinderMap.tsx:110-127` (`usePushData`); `src/location/LocationMap.tsx:124-135`; `src/compose/PositionMapWidget.tsx:95-106`; plus `mapHooks.ts:116-117,147-148,189` each binding `load`+`styledata` ensure handlers.
**Severity:** significant
**Found in:** Pass 2 (cross-sibling: four siblings implement the identical `push on styledata` pattern).
**Evidence:** `styledata` is documented by MapLibre to fire many times as a style + its tiles settle (not once). Each overlay registers a `styledata` listener that calls `getSource(...).setData(geojson)`, and each `useMap*` hook registers `load`+`styledata` ensure handlers that call `getSource`/`getLayer` guards + potential `addSource`/`addLayer`. On a map with the grid layer + station pins + operator pin (StationFinder) or grid + square + marker (Location), that is 3-5 sources × (re-push + ensure) × every `styledata` tick. The pattern is correct for surviving style swaps, but it means a single `setStyle` (P2/P4) amplifies into many redundant `setData`/`getSource` calls.
**Why it costs on this substrate:** Multiplies the per-style-swap cost. The redundancy is harmless when `setStyle` never runs spuriously — but P2 guarantees one `setStyle` at startup and P4 can trigger more, and each one then pays the full O(overlays × ticks) tax. `text-allow-overlap: true` + `text-ignore-placement: true` on the grid labels (`MaidenheadGridLayer.tsx:54-55`) additionally disables label-collision culling, so every grid label is rasterized even when overlapping — extra glyph raster work on the CPU rasterizer at high grid densities.
**Expected magnitude:** Moderate, multiplicative with P2/P4. The label-overlap setting is a steady per-frame raster cost wherever the grid is dense.
**Fix direction:** Debounce the `styledata` re-push to the trailing edge (or switch to the single `style.load`/`idle` event for re-push). Reconsider `text-ignore-placement: true` for the grid at field/square density.

### P6 — `MaidenheadGridLayer` recomputes the entire lattice GeoJSON on EVERY `moveend` via a forced re-render, including small pans that don't change the level
**Location:** `src/map/MaidenheadGridLayer.tsx:96-120`.
**Severity:** significant
**Found in:** Pass 1 (contract: per-move redundant work) + Pass 3.
**Evidence:** `moveend` bumps a `setTick` counter (`:98-105`) forcing a re-render. On re-render, `effBounds` is recomputed from `map.getBounds()` as a FRESH object literal every render (`:107-114`), and the `useMemo` at `:117` keys on `effBounds?.south/west/north/east` — so any pan (which changes bounds) re-runs `gridLines(...)` + `gridToGeoJSON(...)` and produces a new feature collection, which the effect at `:129` then `setData`s. For a continuous drag-pan this is a full lattice rebuild + GeoJSON serialize + source re-tessellation on every pan release.
**Why it costs on this substrate:** Rebuilding and re-uploading a GeoJSON source forces MapLibre to re-tessellate and re-buffer the geometry on the CPU. Doing it for every pan — even pans that keep the same grid level and barely move — is redundant; the lattice only needs regeneration when the level changes or the viewport leaves the previously-generated extent. This compounds with P5's `styledata` re-push.
**Expected magnitude:** Moderate-to-significant on the Location/Position maps whenever the grid is visible during interaction.
**Fix direction:** Only regenerate when `effLevel` changes or the new bounds exceed a generated-with-margin extent; generate the lattice over a padded bbox so small pans reuse it. Throttle the `moveend` tick.

### P7 — `tuxlinkFlavor()` + `bakeDarkColors()` recompute the entire layer set (and, in dark mode, transform every color) on every `buildBasemapStyle` call
**Location:** `src/map/tuxlinkFlavor.ts:69-71`; `src/map/basemapStyle.ts:115-154`; `src/map/darkStyle.ts:98-111`.
**Severity:** minor
**Found in:** Pass 1 + Pass 3.
**Evidence:** `buildBasemapStyle` calls `layers(BASEMAP_SOURCE_ID, tuxlinkFlavor(), {lang:'en'})` (the full protomaps layer generator — dozens of layers) and, per installed pack, calls it AGAIN and filters/remaps. In dark mode each call also runs `bakeDarkColors` which deep-copies every layer and walks every `*-color` paint value (including recursing through data-driven expression arrays). Nothing is memoized; every `setStyle` (P2/P4) and the construct call re-run all of this.
**Why it costs on this substrate:** Layer-set generation + per-color matrix transform is pure JS main-thread work done synchronously before the GL style upload. It is small relative to tile decode but stacks onto the already-redundant `setStyle` calls. The cost is also O(packs) — N packs means N full `layers()` generations.
**Expected magnitude:** Minor in isolation; meaningful only because P2/P4 invoke it more than necessary.
**Fix direction:** Memoize the baked light/dark layer arrays by `(flavor)` and the per-pack layer remap by `(flavor, packId)`. Reducing `setStyle` frequency (P2/P4) largely subsumes this.

### P8 — `read_range` allocates a fresh zeroed `Vec<u8>` and copies for every range read; no buffer reuse on the hot path
**Location:** `src-tauri/src/basemap/mod.rs:81-92` (`read_at` → `vec![0u8; n]` then `read_exact_at`).
**Severity:** minor
**Found in:** Pass 4 (concurrency/allocation on hot path).
**Evidence:** Each tile/directory read does `let mut buf = vec![0u8; n]` (zero-initializes the buffer the kernel is about to overwrite) then `read_exact_at`. The zeroing is wasted work, and there is one allocation per request. This is on the per-tile path that P1 currently forces to run far more often than necessary.
**Why it costs on this substrate:** Small per-call, but it runs on `spawn_blocking` worker threads contending for the same CPU as llvmpipe. The zero-fill is avoidable. The cost is dominated by P1/P3, but is worth noting because fixing P1 reduces call count while this remains per-surviving-call.
**Expected magnitude:** Minor.
**Fix direction:** Use `Vec::with_capacity(n)` + `read_at` into the spare capacity via a `read`-style API, or accept the allocation but skip the zero-init (e.g. `MaybeUninit`/`read_buf` once stable). Low priority relative to P1-P3.

---

## Design Concerns (not individual bugs)

1. **The 45-fps forecast is structurally non-comparable to production.** The R4 spike scene (per the design doc, lines 17-28) measured a baked-dark vs CSS-filter comparison on a trivial mocked scene with no tile decode, no markers, no compositing. Every one of P1-P7 is absent from that harness. The forecast should be treated as an upper bound on the *renderer* cost only, never an app-level prediction. This is the root framing error behind the symptom, not a single code defect.

2. **`maxZoom = 14` with overzoom of the z0-6 overview outside pack coverage** (`MapLibreMap.tsx:43`, `basemapStyle.ts:104-109`). Overzooming a z6 source to z14 means MapLibre rasterizes heavily-scaled geometry — coarse but still drawn — across the whole viewport outside any pack. Combined with the software rasterizer this is wasted fill where the user gets no detail. Consider clamping overview overzoom or reducing `maxZoom` where no pack is installed.

3. **No HTTP caching contract anywhere on the custom scheme** (both the pmtiles branch and the legacy raster branch in `lib.rs`). The immutability of archive bytes is asserted via a length-derived ETag but never leveraged for caching. This is the single cheapest high-impact fix (P1).

4. **`setStyle` is overloaded as the mechanism for THREE distinct changes** (flavor, pack add, and — via the observer — spurious theme-style mutations). Each is the most expensive map operation. The architecture would benefit from incremental `addSource`/`removeSource` for pack changes, reserving `setStyle` strictly for the light↔dark bake swap.

## Note for testing-pitfalls.md

The defects here (P1 missing cache header, P2 redundant `setStyle`, P3 main-thread decompress, P6 per-move lattice rebuild) are all PERFORMANCE-correctness issues that NO unit test in `src/map/*.test.tsx` could catch — they pass through the maplibre mock, which has no real tile decode, no real `setStyle` cost, and no network/cache layer. The class of bug ("forecast from a mocked harness, shipped against a real decode+raster path") is invisible to the existing test strategy by construction. The only gate that would catch it is a grim/real-WebKitGTK frame-timing smoke during pan/zoom with a real archive registered and at least one pack installed — the exact scenario the R4 harness omitted. Recommend documenting that map perf claims require a real-archive grim smoke, never the mock.
