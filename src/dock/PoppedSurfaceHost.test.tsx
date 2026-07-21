// src/dock/PoppedSurfaceHost.test.tsx — bd tuxlink-dmwte task 7.
//
// Tests the HOST's own chrome/lifecycle logic (title bar, dock-back/close
// envelopes, close-intent filtering, Ctrl+W, theme storage listener) against
// a FAKE surface registry — the real registry entries (tac_map/aprs_chat)
// mount AprsPositionsMap's full hook graph (react-query, leaflet, protomaps),
// which is exercised by AprsPositionsMap.test.tsx / the wire-walk gate /
// typecheck, not re-mounted here. This keeps PoppedSurfaceHost's own tests
// fast and focused on what Task 7 actually owns: the shell, not the surfaces.
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import type { ReactElement } from 'react';
import type { SurfaceComponentProps } from './surfaceRegistry';

// ---- Tauri mocks (routinesApi.test.ts's per-file vi.mock pattern; teardown
// pitfall — invoke mocks get called with NO args at cleanup, so the
// implementation below must tolerate `cmd === undefined`). ----
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const listenMock = vi.hoisted(() => vi.fn());
const listenHandlers = vi.hoisted(
  () => new Map<string, (e: { payload: unknown }) => void>(),
);
vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, cb: (e: { payload: unknown }) => void) => listenMock(event, cb),
}));

const win = vi.hoisted(() => ({
  minimize: vi.fn(async () => {}),
  toggleMaximize: vi.fn(async () => {}),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => win }));

// Fake the registry (see file-header note) — one lightweight Component +
// StatusStrip per surface, with the real §3 wire-table titles so the title
// bar assertions below match production copy.
let lastRegisterGetContext: ((fn: () => unknown | null) => void) | null = null;
let lastComponentContext: unknown = undefined;
vi.mock('./surfaceRegistry', () => {
  function makeComponent(id: string) {
    return function FakeComponent({ context, registerGetContext }: SurfaceComponentProps) {
      lastRegisterGetContext = registerGetContext;
      lastComponentContext = context;
      return <div data-testid={`surface-${id}`}>surface:{id}</div>;
    };
  }
  function makeStrip(id: string) {
    return function FakeStrip() {
      return <div data-testid={`strip-${id}`}>strip:{id}</div>;
    };
  }
  return {
    SURFACE_REGISTRY: {
      routines: { id: 'routines', title: 'Routines - Tuxlink', Component: makeComponent('routines'), StatusStrip: makeStrip('routines') },
      tac_map: { id: 'tac_map', title: 'Tac Map - Tuxlink', Component: makeComponent('tac_map'), StatusStrip: makeStrip('tac_map') },
      aprs_chat: { id: 'aprs_chat', title: 'APRS Chat - Tuxlink', Component: makeComponent('aprs_chat'), StatusStrip: makeStrip('aprs_chat') },
      // bd tuxlink-9obx2: the fourth surface, same fake-registry treatment.
      station_intelligence: {
        id: 'station_intelligence',
        title: 'Station Intelligence - Tuxlink',
        Component: makeComponent('station_intelligence'),
        StatusStrip: makeStrip('station_intelligence'),
      },
    },
  };
});

// ConsentGate (routines-only mount, behavior #6) pulls in the real routines
// API surface; stub it so this file stays scoped to PoppedSurfaceHost's own
// chrome, not ConsentGate's (separately-tested) internals.
vi.mock('../routines/ConsentGate', () => ({
  ConsentGate: () => <div data-testid="consent-gate-stub" />,
}));

import { PoppedSurfaceHost } from './PoppedSurfaceHost';
import {
  COLOR_SCHEME_STORAGE_KEY,
  CUSTOM_THEME_STORAGE_KEY,
} from '../shell/colorScheme';

function emptySnapshot() {
  return {
    surfaces: {
      routines: 'popped', tac_map: 'popped', aprs_chat: 'popped', elmer: 'popped',
      station_intelligence: 'popped',
    },
    context: {
      routines: null, tac_map: null, aprs_chat: null, elmer: null,
      station_intelligence: null,
    },
  };
}

function renderHost(el: ReactElement) {
  return render(el);
}

