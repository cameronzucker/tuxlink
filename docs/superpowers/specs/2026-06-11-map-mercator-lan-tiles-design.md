# Map: ingest standard Web Mercator (EPSG:3857) LAN tiles (design)

**bd:** tuxlink-7h2m · 2026-06-11 · branch `bd-tuxlink-7h2m/mercator-lan-tiles`
**Supersedes the CRS decision in:** `docs/design/2026-06-08-offline-map-foundation-approach.md` §1 ("Why EPSG4326 not … EPSG3857") and `docs/plans/2026-06-09-dyop-lan-tiles-plan.md` ("BaseMap stays EPSG:4326; the source MUST serve geodetic tiles and is rejected on CRS mismatch").
**Memory:** `project_lan_tiles_mercator_ok`.

## Problem

The Request Center / station map / position picker can't use the operator's self-hosted tile server. Configuring a tile URL (Geographica, `pandora:8090`) appears to "validate," but no map renders its tiles and zoom stays capped at the coarse bundled raster. Investigation (2026-06-11) found three defects, rooted in one wrong decision:

1. **Wrong CRS rule.** The dyop spec made `BaseMap` EPSG:4326-only and the gatekeeper reject Web Mercator. That was a misinterpretation of the operator's actual intent: "never use public OSM servers like the lazy WLE implementation" means **don't bake public-tile-server abuse into the product** — NOT "refuse Web Mercator" or "no network ingestion." Self-hosted/LAN servers serving standard 3857 XYZ tiles (the universal tile format) are the **intended** case. The real control is the **LAN-only / SSRF host gatekeeper**, not the coordinate system.
2. **Validation false-accepts.** `src-tauri/src/tiles/crs.rs::probe_source_crs` derives its metadata URLs by appending to `source.url` raw — but `source.url` is the XYZ template `…/{z}/{x}/{y}.png`, so it fetches `…/{z}/{x}/{y}.png/tilejson.json`, `…/{z}/{x}/{y}.png?SERVICE=WMTS…`, all garbage/404. It never reaches the real metadata, returns `Unknown`; and `MapTileSourceSettings.tsx:94` hardcodes `crs:'Geodetic'` (the only enum variant), which is the "operator asserts geodetic" override that flips `Unknown` → accept → `LanLive`. So any reachable XYZ source false-validates, hiding the CRS mismatch.
3. **Consumers never wired.** No `BaseMap` consumer passes the `tileSource` prop, so `maxZoom` is pinned at 2 regardless of config. (Tracked: tuxlink-n6xu, tuxlink-24px — absorbed here.)

## Corrected intent (premises, operator-confirmed 2026-06-11)

1. The LAN-only/SSRF host gatekeeper STAYS as the "no public-OSM-abuse" control. The CRS restriction is removed.
2. tuxlink accepts standard Web Mercator (3857) XYZ tiles from a self-hosted/LAN source.
3. The bundled offline fallback STAYS — tiles are opt-in; the app is fully functional offline with zero config and never reaches a public server.
4. The Maidenhead grid / GRIB / click-to-locate keep working (they already are CRS-agnostic — see Blast radius).

## Decision: Always EPSG:3857 + new bundled base

