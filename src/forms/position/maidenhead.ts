/**
 * Maidenhead locator conversions — TypeScript port of
 * src-tauri/src/position/maidenhead.rs.
 *
 * Supports 4-char (field + square) and 6-char (+ subsquare) grids.
 * Returns the CENTER of the grid square so consumers get a representative
 * coordinate without a systematic corner offset.
 */

/** Parsed lat/lon pair (WGS-84 degrees). */
export interface LatLon {
  lat: number;
  lon: number;
}

/**
 * Convert a 4- or 6-char Maidenhead locator to the lat/lon at the CENTER
 * of the square.
 *
 * Returns `null` for malformed input (wrong length, out-of-range characters).
 * Mirrors grid_to_lat_lon() in src-tauri/src/position/maidenhead.rs — keep the
 * two implementations in sync if the algorithm changes.
 */
export function gridToLatLon(grid: string): LatLon | null {
  const g = grid.toUpperCase();
  if (g.length !== 4 && g.length !== 6) return null;

  // Field pair: A–R (18 values)
  const fieldA = g.charCodeAt(0) - 65; // 'A' = 65
  const fieldB = g.charCodeAt(1) - 65;
  if (fieldA < 0 || fieldA > 17 || fieldB < 0 || fieldB > 17) return null;

  // Square pair: 0–9
  if (!isDigit(g[2]) || !isDigit(g[3])) return null;
  const sqA = parseInt(g[2], 10);
  const sqB = parseInt(g[3], 10);

  let lon = fieldA * 20.0 - 180.0 + sqA * 2.0;
  let lat = fieldB * 10.0 - 90.0 + sqB * 1.0;

  if (g.length === 6) {
    // Subsquare pair: a–x (24 values); accept upper or lower input
    const subA = g.charCodeAt(4) - 65; // already uppercased
    const subB = g.charCodeAt(5) - 65;
    if (subA < 0 || subA > 23 || subB < 0 || subB > 23) return null;
    lon += subA * (5.0 / 60.0);
    lat += subB * (2.5 / 60.0);
    // Center of the subsquare (half of the subsquare step size)
    lon += 2.5 / 60.0;
    lat += 1.25 / 60.0;
  } else {
    // Center of the 2°×1° square
    lon += 1.0;
    lat += 0.5;
  }

  return { lat, lon };
}

function isDigit(ch: string): boolean {
  return ch >= '0' && ch <= '9';
}
