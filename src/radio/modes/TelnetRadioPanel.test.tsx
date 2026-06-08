// src/radio/modes/TelnetRadioPanel.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';
import type { FavoriteDial } from '../../favorites/types';
import { TelnetRadioPanel } from './TelnetRadioPanel';

// The panel now mounts FavoritesTabs/useFavorites (react-query), so every
// render must be wrapped in a QueryClientProvider or the queries throw
// "No QueryClient set". retry:false keeps a rejected favorites read from
// retrying through the test. (Mirrors the ARDOP/Packet test renderPanel.)
const renderPanel = (ui: ReactElement) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
};

// The Server (Host) + Transport controls now live in the FavoritesTabs
// "Manual" tab (Task B6-TELNET). Radix Tabs.Trigger switches on mouseDown
// (button 0) under jsdom, not click. Tests that need those controls call
// this to switch to the Manual tab first. (Start/Stop stay OUTSIDE the tabs,
// so they remain directly reachable.)
const switchToManualTab = async () => {
  const manual = await screen.findByRole('tab', { name: 'Manual' });
  fireEvent.mouseDown(manual, { button: 0 });
};

// Tauri IPC mocks. `invoke` returns command-specific defaults; `listen`
// captures the registered handler so tests can dispatch synthetic
// `session_log:line` events.
let lastSessionLogHandler: ((event: { payload: unknown }) => void) | null = null;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
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

// Default invoke implementation — applied per-test in beforeEach so a test
// that overrides via mockImplementation cannot leak into the next test.
// ALL custom mockImplementations fall through to this so the mounted
// FavoritesTabs/useFavorites queries (favorites_read / favorites_recents /
// position_current_fix / favorite_tod_hint) resolve to benign shapes — a
// query that returned undefined would emit react-query's "Query data cannot
// be undefined" console error.
const defaultInvokeImpl = async (cmd: string, _args?: unknown) => {
  if (cmd === 'config_read') {
    return { host: 'cms.winlink.org', transport: 'CmsSsl' };
  }
  if (cmd === 'session_log_snapshot') {
    return [];
  }
  // Favorites surface (Task B6-TELNET). The mounted FavoritesTabs/useFavorites
  // issue these reads; return empty/benign shapes so the queries RESOLVE
  // (rejecting or returning undefined would noisily fail in jsdom). Tests that
  // need a clickable favorite override favorites_read / favorites_recents.
  if (cmd === 'favorites_read') {
    return { schema_version: 1, favorites: [], log: [] };
  }
  if (cmd === 'favorites_recents') return [];
  if (cmd === 'position_current_fix') return { grid: null };
  if (cmd === 'favorite_tod_hint') return null;
  return undefined;
};

