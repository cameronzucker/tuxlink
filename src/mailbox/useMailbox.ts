// Mailbox list query hook.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.2
// bd issue: tuxlink-zsm (Task 12)
//
// Wraps the `mailbox_list` Tauri command in a TanStack Query with a 10s
// refetch interval (spec §5.2). The `drafts` folder is NOT a backend folder
// (spec §2.2) — callers render Drafts from the local draft store (Task 14's
// `listDraftIds`), so this hook is disabled for `drafts`/`deleted` and never
// dispatches a backend command for them.

import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { MailboxFolder, MessageMeta } from './types';
import { devFixtureFor } from './devFixture';

/// Folders that hit a backend command. `drafts` (local store) and `deleted`
/// (disabled placeholder) are excluded — querying them is a frontend bug the
/// Rust `parse_folder` would also reject.
const BACKEND_FOLDERS: ReadonlySet<MailboxFolder> = new Set<MailboxFolder>([
  'inbox',
  'outbox',
  'sent',
]);

export function isBackendFolder(folder: MailboxFolder): boolean {
  return BACKEND_FOLDERS.has(folder);
}

/// Fetch a backend folder's messages via the `mailbox_list` command. The
/// Tauri layer returns `MessageMetaDto[]` (camelCase) which is structurally
/// `MessageMeta[]` on the TS side.
export async function fetchMailbox(folder: MailboxFolder): Promise<MessageMeta[]> {
  return invoke<MessageMeta[]>('mailbox_list', { folder });
}

export interface UseMailboxResult {
  messages: MessageMeta[];
  isLoading: boolean;
  isError: boolean;
  error: unknown;
}

/// Query a folder's messages. 10s refetch (spec §5.2). Disabled for
/// non-backend folders, where it returns an empty list with no dispatch.
export function useMailbox(folder: MailboxFolder): UseMailboxResult {
  const enabled = isBackendFolder(folder);
  const query = useQuery({
    queryKey: ['mailbox', folder],
    queryFn: () => fetchMailbox(folder),
    refetchInterval: 10_000,
    enabled,
  });

  const data = query.data ?? [];
  // Dev fixture: when the real backend yields nothing (empty / NotConfigured)
  // and the dev fixture is active (vite dev server only — devFixture.ts), fall
  // back to sample data so the UI is populated for local validation. Off in
  // tests + production, so this returns [] there and the real result stands.
  const messages = data.length > 0 ? data : devFixtureFor(folder);
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
