/**
 * protomaps-leaflet base layer builder over the `tile://` PMTiles seam
 * (tuxlink-6kdw, plan phase 1 / Task 2 — THE SEAM CRUX).
 *
 * The Leaflet twin of `basemapStyle.ts`. Where the MapLibre path assembles a GL
 * style and registers a `pmtiles` Protocol, the Leaflet path hands
 * protomaps-leaflet's `leafletLayer` a plain `tile://pmtiles/<id>` URL and lets
 * it build its own PMTiles "view" internally. Its internal `new PMTiles(url)`
 * issues `fetch('tile://pmtiles/world', { headers: { Range } })` — the SAME
 * fetch path the MapLibre `pmtiles` Protocol already proves works against the
 * Rust HTTP-206 seam, and which CSP `connect-src tile:` permits. NO
 * `addProtocol`/Protocol registration is needed (that is a MapLibre-only
 * mechanism).
 *
 * ── Confirmed vendored API (protomaps-leaflet 5.1.0, inspected in
 *    `src/vendor/protomaps-leaflet/index.d.ts`; see PROVENANCE.md) ──
 *   export `leafletLayer(options?: LeafletLayerOptions)` → an `L.GridLayer`.
 *   `LeafletLayerOptions extends L.GridLayerOptions`. Option names used here:
 *     - `url: PMTiles | string`  — the tile source; we pass the `tile://` string.
 *     - `flavor: string`         — `'light' | 'dark'`; protomaps-leaflet picks
 *                                  paint-rule colors (NOT a CSS filter / bake).
 *     - `lang: string`           — label language; `'en'`.
 *     - `attribution: string`    — ODbL/OSM credit string.
 *     - `minZoom` / `maxZoom` / `pane` — inherited from `L.GridLayerOptions`;
 *                                  `minZoom` clamps pack layers to z6+.
 *   NOTE: `SourceOptions` (the `{ sources }` form) is
 *   `{ levelDiff?; maxDataZoom?; url?; sources? }` — it carries NO top-level
 *   `maxzoom`. So an explicit overview maxzoom cap is NOT expressible via
 *   `sources`; the plain `{ url }` form is used and overzoom relies on the
 *   PMTiles archive header's own maxzoom (the bundled overview is z0–6),
 *   exactly like the MapLibre path (see basemapStyle.ts §"never blank").
 *
 * Serving (fully offline): the Rust 206 seam serves both the bundled world
 * overview (`tile://pmtiles/world`, z0–6) and each downloaded region pack
 * (`tile://pmtiles/<id>`, z0–14).
 */
import { leafletLayer } from '../vendor/protomaps-leaflet';
import type { Layer as LeafletLayer } from 'leaflet';

/** Supported base-layer flavors. `dark` is protomaps-leaflet's paint-rule dark
 * (NOT a CSS filter, NOT the MapLibre `tuxlinkFlavor` bake-invert). */
export type BasemapFlavor = 'light' | 'dark';

/** An installed region pack to composite over the world overview (R7).
 * `id` is the registered archive id served at `tile://pmtiles/<id>`. Declared
 * locally (NOT imported from `basemapStyle.ts`) to keep the Leaflet and
 * MapLibre substrates independent. */
export interface PackSource {
  id: string;
}

/** PMTiles seam URL for an archive id → the Rust HTTP-206 custom protocol. */
export const PMTILES_TILE_URL = (id: string): string => `tile://pmtiles/${id}`;

/** ODbL attribution required for OSM-derived vector tiles. */
export const OSM_ATTRIBUTION = '© OpenStreetMap contributors';

/** Archive id of the always-present bundled world overview (z0–6). */
const WORLD_OVERVIEW_ID = 'world';

/** Zoom at and above which a downloaded region pack's detailed layers take over
 * (R7; mirrors `basemapStyle.REGION_MINZOOM`). The bundled overview overzooms
 * past z6 so the viewport is never blank; packs clamp to z6+ and draw on top. */
export const REGION_MINZOOM = 6;

/**
 * Build the protomaps-leaflet base layer(s) for the given flavor over the
 * `tile://` PMTiles seam.
 *
 * Returns `[overview, ...packLayers]`:
 *  - The world overview (`tile://pmtiles/world`) is the always-present base. It
 *    is left UNCLAMPED so Leaflet overzooms its z6 tiles for z7–14 (never blank
 *    outside pack coverage), matching the MapLibre overzoom behavior.
 *  - One protomaps-leaflet layer per installed pack
 *    (`tile://pmtiles/<id>`), clamped to `minZoom: REGION_MINZOOM` and drawn on
 *    top of the overview, so inside a downloaded pack's coverage the detailed
 *    z6–14 tiles win while outside it the overzoomed overview still shows.
 *
 * The caller (LeafletMap, Task 4) adds these to the map in array order; later
 * entries paint above earlier ones, so packs naturally layer above the overview.
 */
export function buildBaseLayers(flavor: BasemapFlavor, packs: PackSource[] = []): LeafletLayer[] {
  const overview = leafletLayer({
    url: PMTILES_TILE_URL(WORLD_OVERVIEW_ID),
    flavor,
    lang: 'en',
    attribution: OSM_ATTRIBUTION,
  }) as unknown as LeafletLayer;

  const packLayers = packs.map(
    (pack) =>
      leafletLayer({
        url: PMTILES_TILE_URL(pack.id),
        flavor,
        lang: 'en',
        attribution: OSM_ATTRIBUTION,
        // Clamp to z6+ so the pack's detail only takes over where the overview
        // runs out, and never competes with the overview at low zooms.
        minZoom: REGION_MINZOOM,
      }) as unknown as LeafletLayer,
  );

  return [overview, ...packLayers];
}
