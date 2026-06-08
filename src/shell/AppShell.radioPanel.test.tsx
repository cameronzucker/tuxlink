// AppShell radio-panel visibility tests (renamed from AppShell.modemDock.test
// during radio-panel-shell P1.5). The RadioPanel mounts as the right-hand
// column when ANY of:
//   - a connection entry is selected in the sidebar
//   - any modem is in a non-stopped state
//   - View → Toggle Radio Panel pin is on (Ctrl+Shift+M)
//
// As of P4 (radio-panel-ardop), the legacy ArdopDock is GONE — ARDOP HF
// routes to the new ArdopRadioPanel inside the RadioPanel slot, with no
// secondary mount. tuxlink-dfmf Phase 2 (this session) adds the
// VaraRadioPanel dispatch for vara-hf / vara-fm; the placeholder no
// longer mounts for any built mode. The `panes--with-legacy-dock` class
// is no longer applied to anything.
//
// This file lives separately from AppShell.test.tsx so the panel-mount story
// is readable in isolation. The provider wrapping + Tauri IPC mocks mirror
// the existing AppShell test so the shell mounts cleanly under jsdom.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MessageMeta } from '../mailbox/types';
import { STOPPED, type ModemStatus } from '../modem/types';

// Mock the useModemStatus hook directly so the test controls the modem state
// without touching `invoke('modem_get_status')` or the `listen` event channel.
// tuxlink-sndh: AppShell now consumes `useModemIsActive()` (the focused
// selector) instead of `useModemStatus()`. Derive the boolean from the same
// mock so the existing per-test `mockUseModemStatus.mockReturnValue(...)`
// setup keeps working unchanged.
const mockUseModemStatus = vi.fn();
vi.mock('../modem/useModemStatus', () => ({
  useModemStatus: () => mockUseModemStatus(),
  useModemIsActive: () => mockUseModemStatus().status.state !== 'stopped',
  MODEM_STATUS_EVENT: 'modem:status',
}));

// Tauri IPC mocks — match the existing AppShell.test.tsx setup so the shell
// mounts (DashboardRibbon's useStatusData, SessionLog snapshot, etc.).
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'config_read') return null;
    if (cmd === 'backend_status') return null;
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'message_read') return null;
    // ArdopRadioPanel calls config_get_ardop when Open WebGUI is clicked,
    // but it's safe to return a benign default here so test-time imports
    // don't hit `undefined` and surface noisy errors.
    if (cmd === 'config_get_ardop') return { cmd_port: 8515 };
    // tuxlink-dfmf: VARA panel benign defaults so the panel can mount in
    // shell tests without touching the live VaraSession.
    if (cmd === 'config_get_vara') {
      return { host: '127.0.0.1', cmd_port: 8300, data_port: 8301, bandwidth_hz: null };
    }
    if (cmd === 'vara_status') {
      return { state: 'closed', lastError: null, boundHost: null, boundCmdPort: null };
    }
    if (cmd === 'platform_info') {
      return { arch: 'x86_64', os: 'linux', varaSupported: true };
    }
    if (cmd === 'packet_config_get') {
      return {
        ssid: 7,
        listenDefault: true,
        linkKind: 'Tcp',
        tcpHost: '127.0.0.1',
        tcpPort: 8001,
        serialDevice: null,
        serialBaud: null,
        txdelay: 30,
        persistence: 63,
        slotTime: 10,
        paclen: 128,
        maxframe: 4,
        t1Ms: 3000,
        n2Retries: 10,
      };
    }
    // position_status: PositionStatusDto — no GPS fix, empty grids (null-state).
    // Without this stub, react-query's queryFn receives `undefined` and emits
    // "Query data cannot be undefined" warnings on every poll tick (tuxlink-hnkn).
    if (cmd === 'position_status') return { gps_ready: false, broadcast_grid: '', ui_grid: '' };
    // Search IPC stubs — silence react-query's "undefined queryFn" warning for
    // the useSavedSearches hook that mounts inside the SearchBar in the ribbon.
    if (cmd === 'tauri_search_list_saved') return [];
    if (cmd === 'tauri_search_list_recent') return [];
    return undefined;
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: 'main',
    setTitle: vi.fn(async () => {}),
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    startResizeDragging: vi.fn(async () => {}),
  }),
  ResizeDirection: { North:'North',South:'South',East:'East',West:'West',NorthEast:'NorthEast',NorthWest:'NorthWest',SouthEast:'SouthEast',SouthWest:'SouthWest' },
}));

vi.mock('react-virtuoso', () => ({
  Virtuoso: ({
    data,
    itemContent,
  }: {
    data: MessageMeta[];
    itemContent: (i: number, m: MessageMeta) => unknown;
  }) => (
    <div data-testid="virtuoso-mock">
      {data.map((m, i) => (
        <div key={m.id}>{itemContent(i, m) as ReactNode}</div>
      ))}
    </div>
  ),
}));

