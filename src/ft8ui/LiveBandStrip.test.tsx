// src/ft8ui/LiveBandStrip.test.tsx — Task C7.
//
// `LiveBandStrip` composes real children (Waterfall, DecodeFeed,
// BandSubsetPopover) rather than mocking them out — they are already
// unit-tested standalone (C8/C11/C10); this file exercises the CONTAINER's
// own responsibilities: header dot/chips, flags-overlay layer, stats,
// collapse persistence + force-expand override, and the leaf composition
// wiring (blockingSessionMode passthrough, expanded gating).
//
// `@tauri-apps/api/{core,event}` are mocked at module level (repo idiom, see
// useFt8Listener.test.ts / Waterfall.test.tsx / BandSubsetPopover.test.tsx):
// `invoke` is GATED ON `cmd` so vitest's stray no-arg teardown call
// (feedback_vitest_invoke_mock_cleanup_call) is inert; `listen` resolves to a
// no-op unlisten so Waterfall's subscribe effect never crashes in jsdom.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { LiveBandStrip, dotToneFor, formatDialMHz, type LiveBandStripProps } from './LiveBandStrip';
import { stripStats } from './deriveBandActivity';
import type { DecodeDto, Ft8Snapshot, Ft8UiState, Ft8Flags, SlotRecord } from './ft8Types';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

const NOW = 1_000_000_000; // arbitrary epoch ms anchor — matches DecodeFeed.test.tsx's convention.
const STORAGE_KEY = 'tuxlink:ft8:strip';

function makeSnapshot(over: Partial<Ft8Snapshot> = {}): Ft8Snapshot {
  return {
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
    ...over,
  };
}

function makeUiState(
  state: Ft8UiState,
  flagsOver: Partial<Ft8Flags> = {},
): { state: Ft8UiState; flags: Ft8Flags } {
  return {
    state,
    flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false, ...flagsOver },
  };
}

function mkDecode(over: Partial<DecodeDto> = {}): DecodeDto {
  return {
    slotUtcMs: NOW,
    snrDb: -10,
    dtS: 0,
    freqHz: 1500,
    message: 'CQ N0CALL EM12',
    fromCall: 'N0CALL',
    toCall: null,
    grid: 'EM12',
    partial: false,
    ...over,
  };
}

function mkSlot(slotUtcMs: number, decodes: DecodeDto[], over: Partial<SlotRecord> = {}): SlotRecord {
  return {
    slotUtcMs,
    band: '20m',
    dialHz: 14_074_000,
    bandSource: 'cat-confirmed',
    bandLabelConfirmedUtcMs: null,
    outcome: decodes.length ? { kind: 'decoded' } : { kind: 'band-dead' },
    decodes,
    partialSalvage: false,
    lostFrames: 0,
    boundarySkewFrames: 0,
    clipFraction: 0,
    rmsDbfs: -20,
    dwellSlotIndex: null,
    ...over,
  };
}

function renderStrip(overrides: Partial<LiveBandStripProps> = {}) {
  const props: LiveBandStripProps = {
    snapshot: makeSnapshot(),
    uiState: makeUiState('decoding'),
    decodesRing: [],
    nowMs: NOW,
    ...overrides,
  };
  return render(<LiveBandStrip {...props} />);
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'ft8_cat_probe') return new Promise(() => {}); // never resolves — deterministic
    return Promise.resolve();
  });
  window.localStorage.clear();
});

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

// ---------------------------------------------------------------------------
// dotToneFor — pure mapping, exported for direct unit testing.
// ---------------------------------------------------------------------------

