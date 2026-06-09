// Virtualized message list — the left pane of the Mock D shell.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.2, §4.2
// bd issue: tuxlink-zsm (Task 12); rebuilt to Mock D under tuxlink-yd4.
//
// ROW DESIGN (tuxlink-yd4, 2026-05-20): the rows match the approved mock's
// `.row` (3-line grid), ported verbatim:
//   line 1: unread-dot + sender              | date (right)
//   line 2: [form-tag] subject               | size (right)
//   line 3: preview snippet (ellipsised, spans both columns)
// Read rows (no `.unread`) are dimmer/lighter (CSS keys off `:not(.unread)`).
// Selecting a row calls `onSelect(id)` — selection state only; it does NOT
// remount/route the shell (no-full-view-swap invariant, spec §4.2).
// Virtualization via react-virtuoso (rows tested via the exported `MessageRow`;
// the real Virtuoso renders into a zero-height scroller under jsdom).

import React, { memo, useCallback, useMemo } from 'react';
import { Virtuoso } from 'react-virtuoso';
import type { MailboxFolderRef, MessageMeta, UserFolder } from './types';
import { MessageContextMenu } from './MessageContextMenu';
import { DEFAULT_SORT_STATE, type SortState, sortMessages } from './messageSort';
import { MessageListSortControl } from './MessageListSortControl';
import { MessageBulkBar } from './MessageBulkBar';

/// Empty-folder copy (spec §5.2).
export const EMPTY_FOLDER_COPY =
  'No messages yet. Press F5 or Session → Connect to check for new mail.';

/// "Not connected" empty state for an offline/unconfigured backend (spec
/// §1.1 / §3.1 — NotConfigured renders as a calm empty state, not an error).
export const NOT_CONNECTED_COPY =
  'Not connected. Complete setup or connect to the CMS to load mail.';

/// Compact, Mail.app-style smart date for the row's date column, matching the
/// mock literally: `HH:MM` today, "Yesterday", "N days ago" within a week, then
/// the calendar date. Computed in UTC (emcomm — the day boundary is UTC, never
/// local). `now` is injectable for deterministic tests. Falls back to the raw
/// string when unparseable so a malformed date never blanks the row.
export function formatRowDate(iso: string, now: Date = new Date()): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  const startOfUtcDay = (x: Date) => Date.UTC(x.getUTCFullYear(), x.getUTCMonth(), x.getUTCDate());
  const dayMs = 24 * 60 * 60 * 1000;
  const diffDays = Math.round((startOfUtcDay(now) - startOfUtcDay(d)) / dayMs);

  if (diffDays <= 0) {
    // Today (or a clock-skew future timestamp) → time of day (UTC clock).
    return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
  }
  if (diffDays === 1) return 'Yesterday';
  if (diffDays < 7) return `${diffDays} days ago`;
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;
}

/// Full compact UTC label (`YYYY-MM-DD HH:MMZ`). Retained as an exported util
/// (reading-pane / tooltips / tests) though the row now uses `formatRowDate`.
export function formatListDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  return (
    `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}Z`
  );
}

