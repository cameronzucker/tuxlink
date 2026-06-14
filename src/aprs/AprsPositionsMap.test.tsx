import { describe, it, expect } from 'vitest';
import { render, act } from '@testing-library/react';
import { getLastMap, type MapLibreMock } from '../map/testMapLibreMock';
import { AprsPositionsMap, ambiguityRadiusMeters } from './AprsPositionsMap';
import type { HeardPosition } from './aprsTypes';

// MapLibre re-expression (mirrors StationFinderMap.test.tsx): pins are GeoJSON
// circle-layer features driven through the global maplibre-gl test double, not
// Leaflet markers. These tests prove the source/feature wiring (one feature per
// heard station, callsign label property). Render fidelity is grim-only.

const positions: HeardPosition[] = [
  { call: 'KK6XYZ', lat: 49.05, lon: -72.03, symbolTable: '/', symbolCode: '-', comment: 'Hello', at: 1, ambiguity: 0 },
  { call: 'W7ABC', lat: 40.0, lon: -100.0, symbolTable: '/', symbolCode: '>', comment: 'Mobile', at: 2, ambiguity: 0 },
];

interface PinFeature {
  properties: { call: string; comment: string; ambiguity: number };
  geometry: { coordinates: [number, number] };
}

interface CircleFeature {
  properties: { call: string; ambiguity: number };
  geometry: { type: string };
}

function loadLast(): MapLibreMock {
  const map = getLastMap()!;
  act(() => map.__emit('load'));
  return map;
}

function sourceData(map: MapLibreMock, id: string): { features: PinFeature[] } {
  return (map.getSource(id) as { data: { features: PinFeature[] } }).data;
}

function circleData(map: MapLibreMock, id: string): { features: CircleFeature[] } {
  return (map.getSource(id) as { data: { features: CircleFeature[] } }).data;
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

  it('carries the ambiguity level onto each pin feature', () => {
    const amb: HeardPosition[] = [
      { call: 'FUZZY', lat: 40, lon: -100, symbolTable: '/', symbolCode: '>', comment: '', at: 1, ambiguity: 2 },
    ];
    render(<AprsPositionsMap positions={amb} />);
    const map = loadLast();
    expect(sourceData(map, 'aprs-positions').features[0].properties.ambiguity).toBe(2);
  });

  it('renders an uncertainty region only for ambiguous fixes (RF-honesty)', () => {
    const amb: HeardPosition[] = [
      { call: 'EXACT', lat: 49, lon: -72, symbolTable: '/', symbolCode: '-', comment: '', at: 1, ambiguity: 0 },
      { call: 'FUZZY', lat: 40, lon: -100, symbolTable: '/', symbolCode: '>', comment: '', at: 2, ambiguity: 2 },
    ];
    render(<AprsPositionsMap positions={amb} />);
    const map = loadLast();
    const circles = circleData(map, 'aprs-position-uncertainty').features;
    // Only the masked fix gets a region; the exact fix does not (no false halo).
    expect(circles).toHaveLength(1);
    expect(circles[0].properties.call).toBe('FUZZY');
    // It is a region (polygon), not a point — honest about the uncertainty.
    expect(circles[0].geometry.type).toBe('Polygon');
  });

  it('maps ambiguity level to a growing uncertainty radius; 0 = none', () => {
    expect(ambiguityRadiusMeters(0)).toBe(0);
    expect(ambiguityRadiusMeters(1)).toBeGreaterThan(0);
    expect(ambiguityRadiusMeters(2)).toBeGreaterThan(ambiguityRadiusMeters(1));
    expect(ambiguityRadiusMeters(4)).toBeGreaterThan(ambiguityRadiusMeters(3));
  });

  it('popup discloses last-heard age and approximate-position note on click', () => {
    const fuzzy: HeardPosition[] = [
      {
        call: 'FUZZY',
        lat: 40,
        lon: -100,
        symbolTable: '/',
        symbolCode: '>',
        comment: 'mobile',
        // Heard ~20 min ago so the age is non-trivial and the pin is stale.
        at: Date.now() - 20 * 60 * 1000,
        ambiguity: 2,
      },
    ];
    const { getByTestId } = render(<AprsPositionsMap positions={fuzzy} />);
    const map = loadLast();
    act(() => map.__emit('click:aprs-position-pins', { features: [{ properties: { call: 'FUZZY' } }] }));
    expect(getByTestId('aprs-position-age').textContent).toContain('min ago');
    expect(getByTestId('aprs-position-ambiguity').textContent).toContain('approximate');
  });
});
