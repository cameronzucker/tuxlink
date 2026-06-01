// src/radio/modes/TelnetP2pRadioPanel.test.tsx
//
// TDD tests for the P2P Telnet right-hand radio panel (Task 6, path (a)).
// Mirrors TelnetRadioPanel.test.tsx conventions: mock invoke + listen,
// reset per-test, verify render + IPC wiring.
//
// Tauri commands under test:
//   telnet_p2p_dial({ req: { host, port, peer_callsign, my_callsign, locator } })
//     → { sent_count, received_count }  (on success)
//     → throws string  (on failure)
//   p2p_peer_password_status(callsign) → "Set" | "NotSet"
//   p2p_peer_password_set(callsign, password) → void
//   p2p_peer_password_clear(callsign) → void

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { TelnetP2pRadioPanel } from './TelnetP2pRadioPanel';

// ── Tauri mocks ────────────────────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

// Default invoke implementation:
//   - config_read → callsign + grid (sourced the same way TelnetRadioPanel does)
//   - p2p_peer_password_status → 'NotSet'
//   - session_log_snapshot → []
const defaultInvokeImpl = async (cmd: string) => {
  if (cmd === 'config_read') {
    return { callsign: 'N0CALL', grid: 'CN87' };
  }
  if (cmd === 'p2p_peer_password_status') {
    return 'NotSet';
  }
  if (cmd === 'session_log_snapshot') {
    return [];
  }
  return undefined;
};

describe('<TelnetP2pRadioPanel>', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
  });

  // ── Rendering ─────────────────────────────────────────────────────────

  it('renders the Telnet P2P panel title', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet P2P');
  });

  it('renders peer host input with default 127.0.0.1', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    const hostInput = screen.getByTestId('p2p-host-input') as HTMLInputElement;
    expect(hostInput.value).toBe('127.0.0.1');
  });

  it('renders port input with default 8772 (WLE P2P parity)', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    const portInput = screen.getByTestId('p2p-port-input') as HTMLInputElement;
    expect(portInput.value).toBe('8772');
  });

  it('renders peer callsign input', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('p2p-peer-callsign-input')).toBeInTheDocument();
  });

  it('renders password status badge showing <not set> initially', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    // Status badge should show after the callsign is known; initial state = <not set>
    expect(screen.getByTestId('p2p-password-status')).toHaveTextContent('<not set>');
  });

  it('renders Set and Clear password buttons', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('p2p-password-set-btn')).toBeInTheDocument();
    expect(screen.getByTestId('p2p-password-clear-btn')).toBeInTheDocument();
  });

  it('renders Connect button', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('p2p-connect-btn')).toBeInTheDocument();
  });

  // ── Callsign / locator sourced from config_read ──────────────────────

  it('reads my_callsign and locator from config_read on mount', async () => {
    const core = await import('@tauri-apps/api/core');
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith('config_read');
    });
  });

  // ── Connect button invokes telnet_p2p_dial ────────────────────────────

  it('Connect button calls telnet_p2p_dial with current form values', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'p2p_peer_password_status') return 'NotSet';
      if (cmd === 'session_log_snapshot') return [];
      if (cmd === 'telnet_p2p_dial') {
        // Verify the argument shape and return success
        const req = (args as { req: { host: string; port: number; peer_callsign: string; my_callsign: string; locator: string } }).req;
        expect(req.host).toBe('192.168.1.50');
        expect(req.port).toBe(8772);
        expect(req.peer_callsign).toBe('W7AUX');
        expect(req.my_callsign).toBe('N0CALL');
        expect(req.locator).toBe('CN87');
        return { sent_count: 1, received_count: 2 };
      }
      return undefined;
    });

    render(<TelnetP2pRadioPanel onClose={() => {}} />);

    // Wait for config to load
    await waitFor(() => expect((core.invoke as ReturnType<typeof vi.fn>)).toHaveBeenCalledWith('config_read'));

    fireEvent.change(screen.getByTestId('p2p-host-input'), { target: { value: '192.168.1.50' } });
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), { target: { value: 'W7AUX' } });
    fireEvent.click(screen.getByTestId('p2p-connect-btn'));

    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith('telnet_p2p_dial', {
        req: {
          host: '192.168.1.50',
          port: 8772,
          peer_callsign: 'W7AUX',
          my_callsign: 'N0CALL',
          locator: 'CN87',
        },
      });
    });
  });

  it('shows Sent N, received M. on successful dial', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'p2p_peer_password_status') return 'NotSet';
      if (cmd === 'session_log_snapshot') return [];
      if (cmd === 'telnet_p2p_dial') return { sent_count: 3, received_count: 1 };
      return undefined;
    });

    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    await waitFor(() => expect((core.invoke as ReturnType<typeof vi.fn>)).toHaveBeenCalledWith('config_read'));

    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), { target: { value: 'W7AUX' } });
    fireEvent.click(screen.getByTestId('p2p-connect-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('p2p-result')).toHaveTextContent('Sent 3, received 1.');
    });
  });

  it('shows error string when telnet_p2p_dial rejects', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'p2p_peer_password_status') return 'NotSet';
      if (cmd === 'session_log_snapshot') return [];
      if (cmd === 'telnet_p2p_dial') throw new Error('Connection refused');
      return undefined;
    });

    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    await waitFor(() => expect((core.invoke as ReturnType<typeof vi.fn>)).toHaveBeenCalledWith('config_read'));

    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), { target: { value: 'W7AUX' } });
    fireEvent.click(screen.getByTestId('p2p-connect-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('p2p-error')).toHaveTextContent('Connection refused');
    });
  });

  // ── Password status badge ─────────────────────────────────────────────

  it('password status badge shows <set> when p2p_peer_password_status returns Set', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'p2p_peer_password_status') return 'Set';
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });

    render(<TelnetP2pRadioPanel onClose={() => {}} />);

    // Type in a callsign to trigger a status fetch
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), { target: { value: 'W7AUX' } });

    await waitFor(() => {
      expect(screen.getByTestId('p2p-password-status')).toHaveTextContent('<set>');
    });
  });

  it('password status badge updates to <set> after password is cleared via Set (mocked prompt)', async () => {
    // This test checks that clearing via the Clear button updates the badge to <not set>
    const core = await import('@tauri-apps/api/core');
    let statusResult = 'Set';
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'p2p_peer_password_status') return statusResult;
      if (cmd === 'p2p_peer_password_clear') { statusResult = 'NotSet'; return undefined; }
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });

    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), { target: { value: 'W7AUX' } });

    await waitFor(() => {
      expect(screen.getByTestId('p2p-password-status')).toHaveTextContent('<set>');
    });

    fireEvent.click(screen.getByTestId('p2p-password-clear-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('p2p-password-status')).toHaveTextContent('<not set>');
    });
  });

  // ── Session log section ───────────────────────────────────────────────

  it('renders the Session log section', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  // ── Close button ──────────────────────────────────────────────────────

  it('close button calls onClose', () => {
    const onClose = vi.fn();
    render(<TelnetP2pRadioPanel onClose={onClose} />);
    fireEvent.click(screen.getByTestId('radio-panel-close'));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
