# Vector Map Performance — Consolidated Findings + RCA

**Date:** 2026-06-15
**Agent:** raven-poplar-clover
**bd issue:** tuxlink-vnk7
**Scope:** the shipped offline vector-map subsystem on `origin/main` (`a53d247b`) — `src/map/*`, its consumers (`src/catalog/StationFinderMap.tsx`, `src/location/LocationMap.tsx`, `src/compose/PositionMapWidget.tsx`, APRS map), and the Rust tile seam (`src-tauri/src/basemap/*`, `tiles/*`, `lib.rs` GL env + custom-scheme handler).
**Symptom:** panning, tile loading, and overall app responsiveness on the Pi 5 are much worse than the ~45 fps the design forecast.
**Sources (5):** code-bug-hunters exploratory / holistic / multipass; Codex adrev round A (perf bug hunt); Codex adrev round B (design review). Raw Codex transcripts: `dev/adversarial/2026-06-15-mapperf-{bugs,design}-codex.md` (gitignored, local).
**Cross-validation:** every confirmed bug below was read at the cited `file:line` by the consolidator, not taken on a hunter's word.

---

## Executive summary

- **11 confirmed performance defects** (1 critical, 4 significant, 6 minor/contributory).
- **4 design decisions requiring operator input** — including whether the WebGL-vector renderer itself is the right choice on a software rasterizer.
- **3 false-positives / cleared suspects** (DOM-marker reflow, the Rust read_range seam, the retired LAN-raster path).
- The single highest-leverage *cheap* fix is a one-line `Cache-Control` header; the single highest-impact *structural* defect is per-pack style duplication.

**The RCA in one paragraph:** The ~45-fps number was measured on a proxy (the front-end-only render-harness with mocked data and a trivial scene) that omitted every expensive production path — real tile decode, markers, and region-pack compositing — so it was never a valid app-level forecast. Underneath that measurement error sits a substrate reality: MapLibre's WebGL vector rendering is forced through llvmpipe **CPU** software rasterization (Pi-5 hardware WebGL is broken), which is the worst-case cost model for a vector GL renderer. On top of that substrate, a cluster of shipped code defects multiply the per-frame and per-tile cost — pack-layer duplication, a redundant post-load `setStyle`, runtime dark re-baking, an untuned canvas (device-pixel-ratio + tile fades), main-thread tile decompression, a tile refetch storm, and per-pan grid/overlay rebuilds. They stack multiplicatively, which is how a "tight but ~45 fps" spike becomes a sluggish app. Whole-app sluggishness follows because all of this saturates the single shared WebKitGTK CPU, starving main-thread input and React everywhere.

---

## Confirmed Bugs

### B1. Region packs duplicate the ENTIRE Protomaps layer stack, per pack, coverage-ungated  — CRITICAL
**Consensus:** exploratory#1, holistic#2/#5, Codex-bugs#1, Codex-design#3 (4/5 sources). Verified.
**Location:** `src/map/basemapStyle.ts:139-154` (loop), `:127` (base), `MapLibreMap.tsx:220-228` (applied via setStyle).
**Evidence:** `buildBasemapStyle` appends `layers(packSourceId, tuxlinkFlavor(), {lang:'en'})` — the full `@protomaps/basemaps` set (~69 layers, ~13 of them `symbol`/label layers; `basemapStyle.test.ts:38` asserts `>50`) — for **each** installed pack, on top of the base ~69. Pack layers are gated only by `minzoom >= REGION_MINZOOM (6)` (`:151`), **not** by the pack's bounding box. So at z≥6 every pack's full layer set is filter/paint-evaluated **globally**, everywhere in the viewport, not just inside the pack's coverage.
**Impact:** 1 pack ≈ ~137 layers / ~26 label layers; 3 packs ≈ ~273 / ~52. Label layers (glyph shaping + collision + halo) are the most expensive primitive on llvmpipe and run every frame during a pan. The 45-fps spike measured **zero** packs, so it never hit this cliff. This is the largest shipped-map defect the moment any region pack exists.
**Blast radius:** `basemapStyle.ts` + the `setStyle` call site. Fix interacts with B2/B3 (same files). Changing compositing must preserve the "never blank; full detail in pack coverage" success criterion (design §R7) and must keep dropping the pack `background` layer (basemapStyle.ts:132-138 — already correct).
**Fix approach:** emit only fill/line *detail* layers per pack (let the single base own labels), and/or coverage-gate pack layers to the pack bbox, and/or merge overview+region into one virtual source. Cap total label layers.

