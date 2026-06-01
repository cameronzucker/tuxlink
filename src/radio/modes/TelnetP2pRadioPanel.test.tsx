// src/radio/modes/TelnetP2pRadioPanel.test.tsx
//
// Tests for TelnetP2pRadioPanel (tuxlink-0pnb refactor: structural mirror of
// TelnetRadioPanel). Mirrors TelnetRadioPanel.test.tsx conventions exactly:
//   - Same vi.mock structure for @tauri-apps/api/core + event.
//   - lastSessionLogHandler capture for live-tail tests.
//   - defaultInvokeImpl + beforeEach reset pattern.
//
// Tauri commands under test:
//   config_read()                               → { callsign, grid }
//   telnet_p2p_connect({ req: {...} })          → { sent_count, received_count }
//   telnet_p2p_abort()                          → void
//   p2p_peer_password_status(callsign)          → "Set" | "NotSet"
//   p2p_peer_password_set(callsign, password)   → void
//   p2p_peer_password_clear(callsign)           → void

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { TelnetP2pRadioPanel } from './TelnetP2pRadioPanel';

// Tauri IPC mocks. `invoke` returns command-specific defaults; `listen`
// captures the registered handler so tests can dispatch synthetic
// `session_log:line` events (same pattern as TelnetRadioPanel.test.tsx).
let lastSessionLogHandler: ((event: { payload: unknown }) => void) | null = null;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
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
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    if (event === 'session_log:line') {
      lastSessionLogHandler = handler;
    }
    return () => {
      lastSessionLogHandler = null;
    };
  }),
}));

