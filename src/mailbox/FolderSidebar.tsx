// Folder sidebar — Mock B `.sidebar` (Mailbox + Connections sections, 200px).
//
// Source: the MOCK B sidebar (2026-05-17-mocks-v1-four-directions.html lines
// 1190-1201). v0.0.1 functional folders are Inbox + Sent; Outbox + Archive are
// disabled (v0.1). The Connections section is a session-type accordion driven
// by SESSION_TYPES. Selecting a built protocol calls
// `onSelectConnection({ sessionType, protocol })`. Styling lives in AppShell.css.

import { useState, useEffect } from 'react';
import type { MailboxFolder } from './types';
import { SESSION_TYPES } from '../connections/sessionTypes';
import type { SessionTypeId, ConnectionKey } from '../connections/sessionTypes';

// Re-export ConnectionKey so existing importers (AppShell.tsx, etc.) keep resolving.
export type { ConnectionKey } from '../connections/sessionTypes';

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
  const [expanded, setExpanded] = useState<Partial<Record<SessionTypeId, boolean>>>({});

  // Ensure the selected session type is always visible — auto-expand its accordion
  // section whenever selectedConnection is set (or changes). Never collapses anything
  // the user opened; only ensures the active section is open.
  useEffect(() => {
    if (selectedConnection) {
      setExpanded((e) => (e[selectedConnection.sessionType] ? e : { ...e, [selectedConnection.sessionType]: true }));
    }
  }, [selectedConnection]);

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
      {SESSION_TYPES.map((s) => (
        <div key={s.id}>
          {/* Session-type header (accordion toggle) */}
          <button
            type="button"
            data-testid={`sess-${s.id}`}
            className="nav-item"
            aria-expanded={!!expanded[s.id]}
            onClick={() => setExpanded((e) => ({ ...e, [s.id]: !e[s.id] }))}
          >
            <span aria-hidden="true">{expanded[s.id] ? '▾' : '▸'}</span>
            {s.label}
          </button>

          {/* Protocol rows (only shown when expanded) */}
          {expanded[s.id] &&
            s.protocols.map((p) => {
              const isActive =
                selectedConnection?.sessionType === s.id &&
                selectedConnection?.protocol === p.id;
              const isPacketRow = p.id === 'packet';

              return (
                <button
                  key={p.id}
                  type="button"
                  data-testid={`proto-${s.id}-${p.id}`}
                  className={['nav-item', 'proto', isActive ? 'active' : '']
                    .filter(Boolean)
                    .join(' ')}
                  disabled={!p.built}
                  aria-current={isActive ? 'true' : undefined}
                  onClick={() => onSelectConnection?.({ sessionType: s.id, protocol: p.id })}
                >
                  {/* Transport-state dot for the active packet row */}
                  {isPacketRow && (
                    <span
                      className={`conn-dot ${packetState}`}
                      data-testid="conn-packet-dot"
                      aria-hidden="true"
                    />
                  )}
                  {p.label}
                  {!p.built && <span className="v01-badge">soon</span>}
                </button>
              );
            })}
        </div>
      ))}
    </nav>
  );
}
