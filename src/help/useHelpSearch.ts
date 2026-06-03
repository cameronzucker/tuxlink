import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';

/**
 * Frontend type for a single docs-search hit, mirroring the Rust-side
 * `DocsHit` struct in src-tauri/src/search/docs_index.rs.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §9.2.
 */
export interface DocsHit {
  slug: string;
  title: string;
  snippet: string;  // may contain <mark>...</mark> spans from FTS5 snippet()
}

/**
 * Issues a `docs_search` query against the FTS5-backed user-guide index.
 * Empty / whitespace queries skip the invoke entirely so the operator's
 * empty-input sidebar state shows the grouped topic list, not "no matches".
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §9.4.
 */
export function useHelpSearch(query: string) {
  const trimmed = query.trim();
  return useQuery({
    queryKey: ['help', 'search', trimmed],
    queryFn: () => invoke<DocsHit[]>('docs_search', { query: trimmed }),
    enabled: trimmed.length > 0,
    staleTime: 60_000,
  });
}
