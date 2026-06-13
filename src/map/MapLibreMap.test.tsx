/**
 * Wiring tests for the MapLibreMap component (tuxlink-ndi4, plan phase 2).
 *
 * Verifies the C11 re-expression: construction options, the pmtiles protocol
 * registration, click→onMapClick (clamped), onZoomChange seeding on load AND
 * moveend (A17), context provision to children, and teardown. Render/projection
 * correctness is grim-only (jsdom has no WebGL).
 */
import { describe, it, expect, vi } from 'vitest';
import { render, act } from '@testing-library/react';
import maplibregl from 'maplibre-gl';
import { getLastMap } from './testMapLibreMock';
import { MapLibreMap } from './MapLibreMap';
import { useMapContext } from './MapContext';

function loadMap(map: NonNullable<ReturnType<typeof getLastMap>>) {
  act(() => {
    map.__emit('load');
  });
}

describe('MapLibreMap', () => {
  it('constructs a map and registers the pmtiles protocol', () => {
    render(<MapLibreMap />);
    expect(getLastMap()).toBeTruthy();
    expect(vi.mocked(maplibregl.addProtocol)).toHaveBeenCalledWith(
      'pmtiles',
      expect.any(Function),
    );
  });

  it('passes initialCenter (as lng,lat) and initialZoom to the constructor', () => {
    render(<MapLibreMap initialCenter={{ lat: 47.6, lon: -122.3 }} initialZoom={9} />);
    const map = getLastMap()!;
    expect(map.__state.options.center).toEqual([-122.3, 47.6]);
    expect(map.getZoom()).toBe(9);
  });

  it('bridges map click to onMapClick with a clamped lat/lon', () => {
    const onMapClick = vi.fn();
    render(<MapLibreMap onMapClick={onMapClick} />);
    const map = getLastMap()!;
    act(() => map.__emit('click', { lngLat: { lng: 10, lat: 20 } }));
    expect(onMapClick).toHaveBeenCalledWith({ lat: 20, lon: 10 });
  });

  it('clamps an out-of-range click to the world rectangle (±90 / ±180)', () => {
    const onMapClick = vi.fn();
    render(<MapLibreMap onMapClick={onMapClick} />);
    const map = getLastMap()!;
    act(() => map.__emit('click', { lngLat: { lng: 200, lat: 95 } }));
    expect(onMapClick).toHaveBeenCalledWith({ lat: 90, lon: 180 });
  });

  it('fires onZoomChange on load (seed) and on moveend with the real zoom (A17)', () => {
    const onZoomChange = vi.fn();
    render(<MapLibreMap initialZoom={5} onZoomChange={onZoomChange} />);
    const map = getLastMap()!;
    loadMap(map);
    expect(onZoomChange).toHaveBeenCalledWith(5); // seed on load, not a stale literal
    map.__setZoom(8);
    act(() => map.__emit('moveend'));
    expect(onZoomChange).toHaveBeenLastCalledWith(8);
  });

  it('provides the map to children via context only after load', () => {
    function Probe() {
      const map = useMapContext();
      return <div data-testid="probe">{map ? 'has-map' : 'no-map'}</div>;
    }
    const { getByTestId } = render(
      <MapLibreMap>
        <Probe />
      </MapLibreMap>,
    );
    expect(getByTestId('probe').textContent).toBe('no-map');
    loadMap(getLastMap()!);
    expect(getByTestId('probe').textContent).toBe('has-map');
  });

  it('flyTo recenters when initialCenter arrives async (was absent at mount)', () => {
    // StationFinderMap case: operator grid resolves after the map mounts.
    const { rerender } = render(<MapLibreMap />);
    const map = getLastMap()!;
    loadMap(map);
    expect(map.flyTo).not.toHaveBeenCalled();
    rerender(<MapLibreMap initialCenter={{ lat: 47.6, lon: -122.3 }} />);
    expect(map.flyTo).toHaveBeenCalledWith({ center: [-122.3, 47.6] });
  });

  it('does NOT flyTo for a center that was already present at construction', () => {
    const { rerender } = render(<MapLibreMap initialCenter={{ lat: 10, lon: 20 }} />);
    const map = getLastMap()!;
    loadMap(map);
    // Re-render with the SAME center must not animate.
    rerender(<MapLibreMap initialCenter={{ lat: 10, lon: 20 }} />);
    expect(map.flyTo).not.toHaveBeenCalled();
  });

  it('adds an attribution control and removes the map on unmount', () => {
    const { unmount } = render(<MapLibreMap />);
    const map = getLastMap()!;
    expect(map.addControl).toHaveBeenCalled();
    unmount();
    expect(map.remove).toHaveBeenCalled();
  });
});
