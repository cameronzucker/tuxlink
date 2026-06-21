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
/// `identity` is optional — forwarded by delete/restore to resolve the correct
/// per-identity namespace; ignored by move/mark-read callers (they use the
/// shared namespace). Matches the Rust `MessageRefDto` wire shape.
export interface BulkMessageRef {
  folder: MailboxFolderRef;
  id: string;
  identity?: string;
}

/// Map a selection set to per-message `{ folder, id, identity }` refs.
///
/// - Each id resolves to its row's own `message.folder` when present
///   (cross-folder search hits) and falls back to `fallbackFolder` otherwise.
/// - `identity` is taken from `message.identity` when present (for per-identity
///   namespace resolution in delete/restore).
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
    .map((id) => {
      const m = byId.get(id)!;
      return {
        folder: (m.folder as MailboxFolderRef | undefined) ?? fallbackFolder,
        id,
        identity: m.identity,
      };
    });
}

/// Return a copy of `set` with `id` removed, or the original set unchanged when
/// `id` is absent (stable identity avoids a needless re-render/churn).
export function dropId(set: Set<string>, id: string): Set<string> {
  if (!set.has(id)) return set;
  const next = new Set(set);
  next.delete(id);
  return next;
}

/// Return a copy of `set` with every id in `ids` removed, or the original set
/// when nothing intersects. Used to drop a whole bulk target set from the
/// selection after a move/archive — including stale ids that never produced a
/// move item (they would otherwise strand the bulk bar count, #499/Codex P2).
export function dropIds(set: Set<string>, ids: ReadonlySet<string>): Set<string> {
  if (![...ids].some((id) => set.has(id))) return set;
  return new Set([...set].filter((id) => !ids.has(id)));
}
