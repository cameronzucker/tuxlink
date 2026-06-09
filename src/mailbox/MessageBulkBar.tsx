/// Bulk action bar shown in the message-list header slot while a selection set
/// is non-empty (tuxlink-etxt). Replaces the sort control in place — no new
/// vertical space, never floats over the reading pane.

export interface MessageBulkBarProps {
  count: number;
  onMarkRead: () => void;
  onMarkUnread: () => void;
  onClear: () => void;
}

export function MessageBulkBar({ count, onMarkRead, onMarkUnread, onClear }: MessageBulkBarProps) {
  return (
    <div className="message-bulk-bar" role="toolbar" aria-label="Selection actions" data-testid="message-bulk-bar">
      <span className="bulk-count" aria-live="polite">{count} selected</span>
      <span className="bulk-spacer" />
      <button type="button" className="bulk-btn primary" onClick={onMarkRead}>Mark read</button>
      <button type="button" className="bulk-btn" onClick={onMarkUnread}>Mark unread</button>
      <button type="button" className="bulk-btn clear" aria-label="Clear selection" onClick={onClear}>✕</button>
    </div>
  );
}