/// Human-readable size; empty string when zero. Used for the row's `.size`.
export function formatSize(bytes: number): string {
  if (!bytes || bytes <= 0) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/// The single correspondent shown on a row. For Sent/Outbox the recipient(s)
/// are salient; everywhere else (Inbox/Drafts/Deleted) the sender is.
export function correspondentLabel(msg: MessageMeta, folder: MailboxFolderRef): string {
  if (folder === 'sent' || folder === 'outbox') {
    return msg.to.length > 0 ? msg.to.join(', ') : msg.from;
  }
  // System inbox/drafts/deleted/archive AND any user-folder slug: show `from`.
  return msg.from;
}

/// A range within a rendered field that should be highlighted with <mark>.
/// `field` names the prop being highlighted; `start`/`end` are character offsets
/// (exclusive end) into the displayed string. Absent → no highlight.
export interface HighlightRange {
  field: 'subject' | 'preview';
  start: number;
  end: number;
}

/// Render a text string with matched ranges wrapped in <mark> elements.
/// When ranges is empty/absent the text is returned as a plain string node.
function applyHighlights(text: string, ranges: HighlightRange[], field: 'subject' | 'preview'): React.ReactNode {
  const fieldRanges = ranges.filter((r) => r.field === field);
  if (fieldRanges.length === 0) return text;

  // Merge + sort ranges by start position.
  const sorted = [...fieldRanges].sort((a, b) => a.start - b.start);
  const nodes: React.ReactNode[] = [];
  let cursor = 0;
  for (const { start, end } of sorted) {
    if (start > cursor) nodes.push(text.slice(cursor, start));
    nodes.push(<mark key={`${field}-${start}`}>{text.slice(start, end)}</mark>);
    cursor = end;
  }
  if (cursor < text.length) nodes.push(text.slice(cursor));
  return <>{nodes}</>;
}

export interface MessageRowProps {
  message: MessageMeta;
  folder: MailboxFolderRef;
  /// True when this row's message is the open/reading-pane message (was `selected`).
  isOpen: boolean;
  /// True when this row is part of the multi-select selection set (tuxlink-etxt Task 8).
  inSelection: boolean;
  /// Unified click handler; parent resolves Ctrl/Shift modifiers into selection
  /// set changes or a plain open. Replaces the direct `onSelect` on each row.
  onRowClick: (id: string, mods: { ctrl: boolean; shift: boolean }) => void;
  /// Direct keyboard-select handler (Enter/Space → open). Task 9 may update this.
  onSelect: (id: string) => void;
  /// Highlight ranges for this row (from a search result). Absent → no highlights.
  matchHighlight?: HighlightRange[];
  /// When true and message.folder is set, render a folder badge inline-left of
  /// the subject (cross-folder search mode, spec §7.2).
  showFolderTag?: boolean;
  /// Right-click handler (tuxlink-ejph). Receives the click event + the
  /// message so the parent can position a context menu at the cursor.
  /// Absent → browser default context menu (no overlay rendered).
  onContextMenu?: (e: React.MouseEvent, message: MessageMeta) => void;
  /// U-key read/unread toggle (tuxlink-etxt Task 13). Called with the message
  /// id, the source folder, and the target read value (true = mark read,
  /// false = mark unread). Optional — no-op when absent so direct MessageRow
  /// render tests don't need to supply it.
  onRowSetReadState?: (id: string, folder: MailboxFolderRef, read: boolean) => void;
}

/// Custom DataTransfer MIME for tuxlink message drags (tuxlink-ejph). The
/// payload is JSON `{ id, folder }` so the drop target knows both pieces.
/// Distinct MIME so we don't conflict with browser drags of text/links.
export const TUXLINK_DRAG_MIME = 'application/x-tuxlink-message';

/// One Mock D list row (3-line `.row`). Pure presentation + click / Enter →
/// `onSelect(id)`. Exported for direct unit testing.
/// tuxlink-sndh: wrapped in React.memo so a parent re-render (e.g. modem-status
/// tick, search keystroke, status poll) doesn't repaint every virtuoso row.
/// Effective only when callers stabilize callback props with useCallback.
export const MessageRow = memo(function MessageRow({ message, folder, isOpen, inSelection, onRowClick, onSelect: _onSelect, matchHighlight, showFolderTag, onContextMenu, onRowSetReadState }: MessageRowProps) {
  // tuxlink-sndh: memoize per-row derived data so it's reused across renders
  // when the row's own props haven't changed.
  const size = useMemo(() => formatSize(message.bodySize), [message.bodySize]);
  // tuxlink-268k (Codex P2): `formatRowDate` is time-dependent — it reads
  // `new Date()` to derive 'Today' / 'Yesterday' / 'N days ago'. Keying a
  // useMemo on just `message.date` would freeze the label across a UTC day
  // boundary; an app left open past midnight would keep showing yesterday's
  // labels. Don't memoize this — the call is cheap (one Date + locale fmt)
  // and we need it to track wall-clock time.
  const dateLabel = formatRowDate(message.date);
  const correspondent = useMemo(
    () => correspondentLabel(message, folder),
    [message, folder],
  );
  const highlights = matchHighlight ?? [];
  const subjectNode = useMemo(
    () => applyHighlights(message.subject, highlights, 'subject'),
    [message.subject, highlights],
  );
  const previewNode = useMemo(
    () => (message.preview ? applyHighlights(message.preview, highlights, 'preview') : null),
    [message.preview, highlights],
  );
  // The row's effective source-folder for drag operations: the message's own
  // folder if present (cross-folder search hits) else the list's active folder.
  const srcFolder = (message.folder as string | undefined) ?? (folder as string);
  return (
    <div
      role="row"
      aria-selected={isOpen}
      data-testid={`message-row-${message.id}`}
      className={['row', message.unread ? 'unread' : '', isOpen ? 'selected' : '', inSelection ? 'in-selection' : '']
        .filter(Boolean)
        .join(' ')}
      onClick={(e) => onRowClick(message.id, { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey })}
      onContextMenu={(e) => {
        if (onContextMenu) {
          e.preventDefault();
          onContextMenu(e, message);
        }
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          // Route through the plain-click path so Enter clears any selection
          // set (same behaviour as a bare mouse click, which goes through
          // onRowClick with ctrl:false/shift:false → clears set + opens).
          onRowClick(message.id, { ctrl: false, shift: false });
        } else if (e.key === ' ') {
          e.preventDefault();
          onRowClick(message.id, { ctrl: true, shift: false });  // Space toggles selection (grid/listbox semantic)
        } else if (e.key === 'u' || e.key === 'U') {
          e.preventDefault();
          // Toggle: a currently-unread message becomes read (read=true); a read one becomes unread.
          // message.unread already equals the desired `read` value, by design.
          onRowSetReadState?.(message.id, srcFolder as MailboxFolderRef, message.unread);
        }
      }}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(
          TUXLINK_DRAG_MIME,
          JSON.stringify({ id: message.id, folder: srcFolder }),
        );
        e.dataTransfer.effectAllowed = 'move';
      }}
      tabIndex={0}
    >
      {/* line 1 — sender (with unread dot) + date */}
      <div className="from" data-testid="row-correspondent">
        {message.unread && <span className="unread-dot" data-testid="row-unread-dot" aria-hidden="true" />}
        <span className="from-text">{correspondent}</span>
      </div>
      <div className="date" data-testid="row-date">
        {dateLabel}
      </div>

      {/* line 2 — [folder-tag] [form-tag] subject + size */}
      <div className="subject">
        {showFolderTag && message.folder && (
          <span className="folder-tag" data-testid="row-folder-tag">
            {message.folder}
          </span>
        )}
        {message.formTag && (
          <span className="form-tag" data-testid="row-form-tag">
            {message.formTag}
          </span>
        )}
        <span className="subject-text" data-testid="row-subject">
          {subjectNode}
        </span>
        {size && (
          <span className="size" data-testid="row-size">
            {size}
          </span>
        )}
      </div>

      {/* line 3 — preview snippet (omitted when absent) */}
      {message.preview && (
        <div className="preview" data-testid="row-preview">
          {previewNode}
        </div>
      )}
    </div>
  );
});

