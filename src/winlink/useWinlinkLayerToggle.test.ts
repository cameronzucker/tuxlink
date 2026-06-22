// useWinlinkLayerToggle — vitest/jsdom tests (tuxlink-s1o1 Task 8)
//
// Asserts the persistence contract:
//   - Default (absent/corrupt storage): on=false, withinHours=6
//   - toggle() flips on and persists (re-read returns on:true)
//   - setWithinHours() persists withinHours
//
// Uses jsdom localStorage (provided by the vitest setup).

import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useWinlinkLayerToggle } from './useWinlinkLayerToggle';

const STORAGE_KEY = 'tuxlink:winlink-layer';

beforeEach(() => {
  window.localStorage.clear();
});

describe('useWinlinkLayerToggle', () => {
  it('defaults to off with withinHours=6 when storage is empty', () => {
    const { result } = renderHook(() => useWinlinkLayerToggle());
    expect(result.current.on).toBe(false);
    expect(result.current.withinHours).toBe(6);
  });

  it('toggle() flips on from false to true and persists to localStorage', () => {
    const { result } = renderHook(() => useWinlinkLayerToggle());
    act(() => result.current.toggle());
    expect(result.current.on).toBe(true);

    // Verify localStorage was updated so a fresh mount reads on:true.
    const stored = JSON.parse(window.localStorage.getItem(STORAGE_KEY)!);
    expect(stored.on).toBe(true);
  });

  it('toggle() persists and a fresh hook mount reads the persisted value', () => {
    const { result: r1 } = renderHook(() => useWinlinkLayerToggle());
    act(() => r1.current.toggle());
    expect(r1.current.on).toBe(true);

    // New instance reads persisted on:true.
    const { result: r2 } = renderHook(() => useWinlinkLayerToggle());
    expect(r2.current.on).toBe(true);
    expect(r2.current.withinHours).toBe(6);
  });

  it('setWithinHours() persists the new withinHours value', () => {
    const { result } = renderHook(() => useWinlinkLayerToggle());
    act(() => result.current.setWithinHours(12));
    expect(result.current.withinHours).toBe(12);

    const stored = JSON.parse(window.localStorage.getItem(STORAGE_KEY)!);
    expect(stored.withinHours).toBe(12);
  });

  it('setWithinHours does not change on; toggle does not change withinHours', () => {
    const { result } = renderHook(() => useWinlinkLayerToggle());
    act(() => result.current.setWithinHours(24));
    expect(result.current.on).toBe(false); // unchanged

    act(() => result.current.toggle());
    expect(result.current.withinHours).toBe(24); // unchanged
  });

  it('falls back to defaults when stored value is corrupt JSON', () => {
    window.localStorage.setItem(STORAGE_KEY, 'NOT_JSON');
    const { result } = renderHook(() => useWinlinkLayerToggle());
    expect(result.current.on).toBe(false);
    expect(result.current.withinHours).toBe(6);
  });

  it('falls back to defaults when stored value is missing required fields', () => {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify({ something: 'else' }));
    const { result } = renderHook(() => useWinlinkLayerToggle());
    expect(result.current.on).toBe(false);
    expect(result.current.withinHours).toBe(6);
  });
});
