/**
 * MessageContextMenu — right-click context menu on a message row (tuxlink-ejph;
 * styling refactored under tuxlink-i2nr).
 *
 * Renders as a positioned overlay at the right-click coordinates. Flat action
 * list so the operator picks a destination in one click. Esc / outside-click /
 * after-action all close.
 *
 * Styling matches `.message-list-sort-menu`/`.tux-ctx-*` (userFolders.css) so
 * hover highlight + spacing are consistent with the rest of the project's
 * inline menus.
 */

import { useEffect, useRef } from 'react';
import type { MailboxFolderRef, UserFolder } from './types';
import type { MessageMeta } from './types';
import { folderBearsReadState } from './readState';
import './userFolders.css';

export interface MessageContextMenuProps {
  message: MessageMeta;
  folder: MailboxFolderRef;
  x: number;
  y: number;
  userFolders: UserFolder[];
  /// Called with `read=true` (unread→read) or `read=false` (read→unread).
  /// Only rendered when `folderBearsReadState(folder)` is true.
  onSetReadState?: (read: boolean) => void;
  onMoveTo: (toFolder: MailboxFolderRef) => void;
  onArchive: () => void;
  onClose: () => void;
  /// Move to Deleted (tuxlink-wl7n). Shown below Archive when folder ≠ 'deleted'.
  /// No confirm — delete-to-trash is always recoverable.
  onDelete?: () => void;
  /// Restore from Deleted (tuxlink-wl7n). Shown instead of Delete/Archive/Move
  /// when folder === 'deleted'. No confirm.
  onRestore?: () => void;
  /// Permanently delete a message from Deleted (tuxlink-wl7n). Shown alongside
  /// Restore when folder === 'deleted'. The PARENT wires this to a confirm dialog
  /// (Task 14); the menu just calls the prop without its own modal.
  onDeletePermanently?: () => void;
  /// Selection mode (tuxlink-l80q). When set, the right-clicked row is part of
  /// the multi-select set and every action applies to all `selectionCount`
  /// selected messages. The menu shows an "N messages" header, an
  /// "Acting on N selected messages" footer, and BOTH Mark-as-read /
  /// Mark-as-unread items (a mixed selection has no single toggle direction).
  /// Absent → single-target mode (acts on `message` only), today's behavior.
  selectionCount?: number;
}

/// System destinations always shown above the user-folder list. Drafts/
/// Outbox/Deleted are intentionally excluded: Drafts is local-only, Outbox
/// is the send queue (footgun to drop a read message into), Deleted is
/// unimplemented.
const SYSTEM_DESTINATIONS: readonly { slug: MailboxFolderRef; label: string }[] = [
  { slug: 'inbox', label: 'Inbox' },
  { slug: 'sent', label: 'Sent' },
];

