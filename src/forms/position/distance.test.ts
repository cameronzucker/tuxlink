import { describe, it, expect } from 'vitest';
import { haversineKm, distanceBetweenGrids } from './distance';

describe('haversineKm', () => {
  it('identical points → 0', () => {
    const p = { lat: 47.6, lon: -122.3 };
    expect(haversineKm(p, p)).toBe(0);
  });

  it('identical points (different object) → 0', () => {
    const a = { lat: 47.6, lon: -122.3 };
    const b = { lat: 47.6, lon: -122.3 };
    expect(haversineKm(a, b)).toBeCloseTo(0, 6);
  });

  it('Nashville → Los Angeles ≈ 2887.26 km (textbook haversine reference)', () => {
    // Reference: Veness 2010 — commonly cited haversine worked example
    const nashville = { lat: 36.12, lon: -86.67 };
    const losAngeles = { lat: 33.94, lon: -118.40 };
    const d = haversineKm(nashville, losAngeles);
    expect(Math.abs(d - 2887.26)).toBeLessThan(1);
  });

  it('symmetry: haversineKm(a,b) === haversineKm(b,a)', () => {
    const nashville = { lat: 36.12, lon: -86.67 };
    const losAngeles = { lat: 33.94, lon: -118.40 };
    const ab = haversineKm(nashville, losAngeles);
    const ba = haversineKm(losAngeles, nashville);
    expect(Math.abs(ab - ba)).toBeLessThan(1e-9);
  });
});

describe('distanceBetweenGrids', () => {
  it('two valid grids return a positive finite number', () => {
    // CN87 = Seattle area, DM79 = Denver area; real separation ~1600–2100 km
    const d = distanceBetweenGrids('CN87', 'DM79');
    expect(d).not.toBeNull();
    expect(d!).toBeGreaterThan(1000);
    expect(d!).toBeLessThan(2500);
    expect(isFinite(d!)).toBe(true);
  });

  it('same grid → 0 (center-to-center identical)', () => {
    const d = distanceBetweenGrids('CN87', 'CN87');
    expect(d).not.toBeNull();
    expect(d!).toBeCloseTo(0, 6);
  });

  it('malformed grid (wrong length) → null', () => {
    expect(distanceBetweenGrids('X', 'CN87')).toBeNull();
    expect(distanceBetweenGrids('CN87', 'X')).toBeNull();
    expect(distanceBetweenGrids('X', 'X')).toBeNull();
  });

  it('malformed grid (out-of-range field letters) → null', () => {
    // ZZ99: Z is out of range for field letters (A–R only)
    expect(distanceBetweenGrids('ZZ99', 'CN87')).toBeNull();
    expect(distanceBetweenGrids('CN87', 'ZZ99')).toBeNull();
  });

  it('null argument → null (no crash)', () => {
    expect(distanceBetweenGrids(null, 'CN87')).toBeNull();
    expect(distanceBetweenGrids('CN87', null)).toBeNull();
    expect(distanceBetweenGrids(null, null)).toBeNull();
  });

  it('undefined argument → null (no crash)', () => {
    expect(distanceBetweenGrids(undefined, 'CN87')).toBeNull();
    expect(distanceBetweenGrids('CN87', undefined)).toBeNull();
    expect(distanceBetweenGrids(undefined, undefined)).toBeNull();
  });
});
