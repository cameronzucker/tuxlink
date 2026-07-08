import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useThinkingPulse } from './useThinkingPulse';
import { RADIO_VERBS } from './radioVerbs';

describe('useThinkingPulse', () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it('is inert while inactive (elapsed stays 0)', () => {
    const { result } = renderHook(() => useThinkingPulse(false));
    act(() => { vi.advanceTimersByTime(5000); });
    expect(result.current.elapsedSecs).toBe(0);
  });

  it('starts from a RADIO_VERBS phrase and ticks elapsed once per second', () => {
    const { result } = renderHook(() => useThinkingPulse(true));
    expect(RADIO_VERBS).toContain(result.current.verb);
    act(() => { vi.advanceTimersByTime(3000); });
    expect(result.current.elapsedSecs).toBe(3);
  });

  it('rotates to a different RADIO_VERBS phrase after ~3s', () => {
    const { result } = renderHook(() => useThinkingPulse(true));
    const before = result.current.verb;
    act(() => { vi.advanceTimersByTime(3000); });
    const after = result.current.verb;
    expect(RADIO_VERBS).toContain(after);
    expect(after).not.toBe(before);
  });

  it('resets elapsed to 0 when re-activated', () => {
    const { result, rerender } = renderHook(({ active }) => useThinkingPulse(active), {
      initialProps: { active: true },
    });
    act(() => { vi.advanceTimersByTime(4000); });
    expect(result.current.elapsedSecs).toBe(4);
    rerender({ active: false });
    rerender({ active: true });
    expect(result.current.elapsedSecs).toBe(0);
  });
});
