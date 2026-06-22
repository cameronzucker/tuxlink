/**
 * LocationMap wiring tests (tuxlink-4hol, Leaflet port). The map runs REAL in
 * jsdom (no engine mock); we capture the live L.Map via vi.spyOn(L,'map') and the
 * draggable marker via the layer group. Wiring only — real render/drag is
 * grim-verified (map subsystem C1).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, act, screen, waitFor } from '@testing-library/react';
import L from 'leaflet';
import { latLonToGrid } from '../forms/position/maidenhead';
import { LocationMap } from './LocationMap';

const invokeMock = vi.hoisted(() =>
  vi.fn(async (cmd: string) => (cmd === 'basemap_list_packs' ? { packs: [] } : undefined)),
);
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('../map/basemapLeaflet', () => ({
  buildBaseLayers: vi.fn(() => [L.layerGroup()]),
  OSM_ATTRIBUTION: '© OpenStreetMap contributors',
  flavorBackground: () => '#34373d',
}));

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

/** The draggable location marker (an L.Marker; the lattice labels are also
 * markers but non-draggable, so filter on the drag handler). */
function locationMarker(): L.Marker | undefined {
  let found: L.Marker | undefined;
  captured!.eachLayer((l) => {
    if (l instanceof L.Marker && (l.options as L.MarkerOptions).draggable) found = l;
  });
  return found;
}
function rectangles(): L.Rectangle[] {
  const out: L.Rectangle[] = [];
  captured!.eachLayer((l) => {
    if (l instanceof L.Rectangle) out.push(l);
  });
  return out;
}

describe('LocationMap (Leaflet)', () => {
  it('renders the map container and constructs a map', async () => {
    await renderMap(<LocationMap grid="EM75km" fixLatLon={null} selectedSource="manual" onGridChange={vi.fn()} />);
    expect(screen.getByTestId('location-map')).toBeInTheDocument();
    expect(captured).toBeTruthy();
  });

  it('clicking the map sets the grid for the clicked point', async () => {
    const onGridChange = vi.fn();
    await renderMap(<LocationMap grid="" fixLatLon={null} selectedSource="manual" onGridChange={onGridChange} />);
    act(() => {
      captured!.fire('click', { latlng: L.latLng(36.1, -86.8) } as L.LeafletMouseEvent);
    });
    expect(onGridChange).toHaveBeenCalledWith(latLonToGrid(36.1, -86.8));
  });

  it('dragging the marker sets the grid by hand (flow 3): commits the dropped point on dragend', async () => {
    const onGridChange = vi.fn();
    await renderMap(<LocationMap grid="EM75km" fixLatLon={null} selectedSource="manual" onGridChange={onGridChange} />);
    const marker = locationMarker()!;
    expect(marker).toBeDefined();
    // Simulate a native Leaflet drag: move the marker, then release.
    act(() => {
      marker.setLatLng([35.0, -90.0]);
      marker.fire('dragend');
    });
    expect(onGridChange).toHaveBeenCalledWith(latLonToGrid(35.0, -90.0));
  });

  it('places the marker at the precise GPS fix when a GPS source is active', async () => {
    await renderMap(
      <LocationMap grid="EM75km" fixLatLon={{ lat: 36.1, lon: -86.8 }} selectedSource="gpsd" onGridChange={vi.fn()} />,
    );
    const marker = locationMarker()!;
    expect(marker.getLatLng().lat).toBeCloseTo(36.1, 4);
    expect(marker.getLatLng().lng).toBeCloseTo(-86.8, 4);
  });

  it('places the marker at the grid centre in manual mode (an arriving fix does not yank it)', async () => {
    const ll = (await import('../forms/position/maidenhead')).gridToLatLon('EM75km')!;
    await renderMap(
      <LocationMap grid="EM75km" fixLatLon={{ lat: 10, lon: 10 }} selectedSource="manual" onGridChange={vi.fn()} />,
    );
    const marker = locationMarker()!;
    expect(marker.getLatLng().lat).toBeCloseTo(ll.lat, 4); // grid centre, not the fix
    expect(marker.getLatLng().lng).toBeCloseTo(ll.lon, 4);
  });

  it('draws the grid-square highlight + the marker when a grid is set', async () => {
    await renderMap(
      <LocationMap grid="EM75km" fixLatLon={{ lat: 36.1, lon: -86.8 }} selectedSource="gpsd" onGridChange={vi.fn()} />,
    );
    expect(rectangles()).toHaveLength(1);
    expect(locationMarker()).toBeDefined();
  });

  // tuxlink-ivfr: the location-pin divIcon html is set via innerHTML, so a parsed
  // inline `style` is blocked by the production Tauri CSP `style-src` nonce. The
  // pin must be styled by the .location-pin CSS class, never inline.
  it('styles the location pin via a CSS class, not a CSP-blocked inline style', async () => {
    const { container } = await renderMap(
      <LocationMap grid="EM75km" fixLatLon={{ lat: 36.1, lon: -86.8 }} selectedSource="gpsd" onGridChange={vi.fn()} />,
    );
    const pin = container.querySelector('.location-pin');
    expect(pin).not.toBeNull();
    expect(pin!.getAttribute('style')).toBeNull();
  });

  // tuxlink-gf5s: with a GPS source active the live fix updates every tick; the
  // camera must hold still (marker tracks the fix, not the map) so the operator can
  // pan to hand-set. Passing the live fix as initialCenter re-flyTo'd on every tick.
  it('does not chase the live GPS fix — stable center, no flyTo churn', async () => {
    const { rerender } = await renderMap(
      <LocationMap grid="EM75km" fixLatLon={{ lat: 33.4, lon: -112.0 }} selectedSource="gpsd" onGridChange={vi.fn()} />,
    );
    const flySpy = vi.spyOn(captured!, 'flyTo');
    // A fresh GPS fix arrives at a far-away location.
    await act(async () => {
      rerender(
        <LocationMap grid="EM75km" fixLatLon={{ lat: 40.7, lon: -74.0 }} selectedSource="gpsd" onGridChange={vi.fn()} />,
      );
      await Promise.resolve();
    });
    expect(flySpy).not.toHaveBeenCalled();
  });
});
