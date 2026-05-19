// Folder sidebar (left region of the app shell).
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §2.2, §5.2
// bd issue: tuxlink-zsm (Task 12)
//
// Inbox / Outbox / Sent / Drafts are functional; Deleted / Templates are
// disabled placeholders (no backend delete/move + no templates in v0.0.1,
// spec §2.2). Selecting a folder calls `onSelectFolder(folder)`. Counts come
// from the per-folder query for backend folders (passed in by AppShell) and
// from the local draft store for Drafts.

import type { MailboxFolder } from './types';
import { listDraftIds } from './draftIds';

export interface SidebarFolderDef {
  id: MailboxFolder | 'templates';
  label: string;
  enabled: boolean;
}

/// Folder rows in display order (spec §2.2). `templates` is not a
/// `MailboxFolder` (it has no backend mapping), only a disabled placeholder.
export const SIDEBAR_FOLDERS: readonly SidebarFolderDef[] = [
  { id: 'inbox', label: 'Inbox', enabled: true },
  { id: 'outbox', label: 'Outbox', enabled: true },
  { id: 'sent', label: 'Sent', enabled: true },
  { id: 'drafts', label: 'Drafts', enabled: true },
  { id: 'deleted', label: 'Deleted', enabled: false },
  { id: 'templates', label: 'Templates', enabled: false },
];

export interface FolderSidebarProps {
  selectedFolder: MailboxFolder;
  onSelectFolder: (folder: MailboxFolder) => void;
  /// Per-folder counts for the functional BACKEND folders, keyed by folder
  /// id. AppShell supplies these from its queries. Missing → no badge.
  counts?: Partial<Record<MailboxFolder, number>>;
}

export function FolderSidebar({ selectedFolder, onSelectFolder, counts = {} }: FolderSidebarProps) {
  // Drafts count is local, not a backend query.
  const draftCount = listDraftIds().length;

  return (
    <nav className="folder-sidebar" data-testid="folder-sidebar" aria-label="Mailbox folders">
      <ul role="list">
        {SIDEBAR_FOLDERS.map((f) => {
          const isSelected = f.enabled && f.id === selectedFolder;
          const count =
            f.id === 'drafts'
              ? draftCount
              : f.enabled && f.id !== 'templates'
                ? counts[f.id as MailboxFolder]
                : undefined;
          return (
            <li key={f.id}>
              <button
                type="button"
                data-testid={`folder-${f.id}`}
                className={[
                  'folder-item',
                  isSelected ? 'selected' : '',
                  f.enabled ? '' : 'disabled',
                ]
                  .filter(Boolean)
                  .join(' ')}
                disabled={!f.enabled}
                aria-disabled={!f.enabled}
                aria-current={isSelected ? 'true' : undefined}
                title={f.enabled ? undefined : `${f.label} arrives in a later release`}
                onClick={() => {
                  // Only functional folders are selectable. Templates is not a
                  // MailboxFolder, so it can never be selected.
                  if (f.enabled && f.id !== 'templates') {
                    onSelectFolder(f.id as MailboxFolder);
                  }
                }}
              >
                <span className="folder-label">{f.label}</span>
                {typeof count === 'number' && count > 0 && (
                  <span className="folder-count" data-testid={`folder-count-${f.id}`}>
                    {count}
                  </span>
                )}
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
