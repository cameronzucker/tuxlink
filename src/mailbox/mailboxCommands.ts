// Thin `invoke` wrappers for delete / restore / trash commands (tuxlink-wl7n Task 10).
//
// Each function is a typed façade over the Tauri command surface so callers
// (AppShell, MessageView, MessageContextMenu) don't scatter raw `invoke` strings
// throughout the codebase. Query invalidation is the caller's responsibility —
// each command fires `mailbox:changed` server-side, which the
// `useMailboxChangeEvents` subscriber already picks up, but explicit
// `queryClient.invalidateQueries` in the callers keeps latency minimal.

import { invoke } from '@tauri-apps/api/core';
import type { MailboxFolderRef } from './types';

export interface DeleteItem {
  id: string;
  folder: MailboxFolderRef;
  identity?: string;
}

/// Bulk delete — moves each listed message to the Deleted folder.
/// For the single-message case, pass a one-element array.
/// `identity` is forwarded to the backend so delete/restore target the correct
/// per-identity namespace when a message belongs to a non-default identity.
export async function deleteMessages(items: DeleteItem[]): Promise<void> {
  await invoke<void>('message_delete_bulk', {
    items: items.map(({ id, folder, identity }) => ({ id, folder, identity })),
  });
}

/// Bulk restore — reads each message's `.trash` sidecar and moves it back to
/// its origin folder. Accepts only MIDs (the sidecar carries the origin).
export async function restoreMessages(ids: string[]): Promise<void> {
  await invoke<void>('message_restore_bulk', { ids });
}

/// Empty the entire Deleted folder. Returns the count of messages purged.
export async function emptyTrash(): Promise<number> {
  return invoke<number>('trash_empty');
}

/// Permanently delete a single message from the Deleted folder (no recovery).
export async function purgeMessage(id: string): Promise<void> {
  await invoke<void>('trash_purge_one', { id });
}
