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

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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
    if (cmd === 'config_get_ardop') {
      return {
        binary: 'ardopcf',
        capture_device: '',
        playback_device: '',
        ptt_serial_path: null,
        cmd_port: 8515,
        bandwidth_hz: null,
        webgui_port: null,
      };
    }
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
    // Favorites surface defaults (B7): FavoritesTabs/useFavorites mount inside
    // ArdopRadioPanel when the modem is stopped. Benign empty defaults so queries
    // resolve cleanly across all tests that select ARDOP HF.
    if (cmd === 'favorites_read') return { schema_version: 1, favorites: [], log: [] };
    if (cmd === 'favorites_recents') return [];
    if (cmd === 'position_current_fix') return { grid: null };
    if (cmd === 'favorite_tod_hint') return null;
    // ArdopRadioPanel listener state (useListenerState) — benign defaults.
    if (cmd === 'ardop_allowed_stations_get') return { allow_all: true, callsigns: [] };
    // ArdopRadioPanel Radio section device pickers — empty lists are valid.
    if (cmd === 'ardop_list_audio_devices') return { captures: [], playbacks: [] };
    if (cmd === 'packet_list_serial_devices') return [];
    // P7: managed packet sound-card picker — empty list is valid (the panel
    // defaults to BYO here since packet_config_get returns linkKind:'Tcp').
    if (cmd === 'packet_list_audio_devices') return [];
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
  useMoveUserFolder: () => ({ mutate: vi.fn(), mutateAsync: vi.fn(), isPending: false }),
  USER_FOLDERS_QUERY_KEY: ['userFolders'],
}));

import { AppShell } from './AppShell';
import { COMPACT_MEDIA_QUERY } from './useViewport';

function renderShell() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

