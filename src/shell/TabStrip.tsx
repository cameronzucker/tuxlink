// Tab strip — folder navigation for the Mock D shell (replaces the synthesis
// FolderSidebar in the default composition).
//
// tuxlink-yd4 (2026-05-20): Mock D uses a horizontal tab strip (mock shows
// Inbox / Sent; we surface the functional folders Inbox / Outbox / Sent /
// Drafts as tabs with counts). The active tab is the selected folder. Selecting
// a tab calls `onSelectFolder(folder)` — same contract the FolderSidebar had,
// so AppShell's selection plumbing is unchanged. Disabled/placeholder folders
// (Deleted / Templates) are dropped from v0.0.1's tabs entirely (they were
// disabled rows in the sidebar; a disabled tab is noise in the minimal mock).
//
// Markup matches the mock's `.tab-strip` / `.tab` / `.tab-count` (AppShell.css,
// scoped under `.layout-d`). Mock D omits the `.tab-action` (Connect) button
// that Mock A carries.

import type { MailboxFolder } from '../mailbox/types';

export interface ShellTabDef {
  id: MailboxFolder;
  label: string;
}

/// Tabs in display order. The four functional folders (spec §2.2); the
/// disabled Deleted/Templates placeholders are not surfaced as tabs.
export const SHELL_TABS: readonly ShellTabDef[] = [
  { id: 'inbox', label: 'Inbox' },
  { id: 'outbox', label: 'Outbox' },
  { id: 'sent', label: 'Sent' },
  { id: 'drafts', label: 'Drafts' },
];

export interface TabStripProps {
  selectedFolder: MailboxFolder;
  onSelectFolder: (folder: MailboxFolder) => void;
  /// Per-folder message counts. Missing or zero → no count badge (matches the
  /// FolderSidebar's suppress-zero behaviour).
  counts?: Partial<Record<MailboxFolder, number>>;
}

export function TabStrip({ selectedFolder, onSelectFolder, counts = {} }: TabStripProps) {
  return (
    <div className="tab-strip" data-testid="tab-strip" role="tablist" aria-label="Mailbox folders">
      {SHELL_TABS.map((t) => {
        const active = t.id === selectedFolder;
        const count = counts[t.id];
        return (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={active}
            data-testid={`tab-${t.id}`}
            className={active ? 'tab active' : 'tab'}
            onClick={() => onSelectFolder(t.id)}
          >
            {t.label}
            {typeof count === 'number' && count > 0 && (
              <span className="tab-count" data-testid={`tab-count-${t.id}`}>
                {count}
              </span>
            )}
          </button>
        );
      })}
      <div className="tab-spacer" />
    </div>
  );
}