export interface MessageListProps {
  folder: MailboxFolderRef;
  messages: MessageMeta[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  /// When true and the list is empty, show the "not connected" copy instead
  /// of the generic empty-folder copy (backend offline / NotConfigured).
  notConnected?: boolean;
  /// Per-message highlight ranges from a search result (keyed by message id).
  /// Passed through to each MessageRow. Absent → no highlights.
  matchHighlights?: Record<string, HighlightRange[]>;
  /// When true, rows render a folder badge when message.folder is set
  /// (cross-folder search mode, spec §7.2).
  showFolderTag?: boolean;
  /// Current sort state for the list. Default `{ key: 'date', direction: 'desc' }`
  /// (matches the backend's date-desc baseline so the no-prop case is
  /// "newest first").
  sortState?: SortState;
  /// User picked a new sort key or direction in the header control. When
  /// absent the control is hidden (the list still sorts by `sortState` for
  /// callers that want to drive sort externally without exposing the picker).
  onSortStateChange?: (state: SortState) => void;
  /// Operator's user folders, shown in the right-click context menu's
  /// Move-to list (tuxlink-ejph). Absent → no user-folder rows; the menu
  /// still shows Inbox/Sent/Archive.
  userFolders?: UserFolder[];
  /// Right-click → Move-to handler (tuxlink-ejph). Receives the message,
  /// the source folder (so the backend can find the file — the row's own
  /// `message.folder` takes precedence for cross-folder search results),
  /// and the destination folder slug.
  onMoveMessage?: (id: string, fromFolder: MailboxFolderRef, toFolder: MailboxFolderRef) => void;
  /// Right-click → Archive handler. Distinct from `onMoveMessage(_, _, 'archive')`
  /// so callers can stash Archive shortcuts (e.g. for telemetry) without
  /// instrumenting the generic move path.
  onArchiveMessage?: (id: string, fromFolder: MailboxFolderRef) => void;
  /// The multi-select selection set (tuxlink-etxt Task 8). Optional; defaults
  /// to an empty set so AppShell compiles without change until Task 11 wires
  /// the real state. ReadonlySet allows the module-level EMPTY_SELECTION
  /// constant to satisfy the default without allocating per render.
  selectedIds?: ReadonlySet<string>;
  /// Called whenever the selection set should change (Ctrl/Shift+click). Optional
  /// no-op default mirrors the `selectedIds` default (safe for unupgraded callers).
  onSelectionChange?: (next: Set<string>) => void;
  /// Mark the given set of messages read or unread (tuxlink-etxt Task 10).
  /// Optional — Task 11 wires the real AppShell handler; the bulk bar's `?.`
  /// guard keeps AppShell compiling in the interim.
  onBulkSetReadState?: (ids: Set<string>, read: boolean) => void;
  /// Move the given set of messages to a destination folder (tuxlink-l80q).
  /// Drives the bulk bar's Move ▾ and the selection-mode context menu.
  onBulkMove?: (ids: Set<string>, to: MailboxFolderRef) => void;
  /// Archive the given set of messages (tuxlink-l80q). Drives the bulk bar's
  /// Archive button and the selection-mode context menu's Archive item.
  onBulkArchive?: (ids: Set<string>) => void;
  /// Single-message read/unread toggle — context-menu and U-key (tuxlink-etxt
  /// Tasks 12 + 13). Optional so existing callers compile without change.
  onSetReadState?: (id: string, folder: MailboxFolderRef, read: boolean) => void;
}

/// Stable empty-selection default so the no-selection caller (pre-Task-11) does
/// not allocate a new Set each render and churn MessageRow's memo.
const EMPTY_SELECTION: ReadonlySet<string> = new Set<string>();

/// The list pane. Renders the mock's `.rows-pane` as its root (the 420px left
/// column of `.panes`); Virtuoso scrolls inside it.
export function MessageList({
  folder,
  messages,
  selectedId,
  onSelect,
  notConnected = false,
  matchHighlights,
  showFolderTag,
  sortState = DEFAULT_SORT_STATE,
  onSortStateChange,
  userFolders,
  onMoveMessage,
  onArchiveMessage,
  selectedIds = EMPTY_SELECTION,
  onSelectionChange = () => {},
  onBulkSetReadState,
  onBulkMove,
  onBulkArchive,
  onSetReadState,
}: MessageListProps) {
  // Sort client-side so changing modes doesn't require a backend re-fetch.
  // Memo keyed on (messages, sortState, folder) — folder affects sender-* in
  // sent/outbox where the key is the recipient, not the sender.
  const sortedMessages = React.useMemo(
    () => sortMessages(messages, sortState, folder),
    [messages, sortState, folder],
  );

  // Multi-select anchor + row click handler (tuxlink-etxt Task 8).
  // anchorRef tracks the last Ctrl+clicked row for Shift+click range selection.
  const anchorRef = React.useRef<string | null>(null);
  const onRowClick = useCallback(
    (id: string, mods: { ctrl: boolean; shift: boolean }) => {
      if (mods.shift && anchorRef.current) {
        const ids = sortedMessages.map((m) => m.id);
        const a = ids.indexOf(anchorRef.current);
        const b = ids.indexOf(id);
        if (a !== -1 && b !== -1) {
          const [lo, hi] = a < b ? [a, b] : [b, a];
          onSelectionChange(new Set(ids.slice(lo, hi + 1)));
          return;
        }
      }
      if (mods.ctrl) {
        const next = new Set(selectedIds);
        if (next.has(id)) next.delete(id); else next.add(id);
        anchorRef.current = id;
        onSelectionChange(next);
        return;
      }
      // Plain click: open the message and clear any selection set.
      anchorRef.current = id;
      if (selectedIds.size > 0) onSelectionChange(new Set());
      onSelect(id);
    },
    [sortedMessages, selectedIds, onSelectionChange, onSelect],
  );

  // Right-click context menu state (tuxlink-ejph). Wires onContextMenu on
  // each row to a positioned overlay. Only mounted when handlers are
  // supplied — absence acts as feature-flag for tests that don't exercise
  // the menu.
  const [ctxMenu, setCtxMenu] = React.useState<{
    message: MessageMeta;
    x: number;
    y: number;
    // tuxlink-l80q: true when the right-clicked row was part of the selection
    // set at open time → the menu acts on ALL selected messages. Captured at
    // open (not derived live) so the out-of-selection reset below can't flip
    // an already-open menu back to single mode.
    selectionMode: boolean;
  } | null>(null);
  // tuxlink-sndh: stabilize the callback so the memoized MessageRow can
  // skip re-render when nothing else about the row's props changed.
  const ctxAvailable = Boolean(onMoveMessage || onArchiveMessage);
  // tuxlink-l80q: OS convention — right-clicking a row already in the selection
  // acts on the whole selection; right-clicking a row OUTSIDE the selection
  // resets the selection to that single row and acts single-target.
  const onContextMenu = useCallback(
    (e: React.MouseEvent, message: MessageMeta) => {
      const inSelection = selectedIds.size > 0 && selectedIds.has(message.id);
      // OS convention (Codex P2): right-clicking a row OUTSIDE an existing
      // selection resets the selection to that single row — which highlights it
      // — and the menu then acts single-target. With no prior selection,
      // right-click leaves the selection untouched (no spurious 1-row select).
      if (!inSelection && selectedIds.size > 0) onSelectionChange(new Set([message.id]));
      setCtxMenu({ message, x: e.clientX, y: e.clientY, selectionMode: inSelection });
    },
    [selectedIds, onSelectionChange],
  );
  const rowContextMenu = ctxAvailable ? onContextMenu : undefined;
  // The source folder is the row's own message.folder when present
  // (cross-folder search hits) and falls back to the list's active folder
  // otherwise. The Tauri backend uses this as the `from` arg for the move.
  const ctxSourceFolder = ctxMenu
    ? ((ctxMenu.message.folder as MailboxFolderRef | undefined) ?? folder)
    : folder;

  return (
    <div className="rows-pane" data-testid="rows-pane">
      {(onSortStateChange || selectedIds.size > 0) && (
        <div className="rows-pane-header" data-testid="rows-pane-header">
          {selectedIds.size > 0 ? (
            <MessageBulkBar
              count={selectedIds.size}
              currentFolder={folder}
              userFolders={userFolders ?? []}
              onMarkRead={() => onBulkSetReadState?.(new Set(selectedIds), true)}
              onMarkUnread={() => onBulkSetReadState?.(new Set(selectedIds), false)}
              onArchive={() => onBulkArchive?.(new Set(selectedIds))}
              onMove={(to) => onBulkMove?.(new Set(selectedIds), to)}
              onClear={() => onSelectionChange(new Set())}
            />
          ) : (
            onSortStateChange && <MessageListSortControl value={sortState} onChange={onSortStateChange} />
          )}
        </div>
      )}
      {sortedMessages.length === 0 ? (
        <div className="message-list message-list-empty" data-testid="message-list-empty">
          {notConnected ? NOT_CONNECTED_COPY : EMPTY_FOLDER_COPY}
        </div>
      ) : (
        <div className="message-list" data-testid="message-list">
          <Virtuoso
            data={sortedMessages}
            computeItemKey={(_index, msg) => msg.id}
            itemContent={(_index, msg) => (
              <MessageRow
                message={msg}
                folder={folder}
                isOpen={msg.id === selectedId}
                inSelection={selectedIds.has(msg.id)}
                onRowClick={onRowClick}
                onSelect={onSelect}
                matchHighlight={matchHighlights?.[msg.id]}
                showFolderTag={showFolderTag}
                onContextMenu={rowContextMenu}
                onRowSetReadState={onSetReadState}
              />
            )}
          />
        </div>
      )}
      {ctxMenu && (
        <MessageContextMenu
          message={ctxMenu.message}
          folder={ctxSourceFolder}
          x={ctxMenu.x}
          y={ctxMenu.y}
          userFolders={userFolders ?? []}
          selectionCount={ctxMenu.selectionMode ? selectedIds.size : undefined}
          onSetReadState={(read) => {
            if (ctxMenu.selectionMode) onBulkSetReadState?.(new Set(selectedIds), read);
            else onSetReadState?.(ctxMenu.message.id, ctxSourceFolder, read);
          }}
          onMoveTo={(to) => {
            if (ctxMenu.selectionMode) onBulkMove?.(new Set(selectedIds), to);
            else onMoveMessage?.(ctxMenu.message.id, ctxSourceFolder, to);
          }}
          onArchive={() => {
            if (ctxMenu.selectionMode) onBulkArchive?.(new Set(selectedIds));
            else onArchiveMessage?.(ctxMenu.message.id, ctxSourceFolder);
          }}
          onClose={() => setCtxMenu(null)}
        />
      )}
    </div>
  );
}