vi.mock('../mailbox/useMailbox', () => ({
  useMailboxChangeEvents: () => {},
  useMailbox: (_folder: string) => ({
    messages: [],
    isLoading: false,
    isError: false,
    error: null,
  }),
  isBackendFolder: (f: string) => f === 'inbox' || f === 'outbox' || f === 'sent',
  isUserFolderSlug: (s: string) => /^[a-z0-9-]+$/.test(s) && !s.startsWith('-') && !s.endsWith('-'),
}));

vi.mock('../mailbox/useUserFolders', () => ({
  useUserFolders: () => ({ folders: [], isLoading: false, isError: false, error: null }),
  useCreateUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useDeleteUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRenameUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  USER_FOLDERS_QUERY_KEY: ['userFolders'],
}));

import { AppShell } from './AppShell';

function renderShell() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

// tuxlink-813d D3: the Connections accordion moved into the FolderSidebar
// flyout (only mounted while the rail is expanded). Open the flyout, then
// click the session header + protocol. (Selecting a protocol closes the
// flyout, which does not affect the selected connection.)
function selectConnection(sessTestId: string, protoTestId: string) {
  fireEvent.click(screen.getByTestId('rail-expand-btn'));
  fireEvent.click(screen.getByTestId(sessTestId));
  fireEvent.click(screen.getByTestId(protoTestId));
}

const RUNNING: ModemStatus = {
  state: 'connected-irs',
  peer: 'W7RMS-10',
  mode: '4FSK 500',
  widthHz: 500,
  pttBackend: 'rts',
  snDb: 8.4,
  vuDbfs: -18.0,
  throughputBps: 540,
  bytesRx: 4128,
  bytesTx: 982,
  uptimeSec: 222,
  arqFlags: { busy: false, rx: false, tx: false },
  lastError: null,
  quality: null,
};

