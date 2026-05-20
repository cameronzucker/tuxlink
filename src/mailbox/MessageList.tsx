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

import { Virtuoso } from 'react-virtuoso';
import type { MailboxFolder, MessageMeta } from './types';

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
export function correspondentLabel(msg: MessageMeta, folder: MailboxFolder): string {
  if (folder === 'sent' || folder === 'outbox') {
    return msg.to.length > 0 ? msg.to.join(', ') : msg.from;
  }
  return msg.from;
}

export interface MessageRowProps {
  message: MessageMeta;
  folder: MailboxFolder;
  selected: boolean;
  onSelect: (id: string) => void;
}

/// One Mock D list row (3-line `.row`). Pure presentation + click / Enter →
/// `onSelect(id)`. Exported for direct unit testing.
export function MessageRow({ message, folder, selected, onSelect }: MessageRowProps) {
  const size = formatSize(message.bodySize);
  return (
    <div
      role="row"
      aria-selected={selected}
      data-testid={`message-row-${message.id}`}
      className={['row', message.unread ? 'unread' : '', selected ? 'selected' : '']
        .filter(Boolean)
        .join(' ')}
      onClick={() => onSelect(message.id)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect(message.id);
        }
      }}
      tabIndex={0}
    >
      {/* line 1 — sender (with unread dot) + date */}
      <div className="from" data-testid="row-correspondent">
        {message.unread && <span className="unread-dot" data-testid="row-unread-dot" aria-hidden="true" />}
        <span className="from-text">{correspondentLabel(message, folder)}</span>
      </div>
      <div className="date" data-testid="row-date">
        {formatRowDate(message.date)}
      </div>

      {/* line 2 — [form-tag] subject + size */}
      <div className="subject">
        {message.formTag && (
          <span className="form-tag" data-testid="row-form-tag">
            {message.formTag}
          </span>
        )}
        <span className="subject-text" data-testid="row-subject">
          {message.subject}
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
          {message.preview}
        </div>
      )}
    </div>
  );
}

export interface MessageListProps {
  folder: MailboxFolder;
  messages: MessageMeta[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  /// When true and the list is empty, show the "not connected" copy instead
  /// of the generic empty-folder copy (backend offline / NotConfigured).
  notConnected?: boolean;
}

/// The list pane. Renders the mock's `.rows-pane` as its root (the 420px left
/// column of `.panes`); Virtuoso scrolls inside it.
export function MessageList({
  folder,
  messages,
  selectedId,
  onSelect,
  notConnected = false,
}: MessageListProps) {
  return (
    <div className="rows-pane" data-testid="rows-pane">
      {messages.length === 0 ? (
        <div className="message-list message-list-empty" data-testid="message-list-empty">
          {notConnected ? NOT_CONNECTED_COPY : EMPTY_FOLDER_COPY}
        </div>
      ) : (
        <div className="message-list" data-testid="message-list">
          <Virtuoso
            data={messages}
            computeItemKey={(_index, msg) => msg.id}
            itemContent={(_index, msg) => (
              <MessageRow
                message={msg}
                folder={folder}
                selected={msg.id === selectedId}
                onSelect={onSelect}
              />
            )}
          />
        </div>
      )}
    </div>
  );
}
