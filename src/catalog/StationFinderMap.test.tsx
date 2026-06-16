import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, act, screen, fireEvent } from '@testing-library/react';
import { getLastMap, type MapLibreMock } from '../map/testMapLibreMock';
import { gridToLatLon } from '../forms/position/maidenhead';
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
  id: string;
  properties: { key: string; tier: string };
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

  it('encodes the reachability tier and a stable feature id on each feature', () => {
    const key0 = stationKey(stations[0]);
    const tiers = new Map<string, ReachTier>([[key0, 'good']]);
    render(<StationFinderMap stations={stations} operatorGrid="" tiers={tiers} selectedKey={key0} onSelect={() => {}} />);
    const map = loadLast();
    const feats = sourceData(map, 'stations').features;
    const good = feats.find((f) => f.properties.key === key0)!;
    expect(good.properties.tier).toBe('good');
    expect(good.id).toBe(key0); // top-level id targets setFeatureState
    // `selected` is no longer baked into feature properties — it is feature-state.
    expect('selected' in good.properties).toBe(false);
    const other = feats.find((f) => f.properties.key !== key0)!;
    expect(other.properties.tier).toBe('untiered'); // no tier → untiered fallback
  });

  it('drives selection via setFeatureState — selecting does NOT rebuild the FC', () => {
    const key0 = stationKey(stations[0]);
    // Hold `stations`/`tiers` identity stable so only `selectedKey` changes — this
    // isolates the selection path (feature-state), which must not push setData.
    const tiers = new Map<string, ReachTier>();
    const { rerender } = render(
      <StationFinderMap stations={stations} operatorGrid="" tiers={tiers} selectedKey={null} onSelect={() => {}} />,
    );
    const map = loadLast();
    const src = map.getSource('stations') as { setData: ReturnType<typeof vi.fn> };
    src.setData.mockClear();

    act(() => {
      rerender(
        <StationFinderMap stations={stations} operatorGrid="" tiers={tiers} selectedKey={key0} onSelect={() => {}} />,
      );
    });

    // Selection flips one feature's state; it does NOT push a new FeatureCollection.
    expect(map.setFeatureState).toHaveBeenCalledWith(
      { source: 'stations', id: key0 },
      { selected: true },
    );
    expect(src.setData).not.toHaveBeenCalled();
  });

  it('promotes the station key to the feature id (feature-state needs it on GeoJSON)', () => {
    // MapLibre silently ignores feature-state on a GeoJSON source with top-level
    // STRING ids unless promoteId is set — the root cause of "selection never
    // showed" (operator 2026-06-16). The stations source must promote `key`.
    render(<StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />);
    const map = loadLast();
    expect(map.addSource).toHaveBeenCalledWith('stations', expect.objectContaining({ promoteId: 'key' }));
  });

  it('re-applies the selected feature-state after a data push (setData clears it)', () => {
    // Regression (operator 2026-06-16): GeoJSONSource.setData drops all
    // feature-state, and reachability tiers stream in (each a setData), so the
    // selected pin lost its emphasis the instant a tier updated. The map must
    // re-apply feature-state when the source finishes (re)loading.
    const key0 = stationKey(stations[0]);
    render(
      <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={key0} onSelect={() => {}} />,
    );
    const map = loadLast();
    (map.setFeatureState as ReturnType<typeof vi.fn>).mockClear();

    // Simulate a tier-driven setData reload completing.
    act(() => map.__emit('sourcedata', { sourceId: 'stations', isSourceLoaded: true }));

    expect(map.setFeatureState).toHaveBeenCalledWith({ source: 'stations', id: key0 }, { selected: true });
    // An unrelated source's load must NOT trigger a re-apply.
    (map.setFeatureState as ReturnType<typeof vi.fn>).mockClear();
    act(() => map.__emit('sourcedata', { sourceId: 'operator', isSourceLoaded: true }));
    expect(map.setFeatureState).not.toHaveBeenCalled();
  });

  it('changing the selection clears the previous feature-state then sets the new one', () => {
    const key0 = stationKey(stations[0]);
    const key1 = stationKey(stations[1]);
    const { rerender } = render(
      <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={key0} onSelect={() => {}} />,
    );
    const map = loadLast();
    (map.setFeatureState as ReturnType<typeof vi.fn>).mockClear();
    (map.removeFeatureState as ReturnType<typeof vi.fn>).mockClear();

    act(() => {
      rerender(
        <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={key1} onSelect={() => {}} />,
      );
    });

    expect(map.removeFeatureState).toHaveBeenCalledWith({ source: 'stations', id: key0 }, 'selected');
    expect(map.setFeatureState).toHaveBeenCalledWith({ source: 'stations', id: key1 }, { selected: true });
  });

  it('changing stations still pushes a new FeatureCollection (setData)', () => {
    const { rerender } = render(
      <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    const map = loadLast();
    const src = map.getSource('stations') as { setData: ReturnType<typeof vi.fn> };
    src.setData.mockClear();

    act(() => {
      rerender(
        <StationFinderMap stations={[stations[0]]} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
      );
    });

    expect(src.setData).toHaveBeenCalled();
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

describe('StationFinderMap viewport persistence (tuxlink-dwzu)', () => {
  const KEY = 'tuxlink:map-viewport:station-finder';
  beforeEach(() => window.localStorage.clear());

  it('opens at the saved viewport and suppresses the operator flyTo when one is stored', () => {
    window.localStorage.setItem(KEY, JSON.stringify({ center: { lat: 40, lon: -100 }, zoom: 8 }));
    render(
      <StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    const map = getLastMap()!;
    expect(map.__state.options.center).toEqual([-100, 40]); // [lon, lat] — the saved view, not the operator
    expect(map.getZoom()).toBe(8);
    act(() => map.__emit('load'));
    expect(map.flyTo).not.toHaveBeenCalled(); // saved view wins: no laborious pan to the operator
  });

  it('falls back to the operator position at OPERATOR_ZOOM on first run (no saved viewport)', () => {
    render(
      <StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    const map = getLastMap()!;
    const me = gridToLatLon('DM43bp')!;
    expect(map.__state.options.center).toEqual([me.lon, me.lat]);
    expect(map.getZoom()).toBe(6); // OPERATOR_ZOOM
  });

  it('persists the viewport after the operator pans (moveend, debounced)', () => {
    vi.useFakeTimers();
    try {
      render(
        <StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
      );
      const map = getLastMap()!;
      act(() => map.__emit('load'));
      map.__setCenter(-71.06, 42.36);
      map.__setZoom(10);
      act(() => map.__emit('moveend'));
      act(() => vi.advanceTimersByTime(400));
      expect(JSON.parse(window.localStorage.getItem(KEY)!)).toEqual({
        center: { lat: 42.36, lon: -71.06 },
        zoom: 10,
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('recenters on the operator at OPERATOR_ZOOM when the recenter control is clicked', () => {
    render(
      <StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    const map = getLastMap()!;
    act(() => map.__emit('load')); // MapContext provides the live map after load
    const me = gridToLatLon('DM43bp')!;
    fireEvent.click(screen.getByTestId('map-recenter'));
    expect(map.flyTo).toHaveBeenCalledWith({ center: [me.lon, me.lat], zoom: 6 });
  });

  it('hides the recenter control when no operator grid is known', () => {
    render(
      <StationFinderMap stations={[]} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    act(() => getLastMap()!.__emit('load'));
    expect(screen.queryByTestId('map-recenter')).toBeNull();
  });
});
