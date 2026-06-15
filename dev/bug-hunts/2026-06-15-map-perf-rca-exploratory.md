# Bug Hunt Report — Offline Vector-Map Performance RCA (exploratory)

Agent: raven-poplar-clover · 2026-06-15 · depth-first exploration

## Scope

Hunted the shipped offline vector-map subsystem for defects that degrade
pan/load/render perf or make the whole app sluggish while a map is mounted on
the **software-GL (llvmpipe) Pi 5** — every WebGL draw is CPU-rasterized, so any
per-frame / per-move work and any inflation of the rendered scene (layers,
labels, sources) is paid in CPU cycles the rasterizer needs.

Explored deeply (read in full):
- `src/map/MapLibreMap.tsx` — the construct-once owner + event wiring (prime suspect: per-move handlers).
- `src/map/basemapStyle.ts` + `darkStyle.ts` + `tuxlinkFlavor.ts` — the style assembler (prime suspect: layer/label count, runtime baking).
- `src/map/MaidenheadGridLayer.tsx` — self-driving overlay recomputed on every `moveend`.
- `src/catalog/StationFinderMap.tsx`, `src/location/LocationMap.tsx`, `src/compose/PositionMapWidget.tsx` + `PositionPickerOverlay.tsx` — the three map consumers.
- `src/map/mapHooks.ts`, `useBasemapFlavor.ts`, `useDownloadProgress.ts`, `projection.ts`.
- Backend seam: `src-tauri/src/lib.rs` (tile scheme handler), `src-tauri/src/basemap/mod.rs` (`read_range` / `PmtilesRegistry`), skim of `tiles/serve.rs`.

Grounding for the forecast gap: `docs/design/...-vector-osm-basemap-design.md:28`
records the 45 fps figure as the **baked-dark GL style at the light baseline**
measured in the front-end-only R4 spike (trivial scene, mocked Tauri, no real
tile decode, no markers, no pack compositing). The real app pays for all of
those, which is the umbrella reason the forecast doesn't hold; the findings
below are the concrete mechanisms.

**Backend verdict:** the Rust seam is *not* the bottleneck. `read_range` uses
lock-free positioned `pread` (`mod.rs:81`), runs on `spawn_blocking`
(`lib.rs:262`), does zero content decoding (the JS `pmtiles` lib decompresses),
and `MAX_RESPONSE_BYTES` only refuses anomalous whole-archive reads — the real
client only sends small bounded ranges, so the cap never trips on the happy
path. No finding there. The cost is on the front end and in the rendered scene.

## Bugs

