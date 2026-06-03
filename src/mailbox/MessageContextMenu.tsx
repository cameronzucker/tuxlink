/**
 * MessageContextMenu — right-click context menu on a message row (tuxlink-ejph).
 *
 * Phase 2 (tuxlink-f62f, PR #284) shipped a reading-pane "Move ▾" dropdown as
 * the only path to move a message into a user folder. That's not the natural
 * mail-client mental model — operators expect right-click → "Move to" — which
 * the user-folders spec §6 D5 had promised as the v1 path. This component
 * delivers it.
 *
 * Renders as a positioned overlay at the right-click coordinates. Action rows
 * are flat (no nested submenus) so the operator can pick a destination folder
 * in a single click. Clicking outside or pressing Escape closes.
 */

import { useEffect, useRef } from 'react';
import type { MailboxFolderRef, UserFolder } from './types';
import type { MessageMeta } from './types';

export interface MessageContextMenuProps {
  /// The right-clicked message + the folder it currently lives in.
  message: MessageMeta;
  folder: MailboxFolderRef;
  /// Screen-coordinate origin (from the right-click event's clientX/Y).
  x: number;
  y: number;
  /// Operator-created folders, shown as Move-to destinations.
  userFolders: UserFolder[];
  /// Move the message to the chosen folder. Receives both the source folder
  /// (so the backend can find the file) and the destination.
  onMoveTo: (toFolder: MailboxFolderRef) => void;
  /// Archive shortcut — equivalent to onMoveTo('archive') but distinguished
  /// in the menu UI per spec §6 D5 (Archive is the one-tap path).
  onArchive: () => void;
  /// Close the menu (Esc / outside-click / after-action).
  onClose: () => void;
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
  onMoveTo,
  onArchive,
  onClose,
}: MessageContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  // Esc closes.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  // Outside-click closes. The handler runs on mousedown so the click that
  // dismisses doesn't also activate whatever was underneath.
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

  // Clip to viewport edges — if the right-click happens near the right or
  // bottom edge, shift the menu inward so it doesn't render off-screen.
  // Defaults assume 240px wide and 320px tall; the actual layout is dynamic
  // but these caps catch the common case without measuring on every render.
  const MENU_W = 240;
  const MENU_H = 320;
  const left = Math.min(x, window.innerWidth - MENU_W - 4);
  const top = Math.min(y, window.innerHeight - MENU_H - 4);

  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Message actions"
      data-testid="message-context-menu"
      style={{
        position: 'fixed',
        left,
        top,
        minWidth: MENU_W,
        background: 'var(--elevated, #1e2832)',
        border: '1px solid var(--border-strong, #2c3744)',
        borderRadius: 6,
        padding: '4px 0',
        fontSize: 12,
        color: 'var(--text, #e4ebf2)',
        boxShadow: '0 6px 20px rgba(0, 0, 0, 0.6)',
        zIndex: 200,
      }}
    >
      <div
        data-testid="ctx-msg-header"
        style={{
          padding: '6px 14px 4px',
          fontSize: 10,
          textTransform: 'uppercase',
          letterSpacing: '0.06em',
          color: 'var(--text-faint, #5d6975)',
        }}
      >
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
            style={{ ...itemStyle, opacity: self ? 0.4 : 1, cursor: self ? 'default' : 'pointer' }}
          >
            {d.label}
            {self && <span style={hintStyle}> (current)</span>}
          </button>
        );
      })}
      <button
        type="button"
        role="menuitem"
        data-testid="ctx-archive"
        disabled={folder === 'archive'}
        onClick={actAndClose(onArchive)}
        style={{
          ...itemStyle,
          opacity: folder === 'archive' ? 0.4 : 1,
          cursor: folder === 'archive' ? 'default' : 'pointer',
        }}
      >
        Archive
        {folder === 'archive' && <span style={hintStyle}> (current)</span>}
      </button>
      {userFolders.length > 0 && (
        <>
          <div style={separatorStyle} />
          <div
            style={{
              padding: '6px 14px 4px',
              fontSize: 10,
              textTransform: 'uppercase',
              letterSpacing: '0.06em',
              color: 'var(--text-faint, #5d6975)',
            }}
          >
            Folders
          </div>
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
                style={{ ...itemStyle, opacity: self ? 0.4 : 1, cursor: self ? 'default' : 'pointer' }}
              >
                {uf.displayName}
                {self && <span style={hintStyle}> (current)</span>}
              </button>
            );
          })}
        </>
      )}
      {/* Footer hint so the operator notices the source-of-truth message
          even though the menu is positioned over the row. */}
      <div style={separatorStyle} />
      <div
        data-testid="ctx-msg-id"
        style={{
          padding: '4px 14px 6px',
          fontSize: 10,
          color: 'var(--text-faint, #5d6975)',
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
        }}
      >
        {truncate(message.subject, 32)}
      </div>
    </div>
  );
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max - 1) + '…';
}

const itemStyle: React.CSSProperties = {
  display: 'block',
  width: '100%',
  textAlign: 'left',
  padding: '6px 14px',
  background: 'transparent',
  border: 'none',
  color: 'inherit',
  fontSize: 12,
  fontFamily: 'inherit',
  cursor: 'pointer',
};

const separatorStyle: React.CSSProperties = {
  height: 1,
  background: 'var(--border, #1f2832)',
  margin: '4px 0',
};

const hintStyle: React.CSSProperties = {
  color: 'var(--text-faint, #5d6975)',
  fontSize: 11,
  marginLeft: 6,
};
