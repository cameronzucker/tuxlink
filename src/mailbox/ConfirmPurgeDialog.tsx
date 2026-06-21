/**
 * ConfirmPurgeDialog — inline confirm modal for permanent-delete actions
 * (tuxlink-wl7n Task 14).
 *
 * Mirrors `DeleteFolderDialog`'s structure + `.tux-folder-*` styling from
 * `userFolders.css`. Used for three permanent actions:
 *   - per-item "Delete permanently" from the Deleted folder
 *   - bulk "Delete permanently" from the Deleted folder
 *   - "Empty Trash" (all messages in the Deleted folder)
 *
 * Delete (move to Trash) is NOT confirmed — only permanent actions use this
 * dialog. Body copy is EXACT per the spec.
 */

import { useEffect, useState } from 'react';
import './userFolders.css';

export interface ConfirmPurgeDialogProps {
  /** Whether the dialog is visible. */
  open: boolean;
  /** How many messages will be permanently deleted (drives plural copy). */
  count: number;
  /** Called when the operator confirms. May be async; rejection is surfaced
   *  as a `.tux-folder-error` line while keeping the dialog open. */
  onConfirm: () => void | Promise<void>;
  /** Called on Cancel / × / Escape / backdrop click. */
  onCancel: () => void;
}

export function ConfirmPurgeDialog({
  open,
  count,
  onConfirm,
  onCancel,
}: ConfirmPurgeDialogProps) {
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  // Reset error whenever the dialog opens/closes so stale errors don't linger.
  useEffect(() => {
    if (open) {
      setError(null);
      setPending(false);
    }
  }, [open]);

  // Escape-to-close (mirrors DeleteFolderDialog).
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onCancel();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onCancel]);

  if (!open) return null;

  const bodyCopy =
    `Permanently delete ${count} message${count === 1 ? '' : 's'}? This cannot be undone.`;

  async function handleConfirm() {
    setError(null);
    setPending(true);
    try {
      await onConfirm();
    } catch (err) {
      const msg =
        err && typeof err === 'object' && 'message' in err && typeof err.message === 'string'
          ? err.message
          : 'Could not permanently delete the message(s).';
      setError(msg);
    } finally {
      setPending(false);
    }
  }

  return (
    <div
      className="tux-folder-backdrop"
      role="presentation"
      data-testid="purge-dialog-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        role="dialog"
        aria-label="Permanently delete messages"
        aria-modal="true"
        data-testid="purge-dialog"
        className="tux-folder-dialog"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-folder-header">
          <h3 className="tux-folder-title">Delete permanently?</h3>
          <button
            type="button"
            className="tux-folder-close"
            aria-label="Close"
            data-testid="purge-dialog-close"
            onClick={onCancel}
          >
            ×
          </button>
        </div>
        <div className="tux-folder-body" data-testid="purge-dialog-body">
          {bodyCopy}
          {error && (
            <div
              data-testid="purge-dialog-error"
              role="alert"
              className="tux-folder-error"
            >
              {error}
            </div>
          )}
        </div>
        <div className="tux-folder-actions">
          <button
            type="button"
            onClick={onCancel}
            data-testid="purge-dialog-cancel"
            className="tux-folder-btn"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={pending}
            onClick={() => void handleConfirm()}
            data-testid="purge-dialog-confirm"
            className="tux-folder-btn tux-folder-btn-danger"
          >
            {pending ? 'Deleting…' : 'Delete permanently'}
          </button>
        </div>
      </div>
    </div>
  );
}
