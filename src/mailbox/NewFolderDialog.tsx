/**
 * NewFolderDialog — inline modal that creates a user folder (tuxlink-f62f).
 *
 * Opened from the sidebar's "Folders" section `+` button. The form is a
 * single text input for the display name; slug is derived backend-side.
 * Backend rejection (reserved name, duplicate, invalid chars) surfaces as
 * an inline error under the input — no toast (per the inline-UI preference).
 *
 * Esc closes; Enter submits. Matches the existing inline-overlay pattern of
 * SettingsPanel / ThemeDesigner / AboutDialog.
 */

import { useEffect, useState } from 'react';
import { useCreateUserFolder } from './useUserFolders';

export interface NewFolderDialogProps {
  open: boolean;
  onClose: () => void;
  /** Optional callback fired with the new folder's slug after a successful
   *  create. Lets callers (e.g. the Move-to picker) chain "create then move
   *  the open message into the new folder" in one operator action. */
  onCreated?: (slug: string) => void;
}

export function NewFolderDialog({ open, onClose, onCreated }: NewFolderDialogProps) {
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const create = useCreateUserFolder();

  // Esc closes (matches AboutDialog / SettingsPanel).
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  // Reset state every time the dialog opens so a previous create's error
  // doesn't bleed into a fresh attempt.
  useEffect(() => {
    if (open) {
      setName('');
      setError(null);
    }
  }, [open]);

  if (!open) return null;

  function reasonFromError(err: unknown): string {
    // UiError discriminated union (mirrors types.ts asUiError). MessageRejected
    // arrives as { kind: 'Rejected', detail: '<reason>' }.
    if (err && typeof err === 'object' && 'kind' in err) {
      const e = err as { kind: string; detail?: unknown };
      if (e.kind === 'Rejected' && typeof e.detail === 'string') return e.detail;
      if (e.kind === 'NotConfigured' && typeof e.detail === 'string') {
        return 'Backend offline — connect first, then try again.';
      }
    }
    return 'Could not create the folder. Please try a different name.';
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      const folder = await create.mutateAsync(name);
      onCreated?.(folder.slug);
      onClose();
    } catch (err) {
      setError(reasonFromError(err));
    }
  }

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      data-testid="new-folder-backdrop"
      onClick={(e) => {
        // Click on backdrop (but not on the modal itself) closes.
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
        aria-label="New folder"
        data-testid="new-folder-dialog"
        style={{
          background: 'var(--surface, #141c23)',
          color: 'var(--text, #e4ebf2)',
          border: '1px solid var(--border-strong, #2c3744)',
          borderRadius: 8,
          padding: '20px 24px',
          width: 380,
          boxShadow: '0 12px 40px rgba(0, 0, 0, 0.7)',
          fontFamily: 'inherit',
        }}
      >
        <h3 style={{ margin: '0 0 12px', fontSize: 14, fontWeight: 600 }}>New folder</h3>
        <label htmlFor="new-folder-name" style={{ display: 'block', fontSize: 12, color: 'var(--text-dim, #94a0ad)', marginBottom: 4 }}>
          Folder name
        </label>
        <input
          id="new-folder-name"
          type="text"
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. ARES Drills"
          maxLength={40}
          data-testid="new-folder-name-input"
          style={{
            background: 'var(--bg, #0d1318)',
            border: '1px solid var(--border-strong, #2c3744)',
            color: 'inherit',
            fontFamily: 'inherit',
            fontSize: 13,
            padding: '6px 10px',
            width: '100%',
            borderRadius: 4,
            outline: 'none',
          }}
        />
        <div style={{ fontSize: 11, color: 'var(--text-faint, #5d6975)', marginTop: 4 }}>
          3–40 characters. Reserved names (Inbox, Sent, Outbox, Drafts, Archive) are rejected.
        </div>
        {error && (
          <div
            data-testid="new-folder-error"
            role="alert"
            style={{ fontSize: 12, color: 'var(--error, #ee6b6b)', marginTop: 8 }}
          >
            {error}
          </div>
        )}
        <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end', marginTop: 18 }}>
          <button
            type="button"
            onClick={onClose}
            data-testid="new-folder-cancel"
            style={btnStyle}
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={create.isPending || name.trim().length < 3}
            data-testid="new-folder-create"
            style={{ ...btnStyle, borderColor: 'var(--success, #5dd6a0)', color: 'var(--success, #5dd6a0)' }}
          >
            {create.isPending ? 'Creating…' : 'Create'}
          </button>
        </div>
      </form>
    </div>
  );
}

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
