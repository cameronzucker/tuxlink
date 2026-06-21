/**
 * PositionMapWidget tests — SHAPE/WIRING ONLY (C1).
 *
 * Leaflet re-expression: the map runs REAL in jsdom (no engine mock); we capture
 * the live L.Map via vi.spyOn(L,'map') and inspect its layers. These assert the
 * logical structure: a circleMarker pin + an L.rectangle grid-square at the grid
 * lat/lon, a click firing onGridChange with a 6-char grid, an invalid grid → no
 * overlay, and the onZoomChange bridge. Real render is grim-verified; the offline
 * PMTiles seam is a substrate property covered by basemapLeaflet/LeafletMap tests.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, act, waitFor } from '@testing-library/react';
import L from 'leaflet';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';

// LeafletMap fetches packs via invoke('basemap_list_packs') (wants {packs}).
const invokeMock = vi.hoisted(() =>
  vi.fn(async (cmd: string) => (cmd === 'basemap_list_packs' ? { packs: [] } : undefined)),
);
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

// Mock the base-layer builder → inert layer (PMTiles fetch/decode is grim-verified).
vi.mock('../map/basemapLeaflet', () => ({
  buildBaseLayers: vi.fn(() => [L.layerGroup()]),
  OSM_ATTRIBUTION: '© OpenStreetMap contributors',
  flavorBackground: () => '#34373d',
}));

import { PositionMapWidget } from './PositionMapWidget';

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

async function renderMap(ui: React.ReactElement) {
  const result = render(ui);
  await act(async () => {
    await Promise.resolve();
  });
  await waitFor(() => expect(captured).not.toBeNull());
  return result;
}

function dots(): L.CircleMarker[] {
  const out: L.CircleMarker[] = [];
  captured!.eachLayer((l) => {
    if (l instanceof L.CircleMarker) out.push(l);
  });
  return out;
}
function rectangles(): L.Rectangle[] {
  const out: L.Rectangle[] = [];
  captured!.eachLayer((l) => {
    if (l instanceof L.Rectangle) out.push(l);
  });
  return out;
}

describe('<PositionMapWidget> (offline, shape only)', () => {
  it('renders a pin + grid-square at the grid center', async () => {
    await renderMap(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);
    const ll = gridToLatLon('CN87us')!;
    const pin = dots()[0];
    expect(pin).toBeDefined();
    expect(pin.getLatLng().lat).toBeCloseTo(ll.lat, 4);
    expect(pin.getLatLng().lng).toBeCloseTo(ll.lon, 4);
    expect(rectangles()).toHaveLength(1); // the grid-square highlight
  });

  it('clicking the map fires onGridChange with the 6-char grid', async () => {
    const onGridChange = vi.fn();
    await renderMap(<PositionMapWidget grid="CN87us" onGridChange={onGridChange} />);
    const ll = gridToLatLon('JN58td')!;
    act(() => {
      captured!.fire('click', { latlng: L.latLng(ll.lat, ll.lon) } as L.LeafletMouseEvent);
    });
    expect(onGridChange).toHaveBeenCalledOnce();
    const result = onGridChange.mock.calls[0][0] as string;
    expect(result).toHaveLength(6);
    expect(result).toBe(latLonToGrid(ll.lat, ll.lon));
  });

  it('invalid grid renders the map with no pin or square', async () => {
    await renderMap(<PositionMapWidget grid="ZZ99" onGridChange={vi.fn()} />);
    expect(dots()).toHaveLength(0);
    expect(rectangles()).toHaveLength(0);
  });

  it('forwards onZoomChange (seeded on ready with the real zoom)', async () => {
    const onZoomChange = vi.fn();
    await renderMap(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} onZoomChange={onZoomChange} />);
    expect(onZoomChange).toHaveBeenCalledWith(6); // initialZoom for a placed grid
  });
});
