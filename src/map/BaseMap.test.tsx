/**
 * BaseMap shape test — SHAPE ONLY (C1).
 *
 * jsdom cannot render Leaflet, so this asserts only that BaseMap wires the
 * EPSG4326 CRS, world bounds, and bundled ImageOverlay, and bridges clicks to
 * onMapClick. Real projection arithmetic is proven in projection.test.ts; real
 * render / pan correctness is verified via grim on WebKitGTK — do NOT assert
 * projection through this mock.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { fireMapEvent, resetMapMock } from './testMapMock';

vi.mock('react-leaflet', async () => (await import('./testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('./testMapMock')).createLeafletMock());
vi.mock('./assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));

import { BaseMap } from './BaseMap';
import { WORLD_BOUNDS } from './projection';

describe('<BaseMap> (shape only)', () => {
  beforeEach(() => {
    resetMapMock();
  });

  it('renders the MapContainer with EPSG4326 CRS and world maxBounds', () => {
    render(<BaseMap />);
    const container = screen.getByTestId('leaflet-map');
    expect(container).toBeInTheDocument();
    expect(container.dataset.crs).toContain('4326');
    expect(container.dataset.maxbounds).toBe(JSON.stringify(WORLD_BOUNDS));
  });

  it('caps zoom at 4 and disables map-copy wrapping (offline single-world)', () => {
    render(<BaseMap />);
    const container = screen.getByTestId('leaflet-map');
    expect(container.dataset.maxzoom).toBe('4');
    expect(container.dataset.worldcopyjump).toBe('false');
  });

  it('renders the bundled ImageOverlay across the full world rectangle', () => {
    render(<BaseMap />);
    const overlay = screen.getByTestId('image-overlay');
    expect(overlay.dataset.bounds).toBe(JSON.stringify(WORLD_BOUNDS));
    expect(overlay.dataset.url).toBe('/world-equirect-2048.png');
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

  it('renders children inside the map', () => {
    render(
      <BaseMap>
        <div data-testid="child-layer" />
      </BaseMap>,
    );
    expect(screen.getByTestId('child-layer')).toBeInTheDocument();
  });
});
