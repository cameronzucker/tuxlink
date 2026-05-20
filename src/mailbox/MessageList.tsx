// Virtualized message list (center region of the app shell).
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.2, §4.2
// bd issue: tuxlink-zsm (Task 12); restructured under tuxlink-cbz (fidelity).
//
// ROW DESIGN (tuxlink-cbz, 2026-05-20): the original Express-style columnar
// row (time·From·To·Subject·#·size) overflowed the narrow list pane and did
// not match mock-d. Per operator decision (2026-05-20) the rows now use the
// mock-d COMPACT 2-line layout: an amber unread dot in a fixed gutter, then
// line 1 = correspondent + UTC time, line 2 = subject + attachment marker.
// Unread rows are bold (via CSS). Selecting a row calls `onSelect(id)` — it
// updates SELECTION STATE only; it does NOT remount/route the shell (the
// no-full-view-swap invariant, spec §4.2). Virtualization via react-virtuoso.
//
// Testability: the per-row presentation (`MessageRow`) and the formatters are
// exported and unit-tested directly. react-virtuoso renders into a
// zero-height scroller under jsdom, so list-level tests assert the empty
// state + that the component mounts; row rendering is verified via
// `MessageRow`.

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

/// Human-readable size; empty string when zero. Retained as an exported util
/// (reading-pane / tooltips) though the compact row no longer shows a size
/// column (mock-d minimalism, operator decision 2026-05-20).
export function formatSize(bytes: number): string {
  if (!bytes || bytes <= 0) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/// The single correspondent shown on a compact row. For Sent/Outbox the
/// recipient(s) are salient; everywhere else (Inbox/Drafts/Deleted) the sender
/// is. Unlike the prior `toColumnLabel`, Inbox ALWAYS shows the sender even
/// when `to` is populated — the reader is looking at who wrote to them.
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

/// One compact list row (mock-d). A fixed-width gutter holds the amber unread
/// dot (rendered only when unread, but the gutter reserves its width either
/// way so read/unread rows stay left-aligned). Pure presentation + a click /
/// Enter → `onSelect(id)`. Exported for direct unit testing.
export function MessageRow({ message, folder, selected, onSelect }: MessageRowProps) {
  return (
    <div
      role="row"
      aria-selected={selected}
      data-testid={`message-row-${message.id}`}
      className={['message-row', message.unread ? 'unread' : 'read', selected ? 'selected' : '']
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
      <div className="message-row__gutter" aria-hidden="true">
        {message.unread && <span className="message-row__dot" data-testid="row-unread-dot" />}
      </div>
      <div className="message-row__body">
        <div className="message-row__line message-row__line--top">
          <span className="message-row__correspondent" data-testid="row-correspondent">
            {correspondentLabel(message, folder)}
          </span>
          <span className="message-row__date" data-testid="row-date">
            {formatListDate(message.date)}
          </span>
        </div>
        <div className="message-row__line message-row__line--bottom">
          <span className="message-row__subject" data-testid="row-subject">
            {message.subject}
          </span>
          {message.hasAttachments && (
            <span
              className="message-row__attach"
              data-testid="row-attach"
              aria-label="has attachment"
              title="Has attachment"
            >
              📎
            </span>
          )}
        </div>
      </div>
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