### B2. A second full `setStyle` fires right after first load whenever any pack is installed — SIGNIFICANT
**Consensus:** exploratory#2, holistic#5, multipass#P2, Codex-bugs#2 (4/5). Verified.
**Location:** `MapLibreMap.tsx:110-124` (fetchPacks → setPacks), `:219-228` (rebuild effect).
**Evidence:** the map is constructed overview-only (`:132` builds style from `flavorRef` with no packs). `fetchPacks()` then resolves `basemap_list_packs`, calls `setPacks(...)`; the style-key effect seeded its ref to `${flavor}|` (empty, `:223`), so the key now differs and `map.setStyle(buildBasemapStyle(flavor, packs))` runs — a **full teardown + reparse + re-source + re-decode of visible tiles + overlay re-add storm**, moments after first paint, on the busiest substrate. The empty-packs case is correctly suppressed by the key guard; the regression is exactly the "packs installed" case the spike omitted, and it pays B1's now-much-larger style.
**Blast radius:** `MapLibreMap.tsx`. Coordinate with B1.
**Fix approach:** construct the initial style with installed packs already known (gate construction on the first `fetchPacks`, or seed from backend startup state), or composite a pack incrementally with `addSource`/`addLayer`. Reserve `setStyle` for genuine flavor swaps.

### B3. Dark style is re-baked at RUNTIME on every style build, not "once at build time" as documented — SIGNIFICANT
**Consensus:** exploratory#3/#4, holistic#5, multipass#P7 (3/5). Verified.
**Location:** `src/map/darkStyle.ts:98-111` (`bakeDarkColors`), `basemapStyle.ts:115-116,127,146`. Docstrings claiming "transformed once at build time": `MapLibreMap.tsx:71`, `basemapStyle.ts:6`, `darkStyle.ts:4-6`.
**Evidence:** `bakeDarkColors` deep-copies every layer, iterates every paint key, regex-tests every `*-color`, and recurses into data-driven expression arrays — executed on the UI thread on **every** `buildBasemapStyle('dark')` call: at mount, again on B2's redundant `setStyle`, again per theme toggle, and **once per pack** (`:146`). It is not memoized. The "baked at build time" comments are aspirational; nothing precomputes the dark layer array.
**Impact:** a multi-hundred-ms main-thread stall on the Pi at each (re)build, amplified by B1 (×packs) and B2 (×2 on open). Plus a false "baked once" invariant that hid the cost.
**Blast radius:** `darkStyle.ts` + `basemapStyle.ts`.
**Fix approach:** precompute the baked dark layer array once (module-level memo keyed by flavor, or emit it from the bundle build) and reuse; per-pack, remap a cached layer set rather than re-invoking `layers()` + `bakeDarkColors`.

### B4. No `Cache-Control` on PMTiles 206 responses → tile/directory refetch storm — SIGNIFICANT (cheapest fix)
**Consensus:** holistic#3, multipass#P1, Codex-bugs#4 (3/5). Verified.
**Location:** `src-tauri/src/lib.rs` response builder (~`:271-296`).
**Evidence:** the 206/200 response sets Content-Type, Accept-Ranges, CORS, Access-Control-Expose-Headers, Content-Length, ETag (`format!("\"{}\"", total_len)`), and Content-Range — but **no `Cache-Control`**. The `pmtiles` JS client issues a separate Range request per directory page and per tile; with no cache directive the webview has no instruction to reuse the **immutable** header/directory/leaf ranges, so it can re-request the same bytes on essentially every tile resolution during pan/zoom — each a full webview→wry→`spawn_blocking`→`pread`→marshal round trip competing with llvmpipe.
**Blast radius:** one Rust file, isolated, no behavior change (bytes are immutable for the session — the ETag is already length-derived).
**Fix approach:** add `Cache-Control: public, max-age=31536000, immutable`. Highest leverage per line of change.

