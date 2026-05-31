import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { QuerySpec, RecentSearch, SavedSearch } from './types';

const SAVED_KEY = ['search', 'saved'];
const RECENT_KEY = ['search', 'recent'];

export function useSavedSearches() {
  const qc = useQueryClient();
  const saved = useQuery({ queryKey: SAVED_KEY, queryFn: () => invoke<SavedSearch[]>('tauri_search_list_saved') });
  const recent = useQuery({ queryKey: RECENT_KEY, queryFn: () => invoke<RecentSearch[]>('tauri_search_list_recent') });

  const refetchAll = () => Promise.all([qc.invalidateQueries({ queryKey: SAVED_KEY }), qc.invalidateQueries({ queryKey: RECENT_KEY })]);

  return {
    saved: saved.data ?? [],
    recent: recent.data ?? [],
    isLoading: saved.isLoading || recent.isLoading,

    save: async (name: string, spec: QuerySpec): Promise<SavedSearch> => {
      const result = await invoke<SavedSearch>('tauri_search_save', { name, spec });
      await refetchAll();
      return result;
    },
    unsave: async (id: string) => {
      await invoke('tauri_search_unsave', { id });
      await refetchAll();
    },
    rename: async (id: string, name: string) => {
      await invoke('tauri_search_rename', { id, name });
      await refetchAll();
    },
    reorder: async (orderedIds: string[]) => {
      await invoke('tauri_search_reorder', { orderedIds });
      await refetchAll();
    },
    rebuildIndex: async (): Promise<{ messagesIndexed: number; elapsedMs: number }> => {
      const stats = await invoke<{ messagesIndexed: number; elapsedMs: number }>('tauri_search_rebuild_index');
      await refetchAll();
      return stats;
    },
  };
}
