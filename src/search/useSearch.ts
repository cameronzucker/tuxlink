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
  // (e.g. `from:KX5DD damage`). The spec is always derived from it via
  // parseQuery. When a saved search is loaded, its deparseQuery'd string
  // is what populates rawText — typing edits it in place. `activeSaved`
  // is just a UI label (★ Name) showing the user this query started life
  // as a saved one; the moment rawText diverges from the saved spec, the
  // label auto-detaches.
  const [rawText, setRawTextInner] = useState('');
  const [debouncedSpec, setDebouncedSpec] = useState<QuerySpec>(EMPTY_SPEC);
  const [activeSaved, setActiveSaved] = useState<SavedSearch | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const spec = useMemo<QuerySpec>(() => parseQuery(rawText), [rawText]);

  // Wrap setRawText so user typing automatically detaches a saved-search
  // label when the text no longer matches the saved spec. Saved-search
  // loading bypasses this (it sets rawText to the canonical deparse, so
  // the comparison succeeds and the label is preserved).
  const setRawText = useCallback((next: string) => {
    setRawTextInner(next);
    setActiveSaved((prev) => {
      if (!prev) return null;
      const canon = deparseQuery(prev.spec);
      return next === canon ? prev : null;
    });
  }, []);

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
    setRawTextInner('');
    setActiveSaved(null);
  }, []);

  // Load a saved search: set rawText to the deparsed canonical form AND
  // mark activeSaved. Use the inner setter so we don't trip the
  // auto-detach in setRawText.
  const setActiveSavedSearch = useCallback((saved: SavedSearch | null) => {
    setActiveSaved(saved);
    setRawTextInner(saved ? deparseQuery(saved.spec) : '');
  }, []);

  // Detach the saved-search label without clearing the input. Used after
  // unsave (★ click) so the typed query remains active even though the
  // saved-search backing was deleted from storage.
  const clearActiveSaved = useCallback(() => {
    setActiveSaved(null);
  }, []);

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
