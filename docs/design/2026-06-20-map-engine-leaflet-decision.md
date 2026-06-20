# Map-engine decision: adopt Leaflet + protomaps-leaflet (tuxlink-ez7t)

**Date:** 2026-06-20 · **Status:** PROPOSED (pending plan-eng-review) · **Agent:** plover-bison-delta
**Supersedes the engine choice of:** `docs/design/2026-06-13-self-hosted-vector-osm-basemap-design.md` (ndi4)
**Relates to:** tuxlink-ez7t (this decision), tuxlink-s587 (WX chip rebuild, gated on this), tuxlink-k0zz / PR #838 (fade trace, rebuild in chosen engine)

## Decision

Replace MapLibre GL JS with **Leaflet + protomaps-leaflet** as the single map engine across all map surfaces, **retaining the existing vector PMTiles offline tile stack unchanged.**

## The decision criterion

The map subsystem renders on a Raspberry Pi 5 under WebKitGTK with **software OpenGL (llvmpipe)** — no usable GPU acceleration in this environment. MapLibre GL is a WebGL engine, so on this host the CPU emulates a GPU pipeline in software. That path is the source of the recurring map failures (the "TV static" DMA-BUF plague, the WebGL compositing-readback failure, the historical drunk-map and animation pegs).

The ndi4 migration selected MapLibre for one load-bearing reason: **vector tiles.** Regional raster at z0–14 is many gigabytes; the same coverage as vector PMTiles is hundreds of megabytes. The Pi may also host other services, and the deployment audience cannot be assumed to have terabyte-class storage. Vector storage is therefore a hard requirement, and the real question for any alternative engine is singular:

> Can a non-WebGL engine render the project's existing vector PMTiles on this hardware, preserving the disk-efficient vector stack?

## Evidence (measured this session)

A standalone spike harness rendered the project's **actual** vector PMTiles (`@protomaps/basemaps`-schema, a real Phoenix z0–13 region extract) in real WebKitGTK 4.1 on this Pi, with `WEBKIT_DISABLE_DMABUF_RENDERER=1`.

**Confirmed:**
- **Vector render:** protomaps-leaflet@5 (`flavor: 'dark'`) renders the vector tiles to Canvas2D as a correct dark street grid — roads, water, landuse. No raster conversion. The vector PMTiles backend is reused unchanged.
- **Performance, quiet Pi (load < 3):**
  - First canvas paint: **~100 ms**
  - Drag (dispatched-pointer, 142 rAF frames): **median 16 ms / 63 fps**, p95 25 ms
  - Animated fade-trace, proper single-canvas implementation (374 frames): **median 18 ms / 56 fps**, ~0.8 of one core while animating, settling to ~1% when stopped
  - Idle CPU (static map): **~1–2% of one core**
- **Dark mode** is produced by paint-rule colors, not a CSS filter — eliminating the per-pixel CPU filter pass that the ndi4 R4 spike measured at ~15 fps on a Leaflet+raster surface.
- **Capability:** vector basemap, HTML chip markers, polylines, and a 56 fps animated fade-trace all render — covering the cn84/k0zz animated-path case that was reverted on MapLibre.

**Not established (stated plainly):**
- **No head-to-head perf comparison against MapLibre exists.** The MapLibre twin could not be rendered in the standalone harness: WebGL content fails to composite under WebKitGTK on this host (`glReadnPixelsRobustANGLE: INVALID_OPERATION` on buffer readback with DMA-BUF disabled; the whole page is "TV static" with it enabled). The real Tauri app renders MapLibre via its own WebKit GL configuration, which a bare harness does not replicate. The decision therefore does **not** claim Leaflet is faster than MapLibre — only that it renders the vector tiles and performs well, while avoiding the WebGL fragility entirely.

## Why this is not a re-litigation of ndi4

