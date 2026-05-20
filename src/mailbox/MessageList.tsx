// Virtualized message list (center region of the app shell).
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.2, §4.2
// bd issue: tuxlink-zsm (Task 12)
//
// Columns: UTC time · From · To · Subject · `#` (attachment) · size (nonzero).
// Unread rows are bold. Selecting a row calls `onSelect(id)` — it updates
// SELECTION STATE only; it does NOT remount/route the shell (the
// no-full-view-swap invariant, spec §4.2). Virtualization via react-virtuoso.
//
// Testability: the per-row presentation (`MessageRow`) and the formatters are
// exported and unit-tested directly. react-virtuoso renders into a
// zero-height scroller under jsdom, so list-level tests assert the empty
// state + that the component mounts; row rendering is verified via
// `MessageRow` (testing-pitfalls: static tests verify logic, not the
// virtualized widget's pixel output — the live list is an M2 smoke gate).

import { Virtuoso } from 'react-virtuoso';
import type { MailboxFolder, MessageMeta } from './types';

/// Empty-folder copy (spec §5.2).
export const EMPTY_FOLDER_COPY =
  'No messages yet. Press F5 or Session → Connect to check for new mail.';

/// "Not connected" empty state for an offline/unconfigured backend (spec
/// §1.1 / §3.1 — NotConfigured renders as a calm empty state, not an error).
export const NOT_CONNECTED_COPY =
  'Not connected. Complete setup or connect to the CMS to load mail.';

/// Format an RFC 3339 UTC timestamp as a compact UTC list label
/// (`YYYY-MM-DD HH:MMZ`). Falls back to the raw string if unparseable so a
/// malformed date never blanks the row.
export function formatListDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  return (
    `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}Z`
  );
}

/// Human-readable size; empty string when zero (the size column is suppressed
/// for zero-byte rows per spec §5.2 "size (when nonzero)").
export function formatSize(bytes: number): string {
  if (!bytes || bytes <= 0) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/// The "To" column label. For Inbox, recipients are typically empty (Pat
/// degradation) and the sender is the salient party, so show the sender as a
/// fallback; for Sent/Outbox show the recipient list. Spec §2.1.
export function toColumnLabel(msg: MessageMeta, folder: MailboxFolder): string {
  if (msg.to.length > 0) return msg.to.join(', ');
  if (folder === 'inbox') return msg.from;
  return '';
}

export interface MessageRowProps {
  message: MessageMeta;
  folder: MailboxFolder;
  selected: boolean;
  onSelect: (id: string) => void;
}

/// One list row. Pure presentation + a click → `onSelect(id)`. Bold when
/// unread. Exported for direct unit testing.
export function MessageRow({ message, folder, selected, onSelect }: MessageRowProps) {
  const size = formatSize(message.bodySize);
  return (
    <div
      role="row"
      aria-selected={selected}
      data-testid={`message-row-${message.id}`}
      className={[
        'message-row',
        message.unread ? 'unread' : 'read',
        selected ? 'selected' : '',
      ]
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
      style={{ fontWeight: message.unread ? 700 : 400 }}
    >
      <span className="col-date" data-testid="row-date">
        {formatListDate(message.date)}
      </span>
      <span className="col-from" data-testid="row-from">
        {message.from}
      </span>
      <span className="col-to" data-testid="row-to">
        {toColumnLabel(message, folder)}
      </span>
      <span className="col-subject" data-testid="row-subject">
        {message.subject}
      </span>
      <span className="col-attach" data-testid="row-attach" aria-hidden={!message.hasAttachments}>
        {message.hasAttachments ? '#' : ''}
      </span>
      <span className="col-size" data-testid="row-size">
        {size}
      </span>
    </div>
  );
}

export interface MessageListProps {
  folder: MailboxFolder;
  messages: MessageMeta[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  /// When true and the list is empty, show the "not connected" copy instead
  /// of the generic empty-folder copy (backend is offline / NotConfigured).
  notConnected?: boolean;
}

export function MessageList({
  folder,
  messages,
  selectedId,
  onSelect,
  notConnected = false,
}: MessageListProps) {
  if (messages.length === 0) {
    return (
      <div className="message-list message-list-empty" data-testid="message-list-empty">
        {notConnected ? NOT_CONNECTED_COPY : EMPTY_FOLDER_COPY}
      </div>
    );
  }

  return (
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
  );
}
