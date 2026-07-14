// Phase D1 — the REACHABILITY test.
//
// Phase C built LiveBandStrip, Ft8SetupSurface, Waterfall, DecodeFeed and
// BandSubsetPopover, every one of them CI-green and unit-tested, and mounted NONE
// of them: `<LiveBandStrip` had zero call sites in production code, so the entire
// Station Intelligence FT-8 feature was invisible to the operator while every
// component test passed. A component test cannot catch that — it renders the
// component directly, which is exactly the thing production wasn't doing.
//
// This test renders the REAL StationFinderPanel the way AppShell does and asserts
// the FT-8 surfaces are actually there. It fails if anyone unmounts the strip
// again, and it is the check that was missing.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import { invoke } from '@tauri-apps/api/core';
import { StationFinderPanel } from './StationFinderPanel';
import { Ft8ListenerProvider } from '../ft8ui/useFt8Listener';
// tuxlink-10bkw Task 6: the panel calls useFirstOpenTip('find-a-station'),
// which throws outside a <HintProvider> ancestor.
import { HintProvider } from '../onboarding/HintProvider';

/** A listener snapshot in a live, decoding state — the strip's live body arm.
 *  Shape mirrors `Ft8Snapshot` (ft8Types.ts) exactly; a drifted fixture would
 *  derive 'off' and silently weaken the assertions below. */
const DECODING_SNAPSHOT = {
  service: { axis: 'listening' },
  flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
  slotPhase: 'decoded',
  band: '20m',
  dialHz: 14_074_000,
  bandSource: 'cat-confirmed',
  bandLabelConfirmedUtcMs: 1000,
  sweep: { mode: 'inactive', bandIdx: null, dwellProgress: null },
  engineVersion: 'jt9 2.6.1',
  nConsecutive: 0,
  kConsecutive: 0,
  lastSlotUtcMs: null,
  lastFailure: null,
  availableDevices: null,
  ringTail: [],
  sweepConfig: { enabled: false, bands: ['20m'], dwellSlots: 4 },
  configuredDeviceName: 'Digirig Mobile',
};

function renderPanel() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <HintProvider>
        <Ft8ListenerProvider>
          <StationFinderPanel onClose={() => {}} />
        </Ft8ListenerProvider>
      </HintProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'ft8_listener_snapshot') return DECODING_SNAPSHOT;
    if (cmd === 'catalog_fetch_stations' || cmd === 'catalog_list') return [];
    if (cmd === 'p2p_capabilities') return { finder_peers: false, map_peers: false };
    if (cmd === 'contacts_read') return { contacts: [] };
    if (cmd === 'position_current_fix') return { grid: 'DM43bp' };
    if (cmd === 'basemap_list_packs') return { packs: [] };
    return undefined;
  });
});

describe('StationFinderPanel — FT-8 surfaces are actually mounted (Phase D1)', () => {
  it('mounts the live band strip in the real panel (it had ZERO call sites before D1)', async () => {
    renderPanel();
    // The strip is the host for the waterfall, decode feed, stats and the band
    // popover. If this is absent, the whole FT-8 feature is unreachable — which is
    // exactly the state Phase C shipped in.
    await waitFor(() => expect(screen.getByTestId('ft8-strip')).toBeTruthy());
  });

  it('feeds the strip the LIVE listener state (not a hardcoded/default snapshot)', async () => {
    renderPanel();
    const strip = await screen.findByTestId('ft8-strip');
    // data-state is derived from the snapshot the panel threaded in. A strip wired
    // to nothing would sit in the pre-hydrate 'off' state forever.
    await waitFor(() => {
      expect(strip.getAttribute('data-state')).not.toBe('off');
    });
  });

  it('reads ft8_listener_snapshot — proving the panel is inside the listener provider', async () => {
    renderPanel();
    await waitFor(() =>
      expect(vi.mocked(invoke).mock.calls.some((c) => c[0] === 'ft8_listener_snapshot')).toBe(true),
    );
  });
});