// Default invoke implementation — applied per-test in beforeEach so an
// override in one test cannot leak into the next.
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
    lastSessionLogHandler = null;
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
  });

  // ── Panel title ───────────────────────────────────────────────────────────

  it('renders the Telnet P2P panel title', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet P2P');
  });

  // ── Peer Station section ──────────────────────────────────────────────────

  it('renders peer host input with default 127.0.0.1', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    const hostInput = screen.getByTestId('p2p-host-input') as HTMLInputElement;
    expect(hostInput.value).toBe('127.0.0.1');
  });

  it('renders peer callsign input', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('p2p-peer-callsign-input')).toBeInTheDocument();
  });

  it('renders localhost quick-pick chip', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('p2p-pick-127.0.0.1')).toBeInTheDocument();
  });

  it('clicking the localhost quick-pick chip sets the host input', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    const hostInput = screen.getByTestId('p2p-host-input') as HTMLInputElement;
    fireEvent.change(hostInput, { target: { value: '192.168.1.50' } });
    expect(hostInput.value).toBe('192.168.1.50');
    fireEvent.click(screen.getByTestId('p2p-pick-127.0.0.1'));
    expect(hostInput.value).toBe('127.0.0.1');
  });

  it('typing in the host input and blurring trims whitespace', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    const hostInput = screen.getByTestId('p2p-host-input') as HTMLInputElement;
    fireEvent.change(hostInput, { target: { value: '  192.168.1.50  ' } });
    fireEvent.blur(hostInput);
    expect(hostInput.value).toBe('192.168.1.50');
  });

  it('peer callsign input forces uppercase', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    const callsignInput = screen.getByTestId('p2p-peer-callsign-input') as HTMLInputElement;
    fireEvent.change(callsignInput, { target: { value: 'w7aux' } });
    expect(callsignInput.value).toBe('W7AUX');
  });

  // ── Transport section (plaintext-only note) ───────────────────────────────

  it('renders the Transport section with plaintext note (no TLS option)', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    // The transport section shows "Plaintext · port 8772" — no radio buttons,
    // no TLS option. WLE P2P is plaintext-only per spec §4.3.
    expect(screen.getByText(/Plaintext · port 8772/)).toBeInTheDocument();
  });

  // ── Peer Password section ─────────────────────────────────────────────────

  it('renders password status badge showing Not set initially', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('p2p-password-status')).toHaveTextContent('Not set');
  });

  it('renders Set and Clear password buttons', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('p2p-password-set-btn')).toBeInTheDocument();
    expect(screen.getByTestId('p2p-password-clear-btn')).toBeInTheDocument();
  });

  it('password status badge shows Set when p2p_peer_password_status returns Set', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'p2p_peer_password_status') return 'Set';
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), {
      target: { value: 'W7AUX' },
    });
    await waitFor(() => {
      expect(screen.getByTestId('p2p-password-status')).toHaveTextContent('Set');
    });
  });

  it('Clear button clears the password and updates the badge to Not set', async () => {
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
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), {
      target: { value: 'W7AUX' },
    });
    await waitFor(() => {
      expect(screen.getByTestId('p2p-password-status')).toHaveTextContent('Set');
    });
    fireEvent.click(screen.getByTestId('p2p-password-clear-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('p2p-password-status')).toHaveTextContent('Not set');
    });
  });

  // ── config_read on mount ──────────────────────────────────────────────────

  it('reads my_callsign and locator from config_read on mount', async () => {
    const core = await import('@tauri-apps/api/core');
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith('config_read');
    });
  });

  it('falls back gracefully when config_read rejects', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') throw new Error('NotConfigured');
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    // Should not throw; panel renders with empty callsign/locator defaults.
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('p2p-host-input')).toBeInTheDocument();
  });

  // ── Session log section ───────────────────────────────────────────────────

  it('renders the Session log section', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('renders backend log lines that arrive on session_log:line', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    await waitFor(() => expect(lastSessionLogHandler).not.toBeNull());
    act(() => {
      lastSessionLogHandler!({
        payload: {
          seq: 1,
          timestampIso: '2026-06-01T12:00:00.000Z',
          level: 'info',
          source: 'backend',
          message: 'Connecting to W7AUX @ 127.0.0.1:8772 (P2P-Telnet)…',
        },
      });
    });
    expect(
      await screen.findByText(/Connecting to W7AUX @ 127\.0\.0\.1:8772/),
    ).toBeInTheDocument();
  });

  // ── Connect / Stop actions ────────────────────────────────────────────────

  it('renders Connect and Stop actions', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByRole('button', { name: /Connect/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Stop/i })).toBeInTheDocument();
  });

  it('clicking Connect fires telnet_p2p_connect with current form values', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
        if (cmd === 'p2p_peer_password_status') return 'NotSet';
        if (cmd === 'session_log_snapshot') return [];
        if (cmd === 'telnet_p2p_connect') {
          // Verify the argument shape and return a success result.
          const req = (args as { req: {
            host: string; port: number; peer_callsign: string;
            my_callsign: string; locator: string;
          } }).req;
          expect(req.host).toBe('192.168.1.50');
          expect(req.port).toBe(8772);
          expect(req.peer_callsign).toBe('W7AUX');
          expect(req.my_callsign).toBe('N0CALL');
          expect(req.locator).toBe('CN87');
          return { sent_count: 1, received_count: 2 };
        }
        return undefined;
      },
    );
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    await waitFor(() =>
      expect((core.invoke as ReturnType<typeof vi.fn>)).toHaveBeenCalledWith('config_read'),
    );
    fireEvent.change(screen.getByTestId('p2p-host-input'), {
      target: { value: '192.168.1.50' },
    });
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), {
      target: { value: 'W7AUX' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Connect/i }));
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith('telnet_p2p_connect', {
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

  it('clicking Stop fires telnet_p2p_abort', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: /Stop/i }));
    expect(invoke).toHaveBeenCalledWith('telnet_p2p_abort');
  });

  it('shows Sent N, received M. on successful connect', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'p2p_peer_password_status') return 'NotSet';
      if (cmd === 'session_log_snapshot') return [];
      if (cmd === 'telnet_p2p_connect') return { sent_count: 3, received_count: 1 };
      return undefined;
    });
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    await waitFor(() =>
      expect((core.invoke as ReturnType<typeof vi.fn>)).toHaveBeenCalledWith('config_read'),
    );
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), {
      target: { value: 'W7AUX' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Connect/i }));
    await waitFor(() => {
      expect(screen.getByTestId('p2p-result')).toHaveTextContent('Sent 3, received 1.');
    });
  });

  it('shows error string when telnet_p2p_connect rejects', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'p2p_peer_password_status') return 'NotSet';
      if (cmd === 'session_log_snapshot') return [];
      if (cmd === 'telnet_p2p_connect') throw new Error('Connection refused');
      return undefined;
    });
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    await waitFor(() =>
      expect((core.invoke as ReturnType<typeof vi.fn>)).toHaveBeenCalledWith('config_read'),
    );
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), {
      target: { value: 'W7AUX' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Connect/i }));
    await waitFor(() => {
      expect(screen.getByTestId('p2p-error')).toHaveTextContent('Connection refused');
    });
  });

  // ── Header sub shows host:port ────────────────────────────────────────────

  it('header sub shows host:port with default values on mount', () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    expect(screen.getByText('127.0.0.1:8772')).toBeInTheDocument();
  });

  it('header sub includes peer callsign when entered', async () => {
    render(<TelnetP2pRadioPanel onClose={() => {}} />);
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), {
      target: { value: 'W7AUX' },
    });
    await waitFor(() => {
      expect(screen.getByText('W7AUX @ 127.0.0.1:8772')).toBeInTheDocument();
    });
  });

  // ── Close button ──────────────────────────────────────────────────────────

  it('close button calls onClose', () => {
    const onClose = vi.fn();
    render(<TelnetP2pRadioPanel onClose={onClose} />);
    fireEvent.click(screen.getByTestId('radio-panel-close'));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
