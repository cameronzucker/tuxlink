// Main application shell — Mock D (Mail.app-minimal).
//
// tuxlink-yd4 (2026-05-20): v0.0.1 adopts Mock D. This SUPERSEDES the synthesis
// layout (docs/design/v0.0.1-ux-mockups.md §3 decision #4 — ribbon + folder
// sidebar + session-log dock + reserved dock column). The shell now renders the
// mock's `layout-D`: a tab strip (folders), a 420px | 1fr two-pane body
// (MessageList | MessageView), and a minimal bottom status bar.
//
// Dropped from the default composition (component files retained / parked, not
// deleted): DashboardRibbon, FolderSidebar, the reserved dock column. The
// callsign / grid / connection state move into the StatusBar (the mock's
// minimum-viable visibility surface). The SessionLog is deferred entirely from
// the default pixels and reached via View → Session Log (menu:view:session_log)
// — the mock's own escape valve so the emcomm debug surface isn't lost.
//
// Selection ownership is unchanged from Task 12 (spec §4.2): AppShell owns
// `selectedFolder` + `selectedMessage: {folder, id} | null`. The folder is
// carried with the id (both `message_read` and the cache key need it).
// Selecting a folder resets the selection; selecting a row updates ONLY the
// reader (no remount / route — the no-full-view-swap invariant).

import { useState, useCallback, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { MessageList } from '../mailbox/MessageList';
import { useMailbox } from '../mailbox/useMailbox';
import { isNotConfigured } from '../mailbox/types';
import type { MailboxFolder } from '../mailbox/types';
import { listDraftIds } from '../mailbox/draftIds';
import { DEV_SELECTED } from '../mailbox/devFixture';
import { TabStrip } from './TabStrip';
import { StatusBar } from './StatusBar';
import { useStatusData } from './useStatus';
import MessageView from '../mailbox/MessageView';
import { SessionLog } from '../session/SessionLog';
import './AppShell.css';

/// Human label for a folder (titlebar + a11y). Mirrors the tab labels.
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
  // DEV_SELECTED is null outside the vite dev server (devFixture.ts), so this
  // starts as `null` (the real empty-reading-pane state) in tests + production.
  const [selectedMessage, setSelectedMessage] = useState<SelectedMessage | null>(DEV_SELECTED);
  // View → Toggle Status Bar (menu:view:status_bar). Shown by default — in
  // Mock D the status bar is the only at-a-glance callsign/grid surface.
  const [showStatusBar, setShowStatusBar] = useState(true);
  // View → Session Log (menu:view:session_log). HIDDEN by default — Mock D
  // defers the session log entirely; it appears as a bottom strip only when the
  // operator opts in (debug surface for when telnet hangs).
  const [showSessionLog, setShowSessionLog] = useState(false);

  // Selected-folder query drives the list. Inbox/Outbox/Sent are also queried
  // for their tab counts; Drafts is local (draftIds). `drafts`/`deleted` are
  // non-backend, so useMailbox is a disabled no-op for them.
  const { messages, error } = useMailbox(selectedFolder);
  const inbox = useMailbox('inbox');
  const outbox = useMailbox('outbox');
  const sent = useMailbox('sent');
  const notConnected = isNotConfigured(error);

  const counts: Partial<Record<MailboxFolder, number>> = {
    inbox: inbox.messages.length,
    outbox: outbox.messages.length,
    sent: sent.messages.length,
    drafts: listDraftIds().length,
  };

  // Status data (callsign / grid / connection state) — a single poll, owned
  // here so the window title can reuse the callsign. Passed to StatusBar.
  const statusData = useStatusData();

  // Native titlebar text, matching the mock's "Tuxlink — Inbox · W4PHS". The
  // window title tracks the active folder + callsign. Guarded so it's a no-op
  // outside a Tauri runtime (tests / SSR).
  useEffect(() => {
    const station = statusData.callsign ? ` · ${statusData.callsign}` : '';
    const title = `Tuxlink — ${FOLDER_LABELS[selectedFolder]}${station}`;
    try {
      void getCurrentWindow().setTitle(title);
    } catch {
      /* no Tauri runtime (tests) — title is cosmetic */
    }
  }, [selectedFolder, statusData.callsign]);

  // The native View menu broadcasts `menu:view:status_bar` / `menu:view:session_log`
  // on the "menu" channel (menu.rs). One listener, switch on the payload. (Main
  // window only renders the shell, so this is implicitly main-only.)
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let mounted = true;
    listen<string>('menu', (event) => {
      if (event.payload === 'menu:view:status_bar') {
        setShowStatusBar((s) => !s);
      } else if (event.payload === 'menu:view:session_log') {
        setShowSessionLog((s) => !s);
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
    <div className="layout-d" data-testid="app-shell-root">
      <TabStrip selectedFolder={selectedFolder} onSelectFolder={onSelectFolder} counts={counts} />

      <div className="panes" data-testid="shell-panes">
        <MessageList
          folder={selectedFolder}
          messages={messages}
          selectedId={selectedMessage?.id ?? null}
          onSelect={onSelectMessage}
          notConnected={notConnected}
        />
        <MessageView selectedMessage={selectedMessage} />
      </div>

      {/* Session log — deferred from default pixels; View → Session Log. */}
      {showSessionLog && (
        <div className="shell-sessionlog" data-testid="region-sessionlog">
          <SessionLog />
        </div>
      )}

      {/* Minimal status bar — dot+state · callsign · grid (left), version (right). */}
      <StatusBar show={showStatusBar} data={statusData} />
    </div>
  );
}
