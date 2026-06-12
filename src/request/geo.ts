/**
 * Pure geo-resolution layer for the Request Center.
 *
 * Turns an operator's Maidenhead grid into:
 *   - a US state code (USPS 2-letter, matching the catalog's WX_US_<ST> categories), and
 *   - a marine sea-area catalog category (WX_EASTPAC / WX_PACIFIC / WX_ATLANTIC /
 *     WX_CAR_GULF / WX_US_COAST),
 * so location-aware request cards can map to the right catalog entries.
 *
 * No Tauri, no React, no network at runtime. The state-boundary dataset is
 * bundled at build time (see ./us-states.geo.json + scripts/build-us-states-geojson.md).
 */

import statesGeoJson from './us-states.geo.json';
import nwsZonesGeoJson from './nws-zones.geo.json';
import radarRegionsJson from './radar-regions.json';

export interface LatLon {
  lat: number;
  lon: number;
}

// ---------------------------------------------------------------------------
// Function 1 — gridToLatLon
// ---------------------------------------------------------------------------

/**
 * Standard Maidenhead center decode for 2-char (field) and 4-char (square)
 * grids. Case-insensitive. Returns the CENTER of the grid square.
 * Invalid input → null.
 *
 * Mirrors the field/square math of src/forms/position/maidenhead.ts; that
 * module rejects 2-char input, so this request-layer copy extends it with the
 * 2-char field case the Request Center needs.
 */
export function gridToLatLon(grid: string): LatLon | null {
  const g = grid.trim().toUpperCase();
  // Accept field (2 chars), square (4), or subsquare (6) — and tolerate
  // extended precision (8+) by decoding only the pairs it recognises. The app
  // stores the grid at full 6-char precision (config.rs), so a strict 2/4-only
  // decode dropped the operator's real location to null, collapsing the whole
  // location section + the state-name suffix (tuxlink-lfz4).
  if (g.length < 2) return null;

  // Field pair: A–R (18 values)
  const fieldA = g.charCodeAt(0) - 65; // 'A' = 65
  const fieldB = g.charCodeAt(1) - 65;
  if (fieldA < 0 || fieldA > 17 || fieldB < 0 || fieldB > 17) return null;

  let lon = fieldA * 20.0 - 180.0;
  let lat = fieldB * 10.0 - 90.0;

  // Field only (or a malformed square pair) → center of the 20°×10° field.
  if (g.length < 4 || !isDigit(g[2]) || !isDigit(g[3])) {
    return { lat: lat + 5.0, lon: lon + 10.0 };
  }

  // Square pair: 0–9 → 2°×1° cell.
  lon += parseInt(g[2], 10) * 2.0;
  lat += parseInt(g[3], 10) * 1.0;

  // Subsquare pair: A–X (24 values) → 5′×2.5′ cell. Absent/malformed → square center.
  if (g.length < 6) {
    return { lat: lat + 0.5, lon: lon + 1.0 };
  }
  const subA = g.charCodeAt(4) - 65;
  const subB = g.charCodeAt(5) - 65;
  if (subA < 0 || subA > 23 || subB < 0 || subB > 23) {
    return { lat: lat + 0.5, lon: lon + 1.0 };
  }
  lon += subA * (2.0 / 24.0);
  lat += subB * (1.0 / 24.0);
  // Center of the subsquare.
  return { lat: lat + 1.0 / 48.0, lon: lon + 1.0 / 24.0 };
}

function isDigit(ch: string): boolean {
  return ch >= '0' && ch <= '9';
}

// ---------------------------------------------------------------------------
// Function 2 — latLonToUsState
// ---------------------------------------------------------------------------

// Minimal GeoJSON shape we consume from the bundled dataset.
interface StateFeature {
  properties: { usps: string };
  geometry:
    | { type: 'Polygon'; coordinates: number[][][] }
    | { type: 'MultiPolygon'; coordinates: number[][][][] };
}

const STATE_FEATURES = (statesGeoJson as { features: StateFeature[] }).features;

/**
 * Point-in-polygon over bundled simplified US state boundaries.
 * Returns the USPS 2-letter code for a point inside a state; null for
 * ocean / non-US.
 *
 * Boundary accuracy: the dataset is display-simplified (see the sourcing note),
 * so points within ~1 km of a state line may resolve to the neighbour. Treat
 * the result as a regional hint, not an authoritative jurisdiction.
 *
 * MultiPolygon (Hawaii, Alaska/Aleutians, island/peninsula states) is handled
 * by iterating every polygon; each polygon's first ring is the outer boundary
 * and subsequent rings are holes (a point in a hole is excluded).
 */
