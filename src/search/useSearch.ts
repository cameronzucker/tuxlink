import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { EMPTY_SPEC, type QuerySpec, type SavedSearch, type SearchResults } from './types';
import { deparseQuery, parseQuery } from './parseQuery';

const DEBOUNCE_MS = 150;

function specIsActive(spec: QuerySpec): boolean {
  return !!(spec.free_text && spec.free_text.trim()) || Object.keys(spec.filters).length > 0;
}

export function useSearch() {
  // `rawText` is what the user types — full Gmail-style operator string
  // (e.g. `from:KX5DD damage`). The spec is derived from it via parseQuery,
  // EXCEPT when an activeSaved is set (then the saved-search spec wins and
  // the input shows the saved-search NAME rather than rawText).
  const [rawText, setRawText] = useState('');
  const [debouncedSpec, setDebouncedSpec] = useState<QuerySpec>(EMPTY_SPEC);
  const [activeSaved, setActiveSaved] = useState<SavedSearch | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const spec = useMemo<QuerySpec>(
    () => (activeSaved ? activeSaved.spec : parseQuery(rawText)),
    [rawText, activeSaved],
  );

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setDebouncedSpec(spec), DEBOUNCE_MS);
    return () => { if (timer.current) clearTimeout(timer.current); };
  }, [spec]);

  const active = useMemo(() => specIsActive(debouncedSpec), [debouncedSpec]);

  const query = useQuery({
    queryKey: ['search', debouncedSpec],
    queryFn: async (): Promise<SearchResults> => {
      return await invoke<SearchResults>('tauri_search_run', { spec: debouncedSpec });
    },
    enabled: active,
    staleTime: 0,
  });

  const clear = useCallback(() => {
    setRawText('');
    setActiveSaved(null);
  }, []);

  const setActiveSavedSearch = useCallback((saved: SavedSearch | null) => {
    setActiveSaved(saved);
    setRawText(saved ? deparseQuery(saved.spec) : '');
  }, []);

  // Detach the saved-search label but keep the equivalent rawText so the
  // query stays visible. Codex adrev fix (find-messages P2): unsave must
  // not blank the search.
  const clearActiveSaved = useCallback(() => {
    if (activeSaved) setRawText(deparseQuery(activeSaved.spec));
    setActiveSaved(null);
  }, [activeSaved]);

  return {
    rawText,
    setRawText,
    spec,
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
