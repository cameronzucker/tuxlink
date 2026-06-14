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

  it('clamps the constructor center when initialCenter is outside the world (rwo6)', () => {
    render(<MapLibreMap initialCenter={{ lat: 95, lon: 250 }} />);
    const map = getLastMap()!;
    expect(map.__state.options.center).toEqual([180, 85.0511]);
  });

  it('clamps the flyTo center when an async initialCenter is outside the world (rwo6)', () => {
    const { rerender } = render(<MapLibreMap />);
    const map = getLastMap()!;
    loadMap(map);
    rerender(<MapLibreMap initialCenter={{ lat: -95, lon: -250 }} />);
    expect(map.flyTo).toHaveBeenCalledWith({ center: [-180, -85.0511] });
  });

  it('re-centers back into the world on moveend when panned into the void (rwo6)', () => {
    render(<MapLibreMap />);
    const map = getLastMap()!;
    loadMap(map);
    // Pan the center east past the antimeridian (renderWorldCopies=false → gray).
    map.__setCenter(215, 10);
    act(() => map.__emit('moveend'));
    expect(map.setCenter).toHaveBeenCalledWith([180, 10]);
  });

  it('does NOT re-center on moveend when the center is already in the world (rwo6)', () => {
    render(<MapLibreMap />);
    const map = getLastMap()!;
    loadMap(map);
    map.__setCenter(-122, 47);
    act(() => map.__emit('moveend'));
    expect(map.setCenter).not.toHaveBeenCalled();
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

  it('follows the app color scheme when no flavor prop is given', () => {
    document.documentElement.dataset.theme = 'daylight'; // a light preset
    try {
      render(<MapLibreMap />);
      const style = getLastMap()!.__state.options.style as { sprite: string };
      // maplibre v5 requires absolute sprite URLs (tuxlink-56ki).
      expect(style.sprite).toBe(`${location.origin}/basemap/sprites/light`);
    } finally {
      delete document.documentElement.dataset.theme;
    }
  });

  it('setStyle swaps to the dark style when flavor changes after mount', () => {
    const { rerender } = render(<MapLibreMap flavor="light" />);
    const map = getLastMap()!;
    loadMap(map);
    expect(map.setStyle).not.toHaveBeenCalled(); // constructor used the initial flavor
    rerender(<MapLibreMap flavor="dark" />);
    expect(map.setStyle).toHaveBeenCalledTimes(1);
    const style = vi.mocked(map.setStyle).mock.calls[0][0] as { sprite: string };
    expect(style.sprite).toBe(`${location.origin}/basemap/sprites/dark`);
  });

  it('adds an attribution control and removes the map on unmount', () => {
    const { unmount } = render(<MapLibreMap />);
    const map = getLastMap()!;
    expect(map.addControl).toHaveBeenCalled();
    unmount();
    expect(map.remove).toHaveBeenCalled();
  });
});
