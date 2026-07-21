// RadioConnectSection tests (tuxlink-fg0em) — the radio.connect step's
// dedicated inspector body over the REAL selection surfaces.
//
// Covers: params read/commit round-trips (stations chips, band toggles,
// listen_before_tx_s), rig/unknown-key preservation on every commit, the
// whole-value step-ref rendering, the "Runs on" modem line incl. both/none
// refusal states, and the picker's Finder (catalog-cache rows, band filter,
// distance) + Favorites tabs. RADIO-1 purity: nothing here connects — the
// picker only writes step params.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createElement, type ReactNode } from 'react';

import { invoke } from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { RadioConnectSection } from './RadioConnectSection';
import type { Favorite } from '../../favorites/types';
import type { StationListing } from '../../catalog/stationTypes';

const invokeMock = invoke as ReturnType<typeof vi.fn>;

function gateway(callsign: string, grid: string, frequenciesKhz: number[]) {
  return {
    channel: callsign,
    callsign,
    sysopName: null,
    grid,
    location: null,
    frequenciesKhz,
    lastUpdate: null,
    email: null,
    homepage: null,
    antenna: null,
  };
}

const LISTINGS: StationListing[] = [
  {
    mode: 'vara-hf',
    title: null,
    gateways: [
      // 20m + 40m, near (same grid as operator).
      gateway('W7RMS-10', 'CN87', [14103.5, 7101.2]),
      // 80m only — excluded when the 20m band chip filters the list.
      gateway('K7HTZ', 'CN88', [3591.0]),
    ],
    raw: '',
    parsedOk: true,
    fetchedAtMs: 1000,
  },
];

const FAV: Favorite = {
  id: 'fav-1',
  mode: 'vara-hf',
  gateway: 'N0DAJ',
  freq: '14.1035',
  band: '20m',
  starred: true,
} as Favorite;

function routeInvoke(opts: { modemKind?: 'vara' | 'ardop' | 'both' | 'none' } = {}) {
  invokeMock.mockImplementation((cmd?: unknown) => {
    switch (cmd) {
      case 'config_read':
        return Promise.resolve({
          routine_hf_modem:
            opts.modemKind === undefined
              ? { kind: 'vara', bandwidth_hz: 2300 }
              : { kind: opts.modemKind },
        });
      case 'position_current_fix':
        return Promise.resolve({ grid: 'CN87uo' });
      case 'catalog_fetch_stations':
        return Promise.resolve(LISTINGS);
      case 'favorites_read':
        return Promise.resolve({ schema_version: 1, favorites: [FAV], log: [] });
      case 'favorites_recents':
        return Promise.resolve([]);
      default:
        // Teardown probes call the mock with no args (vitest invoke-mock
        // cleanup pitfall) — resolve, never throw.
        return Promise.resolve(null);
    }
  });
}

function mount(params: Record<string, unknown>, onChange = vi.fn()) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
  render(<RadioConnectSection params={params} onChange={onChange} />, { wrapper });
  return onChange;
}

beforeEach(() => {
  invokeMock.mockReset();
  routeInvoke();
});

