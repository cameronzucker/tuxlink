import { createElement, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import type { Favorite, FavoriteDial, StationsFile } from './types';

// --- Mocks (MUST precede the module-under-test import) ---------------------
// invoke is the Tauri command bridge. Favorites intentionally has NO cross-window
// listener (no favorites:changed event is emitted by the backend — YAGNI single-window
// radio-dock surface). We therefore do NOT mock @tauri-apps/api/event here.

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { FAVORITES_QUERY_KEY, useFavorites } from './useFavorites';

const invokeMock = invoke as ReturnType<typeof vi.fn>;

// Sample data
const STARRED_HF: Favorite = {
  id: 'f1',
  mode: 'ardop-hf',
  gateway: 'W6XYZ',
  freq: '14105.0',
  band: '20m',
  grid: 'CN87',
  starred: true,
  created_at: '2026-06-07T12:00:00+00:00',
  updated_at: '2026-06-07T12:00:00+00:00',
};

const UNSTARRED_HF: Favorite = {
  id: 'f2',
  mode: 'ardop-hf',
  gateway: 'W7ABC',
  freq: '14107.0',
  starred: false,
  last_attempt_at: '2026-06-07T15:00:00+00:00',
  created_at: '2026-06-07T10:00:00+00:00',
  updated_at: '2026-06-07T15:00:00+00:00',
};

const STARRED_OTHER_MODE: Favorite = {
  id: 'f3',
  mode: 'packet',
  gateway: 'K9PKT',
  starred: true,
  created_at: '2026-06-07T12:00:00+00:00',
  updated_at: '2026-06-07T12:00:00+00:00',
};

const SAMPLE_FILE: StationsFile = {
  schema_version: 1,
  favorites: [STARRED_HF, UNSTARRED_HF, STARRED_OTHER_MODE],
  log: [],
};

const SAMPLE_RECENTS: Favorite[] = [UNSTARRED_HF];

const DIAL: FavoriteDial = {
  mode: 'ardop-hf',
  gateway: 'W6XYZ',
  freq: '14105.0',
  band: '20m',
  grid: 'CN87',
};

function wrapperWith(qc: QueryClient) {
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
}

function newQc() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

beforeEach(() => {
  invokeMock.mockReset();
  // Default: favorites_read returns the sample file; favorites_recents returns sample recents; mutations resolve.
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === 'favorites_read') return Promise.resolve(SAMPLE_FILE);
    if (cmd === 'favorites_recents') return Promise.resolve(SAMPLE_RECENTS);
    void args;
    return Promise.resolve(undefined);
  });
});

