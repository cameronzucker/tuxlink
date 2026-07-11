import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, act, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import L from 'leaflet';
import { gridToLatLon } from '../forms/position/maidenhead';
import { stationKey } from './useReachabilityMap';
import type { ReachTier } from './reachability';
import type { Station } from './stationModel';

// Leaflet re-expression: each station is an L.circleMarker rendered on an explicit
// SVG renderer. These tests run the REAL Leaflet map in jsdom (no engine mock) and
// inspect the live layer objects: one marker per placeable station, the data-driven
// tier style, selection emphasis applied in place (no marker churn), the operator
// pin, and click→onSelect. Render fidelity is grim-verified.

// LeafletMap fetches packs via invoke('basemap_list_packs') (wants {packs}). Task
// 24 wired StationFinderMap to its own usePeers()/useP2pCapabilities() (the peer
// circle layer, gated on map_peers) — those call peers_read/p2p_capabilities.
const invokeMock = vi.hoisted(() =>
  vi.fn(async (cmd: string) => {
    if (cmd === 'basemap_list_packs') return { packs: [] };
    if (cmd === 'peers_read') return { schema_version: 1, peers: [] };
    if (cmd === 'p2p_capabilities') {
      return {
        peer_store: false, finder_peers: false, map_peers: false, settings_editor: false,
        agent_find_peers: false, agent_telnet_dial: false, vara_engine_split: false,
        favorites_peer_link: false,
      };
    }
    return undefined;
  }),
);
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

// Mock the base-layer builder → inert layer: a real protomaps-leaflet GridLayer
// would try to fetch/decode PMTiles to canvas in jsdom. Base render is grim-verified.
vi.mock('../map/basemapLeaflet', () => ({
  buildBaseLayers: vi.fn(() => [L.layerGroup()]),
  OSM_ATTRIBUTION: '© OpenStreetMap contributors',
  flavorBackground: () => '#34373d',
}));

import { StationFinderMap } from './StationFinderMap';

// Leaflet sizes from clientWidth/Height; jsdom reports 0. Shim the prototype.
const origW = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
const origH = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight');

const realLMap = L.map.bind(L);
let captured: L.Map | null = null;

beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, value: 800 });
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, value: 600 });
  captured = null;
  vi.spyOn(L, 'map').mockImplementation(((el: HTMLElement | string, opts?: L.MapOptions) => {
    const m = realLMap(el as HTMLElement, opts);
    captured = m;
    return m;
  }) as typeof L.map);
  window.localStorage.clear();
  invokeMock.mockClear();
});
afterEach(() => {
  vi.restoreAllMocks();
  if (origW) Object.defineProperty(HTMLElement.prototype, 'clientWidth', origW);
  if (origH) Object.defineProperty(HTMLElement.prototype, 'clientHeight', origH);
});

