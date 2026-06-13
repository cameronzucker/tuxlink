import { describe, it, expect, vi } from 'vitest';
import { render, act } from '@testing-library/react';
import { getLastMap, type MapLibreMock } from '../map/testMapLibreMock';
import { stationKey } from './useReachabilityMap';
import { StationFinderMap } from './StationFinderMap';
import type { ReachTier } from './reachability';
import type { Station } from './stationModel';

// MapLibre re-expression: pins are GeoJSON circle-layer features, not Leaflet
// markers. These tests prove the source/feature wiring (tier + selected props,
// gridless drop, operator pin, layer-scoped click→onSelect). Render fidelity +
// recenter are covered by grim + MapLibreMap's own tests respectively.

const stations: Station[] = [
  { baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, gatewayAntenna: null, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
  { baseCallsign: 'K0ABC', grid: 'EN34', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, gatewayAntenna: null, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
  { baseCallsign: 'NOGRID', grid: '', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, gatewayAntenna: null, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
];

interface PinFeature {
  properties: { key: string; tier: string; selected: boolean };
}

function loadLast(): MapLibreMock {
  const map = getLastMap()!;
  act(() => map.__emit('load'));
  return map;
}

function sourceData(map: MapLibreMock, id: string): { features: PinFeature[] } {
  return (map.getSource(id) as { data: { features: PinFeature[] } }).data;
}

describe('StationFinderMap', () => {
  it('builds one feature per placeable station, dropping gridless ones', () => {
    render(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />);
    const map = loadLast();
    expect(sourceData(map, 'stations').features).toHaveLength(2); // NOGRID dropped
  });

  it('encodes the reachability tier and selected flag on each feature', () => {
    const key0 = stationKey(stations[0]);
    const tiers = new Map<string, ReachTier>([[key0, 'good']]);
    render(<StationFinderMap stations={stations} operatorGrid="" tiers={tiers} selectedKey={key0} onSelect={() => {}} />);
    const map = loadLast();
    const feats = sourceData(map, 'stations').features;
    const selected = feats.find((f) => f.properties.key === key0)!;
    expect(selected.properties.tier).toBe('good');
    expect(selected.properties.selected).toBe(true);
    const other = feats.find((f) => f.properties.key !== key0)!;
    expect(other.properties.tier).toBe('untiered'); // no tier → untiered fallback
    expect(other.properties.selected).toBe(false);
  });

  it('places the operator pin only when a grid is set', () => {
    const { rerender } = render(<StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />);
    let map = loadLast();
    expect(sourceData(map, 'operator').features).toHaveLength(0);

    rerender(<StationFinderMap stations={stations} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />);
    map = getLastMap()!;
    expect(sourceData(map, 'operator').features).toHaveLength(1);
  });

  it('fires onSelect when a station pin is clicked (layer-scoped click)', () => {
    const onSelect = vi.fn();
    const key0 = stationKey(stations[0]);
    render(<StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={onSelect} />);
    const map = loadLast();
    act(() => map.__emit('click:station-pins', { features: [{ properties: { key: key0 } }] }));
    expect(onSelect).toHaveBeenCalledWith(stations[0]);
  });
});
