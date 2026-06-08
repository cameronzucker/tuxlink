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

import { useState, useCallback, useEffect, useMemo, useRef, lazy, Suspense } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useQueryClient } from '@tanstack/react-query';
import { MessageList } from '../mailbox/MessageList';
import type { HighlightRange } from '../mailbox/MessageList';
import { type SortState, loadSortState, saveSortState } from '../mailbox/messageSort';
import { useMailbox, useMailboxChangeEvents } from '../mailbox/useMailbox';
import { DRAFTS_CHANGED_EVENT, listDraftMessages } from '../mailbox/draftMailbox';
import { isNotConfigured } from '../mailbox/types';
import type { MailboxFolder, MailboxFolderRef, MessageMeta } from '../mailbox/types';
import { useUserFolders } from '../mailbox/useUserFolders';
import { FolderContextMenu } from '../mailbox/FolderContextMenu';
import type { UserFolder } from '../mailbox/types';
import type { MessageMetaDto } from '../search/types';
import { DEV_SELECTED } from '../mailbox/devFixture';
import { FolderSidebar } from '../mailbox/FolderSidebar';
import type { ConnectionKey } from '../mailbox/FolderSidebar';
import { DashboardRibbon } from './DashboardRibbon';
import { StatusBar } from './StatusBar';
import { useStatusData, type StatusTone } from './useStatus';
import { applyColorScheme, saveColorScheme } from './colorScheme';

// tuxlink-perf-coldstart: lazy-load every overlay/dialog panel. None of these
// are on the cold-start critical path — they only paint when the operator
// opens them via a menu or button. Removing them from the eager import graph
// trims the bundle that gates first paint of the main shell. Each lazy panel
// is also gated at its call site (`{flag && <Suspense>…</Suspense>}`) so the
// module + its CSS aren't fetched until the open flag flips true.
const SettingsPanel = lazy(() =>
  import('./SettingsPanel').then((m) => ({ default: m.SettingsPanel })),
);
const ThemeDesigner = lazy(() =>
  import('./ThemeDesigner').then((m) => ({ default: m.ThemeDesigner })),
);
const AboutDialog = lazy(() =>
  import('./AboutDialog').then((m) => ({ default: m.AboutDialog })),
);
const CatalogRequestPanel = lazy(() =>
  import('../catalog/CatalogRequestPanel').then((m) => ({ default: m.CatalogRequestPanel })),
);
// tuxlink-a2gd: location-aware Catalog Builder (sibling overlay panel, not a main-content view).
const CatalogBuilderPanel = lazy(() =>
  import('../catalog/CatalogBuilderPanel').then((m) => ({ default: m.CatalogBuilderPanel })),
);
const GribRequestPanel = lazy(() =>
  import('../grib/GribRequestPanel').then((m) => ({ default: m.GribRequestPanel })),
);
const NewFolderDialog = lazy(() =>
  import('../mailbox/NewFolderDialog').then((m) => ({ default: m.NewFolderDialog })),
);
const RenameFolderDialog = lazy(() =>
  import('../mailbox/RenameFolderDialog').then((m) => ({ default: m.RenameFolderDialog })),
);
const DeleteFolderDialog = lazy(() =>
  import('../mailbox/DeleteFolderDialog').then((m) => ({ default: m.DeleteFolderDialog })),
);

// tuxlink-djnl: lazy-load MessageView so the cold-start path doesn't pull
// the forms registry (src/forms/index.ts side-effect imports every ICS-213,
// ICS-309, bulletin, position, damage-assessment renderer at module load).
// AppShell uses the eager MessageViewEmpty as both the no-selection render
// AND the Suspense fallback while the lazy chunk loads on first selection.
import { MessageViewEmpty, MessageViewLoading } from '../mailbox/MessageViewEmpty';
import {
  ReportIssueModal,
  useReportIssueController,
  type ReportIssueState,
} from '../help/ReportIssueModal';
const MessageView = lazy(() => import('../mailbox/MessageView'));
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
import { StubPanel } from '../connections/StubPanel';
import { SearchBar } from '../search/SearchBar';
import { deparseQuery } from '../search/parseQuery';

// tuxlink-twym: lazy-load the five real radio panels + the two search
// overlays. All seven are conditionally mounted (mode-switch / dropdown
// open / saved-search panel open), so React.lazy + Suspense with the
// existing call-site gate is a free win — chunks only fetch on first open.
//
// Loaders are extracted as named functions so the AppShell-level idle
// preload (radioPanelPreloadEffect below) can re-invoke them; React caches
// the resolved module so the second invocation is a no-op, and the operator
// no longer sees a blank Suspense fallback period when they first click a
// radio mode in the sidebar. tuxlink-0ye6 operator report 2026-06-04:
// opening VARA/ARDOP panels felt sluggish because the cold chunk had to
// download + Vite-transform (≈1-3 s on Pi 5) under the Suspense fallback.
const loadTelnetRadioPanel = () =>
  import('../radio/modes/TelnetRadioPanel').then((m) => ({ default: m.TelnetRadioPanel }));
