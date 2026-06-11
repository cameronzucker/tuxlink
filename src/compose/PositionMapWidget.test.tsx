/**
 * PositionMapWidget tests — SHAPE ONLY (C1).
 *
 * jsdom cannot render Leaflet; react-leaflet + leaflet are mocked at the module
 * boundary via the canonical map mock. These tests assert logical structure
 * (offline base map present, marker at the grid lat/lon, click fires
 * onGridChange with a 6-char grid, NO online tile layer). Real render is
 * verified via grim on WebKitGTK.
 *
 * What this covers:
 *   1. Renders the offline BaseMap (ImageOverlay) with a marker at the grid.
 *   2. Renders the grid-square rectangle overlay.
 *   3. A click fires onGridChange with the correct 6-char grid.
 *   4. Invalid grid → still renders the map, no marker/rectangle.
 *   5. NEGATIVE: no OSM tile layer / no external tile URL appears (offline-only).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import { fireMapEvent, fireZoomEvent, resetMapMock } from '../map/testMapMock';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
// BaseMap imports world-mercator-2048.png (EPSG:3857 substrate, Task 3 migration)
vi.mock('../map/assets/world-mercator-2048.png', () => ({ default: '/world-mercator-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));
vi.mock('../map/useTileSource', () => ({ useTileSource: vi.fn() }));

import { PositionMapWidget } from './PositionMapWidget';
import { useTileSource } from '../map/useTileSource';
import type { TileSource, TileSourceStatus } from '../map/tileSource';

const LAN_SOURCE: TileSource = {
  url: 'http://192.168.1.10:8080/{z}/{x}/{y}.png',
  scheme: 'Xyz',
  minZoom: 0,
  maxZoom: 16,
  cacheBudgetMb: 256,
  attribution: null,
  label: 'LAN source',
};
const LAN_STATUS: TileSourceStatus = { kind: 'lan-live', zoom: 16, label: 'LAN source', cachedAt: null };

describe('<PositionMapWidget> (offline, shape only)', () => {
  beforeEach(() => {
    resetMapMock();
    vi.mocked(useTileSource).mockReturnValue(null);
  });

  it('renders the offline base map with a marker at the grid center', () => {
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);

    expect(screen.getByTestId('leaflet-map')).toBeInTheDocument();
    expect(screen.getByTestId('image-overlay')).toBeInTheDocument();

    const marker = screen.getByTestId('leaflet-marker');
    const ll = gridToLatLon('CN87us')!;
    const pos = JSON.parse(marker.dataset.position ?? '[]') as [number, number];
    expect(pos[0]).toBeCloseTo(ll.lat, 4);
    expect(pos[1]).toBeCloseTo(ll.lon, 4);
  });

  it('renders the grid-square rectangle overlay', () => {
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);
    expect(screen.getByTestId('leaflet-rectangle')).toBeInTheDocument();
  });

  it('clicking the map fires onGridChange with the 6-char grid', () => {
    const onGridChange = vi.fn();
    render(<PositionMapWidget grid="CN87us" onGridChange={onGridChange} />);

    const ll = gridToLatLon('JN58td')!;
    fireMapEvent('click', { lat: ll.lat, lng: ll.lon });

    expect(onGridChange).toHaveBeenCalledOnce();
    const result = onGridChange.mock.calls[0][0] as string;
    expect(result).toHaveLength(6); // preserve the existing length-6 contract
    expect(result).toBe(latLonToGrid(ll.lat, ll.lon));
  });

  it('invalid grid still renders the map without marker or rectangle', () => {
    render(<PositionMapWidget grid="ZZ99" onGridChange={vi.fn()} />);
    expect(screen.getByTestId('leaflet-map')).toBeInTheDocument();
    expect(screen.queryByTestId('leaflet-marker')).toBeNull();
    expect(screen.queryByTestId('leaflet-rectangle')).toBeNull();
  });

  it('never renders an online tile layer or external tile URL (offline-only)', () => {
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);

    // The only base layer is the bundled offline Mercator overlay (EPSG:3857).
    const overlay = screen.getByTestId('image-overlay');
    expect(overlay.dataset.url).toBe('/world-mercator-2048.png');
    expect(overlay.dataset.url).not.toContain('http');

    // No legacy OSM tile layer; nothing references an external tile host.
    expect(screen.queryByTestId('osm-tile-layer')).toBeNull();
    expect(document.body.innerHTML).not.toContain('openstreetmap');
    expect(document.body.innerHTML).not.toContain('tile.');
  });

  // Task 7: validate tile source + onZoomChange wiring
  it('passes tileSource to BaseMap when useTileSource returns a lan-live source', () => {
    vi.mocked(useTileSource).mockReturnValue({ source: LAN_SOURCE, status: LAN_STATUS });
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);
    // When tileSource is tile-backed, BaseMap renders TileLayerBridge → leaflet-tilelayer
    expect(screen.getByTestId('leaflet-tilelayer')).toBeInTheDocument();
  });

  it('renders no TileLayer when useTileSource returns null (offline fallback)', () => {
    vi.mocked(useTileSource).mockReturnValue(null);
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);
    expect(screen.queryByTestId('leaflet-tilelayer')).toBeNull();
  });

  it('forwards onZoomChange to BaseMap when provided', () => {
    const onZoomChange = vi.fn();
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} onZoomChange={onZoomChange} />);
    // Function props are stripped from data-* by testMapMock; fire the zoomend event
    // through the mock to prove the handler is wired.
    fireZoomEvent();
    expect(onZoomChange).toHaveBeenCalledWith(1); // mockZoom default
  });
});
