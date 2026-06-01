// Main application shell — Mock B (Principles-faithful), the approved main-UI
// design (docs/design/mockups/images/mock-b-principles-faithful.png + the MOCK B
// block in 2026-05-17-mocks-v1-four-directions.html; ratified by ADR 0013).
//
// Layout (post-P1 radio-panel-shell): titlebar / menu bar / dashboard ribbon /
// chip-strip / panes[ sidebar | message list | reading pane | radio-panel |
// legacy ArdopDock ] / status bar. The right-hand radio-panel + legacy
// ArdopDock mount conditionally per spec §3.3; the ArdopDock is removed in P4
// once the per-mode panels (P2-P3) cover its surface.
//
// Selection ownership (unchanged from Task 12): AppShell owns `selectedFolder`
// + `selectedMessage: {folder, id} | null`. The folder is carried with the id.
// Selecting a folder resets the selection; selecting a row updates only the
// reader (no remount / route).
//
// Compose is a separate floating Tauri window (compose_window.rs), opened from
// File → New Message and the reading-pane reply actions.

import { useState, useCallback, useEffect, useMemo, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useQueryClient } from '@tanstack/react-query';
import { MessageList } from '../mailbox/MessageList';
import type { HighlightRange } from '../mailbox/MessageList';
import { useMailbox } from '../mailbox/useMailbox';
import { isNotConfigured } from '../mailbox/types';
import type { MailboxFolder, MessageMeta } from '../mailbox/types';
import type { MessageMetaDto } from '../search/types';
import { DEV_SELECTED } from '../mailbox/devFixture';
import { FolderSidebar } from '../mailbox/FolderSidebar';
import type { ConnectionKey } from '../mailbox/FolderSidebar';
import { DashboardRibbon } from './DashboardRibbon';
import { SettingsPanel } from './SettingsPanel';
import { StatusBar } from './StatusBar';
import { useStatusData } from './useStatus';
import { applyColorScheme, saveColorScheme } from './colorScheme';
import MessageView from '../mailbox/MessageView';
import { TitleBar } from './chrome/TitleBar';
import { MenuBar } from './chrome/MenuBar';
import { ResizeHandles } from './chrome/ResizeHandles';
import { useAccelerators } from './chrome/useAccelerators';
import { dispatchMenuAction, type MenuHandlers } from './chrome/dispatchMenuAction';
import { useMessage } from '../mailbox/useMessage';
import { openReplyWindow } from '../mailbox/replyActions';
import { newDraftId } from '../routing';
import { effectiveCall } from '../packet/packetConfig';
import { derivePacketUiState, type PacketUiState } from '../packet/packetStatus';
import { usePacketConfig } from '../packet/usePacketConfig';
import { isBuilt } from '../connections/sessionTypes';
import { TelnetRadioPanel } from '../radio/modes/TelnetRadioPanel';
import { TelnetP2pRadioPanel } from '../radio/modes/TelnetP2pRadioPanel';
import { PacketRadioPanel } from '../radio/modes/PacketRadioPanel';
import { ArdopRadioPanel } from '../radio/modes/ArdopRadioPanel';
import { StubPanel } from '../connections/StubPanel';
import { SearchBar } from '../search/SearchBar';
import { SearchDropdown } from '../search/SearchDropdown';
import { deparseQuery } from '../search/parseQuery';
import { SavedSearchesPanel } from '../search/SavedSearchesPanel';
import { useSearch } from '../search/useSearch';
import { useSavedSearches } from '../search/useSavedSearches';
import { useModemStatus } from '../modem/useModemStatus';
import { computePanelMode } from '../radio/radioPanelVisibility';
import type { RadioPanelMode } from '../radio/types';
import { PlaceholderRadioPanel } from '../radio/modes/PlaceholderRadioPanel';
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