const loadTelnetP2pRadioPanel = () =>
  import('../radio/modes/TelnetP2pRadioPanel').then((m) => ({ default: m.TelnetP2pRadioPanel }));
const loadPacketRadioPanel = () =>
  import('../radio/modes/PacketRadioPanel').then((m) => ({ default: m.PacketRadioPanel }));
const loadArdopRadioPanel = () =>
  import('../radio/modes/ArdopRadioPanel').then((m) => ({ default: m.ArdopRadioPanel }));
const loadVaraRadioPanel = () =>
  import('../radio/modes/VaraRadioPanel').then((m) => ({ default: m.VaraRadioPanel }));
const TelnetRadioPanel = lazy(loadTelnetRadioPanel);
const TelnetP2pRadioPanel = lazy(loadTelnetP2pRadioPanel);
const PacketRadioPanel = lazy(loadPacketRadioPanel);
const ArdopRadioPanel = lazy(loadArdopRadioPanel);
const VaraRadioPanel = lazy(loadVaraRadioPanel);
const SearchDropdown = lazy(() =>
  import('../search/SearchDropdown').then((m) => ({ default: m.SearchDropdown })),
);
const SavedSearchesPanel = lazy(() =>
  import('../search/SavedSearchesPanel').then((m) => ({ default: m.SavedSearchesPanel })),
);
import { useSearch } from '../search/useSearch';
import { useSavedSearches } from '../search/useSavedSearches';
import { useModemIsActive } from '../modem/useModemStatus';
import { computePanelMode } from '../radio/radioPanelVisibility';
import type { RadioPanelMode } from '../radio/types';
import { PlaceholderRadioPanel } from '../radio/modes/PlaceholderRadioPanel';
import './AppShell.css';

/// Human label for a system folder (titlebar). Mirrors the sidebar labels.
const FOLDER_LABELS: Record<MailboxFolder, string> = {
  inbox: 'Inbox',
  outbox: 'Outbox',
  sent: 'Sent',
  drafts: 'Drafts',
  deleted: 'Deleted',
  archive: 'Archive',
};

/// Folder-label lookup that handles both system folders and user-folder
/// slugs. For system folders → `FOLDER_LABELS`; for user folders → the
/// display name from the registry; for an unknown slug → the slug itself
/// (graceful fallback while the registry refetches after a create/rename).
function folderLabel(
  folder: MailboxFolderRef,
  userFolders: { slug: string; displayName: string }[],
): string {
  if (folder in FOLDER_LABELS) return FOLDER_LABELS[folder as MailboxFolder];
  const uf = userFolders.find((f) => f.slug === folder);
  return uf?.displayName ?? folder;
}

export interface SelectedMessage {
  folder: MailboxFolderRef;
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
  // selectedFolder accepts either a system-folder identifier or a user-folder
  // slug (tuxlink-f62f). The Tauri commands take either string at the boundary;
  // string-equal is enough to drive the sidebar's active-row highlight.
  const [selectedFolder, setSelectedFolder] = useState<MailboxFolderRef>('inbox');
  // tuxlink-f62f: NewFolderDialog visibility (opened from the sidebar's
  // Folders section `+` button).
  const [newFolderOpen, setNewFolderOpen] = useState(false);
  // tuxlink-ejph: rename / delete dialogs + folder context menu state.
  // `renameFolder` / `deleteFolder` hold the target folder when the dialog
  // is open, null when closed. The context menu carries position + slug.
  const [renameFolder, setRenameFolder] = useState<UserFolder | null>(null);
  const [deleteFolder, setDeleteFolder] = useState<UserFolder | null>(null);
  const [folderCtxMenu, setFolderCtxMenu] = useState<{
    folder: UserFolder;
    x: number;
    y: number;
  } | null>(null);
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
  // Inline theme designer overlay (tuxlink-vgth), opened from View → Color
  // Scheme → Customize…. Same backdrop pattern as SettingsPanel.
  const [themeDesignerOpen, setThemeDesignerOpen] = useState(false);
  // Inline About overlay (tuxlink-35g0), opened from the Help menu.
  // Help → Documentation now opens a separate Tauri webview window via
  // help_window_open (tuxlink-0gsy / spec §4); no in-process state.
  const [aboutOpen, setAboutOpen] = useState(false);
  // Inline Catalog Request panel (tuxlink-ddiq), opened from Message →
  // Catalog Request. Picks WLE catalog inquiries and queues a request
  // message in the outbox routed to INQUIRY@winlink.org.
  const [catalogRequestOpen, setCatalogRequestOpen] = useState(false);
  // tuxlink-a2gd: inline Catalog Builder ("Find a Gateway"), opened from Message → Find a Gateway.
  const [catalogBuilderOpen, setCatalogBuilderOpen] = useState(false);
  // Inline GRIB request panel (tuxlink-vrpk), opened from Message → GRIB
  // File Request. Composes a Saildocs request and queues it in the outbox.
  const [gribRequestOpen, setGribRequestOpen] = useState(false);
  // tuxlink-qjgx Task 8: Report Issue modal state. The controller drives the
  // Save As → export → GitHub URL flow; AppShell owns the state so the modal
  // can be positioned in the global overlay layer.
  const [reportIssueState, setReportIssueState] = useState<ReportIssueState>({ kind: 'idle' });
  const reportIssueController = useReportIssueController(setReportIssueState);

