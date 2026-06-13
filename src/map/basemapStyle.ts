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
import { layers } from '@protomaps/basemaps';
import { bakeDarkColors } from './darkStyle';
import { tuxlinkFlavor } from './tuxlinkFlavor';

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

/** Supported style flavors. `dark` is the build-time-baked GL-native inverted
 * style (L2 — NOT a runtime CSS filter), derived from the light flavor. */
export type BasemapFlavor = 'light' | 'dark';

/** Zoom at and above which a downloaded region pack's detailed layers take over
 * (R7). The bundled overview covers z0–6; packs are z0–14. */
export const REGION_MINZOOM = 6;

/** An installed region pack to composite over the world overview (R7).
 * `id` is the registered archive id served at `tile://pmtiles/<id>`. */
export interface PackSource {
  id: string;
}

/** Style source id for a pack's vector tiles. */
function packSourceId(id: string): string {
  return `pack-${id}`;
}

/** PMTiles protocol URL for a downloaded pack served via the Rust 206 seam. */
function packUrl(id: string): string {
  return `pmtiles://tile://pmtiles/${id}`;
}

/**
 * Build the MapLibre v8 style for the given flavor over the bundled PMTiles
 * world overview, compositing any installed region packs (R7).
 *
 * Both modes are generated from tuxlink's high-contrast `tuxlinkFlavor` (the
 * outdoor light palette). `dark` then bakes every `*-color` (invert →
 * hue-rotate(180°) → brightness(1.33)) — see darkStyle — which reproduces
 * meshmap's warm-roads-on-dark look because the source flavor is bold. The
 * sprite swaps to Protomaps' authored dark sheet (icons are raster, not
 * color-derivable; A7).
 *
 * R7 compositing (never blank; full detail where downloaded): the world overview
 * source is left UNCLAMPED — MapLibre overzooms it past z6, so it is present
 * everywhere as a coarse base (never a blank viewport). Each installed pack adds
 * its own vector source whose layers are clamped to `minzoom >= REGION_MINZOOM`
 * and drawn ON TOP of the overview, so inside a downloaded pack's coverage the
 * detailed z6–14 tiles win, while outside it the overzoomed overview still shows.
 * (This favors the required behavior over A11's literal "disjoint bands", which
 * would blank the viewport above z6 outside any pack.)
 */
export function buildBasemapStyle(
  flavor: BasemapFlavor,
  packs: PackSource[] = [],
): StyleSpecification {
  const bake = (ls: ReturnType<typeof layers>) =>
    flavor === 'dark' ? bakeDarkColors(ls) : ls;

  const sources: StyleSpecification['sources'] = {
    [BASEMAP_SOURCE_ID]: {
      type: 'vector',
      url: PMTILES_SOURCE_URL,
      attribution: OSM_ATTRIBUTION,
    },
  };

  // World overview layers (unclamped — overzoom past z6 = never blank).
  const styleLayers = bake(layers(BASEMAP_SOURCE_ID, tuxlinkFlavor(), { lang: 'en' }));

  // Composite each installed pack as a second source, layers clamped to z6+ and
  // appended (drawn on top of the overview within the pack's coverage).
  //
  // CRITICAL: drop the `background` layer from each pack's set. @protomaps/basemaps
  // `layers()` emits an opaque, SOURCELESS `background` layer that paints the WHOLE
  // canvas. Appended on top of the overview it would hide the overview EVERYWHERE
  // (not just inside the pack) and every overlay beneath it — a solid-colour map the
  // moment any pack is installed. The overview's own background (added above) is the
  // single global base; a pack contributes only its source-bound detail layers,
  // which paint on top exclusively where the pack actually has tiles.
  for (const pack of packs) {
    const sid = packSourceId(pack.id);
    sources[sid] = {
      type: 'vector',
      url: packUrl(pack.id),
      attribution: OSM_ATTRIBUTION,
    };
    const packLayers = bake(layers(sid, tuxlinkFlavor(), { lang: 'en' }))
      .filter((layer) => layer.type !== 'background')
      .map((layer) => ({
        ...layer,
        id: `${sid}-${layer.id}`,
        minzoom: Math.max(layer.minzoom ?? 0, REGION_MINZOOM),
      }));
    styleLayers.push(...packLayers);
  }

  return {
    version: 8,
    glyphs: GLYPHS_URL,
    sprite: `/basemap/sprites/${flavor}`,
    sources,
    layers: styleLayers,
  };
}
