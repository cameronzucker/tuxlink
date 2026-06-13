/**
 * MapLibre basemap style builder (tuxlink-ndi4, plan phase 2 / L1).
 *
 * Assembles a MapLibre GL v8 style from @protomaps/basemaps' light flavor over
 * the bundled PMTiles vector source. The dark flavor (a build-time-baked,
 * GL-native inverted style — plan L2) is added in phase 3; this is the light
 * path that the renderer swap renders first.
 *
 * Serving (fully offline, no cross-service dependency):
 *  - vector tiles: `pmtiles://tile://pmtiles/world` — the `pmtiles` JS protocol
 *    fetches `tile://pmtiles/world` with HTTP-206 Range against the Rust seam
 *    (registered via `maplibregl.addProtocol('pmtiles', …)` in MapLibreMap).
 *  - glyphs + sprites: bundled under the `'self'` origin (frontend
 *    `public/basemap/…`), NOT the pmtiles byte-range path — they are
 *    `{fontstack}/{range}`-keyed whole-file fetches (plan A8). The build script
 *    (`scripts/build-basemap-bundle.sh`) emits them; absent in dev, MapLibre 404s
 *    the labels/icons but the geometry still renders.
 */
import type { StyleSpecification } from 'maplibre-gl';
import { layers, namedFlavor } from '@protomaps/basemaps';

/** Style `sources` key for the vector basemap; @protomaps/basemaps layers
 * reference this exact name. */
export const BASEMAP_SOURCE_ID = 'protomaps';

/** PMTiles protocol URL → the `pmtiles` lib strips `pmtiles://` and Range-fetches
 * `tile://pmtiles/world` against the Rust 206 seam. */
export const PMTILES_SOURCE_URL = 'pmtiles://tile://pmtiles/world';

/** ODbL attribution required for OSM-derived vector tiles (rendered by the
 * MapLibre AttributionControl). */
export const OSM_ATTRIBUTION = '© OpenStreetMap contributors';

/** Bundled glyph PBFs, served from the `'self'` origin. */
const GLYPHS_URL = '/basemap/glyphs/{fontstack}/{range}.pbf';

/** Supported style flavors. Phase 2 ships `light`; phase 3 adds baked `dark`. */
export type BasemapFlavor = 'light';

/**
 * Build the MapLibre v8 style for the given flavor over the bundled PMTiles
 * world overview.
 */
export function buildBasemapStyle(flavor: BasemapFlavor): StyleSpecification {
  return {
    version: 8,
    glyphs: GLYPHS_URL,
    sprite: `/basemap/sprites/${flavor}`,
    sources: {
      [BASEMAP_SOURCE_ID]: {
        type: 'vector',
        url: PMTILES_SOURCE_URL,
        attribution: OSM_ATTRIBUTION,
      },
    },
    layers: layers(BASEMAP_SOURCE_ID, namedFlavor(flavor), { lang: 'en' }),
  };
}
