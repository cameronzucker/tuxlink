// Folder sidebar — Mock B `.sidebar` (Mailbox + Connections sections, 200px).
//
// Source: the MOCK B sidebar (2026-05-17-mocks-v1-four-directions.html lines
// 1190-1201). Functional folders are Inbox + Sent + Outbox; Archive is
// disabled (not yet wired). The Connections section is a session-type accordion driven
// by SESSION_TYPES. Selecting a built protocol calls
// `onSelectConnection({ sessionType, protocol })`. Styling lives in AppShell.css.

import { memo, useState, useEffect } from 'react';
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

/// A sidebar nav-item whose selection key is a `MailboxFolderRef` rather than a
/// concrete `MailboxFolder`. The "Address" section's Contacts entry uses this
/// shape: `'contacts'` is a PSEUDO-folder string (never added to the
/// `MailboxFolder` enum), so its `id` is a plain `MailboxFolderRef`. Sharing
/// the `{ id, label, icon, enabled }` shape with `MAILBOX_ITEMS` lets the
/// icon-rail compact mode (FZ-M1) pick the row up generically rather than via a
/// hardcoded one-off path. The count is NOT sourced from the mailbox `counts`
/// memo (that map is keyed by `MailboxFolder`); it is passed per-item.
interface PseudoFolderItem {
  id: MailboxFolderRef; // a pseudo-folder selection key (e.g. 'contacts')
  label: string;
  icon: string;
  enabled: boolean;
  v01?: boolean;
}

/// Address section (tuxlink-raez, Task A7). A single Contacts pseudo-folder
/// row. Declared as a list (mirroring `MAILBOX_ITEMS`) so the icon-rail picks
/// it up generically; the count is supplied at render time from the
/// `contactsCount` prop (sourced from `useContacts` in AppShell), never the
/// mailbox `counts` memo.
const ADDRESS_ITEMS: readonly PseudoFolderItem[] = [
  { id: 'contacts', label: 'Contacts', icon: '◉', enabled: true },
];

/// Mailbox section (mock B order). All four folders functional as of
/// tuxlink-ca5x (user-folders Phase 1). The Phase 2 open-set folder model
/// (tuxlink-f62f) will lift Archive into a dedicated "Folders" section
/// alongside user-created folders; for Phase 1 it stays here.
const MAILBOX_ITEMS: readonly MailboxItem[] = [
  { id: 'inbox', label: 'Inbox', icon: '▣', enabled: true },
  { id: 'sent', label: 'Sent', icon: '▢', enabled: true },
  { id: 'outbox', label: 'Outbox', icon: '▢', enabled: true },
  { id: 'drafts', label: 'Drafts', icon: '▢', enabled: true },
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
  /** Contacts count for the Address section's Contacts pseudo-folder badge
   *  (tuxlink-raez, Task A7). Sourced from `useContacts().contacts.length` in
   *  AppShell — deliberately SEPARATE from `counts` (the mailbox memo keyed by
   *  `MailboxFolder`), since `'contacts'` is not a mailbox folder. Zero/missing
   *  → no badge. */
  contactsCount?: number;
  /** User-created folders (tuxlink-f62f). Rendered in a dedicated "Folders"
   *  section below "Mailbox", with a `+` button that fires `onCreateFolder`. */
  userFolders?: UserFolder[];
  /** Open the New Folder dialog (sidebar `+` button). */
  onCreateFolder?: () => void;
  /** Drag-drop: drop a message row onto a folder (tuxlink-ejph). The DataTransfer
   *  payload `application/x-tuxlink-message` carries `{ id, folder }` from the
   *  source row; the sidebar handles the drop on each folder row and fires
   *  this callback with the dragged message id + source folder + dropped-on
   *  destination. */
  onDropMessage?: (id: string, fromFolder: MailboxFolderRef, toFolder: MailboxFolderRef) => void;
  /** Right-click on a user folder (tuxlink-ejph). Opens FolderContextMenu. */
  onFolderContextMenu?: (slug: string, x: number, y: number) => void;
  /** Currently selected connection (drives the reading-pane connection panel). */
  selectedConnection?: ConnectionKey | null;
  /** Select a connection (opens its reading-pane panel). */
  onSelectConnection?: (conn: ConnectionKey) => void;
}

/// Custom DataTransfer MIME for tuxlink message drags. Mirrors the export in
/// MessageList.tsx — duplicated here so this module stays free of MessageList
/// imports (FolderSidebar is rendered before MessageList in the panes grid).
const TUXLINK_DRAG_MIME = 'application/x-tuxlink-message';

interface DragPayload {
  id: string;
  folder: string;
}

/// Parse the DataTransfer payload set by MessageRow on dragstart. Returns
/// null when the payload is missing or malformed — drop is then a no-op.
function readDragPayload(e: React.DragEvent): DragPayload | null {
  try {
    const raw = e.dataTransfer.getData(TUXLINK_DRAG_MIME);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<DragPayload>;
    if (typeof parsed.id !== 'string' || typeof parsed.folder !== 'string') return null;
    return { id: parsed.id, folder: parsed.folder };
  } catch {
    return null;
  }
}

