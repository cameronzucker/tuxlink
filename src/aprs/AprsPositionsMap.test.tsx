import { describe, it, expect } from 'vitest';
import { render, act } from '@testing-library/react';
import { getLastMap, type MapLibreMock } from '../map/testMapLibreMock';
import { AprsPositionsMap } from './AprsPositionsMap';
import type { HeardPosition } from './aprsTypes';

// MapLibre re-expression (mirrors StationFinderMap.test.tsx): pins are GeoJSON
// circle-layer features driven through the global maplibre-gl test double, not
// Leaflet markers. These tests prove the source/feature wiring (one feature per
// heard station, callsign label property). Render fidelity is grim-only.

const positions: HeardPosition[] = [
  { call: 'KK6XYZ', lat: 49.05, lon: -72.03, symbolTable: '/', symbolCode: '-', comment: 'Hello', at: 1 },
  { call: 'W7ABC', lat: 40.0, lon: -100.0, symbolTable: '/', symbolCode: '>', comment: 'Mobile', at: 2 },
];

interface PinFeature {
  properties: { call: string; comment: string };
  geometry: { coordinates: [number, number] };
}

function loadLast(): MapLibreMock {
  const map = getLastMap()!;
  act(() => map.__emit('load'));
  return map;
}

function sourceData(map: MapLibreMock, id: string): { features: PinFeature[] } {
  return (map.getSource(id) as { data: { features: PinFeature[] } }).data;
}

describe('AprsPositionsMap', () => {
  it('renders a testid container', () => {
    const { getByTestId } = render(<AprsPositionsMap positions={[]} />);
    expect(getByTestId('aprs-positions-map')).toBeInTheDocument();
  });

  it('builds one feature per heard position with callsign + comment + coords', () => {
    render(<AprsPositionsMap positions={positions} />);
    const map = loadLast();
    const feats = sourceData(map, 'aprs-positions').features;
    expect(feats).toHaveLength(2);
    const xyz = feats.find((f) => f.properties.call === 'KK6XYZ')!;
    expect(xyz.properties.comment).toBe('Hello');
    // GeoJSON coordinate order is [lon, lat].
    expect(xyz.geometry.coordinates).toEqual([-72.03, 49.05]);
  });

  it('plots nothing when no positions are heard', () => {
    render(<AprsPositionsMap positions={[]} />);
    const map = loadLast();
    expect(sourceData(map, 'aprs-positions').features).toHaveLength(0);
  });
});
