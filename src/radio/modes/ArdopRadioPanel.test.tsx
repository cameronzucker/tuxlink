// src/radio/modes/ArdopRadioPanel.test.tsx
//
// Spec §5.3 — ArdopRadioPanel replaces the legacy ArdopDock + ArdopHfStub
// pair (P4.6 deletes both). Composes RadioPanel chrome + Connect form +
// Live + Signal + SessionLog + Actions sections.
//
// These tests cover the structural mounts and connect flow.
// Live numeric / throughput-meter values are exercised by the underlying
// SignalSection + Sparkline tests, not duplicated here.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';
import type { ModemStatus } from '../../modem/types';
import { STOPPED } from '../../modem/types';
import type { FavoriteDial } from '../../favorites/types';
import { emitGatewayPrefill } from '../../favorites/prefillEvent';

// The panel now mounts FavoritesTabs/useFavorites (react-query), so every
// render must be wrapped in a QueryClientProvider or the queries throw
// "No QueryClient set". retry:false keeps a rejected favorites read from
// retrying through the test.
const renderPanel = (ui: ReactElement) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
};

// The hand-entry Target + Bandwidth fields now live in the FavoritesTabs
// "Manual" tab (Task B6-ARDOP). Radix Tabs.Trigger switches on mouseDown
// (button 0) under jsdom, not click. Tests that need the target input call
// this to switch to the Manual tab first.
const switchToManualTab = async () => {
  const manual = await screen.findByRole('tab', { name: 'Manual' });
  fireEvent.mouseDown(manual, { button: 0 });
};

// Tauri IPC mocks. Per-test mockImplementation is re-applied in
// beforeEach so a test that overrides default behavior doesn't leak
// into the next test (P2/P3 idiom).
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

// `shellOpen` is the Tauri-plugin-shell `open()` used to launch the system
// browser for the WebGUI button. Mocked here so the test can assert on the
// URL passed without going through the real Tauri runtime.
vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(async () => undefined),
}));

// Mock useModemStatus so each test feeds the panel a specific ModemStatus.
const mockUseModemStatus = vi.fn();
vi.mock('../../modem/useModemStatus', () => ({
  useModemStatus: () => mockUseModemStatus(),
  MODEM_STATUS_EVENT: 'modem:status',
}));

import { ArdopRadioPanel } from './ArdopRadioPanel';

const defaultInvokeImpl = async (cmd: string, _args?: unknown) => {
  if (cmd === 'session_log_snapshot') return [];
  // Full ardop config so the Radio section can load capture/playback/ptt
  // without choking on missing keys. webgui_port=null exercises the
  // "derive from cmd_port - 1" path (round 3 default).
  if (cmd === 'config_get_ardop') {
    return {
      binary: 'ardopcf',
      capture_device: 'plughw:1,0',
      playback_device: 'plughw:1,0',
      ptt_serial_path: null,
      cmd_port: 8515,
      bandwidth_hz: null,
      webgui_port: null,
    };
  }
  // Listener defaults (tuxlink-7vea backend default flip).
  if (cmd === 'ardop_allowed_stations_get') {
    return { allow_all: true, callsigns: [] };
  }
  // Favorites surface (Task B6-ARDOP). The mounted FavoritesTabs/useFavorites
  // issue these reads; return empty/benign shapes so the queries RESOLVE
  // (rejecting would noisily fail in jsdom). Tests that need a clickable
  // favorite override favorites_read / favorites_recents per-test.
  if (cmd === 'favorites_read') {
    return { schema_version: 1, favorites: [], log: [] };
  }
  if (cmd === 'favorites_recents') return [];
  if (cmd === 'position_current_fix') return { grid: null };
  if (cmd === 'favorite_tod_hint') return null;
  return undefined;
};

const RUNNING: ModemStatus = {
  ...STOPPED,
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
  quality: 72,
};

