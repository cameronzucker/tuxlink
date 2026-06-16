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
import { deriveIdentityFilterOptions } from '../mailbox/identityFilter';
import { selectionToFolderItems, dropId, dropIds } from '../mailbox/bulkSelection';
import { type SortState, loadSortState, saveSortState } from '../mailbox/messageSort';
import { useMailbox, useMailboxChangeEvents } from '../mailbox/useMailbox';
import { DRAFTS_CHANGED_EVENT, listDraftMessages } from '../mailbox/draftMailbox';
import { isNotConfigured } from '../mailbox/types';
import type { MailboxFolder, MailboxFolderRef, MessageMeta } from '../mailbox/types';
import { useUserFolders, useMoveUserFolder } from '../mailbox/useUserFolders';
import { useContacts } from '../contacts/useContacts';
import { ContactsPanel } from '../contacts/ContactsPanel';
import { FolderContextMenu } from '../mailbox/FolderContextMenu';
import type { UserFolder } from '../mailbox/types';
import type { MessageMetaDto } from '../search/types';
import { DEV_SELECTED } from '../mailbox/devFixture';
import { FolderSidebar } from '../mailbox/FolderSidebar';
import type { ConnectionKey } from '../mailbox/FolderSidebar';
import { DashboardRibbon } from './DashboardRibbon';
import { useIdentityList, useActiveIdentity, useIdentitySwitch } from './useIdentities';
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
const UninstallCleanupDialog = lazy(() =>
  import('./UninstallCleanupDialog').then((m) => ({ default: m.UninstallCleanupDialog })),
);
// tuxlink-lqw2: Tools → Verify CMS Connection. Inline overlay that runs the
// connect-only verify_cms_connection probe (shared with the wizard's Step 3).
const VerifyCmsDialog = lazy(() =>
  import('./VerifyCmsDialog').then((m) => ({ default: m.VerifyCmsDialog })),
);
// tuxlink-gife: propagation-aware Find a Station overlay (sibling panel, not a
// main-content view). Supersedes the location-aware Catalog Builder (a2gd).
const StationFinderPanel = lazy(() =>
  import('../catalog/StationFinderPanel').then((m) => ({ default: m.StationFinderPanel })),
);
// tuxlink-eymu: unified Request Center overlay (catalog browse + WLE inquiries
// + Saildocs GRIB). Replaces the Catalog Request menu item and absorbs GRIB as
// an inner view.
const RequestCenter = lazy(() =>
  import('../request/RequestCenter').then((m) => ({ default: m.RequestCenter })),
);
// tuxlink-bsiy: inline pending-message selection panel ("Review Pending
// Messages"). Event-driven — useInboundSelection (below) subscribes to the
// b2f-event channel and surfaces a prompt; the panel only paints when a
// proposal arrives, so it's off the cold-start critical path like the others.
const InboundSelectionPanel = lazy(() =>
  import('../connections/InboundSelectionPanel').then((m) => ({ default: m.InboundSelectionPanel })),
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
import { connectFor, abortFor, MissingTargetError } from '../connections/connectDispatch';
import { emitGatewayPrefill } from '../favorites/prefillEvent';
import type { FavoriteDial } from '../favorites/types';
import { StubPanel } from '../connections/StubPanel';
import { useInboundSelection } from '../connections/useInboundSelection';
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
const loadTelnetPostOfficeRadioPanel = () =>
  import('../radio/modes/TelnetPostOfficeRadioPanel').then((m) => ({ default: m.TelnetPostOfficeRadioPanel }));
const loadPacketRadioPanel = () =>
  import('../radio/modes/PacketRadioPanel').then((m) => ({ default: m.PacketRadioPanel }));
const loadArdopRadioPanel = () =>
  import('../radio/modes/ArdopRadioPanel').then((m) => ({ default: m.ArdopRadioPanel }));
const loadVaraRadioPanel = () =>
  import('../radio/modes/VaraRadioPanel').then((m) => ({ default: m.VaraRadioPanel }));
const TelnetRadioPanel = lazy(loadTelnetRadioPanel);
const TelnetP2pRadioPanel = lazy(loadTelnetP2pRadioPanel);
const TelnetPostOfficeRadioPanel = lazy(loadTelnetPostOfficeRadioPanel);
const PacketRadioPanel = lazy(loadPacketRadioPanel);
// tuxlink-2f2n Task 14: APRS tactical-chat inline surface, reached via the
// sidebar's APRS Chat pseudo-folder (mirrors the Contacts pseudo-folder mount).
// Inline only — NO new window. Lazy because it's off the cold-start path: it
// only paints when the operator selects the APRS Chat row.
const loadAprsChatPanel = () =>
  import('../aprs/AprsChatPanel').then((m) => ({ default: m.AprsChatPanel }));
const AprsChatPanel = lazy(loadAprsChatPanel);
// tuxlink-6vgt: the heard-positions map expands into the reading pane (left of
// the chat dock) when toggled. Lazy so the MapLibre stack only loads on first
// open — same cold-start discipline as the chat panel.
const loadAprsPositionsMap = () =>
  import('../aprs/AprsPositionsMap').then((m) => ({ default: m.AprsPositionsMap }));
const AprsPositionsMap = lazy(loadAprsPositionsMap);
// tuxlink-2f2n Plan 2: APRS chat is re-homed from the sidebar pseudo-folder into
// the shared right dock (chat ⇄ modem). AppShell lifts one useAprsChat instance
// so the status-strip control (unread/listening) + the dock panel share state.
import { useAprsChat } from '../aprs/useAprsChat';
import { useAprsPositions } from '../aprs/useAprsPositions';
import { countUnread } from '../aprs/aprsUnread';
import { AprsDockTabs } from '../aprs/AprsDockTabs';
// The always-live UV-Pro device control strip (tuxlink-ve3j). Capability-gated:
// rendered into AprsChatPanel's controlStrip slot ONLY when the operator has
// declared the native UV-Pro profile (linkKind === 'UvproNative'). A generic KISS
// TNC has no control surface, so it shows plain chat with no strip.
import { UvproControlStrip } from '../uvpro/UvproControlStrip';
// The APRS connect surface (bd-tuxlink-ckmb): a compact status strip in the
// dock header that hosts transport+radio selection and Connect/Disconnect for
// ALL transports (the fresh-install path the in-panel toggle couldn't satisfy).
import { AprsConnectStrip } from '../aprs/AprsConnectStrip';
import type { ModemLinkFields } from '../radio/sections/ModemLinkSection';
import type { PacketLinkKind } from '../packet/packetTypes';
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
import './compactShell.css'; // FZ-M1 compact rules (tuxlink-h7q7) — must follow AppShell.css for equal-specificity overrides to win on order
import { useViewport } from './useViewport';
import { RadioDrawer } from './RadioDrawer';
import { deriveDrawerSessionState } from './drawerSessionState';

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
  // Folders section `+` button). tuxlink-ka3z: `newFolderParent` carries the
  // parent folder when the dialog was opened via "New subfolder here" (null =
  // top-level create).
  const [newFolderOpen, setNewFolderOpen] = useState(false);
  const [newFolderParent, setNewFolderParent] = useState<UserFolder | null>(null);
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
  // FZ-M1 compact mode (tuxlink-h7q7): the radio panel becomes a collapsible
  // push-drawer below the breakpoint. `isCompact` drives the `.compact` root
  // class (non-layout signal); `drawerOpen` toggles the drawer's 4th-column
  // width (closed=44px grip, open=400px panel). Manual open only (operator
  // chose plain Option A — no auto-open).
  const { isCompact } = useViewport();
  const [drawerOpen, setDrawerOpen] = useState(false);
  // APRS tactical chat — lifted here (single instance) so the status-strip
  // control (unread/listening) and the dock panel share one state. (spec §1,§3)
  const aprs = useAprsChat();
  // tuxlink-6vgt: heard-station positions, lifted alongside the chat so the
  // reading-pane map and the dock share one subscription.
  const aprsPositions = useAprsPositions();
  const [aprsOpen, setAprsOpen] = useState(false);
  // Whether the heard-positions map is expanded into the reading-pane region.
  const [aprsMapOpen, setAprsMapOpen] = useState(false);
  const [dockTab, setDockTab] = useState<'aprs' | 'modem'>('aprs');
  const [aprsSeenAt, setAprsSeenAt] = useState(0);
  const aprsUnread = countUnread(aprs.messages, aprsSeenAt);
  const openAprsChat = useCallback(() => {
    setAprsOpen(true);
    setDockTab('aprs');
    setAprsSeenAt(Date.now());
  }, []);
  // Inline GPS/privacy settings overlay (tuxlink-39b), opened from Tools→Settings.
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Inline theme designer overlay (tuxlink-vgth), opened from View → Color
  // Scheme → Customize…. Same backdrop pattern as SettingsPanel.
  const [themeDesignerOpen, setThemeDesignerOpen] = useState(false);
  // Inline About overlay (tuxlink-35g0), opened from the Help menu.
  // Help → Documentation now opens a separate Tauri webview window via
  // help_window_open (tuxlink-0gsy / spec §4); no in-process state.
  const [aboutOpen, setAboutOpen] = useState(false);
  // Inline uninstall cleanup dialog (tuxlink-uodl), opened from Help.
  const [uninstallCleanupOpen, setUninstallCleanupOpen] = useState(false);
  // tuxlink-lqw2: inline Verify CMS Connection overlay, opened from Tools.
  const [verifyCmsOpen, setVerifyCmsOpen] = useState(false);
  // tuxlink-a2gd: inline Catalog Builder ("Find a Gateway"), opened from Message → Find a Gateway.
  const [catalogBuilderOpen, setCatalogBuilderOpen] = useState(false);
  // tuxlink-eymu: Request Center overlay. Carries the initial inner view;
  // null = closed. Opened from Message → Request Center… ('home') and from
  // Message → GRIB File Request… ('grib').
  const [requestCenter, setRequestCenter] = useState<{ initialView: 'home' | 'browse' | 'grib' } | null>(null);
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

  // Mailbox identity filter (Task 11, tuxlink-noa0). `null` = "All identities".
  // Options derived from the identity list (declared below as `identityList`).
  const [identityFilter, setIdentityFilter] = useState<string | null>(null);

  // tuxlink-etxt Task 11: multi-row selection state. Cleared whenever the
  // active folder changes so stale ids from a previous folder can't bleed
  // through to a bulk command against a different folder's messages.
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  useEffect(() => { setSelectedIds(new Set()); }, [selectedFolder]);

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
      void loadTelnetPostOfficeRadioPanel();
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
  // tuxlink-qxqj: the redesigned mailbox bar surfaces outbox-queue depth so
  // the operator knows what's waiting on the next CMS connect without
  // navigating to the Outbox folder. Cheap query — the 10s refetch matches
  // the other sidebar badge queries; no extra IPC burden.
  const outbox = useMailbox('outbox');
  // tuxlink-ca5x + tuxlink-etxt: Archive participates in read/unread state,
  // so its sidebar badge follows Inbox semantics (unread only), not total.
  const archive = useMailbox('archive');
  useMailboxChangeEvents();
  // tuxlink-f62f: operator-created user folders, rendered in the sidebar's
  // Folders section. Backend reads `<root>/.folders.json`.
  const { folders: userFolders } = useUserFolders();
  // tuxlink-ka3z: re-parent mutation for the context-menu "Move to" + drag-drop.
  const moveFolder = useMoveUserFolder();
  // tuxlink-raez (A7): contacts count for the sidebar's Address → Contacts
  // pseudo-folder badge. Sourced from useContacts, NOT the mailbox `counts`
  // memo — `'contacts'` is a pseudo-folder, not a MailboxFolder.
  const { contacts } = useContacts();
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
  // Drafts = local draft count, Archive = unread count. Sent intentionally has
  // no total badge: it is history, not an actionable queue/unread surface.
  // tuxlink-gp8b: Outbox was wired into the sidebar by
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
      // tuxlink-etxt: Archive badge = unread count (matches Inbox badge semantics).
      // User-folder count badges are intentionally deferred — they'd need a per-folder
      // N+1 query; user-folder unread still surfaces in-list via the unread row style.
      archive: archive.messages.filter((m) => m.unread).length,
    }),
    [inbox.messages, outbox.messages, draftMessages, archive.messages],
  );

  // Status data (callsign / grid / connection) — single poll, shared by the
  // dashboard ribbon, the status bar, and the window title.
  const statusData = useStatusData();

  // Packet config — loaded once at AppShell and shared with the ribbon (callsign
  // SSID suffix + inline editor) AND the PacketRadioPanel (which reads its own
  // config and emits writes; the shared listener here picks those up). Operator
  // smoke 2026-05-31 caught that the prior code hardcoded SSID=0 in the ribbon.
  const packetConfig = usePacketConfig();

  // bd-tuxlink-ckmb: APRS connect surface wiring. The dock-header AprsConnectStrip
  // composes ONE connect/disconnect sequence per transport here, so the strip
  // itself stays presentational.
  //   - UvproNative: the engine rides the already-connected UV-Pro session, so
  //     connect = uvpro.connect() THEN aprs_listen_start (two steps); disconnect
  //     = aprs_listen_stop THEN uvpro.disconnect().
  //   - KISS (Tcp/Serial/Bluetooth): the engine opens the link itself, so
  //     connect = aprs_listen_start (one step); disconnect = aprs_listen_stop.
  // Backend rejects (no active identity) propagate to the strip's inline alert.
  // The connect/disconnect SEQUENCE below invokes the uvpro commands directly
  // (not via useUvproControl().connect(), which swallows errors into its own
  // `error` state) so a failure REJECTS and the connect strip surfaces it inline.
  // The UvproControlStrip rendered as the connected-native detail manages its own
  // useUvproControl instance for live status + channel switching.
  const aprsLinkKind = packetConfig.config?.linkKind ?? null;
  const aprsRadioLabel = ((): string | null => {
    const c = packetConfig.config;
    if (!c) return null;
    switch (c.linkKind) {
      case 'Tcp':
        return c.tcpHost && c.tcpPort ? `${c.tcpHost}:${c.tcpPort}` : 'TCP KISS';
      case 'Serial':
        return c.serialDevice ?? 'USB TNC';
      case 'Bluetooth':
        return c.btMac ? `BT ${c.btMac}` : 'Bluetooth';
      case 'UvproNative':
        return c.btMac ? `UV-Pro ${c.btMac}` : 'UV-Pro (native)';
      default:
        return null;
    }
  })();
  // The most-recent link-persist promise. onAprsConnect awaits it before
  // aprs_listen_start so the backend reads the JUST-PERSISTED link, not a stale
  // one (Codex adrev 2026-06-14 P1 race: setLink's packet_config_set is async).
  const aprsLinkPersist = useRef<Promise<void>>(Promise.resolve());
  // The transport the LIVE listener actually came up on (set on a successful
  // connect, cleared on disconnect). Teardown keys off THIS, not the editable
  // aprsLinkKind — otherwise changing the picker while listening would skip the
  // UV-Pro session cleanup (Codex adrev 2026-06-14 P1). null = not listening.
  const aprsActiveTransport = useRef<PacketLinkKind | null>(null);
  const onAprsLinkChange = useCallback(
    (fields: ModemLinkFields) => {
      aprsLinkPersist.current = packetConfig.setLink(fields);
    },
    [packetConfig],
  );
  const onAprsConnect = useCallback(async () => {
    // Wait for the picked link to actually persist before arming.
    await aprsLinkPersist.current;
    if (aprsLinkKind === 'UvproNative') {
      // Ride the native session: connect it first (rejects propagate), then arm
      // the listener. If arming fails (e.g. no active identity), roll the
      // session back so a failed connect never leaves the UV-Pro connected.
      await invoke('uvpro_connect', {});
      try {
        await invoke('aprs_listen_start');
      } catch (err) {
        await invoke('uvpro_disconnect').catch(() => undefined);
        throw err;
      }
    } else {
      await invoke('aprs_listen_start');
    }
    // Record the transport the listener actually came up on for teardown.
    aprsActiveTransport.current = aprsLinkKind;
  }, [aprsLinkKind]);
  const onAprsDisconnect = useCallback(async () => {
    const active = aprsActiveTransport.current;
    try {
      await invoke('aprs_listen_stop');
    } finally {
      // Clean up the UV-Pro session even if stopping the engine threw — keyed to
      // the transport that was actually live, not the (possibly edited) picker.
      if (active === 'UvproNative') {
        await invoke('uvpro_disconnect').catch(() => undefined);
      }
      aprsActiveTransport.current = null;
    }
  }, []);

  // Status-bar APRS slider (tuxlink-l0z5): the ribbon's on/off switch starts/stops
  // the listener via the SAME composed sequences the dock connect-strip uses, so
  // both surfaces drive one backend state. Turning ON also opens the dock so the
  // operator lands on the live chat (and, on failure, on the connect-strip that
  // surfaces why + offers the transport picker). `listening` stays backend-truth:
  // a reject never optimistically flips the switch — the aprs-listening:change
  // event is the only thing that does.
  const [aprsToggling, setAprsToggling] = useState(false);
  const onToggleAprsListening = useCallback(async () => {
    if (aprsToggling) return;
    // Not listening + NO radio configured: there is nothing to start. Rather than
    // fire a connect that fails silently (the tuxlink-ube7 "does nothing" report),
    // just open the APRS panel — its connect strip auto-expands the radio picker
    // when there's no link, so the operator lands exactly where they set one up.
    if (!aprs.listening && aprsLinkKind == null) {
      openAprsChat();
      return;
    }
    setAprsToggling(true);
    try {
      if (aprs.listening) {
        await onAprsDisconnect();
      } else {
        openAprsChat();
        await onAprsConnect();
      }
    } catch {
      // Backend truth: a failed start leaves the indicator Off; the dock (opened
      // above) shows the connect strip + its inline error for retry.
    } finally {
      setAprsToggling(false);
    }
  }, [aprsToggling, aprs.listening, aprsLinkKind, onAprsConnect, onAprsDisconnect, openAprsChat]);

  // Phase 7 (tuxlink-noa0): identity list + active session + switch mutation for
  // the dashboard's inline IdentitySwitcher. The list/active queries feed the
  // closed chip + dropdown; the mutation authenticates (= switches) and
  // invalidates both queries on success (see useIdentitySwitch). QueryClientProvider
  // is the same ancestor useMailbox / useStatusData already rely on.
  const identityList = useIdentityList();
  const activeIdentity = useActiveIdentity();
  const identitySwitch = useIdentitySwitch();
  // Stable handler (the memo'd DashboardRibbon must not re-render on every 2s
  // status poll) that also RESETS the mutation after it settles, so the typed
  // credential held in the mutation's `variables` does not linger in the
  // MutationCache (default gcTime 5m) past the switch. The switcher reads the
  // error from the thrown rejection it catches, so the reset loses no UI state.
  const switchMutateAsync = identitySwitch.mutateAsync;
  const switchReset = identitySwitch.reset;
  const onSwitchIdentity = useCallback(
    async (args: { callsign: string; credential: string; tacticalLabel: string | null }) => {
      try {
        await switchMutateAsync(args);
      } finally {
        switchReset();
      }
    },
    [switchMutateAsync, switchReset],
  );
  // Task 11 (tuxlink-noa0): toolbar identity-filter options derived from the
  // same identity list (no second backend call). "All identities" + one entry
  // per FULL callsign + one per tactical label.
  const identityFilterOptions = useMemo(
    () => deriveIdentityFilterOptions(identityList.data ?? null),
    [identityList.data],
  );

  // tuxlink-bsiy: inbound pending-message selection ("Review Pending Messages").
  // Subscribes to the b2f-event channel for `inbound_proposals_offered`; when a
  // proposal arrives, `inbound.prompt` is non-null and the inline panel mounts
  // below. The operator's choice resolves via cms_resolve_inbound_selection.
  const inbound = useInboundSelection();

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
  // tuxlink-gife: VARA HF/FM modems now consume station prefill (VaraRadioPanel
  // listens via listenGatewayPrefill + sets its RMS-gateway target), so the
  // Find-a-Station map's VARA channels — the HF majority — are usable too.
  const catalogPrefillMode =
    radioPanelMode?.kind === 'packet'
      ? 'packet'
      : radioPanelMode?.kind === 'ardop-hf'
      ? 'ardop-hf'
      : radioPanelMode?.kind === 'vara-hf'
      ? 'vara-hf'
      : radioPanelMode?.kind === 'vara-fm'
      ? 'vara-fm'
      : undefined;

  // Close the radio panel entirely (drop the connection + unpin). Shared by the
  // per-mode panels' close buttons (DRY) and also collapses the compact drawer
  // so a re-opened panel starts closed (tuxlink-h7q7).
  const closeRadioPanel = useCallback(() => {
    setSelectedConnection(null);
    setPinRadioPanel(false);
    setDrawerOpen(false);
  }, []);

  // radioPanelMode is DERIVED — it can go null when a modem session ends on its
  // own (not only via the close button). Reset the compact drawer whenever the
  // panel unmounts (F8 Claude adrev). tuxlink-813d operator smoke #1/#3: ALSO
  // auto-open the drawer when a panel newly appears (or the mode changes) in
  // compact — selecting a modem mode, or Ctrl+Shift+M (which forces the panel
  // via the pin), should open the drawer immediately instead of leaving it
  // collapsed off-screen. Keyed on a stable kind:intent string (computePanelMode
  // returns a fresh object each render) so a deliberate collapse of an UNCHANGED
  // mode persists; a NEW mode (or first mount) re-opens.
  const panelKey = radioPanelMode ? `${radioPanelMode.kind}:${radioPanelMode.intent}` : null;
  const prevPanelKey = useRef(panelKey);
  useEffect(() => {
    if (panelKey === null) {
      setDrawerOpen(false);
    } else if (isCompact && panelKey !== prevPanelKey.current) {
      setDrawerOpen(true);
    }
    prevPanelKey.current = panelKey;
  }, [panelKey, isCompact]);

  // CMS connect: run one exchange (send outbox + receive), then refresh the
  // mailbox so any downloaded messages appear. The button lives in the ribbon;
  // progress + any failure reason surface in the session log (emitted by the
  // backend), not beside the button.
  const queryClient = useQueryClient();
  const [connecting, setConnecting] = useState(false);
  // tuxlink-pmp5: the "review pending inbound before download" preference, shown
  // as the inline "On connect" control in the dashboard ribbon. null until
  // config_read resolves (the ribbon treats null as the on/Review default).
  const [reviewInbound, setReviewInbound] = useState<boolean | null>(null);

  const onConnect = useCallback(async () => {
    // Codex #1: don't start a second connect while one is in flight. The Connect
    // button is disabled, but the F5 / Ctrl+Shift+O accelerator also routes here.
    // The backend single-flight guard is the hard guarantee; this just avoids a
    // spurious "already in progress" error line on a double-press.
    if (connecting) return;
    // tuxlink-vu97: the ribbon Connect now fires the LAST-SELECTED mode's full
    // send/receive (connect + exchange) for ARDOP / VARA / packet too — not just
    // Telnet-CMS — with the radio pane kept CLOSED. The prior navigate-only
    // branch (setSelectedConnection → open the pane, dial nothing) is gone:
    // connectFor replicates each panel's exact connect+exchange invoke sequence,
    // reading the operator-configured target the panels persist to localStorage.
    setConnecting(true);
    try {
      await connectFor(activeConnection);
      await queryClient.invalidateQueries({ queryKey: ['mailbox'] });
    } catch (e) {
      // A backend connect/exchange failure surfaces in the session log + the
      // connection-status ribbon (emitted by the backend) — nothing inline
      // beside the button. The one purely-frontend failure is MissingTargetError
      // (no persisted target for an RF mode). tuxlink-nnjz: route it through the
      // session_log_append command so it lands in the log window (a visible
      // 'warn' row with the actionable message) instead of a console-only warn
      // that left the operator seeing Connect silently "do nothing." The fix is
      // to open the mode's panel and set a target, which persists it for the
      // next ribbon Connect.
      if (e instanceof MissingTargetError) {
        console.warn(`Connect: ${e.message}`);
        void invoke('session_log_append', { level: 'warn', message: e.message }).catch(
          () => {
            /* backend absent (pre-bootstrap) — console.warn above is the fallback */
          },
        );
      }
    } finally {
      setConnecting(false);
    }
  }, [activeConnection, queryClient, connecting]);

  const onAbort = useCallback(() => {
    // tuxlink-vu97: abort the LAST-SELECTED mode (cms_abort / modem_ardop_disconnect
    // / vara_close_session / packet→cms_abort). Fire-and-forget (tuxlink-9z2): the
    // abort shuts the connecting socket; the in-flight connectFor promise then
    // resolves (Cancelled) and its `finally` clears `connecting`. The session log
    // carries the "Aborting…" line.
    void abortFor(activeConnection);
  }, [activeConnection]);

  // tuxlink-pmp5: load the review-inbound preference once so the ribbon's "On
  // connect" control reflects the persisted choice. Reads the LIVE config via the
  // same command SettingsPanel used; a failure leaves it null (Review default).
  useEffect(() => {
    let mounted = true;
    invoke<{ review_inbound_before_download: boolean }>('config_read')
      .then((c) => { if (mounted) setReviewInbound(c.review_inbound_before_download); })
      .catch(() => { /* leave null → ribbon shows the Review default */ });
    return () => { mounted = false; };
  }, []);

  // Persist the operator's "On connect" choice. Optimistic (mirrors the prior
  // SettingsPanel toggle); revert on failure so the ribbon never lies about the
  // persisted state. config_set_review_inbound also refreshes the live backend.
  const onReviewInboundChange = useCallback((enabled: boolean) => {
    setReviewInbound(enabled);
    void invoke('config_set_review_inbound', { enabled }).catch(() => {
      setReviewInbound(!enabled);
    });
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
      // A moved row leaves the view; drop it from the selection set so it can't
      // strand the bulk bar on an invisible row (matters now that an
      // out-of-selection right-click resets the selection to that single row).
      setSelectedIds((cur) => dropId(cur, id));
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
      setSelectedIds((cur) => dropId(cur, id));
    } catch {
      /* surfaced via Rust logs */
    }
  }, [queryClient]);

  // tuxlink-etxt Task 12+13: single-message read/unread toggle — wired from the
  // context-menu (T12) and the U key (T13). Mirrors the invoke + invalidate
  // pattern of the other per-message handlers; the try/catch keeps an unhandled
  // rejection from surfacing in the UI (failure is recoverable via next refetch).
  const setMessageReadState = useCallback(async (id: string, folder: MailboxFolderRef, read: boolean) => {
    try {
      await invoke('message_set_read_state', { folder, id, read });
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      void queryClient.invalidateQueries({ queryKey: ['search'] });
    } catch {
      /* surfaced via Rust logs; next refetch resyncs */
    }
  }, [queryClient]);

  // tuxlink-etxt Task 11: bulk read/unread for selected rows. Each id is mapped
  // to its own folder (present on the row when search is cross-folder; falls back
  // to the active folder for single-folder views). Mirrors the invoke + invalidate
  // pattern used by the existing single-message move/archive handlers above.
  //
  // Selection is intentionally retained after a bulk action; the operator clears
  // it via the ✕ button or by switching folders — do not auto-clear on success.
  const bulkSetReadState = useCallback(async (ids: Set<string>, read: boolean) => {
    // selectionToFolderItems maps each id to its own folder and drops stale ids
    // (Fix 3, #499): a row removed between select and act must never fall back
    // to selectedFolder for an unknown message — that could target the wrong
    // folder in a cross-folder search view.
    const items = selectionToFolderItems(ids, visibleMessages, selectedFolder);
    try {
      await invoke('message_set_read_state_bulk', { items, read });
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      // Fix 1: also invalidate search results so unread state stays current
      // when a bulk read/unread action is taken while a search view is active.
      void queryClient.invalidateQueries({ queryKey: ['search'] });
    } catch {
      /* surfaced via Rust logs; next refetch resyncs */
    }
  }, [visibleMessages, selectedFolder, queryClient]);

  // tuxlink-l80q: bulk move (and Archive = move-to-archive) for the selected
  // rows. Drives the bulk bar's Move ▾ / Archive AND the selection-mode context
  // menu. Same cross-folder id→folder mapping + stale-id filter as the read
  // handler; additionally drops items already in the destination (a no-op move
  // — e.g. a cross-folder hit whose own folder equals `to`).
  //
  // Unlike bulk read/unread (which retains the selection), a move removes the
  // rows from the current view, so the moved ids are dropped from the selection
  // and the reading pane clears if the open message was among them. Mirrors the
  // single moveByIdToFolder/archiveByIdAndFolder handlers' selectedMessage clear.
  const bulkMoveToFolder = useCallback(async (ids: Set<string>, to: MailboxFolderRef) => {
    const items = selectionToFolderItems(ids, visibleMessages, selectedFolder)
      .filter((it) => it.folder !== to);
    if (items.length === 0) return;
    try {
      await invoke('message_move_bulk', { items, to });
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      void queryClient.invalidateQueries({ queryKey: ['search'] });
      // Drop the WHOLE requested set from the selection (Codex P2): items that
      // moved, plus any stale ids that selectionToFolderItems filtered out
      // (rows gone from the view before the action) — leaving them selected
      // would strand the bulk bar count on invisible rows. The reading pane
      // only clears if the OPEN message actually moved.
      const movedIds = new Set(items.map((it) => it.id));
      setSelectedIds((cur) => dropIds(cur, ids));
      setSelectedMessage((cur) => (cur && movedIds.has(cur.id) ? null : cur));
    } catch {
      /* surfaced via Rust logs; next refetch resyncs */
    }
  }, [visibleMessages, selectedFolder, queryClient]);

  const bulkArchive = useCallback(
    (ids: Set<string>) => bulkMoveToFolder(ids, 'archive'),
    [bulkMoveToFolder],
  );

  const handlers: MenuHandlers = useMemo(() => ({
    openCompose: () => { void invoke('compose_window_open', { draftId: newDraftId() }); },
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
    openUninstallCleanup: () => setUninstallCleanupOpen(true),
    // tuxlink-lqw2: Tools → Verify CMS Connection opens the inline probe overlay.
    verifyCms: () => setVerifyCmsOpen(true),
    reportIssue: () => {
      // tuxlink-qjgx Task 8: Report Issue flow — auto-export + pre-filled
      // GitHub URL. The controller handles Save As → export → browser open
      // and drives the ReportIssueModal state machine (spec §8.5).
      reportIssueController.start();
    },
    openCatalogBuilder: () => setCatalogBuilderOpen(true),
    openRequestCenter: (initialView = 'home') => setRequestCenter({ initialView }),
    quit: () => { void invoke('app_quit'); },
  }), [openMessage, archiveOpen, reportIssueController]);

  const editDraft = useCallback((draftId: string) => {
    void invoke('compose_window_open', { draftId });
  }, []);

  // The Archive button render gate: only show when something is selected AND
  // it's not already in Archive (where archive is a no-op). Local Drafts are
  // not backend mailbox messages, so they get an explicit Edit Draft action
  // instead of Archive/Move.
  const onArchiveMessage = (
    selectedMessage
    && selectedMessage.folder !== 'archive'
    && selectedMessage.folder !== 'drafts'
  )
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

  // Find-a-Station "Use →" (tuxlink-s0r1): arm the matching modem on demand so
  // the operator doesn't have to open it first. Opening a panel is UI, not TX —
  // RADIO-1 is honored by the panel's own Connect button. The prefill is retained
  // (prefillEvent) so the just-opened panel consumes it once it mounts + subscribes.
  // We close the finder so the operator lands on the armed, prefilled modem.
  const handleStationUse = useCallback(
    (dial: FavoriteDial) => {
      // RadioMode and ProtocolId share the same string set; a station channel is
      // never 'telnet'. CMS = connect to the dialed RMS gateway over that protocol.
      onSelectConnection({ sessionType: 'cms', protocol: dial.mode as ConnectionKey['protocol'] });
      emitGatewayPrefill(dial);
      setCatalogBuilderOpen(false);
    },
    [onSelectConnection],
  );

  // tuxlink-268k (Codex P3): stabilize the two inline FolderSidebar
  // callbacks so the React.memo wrap actually skips re-renders. Before
  // these existed inline at the call site, the shallow-compare always
  // failed on every shell render even when the sidebar's visible state
  // hadn't changed.
  const onCreateFolder = useCallback(() => {
    // Top-level create from the "+" button: clear any subfolder parent context.
    setNewFolderParent(null);
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

  // The per-mode radio-panel body. Shared by two mount contexts (tuxlink-iehg):
  // the bare dock (radioPanelMode-only, !aprsOpen) and the modem tab inside the
  // shared APRS dock surface (aprsOpen && dockTab === 'modem'). Exactly one
  // per-mode panel renders, selected by radioPanelMode; null collapses to
  // nothing. Telnet (P2), Packet (P3), ARDOP HF (P4), and VARA HF/FM (Phase 2 —
  // tuxlink-dfmf) ship real implementations; any other mode falls through to the
  // placeholder. The P1 dual-mount of ArdopDock + placeholder for ARDOP HF is
  // GONE — the ArdopRadioPanel covers the full dial-and-live-state surface.
  const radioBody = (
    <>
      {radioPanelMode && radioPanelMode.kind === 'telnet' && radioPanelMode.intent === 'cms' && (
        <Suspense fallback={null}>
          <TelnetRadioPanel
            onClose={closeRadioPanel}
          />
        </Suspense>
      )}
      {radioPanelMode && radioPanelMode.kind === 'telnet' && radioPanelMode.intent === 'p2p' && (
        <Suspense fallback={null}>
          <TelnetP2pRadioPanel
            onClose={closeRadioPanel}
          />
        </Suspense>
      )}
      {radioPanelMode && radioPanelMode.kind === 'telnet' && radioPanelMode.intent === 'post-office' && (
        <Suspense fallback={null}>
          <TelnetPostOfficeRadioPanel mode="local" onClose={closeRadioPanel} />
        </Suspense>
      )}
      {radioPanelMode && radioPanelMode.kind === 'telnet' && radioPanelMode.intent === 'network-po' && (
        <Suspense fallback={null}>
          <TelnetPostOfficeRadioPanel mode="network" onClose={closeRadioPanel} />
        </Suspense>
      )}
      {radioPanelMode && radioPanelMode.kind === 'packet' && (
        <Suspense fallback={null}>
          <PacketRadioPanel
            intent={radioPanelMode.intent}
            baseCall={statusData.callsign}
            onClose={closeRadioPanel}
            onFindGateway={() => setCatalogBuilderOpen(true)}
          />
        </Suspense>
      )}
      {radioPanelMode && radioPanelMode.kind === 'ardop-hf' && (
        <Suspense fallback={null}>
          <ArdopRadioPanel
            mode={radioPanelMode}
            onClose={closeRadioPanel}
            onFindGateway={() => setCatalogBuilderOpen(true)}
          />
        </Suspense>
      )}
      {radioPanelMode &&
        (radioPanelMode.kind === 'vara-hf' || radioPanelMode.kind === 'vara-fm') && (
          <Suspense fallback={null}>
            <VaraRadioPanel
              mode={radioPanelMode}
              onClose={closeRadioPanel}
              onFindGateway={() => setCatalogBuilderOpen(true)}
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
            onClose={closeRadioPanel}
          />
        )}
    </>
  );

  return (
    <div className={`layout-b${isCompact ? ' compact' : ''}`} data-testid="app-shell-root">
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
          reviewInbound={reviewInbound}
          onReviewInboundChange={onReviewInboundChange}
          aprs={{
            listening: aprs.listening,
            unread: aprsUnread,
            onOpen: openAprsChat,
            onToggleListening: onToggleAprsListening,
            toggleBusy: aprsToggling,
          }}
          identities={identityList.data ?? null}
          activeIdentity={activeIdentity.data ?? null}
          onSwitchIdentity={onSwitchIdentity}
        />
      </div>

      <div
        className={`panes${radioPanelMode !== null || aprsOpen ? ' panes--with-dock' : ''}${drawerOpen ? ' drawer-open' : ''}`}
        data-testid="shell-panes"
      >
        <FolderSidebar
          compact={isCompact}
          selectedFolder={selectedFolder}
          onSelectFolder={onSelectFolder}
          counts={counts}
          contactsCount={contacts.length}
          userFolders={userFolders}
          onCreateFolder={onCreateFolder}
          onDropMessage={moveByIdToFolder}
          onBulkDropMessage={bulkMoveToFolder}
          onFolderContextMenu={onFolderContextMenu}
          onReparentFolder={(slug, parentSlug) => moveFolder.mutate({ slug, parentSlug })}
          selectedConnection={selectedConnection}
          onSelectConnection={onSelectConnection}
        />
        {/* M8 (tuxlink-raez / A8): the Contacts pseudo-folder replaces BOTH the
            MessageList column AND the reading pane with the inline ContactsPanel
            list/detail surface. The early-return wraps both — placing it inside
            the reading-pane ternary alone would leave MessageList rendered to the
            left (two list columns). */}
        {selectedFolder === 'contacts' ? (
          <ContactsPanel />
        ) : (
          <>
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
          selectedIds={selectedIds}
          onSelectionChange={setSelectedIds}
          onBulkSetReadState={bulkSetReadState}
          onBulkMove={bulkMoveToFolder}
          onBulkArchive={bulkArchive}
          onSetReadState={setMessageReadState}
          identityFilter={identityFilter}
          onIdentityFilterChange={setIdentityFilter}
          identityFilterOptions={identityFilterOptions}
        />
        {(() => {
          // tuxlink-6vgt: when the APRS dock is open AND its Map toggle is on,
          // the heard-positions map EXPANDS INTO the reading-pane region (left
          // of the right-side chat dock). The MessageList column stays; only
          // the reading pane is replaced by the map. Closing the toggle (or the
          // dock) restores the normal reading pane. A later issue makes this a
          // pop-out window — this in-pane render does not preclude that.
          if (aprsOpen && aprsMapOpen) {
            return (
              <Suspense fallback={null}>
                {/* tuxlink-dwzu: the operator grid is the first-run center +
                    recenter target for the positions map (null when unset). */}
                <AprsPositionsMap
                  positions={aprsPositions.positions}
                  operatorGrid={statusData.grid ?? undefined}
                />
              </Suspense>
            );
          }
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
                  <MessageView
                    selectedMessage={selectedMessage}
                    onArchive={onArchiveMessage}
                    userFolders={userFolders}
                    onMove={selectedMessage.folder === 'drafts' ? undefined : moveOpen}
                    onEditDraft={editDraft}
                    radioDrawerOpen={isCompact && drawerOpen}
                  />
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
          if ((sessionType === 'post-office' || sessionType === 'network-po') && protocol === 'telnet') {
            // tuxlink-6c9y: TelnetPostOfficeRadioPanel owns the Post Office
            // dial UI in the right panel; reading pane falls back to mail
            // (same pattern as the other telnet/packet/ardop/vara panes).
            return readingPane;
          }
          // Built but unhandled — defensive stub
          return <StubPanel sessionType={sessionType} protocol={protocol} />;
        })()}
          </>
        )}
        {/* FZ-M1 compact (tuxlink-h7q7): wrap the radio-panel mount in the
            push-drawer. Desktop (>=1366px): RadioDrawer is display:contents, so
            the panel is the 4th grid column exactly as before. Compact: the
            wrapper is the collapsible 4th column (44px grip / 400px open). The
            inner per-mode conditionals are unchanged. */}
        {(radioPanelMode !== null || aprsOpen) && (
          <RadioDrawer
            open={drawerOpen}
            onToggle={() => setDrawerOpen((o) => !o)}
            sessionState={deriveDrawerSessionState({
              connecting,
              status: statusData.status,
              modemIsActive,
            })}
          >

        {/* tuxlink-2f2n Plan 2: the shared dock hosts the APRS chat (default
            tenant once opened) or the modem console; the tab row flips between
            them. The tabs only render once the operator has opened chat;
            otherwise the dock is the modem-only surface it always was.

            tuxlink-iehg: when aprsOpen, the tab row + active body MUST be wrapped
            in ONE `.aprs-dock-surface` element. The drawer (`.radio-drawer` +
            `.radio-drawer-body`) is `display:contents` on desktop, so each direct
            child of the body is promoted to a grid item of `.panes`. The grid
            budgets exactly ONE dock column (`.panes--with-dock`), so two bare
            children (tabs + panel) overflowed: the second landed in an implicit
            track at the bottom-left. The wrapper keeps them as a single grid item
            (a flex column) that fills the one dock column. The `!aprsOpen`
            radio-panel-only path stays a bare grid item — unchanged. */}
        {aprsOpen ? (
          <div className="aprs-dock-surface" data-testid="aprs-dock-surface">
            <AprsDockTabs
              active={dockTab}
              unread={aprsUnread}
              modemEnabled={radioPanelMode !== null}
              onSelect={(tab) => {
                setDockTab(tab);
                if (tab === 'aprs') setAprsSeenAt(Date.now());
              }}
              onClose={() => {
                setAprsOpen(false);
                // Closing the dock collapses the map back to the normal reading pane.
                setAprsMapOpen(false);
              }}
              mapOpen={aprsMapOpen}
              onToggleMap={() => setAprsMapOpen((o) => !o)}
            />
            {dockTab === 'aprs' ? (
              <>
                {/* bd-tuxlink-ckmb: the connect surface lives in the dock-header
                    band, ABOVE the chat, for ALL transports (the fresh-install
                    path the old in-panel Start/Stop toggle couldn't satisfy). */}
                <AprsConnectStrip
                  listening={aprs.listening}
                  linkKind={aprsLinkKind}
                  radioLabel={aprsRadioLabel}
                  allowUvproNative
                  // tuxlink-hoi1 B2: seed the picker from the SAVED link so a
                  // segment tap can't blank the address (e.g. btMac -> null).
                  tcpHost={packetConfig.config?.tcpHost ?? undefined}
                  tcpPort={packetConfig.config?.tcpPort ?? undefined}
                  serialDevice={packetConfig.config?.serialDevice ?? undefined}
                  serialBaud={packetConfig.config?.serialBaud ?? undefined}
                  btMac={packetConfig.config?.btMac ?? undefined}
                  onConnect={onAprsConnect}
                  onDisconnect={onAprsDisconnect}
                  onLinkChange={onAprsLinkChange}
                />
                <Suspense fallback={null}>
                  <AprsChatPanel
                    messages={aprs.messages}
                    send={aprs.send}
                    getConfig={aprs.getConfig}
                    setConfig={aprs.setConfig}
                    controlStrip={
                      // The native UV-Pro device detail (channel/freq/battery)
                      // remains co-presented with chat once connected-native; the
                      // connect strip above subsumes the connect ACTION, this is
                      // the live-device DETAIL only.
                      packetConfig.config?.linkKind === 'UvproNative' ? (
                        <UvproControlStrip />
                      ) : undefined
                    }
                  />
                </Suspense>
              </>
            ) : (
              radioBody
            )}
          </div>
        ) : (
          radioBody
        )}
          </RadioDrawer>
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

      {uninstallCleanupOpen && (
        <Suspense fallback={null}>
          <UninstallCleanupDialog open={true} onClose={() => setUninstallCleanupOpen(false)} />
        </Suspense>
      )}

      {verifyCmsOpen && (
        <Suspense fallback={null}>
          <VerifyCmsDialog open={true} onClose={() => setVerifyCmsOpen(false)} />
        </Suspense>
      )}

      {catalogBuilderOpen && (
        <Suspense fallback={null}>
          <StationFinderPanel
            activePrefillMode={catalogPrefillMode}
            onUse={handleStationUse}
            onClose={() => setCatalogBuilderOpen(false)}
          />
        </Suspense>
      )}

      {requestCenter && (
        <Suspense fallback={null}>
          <RequestCenter initialView={requestCenter.initialView} onClose={() => setRequestCenter(null)} />
        </Suspense>
      )}

      {newFolderOpen && (
        <Suspense fallback={null}>
          <NewFolderDialog
            open={true}
            parentSlug={newFolderParent?.slug}
            parentName={newFolderParent?.displayName}
            onClose={() => {
              setNewFolderOpen(false);
              setNewFolderParent(null);
            }}
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
            childCount={userFolders.filter((f) => f.parentSlug === deleteFolder.slug).length}
            childNames={userFolders
              .filter((f) => f.parentSlug === deleteFolder.slug)
              .map((f) => f.displayName)}
            onClose={() => setDeleteFolder(null)}
            onDeleted={(removedSlugs) => {
              // Cascade-aware (A5): if the operator was viewing the parent OR any
              // cascaded child that's now gone, navigate back to Inbox so they
              // don't sit on a slug that no longer resolves.
              if (removedSlugs.includes(selectedFolder as string)) {
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
          allFolders={userFolders}
          x={folderCtxMenu.x}
          y={folderCtxMenu.y}
          onRename={() => setRenameFolder(folderCtxMenu.folder)}
          onDelete={() => setDeleteFolder(folderCtxMenu.folder)}
          onNewSubfolder={() => {
            // "New subfolder here" — open the create dialog with this folder as
            // the parent (tuxlink-ka3z).
            setNewFolderParent(folderCtxMenu.folder);
            setNewFolderOpen(true);
          }}
          onMoveTo={(parentSlug) =>
            moveFolder.mutate({ slug: folderCtxMenu.folder.slug, parentSlug })
          }
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

      {/* tuxlink-bsiy: inline pending-message selection ("Review Pending
       *  Messages"). Event-driven — mounts only when the backend offers
       *  proposals before download (useInboundSelection). Lazy-loaded; the
       *  panel resolves via cms_resolve_inbound_selection. */}
      {inbound.prompt && (
        <Suspense fallback={null}>
          <InboundSelectionPanel
            proposals={inbound.prompt.proposals}
            onSubmit={inbound.submit}
            onClose={inbound.close}
          />
        </Suspense>
      )}
    </div>
  );
}
