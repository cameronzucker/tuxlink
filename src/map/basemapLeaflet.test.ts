import { describe, it, expect, vi } from 'vitest';

// Mock the vendored protomaps-leaflet so the test asserts WIRING, not render.
// (A real `leafletLayer` returns a Leaflet GridLayer that tries to load/decode
// PMTiles to canvas — impossible in jsdom. The seam contract under test is the
// OPTIONS object passed to `leafletLayer`, not the render.)
// `vi.mock` is hoisted above imports, so the spy it references must also be
// hoisted (a plain top-level `const` is in the temporal-dead-zone at mock-eval
// time → "Cannot access before initialization"). `vi.hoisted` lifts it.
const { leafletLayerSpy } = vi.hoisted(() => ({
  leafletLayerSpy: vi.fn((opts: unknown) => ({ __pm: true, opts })),
}));
vi.mock('../vendor/protomaps-leaflet', () => ({ leafletLayer: leafletLayerSpy }));

import { buildBaseLayers, PMTILES_TILE_URL, OSM_ATTRIBUTION } from './basemapLeaflet';

describe('basemapLeaflet', () => {
  it('builds a single overview layer (dark) over the tile:// world seam when no packs', () => {
    leafletLayerSpy.mockClear();
    const layers = buildBaseLayers('dark', []);
    expect(layers).toHaveLength(1);
    expect(leafletLayerSpy).toHaveBeenCalledTimes(1);
    const opts = leafletLayerSpy.mock.calls[0][0] as Record<string, unknown>;
    expect(opts.flavor).toBe('dark');
    // overview is wired to the world seam (via source or url; assert the URL text appears)
    expect(JSON.stringify(opts)).toContain('tile://pmtiles/world');
  });

  it('appends one pack layer per installed pack, clamped to minZoom 6, above the overview', () => {
    leafletLayerSpy.mockClear();
    const layers = buildBaseLayers('light', [{ id: 'continent-na' }]);
    expect(layers).toHaveLength(2);
    const packOpts = leafletLayerSpy.mock.calls.at(-1)![0] as Record<string, unknown>;
    expect((packOpts.minZoom ?? packOpts.minzoom) as number).toBe(6);
    expect(JSON.stringify(packOpts)).toContain('tile://pmtiles/continent-na');
  });

  it('passes the light flavor through and wires the overview to the world seam', () => {
    leafletLayerSpy.mockClear();
    buildBaseLayers('light', []);
    const opts = leafletLayerSpy.mock.calls[0][0] as Record<string, unknown>;
    expect(opts.flavor).toBe('light');
    expect(JSON.stringify(opts)).toContain('tile://pmtiles/world');
  });

  it('exposes the tile:// URL helper', () => {
    expect(PMTILES_TILE_URL('world')).toBe('tile://pmtiles/world');
    expect(PMTILES_TILE_URL('continent-na')).toBe('tile://pmtiles/continent-na');
  });

  it('exposes the OSM attribution string', () => {
    expect(OSM_ATTRIBUTION).toBe('© OpenStreetMap contributors');
  });
});
