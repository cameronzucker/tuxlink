/**
 * LocationMap wiring tests (tuxlink-yy1m, MapLibre port). The global maplibre
 * mock (src/test-setup.ts) backs `new maplibregl.Map`; we drive map events via
 * `__emit`. Wiring only — real render/drag is grim-verified (map subsystem C1).
 */
import { describe, it, expect, vi } from 'vitest';
import { render, act, screen } from '@testing-library/react';
import { getLastMap, type MapLibreMock } from '../map/testMapLibreMock';
import { latLonToGrid } from '../forms/position/maidenhead';
import { LocationMap } from './LocationMap';

function loadLast(): MapLibreMock {
  const map = getLastMap()!;
  act(() => map.__emit('load'));
  return map;
}

describe('LocationMap (MapLibre)', () => {
  it('renders the map container and constructs a map', () => {
    render(<LocationMap grid="EM75km" fixLatLon={null} selectedSource="manual" onGridChange={vi.fn()} />);
    expect(screen.getByTestId('location-map')).toBeInTheDocument();
    expect(getLastMap()).toBeTruthy();
  });

  it('clicking the map sets the grid for the clicked point', () => {
    const onGridChange = vi.fn();
    render(<LocationMap grid="" fixLatLon={null} selectedSource="manual" onGridChange={onGridChange} />);
    const map = loadLast();
    act(() => map.__emit('click', { lngLat: { lng: -86.8, lat: 36.1 } }));
    expect(onGridChange).toHaveBeenCalledWith(latLonToGrid(36.1, -86.8));
  });

  it('dragging the marker sets the grid by hand (flow 3): disables pan on grab, commits on release', () => {
    const onGridChange = vi.fn();
    render(<LocationMap grid="EM75km" fixLatLon={null} selectedSource="manual" onGridChange={onGridChange} />);
    const map = loadLast();
    // Grab the marker layer, drag, release somewhere new.
    act(() => map.__emit('mousedown:loc-pin-dot', { lngLat: { lng: -86.8, lat: 36.1 }, preventDefault: () => {} }));
    expect(map.dragPan.disable).toHaveBeenCalled();
    act(() => map.__emit('mousemove', { lngLat: { lng: -90.0, lat: 35.0 } }));
    act(() => map.__emit('mouseup', { lngLat: { lng: -90.0, lat: 35.0 } }));
    expect(map.dragPan.enable).toHaveBeenCalled();
    expect(onGridChange).toHaveBeenCalledWith(latLonToGrid(35.0, -90.0));
  });

  it('a mousemove with no active drag does not change the grid', () => {
    const onGridChange = vi.fn();
    render(<LocationMap grid="EM75km" fixLatLon={null} selectedSource="manual" onGridChange={onGridChange} />);
    const map = loadLast();
    act(() => map.__emit('mousemove', { lngLat: { lng: -90, lat: 35 } }));
    act(() => map.__emit('mouseup', { lngLat: { lng: -90, lat: 35 } }));
    expect(onGridChange).not.toHaveBeenCalled();
  });

  it('registers the marker overlay source + layers once the style is loaded', () => {
    render(<LocationMap grid="EM75km" fixLatLon={{ lat: 36.1, lon: -86.8 }} selectedSource="gpsd" onGridChange={vi.fn()} />);
    const map = loadLast();
    act(() => map.__emit('styledata'));
    expect(map.getSource('location-pin')).toBeTruthy();
    expect(map.getLayer('loc-pin-dot')).toBeTruthy();
  });
});