  // Message-list sort (tuxlink-2x0l). Lazy-init from localStorage so the
  // first render already uses the persisted preference (no flash of default).
  // Global preference for now; per-folder defaults are a Phase 3 idea.
  const [sortState, setSortState] = useState<SortState>(() => loadSortState());
  const onSortStateChange = useCallback((next: SortState) => {
    setSortState(next);
    saveSortState(next);
  }, []);

  // Connection panel: null = no panel; a {sessionType, protocol} key selects the reading-pane connection pane.
  const [selectedConnection, setSelectedConnection] = useState<ConnectionKey | null>(null);
  // tuxlink-479c: remember the last operator-selected/open transport separately
  // from panel visibility. Closing the right-hand panel should hide the panel,
  // not reset the dashboard/ribbon intent back to Telnet.
  const [activeConnection, setActiveConnection] = useState<ConnectionKey>({
    sessionType: 'cms',
    protocol: 'telnet',
  });

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

  // Warm the lazy radio-panel chunks at app idle so the operator's first
  // click on a sidebar connection doesn't sit on a blank Suspense fallback
  // while Vite cold-transforms a ~1000-line TSX file. Loaders return cached
  // module exports after the first call, so the eager preload's work is
  // fully reused by React.lazy on the operator click. tuxlink-0ye6 operator
  // report 2026-06-04 — opening VARA/ARDOP panels felt very sluggish; static
  // analysis ruled out heavy module-scope side effects, leaving cold chunk
  // load as the dominant first-open cost.
  useEffect(() => {
    const preload = () => {
      void loadTelnetRadioPanel();
      void loadTelnetP2pRadioPanel();
      void loadPacketRadioPanel();
      void loadArdopRadioPanel();
      void loadVaraRadioPanel();
    };
    // WebKitGTK (Tauri's web view) supports requestIdleCallback; the
    // setTimeout fallback covers test envs (jsdom) and older webviews.
    const w = window as Window & {
      requestIdleCallback?: (cb: () => void, opts?: { timeout: number }) => number;
      cancelIdleCallback?: (id: number) => void;
    };
    if (typeof w.requestIdleCallback === 'function') {
      const id = w.requestIdleCallback(preload, { timeout: 2000 });
      return () => w.cancelIdleCallback?.(id);
    }
    const id = window.setTimeout(preload, 500);
    return () => window.clearTimeout(id);
  }, []);

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
  // tuxlink-qxqj: the redesigned mailbox bar surfaces outbox-queue depth so
  // the operator knows what's waiting on the next CMS connect without
  // navigating to the Outbox folder. Cheap query — the 10s refetch matches
  // inbox/sent's polling cadence; no extra IPC burden.
  const outbox = useMailbox('outbox');
  // tuxlink-ca5x: Archive count for the sidebar badge. Per spec §6 (D9), user
  // folders show total count (matching Sent), not unread — archived messages
  // are almost always already read; "0 unread / 180 total" would mislead.
  const archive = useMailbox('archive');
  useMailboxChangeEvents();
  // tuxlink-f62f: operator-created user folders, rendered in the sidebar's
  // Folders section. Backend reads `<root>/.folders.json`.
  const { folders: userFolders } = useUserFolders();
  const notConnected = isNotConfigured(error);
  const [draftMessages, setDraftMessages] = useState<MessageMeta[]>(() => listDraftMessages());

  useEffect(() => {
    const refreshDrafts = () => setDraftMessages(listDraftMessages());
    refreshDrafts();
    window.addEventListener(DRAFTS_CHANGED_EVENT, refreshDrafts);
    window.addEventListener('storage', refreshDrafts);
    window.addEventListener('focus', refreshDrafts);
    const interval = window.setInterval(refreshDrafts, 2000);
    return () => {
      window.removeEventListener(DRAFTS_CHANGED_EVENT, refreshDrafts);
      window.removeEventListener('storage', refreshDrafts);
      window.removeEventListener('focus', refreshDrafts);
      window.clearInterval(interval);
    };
  }, []);

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