describe('dotToneFor', () => {
  it('renders RED for wedged, regardless of flags (severity pin — distinct from amber)', () => {
    expect(dotToneFor('wedged', { clockUnsynced: false, catFixedBand: false, jt9Degraded: false })).toBe(
      'red',
    );
    expect(dotToneFor('wedged', { clockUnsynced: true, catFixedBand: true, jt9Degraded: true })).toBe(
      'red',
    );
  });

  it('renders GREEN for the three live phase rows with no flags active', () => {
    for (const s of ['decoding', 'waiting-first-slot', 'band-dead'] as const) {
      expect(dotToneFor(s, { clockUnsynced: false, catFixedBand: false, jt9Degraded: false })).toBe(
        'green',
      );
    }
  });

  it('overlays AMBER on a live state when clockUnsynced or jt9Degraded is set', () => {
    expect(dotToneFor('decoding', { clockUnsynced: true, catFixedBand: false, jt9Degraded: false })).toBe(
      'amber',
    );
    expect(dotToneFor('band-dead', { clockUnsynced: false, catFixedBand: false, jt9Degraded: true })).toBe(
      'amber',
    );
  });

  it('renders AMBER for needs-setup / device-lost / yielded', () => {
    for (const s of ['needs-setup', 'device-lost', 'yielded'] as const) {
      expect(dotToneFor(s, { clockUnsynced: false, catFixedBand: false, jt9Degraded: false })).toBe(
        'amber',
      );
    }
  });

  it('renders OFF/neutral for off and transitional', () => {
    expect(dotToneFor('off', { clockUnsynced: false, catFixedBand: false, jt9Degraded: false })).toBe(
      'off',
    );
    expect(
      dotToneFor('transitional', { clockUnsynced: false, catFixedBand: false, jt9Degraded: false }),
    ).toBe('off');
  });
});

describe('formatDialMHz', () => {
  it('formats Hz to 3-decimal MHz, no unit suffix', () => {
    expect(formatDialMHz(14_074_000)).toBe('14.074');
  });
  it('renders an em-dash placeholder for null/undefined', () => {
    expect(formatDialMHz(null)).toBe('—');
    expect(formatDialMHz(undefined)).toBe('—');
  });
});

// ---------------------------------------------------------------------------
// Header dot — wedged RED, distinct from amber.
// ---------------------------------------------------------------------------

describe('LiveBandStrip — header dot', () => {
  it('renders a RED dot for wedged, with a class distinct from si-dot--amber', () => {
    renderStrip({ uiState: makeUiState('wedged') });
    const dot = screen.getByTestId('ft8-strip-dot');
    expect(dot).toHaveAttribute('data-tone', 'red');
    expect(dot.className).toContain('si-dot--red');
    expect(dot.className).not.toContain('si-dot--amber');
  });

  it('renders GREEN (no modifier class) while decoding with no flags', () => {
    renderStrip({ uiState: makeUiState('decoding') });
    const dot = screen.getByTestId('ft8-strip-dot');
    expect(dot).toHaveAttribute('data-tone', 'green');
    expect(dot.className).toBe('si-dot');
  });
});

// ---------------------------------------------------------------------------
// Force-expand beats persisted collapse.
// ---------------------------------------------------------------------------

