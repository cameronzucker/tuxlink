import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, screen } from '@testing-library/react';

// --- Tauri IPC mocks --------------------------------------------------------
// App.tsx no longer subscribes to the "menu" channel (tuxlink-ng3 Task 10), but
// the trees it mounts still need these stubs: the real AppShell mounts the HTML
// chrome (getCurrentWindow), the ribbon (config_read/backend_status), and the
// session log (session_log_snapshot + listen); Compose uses onCloseRequested.
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

// tuxlink-k0q3 + tuxlink-01vd: Wizard and Compose are now React.lazy-loaded
// inside App.tsx. Vitest's dynamic-import path is slow on the Pi5 (the test
// previously raced the lazy resolve against waitFor's 1000ms default). Mock
// the modules so the lazy import returns synchronously — the test asserts
// routing, not Wizard internals.
vi.mock('./wizard/Wizard', () => ({
  Wizard: () => <div data-testid="wizard-root">[wizard mock]</div>,
}));
vi.mock('./compose/Compose', () => ({
  Compose: ({ draftId }: { draftId: string }) => (
    <div data-testid="compose-root" data-draft-id={draftId} />
  ),
}));

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

  // tuxlink-ng3 (Task 10): App no longer subscribes to the app-global "menu"
  // event channel. The File → New Message path now dispatches in-process inside
  // AppShell (see AppShell.test.tsx "File → New Message opens a compose
  // window"); the broadcast listener — the Codex F7 recursion source — is gone.
  it('main window does NOT subscribe to the menu channel', async () => {
    routeInvoke(true);
    render(<App />);
    await waitFor(() => expect(screen.getByTestId('app-shell-root')).toBeInTheDocument());
    expect(listenMock).not.toHaveBeenCalledWith('menu', expect.any(Function));
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

  // Codex F7 regression guard: a compose window must NOT subscribe to the
  // "menu" channel. The app-global broadcast (the recursion source) was removed
  // in tuxlink-ng3 Task 10 — App no longer subscribes from any window — so this
  // now holds trivially; the assertion stays as a guard against reintroducing
  // a per-window menu listener.
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
