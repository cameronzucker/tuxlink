import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';

// StationFinderMap renders on MapLibreMap (globally mocked via test-setup).

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { StationFinderPanel } from './StationFinderPanel';

function renderPanel(ui: ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const N0DAJ = {
  channel: 'N0DAJ', callsign: 'N0DAJ', sysopName: 'Doug', grid: 'DM34oa', location: 'Wickenburg, AZ',
  frequenciesKhz: [3590, 7103], lastUpdate: null, email: null, homepage: null,
};

beforeEach(() => {
  // tuxlink-liqs9: the finder view now persists to localStorage; clear it
  // between tests so one test's filters/selection don't leak into the next.
  window.localStorage.clear();
  vi.mocked(invoke).mockReset();
  // cmd-gated so the runner's stray no-arg cleanup call stays inert.
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { grid: 'DM43bp' } as unknown as never;
    if (cmd === 'propagation_prefs_read')
      return { antenna_preset: 'efhw-sloper', req_snr_db: 22, tx_power_w: 100, antenna_height_m: 9, ground_type: 'average', noise_environment: 'residential' } as unknown as never;
    if (cmd === 'propagation_prefs_write') return undefined as unknown as never;
    if (cmd === 'catalog_fetch_stations')
      return [{ mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1_700_000_000_000, gateways: [N0DAJ] }] as unknown as never;
    if (cmd === 'propagation_predict_path')
      return {
        bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
        channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }],
      } as unknown as never;
    // tuxlink-5016: the panel now reads the favorites file to drive the ★ save
    // affordance. Return an empty file so the query RESOLVES (a bare undefined
    // would trip react-query's "Query data cannot be undefined").
    if (cmd === 'favorites_read') return { favorites: [] } as unknown as never;
    // Task 23: the panel now also reads the P2P capability bits + peer roster
    // (usePeers/useP2pCapabilities). Default to capability-off + an empty
    // roster for tests that don't care about peers, for the same "no bare
    // undefined" reason as favorites_read above.
    if (cmd === 'p2p_capabilities')
      return {
        peer_store: false, finder_peers: false, map_peers: false, settings_editor: false,
        agent_find_peers: false, agent_telnet_dial: false, vara_engine_split: false,
        favorites_peer_link: false,
      } as unknown as never;
    if (cmd === 'peers_read') return { schema_version: 1, peers: [] } as unknown as never;
    return undefined as unknown as never;
  });
});

