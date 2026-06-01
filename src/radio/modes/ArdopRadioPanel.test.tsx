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

// Mock useModemStatus so each test feeds the panel a specific ModemStatus.
const mockUseModemStatus = vi.fn();
vi.mock('../../modem/useModemStatus', () => ({
  useModemStatus: () => mockUseModemStatus(),
  MODEM_STATUS_EVENT: 'modem:status',
}));

import { ArdopRadioPanel } from './ArdopRadioPanel';

const defaultInvokeImpl = async (cmd: string) => {
  if (cmd === 'session_log_snapshot') return [];
  if (cmd === 'config_get_ardop') return { cmd_port: 8515 };
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

  it('Open WebGUI button opens a URL on cmd_port - 1 (defaults to 8514)', async () => {
    mockUseModemStatus.mockReturnValue({
      status: RUNNING,
      loading: false,
      error: null,
    });
    const openSpy = vi.spyOn(window, 'open').mockImplementation(() => null);
    render(<ArdopRadioPanel onClose={() => {}} />);
    const webguiBtn = screen.getByTestId('ardop-open-webgui-btn');
    fireEvent.click(webguiBtn);
    await waitFor(() => {
      expect(openSpy).toHaveBeenCalledWith('http://localhost:8514/', '_blank');
    });
    openSpy.mockRestore();
  });

  it('does not open WebGUI when cmd_port is below 2 (surfaces an error instead)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_ardop') return { cmd_port: 1 };
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    mockUseModemStatus.mockReturnValue({
      status: RUNNING,
      loading: false,
      error: null,
    });
    const openSpy = vi.spyOn(window, 'open').mockImplementation(() => null);
    render(<ArdopRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('ardop-open-webgui-btn'));
    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/cmd_port/);
    });
    expect(openSpy).not.toHaveBeenCalled();
    openSpy.mockRestore();
  });

  it('close button fires onClose', () => {
    const onClose = vi.fn();
    render(<ArdopRadioPanel onClose={onClose} />);
    fireEvent.click(screen.getByTestId('radio-panel-close'));
    expect(onClose).toHaveBeenCalled();
  });
});