export function latLonToUsState(lat: number, lon: number): string | null {
  for (const f of STATE_FEATURES) {
    if (f.geometry.type === 'Polygon') {
      if (pointInPolygonWithHoles(lon, lat, f.geometry.coordinates)) {
        return f.properties.usps;
      }
    } else {
      for (const poly of f.geometry.coordinates) {
        if (pointInPolygonWithHoles(lon, lat, poly)) {
          return f.properties.usps;
        }
      }
    }
  }
  return null;
}

/**
 * A single GeoJSON polygon: rings[0] is the outer boundary, rings[1..] are
 * holes. A point is "in" the polygon iff it is inside the outer ring and not
 * inside any hole. Coordinates are [lon, lat].
 */
function pointInPolygonWithHoles(
  lon: number,
  lat: number,
  rings: number[][][],
): boolean {
  if (rings.length === 0) return false;
  if (!pointInRing(lon, lat, rings[0])) return false;
  for (let i = 1; i < rings.length; i++) {
    if (pointInRing(lon, lat, rings[i])) return false; // inside a hole
  }
  return true;
}

/**
 * Standard ray-casting (even–odd rule) point-in-ring test.
 * `ring` is an array of [lon, lat] vertices.
 *
 * Antimeridian-safe: a ring whose longitude span exceeds 180° actually wraps
 * across ±180° (Alaska's Aleutian arm, Pacific island offices). Naive longitude
 * comparison miscounts ray crossings at the seam, silently mis-resolving points
 * near the dateline. When a wrap is detected, all ring longitudes AND the test
 * point are shifted into a continuous 0–360 frame before ray-casting. Rings that
 * do not wrap (the entire CONUS) are unaffected (tuxlink-z1b7 DoD #11).
 */
function pointInRing(lon: number, lat: number, ring: number[][]): boolean {
  let minLon = Infinity;
  let maxLon = -Infinity;
  for (const v of ring) {
    if (v[0] < minLon) minLon = v[0];
    if (v[0] > maxLon) maxLon = v[0];
  }
  const wraps = maxLon - minLon > 180;
  const fx = (x: number): number => (wraps && x < 0 ? x + 360 : x);
  const px = wraps && lon < 0 ? lon + 360 : lon;

  let inside = false;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const xi = fx(ring[i][0]);
    const yi = ring[i][1];
    const xj = fx(ring[j][0]);
    const yj = ring[j][1];
    const intersects =
      yi > lat !== yj > lat &&
      px < ((xj - xi) * (lat - yi)) / (yj - yi) + xi;
    if (intersects) inside = !inside;
  }
  return inside;
}

// ---------------------------------------------------------------------------
// Function 3 — gridToNwsZone
// ---------------------------------------------------------------------------

export interface NwsZone { id: string; name: string; state: string; }

interface ZoneFeature {
  properties: { id: string; name: string; state: string };
  geometry:
    | { type: 'Polygon'; coordinates: number[][][] }
    | { type: 'MultiPolygon'; coordinates: number[][][][] };
}
const ZONE_FEATURES = (nwsZonesGeoJson as { features: ZoneFeature[] }).features;

/** Point-in-polygon over bundled NWS public-zone geometry. Returns the zone
 *  covering the point, or null for ocean / non-US / a gap in the bundled set.
 *  Same ray-casting technique as latLonToUsState. */
