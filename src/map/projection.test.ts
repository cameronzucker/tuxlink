import { describe, expect, it } from 'vitest';
import { pixelToLatLon, latLonToPixel, clampLatLon, WORLD_BOUNDS } from './projection';

describe('EPSG4326 projection (plate carrée, linear)', () => {
  const W = 2048,
    H = 1024;
  it('maps image corners to world corners', () => {
    expect(pixelToLatLon(0, 0, W, H)).toEqual({ lat: 90, lon: -180 }); // top-left
    expect(pixelToLatLon(W, H, W, H)).toEqual({ lat: -90, lon: 180 }); // bottom-right
  });
  it('maps image center to (0,0)', () => {
    expect(pixelToLatLon(W / 2, H / 2, W, H)).toEqual({ lat: 0, lon: 0 });
  });
  it('round-trips pixel→latlon→pixel', () => {
    const px = 512,
      py = 300;
    const { lat, lon } = pixelToLatLon(px, py, W, H);
    const back = latLonToPixel(lat, lon, W, H);
    expect(back.x).toBeCloseTo(px, 6);
    expect(back.y).toBeCloseTo(py, 6);
  });
  it('round-trips latlon→pixel→latlon for an interior point', () => {
    const lat = 37.5,
      lon = -122.25;
    const { x, y } = latLonToPixel(lat, lon, W, H);
    const back = pixelToLatLon(x, y, W, H);
    expect(back.lat).toBeCloseTo(lat, 6);
    expect(back.lon).toBeCloseTo(lon, 6);
  });
  it('clamps out-of-range coordinates to the world rectangle', () => {
    expect(clampLatLon(95, 200)).toEqual({ lat: 90, lon: 180 });
    expect(clampLatLon(-95, -200)).toEqual({ lat: -90, lon: -180 });
  });
  it('clamp is idempotent on in-range values', () => {
    expect(clampLatLon(0, 0)).toEqual({ lat: 0, lon: 0 });
    expect(clampLatLon(-89.9, 179.9)).toEqual({ lat: -89.9, lon: 179.9 });
  });
  it('exposes WORLD_BOUNDS as [[south,west],[north,east]] for ImageOverlay/maxBounds', () => {
    expect(WORLD_BOUNDS).toEqual([
      [-90, -180],
      [90, 180],
    ]);
  });
});
