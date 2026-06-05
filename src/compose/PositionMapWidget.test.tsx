/**
 * PositionMapWidget tests.
 *
 * JSDOM does not implement canvas / SVG / WebGL so we cannot run a real
 * Leaflet map in CI. react-leaflet is mocked at the module boundary; the
 * mock renders lightweight divs with data-testid attributes that let us
 * assert on the *logical* structure (tile layer present/absent, marker at
 * the right position, click fires onGridChange) without a real canvas.
 *
 * What this covers:
 *   1. Renders the map container with a marker at the grid's lat/lon.
 *   2. A simulated click fires onGridChange with the correct grid.
 *   3. When navigator.onLine is false, no tile layer is rendered.
 *   4. Invalid grid → still renders (defaults to world view, no marker).
 *
 * Limitation: The offline tile-error detection path (Leaflet tileerror
 * event → setIsOnline(false)) is exercised through the window 'offline'
 * event in test 5 rather than Leaflet's internal event system — the mock
 * replaces the Leaflet map instance. This is a known CI coverage gap.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';

// ── Shared click-handler registry ────────────────────────────────────────────
// useMapEvents stores its click handler here; MapContainer reads it on click.
// Must live outside vi.mock() factory (hoisted to module scope) so both sides
// see the same object.
let _clickHandler: ((lat: number, lng: number) => void) | null = null;

// ── react-leaflet mock ───────────────────────────────────────────────────────
vi.mock('react-leaflet', async () => {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const React = (await import('react')) as typeof import('react');

  function MapContainer({
    children,
    center,
    zoom,
    style,
    'data-testid': testId,
  }: {
    children?: React.ReactNode;
    center?: [number, number];
    zoom?: number;
    style?: React.CSSProperties;
    'data-testid'?: string;
  }) {
    const c = center ?? [0, 0];
    const handleClick = (e: React.MouseEvent<HTMLDivElement>) => {
      const el = e.currentTarget as HTMLDivElement;
      const lat = parseFloat(el.dataset.clickLat ?? '0');
      const lng = parseFloat(el.dataset.clickLng ?? '0');
      if (_clickHandler) _clickHandler(lat, lng);
    };
    return React.createElement(
      'div',
      {
        'data-testid': testId ?? 'leaflet-map-container',
        'data-center-lat': c[0],
        'data-center-lng': c[1],
        'data-zoom': zoom,
        onClick: handleClick,
        style,
      },
      children,
    );
  }

  function TileLayer({ 'data-testid': testId }: { url?: string; attribution?: string; 'data-testid'?: string }) {
    return React.createElement('div', { 'data-testid': testId ?? 'osm-tile-layer' });
  }

  function Marker({ position }: { position: [number, number] }) {
    return React.createElement('div', {
      'data-testid': 'leaflet-marker',
      'data-lat': position[0],
      'data-lng': position[1],
    });
  }

  function Rectangle({ bounds }: { bounds: [[number, number], [number, number]]; pathOptions?: object }) {
    return React.createElement('div', {
      'data-testid': 'leaflet-rectangle',
      'data-bounds': JSON.stringify(bounds),
    });
  }

  // useMap: returns a fake Leaflet Map (on/off stubs only — used by MapInteractor).
  function useMap() {
    return { on: vi.fn(), off: vi.fn() };
  }

  // useMapEvents: register the click handler into module-level _clickHandler.
  function useMapEvents(handlers: { click?: (e: { latlng: { lat: number; lng: number } }) => void }) {
    // Update the shared handler reference on every render so it always reflects
    // the latest closure (avoids stale-handler bugs).
    _clickHandler = handlers.click
      ? (lat: number, lng: number) => handlers.click!({ latlng: { lat, lng } })
      : null;
    return { on: vi.fn(), off: vi.fn() }; // return value is unused by callers
  }

  return { MapContainer, TileLayer, Marker, Rectangle, useMap, useMapEvents };
});

// ── Leaflet core mock ────────────────────────────────────────────────────────
vi.mock('leaflet', () => ({
  default: {
    Icon: {
      Default: {
        prototype: {},
        mergeOptions: vi.fn(),
      },
    },
  },
}));

// ── Asset + CSS mocks ────────────────────────────────────────────────────────
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));

// ── Component import (after mocks are in place) ──────────────────────────────
import { PositionMapWidget } from './PositionMapWidget';

// ── navigator.onLine helpers ─────────────────────────────────────────────────
function setOnlineStatus(online: boolean) {
  Object.defineProperty(navigator, 'onLine', {
    value: online,
    writable: true,
    configurable: true,
  });
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe('<PositionMapWidget>', () => {
  beforeEach(() => {
    _clickHandler = null;
    setOnlineStatus(true);
  });

  afterEach(() => {
    _clickHandler = null;
    setOnlineStatus(true);
  });

  it('renders the map container with a marker at the grid center', () => {
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);

    const container = screen.getByTestId('leaflet-map-container');
    expect(container).toBeInTheDocument();

    const marker = screen.getByTestId('leaflet-marker');
    expect(marker).toBeInTheDocument();

    const ll = gridToLatLon('CN87us');
    expect(ll).not.toBeNull();
    expect(parseFloat(marker.dataset.lat ?? '')).toBeCloseTo(ll!.lat, 4);
    expect(parseFloat(marker.dataset.lng ?? '')).toBeCloseTo(ll!.lon, 4);
  });

  it('renders the grid-square rectangle overlay', () => {
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);
    expect(screen.getByTestId('leaflet-rectangle')).toBeInTheDocument();
  });

  it('clicking on the map fires onGridChange with the 6-char grid', () => {
    const onGridChange = vi.fn();
    render(<PositionMapWidget grid="CN87us" onGridChange={onGridChange} />);

    // Set the lat/lon the simulated click should report on the container.
    const ll = gridToLatLon('JN58td')!;
    const container = screen.getByTestId('leaflet-map-container');
    container.dataset.clickLat = String(ll.lat);
    container.dataset.clickLng = String(ll.lon);

    // Directly invoke the registered click handler (simulates MapContainer's
    // onClick → _clickHandler path without fighting JSDOM's event propagation).
    expect(_clickHandler).not.toBeNull();
    act(() => { _clickHandler!(ll.lat, ll.lon); });

    expect(onGridChange).toHaveBeenCalledOnce();
    const result = onGridChange.mock.calls[0][0] as string;
    expect(result).toHaveLength(6);
    // Round-trip: latLonToGrid of the JN58td center should give JN58td
    expect(result).toBe(latLonToGrid(ll.lat, ll.lon));
  });

  it('renders the OSM tile layer when navigator.onLine is true', () => {
    setOnlineStatus(true);
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);
    expect(screen.getByTestId('osm-tile-layer')).toBeInTheDocument();
  });

  it('does NOT render the tile layer when navigator.onLine is false', () => {
    setOnlineStatus(false);
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);
    expect(screen.queryByTestId('osm-tile-layer')).toBeNull();
  });

  it('switches to offline mode when the window fires the offline event', () => {
    setOnlineStatus(true);
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);

    // Initially online → tile layer visible
    expect(screen.getByTestId('osm-tile-layer')).toBeInTheDocument();

    // Fire the browser offline event → component's effect handler calls setIsOnline(false)
    act(() => { window.dispatchEvent(new Event('offline')); });

    // Tile layer should disappear after the state update
    expect(screen.queryByTestId('osm-tile-layer')).toBeNull();
  });

  it('invalid grid still renders the map container without marker or rectangle', () => {
    render(<PositionMapWidget grid="ZZ99" onGridChange={vi.fn()} />);
    expect(screen.getByTestId('leaflet-map-container')).toBeInTheDocument();
    // No marker for invalid grid
    expect(screen.queryByTestId('leaflet-marker')).toBeNull();
    // No rectangle either
    expect(screen.queryByTestId('leaflet-rectangle')).toBeNull();
    // MapContainer center should default to 0,0 (world view)
    const container = screen.getByTestId('leaflet-map-container');
    expect(parseFloat(container.dataset.centerLat ?? '')).toBe(0);
    expect(parseFloat(container.dataset.centerLng ?? '')).toBe(0);
  });
});
