/**
 * PositionMapWidget tests — SHAPE/WIRING ONLY (C1).
 *
 * jsdom has no WebGL; the map is the global maplibre test double. These assert
 * logical structure: a GeoJSON pin + grid-square at the grid lat/lon, a click
 * firing onGridChange with a 6-char grid, an invalid grid → no features, the
 * style backed by the bundled PMTiles source (no external tiles), and the
 * onZoomChange bridge. Real render is grim-verified.
 */
import { describe, it, expect, vi } from 'vitest';
import { render, act } from '@testing-library/react';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import { getLastMap, type MapLibreMock } from '../map/testMapLibreMock';
import { PositionMapWidget } from './PositionMapWidget';

interface Feat {
  properties: { kind: string };
  geometry: { type: string; coordinates: number[] | number[][][] };
}

function loadLast(): MapLibreMock {
  const map = getLastMap()!;
  act(() => map.__emit('load'));
  return map;
}
function positionFeatures(map: MapLibreMock): Feat[] {
  return (map.getSource('position') as { data: { features: Feat[] } }).data.features;
}

describe('<PositionMapWidget> (offline, shape only)', () => {
  it('renders a pin + grid-square at the grid center', () => {
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);
    const map = loadLast();
    const feats = positionFeatures(map);
    const pin = feats.find((f) => f.properties.kind === 'pin')!;
    const ll = gridToLatLon('CN87us')!;
    expect((pin.geometry.coordinates as number[])[0]).toBeCloseTo(ll.lon, 4);
    expect((pin.geometry.coordinates as number[])[1]).toBeCloseTo(ll.lat, 4);
    expect(feats.some((f) => f.properties.kind === 'square')).toBe(true);
  });

  it('clicking the map fires onGridChange with the 6-char grid', () => {
    const onGridChange = vi.fn();
    render(<PositionMapWidget grid="CN87us" onGridChange={onGridChange} />);
    const map = loadLast();
    const ll = gridToLatLon('JN58td')!;
    act(() => map.__emit('click', { lngLat: { lng: ll.lon, lat: ll.lat } }));
    expect(onGridChange).toHaveBeenCalledOnce();
    const result = onGridChange.mock.calls[0][0] as string;
    expect(result).toHaveLength(6);
    expect(result).toBe(latLonToGrid(ll.lat, ll.lon));
  });

  it('invalid grid renders the map with no pin or square', () => {
    render(<PositionMapWidget grid="ZZ99" onGridChange={vi.fn()} />);
    const map = loadLast();
    expect(positionFeatures(map)).toHaveLength(0);
  });

  it('is backed by the bundled PMTiles source — no external tile URL (offline-only)', () => {
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} />);
    const map = getLastMap()!;
    const style = map.__state.options.style as { sources: Record<string, { url?: string }> };
    const url = style.sources.protomaps.url ?? '';
    expect(url).toContain('pmtiles://');
    expect(url).not.toContain('http');
    expect(url).not.toContain('openstreetmap');
  });

  it('forwards onZoomChange (seeded on load with the real zoom)', () => {
    const onZoomChange = vi.fn();
    render(<PositionMapWidget grid="CN87us" onGridChange={vi.fn()} onZoomChange={onZoomChange} />);
    loadLast();
    expect(onZoomChange).toHaveBeenCalledWith(6); // initialZoom for a placed grid
  });
});
