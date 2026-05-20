// Main application shell — owns selection state + the CSS-grid layout.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §4
// bd issue: tuxlink-zsm (Task 12) → tuxlink-8zg (orchestrator integration)
//
// Grid regions per §4.1: ribbon (top) · sidebar (left) · list (center) ·
// reader (right) · dock (reserved, renders nothing) · sessionlog (bottom
// strip) · statusbar (bottom). Task 12 shipped this with INLINE PLACEHOLDER
// <div>s for ribbon/reader/sessionlog/statusbar; the orchestrator integration
// commit (spec §4.3) swaps those placeholders for the REAL components —
// DashboardRibbon (Task 16), MessageView (Task 13), SessionLog (Task 15),
// StatusBar (Task 16) — concentrating the shared-file edit into one diff to
// avoid cross-PR conflicts.
//
// Selection ownership (§4.2): AppShell owns `selectedFolder` and
// `selectedMessage: {folder, id} | null`. The folder is CARRIED WITH the id
// (Codex F1) because `message_read`/`read_message_in` both require it — a
// bare id would recreate the Inbox-only bug. Selecting a different folder
// resets `selectedMessage` to null. Selecting a row updates ONLY
// `selectedMessage` (the reader); it never remounts/routes the shell — the
// no-full-view-swap invariant (§4.2).
//
// Counts (§5.2 + Codex Task-12 finding 2): FolderSidebar is fed a `counts`
// map sourced from per-folder mailbox queries (Inbox/Outbox/Sent). Drafts is
// local (FolderSidebar reads `listDraftIds()` itself — now backed by Task
// 14's store via `draftIds.ts`'s re-export).

import { useState, useCallback, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { FolderSidebar } from '../mailbox/FolderSidebar';
import { MessageList } from '../mailbox/MessageList';
import { useMailbox } from '../mailbox/useMailbox';
import { isNotConfigured } from '../mailbox/types';
import type { MailboxFolder } from '../mailbox/types';
import { DashboardRibbon } from './DashboardRibbon';
import { StatusBar } from './StatusBar';
import MessageView from '../mailbox/MessageView';
import { SessionLog } from '../session/SessionLog';
import './AppShell.css';

export interface SelectedMessage {
  folder: MailboxFolder;
  id: string;
}

export function AppShell() {
  const [selectedFolder, setSelectedFolder] = useState<MailboxFolder>('inbox');
  const [selectedMessage, setSelectedMessage] = useState<SelectedMessage | null>(null);
  // View → Toggle Status Bar (menu:view:status_bar). Shown by default.
  const [showStatusBar, setShowStatusBar] = useState(true);

  // Selected-folder query drives the center list + its empty/not-connected
  // state. The other two backend folders are queried only for their sidebar
  // counts (§5.2 / Codex finding 2). `drafts`/`deleted` are non-backend, so
  // useMailbox is a no-op (disabled) for them.
  const { messages, error } = useMailbox(selectedFolder);
  const inbox = useMailbox('inbox');
  const outbox = useMailbox('outbox');
  const sent = useMailbox('sent');
  const notConnected = isNotConfigured(error);

  // Per-folder counts for the BACKEND folders (Inbox/Outbox/Sent). Drafts is
  // local and counted inside FolderSidebar via listDraftIds(); Deleted is a
  // disabled placeholder (no count). Spec §5.2 / Codex Task-12 finding 2.
  const counts: Partial<Record<MailboxFolder, number>> = {
    inbox: inbox.messages.length,
    outbox: outbox.messages.length,
    sent: sent.messages.length,
  };

  // View → Toggle Status Bar: the native menu broadcasts `menu:view:status_bar`
  // on the "menu" channel (menu.rs). Listen for it and flip visibility. (Main
  // window only renders the shell, so this listener is implicitly main-only.)
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let mounted = true;
    listen<string>('menu', (event) => {
      if (event.payload === 'menu:view:status_bar') {
        setShowStatusBar((s) => !s);
      }
    }).then((fn) => {
      if (mounted) unlisten = fn;
      else fn();
    });
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);

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

  const windowInfo = `${folderLabel(selectedFolder)} · ${messages.length} message${
    messages.length === 1 ? '' : 's'
  }`;

  return (
    <div className="app-shell" data-testid="app-shell-root">
      {/* ribbon — Task 16 DashboardRibbon (real, integration commit) */}
      <div className="region-ribbon">
        <DashboardRibbon />
      </div>

      {/* sidebar — Task 12 (real) */}
      <div className="region-sidebar">
        <FolderSidebar
          selectedFolder={selectedFolder}
          onSelectFolder={onSelectFolder}
          counts={counts}
        />
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

      {/* reader — Task 13 MessageView (real, integration commit) */}
      <div className="region-reader">
        <MessageView selectedMessage={selectedMessage} />
      </div>

      {/* dock — reserved column, renders nothing (Task 16.5 out of scope, §4.1) */}
      <div className="region-dock" data-testid="region-dock-reserved" aria-hidden="true" />

      {/* sessionlog — Task 15 SessionLog (real, integration commit) */}
      <div className="region-sessionlog">
        <SessionLog />
      </div>

      {/* statusbar — Task 16 StatusBar (real, integration commit). Toggle via
          View → Toggle Status Bar; when hidden the component returns null. */}
      <div className="region-statusbar">
        <StatusBar show={showStatusBar} windowInfo={windowInfo} />
      </div>
    </div>
  );
}

/// Human label for the status-bar window-info string. Drafts/Deleted are
/// included for completeness even though the list is empty for them.
function folderLabel(folder: MailboxFolder): string {
  switch (folder) {
    case 'inbox':
      return 'Inbox';
    case 'outbox':
      return 'Outbox';
    case 'sent':
      return 'Sent';
    case 'drafts':
      return 'Drafts';
    case 'deleted':
      return 'Deleted';
  }
}
