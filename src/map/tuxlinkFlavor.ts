/**
 * tuxlinkFlavor — tuxlink's high-contrast basemap palette (tuxlink-ndi4 phase 3).
 *
 * A punched-up variant of `@protomaps/basemaps`' light flavor, serving DOUBLE duty:
 *
 *  1. LIGHT mode is tuxlink's primary outdoor theme — used almost exclusively in
 *     bright-sunlight, high-contrast field/EmComm applications. Stock Protomaps
 *     light has near-invisible light-gray roads on beige; this colors the road
 *     network boldly (OSM-style orange/yellow ramp), saturates water, and
 *     strengthens the greens so the map is legible in direct sun.
 *
 *  2. DARK mode is the SAME flavor run through the bake-invert (darkStyle.ts).
 *     meshmap's dark look is literally `invert(1) hue-rotate(180°) brightness(1.33)`
 *     over the *standard OSM raster* — vivid, colored roads inverted. Inverting a
 *     minimalist palette stays muted ("dark and smeary"); inverting THIS bold
 *     palette reproduces meshmap's warm-roads-on-dark legibility, in pure vector
 *     (so it runs at ~45fps baked, not ~15fps as a runtime CSS filter — the L2
 *     decision). Operator-approved 2026-06-13: "looks just like meshmap now."
 *
 * Mechanism: spread the stock light flavor, override the contrast-bearing slots.
 * Any slot left unset inherits Protomaps' value. Colors are first-pass tuned and
 * meant to be adjusted here as one source of truth for both modes.
 */
import { namedFlavor } from '@protomaps/basemaps';

/** Contrast-bearing slot overrides applied on top of the stock light flavor. */
export const TUXLINK_FLAVOR_OVERRIDES: Record<string, string> = {
  background: '#dedad2',
  earth: '#ece8e0',
  water: '#2f7fc4',
  // Vegetation / landcover — bolder greens for sun legibility + dark texture.
  wood_a: '#8fc77a',
  wood_b: '#74bb5c',
  park_a: '#bfe3a8',
  park_b: '#93d07c',
  scrub_a: '#cfe0b0',
  scrub_b: '#b6d490',
  glacier: '#e8f0f5',
  sand: '#efe2c0',
  beach: '#f0e1bd',
  // Road network — the legibility carrier. tuxlink-hzwc bug #8: the prior ramp
  // (highway #e85d3a, major #f2933a, minor_a #f7c948) rendered a "bright
  // yellow/orange spaghetti" of streets at city zooms. These are ~50%
  // desaturated toward each color's own luminance — chroma drops (no longer
  // garish) while LIGHTNESS is preserved, which is load-bearing: dark mode is
  // this flavor inverted (darkStyle), so lightening residential roads would make
  // them vanish on dark. Muted, MeshMap-like, both modes safe. Operator-smoke on
  // the converged build to confirm the level of mute.
  highway: '#b5705e',
  highway_casing_early: '#784435',
  highway_casing_late: '#784435',
  major: '#cb9c6f',
  major_casing_early: '#9b724b',
  major_casing_late: '#9b724b',
  minor_a: '#dfc888',
  minor_b: '#ffffff',
  minor_casing: '#9a8f80',
  minor_service: '#ffffff',
  minor_service_casing: '#b0a596',
  link: '#cb9c6f',
  link_casing: '#9b724b',
  other: '#ffffff',
  tunnel_highway: '#d6b2a5',
  tunnel_major: '#e3c9b4',
  tunnel_minor: '#eeeeee',
  bridges_highway: '#b5705e',
  bridges_major: '#cb9c6f',
  bridges_minor: '#ffffff',
  railway: '#8a7f70',
  boundaries: '#9a5fa6',
  pier: '#d8d4cc',
  buildings: '#d7cfc2',
};

/** The tuxlink high-contrast flavor: stock Protomaps light + the bold overrides. */
export function tuxlinkFlavor(): ReturnType<typeof namedFlavor> {
  return { ...namedFlavor('light'), ...TUXLINK_FLAVOR_OVERRIDES };
}