export function MessageContextMenu({
  message,
  folder,
  x,
  y,
  userFolders,
  onSetReadState,
  onMoveTo,
  onArchive,
  onClose,
  onDelete,
  onRestore,
  onDeletePermanently,
  selectionCount,
}: MessageContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const selectionMode = selectionCount !== undefined;
  const plural = (selectionCount ?? 0) === 1 ? 'message' : 'messages';
  // tuxlink-wl7n: when in the Deleted folder the action set switches to
  // Restore + Delete-permanently; Delete/Archive/Move are suppressed.
  const isDeleted = folder === 'deleted';

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  useEffect(() => {
    function onMouseDown(e: MouseEvent) {
      const node = ref.current;
      if (node && !node.contains(e.target as Node)) onClose();
    }
    document.addEventListener('mousedown', onMouseDown);
    return () => document.removeEventListener('mousedown', onMouseDown);
  }, [onClose]);

  function actAndClose(fn: () => void) {
    return () => {
      fn();
      onClose();
    };
  }

  const MENU_W = 220;
  const MENU_H = 320;
  const left = Math.min(x, window.innerWidth - MENU_W - 4);
  const top = Math.min(y, window.innerHeight - MENU_H - 4);

  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Message actions"
      data-testid="message-context-menu"
      className="tux-ctx-menu"
      style={{ position: 'fixed', left, top, minWidth: MENU_W }}
    >
      {selectionMode && (
        <>
          <div className="tux-ctx-label" data-testid="ctx-selection-header">
            {selectionCount} {plural}
          </div>
          <div className="tux-ctx-separator" />
        </>
      )}
      {folderBearsReadState(folder) && (
        <>
          {selectionMode ? (
            <>
              <button
                type="button"
                role="menuitem"
                className="tux-ctx-item"
                data-testid="ctx-set-read"
                onClick={actAndClose(() => onSetReadState?.(true))}
              >
                Mark as read
              </button>
              <button
                type="button"
                role="menuitem"
                className="tux-ctx-item"
                data-testid="ctx-set-unread"
                onClick={actAndClose(() => onSetReadState?.(false))}
              >
                Mark as unread
              </button>
            </>
          ) : (
            <button
              type="button"
              role="menuitem"
              className="tux-ctx-item"
              data-testid="ctx-set-read-state"
              onClick={actAndClose(() => onSetReadState?.(message.unread))}
            >
              {message.unread ? 'Mark as read' : 'Mark as unread'}
            </button>
          )}
          <div className="tux-ctx-separator" />
        </>
      )}
      {/* tuxlink-wl7n: Deleted folder shows Restore + Delete-permanently;
          all other folders show Move to / Archive / Delete. */}
      {isDeleted ? (
        <>
          {onRestore && (
            <button
              type="button"
              role="menuitem"
              data-testid="ctx-restore"
              className="tux-ctx-item"
              onClick={actAndClose(onRestore)}
            >
              Restore
            </button>
          )}
          {onDeletePermanently && (
            <button
              type="button"
              role="menuitem"
              data-testid="ctx-delete-permanently"
              className="tux-ctx-item"
              onClick={actAndClose(onDeletePermanently)}
            >
              Delete permanently
            </button>
          )}
        </>
      ) : (
        <>
          <div className="tux-ctx-label" data-testid="ctx-msg-header">
            Move to
          </div>
          {SYSTEM_DESTINATIONS.map((d) => {
            const self = d.slug === folder;
            return (
              <button
                type="button"
                role="menuitem"
                key={d.slug}
                data-testid={`ctx-move-${d.slug}`}
                disabled={self}
                onClick={actAndClose(() => !self && onMoveTo(d.slug))}
                className="tux-ctx-item"
              >
                {d.label}
                {self && <span className="tux-ctx-item-hint">(current)</span>}
              </button>
            );
          })}
          <button
            type="button"
            role="menuitem"
            data-testid="ctx-archive"
            disabled={folder === 'archive'}
            onClick={actAndClose(onArchive)}
            className="tux-ctx-item"
          >
            Archive
            {folder === 'archive' && <span className="tux-ctx-item-hint">(current)</span>}
          </button>
          {onDelete && (
            <button
              type="button"
              role="menuitem"
              data-testid="ctx-delete"
              className="tux-ctx-item"
              onClick={actAndClose(onDelete)}
            >
              Delete
            </button>
          )}
          {userFolders.length > 0 && (
            <>
              <div className="tux-ctx-separator" />
              <div className="tux-ctx-label">Folders</div>
              {userFolders.map((uf) => {
                const self = uf.slug === folder;
                return (
                  <button
                    type="button"
                    role="menuitem"
                    key={uf.slug}
                    data-testid={`ctx-move-${uf.slug}`}
                    disabled={self}
                    onClick={actAndClose(() => !self && onMoveTo(uf.slug))}
                    className="tux-ctx-item"
                  >
                    {uf.displayName}
                    {self && <span className="tux-ctx-item-hint">(current)</span>}
                  </button>
                );
              })}
            </>
          )}
        </>
      )}
      <div className="tux-ctx-separator" />
      <div className="tux-ctx-footer" data-testid="ctx-msg-id">
        {selectionMode
          ? `Acting on ${selectionCount} selected ${plural}`
          : truncate(message.subject, 32)}
      </div>
    </div>
  );
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max - 1) + '…';
}
