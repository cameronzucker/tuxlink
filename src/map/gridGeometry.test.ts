import { describe, expect, it } from 'vitest';
import { gridLines, GridLevel, levelFromZoom } from './gridGeometry';

describe('levelFromZoom — full zoom range', () => {
  it('keeps z0-2 at Field (UNCHANGED legacy behavior)', () => {
    expect(levelFromZoom(0)).toBe(GridLevel.Field);
    expect(levelFromZoom(1)).toBe(GridLevel.Field);
    expect(levelFromZoom(2)).toBe(GridLevel.Field);
  });

  it('keeps z3+ at Square through the legacy mid-range (UNCHANGED at z3)', () => {
    expect(levelFromZoom(3)).toBe(GridLevel.Square);
    expect(levelFromZoom(6)).toBe(GridLevel.Square);
  });

  it('moves to Subsquare in the high zoom band', () => {
    expect(levelFromZoom(9)).toBe(GridLevel.Subsquare);
    expect(levelFromZoom(12)).toBe(GridLevel.Subsquare);
  });

  it('fades the lattice out (null) at very high zoom', () => {
    expect(levelFromZoom(14)).toBeNull();
    expect(levelFromZoom(16)).toBeNull();
  });
});

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

  it('subsquare level → 6-char labels at subsquare (5′×2.5′) spacing', () => {
    // a tiny window around the JJ00 origin subsquare; subsquare lon step = 5/60°,
    // lat step = 2.5/60°.
    const lonStep = 5 / 60;
    const latStep = 2.5 / 60;
    const g = gridLines({ south: 0, west: 0, north: latStep * 2, east: lonStep * 2 }, GridLevel.Subsquare);
    expect(g.lonLines.length).toBeGreaterThan(0);
    expect(g.latLines.length).toBeGreaterThan(0);
    // every subsquare label is exactly 6 chars (field+square+subsquare)
    for (const l of g.labels) expect(l.text).toHaveLength(6);
    // spacing matches the subsquare step
    expect(g.lonLines[1] - g.lonLines[0]).toBeCloseTo(lonStep, 10);
    expect(g.latLines[1] - g.latLines[0]).toBeCloseTo(latStep, 10);
  });
});

// tuxlink-u4k2: the Leaflet overlay renders ONE DOM marker per label, so an
// unbounded label cross-product froze WebKitGTK on a wide Square-level zoom-out
// (confirmed 10k–130k labels). These guard gridLines at the source: clamp to the
// Maidenhead world, cap the label count, and never loop on non-finite bounds.
describe('gridLines — freeze guards (tuxlink-u4k2)', () => {
  const MAX_GRID_LABELS = 2000;

  it('caps labels for a wide Square-level view (the LocationMap zoom-out freeze repro)', () => {
    // A span far wider than the world (low-zoom Leaflet getBounds + padBounds
    // overshoot). Unguarded this is tens of thousands of labels → DOM explosion.
    const g = gridLines({ south: -90, west: -380, north: 90, east: 380 }, GridLevel.Square);
    expect(g.labels.length).toBeLessThanOrEqual(MAX_GRID_LABELS);
    // Lines stay bounded to the world (no phantom-world-copy lattice).
    expect(g.lonLines.length).toBeLessThanOrEqual(200);
    expect(g.latLines.length).toBeLessThanOrEqual(200);
    expect(Math.min(...g.lonLines)).toBeGreaterThanOrEqual(-180);
    expect(Math.max(...g.lonLines)).toBeLessThanOrEqual(180);
  });

  it('returns an empty lattice (no infinite loop) on non-finite bounds', () => {
    const inf = gridLines({ south: 0, west: 0, north: Infinity, east: Infinity }, GridLevel.Square);
    expect(inf.lonLines).toEqual([]);
    expect(inf.latLines).toEqual([]);
    expect(inf.labels).toEqual([]);
    const nan = gridLines({ south: NaN, west: NaN, north: NaN, east: NaN }, GridLevel.Field);
    expect(nan.lonLines).toEqual([]);
    expect(nan.labels).toEqual([]);
  });

  it('leaves a normal in-world view unchanged (labels present, within cap)', () => {
    // ~30°×30° CONUS-ish Square-level window — the working open-at-z6 case.
    const g = gridLines({ south: 20, west: -120, north: 50, east: -90 }, GridLevel.Square);
    expect(g.labels.length).toBeGreaterThan(0);
    expect(g.labels.length).toBeLessThanOrEqual(MAX_GRID_LABELS);
    // still 4-char Square locators (behavior unchanged for an in-world view)
    for (const l of g.labels) expect(l.text).toHaveLength(4);
  });
});

// tuxlink-gf5s: #864 capped LABELS (DOM markers) but left LINES (SVG <path>)
// unbounded. A Subsquare lattice over a world-width window via the bounds/level
// override props is ~8,640 paths — the same storm class. Cap lines symmetrically.
describe('gridLines — line cap (tuxlink-gf5s)', () => {
  const MAX_GRID_LINES = 1000;

  it('drops lines for a Subsquare lattice over a world-width window (override path)', () => {
    const g = gridLines({ south: -90, west: -180, north: 90, east: 180 }, GridLevel.Subsquare);
    // Would be ~4,320 + ~4,320 lines unguarded; above the cap → dropped.
    expect(g.lonLines).toEqual([]);
    expect(g.latLines).toEqual([]);
    // Labels are also above their cap here → empty too (no marker storm).
    expect(g.labels).toEqual([]);
  });

  it('keeps lines for a full-world Square view (under the line cap)', () => {
    const g = gridLines({ south: -90, west: -180, north: 90, east: 180 }, GridLevel.Square);
    // 181 + 181 = 362 lines ≤ cap → present.
    expect(g.lonLines.length + g.latLines.length).toBeGreaterThan(0);
    expect(g.lonLines.length + g.latLines.length).toBeLessThanOrEqual(MAX_GRID_LINES);
  });

  it('keeps the dense lattice at a real Subsquare zoom (tiny viewport, under cap)', () => {
    // z9-13 Subsquare always pairs with a geographically tiny viewport (~1°).
    const g = gridLines({ south: 33.0, west: -112.5, north: 34.0, east: -111.5 }, GridLevel.Subsquare);
    expect(g.lonLines.length).toBeGreaterThan(0);
    expect(g.lonLines.length + g.latLines.length).toBeLessThanOrEqual(MAX_GRID_LINES);
  });
});
