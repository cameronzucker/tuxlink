import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { usePersistedBucketFilter } from './usePersistedBucketFilter';
import { ALL_BUCKET_KEYS } from '../aprs/stationBuckets';

const KEY = 'tuxlink:test:bucket-filter';

beforeEach(() => window.localStorage.clear());

describe('usePersistedBucketFilter', () => {
  it('defaults to all buckets enabled and collapsed', () => {
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    expect(result.current.collapsed).toBe(true);
    expect([...result.current.enabled].sort()).toEqual([...ALL_BUCKET_KEYS].sort());
  });

  it('toggleBucket removes then re-adds a bucket and persists', () => {
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    act(() => result.current.toggleBucket('weather'));
    expect(result.current.enabled.has('weather')).toBe(false);
    const stored = JSON.parse(window.localStorage.getItem(KEY)!);
    expect(stored.enabled).not.toContain('weather');
    act(() => result.current.toggleBucket('weather'));
    expect(result.current.enabled.has('weather')).toBe(true);
  });

  it('setAll(false) clears all, setAll(true) restores all', () => {
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    act(() => result.current.setAll(false));
    expect(result.current.enabled.size).toBe(0);
    act(() => result.current.setAll(true));
    expect(result.current.enabled.size).toBe(ALL_BUCKET_KEYS.length);
  });

  it('toggleCollapsed flips and persists', () => {
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    act(() => result.current.toggleCollapsed());
    expect(result.current.collapsed).toBe(false);
    expect(JSON.parse(window.localStorage.getItem(KEY)!).collapsed).toBe(false);
  });

  it('restores a saved subset on remount', () => {
    window.localStorage.setItem(KEY, JSON.stringify({ enabled: ['weather', 'igate'], collapsed: false }));
    const { result } = renderHook(() => usePersistedBucketFilter(KEY));
    expect([...result.current.enabled].sort()).toEqual(['igate', 'weather']);
    expect(result.current.collapsed).toBe(false);
  });

  it('drops unknown stored keys; corrupt JSON falls back to all-on', () => {
    window.localStorage.setItem(KEY, JSON.stringify({ enabled: ['weather', 'bogus'], collapsed: true }));
    const { result: r1 } = renderHook(() => usePersistedBucketFilter(KEY));
    expect([...r1.current.enabled]).toEqual(['weather']);

    window.localStorage.setItem(KEY, '{not json');
    const { result: r2 } = renderHook(() => usePersistedBucketFilter(`${KEY}:2`)); // fresh key unaffected; assert corrupt path
    window.localStorage.setItem(`${KEY}:3`, '{not json');
    const { result: r3 } = renderHook(() => usePersistedBucketFilter(`${KEY}:3`));
    expect(r3.current.enabled.size).toBe(ALL_BUCKET_KEYS.length);
    void r2;
  });
});
