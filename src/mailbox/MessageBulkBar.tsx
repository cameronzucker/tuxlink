/// Bulk action bar shown in the message-list header slot while a selection set
/// is non-empty (tuxlink-etxt; Archive + Move added under tuxlink-l80q).
/// Replaces the sort control in place — no new vertical space, never floats
/// over the reading pane.
///
/// Mark read/unread + Archive + Move ▾ all act on the whole selection via the
/// shared bulk handlers in AppShell. The Move dropdown reuses MoveToButton so
/// its destination list + current-folder disabling match the reading-pane and
/// context-menu pickers exactly.

import type { MailboxFolderRef, UserFolder } from './types';
import { MoveToButton } from './MoveToButton';

export interface MessageBulkBarProps {
  count: number;
  /// The active folder. Drives the Move dropdown's current-folder disabling and
  /// the Archive button's disabled state (already in Archive → no-op).
  currentFolder: MailboxFolderRef;
  userFolders: UserFolder[];
  onMarkRead: () => void;
  onMarkUnread: () => void;
  onArchive: () => void;
  onMove: (to: MailboxFolderRef) => void;
  onClear: () => void;
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
}: MessageBulkBarProps) {
  return (
    <div className="message-bulk-bar" role="toolbar" aria-label="Selection actions" data-testid="message-bulk-bar">
      <span className="bulk-count" aria-live="polite">{count} selected</span>
      <span className="bulk-spacer" />
      <button type="button" className="bulk-btn primary" onClick={onMarkRead}>Mark read</button>
      <button type="button" className="bulk-btn" onClick={onMarkUnread}>Mark unread</button>
      <button
        type="button"
        className="bulk-btn"
        onClick={onArchive}
        disabled={currentFolder === 'archive'}
      >
        Archive
      </button>
      <MoveToButton currentFolder={currentFolder} userFolders={userFolders} onMove={onMove} />
      <button type="button" className="bulk-btn clear" aria-label="Clear selection" onClick={onClear}>✕</button>
    </div>
  );
}
