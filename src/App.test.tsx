import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, screen } from '@testing-library/react';
// tuxlink-n4hz regression test imports useQueryClient at the top so the mock
// factory below references the SAME module instance App.tsx imports. A
// require() inside the factory would resolve via CJS and get a duplicate
// instance whose context lookup would fail even with the provider present.
import { useQueryClient } from '@tanstack/react-query';

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
// tuxlink-n4hz regression: the real HelpView calls useHelpSearch → useQuery,
// which throws "No QueryClient set" if App.tsx's QueryClientProvider doesn't
// wrap the help branch. Mock HelpView to a sentinel that USES the
// react-query context — the call to useQueryClient throws if the provider
// isn't above it, surfacing the regression as a test failure rather than as
// a production crash.
vi.mock('./help/HelpView', () => ({
  HelpView: () => {
    useQueryClient();   // throws if no QueryClientProvider above us
    return <div data-testid="help-root" />;
  },
}));
// tuxlink-qjgx / n4hz lesson applied to /logging: LoggingExportSection and
// LoggingSettingsSection both call useQuery, which throws "No QueryClient set"
// without a provider above them. Mock LoggingView to a sentinel that uses
// useQueryClient() — the call throws if App.tsx's isLoggingWindow branch ever
// loses its QueryClientProvider wrapper.
vi.mock('./help/LoggingView', () => ({
  LoggingView: () => {
    useQueryClient();   // throws if no QueryClientProvider above us
    return <div data-testid="logging-root" />;
  },
}));

// bd tuxlink-dmwte task 11 regression guard: the popped-surface webview
// (/pop/<surface>) crashed to a blank window because AprsChatPanel's
// useFirstOpenTip('aprs') hard-throws when no HintProvider is above it, and
// App.tsx's /pop route branch had none (the fix wraps it in <HintProvider>).
// The real SURFACE_REGISTRY entries pull in leaflet/protomaps/live APRS +
// routines hooks (already exercised by PoppedSurfaceHost.test.tsx and
// AprsPositionsMap.test.tsx) — mock the registry the same way that file does,
// but keep ONE real hook call in the aprs_chat sentinel: useFirstOpenTip
// itself. That reproduces the exact crash mechanism from the WebKitGTK smoke
// (useHints() throws "must be used inside <HintProvider>") without mounting
// the rest of AprsChatPanel.
vi.mock('./dock/surfaceRegistry', () => ({
  SURFACE_REGISTRY: {
    routines: {
      id: 'routines',
      title: 'Routines — Tuxlink',
      Component: () => <div data-testid="pop-routines-surface-mock" />,
      StatusStrip: () => <div data-testid="pop-routines-strip-mock" />,
    },
    tac_map: {
      id: 'tac_map',
      title: 'Tac Map — Tuxlink',
      Component: () => <div data-testid="pop-tacmap-surface-mock" />,
      StatusStrip: () => <div data-testid="pop-tacmap-strip-mock" />,
    },
    aprs_chat: {
      id: 'aprs_chat',
      title: 'APRS Chat — Tuxlink',
      Component: () => {
        useFirstOpenTip('aprs'); // the real regression trigger — see comment above
        return <div data-testid="pop-aprschat-surface-mock" />;
      },
      StatusStrip: () => <div data-testid="pop-aprschat-strip-mock" />,
    },
  },
}));
// PoppedSurfaceHost mounts ConsentGate directly (not via the registry) for
// the routines surface only; stub it the same way PoppedSurfaceHost.test.tsx
// does — its own (heavy, routines-API-backed) internals are out of scope here.
vi.mock('./routines/ConsentGate', () => ({
  ConsentGate: () => <div data-testid="consent-gate-mock" />,
}));

import { invoke } from '@tauri-apps/api/core';
import { useFirstOpenTip } from './onboarding/HintProvider';

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

// tuxlink-n4hz: regression for the post-merge crash in PR #312. When the help
// webview mounted, HelpView's useHelpSearch hook called useQuery without a
// QueryClientProvider above it and React rendered nothing. Lifting
// QueryClientProvider up to wrap all routing branches in App.tsx is the fix;
// this test asserts the provider IS above HelpView (the mock above throws if
// useQueryClient fails).
describe('<App> help-window routing (tuxlink-0gsy / tuxlink-n4hz)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listenMock.mockImplementation(async () => () => {});
  });
  afterEach(() => setPath('/'));

  it('renders <HelpView> with QueryClientProvider above it for /help', async () => {
    currentLabel = 'help';
    setPath('/help');
    routeInvoke(true);
    render(<App />);
    // HelpView mounts (which itself requires the QueryClient context per the
    // sentinel mock above); main-window trees do NOT.
    await waitFor(() => expect(screen.getByTestId('help-root')).toBeInTheDocument());
    expect(screen.queryByTestId('wizard-root')).not.toBeInTheDocument();
    expect(screen.queryByTestId('app-shell-root')).not.toBeInTheDocument();
    expect(screen.queryByTestId('compose-root')).not.toBeInTheDocument();
  });
});

