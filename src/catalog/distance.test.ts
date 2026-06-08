import { describe, it, expect } from 'vitest';
import { distanceKm, distanceFromGrids, kmToMi } from './distance';

describe('distanceKm', () => {
  it('is ~0 for identical points', () => {
    expect(distanceKm({ lat: 33.4, lon: -112 }, { lat: 33.4, lon: -112 })).toBeCloseTo(0, 1);
  });

  it('matches a known great-circle distance (Phoenix ↔ LA ≈ 574 km)', () => {
    const d = distanceKm({ lat: 33.45, lon: -112.07 }, { lat: 34.05, lon: -118.24 });
    expect(d).toBeGreaterThan(560);
    expect(d).toBeLessThan(590);
  });
});

describe('distanceFromGrids', () => {
  it('returns a positive distance for two valid grids', () => {
    const d = distanceFromGrids('DM43', 'CM87'); // Phoenix-ish ↔ Bay-Area-ish
    expect(d).not.toBeNull();
    expect(d!).toBeGreaterThan(0);
  });

  it('returns null when a grid is unparseable', () => {
    expect(distanceFromGrids('NOTAGRID', 'DM43')).toBeNull();
  });
});

describe('kmToMi', () => {
  it('converts km to miles', () => {
    expect(kmToMi(100)).toBeCloseTo(62.1371, 2);
  });
});
