/**
 * tuxlinkFlavor — tuxlink's restrained OSM-Carto-class basemap palette
 * (tuxlink-ndi4 phase 3; MeshMap-fidelity rework tuxlink-h17b).
 *
 * An OSM-Carto-class override of `@protomaps/basemaps`' light flavor, serving
 * DOUBLE duty as ONE source of truth for both modes:
 *
 *  1. LIGHT mode is a restrained, daylight-legible map (OSM-Carto colors:
 *     warm-but-muted road ramp, beige earth, muted greens) — the proven
 *     general-purpose daytime cartography.
 *
 *  2. DARK mode is the SAME flavor run through the bake-invert (darkStyle.ts).
 *     MeshMap's dark IS `invert(1) hue-rotate(180°) brightness(1.33)` over the
 *     standard OSM Carto raster; darkStyle bakes that identical transform
 *     GL-natively (~45fps vs ~15fps for a runtime CSS filter). Inverting THIS
 *     OSM-Carto source therefore reproduces MeshMap's dark by construction:
 *     neutral gray canvas, olive greens, salmon freeways, recessive arterials.
 *
 * The earlier "punched-up" ramp inverted to a garish, loud dark (the basemap
 * drift). Restraint here is the fix — see
 * docs/design/2026-06-19-basemap-meshmap-fidelity-design.md.
 *
 * Mechanism: spread the stock light flavor, override the contrast-bearing slots.
 * Any slot left unset inherits Protomaps' value. Tune both modes from here.
 */
import { namedFlavor } from '@protomaps/basemaps';

/** Contrast-bearing slot overrides applied on top of the stock light flavor.
 *
 * These are OSM-Carto-class LIGHT values (tuxlink-h17b). They serve double duty:
 * the LIGHT flavor is a restrained, daylight-legible map; baked-inverted by
 * darkStyle (`invert → hue-rotate(180°) → brightness(1.33)`, the exact MeshMap
 * filter) they produce the MeshMap-class dark — neutral gray canvas, olive
 * greens, salmon freeways, rust/recessive arterials, receding minor streets.
 * The earlier "punched-up" ramp inverted to a garish dark; restraint here is the
 * whole fix. See docs/design/2026-06-19-basemap-meshmap-fidelity-design.md. */
export const TUXLINK_FLAVOR_OVERRIDES: Record<string, string> = {
  background: '#f2efe9',
  earth: '#f2efe9',
  water: '#aad3df',
  // Vegetation / landcover — OSM-Carto greens (bake to olive on dark).
  wood_a: '#add19e',
  wood_b: '#a3c995',
  park_a: '#cdebb0',
  park_b: '#c2e6a0',
  scrub_a: '#c8d7ab',
  scrub_b: '#bccf9c',
  glacier: '#e8f0f5',
  sand: '#f5e9c6',
  beach: '#f5e9c6',
  // Road network — OSM-Carto warm ramp (restrained). Bakes to: highway→salmon
  // #d55e73, major→#5d2b00 (recessive), minor→near-black (recede). The cased
  // structure is inherited from the stock layers; only the colors change.
  highway: '#e990a0',
  highway_casing_early: '#d4748a',
  highway_casing_late: '#d4748a',
  major: '#fcd6a4',
  major_casing_early: '#e0b070',
  major_casing_late: '#e0b070',
  minor_a: '#f7fabf',
  minor_b: '#ffffff',
  minor_casing: '#cfcf9a',
  minor_service: '#ffffff',
  minor_service_casing: '#d6d6d6',
  link: '#fcd6a4',
  link_casing: '#e0b070',
  other: '#ffffff',
  tunnel_highway: '#f2b3bf',
  tunnel_major: '#fde3c0',
  tunnel_minor: '#fafafa',
  bridges_highway: '#e990a0',
  bridges_major: '#fcd6a4',
  bridges_minor: '#ffffff',
  railway: '#b0a394',
  boundaries: '#ac46ac',
  pier: '#e0ddd5',
  buildings: '#d9d0c9',
};

/** The tuxlink high-contrast flavor: stock Protomaps light + the bold overrides. */
export function tuxlinkFlavor(): ReturnType<typeof namedFlavor> {
  return { ...namedFlavor('light'), ...TUXLINK_FLAVOR_OVERRIDES };
}