The ndi4 migration closed the Leaflet door **by assumption, not measurement**: protomaps-leaflet was rejected solely on its documentation ("maintenance mode," "designed for non-interactive layers"), and the only Pi measurements taken (the ~15 fps / ~45 fps figures) compared **dark-mode mechanisms within MapLibre** (CSS filter vs baked-GL), never Leaflet against MapLibre. The "non-interactive" label was also misread: it means the basemap canvas exposes no per-feature mouse events, not that it cannot pan or zoom — irrelevant to a basemap carrying its own marker and line layers. This decision replaces that assumption with measurement.

## Caveats and risks

- **protomaps-leaflet is in maintenance mode.** No new features will be added upstream. Mitigation: it is MIT-licensed and ~120 KB, the spike proves it renders the project's tiles today, and the fork-and-own posture of [ADR 0011] makes vendoring it acceptable if upstream stalls.
- **Continuous animation is not free.** A persistent always-on animation holds ~0.8 of one core. Transient traces (play, then stop) idle back to ~1%. Designs should prefer transient animation.
- **Perf is "good," not "proven faster."** The motivation is robustness (off the WebGL-on-llvmpipe failure class) plus native 2D drawing for chips and traces — not a demonstrated speed win.
- This does **not** bring the Pi 5 GPU online. Both engines render on the CPU here; the change replaces software-GL emulation (expensive, fragile) with native Canvas2D (lighter, robust). Bringing WebKitGTK GPU acceleration online is a separate effort that would instead favor MapLibre, and is out of scope.

## Migration scope

- **Frontend (~3,800 LOC, rewritten):** replace the MapLibre substrate (`MapLibreMap`, `mapHooks`, `basemapStyle`/`darkStyle`/`tuxlinkFlavor`) with Leaflet + protomaps-leaflet; rewrite all five surfaces (`AprsPositionsMap`, `StationFinderMap`, `LocationMap`, the compose position picker, `GridPickerOverlay`) and the Maidenhead grid layers. Pins → Canvas markers, chips → HTML `divIcon`, grid → polygon layers, fade traces → a single-canvas overlay (measured at 56 fps).
- **Backend (~8,000 LOC, untouched):** PMTiles serve, region-pack download, manifest, and the settings UI all remain. protomaps-leaflet reads PMTiles natively.
- **Dependencies:** drop `maplibre-gl` and the `@protomaps/basemaps` GL flavor; add `leaflet` and `protomaps-leaflet`.
- **Dark mode:** protomaps-leaflet's `dark` flavor replaces the baked-GL invert. The stock dark flavor is closer to the MeshMap-dark north star than the bold `tuxlinkFlavor` + bake-invert, which is recorded as having derailed that target.

## Downstream (unblocked by this decision)

- **tuxlink-s587** — rebuild the WX badge as HTML `divIcon` chips to the approved mocks; re-audit the whole WX display.
- **tuxlink-k0zz / PR #838** — close the MapLibre fade-trace PR; rebuild the trace as a single-canvas overlay (the 56 fps approach measured here).
- The cn84 backend via-chain (`decode_digi_hbits`, `InboundPos.via`) is engine-agnostic and stays.

## Alternatives considered

- **B — Leaflet + server-side raster:** rejected. Re-introduces the multi-GB raster storage cost that vector was chosen to avoid, and adds an ARM software-GL rasterizer (build risk) plus an install-time render step. The spike showed in-browser vector rendering is viable, so raster is unnecessary.
- **C — stay on MapLibre:** the status quo. The WebGL-on-llvmpipe failure class is structural; each new map feature re-encounters it.
- **Purpose-built 2D canvas engine from scratch:** rejected. With one engine required across all surfaces (including wide-area pan/zoom for the station finder and position pickers), a hand-rolled renderer would become a general slippy-map engine — the sinkhole this evaluation was scoped to avoid.

## Next step

plan-eng-review on the migration plan (phasing across the five surfaces, the dark-flavor cutover, the s587 and k0zz rebuilds, and the protomaps-leaflet vendoring posture), then execution.
