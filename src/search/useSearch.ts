import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { EMPTY_SPEC, type QuerySpec, type SavedSearch, type SearchResults } from './types';

const DEBOUNCE_MS = 150;

function specIsActive(spec: QuerySpec): boolean {
  return !!(spec.free_text && spec.free_text.trim()) || Object.keys(spec.filters).length > 0;
}

export function useSearch() {
  const [spec, setSpec] = useState<QuerySpec>(EMPTY_SPEC);
  const [debounced, setDebounced] = useState<QuerySpec>(EMPTY_SPEC);
  const [activeSaved, setActiveSaved] = useState<SavedSearch | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setDebounced(spec), DEBOUNCE_MS);
    return () => { if (timer.current) clearTimeout(timer.current); };
  }, [spec]);

  const active = useMemo(() => specIsActive(debounced), [debounced]);

  const query = useQuery({
    queryKey: ['search', debounced],
    queryFn: async (): Promise<SearchResults> => {
      return await invoke<SearchResults>('tauri_search_run', { spec: debounced });
    },
    enabled: active,
    staleTime: 0,
  });

  const clear = useCallback(() => {
    setSpec(EMPTY_SPEC);
    setActiveSaved(null);
  }, []);

  const setActiveSavedSearch = useCallback((saved: SavedSearch | null) => {
    setActiveSaved(saved);
    setSpec(saved ? saved.spec : EMPTY_SPEC);
  }, []);

  // Detach the saved-search label without clearing the query spec. Use this
  // when the user un-stars an active saved search — the search result should
  // remain visible; only the "you are in a saved search" marker is removed
  // (Codex adrev fix — find-messages P2: unsave must not blank the search).
  const clearActiveSaved = useCallback(() => {
    setActiveSaved(null);
  }, []);

  return {
    spec,
    setSpec,
    activeSaved,
    setActiveSavedSearch,
    clearActiveSaved,
    clear,
    results: active ? (query.data ?? null) : null,
    isLoading: query.isLoading,
    error: query.error as Error | null,
    isActive: active,
  };
}
