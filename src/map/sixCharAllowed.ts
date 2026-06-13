/**
 * sixCharAllowed — the 6-char (subsquare) Maidenhead precision gate.
 *
 * tuxlink-ndi4 phase-2 re-derivation (A16). The old gate required a validated LAN
 * raster tile source (lan-live/lan-cached) at depth — that input is gone with the
 * raster basemap. On the vector basemap the bundled z0–6 overview OVERZOOMS, so a
 * precise pick needs only sufficient view ZOOM, not detailed tiles. The gate is
 * therefore zoom-only: 6-char is offered once the operator zooms to at least
 * {@link SIX_CHAR_MIN_ZOOM} (the subsquare level on the z0–14 scale, matching
 * gridGeometry.levelFromZoom). Below that, precision falls back to 4-char.
 *
 * Phase 4 (region packs) MAY refine this to "a z14 pack covers this point"; until
 * then zoom is the honest, non-dead predicate (the selector unlocks as you zoom).
 */

/** Minimum view zoom (z0–14 scale) at which 6-char subsquare precision is offered. */
export const SIX_CHAR_MIN_ZOOM = 9;

export interface MapView {
  /** Current map view zoom on the z0–14 fractional scale. */
  zoom: number;
}

/** True when the view is zoomed in enough to offer 6-char subsquare precision. */
export function sixCharAllowed(view: MapView): boolean {
  return view.zoom >= SIX_CHAR_MIN_ZOOM;
}
