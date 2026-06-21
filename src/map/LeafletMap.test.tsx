import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, act } from '@testing-library/react';
import { StrictMode } from 'react';
import L from 'leaflet';

// Mock the base-layer builder so no real protomaps-leaflet GridLayer loads tiles
// in jsdom (R5 P0/P1) — return an inert LayerGroup. Spy the call count.
const buildBaseLayersSpy = vi.hoisted(() => vi.fn(() => [L.layerGroup()]));
vi.mock('./basemapLeaflet', () => ({
  buildBaseLayers: buildBaseLayersSpy,
  OSM_ATTRIBUTION: '© OpenStreetMap contributors',
  flavorBackground: (f: string) => (f === 'dark' ? '#34373d' : '#cccccc'),
}));
// No backend in jsdom → invoke resolves to an empty pack list.
const invokeMock = vi.hoisted(() => vi.fn(async () => ({ packs: [] })));
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

import { LeafletMap } from './LeafletMap';
import { useLeafletMap } from './LeafletMapContext';

// Leaflet sizes the map from clientWidth/Height; jsdom reports 0. Shim the prototype.
const origW = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
const origH = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight');
beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, value: 800 });
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, value: 600 });
  buildBaseLayersSpy.mockClear();
  invokeMock.mockClear();
});
afterEach(() => {
  if (origW) Object.defineProperty(HTMLElement.prototype, 'clientWidth', origW);
  if (origH) Object.defineProperty(HTMLElement.prototype, 'clientHeight', origH);
});

/** Captures the live map a child sees via context. */
function Capture({ onMap }: { onMap: (m: L.Map | null) => void }) {
  const map = useLeafletMap();
  onMap(map);
  return null;
}

describe('LeafletMap', () => {
  it('renders a map container and provides the map via context after ready', async () => {
    let captured: L.Map | null = null;
    render(
      <LeafletMap initialCenter={{ lat: 33.4, lon: -112.0 }} initialZoom={10}>
        <Capture onMap={(m) => (captured = m)} />
      </LeafletMap>,
    );
    await waitFor(() => expect(captured).not.toBeNull());
    expect(captured!.getZoom()).toBe(10);
  });

  it('calls onMapClick with clamped lat/lon on map click', async () => {
    const onMapClick = vi.fn();
    let captured: L.Map | null = null;
    render(
      <LeafletMap initialCenter={{ lat: 0, lon: 0 }} initialZoom={2} onMapClick={onMapClick}>
        <Capture onMap={(m) => (captured = m)} />
      </LeafletMap>,
    );
    await waitFor(() => expect(captured).not.toBeNull());
    act(() => {
      captured!.fire('click', { latlng: L.latLng(33.4, -112.0) } as L.LeafletMouseEvent);
    });
    expect(onMapClick).toHaveBeenCalledWith({ lat: 33.4, lon: -112.0 });
  });

  it('emits zoom on ready and dedupes a moveend at the same zoom', async () => {
    const onZoomChange = vi.fn();
    let captured: L.Map | null = null;
    render(
      <LeafletMap initialCenter={{ lat: 0, lon: 0 }} initialZoom={5} onZoomChange={onZoomChange}>
        <Capture onMap={(m) => (captured = m)} />
      </LeafletMap>,
    );
    await waitFor(() => expect(captured).not.toBeNull());
    expect(onZoomChange).toHaveBeenCalledWith(5);
    onZoomChange.mockClear();
    act(() => {
      captured!.fire('moveend');
    }); // same zoom
    expect(onZoomChange).not.toHaveBeenCalled();
  });

  it('emits clamped viewport on moveend', async () => {
    const onViewportChange = vi.fn();
    let captured: L.Map | null = null;
    render(
      <LeafletMap initialCenter={{ lat: 10, lon: 20 }} initialZoom={4} onViewportChange={onViewportChange}>
        <Capture onMap={(m) => (captured = m)} />
      </LeafletMap>,
    );
    await waitFor(() => expect(captured).not.toBeNull());
    onViewportChange.mockClear();
    act(() => {
      captured!.setZoom(6);
    });
    expect(onViewportChange).toHaveBeenCalled();
    const [center, zoom] = onViewportChange.mock.calls.at(-1)!;
    expect(center).toHaveProperty('lat');
    expect(center).toHaveProperty('lon');
    expect(typeof zoom).toBe('number');
  });

  it('renders the unavailable panel when construction throws', () => {
    const spy = vi.spyOn(L, 'map').mockImplementationOnce(() => {
      throw new Error('webgl/canvas unavailable');
    });
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { getByTestId } = render(<LeafletMap initialCenter={{ lat: 0, lon: 0 }} initialZoom={2} />);
    expect(getByTestId('map-unavailable')).toBeTruthy();
    spy.mockRestore();
    errSpy.mockRestore();
  });

  it('rebuilds base layers on flavor change but not on a redundant rerender', async () => {
    let captured: L.Map | null = null;
    const { rerender } = render(
      <LeafletMap initialCenter={{ lat: 0, lon: 0 }} initialZoom={2} flavor="dark">
        <Capture onMap={(m) => (captured = m)} />
      </LeafletMap>,
    );
    await waitFor(() => expect(captured).not.toBeNull());
    const afterInit = buildBaseLayersSpy.mock.calls.length;
    expect(afterInit).toBeGreaterThanOrEqual(1);
    // redundant rerender, same flavor → no rebuild
    rerender(
      <LeafletMap initialCenter={{ lat: 0, lon: 0 }} initialZoom={2} flavor="dark">
        <Capture onMap={(m) => (captured = m)} />
      </LeafletMap>,
    );
    expect(buildBaseLayersSpy.mock.calls.length).toBe(afterInit);
    // flavor change → rebuild
    rerender(
      <LeafletMap initialCenter={{ lat: 0, lon: 0 }} initialZoom={2} flavor="light">
        <Capture onMap={(m) => (captured = m)} />
      </LeafletMap>,
    );
    expect(buildBaseLayersSpy.mock.calls.length).toBeGreaterThan(afterInit);
  });

  it('sets native maxBounds (no ported moveend snap-back clamp)', async () => {
    let captured: L.Map | null = null;
    render(
      <LeafletMap initialCenter={{ lat: 0, lon: 0 }} initialZoom={2}>
        <Capture onMap={(m) => (captured = m)} />
      </LeafletMap>,
    );
    await waitFor(() => expect(captured).not.toBeNull());
    const b = captured!.options.maxBounds as L.LatLngBounds;
    expect(b).toBeTruthy();
    expect(b.getNorth()).toBeCloseTo(85.0511, 3);
    expect(b.getSouth()).toBeCloseTo(-85.0511, 3);
    // Tile fade-in disabled (MapLibre fadeDuration:0 parity) so painted tiles snap
    // in instead of fading from transparent ("loading from white") on the software
    // renderer (operator smoke).
    expect(captured!.options.fadeAnimation).toBe(false);
  });

  it('converges to one map under StrictMode double-invoke', async () => {
    let captured: L.Map | null = null;
    render(
      <StrictMode>
        <LeafletMap initialCenter={{ lat: 0, lon: 0 }} initialZoom={3}>
          <Capture onMap={(m) => (captured = m)} />
        </LeafletMap>
      </StrictMode>,
    );
    await waitFor(() => expect(captured).not.toBeNull());
    // a usable single map (no "container already initialized" throw, no leak)
    expect(captured!.getZoom()).toBe(3);
  });
});
