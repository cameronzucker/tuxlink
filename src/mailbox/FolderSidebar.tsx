// Folder sidebar — Mock B `.sidebar` (Mailbox + Connections sections, 200px).
//
// Source: the MOCK B sidebar (2026-05-17-mocks-v1-four-directions.html lines
// 1190-1201). v0.0.1 functional folders are Inbox + Sent; Outbox + Archive are
// disabled (v0.1). The Connections section has informational static items
// (Telnet, VARA HF/FM) plus a selectable Packet (AX.25) item. Selecting a
// functional folder calls `onSelectFolder(folder)`. Selecting the Packet item
// calls `onSelectConnection('packet')`. Styling lives in AppShell.css.

import type { MailboxFolder } from './types';

/** Selectable connection key (drives the reading-pane connection panel). */
export type ConnectionKey = 'packet';

/** Packet transport dot state for the sidebar indicator. */
export type PacketDotState = 'off' | 'listening' | 'connected';

interface MailboxItem {
  id?: MailboxFolder; // present → a functional folder
  label: string;
  icon: string;
  enabled: boolean;
  v01?: boolean;
}

/// Mailbox section (mock B order). Inbox/Sent functional; Outbox/Archive v0.1.
const MAILBOX_ITEMS: readonly MailboxItem[] = [
  { id: 'inbox', label: 'Inbox', icon: '▣', enabled: true },
  { id: 'sent', label: 'Sent', icon: '▢', enabled: true },
  { label: 'Outbox', icon: '▢', enabled: false, v01: true },
  { label: 'Archive', icon: '▢', enabled: false, v01: true },
];

/// Connections section (mock B) — static informational items + selectable Packet.
/// Telnet is live; VARA HF/FM are v0.1 (informational). AX.25 is replaced by
/// the interactive Packet (AX.25) button below.
const CONNECTION_ITEMS: readonly { label: string; icon: string; v01?: boolean }[] = [
  { label: 'Telnet', icon: '●' },
  { label: 'VARA HF', icon: '○', v01: true },
  { label: 'VARA FM', icon: '○', v01: true },
];

export interface FolderSidebarProps {
  selectedFolder: MailboxFolder;
  onSelectFolder: (folder: MailboxFolder) => void;
  /// Per-folder counts (Inbox = unread, Sent = total). Missing/zero → no count.
  counts?: Partial<Record<MailboxFolder, number>>;
  /** Currently selected connection (drives the reading-pane connection panel). */
  selectedConnection?: ConnectionKey | null;
  /** Select a connection (opens its reading-pane panel). */
  onSelectConnection?: (conn: ConnectionKey) => void;
  /** Packet transport dot state (green = listening/connected). */
  packetState?: PacketDotState;
}

export function FolderSidebar({
  selectedFolder,
  onSelectFolder,
  counts = {},
  selectedConnection = null,
  onSelectConnection,
  packetState = 'off',
}: FolderSidebarProps) {
  return (
    <nav className="sidebar" data-testid="folder-sidebar" aria-label="Mailbox and connections">
      <div className="section-label">Mailbox</div>
      {MAILBOX_ITEMS.map((item) => {
        const isFolder = item.id !== undefined && item.enabled;
        const active = isFolder && item.id === selectedFolder;
        const count = item.id ? counts[item.id] : undefined;
        const className = ['nav-item', active ? 'active' : '', item.enabled ? '' : 'disabled']
          .filter(Boolean)
          .join(' ');
        return (
          <button
            key={item.label}
            type="button"
            data-testid={item.id ? `folder-${item.id}` : `folder-${item.label.toLowerCase()}`}
            className={className}
            disabled={!item.enabled}
            aria-current={active ? 'true' : undefined}
            onClick={() => {
              if (isFolder) onSelectFolder(item.id as MailboxFolder);
            }}
          >
            <span className="icon" aria-hidden="true">
              {item.icon}
            </span>
            {item.label}
            {typeof count === 'number' && count > 0 && (
              <span className="count" data-testid={`folder-count-${item.id}`}>
                {count}
              </span>
            )}
            {item.v01 && <span className="v01-badge">v0.1</span>}
          </button>
        );
      })}

      <div className="section-label">Connections</div>
      {CONNECTION_ITEMS.map((c) => (
        <div
          key={c.label}
          className={['nav-item', c.v01 ? 'disabled' : ''].filter(Boolean).join(' ')}
          data-testid={`conn-${c.label.toLowerCase().replace(/[^a-z0-9]/g, '')}`}
        >
          <span className="icon" aria-hidden="true">
            {c.icon}
          </span>
          {c.label}
          {c.v01 && <span className="v01-badge">v0.1</span>}
        </div>
      ))}

      {/* Selectable Packet (AX.25) entry with transport-state dot */}
      <button
        type="button"
        className={['nav-item', 'conn-packet-item', selectedConnection === 'packet' ? 'active' : '']
          .filter(Boolean)
          .join(' ')}
        data-testid="conn-packet"
        aria-current={selectedConnection === 'packet' ? 'true' : undefined}
        onClick={() => onSelectConnection?.('packet')}
      >
        <span
          className={`conn-dot ${packetState}`}
          data-testid="conn-packet-dot"
          aria-hidden="true"
        />
        Packet (AX.25)
      </button>
    </nav>
  );
}