describe('<AppShell> radio panel', () => {
  beforeEach(() => {
    mockUseModemStatus.mockReset();
  });

  it('does NOT render the panel when modem is stopped and no sidebar selection', () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    expect(screen.queryByTestId('radio-panel-root')).not.toBeInTheDocument();
    // P4: legacy ArdopDock removed; the testid no longer exists anywhere.
    expect(screen.queryByTestId('ardop-dock-root')).not.toBeInTheDocument();
    // The 3-col grid class swap is absent.
    expect(screen.getByTestId('shell-panes')).not.toHaveClass('panes--with-dock');
    // P4: panes--with-legacy-dock class is gone for good.
    expect(screen.getByTestId('shell-panes')).not.toHaveClass('panes--with-legacy-dock');
  });

  it('renders the ArdopRadioPanel + 4-col grid class when modem is running (ardop-hf)', async () => {
    mockUseModemStatus.mockReturnValue({ status: RUNNING, loading: false, error: null });
    renderShell();
    // tuxlink-twym: radio panels are now React.lazy → findByTestId awaits the
    // dynamic import (Suspense fallback is null, so the synchronous getByTestId
    // would race the resolution).
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    // P4: the placeholder is no longer mounted for ARDOP HF — the
    // ArdopRadioPanel itself owns the slot. SignalSection is one of its
    // mounted children and uniquely identifies the panel.
    expect(screen.queryByTestId('radio-panel-placeholder')).not.toBeInTheDocument();
    expect(screen.getByTestId('signal-section')).toBeInTheDocument();
    expect(screen.getByTestId('shell-panes')).toHaveClass('panes--with-dock');
    // P4: no more legacy-dock class.
    expect(screen.getByTestId('shell-panes')).not.toHaveClass('panes--with-legacy-dock');
  });

  it('renders the ArdopRadioPanel for transient (non-stopped) states like connecting', async () => {
    mockUseModemStatus.mockReturnValue({
      status: { ...STOPPED, state: 'connecting' },
      loading: false,
      error: null,
    });
    renderShell();
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    expect(screen.getByTestId('signal-section')).toBeInTheDocument();
    const shellPanes = screen.getByTestId('shell-panes');
    expect(shellPanes).toHaveClass('panes--with-dock');
    // P4: connecting modem → ArdopRadioPanel only; no separate dock + no
    // legacy-dock class.
    expect(shellPanes).not.toHaveClass('panes--with-legacy-dock');
    expect(screen.queryByTestId('ardop-dock-root')).not.toBeInTheDocument();
  });

  // P4: ARDOP HF selected → ArdopRadioPanel mounts (single mount). The
  // pre-P4 dual-mount of placeholder + legacy ArdopDock is gone.
  it('renders ArdopRadioPanel when ARDOP HF is selected (modem stopped)', async () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    expect(screen.queryByTestId('radio-panel-root')).not.toBeInTheDocument();
    expect(screen.queryByTestId('ardop-dock-root')).not.toBeInTheDocument();
    // Open the flyout, expand Winlink (CMS) accordion, then pick ARDOP HF.
    selectConnection('sess-cms', 'proto-cms-ardop-hf');
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    // ArdopRadioPanel mounts; SignalSection is unique to it among the
    // built panels (Telnet / Packet don't mount SignalSection).
    expect(screen.getByTestId('signal-section')).toBeInTheDocument();
    // No legacy dock anywhere.
    expect(screen.queryByTestId('ardop-dock-root')).not.toBeInTheDocument();
    const shellPanes = screen.getByTestId('shell-panes');
    expect(shellPanes).toHaveClass('panes--with-dock');
    expect(shellPanes).not.toHaveClass('panes--with-legacy-dock');
  });

  // Codex P1 finding (radio-panel-shell): a running ARDOP modem with no
  // sidebar selection (the "operator clicked Close while ARDOP was on-air"
  // scenario) must show the Ardop panel. P4 means the ArdopRadioPanel
  // itself is the mount — no separate dock.
  it('shows ArdopRadioPanel when modem is running with no sidebar selection', async () => {
    mockUseModemStatus.mockReturnValue({ status: RUNNING, loading: false, error: null });
    renderShell();
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    expect(screen.getByTestId('signal-section')).toBeInTheDocument();
    expect(screen.queryByTestId('ardop-dock-root')).not.toBeInTheDocument();
    const shellPanes = screen.getByTestId('shell-panes');
    expect(shellPanes).toHaveClass('panes--with-dock');
    expect(shellPanes).not.toHaveClass('panes--with-legacy-dock');
  });

  // tuxlink-dfmf Phase 2: VARA HF / VARA FM now route to the VaraRadioPanel
  // instead of the placeholder. The host-input testid (`vara-host-input`)
  // is unique to the VARA panel — its presence confirms VaraRadioPanel
  // mounted, and the placeholder's absence confirms the fall-through no
  // longer catches VARA modes.
  it('renders VaraRadioPanel when VARA HF is selected (modem stopped)', async () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    expect(screen.queryByTestId('radio-panel-root')).not.toBeInTheDocument();
    selectConnection('sess-cms', 'proto-cms-vara-hf');
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    expect(screen.getByTestId('vara-host-input')).toBeInTheDocument();
    // The placeholder must NOT mount alongside — VaraRadioPanel owns
    // the slot.
    expect(screen.queryByTestId('radio-panel-placeholder')).not.toBeInTheDocument();
  });

  it('renders VaraRadioPanel when VARA FM is selected (modem stopped)', async () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    selectConnection('sess-cms', 'proto-cms-vara-fm');
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Vara FM');
    expect(screen.queryByTestId('radio-panel-placeholder')).not.toBeInTheDocument();
  });

  // tuxlink-kb3s: P2P VARA HF/FM flipped to built:true. The panel itself
  // is intent-agnostic — it mounts under either CMS or P2P and only the
  // header title's suffix changes (Winlink vs P2P). These tests pin the
  // P2P mount behavior alongside the CMS tests above.
  it('renders VaraRadioPanel with P2P title when VARA HF is selected under P2P', async () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    selectConnection('sess-p2p', 'proto-p2p-vara-hf');
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    expect(screen.getByTestId('vara-host-input')).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Vara HF P2P');
    expect(screen.queryByTestId('radio-panel-placeholder')).not.toBeInTheDocument();
  });

  it('renders VaraRadioPanel with P2P title when VARA FM is selected under P2P', async () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    selectConnection('sess-p2p', 'proto-p2p-vara-fm');
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Vara FM P2P');
    expect(screen.queryByTestId('radio-panel-placeholder')).not.toBeInTheDocument();
  });

  // tuxlink-mnk4: View → Toggle Radio Panel (Ctrl+Shift+M) must actually
  // toggle the panel. The menu item + accelerator have been wired through
  // dispatchMenuAction since the tuxlink-mnk4 fix; the menu item was
  // renamed from "Toggle Radio Dock" in radio-panel-shell P1.7.
  it('Ctrl+Shift+M toggles the panel when the modem is stopped and no sidebar selection', async () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    expect(screen.queryByTestId('radio-panel-root')).not.toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'm', ctrlKey: true, shiftKey: true });
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'm', ctrlKey: true, shiftKey: true });
    expect(screen.queryByTestId('radio-panel-root')).not.toBeInTheDocument();
  });
});
