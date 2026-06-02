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

import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQuery, useQueryClient, type UseQueryOptions } from '@tanstack/react-query';
import type { ParsedMessage, MailboxFolderRef, UiError } from './types';
import { DEV_FIXTURE, devMessageFor } from './devFixture';

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

  // Opening an inbox message marks it read server-side (message_read →
  // mark_read). Refresh the mailbox lists so the unread badge updates promptly
  // instead of waiting for the 10s poll. Inbox-only — Sent/Outbox have no unread
  // concept. `selection.id` + `dataUpdatedAt` key the effect to each successful
  // (re)load so it fires once per opened message, not on every render.
  const inboxLoaded = result.isSuccess && selection?.folder === 'inbox';
  useEffect(() => {
    if (inboxLoaded) {
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
    }
  }, [inboxLoaded, selection?.id, result.dataUpdatedAt, queryClient]);

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
