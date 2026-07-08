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
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { MessageList } from '../mailbox/MessageList';
import type { HighlightRange } from '../mailbox/MessageList';
import { selectionToFolderItems, dropId, dropIds } from '../mailbox/bulkSelection';
import { type SortState, loadSortState, saveSortState } from '../mailbox/messageSort';
import { useMailbox, useMailboxChangeEvents } from '../mailbox/useMailbox';
import { DRAFTS_CHANGED_EVENT, listDraftMessages } from '../mailbox/draftMailbox';
import { isNotConfigured } from '../mailbox/types';
import { deleteMessages, restoreMessages, purgeMessage, emptyTrash } from '../mailbox/mailboxCommands';
import type { MailboxFolder, MailboxFolderRef, MessageMeta } from '../mailbox/types';
import { useUserFolders, useMoveUserFolder } from '../mailbox/useUserFolders';
import { useContacts } from '../contacts/useContacts';
import { ContactsPanel } from '../contacts/ContactsPanel';
import { FavoritesPanel } from '../favorites/FavoritesPanel';
import { FAVORITES_QUERY_KEY } from '../favorites/useFavorites';
import { FolderContextMenu } from '../mailbox/FolderContextMenu';
import type { UserFolder } from '../mailbox/types';
import type { MessageMetaDto } from '../search/types';
import { DEV_SELECTED } from '../mailbox/devFixture';
import { FolderSidebar } from '../mailbox/FolderSidebar';
import type { ConnectionKey } from '../mailbox/FolderSidebar';
import { DashboardRibbon } from './DashboardRibbon';
import { CloseBehaviorPrompt } from './CloseBehaviorPrompt';
import { useIdentityList, useActiveIdentity, useIdentitySwitch } from './useIdentities';
import { StatusBar } from './StatusBar';
import { useStatusData, type StatusTone, type ConfigViewDto } from './useStatus';
import { applyColorScheme, saveColorScheme } from './colorScheme';

// tuxlink-perf-coldstart: lazy-load every overlay/dialog panel. None of these
// are on the cold-start critical path — they only paint when the operator
// opens them via a menu or button. Removing them from the eager import graph
// trims the bundle that gates first paint of the main shell. Each lazy panel
// is also gated at its call site (`{flag && <Suspense>…</Suspense>}`) so the
// module + its CSS aren't fetched until the open flag flips true.

// tuxlink-13v2l: Elmer agent pane — lazy-loaded; only fetches when the
// operator opens it (cold-start discipline matches the other overlays here).
const ElmerPane = lazy(() =>
  import('../elmer/ElmerPane').then((m) => ({ default: m.ElmerPane })),
);

