// Tests for usePersistedViewport (tuxlink-dwzu): remember + restore the map
// viewport (center+zoom) per surface in localStorage, so a map opens where the
// operator left it instead of the default world view + a flyTo to the operator.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { usePersistedViewport } from './usePersistedViewport';

const KEY = 'tuxlink:map-viewport:test';

describe('usePersistedViewport', () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.useRealTimers();
  });

  it('returns saved=null when nothing is stored', () => {
    const { result } = renderHook(() => usePersistedViewport(KEY));
    expect(result.current.saved).toBeNull();
  });

  it('reads a valid stored viewport on mount', () => {
    window.localStorage.setItem(
      KEY,
      JSON.stringify({ center: { lat: 47.6, lon: -122.3 }, zoom: 9 }),
    );
    const { result } = renderHook(() => usePersistedViewport(KEY));
    expect(result.current.saved).toEqual({ center: { lat: 47.6, lon: -122.3 }, zoom: 9 });
  });

  it('returns saved=null for corrupt JSON', () => {
    window.localStorage.setItem(KEY, '{not valid json');
    const { result } = renderHook(() => usePersistedViewport(KEY));
    expect(result.current.saved).toBeNull();
  });

  it('returns saved=null for out-of-range / non-finite values (cross-version guard)', () => {
    for (const bad of [
      { center: { lat: 200, lon: 0 }, zoom: 9 }, // lat out of range
      { center: { lat: 0, lon: 999 }, zoom: 9 }, // lon out of range
      { center: { lat: 0, lon: 0 }, zoom: -3 }, // zoom out of range
      { center: { lat: 0 }, zoom: 9 }, // missing lon
      { zoom: 9 }, // missing center
    ]) {
      window.localStorage.setItem(KEY, JSON.stringify(bad));
      const { result } = renderHook(() => usePersistedViewport(KEY));
      expect(result.current.saved).toBeNull();
    }
  });

  it('persists the viewport (debounced) on onViewportChange', () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => usePersistedViewport(KEY));
    act(() => {
      result.current.onViewportChange({ lat: 40, lon: -100 }, 7);
    });
    // Not written yet (debounced).
    expect(window.localStorage.getItem(KEY)).toBeNull();
    act(() => {
      vi.advanceTimersByTime(400);
    });
    expect(JSON.parse(window.localStorage.getItem(KEY)!)).toEqual({
      center: { lat: 40, lon: -100 },
      zoom: 7,
    });
  });

  it('coalesces rapid changes to the last value (debounce)', () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => usePersistedViewport(KEY));
    act(() => {
      result.current.onViewportChange({ lat: 1, lon: 1 }, 3);
      result.current.onViewportChange({ lat: 2, lon: 2 }, 4);
      vi.advanceTimersByTime(400);
    });
    expect(JSON.parse(window.localStorage.getItem(KEY)!)).toEqual({
      center: { lat: 2, lon: 2 },
      zoom: 4,
    });
  });

  it('ignores a non-finite viewport (never writes garbage)', () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => usePersistedViewport(KEY));
    act(() => {
      result.current.onViewportChange({ lat: Number.NaN, lon: 0 }, 5);
      vi.advanceTimersByTime(400);
    });
    expect(window.localStorage.getItem(KEY)).toBeNull();
  });

  it('isolates distinct keys', () => {
    vi.useFakeTimers();
    const a = renderHook(() => usePersistedViewport('tuxlink:map-viewport:a'));
    const b = renderHook(() => usePersistedViewport('tuxlink:map-viewport:b'));
    act(() => {
      a.result.current.onViewportChange({ lat: 10, lon: 10 }, 5);
      vi.advanceTimersByTime(400);
    });
    expect(window.localStorage.getItem('tuxlink:map-viewport:a')).not.toBeNull();
    expect(window.localStorage.getItem('tuxlink:map-viewport:b')).toBeNull();
    expect(b.result.current.saved).toBeNull();
  });
});
