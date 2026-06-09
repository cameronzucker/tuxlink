// Task 13 — message reading pane hook.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.3
// bd issue: tuxlink-y5c
//
// `useMessage(folder, id)` wraps the `message_read` Tauri command via
// TanStack Query. The query key is `['message', folder, id]` so that Inbox
// and Sent messages with the same MID are cached separately (spec §4.2 —
// `selectedMessage` always carries the folder; never assume Inbox).
//
// The hook is `enabled` only when both folder and id are non-null, which
// corresponds to `!!selectedMessage` in the AppShell (spec §5.3).
//
// The `buildMessageQueryKey` and `buildMessageQueryOptions` exports are
// factored out for unit testing without requiring a QueryClientProvider.

import { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQuery, useQueryClient, type UseQueryOptions } from '@tanstack/react-query';
import type { ParsedMessage, MailboxFolderRef, UiError } from './types';
import { folderBearsReadState } from './readState';
import { DEV_FIXTURE, devMessageFor } from './devFixture';
import { draftToParsedMessage } from './draftMailbox';
import { loadDraft } from '../compose/useDraft';

// ============================================================================
// Wire types — what the Rust command returns on the wire (camelCase already
// normalised by Tauri's JSON serialization matching Rust's rename_all).
// ParsedMessage in types.ts matches this shape.
// ============================================================================

// ============================================================================
// Query-key factory (exported for tests)
// ============================================================================

export type MessageQueryKey = ['message', MailboxFolderRef, string];

/** Build the TanStack Query key for a single message (folder + id tuple). */
export function buildMessageQueryKey(folder: MailboxFolderRef, id: string): MessageQueryKey {
  return ['message', folder, id];
}

export interface MessageSelection {
  folder: MailboxFolderRef;
  id: string;
}

/**
 * Build the TanStack Query options for a message query.
 * `enabled` is false when selection is null (no message selected).
 */
export function buildMessageQueryOptions(
  selection: MessageSelection | null,
): UseQueryOptions<ParsedMessage, UiError, ParsedMessage, MessageQueryKey> {
  return {
    queryKey: selection
      ? buildMessageQueryKey(selection.folder, selection.id)
      : buildMessageQueryKey('inbox', '__none__'),
    queryFn: async () => {
      if (!selection) throw new Error('no selection');
      if (selection.folder === 'drafts') {
        const draft = loadDraft(selection.id);
        if (!draft) throw { kind: 'NotFound', detail: selection.id } satisfies UiError;
        return draftToParsedMessage(draft);
      }
      return invoke<ParsedMessage>('message_read', {
        folder: selection.folder,
        id: selection.id,
      });
    },
    enabled: selection !== null && !!selection.id,
  };
}

// ============================================================================
// Hook
// ============================================================================

/**
 * Query the `message_read` command for a single message.
 *
 * Returns a TanStack Query result. When `selection` is null (no message
 * selected), `enabled` is false and the hook returns `{ data: undefined,
 * isLoading: false }`.
 *
 * The folder always comes from `selectedMessage.folder` in `AppShell`
 * (spec §4.2). The query key is `['message', folder, id]` so Inbox and
 * Sent messages with the same MID are cached independently.
 */
export function useMessage(selection: MessageSelection | null) {
  const result = useQuery(buildMessageQueryOptions(selection));
  const queryClient = useQueryClient();

  // Mark the message read on open — once per open transition, never on a
  // refetch. A ref records the last (folder/id) key that was marked so that
  // re-renders / TanStack background refetches do NOT re-fire the mark, which
  // would clobber an explicit "Mark Unread" applied to the currently-open
  // message (design §1.4). After the mark completes, invalidate ['mailbox'] so
  // the unread badge updates promptly instead of waiting for the 10s poll.
  // Received-mail folders only (inbox / archive / user-folder slugs);
  // sent / outbox / drafts / deleted carry no read-state.
  const markedRef = useRef<string | null>(null);
  useEffect(() => {
    if (!selection || !result.isSuccess) return;
    const key = `${selection.folder}/${selection.id}`;
    // Deduplicate: same message opened twice in a row (re-render / refetch)
    // must not re-fire the mark. Check BEFORE updating the ref.
    if (markedRef.current === key) return;
    // Update the ref for EVERY open transition — including readless folders
    // (Sent/Outbox/Drafts). This prevents a stale ref from suppressing the
    // mark when the operator returns to a previously-marked message after
    // visiting a readless folder (Fix 5: guard reset on readless opens).
    markedRef.current = key;
    // Only actually invoke for folders that bear read-state.
    if (!folderBearsReadState(selection.folder)) return;
    // Best-effort: on failure the ref stays set (no retry this open); the badge self-heals via the mailbox poll or the next distinct open.
    void invoke('message_set_read_state', {
      folder: selection.folder,
      id: selection.id,
      read: true,
    }).then(() => queryClient.invalidateQueries({ queryKey: ['mailbox'] })).catch(() => {});
  }, [selection?.folder, selection?.id, result.isSuccess, queryClient]);

  // Dev fixture: when the real backend has no data (empty / NotConfigured) and
  // the dev fixture is active (vite dev server only), surface a sample parsed
  // message so the reading pane renders for local validation. Off in tests +
  // production (DEV_FIXTURE is false there), so `result` passes through.
  if (DEV_FIXTURE && selection && !result.data) {
    const fixture = devMessageFor(selection.id);
    if (fixture) {
      return { ...result, data: fixture, isLoading: false, isError: false, error: null };
    }
  }

  return result;
}
