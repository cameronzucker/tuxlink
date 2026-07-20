import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, act, waitFor } from '@testing-library/react';
import L from 'leaflet';
import type { LiveDecodeRow } from '../catalog/LiveDecodesTab';

// Real Leaflet map in jsdom (no engine mock), mirroring Ft8HeardLayer.test.tsx:
// the layer must be raw L.rectangle on an explicit SVG renderer (the map's
// preferCanvas:true default would otherwise route through canvas, which has
// no 2D context under the Pi's software-GL WebKitGTK), so the test inspects
// the LIVE rectangle objects on the real map.

// LeafletMap fetches packs via invoke('basemap_list_packs') (wants {packs}).
// Guard `cmd` before switching: this mock is called with NO args during
// vitest teardown.
const invokeMock = vi.hoisted(() =>
  vi.fn(async (cmd?: string) => (cmd === 'basemap_list_packs' ? { packs: [] } : undefined)),
);
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

// Mock the base-layer builder → inert layer: a real protomaps-leaflet GridLayer
// would try to fetch/decode PMTiles to canvas in jsdom. Base render is grim-verified.
vi.mock('../map/basemapLeaflet', () => ({
  buildBaseLayers: vi.fn(() => [L.layerGroup()]),
  OSM_ATTRIBUTION: '© OpenStreetMap contributors',
  flavorBackground: () => '#34373d',
}));

import { LeafletMap } from './LeafletMap';
import { Ft8HeatLayer, gridSquareBounds } from './Ft8HeatLayer';

// Leaflet sizes from clientWidth/Height; jsdom reports 0. Shim the prototype.
const origW = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
const origH = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight');

const realLMap = L.map.bind(L);
let captured: L.Map | null = null;

beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, value: 800 });
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, value: 600 });
  captured = null;
  vi.spyOn(L, 'map').mockImplementation(((el: HTMLElement | string, opts?: L.MapOptions) => {
    const m = realLMap(el as HTMLElement, opts);
    captured = m;
    return m;
  }) as typeof L.map);
  window.localStorage.clear();
  invokeMock.mockClear();
});
afterEach(() => {
  vi.restoreAllMocks();
  if (origW) Object.defineProperty(HTMLElement.prototype, 'clientWidth', origW);
  if (origH) Object.defineProperty(HTMLElement.prototype, 'clientHeight', origH);
});

/** Render and flush LeafletMap's whenReady (sync) + async pack fetch. */
async function renderMap(ui: React.ReactElement) {
  const result = render(ui);
  await act(async () => {
    await Promise.resolve();
  });
  await waitFor(() => expect(captured).not.toBeNull());
  return result;
}

/** All choropleth rectangles on the live map. `L.Rectangle` is a distinct
 *  Leaflet class from the circleMarker pins/heard-station dots a sibling
 *  layer might place on the same map, so no style-based filter is needed. */
function heatRects(): L.Rectangle[] {
  const out: L.Rectangle[] = [];
  captured!.eachLayer((l) => {
    if (l instanceof L.Rectangle) out.push(l);
  });
  return out;
}

/** True when a live rectangle's bounds match the expected SW/NE pair within
 *  floating-point tolerance. */
function boundsMatch(rect: L.Rectangle, expected: [[number, number], [number, number]]): boolean {
  const sw = rect.getBounds().getSouthWest();
  const ne = rect.getBounds().getNorthEast();
  return (
    Math.abs(sw.lat - expected[0][0]) < 1e-6 &&
    Math.abs(sw.lng - expected[0][1]) < 1e-6 &&
    Math.abs(ne.lat - expected[1][0]) < 1e-6 &&
    Math.abs(ne.lng - expected[1][1]) < 1e-6
  );
}

function row(over: Partial<LiveDecodeRow> = {}): LiveDecodeRow {
  return {
    call: 'W7GTE',
    grid: 'DN26',
    bestSnrDb: -10,
    count: 1,
    band: '20m',
    lastSlotUtcMs: 1_000_000_000,
    ...over,
  };
}

