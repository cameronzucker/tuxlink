// src/radio/useSampleHistory.test.ts
//
// Spec §5.3 — rolling sample-history buffer used by the ARDOP panel's
// S/N + throughput sparklines. The hook pushes the latest value on a
// fixed interval, so the sparkline sees one sample per second by default.

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useSampleHistory } from './useSampleHistory';

describe('useSampleHistory', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns a buffer of the requested length, initialized to 0', () => {
    const { result } = renderHook(() => useSampleHistory(null, 5, 1000));
    expect(result.current).toHaveLength(5);
    expect(result.current).toEqual([0, 0, 0, 0, 0]);
  });

  it('pushes the current value on each tick, dropping the oldest sample', () => {
    let current: number | null = 1;
    const { result, rerender } = renderHook(
      ({ v }: { v: number | null }) => useSampleHistory(v, 3, 1000),
      { initialProps: { v: current } },
    );
    expect(result.current).toEqual([0, 0, 0]);

    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(result.current).toEqual([0, 0, 1]);

    current = 2;
    rerender({ v: current });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(result.current).toEqual([0, 1, 2]);

    current = 3;
    rerender({ v: current });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(result.current).toEqual([1, 2, 3]);
  });

  it('pushes 0 when current is null (treats null as no-reading-yet)', () => {
    const { result } = renderHook(() => useSampleHistory(null, 3, 1000));
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(result.current).toEqual([0, 0, 0]);
  });

  it('respects a custom interval', () => {
    const { result } = renderHook(() => useSampleHistory(7, 2, 500));
    expect(result.current).toEqual([0, 0]);
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(result.current).toEqual([0, 7]);
  });
});
