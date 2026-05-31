import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { useSearch } from './useSearch';
import { EMPTY_SPEC, type SearchResults } from './types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

function wrap() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe('useSearch', () => {
  beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
  afterEach(() => vi.useRealTimers());

  it('returns null results when spec is empty', () => {
    const { result } = renderHook(() => useSearch(), { wrapper: wrap() });
    expect(result.current.results).toBeNull();
  });

  it('calls invoke after debounce when the spec is non-empty', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [], totalMatches: 0, queryMs: 1, effectiveSpec: EMPTY_SPEC,
    } satisfies SearchResults);
    const { result } = renderHook(() => useSearch(), { wrapper: wrap() });
    act(() => result.current.setSpec({ ...EMPTY_SPEC, free_text: 'damage' }));
    expect(invoke).not.toHaveBeenCalled();           // not yet — debounce window
    await act(async () => { vi.advanceTimersByTime(200); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('tauri_search_run', expect.anything()));
  });

  it('exposes setActiveSavedSearch — name surfaces back in `activeSaved`', () => {
    const { result } = renderHook(() => useSearch(), { wrapper: wrap() });
    act(() => result.current.setActiveSavedSearch({ id: '1', name: 'Storm', spec: EMPTY_SPEC, created_at: 0, last_used_at: null, order: 0 }));
    expect(result.current.activeSaved?.name).toBe('Storm');
  });
});
