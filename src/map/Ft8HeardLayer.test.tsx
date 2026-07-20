import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, act, waitFor } from '@testing-library/react';
import L from 'leaflet';
import type { LiveDecodeRow } from '../catalog/LiveDecodesTab';

// Real Leaflet map in jsdom (no engine mock), mirroring PeerLayer.test.tsx and
// StationFinderMap.test.tsx: the layer must be raw L.circleMarker on the SVG
// renderer, so the test inspects the LIVE marker objects on the real map
// rather than a mocked layer.

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
import { Ft8HeardLayer, rampFor, SNR_HOT_DB, SNR_WARM_DB } from './Ft8HeardLayer';

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

/** All heard-station markers on the live map, identified by the layer's
 *  distinctive style (radius 4, no stroke), which distinguishes them from
 *  any other circle marker a sibling layer might place on the same map. */
function heardMarkers(): L.CircleMarker[] {
  const out: L.CircleMarker[] = [];
  captured!.eachLayer((l) => {
    if (l instanceof L.CircleMarker && l.options.radius === 4 && l.options.stroke === false) out.push(l);
  });
  return out;
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

describe('rampFor', () => {
  it('returns the hot colour at/above SNR_HOT_DB', () => {
    expect(rampFor(SNR_HOT_DB)).toBe('#ff5470');
    expect(rampFor(-4)).toBe('#ff5470');
  });

  it('returns the warm colour between SNR_WARM_DB and SNR_HOT_DB', () => {
    expect(rampFor(SNR_WARM_DB)).toBe('#ffcf5c');
    expect(rampFor(-13)).toBe('#ffcf5c');
  });

  it('returns the quiet colour below SNR_WARM_DB', () => {
    expect(rampFor(-19)).toBe('#5c92b3');
  });
});

describe('Ft8HeardLayer (Task 4)', () => {
  it('plots one SNR-coloured marker per gridded row at its grid centroid', async () => {
    const rows: LiveDecodeRow[] = [
      row({ call: 'W7GTE', grid: 'DN26', bestSnrDb: -4 }),
      row({ call: 'K5MDX', grid: 'PM74', bestSnrDb: -19 }),
    ];
    await renderMap(
      <LeafletMap>
        <Ft8HeardLayer rows={rows} enabled />
      </LeafletMap>,
    );
    const markers = heardMarkers();
    expect(markers).toHaveLength(2);

    const hot = markers.find((m) => m.options.fillColor === '#ff5470');
    expect(hot).toBeDefined();
    const quiet = markers.find((m) => m.options.fillColor === '#5c92b3');
    expect(quiet).toBeDefined();
  });

  it('drops a gridless row instead of plotting it', async () => {
    const rows: LiveDecodeRow[] = [row({ call: 'NOGRID', grid: null })];
    await renderMap(
      <LeafletMap>
        <Ft8HeardLayer rows={rows} enabled />
      </LeafletMap>,
    );
    expect(heardMarkers()).toHaveLength(0);
  });

  it('renders no markers at all when disabled (capability-hide, not dimmed)', async () => {
    const rows: LiveDecodeRow[] = [row({ call: 'W7GTE', grid: 'DN26', bestSnrDb: -4 })];
    await renderMap(
      <LeafletMap>
        <Ft8HeardLayer rows={rows} enabled={false} />
      </LeafletMap>,
    );
    expect(heardMarkers()).toHaveLength(0);
  });

  it('binds a tooltip with the callsign and best SNR', async () => {
    const rows: LiveDecodeRow[] = [row({ call: 'W7GTE', grid: 'DN26', bestSnrDb: -4 })];
    await renderMap(
      <LeafletMap>
        <Ft8HeardLayer rows={rows} enabled />
      </LeafletMap>,
    );
    const marker = heardMarkers()[0];
    expect(marker.getTooltip()?.getContent()).toBe('W7GTE · -4 dB');
  });
});
