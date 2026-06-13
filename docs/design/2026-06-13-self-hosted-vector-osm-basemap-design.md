# Design — Self-hosted dark/light OSM vector basemap (meshmap.net look)

**Date:** 2026-06-13
**Agent:** mink-shoal-maple
**bd issue:** tuxlink-ndi4
**Status:** APPROVED (premises + renderer + imagery extension confirmed by operator; build phasing pending eng-review + Codex adrev)
**Code baseline:** `origin/main` (the map subsystem `src/map/` + `src-tauri/src/tiles/` and the #659 raster path live on `main`, merged commit `f3ba5bb9`; they are NOT present on the current `bd-tuxlink-xygm/recover-handoffs` working tree, which is a stale handoff-recovery branch — build this feature from a branch off `main`).
**Supersedes framing in:** handoff `dev/handoffs/2026-06-13-dahlia-spruce-osprey-lan-tiles-shipped-vector-map-design.md` Part 2 (two of its "confirmed facts" are corrected below).

---

## Problem statement

tuxlink needs its own dark/light OSM basemap that works fully offline with no
runtime cross-service dependency. The motivation is the 2026-06-13 LAN-tiles saga
(bd tuxlink-k61j): consuming an external raster tile server for the basemap is
fragile and coverage-gapped. The target aesthetic is meshmap.net's dark map.
Satellite imagery is out of scope for the *self-contained* basemap, but the design
must leave a clean path to an optional imagery/hybrid overlay served from a local
tileserver (see §Imagery extension).

## What meshmap.net actually is (source-verified correction)

The opening move was to pin meshmap.net's style from its source. The result overturns
a load-bearing assumption carried in the prior handoff.

The handoff recorded as a "confirmed fact" that meshmap.net is "MapLibre + an open dark
vector style over OSM vector tiles." That is incorrect. Verified against both
`brianshea2/meshmap.net` (`website/index.html`) and the live site:

- **Renderer:** Leaflet 1.9.4. No MapLibre, no vector tiles.
- **Tiles:** raster, `https://tile.openstreetmap.org/{z}/{x}/{y}.png`.
- **Dark look:** one CSS rule — `body.dark { filter: invert(1) hue-rotate(180deg) brightness(1.33); }`.

Three consequences shape the design:

1. **The look is free and renderer-agnostic.** A CSS `filter` applies to the rendered
   element (raster `<img>`, a 2D canvas, or a WebGL canvas alike). The meshmap dark
   aesthetic is one CSS rule on the basemap rendering surface.
2. **meshmap does not self-host.** It streams raster tiles from `tile.openstreetmap.org`
   at runtime. "No cross-service dependency" and "offline" are tuxlink's own
   requirements, separate from "the meshmap look," and are where the cost lives.
3. **Renderer choice is driven by storage, not appearance.** Regional raster at z0–14
   is many gigabytes; the same region as vector PMTiles is hundreds of megabytes.
   Vector is selected for an offline-first bundle.

The operator's prior rejection of geographica's `darkmatter` style is consistent:
`darkmatter` is a hand-authored dark vector style (Google-dark in character). The
meshmap look is not an authored style — it is an inversion of the ordinary light map.

### What the mock does and does not prove

`dev/scratch/meshmap-look-mock.html` / `.png` renders the light map and the inverted
dark map side by side with realistic station pins. It proves **the aesthetic** (what
`invert(1) hue-rotate(180deg)` does to OSM colors) and **the view-mode concept**. It
runs on a raster `L.tileLayer` in desktop Chromium; it does **not** prove the filter's
behavior or performance on the selected renderer's canvas, nor on WebKitGTK/Pi-5. That
remains a gating spike (R4).

## Confirmed facts retained (not re-litigated)

- The Pi/Geographica serves vector OSM tiles today (probe-verified); not in scope to
  change.
- The operator rejects geographica's `darkmatter` style.
- Emulating an open dark map over OSM data is legally clean (OSM is ODbL); the sole
  runtime obligation is retaining "© OpenStreetMap contributors" attribution.

## Renderer decision — MapLibre GL JS

**Selected: MapLibre GL JS** (full renderer swap from Leaflet).

The first renderer pick in this session was `protomaps-leaflet` (to stay in Leaflet).
That was reversed on a material fact: per Protomaps' own docs, `protomaps-leaflet` "is
in maintenance mode," is "recommended only for legacy projects … otherwise use MapLibre
GL JS," and is "designed for non-interactive layers, because it renders vector tiles to
Canvas (image) elements." Adopting a maintenance-mode, non-interactive-optimized
renderer as the foundation of a pan/zoom basemap is the wrong bet.

The countervailing argument for staying in Leaflet was "preserve the hardened `tile://`
stack." That advantage is smaller than it looked: the raster basemap path is being
retired anyway (see §Retirement & repurposing), so most of that subsystem changes role
regardless. What a Leaflet renderer would have preserved is narrower — the
Leaflet-specific pin/marker/pane code — and that is the only thing MapLibre forces a
rewrite of.

MapLibre advantages that decide it: actively maintained; the renderer Protomaps
recommends; native PMTiles via the `pmtiles` protocol; `@protomaps/basemaps` ships
ready-made light/dark style flavors for the Protomaps schema; GPU/WebGL rendering
(favorable for both map redraw and a full-canvas CSS filter); and a single
sources+layers style model that makes the imagery/hybrid extension natural (see
§Imagery extension). It is also exactly the stack geographica already runs.

## Storage-size correction

The operator's working figure ("global vector is only hundreds of MB") holds for a
single region, not the planet. Published Protomaps PMTiles sizes (OSM):

| Coverage | Max zoom | Size |
|---|---|---|
| World overview (bundled) | z0–6 | ~30–60 MB |
| One US state (region pack) | z0–14 | ~hundreds of MB |
| Whole USA | z0–14 | ~15–25 GB |
| Full planet (reference only) | z0–15 | ~120 GB |

The download model is therefore **per-region packs** (each hundreds of MB). The
full-planet download is **out of scope** (YAGNI for EmComm; it drives most of the
disk/concurrency/range complexity for a path no operator realistically uses) — noted as
a possible future, not built.

## Premises (operator-confirmed 2026-06-13, as amended)

1. The meshmap look is achieved by a CSS invert/hue-rotate filter applied to the
   MapLibre canvas in **dark-vector view mode**. DOM pins, UI controls, and attribution
   live outside the canvas and are unaffected. MapLibre's vector **labels are GL-drawn
   inside the canvas, so they invert with the basemap** — this is intended and matches
   meshmap (labels read correctly against the inverted map). Imagery is its own mode and
   never sits under the filter.
2. Renderer: **MapLibre GL JS** (full swap from Leaflet).
3. Tile format: **PMTiles**. Loaded into MapLibre via the `pmtiles` protocol. Local
   archives are read by a custom byte-range Source (Tauri IPC) as the primary path; a
   `tile://` 206/range-read path is an optimization to spike, not a prerequisite.
4. Coverage: bundle world z0–6 (~30–60 MB) with the app; the operator downloads
   per-region vector packs (z0–14, ~hundreds of MB each) as **permanent** resources
   (not a cache; explicit list/delete). Full-planet download is out of scope.
5. The #659 **raster basemap** path is retired *as the basemap*. Its raster-tile
   transport (the `tile://` scheme, SSRF/host pinning, on-disk cache, circuit breaker,
   status) is a **candidate to repurpose** as the optional imagery/hybrid overlay
   transport rather than delete. Whether to park it for the deferred imagery increment or
   delete now and rebuild later is an explicit eng-review decision (§Imagery extension).
   The Leaflet renderer and its Leaflet-specific code are replaced by MapLibre regardless.
6. Styles: light = `@protomaps/basemaps` `namedFlavor('light')`; dark = the same light
   style with the meshmap CSS filter; imagery/hybrid = a raster imagery source + vector
   labels (no filter). "© OpenStreetMap contributors" attribution retained always.

## Architecture

### Renderer & view modes
MapLibre GL JS mounts in the map container, replacing the Leaflet `MapContainer`,
`ImageOverlay`, `TileLayer`, the custom pane stack, the `divIcon` pins, and the
`RASTER_MAX_ZOOM` / `ApplyMaxZoom` / `RecenterOnOperator` Leaflet machinery. Station
pins and the operator-location marker are re-implemented as `maplibregl.Marker`
elements with click handlers; recenter uses `map.flyTo`/`setCenter`. This is not a
free 1:1 port — three Leaflet-specific details must be re-earned (call out in build
phase 2):
- **Operator-pin z-order.** Leaflet uses `zIndexOffset={1000}` to float the operator
  pin above stations; `maplibregl.Marker` has no `zIndexOffset` and re-sorts HTML
  markers by latitude on move. Pin the operator marker above via explicit
  `element.style.zIndex` and confirm MapLibre's marker sort does not override it.
- **Dot sizing.** The current `divIcon` dots size via the `iconSize`→CSS-from-CSSOM
  trick (the `tuxlink-s0r1` CSP-safe approach, no inline `style=`). MapLibre markers
  take a raw element with their own anchor model; the CSP-safe sizing must be re-proven
  on WebKitGTK, not assumed to transfer.
- **Async-arrival recenter.** `RecenterOnOperator`/`ApplyMaxZoom` existed because
  react-leaflet props are non-reactive; the "data arrives after mount" problem needs an
  equivalent effect driving `flyTo` in the MapLibre component.

Three view modes, switchable by swapping the MapLibre style (geographica's
positron/darkmatter/hybrid pattern):

- **Light vector** — `namedFlavor('light')` over the PMTiles basemap. No filter.
- **Dark vector** — the light style with a CSS `filter` on the MapLibre canvas. This is
  the meshmap look. In-canvas vector labels invert with the map (intended); DOM pins/UI
  do not. Because imagery is its own mode (below), the filter never inverts a photo.
- **Imagery / hybrid** — a raster imagery source (local tileserver) + vector label
  layers on top. No filter. (See §Imagery extension; ships after the vector modes.)

The selected mode persists in config.

### Dark/light mechanism
Dark mode applies one CSS rule to the MapLibre canvas element, scoped to dark-vector
mode only:

```css
.tux-mode-dark .maplibregl-canvas { filter: invert(1) hue-rotate(180deg) brightness(1.33); }
```

Canonical filter string starts from meshmap's exact `invert(1) hue-rotate(180deg)
brightness(1.33)` and is tuned during implementation against the real MapLibre canvas
on WebKitGTK (not the raster mock — see R4). The earlier mock's
`brightness(1.05) contrast(.95)` was an exploratory tweak, not canonical.

### PMTiles loading & serving
- MapLibre style sources reference `pmtiles://<id>`; `maplibregl.addProtocol('pmtiles', …)`
  from the `pmtiles` lib resolves ranges.
- **Primary path:** a Tauri IPC command `pmtiles_read_range(archive_id, offset, length)`
  reads a byte range from a local PMTiles file (bundled resource or downloaded pack).
  This is local file I/O — no network egress — and is the well-trodden fit for the
  `pmtiles` lib's custom `Source` interface. R1 (below) is the spike to confirm wiring
  a custom Source to Tauri `invoke`.
- **Optimization (spike, not required):** teach the `tile://` handler to serve `206
  Partial Content` so range reads avoid IPC overhead. The current handler
  (`src-tauri/src/lib.rs` scheme registration) always returns a full `200` body and has
  no `Range` support — this is net-new, not "reuse." Treat it as an optimization after
  the IPC path works.
- **Concurrency:** MapLibre issues many concurrent range reads while panning. The range
  reader caps concurrent file reads and keeps a small per-archive file-handle/mmap so a
  large pack does not exhaust handles.

### Coverage, download, and pack management
- **Bundled:** world z0–6 PMTiles in app resources (read-only). Guarantees a non-blank
  world map on first launch, offline, no config. MapLibre overzooms it past z6 (low
  detail but present everywhere).
- **Region packs (z0–14):** downloaded into the persistent app-data packs directory and
  recorded in a manifest. Where an active pack has coverage, it is the detailed source;
  elsewhere the overview shows. The exact compositing (two vector sources with
  layer-source filtering vs viewport source-switching vs a merged style) is an
  implementation choice spiked in build phase 4 (R7); the required *behavior* is "never
  blank; full detail in downloaded regions."
- **Source = curated catalog + advanced custom URL.** Because the `@protomaps/basemaps`
  flavors require the **Protomaps basemap schema** (see R3), the default UX is a curated
  catalog of region presets (state/country) pointing at known-schema PMTiles (tuxlink-
  pinned or a pinned Protomaps build). An "advanced: custom URL" option exists, but a
  custom pack is **schema-validated on download** and rejected with a clear error if it
  is not a Protomaps-schema basemap (otherwise the style renders blank/garbled).
- **Download robustness:** pre-flight free-space check (reject if insufficient);
  download to a temp file; validate (PMTiles magic header + metadata schema check);
  atomic rename into the packs directory; only then register in the manifest. On
  startup, sweep and delete any orphaned temp/partial files so a half-written `.pmtiles`
  is never registered or read.
- **Pack manager UI:** list installed packs with size + coverage; delete a pack (frees
  disk); shows total disk used. "Permanent resource, not a cache" means no automatic
  eviction — removal is an explicit operator action.
- **Manifest data model:** a JSON manifest in app-data, one entry per pack:
  `{ id, name, bbox, minzoom, maxzoom, schema, bytes, source_url, installed_at }`.

### Retirement & repurposing (precise blast radius)
The existing map/tile code on `main` is two distinct things; prior framing conflated
them:

- The **always-present bundled world raster** — `BaseMap.tsx` renders
  `world-mercator-2048.png` as an `ImageOverlay` in the `tux-raster-base` pane (z100),
  independent of any LAN source. **Replaced** by the bundled z0–6 PMTiles vector
  overview.
- The **optional LAN raster tile source** (#659) — `TileLayerBridge.tsx`,
  `useTileSource.ts`, the `map_tile_source` config key, and the `src-tauri/src/tiles/`
  subsystem (`fetch.rs` SSRF gatekeeper, `host.rs`, `cache.rs` LRU + `cache_budget_mb`,
  `breaker.rs` circuit breaker, single-flight de-dup, `commands.rs`
  `tile_source_status` + the `lan-live`/`lan-cached`/`partial`/`unreachable`/
  `incompatible` `StatusKind` union). **Retired as a basemap.** The Leaflet-specific
  bridge component and the basemap-source framing go away. The Rust raster-tile
  transport (scheme + SSRF/host pinning + cache + breaker + status) is a **candidate to
  retain** for the imagery overlay — see §Imagery extension for the exact (non-trivial)
  changes that "retain" actually requires; it is not pure reuse.
- **Config migration (C6):** existing `map_tile_source` (raster basemap source) entries
  are migrated to the new imagery-overlay source setting, or cleared with the operator's
  setting carried over to the imagery picker. No silent orphan.
- **Deps:** remove `leaflet` / `react-leaflet` (and their usage); add `maplibre-gl`,
  `pmtiles`, `@protomaps/basemaps`.

## Imagery extension (designed-for, ships after the vector modes)

The operator wants the option to layer high-detail satellite imagery from a local
tileserver (Geographica `:8090`, `/tiles/data/imagery/{z}/{x}/{y}.jpeg`) on top of the
self-contained vector basemap. MapLibre models this as a raster source + raster layer:

```js
map.addSource('geo-imagery', { type:'raster',
  tiles:['tile://localhost/imagery/{z}/{x}/{y}.jpeg'],   // routed through the tuxlink scheme, NOT a direct http URL
  tileSize:256, maxzoom:18, attribution:'…' });
map.addLayer({ id:'imagery', type:'raster', source:'geo-imagery' });
```

Design rules so this stays clean:

- **Imagery is a view mode, not a layer stacked under the dark filter.** Inverting a
  WebGL canvas that contains a photo inverts the photo. The hybrid mode (imagery +
  labels) carries no filter; the invert filter is exclusive to dark-vector mode.
- **Two transport options — pick one at eng-review (they are mutually exclusive):**
  1. *Route through the retained `tile://` scheme* (recommended for LAN safety). The
     MapLibre raster source points at `tile://…`, so imagery inherits #659's SSRF/host
     pinning + cache + breaker + CSP posture for `http://pandora.local:8090`. **But this
     is not pure reuse:** the existing handler is raster-`.png`-only — `serve.rs`
     `parse_zxy` strips only `.png`, and `fetch.rs` `build_tile_url` base-dir form
     hardcodes `{y}.png`. Retaining the transport for `.jpeg` imagery requires
     generalizing both to a configurable extension + URL template. Net-new work on a
     retained subsystem, honestly labeled.
  2. *Direct MapLibre HTTP source* (`tiles:['http://pandora.local:8090/…{z}/{x}/{y}.jpeg']`).
     Simpler, no scheme changes — **but it bypasses the SSRF/host pinning/cache/breaker
     entirely** and requires the webview CSP to allow the LAN origin. Loses the #659
     safety posture.
  Recommendation: option 1 (safety + CSP control consistent with #659's design), with
  the `.jpeg`/template generalization scoped as explicit work. If eng-review picks
  option 2, then the #659 Rust transport has no remaining consumer and should be
  **deleted** (not parked) per the project's aggressive-deletion posture — making the
  park-vs-delete decision in premise 5 fall out of this choice.
- **Self-containment preserved either way.** The vector base needs no service; imagery is
  opt-in and only live when the tileserver is reachable. MapLibre can equally consume
  geographica's *vector* overlays (publiclands, hillshade) as future sources if wanted.

The initial feature ships light + dark vector modes (self-contained). Imagery/hybrid is
the next increment; the transport decision above determines whether the #659 Rust code
is retained (and generalized) or deleted now.

## Open questions / risks (verify before or during build)

- **R1 — PMTiles range reads via a Tauri-backed `pmtiles` Source (primary path).**
  Confirm the `pmtiles` lib's custom `Source` interface wires cleanly to a Tauri
  `invoke` byte-range command and feeds MapLibre via `addProtocol`. Gating spike.
- **R3 — Schema lock.** `@protomaps/basemaps` flavors require the Protomaps basemap
  schema. The bundle and every region pack (catalog or custom) must match. The concrete
  download-time check: read the PMTiles JSON metadata header and verify its
  `vector_layers` contains the Protomaps basemap layer ids (the `earth`/`water`/
  `landuse`/`roads`/`transit`/`boundaries`/`places`/`pois`/`buildings` set — pin the
  exact id list against a known-good Protomaps build before coding the gate). Reject
  packs whose metadata lacks them. Pin the build for catalog packs. Hard dependency, not
  a footnote; without this the gate is aspirational.
- **R4 — WebKitGTK/Pi-5 performance (gating spike, alongside R1).** A full-canvas CSS
  `invert()+hue-rotate()` over a live-repainting MapLibre WebGL canvas, on WebKitGTK on
  a Pi 5. GPU rendering is favorable, but the project's own rule ("Chromium is not a
  WebKitGTK proxy") means this must be measured on the target, not inferred from the
  desktop raster mock. Tune the canonical filter values here.
- **R5 — Download-source validation.** The custom-URL download leg reuses host/SSRF
  validation, but the existing image-magic-byte check and `MAX_TILE_BYTES` size cap in
  `fetch.rs` are wrong for a multi-hundred-MB PMTiles archive — the download path needs
  its own validation (PMTiles header + schema + size budget), distinct from the per-tile
  raster fetch. Serving ranges from a local file involves no network egress.
- **R7 — Overview+region compositing.** Decide how the global z0–6 overview and a z0–14
  region pack co-render (dual source + source-filtered layers, viewport source-switch,
  or merged). Required behavior: never blank; full detail in downloaded regions.
- **R-RADIO — none.** Map rendering only; no transmission path is touched (RADIO-1
  n/a).

## Success criteria (definition of done — must be reachable in a real build via the UI)

1. First launch, no config, no network: app renders a **light vector** world basemap
   from the bundled z0–6 pack.
2. A light/dark mode toggle applies the meshmap filter to the basemap canvas only in
   dark-vector mode; pins, labels, and UI are unaffected; the choice persists across
   restarts.
3. The operator downloads a region pack from the catalog (or a schema-valid custom URL)
   through the UI; it persists permanently; the map shows full offline detail in that
   region; a pre-flight space check and atomic install prevent corrupt partials.
4. The operator can list and delete installed packs, with disk usage shown.
5. The raster **basemap** path and its Settings surface are removed/migrated; the raster
   transport is retained for imagery (even if the imagery mode UI ships in the next
   increment).
6. "© OpenStreetMap contributors" attribution is shown in all modes.

(Imagery/hybrid mode end-to-end is the success criterion for the follow-up increment,
not the initial ship.)

## Distribution

The world z0–6 PMTiles (~30–60 MB) bundles into the Tauri app resources and the `.deb`
(note the package-size increase). Region packs download at runtime into the persistent
app-data directory; they are not part of the package.

## Dependencies / sequencing

- Add: `maplibre-gl`, `pmtiles`, `@protomaps/basemaps`. Build tooling: `pmtiles` CLI
  (`pmtiles extract` for the bundle + catalog region packs from a pinned planet build).
- Remove: `leaflet`, `react-leaflet`, the Leaflet map components, the raster *basemap*
  path. Repurpose: the `src-tauri/src/tiles/` raster transport for imagery.
- This supersedes the handoff's "reuse geographica's tippecanoe→MBTiles pipeline":
  geographica's pipeline targets MapLibre + MBTiles + a hand-authored style; the PMTiles
  + `@protomaps/basemaps` path uses Protomaps tooling instead. The reused *concept* is
  "build vector tiles from OSM, ship light/dark flavors."
- Gates per project policy: eng-review on this architecture, then the cross-provider
  Codex adversarial review at build (build-robust-features, no-carveout rule). This doc
  is the spec input.

## Suggested build phasing (for eng-review / writing-plans)

1. **Spikes (gate):** R1 (Tauri-backed `pmtiles` Source into MapLibre) and R4
   (WebKitGTK/Pi-5 canvas + filter perf). Settle the IPC-vs-206 path and the canonical
   filter values.
2. MapLibre mounts; render bundled world z0–6 (light) replacing the Leaflet basemap;
   re-implement pins + operator-location as MapLibre markers; remove Leaflet deps.
3. Light/dark mode toggle via the canvas filter; persist.
4. Region download manager: catalog + schema-validated custom URL; pre-flight space,
   temp+atomic install, manifest, list/delete; overview+region compositing (R7).
5. Migrate/remove the raster basemap Settings surface; park the raster transport;
   attribution; docs.
6. (Next increment) Imagery/hybrid mode on the parked transport.

## What was noticed this session

- The operator framed the goal as "emulate meshmap.net," and reading meshmap's source
  literally dissolved an expensive assumption (a full vector-style swap) into a one-line
  CSS filter plus a storage-motivated vector switch.
- "Not as a cache — as a persistent permanent resource" precisely shaped the download
  model: operator-chosen permanent packs with explicit delete, not eviction-prone
  caching.
- The operator's imagery question ("can we still ingest Geographica tiles?") turned a
  planned deletion into a repurposing: #659's raster transport becomes the imagery
  overlay path, and the three-view-mode model keeps the dark filter from ever inverting
  a photo.
