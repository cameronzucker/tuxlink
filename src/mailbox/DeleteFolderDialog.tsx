/**
 * DeleteFolderDialog — inline modal for deleting a user folder (tuxlink-ejph;
 * styling refactored under tuxlink-i2nr).
 *
 * Radio choice for what to do with messages inside (spec §6 D6):
 *  - move_to_inbox (safe default)
 *  - move_to_archive
 *  - delete (permanent)
 *
 * Shares `.tux-folder-*` styling with NewFolderDialog + RenameFolderDialog;
 * radio rows use `.tux-folder-radio-*` which mirror SettingsPanel's option
 * rows for consistent hover behavior.
 */

import { useEffect, useState } from 'react';
import { useDeleteUserFolder, type DeleteFolderAction } from './useUserFolders';
import type { UserFolder } from './types';
import './userFolders.css';

export interface DeleteFolderDialogProps {
  folder: UserFolder | null;
  messageCount?: number;
  /// Direct-subfolder count + names for the blast-radius line (tuxlink-ka3z A8).
  /// When > 0, the dialog warns that deletion cascades to these subfolders.
  childCount?: number;
  childNames?: string[];
  onClose: () => void;
  /// Receives every slug removed (the folder + any cascaded subfolders) so the
  /// host can clear a stale selection (A5).
  onDeleted?: (removedSlugs: string[]) => void;
}

export function DeleteFolderDialog({
  folder,
  messageCount,
  childCount,
  childNames,
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
      const removed = await del.mutateAsync({ slug: folder.slug, onMessages: action });
      onDeleted?.(removed);
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
      className="tux-folder-backdrop"
      role="presentation"
      data-testid="delete-folder-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <form
        onSubmit={submit}
        role="dialog"
        aria-label={`Delete ${folder.displayName}`}
        data-testid="delete-folder-dialog"
        className="tux-folder-dialog"
      >
        <div className="tux-folder-header">
          <h3 className="tux-folder-title">
            Delete &ldquo;{folder.displayName}&rdquo;?
          </h3>
          <button
            type="button"
            className="tux-folder-close"
            aria-label="Close"
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <div className="tux-folder-body">
          {typeof childCount === 'number' && childCount > 0 && (
            <div
              data-testid="delete-folder-blast-radius"
              role="alert"
              className="tux-folder-help tux-folder-help--warn"
            >
              This also removes {childCount} subfolder{childCount === 1 ? '' : 's'}
              {childNames && childNames.length > 0 ? ` (${childNames.join(', ')})` : ''} and all
              messages they contain.
            </div>
          )}
          <div className="tux-folder-help">
            What should happen to its messages{countCopy}?
          </div>
          <div role="radiogroup" aria-label="Cascade action" className="tux-folder-radio-group">
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
            <div data-testid="delete-folder-error" role="alert" className="tux-folder-error">
              {error}
            </div>
          )}
        </div>
        <div className="tux-folder-actions">
          <button
            type="button"
            onClick={onClose}
            data-testid="delete-folder-cancel"
            className="tux-folder-btn"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={del.isPending}
            data-testid="delete-folder-confirm"
            className="tux-folder-btn tux-folder-btn-danger"
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
      className={`tux-folder-radio-row${danger ? ' tux-folder-radio-row-danger' : ''}`}
    >
      <input
        type="radio"
        id={id}
        name="cascade"
        value={value}
        checked={active === value}
        onChange={onSelect}
      />
      <span className="tux-folder-radio-text">
        <span className="tux-folder-radio-label">{label}</span>
        <span className="tux-folder-radio-help">{desc}</span>
      </span>
    </label>
  );
}
