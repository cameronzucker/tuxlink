// src/radio/modes/TelnetRadioPanel.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { TelnetRadioPanel } from './TelnetRadioPanel';

// Tauri IPC mocks. `invoke` returns command-specific defaults; `listen`
// captures the registered handler so tests can dispatch synthetic
// `session_log:line` events.
let lastSessionLogHandler: ((event: { payload: unknown }) => void) | null = null;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'config_read') {
      return { host: 'cms.winlink.org', transport: 'CmsSsl' };
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

// Default invoke implementation — applied per-test in beforeEach so a test
// that overrides via mockImplementation cannot leak into the next test.
const defaultInvokeImpl = async (cmd: string) => {
  if (cmd === 'config_read') {
    return { host: 'cms.winlink.org', transport: 'CmsSsl' };
  }
  if (cmd === 'session_log_snapshot') {
    return [];
  }
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
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet Winlink');
    await waitFor(() => {
      const hostInput = screen.getByTestId('telnet-host-input') as HTMLInputElement;
      expect(hostInput.value).toBe('cms.winlink.org');
    });
  });

  it('renders both transport options with port labels', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByText(/TLS · port 8773/)).toBeInTheDocument();
    expect(screen.getByText(/Plaintext · port 8772/)).toBeInTheDocument();
  });

  it('renders quick-pick chips for dev + prod CMS hosts', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('telnet-pick-cms-z.winlink.org')).toBeInTheDocument();
    expect(screen.getByTestId('telnet-pick-server.winlink.org')).toBeInTheDocument();
  });

  it('falls back to default host/transport when config_read rejects', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') throw new Error('NotConfigured');
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    render(<TelnetRadioPanel onClose={() => {}} />);
    const hostInput = screen.getByTestId('telnet-host-input') as HTMLInputElement;
    expect(hostInput.value).toBe('cms.winlink.org'); // DEFAULT_HOST
  });

  it('reflects a non-default host from config_read', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { host: 'cms-z.winlink.org', transport: 'Telnet' };
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    render(<TelnetRadioPanel onClose={() => {}} />);
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
    (invoke as ReturnType<typeof vi.fn>).mockClear();
    render(<TelnetRadioPanel onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('telnet-host-input')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('telnet-pick-cms-z.winlink.org'));
    expect(invoke).toHaveBeenCalledWith('config_set_connect', {
      host: 'cms-z.winlink.org',
      transport: 'CmsSsl',
    });
  });

  it('editing the host and blurring persists via config_set_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockClear();
    render(<TelnetRadioPanel onClose={() => {}} />);
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
    (invoke as ReturnType<typeof vi.fn>).mockClear();
    render(<TelnetRadioPanel onClose={() => {}} />);
    await waitFor(() => expect(screen.getByTestId('telnet-transport-Telnet')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('telnet-transport-Telnet'));
    expect(invoke).toHaveBeenCalledWith('config_set_connect', {
      host: 'cms.winlink.org',
      transport: 'Telnet',
    });
  });

  it('renders the Session log section', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('renders backend log lines that arrive on session_log:line', async () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
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
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByRole('button', { name: /Start/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Stop/i })).toBeInTheDocument();
  });

  it('clicking Start fires cms_connect', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<TelnetRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: /Start/i }));
    expect(invoke).toHaveBeenCalledWith('cms_connect');
  });

  it('clicking Stop fires cms_abort', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    render(<TelnetRadioPanel onClose={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: /Stop/i }));
    expect(invoke).toHaveBeenCalledWith('cms_abort');
  });

  it('header sub shows host:port composed from transport', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { host: 'cms-z.winlink.org', transport: 'Telnet' };
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    render(<TelnetRadioPanel onClose={() => {}} />);
    await waitFor(() => {
      // Header sub renders host:port (Telnet → 8772)
      expect(screen.getByText('cms-z.winlink.org:8772')).toBeInTheDocument();
    });
  });
});
