// Mailbox list query hook.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.2
// bd issue: tuxlink-zsm (Task 12); user-folder extension tuxlink-f62f.
//
// Wraps the `mailbox_list` Tauri command in a TanStack Query with a 10s
// refetch interval (spec §5.2). The `drafts` folder is NOT a backend folder
// (spec §2.2) — callers render Drafts from the local draft store (Task 14's
// `listDraftIds`), so this hook is disabled for `drafts`/`deleted` and never
// dispatches a backend command for them. User-folder slugs (tuxlink-f62f)
// flow through the same hook + Tauri command — backend dispatches on the
// string at parse time.

import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { MailboxFolder, MailboxFolderRef, MessageMeta } from './types';
import { devFixtureFor } from './devFixture';

export const MAILBOX_QUERY_KEY = ['mailbox'] as const;
export const MAILBOX_CHANGED_EVENT = 'mailbox:changed';

/// System folders that hit a backend command via the `MailboxFolder` enum.
/// `drafts` (local store) and `deleted` (disabled placeholder) are excluded.
/// User-folder slugs (Phase 2, tuxlink-f62f) are NOT in this set — they're
/// recognized by `isUserFolderSlug` and ALSO dispatched to the backend.
const SYSTEM_BACKEND_FOLDERS: ReadonlySet<MailboxFolder> = new Set<MailboxFolder>([
  'inbox',
  'outbox',
  'sent',
  'archive',
]);

/// True for any folder reference that should round-trip through `mailbox_list`.
/// System folders are listed above; user-folder slugs are everything-else
/// that matches the `[a-z0-9-]+` slug shape (Phase 2 — backend validates and
/// returns empty/`NotFound` for unknown slugs, so the generous frontend check
/// here just avoids dispatching for obvious non-folders like `'drafts'`).
export function isBackendFolder(folder: MailboxFolderRef): boolean {
  if ((SYSTEM_BACKEND_FOLDERS as ReadonlySet<string>).has(folder)) return true;
  if (folder === 'drafts' || folder === 'deleted') return false;
  return isUserFolderSlug(folder);
}

/// Slug-shape check (mirrors the Rust `user_folders::validate_slug` rules but
/// is intentionally generous — the source of truth lives backend-side).
export function isUserFolderSlug(s: string): boolean {
  if (s.length === 0 || s.length > 40) return false;
  if (s.startsWith('-') || s.endsWith('-')) return false;
  if (s.includes('--')) return false;
  return /^[a-z0-9-]+$/.test(s);
}

/// Fetch a backend folder's messages via the `mailbox_list` command. The
/// Tauri layer returns `MessageMetaDto[]` (camelCase) which is structurally
/// `MessageMeta[]` on the TS side. Accepts both system folder identifiers
/// and user-folder slugs (the Rust `parse_folder_ref` handles both).
export async function fetchMailbox(folder: MailboxFolderRef): Promise<MessageMeta[]> {
  return invoke<MessageMeta[]>('mailbox_list', { folder });
}

export interface UseMailboxResult {
  messages: MessageMeta[];
  isLoading: boolean;
  isError: boolean;
  error: unknown;
}

/// Query a folder's messages. 10s refetch (spec §5.2). Disabled for
/// non-backend folders (`drafts`, `deleted`) and for invalid slugs.
export function useMailbox(folder: MailboxFolderRef): UseMailboxResult {
  const enabled = isBackendFolder(folder);
  const query = useQuery({
    queryKey: [...MAILBOX_QUERY_KEY, folder],
    queryFn: () => fetchMailbox(folder),
    refetchInterval: 10_000,
    enabled,
  });

  const data = query.data ?? [];
  // Dev fixture: only kicks in for system folders (the fixture data is
  // keyed on `MailboxFolder` literals). User-folder slugs get the real
  // backend result or an empty list — no synthetic content.
  const isSystemFolder = (SYSTEM_BACKEND_FOLDERS as ReadonlySet<string>).has(folder)
    || folder === 'drafts' || folder === 'deleted';
  const messages = data.length > 0
    ? data
    : (isSystemFolder ? devFixtureFor(folder as MailboxFolder) : []);
  const usingFixture = data.length === 0 && messages.length > 0;

  return {
    messages,
    // A disabled query is never "loading" from the user's perspective.
    isLoading: enabled && query.isLoading && !usingFixture,
    // When the fixture is standing in, suppress the error/NotConfigured signal
    // so the list renders rows instead of the "not connected" empty state.
    isError: usingFixture ? false : query.isError,
    error: usingFixture ? undefined : query.error,
  };
}

/// Subscribe once at the shell level so backend mailbox mutations invalidate
/// folder queries immediately instead of waiting for the 10s poll.
export function useMailboxChangeEvents(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    listen<void>(MAILBOX_CHANGED_EVENT, () => {
      void queryClient.invalidateQueries({ queryKey: MAILBOX_QUERY_KEY });
    })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch(() => {
        // No Tauri runtime in some tests/dev harnesses; the 10s refetch remains
        // the fallback path.
      });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [queryClient]);
}
