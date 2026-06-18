/**
 * MapLibre basemap style builder (tuxlink-ndi4, plan phase 2 / L1).
 *
 * Assembles a MapLibre GL v8 style from @protomaps/basemaps' light flavor over
 * the bundled PMTiles vector source. The dark flavor (a GL-native inverted style
 * — plan L2) bakes colors via darkStyle; the per-flavor base layer array is
 * memoized (baked once, not per build — B3, tuxlink-vnk7).
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

/**
 * Resolve a bundled-asset path to an ABSOLUTE URL against the webview's document
 * URL. maplibre v5 rejects root-relative sprite/glyphs URLs ("must be absolute"),
 * silently dropping labels + icons (tuxlink-56ki).
 *
 * Resolve against `location.href`, NOT `location.origin` (tuxlink-1tai / Codex
 * adrev): a custom scheme like `tauri://localhost` is an OPAQUE origin, so
 * `location.origin` is the string `'null'` and concatenating it yields
 * `null/basemap/...` — broken in the packaged build even though it works in the
 * `http://localhost:1420` dev server (a tuple origin). `new URL(path, href)`
 * resolves correctly for `tauri://`, `http://`, and `file://` alike.
 *
 * The fallback base (no `location`, e.g. a non-browser import) is only ever hit
 * outside a webview, where the style is never actually consumed by maplibre.
 */
function absoluteBasemapUrl(path: string): string {
  const base =
    typeof location !== 'undefined' && location.href ? location.href : 'http://localhost/';
  return new URL(path, base).href;
}

/** Bundled glyph PBFs, served absolute from the webview origin.
 *
 * The `{fontstack}`/`{range}` tokens are maplibre template placeholders it
 * substitutes at fetch time — they MUST stay literal. `new URL()` percent-encodes
 * braces (`{` → `%7B`), which would 404 every font, so resolve only the brace-free
 * directory to absolute and append the template by string (tuxlink-1tai). */
function glyphsUrl(): string {
  return `${absoluteBasemapUrl('/basemap/glyphs/')}{fontstack}/{range}.pbf`;
}

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
/**
 * Module-level memo of the per-flavor base (world-overview) layer array. The
 * `@protomaps/basemaps` `layers()` generator + `bakeDarkColors` are pure for a
 * fixed (source, flavor), so compute once and reuse by reference — the dark
 * transform deep-copies + recurses every `*-color` and is multi-hundred-ms on
 * the Pi's software-GL CPU budget (B3, tuxlink-vnk7). Callers MUST NOT mutate the
 * returned array; `buildBasemapStyle` copies it before appending pack layers.
 */
const baseLayerCache = new Map<BasemapFlavor, ReturnType<typeof layers>>();
function baseLayers(flavor: BasemapFlavor): ReturnType<typeof layers> {
  const hit = baseLayerCache.get(flavor);
  if (hit) return hit;
  const built = generalizeRoadDensity(layers(BASEMAP_SOURCE_ID, tuxlinkFlavor(), { lang: 'en' }));
  const baked = flavor === 'dark' ? bakeDarkColors(built) : built;
  baseLayerCache.set(flavor, baked);
  return baked;
}

/// tuxlink-hzwc bug #8: minor / residential / service / link / unclassified road
/// classes carry NO minzoom in the stock flavor, so the whole metro street grid
/// draws at mid zooms — a dense "spaghetti" of streets. Gate those classes to a
/// neighborhood zoom so mid-zoom shows arterials + highways only and the
/// residential grid fills in once the operator zooms in close (the cartographic
/// generalization MeshMap-style maps use). Arterials (`major`) and `highway`
/// keep their defaults — they give context without the clutter. Tunable floor;
/// operator-smoke the level on the converged build.
const MINOR_ROAD_MINZOOM = 13;
const MINOR_ROAD_RE =
  /^roads_(minor|minor_service|link|other)(_casing|_early|_late)?$|^roads_(tunnels|bridges)_(minor|link|other)/;
function generalizeRoadDensity(ls: ReturnType<typeof layers>): ReturnType<typeof layers> {
  return ls.map((layer) =>
    MINOR_ROAD_RE.test(layer.id)
      ? { ...layer, minzoom: Math.max(layer.minzoom ?? 0, MINOR_ROAD_MINZOOM) }
      : layer,
  ) as ReturnType<typeof layers>;
}

export function buildBasemapStyle(
  flavor: BasemapFlavor,
  packs: PackSource[] = [],
): StyleSpecification {
  const sources: StyleSpecification['sources'] = {
    [BASEMAP_SOURCE_ID]: {
      type: 'vector',
      url: PMTILES_SOURCE_URL,
      attribution: OSM_ATTRIBUTION,
      // NOTE (D3, tuxlink-vnk7 — Codex P1): do NOT advertise a source `maxzoom`
      // ABOVE the archive's real max. The bundled overview is z0–6; the PMTiles
      // header already reports maxzoom=6, so MapLibre overzooms the z6 tiles for
      // z7–14 (the "never blank" behavior) WITHOUT requesting nonexistent deeper
      // tiles. An explicit override of 7+ made MapLibre fetch z7 tiles the archive
      // lacks → empty → blank above z6. Overzoom cost is addressed by reducing
      // interactive maxZoom outside pack coverage (deferred, plan A-followup), not
      // by lying about tile availability.
    },
  };

  // World overview layers (memoized per flavor; baked once — B3). Overzoom past
  // z6 = never blank. Copy only when packs are appended so the cache stays pure.
  const base = baseLayers(flavor);
  const styleLayers = packs.length === 0 ? base : [...base];

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
    // A pack contributes only its FILL/LINE detail (the geometry the overzoomed
    // overview lacks). Labels/symbols are owned by the single base overview layer
    // set — duplicating them per pack adds the most expensive llvmpipe primitive
    // (glyph shaping + collision, run globally) for the SAME OSM names, no visual
    // gain (B1, tuxlink-vnk7). So drop `symbol` alongside the existing sourceless
    // `background` drop. Look-preserving: labels still render, from the base.
    const packBuilt = generalizeRoadDensity(layers(sid, tuxlinkFlavor(), { lang: 'en' }));
    const packLayers = (flavor === 'dark' ? bakeDarkColors(packBuilt) : packBuilt)
      .filter((layer) => layer.type !== 'background' && layer.type !== 'symbol')
      .map((layer) => ({
        ...layer,
        id: `${sid}-${layer.id}`,
        minzoom: Math.max(layer.minzoom ?? 0, REGION_MINZOOM),
      }));
    styleLayers.push(...packLayers);
  }

  return {
    version: 8,
    glyphs: glyphsUrl(),
    sprite: absoluteBasemapUrl(`/basemap/sprites/${flavor}`),
    sources,
    layers: styleLayers,
  };
}
