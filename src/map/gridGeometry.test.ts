import { describe, expect, it } from 'vitest';
import { gridLines, GridLevel } from './gridGeometry';

describe('maidenhead overlay geometry', () => {
  it('world view → field lines at 20°/10° spacing', () => {
    const g = gridLines({ south: -90, west: -180, north: 90, east: 180 }, GridLevel.Field);
    expect(g.lonLines).toContain(-180);
    expect(g.lonLines).toContain(0);
    expect(g.lonLines).toContain(160);
    expect(g.lonLines).toContain(180);
    expect(g.latLines).toContain(-90);
    expect(g.latLines).toContain(0);
    expect(g.latLines).toContain(80);
    expect(g.latLines).toContain(90);
    // exact spacing of 20° (lon) / 10° (lat)
    expect(g.lonLines[1] - g.lonLines[0]).toBe(20);
    expect(g.latLines[1] - g.latLines[0]).toBe(10);
    // -180..180 step 20 = 19 lines; -90..90 step 10 = 19 lines
    expect(g.lonLines).toHaveLength(19);
    expect(g.latLines).toHaveLength(19);
  });

  it('clips lines to the visible window', () => {
    const g = gridLines({ south: -1, west: -2, north: 1, east: 2 }, GridLevel.Square);
    expect(Math.min(...g.lonLines)).toBeGreaterThanOrEqual(-2);
    expect(Math.max(...g.lonLines)).toBeLessThanOrEqual(2);
    expect(Math.min(...g.latLines)).toBeGreaterThanOrEqual(-1);
    expect(Math.max(...g.latLines)).toBeLessThanOrEqual(1);
  });

  it('field labels are the 2-char field of the cell CENTER (origin cell → JJ)', () => {
    const g = gridLines({ south: -90, west: -180, north: 90, east: 180 }, GridLevel.Field);
    // cell with SW corner (lon 0, lat 0) → center (lat 5, lon 10) → 'JJ'
    expect(g.labels).toContainEqual({ lat: 5, lon: 10, text: 'JJ' });
    // every field label is exactly 2 chars
    for (const l of g.labels) expect(l.text).toHaveLength(2);
  });

  it('labels the rightmost (near-antimeridian) field cell correctly', () => {
    const g = gridLines({ south: -90, west: -180, north: 90, east: 180 }, GridLevel.Field);
    // rightmost lon cell SW = 160, center lon = 170 → field 'R' (index 17, the last A–R field)
    expect(g.labels).toContainEqual({ lat: 5, lon: 170, text: 'RJ' });
  });

  it('square level → 4-char labels on a zoomed window', () => {
    const g = gridLines({ south: -1, west: -2, north: 1, east: 2 }, GridLevel.Square);
    // cell SW (lon 0, lat 0) → center (lat 0.5, lon 1) → 'JJ00'
    expect(g.labels).toContainEqual({ lat: 0.5, lon: 1, text: 'JJ00' });
    for (const l of g.labels) expect(l.text).toHaveLength(4);
  });
});
