import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';
import { EVIDENCE_RECENCY_MS } from './ft8Evidence';

// StationFinderMap renders on MapLibreMap (globally mocked via test-setup).

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
import { invoke } from '@tauri-apps/api/core';
import { StationFinderPanel } from './StationFinderPanel';
import { Ft8ListenerProvider } from '../ft8ui/useFt8Listener';
import { HintProvider } from '../onboarding/HintProvider';

// Phase D1: the panel now hosts the live FT-8 surface (LiveBandStrip) and reads
// the listener via context, so it must render inside Ft8ListenerProvider —
// useFt8Listener throws otherwise. The provider's own
// snapshot fetch + event listeners degrade silently under the invoke mock (it
// catches), so these station-focused cases are unaffected by it.
//
// tuxlink-10bkw Task 6: the panel also calls useFirstOpenTip('find-a-station'),
// which throws outside a <HintProvider> ancestor — wrap here too. HintOverlay
// itself is never mounted in this file, so onboarding state stays invisible.
function renderPanel(ui: ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <HintProvider>
        <Ft8ListenerProvider>{ui}</Ft8ListenerProvider>
      </HintProvider>
    </QueryClientProvider>,
  );
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
    return undefined as unknown as never;
  });
});

describe('StationFinderPanel', () => {
  it('renders the Find a Station dialog with the controls bar', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    expect(await screen.findByRole('dialog', { name: /station intelligence/i })).toBeTruthy();
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
    expect(await screen.findByRole('dialog', { name: /station intelligence/i })).toBeTruthy();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('catalog_fetch_stations', expect.anything()));
    expect(screen.getByRole('dialog', { name: /station intelligence/i })).toBeTruthy();
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
    await screen.findByRole('dialog', { name: /station intelligence/i });
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
    await screen.findByRole('dialog', { name: /station intelligence/i });
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

  // Task 7 (spec evidence filter): the evidence toggle + SNR threshold join the
  // existing persisted finder view (search/bands/modes/radius/selection) so a
  // close/reopen restores the operator's evidence posture too.
  it('persists evidenceOn to localStorage on toggle', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    await screen.findByRole('dialog', { name: /station intelligence/i });
    const toggle = await screen.findByTestId('map-evidence-toggle');
    expect(toggle).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(toggle);

    await waitFor(() => {
      const raw = window.localStorage.getItem('tuxlink:station-finder:view');
      expect(raw).toBeTruthy();
      const parsed = JSON.parse(raw!) as Record<string, unknown>;
      expect(parsed.evidenceOn).toBe(true);
    });
  });

  it('persists evidenceSnrMinDb on threshold change while the toggle is on', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    await screen.findByRole('dialog', { name: /station intelligence/i });
    fireEvent.click(await screen.findByTestId('map-evidence-toggle'));
    const snr = await screen.findByTestId('map-evidence-snr');

    fireEvent.change(snr, { target: { value: '-15' } });

    await waitFor(() => {
      const raw = window.localStorage.getItem('tuxlink:station-finder:view');
      const parsed = JSON.parse(raw!) as Record<string, unknown>;
      expect(parsed.evidenceSnrMinDb).toBe(-15);
    });
  });

  // Fix round 1 (reviewer finding): evidence must RE-evaluate as time passes.
  // A decode that qualified once must stop corroborating its station past the
  // EVIDENCE_RECENCY_MS window even when NO new decode events arrive (frozen
  // decodesRing). The panel's minute tick (evidenceNowMs) drives this.
  it('expires once-qualifying evidence past the recency window with no new decodes', async () => {
    vi.useFakeTimers();
    try {
      const t0 = Date.now(); // fake clock "now"
      // A qualifying decode: same grid as N0DAJ (decode-to-station distance 0,
      // inside the 50 mi radius floor), band 40m (N0DAJ has a 7103 kHz channel),
      // fresh at t0, SNR comfortably above the -24 default floor.
      const slot = {
        slotUtcMs: t0,
        band: '40m',
        dialHz: 7074000,
        bandSource: 'cat-confirmed',
        bandLabelConfirmedUtcMs: null,
        outcome: { kind: 'decoded' },
        decodes: [{
          slotUtcMs: t0, snrDb: -8, dtS: 0, freqHz: 1500, message: 'CQ N0DAJ DM34',
          fromCall: 'N0DAJ', toCall: null, grid: 'DM34oa', partial: false,
        }],
        partialSalvage: false, lostFrames: 0, boundarySkewFrames: 0,
        clipFraction: 0, rmsDbfs: -20, dwellSlotIndex: null,
      };
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'config_read') return { grid: 'DM43bp' } as unknown as never;
        if (cmd === 'catalog_fetch_stations')
          return [{ mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: t0, gateways: [N0DAJ] }] as unknown as never;
        if (cmd === 'favorites_read') return { favorites: [] } as unknown as never;
        // The reachability sweep runs against the fetched station; an undefined
        // return here would make its worker read `.channels` off undefined and
        // surface an unhandled rejection after the test completes.
        if (cmd === 'propagation_predict_path')
          return {
            bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
            channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }],
          } as unknown as never;
        // The provider's hydrate: a stopped listener whose ringTail still holds
        // the decode (the listener recorded it, then went quiet or stopped).
        if (cmd === 'ft8_listener_snapshot')
          return {
            service: { axis: 'stopped' },
            flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
            slotPhase: 'decoded', band: '40m', dialHz: 7074000,
            bandSource: 'cat-confirmed', bandLabelConfirmedUtcMs: null,
            sweep: { mode: 'inactive', bandIdx: null, dwellProgress: null },
            engineVersion: null, nConsecutive: 0, kConsecutive: 0,
            lastSlotUtcMs: t0, lastFailure: null, availableDevices: null,
            ringTail: [slot],
            sweepConfig: { enabled: false, bands: [], dwellSlots: 4 },
            configuredDeviceName: null,
          } as unknown as never;
        return undefined as unknown as never;
      });

      renderPanel(<StationFinderPanel onClose={() => {}} />);
      // Flush the mount-time async work (snapshot hydrate, catalog fetch, grid).
      await act(async () => {
        await Promise.resolve();
      });

      fireEvent.click(screen.getByTestId('map-evidence-toggle'));
      await act(async () => {
        await Promise.resolve();
      });
      expect(screen.getByTestId('map-evidence-note').textContent).toContain(
        '1 of 1 gateways corroborated',
      );

      // Cross the recency boundary plus one evidence tick. No new decode events
      // arrive (the ring reference never changes), so ONLY the time tick can
      // trigger re-evaluation.
      await act(async () => {
        vi.advanceTimersByTime(EVIDENCE_RECENCY_MS + 61_000);
      });
      expect(screen.getByTestId('map-evidence-note').textContent).toContain(
        '0 of 1 gateways corroborated',
      );
    } finally {
      vi.useRealTimers();
    }
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
