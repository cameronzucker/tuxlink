// Main application shell — Mock B (Principles-faithful), the APPROVED v0.0.1
// design (docs/design/mockups/images/mock-b-principles-faithful.png + the MOCK B
// block in 2026-05-17-mocks-v1-four-directions.html).
//
// Layout (the mock's dashboard + layout-B, combined into one root grid):
//   dashboard ribbon (top) / panes[ sidebar | message list | reading pane ] /
//   human session-log strip / status bar.
//
// Selection ownership (unchanged from Task 12): AppShell owns `selectedFolder`
// + `selectedMessage: {folder, id} | null`. The folder is carried with the id.
// Selecting a folder resets the selection; selecting a row updates only the
// reader (no remount / route).
//
// Compose is a separate floating Tauri window (compose_window.rs), opened from
// File → New Message and the reading-pane reply actions.

import { useState, useCallback, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { MessageList } from '../mailbox/MessageList';
import { useMailbox } from '../mailbox/useMailbox';
import { isNotConfigured } from '../mailbox/types';
import type { MailboxFolder } from '../mailbox/types';
import { DEV_SELECTED } from '../mailbox/devFixture';
import { FolderSidebar } from '../mailbox/FolderSidebar';
import { DashboardRibbon } from './DashboardRibbon';
import { StatusBar } from './StatusBar';
import { useStatusData } from './useStatus';
import MessageView from '../mailbox/MessageView';
import { SessionLog } from '../session/SessionLog';
import './AppShell.css';

/// Human label for a folder (titlebar). Mirrors the sidebar labels.
const FOLDER_LABELS: Record<MailboxFolder, string> = {
  inbox: 'Inbox',
  outbox: 'Outbox',
  sent: 'Sent',
  drafts: 'Drafts',
  deleted: 'Deleted',
};

export interface SelectedMessage {
  folder: MailboxFolder;
  id: string;
}

export function AppShell() {
  const [selectedFolder, setSelectedFolder] = useState<MailboxFolder>('inbox');
  // DEV_SELECTED is null outside the vite dev server, so this starts null (the
  // real empty-reading-pane state) in tests + production.
  const [selectedMessage, setSelectedMessage] = useState<SelectedMessage | null>(DEV_SELECTED);
  // Mock B shows the session log + status bar by default; View → toggles them.
  const [showSessionLog, setShowSessionLog] = useState(true);
  const [showStatusBar, setShowStatusBar] = useState(true);

  const { messages, error } = useMailbox(selectedFolder);
  const inbox = useMailbox('inbox');
  const sent = useMailbox('sent');
  const notConnected = isNotConfigured(error);

  // Sidebar badges (mock B): Inbox = unread count ("3"), Sent = total ("87").
  const counts: Partial<Record<MailboxFolder, number>> = {
    inbox: inbox.messages.filter((m) => m.unread).length,
    sent: sent.messages.length,
  };

  // Status data (callsign / grid / connection) — single poll, shared by the
  // dashboard ribbon, the status bar, and the window title.
  const statusData = useStatusData();

  // Native titlebar: mock B shows "Tuxlink — Inbox". Track the active folder.
  useEffect(() => {
    try {
      void getCurrentWindow().setTitle(`Tuxlink — ${FOLDER_LABELS[selectedFolder]}`);
    } catch {
      /* no Tauri runtime (tests) — title is cosmetic */
    }
  }, [selectedFolder]);

  // View menu toggles (menu.rs broadcasts on the "menu" channel).
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let mounted = true;
    listen<string>('menu', (event) => {
      const p = event.payload;
      if (p === 'menu:view:status_bar') {
        setShowStatusBar((s) => !s);
      } else if (p === 'menu:view:session_log') {
        setShowSessionLog((s) => !s);
      } else if (
        p === 'menu:mailbox:inbox' ||
        p === 'menu:mailbox:sent' ||
        p === 'menu:mailbox:outbox'
      ) {
        setSelectedFolder(p.slice('menu:mailbox:'.length) as MailboxFolder);
        setSelectedMessage(null);
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

  const onSelectFolder = useCallback((folder: MailboxFolder) => {
    setSelectedFolder(folder);
    setSelectedMessage(null);
  }, []);

  const onSelectMessage = useCallback(
    (id: string) => {
      setSelectedMessage({ folder: selectedFolder, id });
    },
    [selectedFolder],
  );

  return (
    <div className="layout-b" data-testid="app-shell-root">
      <DashboardRibbon data={statusData} />

      <div className="panes" data-testid="shell-panes">
        <FolderSidebar
          selectedFolder={selectedFolder}
          onSelectFolder={onSelectFolder}
          counts={counts}
        />
        <MessageList
          folder={selectedFolder}
          messages={messages}
          selectedId={selectedMessage?.id ?? null}
          onSelect={onSelectMessage}
          notConnected={notConnected}
        />
        <MessageView selectedMessage={selectedMessage} />
      </div>

      {showSessionLog && <SessionLog />}

      <StatusBar show={showStatusBar} unread={counts.inbox ?? 0} state={statusData.state} />
    </div>
  );
}