export function gridToNwsZone(lat: number, lon: number): NwsZone | null {
  for (const f of ZONE_FEATURES) {
    const polys =
      f.geometry.type === 'Polygon'
        ? [f.geometry.coordinates]
        : f.geometry.coordinates;
    for (const poly of polys) {
      if (pointInPolygonWithHoles(lon, lat, poly)) return f.properties;
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Function 4 — gridToRadarRegion
// ---------------------------------------------------------------------------

export interface RadarRegion { filename: string; name: string; bbox: [number, number, number, number]; }
const RADAR_REGIONS = (radarRegionsJson as unknown as { regions: RadarRegion[] }).regions
  .filter((r) => Array.isArray(r.bbox) && r.bbox.length === 4);

/** Smallest-area radar region whose bbox contains the point; null if none. */
export function gridToRadarRegion(lat: number, lon: number): RadarRegion | null {
  let best: RadarRegion | null = null;
  let bestArea = Infinity;
  for (const r of RADAR_REGIONS) {
    const [w, s, e, n] = r.bbox;
    if (lon >= w && lon <= e && lat >= s && lat <= n) {
      const area = (e - w) * (n - s);
      if (area < bestArea) { bestArea = area; best = r; }
    }
  }
  return best;
}

// ---------------------------------------------------------------------------
// Function 5 — latLonToSeaArea
// ---------------------------------------------------------------------------

/**
 * The marine sea-area categories that exist in the catalog. `WX_PACIFIC`
 * (open/broad Pacific) and `WX_US_COAST` (generic US-coastal fallback) are
 * valid catalog categories included in the union for completeness; the
 * coordinate-band resolver below currently routes only to the three regions it
 * can distinguish geographically (EASTPAC / CAR_GULF / ATLANTIC).
 */
export type SeaArea =
  | 'WX_EASTPAC'
  | 'WX_PACIFIC'
  | 'WX_ATLANTIC'
  | 'WX_CAR_GULF'
  | 'WX_US_COAST';

/**
 * Map a coastal point to one of the catalog's marine categories:
 *   'WX_EASTPAC'  — US Pacific coast / NE Pacific
 *   'WX_PACIFIC'  — broader / open Pacific
 *   'WX_ATLANTIC' — US Atlantic coast
 *   'WX_CAR_GULF' — Gulf of Mexico / Caribbean
 *   'WX_US_COAST' — generic US coastal fallback
 *
 * Returns null for interior points with no coast within a reasonable distance
 * (inland-exclusion rule).
 *
 * Boundaries + precedence (ADREV REVISION #7). Coordinates are decimal degrees;
 * longitudes are negative in the western hemisphere.
 *
 * Inland-exclusion: a point is considered coastal only if it lies within the
 * lat/lon envelope of one of the bands below. The bands are drawn to hug the
 * coast (and the immediate offshore/onshore margin a station near the water
 * occupies); interior cities (Phoenix, Denver, Chicago) fall outside every
 * band and resolve to null.
 *
 * Precedence when bands overlap: Pacific is tested first (it is geographically
 * disjoint from the others), then GULF, then ATLANTIC. The Gulf and Atlantic
 * bands meet at the southern tip of Florida; the Gulf band stops at the
 * Florida peninsula's west side (lon ≤ -81.0) so that the Florida east coast
 * (Miami, lon ≈ -80.2) resolves to ATLANTIC, not GULF.
 */
export function latLonToSeaArea(lat: number, lon: number): SeaArea | null {
  // --- Pacific (US west coast + SE Alaska panhandle), NE Pacific ---
  // Coast runs roughly along lon -117 (San Diego) to -125 (WA/OR outer coast),
  // lat ~32 (San Diego) to ~49 (US/Canada border). Allow an onshore margin to
  // ~-124.8..-116.5 and a modest offshore reach.
  if (lat >= 32.0 && lat <= 49.5 && lon >= -125.5 && lon <= -116.5) {
    return 'WX_EASTPAC';
  }

  // --- Gulf of Mexico / Caribbean ---
  // Gulf coast spans lon -97.5 (TX) to -81.0 (FL west coast / Keys), lat 24.5
  // (Keys) to 31.0 (LA/MS/AL/FL panhandle coast). The eastern cap at -81.0
  // keeps the FL east coast out of the Gulf band (precedence vs Atlantic).
  if (lat >= 24.5 && lat <= 31.0 && lon >= -97.5 && lon <= -81.0) {
    return 'WX_CAR_GULF';
  }

  // --- US Atlantic coast ---
  // Eastern seaboard from FL (lat ~25) to ME (lat ~45). Coastline longitude
  // ranges from ~-67 (Maine) to ~-81.5 (GA/north FL). Allow an onshore margin
  // to ~-82.0 and a modest offshore reach to ~-66.0.
  if (lat >= 24.0 && lat <= 45.5 && lon >= -82.0 && lon <= -66.0) {
    return 'WX_ATLANTIC';
  }

  // Interior / no coast within range.
  return null;
}