  const folderMessages = selectedFolder === 'drafts' ? draftMessages : messages;
  const visibleMessages = searchResultMessages ?? folderMessages;

  // Sidebar badges (mock B): Inbox = unread count ("3"), Outbox = queue depth
  // ("1 to send" mirrored from the status bar — same `outbox.messages.length`),
  // Drafts = local draft count, Sent = total ("87"). tuxlink-gp8b: Outbox was wired into the sidebar by
  // tuxlink-su2h (PR #219) but its count never made it into this object, so the
  // sidebar showed no badge while the status bar showed the same number — same
  // source data, two surfaces, only one rendered.
  // tuxlink-sndh: memoize so the sidebar's Folder rows don't re-render
  // every time AppShell does. Inputs are stable across non-mailbox renders.
  const counts: Partial<Record<MailboxFolder, number>> = useMemo(
    () => ({
      inbox: inbox.messages.filter((m) => m.unread).length,
      outbox: outbox.messages.length,
      drafts: draftMessages.length,
      sent: sent.messages.length,
      archive: archive.messages.length,
    }),
    [inbox.messages, outbox.messages, draftMessages, sent.messages, archive.messages],
  );

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
  // tuxlink-sndh: use the focused `useModemIsActive()` selector instead of
  // the full `useModemStatus()` to avoid re-rendering the entire shell every
  // 250ms when the Rust modem broadcaster ticks. AppShell only needs to know
  // whether the modem is in any active state; the live-meter panels keep
  // using the full hook for their sparklines.
  const modemIsActive = useModemIsActive();
  // Spec §3.3 visibility rule. computePanelMode applies the OR of
  // (sidebar selection, active modem, pinned toggle) and returns the mode
  // to display, or null when the panel should not mount.
  // In v1, only the ARDOP modem exists; when it's running, the active
  // context is Ardop Winlink. Multi-modem coordination is out of scope
  // per spec §8.
  const activeModem: RadioPanelMode | null = useMemo(
    () => (modemIsActive ? { kind: 'ardop-hf', intent: 'cms' } : null),
    [modemIsActive],
  );

  useEffect(() => {
    const status = statusData.status;
    if (
      status?.kind === 'Listening' ||
      (status?.kind === 'Connected' && status.transport.startsWith('Packet'))
    ) {
      setActiveConnection((cur) => (
        cur.protocol === 'packet' && cur.sessionType === 'cms'
          ? cur
          : { sessionType: 'cms', protocol: 'packet' }
      ));
    }
  }, [statusData.status]);

  const radioPanelSelectedConnection = selectedConnection ?? (pinRadioPanel ? activeConnection : null);
  const radioPanelMode = computePanelMode({
    sidebarSelected: radioPanelSelectedConnection,
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
    if (!(activeConnection.sessionType === 'cms' && activeConnection.protocol === 'telnet')) {
      setSelectedConnection(activeConnection);
      return;
    }
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
  }, [activeConnection, queryClient, connecting]);

  const onAbort = useCallback(() => {
    // Fire-and-forget (tuxlink-9z2): the abort shuts the connecting socket; the
    // in-flight cms_connect promise then resolves (Cancelled) and its `finally`
    // clears `connecting`. The session log carries the "Aborting…" line.
    void invoke('cms_abort');
  }, []);

  // Native titlebar: mock B shows "Tuxlink — Inbox". Track the active folder.
  useEffect(() => {
    try {
      void getCurrentWindow().setTitle(`Tuxlink — ${folderLabel(selectedFolder, userFolders)}`);
    } catch {
      /* no Tauri runtime (tests) — title is cosmetic */
    }
  }, [selectedFolder, userFolders]);

  // The parsed message the reading pane is showing — drives menu/accelerator
  // Reply/Reply All/Forward. Same query key as MessageView's useMessage, so
  // TanStack dedupes (no extra IPC). `data` is undefined when nothing is selected.
  const { data: openMessage } = useMessage(selectedMessage);

  // tuxlink-ca5x: archive the open message. Best-effort — a backend failure
  // logs server-side; the operator can retry, and the next refetch resyncs
  // the row state. No-op when nothing is selected or the open message is
  // already in Archive. Clears the selection after a successful move so the
  // reading pane goes empty (the moved row no longer lives in the current folder).
  const archiveOpen = useCallback(async () => {
    if (!selectedMessage) return;
    if (selectedMessage.folder === 'archive') return;
    try {
      await invoke('mailbox_move', {
        from: selectedMessage.folder,
        to: 'archive',
        id: selectedMessage.id,
      });
      // Invalidate both the source and the destination folder lists so the
      // moved row disappears from the source and appears in Archive on the
      // next refetch. The broader `['mailbox']` invalidation hits both at once.
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      setSelectedMessage(null);
    } catch {
      /* surfaced via Rust logs; next refetch resyncs */
    }
  }, [selectedMessage, queryClient]);