// tuxlink-813d P1 fix: the shell passes `compact={isCompact}` to FolderSidebar.
// jsdom has no `matchMedia` (no global stub in test-setup), so `useViewport`
// returns `isCompact=false` and the shell renders the DESKTOP labeled sidebar —
// the Connections accordion (`sess-*` / `proto-*`) is inline, with no `☰`
// rail-expand button. Click the session header + protocol directly.
function selectConnection(sessTestId: string, protoTestId: string) {
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

  it('renders ArdopRadioPanel with Radio-only title when ARDOP HF is selected under Radio-only', async () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    selectConnection('sess-radio-only', 'proto-radio-only-ardop-hf');
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('ARDOP Radio-only');
    expect(screen.queryByTestId('radio-panel-placeholder')).not.toBeInTheDocument();
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
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('VARA FM');
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
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('VARA HF P2P');
    expect(screen.queryByTestId('radio-panel-placeholder')).not.toBeInTheDocument();
  });

  it('renders VaraRadioPanel with P2P title when VARA FM is selected under P2P', async () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShell();
    selectConnection('sess-p2p', 'proto-p2p-vara-fm');
    expect(await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 })).toBeInTheDocument();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('VARA FM P2P');
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

  // ── Task B7: App-level (production-provider-stack) mount test for Favorites ──
  //
  // The B6 tests wrapped ArdopRadioPanel in a bare QueryClient scaffold. B7
  // mounts the REAL AppShell tree (QueryClientProvider + routing + full shell)
  // so the FavoritesTabs connect surface is exercised through the production
  // provider stack. Mirrors the "ARDOP HF selected, modem stopped" test above
  // for the panel-selection mechanic; adds favorites-data routing (M9) and
  // asserts RADIO-1 safety (no connect/exchange invoke on favorite click).
  describe('B7: Favorites surface via production AppShell stack', () => {
    it(
      'renders Favorites surface and is RADIO-1-safe (prefill-only) through the production path',
      async () => {
        // Override the module-level invoke mock with favorites data so
        // FavoritesTabs renders a starred ardop-hf row. All other commands
        // fall through to the same benign defaults declared in the module-level
        // mock above (the base impl is captured and forwarded for anything not
        // overridden here).
        const core = await import('@tauri-apps/api/core');
        const invokeMock = core.invoke as ReturnType<typeof vi.fn>;

        // Snapshot the module-level impl so we can delegate non-favorites
        // commands. We use mockImplementation to overlay favorites data only.
        invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
          // M9: route all favorites commands the panel fires.
          if (cmd === 'favorites_read') {
            return {
              schema_version: 1,
              favorites: [
                {
                  id: 'fav-1',
                  mode: 'ardop-hf',
                  gateway: 'W7RMS-10',
                  freq: '14105.0',
                  transport: null,
                  band: '20m',
                  grid: 'CN87',
                  note: null,
                  starred: true,
                  last_attempt_at: null,
                  created_at: '2026-06-01T00:00:00+00:00',
                  updated_at: '2026-06-01T00:00:00+00:00',
                },
              ],
              log: [],
            };
          }
          if (cmd === 'favorites_recents') return [];
          // C4: full-precision operator grid for distance (used by FavoritesTabs).
          if (cmd === 'position_current_fix') return { grid: 'CN88', source: 'Gps', fresh: true };
          if (cmd === 'favorite_tod_hint') return null;

          // Delegate to the benign module-level defaults for everything else.
          if (cmd === 'config_read') return null;
          if (cmd === 'backend_status') return null;
          if (cmd === 'session_log_snapshot') return [];
          if (cmd === 'message_read') return null;
          if (cmd === 'config_get_ardop') {
            return {
              binary: 'ardopcf',
              capture_device: '',
              playback_device: '',
              ptt_serial_path: null,
              cmd_port: 8515,
              bandwidth_hz: null,
              webgui_port: null,
            };
          }
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
          if (cmd === 'position_status') return { gps_ready: false, broadcast_grid: '', ui_grid: '' };
          if (cmd === 'tauri_search_list_saved') return [];
          if (cmd === 'tauri_search_list_recent') return [];
          if (cmd === 'ardop_allowed_stations_get') return { allow_all: true, callsigns: [] };
          if (cmd === 'ardop_list_audio_devices') return { captures: [], playbacks: [] };
          if (cmd === 'packet_list_serial_devices') return [];
          // Suppress unused-args lint warning (args is part of the mock signature).
          void args;
          return undefined;
        });

        // 1. Mount the production AppShell via renderShell(), modem stopped.
        mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
        renderShell();
        expect(screen.queryByTestId('radio-panel-root')).not.toBeInTheDocument();

        // 2. Select ARDOP HF — same mechanic as the existing "ARDOP HF selected,
        //    modem stopped" test. This causes the panel to mount via the real
        //    AppShell routing + provider stack.
        fireEvent.click(screen.getByTestId('sess-cms'));
        fireEvent.click(screen.getByTestId('proto-cms-ardop-hf'));
        expect(
          await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 }),
        ).toBeInTheDocument();

        // 3. Assert the Favorites surface rendered through the production stack.
        //    FavoritesTabs renders Radix Tabs with three triggers: Favorites / Recent
        //    / Manual. The Favorites tab is the default, so the favorite row is
        //    already visible without a tab-switch.
        expect(
          await screen.findByRole('tab', { name: 'Favorites' }, { timeout: 10000 }),
        ).toBeInTheDocument();
        expect(screen.getByRole('tab', { name: 'Recent' })).toBeInTheDocument();
        expect(screen.getByRole('tab', { name: 'Manual' })).toBeInTheDocument();

        // The starred ardop-hf favorite's row must appear in the default
        // Favorites tab (no tab-switch required — defaultValue="favorites").
        const connectBtn = await screen.findByTestId('favorite-connect-fav-1', undefined, {
          timeout: 10000,
        });
        expect(connectBtn).toBeInTheDocument();

        // 4. RADIO-1 safety through the production path: clicking the favorite's
        //    Connect must ONLY prefill (onPrefill) and NEVER fire a modem connect
        //    or b2f exchange command. This is the production-stack analog of B6's
        //    consent-non-bypass test.
        invokeMock.mockClear(); // isolate: only watch calls FROM this point on.
        fireEvent.click(connectBtn);
        // Allow one async tick for any (forbidden) async invoke to settle.
        await new Promise((r) => setTimeout(r, 30));

        const callsAfterClick = invokeMock.mock.calls.map(([c]) => c as string);
        expect(callsAfterClick).not.toContain('modem_ardop_connect');
        expect(callsAfterClick).not.toContain('modem_ardop_b2f_exchange');
      },
    );
  });

  // ── P7: managed packet picker via the production AppShell stack ──────────
  //
  // tuxlink-yq3l P7.4 production-mount-path test (memory:
  // test_production_mount_path_not_just_units). The unit tests in
  // PacketRadioPanel.test.tsx wrap the panel in a bare QueryClient scaffold;
  // this mounts the REAL AppShell tree (full provider stack + routing) and
  // selects CMS Packet so the managed picker is exercised end-to-end through
  // production. An UNCONFIGURED packet config makes the panel default to
  // Managed; a single fake device is provided so the picker can persist.
  // jsdom can't detect missing CSS (TEST-1) — this asserts BEHAVIOR (the
  // managed section renders + a device-select persists), not visual layout.
  describe('P7: managed packet picker via production AppShell stack', () => {
    it('mounts the managed picker and persists a device selection through production', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      const FAKE_DEVICE = {
        humanName: 'C-Media USB Audio Device (DRA-100)',
        alsaPlughw: 'plughw:CARD=DRA,DEV=0',
        stableId: { kind: 'byIdSymlink', value: 'usb-C-Media_DRA-100_CM119A-01' },
        pttCandidates: [{ kind: 'cm108Hid', hidrawPath: '/dev/hidraw3' }],
      };
      invokeMock.mockImplementation(async (cmd: string) => {
        // UNCONFIGURED packet config (linkKind:null) → panel defaults to Managed.
        if (cmd === 'packet_config_get') {
          return {
            ssid: 7,
            listenDefault: true,
            linkKind: null,
            tcpHost: null,
            tcpPort: null,
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
        if (cmd === 'packet_list_audio_devices') return [FAKE_DEVICE];
        // Benign defaults for the rest of the shell.
        if (cmd === 'config_read') return null;
        if (cmd === 'backend_status') return null;
        if (cmd === 'session_log_snapshot') return [];
        if (cmd === 'message_read') return null;
        if (cmd === 'position_status') return { gps_ready: false, broadcast_grid: '', ui_grid: '' };
        if (cmd === 'tauri_search_list_saved') return [];
        if (cmd === 'tauri_search_list_recent') return [];
        if (cmd === 'favorites_read') return { schema_version: 1, favorites: [], log: [] };
        if (cmd === 'favorites_recents') return [];
        if (cmd === 'position_current_fix') return { grid: null };
        if (cmd === 'favorite_tod_hint') return null;
        if (cmd === 'packet_allowed_stations_get') return { allow_all: true, callsigns: [] };
        if (cmd === 'packet_list_serial_devices') return [];
        return undefined;
      });

      mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
      renderShell();
      // Select CMS Packet — mounts PacketRadioPanel via real AppShell routing.
      selectConnection('sess-cms', 'proto-cms-packet');
      await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });

      // Managed picker rendered through the production stack (unconfigured →
      // Managed default), and the device dropdown populated.
      expect(
        await screen.findByTestId('managed-modem-section', undefined, { timeout: 10000 }),
      ).toBeInTheDocument();
      const select = (await screen.findByTestId(
        'managed-device-select',
      )) as HTMLSelectElement;

      // Selecting the device persists linkKind:'Managed' + the stableId + the
      // device's ranked-first PTT — end-to-end through the real provider stack.
      fireEvent.change(select, {
        target: { value: 'byIdSymlink:usb-C-Media_DRA-100_CM119A-01' },
      });
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'packet_config_set',
          expect.objectContaining({
            dto: expect.objectContaining({
              linkKind: 'Managed',
              managedAudioDevice: {
                kind: 'byIdSymlink',
                value: 'usb-C-Media_DRA-100_CM119A-01',
              },
              managedPtt: { kind: 'cm108Hid', hidrawPath: '/dev/hidraw3' },
            }),
          }),
        );
      });
    });
  });
});

