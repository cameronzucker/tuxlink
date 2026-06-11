/**
 * BaseMap shape test — SHAPE ONLY (C1).
 *
 * jsdom cannot render Leaflet, so this asserts only that BaseMap wires the
 * EPSG:3857 CRS, Mercator bounds, and bundled ImageOverlay, and bridges clicks
 * and zoom events to onMapClick/onZoomChange. Real projection arithmetic is
 * proven in projection.test.ts; real render / pan correctness is verified via
 * grim on WebKitGTK — do NOT assert projection through this mock.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { fireMapEvent, fireZoomEvent, setMockZoom, resetMapMock } from './testMapMock';

vi.mock('react-leaflet', async () => (await import('./testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('./testMapMock')).createLeafletMock());
vi.mock('./assets/world-mercator-2048.png', () => ({ default: '/world-mercator-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));

import { BaseMap } from './BaseMap';
import { MERCATOR_BOUNDS } from './projection';
import type { TileSource, TileSourceStatus } from './tileSource';

const SOURCE: TileSource = {
  url: 'http://192.168.1.10:8080/{z}/{x}/{y}.png',
  scheme: 'Xyz',
  minZoom: 0,
  maxZoom: 16,
  cacheBudgetMb: 256,
  attribution: null,
  label: 'LAN source',
};
function status(kind: TileSourceStatus['kind'], zoom = 14): TileSourceStatus {
  return { kind, zoom, label: 'LAN source', cachedAt: null };
}

describe('<BaseMap> (shape only)', () => {
  beforeEach(() => {
    resetMapMock();
  });

  it('renders the MapContainer with EPSG:3857 CRS and Mercator maxBounds', () => {
    render(<BaseMap />);
    const container = screen.getByTestId('leaflet-map');
    expect(container).toBeInTheDocument();
    expect(container.dataset.crs).toContain('3857');
    expect(container.dataset.maxbounds).toBe(JSON.stringify(MERCATOR_BOUNDS));
  });

  it('caps zoom at 3 (raster-native under EPSG:3857) and disables map-copy wrapping (offline single-world)', () => {
    render(<BaseMap />);
    const container = screen.getByTestId('leaflet-map');
    expect(container.dataset.maxzoom).toBe('3');
    expect(container.dataset.worldcopyjump).toBe('false');
    // native box-zoom disabled (conflicts with GridMapPicker drag-to-select)
    expect(container.dataset.boxzoom).toBe('false');
  });

  it('renders the bundled Mercator ImageOverlay across the Mercator world rectangle', () => {
    render(<BaseMap />);
    const overlay = screen.getByTestId('image-overlay');
    expect(overlay.dataset.bounds).toBe(JSON.stringify(MERCATOR_BOUNDS));
    expect(overlay.dataset.url).toBe('/world-mercator-2048.png');
  });

  it('bridges a map click to onMapClick with a clamped LatLon', () => {
    const onMapClick = vi.fn();
    render(<BaseMap onMapClick={onMapClick} />);
    fireMapEvent('click', { lat: 0, lng: 0 });
    expect(onMapClick).toHaveBeenCalledWith({ lat: 0, lon: 0 });
  });

  it('clamps an out-of-range click before reporting it', () => {
    const onMapClick = vi.fn();
    render(<BaseMap onMapClick={onMapClick} />);
    fireMapEvent('click', { lat: 95, lng: 200 });
    expect(onMapClick).toHaveBeenCalledWith({ lat: 90, lon: 180 });
  });

  // ── C11 widening (Phase 7.3): optional validated LAN tile layer ──────────

  it('renders no TileLayer and keeps maxZoom 3 when no tileSource is given', () => {
    render(<BaseMap />);
    expect(screen.queryByTestId('leaflet-tilelayer')).toBeNull();
    expect(screen.getByTestId('leaflet-map').dataset.maxzoom).toBe('3');
  });

  it('renders the TileLayer above the ImageOverlay and raises maxZoom when status is lan-live', () => {
    render(<BaseMap tileSource={{ source: SOURCE, status: status('lan-live') }} />);
    const tl = screen.getByTestId('leaflet-tilelayer');
    expect(tl).toBeInTheDocument();
    // raised to the validated source max (16), still <= cap
    expect(screen.getByTestId('leaflet-map').dataset.maxzoom).toBe('16');
    // ImageOverlay always present as the base; TileLayer DOM-ordered after it
    const overlay = screen.getByTestId('image-overlay');
    expect(
      overlay.compareDocumentPosition(tl) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it('renders the TileLayer and raises maxZoom when status is lan-cached', () => {
    render(<BaseMap tileSource={{ source: SOURCE, status: status('lan-cached') }} />);
    expect(screen.getByTestId('leaflet-tilelayer')).toBeInTheDocument();
    expect(screen.getByTestId('leaflet-map').dataset.maxzoom).toBe('16');
  });

  // §8.5 `partial` is a LIVE source with some 404s — it MUST keep the TileLayer
  // rendered and the zoom cap raised (Phase 9.2 reconcile). Dropping the layer
  // would regress the whole view to the coarse raster the moment one edge tile
  // is missing.
  it('renders the TileLayer and raises maxZoom when status is partial', () => {
    render(<BaseMap tileSource={{ source: SOURCE, status: status('partial') }} />);
    const tl = screen.getByTestId('leaflet-tilelayer');
    expect(tl).toBeInTheDocument();
    expect(screen.getByTestId('leaflet-map').dataset.maxzoom).toBe('16');
    // Layer still DOM-ordered above the always-present bundled raster, so a 404
    // tile reveals the raster beneath rather than a grey void.
    const overlay = screen.getByTestId('image-overlay');
    expect(
      overlay.compareDocumentPosition(tl) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it('caps the raised maxZoom at 16 even when the source max exceeds it', () => {
    render(
      <BaseMap tileSource={{ source: { ...SOURCE, maxZoom: 19 }, status: status('lan-live') }} />,
    );
    expect(screen.getByTestId('leaflet-map').dataset.maxzoom).toBe('16');
  });

  it('renders no TileLayer and keeps maxZoom 3 when status is incompatible', () => {
    render(<BaseMap tileSource={{ source: SOURCE, status: status('incompatible') }} />);
    expect(screen.queryByTestId('leaflet-tilelayer')).toBeNull();
    expect(screen.getByTestId('leaflet-map').dataset.maxzoom).toBe('3');
  });

  it('renders no TileLayer and keeps maxZoom 3 when status is unreachable', () => {
    render(<BaseMap tileSource={{ source: SOURCE, status: status('unreachable') }} />);
    expect(screen.queryByTestId('leaflet-tilelayer')).toBeNull();
    expect(screen.getByTestId('leaflet-map').dataset.maxzoom).toBe('3');
  });

  it('renders children inside the map', () => {
    render(
      <BaseMap>
        <div data-testid="child-layer" />
      </BaseMap>,
    );
    expect(screen.getByTestId('child-layer')).toBeInTheDocument();
  });

  // ── onZoomChange bridge (Task 5: 6-char grid gate) ──────────────────────
  it('calls onZoomChange with the current zoom when a zoomend fires', () => {
    const onZoomChange = vi.fn();
    render(<BaseMap onZoomChange={onZoomChange} />);
    // fakeMap.getZoom() returns 1 (default mock zoom)
    fireZoomEvent();
    expect(onZoomChange).toHaveBeenCalledWith(1);
  });

  it('reports the updated zoom value after setMockZoom', () => {
    const onZoomChange = vi.fn();
    render(<BaseMap onZoomChange={onZoomChange} />);
    // Use the mock's zoom seam to simulate zooming to 5
    setMockZoom(5);
    fireZoomEvent();
    expect(onZoomChange).toHaveBeenCalledWith(5);
  });
});
