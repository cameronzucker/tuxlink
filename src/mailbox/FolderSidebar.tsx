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
  /** FZ-M1 compact mode (tuxlink-813d). When `true`, render the collapsed
   *  vertical-text `.vtab` rail + the `☰` expand button + the absolutely-
   *  positioned labeled flyout (the compact rework). When `false` (desktop,
   *  the default), render the ORIGINAL labeled `.sidebar` nav inline — no rail,
   *  no flyout, no `☰`. Driven by `AppShell`'s `useViewport().isCompact` so
   *  there is a single source of truth (the same signal that drives the
   *  `.compact` root class); FolderSidebar never calls `useViewport` itself,
   *  keeping it a pure prop-driven component for tests. */
  compact?: boolean;
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
  compact = false,
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
  const expandBtnRef = useRef<HTMLButtonElement | null>(null);
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

  // Focus management (a11y parity with RadioDrawer): on open, focus the flyout
  // panel so keyboard users land in the navigation (not stranded on the expand
  // button); on close, return focus to the expand button. prevRailExpanded skips
  // the initial mount (same pattern as RadioDrawer.tsx).
  const prevRailExpanded = useRef(railExpanded);
  useEffect(() => {
    if (railExpanded === prevRailExpanded.current) return;
    if (railExpanded) flyoutRef.current?.focus();
    else expandBtnRef.current?.focus();
    prevRailExpanded.current = railExpanded;
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
  const renderRailTab = (opts: {
    key: string;
    testId: string;
    label: string;
    folderRef: MailboxFolderRef | undefined;
    active: boolean;
    count?: number;
    dropSlug?: string;
    onContextMenu?: (e: React.MouseEvent) => void;
  }) => {
    const { key, testId, label, folderRef, active, count, dropSlug, onContextMenu } = opts;
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

      {/* Address section (tuxlink-raez / FZ-M1 coordination). Mirrors the desktop
          Address section in the compact flyout so Contacts is reachable when the
          rail is expanded. `flyout-`-prefixed testids avoid colliding with the
          rail's `folder-<id>`; selecting collapses the flyout. */}
      <div className="section-label">
        <span className="section-label-text">Address</span>
      </div>
      {ADDRESS_ITEMS.map((item) => {
        const active = item.id === selectedFolder;
        const count = item.id === 'contacts' ? contactsCount : undefined;
        const className = ['nav-item', active ? 'active' : '', item.enabled ? '' : 'disabled']
          .filter(Boolean)
          .join(' ');
        return (
          <button
            key={item.id}
            type="button"
            data-testid={`flyout-folder-${item.id}`}
            className={className}
            disabled={!item.enabled}
            aria-current={active ? 'true' : undefined}
            onClick={() => {
              if (item.enabled) selectFolderAndCollapse(item.id);
            }}
          >
            <span className="icon" aria-hidden="true">{item.icon}</span>
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

  // ---- Desktop labeled nav (the ORIGINAL pre-rework sidebar) ---------------
  // Restored verbatim from the pre-rework version (merge-base 5f3be81). On
  // desktop the rail/flyout rework is a no-op at the CSS layer (the `☰` is
  // display:none, the `.vtab` styles are compact-only), so rendering the rail
  // there produced an unstyled, connection-less, create-folder-less sidebar
  // (the tuxlink-813d P1 regression). Desktop must render this labeled nav
  // inline instead — full `.nav-item` rows, the Folders `+`, and the
  // Connections accordion, all reachable without a `☰`. Original testids
  // (`folder-<id>`, `user-folder-<slug>`, `folder-create-btn`, `sess-<id>`,
  // `proto-<s>-<p>`) so existing desktop selectors keep resolving.
  const renderDesktopNav = () => (
    <nav
      className="sidebar"
      data-testid="folder-sidebar"
      aria-label="Mailbox and connections"
    >
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
            <span className="nav-label">{item.label}</span>
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
            <span className="nav-label">{uf.displayName}</span>
          </button>
        );
      })}
      {userFolders.length === 0 && (
        <div className="folders-empty-hint" data-testid="folders-empty-hint">
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
      <div className="section-label">
        <span className="section-label-text">Address</span>
      </div>
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
                  }}
                >
                  <span className="nav-label">{p.label}</span>
                  {!p.built && <span className="v01-badge">soon</span>}
                </button>
              );
            })}
        </div>
      ))}
    </nav>
  );

  // Desktop: render the original labeled sidebar inline. No rail, no flyout,
  // no `☰` — the compact rework does not apply here.
  if (!compact) {
    return renderDesktopNav();
  }

  return (
    <>
      {/* Collapsed vertical-text rail — ALWAYS in the grid (never absolute), so
          expanding never shifts the other panes (tuxlink-813d D3). When the
          flyout is open, the rail's duplicate folder tabs are removed from the
          tab order + a11y tree (`inert` + `aria-hidden`): focus moves into the
          flyout, which owns its own dismissal (scrim / Escape / select), so the
          rail's duplicate accessible names + `aria-current` must not linger
          (Codex a11y P2). */}
      <nav
        className="sidebar"
        data-testid="folder-sidebar"
        aria-label="Mailbox and connections"
        ref={railRef}
        inert={railExpanded ? true : undefined}
        aria-hidden={railExpanded ? 'true' : undefined}
      >
        {/* Rail expand toggle — CSS-hidden at desktop; in compact it opens the
            labeled flyout over the message list (tuxlink-h7q7 / tuxlink-813d). */}
        <button
          ref={expandBtnRef}
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
          return renderRailTab({
            key: item.label,
            testId,
            label: item.label,
            folderRef: isFolder ? (item.id as MailboxFolderRef) : undefined,
            active: !!active,
            count,
            dropSlug: isFolder ? item.id : undefined,
          });
        })}

        {userFolders.map((uf) =>
          renderRailTab({
            key: uf.slug,
            testId: `user-folder-${uf.slug}`,
            label: uf.displayName,
            folderRef: uf.slug,
            active: uf.slug === selectedFolder,
            dropSlug: uf.slug,
            onContextMenu: (e) => {
              if (onFolderContextMenu) {
                e.preventDefault();
                onFolderContextMenu(uf.slug, e.clientX, e.clientY);
              }
            },
          }),
        )}

        {/* Address pseudo-folder(s) in the compact rail (tuxlink-raez / FZ-M1
            coordination). Mirrors the desktop Address section so Contacts is
            reachable in compact mode too; the count comes from `contactsCount`,
            never the mailbox `counts` memo. Shares the `folder-<id>` testid with
            the desktop nav — they never co-render (rail is compact-only). */}
        {ADDRESS_ITEMS.map((item) =>
          renderRailTab({
            key: item.label,
            testId: `folder-${item.id}`,
            label: item.label,
            folderRef: item.enabled ? item.id : undefined,
            active: item.id === selectedFolder,
            count: item.id === 'contacts' ? contactsCount : undefined,
          }),
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
            tabIndex={-1}
          >
            {renderFlyoutNav()}
          </nav>
        </>
      )}
    </>
  );
});
