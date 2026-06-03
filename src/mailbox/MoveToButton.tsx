/**
 * MoveToButton — reading-pane toolbar action that moves the open message to
 * a chosen folder (tuxlink-f62f; styling refactored under tuxlink-i2nr).
 *
 * Renders as "Move ▾" next to the existing Reply / Forward / Archive cluster.
 * Click opens a Radix DropdownMenu listing system folders + user folders. The
 * folder the message is currently in is shown disabled (moving to itself is a
 * no-op the backend would accept, but the UI suppresses it as misleading).
 *
 * Styling matches `.message-list-sort-menu`/`.tux-ctx-*` (userFolders.css) so
 * the dropdown looks identical to the project's other Radix-backed menus.
 */

import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import type { MailboxFolderRef, UserFolder } from './types';
import './userFolders.css';

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
  currentFolder: MailboxFolderRef;
  userFolders: UserFolder[];
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
          Move <span aria-hidden="true" className="tux-move-caret">▾</span>
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          className="tux-ctx-menu"
          data-testid="move-to-menu"
          sideOffset={4}
          align="end"
          collisionPadding={8}
        >
          <DropdownMenu.Label className="tux-ctx-label">System</DropdownMenu.Label>
          {SYSTEM_DESTINATIONS.map((d) => {
            const self = d.slug === currentFolder;
            return (
              <DropdownMenu.Item
                key={d.slug}
                data-testid={`move-to-${d.slug}`}
                disabled={self}
                onSelect={() => !self && onMove(d.slug)}
                className="tux-ctx-item"
              >
                {d.label}
                {self && <span className="tux-ctx-item-hint">(current)</span>}
              </DropdownMenu.Item>
            );
          })}
          {userFolders.length > 0 && (
            <>
              <DropdownMenu.Separator className="tux-ctx-separator" />
              <DropdownMenu.Label className="tux-ctx-label">Folders</DropdownMenu.Label>
              {userFolders.map((uf) => {
                const self = uf.slug === currentFolder;
                return (
                  <DropdownMenu.Item
                    key={uf.slug}
                    data-testid={`move-to-${uf.slug}`}
                    disabled={self}
                    onSelect={() => !self && onMove(uf.slug)}
                    className="tux-ctx-item"
                  >
                    {uf.displayName}
                    {self && <span className="tux-ctx-item-hint">(current)</span>}
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