const SettingsPanel = lazy(() =>
  import('./SettingsPanel').then((m) => ({ default: m.SettingsPanel })),
);
import type { SectionId as SettingsSectionId } from './SettingsPanel';
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
// tuxlink-l9sq4: Tools → Connect an AI agent. Per-agent MCP copy-paste
// connection commands.
const ConnectAgentModal = lazy(() =>
  import('./ConnectAgentModal').then((m) => ({ default: m.ConnectAgentModal })),
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
const ConfirmPurgeDialog = lazy(() =>
  import('../mailbox/ConfirmPurgeDialog').then((m) => ({ default: m.ConfirmPurgeDialog })),
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
import type { FavoriteDial, StationsFile } from '../favorites/types';
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
// tuxlink-2phz: the source-reactive environmental panel (weather + telemetry)
// is the dock's third tenant. Lazy so its (small) chunk stays off the cold-start
// path — same discipline as the chat/map panels.
const loadEnvPanel = () => import('../aprs/EnvPanel').then((m) => ({ default: m.EnvPanel }));
const EnvPanel = lazy(loadEnvPanel);
// tuxlink-2f2n Plan 2: APRS chat is re-homed from the sidebar pseudo-folder into
// the shared right dock (chat ⇄ modem). AppShell lifts one useAprsChat instance
// so the status-strip control (unread/listening) + the dock panel share state.
import { useAprsChat } from '../aprs/useAprsChat';
import { useEgressArm } from '../security/useEgressArm';
import { useAprsPositions } from '../aprs/useAprsPositions';
import { useEnvStations } from '../aprs/useEnvStations';
import { openStationsWindow } from '../aprs/stationsWindow';
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
import { useModemIsActive, useActiveModemMode } from '../modem/useModemStatus';
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

/// Address-section pseudo-folder labels (bd-tuxlink-kiaa). `'favorites'` and
/// `'contacts'` are pseudo-folder selection keys, not `MailboxFolder`s, so they
/// are not in FOLDER_LABELS; this map title-cases them for the window title.
const PSEUDO_FOLDER_LABELS: Record<string, string> = {
  favorites: 'Favorites',
  contacts: 'Contacts',
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
  // Address-section pseudo-folders (not MailboxFolders, so absent from
  // FOLDER_LABELS) get a proper title-cased window-title label.
  if (folder in PSEUDO_FOLDER_LABELS) return PSEUDO_FOLDER_LABELS[folder];
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
  // tuxlink-wl7n Task 14: pending permanent-delete action waiting for the
  // ConfirmPurgeDialog. null = no dialog open. The `kind` distinguishes the
  // three permanent actions (per-item, bulk, empty-trash) so onConfirm can
  // dispatch to the right handler.
  const [pendingPurge, setPendingPurge] = useState<
    | { kind: 'single'; id: string }
    | { kind: 'bulk'; ids: Set<string> }
    | { kind: 'empty' }
    | null
  >(null);
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
  // Egress ARM surface (MCP phase 3.6): the operator arms/disarms agent
  // send-authority and SEES its live state in the dashboard ribbon. Without an
  // arm, nothing the MCP agent does can egress (the operator-present surface for
  // the egress gate).
  const egressArm = useEgressArm();
  // APRS tactical chat — lifted here (single instance) so the status-strip
  // control (unread/listening) and the dock panel share one state. (spec §1,§3)
  const aprs = useAprsChat();
  // tuxlink-6vgt: heard-station positions, lifted alongside the chat so the
  // reading-pane map and the dock share one subscription.
  const aprsPositions = useAprsPositions();
  // tuxlink-2phz: heard weather + telemetry, merged by callsign. Lifted to the
  // shell (like positions) so the per-channel history ring buffers from launch —
  // opening the Station Data tab later shows the buffered series, not an empty
  // graph that only starts filling on first view.
  // Host role: the main shell accumulates env stations from launch and answers
  // snapshot requests from pop-out windows (tuxlink-hzwc bug #4).
  const envStations = useEnvStations({ snapshotRole: 'host' });
  const [aprsOpen, setAprsOpen] = useState(false);
  // Whether the heard-positions map is expanded into the reading-pane region.
  const [aprsMapOpen, setAprsMapOpen] = useState(false);
  const [dockTab, setDockTab] = useState<'aprs' | 'modem' | 'stations'>('aprs');
  // ni5b: a WX-badge click on the map focuses this station's Station Data card.
  // `focusNonce` increments per click so re-clicking the SAME station re-triggers
  // the scroll (a bare callsign wouldn't change) (Codex review).
  const [focusCall, setFocusCall] = useState<string | null>(null);
  const [focusNonce, setFocusNonce] = useState(0);
  const [aprsSeenAt, setAprsSeenAt] = useState(0);
  // tuxlink-hzwc bug #11: "unread" must count traffic heard WHILE AWAY from the
  // APRS Chat tab (a sense of channel volume), and clear as the operator reads.
  // While the Chat tab is the active, open view, every arriving message is seen
  // in the live feed, so advance the watermark on each new message — the count
  // stays 0 here and only accrues once the operator clicks away. Previously the
  // watermark advanced ONLY on tab-select, so the always-visible status-strip
  // count climbed indefinitely while the operator sat on the open Chat tab.
  const viewingAprsChat = aprsOpen && dockTab === 'aprs';
  useEffect(() => {
    if (viewingAprsChat) setAprsSeenAt(Date.now());
  }, [viewingAprsChat, aprs.messages.length]);
  const aprsUnread = viewingAprsChat ? 0 : countUnread(aprs.messages, aprsSeenAt);
  const openAprsChat = useCallback(() => {
    setAprsOpen(true);
    setDockTab('aprs');
    setAprsSeenAt(Date.now());
  }, []);
  // Inline GPS/privacy settings overlay (tuxlink-39b), opened from Tools→Settings.
  const [settingsOpen, setSettingsOpen] = useState(false);
  // tuxlink-vfb3: which Settings section to open on. undefined → SettingsPanel's
  // own default (Location & GPS). The Tools→Settings→Winlink Account entry opens
  // directly on the 'account' section. Read at mount (the panel remounts on each
  // open, so each entry point lands on the right section).
  const [settingsSection, setSettingsSection] = useState<SettingsSectionId | undefined>(undefined);
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
  // tuxlink-l9sq4: Connect an AI agent modal, opened from Tools.
  const [connectAgentOpen, setConnectAgentOpen] = useState(false);
  // tuxlink-13v2l: Elmer agent pane, opened from Tools → Elmer (or ribbon chip).
  const [elmerOpen, setElmerOpen] = useState(false);
  // tuxlink-9uat6: tracks whether the pane has ever been opened this session so
  // we can keep it MOUNTED (hidden) after close rather than unmounting it. This
  // preserves useElmer's event-listener + conversation state across close→reopen
  // and prevents orphaning a running inference. The pane still does not mount
  // until the first open (lazy-load discipline is intact — elmerEverOpened stays
  // false until the operator first opens the pane).
  const [elmerEverOpened, setElmerEverOpened] = useState(false);
  // tuxlink-1wi5w: when true, open the Model section disclosure on next Elmer
  // pane mount. Reset to false after opening so a second plain "Elmer" open
  // does not re-expand (the state flag belongs to the open action, not the pane).
  const [elmerExpandModel, setElmerExpandModel] = useState(false);
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
  // bd-tuxlink-kiaa: starred-favorites count for the sidebar's Address →
  // Favorites pseudo-folder badge. Same ['favorites'] key FavoritesPanel uses
  // (react-query dedupes the fetch); a pseudo-folder, not a MailboxFolder.
  const favoritesQuery = useQuery({
    queryKey: FAVORITES_QUERY_KEY,
    queryFn: () => invoke<StationsFile>('favorites_read'),
  });
  const favoritesCount = useMemo(
    () => (favoritesQuery.data?.favorites ?? []).filter((f) => f.starred).length,
    [favoritesQuery.data],
  );
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

  // Status-bar APRS control (tuxlink-l0z5; tuxlink-a1j3): the ribbon's on/off
  // switch starts/stops the listener via the SAME composed sequences the dock
  // connect-strip uses, so both surfaces drive one backend state. It is a PURE
  // on/off switch — it does NOT open/close the dock, with ONE exception: the very
  // first time, when no radio is configured yet (nothing to start), it opens the
  // dock to the connect strip's picker for setup. `listening` stays backend-truth:
  // a reject never optimistically flips the switch — the aprs-listening:change
  // event is the only thing that does.
  // tuxlink-28o0: a SINGLE in-flight "connecting" flag, shared by every surface
  // that can start the listener (the status-bar control + the dock connect strip),
  // so they show "Connecting…" together. `listening` was already shared backend
  // truth; this closes the in-flight gap the operator hit (status-bar connect, but
  // the dock strip still read "Connect"). Routed through one wrapper so a connect
  // from EITHER surface flips it.
  const [aprsConnecting, setAprsConnecting] = useState(false);
  const runAprsConnect = useCallback(async () => {
    setAprsConnecting(true);
    try {
      await onAprsConnect();
    } finally {
      setAprsConnecting(false);
    }
  }, [onAprsConnect]);
  const [aprsToggling, setAprsToggling] = useState(false);
  const onToggleAprsListening = useCallback(async () => {
    if (aprsToggling || aprsConnecting) return;
    // Not listening + NO radio configured: there is nothing to start. Rather than
    // fire a connect that fails silently (the tuxlink-ube7 "does nothing" report),
    // just open the APRS panel — its connect strip auto-expands the radio picker
    // when there's no link, so the operator lands exactly where they set one up.
    // This first-run setup is the ONLY case where the control touches the dock.
    if (!aprs.listening && aprsLinkKind == null) {
      openAprsChat();
      return;
    }
    if (aprs.listening) {
      setAprsToggling(true);
      try {
        await onAprsDisconnect();
      } catch {
        // Backend truth: a failed stop leaves `listening` as-is.
      } finally {
        setAprsToggling(false);
      }
    } else {
      // tuxlink-a1j3: pure on/off — start listening with the last-configured radio
      // WITHOUT opening the dock (only the no-config first run above does).
      // tuxlink-28o0: via the shared wrapper so the dock strip shows Connecting… too.
      try {
        await runAprsConnect();
      } catch {
        // Backend truth: a failed start leaves the indicator Off; the reason is in
        // the structured log (tuxlink-xyi7). Open the dock to retry via the strip.
      }
    }
  }, [aprsToggling, aprsConnecting, aprs.listening, aprsLinkKind, runAprsConnect, onAprsDisconnect, openAprsChat]);

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
  // tuxlink-7ppfq (Contract 2): the active-modem panel now tracks the SELECTED
  // protocol (VARA-HF vs ARDOP-HF) instead of a hardcoded `ardop-hf`. Deduped
  // selector — derives from `useModemIsActive` (one fire per transition) and
  // memoizes on `activeConnection`, so the shell never re-renders at the 4 Hz
  // modem-broadcaster cadence.
  const activeModem: RadioPanelMode | null = useActiveModemMode(activeConnection);

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

  // tuxlink-7ppfq (Contract 2): persist the operator's selected connection so
  // the MCP `modem_status` surface can report `selected`. Hook the
  // `activeConnection` STATE TRANSITION (not just `onSelectConnection`) so BOTH
  // writers — the sidebar/favorites click AND the status-driven effect above —
  // are captured; hooking only the click would leave React and config out of
  // sync (the hoi1 multi-writer-clobber class). The hydration gate below stops
  // the initial mount-hydrate value from being immediately re-persisted.
  const activeConnHydrated = useRef(false);
  useEffect(() => {
    let cancelled = false;
    invoke<ConfigViewDto>('config_read')
      .then((cfg) => {
        const sel = cfg?.active_connection;
        if (!cancelled && sel) {
          setActiveConnection({
            sessionType: sel.session_type as ConnectionKey['sessionType'],
            protocol: sel.protocol as ConnectionKey['protocol'],
          });
        }
      })
      .catch(() => { /* best-effort hydrate — fall back to the default */ })
      .finally(() => { activeConnHydrated.current = true; });
    return () => { cancelled = true; };
  }, []);
  useEffect(() => {
    if (!activeConnHydrated.current) return; // don't re-persist the hydrated value
    void invoke('config_set_active_connection', {
      sessionType: activeConnection.sessionType,
      protocol: activeConnection.protocol,
    }).catch(() => { /* perception persistence is best-effort */ });
  }, [activeConnection]);

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
    // tuxlink-hzwc bug #6: closing the console from the dock's Modem tab returns
    // to APRS Chat rather than leaving an empty Modem tab selected.
    setDockTab((t) => (t === 'modem' ? 'aprs' : t));
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

  // tuxlink-wl7n: Delete-to-trash, Restore, and Permanently-delete handlers.
  // Mirror the moveByIdToFolder / archiveByIdAndFolder pattern: call the
  // mailboxCommands façade, then invalidate the affected queries so the UI
  // re-fetches without waiting for the next mailbox:changed event.

  /// Move a message to the Deleted folder (recoverable). No confirm.
  /// Invalidates the source folder and the 'deleted' folder.
  /// Looks up the message's owning identity from visibleMessages so the backend
  /// targets the correct per-identity namespace (Codex #2 fix, tuxlink-wl7n Task 13).
  const deleteByIdAndFolder = useCallback(async (
    id: string,
    fromFolder: MailboxFolderRef,
  ) => {
    if (fromFolder === 'deleted') return; // already in Deleted — no-op
    // Resolve the owning identity from the visible list (includes cross-folder
    // search hits). ParsedMessage lacks identity, so visibleMessages is the sole source.
    const identity = visibleMessages.find((m) => m.id === id)?.identity;
    try {
      await deleteMessages([{ id, folder: fromFolder, identity }]);
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      void queryClient.invalidateQueries({ queryKey: ['search'] });
      setSelectedMessage((cur) => (cur?.id === id ? null : cur));
      setSelectedIds((cur) => dropId(cur, id));
    } catch {
      /* surfaced via Rust logs; next refetch resyncs */
    }
  }, [visibleMessages, queryClient]);

  /// Restore a message from the Deleted folder to its origin folder.
  /// Invalidates the 'deleted' folder and the origin folder (covered by ['mailbox']).
  const restoreById = useCallback(async (id: string) => {
    try {
      await restoreMessages([id]);
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      setSelectedMessage((cur) => (cur?.id === id ? null : cur));
      setSelectedIds((cur) => dropId(cur, id));
    } catch {
      /* surfaced via Rust logs; next refetch resyncs */
    }
  }, [queryClient]);

  /// Permanently delete a single message from the Deleted folder (no recovery).
  /// Opens ConfirmPurgeDialog; the actual purge + invalidation runs in onConfirm.
  const purgeById = useCallback((id: string) => {
    setPendingPurge({ kind: 'single', id });
  }, []);

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

  // tuxlink-wl7n Task 13 (Part B2): bulk Delete, Restore, and Delete-permanently
  // handlers. Mirror bulkMoveToFolder / bulkArchive: build delete items carrying
  // identity, call the mailboxCommands façade, invalidate, drop selection.

  /// Move the selected messages to the Deleted folder (recoverable). No confirm —
  /// delete is like Archive per the approved design. Skips items already in 'deleted'.
  const bulkDelete = useCallback(async (ids: Set<string>) => {
    const items = selectionToFolderItems(ids, visibleMessages, selectedFolder)
      .filter((it) => it.folder !== 'deleted');
    if (items.length === 0) return;
    try {
      // `items` (BulkMessageRef[]) already matches DeleteItem[] structurally —
      // {id, folder, identity?} — so pass it straight through, like bulkMoveToFolder.
      await deleteMessages(items);
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      void queryClient.invalidateQueries({ queryKey: ['search'] });
      const deletedIds = new Set(items.map((it) => it.id));
      setSelectedIds((cur) => dropIds(cur, ids));
      setSelectedMessage((cur) => (cur && deletedIds.has(cur.id) ? null : cur));
    } catch {
      /* surfaced via Rust logs; next refetch resyncs */
    }
  }, [visibleMessages, selectedFolder, queryClient]);

  /// Restore the selected messages from the Deleted folder to their origin folders.
  const bulkRestore = useCallback(async (ids: Set<string>) => {
    const items = selectionToFolderItems(ids, visibleMessages, selectedFolder);
    if (items.length === 0) return;
    try {
      await restoreMessages(items.map((it) => it.id));
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      void queryClient.invalidateQueries({ queryKey: ['search'] });
      setSelectedIds((cur) => dropIds(cur, ids));
      setSelectedMessage((cur) => (cur && ids.has(cur.id) ? null : cur));
    } catch {
      /* surfaced via Rust logs; next refetch resyncs */
    }
  }, [visibleMessages, selectedFolder, queryClient]);

  /// Permanently delete the selected messages from the Deleted folder (no recovery).
  /// Opens ConfirmPurgeDialog; the actual purge loop + invalidation runs in onConfirm.
  const bulkPurge = useCallback((ids: Set<string>) => {
    setPendingPurge({ kind: 'bulk', ids });
  }, []);

  /// Open the ConfirmPurgeDialog for the Empty Trash action.
  const emptyTrashFlow = useCallback(() => {
    setPendingPurge({ kind: 'empty' });
  }, []);

  /// Count of messages that will be permanently deleted, used to drive the
  /// ConfirmPurgeDialog body copy. For 'single' = 1; for 'bulk' = ids.size;
  /// for 'empty' = the visible deleted-folder message count (best available
  /// local count — emptyTrash() returns the real server count after the fact).
  const pendingPurgeCount =
    pendingPurge === null
      ? 0
      : pendingPurge.kind === 'single'
        ? 1
        : pendingPurge.kind === 'bulk'
          ? pendingPurge.ids.size
          : visibleMessages.length; // 'empty' — use loaded trash size

  /// Execute the confirmed permanent-delete action, then close the dialog.
  const handlePurgeConfirm = useCallback(async () => {
    if (!pendingPurge) return;
    const p = pendingPurge;
    try {
      if (p.kind === 'single') {
        await purgeMessage(p.id);
      } else if (p.kind === 'bulk') {
        const items = selectionToFolderItems(p.ids, visibleMessages, selectedFolder);
        for (const it of items) {
          await purgeMessage(it.id);
        }
      } else {
        await emptyTrash();
      }
    } finally {
      // Resync regardless of a mid-loop failure (Task-14 review I1): any message
      // purged before a throw is already gone backend-side, so the cache must
      // refresh or the list shows phantom rows. `['mailbox']` covers the
      // `['mailbox','deleted']` child key. On a throw the propagating error skips
      // the `setPendingPurge(null)` below, so the dialog stays open and shows the
      // error (ConfirmPurgeDialog's own catch); on success it closes.
      void queryClient.invalidateQueries({ queryKey: ['mailbox'] });
      if (p.kind === 'single') {
        setSelectedMessage((cur) => (cur?.id === p.id ? null : cur));
        setSelectedIds((cur) => dropId(cur, p.id));
      } else if (p.kind === 'bulk') {
        setSelectedIds((cur) => dropIds(cur, p.ids));
        setSelectedMessage((cur) => (cur && p.ids.has(cur.id) ? null : cur));
      } else {
        setSelectedIds(new Set());
        // Clear the reading pane if the open message was in Deleted.
        setSelectedMessage((cur) => (cur?.folder === 'deleted' ? null : cur));
      }
    }
    setPendingPurge(null);
  }, [pendingPurge, visibleMessages, selectedFolder, queryClient]);

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
    // tuxlink-wl7n: Message → Delete menubar item. Move the open message to the
    // Deleted folder (Trash). No-op when nothing is open; `deleteByIdAndFolder`
    // itself no-ops when the message is already in 'deleted'. (The Del KEY is
    // handled in the reading pane, MessageViewLoaded — see menuModel.ts.)
    delete: () => {
      if (selectedMessage) void deleteByIdAndFolder(selectedMessage.id, selectedMessage.folder);
    },
    // tuxlink-j0m3: fire the webview's native print dialog when a message
    // is open. No-op otherwise — Ctrl+P on an empty reading pane would
    // print the bare chrome and is rarely useful. The print stylesheet
    // (which drops the dashboard/sidebar/statusbar from the printed page)
    // is a follow-up; the unstyled output is still readable for the
    // "save this message" use case.
    print: () => { if (openMessage) window.print(); },
    toggleStatusBar: () => setShowStatusBar((s) => !s),
    // tuxlink-a1j3: Ctrl+Shift+M (Toggle Radio Panel) always brings up the ONE
    // consolidated dock focused on the Modem tab — keeping APRS Chat / Station
    // Data tabs + the Map toggle present alongside it. No flip-to-Chat, and no
    // tab-less standalone panel via this shortcut. Pressing it while already on
    // the open Modem tab closes the dock (the toggle-off), mirroring the dock
    // close control. Pinning the radio panel gives the Modem tab content when no
    // session is otherwise active.
    toggleRadioPanel: () => {
      if (aprsOpen && dockTab === 'modem') {
        // Toggle-off: close the dock AND unpin, so the radio panel doesn't linger
        // as a standalone surface (the pin would otherwise keep radioPanelMode
        // non-null). An active sidebar-selected session stays visible on its own.
        setAprsOpen(false);
        setAprsMapOpen(false);
        setPinRadioPanel(false);
      } else {
        setAprsOpen(true);
        setPinRadioPanel(true);
        setDockTab('modem');
      }
    },
    setScheme: (id) => { applyColorScheme(id); saveColorScheme(id); },
    openSettings: () => { setSettingsSection(undefined); setSettingsOpen(true); },
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
    // tuxlink-l9sq4: Tools → Connect an AI agent opens the modal.
    openConnectAgent: () => setConnectAgentOpen(true),
    // tuxlink-13v2l: Tools → Elmer opens the Elmer agent pane.
    openElmer: () => { setElmerEverOpened(true); setElmerOpen(true); },
    // tuxlink-1wi5w: Tools → Set up Elmer's model… opens the pane with the
    // Model section expanded. Does NOT touch ConnectAgentModal.
    openElmerModel: () => { setElmerEverOpened(true); setElmerExpandModel(true); setElmerOpen(true); },
    // tuxlink-lqw2: Tools → Verify CMS Connection opens the inline probe overlay.
    verifyCms: () => setVerifyCmsOpen(true),
    reportIssue: () => {
      // tuxlink-uxvn: open the modal in `intro` (explanation + Create report)
      // FIRST, instead of dropping straight into a bare OS Save As. The intro's
      // "Create report" button calls controller.start() (Save As → export →
      // browser open, spec §8.5).
      setReportIssueState({ kind: 'intro' });
    },
    openCatalogBuilder: () => setCatalogBuilderOpen(true),
    openRequestCenter: (initialView = 'home') => setRequestCenter({ initialView }),
    quit: () => { void invoke('app_quit'); },
  }), [openMessage, archiveOpen, selectedMessage, deleteByIdAndFolder, reportIssueController, aprsOpen, dockTab]);

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
    (dial: FavoriteDial, candidates?: FavoriteDial[]) => {
      // RadioMode and ProtocolId share the same string set; a station channel is
      // never 'telnet'. CMS = connect to the dialed RMS gateway over that protocol.
      onSelectConnection({ sessionType: 'cms', protocol: dial.mode as ConnectionKey['protocol'] });
      // tuxlink-8fkkk Task B: carry the ranked QSY-on-fail candidate list so the
      // armed panel can send `qsyCandidates` on Connect.
      emitGatewayPrefill(dial, candidates);
      setCatalogBuilderOpen(false);
    },
    [onSelectConnection],
  );

  // bd-tuxlink-kiaa: Connect from the shell-level Favorites home. Identical
  // open-and-arm path to handleStationUse (FavoriteRow's Connect is pure
  // prefill — RADIO-1), minus the finder close: selectedFolder stays
  // 'favorites' so the operator keeps the favorites list in the content area
  // with the armed modem dock open beside it. The operator clicks the panel's
  // own Send/Receive (the Part 97 consent). No transmit fires here.
  const handleFavoritesConnect = useCallback(
    (dial: FavoriteDial) => {
      onSelectConnection({ sessionType: 'cms', protocol: dial.mode as ConnectionKey['protocol'] });
      emitGatewayPrefill(dial);
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
      // tuxlink-jvtu: the heard-positions map shares the reading-pane grid slot
      // (rendered when aprsOpen && aprsMapOpen). With the map open, the
      // MessageList column stays visible but the reader is covered, so a click
      // only highlighted the row. Close the map so the selected message shows;
      // the operator reopens the map via the dock Map toggle. No-op when the
      // map is already closed (React bails on an unchanged state value).
      setAprsMapOpen(false);
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
          reviewInbound={reviewInbound}
          onReviewInboundChange={onReviewInboundChange}
          aprs={{
            listening: aprs.listening,
            unread: aprsUnread,
            onOpen: openAprsChat,
            onToggleListening: onToggleAprsListening,
            toggleBusy: aprsToggling || aprsConnecting,
          }}
          identities={identityList.data ?? null}
          activeIdentity={activeIdentity.data ?? null}
          onSwitchIdentity={onSwitchIdentity}
          egress={{
            status: egressArm.status,
            onArm: (durationSecs) => { void egressArm.arm(durationSecs); },
            onDisarm: () => { void egressArm.disarm(); },
            busy: egressArm.busy,
            error: egressArm.error,
          }}
          onOpenElmer={() => { setElmerEverOpened(true); setElmerOpen((o) => !o); }}
          elmerOpen={elmerOpen}
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
          favoritesCount={favoritesCount}
          userFolders={userFolders}
          onCreateFolder={onCreateFolder}
          onDropMessage={moveByIdToFolder}
          onBulkDropMessage={bulkMoveToFolder}
          onFolderContextMenu={onFolderContextMenu}
          onReparentFolder={(slug, parentSlug) => moveFolder.mutate({ slug, parentSlug })}
          selectedConnection={selectedConnection}
          onSelectConnection={onSelectConnection}
          onEmptyTrash={emptyTrashFlow}
        />
        {/* M8 (tuxlink-raez / A8): the Contacts pseudo-folder replaces BOTH the
            MessageList column AND the reading pane with the inline ContactsPanel
            list/detail surface. The early-return wraps both — placing it inside
            the reading-pane ternary alone would leave MessageList rendered to the
            left (two list columns). */}
        {selectedFolder === 'contacts' ? (
          <ContactsPanel />
        ) : selectedFolder === 'favorites' ? (
          // bd-tuxlink-kiaa: the Favorites pseudo-folder, like Contacts, replaces
          // BOTH the MessageList column AND the reading pane with the inline
          // cross-mode FavoritesPanel. Connect opens+arms the matching modem dock.
          <FavoritesPanel onConnect={handleFavoritesConnect} />
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
          onDeleteMessage={deleteByIdAndFolder}
          onRestoreMessage={restoreById}
          onPurgeMessage={purgeById}
          selectedIds={selectedIds}
          onSelectionChange={setSelectedIds}
          onBulkSetReadState={bulkSetReadState}
          onBulkMove={bulkMoveToFolder}
          onBulkArchive={bulkArchive}
          onBulkDelete={bulkDelete}
          onBulkRestore={bulkRestore}
          onBulkPurge={bulkPurge}
          onSetReadState={setMessageReadState}
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
              // tuxlink-kkr5: the fallback MUST occupy the reading-pane grid
              // slot. AprsPositionsMap is lazy() and pulls the leaflet +
              // protomaps-leaflet chunk (tuxlink-6kdw), so a `fallback={null}`
              // left column 3 empty during the
              // load — CSS grid auto-flow then reflowed the 400px dock LEFT
              // into the 1fr reader track (measured: 400px → 877px), and it
              // snapped back when the map mounted. That right→left reflow was
              // the visible "bounce". An empty `.aprs-positions-map` placeholder
              // holds the slot (same class = same grid placement + surface bg),
              // so the layout is stable across the chunk load.
              <Suspense
                fallback={<div className="aprs-positions-map" aria-hidden="true" />}
              >
                {/* tuxlink-dwzu: the operator grid is the first-run center +
                    recenter target for the positions map (null when unset). */}
                <AprsPositionsMap
                  positions={aprsPositions.positions}
                  operatorGrid={statusData.grid ?? undefined}
                  envStations={envStations.stations}
                  onFocusStation={(call) => {
                    setDockTab('stations');
                    setFocusCall(call);
                    setFocusNonce((n) => n + 1);
                  }}
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
          // tuxlink-wl7n: derive single-message delete/restore/purge closures
          // from selectedMessage so MessageView doesn't need to know the folder.
          const onDeleteMessage = (
            selectedMessage && selectedMessage.folder !== 'deleted' && selectedMessage.folder !== 'drafts'
          )
            ? () => deleteByIdAndFolder(selectedMessage.id, selectedMessage.folder)
            : undefined;
          const onRestoreMessage = (
            selectedMessage && selectedMessage.folder === 'deleted'
          )
            ? () => restoreById(selectedMessage.id)
            : undefined;
          const onPurgeMessage = (
            selectedMessage && selectedMessage.folder === 'deleted'
          )
            ? () => purgeById(selectedMessage.id)
            : undefined;
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
                    onDelete={onDeleteMessage}
                    onRestore={onRestoreMessage}
                    onDeletePermanently={onPurgeMessage}
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
          if (protocol === 'ardop-hf') {
            // The ArdopRadioPanel owns the ARDOP HF dial UI for EVERY intent
            // (cms / radio-only / p2p) — computePanelMode threads the intent in
            // and the panel's b2f exchange routes on it. The reading pane falls
            // back to mail (same pattern as Telnet P2 and VARA below). Matching
            // on protocol alone — not sessionType==='cms' — keeps radio-only
            // (tuxlink-picr) and any future p2p ARDOP off the defensive stub;
            // radio-only+ardop-hf is a BUILT combo (isBuilt=true), so a
            // "coming soon" stub there was a routing gap, not a real one.
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
              // tuxlink-hzwc bug #6: the dock's Modem tab is always reachable —
              // selecting it brings up the radio console (Telnet Winlink by
              // default), so the keystroke is never the only way to enable it.
              modemEnabled
              stationCount={envStations.stations.length}
              onPopOut={dockTab === 'stations' ? () => void openStationsWindow() : undefined}
              onSelect={(tab) => {
                setDockTab(tab);
                if (tab === 'aprs') setAprsSeenAt(Date.now());
                // Ensure the Modem console has content when selected from the dock
                // (pin is OR'd with an active modem / sidebar selection, so this is
                // a no-op when one is already live).
                if (tab === 'modem') setPinRadioPanel(true);
              }}
              onClose={() => {
                setAprsOpen(false);
                // Closing the dock collapses the map back to the normal reading pane.
                setAprsMapOpen(false);
              }}
              mapOpen={aprsMapOpen}
              onToggleMap={() => setAprsMapOpen((o) => !o)}
            />
            {dockTab === 'stations' ? (
              <Suspense fallback={null}>
                <EnvPanel stations={envStations.stations} focusCall={focusCall} focusNonce={focusNonce} />
              </Suspense>
            ) : dockTab === 'aprs' ? (
              <>
                {/* bd-tuxlink-ckmb: the connect surface lives in the dock-header
                    band, ABOVE the chat, for ALL transports (the fresh-install
                    path the old in-panel Start/Stop toggle couldn't satisfy). */}
                <AprsConnectStrip
                  listening={aprs.listening}
                  externalConnecting={aprsConnecting}
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
                  onConnect={runAprsConnect}
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
          <SettingsPanel open={true} initialSection={settingsSection} onClose={() => setSettingsOpen(false)} />
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

      {/* tuxlink-l9sq4: Connect an AI agent modal — per-agent MCP copy-paste
          connection commands. Lazy-loaded; only fetches on first open. */}
      {connectAgentOpen && (
        <Suspense fallback={null}>
          <ConnectAgentModal open={true} onClose={() => setConnectAgentOpen(false)} />
        </Suspense>
      )}

      {/* tuxlink-13v2l: Elmer agent pane — lazy-loaded; only fetches on first
          open (cold-start discipline). The pane manages its own useElmer state;
          AppShell provides the egress-arm context so the arm chip is consistent
          with the ribbon chip (AC-13).
          tuxlink-9uat6: Mount is gated on elmerEverOpened (not elmerOpen) so the
          pane stays MOUNTED when closed, preserving useElmer event-listeners,
          conversation history, and any in-flight inference run. Visibility is
          controlled via the HTML `hidden` attribute on the wrapper div, which
          produces display:none — identical visually to unmounting, no animation
          lost. ElmerPane's position:fixed root escapes the wrapper's flow; the
          display:none on the wrapper still hides the fixed child. */}
      {elmerEverOpened && (
        <div hidden={!elmerOpen}>
          <Suspense fallback={null}>
            <ElmerPane
              egressStatus={egressArm.status}
              onArm={(durationSecs) => { void egressArm.arm(durationSecs); }}
              onDisarm={() => { void egressArm.disarm(); }}
              onRearm={(durationSecs) => { void egressArm.rearm(durationSecs); }}
              egressBusy={egressArm.busy}
              egressError={egressArm.error}
              onClose={() => { setElmerOpen(false); setElmerExpandModel(false); }}
              expandModel={elmerExpandModel}
            />
          </Suspense>
        </div>
      )}

      {/* tuxlink-5rvp / #882: one-time close-behavior explainer. Self-manages
          its open state via the `show-close-prompt` backend event (no parent
          state flag), so it is always mounted. */}
      <CloseBehaviorPrompt />

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
        onProceed={() => reportIssueController.start()}
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

      {/* tuxlink-wl7n Task 14: permanent-delete confirm modal. Replaces the
       *  prior window.confirm stopgaps for purgeById + bulkPurge, and gates
       *  the new Empty Trash action. Three permanent actions share one modal. */}
      <Suspense fallback={null}>
        <ConfirmPurgeDialog
          open={pendingPurge !== null}
          count={pendingPurgeCount}
          onConfirm={handlePurgeConfirm}
          onCancel={() => setPendingPurge(null)}
        />
      </Suspense>
    </div>
  );
}