// tuxlink-813d operator smoke #1a/#3: selecting a connection in compact mode
// must automatically open the radio drawer (the `.panes` div acquires the
// `drawer-open` class). In desktop the drawer does not auto-open (no class).
describe('<AppShell> compact drawer auto-open (tuxlink-813d)', () => {
  beforeEach(() => {
    mockUseModemStatus.mockReset();
    // Stub matchMedia to report compact for the COMPACT_MEDIA_QUERY.
    // This mirrors the pattern in App.test.tsx (tuxlink-h7q7 smoke).
    vi.stubGlobal('matchMedia', (q: string) => ({
      matches: q === COMPACT_MEDIA_QUERY,
      media: q,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
  });
  afterEach(() => vi.unstubAllGlobals());

  // Helper: renders AppShell in compact mode (matchMedia stub is already set
  // in beforeEach above). Desktop renderShell() re-uses the shared factory.
  function renderShellCompact() {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    return render(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );
  }

  it('drawer-open class is absent before any connection is selected (compact)', () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShellCompact();
    const panes = screen.getByTestId('shell-panes');
    expect(panes).not.toHaveClass('drawer-open');
  });

  // tuxlink-813d smoke #1a/#3: selecting a built CMS protocol (telnet) in compact
  // must add `drawer-open` to `.panes` so the radio drawer slides into view.
  // In compact the sidebar renders as a VERTICAL-TEXT RAIL (no labeled nav
  // inline) — the expand button (`rail-expand-btn`) opens the flyout from which
  // the connection accordion is reachable. Click the expand button → expand the
  // CMS session (`sess-cms`) → select Telnet (`proto-cms-telnet`).
  it('auto-opens the drawer when a CMS connection is selected in compact', async () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    renderShellCompact();
    const panes = screen.getByTestId('shell-panes');

    // Pre-condition: no drawer-open initially.
    expect(panes).not.toHaveClass('drawer-open');

    // In compact the sidebar is a vertical-text rail; the expand button is the
    // entry point to the flyout where the connection accordion lives.
    const expandBtn = screen.getByTestId('rail-expand-btn');
    fireEvent.click(expandBtn);

    // Expand the CMS session accordion, then click the Telnet proto row.
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));

    // The radio panel must mount (proves the connection selection registered).
    await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });

    // The compact auto-open effect must have added drawer-open to .panes.
    expect(panes).toHaveClass('drawer-open');
  });

  // Desktop control: WITHOUT the compact matchMedia stub, selecting the same
  // connection must NOT add `drawer-open`. The effect only fires when
  // `isCompact` is true.
  it('does NOT add drawer-open when a connection is selected in desktop mode', async () => {
    // Override the beforeEach compact stub with a stub that always returns
    // false (simulates desktop >=1366px).
    vi.stubGlobal('matchMedia', (_q: string) => ({
      matches: false,
      media: _q,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
    // Use the shared renderShell which mounts without the compact stub.
    renderShell();

    const panes = screen.getByTestId('shell-panes');
    // Desktop: labeled nav is inline (no rail-expand-btn needed).
    selectConnection('sess-cms', 'proto-cms-telnet');
    await screen.findByTestId('radio-panel-root', undefined, { timeout: 10000 });

    // Desktop: drawer-open must NOT be applied.
    expect(panes).not.toHaveClass('drawer-open');
  });
});
