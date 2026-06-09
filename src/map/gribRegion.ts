/**
 * Pure signed-decimal-bbox → GRIB region normalizer.
 *
 * A map drag yields two signed decimal corners in arbitrary order, but
 * `src-tauri/src/grib/composer.rs` consumes whole-degree `{degrees, dir}`
 * fields, does NOT reorder them, and rejects both over-range degrees
 * (>90 / >180) and equal (degenerate) ranges. This module bridges that gap:
 * it orders the corners (lat0=south, lat1=north, lon0=west, lon1=east), floors
 * /ceils OUTWARD so the region always contains the dragged box, clamps to
 * ±90/±180, and guarantees a non-degenerate ≥1° range.
 *
 * Pure math, unit-tested in jsdom — no Leaflet, no DOM.
 */
import type { Latitude, Longitude } from '../grib/types';

export interface GribRegion {
  lat0: Latitude; // south
  lat1: Latitude; // north
  lon0: Longitude; // west
  lon1: Longitude; // east
}

interface SignedLatLon {
  lat: number;
  lon: number;
}

/**
 * Signed decimal latitude → whole-degree `{degrees, dir}`. Rounds to the
 * nearest whole degree, derives hemisphere from the sign (0 → canonical N,
 * never S), and clamps the magnitude to 90.
 */
export function signedToLatitude(lat: number): Latitude {
  const degrees = Math.min(90, Math.round(Math.abs(lat)));
  return { degrees, dir: lat < 0 ? 'S' : 'N' };
}

/**
 * Signed decimal longitude → whole-degree `{degrees, dir}`. Rounds to the
 * nearest whole degree, derives hemisphere from the sign (0 → canonical E,
 * never W), and clamps the magnitude to 180.
 */
export function signedToLongitude(lon: number): Longitude {
  const degrees = Math.min(180, Math.round(Math.abs(lon)));
  return { degrees, dir: lon < 0 ? 'W' : 'E' };
}

/**
 * Two signed decimal corners → an ordered, whole-degree, non-degenerate GRIB
 * region. Floors/ceils OUTWARD (the region contains the drag), clamps to
 * world bounds, then expands any collapsed edge by 1° — at the 90/180 ceiling
 * it expands the opposite edge DOWN since it cannot expand up.
 */
export function signedBboxToGribRegion(a: SignedLatLon, b: SignedLatLon): GribRegion {
  let south = Math.max(-90, Math.floor(Math.min(a.lat, b.lat)));
  let north = Math.min(90, Math.ceil(Math.max(a.lat, b.lat)));
  let west = Math.max(-180, Math.floor(Math.min(a.lon, b.lon)));
  let east = Math.min(180, Math.ceil(Math.max(a.lon, b.lon)));

  // Degeneracy guard: an integer-aligned or zero-area drag collapses
  // floor==ceil. Expand by 1°, or pull the opposite edge down at the ceiling.
  if (north === south) {
    if (north < 90) north += 1;
    else south -= 1;
  }
  if (east === west) {
    if (east < 180) east += 1;
    else west -= 1;
  }

  return {
    lat0: signedToLatitude(south),
    lat1: signedToLatitude(north),
    lon0: signedToLongitude(west),
    lon1: signedToLongitude(east),
  };
}
