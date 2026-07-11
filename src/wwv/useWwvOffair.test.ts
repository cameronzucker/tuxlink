// src/wwv/useWwvOffair.test.ts
//
// Tests for useWwvOffair's Task 16 window-scheduling extension: arm()
// schedules a setTimeout via window.ts's (real, unmocked — it's pure)
// nextCapture() instead of calling the backend immediately; a no_copy
// outcome auto-retries once by re-arming for the next window; cancel()
// suppresses the scheduled fire; the pending timer is cleared on unmount.
// wwvApi is mocked at the module level so no real Tauri invoke() happens.
// Fake timers (which also fake Date under vitest's defaults) stand in for
// the wall clock — no Date.now() appears in any assertion.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { WwvRefreshOutcome } from './wwvApi';

const refreshOffairMock = vi.fn<(nowMs: number) => Promise<WwvRefreshOutcome>>();
const readSnapshotMock = vi.fn();
const catConfiguredMock = vi.fn(() => Promise.resolve(true));
const manualIngestMock = vi.fn<
  (sfi: number, aIndex: number | null, kIndex: number | null, nowMs: number) => Promise<WwvRefreshOutcome>
>();

vi.mock('./wwvApi', () => ({
  refreshOffair: (nowMs: number) => refreshOffairMock(nowMs),
  readSnapshot: () => readSnapshotMock(),
  catConfigured: () => catConfiguredMock(),
  manualIngest: (sfi: number, aIndex: number | null, kIndex: number | null, nowMs: number) =>
    manualIngestMock(sfi, aIndex, kIndex, nowMs),
}));

import { useWwvOffair } from './useWwvOffair';

// Exact hour boundary (same fixture as window.test.ts).
const HOUR_BOUNDARY_MS = 1_783_512_000_000;
const WWV_START_S = 18 * 60 - 5; // 1075 s
const WWVH_START_S = 45 * 60 - 5; // 2695 s
const HOUR_S = 3600;
// Mirrors window.ts's CAPTURE_SPAN_S / the backend's `arecord -d 70` dwell.
const CAPTURE_DWELL_MS = 70_000;

function outcome(no_copy: boolean): WwvRefreshOutcome {
  return no_copy
    ? { updated: false, indices: null, source: 'rf-wwv-voice', no_copy: true, wav_path: null }
    : { updated: true, indices: { sfi: 150 }, source: 'rf-wwv-voice', no_copy: false, wav_path: null };
}

// Resolves refreshOffair only after a simulated ~70s capture dwell, matching
// the backend's real capture duration. This matters for the retry tests:
// with an instantly-resolving mock, Date.now() never advances past the
// window before the no_copy retry computes its NEXT schedule, so the retry
// would (unrealistically) land back inside the same still-active window.
function mockRefreshOffairWithDwell(no_copy: boolean) {
  refreshOffairMock.mockImplementation(
    () => new Promise<WwvRefreshOutcome>((resolve) => setTimeout(() => resolve(outcome(no_copy)), CAPTURE_DWELL_MS)),
  );
}

