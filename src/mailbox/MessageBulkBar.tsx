/// Bulk action bar shown in the message-list header slot while a selection set
/// is non-empty (tuxlink-etxt; Archive + Move added under tuxlink-l80q;
/// Delete/Restore/Delete-permanently added under tuxlink-wl7n Task 13).
/// Replaces the sort control in place — no new vertical space, never floats
/// over the reading pane.
///
/// Mark read/unread + Archive + Move ▾ all act on the whole selection via the
/// shared bulk handlers in AppShell. The Move dropdown reuses MoveToButton so
/// its destination list + current-folder disabling match the reading-pane and
/// context-menu pickers exactly.
///
/// In the Deleted folder: Archive + Move are replaced by Restore +
/// Delete permanently. Mark read/unread stays available (mirrors the
/// single-message context menu which keeps read-state in Deleted).

import type { MailboxFolderRef, UserFolder } from './types';
import { MoveToButton } from './MoveToButton';

export interface MessageBulkBarProps {
  count: number;
  /// The active folder. Drives the Move dropdown's current-folder disabling,
  /// the Archive button's disabled state (already in Archive → no-op), and
  /// the delete/restore/purge button visibility (Deleted folder vs. others).
  currentFolder: MailboxFolderRef;
  userFolders: UserFolder[];
  onMarkRead: () => void;
  onMarkUnread: () => void;
  onArchive: () => void;
  onMove: (to: MailboxFolderRef) => void;
  onClear: () => void;
  /// Move selected messages to Trash (non-Deleted folders only). No confirm.
  onBulkDelete?: () => void;
  /// Restore selected messages from Trash to their origin (Deleted folder only).
  onBulkRestore?: () => void;
  /// Permanently delete selected messages (Deleted folder only). Confirms first.
  onBulkPurge?: () => void;
}

export function MessageBulkBar({
  count,
  currentFolder,
  userFolders,
  onMarkRead,
  onMarkUnread,
  onArchive,
  onMove,
  onClear,
  onBulkDelete,
  onBulkRestore,
  onBulkPurge,
}: MessageBulkBarProps) {
  const inDeleted = currentFolder === 'deleted';
  return (
    <div className="message-bulk-bar" role="toolbar" aria-label="Selection actions" data-testid="message-bulk-bar">
      <span className="bulk-count" aria-live="polite">{count} selected</span>
      <span className="bulk-spacer" />
      <button type="button" className="bulk-btn primary" onClick={onMarkRead}>Mark read</button>
      <button type="button" className="bulk-btn" onClick={onMarkUnread}>Mark unread</button>
      {inDeleted ? (
        <>
          <button type="button" className="bulk-btn" onClick={onBulkRestore}>Restore</button>
          <button type="button" className="bulk-btn" onClick={onBulkPurge}>Delete permanently</button>
        </>
      ) : (
        <>
          <button
            type="button"
            className="bulk-btn"
            onClick={onArchive}
            disabled={currentFolder === 'archive'}
          >
            Archive
          </button>
          <MoveToButton currentFolder={currentFolder} userFolders={userFolders} onMove={onMove} />
          <button type="button" className="bulk-btn" onClick={onBulkDelete}>Delete</button>
        </>
      )}
      <button type="button" className="bulk-btn clear" aria-label="Clear selection" onClick={onClear}>✕</button>
    </div>
  );
}
