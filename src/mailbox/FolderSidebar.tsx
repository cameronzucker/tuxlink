// Folder sidebar — Mock B `.sidebar` (Mailbox + Connections sections, 200px).
//
// Source: the MOCK B sidebar (2026-05-17-mocks-v1-four-directions.html lines
// 1190-1201). Functional folders are Inbox + Sent + Outbox; Archive is
// disabled (not yet wired). The Connections section is a session-type accordion driven
// by SESSION_TYPES. Selecting a built protocol calls
// `onSelectConnection({ sessionType, protocol })`. Styling lives in AppShell.css.

import { useState, useEffect } from 'react';
import type { MailboxFolder, MailboxFolderRef, UserFolder } from './types';
import { SESSION_TYPES } from '../connections/sessionTypes';
import type { SessionTypeId, ConnectionKey } from '../connections/sessionTypes';

// Re-export ConnectionKey so existing importers (AppShell.tsx, etc.) keep resolving.
export type { ConnectionKey } from '../connections/sessionTypes';

interface MailboxItem {
  id?: MailboxFolder; // present → a functional folder
  label: string;
  icon: string;
  enabled: boolean;
  v01?: boolean;
}

/// Mailbox section (mock B order). All four folders functional as of
/// tuxlink-ca5x (user-folders Phase 1). The Phase 2 open-set folder model
/// (tuxlink-f62f) will lift Archive into a dedicated "Folders" section
/// alongside user-created folders; for Phase 1 it stays here.
const MAILBOX_ITEMS: readonly MailboxItem[] = [
  { id: 'inbox', label: 'Inbox', icon: '▣', enabled: true },
  { id: 'sent', label: 'Sent', icon: '▢', enabled: true },
  { id: 'outbox', label: 'Outbox', icon: '▢', enabled: true },
  { id: 'archive', label: 'Archive', icon: '▢', enabled: true },
];

export interface FolderSidebarProps {
  /** Currently-selected folder. Accepts either a system-folder identifier
   *  (one of `MailboxFolder`) OR a user-folder slug; the sidebar uses
   *  string-equal to decide which row gets the active style. */
  selectedFolder: MailboxFolderRef;
  /** Select a folder. Argument is either a `MailboxFolder` (for system
   *  rows) or a user-folder slug (for user rows). The parent (AppShell)
   *  passes both to `setSelectedFolder` interchangeably; the Tauri
   *  commands accept either string at the boundary (tuxlink-f62f). */
  onSelectFolder: (folder: MailboxFolderRef) => void;
  /// Per-folder counts for system folders (Inbox = unread, Sent = total).
  /// Missing/zero → no count. User-folder counts are deferred to Phase 2.5
  /// (N+1 query optimization — backend `user_folders_list_with_counts`).
  counts?: Partial<Record<MailboxFolder, number>>;
  /** User-created folders (tuxlink-f62f). Rendered in a dedicated "Folders"
   *  section below "Mailbox", with a `+` button that fires `onCreateFolder`. */
  userFolders?: UserFolder[];
  /** Open the New Folder dialog (sidebar `+` button). */
  onCreateFolder?: () => void;
  /** Currently selected connection (drives the reading-pane connection panel). */
  selectedConnection?: ConnectionKey | null;
  /** Select a connection (opens its reading-pane panel). */
  onSelectConnection?: (conn: ConnectionKey) => void;
}

export function FolderSidebar({
  selectedFolder,
  onSelectFolder,
  counts = {},
  userFolders = [],
  onCreateFolder,
  selectedConnection = null,
  onSelectConnection,
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
            {item.v01 && <span className="v01-badge">soon</span>}
          </button>
        );
      })}

      {/* User folders (tuxlink-f62f). The Folders section header carries a
          `+` button that fires onCreateFolder; the section is rendered even
          when empty so the operator's path to creating a first folder is
          always visible. */}
      <div
        className="section-label"
        style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}
      >
        <span>Folders</span>
        {onCreateFolder && (
          <button
            type="button"
            data-testid="folder-create-btn"
            onClick={onCreateFolder}
            aria-label="New folder"
            title="New folder"
            style={{
              background: 'transparent',
              border: '1px solid var(--border-strong, #2c3744)',
              borderRadius: 3,
              color: 'inherit',
              fontSize: 13,
              width: 18,
              height: 18,
              display: 'inline-flex',
              alignItems: 'center',
              justifyContent: 'center',
              cursor: 'pointer',
              padding: 0,
              lineHeight: 1,
            }}
          >
            +
          </button>
        )}
      </div>
      {userFolders.map((uf) => {
        const active = uf.slug === selectedFolder;
        return (
          <button
            key={uf.slug}
            type="button"
            data-testid={`user-folder-${uf.slug}`}
            className={['nav-item', active ? 'active' : ''].filter(Boolean).join(' ')}
            aria-current={active ? 'true' : undefined}
            onClick={() => onSelectFolder(uf.slug)}
          >
            <span className="icon" aria-hidden="true">▢</span>
            {uf.displayName}
          </button>
        );
      })}
      {userFolders.length === 0 && (
        <div
          data-testid="folders-empty-hint"
          style={{
            padding: '4px 10px',
            fontSize: 11,
            fontStyle: 'italic',
            color: 'var(--text-faint, #5d6975)',
          }}
        >
          {onCreateFolder ? 'Click + to create one' : 'No custom folders yet'}
        </div>
      )}

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

              // tuxlink-bcgj: the Packet row used to carry a transport-state
              // dot (off/listening/connected). It duplicated the DashboardRibbon's
              // connection chip + made the sidebar asymmetric (no other
              // transport had a dot). Removed for visual cohesion.
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
