import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, act, waitFor } from '@testing-library/react';
import L from 'leaflet';
import type { AggregatedPeer } from '../peers/peerModel';

// Real Leaflet map in jsdom (no engine mock), mirroring StationFinderMap.test.tsx
// and AprsPositionsMap.test.tsx: PeerLayer must be L.divIcon + eventHandlers, NOT
// react-leaflet <Marker> children (a silent false-green no-op in this codebase —
// see WinlinkGatewayLayer/AprsPositionsMap precedent), so the test inspects the
// LIVE marker objects on the real map rather than a mocked layer.

// LeafletMap fetches packs via invoke('basemap_list_packs') (wants {packs}).
const invokeMock = vi.hoisted(() =>
  vi.fn(async (cmd: string) => (cmd === 'basemap_list_packs' ? { packs: [] } : undefined)),
);
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

// Mock the base-layer builder → inert layer: a real protomaps-leaflet GridLayer
// would try to fetch/decode PMTiles to canvas in jsdom. Base render is grim-verified.
vi.mock('../map/basemapLeaflet', () => ({
  buildBaseLayers: vi.fn(() => [L.layerGroup()]),
  OSM_ATTRIBUTION: '© OpenStreetMap contributors',
  flavorBackground: () => '#34373d',
}));

import { LeafletMap } from './LeafletMap';
import { PeerLayer, type PeerVisual } from './PeerLayer';

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
  const result = render(ui);
  await act(async () => {
    await Promise.resolve();
  });
  await waitFor(() => expect(captured).not.toBeNull());
  return result;
}

/** All peer markers on the live map (identified by the divIcon className). */
function peerMarkers(): L.Marker[] {
  const out: L.Marker[] = [];
  captured!.eachLayer((l) => {
    if (l instanceof L.Marker && (l.options.icon as L.DivIcon | undefined)?.options.className === 'peer-pin-icon') {
      out.push(l);
    }
  });
  return out;
}

function peerFixture(over: Partial<AggregatedPeer> = {}): AggregatedPeer {
  return {
    id: 'p1',
    callsign: 'W6ABC',
    origin: 'outgoing',
    tier: 'unconfirmed',
    grid: 'CN87',
    mapPlaceable: true,
    lastSeen: null,
    channels: [],
    endpoints: [],
    ...over,
  };
}

// A trivial reachability-style resolver for tests that don't exercise the
// per-map color logic (that logic lives in StationFinderMap / AprsPositionsMap;
// PeerLayer only applies whatever the resolver returns).
const goodVisual: (p: AggregatedPeer) => PeerVisual = () => ({ tierClass: 'peer-pin--good', dashed: false });

describe('PeerLayer (Task 24)', () => {
  it('places one circle divIcon pin per map-placeable peer when enabled', async () => {
    await renderMap(
      <LeafletMap>
        <PeerLayer peers={[peerFixture()]} enabled visualFor={goodVisual} onSelect={() => {}} />
      </LeafletMap>,
    );
    const markers = peerMarkers();
    expect(markers).toHaveLength(1);
    const html = (markers[0].options.icon as L.DivIcon).options.html as string;
    expect(html).toContain('peer-pin'); // the circle shape class
  });

  it('applies the parent-supplied per-map visual (tier class + dashed) to the divIcon', async () => {
    // Spec §6: color is per-map, resolved by the parent; PeerLayer only renders
    // what `visualFor` returns. A finder "no prediction" peer is dashed untiered;
    // this proves both the tier class and the dashed modifier flow through.
    const untieredDashed: (p: AggregatedPeer) => PeerVisual = () => ({
      tierClass: 'peer-pin--untiered',
      dashed: true,
    });
    await renderMap(
      <LeafletMap>
        <PeerLayer peers={[peerFixture()]} enabled visualFor={untieredDashed} onSelect={() => {}} />
      </LeafletMap>,
    );
    const html = (peerMarkers()[0].options.icon as L.DivIcon).options.html as string;
    expect(html).toContain('peer-pin--untiered');
    expect(html).toContain('peer-pin--dashed');
    expect(html).not.toContain('peer-pin--good'); // the resolver, not an invented default, decides color
  });

  it('escapes a hostile callsign at the divIcon HTML boundary', async () => {
    const hostile = peerFixture({ callsign: '<img src=x>' });
    await renderMap(
      <LeafletMap>
        <PeerLayer peers={[hostile]} enabled visualFor={goodVisual} onSelect={() => {}} />
      </LeafletMap>,
    );
    const html = (peerMarkers()[0].options.icon as L.DivIcon).options.html as string;
    expect(html).toContain('&lt;img'); // escaped
    expect(html).not.toContain('<img src=x>'); // never a live tag
  });

  it('fires onSelect (via a ref, not stale-closed) when a peer pin is clicked', async () => {
    const onSelect = vi.fn();
    const peer = peerFixture();
    const { rerender } = await renderMap(
      <LeafletMap>
        <PeerLayer peers={[peer]} enabled visualFor={goodVisual} onSelect={() => {}} />
      </LeafletMap>,
    );
    // Re-render with a fresh onSelect identity (as a parent re-render would pass)
    // BEFORE the click, proving the marker's handler reads the CURRENT callback
    // via a ref rather than the one captured at marker-creation time.
    await act(async () => {
      rerender(
        <LeafletMap>
          <PeerLayer peers={[peer]} enabled visualFor={goodVisual} onSelect={onSelect} />
        </LeafletMap>,
      );
      await Promise.resolve();
    });
    act(() => {
      peerMarkers()[0].fire('click');
    });
    expect(onSelect).toHaveBeenCalledWith(peer);
  });

  it('hides every peer marker when map_peers is disabled (capability-hide, absence test)', async () => {
    await renderMap(
      <LeafletMap>
        <PeerLayer
          peers={[peerFixture(), peerFixture({ id: 'p2', callsign: 'N0XYZ', grid: 'EN34' })]}
          enabled={false}
          visualFor={goodVisual}
          onSelect={() => {}}
        />
      </LeafletMap>,
    );
    expect(peerMarkers()).toHaveLength(0);
  });

  it('drops rail-only peers (no grid / not map-placeable) instead of pinning them', async () => {
    await renderMap(
      <LeafletMap>
        <PeerLayer
          peers={[peerFixture({ id: 'p2', mapPlaceable: false, grid: undefined })]}
          enabled
          visualFor={goodVisual}
          onSelect={() => {}}
        />
      </LeafletMap>,
    );
    expect(peerMarkers()).toHaveLength(0);
  });

  it('skips a peer pin when its base callsign is currently live on APRS (live RF truth wins)', async () => {
    await renderMap(
      <LeafletMap>
        <PeerLayer
          peers={[peerFixture()]}
          enabled
          visualFor={goodVisual}
          onSelect={() => {}}
          liveAprsCallsigns={new Set(['W6ABC'])}
        />
      </LeafletMap>,
    );
    expect(peerMarkers()).toHaveLength(0);
  });
});
