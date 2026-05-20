// Tab strip — folder navigation for the Mock D shell (replaces the synthesis
// FolderSidebar in the default composition).
//
// tuxlink-yd4 (2026-05-20): Mock D uses a horizontal tab strip with exactly two
// tabs — Inbox + Sent (literal mock). Outbox stays reachable via the Mailbox
// menu (menu:mailbox:*, wired in AppShell); Drafts has no nav surface in the
// approved v0.0.1 mock. The active tab is the selected folder; selecting a tab
// calls `onSelectFolder(folder)` (same contract the FolderSidebar had).
//
// Markup matches the mock's `.tab-strip` / `.tab` / `.tab-count` (AppShell.css,
// scoped under `.layout-d`). Mock D omits the `.tab-action` (Connect) button
// that Mock A carries.

import type { MailboxFolder } from '../mailbox/types';

export interface ShellTabDef {
  id: MailboxFolder;
  label: string;
}

/// Tabs in display order — exactly the two the approved Mock D shows. Other
/// folders (Outbox/Drafts) are reached via the Mailbox menu, not tabs.
export const SHELL_TABS: readonly ShellTabDef[] = [
  { id: 'inbox', label: 'Inbox' },
  { id: 'sent', label: 'Sent' },
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
