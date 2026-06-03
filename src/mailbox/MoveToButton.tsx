/**
 * MoveToButton — reading-pane toolbar action that moves the open message to
 * a chosen folder (tuxlink-f62f).
 *
 * Renders as "Move ▾" next to the existing Reply / Forward / Archive cluster.
 * Click opens a Radix DropdownMenu listing system folders + user folders. The
 * folder the message is currently in is shown disabled (moving to itself is a
 * no-op the backend would accept, but the UI suppresses it as misleading).
 *
 * MVP scope (per spec §6 D5): single-message move via context menu. Multi-
 * select + drag-drop are Phase 3.
 */

import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import type { MailboxFolderRef, UserFolder } from './types';

/// Built-in destination folders shown in the picker. Drafts/Deleted/Outbox
/// are excluded — Drafts is local-only, Deleted is unimplemented, and moving
/// to Outbox bypasses the send queue (operator-grade footgun). Inbox + Sent
/// + Archive are the legitimate system destinations.
const SYSTEM_DESTINATIONS: readonly { slug: MailboxFolderRef; label: string }[] = [
  { slug: 'inbox', label: 'Inbox' },
  { slug: 'sent', label: 'Sent' },
  { slug: 'archive', label: 'Archive' },
];

export interface MoveToButtonProps {
  /// The current folder of the open message. Used to disable the self-target
  /// row (moving to the current folder is a no-op).
  currentFolder: MailboxFolderRef;
  /// User folders to offer as destinations.
  userFolders: UserFolder[];
  /// Move the open message to the chosen folder. Receives the destination
  /// folder identifier (system slug OR user-folder slug).
  onMove: (to: MailboxFolderRef) => void;
}

export function MoveToButton({ currentFolder, userFolders, onMove }: MoveToButtonProps) {
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          className="action-btn"
          data-testid="move-to-btn"
          title="Move to folder"
        >
          Move <span aria-hidden="true">▾</span>
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          className="move-to-menu"
          data-testid="move-to-menu"
          sideOffset={4}
          align="end"
          style={{
            background: 'var(--elevated, #1e2832)',
            border: '1px solid var(--border-strong, #2c3744)',
            borderRadius: 6,
            padding: '4px 0',
            fontSize: 12,
            minWidth: 200,
            boxShadow: '0 6px 20px rgba(0, 0, 0, 0.6)',
            color: 'var(--text, #e4ebf2)',
            zIndex: 100,
          }}
        >
          <DropdownMenu.Label style={labelStyle}>System</DropdownMenu.Label>
          {SYSTEM_DESTINATIONS.map((d) => {
            const self = d.slug === currentFolder;
            return (
              <DropdownMenu.Item
                key={d.slug}
                data-testid={`move-to-${d.slug}`}
                disabled={self}
                onSelect={() => !self && onMove(d.slug)}
                style={{ ...itemStyle, opacity: self ? 0.4 : 1, cursor: self ? 'default' : 'pointer' }}
              >
                {d.label}
                {self && <span style={selfNoteStyle}> (current)</span>}
              </DropdownMenu.Item>
            );
          })}
          {userFolders.length > 0 && (
            <>
              <DropdownMenu.Separator style={separatorStyle} />
              <DropdownMenu.Label style={labelStyle}>Folders</DropdownMenu.Label>
              {userFolders.map((uf) => {
                const self = uf.slug === currentFolder;
                return (
                  <DropdownMenu.Item
                    key={uf.slug}
                    data-testid={`move-to-${uf.slug}`}
                    disabled={self}
                    onSelect={() => !self && onMove(uf.slug)}
                    style={{ ...itemStyle, opacity: self ? 0.4 : 1, cursor: self ? 'default' : 'pointer' }}
                  >
                    {uf.displayName}
                    {self && <span style={selfNoteStyle}> (current)</span>}
                  </DropdownMenu.Item>
                );
              })}
            </>
          )}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

const itemStyle: React.CSSProperties = {
  padding: '6px 14px',
  outline: 'none',
};

const labelStyle: React.CSSProperties = {
  padding: '6px 14px 4px',
  fontSize: 10,
  textTransform: 'uppercase',
  letterSpacing: '0.06em',
  color: 'var(--text-faint, #5d6975)',
};

const separatorStyle: React.CSSProperties = {
  height: 1,
  background: 'var(--border, #1f2832)',
  margin: '4px 0',
};

const selfNoteStyle: React.CSSProperties = {
  color: 'var(--text-faint, #5d6975)',
  fontSize: 11,
  marginLeft: 6,
};