  // tuxlink-f62f: move the open message to any folder (system OR user). Used
  // by the reading-pane "Move ▾" dropdown. Self-target is a no-op the UI
  // also suppresses by disabling the current-folder row in the picker.
  const moveOpen = useCallback(async (to: MailboxFolderRef) => {
    if (!selectedMessage) return;
    if (selectedMessage.folder === to) return;
    try {
      await invoke('mailbox_move', {
        from: selectedMessage.folder,
        to,
        id: selectedMessage.id,
      });
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      setSelectedMessage(null);
    } catch {
      /* surfaced via Rust logs; next refetch resyncs */
    }
  }, [selectedMessage, queryClient]);

  // tuxlink-ejph: explicit-id move + archive handlers for the right-click
  // context menu on a message row. Unlike moveOpen/archiveOpen above, these
  // act on the right-clicked message (NOT necessarily the selected one), so
  // they take id + fromFolder as args. Also handle the case where the
  // right-clicked message is currently open (the reading pane should clear).
  const moveByIdToFolder = useCallback(async (
    id: string,
    fromFolder: MailboxFolderRef,
    toFolder: MailboxFolderRef,
  ) => {
    if (fromFolder === toFolder) return;
    try {
      await invoke('mailbox_move', { from: fromFolder, to: toFolder, id });
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      // Clear selection if the moved message was the one open — its folder
      // changed under the reading pane.
      setSelectedMessage((cur) => (cur?.id === id ? null : cur));
    } catch {
      /* surfaced via Rust logs */
    }
  }, [queryClient]);

