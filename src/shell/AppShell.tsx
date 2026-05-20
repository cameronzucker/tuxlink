// Main application shell — owns selection state + the CSS-grid layout.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §4
// bd issue: tuxlink-zsm (Task 12)
//
// Grid regions per §4.1: ribbon (top) · sidebar (left) · list (center) ·
// reader (right) · dock (reserved, renders nothing) · sessionlog (bottom
// strip) · statusbar (bottom). Task 12 renders sidebar + list for real and
// INLINE PLACEHOLDER <div>s for ribbon/reader/sessionlog/statusbar — it does
// NOT import the sibling components (DashboardRibbon/MessageView/SessionLog/
// StatusBar). The orchestrator integration commit (spec §4.3) swaps the
// placeholders for the real imports; concentrating those edits into one diff
// avoids cross-PR conflicts on this file.
//
// Selection ownership (§4.2): AppShell owns `selectedFolder` and
// `selectedMessage: {folder, id} | null`. The folder is CARRIED WITH the id
// (Codex F1) because `message_read`/`read_message_in` both require it — a
// bare id would recreate the Inbox-only bug. Selecting a different folder
// resets `selectedMessage` to null. Selecting a row updates ONLY
// `selectedMessage` (the reader); it never remounts/routes the shell — the
// no-full-view-swap invariant (§4.2).

import { useState, useCallback } from 'react';
import { FolderSidebar } from '../mailbox/FolderSidebar';
import { MessageList } from '../mailbox/MessageList';
import { useMailbox } from '../mailbox/useMailbox';
import { isNotConfigured } from '../mailbox/types';
import type { MailboxFolder } from '../mailbox/types';
import './AppShell.css';

export interface SelectedMessage {
  folder: MailboxFolder;
  id: string;
}

export function AppShell() {
  const [selectedFolder, setSelectedFolder] = useState<MailboxFolder>('inbox');
  const [selectedMessage, setSelectedMessage] = useState<SelectedMessage | null>(null);

  const { messages, error } = useMailbox(selectedFolder);
  const notConnected = isNotConfigured(error);

  // Selecting a folder resets the message selection (the carried folder must
  // stay consistent with the list being shown). Spec §4.2.
  const onSelectFolder = useCallback((folder: MailboxFolder) => {
    setSelectedFolder(folder);
    setSelectedMessage(null);
  }, []);

  // Selecting a row updates ONLY the reader-pane selection — no route, no
  // remount (§4.2). The folder is carried alongside the id.
  const onSelectMessage = useCallback(
    (id: string) => {
      setSelectedMessage({ folder: selectedFolder, id });
    },
    [selectedFolder],
  );

  return (
    <div className="app-shell" data-testid="app-shell-root">
      {/* ribbon — PLACEHOLDER (Task 16 DashboardRibbon via integration commit) */}
      <div className="region-ribbon" data-testid="region-ribbon-placeholder" aria-hidden="true" />

      {/* sidebar — Task 12 (real) */}
      <div className="region-sidebar">
        <FolderSidebar selectedFolder={selectedFolder} onSelectFolder={onSelectFolder} />
      </div>

      {/* list — Task 12 (real) */}
      <div className="region-list">
        <MessageList
          folder={selectedFolder}
          messages={messages}
          selectedId={selectedMessage?.id ?? null}
          onSelect={onSelectMessage}
          notConnected={notConnected}
        />
      </div>

      {/* reader — PLACEHOLDER (Task 13 MessageView via integration commit) */}
      <div className="region-reader" data-testid="region-reader-placeholder">
        {selectedMessage ? (
          <div data-testid="reader-selection-debug">
            {/* Task 13's MessageView renders here; until then, show the
                selection so the no-swap behavior is observable. */}
            Selected {selectedMessage.folder} / {selectedMessage.id}
          </div>
        ) : (
          <div data-testid="reader-empty">Select a message to read.</div>
        )}
      </div>

      {/* dock — reserved column, renders nothing (Task 16.5 out of scope, §4.1) */}
      <div className="region-dock" data-testid="region-dock-reserved" aria-hidden="true" />

      {/* sessionlog — PLACEHOLDER (Task 15 SessionLog via integration commit) */}
      <div
        className="region-sessionlog"
        data-testid="region-sessionlog-placeholder"
        aria-hidden="true"
      />

      {/* statusbar — PLACEHOLDER (Task 16 StatusBar via integration commit) */}
      <div
        className="region-statusbar"
        data-testid="region-statusbar-placeholder"
        aria-hidden="true"
      />
    </div>
  );
}
