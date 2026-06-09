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
  const g = grid.toUpperCase();
  if (g.length !== 2 && g.length !== 4) return null;

  // Field pair: A–R (18 values)
  const fieldA = g.charCodeAt(0) - 65; // 'A' = 65
  const fieldB = g.charCodeAt(1) - 65;
  if (fieldA < 0 || fieldA > 17 || fieldB < 0 || fieldB > 17) return null;

  let lon = fieldA * 20.0 - 180.0;
  let lat = fieldB * 10.0 - 90.0;

  if (g.length === 2) {
    // Center of the 20°×10° field
    lon += 10.0;
    lat += 5.0;
    return { lat, lon };
  }

  // Square pair: 0–9
  if (!isDigit(g[2]) || !isDigit(g[3])) return null;
  const sqA = parseInt(g[2], 10);
  const sqB = parseInt(g[3], 10);
  lon += sqA * 2.0;
  lat += sqB * 1.0;
  // Center of the 2°×1° square
  lon += 1.0;
  lat += 0.5;

  return { lat, lon };
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
 */
function pointInRing(lon: number, lat: number, ring: number[][]): boolean {
  let inside = false;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const xi = ring[i][0];
    const yi = ring[i][1];
    const xj = ring[j][0];
    const yj = ring[j][1];
    const intersects =
      yi > lat !== yj > lat &&
      lon < ((xj - xi) * (lat - yi)) / (yj - yi) + xi;
    if (intersects) inside = !inside;
  }
  return inside;
}

// ---------------------------------------------------------------------------
// Function 3 — latLonToSeaArea
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
