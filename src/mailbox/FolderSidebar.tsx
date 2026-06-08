// Folder sidebar — Mock B `.sidebar` (Mailbox + Connections sections, 200px).
//
// Source: the MOCK B sidebar (2026-05-17-mocks-v1-four-directions.html lines
// 1190-1201). Functional folders are Inbox + Sent + Outbox; Archive is
// disabled (not yet wired). The Connections section is a session-type accordion driven
// by SESSION_TYPES. Selecting a built protocol calls
// `onSelectConnection({ sessionType, protocol })`. Styling lives in AppShell.css.

import { memo, useState, useEffect, useRef } from 'react';
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

  // FZ-M1 compact rail expand overlay (tuxlink-h7q7 → restructured tuxlink-813d
  // D3). The 52px vertical-text rail ALWAYS stays in the grid; the labeled
  // navigation expands as a SEPARATE absolutely-positioned flyout (sibling of
  // the rail) over the message list, with a scrim. The rail never goes
  // `position: absolute`, so the other panes never shift (grid-implosion fix).
  // CSS-gated to compact; a no-op at desktop (the expand button is
  // display:none there). Dismissal: selecting a folder/connection, a scrim
  // click, an outside pointer-down, or Escape (Claude adrev F11).
  const [railExpanded, setRailExpanded] = useState(false);
  const railRef = useRef<HTMLElement | null>(null);
  const flyoutRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (!railExpanded) return;
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      // Stay open only when the pointer-down lands inside the rail OR the
      // flyout. The scrim is neither, so a scrim pointer-down also closes
      // (in addition to its own onClick handler) — both paths are safe.
      const insideRail = railRef.current?.contains(target);
      const insideFlyout = flyoutRef.current?.contains(target);
      if (!insideRail && !insideFlyout) setRailExpanded(false);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setRailExpanded(false);
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [railExpanded]);

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

  // Collapse on any folder/connection selection — shared by rail tabs and
  // flyout rows.
  const selectFolderAndCollapse = (folder: MailboxFolderRef) => {
    onSelectFolder(folder);
    setRailExpanded(false);
  };

  // ---- Collapsed rail (always rendered, stays in the grid) -----------------
  // Each system + user folder is a vertical-text tab: a reserved `.vslot`
  // (carrying the `.vcount` chip when count>0, kept whether or not a count
  // exists so labels stay aligned) + a `.vlabel`. The drag-drop handlers ride
  // on the tab exactly as the old `.nav-item` rows did. The `☰` expand button
  // opens the flyout. Section headings / the create-`+` / the Connections
  // accordion are NOT on the rail — they live only in the flyout.
  const renderRailTab = (
    key: string,
    testId: string,
    label: string,
    folderRef: MailboxFolderRef | undefined,
    active: boolean,
    count: number | undefined,
    dropSlug: string | undefined,
    onContextMenu?: (e: React.MouseEvent) => void,
  ) => {
    const isDropTarget = dropSlug !== undefined && dragOver === dropSlug;
    return (
      <button
        key={key}
        type="button"
        data-testid={testId}
        className={['vtab', active ? 'active' : '', isDropTarget ? 'drop-target' : '']
          .filter(Boolean)
          .join(' ')}
        aria-current={active ? 'true' : undefined}
        onClick={() => {
          // Non-folder (disabled) system tabs just collapse; only real folders
          // fire selection (mirrors the pre-restructure isFolder guard).
          if (folderRef !== undefined) selectFolderAndCollapse(folderRef);
          else setRailExpanded(false);
        }}
        onContextMenu={onContextMenu}
        onDragOver={dropSlug !== undefined ? makeDragOver(dropSlug) : undefined}
        onDragLeave={dropSlug !== undefined ? handleDragLeave(dropSlug) : undefined}
        onDrop={dropSlug !== undefined ? makeDropHandler(dropSlug as MailboxFolderRef) : undefined}
        style={isDropTarget ? { outline: '1px dashed var(--accent, #f59f3c)' } : undefined}
      >
        <span className="vslot">
          {typeof count === 'number' && count > 0 && (
            <span className="vcount" data-testid={`folder-count-${dropSlug ?? folderRef}`}>
              {count}
            </span>
          )}
        </span>
        <span className="vlabel">{label}</span>
      </button>
    );
  };

  // ---- Flyout labeled nav (rendered only when expanded) --------------------
  // The full labeled navigation (Mailbox section + items, Folders section +
  // create `+`, user folders, Connections accordion). Folder ROWS here use
  // `flyout-`-prefixed testids so they don't collide with the rail's
  // `folder-<id>` / `user-folder-<slug>` (the rail owns those; collision would
  // break existing getBy* selection tests with a duplicate-testid error). The
  // create button + connection rows appear ONLY here, so they keep their
  // existing testids without collision.
  const renderFlyoutNav = () => (
    <>
      <div className="section-label">
        <span className="section-label-text">Mailbox</span>
      </div>
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
            data-testid={item.id ? `flyout-folder-${item.id}` : `flyout-folder-${item.label.toLowerCase()}`}
            className={className}
            disabled={!item.enabled}
            aria-current={active ? 'true' : undefined}
            onClick={() => {
              if (isFolder) selectFolderAndCollapse(item.id as MailboxFolder);
              else setRailExpanded(false);
            }}
            onDragOver={isFolder && dropSlug ? makeDragOver(dropSlug) : undefined}
            onDragLeave={isFolder && dropSlug ? handleDragLeave(dropSlug) : undefined}
            onDrop={isFolder && dropSlug ? makeDropHandler(dropSlug as MailboxFolderRef) : undefined}
            style={isDropTarget ? { outline: '1px dashed var(--accent, #f59f3c)' } : undefined}
          >
            <span className="icon" aria-hidden="true">
              {item.icon}
            </span>
            <span className="nav-label">{item.label}</span>
            {typeof count === 'number' && count > 0 && (
              <span className="count" data-testid={`flyout-folder-count-${item.id}`}>
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
      <div className="section-label section-label--folders">
        <span className="section-label-text">Folders</span>
        {onCreateFolder && (
          <button
            type="button"
            className="folder-create-btn"
            data-testid="folder-create-btn"
            onClick={onCreateFolder}
            aria-label="New folder"
            title="New folder"
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
            data-testid={`flyout-user-folder-${uf.slug}`}
            className={['nav-item', active ? 'active' : '', isDropTarget ? 'drop-target' : '']
              .filter(Boolean)
              .join(' ')}
            aria-current={active ? 'true' : undefined}
            onClick={() => selectFolderAndCollapse(uf.slug)}
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
            <span className="nav-label">{uf.displayName}</span>
          </button>
        );
      })}
      {userFolders.length === 0 && (
        <div className="folders-empty-hint" data-testid="folders-empty-hint">
          {onCreateFolder ? 'Click + to create one' : 'No custom folders yet'}
        </div>
      )}

      <div className="section-label">
        <span className="section-label-text">Connections</span>
      </div>
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
            <span className="icon" aria-hidden="true">{expanded[s.id] ? '▾' : '▸'}</span>
            <span className="nav-label">{s.label}</span>
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
                  onClick={() => {
                    onSelectConnection?.({ sessionType: s.id, protocol: p.id });
                    setRailExpanded(false);
                  }}
                >
                  <span className="nav-label">{p.label}</span>
                  {!p.built && <span className="v01-badge">soon</span>}
                </button>
              );
            })}
        </div>
      ))}
    </>
  );

  return (
    <>
      {/* Collapsed vertical-text rail — ALWAYS in the grid (never absolute), so
          expanding never shifts the other panes (tuxlink-813d D3). */}
      <nav
        className="sidebar"
        data-testid="folder-sidebar"
        aria-label="Mailbox and connections"
        ref={railRef}
      >
        {/* Rail expand toggle — CSS-hidden at desktop; in compact it opens the
            labeled flyout over the message list (tuxlink-h7q7 / tuxlink-813d). */}
        <button
          type="button"
          className="rail-expand-btn"
          data-testid="rail-expand-btn"
          aria-expanded={railExpanded}
          aria-label={railExpanded ? 'Collapse folder labels' : 'Expand folder labels'}
          onClick={() => setRailExpanded((x) => !x)}
        >
          <span aria-hidden="true">{railExpanded ? '⟨' : '☰'}</span>
        </button>

        {MAILBOX_ITEMS.map((item) => {
          const isFolder = item.id !== undefined && item.enabled;
          const active = isFolder && item.id === selectedFolder;
          const count = item.id ? counts[item.id] : undefined;
          const testId = item.id ? `folder-${item.id}` : `folder-${item.label.toLowerCase()}`;
          // Disabled (non-folder) system items render as a flat, non-selecting
          // tab; today every MAILBOX_ITEM is enabled, but keep the guard.
          return renderRailTab(
            item.label,
            testId,
            item.label,
            isFolder ? (item.id as MailboxFolderRef) : undefined,
            !!active,
            count,
            isFolder ? item.id : undefined,
          );
        })}

        {userFolders.map((uf) =>
          renderRailTab(
            uf.slug,
            `user-folder-${uf.slug}`,
            uf.displayName,
            uf.slug,
            uf.slug === selectedFolder,
            undefined,
            uf.slug,
            (e) => {
              if (onFolderContextMenu) {
                e.preventDefault();
                onFolderContextMenu(uf.slug, e.clientX, e.clientY);
              }
            },
          ),
        )}
      </nav>

      {/* Expanded flyout — a SEPARATE absolutely-positioned overlay over the
          message list, with a scrim. The rail above stays in the grid. */}
      {railExpanded && (
        <>
          <div
            className="sidebar-scrim"
            data-testid="sidebar-scrim"
            onClick={() => setRailExpanded(false)}
          />
          <nav
            className="sidebar-flyout"
            data-testid="sidebar-flyout"
            aria-label="Folders and connections"
            ref={flyoutRef}
          >
            {renderFlyoutNav()}
          </nav>
        </>
      )}
    </>
  );
});
