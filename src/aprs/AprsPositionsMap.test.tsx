import React from 'react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, act, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import L from 'leaflet';
import { gridToLatLon } from '../forms/position/maidenhead';
import { ambiguityRadiusMeters } from './AprsPositionsMap';
import type { HeardPosition } from './aprsTypes';
import { listDraftIds, loadDraft } from '../compose/useDraft';
import { STOPPED } from '../modem/types';

// LeafletMap fetches packs via invoke('basemap_list_packs') (wants {packs}); the
// SITREP button calls invoke('compose_window_open'). useRecentGateways calls
// contacts_recent_gateways; useModemStatus calls modem_get_status. Task 24 wired
// the peer circle layer (gated on map_peers) into AprsPositionsMap, so it now
// also calls peers_read + p2p_capabilities — return safe defaults (no peers,
// every capability bit false) so those queries resolve a value instead of
// throwing "Query data cannot be undefined" on every test.
const invokeMock = vi.hoisted(() =>
  vi.fn(async (cmd: string) => {
    if (cmd === 'basemap_list_packs') return { packs: [] };
    if (cmd === 'contacts_recent_gateways') return [];
    if (cmd === 'modem_get_status') return STOPPED;
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

// useModemStatus subscribes via listen('@tauri-apps/api/event'); make it inert.
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

// Mock the base-layer builder → inert layer (R5 P1): a real protomaps-leaflet
// GridLayer would try to fetch/decode PMTiles to canvas in jsdom (AbortSignal /
// no-canvas noise). The base render is grim-verified (Task 7), not unit-tested.
vi.mock('../map/basemapLeaflet', () => ({
  buildBaseLayers: vi.fn(() => [L.layerGroup()]),
  OSM_ATTRIBUTION: '© OpenStreetMap contributors',
  flavorBackground: () => '#34373d',
}));

// Spy whenSheetsReady so we can assert the re-bake wiring exists (R3 P0) and drive
// it; keep every other aprsSprites export real (identity assertions need them).
const sheetsReadyCbs = vi.hoisted(() => [] as Array<() => void>);
vi.mock('../map/aprsSprites', async (orig) => {
  const actual = await orig<typeof import('../map/aprsSprites')>();
  return {
    ...actual,
    whenSheetsReady: vi.fn((cb: () => void) => {
      sheetsReadyCbs.push(cb);
      return () => {};
    }),
  };
});

import { AprsPositionsMap } from './AprsPositionsMap';

// Leaflet sizes from clientWidth/Height; jsdom reports 0. Shim the prototype.
const origW = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
const origH = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight');

// Capture the real L.Map the component constructs (no prod test-registry needed).
const realLMap = L.map.bind(L);
let captured: L.Map | null = null;

beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, value: 800 });
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, value: 600 });
  captured = null;
  sheetsReadyCbs.length = 0;
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
  // intact when tests call result.rerender(<AprsPositionsMap .../>).
  const result = render(ui, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    ),
  });
  await act(async () => {
    await Promise.resolve();
  });
  await waitFor(() => expect(captured).not.toBeNull());
  return result;
}

const pins = (c: HTMLElement) => Array.from(c.querySelectorAll<HTMLElement>('img.aprs-pin'));
const pinByCall = (c: HTMLElement, call: string) =>
  pins(c).find((el) => el.dataset.call === call);
const circles = () => {
  const out: L.Circle[] = [];
  captured!.eachLayer((l) => {
    if (l instanceof L.Circle) out.push(l);
  });
  return out;
};

const positions: HeardPosition[] = [
  { call: 'KK6XYZ', lat: 49.05, lon: -72.03, symbolTable: '/', symbolCode: '-', comment: 'Hello', at: Date.now(), ambiguity: 0 },
  { call: 'W7ABC', lat: 40.0, lon: -100.0, symbolTable: '/', symbolCode: '>', comment: 'Mobile', at: Date.now(), ambiguity: 0 },
];

