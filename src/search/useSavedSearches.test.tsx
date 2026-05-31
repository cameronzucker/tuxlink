import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { useSavedSearches } from './useSavedSearches';
import { EMPTY_SPEC, type SavedSearch, type RecentSearch } from './types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

function wrap() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe('useSavedSearches', () => {
  beforeEach(() => (invoke as unknown as ReturnType<typeof vi.fn>).mockReset());

  it('lists saved + recent on mount', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'tauri_search_list_saved') return Promise.resolve([{ id: '1', name: 'Storm', spec: EMPTY_SPEC, created_at: 0, last_used_at: null, order: 0 } satisfies SavedSearch]);
      if (cmd === 'tauri_search_list_recent') return Promise.resolve([{ spec: EMPTY_SPEC, ran_at: 100 } satisfies RecentSearch]);
      return Promise.resolve(null);
    });
    const { result } = renderHook(() => useSavedSearches(), { wrapper: wrap() });
    await waitFor(() => expect(result.current.saved).toHaveLength(1));
    expect(result.current.saved[0].name).toBe('Storm');
    expect(result.current.recent).toHaveLength(1);
  });

  it('save invokes tauri_search_save', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValue([] satisfies SavedSearch[]);
    const { result } = renderHook(() => useSavedSearches(), { wrapper: wrap() });
    await act(async () => {
      await result.current.save('My pick', EMPTY_SPEC);
    });
    expect(invoke).toHaveBeenCalledWith('tauri_search_save', { name: 'My pick', spec: EMPTY_SPEC });
  });
});
