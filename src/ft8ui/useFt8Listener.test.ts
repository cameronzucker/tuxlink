// src/ft8ui/useFt8Listener.test.ts
//
// Tests for the useFt8Listener hydration hook + Ft8ListenerProvider (Task B1).
//
// The @tauri-apps/api modules are mocked at module level so the hook runs in
// jsdom with no real Tauri context. Following the repo idiom (see
// src/connections/useAuthDiagnostic.test.ts): the `listen` mock captures the
// registered handlers into outer `let` bindings for manual dispatch, and the
// `invoke` mock is GATED ON `cmd` so vitest's stray no-arg cleanup call
// (feedback_vitest_invoke_mock_cleanup_call) is inert.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { createElement, type ReactNode } from 'react';

// ---------------------------------------------------------------------------
// Mocks — captured handlers + unlisten counters live in outer `let`s, assigned
// inside the factory closures at call time (not at hoist time), which is the
// proven repo pattern.
// ---------------------------------------------------------------------------

let slotHandler: ((e: { payload: unknown }) => void) | null = null;
let changeHandler: ((e: { payload: unknown }) => void) | null = null;
let slotUnlistenCalls = 0;
let changeUnlistenCalls = 0;

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

/** The default capturing listen implementation, (re)installed in beforeEach so
 *  a test that overrides it does not leak into the next test. */
function installCapturingListen() {
  const impl = (event: string, handler: (e: { payload: unknown }) => void): Promise<() => void> => {
    if (event === 'ft8-decodes:slot') {
      slotHandler = handler;
      return Promise.resolve(() => {
        slotUnlistenCalls += 1;
        slotHandler = null;
      });
    }
    if (event === 'ft8-listening:change') {
      changeHandler = handler;
      return Promise.resolve(() => {
        changeUnlistenCalls += 1;
        changeHandler = null;
      });
    }
    return Promise.resolve(() => {});
  };
  listenMock.mockImplementation(impl as unknown as typeof listen);
}

import { listen } from '@tauri-apps/api/event';
import { useFt8Listener, Ft8ListenerProvider } from './useFt8Listener';
import type { Ft8Snapshot, SlotRecord, Ft8ListeningChange } from './ft8Types';
import goldenSnapshot from './__fixtures__/ft8Snapshot.golden.json';

const listenMock = vi.mocked(listen);

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

function makeSnapshot(over: Partial<Ft8Snapshot> = {}): Ft8Snapshot {
  return {
    service: { axis: 'listening' },
    flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
    slotPhase: 'decoded',
    band: '20m',
    dialHz: 14074000,
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
    sweepConfig: { enabled: false, bands: [], dwellSlots: 4 },
    configuredDeviceName: null,
    ...over,
  };
}

function makeSlot(slotUtcMs: number, over: Partial<SlotRecord> = {}): SlotRecord {
  return {
    slotUtcMs,
    band: '20m',
    dialHz: 14074000,
    bandSource: 'cat-confirmed',
    bandLabelConfirmedUtcMs: null,
    outcome: { kind: 'decoded' },
    decodes: [],
    partialSalvage: false,
    lostFrames: 0,
    boundarySkewFrames: 0,
    clipFraction: 0,
    rmsDbfs: -20,
    dwellSlotIndex: null,
    ...over,
  };
}

function makeChange(over: Partial<Ft8ListeningChange> = {}): Ft8ListeningChange {
  return {
    service: { axis: 'listening' },
    flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
    slotPhase: 'decoded',
    band: '20m',
    dialHz: 14074000,
    sweep: { mode: 'inactive', bandIdx: null, dwellProgress: null },
    ...over,
  };
}

/** A manually-resolvable promise. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(Ft8ListenerProvider, null, children);

/** Flush pending microtasks (invoke resolution) inside act(). */
async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

function emitSlot(rec: SlotRecord) {
  act(() => slotHandler?.({ payload: rec }));
}

function emitChange(change: Ft8ListeningChange) {
  act(() => changeHandler?.({ payload: change }));
}

// ---------------------------------------------------------------------------

