/**
 * Pure EPSG4326 (plate-carrée) projection helpers for the offline map.
 *
 * Under Leaflet's `L.CRS.EPSG4326`, pixel↔lat/lon is LINEAR over the bundled
 * world raster, so the load-bearing projection is a pure function unit-tested
 * in jsdom WITHOUT Leaflet. The Leaflet component only *calls* these; map
 * rendering correctness is verified via grim on WebKitGTK, never through the
 * react-leaflet test mock.
 *
 * Longitude is linear in x over [-180, 180]; latitude is linear in y over
 * [90, -90] (image y grows downward = south).
 */

/** A lat/lon pair (WGS-84 degrees), local to the map subsystem. */
export interface LatLon {
  lat: number;
  lon: number;
}

/** World rectangle as Leaflet `[[south, west], [north, east]]` for `ImageOverlay`/`maxBounds`. */
export const WORLD_BOUNDS: [[number, number], [number, number]] = [
  [-90, -180],
  [90, 180],
];

/** World rectangle for EPSG:3857: Leaflet clips Web Mercator at ±85.0511°. */
export const MERCATOR_BOUNDS: [[number, number], [number, number]] = [
  [-85.0511, -180],
  [85.0511, 180],
];

/** Convert an image pixel to lat/lon, clamped to the world rectangle. */
export function pixelToLatLon(px: number, py: number, width: number, height: number): LatLon {
  const lon = (px / width) * 360 - 180;
  const lat = 90 - (py / height) * 180;
  return clampLatLon(lat, lon);
}

/** Convert lat/lon to an image pixel (inverse of {@link pixelToLatLon}). */
export function latLonToPixel(
  lat: number,
  lon: number,
  width: number,
  height: number,
): { x: number; y: number } {
  return { x: ((lon + 180) / 360) * width, y: ((90 - lat) / 180) * height };
}

/** Clamp a lat/lon to the world rectangle [-90, 90] × [-180, 180]. */
export function clampLatLon(lat: number, lon: number): LatLon {
  return {
    lat: Math.min(90, Math.max(-90, lat)),
    lon: Math.min(180, Math.max(-180, lon)),
  };
}