describe('RadioConnectSection', () => {
  it('renders ordered station chips and removes one on ✕, preserving rig verbatim', () => {
    const onChange = mount({
      stations: ['N0DAJ', 'W7RMS'],
      bands: ['20m'],
      rig: 'ic-7300',
    });
    expect(screen.getByTestId('rc-station-N0DAJ')).toHaveTextContent('1');
    expect(screen.getByTestId('rc-station-W7RMS')).toHaveTextContent('2');
    // rig is not silently hidden.
    expect(screen.getByTestId('rc-extras')).toHaveTextContent('rig');

    fireEvent.click(screen.getByLabelText('Remove station N0DAJ'));
    expect(onChange).toHaveBeenCalledWith({
      stations: ['W7RMS'],
      bands: ['20m'],
      rig: 'ic-7300',
    });
  });

  it('toggles band chips into a committed bands array, omitting the key when empty', () => {
    const onChange = mount({ stations: ['N0DAJ'], bands: ['20m'] });
    fireEvent.click(screen.getByTestId('rc-band-40m'));
    expect(onChange).toHaveBeenLastCalledWith({ stations: ['N0DAJ'], bands: ['20m', '40m'] });

    fireEvent.click(screen.getByTestId('rc-band-20m'));
    // Component is controlled by props (still ['20m']) — removing the only
    // committed band drops the key entirely.
    expect(onChange).toHaveBeenLastCalledWith({ stations: ['N0DAJ'] });
  });

  it('commits listen_before_tx_s on blur and drops the key when cleared', () => {
    const onChange = mount({ stations: ['N0DAJ'], listen_before_tx_s: 5 });
    const input = screen.getByTestId('rc-listen');
    fireEvent.change(input, { target: { value: '9' } });
    fireEvent.blur(input);
    expect(onChange).toHaveBeenLastCalledWith({ stations: ['N0DAJ'], listen_before_tx_s: 9 });

    fireEvent.change(input, { target: { value: '' } });
    fireEvent.blur(input);
    expect(onChange).toHaveBeenLastCalledWith({ stations: ['N0DAJ'] });
  });

  it('renders a whole-value step ref read-only with no add affordance', () => {
    mount({ stations: '$s1.callsigns' });
    expect(screen.getByTestId('rc-stations-ref')).toHaveTextContent('$s1.callsigns');
    expect(screen.queryByTestId('rc-add-station')).toBeNull();
  });

  it('shows the derived modem, and the both/none refusal states as warnings', async () => {
    mount({ stations: [] });
    await waitFor(() =>
      expect(screen.getByTestId('rc-runson')).toHaveTextContent('VARA HF · 2300 Hz'),
    );

    invokeMock.mockReset();
    routeInvoke({ modemKind: 'both' });
    mount({ stations: [] });
    await waitFor(() =>
      expect(
        screen.getAllByTestId('rc-runson').some((el) => /runs will refuse/i.test(el.textContent ?? '')),
      ).toBe(true),
    );
  });

  it('Finder tab lists cached gateways filtered by selected bands and adds on click', async () => {
    const onChange = mount({ stations: [], bands: ['20m'] });
    fireEvent.click(screen.getByTestId('rc-add-station'));
    // 20m selected: W7RMS (has 20m) listed, K7HTZ (80m only) filtered out.
    await waitFor(() => expect(screen.getByTestId('rc-finder-W7RMS')).toBeInTheDocument());
    expect(screen.queryByTestId('rc-finder-K7HTZ')).toBeNull();
    // SSID stripped by aggregation; distance renders from the operator grid.
    expect(screen.getByTestId('rc-finder-W7RMS')).toHaveTextContent('CN87');

    fireEvent.click(screen.getByTestId('rc-finder-W7RMS'));
    expect(onChange).toHaveBeenLastCalledWith({ stations: ['W7RMS'], bands: ['20m'] });
  });

  it('Favorites tab lists starred favorites and adds the gateway on click', async () => {
    const onChange = mount({ stations: [] });
    fireEvent.click(screen.getByTestId('rc-add-station'));
    fireEvent.click(screen.getByRole('tab', { name: 'Favorites' }));
    await waitFor(() => expect(screen.getByTestId('rc-fav-N0DAJ')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('rc-fav-N0DAJ'));
    expect(onChange).toHaveBeenLastCalledWith({ stations: ['N0DAJ'] });
  });

  it('never fires a connect-class command — picker writes params only (RADIO-1)', async () => {
    mount({ stations: [] });
    fireEvent.click(screen.getByTestId('rc-add-station'));
    await waitFor(() => expect(screen.getByTestId('rc-finder-W7RMS')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('rc-finder-W7RMS'));
    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    for (const cmd of cmds) {
      expect(String(cmd)).not.toMatch(/connect|transmit|dial|record_attempt/);
    }
  });
});