describe('AprsPositionsMap (Leaflet)', () => {
  it('renders a testid container', async () => {
    const { getByTestId } = await renderMap(<AprsPositionsMap positions={[]} />);
    expect(getByTestId('aprs-positions-map')).toBeInTheDocument();
  });

  // tuxlink-ivfr: the sprite divIcon html is set via innerHTML, so a parsed inline
  // `style="width:..."` is blocked by the production Tauri CSP `style-src` nonce
  // (it makes 'unsafe-inline' inert) → the img would fall back to its natural 64px.
  // Size via the width/height ATTRIBUTES, which CSP `style-src` does not govern.
  it('sizes the sprite via width/height attributes, not a CSP-blocked inline style', async () => {
    const { container } = await renderMap(<AprsPositionsMap positions={positions} />);
    const img = pinByCall(container, 'KK6XYZ')!;
    expect(img.getAttribute('width')).toBe('32');
    expect(img.getAttribute('height')).toBe('32');
    expect(img.getAttribute('style')).toBeNull();
  });

  it('renders one pin per heard position with callsign + sprite identity + decoded coords', async () => {
    const { container } = await renderMap(<AprsPositionsMap positions={positions} />);
    expect(pins(container)).toHaveLength(2);
    const xyz = pinByCall(container, 'KK6XYZ')!;
    expect(xyz.nextElementSibling?.textContent).toBe('KK6XYZ'); // the label span
    const mobile = pinByCall(container, 'W7ABC')!;
    expect(mobile.dataset.sprite).toBe('aprs:p:>'); // stable identity (pure helper id)
    // exact (non-ambiguous) fix plots at the decoded coord — no centre shift.
    expect(parseFloat(mobile.dataset.lat!)).toBe(40);
    expect(parseFloat(mobile.dataset.lon!)).toBe(-100);
  });

  it('plots nothing when no positions are heard', async () => {
    const { container } = await renderMap(<AprsPositionsMap positions={[]} />);
    expect(pins(container)).toHaveLength(0);
  });

  it('renders an uncertainty circle ONLY for ambiguous fixes, sized radius = ambiguityRadiusMeters×√2, centred on the cell centre', async () => {
    const amb: HeardPosition[] = [
      { call: 'EXACT', lat: 49, lon: -72, symbolTable: '/', symbolCode: '-', comment: '', at: Date.now(), ambiguity: 0 },
      { call: 'FUZZY', lat: 40, lon: -100, symbolTable: '/', symbolCode: '>', comment: '', at: Date.now(), ambiguity: 2 },
    ];
    const { container } = await renderMap(<AprsPositionsMap positions={amb} />);
    const cs = circles();
    expect(cs).toHaveLength(1); // only the masked fix gets a region (no false halo)
    expect(cs[0].getRadius()).toBeCloseTo(ambiguityRadiusMeters(2) * Math.SQRT2, 3); // √2 is load-bearing
    // ambiguous pin plots at the cell CENTRE (shifted toward increasing magnitude), not the low corner.
    const fuzzy = pinByCall(container, 'FUZZY')!;
    expect(parseFloat(fuzzy.dataset.lat!)).toBeGreaterThan(40);
    expect(parseFloat(fuzzy.dataset.lon!)).toBeLessThan(-100);
  });

  it('maps ambiguity level to a growing uncertainty radius; 0 = none', () => {
    expect(ambiguityRadiusMeters(0)).toBe(0);
    expect(ambiguityRadiusMeters(1)).toBeGreaterThan(0);
    expect(ambiguityRadiusMeters(2)).toBeGreaterThan(ambiguityRadiusMeters(1));
    expect(ambiguityRadiusMeters(4)).toBeGreaterThan(ambiguityRadiusMeters(3));
  });

  it('popup discloses symbol, last-heard age, and approximate-position note on pin click; updates and closes', async () => {
    const fuzzy: HeardPosition[] = [
      { call: 'FUZZY', lat: 40, lon: -100, symbolTable: '/', symbolCode: '_', comment: 'mobile', at: Date.now() - 20 * 60 * 1000, ambiguity: 2 },
    ];
    const { container, getByTestId, queryByTestId, rerender } = await renderMap(<AprsPositionsMap positions={fuzzy} />);
    act(() => {
      pinByCall(container, 'FUZZY')!.closest('.leaflet-marker-icon')?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    expect(getByTestId('aprs-position-symbol').textContent).toContain('Weather station');
    expect(getByTestId('aprs-position-age').textContent).toContain('min ago');
    expect(getByTestId('aprs-position-ambiguity').textContent).toContain('approximate');
    // pruned station → popup closes on its own (live byCall derivation).
    await act(async () => {
      rerender(<AprsPositionsMap positions={[]} />);
      await Promise.resolve();
    });
    expect(queryByTestId('aprs-position-popup')).toBeNull();
  });

  it('greys a stale pin (only the pin) and keeps a fresh pin in colour', async () => {
    const now = Date.now();
    const fixes: HeardPosition[] = [
      { call: 'OLD', lat: 40, lon: -100, symbolTable: '/', symbolCode: '>', comment: '', at: now - 90 * 60 * 1000, ambiguity: 0 },
      { call: 'NEW', lat: 41, lon: -101, symbolTable: '/', symbolCode: '>', comment: '', at: now, ambiguity: 0 },
    ];
    const { container } = await renderMap(<AprsPositionsMap positions={fixes} />);
    expect(pinByCall(container, 'OLD')!.dataset.sprite).toBe('aprs:p:>:grey');
    expect(pinByCall(container, 'OLD')!.classList.contains('aprs-pin--stale')).toBe(true);
    expect(pinByCall(container, 'NEW')!.dataset.sprite).toBe('aprs:p:>');
  });

  it('subscribes to whenSheetsReady so pins re-bake when sheets decode (R3 P0)', async () => {
    const { aprsSpritesWhenReady } = { aprsSpritesWhenReady: (await import('../map/aprsSprites')).whenSheetsReady };
    await renderMap(<AprsPositionsMap positions={positions} />);
    expect(aprsSpritesWhenReady).toHaveBeenCalled();
    // firing the callback re-bakes without error
    expect(() => act(() => sheetsReadyCbs.forEach((cb) => cb()))).not.toThrow();
  });

  it('keeps marker identity stable across a re-render with identical positions (no churn)', async () => {
    const { container, rerender } = await renderMap(<AprsPositionsMap positions={positions} />);
    const before = pinByCall(container, 'W7ABC');
    await act(async () => {
      rerender(<AprsPositionsMap positions={[...positions]} />);
      await Promise.resolve();
    });
    expect(pinByCall(container, 'W7ABC')).toBe(before); // same DOM node → same marker instance
  });
});

describe('AprsPositionsMap WX overlay + filter (ni5b)', () => {
  const wxPositions: HeardPosition[] = [
    { call: 'W7WX', lat: 47, lon: -122, symbolTable: '/', symbolCode: '_', comment: '', at: Date.now(), ambiguity: 0 },
  ];
  const wxEnv = [
    {
      call: 'W7WX',
      project: '',
      seq: null,
      bits: [],
      rain: null,
      lastHeard: Date.now(),
      channels: [
        { key: 'wx:temperature', label: 'Temp', unit: '°F', kind: 'temperature', value: 72, scaled: true, history: [] },
      ],
    },
  ];
  const badges = (c: HTMLElement) => Array.from(c.querySelectorAll<HTMLElement>('.aprs-wx-chip'));

  it('renders a temperature badge for a heard weather station', async () => {
    const { container } = await renderMap(
      <AprsPositionsMap positions={wxPositions} envStations={wxEnv as never} operatorGrid="CN87" />,
    );
    const b = badges(container);
    expect(b).toHaveLength(1);
    expect(b[0].textContent).toContain('72°F');
  });

  it('opens the WX card on badge click (+ focuses the station) and closes it via the × button', async () => {
    const onFocus = vi.fn();
    const { container, getByTestId, queryByTestId } = await renderMap(
      <AprsPositionsMap positions={wxPositions} envStations={wxEnv as never} onFocusStation={onFocus} operatorGrid="CN87" />,
    );
    const chip = badges(container)[0].closest('.leaflet-marker-icon')!;
    // Click the badge → focus the dock station AND open the on-map card.
    act(() => chip.dispatchEvent(new MouseEvent('click', { bubbles: true })));
    expect(onFocus).toHaveBeenCalledWith('W7WX');
    expect(getByTestId('aprs-wx-card').textContent).toContain('W7WX');
    // The card is dismissible via its × (operator-reported: hover-dismiss got stuck).
    fireEvent.click(getByTestId('aprs-wx-card-close'));
    expect(queryByTestId('aprs-wx-card')).toBeNull();
  });

  it('renders no WX badge for a station with no heard weather', async () => {
    const { container } = await renderMap(
      <AprsPositionsMap positions={positions} envStations={[] as never} operatorGrid="CN87" />,
    );
    expect(container.querySelectorAll('.aprs-wx-chip')).toHaveLength(0);
  });

  it('layers panel removes a deselected category as a WHOLE bundle (no orphan disc)', async () => {
    // Two stations: a car (vehicles) and a weather station (weather).
    const positions: HeardPosition[] = [
      { call: 'N7CAR-9', lat: 40, lon: -111, symbolTable: '/', symbolCode: '>', comment: '', at: Date.now(), ambiguity: 0, via: [] },
      { call: 'WX7AB', lat: 41, lon: -112, symbolTable: '/', symbolCode: '_', comment: '', at: Date.now(), ambiguity: 0, via: [] },
    ];
    const { getByTestId, container } = await renderMap(
      <AprsPositionsMap positions={positions} operatorGrid="DN40" />,
    );
    // Default all-on: both pins drawn.
    expect(pinByCall(container, 'N7CAR-9')).toBeDefined();
    expect(pinByCall(container, 'WX7AB')).toBeDefined();

    // Open the panel (default collapsed), then uncheck Vehicles.
    fireEvent.click(getByTestId('aprs-layers-toggle'));
    await act(async () => {
      fireEvent.click(getByTestId('aprs-layers-check-vehicles'));
    });
    // The car's WHOLE bundle leaves the map; the weather station stays. This is the
    // real invariant — the count is filter-independent (still reads 1), so the
    // marker's absence, not the count, proves the filter actually hides the pin.
    expect(pinByCall(container, 'N7CAR-9')).toBeUndefined();
    expect(pinByCall(container, 'WX7AB')).toBeDefined();
    expect(getByTestId('aprs-layers-count-vehicles')).toHaveTextContent('1');

    // Re-check Vehicles restores the car pin.
    await act(async () => {
      fireEvent.click(getByTestId('aprs-layers-check-vehicles'));
    });
    expect(getByTestId('aprs-layers-check-vehicles')).toBeChecked();
    expect(pinByCall(container, 'N7CAR-9')).toBeDefined();
  });

  it('Weather SITREP composes a prefilled draft and opens compose (hepq)', async () => {
    await renderMap(<AprsPositionsMap positions={wxPositions} envStations={wxEnv as never} operatorGrid="CN87" />);
    const btn = screen.getByTestId('aprs-wx-sitrep');
    expect(btn).not.toBeDisabled();
    fireEvent.click(btn);
    const ids = listDraftIds();
    expect(ids.length).toBe(1);
    const draft = loadDraft(ids[0])!;
    expect(draft.subject).toContain('WX SITREP');
    expect(draft.body).toContain('72°F');
    expect(invokeMock).toHaveBeenCalledWith('compose_window_open', { draftId: draft.draftId });
  });

  it('Weather SITREP is disabled with no WX stations heard', async () => {
    await renderMap(<AprsPositionsMap positions={[]} />);
    expect(screen.getByTestId('aprs-wx-sitrep')).toBeDisabled();
  });
});

describe('AprsPositionsMap viewport + operator pin (tuxlink-dwzu)', () => {
  const KEY = 'tuxlink:map-viewport:aprs';

  it('opens at the saved viewport when one is stored', async () => {
    window.localStorage.setItem(KEY, JSON.stringify({ center: { lat: 45, lon: -73 }, zoom: 7 }));
    await renderMap(<AprsPositionsMap positions={[]} />);
    expect(captured!.getCenter().lat).toBeCloseTo(45, 3);
    expect(captured!.getCenter().lng).toBeCloseTo(-73, 3);
    expect(captured!.getZoom()).toBe(7);
  });

  it('centers on the operator at the local zoom on first run when an operator grid is known', async () => {
    await renderMap(<AprsPositionsMap positions={[]} operatorGrid="DM43bp" />);
    const me = gridToLatLon('DM43bp')!;
    expect(captured!.getCenter().lat).toBeCloseTo(me.lat, 2);
    expect(captured!.getCenter().lng).toBeCloseTo(me.lon, 2);
    expect(captured!.getZoom()).toBe(10);
  });

  it('falls back to the world view on first run only when no operator grid is known', async () => {
    await renderMap(<AprsPositionsMap positions={[]} operatorGrid="" />);
    expect(captured!.getCenter().lat).toBeCloseTo(0, 3);
    expect(captured!.getCenter().lng).toBeCloseTo(0, 3);
    expect(captured!.getZoom()).toBe(2);
  });

  it('persists the viewport after a pan (debounced)', async () => {
    vi.useFakeTimers();
    try {
      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      render(<AprsPositionsMap positions={[]} />, {
        wrapper: ({ children }) => (
          <QueryClientProvider client={qc}>{children}</QueryClientProvider>
        ),
      });
      await act(async () => {
        await Promise.resolve();
      });
      expect(captured).not.toBeNull();
      act(() => captured!.setView([47.6, -122.3], 9, { animate: false }));
      act(() => {
        vi.advanceTimersByTime(600);
      });
      expect(JSON.parse(window.localStorage.getItem(KEY)!)).toEqual({
        center: { lat: 47.6, lon: -122.3 },
        zoom: 9,
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('recenters on the operator at OPERATOR_ZOOM when the recenter control is clicked', async () => {
    await renderMap(<AprsPositionsMap positions={[]} operatorGrid="DM43bp" />);
    const me = gridToLatLon('DM43bp')!;
    const flySpy = vi.spyOn(captured!, 'flyTo');
    fireEvent.click(screen.getByTestId('map-recenter'));
    expect(flySpy).toHaveBeenCalledWith([me.lat, me.lon], 10);
  });

  it('renders an operator "you" pin only when an operator grid is set', async () => {
    const { container, rerender } = await renderMap(<AprsPositionsMap positions={[]} operatorGrid="" />);
    expect(container.querySelector('.aprs-you-pin')).toBeNull();
    await act(async () => {
      rerender(<AprsPositionsMap positions={[]} operatorGrid="DM43bp" />);
      await Promise.resolve();
    });
    expect(container.querySelector('.aprs-you-pin')).not.toBeNull();
  });

  it('hides the recenter control when no operator grid is known', async () => {
    await renderMap(<AprsPositionsMap positions={[]} operatorGrid="" />);
    expect(screen.queryByTestId('map-recenter')).toBeNull();
  });
});

// Codex impl-review P1: a re-beacon can change ambiguity / weather readings on an
// EXISTING station; the bundle must fully reconcile, not only move sub-layers.
describe('AprsPositionsMap re-beacon reconciliation (impl P1)', () => {
  const wxEnvAt = (call: string, temp: number) => [
    {
      call,
      project: '',
      seq: null,
      bits: [],
      rain: null,
      lastHeard: Date.now(),
      channels: [
        { key: 'wx:temperature', label: 'Temp', unit: '°F', kind: 'temperature', value: temp, scaled: true, history: [] },
      ],
    },
  ];
  const chips = (c: HTMLElement) => Array.from(c.querySelectorAll<HTMLElement>('.aprs-wx-chip'));

  it('adds an uncertainty disc when a station goes exact → ambiguous, and removes it on ambiguous → exact', async () => {
    const exact: HeardPosition[] = [
      { call: 'MOVER', lat: 40, lon: -100, symbolTable: '/', symbolCode: '>', comment: '', at: Date.now(), ambiguity: 0 },
    ];
    const { rerender } = await renderMap(<AprsPositionsMap positions={exact} />);
    expect(circles()).toHaveLength(0);
    // Re-beacon: same call, now ambiguous.
    await act(async () => {
      rerender(<AprsPositionsMap positions={[{ ...exact[0], ambiguity: 2 }]} />);
      await Promise.resolve();
    });
    expect(circles()).toHaveLength(1);
    expect(circles()[0].getRadius()).toBeCloseTo(ambiguityRadiusMeters(2) * Math.SQRT2, 3);
    // Re-beacon back to exact → the stale disc is removed.
    await act(async () => {
      rerender(<AprsPositionsMap positions={[{ ...exact[0], ambiguity: 0 }]} />);
      await Promise.resolve();
    });
    expect(circles()).toHaveLength(0);
  });

  it('refreshes the WX badge reading when a weather station re-beacons a new value', async () => {
    const pos: HeardPosition[] = [
      { call: 'W7WX', lat: 47, lon: -122, symbolTable: '/', symbolCode: '_', comment: '', at: Date.now(), ambiguity: 0 },
    ];
    const { container, rerender } = await renderMap(
      <AprsPositionsMap positions={pos} envStations={wxEnvAt('W7WX', 72) as never} operatorGrid="CN87" />,
    );
    expect(chips(container)[0].textContent).toContain('72°F');
    await act(async () => {
      rerender(<AprsPositionsMap positions={pos} envStations={wxEnvAt('W7WX', 81) as never} operatorGrid="CN87" />);
      await Promise.resolve();
    });
    expect(chips(container)[0].textContent).toContain('81°F'); // refreshed, not stuck at 72
  });

  it('adds a badge when a station becomes a weather station (non-WX → WX)', async () => {
    const pos: HeardPosition[] = [
      { call: 'W7WX', lat: 47, lon: -122, symbolTable: '/', symbolCode: '_', comment: '', at: Date.now(), ambiguity: 0 },
    ];
    const { container, rerender } = await renderMap(<AprsPositionsMap positions={pos} envStations={[] as never} />);
    expect(chips(container)).toHaveLength(0);
    await act(async () => {
      rerender(<AprsPositionsMap positions={pos} envStations={wxEnvAt('W7WX', 64) as never} />);
      await Promise.resolve();
    });
    expect(chips(container)).toHaveLength(1);
    expect(chips(container)[0].textContent).toContain('64°F');
  });
});