### 1. Region packs duplicate the ENTIRE 69-layer Protomaps style per pack — layer/label count explodes on a CPU rasterizer
**Location:** `src/map/basemapStyle.ts:139-154` (the `for (const pack of packs)` loop)
**Severity:** critical
**Evidence:** `layers(sid, tuxlinkFlavor(), {lang:'en'})` returns the full
Protomaps layer set — **69 layers, 13 of them `symbol` (label) layers** (verified
via `node -e` against `@protomaps/basemaps@5.7.2`). The loop appends that whole
set (minus the single `background` layer = 68 layers, 13 labels) for **each**
installed pack, on top of the 69 base layers. With one pack the style carries
~137 layers / 26 label layers; with three packs ~273 layers / 52 label layers.
Worse, the pack layers are NOT geographically gated beyond `minzoom>=6`
(`basemapStyle.ts:151`) — they have no source-coverage bound, so at z>=6
**everywhere** MapLibre evaluates every pack layer's filter/paint against its
source (which simply returns no tiles outside coverage). Label layers are the
most expensive primitive on a software rasterizer (glyph shaping + collision
detection + halo compositing run on the CPU every frame during a move).
**Impact:** Each installed pack multiplies per-frame layer-evaluation and
label-placement work. This is the single largest scaling cliff: the spike
measured ZERO packs (overview only), so the forecast never saw this cost. On
llvmpipe, doubling label layers roughly doubles the per-pan placement pass.
**Fix direction:** Do not re-emit the full label/symbol set per pack. Emit pack
*fill/line* detail layers only and let the **single** base overview own all
labels (overzoomed labels are legible enough, or bump the overview label
layers' maxzoom). Alternatively gate pack layers behind the pack's actual
bounding box so they are not evaluated globally. Cap total label layers
regardless of pack count.

### 2. The `packs` fetch fires a SECOND full `setStyle` after every map load — a complete style teardown + reparse on the Pi
**Location:** `src/map/MapLibreMap.tsx:119-124` (fetch effect) → `:220-228` (rebuild effect); `setPacks` at `:113`
**Severity:** significant
**Evidence:** The map is constructed with `{flavor, no packs}` (`:132`). On mount
the `fetchPacks` effect calls `basemap_list_packs` and `setPacks(...)`
(`:113`). If any pack is installed, the resulting `packs` state differs from the
construct-time key, so the rebuild effect's guard at `:224` falls through and
calls `map.setStyle(buildBasemapStyle(effectiveFlavor, packs))` (`:227`). The
comment at `:108-109` acknowledges this ("a change drives setStyle"). `setStyle`
discards every source/layer, re-parses the (now much larger — see finding 1)
style, re-fires `styledata`, and forces every owned overlay hook to re-add its
sources/layers (`mapHooks.ts:116-117,147,189`) and every consumer to re-`setData`
(`StationFinderMap.tsx:124`, etc.). This happens right after first paint, when the
operator is most likely already interacting.
**Impact:** A second full style reload (build + parse + re-tessellate all
overview layers) within the first second of every map mount when packs exist —
visible as a stutter/flash and a burst of CPU on the substrate least able to
absorb it. The empty-packs case is fine (key matches, guarded out at `:224`);
the regression is exactly the "real app has packs installed" case the spike
omitted.
**Fix direction:** Build the initial style WITH the currently-installed packs at
construction time (await/seed `packs` before `new maplibregl.Map`, or pass a
synchronously-known pack list), so the common steady state requires no
post-load `setStyle`. Reserve `setStyle` for genuine *changes* (pack
added/removed, flavor toggled) after mount.

### 3. Dark mode is baked at RUNTIME on every style build, not at build time as documented
**Location:** `src/map/basemapStyle.ts:115-116,127,146` calling `bakeDarkColors` (`darkStyle.ts:98-111`)
**Severity:** significant
**Evidence:** Every doc string calls dark mode "build-time-baked"
(`MapLibreMap.tsx:71`, `basemapStyle.ts:6`, `darkStyle.ts:4`, "Each style color
is transformed once at build time"). It is not. `buildBasemapStyle(flavor='dark')`
calls `bake(...)` which runs `bakeDarkColors` — a per-call deep-copy that walks
every layer's `*-color` paint value, recursing into data-driven expression arrays
and regex-testing/parsing every color string (`transformColorValue`,
`xformHex`) — **at runtime, on the UI thread, every time the style is built.**
With finding 1 that is 69×(packs+1) layers' worth of color transforms; with
finding 2 it runs again on the post-load `setStyle`; and again on every theme
toggle and every pack change. The work also allocates a fresh deep copy each
time.
**Impact:** A multi-hundred-millisecond main-thread stall (proportional to layer
count) on each dark-mode style build, on the Pi. Not per-frame, but it lands at
mount + on the redundant `setStyle` of finding 2, compounding the startup
stutter. Also a correctness/expectation gap: the "baked once" invariant the
tests and comments assert is false.
**Fix direction:** Actually bake at build time — precompute the dark layer array
once (module-level memo keyed by flavor, or emit it from the bundle build
script) and reuse it. At minimum memoize `bakeDarkColors(layers(...))` so repeat
builds reuse the result instead of re-walking every color.

### 4. `tuxlinkFlavor()` + `layers(...)` recomputed from scratch on every style build, once per pack
**Location:** `src/map/basemapStyle.ts:127` and `:146` (inside the pack loop); `tuxlinkFlavor.ts:69-71`
**Severity:** minor
**Evidence:** `buildBasemapStyle` calls `tuxlinkFlavor()` (spread-clone of
`namedFlavor('light')` + 40 overrides) and the Protomaps `layers(...)` generator
(builds 69 layer specs with their full data-driven paint expressions) once for
the base and **again inside the loop for every pack** (`:146`). None is memoized;
all of it is regenerated on every `buildBasemapStyle` call (mount, the redundant
post-load `setStyle`, every flavor/pack change).
**Impact:** Redundant allocation + object construction scaling with pack count,
stacked on findings 1-3 at exactly the moments the Pi is busiest. Smaller than
1-3 but the same class and trivially removable.
**Fix direction:** Memoize `tuxlinkFlavor()` (it is pure/constant) and the base
`layers(...)` result; generate pack layers by remapping the cached base set's
`source`/`id`/`minzoom` rather than re-invoking the generator per pack.

### 5. `onZoomChange` fires on every `moveend` (pan AND zoom), pumping React re-renders of the overlay + map subtree per pan
**Location:** `src/map/MapLibreMap.tsx:156,161` (`emitZoom` on `moveend`) → `src/compose/PositionPickerOverlay.tsx:65,121` (`onZoomChange={setViewZoom}`)
**Severity:** significant
**Evidence:** `emitZoom` is registered on `moveend` (`:161`), which fires after
**pan** as well as zoom. In the position picker it is wired straight to a
`useState` setter: `onZoomChange={setViewZoom}` (`PositionPickerOverlay.tsx:121`).
So every pan settles → `setViewZoom(sameOrNewZoom)` → re-render of
PositionPickerOverlay and its `<PositionMapWidget>` child. PositionMapWidget then
re-runs `gridToLatLon(grid)` and hands `initialCenter={ll}` a fresh object
(`PositionMapWidget.tsx:112,119`) every render (the flyTo effect is guarded by
primitive deps so no flyTo fires, but the reconciliation still runs). The setter
is called even when the integer/zoom value is unchanged after a pure pan.
**Impact:** React reconciliation of the modal + map container competes with the
rasterizer for the main thread at the end of every drag, adding input-to-paint
latency on the substrate. The map itself does no work, but the churn is pure
overhead during interaction. (StationFinderMap/LocationMap don't pass
`onZoomChange`, so they avoid this; it's specific to the position picker, the
heaviest map surface — modal + map + grid layer.)
**Fix direction:** Only emit when the zoom actually changes (track last emitted
zoom in a ref in MapLibreMap and short-circuit equal values), or move the
zoom-gate (`sixCharAllowed`) read off React state into a `zoom`-event ref read
at confirm time. Splitting `zoom` (zoom-only) from `moveend` for the
zoom-emission also avoids firing on pure pans.

### 6. MaidenheadGridLayer recomputes the full lattice + GeoJSON on every `moveend` and re-runs the push effect
**Location:** `src/map/MaidenheadGridLayer.tsx:96-105` (the `setTick` bump) → `:117-120` (recompute) → `:129-140` (push effect re-subscribes)
**Severity:** significant
**Evidence:** A `moveend` handler bumps a tick (`:101`) forcing a re-render; the
re-render reads `map.getBounds()`/`getZoom()` fresh (`:107-115`) and `useMemo`
rebuilds the lattice via `gridLines(...)` + `gridToGeoJSON(...)` whenever any
bound edge changes (`:117-120`) — i.e. on every pan. The push effect depends on
`geojson` (`:140`), so a new geojson object each move tears down and re-adds the
`styledata` listener and calls `setData` with a freshly-built FeatureCollection.
At low zoom over a wide viewport the lattice can be hundreds of line features +
label points, and the label layer has `text-allow-overlap:true` +
`text-ignore-placement:true` (`:53-54`) so EVERY label is rendered with no
collision culling.
**Impact:** Per-pan: rebuild a potentially large GeoJSON, push it (re-tessellate
lines + re-shape every label glyph on the CPU), and re-bind a listener. Labels
with overlap/ignore-placement forced on are especially costly on llvmpipe. This
runs in LocationMap (always visible, `LocationMap.tsx:199`) on every drag.
**Impact magnitude:** moderate-to-large at wide viewports/low zoom where the
feature count is highest; small when zoomed in. Compounds finding 5's churn on
the position surfaces.
**Fix direction:** Throttle/debounce the `moveend` recompute; skip the rebuild
when the derived `level` and rounded bounds are unchanged; cap label density
(don't force `text-allow-overlap`/`text-ignore-placement` on, or thin labels by
zoom). Avoid re-subscribing `styledata` on every geojson change (subscribe once,
read latest geojson from a ref).

## Design Concerns

- **The forecast's substrate gap is structural, not a single bug.** The 45 fps
  figure (`design doc:28`) was a baked-dark style at the light baseline in a
  front-end-only spike with no packs, no markers, no real tile decode. Findings
  1-4 are all "the real style is far larger / rebuilt more often than the spike's
  style," and 5-6 are "the real consumers do per-move work the spike's trivial
  scene didn't." Any perf budget should be re-measured against a style with N
  packs installed and the overlays mounted, not the overview-only scene.

- **No MapLibre render-tuning for a CPU rasterizer.** `MapLibreMap.tsx:130-145`
  sets no `fadeDuration: 0` (label fade animation runs extra frames),
  no `maxTileCacheSize` bound, and lets `devicePixelRatio` default (on a HiDPI
  Pi panel that quadruples the pixels llvmpipe must rasterize). For a
  software-GL target these are the standard mitigations and all are absent.
  Worth a deliberate low-power render config.

- **`maxZoom: 14` with overzoom of the z0-6 overview (`MapLibreMap.tsx:43`,
  `basemapStyle.ts:126`).** Outside any pack, the overview is overzoomed 8 levels;
  MapLibre re-tessellates/over-renders coarse geometry to fill high-zoom tiles.
  Combined with finding 1's globally-evaluated pack layers, the z>=6 regime is
  where the rasterizer is most loaded. Confirm the overzoom cost is acceptable or
  clamp overview layers' maxzoom and accept blank-above-coverage.

- **Idempotent re-add cost on every `styledata`.** The owned hooks
  (`mapHooks.ts`) re-run their `ensure` on every `styledata` (which `setStyle`
  fires). That is correct for lifecycle, but it means findings 1-3's redundant
  `setStyle` (finding 2) also pays the full overlay re-add + re-`setData` for
  every consumer each time. Reducing `setStyle` frequency (finding 2) is the
  lever that also shrinks this.

## Testing-pitfalls note

These are perf defects, not correctness ones, so most are outside the
correctness-test remit. But two are assertable cheaply and would have been
caught: (a) finding 1 — a unit test on `buildBasemapStyle(flavor, packs)`
asserting the *total layer count* and *label-layer count* stay within a budget
as packs are added would have flagged the per-pack full-set duplication; (b)
finding 3 — a test asserting `bakeDarkColors` / the dark layer array is computed
once (referential identity across repeat `buildBasemapStyle('dark')` calls)
would have caught the "baked at runtime, not build time" gap that the doc
strings assert is false. Adding a layer/label-count budget assertion to
`basemapStyle.test.ts` is the highest-value, lowest-cost guard.