### B5. Per-tile gzip decompress + MVT parse on the JS thread, competing with the rasterizer — SIGNIFICANT
**Consensus:** holistic#1, multipass#P3, Codex-bugs#5, Codex-design#4 (4/5).
**Location:** `src-tauri/src/basemap/mod.rs:11-16` (Rust does "zero content decoding"); `basemapStyle.ts:10-12` (JS pmtiles protocol decodes); bundle built by `scripts/build-basemap-bundle.sh` (gzip-compressed tiles, per `pmtiles extract` default).
**Evidence:** the Rust seam serves raw bytes by design; the `pmtiles`/MapLibre JS path inflates gzip + parses MVT + tessellates for every newly-visible tile. The default `pmtiles` decompressor uses `DecompressionStream` when present, else synchronous `fflate.decompressSync` on the calling thread. On WebKitGTK this may be synchronous; even if worker-side, it competes with llvmpipe for the same CPU cores. This is the largest *per-tile* cost the harness forecast omitted.
**Blast radius:** the out-of-band bundle build (rebuild required) + a check of `DecompressionStream` availability on the Pi's WebKitGTK.
**Fix approach:** verify `DecompressionStream` on target; bake the bundle with `--no-tile-compression` (larger local bytes — cheap on disk — for zero per-tile main-thread inflate), or provide a native/worker decompression path.

### B6. Maidenhead grid rebuilds the full lattice + `setData` + re-subscribes `styledata` on every `moveend`; labels disable collision culling — SIGNIFICANT
**Consensus:** exploratory#6, holistic#4, multipass#P5/P6, Codex-bugs#6 (4/5). Verified.
**Location:** `src/map/MaidenheadGridLayer.tsx:96-105` (moveend tick), `:107-120` (fresh `getBounds()` object each render → `useMemo` re-runs on any edge change), `:129-140` (push effect depends on the new `geojson` object → re-subscribes `styledata` + `setData` per move), `:54-55` (`text-allow-overlap: true` + `text-ignore-placement: true`).
**Evidence:** every `moveend` bumps a tick → re-render → `effBounds` is a new object → the bounds-keyed `useMemo` rebuilds `gridLines` + the full FeatureCollection → `setData` of fresh geometry → MapLibre re-tessellates/re-buffers. Forced overlap/ignore-placement means **no** label collision culling — every overlapping cell label is rasterized. Always-on in `LocationMap`; also in the position picker and `GridPicker`.
**Blast radius:** `MaidenheadGridLayer.tsx`, isolated.
**Fix approach:** recompute only when the rounded bounds or grid level actually change (debounce / level-gate); subscribe `styledata` once and read geojson from a ref; drop the forced overlap/ignore-placement so collision culling runs.

### B7. MapLibre constructed with default high-cost options for a software renderer — SIGNIFICANT
**Consensus:** exploratory (design), holistic (design), Codex-bugs#3, Codex-design#5 (4/5). Verified absent.
**Location:** `MapLibreMap.tsx:130-145` (constructor options).
**Evidence:** the constructor sets `renderWorldCopies:false` and `attributionControl:false` but does **not** set `pixelRatio` (defaults to `devicePixelRatio` — at DPR 2 that is ~4× the canvas pixels llvmpipe must fill), `fadeDuration` (defaults to 300ms — animates symbol/tile cross-fades, extra CPU passes during loads), `maxTileCacheSize`, or `crossSourceCollisions`/`validateStyle`. All standard software-GL mitigations are absent.
**Blast radius:** `MapLibreMap.tsx` constructor, isolated, very cheap, high value.
**Fix approach:** a Pi/software-GL profile: `pixelRatio: 1`, `fadeDuration: 0`, `validateStyle: false` in production; evaluate `crossSourceCollisions: false` and a bounded `maxTileCacheSize`. Measure DPR on the target first.

