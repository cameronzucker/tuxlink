import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useHelpSearch } from './useHelpSearch';

function makeWrapper(client: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client }, children);
  };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe('useHelpSearch', () => {
  it('does not invoke on empty query', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    renderHook(() => useHelpSearch(''), { wrapper: makeWrapper(client) });
    await new Promise((r) => setTimeout(r, 50));
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('does not invoke on whitespace-only query', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    renderHook(() => useHelpSearch('   '), { wrapper: makeWrapper(client) });
    await new Promise((r) => setTimeout(r, 50));
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('invokes docs_search with the trimmed query when non-empty', async () => {
    invokeMock.mockResolvedValue([]);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    renderHook(() => useHelpSearch('  ardop  '), { wrapper: makeWrapper(client) });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('docs_search', { query: 'ardop' }));
  });

  it('returns the hit array from the backend', async () => {
    invokeMock.mockResolvedValue([
      { slug: '02-connections', title: 'Connections', snippet: '<mark>ARDOP</mark> HF digital' },
    ]);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { result } = renderHook(() => useHelpSearch('ardop'), { wrapper: makeWrapper(client) });
    await waitFor(() => expect(result.current.data?.length).toBe(1));
    expect(result.current.data?.[0]).toMatchObject({
      slug: '02-connections',
      title: 'Connections',
    });
  });
});