describe('<TelnetRadioPanel>', () => {
  beforeEach(async () => {
    lastSessionLogHandler = null;
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(defaultInvokeImpl);
  });

  it('renders the Telnet Winlink panel with host loaded from config_read', async () => {
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet Winlink');
    await switchToManualTab();
    await waitFor(() => {
      const hostInput = screen.getByTestId('telnet-host-input') as HTMLInputElement;
      expect(hostInput.value).toBe('cms.winlink.org');
    });
  });

  it('renders both transport options with port labels', async () => {
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    expect(screen.getByText(/TLS · port 8773/)).toBeInTheDocument();
    expect(screen.getByText(/Plaintext · port 8772/)).toBeInTheDocument();
  });

  it('renders quick-pick chips for dev + prod CMS hosts', async () => {
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    expect(screen.getByTestId('telnet-pick-cms-z.winlink.org')).toBeInTheDocument();
    expect(screen.getByTestId('telnet-pick-server.winlink.org')).toBeInTheDocument();
  });

  it('falls back to default host/transport when config_read rejects', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'config_read') throw new Error('NotConfigured');
      return defaultInvokeImpl(cmd, args);
    });
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    const hostInput = screen.getByTestId('telnet-host-input') as HTMLInputElement;
    expect(hostInput.value).toBe('cms.winlink.org'); // DEFAULT_HOST
  });

  it('reflects a non-default host from config_read', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'config_read') return { host: 'cms-z.winlink.org', transport: 'Telnet' };
      return defaultInvokeImpl(cmd, args);
    });
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    await waitFor(() => {
      const hostInput = screen.getByTestId('telnet-host-input') as HTMLInputElement;
      expect(hostInput.value).toBe('cms-z.winlink.org');
    });
    // Transport radio also reflects config_read
    const telnetRadio = screen.getByTestId('telnet-transport-Telnet') as HTMLInputElement;
    expect(telnetRadio.checked).toBe(true);
  });

  it('clicking a quick-pick chip persists the new host via config_set_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    await waitFor(() => expect(screen.getByTestId('telnet-host-input')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('telnet-pick-cms-z.winlink.org'));
    expect(invoke).toHaveBeenCalledWith('config_set_connect', {
      host: 'cms-z.winlink.org',
      transport: 'CmsSsl',
    });
  });

  it('editing the host and blurring persists via config_set_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    const hostInput = (await screen.findByTestId('telnet-host-input')) as HTMLInputElement;
    fireEvent.change(hostInput, { target: { value: 'my.cms.example' } });
    fireEvent.blur(hostInput);
    expect(invoke).toHaveBeenCalledWith('config_set_connect', {
      host: 'my.cms.example',
      transport: 'CmsSsl',
    });
  });

  it('selecting a different transport persists via config_set_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    await switchToManualTab();
    await waitFor(() => expect(screen.getByTestId('telnet-transport-Telnet')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('telnet-transport-Telnet'));
    expect(invoke).toHaveBeenCalledWith('config_set_connect', {
      host: 'cms.winlink.org',
      transport: 'Telnet',
    });
  });

  it('renders the Session log section', () => {
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('renders backend log lines that arrive on session_log:line', async () => {
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    await waitFor(() => expect(lastSessionLogHandler).not.toBeNull());
    act(() => {
      lastSessionLogHandler!({
        payload: {
          seq: 1,
          timestampIso: '2026-05-31T19:35:58.000Z',
          level: 'info',
          source: 'backend',
          message: 'Connecting to cms.winlink.org:8773',
        },
      });
    });
    expect(await screen.findByText(/Connecting to cms\.winlink\.org:8773/)).toBeInTheDocument();
  });

  it('renders Start and Stop actions', () => {
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByRole('button', { name: /Start/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Stop/i })).toBeInTheDocument();
  });

  it('clicking Start fires cms_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: /Start/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('cms_connect'));
  });

  it('clicking Stop fires cms_abort', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: /Stop/i }));
    expect(invoke).toHaveBeenCalledWith('cms_abort');
  });

  it('header sub shows host:port composed from transport', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'config_read') return { host: 'cms-z.winlink.org', transport: 'Telnet' };
      return defaultInvokeImpl(cmd, args);
    });
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    await waitFor(() => {
      // Header sub renders host:port (Telnet → 8772)
      expect(screen.getByText('cms-z.winlink.org:8772')).toBeInTheDocument();
    });
  });

  it('renders AuthDiagnosticBanner / panel body without throwing', async () => {
    renderPanel(<TelnetRadioPanel onClose={() => {}} />);
    // Verify the component is mounted in the panel hierarchy without throwing.
    expect(screen.getByTestId('session-log-section')).not.toBeNull();
  });

  // ── Favorites integration (Task B6-TELNET) ───────────────────────────────
  //
  // RADIO-1 + H7 + M4 + M13. A favorite's Connect PRE-FILLS host + transport
  // only (never transmits). Telnet's `cms_connect` is a BLOCKING connect→B2F,
  // so the honest signal is the resolve/reject of that single call: `reached`
  // is recorded on resolve, `failed` is recorded in the CATCH (never finally).
  // The record dial keys on transport (H7) — no freq/band. The record
  // timestamp carries a UTC offset (M4 / H1).

  const findRecordCalls = (invokeMock: ReturnType<typeof vi.fn>) =>
    invokeMock.mock.calls.filter(([cmd]) => cmd === 'favorite_record_attempt');

  describe('Favorites integration (B6-TELNET)', () => {
    it('records reached when cms_connect resolves', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'cms_connect') return null; // resolves = on-air reach
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<TelnetRadioPanel onClose={() => {}} />);
      fireEvent.click(screen.getByRole('button', { name: /Start/i }));
      await waitFor(() => {
        const calls = findRecordCalls(invokeMock);
        expect(calls).toHaveLength(1);
        const [, args] = calls[0] as [
          string,
          { dial: FavoriteDial; outcome: string },
        ];
        expect(args.outcome).toBe('reached');
        expect(args.dial.mode).toBe('telnet');
        expect(args.dial.gateway).toBe('cms.winlink.org');
        expect(args.dial.transport).toBe('CmsSsl');
      });
    });

    it('records failed when cms_connect rejects (H4 — failure observable)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'cms_connect') throw new Error('connection refused');
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<TelnetRadioPanel onClose={() => {}} />);
      fireEvent.click(screen.getByRole('button', { name: /Start/i }));
      await waitFor(() => {
        const calls = findRecordCalls(invokeMock);
        expect(calls).toHaveLength(1);
        const [, args] = calls[0] as [
          string,
          { dial: FavoriteDial; outcome: string },
        ];
        expect(args.outcome).toBe('failed');
        expect(args.dial.mode).toBe('telnet');
        expect(args.dial.gateway).toBe('cms.winlink.org');
      });
    });

    it('CONSENT NON-BYPASS (M13): a favorite Connect pre-fills only, never transmits', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      // Route a starred telnet favorite (gateway + transport) so the Favorites
      // tab has a row.
      const fav = {
        id: 'fav-1',
        mode: 'telnet' as const,
        gateway: 'cms-z.winlink.org',
        transport: 'Telnet' as const,
        starred: true,
        created_at: '2026-06-08T00:00:00-07:00',
        updated_at: '2026-06-08T00:00:00-07:00',
      };
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'favorites_read') {
          return { schema_version: 1, favorites: [fav], log: [] };
        }
        if (cmd === 'favorites_recents') return [];
        if (cmd === 'cms_connect') return null;
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<TelnetRadioPanel onClose={() => {}} />);

      // Default tab is Favorites; the favorite's Connect appears there.
      const connectBtn = await screen.findByTestId('favorite-connect-fav-1');
      fireEvent.click(connectBtn);
      // Let any (forbidden) async invoke settle.
      await new Promise((r) => setTimeout(r, 20));

      // RADIO-1: the prefill must NOT have fired cms_connect.
      expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'cms_connect')).toBe(false);

      // H7 prefill persists host + transport via config_set_connect so the
      // operator's later Start dials the right server. (config_set_connect is
      // config persistence, NOT a connect/transmit.)
      expect(invokeMock).toHaveBeenCalledWith('config_set_connect', {
        host: 'cms-z.winlink.org',
        transport: 'Telnet',
      });

      // Prefill worked: the Manual tab's Host input + transport radio reflect it.
      await switchToManualTab();
      const hostInput = (await screen.findByTestId('telnet-host-input')) as HTMLInputElement;
      expect(hostInput.value).toBe('cms-z.winlink.org');
      const telnetRadio = screen.getByTestId('telnet-transport-Telnet') as HTMLInputElement;
      expect(telnetRadio.checked).toBe(true);

      // Consent gate intact: clicking Start NOW invokes cms_connect.
      fireEvent.click(screen.getByRole('button', { name: /Start/i }));
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('cms_connect');
      });
    });

    it('records an offset-bearing ts_local (M4) — not a UTC Z timestamp', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'cms_connect') return null;
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<TelnetRadioPanel onClose={() => {}} />);
      fireEvent.click(screen.getByRole('button', { name: /Start/i }));
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

    it('telnet dial keys on transport (H7) — no freq/band on the recorded dial', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'cms_connect') return null;
        return defaultInvokeImpl(cmd, args);
      });
      renderPanel(<TelnetRadioPanel onClose={() => {}} />);
      // Switch to Manual and select the Plaintext (Telnet) transport so the
      // recorded dial carries a non-default transport.
      await switchToManualTab();
      await waitFor(() =>
        expect(screen.getByTestId('telnet-transport-Telnet')).toBeInTheDocument(),
      );
      fireEvent.click(screen.getByTestId('telnet-transport-Telnet'));
      fireEvent.click(screen.getByRole('button', { name: /Start/i }));
      await waitFor(() => {
        const calls = findRecordCalls(invokeMock);
        expect(calls).toHaveLength(1);
        const [, args] = calls[0] as [string, { dial: FavoriteDial }];
        expect(args.dial.transport).toBe('Telnet');
        expect(args.dial.freq).toBeUndefined();
        expect(args.dial.band).toBeUndefined();
      });
    });
  });
});
