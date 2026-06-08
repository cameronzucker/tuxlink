// Local great-circle distance for distance-sorted station results.
//
// TODO: replace with CF's shared helper once it lands. The Contacts+Favorites agent
// (shoal-raven-gorge, tuxlink-raez) creates `src/forms/position/distance.ts` exporting
// `haversineKm` + `distanceBetweenGrids` (deliberately NOT in maidenhead.ts, to avoid a merge
// conflict on the one file both plans agree a2gd must not modify). Post-merge swap:
//   distanceKm        -> haversineKm
//   distanceFromGrids -> distanceBetweenGrids
// (a2gd still imports `gridToLatLon` from maidenhead.ts read-only.)

import { gridToLatLon, type LatLon } from '../forms/position/maidenhead';

const EARTH_RADIUS_KM = 6371;
const toRad = (d: number) => (d * Math.PI) / 180;

export function distanceKm(a: LatLon, b: LatLon): number {
  const dLat = toRad(b.lat - a.lat);
  const dLon = toRad(b.lon - a.lon);
  const lat1 = toRad(a.lat);
  const lat2 = toRad(b.lat);
  const h =
    Math.sin(dLat / 2) ** 2 + Math.cos(lat1) * Math.cos(lat2) * Math.sin(dLon / 2) ** 2;
  return 2 * EARTH_RADIUS_KM * Math.asin(Math.min(1, Math.sqrt(h)));
}

export function distanceFromGrids(gridA: string, gridB: string): number | null {
  const a = gridToLatLon(gridA);
  const b = gridToLatLon(gridB);
  if (!a || !b) return null;
  return distanceKm(a, b);
}

export const kmToMi = (km: number) => km * 0.621371;