describe('StationFinderPanel', () => {
  it('renders the Find a Station dialog with the controls bar', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    expect(await screen.findByRole('dialog', { name: /find a station/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /40 m/ })).toBeTruthy();
  });

  it('fetches + aggregates stations and mounts the map', async () => {
    // N0DAJ (DM34oa) is ~134 mi from the operator (DM43bp) — inside the default
    // 500 mi radius. Pins are now GeoJSON circle-layer features (MapLibre), not
    // Leaflet markers; the per-station feature wiring is covered in
    // StationFinderMap.test. Here the integration check is that the panel fetches
    // and mounts the station map. (Real pin colour/click → browser smoke.)
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    expect(await screen.findByTestId('station-map')).toBeTruthy();
  });

  // NOTE: pin-click → rail population is validated by browser smoke, not here:
  // pins are L.divIcon markers and the test mock cannot fire their eventHandlers.
  // StationRail's render-from-props is covered in StationRail.test.tsx.

  it('closes on the × button', async () => {
    const onClose = vi.fn();
    renderPanel(<StationFinderPanel onClose={onClose} />);
    fireEvent.click(await screen.findByRole('button', { name: /close/i }));
    expect(onClose).toHaveBeenCalled();
  });

  it('does not crash when catalog_fetch_stations resolves undefined (degenerate backend)', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { grid: 'DM43bp' } as unknown as never;
      if (cmd === 'catalog_fetch_stations') return undefined as unknown as never; // null/empty response
      return undefined as unknown as never;
    });
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    // The dialog renders on first paint; the crash (if any) is on the post-fetch
    // re-render. Wait a tick so the fetch resolves, then assert still mounted.
    expect(await screen.findByRole('dialog', { name: /find a station/i })).toBeTruthy();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('catalog_fetch_stations', expect.anything()));
    expect(screen.getByRole('dialog', { name: /find a station/i })).toBeTruthy();
  });

  it('closes on Escape', async () => {
    const onClose = vi.fn();
    renderPanel(<StationFinderPanel onClose={onClose} />);
    await screen.findByRole('dialog');
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  // tuxlink-q1tm regression: a GPS operator has NO manual grid
  // (config.identity.grid = null, position_source = Gps); the live grid comes
  // from the PositionArbiter via `position_current_fix`. Find a Station must use
  // it, or the aiming/bearing header + HF prediction die and the panel falsely
  // shows "set your location". This test is RED if the panel reads config_read
  // alone (the pre-fix behavior).
  it('resolves the operator grid from GPS (position_current_fix) when no manual grid is set', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'position_current_fix')
        return { grid: 'DM43bp', source: 'Gps', fresh: true } as unknown as never;
      if (cmd === 'config_read') return { grid: null } as unknown as never; // no manual grid
      if (cmd === 'propagation_prefs_read')
        return { antenna_preset: 'efhw-sloper', req_snr_db: 22, tx_power_w: 100, antenna_height_m: 9, ground_type: 'average', noise_environment: 'residential' } as unknown as never;
      if (cmd === 'catalog_fetch_stations')
        return [{ mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1_700_000_000_000, gateways: [N0DAJ] }] as unknown as never;
      if (cmd === 'propagation_predict_path')
        return {
          bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
          channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }],
        } as unknown as never;
      return undefined as unknown as never;
    });
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    await screen.findByRole('dialog', { name: /find a station/i });
    // The GPS grid resolved → the "set your location" degraded hint is absent.
    await waitFor(() =>
      expect(screen.queryByText(/set your location \(status bar\)/i)).toBeNull(),
    );
  });

  // tuxlink-ziyu regression: a burst of antenna-control changes (a height-slider
  // drag fires onChange per grid-index crossing) must NOT persist + recompute
  // once per event. Before the fix, each change synchronously called
  // propagation_prefs_write and bumped the reachability reload key, launching a
  // full N-station voacapl re-sweep per tick. The debounced commit defers the
  // write and coalesces the burst into a single persist.
  it('debounces + coalesces a burst of antenna-control changes into one persist', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    await screen.findByRole('dialog', { name: /find a station/i });
    const slider = await screen.findByTestId('antenna-height-slider');

    // Fire a rapid burst (a slider drag across several grid stops).
    for (const idx of ['0', '1', '2', '3', '4']) {
      fireEvent.change(slider, { target: { value: idx } });
    }
    // Deferred, not synchronous: the old code would already have written here.
    expect(invoke).not.toHaveBeenCalledWith('propagation_prefs_write', expect.anything());

    // After the debounce settles, exactly one persist for the whole burst.
    await waitFor(
      () => expect(invoke).toHaveBeenCalledWith('propagation_prefs_write', expect.anything()),
      { timeout: 1500 },
    );
    const writes = vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'propagation_prefs_write');
    expect(writes.length).toBe(1);
  });

  // Task 23 (spec §5, R5-8 capability hide): the P2P peer roster joins the
  // finder as a Gateway/Peer type filter. A gridless telnet-only peer has no
  // map pin, so it must still surface in the rail (untiered) or it's
  // invisible entirely — that's the aggregatePeers contract [R4-8] this task
  // wires into the panel.
  const GRIDLESS_TELNET_PEER = {
    id: 'peer-1',
    canonical_base: 'W6XYZ',
    presented_callsigns: ['W6XYZ'],
    identity_kind: 'unknown',
    do_not_merge: false,
    conflict: false,
    source: 'auto',
    origin: 'incoming',
    contact_id: null,
    grid: null,
    note: '',
    created_at: '2026-07-01T00:00:00Z',
    last_connected_at: null,
    channels: [],
    endpoints: [
      {
        id: 'ep-1',
        host: '203.0.113.5',
        port: 8772,
        provenance: 'observed-incoming',
        last_seen: '2026-07-01T00:00:00Z',
      },
    ],
  };

  function mockInvokeWithPeers(finderPeers: boolean) {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { grid: 'DM43bp' } as unknown as never;
      if (cmd === 'propagation_prefs_read')
        return {
          antenna_preset: 'efhw-sloper', req_snr_db: 22, tx_power_w: 100,
          antenna_height_m: 9, ground_type: 'average', noise_environment: 'residential',
        } as unknown as never;
      if (cmd === 'catalog_fetch_stations') return [] as unknown as never;
      if (cmd === 'favorites_read') return { favorites: [] } as unknown as never;
      if (cmd === 'p2p_capabilities')
        return {
          peer_store: true, finder_peers: finderPeers, map_peers: true, settings_editor: true,
          agent_find_peers: true, agent_telnet_dial: true, vara_engine_split: true,
          favorites_peer_link: true,
        } as unknown as never;
      if (cmd === 'peers_read') return { schema_version: 1, peers: [GRIDLESS_TELNET_PEER] } as unknown as never;
      return undefined as unknown as never;
    });
  }

  it('renders the Gateway/Peer type chips + a gridless peer untiered in the rail; toggling Peer off hides it', async () => {
    mockInvokeWithPeers(true);
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    await screen.findByRole('dialog', { name: /find a station/i });

    // (a) the Gateway/Peer type chips render.
    expect(await screen.findByTestId('type-chip-gateway')).toBeTruthy();
    expect(screen.getByTestId('type-chip-peer')).toBeTruthy();

    // (c) the gridless peer appears in the rail, untiered, even though it has
    // no grid to be map-placeable with.
    expect(await screen.findByTestId('peer-row-peer-1')).toBeTruthy();
    expect(screen.getByTestId('peer-untiered-peer-1')).toBeTruthy();

    // (b) toggling Peer off hides the peer row (hide, not disable — it's gone
    // from the DOM, not merely dimmed/disabled).
    fireEvent.click(screen.getByTestId('type-chip-peer'));
    await waitFor(() => expect(screen.queryByTestId('peer-row-peer-1')).toBeNull());
  });

  it('capability-hides the type chips and peer rows entirely when finder_peers is false', async () => {
    mockInvokeWithPeers(false);
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    await screen.findByRole('dialog', { name: /find a station/i });

    // (d) NO peer chip and NO peer row — even though peers_read still
    // returned a peer. This is the capability HIDE, not a data-empty state.
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('p2p_capabilities'));
    expect(screen.queryByTestId('type-chip-peer')).toBeNull();
    expect(screen.queryByTestId('type-chip-gateway')).toBeNull();
    expect(screen.queryByTestId('peer-row-peer-1')).toBeNull();
    expect(screen.queryByTestId('peer-rows')).toBeNull();
  });

  it('shows the "set your location" hint only when neither GPS nor a manual grid is available', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'position_current_fix') return { grid: null } as unknown as never;
      if (cmd === 'config_read') return { grid: null } as unknown as never;
      if (cmd === 'propagation_prefs_read')
        return { antenna_preset: 'efhw-sloper', req_snr_db: 22, tx_power_w: 100, antenna_height_m: 9, ground_type: 'average', noise_environment: 'residential' } as unknown as never;
      return undefined as unknown as never;
    });
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    await screen.findByRole('dialog');
    expect(await screen.findByText(/set your location \(status bar\)/i)).toBeTruthy();
  });
});