/// Map a search-result DTO (camelCase, from Rust) to the MessageMeta shape
/// used by MessageList. The DTO's `folder: string` is cast to MailboxFolder;
/// callers are responsible for ensuring the backend only returns valid values.
/// `preview` and `formTag` carry through when present.
function dtoToMessageMeta(d: MessageMetaDto): MessageMeta {
  return {
    id: d.id,
    subject: d.subject,
    from: d.from,
    to: d.to,
    date: d.date,
    unread: d.unread,
    bodySize: d.bodySize,
    hasAttachments: d.hasAttachments,
    formTag: d.formTag,
    // preview is absent from MessageMetaDto — leave undefined
    folder: d.folder as MailboxFolder,
  };
}

/// Build per-message highlight ranges from a free-text query token.
/// Single-occurrence, first-token, case-insensitive (current scope). Multi-token
/// and multi-occurrence highlighting are a follow-up.
function computeHighlights(
  items: MessageMetaDto[],
  freeText: string | null,
): Record<string, HighlightRange[]> {
  if (!freeText || !freeText.trim()) return {};
  const needle = freeText.trim().toLowerCase();
  const out: Record<string, HighlightRange[]> = {};
  for (const item of items) {
    const subj = item.subject ?? '';
    const idx = subj.toLowerCase().indexOf(needle);
    if (idx >= 0) {
      out[item.id] = [{ field: 'subject', start: idx, end: idx + needle.length }];
    }
  }
  return out;
}

