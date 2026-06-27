import { describe, it, expect, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { usePersistedState } from './usePersistedState';

const isBool = (v: unknown): v is boolean => typeof v === 'boolean';
const isStr = (v: unknown): v is string => typeof v === 'string';

describe('usePersistedState', () => {
  beforeEach(() => window.localStorage.clear());

  it('returns the initial value when nothing is stored', () => {
    const { result } = renderHook(() => usePersistedState('t:a', false, isBool));
    expect(result.current[0]).toBe(false);
  });

  it('persists under the tuxlink: prefix and restores on remount', () => {
    const first = renderHook(() => usePersistedState('show-raw', false, isBool));
    act(() => first.result.current[1](true));
    expect(JSON.parse(window.localStorage.getItem('tuxlink:show-raw')!)).toBe(true);

    // A fresh mount (panel close/reopen) restores the last value, not the default.
    const second = renderHook(() => usePersistedState('show-raw', false, isBool));
    expect(second.result.current[0]).toBe(true);
  });

  it('falls back to initial on corrupt or wrong-type stored data', () => {
    window.localStorage.setItem('tuxlink:b', 'not json{');
    const corrupt = renderHook(() => usePersistedState('b', 'fav', isStr));
    expect(corrupt.result.current[0]).toBe('fav');

    window.localStorage.setItem('tuxlink:c', JSON.stringify(42));
    const wrongType = renderHook(() => usePersistedState('c', 'fav', isStr));
    expect(wrongType.result.current[0]).toBe('fav');
  });

  it('round-trips a string (last-used tab)', () => {
    const h = renderHook(() => usePersistedState('connect-tab:ardop-hf', 'favorites', isStr));
    act(() => h.result.current[1]('manual'));
    expect(
      renderHook(() => usePersistedState('connect-tab:ardop-hf', 'favorites', isStr)).result
        .current[0],
    ).toBe('manual');
  });
});