### B8. `onZoomChange` fires on every `moveend` (pans included) → React re-render of the modal+map subtree at the end of every drag — CONTRIBUTORY
**Consensus:** exploratory#5 (1/5, but verified). 
**Location:** `MapLibreMap.tsx:156` (`emitZoom`), `:161` (on `moveend`); consumer `src/compose/PositionPickerOverlay.tsx` wires `onZoomChange={setViewZoom}`.
**Evidence:** `emitZoom` calls `onZoomRef.current?.(getZoom())` on `moveend` (which fires after pans, not just zooms) straight into a `useState` setter, so every drag-settle re-renders the heaviest surface (modal + map + grid) even when zoom is unchanged. React reconciliation competes with the rasterizer at the end of each drag.
**Blast radius:** `MapLibreMap.tsx` + the picker consumer.
**Fix approach:** emit only on actual zoom change (compare to a last-emitted ref), or separate a `zoom` event from `moveend`.

### B9. Overlay re-push tax amplified by every `styledata`; prop-identity churn busts memos; selection rebuilds the whole FeatureCollection — CONTRIBUTORY
**Consensus:** holistic#4, multipass#P5, Codex-bugs#7/#8 (3/5).
**Location:** `mapHooks.ts:178-201`; `StationFinderMap.tsx:110-145` (`tiers` Map prop identity busts the memo; `data` in the push effect deps → re-subscribe + full source replace on every change; a click rebuilds the whole FC to flip one `selected` flag); `LocationMap.tsx:124-135`; `PositionMapWidget.tsx:95-106`.
**Evidence:** overlays re-ensure sources/layers and re-`setData` on `styledata`; correct once, but every B2/B4-driven `setStyle` amplifies into many redundant `setData`/`getSource`/`addLayer` calls across 3-5 sources. Selection toggling rebuilds an entire FeatureCollection instead of using `setFeatureState`.
**Blast radius:** the three consumers + `mapHooks.ts`. Reducing `setStyle` frequency (B2) is the shared lever.
**Fix approach:** stabilize prop identity (memoize `tiers`), drop `data` from the push effect deps (use a ref), use `setFeatureState` for selection, restore overlays once after `style.load` rather than on every `styledata`.

### B10. Drag overlays call `setData` per pointer `mousemove` — CONTRIBUTORY
**Consensus:** Codex-bugs#7 (1/5).
**Location:** `GridPicker.tsx:169-172,216-233` (selection drag → React state + `setData` per mousemove); `LocationMap.tsx:159-163` (pin drag → `setData` per map mousemove).
**Evidence:** pointer events can exceed render cadence, rebuilding GeoJSON repeatedly while the map is already CPU-limited.
**Blast radius:** the two picker surfaces.
**Fix approach:** throttle to one update per `requestAnimationFrame`; mutate an imperative preview source during drag; commit React state on mouseup.

### B11. Per-read zero-initialized `Vec` allocation on the hot read path — MINOR
**Consensus:** multipass#P8 (1/5).
**Location:** `src-tauri/src/basemap/mod.rs:81-92` (`vec![0u8; n]`).
**Evidence:** zero-inits a buffer the kernel immediately overwrites, one alloc per request, on `spawn_blocking` threads contending with llvmpipe. Dominated by B4/B5 but per-surviving-call.
**Fix approach:** read into an uninitialized/`with_capacity` buffer.

---

## Design Decisions Requiring Operator Input