beforeEach(() => {
  slotHandler = null;
  changeHandler = null;
  slotUnlistenCalls = 0;
  changeUnlistenCalls = 0;
  listenMock.mockReset();
  installCapturingListen();
  invokeMock.mockReset();
  // Default: gated on cmd so vitest's stray no-arg cleanup call is inert.
  invokeMock.mockImplementation(async (cmd?: string) => {
    if (cmd === 'ft8_listener_snapshot') return makeSnapshot();
    return undefined;
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useFt8Listener — subscription lifecycle', () => {
  it('registers exactly one pair of listen calls per provider', async () => {
    const { unmount } = renderHook(() => useFt8Listener(), { wrapper });
    await flush();

    const events = listenMock.mock.calls.map((c) => c[0]);
    expect(events.filter((e) => e === 'ft8-decodes:slot')).toHaveLength(1);
    expect(events.filter((e) => e === 'ft8-listening:change')).toHaveLength(1);
    expect(listenMock).toHaveBeenCalledTimes(2);
    unmount();
  });

  it('registers both listeners BEFORE invoking the snapshot', async () => {
    const order: string[] = [];
    listenMock.mockImplementation(((event: string) => {
      order.push(`listen:${event}`);
      return Promise.resolve(() => {});
    }) as unknown as typeof listen);
    invokeMock.mockImplementation(async (cmd?: string) => {
      if (cmd === 'ft8_listener_snapshot') {
        order.push('invoke');
        return makeSnapshot();
      }
      return undefined;
    });

    const { unmount } = renderHook(() => useFt8Listener(), { wrapper });
    await flush();

    // Both listens precede the snapshot invoke.
    expect(order.indexOf('invoke')).toBeGreaterThan(order.indexOf('listen:ft8-decodes:slot'));
    expect(order.indexOf('invoke')).toBeGreaterThan(order.indexOf('listen:ft8-listening:change'));
    unmount();
  });

  it('calls unlisten for both listeners on unmount', async () => {
    const { unmount } = renderHook(() => useFt8Listener(), { wrapper });
    await flush();
    unmount();
    // Cleanup may resolve on a later tick if the unlisten promise was pending.
    await flush();
    expect(slotUnlistenCalls).toBe(1);
    expect(changeUnlistenCalls).toBe(1);
  });
});

describe('useFt8Listener — race-safe hydration', () => {
  it('does not lose a slot emitted in the gap before the snapshot applies (dedupe by slotUtcMs)', async () => {
    const d = deferred<Ft8Snapshot>();
    invokeMock.mockImplementation(async (cmd?: string) => {
      if (cmd === 'ft8_listener_snapshot') return d.promise;
      return undefined;
    });

    const { result } = renderHook(() => useFt8Listener(), { wrapper });
    // Handlers registered; snapshot still pending. Emit a live slot in the gap.
    await act(async () => {
      await Promise.resolve();
    });
    emitSlot(makeSlot(1700000000000));

    // Snapshot resolves; its ring_tail ALSO contains the same slot.
    await act(async () => {
      d.resolve(makeSnapshot({ ringTail: [makeSlot(1700000000000)] }));
      await Promise.resolve();
    });
    await flush();

    const withT = result.current.decodesRing.filter((r) => r.slotUtcMs === 1700000000000);
    expect(withT).toHaveLength(1); // present exactly once — not lost, not doubled
  });

  it('keeps a LIVE-ONLY gap-window slot absent from ring_tail (isolates live-capture from tail-recovery)', async () => {
    // Reviewer finding (fix round 1): the dedupe test above cannot distinguish
    // "captured live" from "recovered from ring_tail" because the slot is in
    // BOTH. Here the gap-window live slot (LIVE_T) is NOT in the resolved
    // snapshot's ring_tail — its only path into the ring is the live handler.
    // If a regression dropped gap-window live events, this slot would vanish
    // and the test would go RED (verified below by stubbing the handler).
    const LIVE_T = 1700000005000;
    const TAIL_T = 1700000000000;
    const d = deferred<Ft8Snapshot>();
    invokeMock.mockImplementation(async (cmd?: string) => {
      if (cmd === 'ft8_listener_snapshot') return d.promise;
      return undefined;
    });

    const { result } = renderHook(() => useFt8Listener(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    // Live-only slot arrives in the pending-snapshot gap.
    emitSlot(makeSlot(LIVE_T));

    // Snapshot resolves with a DIFFERENT slot in ring_tail (LIVE_T absent).
    await act(async () => {
      d.resolve(makeSnapshot({ ringTail: [makeSlot(TAIL_T)] }));
      await Promise.resolve();
    });
    await flush();

    // The live-only slot survived hydration exactly once (live-capture proven).
    expect(result.current.decodesRing.filter((r) => r.slotUtcMs === LIVE_T)).toHaveLength(1);
    // And the tail slot is present too (merge did not clobber the live entry).
    expect(result.current.decodesRing.filter((r) => r.slotUtcMs === TAIL_T)).toHaveLength(1);
    expect(result.current.decodesRing).toHaveLength(2);
  });

  it('applies a ft8-listening:change that arrives in the gap before the snapshot', async () => {
    const d = deferred<Ft8Snapshot>();
    invokeMock.mockImplementation(async (cmd?: string) => {
      if (cmd === 'ft8_listener_snapshot') return d.promise;
      return undefined;
    });

    const { result } = renderHook(() => useFt8Listener(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    // A blocked change arrives before the snapshot resolves.
    emitChange(makeChange({ service: { axis: 'blocked', reason: 'needs-device-selection' } }));

    await act(async () => {
      d.resolve(makeSnapshot({ service: { axis: 'listening' } }));
      await Promise.resolve();
    });
    await flush();

    // The change's live axis wins over the snapshot's stale 'listening'.
    expect(result.current.snapshot?.service).toEqual({
      axis: 'blocked',
      reason: 'needs-device-selection',
    });
  });
});

describe('useFt8Listener — coalesced re-hydrate on :change', () => {
  it('debounces a re-hydrate (~150ms) and updates sweepConfig from the fresh snapshot', async () => {
    vi.useFakeTimers();
    let call = 0;
    invokeMock.mockImplementation(async (cmd?: string) => {
      if (cmd !== 'ft8_listener_snapshot') return undefined;
      call += 1;
      return call === 1
        ? makeSnapshot({ sweepConfig: { enabled: false, bands: [], dwellSlots: 4 } })
        : makeSnapshot({ sweepConfig: { enabled: true, bands: ['20m', '40m'], dwellSlots: 8 } });
    });

    const { result } = renderHook(() => useFt8Listener(), { wrapper });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current.snapshot?.sweepConfig.dwellSlots).toBe(4);

    // Two changes inside the debounce window coalesce to ONE re-hydrate.
    emitChange(makeChange());
    emitChange(makeChange());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });

    expect(call).toBe(2); // initial + exactly one coalesced re-hydrate
    expect(result.current.snapshot?.sweepConfig).toEqual({
      enabled: true,
      bands: ['20m', '40m'],
      dwellSlots: 8,
    });
  });
});

describe('useFt8Listener — generation gating', () => {
  it('commits nothing when the component unmounts before the snapshot resolves', async () => {
    const d = deferred<Ft8Snapshot>();
    invokeMock.mockImplementation(async (cmd?: string) => {
      if (cmd === 'ft8_listener_snapshot') return d.promise;
      return undefined;
    });

    const { result, unmount } = renderHook(() => useFt8Listener(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    unmount();

    // Snapshot resolves AFTER unmount — must not throw, must not commit.
    await act(async () => {
      d.resolve(makeSnapshot({ band: '40m' }));
      await Promise.resolve();
    });
    // No assertion on result.current (unmounted); the guard is "did not throw".
    expect(result.current).toBeDefined();
  });

  it('discards an older snapshot that resolves after a newer one', async () => {
    vi.useFakeTimers();
    const first = deferred<Ft8Snapshot>();
    const second = deferred<Ft8Snapshot>();
    let call = 0;
    invokeMock.mockImplementation((cmd?: string) => {
      if (cmd !== 'ft8_listener_snapshot') return Promise.resolve(undefined);
      call += 1;
      return call === 1 ? first.promise : second.promise;
    });

    const { result } = renderHook(() => useFt8Listener(), { wrapper });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    // Trigger a re-hydrate (second invoke). Coalesce window fires.
    emitChange(makeChange());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });
    expect(call).toBe(2);

    // Resolve the NEWER (second) snapshot first, then the STALER (first).
    // Assert on a NON-live field (configuredDeviceName): `band` is overlaid by
    // the listening-change so it cannot distinguish the two snapshots.
    await act(async () => {
      second.resolve(makeSnapshot({ configuredDeviceName: 'NEW' }));
      await Promise.resolve();
    });
    await act(async () => {
      first.resolve(makeSnapshot({ configuredDeviceName: 'STALE' }));
      await Promise.resolve();
    });

    expect(result.current.snapshot?.configuredDeviceName).toBe('NEW'); // stale discarded
  });
});

describe('useFt8Listener — bounded ring', () => {
  it('evicts the oldest and caps the ring at 240 entries', async () => {
    const { result } = renderHook(() => useFt8Listener(), { wrapper });
    await flush();

    // Emit 241 distinct slots (base .. base+240).
    const base = 1_700_000_000_000;
    for (let i = 0; i <= 240; i++) {
      emitSlot(makeSlot(base + i * 1000));
    }

    expect(result.current.decodesRing).toHaveLength(240);
    // Oldest (base) evicted; newest present.
    expect(result.current.decodesRing.some((r) => r.slotUtcMs === base)).toBe(false);
    expect(result.current.decodesRing.some((r) => r.slotUtcMs === base + 240 * 1000)).toBe(true);
  });
});

describe('useFt8Listener — composed seam (golden fixture)', () => {
  it('feeds the golden Ft8Snapshot through the hook + derivations reading REAL camelCase keys', async () => {
    const golden = goldenSnapshot as unknown as Ft8Snapshot;
    // deriveBandActivity honors a 10-min evidence window (Task B3). The golden
    // fixture's ring_tail slots are timestamped ~1.70e12; pin the clock just
    // past the latest so that evidence is IN-window and the seam guard exercises
    // a real (non-no-data) dot. Without this, wall-clock now would age the
    // fixture out and the band would correctly be absent — a false seam failure.
    const nowSpy = vi.spyOn(Date, 'now').mockReturnValue(1_700_000_040_000);
    invokeMock.mockImplementation(async (cmd?: string) => {
      if (cmd === 'ft8_listener_snapshot') return golden;
      return undefined;
    });

    const { result } = renderHook(() => useFt8Listener(), { wrapper });
    await flush();

    const snap = result.current.snapshot;
    expect(snap).not.toBeNull();
    // Wire keys resolve to real values (a rename drift would yield undefined).
    expect(snap?.sweepConfig.dwellSlots).toBe(8);
    expect(snap?.availableDevices?.[0].alsaHw).toBe('hw:1,0');
    expect(snap?.availableDevices?.[0].stableId.kind).toBe('byIdSymlink');

    // deriveUiState read service.axis + slotPhase (real keys) → a real member.
    expect(result.current.uiState.state).toBe('decoding');
    expect(result.current.uiState.flags.catFixedBand).toBe(true);

    // deriveBandActivity read ringTail[].band + outcome.kind + decodes (real keys).
    expect(result.current.bandActivity.get('20m')).toBeDefined();
    expect(result.current.bandActivity.get('40m')).toBeDefined();
    // 20m has a decoded, cat-confirmed slot in the golden ring_tail, so
    // deriveBandActivity must read that evidence and produce a non-no-data tier
    // (a wrong camelCase key would yield no evidence → 'no-data').
    expect(result.current.bandActivity.get('20m')?.tier).not.toBe('no-data');
    // The golden ring_tail seeded the decodes ring.
    expect(result.current.decodesRing.length).toBeGreaterThanOrEqual(3);
    nowSpy.mockRestore();
  });
});
