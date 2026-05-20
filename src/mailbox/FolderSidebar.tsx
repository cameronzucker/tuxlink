// Folder sidebar — Mock B `.sidebar` (Mailbox + Connections sections, 200px).
//
// Source: the MOCK B sidebar (2026-05-17-mocks-v1-four-directions.html lines
// 1190-1201). v0.0.1 functional folders are Inbox + Sent; Outbox + Archive are
// disabled (v0.1). The Connections section is informational (Telnet live;
// VARA HF/FM + AX.25 are v0.1). Selecting a functional folder calls
// `onSelectFolder(folder)`. Styling lives in AppShell.css (`.layout-b .sidebar`).

import type { MailboxFolder } from './types';

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

/// Connections section (mock B) — informational, not folders. Telnet is live;
/// the rest are v0.1.
const CONNECTION_ITEMS: readonly { label: string; icon: string; v01?: boolean }[] = [
  { label: 'Telnet', icon: '●' },
  { label: 'VARA HF', icon: '○', v01: true },
  { label: 'VARA FM', icon: '○', v01: true },
  { label: 'AX.25', icon: '○', v01: true },
];

export interface FolderSidebarProps {
  selectedFolder: MailboxFolder;
  onSelectFolder: (folder: MailboxFolder) => void;
  /// Per-folder counts (Inbox = unread, Sent = total). Missing/zero → no count.
  counts?: Partial<Record<MailboxFolder, number>>;
}

export function FolderSidebar({ selectedFolder, onSelectFolder, counts = {} }: FolderSidebarProps) {
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
    </nav>
  );
}
