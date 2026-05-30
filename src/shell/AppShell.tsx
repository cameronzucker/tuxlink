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
import { SettingsPanel } from './SettingsPanel';
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
import { derivePacketUiState, type PacketUiState } from '../packet/packetStatus';
import { isBuilt } from '../connections/sessionTypes';
import { TelnetCmsPanelContainer } from '../connections/TelnetCmsPanel';
import { StubPanel } from '../connections/StubPanel';
import { ArdopHfStub } from '../connections/ArdopHfStub';
import { useModemStatus } from '../modem/useModemStatus';
import { ArdopDock } from '../modem/ArdopDock';
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
  // Inline GPS/privacy settings overlay (tuxlink-39b), opened from Tools→Settings.
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Connection panel: null = no panel; a {sessionType, protocol} key selects the reading-pane connection pane.
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

  // Modem (ARDOP HF) status — drives the right-hand dock's mount + the
  // panes-grid column-count swap (tuxlink-4ek Task 4.3). The dock appears
  // whenever the modem is doing anything other than 'stopped' so the operator
  // can see what the link is doing without hunting for a hidden panel.
  const { status: modemStatus } = useModemStatus();
  const dockVisible = modemStatus.state !== 'stopped';

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
    openSettings: () => setSettingsOpen(true),
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

  // Derive the packet UI state for the ribbon + status bar indicators from the
  // LIVE backend status (tuxlink-orj). The real feed has landed: the backend now
  // reports Listening (armed) / Connected for packet, so the indicator reflects
  // what the link is actually doing instead of the prior hard-coded placeholder.
  // Honesty is preserved by construction — derivePacketUiState only claims
  // listening/connected when the backend status says so (Listening, or Connected
  // with a packet transport), never from panel selection alone.
  const packetUi: PacketUiState = useMemo(
    () =>
      derivePacketUiState(
        statusData.status ?? null,
        selectedConnection?.protocol === 'packet',
        effectiveCall(statusData.callsign, 0), // v0.1 placeholder SSID
      ),
    [statusData.status, selectedConnection, statusData.callsign],
  );

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

      <div
        className={`panes${dockVisible ? ' panes--with-dock' : ''}`}
        data-testid="shell-panes"
      >
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
        {(() => {
          if (selectedConnection === null) {
            return <MessageView selectedMessage={selectedMessage} />;
          }
          if (!isBuilt(selectedConnection)) {
            return <StubPanel sessionType={selectedConnection.sessionType} protocol={selectedConnection.protocol} />;
          }
          const { sessionType, protocol } = selectedConnection;
          if (sessionType === 'cms' && protocol === 'telnet') {
            return <TelnetCmsPanelContainer />;
          }
          if (sessionType === 'cms' && protocol === 'packet') {
            return <PacketConnectionPanelContainer baseCall={statusData.callsign} intent="cms-gateway" />;
          }
          if (sessionType === 'cms' && protocol === 'ardop-hf') {
            // The actual dial UI lives in the right-hand ArdopDock; the
            // reading-pane just directs the operator there (tuxlink-4ek 4.3).
            return <ArdopHfStub />;
          }
          if (sessionType === 'p2p' && protocol === 'packet') {
            return <PacketConnectionPanelContainer baseCall={statusData.callsign} intent="p2p" />;
          }
          // Built but unhandled — defensive stub
          return <StubPanel sessionType={sessionType} protocol={protocol} />;
        })()}
        {dockVisible && <ArdopDock />}
      </div>

      {showSessionLog && <SessionLog />}

      <StatusBar show={showStatusBar} unread={counts.inbox ?? 0} state={statusData.state} packet={packetUi} />

      <SettingsPanel open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}
