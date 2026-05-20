import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, screen } from '@testing-library/react';

// --- Tauri IPC mocks --------------------------------------------------------
// App.tsx now also imports @tauri-apps/api/event (listen) and
// @tauri-apps/api/window (getCurrentWindow), and the real AppShell mounts the
// ribbon (config_read/backend_status) + session log (session_log_snapshot).
// Route invoke by command so each consumer gets a shape-correct value.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
let currentLabel = 'main';
const listenMock = vi.fn(
  async (_event?: unknown, _cb?: unknown): Promise<() => void> => () => {},
);
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: currentLabel,
    // Compose.tsx dynamically imports this to wire native-close handling.
    onCloseRequested: vi.fn(async () => () => {}),
    close: vi.fn(async () => {}),
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) =>
    (listenMock as (...a: unknown[]) => Promise<() => void>)(...args),
}));
// react-virtuoso renders nothing under jsdom; stub so MessageList mounts.
vi.mock('react-virtuoso', () => ({ Virtuoso: () => <div data-testid="virtuoso-mock" /> }));

import { invoke } from '@tauri-apps/api/core';

function routeInvoke(wizardCompleted: boolean) {
  (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
    if (cmd === 'get_wizard_completed') return Promise.resolve(wizardCompleted);
    if (cmd === 'mailbox_list') return Promise.resolve([]);
    if (cmd === 'config_read') return Promise.resolve(null);
    if (cmd === 'backend_status') return Promise.resolve(null);
    if (cmd === 'session_log_snapshot') return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
}

function setPath(pathname: string) {
  Object.defineProperty(window, 'location', {
    configurable: true,
    value: { pathname },
  });
}

import App from './App';

describe('<App> main-window routing', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentLabel = 'main';
    setPath('/');
  });

  it('renders wizard when wizard_completed=false', async () => {
    routeInvoke(false);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('wizard-root')).toBeInTheDocument());
  });

  it('renders main shell when wizard_completed=true', async () => {
    routeInvoke(true);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('app-shell-root')).toBeInTheDocument());
  });

  it('falls back to wizard when get_wizard_completed rejects', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'get_wizard_completed') return Promise.reject(new Error('no config'));
      return Promise.resolve([]);
    });
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('wizard-root')).toBeInTheDocument());
  });

  // Main window subscribes to the "menu" channel for the menu:file:new →
  // compose-window-open wiring (spec §4.3).
  it('main window subscribes to the menu channel', async () => {
    routeInvoke(false);
    render(<App />);
    await waitFor(() => expect(listenMock).toHaveBeenCalledWith('menu', expect.any(Function)));
  });

  // Clicking File→New Message opens a compose window via compose_window_open.
  it('a menu:file:new event opens a compose window', async () => {
    routeInvoke(false);
    let menuHandler: ((e: { payload: string }) => void) | undefined;
    listenMock.mockImplementation(async (_evt: unknown, cb: unknown) => {
      menuHandler = cb as (e: { payload: string }) => void;
      return () => {};
    });
    render(<App />);
    await waitFor(() => expect(menuHandler).toBeDefined());
    menuHandler!({ payload: 'menu:file:new' });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'compose_window_open',
        expect.objectContaining({ draftId: expect.any(String) }),
      ),
    );
  });
});

describe('<App> compose-window routing (spec §5.4 / Codex F7)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listenMock.mockImplementation(async () => () => {});
  });
  afterEach(() => setPath('/'));

  it('renders <Compose> for a /compose/<draftId> route', async () => {
    currentLabel = 'compose-draft-xyz';
    setPath('/compose/draft-xyz');
    routeInvoke(true);
    render(<App />);
    // Compose mounts; the wizard/shell do NOT.
    await waitFor(() => expect(screen.getByTestId('compose-root')).toBeInTheDocument());
    expect(screen.queryByTestId('wizard-root')).not.toBeInTheDocument();
    expect(screen.queryByTestId('app-shell-root')).not.toBeInTheDocument();
  });

  // Codex F7: a compose window must NOT listen for menu:file:new (the event
  // broadcasts to every webview), or it recursively spawns compose windows.
  it('compose window does NOT subscribe to the menu channel', async () => {
    currentLabel = 'compose-draft-xyz';
    setPath('/compose/draft-xyz');
    routeInvoke(true);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('compose-root')).toBeInTheDocument());
    // No subscription to the menu channel from a compose window.
    expect(listenMock).not.toHaveBeenCalledWith('menu', expect.any(Function));
  });
});
