/**
 * protomaps-leaflet base layer builder over the `tile://` PMTiles seam
 * (tuxlink-6kdw, plan phase 1 / Task 2 — THE SEAM CRUX).
 *
 * The Leaflet twin of `basemapStyle.ts`. Three rules, each fixing a P0 the
 * cross-provider adversarial review caught (the first cut shipped a blank map):
 *
 *  1. PMTiles INSTANCE, not a string URL. protomaps-leaflet picks `PmtilesSource`
 *     ONLY when a string url's pathname ends `.pmtiles`; `tile://pmtiles/world`
 *     (pathname `/world`) would fall through to `ZxySource` — no Range request, it
 *     parses the whole archive as one MVT tile → blank. Passing
 *     `new PMTiles('tile://pmtiles/<id>')` (the `.d.ts` types `url: PMTiles |
 *     string`) forces `PmtilesSource`, whose internal Range-fetch of the Rust
 *     206 seam is the SAME path the MapLibre `pmtiles` Protocol already proves.
 *     NO `addProtocol` (that is a MapLibre-only mechanism).
 *  2. Cap `maxDataZoom` per source. protomaps-leaflet defaults `maxDataZoom:15`
 *     and requests data at z=displayZ−1; the bundled overview is z0–6, so above
 *     ~z8 it would request z7+ data the archive lacks → blank. The overview is
 *     capped at 6; each pack carries its real maxzoom (continent-na is z0–14).
 *  3. Packs carry NO flavor/background/labels. A flavored layer paints its
 *     `backgroundColor` on EVERY rendered tile, so a pack's empty tiles outside
 *     its coverage would mask the overview. Only the OVERVIEW is flavored (one
 *     global background + labels); each PACK passes explicit `paintRules`
 *     (from the same flavor) + `labelRules: []` + no background, so it draws only
 *     its detail geometry and is transparent elsewhere. Mirrors
 *     `basemapStyle.ts` dropping `background` + `symbol` from pack layer sets.
 *
 * Composite ordering: explicit `zIndex` (overview 1, packs 2+) so packs paint
 * above the overview regardless of add order; packs clamp to `minZoom: 6`.
 *
 * ── Confirmed vendored API (protomaps-leaflet 5.1.0, `index.d.ts`) ──
 *   `leafletLayer(opts)` → `L.GridLayer`. `LeafletLayerOptions extends
 *   L.GridLayerOptions` with `url?: PMTiles | string`, `paintRules?`,
 *   `labelRules?`, `maxDataZoom?`, `flavor?`, `backgroundColor?`, `attribution?`,
 *   `lang?`, inherited `minZoom`/`maxZoom`/`zIndex`/`pane`. Exported helpers:
 *   `paintRules(flavor)`, `labelRules(flavor, lang)`. The flavor object comes
 *   from `@protomaps/basemaps`' `namedFlavor(name)`.
 *
 * Serving (fully offline): the Rust 206 seam serves both the bundled world
 * overview (`tile://pmtiles/world`, z0–6) and each downloaded region pack
 * (`tile://pmtiles/<id>`, z0–14).
 */
import { leafletLayer, paintRules as pmPaintRules } from '../vendor/protomaps-leaflet';
import { PMTiles } from 'pmtiles';
import { namedFlavor } from '@protomaps/basemaps';
import type { Layer as LeafletLayer } from 'leaflet';

/** Supported base-layer flavors. `dark` is protomaps-leaflet's paint-rule dark
 * (NOT a CSS filter, NOT the MapLibre `tuxlinkFlavor` bake-invert). */
export type BasemapFlavor = 'light' | 'dark';

/** An installed region pack to composite over the world overview (R7).
 * `id` is the registered archive id served at `tile://pmtiles/<id>`; `maxZoom`
 * is its real archive max (caps `maxDataZoom` so overzoom never requests absent
 * tiles). Declared LOCALLY (NOT imported from `basemapStyle.ts`) to keep the
 * Leaflet and MapLibre substrates independent. */
export interface PackSource {
  id: string;
  maxZoom?: number;
}

/** PMTiles seam URL for an archive id → the Rust HTTP-206 custom protocol. */
export const PMTILES_TILE_URL = (id: string): string => `tile://pmtiles/${id}`;

/** The flavor's own background color (dark `#34373d` / light `#cccccc`). Used as
 * the MAP CONTAINER background so a blank-until-painted tile shows this instead of
 * Leaflet's default light `#ddd` — i.e. load/zoom gaps blend into the map rather
 * than flashing white on the software renderer (operator smoke, tuxlink-6kdw). */