describe('gridSquareBounds', () => {
  it('returns the literal SW/NE rectangle for a 4-char square (DN26)', () => {
    // Verified against the existing, already-tested gridToLatLon (see
    // maidenhead.test.ts's CN87 case): DN26 → D=lon field 3 → -180+3*20=-120;
    // N=lat field 13 → -90+13*10=40; digits '2'/'6' → lon+2*2=-116,
    // lat+6*1=46. That is the SW corner; the 4-char square spans 2 deg lon by
    // 1 deg lat, so NE = SW + (1 lat, 2 lon) = (47, -114).
    expect(gridSquareBounds('DN26')).toEqual([
      [46, -116],
      [47, -114],
    ]);
  });

  it('is case-insensitive', () => {
    expect(gridSquareBounds('dn26')).toEqual(gridSquareBounds('DN26'));
  });

  it('returns null for input that is not exactly 4 characters', () => {
    expect(gridSquareBounds('DN26oa')).toBeNull();
    expect(gridSquareBounds('DN2')).toBeNull();
    expect(gridSquareBounds('')).toBeNull();
  });

  it('returns null for an out-of-range square (malformed grid)', () => {
    expect(gridSquareBounds('ZZ99')).toBeNull();
  });
});

describe('Ft8HeatLayer (Task 5, spec L5)', () => {
  it('renders one choropleth rectangle per grid square, opacity density-scaled by station count', async () => {
    const rows: LiveDecodeRow[] = [
      row({ call: 'W7GTE', grid: 'DN26' }),
      row({ call: 'W7ABC', grid: 'DN26' }),
      row({ call: 'W7XYZ', grid: 'DN26' }),
      row({ call: 'JA1ABC', grid: 'PM74' }),
    ];
    await renderMap(
      <LeafletMap>
        <Ft8HeatLayer rows={rows} enabled />
      </LeafletMap>,
    );
    const rects = heatRects();
    expect(rects).toHaveLength(2);

    const dn26 = rects.find((r) => boundsMatch(r, gridSquareBounds('DN26')!));
    const pm74 = rects.find((r) => boundsMatch(r, gridSquareBounds('PM74')!));
    expect(dn26).toBeDefined();
    expect(pm74).toBeDefined();

    // scaled = 0.15 + 0.55 * count/maxCount; maxCount = 3 (DN26's station count).
    expect(dn26!.options.fillOpacity).toBeCloseTo(0.15 + 0.55 * (3 / 3), 5);
    expect(pm74!.options.fillOpacity).toBeCloseTo(0.15 + 0.55 * (1 / 3), 5);
    expect(dn26!.options.fillOpacity as number).toBeGreaterThan(pm74!.options.fillOpacity as number);

    expect(dn26!.options.fillColor).toBe('#ff5470');
    expect(dn26!.options.stroke).toBe(false);
  });

  it('skips rows with no grid instead of plotting them', async () => {
    const rows: LiveDecodeRow[] = [row({ call: 'NOGRID', grid: null }), row({ call: 'W7GTE', grid: 'DN26' })];
    await renderMap(
      <LeafletMap>
        <Ft8HeatLayer rows={rows} enabled />
      </LeafletMap>,
    );
    expect(heatRects()).toHaveLength(1);
  });

  it('renders nothing for an empty row set', async () => {
    await renderMap(
      <LeafletMap>
        <Ft8HeatLayer rows={[]} enabled />
      </LeafletMap>,
    );
    expect(heatRects()).toHaveLength(0);
  });

  it('renders no rectangles at all when disabled (capability-hide, not dimmed)', async () => {
    const rows: LiveDecodeRow[] = [row({ call: 'W7GTE', grid: 'DN26' })];
    await renderMap(
      <LeafletMap>
        <Ft8HeatLayer rows={rows} enabled={false} />
      </LeafletMap>,
    );
    expect(heatRects()).toHaveLength(0);
  });
});
