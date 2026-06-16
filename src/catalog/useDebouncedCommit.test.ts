import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useDebouncedCommit } from './useDebouncedCommit';

describe('useDebouncedCommit', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('coalesces a burst of rapid calls into a single trailing commit with the latest value', () => {
    const commit = vi.fn();
    const { result } = renderHook(() => useDebouncedCommit<number>(commit, 300));

    // Simulate a slider drag firing many onChange events in quick succession.
    act(() => {
      for (let v = 1; v <= 10; v++) result.current(v);
    });
    // Nothing fires until the burst settles.
    expect(commit).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(300));
    // Exactly ONE commit, with the final value — not ten N-station re-sweeps.
    expect(commit).toHaveBeenCalledTimes(1);
    expect(commit).toHaveBeenCalledWith(10);
  });

  it('commits again for a change that arrives after the debounce window settled', () => {
    const commit = vi.fn();
    const { result } = renderHook(() => useDebouncedCommit<number>(commit, 300));

    act(() => result.current(1));
    act(() => vi.advanceTimersByTime(300));
    act(() => result.current(2));
    act(() => vi.advanceTimersByTime(300));

    expect(commit).toHaveBeenCalledTimes(2);
    expect(commit).toHaveBeenNthCalledWith(1, 1);
    expect(commit).toHaveBeenNthCalledWith(2, 2);
  });

  it('flushes a pending value on unmount so a final drag is not silently dropped', () => {
    const commit = vi.fn();
    const flush = vi.fn();
    const { result, unmount } = renderHook(() => useDebouncedCommit<number>(commit, 300, flush));

    act(() => result.current(42)); // pending, timer not yet fired
    expect(commit).not.toHaveBeenCalled();
    expect(flush).not.toHaveBeenCalled();

    unmount();
    // The pending value is persisted via the setState-free flush path; the
    // trailing commit (which would touch unmounted component state) never runs.
    expect(flush).toHaveBeenCalledTimes(1);
    expect(flush).toHaveBeenCalledWith(42);
    expect(commit).not.toHaveBeenCalled();
  });

  it('does not flush on unmount when there is no pending value', () => {
    const commit = vi.fn();
    const flush = vi.fn();
    const { result, unmount } = renderHook(() => useDebouncedCommit<number>(commit, 300, flush));

    act(() => result.current(7));
    act(() => vi.advanceTimersByTime(300)); // commit fires, nothing left pending
    expect(commit).toHaveBeenCalledTimes(1);

    unmount();
    expect(flush).not.toHaveBeenCalled();
  });

  it('always commits the latest closure, even if commit identity changes between calls', () => {
    const first = vi.fn();
    const second = vi.fn();
    const { result, rerender } = renderHook(({ cb }) => useDebouncedCommit<number>(cb, 300), {
      initialProps: { cb: first },
    });

    act(() => result.current(1));
    rerender({ cb: second });
    act(() => vi.advanceTimersByTime(300));

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith(1);
  });
});