// tuxlink-djnl: React.memo so shell-level renders (status polls, search
// keystrokes, modem-state changes) skip the sidebar when its inputs are
// unchanged. counts is memoized in AppShell (PR #305); userFolders is
// react-query data (stable across no-op refetches); all callbacks are
// useCallback'd at the AppShell layer.
export const FolderSidebar = memo(function FolderSidebar({
  selectedFolder,
  onSelectFolder,
  counts = {},
  contactsCount = 0,
  userFolders = [],
  onCreateFolder,
  onDropMessage,
  onFolderContextMenu,
  selectedConnection = null,
  onSelectConnection,
}: FolderSidebarProps) {
  // Drag-over visual state — which folder slug currently has the drag hovering.
  // null when nothing is being dragged or the drag is outside a folder.
  const [dragOver, setDragOver] = useState<string | null>(null);

  // Drop handler factory — wraps the payload parse + the AppShell callback.
  const makeDropHandler = (toFolder: MailboxFolderRef) => (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(null);
    const payload = readDragPayload(e);
    if (!payload) return;
    if (payload.folder === toFolder) return; // no-op self-drop
    onDropMessage?.(payload.id, payload.folder as MailboxFolderRef, toFolder);
  };

  // dragOver listeners — call preventDefault so the drop event actually fires
  // (HTML5 DnD requires dragover.preventDefault to mark a valid drop target).
  const makeDragOver = (slug: string) => (e: React.DragEvent) => {
    if (!onDropMessage) return;
    // Only accept drags carrying our payload (browser drags get ignored).
    if (!e.dataTransfer.types.includes(TUXLINK_DRAG_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (dragOver !== slug) setDragOver(slug);
  };
  const handleDragLeave = (slug: string) => () => {
    if (dragOver === slug) setDragOver(null);
  };
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
        const isDropTarget = isFolder && dragOver === item.id;
        const className = [
          'nav-item',
          active ? 'active' : '',
          item.enabled ? '' : 'disabled',
          isDropTarget ? 'drop-target' : '',
        ]
          .filter(Boolean)
          .join(' ');
        const dropSlug = item.id;
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
            onDragOver={isFolder && dropSlug ? makeDragOver(dropSlug) : undefined}
            onDragLeave={isFolder && dropSlug ? handleDragLeave(dropSlug) : undefined}
            onDrop={isFolder && dropSlug ? makeDropHandler(dropSlug as MailboxFolderRef) : undefined}
            style={isDropTarget ? { outline: '1px dashed var(--accent, #f59f3c)' } : undefined}
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
        const isDropTarget = dragOver === uf.slug;
        return (
          <button
            key={uf.slug}
            type="button"
            data-testid={`user-folder-${uf.slug}`}
            className={['nav-item', active ? 'active' : '', isDropTarget ? 'drop-target' : '']
              .filter(Boolean)
              .join(' ')}
            aria-current={active ? 'true' : undefined}
            onClick={() => onSelectFolder(uf.slug)}
            onContextMenu={(e) => {
              if (onFolderContextMenu) {
                e.preventDefault();
                onFolderContextMenu(uf.slug, e.clientX, e.clientY);
              }
            }}
            onDragOver={makeDragOver(uf.slug)}
            onDragLeave={handleDragLeave(uf.slug)}
            onDrop={makeDropHandler(uf.slug)}
            style={isDropTarget ? { outline: '1px dashed var(--accent, #f59f3c)' } : undefined}
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

      {/* Address section (tuxlink-raez, Task A7). The Contacts pseudo-folder
          row. `'contacts'` is NOT a MailboxFolder — it never enters the
          mailbox `counts` memo (that map is keyed by MailboxFolder), and it is
          not drag-droppable (no message can be filed into Contacts). The count
          comes from the dedicated `contactsCount` prop (useContacts in
          AppShell). Declared as a list (ADDRESS_ITEMS) so the icon-rail compact
          mode picks the row up generically. */}
      <div className="section-label">Address</div>
      {ADDRESS_ITEMS.map((item) => {
        const active = item.id === selectedFolder;
        // Per-item count: only Contacts has one, from `contactsCount`. Kept off
        // the mailbox `counts` memo on purpose (M-scope: pseudo-folder).
        const count = item.id === 'contacts' ? contactsCount : undefined;
        const className = ['nav-item', active ? 'active' : '', item.enabled ? '' : 'disabled']
          .filter(Boolean)
          .join(' ');
        return (
          <button
            key={item.id}
            type="button"
            data-testid={`folder-${item.id}`}
            className={className}
            disabled={!item.enabled}
            aria-current={active ? 'true' : undefined}
            onClick={() => {
              if (item.enabled) onSelectFolder(item.id);
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
});