The map switches to a single CRS — `L.CRS.EPSG3857` (Web Mercator, Leaflet's default, the standard slippy-map projection). The bundled offline base is regenerated as a low-zoom **Web-Mercator** world raster so the offline fallback still works. One CRS everywhere, standard tiles render natively, no dual-path complexity.

### Approaches considered

- **A — Dual-CRS, source-driven:** 4326+equirect when no source, 3857+tiles when a source is active (remount map on source change). Keeps the existing equirect base pristine. Rejected: two CRS code paths + remount-on-switch; more state to reason about than the single-CRS model.
- **B — Always 3857 + new base (CHOSEN):** one CRS; regenerate the offline base as a Mercator raster. Cons accepted: regenerate+verify one asset; Mercator clips poles at ±85.05° (irrelevant for EmComm/US use); offline fallback is low-zoom only (acceptable — the offline base is a "you have no tile server" backstop, not a precision tool).
- **C — Keep 4326, reproject 3857 tiles server-side:** rejected on the record. Per-tile reprojection is expensive, lossy, complex, and essentially unshipped anywhere.

## Blast radius

Verified against `origin/main` (adversarial review 2026-06-11 corrected an earlier under-count — see the `coord.rs` item, which is load-bearing):

**Changed:**
- `src/map/BaseMap.tsx` — `crs={L.CRS.EPSG3857}`; base raster + bounds. `WORLD_BOUNDS` (`[[-90,-180],[90,180]]`) must become the Mercator extent `[[-85.0511,-180],[85.0511,180]]` for `maxBounds` (Leaflet clips ImageOverlay gracefully, but `maxBounds` should match Mercator's ±85.0511° limit).
- **`src-tauri/src/tiles/coord.rs` — MANDATORY, was missed.** `TileCoord::new` enforces `x < 2^(z+1)` (WorldCRS84Quad/4326: 2 tiles wide at z0). Under EPSG:3857 (WebMercatorQuad: 1 tile wide at z0) it MUST be `x < 2^z`. Left unfixed, tile-coordinate validation is too permissive at every zoom and the eastern hemisphere serves wrong/blank tiles (Leaflet requests standard slippy `x < 2^z`; the upstream 404s on over-bound x). Update the module header ("serves ONLY geodetic … WorldCRS84Quad"), the bound, and the `coord.rs` test fixtures that lock the geodetic counts.
- The Rust gatekeeper (`commands.rs` status mapping) — accept Mercator (see CRS-gate decision below).
- The bundled base asset (Mercator raster).
- The 4 consumer wirings (`StationFinderMap`, `GridMapPicker`, `PositionMapWidget`, `PositionPickerOverlay`).

**Verified CRS-agnostic (no change needed):**
- `src/map/projection.ts::pixelToLatLon` / `latLonToPixel` have **zero live callers** (vestigial; only `projection.test.ts` + a `crs.rs` doc-comment reference them). Live exports `WORLD_BOUNDS` + `clampLatLon` are used only in `BaseMap.tsx`.
- `src/map/gridGeometry.ts` + `MaidenheadOverlay.tsx` emit lat/lon line/marker positions in **degrees**; Leaflet projects them through the active CRS (confirmed: `Polyline`/`Marker` take `[lat,lon]`). Same for GRIB bbox + click-to-locate. Lines of constant lat/lon correctly converge toward the poles under 3857 — desired behavior.
- `src-tauri/src/tiles/host.rs` / `fetch.rs` (the SSRF/LAN egress policy — the real control) are CRS-independent; the gate inversion does not touch them.

## Work breakdown (feeds writing-plans)

1. **Bundled Mercator base asset.** Regenerate `src/map/assets/` world raster in Web Mercator (e.g. Natural Earth II → `gdalwarp -t_srs EPSG:3857` → resize → optimize, mirroring the existing `scripts`-driven asset note + CREDITS provenance). Bounded ≤ ~1.5 MB. Covers the Mercator world to ±85.05°.
2. **`BaseMap` → EPSG:3857.** `crs={L.CRS.EPSG3857}`; base `ImageOverlay` uses the new Mercator raster over the Mercator world bounds (or a `TileLayer` of the bundled base if tiled). Keep the `tileSource`-raises-`maxZoom` logic; drop the 4326-specific `WORLD_BOUNDS`/equirect assumptions. Verify pan/zoom/click via grim on WebKitGTK (per the existing C1 rule — jsdom can't render Leaflet).
3. **Fix `coord.rs` for WebMercatorQuad (MANDATORY — see Blast radius).** Change `TileCoord::new`'s x bound from `2^(z+1)` to `2^z`; update the module header, comments, and the geodetic-count test fixtures. Without this, tile serving is wrong at every zoom.
4. **DELETE the CRS gate (resolved open question).** Keep `host.rs` (private-IP/SSRF policy — the real and only control) untouched. **Delete** `crs.rs` entirely (the `probe_source_crs` probe, the `geodetic_tile_index` alignment helper, and its ~500 lines of 4326-specific tests — all dead once Mercator is valid; there is nothing meaningful left to reject on CRS grounds). Remove the `Crs`/`crs` field from `TileSource` (Rust + `tileSource.ts` + `MapTileSourceSettings.tsx`'s hardcoded `crs:'Geodetic'`). Simplify `commands.rs` validation to: URL-shape → reachability/image probe (`fetch` one tile) → `LanLive` / `Unreachable` / `Incompatible` (the last reserved for "responded but not an image / bad status" — `NotAnImage`/`Status`/`Redirect`, which already exists). This deletes the false-accept bug rather than patching it. (The doc's earlier description of the probe bug was imprecise: `crs.rs` does `base = source.url.trim_end_matches('/')` then probes `{base}/tilejson.json` and `{base}` — but `source.url` is the `…/{z}/{x}/{y}.png` template, so `base` keeps the template suffix and every probe path is built from the wrong root → always `Unknown` → the hardcoded `crs:'Geodetic'` override force-accepts. Deletion moots all of it.)
5. **Wire `tileSource` into all 4 consumers** (absorbs tuxlink-n6xu + tuxlink-24px): `StationFinderMap`, `GridMapPicker`, `PositionMapWidget`, `PositionPickerOverlay` fetch the validated source+status (a shared `useTileSource` hook) and pass `tileSource` to `BaseMap` so the zoom cap rises and tiles render. The 6-char precision gate (`sixCharAllowed`) currently reads a hardcoded `RASTER_VIEW_ZOOM=2` in `PositionPickerOverlay`, which renders `PositionMapWidget`→`BaseMap` (so `useMap()` is not reachable from the overlay scope). Wire the live zoom out via a Leaflet `zoomend`→callback bridge inside `BaseMap` (a new `onZoomChange?` prop) lifted to the overlay, OR a shared zoom-state hook — pick in writing-plans; the gate must read the live tile-backed zoom, not the constant.
6. **Settings copy + scheme.** Update `MapTileSourceSettings` help text (currently "MUST serve EPSG:4326") to reflect standard Web Mercator XYZ. Keep the XYZ/TMS scheme toggle.
7. **Supersede the wrong docs.** Add a superseding note to the two spec/plan docs named above + correct the `crs.rs` module header. AGENTS.md parity check if any CLAUDE.md rule changes (none expected).
8. **Verify with Geographica live.** Configure `http://localhost:8090/styles/darkmatter/{z}/{x}/{y}.png` (or `pandora.local:8090` from another host), confirm it validates as `lan-live`, renders tiles, and zoom unlocks. grim-verify on WebKitGTK at 1920×1080.

## Success criteria

- Geographica's standard Mercator endpoint configures → `lan-live`, tiles render, zoom unlocks past 2 to the source's max, in every map pane.
- A public FQDN / public IP is still REJECTED by the LAN/SSRF gatekeeper (the real control is intact).
- Offline with zero config: the bundled Mercator base still renders; app fully functional; no network reached.
- Maidenhead grid / GRIB / click-to-locate correct under 3857 (grim-verified).
- The two wrong spec/plan docs carry superseding notes; `crs.rs` header corrected.

## Open questions

- **RESOLVED — CRS probing:** DELETE the CRS gate entirely (Work item 4). The LAN/SSRF host gate is the only real control; with 3857 valid there is nothing meaningful to reject on CRS grounds, and a non-image/bad URL already fails the reachability/image probe. Removes the false-accept bug + ~500 lines of dead 4326 probe tests.
- **Bundled base as `ImageOverlay` vs a bundled low-zoom `TileLayer`?** ImageOverlay is simplest; a bundled z0–z3 tile pyramid aligns more naturally under 3857. Decide in writing-plans.
- **Mercator pole clip (±85.05°)** — confirmed no map pane needs >85° latitude (EmComm/US). `WORLD_BOUNDS`/`maxBounds` updated to the Mercator extent (Blast radius).

## Next

writing-plans → subagent-driven-development on this branch, with the grim/WebKitGTK render gate (memory `reference_webkitgtk_render_harness`) as the UI gate and a Geographica live-config smoke as the acceptance check.
