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

import { useState, useCallback, useEffect, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useQueryClient } from '@tanstack/react-query';
import { MessageList } from '../mailbox/MessageList';
import { useMailbox } from '../mailbox/useMailbox';
import { isNotConfigured } from '../mailbox/types';
import type { MailboxFolder } from '../mailbox/types';
import { DEV_SELECTED } from '../mailbox/devFixture';
import { FolderSidebar } from '../mailbox/FolderSidebar';
import type { ConnectionKey } from '../mailbox/FolderSidebar';
import { DashboardRibbon } from './DashboardRibbon';
import { StatusBar } from './StatusBar';
import { useStatusData } from './useStatus';
import { applyColorScheme, saveColorScheme } from './colorScheme';
import MessageView from '../mailbox/MessageView';
import { SessionLog } from '../session/SessionLog';
import { TitleBar } from './chrome/TitleBar';
import { MenuBar } from './chrome/MenuBar';
import { ResizeHandles } from './chrome/ResizeHandles';
import { useAccelerators } from './chrome/useAccelerators';
import { dispatchMenuAction, type MenuHandlers } from './chrome/dispatchMenuAction';
import { useMessage } from '../mailbox/useMessage';
import { openReplyWindow } from '../mailbox/replyActions';
import { newDraftId } from '../routing';
import { PacketConnectionPanelContainer } from '../packet/PacketConnectionPanel';
import { effectiveCall } from '../packet/packetConfig';
import type { PacketUiState } from '../packet/packetStatus';
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

  // Connection panel: null = no panel; 'packet' = reading-pane shows PacketConnectionPanel.
  const [selectedConnection, setSelectedConnection] = useState<ConnectionKey | null>(null);

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

  // CMS connect: run one exchange (send outbox + receive), then refresh the
  // mailbox so any downloaded messages appear. The button lives in the ribbon;
  // progress + any failure reason surface in the session log (emitted by the
  // backend), not beside the button.
  const queryClient = useQueryClient();
  const [connecting, setConnecting] = useState(false);

  const onConnect = useCallback(async () => {
    // Codex #1: don't start a second connect while one is in flight. The Connect
    // button is disabled, but the F5 / Ctrl+Shift+O accelerator also routes here.
    // The backend single-flight guard is the hard guarantee; this just avoids a
    // spurious "already in progress" error line on a double-press.
    if (connecting) return;
    setConnecting(true);
    try {
      await invoke('cms_connect');
      await queryClient.invalidateQueries({ queryKey: ['mailbox'] });
    } catch {
      // The result and any failure reason are shown in the session log + the
      // connection-status ribbon by the backend — nothing inline here.
    } finally {
      setConnecting(false);
    }
  }, [queryClient, connecting]);

  const onAbort = useCallback(() => {
    // Fire-and-forget (tuxlink-9z2): the abort shuts the connecting socket; the
    // in-flight cms_connect promise then resolves (Cancelled) and its `finally`
    // clears `connecting`. The session log carries the "Aborting…" line.
    void invoke('cms_abort');
  }, []);

  // Native titlebar: mock B shows "Tuxlink — Inbox". Track the active folder.
  useEffect(() => {
    try {
      void getCurrentWindow().setTitle(`Tuxlink — ${FOLDER_LABELS[selectedFolder]}`);
    } catch {
      /* no Tauri runtime (tests) — title is cosmetic */
    }
  }, [selectedFolder]);

  // The parsed message the reading pane is showing — drives menu/accelerator
  // Reply/Reply All/Forward. Same query key as MessageView's useMessage, so
  // TanStack dedupes (no extra IPC). `data` is undefined when nothing is selected.
  const { data: openMessage } = useMessage(selectedMessage);

  const handlers: MenuHandlers = useMemo(() => ({
    openCompose: () => { void invoke('compose_window_open', { draftId: newDraftId() }); },
    connect: onConnect,
    // Operator decision 2026-05-21 (option b): Reply/Reply All/Forward open a
    // reply window from the current selection — making good on the reading-pane
    // button label "Reply (Ctrl+R)". Reuses openReplyWindow (seeds a prefilled
    // draft + opens a compose window). No-op when nothing is selected.
    reply: () => { if (openMessage) void openReplyWindow(openMessage, 'reply').catch(() => {}); },
    replyAll: () => { if (openMessage) void openReplyWindow(openMessage, 'replyAll').catch(() => {}); },
    forward: () => { if (openMessage) void openReplyWindow(openMessage, 'forward').catch(() => {}); },
    toggleSessionLog: () => setShowSessionLog((s) => !s),
    toggleStatusBar: () => setShowStatusBar((s) => !s),
    selectFolder: (folder) => { setSelectedFolder(folder); setSelectedMessage(null); setSelectedConnection(null); },
    setScheme: (id) => { applyColorScheme(id); saveColorScheme(id); },
    quit: () => { void invoke('app_quit'); },
  }), [onConnect, openMessage]);

  const onMenuAction = useCallback((id: string) => dispatchMenuAction(id, handlers), [handlers]);
  useAccelerators(onMenuAction);

  const onSelectFolder = useCallback((folder: MailboxFolder) => {
    setSelectedFolder(folder);
    setSelectedMessage(null);
    setSelectedConnection(null);
  }, []);

  const onSelectConnection = useCallback((conn: ConnectionKey) => {
    setSelectedConnection(conn);
    setSelectedMessage(null);
  }, []);

  const onSelectMessage = useCallback(
    (id: string) => {
      setSelectedMessage({ folder: selectedFolder, id });
      setSelectedConnection(null);
    },
    [selectedFolder],
  );

  // Derive the packet UI state for the ribbon + status bar indicators.
  // The panel being open means packet is the SELECTED transport — it does NOT
  // mean we are listening or connected. There is no live packet-session feed yet,
  // so we never assert "Listening"/"Connected" here (that would lie about on-air
  // state — unacceptable for EmComm). listening/connected stay false until a real
  // backend status feed lands; the indicator then shows an honest "not connected".
  const packetUi: PacketUiState = useMemo(() => ({
    active: selectedConnection === 'packet',
    listening: false,
    connected: false,
    effectiveCall: effectiveCall(statusData.callsign, 0), // v0.1 placeholder SSID
    linkLabel: '',
  }), [selectedConnection, statusData.callsign]);

  return (
    <div className="layout-b" data-testid="app-shell-root">
      <TitleBar folderLabel={FOLDER_LABELS[selectedFolder]} />
      <MenuBar onAction={onMenuAction} />
      <ResizeHandles />
      <DashboardRibbon
        data={statusData}
        onConnect={onConnect}
        connecting={connecting}
        onAbort={onAbort}
        packet={packetUi}
      />

      <div className="panes" data-testid="shell-panes">
        <FolderSidebar
          selectedFolder={selectedFolder}
          onSelectFolder={onSelectFolder}
          counts={counts}
          selectedConnection={selectedConnection}
          onSelectConnection={onSelectConnection}
          packetState={packetUi.connected ? 'connected' : packetUi.listening ? 'listening' : 'off'}
        />
        <MessageList
          folder={selectedFolder}
          messages={messages}
          selectedId={selectedMessage?.id ?? null}
          onSelect={onSelectMessage}
          notConnected={notConnected}
        />
        {selectedConnection === 'packet' ? (
          <PacketConnectionPanelContainer baseCall={statusData.callsign} />
        ) : (
          <MessageView selectedMessage={selectedMessage} />
        )}
      </div>

      {showSessionLog && <SessionLog />}

      <StatusBar show={showStatusBar} unread={counts.inbox ?? 0} state={statusData.state} packet={packetUi} />
    </div>
  );
}
