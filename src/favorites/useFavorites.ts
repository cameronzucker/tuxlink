// useFavorites — the Favorites data layer (TanStack Query). Task B3.
//
// Mirrors `src/contacts/useContacts.ts` conventions minus the H9 cross-window
// listener. Favorites are a single-window radio-dock surface — the backend emits
// no `favorites:changed` event (intentional YAGNI; see comment in
// `src-tauri/src/favorites/commands.rs`). Do NOT add a listen() effect.
//
// Contract:
//   - Two underlying queries:
//       ['favorites']                    → favorites_read() → whole StationsFile
//       ['favorites', 'recents', mode]   → favorites_recents({ mode }) → Favorite[]
//   - `favorites` = StationsFile.favorites filtered to mode === <mode> && starred.
//   - `recents` = favorites_recents result (server-sorted, non-starred, capped).
//   - Mutations await invoke, then invalidate ['favorites']. Because the recents
//     query key is prefix-matched (['favorites', 'recents', mode] starts with
//     ['favorites']), a single prefix-invalidation refetches BOTH queries.
//   - Mutation errors are NON-BLOCKING (.catch(() => {})): no error field in the
//     return type; errors surface in the backend session log only (Cross-cutting §1).

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { Favorite, FavoriteDial, RadioMode, StationsFile } from './types';

/// Query key for the whole stations file. A prefix-match invalidation of this
/// key also refetches the recents query (whose key starts with this prefix).
export const FAVORITES_QUERY_KEY = ['favorites'] as const;

export interface UseFavorites {
  favorites: Favorite[];
  recents: Favorite[];
  isLoading: boolean;
  upsert: (favorite: Favorite) => Promise<void>;
  remove: (id: string) => Promise<void>;
  star: (id: string, starred: boolean) => Promise<void>;
  recordAttempt: (
    dial: FavoriteDial,
    outcome: 'reached' | 'failed',
    ts_local: string,
  ) => Promise<void>;
}

export function useFavorites(mode: RadioMode): UseFavorites {
  const qc = useQueryClient();

  // Query 1: the whole StationsFile — derive starred, mode-filtered favorites.
  const readQuery = useQuery({
    queryKey: FAVORITES_QUERY_KEY,
    queryFn: () => invoke<StationsFile>('favorites_read'),
  });

  // Query 2: server-sorted non-starred recents for this mode.
  const recentsQuery = useQuery({
    queryKey: ['favorites', 'recents', mode],
    queryFn: () => invoke<Favorite[]>('favorites_recents', { mode }),
  });

  // A single prefix-invalidate that covers both ['favorites'] and
  // ['favorites', 'recents', mode] in one call (TanStack prefix-match semantics).
  const invalidate = () => qc.invalidateQueries({ queryKey: FAVORITES_QUERY_KEY });

  return {
    favorites:
      readQuery.data?.favorites.filter((f) => f.mode === mode && f.starred) ?? [],
    recents: recentsQuery.data ?? [],
    isLoading: readQuery.isLoading || recentsQuery.isLoading,

    upsert: async (favorite: Favorite) => {
      await invoke('favorite_upsert', { favorite }).catch(() => {});
      await invalidate();
    },
    remove: async (id: string) => {
      await invoke('favorite_delete', { id }).catch(() => {});
      await invalidate();
    },
    star: async (id: string, starred: boolean) => {
      await invoke('favorite_star', { id, starred }).catch(() => {});
      await invalidate();
    },
    recordAttempt: async (
      dial: FavoriteDial,
      outcome: 'reached' | 'failed',
      ts_local: string,
    ) => {
      await invoke('favorite_record_attempt', { dial, outcome, ts_local }).catch(() => {});
      await invalidate();
    },
  };
}
