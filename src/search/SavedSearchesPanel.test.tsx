import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { SavedSearchesPanel } from './SavedSearchesPanel';
import { EMPTY_SPEC, type SavedSearch } from './types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

const STORM: SavedSearch = {
  id: '1', name: 'Storm Net 5/30', spec: EMPTY_SPEC,
  created_at: 0, last_used_at: null, order: 0,
};

function wrap() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe('SavedSearchesPanel', () => {
  beforeEach(() => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'tauri_search_list_saved') return Promise.resolve([STORM]);
      if (cmd === 'tauri_search_list_recent') return Promise.resolve([]);
      return Promise.resolve(null);
    });
  });

  it('lists existing saved searches by name', async () => {
    render(<SavedSearchesPanel onClose={() => {}} />, { wrapper: wrap() });
    await waitFor(() => expect(screen.getByText(/Storm Net 5\/30/)).toBeInTheDocument());
  });

  it('does NOT host a "new saved search" form (creation lives in SearchDropdown)', () => {
    render(<SavedSearchesPanel onClose={() => {}} />, { wrapper: wrap() });
    expect(screen.queryByTestId('new-saved-search')).not.toBeInTheDocument();
    expect(screen.queryByTestId('new-saved-name-input')).not.toBeInTheDocument();
  });

  it('rebuild button triggers tauri_search_rebuild_index', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'tauri_search_list_saved') return Promise.resolve([]);
      if (cmd === 'tauri_search_list_recent') return Promise.resolve([]);
      if (cmd === 'tauri_search_rebuild_index') return Promise.resolve({ messagesIndexed: 5, elapsedMs: 42 });
      return Promise.resolve(null);
    });
    render(<SavedSearchesPanel onClose={() => {}} />, { wrapper: wrap() });
    fireEvent.click(screen.getByTestId('rebuild-index-btn'));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('tauri_search_rebuild_index'));
  });
});
