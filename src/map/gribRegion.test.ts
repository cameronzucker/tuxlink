import { describe, expect, it } from 'vitest';
import { signedToLatitude, signedToLongitude, signedBboxToGribRegion } from './gribRegion';

describe('signed coord → {degrees,dir}', () => {
  it('hemispheres', () => {
    expect(signedToLatitude(33.7)).toEqual({ degrees: 34, dir: 'N' }); // round to whole deg
    expect(signedToLatitude(-33.2)).toEqual({ degrees: 33, dir: 'S' });
    expect(signedToLongitude(-118.4)).toEqual({ degrees: 118, dir: 'W' });
    expect(signedToLongitude(118.6)).toEqual({ degrees: 119, dir: 'E' });
  });
  it('zero is canonical N / E (not S / W)', () => {
    expect(signedToLatitude(0)).toEqual({ degrees: 0, dir: 'N' });
    expect(signedToLongitude(0)).toEqual({ degrees: 0, dir: 'E' });
  });
  it('clamps out-of-range magnitudes to 90 / 180', () => {
    expect(signedToLatitude(95)).toEqual({ degrees: 90, dir: 'N' });
    expect(signedToLongitude(-200)).toEqual({ degrees: 180, dir: 'W' });
  });
});

describe('signedBboxToGribRegion (two signed corners → ordered, whole-degree, non-degenerate)', () => {
  it('normalizes corner order to south/north + west/east and floors/ceils OUTWARD', () => {
    // dragged NE→SW: cornerA = (60.2N,120.9W) cornerB = (40.8N,140.1W)
    const r = signedBboxToGribRegion({ lat: 60.2, lon: -120.9 }, { lat: 40.8, lon: -140.1 });
    expect(r.lat0).toEqual({ degrees: 40, dir: 'N' }); // south, floor outward
    expect(r.lat1).toEqual({ degrees: 61, dir: 'N' }); // north, ceil outward
    expect(r.lon0).toEqual({ degrees: 141, dir: 'W' }); // west (more-negative), expand outward
    expect(r.lon1).toEqual({ degrees: 120, dir: 'W' }); // east
  });
  it('expands a sub-degree drag so the region is never degenerate', () => {
    const r = signedBboxToGribRegion({ lat: 40.2, lon: -120.2 }, { lat: 40.6, lon: -120.6 });
    expect(r.lat0.degrees).toBeLessThan(r.lat1.degrees); // not equal → composer accepts
    expect(r.lon0).toEqual({ degrees: 121, dir: 'W' });
    expect(r.lon1).toEqual({ degrees: 120, dir: 'W' });
  });
  it('handles equator/prime-meridian spanning boxes', () => {
    const r = signedBboxToGribRegion({ lat: -5.3, lon: -3.1 }, { lat: 5.3, lon: 3.1 });
    expect(r.lat0).toEqual({ degrees: 6, dir: 'S' });
    expect(r.lat1).toEqual({ degrees: 6, dir: 'N' });
    expect(r.lon0).toEqual({ degrees: 4, dir: 'W' });
    expect(r.lon1).toEqual({ degrees: 4, dir: 'E' });
  });
  it('clamps a near-pole / near-antimeridian box to ≤90 / ≤180 (composer rejects over-range)', () => {
    const r = signedBboxToGribRegion({ lat: 89.6, lon: 179.6 }, { lat: 88.2, lon: 178.1 });
    expect(r.lat1.degrees).toBeLessThanOrEqual(90); // ceil(89.6)=90, clamped, OK
    expect(r.lon1.degrees).toBeLessThanOrEqual(180);
  });
  it('never emits a degenerate (equal) range — even for an integer-aligned/zero drag', () => {
    const r = signedBboxToGribRegion({ lat: 40, lon: -120 }, { lat: 40, lon: -120 });
    expect(r.lat0.degrees).not.toEqual(r.lat1.degrees); // floor(40)==ceil(40) would collapse → guard must expand
    expect(r.lon0.degrees).not.toEqual(r.lon1.degrees);
  });
  it('expands DOWN at the pole/antimeridian ceiling (cannot expand up past 90/180)', () => {
    // a degenerate drag pinned at the north pole / antimeridian: floor==ceil==90/180
    const r = signedBboxToGribRegion({ lat: 90, lon: 180 }, { lat: 90, lon: 180 });
    expect(r.lat0).toEqual({ degrees: 89, dir: 'N' }); // south pulled down 1°
    expect(r.lat1).toEqual({ degrees: 90, dir: 'N' }); // north stays at the 90 ceiling
    expect(r.lon0).toEqual({ degrees: 179, dir: 'E' }); // west pulled down 1°
    expect(r.lon1).toEqual({ degrees: 180, dir: 'E' }); // east stays at the 180 ceiling
  });
  it('produces an ordered, in-range region for a south-west hemisphere box', () => {
    const r = signedBboxToGribRegion({ lat: -34.1, lon: -58.9 }, { lat: -33.4, lon: -58.2 });
    // Buenos Aires-ish: both corners S/W
    expect(r.lat0).toEqual({ degrees: 35, dir: 'S' }); // south = floor(min) outward = -35
    expect(r.lat1).toEqual({ degrees: 33, dir: 'S' }); // north = ceil(max) = -33
    expect(r.lat0.degrees).toBeGreaterThan(r.lat1.degrees); // in S, larger degrees = further south
    expect(r.lon0).toEqual({ degrees: 59, dir: 'W' });
    expect(r.lon1).toEqual({ degrees: 58, dir: 'W' });
  });
});
