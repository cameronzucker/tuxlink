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

describe('<TelnetRadioPanel>', () => {
  beforeEach(() => {
    lastSessionLogHandler = null;
  });

  it('renders the Telnet Winlink panel with endpoint and transport from config_read', async () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('Telnet Winlink');
    // `config_read` resolves async; once it lands the Endpoint field shows
    // the configured host + the port the transport implies (8773 for SSL).
    await waitFor(() => {
      expect(screen.getByText(/cms\.winlink\.org:8773/)).toBeInTheDocument();
    });
    expect(screen.getByText(/CMS-SSL/)).toBeInTheDocument();
  });

  it('falls back to default host/transport when config_read rejects (pre-wizard)', async () => {
    // Persistent override (not Once) — both config_read and session_log_snapshot
    // fire on mount; mockImplementationOnce would only catch whichever happens
    // first and let the other fall through to the global mock.
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') throw new Error('NotConfigured');
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    render(<TelnetRadioPanel onClose={() => {}} />);
    // Defaults render synchronously; the rejected config_read just leaves
    // the fallback values in place.
    expect(screen.getByText(/cms\.winlink\.org:8773/)).toBeInTheDocument();
    expect(screen.getByText(/CMS-SSL/)).toBeInTheDocument();
  });

  it('reflects a non-default host from config_read', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { host: 'cms-z.winlink.org', transport: 'CmsSsl' };
      if (cmd === 'session_log_snapshot') return [];
      return undefined;
    });
    render(<TelnetRadioPanel onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByText(/cms-z\.winlink\.org:8773/)).toBeInTheDocument();
    });
  });

  it('renders the Session log section', () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    expect(screen.getByTestId('session-log-section')).toBeInTheDocument();
  });

  it('renders backend log lines that arrive on session_log:line', async () => {
    render(<TelnetRadioPanel onClose={() => {}} />);
    // Wait for the listen() mount effect to register the handler.
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
});