/** Render and flush LeafletMap's whenReady (sync) + async pack fetch. */
async function renderMap(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  // Use the `wrapper` option so RTL's `rerender` keeps the QueryClientProvider
  // intact when tests call result.rerender(<StationFinderMap .../>).
  const result = render(ui, {
    wrapper: ({ children }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>,
  });
  await act(async () => {
    await Promise.resolve();
  });
  await waitFor(() => expect(captured).not.toBeNull());
  return result;
}

/** All circleMarkers on the live map. */
function circleMarkers(): L.CircleMarker[] {
  const out: L.CircleMarker[] = [];
  captured!.eachLayer((l) => {
    if (l instanceof L.CircleMarker) out.push(l);
  });
  return out;
}

/** The station/operator pin nearest a grid centroid. Excludes the glow disc
 * (the only marker with `stroke:false`), which `syncGlow` parks ON TOP of the
 * selected pin — so without this filter a selected pin's coords would match the
 * glow first and report the glow's style instead of the pin's. */
function markerAtGrid(grid: string): L.CircleMarker | undefined {
  const ll = gridToLatLon(grid)!;
  return circleMarkers().find((m) => {
    if (m.options.stroke === false) return false; // skip the glow disc
    const p = m.getLatLng();
    return Math.abs(p.lat - ll.lat) < 1e-6 && Math.abs(p.lng - ll.lon) < 1e-6;
  });
}

/** The station pins only — exclude the operator pin (non-interactive) + glow. */
function stationPins(): L.CircleMarker[] {
  return circleMarkers().filter((m) => m.options.interactive !== false);
}

const stations: Station[] = [
  { baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, gatewayAntenna: null, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
  { baseCallsign: 'K0ABC', grid: 'EN34', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, gatewayAntenna: null, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
  { baseCallsign: 'NOGRID', grid: '', sysopName: null, location: null, modes: ['vara-hf'], fetchedAtMs: 1, gatewayAntenna: null, channels: [{ mode: 'vara-hf', frequencyKhz: 7103, band: '40m' }] },
];

describe('StationFinderMap (Leaflet)', () => {
  it('builds one pin per placeable station, dropping gridless ones', async () => {
    await renderMap(
      <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    // DM34oa + EN34 placed; NOGRID dropped. No operator grid → no operator pin.
    expect(stationPins()).toHaveLength(2);
    expect(markerAtGrid('DM34oa')).toBeDefined();
    expect(markerAtGrid('EN34')).toBeDefined();
  });

  it('encodes the reachability tier into the pin colour + radius', async () => {
    const key0 = stationKey(stations[0]);
    const tiers = new Map<string, ReachTier>([[key0, 'good']]);
    await renderMap(
      <StationFinderMap stations={stations} operatorGrid="" tiers={tiers} selectedKey={null} onSelect={() => {}} />,
    );
    const good = markerAtGrid('DM34oa')!;
    expect(good.options.fillColor).toBe('#41ba6c'); // good → green
    expect(good.options.radius).toBe(10); // good base radius
    const untiered = markerAtGrid('EN34')!;
    expect(untiered.options.fillColor).toBe('#9fb6cc'); // no tier → untiered fallback colour
    expect(untiered.options.radius).toBe(7); // untiered base radius
  });

  it('drives selection by re-styling the marker in place — selecting does NOT recreate it', async () => {
    const key0 = stationKey(stations[0]);
    const tiers = new Map<string, ReachTier>([[key0, 'good']]);
    const { rerender } = await renderMap(
      <StationFinderMap stations={stations} operatorGrid="" tiers={tiers} selectedKey={null} onSelect={() => {}} />,
    );
    const before = markerAtGrid('DM34oa')!;
    expect(before.options.weight).toBe(0.6); // thin rim when unselected

    await act(async () => {
      rerender(
        <StationFinderMap stations={stations} operatorGrid="" tiers={tiers} selectedKey={key0} onSelect={() => {}} />,
      );
      await Promise.resolve();
    });

    const after = markerAtGrid('DM34oa')!;
    expect(after).toBe(before); // same instance — re-styled, not rebuilt
    expect(after.options.weight).toBe(2); // bright rim when selected
    expect(after.options.radius).toBe(12); // selected bump (good 10→12)
  });

  it('changing the selection restores the previous pin and emphasises the new one', async () => {
    const key0 = stationKey(stations[0]);
    const key1 = stationKey(stations[1]);
    const { rerender } = await renderMap(
      <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={key0} onSelect={() => {}} />,
    );
    expect(markerAtGrid('DM34oa')!.options.weight).toBe(2);

    await act(async () => {
      rerender(
        <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={key1} onSelect={() => {}} />,
      );
      await Promise.resolve();
    });

    expect(markerAtGrid('DM34oa')!.options.weight).toBe(0.6); // previous restored
    expect(markerAtGrid('EN34')!.options.weight).toBe(2); // new emphasised
    void key1;
  });

  it('adds + removes pins as the station set changes', async () => {
    const { rerender } = await renderMap(
      <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    expect(stationPins()).toHaveLength(2);
    await act(async () => {
      rerender(
        <StationFinderMap stations={[stations[0]]} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
      );
      await Promise.resolve();
    });
    expect(stationPins()).toHaveLength(1);
    expect(markerAtGrid('DM34oa')).toBeDefined();
    expect(markerAtGrid('EN34')).toBeUndefined();
  });

  it('places the operator pin only when a grid is set', async () => {
    const { rerender } = await renderMap(
      <StationFinderMap stations={[]} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    // No stations, no operator grid → only the (non-interactive) glow disc exists.
    expect(stationPins()).toHaveLength(0);
    expect(markerAtGrid('DM43bp')).toBeUndefined();
    await act(async () => {
      rerender(
        <StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
      );
      await Promise.resolve();
    });
    expect(markerAtGrid('DM43bp')).toBeDefined(); // operator pin placed
  });

  it('fires onSelect when a station pin is clicked', async () => {
    const onSelect = vi.fn();
    await renderMap(
      <StationFinderMap stations={stations} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={onSelect} />,
    );
    act(() => {
      markerAtGrid('DM34oa')!.fire('click');
    });
    expect(onSelect).toHaveBeenCalledWith(stations[0]);
  });
});

describe('StationFinderMap viewport persistence (tuxlink-dwzu)', () => {
  const KEY = 'tuxlink:map-viewport:station-finder';

  it('opens at the saved viewport and suppresses the operator flyTo when one is stored', async () => {
    window.localStorage.setItem(KEY, JSON.stringify({ center: { lat: 40, lon: -100 }, zoom: 8 }));
    await renderMap(
      <StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    expect(captured!.getCenter().lat).toBeCloseTo(40, 3);
    expect(captured!.getCenter().lng).toBeCloseTo(-100, 3);
    expect(captured!.getZoom()).toBe(8); // saved view wins, not the operator
  });

  it('falls back to the operator position at OPERATOR_ZOOM on first run (no saved viewport)', async () => {
    await renderMap(
      <StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    const me = gridToLatLon('DM43bp')!;
    expect(captured!.getCenter().lat).toBeCloseTo(me.lat, 2);
    expect(captured!.getCenter().lng).toBeCloseTo(me.lon, 2);
    expect(captured!.getZoom()).toBe(6); // OPERATOR_ZOOM
  });

  it('persists the viewport after the operator pans (debounced)', async () => {
    vi.useFakeTimers();
    try {
      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      render(
        <QueryClientProvider client={qc}>
          <StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />
        </QueryClientProvider>,
      );
      await act(async () => {
        await Promise.resolve();
      });
      expect(captured).not.toBeNull();
      act(() => captured!.setView([42.36, -71.06], 10, { animate: false }));
      act(() => {
        vi.advanceTimersByTime(600);
      });
      expect(JSON.parse(window.localStorage.getItem(KEY)!)).toEqual({
        center: { lat: 42.36, lon: -71.06 },
        zoom: 10,
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('recenters on the operator at OPERATOR_ZOOM when the recenter control is clicked', async () => {
    await renderMap(
      <StationFinderMap stations={[]} operatorGrid="DM43bp" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    const me = gridToLatLon('DM43bp')!;
    const flySpy = vi.spyOn(captured!, 'flyTo');
    fireEvent.click(screen.getByTestId('map-recenter'));
    expect(flySpy).toHaveBeenCalledWith([me.lat, me.lon], 6);
  });

  it('hides the recenter control when no operator grid is known', async () => {
    await renderMap(
      <StationFinderMap stations={[]} operatorGrid="" tiers={new Map()} selectedKey={null} onSelect={() => {}} />,
    );
    expect(screen.queryByTestId('map-recenter')).toBeNull();
  });
});