describe('<ArdopRadioPanel>', () => {
  beforeEach(async () => {
    mockUseModemStatus.mockReset();
    mockUseModemStatus.mockReturnValue({
      status: STOPPED,
      loading: false,
      error: null,
    });
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
  });

  it('renders the Ardop Winlink title in the RadioPanel chrome', () => {
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Ardop Winlink');
  });

  it('mounts the SessionLogSection (children of RadioPanel body)', () => {
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('mounts the SignalSection with the Quality value from ModemStatus', () => {
    mockUseModemStatus.mockReturnValue({
      status: RUNNING,
      loading: false,
      error: null,
    });
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('signal-section')).toBeInTheDocument();
    expect(screen.getByTestId('quality-score')).toHaveTextContent('72');
  });

  it('renders the target-callsign input in the Connect form (stopped state)', async () => {
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    expect(await screen.findByTestId('ardop-target-input')).toBeInTheDocument();
  });

  it('Start button is disabled when target callsign is empty', () => {
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    const start = screen.getByTestId('ardop-start-btn') as HTMLButtonElement;
    expect(start.disabled).toBe(true);
  });

  it('Start button directly fires modem_ardop_connect without a consent modal (no-tuxlink-added-safeguards)', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    const target = (await screen.findByTestId('ardop-target-input')) as HTMLInputElement;
    fireEvent.change(target, { target: { value: 'W7RMS-10' } });
    const start = screen.getByTestId('ardop-start-btn') as HTMLButtonElement;
    expect(start.disabled).toBe(false);
    fireEvent.click(start);
    // No modal should appear.
    expect(screen.queryByRole('dialog')).toBeNull();
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'modem_ardop_connect',
        expect.objectContaining({ target: 'W7RMS-10' }),
      );
    });
    // modem_mint_consent must NOT be called — consent token dropped.
    expect(invokeMock).not.toHaveBeenCalledWith('modem_mint_consent');
  });

  it('Stop button fires modem_ardop_disconnect when modem is running', async () => {
    mockUseModemStatus.mockReturnValue({
      status: RUNNING,
      loading: false,
      error: null,
    });
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('ardop-stop-btn'));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('modem_ardop_disconnect');
    });
  });

  it('Send/Receive button is disabled when modem is not connected', () => {
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    const btn = screen.queryByTestId('ardop-send-receive-btn');
    // In stopped state the running-only action row may not render at all.
    if (btn) {
      expect((btn as HTMLButtonElement).disabled).toBe(true);
    } else {
      // It's acceptable for the running-only action to not appear in
      // stopped state; the structural test is that it's not enabled.
      expect(btn).not.toBeInTheDocument();
    }
  });

  it('Open WebGUI button opens a URL on cmd_port - 1 (defaults to 8514) via tauri-plugin-shell', async () => {
    mockUseModemStatus.mockReturnValue({
      status: RUNNING,
      loading: false,
      error: null,
    });
    const shell = await import('@tauri-apps/plugin-shell');
    const shellOpenMock = shell.open as ReturnType<typeof vi.fn>;
    shellOpenMock.mockClear();
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    const webguiBtn = screen.getByTestId('ardop-open-webgui-btn');
    fireEvent.click(webguiBtn);
    await waitFor(() => {
      expect(shellOpenMock).toHaveBeenCalledWith('http://localhost:8514/');
    });
  });

  it('does not open WebGUI when cmd_port is below 2 (surfaces an error instead)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'config_get_ardop') {
        return {
          binary: 'ardopcf',
          capture_device: '',
          playback_device: '',
          ptt_serial_path: null,
          cmd_port: 1,
          bandwidth_hz: null,
          webgui_port: null,
        };
      }
      return defaultInvokeImpl(cmd, args);
    });
    mockUseModemStatus.mockReturnValue({
      status: RUNNING,
      loading: false,
      error: null,
    });
    const shell = await import('@tauri-apps/plugin-shell');
    const shellOpenMock = shell.open as ReturnType<typeof vi.fn>;
    shellOpenMock.mockClear();
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('ardop-open-webgui-btn'));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/cmd_port/);
    });
    expect(shellOpenMock).not.toHaveBeenCalled();
  });

  // Operator smoke 2026-05-31 round 3: "ARDOP Open WebGUI opens but
  // localhost:8514 returns connection refused." Root-cause investigation
  // showed `-G <cmd_port - 1>` IS passed to ardopcf — the most likely
  // operator-observable cause is clicking Open WebGUI while ardopcf isn't
  // running. Gate the button behind the stopped state so the operator
  // can't request the URL before there's anything bound to it.
  it('Open WebGUI button is disabled when ardopcf is stopped (round 3)', async () => {
    // Default state from beforeEach is STOPPED.
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    const btn = screen.getByTestId('ardop-open-webgui-btn') as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    // Title hint must tell the operator WHY: ardopcf must be running first.
    expect(btn.title.toLowerCase()).toContain('start');
  });

  it('Open WebGUI uses the operator-pinned webgui_port override when set (round 3)', async () => {
    // Operator pins webgui_port=9080 (non-conventional ardopcf build).
    // The button MUST open the override port, not the cmd_port-1 default,
    // so it matches what the spawn passed to `-G`.
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'config_get_ardop') {
        return {
          binary: 'ardopcf',
          capture_device: 'plughw:1,0',
          playback_device: 'plughw:1,0',
          ptt_serial_path: null,
          cmd_port: 8515,
          bandwidth_hz: null,
          webgui_port: 9080,
        };
      }
      return defaultInvokeImpl(cmd, args);
    });
    mockUseModemStatus.mockReturnValue({ status: RUNNING, loading: false, error: null });
    const shell = await import('@tauri-apps/plugin-shell');
    const shellOpenMock = shell.open as ReturnType<typeof vi.fn>;
    shellOpenMock.mockClear();
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('ardop-open-webgui-btn'));
    await waitFor(() => {
      expect(shellOpenMock).toHaveBeenCalledWith('http://localhost:9080/');
    });
  });

  it('webgui_port input field persists override on blur (round 3)', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    // Wait for initial load.
    await waitFor(() => {
      expect(screen.getByTestId('ardop-webgui-port-input')).toBeInTheDocument();
    });
    const input = screen.getByTestId('ardop-webgui-port-input') as HTMLInputElement;
    // Default (None) renders as empty + a placeholder showing the derived port.
    expect(input.value).toBe('');
    expect(input.placeholder).toMatch(/auto/i);

    fireEvent.change(input, { target: { value: '9080' } });
    fireEvent.blur(input);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_ardop',
        expect.objectContaining({
          value: expect.objectContaining({ webgui_port: 9080 }),
        }),
      );
    });
  });

  it('webgui_port empty input clears the override (round 3)', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    // Seed with an override so we can clear it.
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'config_get_ardop') {
        return {
          binary: 'ardopcf',
          capture_device: 'plughw:1,0',
          playback_device: 'plughw:1,0',
          ptt_serial_path: null,
          cmd_port: 8515,
          bandwidth_hz: null,
          webgui_port: 9080,
        };
      }
      return defaultInvokeImpl(cmd, args);
    });
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    await waitFor(() => {
      const input = screen.getByTestId('ardop-webgui-port-input') as HTMLInputElement;
      expect(input.value).toBe('9080');
    });
    const input = screen.getByTestId('ardop-webgui-port-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.blur(input);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_ardop',
        expect.objectContaining({
          value: expect.objectContaining({ webgui_port: null }),
        }),
      );
    });
  });

  it('does not render a consent modal when Start is clicked', async () => {
    // Mock invoke so config_get_ardop resolves; modem_ardop_connect is
    // recorded; modem_mint_consent must NOT be invoked.
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    const invokes: { cmd: string; args?: unknown }[] = [];
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      invokes.push({ cmd, args });
      if (cmd === 'config_get_ardop') {
        return {
          binary: 'ardopcf',
          capture_device: 'plughw:1,0',
          playback_device: 'plughw:1,0',
          ptt_serial_path: null,
          cmd_port: 8515,
          bandwidth_hz: null,
          webgui_port: null,
        };
      }
      if (cmd === 'modem_ardop_connect') return null;
      if (cmd === 'session_log_snapshot') return [];
      if (cmd === 'ardop_allowed_stations_get') return { allow_all: true, callsigns: [] };
      return null;
    });

    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    const target = (await screen.findByTestId('ardop-target-input')) as HTMLInputElement;
    fireEvent.change(target, { target: { value: 'K7TEST' } });
    fireEvent.click(screen.getByTestId('ardop-start-btn'));

    expect(screen.queryByTestId('consent-modal')).toBeNull();
    expect(invokes.find((i) => i.cmd === 'modem_mint_consent')).toBeUndefined();
    await waitFor(() => {
      expect(invokes.find((i) => i.cmd === 'modem_ardop_connect')).toBeDefined();
    });
  });

  it('close button fires onClose', () => {
    const onClose = vi.fn();
    renderPanel(<ArdopRadioPanel onClose={onClose} />);
    fireEvent.click(screen.getByTestId('radio-panel-close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('does not render a "Dial as" intent toggle', () => {
    renderPanel(<ArdopRadioPanel onClose={() => {}} />);
    expect(screen.queryByTestId('ardop-intent-select')).toBeNull();
    expect(screen.queryByText(/Dial as/i)).toBeNull();
  });

  // Operator smoke 2026-05-31: Radio section parity with AX.25's
  // ModemLinkSection — audio capture + playback + PTT serial editable inline.
  describe('Radio section', () => {
    it('mounts the Radio section in stopped state', async () => {
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() =>
        expect(screen.getByTestId('ardop-radio-section')).toBeInTheDocument(),
      );
      expect(screen.getByTestId('ardop-capture-input')).toBeInTheDocument();
      expect(screen.getByTestId('ardop-playback-input')).toBeInTheDocument();
      expect(screen.getByTestId('ardop-ptt-input')).toBeInTheDocument();
    });

    it('loads capture/playback/PTT from config_get_ardop on mount', async () => {
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() => {
        const cap = screen.getByTestId('ardop-capture-input') as HTMLInputElement;
        expect(cap.value).toBe('plughw:1,0');
      });
    });

    it('persists capture device via config_set_ardop on blur', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      // Wait for initial load to complete (otherwise persistArdop would
      // see ardopConfig=null and bail).
      await waitFor(() => {
        expect((screen.getByTestId('ardop-capture-input') as HTMLInputElement).value).toBe('plughw:1,0');
      });
      const cap = screen.getByTestId('ardop-capture-input') as HTMLInputElement;
      fireEvent.change(cap, { target: { value: 'plughw:2,0' } });
      fireEvent.blur(cap);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'config_set_ardop',
          expect.objectContaining({
            value: expect.objectContaining({ capture_device: 'plughw:2,0' }),
          }),
        );
      });
    });

    it('empty PTT serial path persists as null (= VOX)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // Seed with a non-empty PTT path so we can clear it.
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'config_get_ardop') {
          return {
            binary: 'ardopcf',
            capture_device: '',
            playback_device: '',
            ptt_serial_path: '/dev/ttyUSB0',
            cmd_port: 8515,
            bandwidth_hz: null,
          };
        }
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() => {
        const ptt = screen.getByTestId('ardop-ptt-input') as HTMLInputElement;
        expect(ptt.value).toBe('/dev/ttyUSB0');
      });
      const ptt = screen.getByTestId('ardop-ptt-input') as HTMLInputElement;
      fireEvent.change(ptt, { target: { value: '' } });
      fireEvent.blur(ptt);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'config_set_ardop',
          expect.objectContaining({
            value: expect.objectContaining({ ptt_serial_path: null }),
          }),
        );
      });
    });

    it('Radio section is hidden when the modem is running (settings consumed at spawn)', () => {
      mockUseModemStatus.mockReturnValue({
        status: RUNNING,
        loading: false,
        error: null,
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      expect(screen.queryByTestId('ardop-radio-section')).not.toBeInTheDocument();
    });

    // tuxlink-jmfm Task 3: Settings ARDOP fieldset was deleted in Task 2;
    // cmd_port + binary were the two controls without an inline-edit
    // surface in the panel. These tests pin the rows + their persist-on-blur
    // behavior so the operator can edit both inline.
    it('Radio section has a cmd_port input row (tuxlink-jmfm)', async () => {
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() =>
        expect(screen.getByTestId('ardop-cmd-port-input')).toBeInTheDocument(),
      );
    });

    it('Radio section has an ardopcf binary input row (tuxlink-jmfm)', async () => {
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() =>
        expect(screen.getByTestId('ardop-binary-input')).toBeInTheDocument(),
      );
    });

    it('cmd_port input persists on blur (tuxlink-jmfm)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      // Wait for initial load (default config has cmd_port=8515 so the
      // input renders that value once ardopConfig hydrates).
      await waitFor(() => {
        expect((screen.getByTestId('ardop-cmd-port-input') as HTMLInputElement).value).toBe('8515');
      });
      const cmd = screen.getByTestId('ardop-cmd-port-input') as HTMLInputElement;
      fireEvent.change(cmd, { target: { value: '8520' } });
      fireEvent.blur(cmd);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'config_set_ardop',
          expect.objectContaining({
            value: expect.objectContaining({ cmd_port: 8520 }),
          }),
        );
      });
    });

    it('binary input persists on blur (tuxlink-jmfm)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      // Wait for initial load (default config has binary='ardopcf').
      await waitFor(() => {
        expect((screen.getByTestId('ardop-binary-input') as HTMLInputElement).value).toBe('ardopcf');
      });
      const bin = screen.getByTestId('ardop-binary-input') as HTMLInputElement;
      fireEvent.change(bin, { target: { value: '/usr/local/bin/ardopcf' } });
      fireEvent.blur(bin);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'config_set_ardop',
          expect.objectContaining({
            value: expect.objectContaining({ binary: '/usr/local/bin/ardopcf' }),
          }),
        );
      });
    });

    // Code-quality review follow-up (tuxlink-jmfm Task 3): three Important
    // findings on the initial T3 commit (9b73157) — commitBinary silently
    // dropped empty input; commitCmdPort used parseInt (lossy) instead of
    // Number + Number.isInteger (strict); commitCmdPort was missing the
    // n <= 65535 upper bound that commitWebguiPort enforces.
    it('commitBinary reverts on empty input (tuxlink-jmfm follow-up)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      // Wait for initial load (default config has binary='ardopcf').
      await waitFor(() => {
        expect((screen.getByTestId('ardop-binary-input') as HTMLInputElement).value).toBe('ardopcf');
      });
      const bin = screen.getByTestId('ardop-binary-input') as HTMLInputElement;
      // Clear writes count BEFORE the operator action so we can assert
      // config_set_ardop was NOT called by the empty-input commit.
      invokeMock.mockClear();
      fireEvent.change(bin, { target: { value: '' } });
      fireEvent.blur(bin);
      // The input MUST resync to the persisted 'ardopcf' (not stay empty).
      await waitFor(() => {
        expect((screen.getByTestId('ardop-binary-input') as HTMLInputElement).value).toBe('ardopcf');
      });
      // And no persist call should have fired.
      const setCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === 'config_set_ardop');
      expect(setCalls).toHaveLength(0);
    });

    it('commitCmdPort rejects non-numeric input strictly (tuxlink-jmfm follow-up)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() => {
        expect((screen.getByTestId('ardop-cmd-port-input') as HTMLInputElement).value).toBe('8515');
      });
      const cmd = screen.getByTestId('ardop-cmd-port-input') as HTMLInputElement;
      // Clear writes count BEFORE the operator action so we can assert
      // config_set_ardop was NOT called by the bad-input commit.
      // parseInt('8515abc', 10) === 8515 would have silently accepted this;
      // Number('8515abc') === NaN rejects.
      invokeMock.mockClear();
      fireEvent.change(cmd, { target: { value: '8515abc' } });
      fireEvent.blur(cmd);
      await waitFor(() => {
        expect((screen.getByTestId('ardop-cmd-port-input') as HTMLInputElement).value).toBe('8515');
      });
      const setCalls = invokeMock.mock.calls.filter(([c]) => c === 'config_set_ardop');
      expect(setCalls).toHaveLength(0);
    });

    it('commitCmdPort rejects port > 65535 (tuxlink-jmfm follow-up)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() => {
        expect((screen.getByTestId('ardop-cmd-port-input') as HTMLInputElement).value).toBe('8515');
      });
      const cmd = screen.getByTestId('ardop-cmd-port-input') as HTMLInputElement;
      invokeMock.mockClear();
      fireEvent.change(cmd, { target: { value: '99999' } });
      fireEvent.blur(cmd);
      // Input MUST revert to the persisted 8515.
      await waitFor(() => {
        expect((screen.getByTestId('ardop-cmd-port-input') as HTMLInputElement).value).toBe('8515');
      });
      const setCalls = invokeMock.mock.calls.filter(([c]) => c === 'config_set_ardop');
      expect(setCalls).toHaveLength(0);
    });
  });

  // tuxlink-y7x7: ALSA + PTT pickers (capture/playback dropdown from
  // ardop_list_audio_devices, PTT dropdown from packet_list_serial_devices).
  // Restores the picker UX the placeholder-ghost text inputs only pretended
  // to have. Manual-fallback inputs preserved under the old testIds so the
  // existing Radio-section tests above still apply.
  describe('Radio section device pickers', () => {
    it('loads ALSA capture + playback lists on mount via ardop_list_audio_devices', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
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
        if (cmd === 'ardop_list_audio_devices') {
          return {
            captures: [
              { name: 'plughw:CARD=Device,DEV=0', description: 'USB Audio CODEC', isHardware: true },
              { name: 'default', description: 'Default Audio Device', isHardware: false },
            ],
            playbacks: [
              { name: 'plughw:CARD=Device,DEV=0', description: 'USB Audio CODEC', isHardware: true },
            ],
          };
        }
        if (cmd === 'packet_list_serial_devices') return [];
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('ardop_list_audio_devices');
      });
      const captureSel = await screen.findByTestId('ardop-capture-select') as HTMLSelectElement;
      const playbackSel = await screen.findByTestId('ardop-playback-select') as HTMLSelectElement;
      const capValues = Array.from(captureSel.options).map((o) => o.value);
      const playValues = Array.from(playbackSel.options).map((o) => o.value);
      expect(capValues).toContain('plughw:CARD=Device,DEV=0');
      // tuxlink-y7nq: non-hardware entries (`default`, `pulse`, plugin chains)
      // are filtered out of the dropdown — the operator can still type them
      // via the manual-fallback input below the dropdown.
      expect(capValues).not.toContain('default');
      expect(playValues).toContain('plughw:CARD=Device,DEV=0');
    });

    // tuxlink-y7nq: pin the hardware-only filter so a future regression
    // (e.g., re-introducing the plugin chains to the dropdown) fails here.
    it('Capture/Playback dropdowns hide ALSA plugin chains, keeping plughw/hw:CARD only', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
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
        if (cmd === 'ardop_list_audio_devices') {
          // Representative of a real `arecord -L` snapshot: plugin chains +
          // sysdefault + a single hardware USB CODEC entry.
          return {
            captures: [
              { name: 'null', description: 'Discard all samples', isHardware: false },
              { name: 'default', description: 'Default Audio Device', isHardware: false },
              { name: 'pulse', description: 'PulseAudio Sound Server', isHardware: false },
              { name: 'lavrate', description: 'Rate Converter', isHardware: false },
              { name: 'plughw:CARD=Device,DEV=0', description: 'USB Audio CODEC', isHardware: true },
              { name: 'hw:CARD=Device,DEV=0', description: 'USB Audio CODEC raw', isHardware: true },
            ],
            playbacks: [
              { name: 'null', description: 'Discard all samples', isHardware: false },
              { name: 'plughw:CARD=Device,DEV=0', description: 'USB Audio CODEC', isHardware: true },
            ],
          };
        }
        if (cmd === 'packet_list_serial_devices') return [];
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const captureSel = await screen.findByTestId('ardop-capture-select') as HTMLSelectElement;
      const playbackSel = await screen.findByTestId('ardop-playback-select') as HTMLSelectElement;
      await waitFor(() => {
        expect(Array.from(captureSel.options).map((o) => o.value))
          .toContain('plughw:CARD=Device,DEV=0');
      });
      const capValues = Array.from(captureSel.options).map((o) => o.value);
      // Hardware kept, plugin chains dropped.
      expect(capValues).toContain('plughw:CARD=Device,DEV=0');
      expect(capValues).toContain('hw:CARD=Device,DEV=0');
      for (const noisy of ['null', 'default', 'pulse', 'lavrate']) {
        expect(capValues).not.toContain(noisy);
      }
      const playValues = Array.from(playbackSel.options).map((o) => o.value);
      expect(playValues).toContain('plughw:CARD=Device,DEV=0');
      expect(playValues).not.toContain('null');
    });

    it('selecting a capture device persists capture_device via config_set_ardop', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
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
        if (cmd === 'ardop_list_audio_devices') {
          return {
            captures: [
              { name: 'plughw:CARD=Device,DEV=0', description: 'USB Audio CODEC', isHardware: true },
            ],
            playbacks: [],
          };
        }
        if (cmd === 'packet_list_serial_devices') return [];
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const captureSel = await screen.findByTestId('ardop-capture-select') as HTMLSelectElement;
      await waitFor(() => {
        expect(Array.from(captureSel.options).map((o) => o.value))
          .toContain('plughw:CARD=Device,DEV=0');
      });
      invokeMock.mockClear();
      fireEvent.change(captureSel, { target: { value: 'plughw:CARD=Device,DEV=0' } });
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'config_set_ardop',
          expect.objectContaining({
            value: expect.objectContaining({ capture_device: 'plughw:CARD=Device,DEV=0' }),
          }),
        );
      });
    });

    it('Refresh button re-invokes ardop_list_audio_devices', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('ardop_list_audio_devices');
      });
      const callsBefore = invokeMock.mock.calls.filter(
        ([c]) => c === 'ardop_list_audio_devices',
      ).length;
      fireEvent.click(screen.getByTestId('ardop-capture-refresh'));
      await waitFor(() => {
        const callsAfter = invokeMock.mock.calls.filter(
          ([c]) => c === 'ardop_list_audio_devices',
        ).length;
        expect(callsAfter).toBe(callsBefore + 1);
      });
    });

    it('PTT picker filters to USB serial entries + includes (none = VOX) option', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
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
        if (cmd === 'ardop_list_audio_devices') return { captures: [], playbacks: [] };
        if (cmd === 'packet_list_serial_devices') {
          return [
            { path: '/dev/ttyUSB0', kind: 'usb', label: 'USB serial' },
            // UART excluded — ARDOP PTT picker only surfaces USB-class.
            { path: '/dev/ttyAMA0', kind: 'uart', label: 'On-board UART' },
          ];
        }
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const pttSel = await screen.findByTestId('ardop-ptt-select') as HTMLSelectElement;
      await waitFor(() => {
        expect(Array.from(pttSel.options).map((o) => o.value)).toContain('/dev/ttyUSB0');
      });
      const values = Array.from(pttSel.options).map((o) => o.value);
      // (none = VOX) is an empty-string option at the top.
      expect(values).toContain('');
      expect(values).toContain('/dev/ttyUSB0');
      expect(values).not.toContain('/dev/ttyAMA0');
    });

    it('selecting (none) in PTT picker persists ptt_serial_path=null (VOX)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'config_get_ardop') {
          return {
            binary: 'ardopcf',
            capture_device: '',
            playback_device: '',
            ptt_serial_path: '/dev/ttyUSB0',
            cmd_port: 8515,
            bandwidth_hz: null,
            webgui_port: null,
          };
        }
        if (cmd === 'ardop_list_audio_devices') return { captures: [], playbacks: [] };
        if (cmd === 'packet_list_serial_devices') {
          return [{ path: '/dev/ttyUSB0', kind: 'usb', label: 'USB serial' }];
        }
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const pttSel = await screen.findByTestId('ardop-ptt-select') as HTMLSelectElement;
      await waitFor(() => {
        expect(pttSel.value).toBe('/dev/ttyUSB0');
      });
      invokeMock.mockClear();
      fireEvent.change(pttSel, { target: { value: '' } });
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'config_set_ardop',
          expect.objectContaining({
            value: expect.objectContaining({ ptt_serial_path: null }),
          }),
        );
      });
    });
  });

  // ── Listen section (tuxlink-7vea) ────────────────────────────────────────
  //
  // The ARDOP listener (allowlist + arms record + LISTEN TRUE/FALSE wiring)
  // landed on this branch alongside the UI. The panel does NOT carry a
  // station-password expander (ARDOP has none per ardop-p2p.md divergence 2)
  // and does NOT carry a listener-setup expander (modem TCP details live
  // in the Radio section above).

  describe('Listen section', () => {
    beforeEach(() => {
      mockUseModemStatus.mockReturnValue({ status: STOPPED });
    });

    it('renders the Listen section', async () => {
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      expect(await screen.findByTestId('ardop-listen-section')).toBeInTheDocument();
    });

    it('Arm button click fires ardop_listen', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(defaultInvokeImpl);
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const armBtn = await screen.findByTestId('ardop-listen-arm-btn');
      fireEvent.click(armBtn);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('ardop_listen');
      });
    });

    it('Disarm button (after arming) fires ardop_set_listen with enabled=false', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(defaultInvokeImpl);
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const armBtn = await screen.findByTestId('ardop-listen-arm-btn');
      fireEvent.click(armBtn);
      const disarmBtn = await screen.findByTestId('ardop-listen-disarm-btn');
      fireEvent.click(disarmBtn);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('ardop_set_listen', {
          enabled: false,
        });
      });
    });

    it('shows an error when ardop_listen rejects', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'ardop_listen') {
          throw new Error('ARDOP modem not running');
        }
        return defaultInvokeImpl(cmd);
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const armBtn = await screen.findByTestId('ardop-listen-arm-btn');
      fireEvent.click(armBtn);
      await waitFor(() => {
        expect(screen.getByTestId('ardop-listen-error')).toHaveTextContent(
          /ARDOP modem not running/,
        );
      });
    });

    it('Allow-any-peer toggle fires ardop_allowed_stations_set_allow_all', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(defaultInvokeImpl);
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const expander = await screen.findByTestId('ardop-allowed-expander');
      fireEvent.click(expander);
      const toggle = await screen.findByTestId('ardop-allowed-allow-all-toggle');
      fireEvent.click(toggle);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'ardop_allowed_stations_set_allow_all',
          { allowAll: false },
        );
      });
    });

    it('adding a callsign fires ardop_allowed_stations_add', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(defaultInvokeImpl);
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const expander = await screen.findByTestId('ardop-allowed-expander');
      fireEvent.click(expander);
      const addBtn = await screen.findByTestId('ardop-allowed-callsign-add-btn');
      fireEvent.click(addBtn);
      const input = await screen.findByTestId('ardop-allowed-callsign-add-input');
      fireEvent.change(input, { target: { value: 'w7rms' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'ardop_allowed_stations_add',
          { callsign: 'W7RMS' },
        );
      });
    });

    it('removing a callsign fires ardop_allowed_stations_remove', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'ardop_allowed_stations_get') {
          return { allow_all: false, callsigns: ['W7RMS'] };
        }
        return defaultInvokeImpl(cmd);
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const expander = await screen.findByTestId('ardop-allowed-expander');
      fireEvent.click(expander);
      const removeBtn = await screen.findByTestId('ardop-allowed-callsign-remove-W7RMS');
      fireEvent.click(removeBtn);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'ardop_allowed_stations_remove',
          { callsign: 'W7RMS' },
        );
      });
    });

    it('ARDOP allowed-stations editor does NOT render an IP row', async () => {
      const core = await import('@tauri-apps/api/core');
      (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      const expander = await screen.findByTestId('ardop-allowed-expander');
      fireEvent.click(expander);
      await waitFor(() =>
        expect(screen.getByTestId('ardop-allowed-callsign-row')).toBeInTheDocument(),
      );
      expect(screen.queryByTestId('ardop-allowed-ip-row')).not.toBeInTheDocument();
    });

    it('Listen section does NOT render a Station Password expander', async () => {
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await screen.findByTestId('ardop-listen-section');
      expect(screen.queryByTestId('ardop-station-pw-expander')).not.toBeInTheDocument();
    });

    it('Listen section does NOT render a Listener setup expander', async () => {
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await screen.findByTestId('ardop-listen-section');
      expect(screen.queryByTestId('ardop-listen-setup-expander')).not.toBeInTheDocument();
    });
  });

  // ── Favorites integration (Task B6-ARDOP) ────────────────────────────────
  //
  // RADIO-1 + C3 + M4. A favorite's Connect PRE-FILLS the target only (never
  // transmits). `reached` is recorded on the on-air connected-* transition (not
  // when modem_ardop_connect resolves); `failed` is recorded in the
  // b2f_exchange catch (not finally, not on a busy-guard / local-spawn path).
  // The record timestamp carries a UTC offset (M4 / H1).

  describe('Favorites integration (B6-ARDOP)', () => {
    // A connected status with a freshly-rendered QueryClient. `state` and
    // `peer` are the two record-trigger inputs.
    const connectedStatus = (peer: string = 'W7RMS-10'): ModemStatus => ({
      ...RUNNING,
      state: 'connected-irs',
      peer,
    });
    const findRecordCalls = (invokeMock: ReturnType<typeof vi.fn>) =>
      invokeMock.mock.calls.filter(([cmd]) => cmd === 'favorite_record_attempt');

    it('records reached on the connected-* link transition (C3), once', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // Start NOT connected so the record-on-transition effect does not fire
      // on mount.
      mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
      const { rerender } = renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      expect(findRecordCalls(invokeMock)).toHaveLength(0);

      // The modem reaches the on-air ARQ link.
      mockUseModemStatus.mockReturnValue({
        status: connectedStatus('W7RMS-10'),
        loading: false,
        error: null,
      });
      rerender(
        <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
          <ArdopRadioPanel onClose={() => {}} />
        </QueryClientProvider>,
      );

      await waitFor(() => {
        const calls = findRecordCalls(invokeMock);
        expect(calls).toHaveLength(1);
        const [, args] = calls[0] as [string, { dial: FavoriteDial; outcome: string }];
        expect(args.outcome).toBe('reached');
        expect(args.dial.gateway).toBe('W7RMS-10');
        expect(args.dial.mode).toBe('ardop-hf');
      });

      // A subsequent status tick at the SAME connected state must NOT re-record.
      rerender(
        <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
          <ArdopRadioPanel onClose={() => {}} />
        </QueryClientProvider>,
      );
      await new Promise((r) => setTimeout(r, 20));
      expect(findRecordCalls(invokeMock)).toHaveLength(1);
    });

    it('does NOT record reached when the modem never reaches connected-*', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // modem_ardop_connect resolves but the status stays non-connected.
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'modem_ardop_connect') return null;
        return defaultInvokeImpl(cmd, args);
      });
      mockUseModemStatus.mockReturnValue({
        status: { ...STOPPED, state: 'connecting', peer: null },
        loading: false,
        error: null,
      });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await new Promise((r) => setTimeout(r, 30));
      expect(findRecordCalls(invokeMock)).toHaveLength(0);
    });

    it('records failed in the b2f_exchange catch (C3) — not on a pre-air guard', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'modem_ardop_b2f_exchange') {
          throw new Error('CMS rejected');
        }
        return defaultInvokeImpl(cmd, args);
      });
      // Mount NOT connected, then transition to connected-*, so the on-air
      // `reached` is logged via a genuine STOPPED→connected transition (the
      // record-on-transition guard ignores a mount that begins already
      // connected — see ArdopRadioPanel's recordedConnRef init).
      mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
      const { rerender } = renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      mockUseModemStatus.mockReturnValue({
        status: connectedStatus('W7RMS-10'),
        loading: false,
        error: null,
      });
      rerender(
        <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
          <ArdopRadioPanel onClose={() => {}} />
        </QueryClientProvider>,
      );
      // The on-air transition records ONE `reached`. Clear so we isolate the
      // failed record from the exchange below.
      await waitFor(() => expect(findRecordCalls(invokeMock).length).toBeGreaterThanOrEqual(1));
      invokeMock.mockClear();
      // Re-install the throwing impl (mockClear wipes the implementation).
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'modem_ardop_b2f_exchange') {
          throw new Error('CMS rejected');
        }
        return defaultInvokeImpl(cmd, args);
      });
      fireEvent.click(screen.getByTestId('ardop-send-receive-btn'));
      await waitFor(() => {
        const failed = findRecordCalls(invokeMock).filter(
          ([, a]) => (a as { outcome: string }).outcome === 'failed',
        );
        expect(failed).toHaveLength(1);
        const [, args] = failed[0] as [string, { dial: FavoriteDial; outcome: string }];
        expect(args.dial.gateway).toBe('W7RMS-10');
      });
    });

    it('busy-guard / not-exchange-ready Send/Receive records NOTHING', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // Stopped: Send/Receive isn't rendered (and the guard would bail anyway).
      mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      await new Promise((r) => setTimeout(r, 20));
      // No on-air transition (stopped) and no exchange → no record at all.
      expect(findRecordCalls(invokeMock)).toHaveLength(0);
      // The Send/Receive button isn't present in the stopped action row.
      expect(screen.queryByTestId('ardop-send-receive-btn')).toBeNull();
    });

    it('CONSENT NON-BYPASS (M13): a favorite Connect pre-fills only, never transmits', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // Route a starred ardop-hf favorite so the Favorites tab has a row.
      const fav = {
        id: 'fav-1',
        mode: 'ardop-hf' as const,
        gateway: 'W7RMS-10',
        band: '40m',
        starred: true,
        created_at: '2026-06-08T00:00:00-07:00',
        updated_at: '2026-06-08T00:00:00-07:00',
      };
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'favorites_read') {
          return { schema_version: 1, favorites: [fav], log: [] };
        }
        if (cmd === 'favorites_recents') return [];
        if (cmd === 'modem_ardop_connect') return null;
        return defaultInvokeImpl(cmd, args);
      });
      mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);

      // Default tab is Favorites; the favorite's Connect appears there.
      const connectBtn = await screen.findByTestId('favorite-connect-fav-1');
      fireEvent.click(connectBtn);
      // Let any (forbidden) async invoke settle.
      await new Promise((r) => setTimeout(r, 20));

      // RADIO-1: neither connect nor exchange may have fired from the prefill.
      expect(
        invokeMock.mock.calls.some(([cmd]) => cmd === 'modem_ardop_connect'),
      ).toBe(false);
      expect(
        invokeMock.mock.calls.some(([cmd]) => cmd === 'modem_ardop_b2f_exchange'),
      ).toBe(false);

      // Prefill worked: the Manual tab's target now holds the gateway.
      await switchToManualTab();
      const target = (await screen.findByTestId('ardop-target-input')) as HTMLInputElement;
      expect(target.value).toBe('W7RMS-10');

      // Consent gate intact: clicking Start NOW invokes modem_ardop_connect.
      fireEvent.click(screen.getByTestId('ardop-start-btn'));
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          'modem_ardop_connect',
          expect.objectContaining({ target: 'W7RMS-10' }),
        );
      });
    });

    it('station-picker prefill event fills the ARDOP target without transmitting', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);

      act(() => {
        emitGatewayPrefill({
          mode: 'ardop-hf',
          gateway: 'W6ABC',
          freq: '14.105',
          grid: 'CN87',
        });
      });

      await switchToManualTab();
      const target = (await screen.findByTestId('ardop-target-input')) as HTMLInputElement;
      expect(target.value).toBe('W6ABC');
      expect(
        invokeMock.mock.calls.some(([cmd]) => cmd === 'modem_ardop_connect'),
      ).toBe(false);
      expect(
        invokeMock.mock.calls.some(([cmd]) => cmd === 'modem_ardop_b2f_exchange'),
      ).toBe(false);
    });

    it('records an offset-bearing ts_local (M4) — not a UTC Z timestamp', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // Mount NOT connected, then transition to connected-* so the `reached`
      // record fires via a genuine transition (a mount that begins already
      // connected is treated as already-recorded — see recordedConnRef init).
      mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
      const { rerender } = renderPanel(<ArdopRadioPanel onClose={() => {}} />);
      mockUseModemStatus.mockReturnValue({
        status: connectedStatus('W7RMS-10'),
        loading: false,
        error: null,
      });
      rerender(
        <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
          <ArdopRadioPanel onClose={() => {}} />
        </QueryClientProvider>,
      );
      await waitFor(() => {
        const calls = findRecordCalls(invokeMock);
        expect(calls.length).toBeGreaterThanOrEqual(1);
        const [, args] = calls[0] as [string, { tsLocal: string }];
        // camelCase wire key + offset-bearing (±HH:MM), never Z.
        expect(typeof args.tsLocal).toBe('string');
        expect(args.tsLocal).toMatch(/[+-]\d{2}:\d{2}$/);
        expect(args.tsLocal.endsWith('Z')).toBe(false);
      });
    });

    it('ARDOP prefill sets ONLY the target (no freq input on the ARDOP form) (H8)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      const fav = {
        id: 'fav-2',
        mode: 'ardop-hf' as const,
        gateway: 'KE7XYZ-10',
        freq: '7.103 MHz',
        band: '40m',
        starred: true,
        created_at: '2026-06-08T00:00:00-07:00',
        updated_at: '2026-06-08T00:00:00-07:00',
      };
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'favorites_read') {
          return { schema_version: 1, favorites: [fav], log: [] };
        }
        if (cmd === 'favorites_recents') return [];
        return defaultInvokeImpl(cmd, args);
      });
      mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false, error: null });
      renderPanel(<ArdopRadioPanel onClose={() => {}} />);

      const connectBtn = await screen.findByTestId('favorite-connect-fav-2');
      fireEvent.click(connectBtn);

      await switchToManualTab();
      const target = (await screen.findByTestId('ardop-target-input')) as HTMLInputElement;
      expect(target.value).toBe('KE7XYZ-10');
      // The ARDOP Connect form has no freq input — only target + bandwidth.
      expect(screen.queryByTestId('ardop-freq-input')).toBeNull();
    });
  });
});
