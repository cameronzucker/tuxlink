/**
 * Tests for useDownloadProgress (tuxlink-9n9t) — the hook that turns the
 * `basemap:download-progress` / `basemap:download-done` event stream for one
 * active pack into { bytes, total, percent, rateBps, etaSecs, status }.
 *
 * `@tauri-apps/api/event` is mocked so the test drives payloads synchronously,
 * mirroring useAprsPositions.test.ts. `performance.now()` is stubbed so rate/eta
 * are deterministic across event arrivals.
 */
import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const handlers: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => {
      delete handlers[name];
    });
  },
}));

import { useDownloadProgress } from './useDownloadProgress';
import { DOWNLOAD_PROGRESS_EVENT, DOWNLOAD_DONE_EVENT } from './offlineMaps';

function emitProgress(packId: string, bytes: number, total: number) {
  act(() => {
    handlers[DOWNLOAD_PROGRESS_EVENT]?.({ payload: { packId, bytes, total } });
  });
}
function emitDone(packId: string, ok: boolean, error: string | null = null) {
  act(() => {
    handlers[DOWNLOAD_DONE_EVENT]?.({ payload: { packId, ok, error } });
  });
}

let nowMs = 0;
beforeEach(() => {
  for (const k of Object.keys(handlers)) delete handlers[k];
  nowMs = 0;
  vi.spyOn(performance, 'now').mockImplementation(() => nowMs);
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('useDownloadProgress', () => {
  it('starts idle with no active pack', () => {
    const { result } = renderHook(() => useDownloadProgress(null));
    expect(result.current.status).toBe('idle');
    expect(result.current.bytes).toBe(0);
    expect(result.current.percent).toBe(0);
  });

  it('tracks bytes/total/percent for the active pack', async () => {
    const { result } = renderHook(() => useDownloadProgress('tier-wide'));
    await act(async () => {});
    emitProgress('tier-wide', 500_000_000, 1_000_000_000);
    expect(result.current.status).toBe('downloading');
    expect(result.current.bytes).toBe(500_000_000);
    expect(result.current.total).toBe(1_000_000_000);
    expect(result.current.percent).toBeCloseTo(0.5, 3);
  });

  it('latches onto the first pack and ignores a concurrent emitter', async () => {
    const { result } = renderHook(() => useDownloadProgress('active'));
    await act(async () => {});
    // First event wins; a second pack emitting concurrently is ignored.
    emitProgress('tier-wide', 500, 1000);
    emitProgress('continent-na', 999, 1000);
    expect(result.current.status).toBe('downloading');
    expect(result.current.bytes).toBe(500);
  });

  it('stays idle while inactive (no subscription)', () => {
    const { result } = renderHook(() => useDownloadProgress(null));
    emitProgress('tier-wide', 999, 1000);
    expect(result.current.status).toBe('idle');
    expect(result.current.bytes).toBe(0);
  });

  it('computes a positive rate and finite eta from successive samples', async () => {
    const { result } = renderHook(() => useDownloadProgress('tier-wide'));
    await act(async () => {});
    nowMs = 0;
    emitProgress('tier-wide', 0, 1_000_000_000);
    nowMs = 1000; // +1s
    emitProgress('tier-wide', 10_000_000, 1_000_000_000); // 10 MB in 1s = 10 MB/s
    expect(result.current.rateBps).toBeGreaterThan(0);
    // remaining 990 MB / 10 MB/s ≈ 99s
    expect(result.current.etaSecs).toBeGreaterThan(0);
    expect(Number.isFinite(result.current.etaSecs as number)).toBe(true);
  });

  it('clamps percent below 1 until done, then snaps to 1 on done-ok', async () => {
    const { result } = renderHook(() => useDownloadProgress('tier-wide'));
    await act(async () => {});
    emitProgress('tier-wide', 1_000_000_000, 1_000_000_000); // 100% bytes
    expect(result.current.percent).toBeLessThan(1);
    emitDone('tier-wide', true);
    expect(result.current.status).toBe('done');
    expect(result.current.percent).toBe(1);
  });

  it('clamps the denominator up when bytes exceed the estimate (C4)', async () => {
    const { result } = renderHook(() => useDownloadProgress('tier-wide'));
    await act(async () => {});
    // Estimate is 1.0 GB but the real extract is already 1.4 GB — the bar must
    // not report >100% or "1.4 GB / 1.0 GB".
    emitProgress('tier-wide', 1_400_000_000, 1_000_000_000);
    expect(result.current.percent).toBeLessThanOrEqual(1);
    expect(result.current.total).toBeGreaterThanOrEqual(result.current.bytes);
    // Past the estimate → indeterminate "finishing" rather than a stuck 99%.
    expect(result.current.finishing).toBe(true);
  });

  it('flags finishing within epsilon of the estimate (C4)', async () => {
    const { result } = renderHook(() => useDownloadProgress('tier-wide'));
    await act(async () => {});
    // 99.9% of the estimate — inside the finishing epsilon.
    emitProgress('tier-wide', 999_000_000, 1_000_000_000);
    expect(result.current.finishing).toBe(true);
    expect(result.current.percent).toBeLessThanOrEqual(1);
  });

  it('is not finishing early in the download (C4)', async () => {
    const { result } = renderHook(() => useDownloadProgress('tier-wide'));
    await act(async () => {});
    emitProgress('tier-wide', 100_000_000, 1_000_000_000); // 10%
    expect(result.current.finishing).toBe(false);
    expect(result.current.total).toBe(1_000_000_000);
    expect(result.current.percent).toBeCloseTo(0.1, 3);
  });

  it('surfaces an error on done with ok=false', async () => {
    const { result } = renderHook(() => useDownloadProgress('tier-wide'));
    await act(async () => {});
    emitProgress('tier-wide', 100, 1000);
    emitDone('tier-wide', false, 'go-pmtiles exit 1: boom');
    expect(result.current.status).toBe('error');
    expect(result.current.error).toBe('go-pmtiles exit 1: boom');
  });

  it('reports cancelled status from a cancel done payload', async () => {
    const { result } = renderHook(() => useDownloadProgress('tier-wide'));
    await act(async () => {});
    emitProgress('tier-wide', 100, 1000);
    emitDone('tier-wide', false, 'download cancelled');
    // A cancelled done is still a terminal "not downloading" state; the row
    // returns to idle in the UI. The hook exposes it as 'cancelled'.
    expect(result.current.status).toBe('cancelled');
  });

  it('resets when the active packId changes', async () => {
    const { result, rerender } = renderHook(({ id }) => useDownloadProgress(id), {
      initialProps: { id: 'tier-wide' as string | null },
    });
    await act(async () => {});
    emitProgress('tier-wide', 500, 1000);
    expect(result.current.bytes).toBe(500);
    rerender({ id: 'continent-na' });
    expect(result.current.status).toBe('idle');
    expect(result.current.bytes).toBe(0);
  });
});
