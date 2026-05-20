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

import { invoke } from '@tauri-apps/api/core';
import { useQuery, type UseQueryOptions } from '@tanstack/react-query';
import type { ParsedMessage, MailboxFolder, UiError } from './types';

// ============================================================================
// Wire types — what the Rust command returns on the wire (camelCase already
// normalised by Tauri's JSON serialization matching Rust's rename_all).
// ParsedMessage in types.ts matches this shape.
// ============================================================================

// ============================================================================
// Query-key factory (exported for tests)
// ============================================================================

export type MessageQueryKey = ['message', MailboxFolder, string];

/** Build the TanStack Query key for a single message (folder + id tuple). */
export function buildMessageQueryKey(folder: MailboxFolder, id: string): MessageQueryKey {
  return ['message', folder, id];
}

export interface MessageSelection {
  folder: MailboxFolder;
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
  return useQuery(buildMessageQueryOptions(selection));
}
