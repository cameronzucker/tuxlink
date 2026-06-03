/**
 * RenameFolderDialog — inline modal for renaming a user folder (tuxlink-ejph;
 * styling refactored under tuxlink-i2nr).
 *
 * Display name only; the slug stays stable so on-disk messages don't churn
 * (spec §3.1). Backend rejection (reserved name, validation) surfaces as an
 * inline error. Shares `.tux-folder-*` styling with NewFolderDialog +
 * DeleteFolderDialog.
 */

import { useEffect, useState } from 'react';
import { useRenameUserFolder } from './useUserFolders';
import type { UserFolder } from './types';
import './userFolders.css';

export interface RenameFolderDialogProps {
  folder: UserFolder | null;
  onClose: () => void;
}

export function RenameFolderDialog({ folder, onClose }: RenameFolderDialogProps) {
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const rename = useRenameUserFolder();

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
      setName(folder.displayName);
      setError(null);
    }
  }, [folder]);

  if (!folder) return null;

  function reasonFromError(err: unknown): string {
    if (err && typeof err === 'object' && 'kind' in err) {
      const e = err as { kind: string; detail?: unknown };
      if (e.kind === 'Rejected' && typeof e.detail === 'string') return e.detail;
      if (e.kind === 'NotFound') return 'Folder no longer exists.';
    }
    return 'Could not rename. Please try a different name.';
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!folder) return;
    setError(null);
    try {
      await rename.mutateAsync({ slug: folder.slug, displayName: name });
      onClose();
    } catch (err) {
      setError(reasonFromError(err));
    }
  }

  const unchanged = name.trim() === folder.displayName;

  return (
    <div
      className="tux-folder-backdrop"
      role="presentation"
      data-testid="rename-folder-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <form
        onSubmit={submit}
        role="dialog"
        aria-label="Rename folder"
        data-testid="rename-folder-dialog"
        className="tux-folder-dialog"
      >
        <div className="tux-folder-header">
          <h3 className="tux-folder-title">Rename folder</h3>
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
          <label htmlFor="rename-folder-name" className="tux-folder-field-label">
            New name
          </label>
          <input
            id="rename-folder-name"
            type="text"
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={40}
            data-testid="rename-folder-name-input"
            className="tux-folder-input"
          />
          <div className="tux-folder-help">
            3–40 characters. The slug stays stable; only the display name updates.
          </div>
          {error && (
            <div data-testid="rename-folder-error" role="alert" className="tux-folder-error">
              {error}
            </div>
          )}
        </div>
        <div className="tux-folder-actions">
          <button
            type="button"
            onClick={onClose}
            data-testid="rename-folder-cancel"
            className="tux-folder-btn"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={rename.isPending || unchanged || name.trim().length < 3}
            data-testid="rename-folder-save"
            className="tux-folder-btn tux-folder-btn-primary"
          >
            {rename.isPending ? 'Saving…' : 'Save'}
          </button>
        </div>
      </form>
    </div>
  );
}