### D1. Renderer substrate mismatch — keep MapLibre vector GL, or pivot (back) to raster tiles?
**The concern (Codex-design#1/#8, holistic design):** MapLibre vector GL was chosen for a GPU cost model, but the shipped Pi path forces all WebGL through **llvmpipe CPU software rasterization** (hardware WebGL is broken — `lib.rs` software-GL block). Vector GL pays per-frame shader/fill/layer/symbol/label/overdraw cost on the CPU; a raster strategy mostly blits already-rendered tiles and decodes only newly-exposed ones. Codex estimates a possible *order-of-magnitude* pan-time difference and frames raster as "the cleaner architectural move on this hardware."
**Why it needs a decision:** this would reverse the ndi4 design's deliberate Leaflet→MapLibre swap and re-introduce a raster pipeline (the #659 transport was retired-as-basemap but the design kept it as a candidate for imagery). Trade-offs: raster packs are ~3-10× larger and need a tile-build pipeline, lose dynamic styling and get baked labels; vector keeps small packs + dynamic light/dark.
**Recommendation:** do **not** pivot first. Apply the cheap/structural code-fix tier (B1-B7) and **measure on a real packaged Pi build with a pack installed** (D4) before committing to a renderer change. The amplifier defects are large enough that vector-GL may be viable once they're removed; a pivot is expensive and partly re-treads retired work. If, after the fix tier + measurement, frame time is still unacceptable, D1 becomes the next increment with real numbers behind it.

### D2. Style complexity — adopt a tuxlink-specific minimal EmComm style?
**The concern (Codex-design#3, exploratory#1, holistic#2):** the `@protomaps/basemaps` flavor carries >50 layers including many label layers — far more than a software rasterizer affords, before B1 multiplies it.
**Options:** (a) keep the full Protomaps flavor and rely on B1's compositing fix; (b) author a reduced style (water / land / boundaries / major roads / limited places) tuned for EmComm legibility and llvmpipe budget.
**Recommendation:** pursue (b) as a fast follow to B1 — a reduced base style compounds with every other fix and directly attacks the dominant label cost. It loses some OSM richness, which is an operator call on the EmComm aesthetic.

### D3. `maxZoom: 14` + overview overzoom — clamp the overview and gate deep zoom to pack coverage?
**The concern (all sources):** interactive `maxZoom` is 14 (`MapLibreMap.tsx:42`) but the bundled overview is only z0-6 and is left unclamped to overzoom everywhere (`basemapStyle.ts:102`). Outside pack coverage the app spends CPU rasterizing 8 levels of stretched coarse geometry with no added detail.
**Options:** clamp the overview source `maxzoom` to ~7-8; optionally unlock z14 only inside installed pack coverage (more camera-state logic, clearer UX).
**Recommendation:** clamp the overview maxzoom now (cheap, pure win); treat coverage-gated deep zoom as part of the B1 compositing rework.

### D4. The ship gate itself — institute a real on-Pi frame-timing smoke
**The concern (Codex-design#2, multipass testing note):** the 45-fps gate came from a mocked front-end-only harness. Codex estimates 2-5× optimism vs production.
**Recommendation:** make the perf gate a packaged WebKitGTK run on the Pi under software GL at the real window resolution, with the production world pack + ≥1 region pack + station pins + the grid, measuring p50/p95 frame time and input latency during scripted pan/zoom. This is the methodology the project's "Chromium is not a WebKitGTK proxy" rule already implies. Process/test change — see Test Gap Analysis.

---

## False Positives / Cleared Suspects

### FP1. DOM-marker reflow per move — NOT PRESENT.
Flagged as a hypothesis at scope-time (consolidator) and gestured at by the consumer-marker question. **Cleared:** Codex-bugs (non-finding) + Codex-design#7 + holistic confirm station/location/APRS/position markers are **GL GeoJSON layers**, not `maplibregl.Marker` DOM elements — no DOM layout/reflow cost. (The cost they *do* carry is the GL overlay re-push tax, captured as B9.)

### FP2. The Rust `read_range` seam / `MAX_RESPONSE_BYTES` / file reopen — NOT a bottleneck.
**Cleared by all three hunters + Codex-bugs non-findings:** `basemap::read_range` is a lock-free positioned `pread` on a retained `Arc<File>` (no per-range reopen), run on `spawn_blocking`, doing zero content decode; `MAX_RESPONSE_BYTES` (16 MiB) only refuses anomalous whole-archive reads and never trips on the small bounded happy-path ranges. (B4 and B11 are about the *response headers* and *allocation*, not the read logic.)

### FP3. The retired LAN-raster transport (`src-tauri/src/tiles/{serve,fetch,cache,breaker,host}.rs`) — DORMANT.
**Cleared by holistic scope correction:** `tileSource` was removed (design A5); no consumer mounts a raster source. ~0 runtime contribution. (Parked for the future imagery overlay.)

---

## Test Gap Analysis (Phase 4)

The defining property of this whole class: **none of B1-B7 is catchable by the existing `src/map/*.test.tsx` suite**, because every test runs against the MapLibre mock (`testMapLibreMock.ts`) — which has no real tile decode, no real `setStyle` cost, no network/cache layer, and no rasterizer. The bug class ("a perf forecast taken from a mocked harness, shipped against a real decode+raster path") is invisible to the test strategy *by construction*.

| Bug | Why tests missed it | Catch mechanism |
|---|---|---|
| B1 pack-layer multiplication | mock ignores layer count/cost; no test asserts a layer/label *budget* | unit: assert `buildBasemapStyle(f, [p1,p2]).layers.length` and label-layer count stay within a budget; the duplication would fail it |
| B2 double `setStyle` on open | mock `setStyle` is free; no test counts style loads on the construct→packs path | unit: spy `setStyle`/style builds across mount-with-packs; assert ≤1 |
| B3 runtime re-bake | no referential-identity assertion on the dark layer array | unit: assert two `buildBasemapStyle('dark')` calls return the **same** memoized baked array (identity), and that `bakeDarkColors` is invoked ≤ once per flavor |
| B4 no Cache-Control | no test inspects response headers from the scheme handler | Rust unit: assert the pmtiles response carries `Cache-Control: …immutable` |
| B5 main-thread decompress | mock serves canned tiles; no decode happens | only a real-archive on-Pi frame-timing smoke (D4) surfaces it |
| B6 grid rebuild per move | mock `setData` is free; no test counts recomputes per `moveend` | unit: count `gridToGeoJSON`/`setData` calls across N `moveend`s with unchanged level; assert no recompute when level + rounded bounds unchanged |
| B7 untuned canvas | mock ignores constructor options | unit: assert the production constructor passes `pixelRatio:1`/`fadeDuration:0` under the Pi profile |

**The structural gap (B5, and confirmation of all the rest):** there is no real-engine, real-archive, frame-timing smoke. This is the same blind spot the `dev/render-harness` README was created to address for *visual* defects — it now needs a **performance** sibling (D4): a packaged-Pi pan/zoom timing run with a pack + pins, asserting p95 frame time. That is the only thing that would have caught the gap between the 45-fps forecast and the shipped reality, and it is the gate that should front any future map perf claim.

**`dev/testing-pitfalls.md` candidate addition (generalizable):** *"A performance forecast measured on the mocked render-harness (front-end only, canned Tauri data, trivial scene) is NOT an app-level fps prediction and MUST NOT gate a perf-sensitive ship. The harness omits real tile decode, markers, and data compositing. Perf claims for the map require a packaged WebKitGTK run on the Pi under software GL at real resolution, with a region pack + pins + overlays mounted, measuring p50/p95 frame time during scripted pan/zoom."*

---

## Fix sequencing (recommendation feeding the fix plan)

**Tier 1 — cheap, isolated, no design decision (do first, then measure):** B4 (Cache-Control), B7 (Pi render profile), B3 (memoize dark bake), B11 (alloc). Each is a small, independent change.

**Tier 2 — structural, coordinated in `basemapStyle.ts`+`MapLibreMap.tsx`:** B1 (pack compositing — detail-only/coverage-gated layers) + B2 (construct-with-packs, drop the post-load setStyle) + D3 (clamp overview maxzoom). One coordinated task; they touch the same files and the same `setStyle` path.

**Tier 3 — overlay/interaction churn:** B6 (grid debounce + collision culling), B8 (zoom-only emit), B9 (overlay re-push + `setFeatureState`), B10 (rAF-throttle drags).

**Tier 4 — bundle/build:** B5 (uncompressed-tile bundle or worker decode) — needs the out-of-band bundle rebuilt.

**Measure between Tier 2 and any D1/D2 decision** on a real packaged Pi build (D4). The renderer pivot (D1) and minimal style (D2) are decided *after* the code-fix tier is measured — not before.