beforeEach(() => {
  lastRegisterGetContext = null;
  lastComponentContext = undefined;
  invokeMock.mockReset();
  // Teardown pitfall: invoke mocks get called with NO args — always resolve.
  invokeMock.mockImplementation((cmd?: string) =>
    cmd === 'dock_state_get' ? Promise.resolve(emptySnapshot()) : Promise.resolve());
  listenMock.mockReset();
  listenHandlers.clear();
  listenMock.mockImplementation((event: string, cb: (e: { payload: unknown }) => void) => {
    listenHandlers.set(event, cb);
    return Promise.resolve(() => {
      listenHandlers.delete(event);
    });
  });
  win.minimize.mockClear();
  win.toggleMaximize.mockClear();
  localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
  document.documentElement.removeAttribute('style');
});

afterEach(() => {
  cleanup();
});

describe('PoppedSurfaceHost — title bar (behavior 1)', () => {
  it('renders title bar with dock-back, min, max, close — all labeled buttons (spec §4)', () => {
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    expect(screen.getByRole('button', { name: /dock back into main window/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^minimize$/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^maximize$/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^close$/i })).toBeInTheDocument();
    expect(screen.getByText('Tac Map - Tuxlink')).toBeInTheDocument();
  });

  it('mounts the 8 edge/corner resize handles — borderless pop windows have no native grips (tuxlink-dwcqx)', () => {
    const { container } = renderHost(<PoppedSurfaceHost surface="tac_map" />);
    expect(container.querySelectorAll('.tux-resize').length).toBe(8);
  });

  it('minimize/maximize call the window API directly, never through dockBack', () => {
    renderHost(<PoppedSurfaceHost surface="aprs_chat" />);
    fireEvent.click(screen.getByRole('button', { name: /^minimize$/i }));
    fireEvent.click(screen.getByRole('button', { name: /^maximize$/i }));
    expect(win.minimize).toHaveBeenCalledOnce();
    expect(win.toggleMaximize).toHaveBeenCalledOnce();
    expect(invokeMock).not.toHaveBeenCalledWith('surface_dock_back', expect.anything());
  });

  it('⇤ dock-back invokes surface_dock_back with {foreground:true} and the collected state', async () => {
    renderHost(<PoppedSurfaceHost surface="routines" />);
    // The (fake) surface registers its state-collector once the surface
    // component mounts, which is gated on the async dock_state_get() context
    // fetch settling first.
    await waitFor(() => expect(lastRegisterGetContext).not.toBeNull());
    lastRegisterGetContext!(() => ({ view: 'designer', routine: 'x', tab: 'design' }));
    fireEvent.click(screen.getByRole('button', { name: /dock back into main window/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('surface_dock_back', {
        surface: 'routines',
        context: { foreground: true, state: { view: 'designer', routine: 'x', tab: 'design' } },
      }));
  });
});

describe('PoppedSurfaceHost — ✕ / Ctrl+W (behavior 1 + 3)', () => {
  it('✕ and Ctrl+W invoke surface_dock_back with the {foreground:false} envelope, never window.close (spec §4)', async () => {
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    fireEvent.keyDown(window, { key: 'w', ctrlKey: true });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('surface_dock_back',
        expect.objectContaining({ surface: 'tac_map', context: expect.objectContaining({ foreground: false }) })));
  });

  it('the ✕ button drives the same envelope as Ctrl+W', async () => {
    renderHost(<PoppedSurfaceHost surface="aprs_chat" />);
    fireEvent.click(screen.getByRole('button', { name: /^close$/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('surface_dock_back',
        expect.objectContaining({ surface: 'aprs_chat', context: expect.objectContaining({ foreground: false }) })));
  });

  it('Ctrl+W preventDefaults so the browser default (nothing, but belt-and-braces) never fires', () => {
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    const event = new KeyboardEvent('keydown', { key: 'w', ctrlKey: true, cancelable: true });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it('Ctrl+W with a capital "W" (CapsLock) still drives the same envelope as lowercase Ctrl+W', async () => {
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    fireEvent.keyDown(window, { key: 'W', ctrlKey: true });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('surface_dock_back',
        expect.objectContaining({ surface: 'tac_map', context: expect.objectContaining({ foreground: false }) })));
  });
});

describe('PoppedSurfaceHost — close-intent (behavior 2)', () => {
  it('runs the ✕ path when dock:close-intent names THIS surface', async () => {
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    await waitFor(() => expect(listenHandlers.has('dock:close-intent')).toBe(true));
    listenHandlers.get('dock:close-intent')!({ payload: { surface: 'tac_map' } });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('surface_dock_back',
        expect.objectContaining({ surface: 'tac_map', context: expect.objectContaining({ foreground: false }) })));
  });

  it('ignores dock:close-intent naming a DIFFERENT surface (belt-and-braces)', async () => {
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    await waitFor(() => expect(listenHandlers.has('dock:close-intent')).toBe(true));
    listenHandlers.get('dock:close-intent')!({ payload: { surface: 'aprs_chat' } });
    // Give any (wrongly-fired) async dockBack a tick to land, then assert it didn't.
    await new Promise((r) => setTimeout(r, 0));
    expect(invokeMock).not.toHaveBeenCalledWith('surface_dock_back', expect.anything());
  });
});

describe('PoppedSurfaceHost — dock-back rejection (behavior 2, review-loop-3 F2)', () => {
  it('logs console.error and keeps the window rendered when surface_dock_back rejects (✕ path)', async () => {
    const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    invokeMock.mockImplementation((cmd?: string) => {
      if (cmd === 'dock_state_get') return Promise.resolve(emptySnapshot());
      if (cmd === 'surface_dock_back') return Promise.reject(new Error('backend unavailable'));
      return Promise.resolve();
    });
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    fireEvent.click(screen.getByRole('button', { name: /^close$/i }));

    await waitFor(() =>
      expect(consoleErrorSpy).toHaveBeenCalledWith(
        expect.stringContaining('[dock] dock-back failed for tac_map'),
        expect.any(Error),
      ));
    // No crash — the host is still mounted and rendering.
    expect(screen.getByTestId('pop-surface-host-tac_map')).toBeInTheDocument();
    consoleErrorSpy.mockRestore();
  });

  it('logs console.error when surface_dock_back rejects via the ⇤ dock-back path', async () => {
    const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    invokeMock.mockImplementation((cmd?: string) => {
      if (cmd === 'dock_state_get') return Promise.resolve(emptySnapshot());
      if (cmd === 'surface_dock_back') return Promise.reject(new Error('backend unavailable'));
      return Promise.resolve();
    });
    renderHost(<PoppedSurfaceHost surface="routines" />);
    fireEvent.click(screen.getByRole('button', { name: /dock back into main window/i }));

    await waitFor(() =>
      expect(consoleErrorSpy).toHaveBeenCalledWith(
        expect.stringContaining('[dock] dock-back failed for routines'),
        expect.any(Error),
      ));
    expect(screen.getByTestId('pop-surface-host-routines')).toBeInTheDocument();
    consoleErrorSpy.mockRestore();
  });
});

describe('PoppedSurfaceHost — theme (behavior 4)', () => {
  it('applies the stored scheme on mount', () => {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'github-dark');
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    expect(document.documentElement.dataset.theme).toBe('github-dark');
  });

  it('the mount apply passes broadcast:false — a popped window is a listener, never an originator (review-loop-3 F5)', async () => {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'github-dark');
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    // Give the dynamic import inside applyColorScheme a tick to settle, so
    // a wrongly-broadcasting mount apply would have had the chance to fire.
    await new Promise((r) => setTimeout(r, 0));
    expect(invokeMock).not.toHaveBeenCalledWith('theme_broadcast_scheme', expect.anything());
  });

  it('storage event on tuxlink.colorScheme re-applies the scheme', () => {
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'night-red');
    window.dispatchEvent(new StorageEvent('storage', { key: COLOR_SCHEME_STORAGE_KEY, newValue: 'night-red' }));
    expect(document.documentElement.dataset.theme).toBe('night-red');
  });

  it('storage event on tuxlink.customTheme re-applies the injected tokens (adrev R5-F9)', () => {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'custom');
    const theme = {
      name: 'Ops', mode: 'dark', tokens: {
        bg: '#010203', surface: '#020304', 'surface-2': '#030405', elevated: '#040506',
        border: '#050607', 'border-strong': '#060708', 'border-soft': '#070809',
        text: '#f0f0f0', 'text-dim': '#c0c0c0', 'text-faint': '#a0a0a0',
        accent: '#ff0000', 'accent-2': '#ff1111', 'unread-dot': '#ff2222',
        success: '#00ff00', error: '#ff0000', info: '#0000ff', 'form-tag': '#ff00ff',
        'modem-accent': '#00ffaa', 'modem-accent-2': '#00ffbb', 'modem-accent-soft': 'rgba(0,255,170,0.1)',
        'modem-accent-fg': '#000000', 'tux-accent-fg': '#000000', 'tux-danger-fg': '#000000',
      },
    };
    localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, JSON.stringify(theme));
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    expect(document.documentElement.style.getPropertyValue('--bg')).toBe('#010203');

    // Edit the custom theme (scheme key unchanged, still 'custom') and fire a
    // storage event on the CUSTOM_THEME key only.
    const edited = { ...theme, tokens: { ...theme.tokens, bg: '#111213' } };
    localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, JSON.stringify(edited));
    window.dispatchEvent(new StorageEvent('storage', { key: CUSTOM_THEME_STORAGE_KEY, newValue: JSON.stringify(edited) }));
    expect(document.documentElement.style.getPropertyValue('--bg')).toBe('#111213');
  });

  it('ignores an unrelated storage key', () => {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'default');
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    localStorage.setItem('some.other.key', 'night-red');
    window.dispatchEvent(new StorageEvent('storage', { key: 'some.other.key', newValue: 'night-red' }));
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});

