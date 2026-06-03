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
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { TelnetP2pRadioPanel } from './TelnetP2pRadioPanel';

/// Wrap the panel in a fresh QueryClient per test so the `useQueryClient`
/// hook resolves (tuxlink-l55l added mailbox-query invalidation after a
/// successful dial). `retry: false` keeps test runs deterministic.
function renderPanel(props: { onClose?: () => void } = {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <TelnetP2pRadioPanel onClose={props.onClose ?? (() => {})} />
    </QueryClientProvider>,
  );
}

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
  // Listener defaults — backend ships allow_all=true (post-flip per
  // tuxlink-7vea); see commit 5261f59.
  if (cmd === 'telnet_listen_config_get') {
    return { port: 8774, bind_addr: '127.0.0.1', ttl_secs: 3600 };
  }
  if (cmd === 'telnet_station_password_is_set') {
    // Backend returns StationPasswordStatus enum string, not bool.
    return 'NotSet';
  }
  if (cmd === 'telnet_allowed_stations_get') {
    return { allow_all: true, callsigns: [], ips: [] };
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
    renderPanel();
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet P2P');
  });

  // ── Peer Station section ──────────────────────────────────────────────────

  it('renders peer host input with default 127.0.0.1', () => {
    renderPanel();
    const hostInput = screen.getByTestId('p2p-host-input') as HTMLInputElement;
    expect(hostInput.value).toBe('127.0.0.1');
  });

  it('renders peer callsign input', () => {
    renderPanel();
    expect(screen.getByTestId('p2p-peer-callsign-input')).toBeInTheDocument();
  });

  it('renders localhost quick-pick chip', () => {
    renderPanel();
    expect(screen.getByTestId('p2p-pick-127.0.0.1')).toBeInTheDocument();
  });

  it('clicking the localhost quick-pick chip sets the host input', () => {
    renderPanel();
    const hostInput = screen.getByTestId('p2p-host-input') as HTMLInputElement;
    fireEvent.change(hostInput, { target: { value: '192.168.1.50' } });
    expect(hostInput.value).toBe('192.168.1.50');
    fireEvent.click(screen.getByTestId('p2p-pick-127.0.0.1'));
    expect(hostInput.value).toBe('127.0.0.1');
  });

  it('typing in the host input and blurring trims whitespace', () => {
    renderPanel();
    const hostInput = screen.getByTestId('p2p-host-input') as HTMLInputElement;
    fireEvent.change(hostInput, { target: { value: '  192.168.1.50  ' } });
    fireEvent.blur(hostInput);
    expect(hostInput.value).toBe('192.168.1.50');
  });

  it('peer callsign input forces uppercase', () => {
    renderPanel();
    const callsignInput = screen.getByTestId('p2p-peer-callsign-input') as HTMLInputElement;
    fireEvent.change(callsignInput, { target: { value: 'w7aux' } });
    expect(callsignInput.value).toBe('W7AUX');
  });

  // ── Transport section (plaintext-only note) ───────────────────────────────

  it('renders the Transport section with plaintext note (no TLS option)', () => {
    renderPanel();
    // Transport section explains plaintext-only — no radio buttons, no TLS
    // option. WLE P2P is plaintext-only per spec §4.3.
    expect(screen.getByText(/Plaintext only/)).toBeInTheDocument();
  });

  it('port input defaults to 8772 and is operator-editable', () => {
    renderPanel();
    const portInput = screen.getByTestId('p2p-port-input') as HTMLInputElement;
    expect(portInput.value).toBe('8772');
    fireEvent.change(portInput, { target: { value: '9000' } });
    expect(portInput.value).toBe('9000');
  });

  // ── Peer Password section ─────────────────────────────────────────────────

  it('renders password status badge showing Not set initially', () => {
    renderPanel();
    expect(screen.getByTestId('p2p-password-status')).toHaveTextContent('Not set');
  });

  it('renders Set and Clear password buttons', () => {
    renderPanel();
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
    renderPanel();
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
    renderPanel();
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
    renderPanel();
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
    renderPanel();
    expect(screen.getByTestId('p2p-host-input')).toBeInTheDocument();
  });

  // ── Session log section ───────────────────────────────────────────────────

  it('renders the Session log section', () => {
    renderPanel();
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('renders backend log lines that arrive on session_log:line', async () => {
    renderPanel();
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
    renderPanel();
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
    renderPanel();
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

  it('operator port override flows to telnet_p2p_connect', async () => {
    const core = await import('@tauri-apps/api/core');
    let observedPort = 0;
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      async (cmd: string, args?: unknown) => {
        if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
        if (cmd === 'p2p_peer_password_status') return 'NotSet';
        if (cmd === 'session_log_snapshot') return [];
        if (cmd === 'telnet_p2p_connect') {
          observedPort = (args as { req: { port: number } }).req.port;
          return { sent_count: 0, received_count: 0 };
        }
        return undefined;
      },
    );
    renderPanel();
    await waitFor(() =>
      expect((core.invoke as ReturnType<typeof vi.fn>)).toHaveBeenCalledWith('config_read'),
    );
    fireEvent.change(screen.getByTestId('p2p-port-input'), { target: { value: '9000' } });
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), { target: { value: 'W7AUX' } });
    fireEvent.click(screen.getByRole('button', { name: /Connect/i }));
    await waitFor(() => expect(observedPort).toBe(9000));
  });

  it('clicking Stop fires telnet_p2p_abort', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel();
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
    renderPanel();
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
    renderPanel();
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
    renderPanel();
    expect(screen.getByText('127.0.0.1:8772')).toBeInTheDocument();
  });

  it('header sub includes peer callsign when entered', async () => {
    renderPanel();
    fireEvent.change(screen.getByTestId('p2p-peer-callsign-input'), {
      target: { value: 'W7AUX' },
    });
    await waitFor(() => {
      expect(screen.getByText('W7AUX @ 127.0.0.1:8772')).toBeInTheDocument();
    });
  });

  // ── Listen section (tuxlink-7vea) ────────────────────────────────────────
  //
  // The Listen section was added per spec 2026-06-03-listener-ui-design.md
  // §1.3. Tests assert the full mutation surface (arm/disarm, allowlist
  // add/remove, allow-any-peer toggle, station-password set/clear, listener
  // config edits) and the armed-state indicator.

  it('renders the Listen section', async () => {
    renderPanel();
    expect(await screen.findByTestId('telnet-listen-section')).toBeInTheDocument();
  });

  it('Arm button click fires telnet_listen and flips the status to ARMED', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('telnet-listen-arm-btn')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-listen-arm-btn'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('telnet_listen');
    });
    await waitFor(() => {
      expect(screen.getByTestId('telnet-listen-status')).toHaveTextContent(/ARMED/);
    });
  });

  it('Disarm button click fires telnet_set_listen with enabled=false', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel();
    // First arm so the disarm button appears.
    await waitFor(() =>
      expect(screen.getByTestId('telnet-listen-arm-btn')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-listen-arm-btn'));
    await waitFor(() =>
      expect(screen.getByTestId('telnet-listen-disarm-btn')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-listen-disarm-btn'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('telnet_set_listen', { enabled: false });
    });
  });

  it('Allow-any-peer toggle fires telnet_allowed_stations_set_allow_all', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('telnet-allowed-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-allowed-expander'));
    await waitFor(() =>
      expect(screen.getByTestId('telnet-allowed-allow-all-toggle')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-allowed-allow-all-toggle'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'telnet_allowed_stations_set_allow_all',
        { enabled: false },
      );
    });
  });

  it('adding a callsign fires telnet_allowed_stations_add_callsign', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('telnet-allowed-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-allowed-expander'));
    await waitFor(() =>
      expect(screen.getByTestId('telnet-allowed-callsign-add-btn')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-allowed-callsign-add-btn'));
    const input = await screen.findByTestId('telnet-allowed-callsign-add-input');
    fireEvent.change(input, { target: { value: 'w7aux' } });
    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'telnet_allowed_stations_add_callsign',
        { callsign: 'W7AUX' },
      );
    });
  });

  it('removing a callsign fires telnet_allowed_stations_remove_callsign', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'telnet_allowed_stations_get') {
        return { allow_all: false, callsigns: ['N7CPZ'], ips: [] };
      }
      return defaultInvokeImpl(cmd);
    });
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('telnet-allowed-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-allowed-expander'));
    await waitFor(() =>
      expect(screen.getByTestId('telnet-allowed-callsign-remove-N7CPZ')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-allowed-callsign-remove-N7CPZ'));
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith(
        'telnet_allowed_stations_remove_callsign',
        { callsign: 'N7CPZ' },
      );
    });
  });

  it('adding an IP pattern fires telnet_allowed_stations_add_ip', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('telnet-allowed-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-allowed-expander'));
    await waitFor(() =>
      expect(screen.getByTestId('telnet-allowed-ip-add-btn')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-allowed-ip-add-btn'));
    const input = await screen.findByTestId('telnet-allowed-ip-add-input');
    fireEvent.change(input, { target: { value: '192.168.1.*' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'telnet_allowed_stations_add_ip',
        { pattern: '192.168.1.*' },
      );
    });
  });

  it('Station password Set fires telnet_station_password_set', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const promptSpy = vi.spyOn(window, 'prompt').mockReturnValue('hunter2');
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('telnet-station-pw-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-station-pw-expander'));
    const setBtn = await screen.findByTestId('telnet-station-pw-set-btn');
    fireEvent.click(setBtn);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'telnet_station_password_set',
        { password: 'hunter2' },
      );
    });
    promptSpy.mockRestore();
  });

  it('Station password Clear fires telnet_station_password_clear', async () => {
    const core = await import('@tauri-apps/api/core');
    // Backend returns the StationPasswordStatus enum, NOT a bool — Codex
    // 2026-06-03 fix.
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'telnet_station_password_is_set') return 'Set';
      return defaultInvokeImpl(cmd);
    });
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('telnet-station-pw-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-station-pw-expander'));
    const clearBtn = await screen.findByTestId('telnet-station-pw-clear-btn');
    await waitFor(() => expect(clearBtn).not.toBeDisabled());
    fireEvent.click(clearBtn);
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith('telnet_station_password_clear');
    });
  });

  it('listener setup TTL change fires telnet_listen_config_set', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('telnet-listen-setup-expander')).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId('telnet-listen-setup-expander'));
    const ttlSelect = await screen.findByTestId('telnet-listen-ttl-select');
    fireEvent.change(ttlSelect, { target: { value: String(15 * 60) } });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'telnet_listen_config_set',
        // Codex 2026-06-03: backend signature is `req: TelnetListenConfigDto`,
        // so the DTO must be wrapped in `{ req: ... }`.
        expect.objectContaining({
          req: expect.objectContaining({ ttl_secs: 15 * 60 }),
        }),
      );
    });
  });

  it('listener-config get is fetched on mount', async () => {
    const core = await import('@tauri-apps/api/core');
    renderPanel();
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith('telnet_listen_config_get');
    });
  });

  it('allowlist count chip reflects allow_all default', async () => {
    renderPanel();
    await waitFor(() =>
      expect(screen.getByTestId('telnet-allowed-count')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('telnet-allowed-count')).toHaveTextContent(/allow any/);
  });

  // ── Close button ──────────────────────────────────────────────────────────

  it('close button calls onClose', () => {
    const onClose = vi.fn();
    renderPanel({ onClose });
    fireEvent.click(screen.getByTestId('radio-panel-close'));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