export function AppShell() {
  const [selectedFolder, setSelectedFolder] = useState<MailboxFolder>('inbox');
  // DEV_SELECTED is null outside the vite dev server, so this starts null (the
  // real empty-reading-pane state) in tests + production.
  const [selectedMessage, setSelectedMessage] = useState<SelectedMessage | null>(DEV_SELECTED);
  // Mock B shows the status bar by default; View → toggles it. The bottom
  // session-log strip was removed in radio-panel-shell P1.6 — the log moves
  // into the radio panel as a per-mode section in P2-P4 (spec §3.7 + §4.3).
  const [showStatusBar, setShowStatusBar] = useState(true);
  // tuxlink-mnk4: pin-on flag for the radio panel (View → Toggle Radio Panel /
  // Ctrl+Shift+M). Pure additive override — when true, forces the panel
  // visible even on views/states where the auto rule would hide it. Never
  // forces hide: an active modem (or being on the ARDOP HF view) always shows
  // the panel so the operator can't accidentally hide a live link.
  // (radio-panel-shell P1.7: renamed from pinRadioDock — the dock-vs-panel
  // distinction is dropped per spec §3.2 + §3.7.)
  const [pinRadioPanel, setPinRadioPanel] = useState(false);
  // Inline GPS/privacy settings overlay (tuxlink-39b), opened from Tools→Settings.
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Connection panel: null = no panel; a {sessionType, protocol} key selects the reading-pane connection pane.
  const [selectedConnection, setSelectedConnection] = useState<ConnectionKey | null>(null);

  // Find-messages: search + saved-search state (Task 17).
  const search = useSearch();
  const saved = useSavedSearches();
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const searchZoneRef = useRef<HTMLDivElement>(null);

  // Close the search dropdown on mousedown outside the search-zone wrapper.
  // The dropdown stays open on clicks INSIDE the zone (e.g. dropdown rows,
  // the SearchBar input itself) — this only triggers on background clicks.
  useEffect(() => {
    if (!dropdownOpen) return;
    const onMouseDown = (e: MouseEvent) => {
      const node = searchZoneRef.current;
      if (node && !node.contains(e.target as Node)) setDropdownOpen(false);
    };
    document.addEventListener('mousedown', onMouseDown);
    return () => document.removeEventListener('mousedown', onMouseDown);
  }, [dropdownOpen]);
  // Saved-searches management panel (Task 18).
  const [savedSearchesOpen, setSavedSearchesOpen] = useState(false);

  const metaText = useMemo((): string | null => {
    if (!search.isActive) return null;
    const r = search.results;
    if (!r) return search.isLoading ? 'Searching…' : null;
    const star = search.activeSaved ? ` · ★ ${search.activeSaved.name}` : '';
    return `${r.totalMatches} matches · ${r.queryMs} ms${star}`;
  }, [search.isActive, search.results, search.isLoading, search.activeSaved]);

  const { messages, error } = useMailbox(selectedFolder);
  const inbox = useMailbox('inbox');
  const sent = useMailbox('sent');
  const notConnected = isNotConfigured(error);

  // Search-result wiring (tuxlink-c7qz): when search is active, swap the
  // folder-scoped messages for search results. When search is active but
  // results haven't arrived yet (null), show an empty list — never fall back
  // to folder contents while a search is running (that would be misleading).
  const searchResultMessages = useMemo(
    () =>
      search.isActive
        ? (search.results?.items ?? []).map(dtoToMessageMeta)
        : null,
    [search.isActive, search.results],
  );

  const searchHighlights = useMemo(
    () =>
      search.isActive
        ? computeHighlights(search.results?.items ?? [], search.spec.free_text)
        : undefined,
    [search.isActive, search.results, search.spec.free_text],
  );

  // Show folder badges when search is cross-folder (no FOLDER chip applied).
  const searchIsCrossFolder =
    search.isActive &&
    (!search.spec.filters.folder ||
      (search.spec.filters.folder.kind === 'folder' &&
        search.spec.filters.folder.value === 'all'));

  const visibleMessages = searchResultMessages ?? messages;

  // Sidebar badges (mock B): Inbox = unread count ("3"), Sent = total ("87").
  const counts: Partial<Record<MailboxFolder, number>> = {
    inbox: inbox.messages.filter((m) => m.unread).length,
    sent: sent.messages.length,
  };

  // Status data (callsign / grid / connection) — single poll, shared by the
  // dashboard ribbon, the status bar, and the window title.
  const statusData = useStatusData();

  // Packet config — loaded once at AppShell and shared with the ribbon (callsign
  // SSID suffix + inline editor) AND the PacketRadioPanel (which reads its own
  // config and emits writes; the shared listener here picks those up). Operator
  // smoke 2026-05-31 caught that the prior code hardcoded SSID=0 in the ribbon.
  const packetConfig = usePacketConfig();

  // Modem (ARDOP HF) status — feeds the radio-panel visibility check + the
  // panes-grid column-count swap (tuxlink-4ek Task 4.3 baseline; radio-panel-
  // shell P1.5+ migrated to computePanelMode). The panel appears:
  //   - whenever the modem is doing anything other than 'stopped' (so the
  //     operator always sees an active link without hunting for a panel), OR
  //   - when the operator has selected any connection in the sidebar (cold-
  //     start dial form lives inside the panel — without this, ArdopHfStub's
  //     "use the modem dock on the right" message points at nothing and the
  //     operator can't spawn the modem at all — tuxlink-mnk4), OR
  //   - when the View → Toggle Radio Panel pin is on (Ctrl+Shift+M).
  const { status: modemStatus } = useModemStatus();
  // Spec §3.3 visibility rule. computePanelMode applies the OR of
  // (sidebar selection, active modem, pinned toggle) and returns the mode
  // to display, or null when the panel should not mount.
  // In v1, only the ARDOP modem exists; when it's running, the active
  // context is Ardop Winlink. Multi-modem coordination is out of scope
  // per spec §8.
  const activeModem: RadioPanelMode | null =
    modemStatus.state !== 'stopped'
      ? { kind: 'ardop-hf', intent: 'cms' }
      : null;

  const radioPanelMode = computePanelMode({
    sidebarSelected: selectedConnection,
    activeModem,
    togglePinned: pinRadioPanel,
  });

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
    toggleStatusBar: () => setShowStatusBar((s) => !s),
    toggleRadioPanel: () => setPinRadioPanel((s) => !s),
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

  // 2026-05-31 operator-flagged: selectedConnection and selectedMessage are
  // independent now that Telnet lives in the right-hand RadioPanel (P2). The
  // pre-P2 design clobbered each other because both fought for the reading
  // pane; the post-P2 reading pane shows MessageView for Telnet so a
  // connection-panel + open-message can coexist. Other modes (Packet/ARDOP)
  // still claim the reading pane via their per-protocol mount; the operator
  // simply sees the connection panel there until they click a different
  // connection or close the panel. selectedMessage is preserved for when
  // they navigate back.
  const onSelectConnection = useCallback((conn: ConnectionKey) => {
    setSelectedConnection(conn);
  }, []);

  const onSelectMessage = useCallback(
    (id: string) => {
      // When a search is active, the clicked row may live in a folder other
      // than the sidebar's selectedFolder. Look up the row's own folder
      // from the search results; fall back to the sidebar folder for the
      // regular folder-scoped browse case.
      const hit = searchResultMessages?.find((m) => m.id === id);
      const folder = (hit?.folder as MailboxFolder | undefined) ?? selectedFolder;
      setSelectedMessage({ folder, id });
    },
    [selectedFolder, searchResultMessages],
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
        // Operator smoke 2026-05-31: the prior hard-coded `0` made the ribbon
        // callsign show `<base>-0` regardless of the configured SSID. Source
        // the SSID from the shared packet config so the ribbon, status bar,
        // and PacketRadioPanel all agree on `<base>-<ssid>`.
        effectiveCall(statusData.callsign, packetConfig.ssid),
      ),
    [statusData.status, selectedConnection, statusData.callsign, packetConfig.ssid],
  );

  return (
    <div className="layout-b" data-testid="app-shell-root">
      <TitleBar folderLabel={FOLDER_LABELS[selectedFolder]} />
      <MenuBar onAction={onMenuAction} />
      <ResizeHandles />
      <div className="ribbon-with-search">
        <div className="search-zone" data-testid="search-zone" ref={searchZoneRef}>
          <SearchBar
            value={search.rawText}
            activeSaved={search.activeSaved}
            onValueChange={search.setRawText}
            onUnsave={async () => {
              if (search.activeSaved) {
                await saved.unsave(search.activeSaved.id);
                // Codex adrev fix (find-messages P2): only detach the saved-search
                // label; the deparsed rawText survives so the query stays active.
                search.clearActiveSaved();
              }
            }}
            onToggleDropdown={() => setDropdownOpen((o) => !o)}
            dropdownOpen={dropdownOpen}
            onCommit={() => { void saved.recordRecent(search.spec); }}
            metaText={metaText}
          />
          {dropdownOpen && (
            <SearchDropdown
              saved={saved.saved}
              recent={saved.recent}
              activeSavedId={search.activeSaved?.id ?? null}
              onRunSaved={(s) => { search.setActiveSavedSearch(s); setDropdownOpen(false); }}
              onRunRecent={(r) => { search.setRawText(deparseQuery(r.spec)); setDropdownOpen(false); }}
              onPromoteRecent={async (r, name) => {
                // Codex adrev fix (find-messages P2): use promote_recent so the
                // recent entry is removed atomically — avoids duplicate in dropdown.
                await saved.promoteRecent(name, r.spec);
              }}
              onUnsaveActive={async () => { if (search.activeSaved) await saved.unsave(search.activeSaved.id); }}
              onManage={() => { setSavedSearchesOpen(true); setDropdownOpen(false); }}
              onClose={() => setDropdownOpen(false)}
              onClearRecent={() => { void saved.clearRecent(); }}
            />
          )}
        </div>
        <DashboardRibbon
          data={statusData}
          onConnect={onConnect}
          connecting={connecting}
          onAbort={onAbort}
          packet={packetUi}
          ssid={packetConfig.config ? packetConfig.ssid : undefined}
          onSsidChange={packetConfig.config ? packetConfig.setSsid : undefined}
        />
      </div>

      <div
        className={`panes${radioPanelMode !== null ? ' panes--with-dock' : ''}`}
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
          messages={visibleMessages}
          selectedId={selectedMessage?.id ?? null}
          onSelect={onSelectMessage}
          notConnected={search.isActive ? false : notConnected}
          matchHighlights={searchHighlights}
          showFolderTag={searchIsCrossFolder}
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
            // P2: Telnet UI now lives in the right-hand TelnetRadioPanel.
            // The reading pane falls back to messages so the operator
            // can read mail while the connection panel handles transport.
            return <MessageView selectedMessage={selectedMessage} />;
          }
          if (sessionType === 'cms' && protocol === 'packet') {
            // P3: PacketRadioPanel owns the Packet dial UI in the right
            // radio panel; reading pane falls back to mail (same pattern
            // as Telnet (P2) and ARDOP (P4)).
            return <MessageView selectedMessage={selectedMessage} />;
          }
          if (sessionType === 'cms' && protocol === 'ardop-hf') {
            // P4: the ArdopRadioPanel owns the ARDOP HF dial UI; the
            // reading pane falls back to mail (same pattern as Telnet,
            // P2). Eliminates the P1 dual-mount of placeholder + ArdopDock.
            return <MessageView selectedMessage={selectedMessage} />;
          }
          if (sessionType === 'p2p' && protocol === 'packet') {
            // P3 (P2P branch): same — PacketRadioPanel handles the dial UI.
            return <MessageView selectedMessage={selectedMessage} />;
          }
          if (sessionType === 'p2p' && protocol === 'telnet') {
            // P2P Telnet (tuxlink-0pnb): TelnetP2pRadioPanel owns the dial
            // UI in the right panel; reading pane falls back to mail, same
            // pattern as Telnet CMS (P2) and Packet P2P (P3).
            return <MessageView selectedMessage={selectedMessage} />;
          }
          // Built but unhandled — defensive stub
          return <StubPanel sessionType={sessionType} protocol={protocol} />;
        })()}
        {/* Per-mode radio panels. Telnet (P2), Packet (P3), and ARDOP HF
            (P4) ship their real implementations; VARA HF / VARA FM still
            fall through to the placeholder until P5 lands. The P1 dual-
            mount of ArdopDock + placeholder for ARDOP HF is GONE — the
            ArdopRadioPanel covers the full dial-and-live-state surface
            on its own. */}
        {radioPanelMode && radioPanelMode.kind === 'telnet' && radioPanelMode.intent === 'cms' && (
          <TelnetRadioPanel
            onClose={() => {
              setSelectedConnection(null);
              setPinRadioPanel(false);
            }}
          />
        )}
        {radioPanelMode && radioPanelMode.kind === 'telnet' && radioPanelMode.intent === 'p2p' && (
          <TelnetP2pRadioPanel
            onClose={() => {
              setSelectedConnection(null);
              setPinRadioPanel(false);
            }}
          />
        )}
        {radioPanelMode && radioPanelMode.kind === 'packet' && (
          <PacketRadioPanel
            intent={radioPanelMode.intent}
            baseCall={statusData.callsign}
            onClose={() => {
              setSelectedConnection(null);
              setPinRadioPanel(false);
            }}
          />
        )}
        {radioPanelMode && radioPanelMode.kind === 'ardop-hf' && (
          <ArdopRadioPanel
            onClose={() => {
              setSelectedConnection(null);
              setPinRadioPanel(false);
            }}
          />
        )}
        {radioPanelMode &&
          radioPanelMode.kind !== 'telnet' &&
          radioPanelMode.kind !== 'packet' &&
          radioPanelMode.kind !== 'ardop-hf' && (
            <PlaceholderRadioPanel
              mode={radioPanelMode}
              onClose={() => {
                setSelectedConnection(null);
                setPinRadioPanel(false);
              }}
            />
          )}
      </div>

      <StatusBar show={showStatusBar} unread={counts.inbox ?? 0} state={statusData.state} packet={packetUi} />

      <SettingsPanel open={settingsOpen} onClose={() => setSettingsOpen(false)} />

      {savedSearchesOpen && (
        <SavedSearchesPanel onClose={() => setSavedSearchesOpen(false)} />
      )}
    </div>
  );
}