// tuxlink-qjgx / n4hz lesson: the /logging route (isLoggingWindow branch in
// App.tsx) must have QueryClientProvider above LoggingView because
// LoggingExportSection + LoggingSettingsSection both call useQuery. This test
// enforces the structural constraint: the LoggingView mock uses useQueryClient()
// which throws if no provider is above it.
describe('<App> logging-window routing (tuxlink-qjgx)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listenMock.mockImplementation(async () => () => {});
  });
  afterEach(() => setPath('/'));

  it('renders <LoggingView> with QueryClientProvider above it for /logging', async () => {
    currentLabel = 'logging';
    setPath('/logging');
    routeInvoke(true);
    render(<App />);
    // LoggingView mounts (which itself requires the QueryClient context per the
    // sentinel mock above); main-window trees do NOT.
    await waitFor(() => expect(screen.getByTestId('logging-root')).toBeInTheDocument());
    expect(screen.queryByTestId('wizard-root')).not.toBeInTheDocument();
    expect(screen.queryByTestId('app-shell-root')).not.toBeInTheDocument();
    expect(screen.queryByTestId('compose-root')).not.toBeInTheDocument();
    expect(screen.queryByTestId('help-root')).not.toBeInTheDocument();
  });
});

// tuxlink-h7q7 (Codex adrev R1 #13): a SMOKE test only — proves the production
// <App/> tree (QueryClientProvider wrapping AppShell — the n4hz mount-path
// lesson) mounts in compact mode without crashing. It is NOT a layout guard;
// the shell's compact invariants are owned by the CSS-string tests + the
// mandatory Playwright pass.
import { COMPACT_MEDIA_QUERY } from './shell/useViewport';

describe('<App> compact mount smoke (tuxlink-h7q7)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentLabel = 'main';
    setPath('/');
    vi.stubGlobal('matchMedia', (q: string) => ({
      matches: q === COMPACT_MEDIA_QUERY,
      media: q,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
  });
  afterEach(() => vi.unstubAllGlobals());

  it('mounts the production shell in compact mode and applies the .compact root class', async () => {
    routeInvoke(true);
    render(<App />);
    const root = await screen.findByTestId('app-shell-root');
    expect(root.classList.contains('compact')).toBe(true);
  });
});

// bd tuxlink-dmwte task 11: regression guard for the fix that wraps App.tsx's
// /pop/<surface> branch in <HintProvider>. Before this fix, popping APRS Chat
// out into its own webview rendered a blank window — AprsChatPanel's
// useFirstOpenTip('aprs') call throws "useHints must be used inside
// <HintProvider>" with no provider above it, and the /pop branch had none.
// There was no automated coverage of the /pop route at all, so a future
// change that drops the wrap would ship silently. Asserts each of the three
// dockable surfaces' pop-out route renders <PoppedSurfaceHost>'s chrome (the
// ⇤ dock-back button — PoppedSurfaceHost.test.tsx's accessible name) without
// throwing.
describe('<App> popped-surface routing (bd tuxlink-dmwte task 11)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listenMock.mockImplementation(async () => () => {});
    routeInvoke(true);
    // PoppedSurfaceHost reads `dock_state_get` at mount (unwraps the
    // continuity-token envelope); routeInvoke's default undefined fallback
    // would throw inside PoppedSurfaceHost's .then() on `snap.context[...]`.
    const base = (invoke as ReturnType<typeof vi.fn>).getMockImplementation()!;
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'dock_state_get') {
        return Promise.resolve({
          surfaces: { routines: 'popped', tac_map: 'popped', aprs_chat: 'popped' },
          context: { routines: null, tac_map: null, aprs_chat: null },
        });
      }
      return base(cmd);
    });
  });
  afterEach(() => setPath('/'));

  it('renders <PoppedSurfaceHost> for /pop/routines without throwing', async () => {
    currentLabel = 'pop-routines';
    setPath('/pop/routines');
    render(<App />);
    // The surface Component only mounts once `dock_state_get` resolves
    // (contextLoaded) — wait on it rather than the title-bar chrome (which
    // paints a render tick earlier and would race the assertion below).
    await waitFor(() =>
      expect(screen.getByTestId('pop-routines-surface-mock')).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: /dock back into main window/i })).toBeInTheDocument();
    expect(screen.queryByTestId('error-boundary-fallback')).not.toBeInTheDocument();
  });

  it('renders <PoppedSurfaceHost> for /pop/tacmap without throwing', async () => {
    currentLabel = 'pop-tacmap';
    setPath('/pop/tacmap');
    render(<App />);
    await waitFor(() =>
      expect(screen.getByTestId('pop-tacmap-surface-mock')).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: /dock back into main window/i })).toBeInTheDocument();
    expect(screen.queryByTestId('error-boundary-fallback')).not.toBeInTheDocument();
  });

  // The concrete regression: only the aprs_chat surface calls
  // useFirstOpenTip, so this is the case that actually caught the missing
  // HintProvider in production.
  it('renders <PoppedSurfaceHost> for /pop/aprschat without throwing (the WebKitGTK-smoke regression)', async () => {
    currentLabel = 'pop-aprschat';
    setPath('/pop/aprschat');
    render(<App />);
    await waitFor(() =>
      expect(screen.getByTestId('pop-aprschat-surface-mock')).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: /dock back into main window/i })).toBeInTheDocument();
    expect(screen.queryByTestId('error-boundary-fallback')).not.toBeInTheDocument();
  });
});