  const archiveByIdAndFolder = useCallback(async (
    id: string,
    fromFolder: MailboxFolderRef,
  ) => {
    if (fromFolder === 'archive') return;
    try {
      await invoke('mailbox_move', { from: fromFolder, to: 'archive', id });
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      setSelectedMessage((cur) => (cur?.id === id ? null : cur));
    } catch {
      /* surfaced via Rust logs */
    }
  }, [queryClient]);

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
    archive: () => { void archiveOpen(); },
    // tuxlink-j0m3: fire the webview's native print dialog when a message
    // is open. No-op otherwise — Ctrl+P on an empty reading pane would
    // print the bare chrome and is rarely useful. The print stylesheet
    // (which drops the dashboard/sidebar/statusbar from the printed page)
    // is a follow-up; the unstyled output is still readable for the
    // "save this message" use case.
    print: () => { if (openMessage) window.print(); },
    toggleStatusBar: () => setShowStatusBar((s) => !s),
    toggleRadioPanel: () => setPinRadioPanel((s) => !s),
    // tuxlink-u4ky: don't clear selectedConnection on folder switch.
    // selectedConnection drives the right-hand radio panel mount; the post-P2
    // design (see comment below on onSelectConnection) says it's independent
    // of folder navigation, but this pre-P2-era line was clobbering the
    // running connection's panel any time the operator switched folders.
    selectFolder: (folder) => { setSelectedFolder(folder); setSelectedMessage(null); },
    setScheme: (id) => { applyColorScheme(id); saveColorScheme(id); },
    openSettings: () => setSettingsOpen(true),
    openThemeDesigner: () => setThemeDesignerOpen(true),
    openAbout: () => setAboutOpen(true),
    // tuxlink-0gsy / spec §4.1: Help → Documentation opens the separate
    // Tauri webview at /help instead of the old inline modal. The command
    // is idempotent (single-instance — re-clicks focus the existing window).
    openHelp: () => {
      void invoke('help_window_open').catch((err) => {
        // The Help menu item should never become a no-op; log so an
        // unexpected runtime failure surfaces in the operator's console
        // rather than miss silently.
        console.error('help_window_open failed:', err);
      });
    },
    // tuxlink-qjgx Task 8: Help → Logging opens the Logging window.
    openLogging: () => {
      void invoke('logging_window_open').catch((err) => {
        console.error('logging_window_open failed:', err);
      });
    },
    reportIssue: () => {
      // tuxlink-qjgx Task 8: Report Issue flow — auto-export + pre-filled
      // GitHub URL. The controller handles Save As → export → browser open
      // and drives the ReportIssueModal state machine (spec §8.5).
      reportIssueController.start();
    },
    openCatalogRequest: () => setCatalogRequestOpen(true),
    openCatalogBuilder: () => setCatalogBuilderOpen(true),
    openGribRequest: () => setGribRequestOpen(true),
    quit: () => { void invoke('app_quit'); },
  }), [onConnect, openMessage, archiveOpen, reportIssueController]);

  // The Archive button render gate: only show when something is selected AND
  // it's not already in Archive (where archive is a no-op). MessageView reads
  // the absence of onArchive as "don't render the button."
  const onArchiveMessage = (selectedMessage && selectedMessage.folder !== 'archive')
    ? archiveOpen
    : undefined;

  const onMenuAction = useCallback((id: string) => dispatchMenuAction(id, handlers), [handlers]);
  useAccelerators(onMenuAction);

  const onSelectFolder = useCallback((folder: MailboxFolderRef) => {
    setSelectedFolder(folder);
    setSelectedMessage(null);
    // tuxlink-u4ky: selectedConnection deliberately preserved — see the
    // comment on onSelectConnection below for the post-P2 independence
    // contract. Clearing it here closed the running radio panel on every
    // sidebar folder click (smoke-walk regression 2026-06-05).
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
    setActiveConnection(conn);
  }, []);

  // tuxlink-268k (Codex P3): stabilize the two inline FolderSidebar
  // callbacks so the React.memo wrap actually skips re-renders. Before
  // these existed inline at the call site, the shallow-compare always
  // failed on every shell render even when the sidebar's visible state
  // hadn't changed.
  const onCreateFolder = useCallback(() => {
    setNewFolderOpen(true);
  }, []);
  const onFolderContextMenu = useCallback(
    (slug: string, x: number, y: number) => {
      const f = userFolders.find((uf) => uf.slug === slug);
      if (f) setFolderCtxMenu({ folder: f, x, y });
    },
    [userFolders],
  );

  const onSelectMessage = useCallback(
    (id: string) => {
      // When a search is active, the clicked row may live in a folder other
      // than the sidebar's selectedFolder. Look up the row's own folder
      // from the search results; fall back to the sidebar folder for the
      // regular folder-scoped browse case.
      const hit = searchResultMessages?.find((m) => m.id === id);
      const folder = (hit?.folder as MailboxFolder | undefined) ?? selectedFolder;
      if (folder === 'drafts') {
        setSelectedMessage(null);
        void invoke('compose_window_open', { draftId: id });
        return;
      }
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
        activeConnection.protocol === 'packet',
        // Operator smoke 2026-05-31: the prior hard-coded `0` made the ribbon
        // callsign show `<base>-0` regardless of the configured SSID. Source
        // the SSID from the shared packet config so the ribbon, status bar,
        // and PacketRadioPanel all agree on `<base>-<ssid>`.
        effectiveCall(statusData.callsign, packetConfig.ssid),
      ),
    [statusData.status, activeConnection, statusData.callsign, packetConfig.ssid],
  );

  // Ribbon override for active non-packet radio transports (ARDOP / VARA).
  // Mirrors the packet override: when the operator's last-active transport is a
  // radio modem but the live backend is idle/disconnected (e.g. after closing
  // the radio pane), the ribbon should still name that transport instead of
  // falling back to the generic config-derived "Idle · <configTransport>" label.
  // Packet was already handled via `packetUi`; ARDOP/VARA were not — that was
  // the smoke-walk item 38 residual gap. Live Connecting/Connected/Listening
  // states already name the transport via formatConnectionState, so this only
  // overrides the idle/disconnected fallback.
  const radioConn = useMemo<{ label: string; tone: StatusTone } | null>(() => {
    const RADIO_LABELS: Record<string, string> = {
      'ardop-hf': 'ARDOP HF',
      'vara-hf': 'VARA HF',
      'vara-fm': 'VARA FM',
    };
    const label = RADIO_LABELS[activeConnection.protocol];
    if (!label) return null;
    const status = statusData.status ?? null;
    if (status === null || status.kind === 'Disconnected') {
      return { label: `${label} · not connected`, tone: 'idle' };
    }
    return null;
  }, [activeConnection, statusData.status]);

  return (
    <div className="layout-b" data-testid="app-shell-root">
      <TitleBar folderLabel={folderLabel(selectedFolder, userFolders)} />
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
            <Suspense fallback={null}>
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
            </Suspense>
          )}
        </div>
        <DashboardRibbon
          data={statusData}
          onConnect={onConnect}
          connecting={connecting}
          onAbort={onAbort}
          packet={packetUi}
          radioConn={radioConn}
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
          userFolders={userFolders}
          onCreateFolder={onCreateFolder}
          onDropMessage={moveByIdToFolder}
          onFolderContextMenu={onFolderContextMenu}
          selectedConnection={selectedConnection}
          onSelectConnection={onSelectConnection}
        />
        <MessageList
          folder={selectedFolder}
          messages={visibleMessages}
          selectedId={selectedMessage?.id ?? null}
          onSelect={onSelectMessage}
          notConnected={search.isActive ? false : notConnected}
          matchHighlights={searchHighlights}
          showFolderTag={searchIsCrossFolder}
          sortState={sortState}
          onSortStateChange={onSortStateChange}
          userFolders={userFolders}
          onMoveMessage={moveByIdToFolder}
          onArchiveMessage={archiveByIdAndFolder}
        />
        {(() => {
          // tuxlink-djnl: shared render fragment for the reading pane. When
          // nothing is selected, render the eager MessageViewEmpty directly
          // (no lazy fetch — cold-start sees this). When a message IS
          // selected, gate the lazy MessageView behind Suspense with a
          // loading-specific fallback (tuxlink-268k Codex P3: previously
          // used MessageViewEmpty, which flashed the "Select a message"
          // copy under a highlighted row during the brief chunk fetch).
          const readingPane = selectedMessage
            ? (
                <Suspense fallback={<MessageViewLoading />}>
                  <MessageView selectedMessage={selectedMessage} onArchive={onArchiveMessage} userFolders={userFolders} onMove={moveOpen} />
                </Suspense>
              )
            : <MessageViewEmpty />;
          if (selectedConnection === null) {
            return readingPane;
          }
          if (!isBuilt(selectedConnection)) {
            return <StubPanel sessionType={selectedConnection.sessionType} protocol={selectedConnection.protocol} />;
          }
          const { sessionType, protocol } = selectedConnection;
          if (sessionType === 'cms' && protocol === 'telnet') {
            // P2: Telnet UI now lives in the right-hand TelnetRadioPanel.
            // The reading pane falls back to messages so the operator
            // can read mail while the connection panel handles transport.
            return readingPane;
          }
          if (sessionType === 'cms' && protocol === 'packet') {
            // P3: PacketRadioPanel owns the Packet dial UI in the right
            // radio panel; reading pane falls back to mail (same pattern
            // as Telnet (P2) and ARDOP (P4)).
            return readingPane;
          }
          if (sessionType === 'cms' && protocol === 'ardop-hf') {
            // P4: the ArdopRadioPanel owns the ARDOP HF dial UI; the
            // reading pane falls back to mail (same pattern as Telnet,
            // P2). Eliminates the P1 dual-mount of placeholder + ArdopDock.
            return readingPane;
          }
          if (sessionType === 'p2p' && protocol === 'packet') {
            // P3 (P2P branch): same — PacketRadioPanel handles the dial UI.
            return readingPane;
          }
          if (sessionType === 'p2p' && protocol === 'telnet') {
            // P2P Telnet (tuxlink-0pnb): TelnetP2pRadioPanel owns the dial
            // UI in the right panel; reading pane falls back to mail, same
            // pattern as Telnet CMS (P2) and Packet P2P (P3).
            return readingPane;
          }
          if (protocol === 'vara-hf' || protocol === 'vara-fm') {
            // tuxlink-dfmf Phase 2 + tuxlink-kb3s: VaraRadioPanel owns the
            // VARA dial UI in the right panel under either CMS or P2P
            // intent; reading pane falls back to mail (same pattern as
            // Telnet/Packet/ARDOP). Phase 2 surfaces TCP transport +
            // config; RF CONNECT arrives in Phase 3.
            return readingPane;
          }
          // Built but unhandled — defensive stub
          return <StubPanel sessionType={sessionType} protocol={protocol} />;
        })()}
        {/* Per-mode radio panels. Telnet (P2), Packet (P3), ARDOP HF (P4),
            and VARA HF/FM (Phase 2 — tuxlink-dfmf) ship their real
            implementations; any other mode (none today) would fall through
            to the placeholder. The P1 dual-mount of ArdopDock + placeholder
            for ARDOP HF is GONE — the ArdopRadioPanel covers the full
            dial-and-live-state surface on its own. */}
        {radioPanelMode && radioPanelMode.kind === 'telnet' && radioPanelMode.intent === 'cms' && (
          <Suspense fallback={null}>
            <TelnetRadioPanel
              onClose={() => {
                setSelectedConnection(null);
                setPinRadioPanel(false);
              }}
            />
          </Suspense>
        )}
        {radioPanelMode && radioPanelMode.kind === 'telnet' && radioPanelMode.intent === 'p2p' && (
          <Suspense fallback={null}>
            <TelnetP2pRadioPanel
              onClose={() => {
                setSelectedConnection(null);
                setPinRadioPanel(false);
              }}
            />
          </Suspense>
        )}
        {radioPanelMode && radioPanelMode.kind === 'packet' && (
          <Suspense fallback={null}>
            <PacketRadioPanel
              intent={radioPanelMode.intent}
              baseCall={statusData.callsign}
              onClose={() => {
                setSelectedConnection(null);
                setPinRadioPanel(false);
              }}
            />
          </Suspense>
        )}
        {radioPanelMode && radioPanelMode.kind === 'ardop-hf' && (
          <Suspense fallback={null}>
            <ArdopRadioPanel
              onClose={() => {
                setSelectedConnection(null);
                setPinRadioPanel(false);
              }}
            />
          </Suspense>
        )}
        {radioPanelMode &&
          (radioPanelMode.kind === 'vara-hf' || radioPanelMode.kind === 'vara-fm') && (
            <Suspense fallback={null}>
              <VaraRadioPanel
                mode={radioPanelMode}
                onClose={() => {
                  setSelectedConnection(null);
                  setPinRadioPanel(false);
                }}
              />
            </Suspense>
          )}
        {radioPanelMode &&
          radioPanelMode.kind !== 'telnet' &&
          radioPanelMode.kind !== 'packet' &&
          radioPanelMode.kind !== 'ardop-hf' &&
          radioPanelMode.kind !== 'vara-hf' &&
          radioPanelMode.kind !== 'vara-fm' && (
            <PlaceholderRadioPanel
              mode={radioPanelMode}
              onClose={() => {
                setSelectedConnection(null);
                setPinRadioPanel(false);
              }}
            />
          )}
      </div>

      <StatusBar
        show={showStatusBar}
        unread={counts.inbox ?? 0}
        outboxQueued={outbox.messages.length}
      />

      {/* tuxlink-perf-coldstart: lazy-mounted overlays. Each module + its CSS
       *  loads on first open; subsequent opens reuse the cached chunk. The
       *  Suspense fallback is null so a click-to-open never flashes a spinner
       *  on top of the shell — Vite chunks for these panels are tiny so the
       *  network/disk wait is effectively the JS evaluate phase. */}
      {settingsOpen && (
        <Suspense fallback={null}>
          <SettingsPanel open={true} onClose={() => setSettingsOpen(false)} />
        </Suspense>
      )}

      {themeDesignerOpen && (
        <Suspense fallback={null}>
          <ThemeDesigner open={true} onClose={() => setThemeDesignerOpen(false)} />
        </Suspense>
      )}

      {aboutOpen && (
        <Suspense fallback={null}>
          <AboutDialog open={true} onClose={() => setAboutOpen(false)} />
        </Suspense>
      )}

      {catalogRequestOpen && (
        <Suspense fallback={null}>
          <CatalogRequestPanel onClose={() => setCatalogRequestOpen(false)} />
        </Suspense>
      )}

      {catalogBuilderOpen && (
        <Suspense fallback={null}>
          <CatalogBuilderPanel onClose={() => setCatalogBuilderOpen(false)} />
        </Suspense>
      )}

      {gribRequestOpen && (
        <Suspense fallback={null}>
          <GribRequestPanel onClose={() => setGribRequestOpen(false)} />
        </Suspense>
      )}

      {newFolderOpen && (
        <Suspense fallback={null}>
          <NewFolderDialog
            open={true}
            onClose={() => setNewFolderOpen(false)}
            onCreated={(slug) => {
              // Navigate to the new folder so the operator sees their creation
              // succeed (matches the create-and-select expectation of every
              // mail client). The folder is empty until messages are moved in.
              setSelectedFolder(slug);
              setSelectedMessage(null);
            }}
          />
        </Suspense>
      )}

      {renameFolder !== null && (
        <Suspense fallback={null}>
          <RenameFolderDialog
            folder={renameFolder}
            onClose={() => setRenameFolder(null)}
          />
        </Suspense>
      )}

      {deleteFolder !== null && (
        <Suspense fallback={null}>
          <DeleteFolderDialog
            folder={deleteFolder}
            onClose={() => setDeleteFolder(null)}
            onDeleted={(slug) => {
              // If the operator was viewing the now-gone folder, navigate back
              // to Inbox so they don't sit on a slug that no longer resolves.
              if (selectedFolder === slug) {
                setSelectedFolder('inbox');
                setSelectedMessage(null);
              }
            }}
          />
        </Suspense>
      )}

      {folderCtxMenu && (
        <FolderContextMenu
          folder={folderCtxMenu.folder}
          x={folderCtxMenu.x}
          y={folderCtxMenu.y}
          onRename={() => setRenameFolder(folderCtxMenu.folder)}
          onDelete={() => setDeleteFolder(folderCtxMenu.folder)}
          onClose={() => setFolderCtxMenu(null)}
        />
      )}

      {savedSearchesOpen && (
        <Suspense fallback={null}>
          <SavedSearchesPanel onClose={() => setSavedSearchesOpen(false)} />
        </Suspense>
      )}

      {/* tuxlink-qjgx Task 8: Report Issue modal — auto-export + pre-filled
       *  GitHub URL (spec §8.5). Not lazy-loaded: the modal state machine
       *  drives visibility (idle = no DOM output); the component is tiny. */}
      <ReportIssueModal
        state={reportIssueState}
        onClose={() => setReportIssueState({ kind: 'idle' })}
      />
    </div>
  );
}
