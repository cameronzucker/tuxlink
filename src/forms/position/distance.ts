import { gridToLatLon, type LatLon } from './maidenhead';

/** Great-circle distance in kilometers between two WGS-84 points (haversine, R=6371 km). */
export function haversineKm(a: LatLon, b: LatLon): number {
  const R = 6371;
  const dLat = ((b.lat - a.lat) * Math.PI) / 180;
  const dLon = ((b.lon - a.lon) * Math.PI) / 180;
  const lat1 = (a.lat * Math.PI) / 180;
  const lat2 = (b.lat * Math.PI) / 180;

  const sinHalfDlat = Math.sin(dLat / 2);
  const sinHalfDlon = Math.sin(dLon / 2);
  const h =
    sinHalfDlat * sinHalfDlat +
    Math.cos(lat1) * Math.cos(lat2) * sinHalfDlon * sinHalfDlon;

  return 2 * R * Math.asin(Math.sqrt(h));
}

/**
 * Distance in km between two Maidenhead grids (each converted via gridToLatLon to its
 * square center). Returns null if EITHER grid is null OR malformed (gridToLatLon → null).
 * Accepts `string | null | undefined` for ergonomics at the call site (operator grid may be absent).
 */
export function distanceBetweenGrids(
  gridA: string | null | undefined,
  gridB: string | null | undefined,
): number | null {
  if (gridA == null || gridB == null) return null;
  const a = gridToLatLon(gridA);
  const b = gridToLatLon(gridB);
  if (!a || !b) return null;
  return haversineKm(a, b);
}