export function flavorBackground(flavor: BasemapFlavor): string {
  return (namedFlavor(flavor) as { background: string }).background;
}

/** ODbL attribution required for OSM-derived vector tiles. */
export const OSM_ATTRIBUTION = '© OpenStreetMap contributors';

/** Zoom at and above which a downloaded region pack's detail takes over (R7;
 * mirrors `basemapStyle.REGION_MINZOOM`). The overview overzooms past z6 so the
 * viewport is never blank; packs clamp to z6+ and draw on top. */
export const REGION_MINZOOM = 6;

/** Archive id of the always-present bundled world overview (z0–6). */
const WORLD_OVERVIEW_ID = 'world';
/** The bundled overview is z0–6; cap data requests so overzoom does not ask for
 * absent z7+ tiles (R2 P0#2). */
const OVERVIEW_MAX_DATA_ZOOM = 6;
/** Fallback pack data-zoom cap when a pack's real maxzoom is not known yet. */
const DEFAULT_PACK_MAX_DATA_ZOOM = 14;

/**
 * Smoothness tuning for the Canvas2D GridLayer on the Pi's software renderer.
 * Unlike MapLibre's GPU-texture cache, protomaps-leaflet re-PAINTS per-tile
 * canvases on each view change, so rapid pan/zoom flashes white while tiles
 * repaint. These options narrow that gap (operator smoke, tuxlink-6kdw):
 *  - `updateWhenZooming: false` — during a zoom gesture, SCALE the existing tiles
 *    (CSS transform) and only repaint crisp tiles after the zoom settles, rather
 *    than repainting mid-zoom. The biggest win for "white during zoom".
 *  - `keepBuffer: 4` — retain more off-screen tiles (Leaflet default 2) so panning
 *    back does not repaint.
 *  - `devicePixelRatio: 1` — render fewer pixels per tile (the basemap is
 *    situational, not print); mirrors the MapLibre `pixelRatio:1` llvmpipe choice.
 */
const SMOOTH_RENDER = {
  updateWhenZooming: false,
  keepBuffer: 4,
  devicePixelRatio: 1,
} as const;

/**
 * Build the protomaps-leaflet base layer(s) for the given flavor over the
 * `tile://` PMTiles seam. Returns `[overview, ...packLayers]`:
 *  - overview: flavored (background + labels), `maxDataZoom: 6`, `zIndex: 1`,
 *    left to overzoom past z6 (never blank outside pack coverage).
 *  - each pack: explicit `paintRules` from the same flavor, `labelRules: []`,
 *    NO flavor/background (so empty pack tiles never mask the overview),
 *    `maxDataZoom: pack.maxZoom ?? 14`, `minZoom: 6`, `zIndex: 2+i`.
 *
 * The caller (LeafletMap, Task 4) adds these to the map; the explicit zIndex
 * makes compositing independent of add order.
 */
export function buildBaseLayers(flavor: BasemapFlavor, packs: PackSource[] = []): LeafletLayer[] {
  const overview = leafletLayer({
    ...SMOOTH_RENDER,
    url: new PMTiles(PMTILES_TILE_URL(WORLD_OVERVIEW_ID)),
    flavor,
    lang: 'en',
    attribution: OSM_ATTRIBUTION,
    maxDataZoom: OVERVIEW_MAX_DATA_ZOOM,
    zIndex: 1,
  }) as unknown as LeafletLayer;

  const packLayers = packs.map(
    (pack, i) =>
      leafletLayer({
        ...SMOOTH_RENDER,
        url: new PMTiles(PMTILES_TILE_URL(pack.id)),
        // Explicit paint rules from the SAME flavor, but NO `flavor`/`backgroundColor`
        // and NO labels — the pack draws only its detail geometry, transparent
        // elsewhere, so its empty tiles never mask the overview (R2 P0#3).
        paintRules: pmPaintRules(namedFlavor(flavor)),
        labelRules: [],
        // Same OSM/ODbL credit as the overview (Leaflet refcounts identical strings
        // → shows once). Without it the vendored layer injects its own default
        // "Protomaps © OSM" credit, defeating the single attribution source (impl P2).
        attribution: OSM_ATTRIBUTION,
        lang: 'en',
        maxDataZoom: pack.maxZoom ?? DEFAULT_PACK_MAX_DATA_ZOOM,
        minZoom: REGION_MINZOOM,
        zIndex: 2 + i,
      }) as unknown as LeafletLayer,
  );

  return [overview, ...packLayers];
}
