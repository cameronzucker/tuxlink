/**
 * RenameFolderDialog — inline modal for renaming a user folder (tuxlink-ejph,
 * spec §6).
 *
 * Display name only; the slug stays stable so on-disk messages don't churn
 * (spec §3.1). Backend rejection (reserved name, validation) surfaces as an
 * inline error.
 */

import { useEffect, useState } from 'react';
import { useRenameUserFolder } from './useUserFolders';
import type { UserFolder } from './types';

export interface RenameFolderDialogProps {
  /// The folder being renamed. `null` closes the dialog; `displayName` is
  /// used to seed the input on open.
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

  // Reset state every time a new folder is rolled in (open).
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
      className="modal-backdrop"
      role="presentation"
      data-testid="rename-folder-backdrop"
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
        aria-label="Rename folder"
        data-testid="rename-folder-dialog"
        style={dialogStyle}
      >
        <h3 style={{ margin: '0 0 12px', fontSize: 14, fontWeight: 600 }}>Rename folder</h3>
        <label htmlFor="rename-folder-name" style={labelStyle}>
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
          style={inputStyle}
        />
        <div style={{ fontSize: 11, color: 'var(--text-faint, #5d6975)', marginTop: 4 }}>
          3–40 characters. The slug stays stable; only the display name updates.
        </div>
        {error && (
          <div data-testid="rename-folder-error" role="alert" style={errorStyle}>
            {error}
          </div>
        )}
        <div style={actionsStyle}>
          <button type="button" onClick={onClose} data-testid="rename-folder-cancel" style={btnStyle}>
            Cancel
          </button>
          <button
            type="submit"
            disabled={rename.isPending || unchanged || name.trim().length < 3}
            data-testid="rename-folder-save"
            style={{ ...btnStyle, borderColor: 'var(--success, #5dd6a0)', color: 'var(--success, #5dd6a0)' }}
          >
            {rename.isPending ? 'Saving…' : 'Save'}
          </button>
        </div>
      </form>
    </div>
  );
}

const dialogStyle: React.CSSProperties = {
  background: 'var(--surface, #141c23)',
  color: 'var(--text, #e4ebf2)',
  border: '1px solid var(--border-strong, #2c3744)',
  borderRadius: 8,
  padding: '20px 24px',
  width: 380,
  boxShadow: '0 12px 40px rgba(0, 0, 0, 0.7)',
  fontFamily: 'inherit',
};
const labelStyle: React.CSSProperties = {
  display: 'block',
  fontSize: 12,
  color: 'var(--text-dim, #94a0ad)',
  marginBottom: 4,
};
const inputStyle: React.CSSProperties = {
  background: 'var(--bg, #0d1318)',
  border: '1px solid var(--border-strong, #2c3744)',
  color: 'inherit',
  fontFamily: 'inherit',
  fontSize: 13,
  padding: '6px 10px',
  width: '100%',
  borderRadius: 4,
  outline: 'none',
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