describe('PoppedSurfaceHost — strips + consent (behaviors 5 + 6)', () => {
  it('renders the surface-specific status strip', () => {
    renderHost(<PoppedSurfaceHost surface="aprs_chat" />);
    expect(screen.getByTestId('strip-aprs_chat')).toBeInTheDocument();
  });

  it('mounts ConsentGate for routines only', () => {
    renderHost(<PoppedSurfaceHost surface="routines" />);
    expect(screen.getByTestId('consent-gate-stub')).toBeInTheDocument();
  });

  it('does not mount ConsentGate for tac_map / aprs_chat / station_intelligence', () => {
    renderHost(<PoppedSurfaceHost surface="tac_map" />);
    expect(screen.queryByTestId('consent-gate-stub')).not.toBeInTheDocument();
    cleanup();
    renderHost(<PoppedSurfaceHost surface="station_intelligence" />);
    expect(screen.queryByTestId('consent-gate-stub')).not.toBeInTheDocument();
  });

  // bd tuxlink-9obx2: the fifth surface mounts through the SAME generic
  // registry path as the others; no PoppedSurfaceHost.tsx code change was
  // needed to add it (registry wiring only). The surface Component mounts
  // only after `contextLoaded` flips (post the mocked `dock_state_get`
  // microtask), so this must await rather than assert synchronously.
  it('renders the station_intelligence surface component and its status strip', async () => {
    renderHost(<PoppedSurfaceHost surface="station_intelligence" />);
    expect(await screen.findByTestId('surface-station_intelligence')).toBeInTheDocument();
    expect(screen.getByTestId('strip-station_intelligence')).toBeInTheDocument();
  });

  it('unwraps the token envelope and passes its state half to the surface component', async () => {
    // tuxlink-dmwte task 8 (seam note 1): the registry stores the full envelope
    // `{ foreground, state }`; the host UNWRAPS `.state` so the component gets
    // the bare token state (matching SurfaceComponentProps.context).
    invokeMock.mockImplementation((cmd?: string) =>
      cmd === 'dock_state_get'
        ? Promise.resolve({
            surfaces: { routines: 'popped', tac_map: 'docked', aprs_chat: 'docked' },
            context: {
              routines: { foreground: true, state: { view: { view: 'dashboard' } } },
              tac_map: null,
              aprs_chat: null,
            },
          })
        : Promise.resolve());
    renderHost(<PoppedSurfaceHost surface="routines" />);
    await waitFor(() => expect(screen.getByTestId('surface-routines')).toBeInTheDocument());
    expect(lastComponentContext).toEqual({ view: { view: 'dashboard' } });
  });
});

// tuxlink-y6whc: the centered .pop-title (flex:1, z-index:1) sat ABOVE the
// .tux-drag inset:0 drag region and blanketed every draggable pixel — popped
// windows could not be moved by their title bar. The title is inert text;
// pointer-events:none lets mousedowns fall through to the drag region.
// CSS-source assertion (the jsdom render can't exercise wry's drag hit-test).
const HOST_CSS_MODULES = import.meta.glob('./PoppedSurfaceHost.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

describe('pop title bar drag region (tuxlink-y6whc)', () => {
  it('.pop-title is pointer-transparent so the drag region beneath receives mousedown', () => {
    const css = HOST_CSS_MODULES['./PoppedSurfaceHost.css'];
    const block = css.match(/\.pop-titlebar \.pop-title \{[^}]*\}/)?.[0] ?? '';
    expect(block).toContain('pointer-events: none');
  });
});