// Advances fake timers AND flushes the resulting React state updates +
// pending microtasks (fireCapture's `await refreshOffair` / `await
// refreshSnapshot` chain). Bare `vi.advanceTimersByTimeAsync` outside `act`
// fires the timer and resolves the awaited promises, but React's own
// scheduler flush lands on a microtask/task that isn't drained until the
// NEXT `act`-wrapped turn — so `result.current` can read one state update
// stale (armed instead of done) without this wrapper.
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(HOUR_BOUNDARY_MS);
  refreshOffairMock.mockReset();
  readSnapshotMock.mockReset();
  readSnapshotMock.mockResolvedValue(null);
  catConfiguredMock.mockReset();
  catConfiguredMock.mockResolvedValue(true);
  manualIngestMock.mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useWwvOffair', () => {
  it('arm() sets status armed with a windowLabel, then fires the capture at the window and reaches done', async () => {
    refreshOffairMock.mockResolvedValue(outcome(false));
    const { result } = renderHook(() => useWwvOffair());

    act(() => {
      result.current.arm(HOUR_BOUNDARY_MS);
    });

    expect(result.current.status).toBe('armed');
    expect(result.current.windowLabel).toBe('WWV :18');
    expect(refreshOffairMock).not.toHaveBeenCalled();

    await advance(WWV_START_S * 1000);

    expect(refreshOffairMock).toHaveBeenCalledOnce();
    expect(result.current.status).toBe('done');
  });

  it('fires immediately (delayMs 0) when arm() is called inside an active window span', async () => {
    refreshOffairMock.mockResolvedValue(outcome(false));
    // 5 s into the WWV window (1075..1145) — already inside the span.
    const insideWindowMs = HOUR_BOUNDARY_MS + (WWV_START_S + 5) * 1000;
    vi.setSystemTime(insideWindowMs);
    const { result } = renderHook(() => useWwvOffair());

    act(() => {
      result.current.arm(insideWindowMs);
    });
    expect(result.current.status).toBe('armed');

    await advance(0);

    expect(refreshOffairMock).toHaveBeenCalledOnce();
    expect(result.current.status).toBe('done');
  });

  it('auto-retries once on no_copy (re-arms for the next window), then settles to nocopy on a second no_copy', async () => {
    mockRefreshOffairWithDwell(true);
    const { result } = renderHook(() => useWwvOffair());

    act(() => {
      result.current.arm(HOUR_BOUNDARY_MS);
    });
    expect(result.current.windowLabel).toBe('WWV :18');

    // First fire (WWV :18 window, plus the simulated capture dwell) —
    // no_copy triggers the auto re-arm for WWVH :45.
    await advance(WWV_START_S * 1000 + CAPTURE_DWELL_MS);

    expect(refreshOffairMock).toHaveBeenCalledOnce();
    expect(result.current.status).toBe('armed');
    expect(result.current.windowLabel).toBe('WWVH :45');

    // Second fire (WWVH :45 window, plus dwell) — retry budget already
    // spent, so a second no_copy settles to 'nocopy' rather than arming a
    // third time. Delay is relative to where the first dwell left the
    // clock (WWV_START_S + 70s into the hour).
    const delayToWwvhS = WWVH_START_S - (WWV_START_S + 70);
    await advance(delayToWwvhS * 1000 + CAPTURE_DWELL_MS);

    expect(refreshOffairMock).toHaveBeenCalledTimes(2);
    expect(result.current.status).toBe('nocopy');

    // No third arm: advancing well past another window boundary must not
    // produce a third refreshOffair call.
    await advance(3600 * 1000);
    expect(refreshOffairMock).toHaveBeenCalledTimes(2);
  });

  it('a fresh user-initiated arm() resets the retry budget after a prior retried-and-settled cycle', async () => {
    mockRefreshOffairWithDwell(true);
    const { result } = renderHook(() => useWwvOffair());

    act(() => {
      result.current.arm(HOUR_BOUNDARY_MS);
    });
    await advance(WWV_START_S * 1000 + CAPTURE_DWELL_MS); // fire 1 (no_copy) -> auto re-arm to WWVH
    const delayToWwvhS = WWVH_START_S - (WWV_START_S + 70);
    await advance(delayToWwvhS * 1000 + CAPTURE_DWELL_MS); // fire 2 (no_copy) -> settles nocopy
    expect(result.current.status).toBe('nocopy');
    expect(refreshOffairMock).toHaveBeenCalledTimes(2);

    // Fresh user arm — retry budget resets, so a third fire that returns
    // no_copy triggers a genuine auto-retry (armed) again, not an immediate
    // second-strike settle. Date.now() is now WWVH_START_S + 70s into the
    // hour (past both spans this hour), so the fresh arm rolls into next
    // hour's WWV :18 window.
    act(() => {
      result.current.arm(Date.now());
    });
    expect(result.current.status).toBe('armed');

    const delayToNextWwvS = HOUR_S + WWV_START_S - (WWVH_START_S + 70);
    await advance(delayToNextWwvS * 1000 + CAPTURE_DWELL_MS);

    expect(refreshOffairMock).toHaveBeenCalledTimes(3);
    expect(result.current.status).toBe('armed'); // auto-retried again
  });

  it('cancel() after arm() returns to idle and suppresses the scheduled capture', async () => {
    refreshOffairMock.mockResolvedValue(outcome(false));
    const { result } = renderHook(() => useWwvOffair());

    act(() => {
      result.current.arm(HOUR_BOUNDARY_MS);
    });
    expect(result.current.status).toBe('armed');

    act(() => {
      result.current.cancel();
    });
    expect(result.current.status).toBe('idle');
    expect(result.current.windowLabel).toBeNull();

    await advance(WWV_START_S * 1000);

    expect(refreshOffairMock).not.toHaveBeenCalled();
  });

  it('clears the pending timer on unmount (no fire-after-unmount)', async () => {
    refreshOffairMock.mockResolvedValue(outcome(false));
    const { result, unmount } = renderHook(() => useWwvOffair());

    act(() => {
      result.current.arm(HOUR_BOUNDARY_MS);
    });
    expect(result.current.status).toBe('armed');

    unmount();

    await advance(WWV_START_S * 1000);

    expect(refreshOffairMock).not.toHaveBeenCalled();
  });

  it('does not touch state (or fire the follow-on snapshot read) when the in-flight fireCapture chain resolves after unmount', async () => {
    // A deferred refreshOffair: the timer fires and fireCapture starts (so
    // it's genuinely in-flight, past the timer-clearing cleanup entirely)
    // before we unmount and only THEN resolve it — this is the case the
    // timeoutRef cleanup can't catch, only the mountedRef guard can.
    let resolveOffair: (o: WwvRefreshOutcome) => void = () => {};
    refreshOffairMock.mockImplementation(
      () =>
        new Promise<WwvRefreshOutcome>((resolve) => {
          resolveOffair = resolve;
        }),
    );
    readSnapshotMock.mockResolvedValue(null);
    const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    const { result, unmount } = renderHook(() => useWwvOffair());

    act(() => {
      result.current.arm(HOUR_BOUNDARY_MS);
    });
    expect(result.current.status).toBe('armed');

    // Fire the timer: fireCapture runs synchronously up to and including
    // `await refreshOffair(...)`, which parks on our still-pending promise.
    await advance(WWV_START_S * 1000);
    expect(refreshOffairMock).toHaveBeenCalledOnce();
    expect(result.current.status).toBe('capturing');

    unmount();

    // Resolve the outcome AFTER unmount and flush the resulting microtask
    // chain. Without the mountedRef guard, fireCapture would call
    // setResult/setStatus and then `await refreshSnapshot()` (which itself
    // calls readSnapshot()/setSnapshot) on the unmounted hook.
    await act(async () => {
      resolveOffair(outcome(false));
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    // The guarded tail of fireCapture bails before `await refreshSnapshot()`
    // once unmounted, so the follow-on IPC read never fires.
    expect(readSnapshotMock).not.toHaveBeenCalled();
    // No React "state update on unmounted component" dev warning.
    expect(consoleErrorSpy).not.toHaveBeenCalled();

    consoleErrorSpy.mockRestore();
  });

  it('captures wavPath from a no-copy outcome, then clears it on a fresh user arm', async () => {
    refreshOffairMock.mockResolvedValue({
      updated: false,
      indices: null,
      source: 'rf-wwv-voice',
      no_copy: true,
      wav_path: '/tmp/clip.wav',
    });
    const { result } = renderHook(() => useWwvOffair());

    act(() => {
      result.current.arm(HOUR_BOUNDARY_MS);
    });
    // First fire is the auto-retry (no_copy re-arms once); the wavPath from
    // that fire should already be visible even though status is 'armed'
    // again, not settled.
    await advance(WWV_START_S * 1000);
    expect(result.current.wavPath).toBe('/tmp/clip.wav');

    act(() => {
      result.current.arm(Date.now());
    });
    expect(result.current.wavPath).toBeNull();
  });

  it('refreshCat() populates catConfigured, swallowing a rejected invoke like refreshSnapshot', async () => {
    catConfiguredMock.mockResolvedValue(false);
    const { result } = renderHook(() => useWwvOffair());

    expect(result.current.catConfigured).toBeNull();
    await act(async () => {
      await result.current.refreshCat();
    });
    expect(result.current.catConfigured).toBe(false);
  });

  it('manualIngest() sets status done and refreshes the snapshot on success', async () => {
    manualIngestMock.mockResolvedValue({
      updated: true,
      indices: { sfi: 133 },
      source: 'rf-wwv-manual',
      no_copy: false,
      wav_path: null,
    });
    readSnapshotMock.mockResolvedValue({
      indices: { sfi: 133 },
      updated_at_ms: HOUR_BOUNDARY_MS,
      source: 'rf-wwv-manual',
      forecast_updated: true,
    });
    const { result } = renderHook(() => useWwvOffair());

    await act(async () => {
      await result.current.manualIngest(133, null, null);
    });

    expect(manualIngestMock).toHaveBeenCalledWith(133, null, null, expect.any(Number));
    expect(result.current.status).toBe('done');
    expect(readSnapshotMock).toHaveBeenCalled();
  });

  it('manualIngest() sets status error when the backend call throws', async () => {
    manualIngestMock.mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useWwvOffair());

    await act(async () => {
      await result.current.manualIngest(133, null, null);
    });

    expect(result.current.status).toBe('error');
  });
});
