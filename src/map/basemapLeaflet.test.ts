import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the seam libs so the test asserts WIRING, not render. (A real
// `leafletLayer` returns a Leaflet GridLayer that tries to load/decode PMTiles to
// canvas — impossible in jsdom. The seam contract under test is the OPTIONS object
// passed to `leafletLayer`, not the render — the real seam is proven via grim in
// Task 7.) `vi.mock` is hoisted above imports, so the spies it references must be
// hoisted too (`vi.hoisted`) or they are in the temporal-dead-zone at mock-eval.
const { leafletLayerSpy, pmPaintRulesSpy, pmLabelRulesSpy, namedFlavorSpy, pmtilesCtor } =
  vi.hoisted(() => ({
    leafletLayerSpy: vi.fn((opts: Record<string, unknown>) => ({ __pm: true, opts })),
    pmPaintRulesSpy: vi.fn(() => [{ dataLayer: 'roads', symbolizer: {} }]),
    pmLabelRulesSpy: vi.fn(() => []),
    namedFlavorSpy: vi.fn((n: string) => ({ __flavor: n })),
    pmtilesCtor: vi.fn().mockImplementation((url: string) => ({ __pmtiles: true, url })),
  }));
vi.mock('../vendor/protomaps-leaflet', () => ({
  leafletLayer: leafletLayerSpy,
  paintRules: pmPaintRulesSpy,
  labelRules: pmLabelRulesSpy,
}));
vi.mock('pmtiles', () => ({ PMTiles: pmtilesCtor }));
vi.mock('@protomaps/basemaps', () => ({ namedFlavor: namedFlavorSpy }));

import { buildBaseLayers, PMTILES_TILE_URL, OSM_ATTRIBUTION } from './basemapLeaflet';

const optsOf = (i: number) => leafletLayerSpy.mock.calls[i][0] as Record<string, any>;

beforeEach(() => {
  leafletLayerSpy.mockClear();
  pmPaintRulesSpy.mockClear();
  namedFlavorSpy.mockClear();
});

describe('basemapLeaflet', () => {
  it('overview: one flavored layer over a PMTiles INSTANCE of the world seam, maxDataZoom 6, zIndex 1', () => {
    const layers = buildBaseLayers('dark', []);
    expect(layers).toHaveLength(1);
    expect(leafletLayerSpy).toHaveBeenCalledTimes(1);
    const o = optsOf(0);
    expect(o.flavor).toBe('dark'); // overview carries the flavor (background + labels)
    expect(o.url.__pmtiles).toBe(true); // a PMTiles INSTANCE, not a string (R2 P0#1)
    expect(o.url.url).toBe('tile://pmtiles/world');
    expect(o.maxDataZoom).toBe(6); // overzoom cap so it never requests z7+ the z0-6 archive lacks (R2 P0#2)
    expect(o.zIndex).toBe(1);
    expect(o.attribution).toBe(OSM_ATTRIBUTION);
    // Software-GL smoothness tuning (operator smoke): scale-don't-repaint on zoom,
    // keep more tiles on pan, render fewer pixels per tile.
    expect(o.updateWhenZooming).toBe(false);
    expect(o.keepBuffer).toBe(4);
    expect(o.devicePixelRatio).toBe(1);
  });

  it('pack: NO flavor, NO backgroundColor, explicit paintRules + empty labelRules, maxDataZoom, minZoom 6, higher zIndex (R2 P0#3)', () => {
    const layers = buildBaseLayers('dark', [{ id: 'continent-na', maxZoom: 14 }]);
    expect(layers).toHaveLength(2);
    const p = optsOf(1);
    expect(p.url.__pmtiles).toBe(true);
    expect(p.url.url).toBe('tile://pmtiles/continent-na');
    expect(p.flavor).toBeUndefined(); // packs are NOT flavored (empty pack tiles would mask the overview)
    expect(p.backgroundColor).toBeUndefined();
    expect(Array.isArray(p.paintRules)).toBe(true); // explicit paint rules from namedFlavor(flavor)
    expect(p.labelRules).toEqual([]); // labels owned by the overview (no duplicate glyph cost)
    expect(p.maxDataZoom).toBe(14);
    expect(p.minZoom).toBe(6);
    expect(p.zIndex).toBeGreaterThan(optsOf(0).zIndex); // packs paint above the overview
    // pack paint rules are derived from the SAME flavor as the overview
    expect(namedFlavorSpy).toHaveBeenCalledWith('dark');
  });

  it('pack maxDataZoom defaults to 14 when the pack has no maxZoom', () => {
    buildBaseLayers('light', [{ id: 'continent-na' }]);
    expect(optsOf(1).maxDataZoom).toBe(14);
  });

  it('passes the light flavor through to the overview', () => {
    buildBaseLayers('light', []);
    expect(optsOf(0).flavor).toBe('light');
  });

  it('exposes the tile:// URL helper and the OSM attribution', () => {
    expect(PMTILES_TILE_URL('world')).toBe('tile://pmtiles/world');
    expect(PMTILES_TILE_URL('continent-na')).toBe('tile://pmtiles/continent-na');
    expect(OSM_ATTRIBUTION).toBe('© OpenStreetMap contributors');
  });
});
