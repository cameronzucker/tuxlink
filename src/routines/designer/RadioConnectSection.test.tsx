// RadioConnectSection tests (tuxlink-fg0em) — the radio.connect step's
// dedicated inspector body over the REAL selection surfaces.
//
// The mount is a CONTROLLED harness (adrev 5.6: a fire-and-inspect mock
// parent is false-green for sequential edits) — every onChange re-renders
// the component with the committed params, exactly like the real designer's
// draft state does.
//
// Covers: params read/commit round-trips, sequential edits, rig/unknown-key
// preservation, whole-value refs in BOTH stations and bands (read-only +
// preserved across unrelated commits), malformed-listen preservation and
// explicit replacement, integer-only listen, duplicate-station removal by
// index, the "Runs on" transport line incl. packet precedence and the
// both/none refusal states, and the picker's Finder/Favorites tabs.
// RADIO-1 purity: nothing here connects — the picker only writes params.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useState, createElement, type ReactNode } from 'react';

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

function routeInvoke(opts: { modemKind?: 'packet' | 'vara' | 'ardop' | 'both' | 'none' } = {}) {
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

/** Controlled harness: commits feed back into props, like the real draft. */
function Harness({
  initial,
  onCommit,
}: {
  initial: Record<string, unknown>;
  onCommit: (p: Record<string, unknown>) => void;
}) {
  const [params, setParams] = useState(initial);
  return (
    <RadioConnectSection
      params={params}
      onChange={(p) => {
        setParams(p);
        onCommit(p);
      }}
    />
  );
}

function mount(initial: Record<string, unknown>) {
  const onCommit = vi.fn();
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
  const utils = render(<Harness initial={initial} onCommit={onCommit} />, { wrapper });
  return { onCommit, ...utils };
}

beforeEach(() => {
  invokeMock.mockReset();
  routeInvoke();
});

describe('RadioConnectSection', () => {
  it('renders ordered station chips and removes by INDEX, preserving rig verbatim', () => {
    const { onCommit } = mount({
      // Duplicate station is legal — the runtime walks the exact sequence.
      stations: ['N0DAJ', 'W7RMS', 'N0DAJ'],
      bands: ['20m'],
      rig: 'ic-7300',
    });
    expect(screen.getByTestId('rc-station-0-N0DAJ')).toHaveTextContent('1');
    expect(screen.getByTestId('rc-station-2-N0DAJ')).toHaveTextContent('3');
    expect(screen.getByTestId('rc-extras')).toHaveTextContent('rig');

    // Removing the FIRST N0DAJ keeps the third-position duplicate.
    fireEvent.click(screen.getByLabelText('Remove station 1 (N0DAJ)'));
    expect(onCommit).toHaveBeenLastCalledWith({
      stations: ['W7RMS', 'N0DAJ'],
      bands: ['20m'],
      rig: 'ic-7300',
    });
  });

  it('sequential band toggles accumulate through the controlled parent', () => {
    const { onCommit } = mount({ stations: ['N0DAJ'], bands: ['20m'] });
    fireEvent.click(screen.getByTestId('rc-band-40m'));
    expect(onCommit).toHaveBeenLastCalledWith({ stations: ['N0DAJ'], bands: ['20m', '40m'] });

    // The controlled harness fed ['20m','40m'] back — removing 20m keeps 40m.
    fireEvent.click(screen.getByTestId('rc-band-20m'));
    expect(onCommit).toHaveBeenLastCalledWith({ stations: ['N0DAJ'], bands: ['40m'] });

    // Removing the last band drops the key entirely.
    fireEvent.click(screen.getByTestId('rc-band-40m'));
    expect(onCommit).toHaveBeenLastCalledWith({ stations: ['N0DAJ'] });
  });

  it('preserves a whole-value bands ref across unrelated commits and renders it read-only', () => {
    const { onCommit } = mount({ stations: ['N0DAJ'], bands: '$s1.bands' });
    expect(screen.getByTestId('rc-bands-ref')).toHaveTextContent('$s1.bands');
    expect(screen.queryByTestId('rc-bands')).toBeNull();

    // An unrelated edit (listen) must NOT touch the ref.
    const input = screen.getByTestId('rc-listen');
    fireEvent.change(input, { target: { value: '5' } });
    fireEvent.blur(input);
    expect(onCommit).toHaveBeenLastCalledWith({
      stations: ['N0DAJ'],
      bands: '$s1.bands',
      listen_before_tx_s: 5,
    });
  });

  it('preserves a malformed listen value on unrelated commits, drops it on explicit replace', () => {
    const { onCommit } = mount({ stations: ['N0DAJ'], listen_before_tx_s: 'soon' });
    expect(screen.getByTestId('rc-listen-uneditable')).toBeInTheDocument();

    // Unrelated commit: malformed value carried verbatim.
    fireEvent.click(screen.getByTestId('rc-band-20m'));
    expect(onCommit).toHaveBeenLastCalledWith({
      stations: ['N0DAJ'],
      bands: ['20m'],
      listen_before_tx_s: 'soon',
    });

    // Explicit replacement through the field WINS over the old raw value.
    const input = screen.getByTestId('rc-listen');
    fireEvent.change(input, { target: { value: '7' } });
    fireEvent.blur(input);
    expect(onCommit).toHaveBeenLastCalledWith({
      stations: ['N0DAJ'],
      bands: ['20m'],
      listen_before_tx_s: 7,
    });
  });

  it('rejects non-integer listen values (backend contract is u64)', () => {
    const { onCommit } = mount({ stations: ['N0DAJ'], listen_before_tx_s: 5 });
    const input = screen.getByTestId('rc-listen');
    fireEvent.change(input, { target: { value: '1.5' } });
    fireEvent.blur(input);
    // No commit with a fraction; the field reverts to the committed value.
    expect(onCommit).not.toHaveBeenCalled();
    expect((input as HTMLInputElement).value).toBe('5');

    fireEvent.change(input, { target: { value: '' } });
    fireEvent.blur(input);
    expect(onCommit).toHaveBeenLastCalledWith({ stations: ['N0DAJ'] });
  });

  it('renders a whole-value stations ref read-only with no add affordance', () => {
    mount({ stations: '$s1.callsigns' });
    expect(screen.getByTestId('rc-stations-ref')).toHaveTextContent('$s1.callsigns');
    expect(screen.queryByTestId('rc-add-station')).toBeNull();
  });

  it('shows the derived transport: VARA, packet precedence, and the refusal states', async () => {
    mount({ stations: [] });
    await waitFor(() =>
      expect(screen.getByTestId('rc-runson')).toHaveTextContent('VARA HF · 2300 Hz'),
    );

    invokeMock.mockReset();
    routeInvoke({ modemKind: 'packet' });
    mount({ stations: [] });
    await waitFor(() =>
      expect(
        screen
          .getAllByTestId('rc-runson')
          .some((el) => /Packet \(KISS\)/.test(el.textContent ?? '')),
      ).toBe(true),
    );

    invokeMock.mockReset();
    routeInvoke({ modemKind: 'both' });
    mount({ stations: [] });
    await waitFor(() =>
      expect(
        screen
          .getAllByTestId('rc-runson')
          .some((el) => /runs will refuse/i.test(el.textContent ?? '')),
      ).toBe(true),
    );
  });

  it('Finder waits for config, fetches the configured mode only, filters by band, adds on click', async () => {
    const { onCommit } = mount({ stations: [], bands: ['20m'] });
    fireEvent.click(screen.getByTestId('rc-add-station'));
    await waitFor(() => expect(screen.getByTestId('rc-finder-W7RMS')).toBeInTheDocument());
    expect(screen.queryByTestId('rc-finder-K7HTZ')).toBeNull();

    // The fetch ran ONLY for the configured modem's mode (vara-hf) — not the
    // both-modes fallback of an unresolved config (adrev consensus P2).
    const fetchCalls = invokeMock.mock.calls.filter((c) => c[0] === 'catalog_fetch_stations');
    expect(fetchCalls.length).toBe(1);
    expect(fetchCalls[0][1]).toMatchObject({ modes: ['vara-hf'] });

    fireEvent.click(screen.getByTestId('rc-finder-W7RMS'));
    expect(onCommit).toHaveBeenLastCalledWith({ stations: ['W7RMS'], bands: ['20m'] });
  });

  it('Favorites tab lists starred favorites and adds the gateway on click', async () => {
    const { onCommit } = mount({ stations: [] });
    fireEvent.click(screen.getByTestId('rc-add-station'));
    fireEvent.click(screen.getByRole('tab', { name: 'Favorites' }));
    await waitFor(() => expect(screen.getByTestId('rc-fav-N0DAJ')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('rc-fav-N0DAJ'));
    expect(onCommit).toHaveBeenLastCalledWith({ stations: ['N0DAJ'] });
  });

  it('resyncs the listen field when a same-step external update changes params', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(QueryClientProvider, { client: qc }, children);
    const onChange = vi.fn();
    const { rerender } = render(
      <RadioConnectSection params={{ stations: ['N0DAJ'], listen_before_tx_s: 5 }} onChange={onChange} />,
      { wrapper },
    );
    expect((screen.getByTestId('rc-listen') as HTMLInputElement).value).toBe('5');
    // External (JSON-mode) update of the SAME step — no remount.
    await act(async () => {
      rerender(
        <RadioConnectSection
          params={{ stations: ['N0DAJ'], listen_before_tx_s: 9 }}
          onChange={onChange}
        />,
      );
    });
    expect((screen.getByTestId('rc-listen') as HTMLInputElement).value).toBe('9');
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
