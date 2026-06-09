// User-folder registry hook (tuxlink-f62f — Phase 2).
//
// Spec: docs/superpowers/specs/2026-06-02-user-folders-design.md §4.2.
//
// Wraps the Rust `user_folders_list` command in a TanStack Query. The result
// is the operator's current user folders sorted oldest-first (matches
// `Mailbox::list_user_folders`'s sort), so first-created sticks to the top
// of the Folders section in the sidebar.
//
// Mutations (folder_create / folder_delete) live in dedicated mutation hooks
// (`useCreateUserFolder` / `useDeleteUserFolder`) so React Query's cache
// invalidation is centralized — every successful create/delete invalidates
// `['userFolders']` so the sidebar repaints from the source of truth.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { UserFolder } from './types';

export const USER_FOLDERS_QUERY_KEY = ['userFolders'] as const;

/// Read the current user-folder list. Backend returns Rust's `UserFolderDto`
/// (camelCase via serde) which is structurally equivalent to the TS
/// `UserFolder` shape.
async function fetchUserFolders(): Promise<UserFolder[]> {
  return invoke<UserFolder[]>('user_folders_list');
}

/// Read user folders with a TanStack Query. `data ?? []` is the safe default
/// for sidebar rendering when the query is still loading or when the backend
/// is offline (`NotConfigured` → empty list rather than error).
export function useUserFolders(): {
  folders: UserFolder[];
  isLoading: boolean;
  isError: boolean;
  error: unknown;
} {
  const q = useQuery({
    queryKey: USER_FOLDERS_QUERY_KEY,
    queryFn: fetchUserFolders,
    // Don't aggressively refetch — folder lists change only when the operator
    // explicitly creates/renames/deletes one. Stale-while-revalidate keeps the
    // UI snappy; invalidation after mutations propagates real changes.
    staleTime: 60_000,
  });
  return {
    folders: q.data ?? [],
    isLoading: q.isLoading,
    isError: q.isError,
    error: q.error,
  };
}

/// Mutation: create a user folder by display name. Slug is derived backend-side
/// (`ARES Drills` → `ares-drills`). Reserved names + duplicate slugs are
/// rejected with `UiError::Rejected` carrying the human-readable reason.
/// Invalidates `['userFolders']` on success so the sidebar picks up the new
/// row.
export function useCreateUserFolder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ displayName, parentSlug }: { displayName: string; parentSlug?: string }) =>
      invoke<UserFolder>('folder_create', { displayName, parentSlug }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: USER_FOLDERS_QUERY_KEY });
    },
  });
}

/// Mutation: re-parent a user folder (spec D3). `parentSlug` undefined/absent
/// promotes the folder to top level. Metadata-only on the backend — no message
/// files move. Invalidates `['userFolders']` so the sidebar tree re-renders.
export function useMoveUserFolder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ slug, parentSlug }: { slug: string; parentSlug?: string }) =>
      invoke<UserFolder>('folder_move', { slug, parentSlug }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: USER_FOLDERS_QUERY_KEY });
    },
  });
}

/// Mutation: rename a user folder. Display name only; slug stays stable
/// (spec §3.1) so on-disk messages don't churn. Invalidates `['userFolders']`
/// on success so the sidebar picks up the new label. tuxlink-ejph (Phase 3).
export function useRenameUserFolder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ slug, displayName }: { slug: string; displayName: string }) =>
      invoke<UserFolder>('folder_rename', { slug, displayName }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: USER_FOLDERS_QUERY_KEY });
    },
  });
}

/// What to do with messages remaining in a user folder when the folder is
/// deleted (spec §6 D6). Mirrors the Rust `DeleteAction` enum on the wire.
export type DeleteFolderAction = 'move_to_inbox' | 'move_to_archive' | 'delete';

/// Mutation: delete a user folder, cascading to its subfolders (spec §6 D6).
/// The `onMessages` selector controls disposition — `move_to_inbox` is the safe
/// default the dialog picks. Resolves to the slugs actually removed (parent +
/// children) so the caller can clear a stale selection (A5). On success
/// invalidates `['userFolders']` and `['mailbox']` so the sidebar drops the rows
/// AND any folder lists currently showing those messages re-fetch the cascaded
/// destination.
export function useDeleteUserFolder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ slug, onMessages }: { slug: string; onMessages: DeleteFolderAction }) =>
      invoke<string[]>('folder_delete', { slug, onMessages }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: USER_FOLDERS_QUERY_KEY });
      void qc.invalidateQueries({ queryKey: ['mailbox'] });
    },
  });
}
