/**
 * NewFolderDialog — inline modal that creates a user folder (tuxlink-f62f;
 * styling refactored under tuxlink-i2nr).
 *
 * Opened from the sidebar's "Folders" section `+` button. The form is a
 * single text input for the display name; slug is derived backend-side.
 * Backend rejection (reserved name, duplicate, invalid chars) surfaces as
 * an inline error under the input — no toast (per the inline-UI preference).
 *
 * Esc closes; Enter submits. Styling matches `.tux-folder-*` (userFolders.css)
 * which mirrors the project's SettingsPanel / ThemeDesigner / AboutDialog
 * inline-overlay convention.
 */

import { useEffect, useState } from 'react';
import { useCreateUserFolder } from './useUserFolders';
import './userFolders.css';

export interface NewFolderDialogProps {
  open: boolean;
  onClose: () => void;
  onCreated?: (slug: string) => void;
  /// When set, the new folder is created as a subfolder of this slug (spec D3).
  /// `parentName` is shown in the dialog so the operator knows where it lands.
  parentSlug?: string;
  parentName?: string;
}

export function NewFolderDialog({
  open,
  onClose,
  onCreated,
  parentSlug,
  parentName,
}: NewFolderDialogProps) {
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const create = useCreateUserFolder();

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
    // UiError discriminated union. MessageRejected arrives as
    // { kind: 'Rejected', detail: '<reason>' }.
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
      const folder = await create.mutateAsync({ displayName: name, parentSlug });
      onCreated?.(folder.slug);
      onClose();
    } catch (err) {
      setError(reasonFromError(err));
    }
  }

  return (
    <div
      className="tux-folder-backdrop"
      role="presentation"
      data-testid="new-folder-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <form
        onSubmit={submit}
        role="dialog"
        aria-label="New folder"
        data-testid="new-folder-dialog"
        className="tux-folder-dialog"
      >
        <div className="tux-folder-header">
          <h3 className="tux-folder-title">{parentName ? 'New subfolder' : 'New folder'}</h3>
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
          {parentName && (
            <div className="tux-folder-help" data-testid="new-folder-parent-context">
              Inside: <strong>{parentName}</strong>
            </div>
          )}
          <label htmlFor="new-folder-name" className="tux-folder-field-label">
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
            className="tux-folder-input"
          />
          <div className="tux-folder-help">
            3–40 characters. Reserved names (Inbox, Sent, Outbox, Drafts, Archive) are rejected.
          </div>
          {error && (
            <div
              data-testid="new-folder-error"
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
            onClick={onClose}
            data-testid="new-folder-cancel"
            className="tux-folder-btn"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={create.isPending || name.trim().length < 3}
            data-testid="new-folder-create"
            className="tux-folder-btn tux-folder-btn-primary"
          >
            {create.isPending ? 'Creating…' : 'Create'}
          </button>
        </div>
      </form>
    </div>
  );
}
