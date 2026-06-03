/**
 * DeleteFolderDialog — inline modal for deleting a user folder (tuxlink-ejph,
 * spec §6 D6).
 *
 * Radio choice for what to do with messages inside:
 *  - move_to_inbox (safe default)
 *  - move_to_archive
 *  - delete (permanent)
 *
 * Mirrors the mock's "What should happen to it?" picker. After a successful
 * delete, the folder is gone from the registry + cascade applied; cache
 * invalidation in the mutation hook drops the sidebar row.
 */

import { useEffect, useState } from 'react';
import { useDeleteUserFolder, type DeleteFolderAction } from './useUserFolders';
import type { UserFolder } from './types';

export interface DeleteFolderDialogProps {
  /// The folder being deleted. `null` closes the dialog.
  folder: UserFolder | null;
  /// Best-effort message count inside the folder, for the dialog headline.
  /// Absent → headline omits the count (still safe).
  messageCount?: number;
  onClose: () => void;
  /// Fired after a successful delete with the cascade action chosen, so
  /// callers can (e.g.) navigate away if they were viewing the now-gone
  /// folder.
  onDeleted?: (slug: string, action: DeleteFolderAction) => void;
}

export function DeleteFolderDialog({
  folder,
  messageCount,
  onClose,
  onDeleted,
}: DeleteFolderDialogProps) {
  const [action, setAction] = useState<DeleteFolderAction>('move_to_inbox');
  const [error, setError] = useState<string | null>(null);
  const del = useDeleteUserFolder();

  useEffect(() => {
    if (!folder) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [folder, onClose]);

  useEffect(() => {
    if (folder) {
      setAction('move_to_inbox');
      setError(null);
    }
  }, [folder]);

  if (!folder) return null;

  function reasonFromError(err: unknown): string {
    if (err && typeof err === 'object' && 'kind' in err) {
      const e = err as { kind: string; detail?: unknown };
      if (e.kind === 'Rejected' && typeof e.detail === 'string') return e.detail;
    }
    return 'Could not delete the folder.';
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!folder) return;
    setError(null);
    try {
      await del.mutateAsync({ slug: folder.slug, onMessages: action });
      onDeleted?.(folder.slug, action);
      onClose();
    } catch (err) {
      setError(reasonFromError(err));
    }
  }

  const countCopy = typeof messageCount === 'number' && messageCount > 0
    ? ` (${messageCount} message${messageCount === 1 ? '' : 's'} inside)`
    : '';

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      data-testid="delete-folder-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0, 0, 0, 0.55)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 1000,
      }}
    >
      <form
        onSubmit={submit}
        role="dialog"
        aria-label={`Delete ${folder.displayName}`}
        data-testid="delete-folder-dialog"
        style={dialogStyle}
      >
        <h3 style={{ margin: '0 0 12px', fontSize: 14, fontWeight: 600 }}>
          Delete &ldquo;{folder.displayName}&rdquo;?
        </h3>
        <p style={{ fontSize: 12, color: 'var(--text-dim, #94a0ad)', margin: '0 0 12px' }}>
          What should happen to its messages{countCopy}?
        </p>
        <div role="radiogroup" aria-label="Cascade action">
          <Row
            id="del-mv-inbox"
            value="move_to_inbox"
            active={action}
            label="Move messages to Inbox"
            desc="Safe default. Folder is deleted; the messages reappear in Inbox."
            onSelect={() => setAction('move_to_inbox')}
          />
          <Row
            id="del-mv-archive"
            value="move_to_archive"
            active={action}
            label="Move messages to Archive"
            desc="For long-term keep-but-out-of-sight."
            onSelect={() => setAction('move_to_archive')}
          />
          <Row
            id="del-rm"
            value="delete"
            active={action}
            label="Delete messages too"
            desc="Permanent. Not recoverable without a backup."
            danger
            onSelect={() => setAction('delete')}
          />
        </div>
        {error && (
          <div data-testid="delete-folder-error" role="alert" style={errorStyle}>
            {error}
          </div>
        )}
        <div style={actionsStyle}>
          <button type="button" onClick={onClose} data-testid="delete-folder-cancel" style={btnStyle}>
            Cancel
          </button>
          <button
            type="submit"
            disabled={del.isPending}
            data-testid="delete-folder-confirm"
            style={{ ...btnStyle, borderColor: 'var(--error, #ee6b6b)', color: 'var(--error, #ee6b6b)' }}
          >
            {del.isPending ? 'Deleting…' : 'Delete folder'}
          </button>
        </div>
      </form>
    </div>
  );
}

interface RowProps {
  id: string;
  value: DeleteFolderAction;
  active: DeleteFolderAction;
  label: string;
  desc: string;
  danger?: boolean;
  onSelect: () => void;
}

function Row({ id, value, active, label, desc, danger, onSelect }: RowProps) {
  return (
    <label
      htmlFor={id}
      data-testid={`delete-folder-row-${value}`}
      style={{
        display: 'flex',
        alignItems: 'flex-start',
        gap: 8,
        marginBottom: 8,
        cursor: 'pointer',
      }}
    >
      <input
        type="radio"
        id={id}
        name="cascade"
        value={value}
        checked={active === value}
        onChange={onSelect}
      />
      <span>
        <span style={{ fontSize: 12, color: danger ? 'var(--error, #ee6b6b)' : 'inherit' }}>
          {label}
        </span>
        <span style={{ display: 'block', fontSize: 11, color: 'var(--text-faint, #5d6975)' }}>
          {desc}
        </span>
      </span>
    </label>
  );
}

const dialogStyle: React.CSSProperties = {
  background: 'var(--surface, #141c23)',
  color: 'var(--text, #e4ebf2)',
  border: '1px solid var(--border-strong, #2c3744)',
  borderRadius: 8,
  padding: '20px 24px',
  width: 400,
  boxShadow: '0 12px 40px rgba(0, 0, 0, 0.7)',
  fontFamily: 'inherit',
};
const errorStyle: React.CSSProperties = {
  fontSize: 12,
  color: 'var(--error, #ee6b6b)',
  marginTop: 8,
};
const actionsStyle: React.CSSProperties = {
  display: 'flex',
  gap: 8,
  justifyContent: 'flex-end',
  marginTop: 18,
};
const btnStyle: React.CSSProperties = {
  background: 'transparent',
  border: '1px solid var(--border-strong, #2c3744)',
  color: 'inherit',
  padding: '4px 10px',
  borderRadius: 4,
  fontSize: 12,
  cursor: 'pointer',
  fontFamily: 'inherit',
};
