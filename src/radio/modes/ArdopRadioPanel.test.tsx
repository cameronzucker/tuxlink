// src/radio/modes/ArdopRadioPanel.test.tsx
//
// Spec §5.3 — ArdopRadioPanel replaces the legacy ArdopDock + ArdopHfStub
// pair (P4.6 deletes both). Composes RadioPanel chrome + Connect form +
// Live + Signal + SessionLog + Actions sections.
//
// These tests cover the structural mounts and RADIO-1 consent flow.
// Live numeric / throughput-meter values are exercised by the underlying
// SignalSection + Sparkline tests, not duplicated here.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ModemStatus } from '../../modem/types';
import { STOPPED } from '../../modem/types';

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

const defaultInvokeImpl = async (cmd: string) => {
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
  if (cmd === 'modem_mint_consent') return 'test-token';
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
    render(<ArdopRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Ardop Winlink');
  });

  it('mounts the SessionLogSection (children of RadioPanel body)', () => {
    render(<ArdopRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('mounts the SignalSection with the Quality value from ModemStatus', () => {
    mockUseModemStatus.mockReturnValue({
      status: RUNNING,
      loading: false,
      error: null,
    });
    render(<ArdopRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('signal-section')).toBeInTheDocument();
    expect(screen.getByTestId('quality-score')).toHaveTextContent('72');
  });

  it('renders the target-callsign input in the Connect form (stopped state)', () => {
    render(<ArdopRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('ardop-target-input')).toBeInTheDocument();
  });

  it('Start button is disabled when target callsign is empty', () => {
    render(<ArdopRadioPanel onClose={() => {}} />);
    const start = screen.getByTestId('ardop-start-btn') as HTMLButtonElement;
    expect(start.disabled).toBe(true);
  });

  it('Start button opens the RADIO-1 consent modal when clicked with a target', async () => {
    render(<ArdopRadioPanel onClose={() => {}} />);
    const target = screen.getByTestId('ardop-target-input') as HTMLInputElement;
    fireEvent.change(target, { target: { value: 'W7RMS-10' } });
    const start = screen.getByTestId('ardop-start-btn') as HTMLButtonElement;
    expect(start.disabled).toBe(false);
    fireEvent.click(start);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('Consent confirm path mints a token via modem_mint_consent and fires modem_ardop_connect', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<ArdopRadioPanel onClose={() => {}} />);
    const target = screen.getByTestId('ardop-target-input') as HTMLInputElement;
    fireEvent.change(target, { target: { value: 'W7RMS-10' } });
    fireEvent.click(screen.getByTestId('ardop-start-btn'));
    // Tick the ack checkbox in the modal (the modal is the only `role="dialog"`).
    const dialog = screen.getByRole('dialog');
    const ack = dialog.querySelector('input[type="checkbox"]') as HTMLInputElement;
    fireEvent.click(ack);
    const modalConnect = dialog.querySelector('button:not([disabled])') as HTMLButtonElement;
    // The dialog has Cancel + Connect; pick the one labelled Connect.
    const connectButton = Array.from(dialog.querySelectorAll('button')).find(
      (b) => b.textContent === 'Connect',
    )!;
    expect(connectButton).not.toBe(undefined);
    expect(modalConnect).not.toBe(null);
    fireEvent.click(connectButton);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('modem_mint_consent');
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'modem_ardop_connect',
        expect.objectContaining({ target: 'W7RMS-10', consentToken: 'test-token' }),
      );
    });
  });

  it('Stop button fires modem_ardop_disconnect when modem is running', async () => {
    mockUseModemStatus.mockReturnValue({
      status: RUNNING,
      loading: false,
      error: null,
    });
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<ArdopRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('ardop-stop-btn'));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('modem_ardop_disconnect');
    });
  });

  it('Send/Receive button is disabled when modem is not connected', () => {
    render(<ArdopRadioPanel onClose={() => {}} />);
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
    render(<ArdopRadioPanel onClose={() => {}} />);
    const webguiBtn = screen.getByTestId('ardop-open-webgui-btn');
    fireEvent.click(webguiBtn);
    await waitFor(() => {
      expect(shellOpenMock).toHaveBeenCalledWith('http://localhost:8514/');
    });
  });

  it('does not open WebGUI when cmd_port is below 2 (surfaces an error instead)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
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
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    mockUseModemStatus.mockReturnValue({
      status: RUNNING,
      loading: false,
      error: null,
    });
    const shell = await import('@tauri-apps/plugin-shell');
    const shellOpenMock = shell.open as ReturnType<typeof vi.fn>;
    shellOpenMock.mockClear();
    render(<ArdopRadioPanel onClose={() => {}} />);
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
    render(<ArdopRadioPanel onClose={() => {}} />);
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
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
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
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    mockUseModemStatus.mockReturnValue({ status: RUNNING, loading: false, error: null });
    const shell = await import('@tauri-apps/plugin-shell');
    const shellOpenMock = shell.open as ReturnType<typeof vi.fn>;
    shellOpenMock.mockClear();
    render(<ArdopRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('ardop-open-webgui-btn'));
    await waitFor(() => {
      expect(shellOpenMock).toHaveBeenCalledWith('http://localhost:9080/');
    });
  });

  it('webgui_port input field persists override on blur (round 3)', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<ArdopRadioPanel onClose={() => {}} />);
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
    invokeMock.mockImplementation(async (cmd: string) => {
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
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    render(<ArdopRadioPanel onClose={() => {}} />);
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

  it('close button fires onClose', () => {
    const onClose = vi.fn();
    render(<ArdopRadioPanel onClose={onClose} />);
    fireEvent.click(screen.getByTestId('radio-panel-close'));
    expect(onClose).toHaveBeenCalled();
  });

  // Operator smoke 2026-05-31: Radio section parity with AX.25's
  // ModemLinkSection — audio capture + playback + PTT serial editable inline.
  describe('Radio section', () => {
    it('mounts the Radio section in stopped state', async () => {
      render(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() =>
        expect(screen.getByTestId('ardop-radio-section')).toBeInTheDocument(),
      );
      expect(screen.getByTestId('ardop-capture-input')).toBeInTheDocument();
      expect(screen.getByTestId('ardop-playback-input')).toBeInTheDocument();
      expect(screen.getByTestId('ardop-ptt-input')).toBeInTheDocument();
    });

    it('loads capture/playback/PTT from config_get_ardop on mount', async () => {
      render(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() => {
        const cap = screen.getByTestId('ardop-capture-input') as HTMLInputElement;
        expect(cap.value).toBe('plughw:1,0');
      });
    });

    it('persists capture device via config_set_ardop on blur', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      invokeMock.mockImplementation(async (cmd: string) => {
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
        if (cmd === 'session_log_snapshot') return [];
        return undefined;
      });
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      render(<ArdopRadioPanel onClose={() => {}} />);
      expect(screen.queryByTestId('ardop-radio-section')).not.toBeInTheDocument();
    });

    // tuxlink-jmfm Task 3: Settings ARDOP fieldset was deleted in Task 2;
    // cmd_port + binary were the two controls without an inline-edit
    // surface in the panel. These tests pin the rows + their persist-on-blur
    // behavior so the operator can edit both inline.
    it('Radio section has a cmd_port input row (tuxlink-jmfm)', async () => {
      render(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() =>
        expect(screen.getByTestId('ardop-cmd-port-input')).toBeInTheDocument(),
      );
    });

    it('Radio section has an ardopcf binary input row (tuxlink-jmfm)', async () => {
      render(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() =>
        expect(screen.getByTestId('ardop-binary-input')).toBeInTheDocument(),
      );
    });

    it('cmd_port input persists on blur (tuxlink-jmfm)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      invokeMock.mockImplementation(async (cmd: string) => {
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
        if (cmd === 'session_log_snapshot') return [];
        return undefined;
      });
      render(<ArdopRadioPanel onClose={() => {}} />);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('ardop_list_audio_devices');
      });
      const captureSel = await screen.findByTestId('ardop-capture-select') as HTMLSelectElement;
      const playbackSel = await screen.findByTestId('ardop-playback-select') as HTMLSelectElement;
      const capValues = Array.from(captureSel.options).map((o) => o.value);
      const playValues = Array.from(playbackSel.options).map((o) => o.value);
      expect(capValues).toContain('plughw:CARD=Device,DEV=0');
      expect(capValues).toContain('default');
      expect(playValues).toContain('plughw:CARD=Device,DEV=0');
    });

    it('selecting a capture device persists capture_device via config_set_ardop', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string) => {
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
        if (cmd === 'session_log_snapshot') return [];
        return undefined;
      });
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      invokeMock.mockImplementation(async (cmd: string) => {
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
        if (cmd === 'session_log_snapshot') return [];
        return undefined;
      });
      render(<ArdopRadioPanel onClose={() => {}} />);
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
      invokeMock.mockImplementation(async (cmd: string) => {
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
        if (cmd === 'session_log_snapshot') return [];
        return undefined;
      });
      render(<ArdopRadioPanel onClose={() => {}} />);
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
});