describe('LiveBandStrip — force-expand overrides persisted collapse', () => {
  it('seeded tuxlink:ft8:strip=collapsed + uiState wedged -> strip renders expanded', () => {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(true)); // "collapsed"
    renderStrip({ uiState: makeUiState('wedged') });
    const strip = screen.getByTestId('ft8-strip');
    expect(strip).toHaveAttribute('data-collapsed', 'false');
    // The body region — here the wedged restart banner — is actually visible,
    // not merely present-but-hidden.
    expect(screen.getByTestId('ft8-strip-banner-wedged')).toBeVisible();
  });

  it('the same seeded collapse DOES apply for a non-force-expand state (decoding)', () => {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(true));
    renderStrip({ uiState: makeUiState('decoding') });
    const strip = screen.getByTestId('ft8-strip');
    expect(strip).toHaveAttribute('data-collapsed', 'true');
    expect(screen.queryByTestId('ft8-strip-body')).not.toBeInTheDocument();
  });

  it('operator can transiently re-collapse during a force-expand state without persisting it', () => {
    renderStrip({ uiState: makeUiState('needs-setup') });
    expect(screen.getByTestId('ft8-strip')).toHaveAttribute('data-collapsed', 'false');
    fireEvent.click(screen.getByTestId('ft8-strip-collapse'));
    expect(screen.getByTestId('ft8-strip')).toHaveAttribute('data-collapsed', 'true');
    // Not persisted: the storage key is untouched by a force-expand-episode toggle.
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// SWEEP PAUSED chip (fallback-hold).
// ---------------------------------------------------------------------------

describe('LiveBandStrip — SWEEP PAUSED chip', () => {
  it('renders when snapshot.sweep.mode is fallback-hold', () => {
    renderStrip({
      snapshot: makeSnapshot({ sweep: { mode: 'fallback-hold', bandIdx: 1, dwellProgress: 0.4 } }),
    });
    expect(screen.getByTestId('ft8-strip-chip-sweep-paused')).toHaveTextContent(
      'SWEEP PAUSED — radio not responding',
    );
  });

  it('omits the chip otherwise', () => {
    renderStrip({ snapshot: makeSnapshot({ sweep: { mode: 'inactive', bandIdx: null, dwellProgress: null } }) });
    expect(screen.queryByTestId('ft8-strip-chip-sweep-paused')).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// jt9Degraded chip renders snapshot.lastFailure.
// ---------------------------------------------------------------------------

describe('LiveBandStrip — jt9Degraded chip', () => {
  it('renders snapshot.lastFailure verbatim, not invented copy', () => {
    renderStrip({
      uiState: makeUiState('decoding', { jt9Degraded: true }),
      snapshot: makeSnapshot({ lastFailure: 'jt9 exited with signal 11 (segfault) 3x in 60s' }),
    });
    expect(screen.getByTestId('ft8-strip-chip-jt9-degraded')).toHaveTextContent(
      'jt9 exited with signal 11 (segfault) 3x in 60s',
    );
  });

  it('omits the chip when jt9Degraded is false', () => {
    renderStrip({ uiState: makeUiState('decoding', { jt9Degraded: false }) });
    expect(screen.queryByTestId('ft8-strip-chip-jt9-degraded')).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// clockUnsynced banner renders OVER the live body — body still present.
// ---------------------------------------------------------------------------

describe('LiveBandStrip — clockUnsynced flags-overlay', () => {
  it('renders the amber banner WHILE the live body (waterfall + feed) still renders', () => {
    renderStrip({
      uiState: makeUiState('decoding', { clockUnsynced: true }),
      decodesRing: [mkSlot(NOW, [mkDecode()])],
    });
    // Overlay present:
    const banner = screen.getByTestId('ft8-strip-banner-clock-unsynced');
    expect(banner.className).toContain('si-banner--warn');
    expect(banner).toHaveTextContent('System clock is not synchronized');
    // Body NOT replaced — both present simultaneously:
    expect(screen.getByTestId('ft8-strip-body')).toBeInTheDocument();
    expect(screen.getByTestId('ft8-waterfall-canvas')).toBeInTheDocument();
    expect(screen.getByTestId('decode-feed')).toBeInTheDocument();
  });

  it('omits the banner when clockUnsynced is false', () => {
    renderStrip({ uiState: makeUiState('decoding', { clockUnsynced: false }) });
    expect(screen.queryByTestId('ft8-strip-banner-clock-unsynced')).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Wedged restart banner + no live leaves mounted.
// ---------------------------------------------------------------------------

describe('LiveBandStrip — wedged state', () => {
  it('renders the restart banner and does not mount the live waterfall/feed', () => {
    renderStrip({ uiState: makeUiState('wedged') });
    expect(screen.getByTestId('ft8-strip-banner-wedged')).toHaveTextContent(
      'Audio capture is wedged — restart Tuxlink.',
    );
    expect(screen.queryByTestId('ft8-waterfall-canvas')).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Stats — sourced from B3's stripStats over decodesRing.
// ---------------------------------------------------------------------------

describe('LiveBandStrip — stats', () => {
  it('renders holding/dial/decodes-per-min/grids-heard from stripStats', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW - 15_000, [
        mkDecode({ slotUtcMs: NOW - 15_000, grid: 'EM12' }),
        mkDecode({ slotUtcMs: NOW - 15_000, grid: 'DM34' }),
      ]),
      mkSlot(NOW, [mkDecode({ slotUtcMs: NOW, grid: 'EM12' })]),
    ];
    const expected = stripStats(ring, '20m', NOW);
    renderStrip({ decodesRing: ring, snapshot: makeSnapshot({ band: '20m', dialHz: 14_074_000 }) });

    const stats = screen.getByTestId('ft8-strip-stats');
    expect(stats).toHaveTextContent('holding');
    expect(stats).toHaveTextContent('20m');
    expect(stats).toHaveTextContent('14.074');
    expect(stats).toHaveTextContent(`${Math.round(expected.decodesPerMin)}`);
    expect(stats).toHaveTextContent(`${expected.gridsHeard}`);
    expect(stats).toHaveTextContent('grids heard');
  });

  it('renders placeholder stats while snapshot is null (pre-hydrate)', () => {
    renderStrip({ snapshot: null, uiState: makeUiState('off') });
    const stats = screen.getByTestId('ft8-strip-stats');
    expect(stats).toHaveTextContent('—'); // dial placeholder
    expect(stats).toHaveTextContent('0'); // decodes/min + grids heard both 0
  });
});

// ---------------------------------------------------------------------------
// Leaf composition — Waterfall expanded gating, popover + blockingSessionMode.
// ---------------------------------------------------------------------------

describe('LiveBandStrip — leaf composition', () => {
  it('mounts Waterfall only for live-body states (decoding), not for off', () => {
    const { unmount } = renderStrip({ uiState: makeUiState('decoding') });
    expect(screen.getByTestId('ft8-waterfall-canvas')).toBeInTheDocument();
    unmount();

    renderStrip({ uiState: makeUiState('off'), snapshot: makeSnapshot({ service: { axis: 'stopped' } }) });
    expect(screen.queryByTestId('ft8-waterfall-canvas')).not.toBeInTheDocument();
  });

  it('opens BandSubsetPopover from the holding trigger and passes blockingSessionMode through', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'ft8_cat_probe') {
        return Promise.reject({ kind: 'modem-busy', detail: 'a modem session is active' });
      }
      return Promise.resolve();
    });
    renderStrip({ blockingSessionMode: 'VARA' });
    expect(screen.queryByTestId('band-subset-popover')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('ft8-strip-holding-trigger'));
    expect(screen.getByTestId('band-subset-popover')).toBeInTheDocument();
    const caption = await screen.findByTestId('band-subset-sweep-caption');
    expect(caption).toHaveTextContent('radio busy with VARA session — disconnect first');
  });

  it('closes the popover on Escape', () => {
    renderStrip();
    fireEvent.click(screen.getByTestId('ft8-strip-holding-trigger'));
    expect(screen.getByTestId('band-subset-popover')).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByTestId('band-subset-popover')).not.toBeInTheDocument();
  });

  it('renders the DecodeFeed leaf from decodesRing', () => {
    renderStrip({ decodesRing: [mkSlot(NOW, [mkDecode({ message: 'CQ TEST GRID' })])] });
    expect(screen.getByTestId('decode-feed')).toHaveTextContent('CQ TEST GRID');
  });
});

// ---------------------------------------------------------------------------
// Non-live-body states render distinct, sensible bodies.
// ---------------------------------------------------------------------------

describe('LiveBandStrip — non-live-body states', () => {
  it('off renders the Not-listening CTA', () => {
    renderStrip({ uiState: makeUiState('off'), snapshot: makeSnapshot({ service: { axis: 'stopped' } }) });
    expect(screen.getByTestId('ft8-strip-body-off')).toBeInTheDocument();
    expect(screen.getByTestId('ft8-strip-start-cta')).toHaveTextContent('Start listening on 20m →');
  });

  it('needs-setup renders the notice + an Open-setup re-entry to the full-body surface', () => {
    // QA round-3 finding 2: the full setup surface is the PANEL's body now
    // (firstrun-v2 mock) — the strip's arm is a one-line re-entry, never a
    // nested surface.
    const onOpen = vi.fn();
    renderStrip({
      uiState: makeUiState('needs-setup'),
      snapshot: makeSnapshot({ service: { axis: 'blocked', reason: 'needs-device-selection' } }),
      onOpenFullSetup: onOpen,
    });
    expect(screen.getByTestId('ft8-strip-body-needs-setup')).toHaveTextContent(/setup required/i);
    fireEvent.click(screen.getByTestId('ft8-strip-open-setup'));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it('device-lost renders the compact reconnecting message and link', () => {
    const onOpen = vi.fn();
    renderStrip({
      uiState: makeUiState('device-lost'),
      snapshot: makeSnapshot({ service: { axis: 'blocked', reason: 'device-absent' } }),
      onOpenFullSetup: onOpen,
    });
    expect(screen.getByTestId('ft8-strip-body-device-lost')).toHaveTextContent('reconnecting');
    fireEvent.click(screen.getByTestId('ft8-strip-device-lost-link'));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// Provenance chip (bandSource + catFixedBand).
// ---------------------------------------------------------------------------

describe('LiveBandStrip — provenance chip', () => {
  it('renders OPERATOR-ASSERTED when catFixedBand is true', () => {
    renderStrip({ uiState: makeUiState('decoding', { catFixedBand: true }) });
    expect(screen.getByTestId('ft8-strip-chip-band-provenance')).toHaveTextContent('OPERATOR-ASSERTED');
  });

  it('renders dashed UNCONFIRMED with the dial when bandSource is default-unconfirmed', () => {
    renderStrip({ snapshot: makeSnapshot({ bandSource: 'default-unconfirmed', dialHz: 14_074_000 }) });
    const chip = screen.getByTestId('ft8-strip-chip-band-provenance');
    expect(chip).toHaveTextContent('UNCONFIRMED');
    expect(chip).toHaveTextContent('14.074');
  });

  it('renders CAT CONFIRMED for a cat-confirmed band with catFixedBand false', () => {
    renderStrip({ snapshot: makeSnapshot({ bandSource: 'cat-confirmed' }) });
    expect(screen.getByTestId('ft8-strip-chip-band-provenance')).toHaveTextContent('CAT CONFIRMED');
  });
});

// ---------------------------------------------------------------------------
// Finding 4b: header "setup" affordance — the only way back to the setup
// surface once the strip has moved past needs-setup into a live body state
// (decoding / waiting-first-slot / band-dead / yielded).
// ---------------------------------------------------------------------------

describe('LiveBandStrip — header setup button', () => {
  it('renders in a live-body state and fires onOpenFullSetup on click', () => {
    const onOpen = vi.fn();
    renderStrip({ uiState: makeUiState('decoding'), onOpenFullSetup: onOpen });
    const btn = screen.getByTestId('ft8-strip-setup-btn');
    expect(btn).toHaveTextContent('setup');
    fireEvent.click(btn);
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it('renders in the off (non-live-body) state too', () => {
    renderStrip({ uiState: makeUiState('off'), snapshot: makeSnapshot({ service: { axis: 'stopped' } }) });
    expect(screen.getByTestId('ft8-strip-setup-btn')).toBeInTheDocument();
  });

  it('does NOT render while the strip is collapsed', () => {
    renderStrip({ uiState: makeUiState('decoding') });
    fireEvent.click(screen.getByTestId('ft8-strip-collapse')); // collapse it
    expect(screen.queryByTestId('ft8-strip-setup-btn')).toBeNull();
  });
});