describe('useFavorites', () => {
  it('invokes both favorites_read and favorites_recents(mode) on mount (M9)', async () => {
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(newQc()) });

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(invokeMock).toHaveBeenCalledWith('favorites_read');
    expect(invokeMock).toHaveBeenCalledWith('favorites_recents', { mode: 'ardop-hf' });
  });

  it('exposes starred mode-filtered favorites from favorites_read', async () => {
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(newQc()) });

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    // Only STARRED_HF: starred AND ardop-hf
    expect(result.current.favorites).toEqual([STARRED_HF]);
  });

  it('excludes non-starred entries from favorites', async () => {
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(newQc()) });

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    // UNSTARRED_HF is ardop-hf but not starred — must be excluded from favorites
    expect(result.current.favorites).not.toContainEqual(expect.objectContaining({ id: 'f2' }));
  });

  it('excludes starred entries from other modes in favorites', async () => {
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(newQc()) });

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    // STARRED_OTHER_MODE is starred but mode=packet — must be excluded
    expect(result.current.favorites).not.toContainEqual(expect.objectContaining({ id: 'f3' }));
  });

  it('exposes recents from favorites_recents (server-sorted, non-starred)', async () => {
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(newQc()) });

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(result.current.recents).toEqual(SAMPLE_RECENTS);
  });

  it('defaults favorites and recents to [] before reads resolve / on null reads', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'favorites_read') return Promise.resolve(undefined);
      if (cmd === 'favorites_recents') return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(newQc()) });

    // Synchronously (first render, before any query resolves) both are [].
    expect(result.current.favorites).toEqual([]);
    expect(result.current.recents).toEqual([]);

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.favorites).toEqual([]);
    expect(result.current.recents).toEqual([]);
  });

  it('upsert invokes favorite_upsert with {favorite} then invalidates [favorites]', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(qc) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    invalidateSpy.mockClear();

    await result.current.upsert(STARRED_HF);

    expect(invokeMock).toHaveBeenCalledWith('favorite_upsert', { favorite: STARRED_HF });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: FAVORITES_QUERY_KEY });
  });

  it('remove invokes favorite_delete with {id} then invalidates [favorites]', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(qc) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    invalidateSpy.mockClear();

    await result.current.remove('f1');

    expect(invokeMock).toHaveBeenCalledWith('favorite_delete', { id: 'f1' });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: FAVORITES_QUERY_KEY });
  });

  it('star invokes favorite_star with {id, starred} then invalidates [favorites]', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(qc) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    invalidateSpy.mockClear();

    await result.current.star('f2', true);

    expect(invokeMock).toHaveBeenCalledWith('favorite_star', { id: 'f2', starred: true });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: FAVORITES_QUERY_KEY });
  });

  it('recordAttempt invokes favorite_record_attempt with {dial, outcome, tsLocal} (no unit_id) then invalidates', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(qc) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    invalidateSpy.mockClear();

    const ts_local = '2026-06-08T10:00:00-07:00';
    await result.current.recordAttempt(DIAL, 'reached', ts_local);

    // tsLocal (camelCase) is the correct Tauri wire key for Rust param `ts_local: String`.
    // Tauri auto-camelCases snake_case Rust args; snake_case keys silently fail to bind.
    expect(invokeMock).toHaveBeenCalledWith('favorite_record_attempt', {
      dial: DIAL,
      outcome: 'reached',
      tsLocal: ts_local,
    });
    // Confirm unit_id was NOT included in the invocation args
    const callArgs = invokeMock.mock.calls.find(
      ([cmd]) => cmd === 'favorite_record_attempt',
    );
    expect(callArgs).toBeDefined();
    const argObj = callArgs![1] as Record<string, unknown>;
    expect(argObj).not.toHaveProperty('unit_id');
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: FAVORITES_QUERY_KEY });
  });

  it('recordAttempt passes outcome "failed" correctly', async () => {
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(newQc()) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    const ts_local = '2026-06-08T22:00:00-07:00';
    await result.current.recordAttempt(DIAL, 'failed', ts_local);

    expect(invokeMock).toHaveBeenCalledWith('favorite_record_attempt', {
      dial: DIAL,
      outcome: 'failed',
      tsLocal: ts_local,
    });
  });

  it('a failing mutation invoke does NOT reject (non-blocking .catch)', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'favorites_read') return Promise.resolve(SAMPLE_FILE);
      if (cmd === 'favorites_recents') return Promise.resolve(SAMPLE_RECENTS);
      return Promise.reject(new Error('backend down'));
    });
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(newQc()) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    // All mutations must resolve (not reject) even when invoke throws.
    await expect(result.current.upsert(STARRED_HF)).resolves.toBeUndefined();
    await expect(result.current.remove('f1')).resolves.toBeUndefined();
    await expect(result.current.star('f1', false)).resolves.toBeUndefined();
    await expect(
      result.current.recordAttempt(DIAL, 'reached', '2026-06-08T10:00:00-07:00'),
    ).resolves.toBeUndefined();
  });

  it('prefix-invalidation of [favorites] refetches both queries (recents key is prefixed)', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useFavorites('ardop-hf'), { wrapper: wrapperWith(qc) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    invalidateSpy.mockClear();

    // A single mutation (upsert) issues exactly one invalidateQueries call with
    // the ['favorites'] root key; TanStack Query's prefix match propagates it to
    // the ['favorites', 'recents', mode] key as well.
    await result.current.upsert(STARRED_HF);

    expect(invalidateSpy).toHaveBeenCalledTimes(1);
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: FAVORITES_QUERY_KEY });
  });
});
