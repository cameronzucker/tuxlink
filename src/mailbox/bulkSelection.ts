// Shared selection→folder mapping for the multi-select bulk handlers
// (tuxlink-l80q). A selection is a Set of message ids; a bulk command needs
// each message paired with its OWN source folder so a cross-folder search
// selection (which mixes folders) targets each message correctly.
//
// Extracted from the original inline mapping in AppShell's bulkSetReadState so
// the read-state, move, and archive bulk handlers all share one implementation
// — and so the Fix-3 stale-id filter (#499) is tested in one place.

import type { MailboxFolderRef, MessageMeta } from './types';

/// One message reference for a bulk command: the message's own folder + id.
/// Matches the Rust `MessageRefDto` wire shape (`{ folder, id }`).
export interface BulkMessageRef {
  folder: MailboxFolderRef;
  id: string;
}

/// Map a selection set to per-message `{ folder, id }` refs.
///
/// - Each id resolves to its row's own `message.folder` when present
///   (cross-folder search hits) and falls back to `fallbackFolder` otherwise.
/// - Ids NOT present in `visible` are dropped (Fix-3, #499): a stale selection
///   (row removed between select and act) must never fall back to the active
///   folder for an unknown message — that could target the wrong folder in a
///   cross-folder view.
export function selectionToFolderItems(
  ids: ReadonlySet<string>,
  visible: MessageMeta[],
  fallbackFolder: MailboxFolderRef,
): BulkMessageRef[] {
  const byId = new Map(visible.map((m) => [m.id, m] as const));
  return [...ids]
    .filter((id) => byId.has(id))
    .map((id) => ({
      folder: (byId.get(id)!.folder as MailboxFolderRef | undefined) ?? fallbackFolder,
      id,
    }));
}
