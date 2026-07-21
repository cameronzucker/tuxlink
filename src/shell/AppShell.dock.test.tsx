// AppShell ⇄ dock-registry wiring (tuxlink-dmwte task 8, spec §5/§6).
// The dock-state module is mocked so this suite controls the snapshot the
// shell sees: Routines docked vs popped, and a popped→docked foreground
// arrival. Mock scaffold copied from AppShell.routines.test.tsx (that file
// knows which backends a real AppShell mount needs).
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, within, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useState, useEffect, type ReactNode } from 'react';
import type { MessageMeta } from '../mailbox/types';
import type { DockSnapshot, DockMode } from '../dock/dockState';

// --- dock-state module mock ------------------------------------------------
// `useDockState` returns the mutable `dockRef.current`; the transition test
// reassigns it to a fresh object and `rerender`s so the shell's `dock:changed`
// effect (keyed on the snapshot) fires. Spies + the mutable ref go through
// `vi.hoisted` since the `vi.mock` factory is hoisted above module top-level.
//
// `dockListeners` backs a SECOND way to push a dock-state transition: see
// `pushDockSnapshot` below. Most of this file's arrival tests mutate
// `dockRef.current` and then force a full-tree `rerender(<AppShell />)`,
// which re-executes every ancestor in the tree, INCLUDING `<HintProvider>`
// (AppShell.tsx wraps `AppShellInner` in it). `HintProvider`'s context
// `value` is a fresh, unmemoized object literal every render, so a full-tree
// rerender always hands `AppShellInner` a new `hints` reference regardless
// of the dock transition. That masks any bug in a `useMemo`/`useCallback`
// dependency array that omits a dock-derived value (like `stationIntelPopped`
// below): the memo recomputes anyway, for the unrelated reason that `hints`
// changed too, and a missing dependency test would false-negative. A real
// `dock:changed` event in production re-renders ONLY `AppShellInner` (it owns
// the `useDockState()` call), never its `HintProvider` ancestor, so
// `pushDockSnapshot` reproduces that: it notifies the reactive `useDockState`
// mock's subscribers directly, inside `act()`, without touching RTL's
// `rerender` or the ancestor tree.
const { mockFocusSurface, mockPopOut, mockDockBack, dockRef, dockListeners } = vi.hoisted(() => ({
  mockFocusSurface: vi.fn(async () => {}),
  mockPopOut: vi.fn(async () => {}),
  mockDockBack: vi.fn(async () => {}),
  dockRef: { current: null as DockSnapshot | null },
  dockListeners: new Set<() => void>(),
}));

/** Push a new dock snapshot the way a real `dock:changed` event would land:
 *  ONLY the component(s) subscribed via `useDockState()` re-render, not the
 *  whole tree. See the mock's header comment above for why this differs from
 *  the `dockRef.current = X; rerender(<AppShell />)` pattern used elsewhere
 *  in this file. */
function pushDockSnapshot(snap: DockSnapshot) {
  act(() => {
    dockRef.current = snap;
    dockListeners.forEach((listener) => listener());
  });
}

function snapshot(routines: DockMode, context: unknown = null): DockSnapshot {
  return {
    surfaces: {
      routines,
      tac_map: 'docked',
      aprs_chat: 'docked',
      elmer: 'docked',
      station_intelligence: 'docked',
    },
    context: {
      routines: context,
      tac_map: null,
      aprs_chat: null,
      elmer: null,
      station_intelligence: null,
    },
  };
}

// tuxlink-dmwte task 9: same shape as `snapshot()` above, but flips tac_map's
// mode/context instead of routines' — routines stays docked/null throughout
// (task 9 does not touch it).
function tacMapSnapshot(tac_map: DockMode, context: unknown = null): DockSnapshot {
  return {
    surfaces: {
      routines: 'docked',
      tac_map,
      aprs_chat: 'docked',
      elmer: 'docked',
      station_intelligence: 'docked',
    },
    context: {
      routines: null,
      tac_map: context,
      aprs_chat: null,
      elmer: null,
      station_intelligence: null,
    },
  };
}

// tuxlink-dmwte task 10: flips aprs_chat's mode/context; routines + tac_map
// stay docked/null throughout (task 10 does not touch them).
function aprsChatSnapshot(aprs_chat: DockMode, context: unknown = null): DockSnapshot {
  return {
    surfaces: {
      routines: 'docked',
      tac_map: 'docked',
      aprs_chat,
      elmer: 'docked',
      station_intelligence: 'docked',
    },
    context: {
      routines: null,
      tac_map: null,
      aprs_chat: context,
      elmer: null,
      station_intelligence: null,
    },
  };
}

// bd tuxlink-mfssz: flips elmer's mode/context; the other surfaces stay
// docked/null throughout.
function elmerSnapshot(elmer: DockMode, context: unknown = null): DockSnapshot {
  return {
    surfaces: {
      routines: 'docked',
      tac_map: 'docked',
      aprs_chat: 'docked',
      elmer,
      station_intelligence: 'docked',
    },
    context: {
      routines: null,
      tac_map: null,
      aprs_chat: null,
      elmer: context,
      station_intelligence: null,
    },
  };
}

// bd tuxlink-9obx2: flips station_intelligence's mode/context; the other
// surfaces stay docked/null throughout (this surface's wiring does not
// touch them).
function stationIntelSnapshot(station_intelligence: DockMode, context: unknown = null): DockSnapshot {
  return {
    surfaces: {
      routines: 'docked',
      tac_map: 'docked',
      aprs_chat: 'docked',
      elmer: 'docked',
      station_intelligence,
    },
    context: {
      routines: null,
      tac_map: null,
      aprs_chat: null,
      elmer: null,
      station_intelligence: context,
    },
  };
}

vi.mock('../dock/dockState', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../dock/dockState')>();
  return {
    ...actual, // keep the real consentHostWindow + SURFACE_WINDOW_LABEL
    // Reactive: subscribes to `dockListeners` so `pushDockSnapshot` (above)
    // can trigger a re-render of ONLY the calling component (AppShellInner),
    // matching how the real `dock:changed` listener drives `useDockState`.
    // Still returns the plain `dockRef.current` value at every render, so
    // every OTHER test in this file that sets `dockRef.current` directly
    // (at initial mount, or via a full-tree `rerender`) is unaffected.
    useDockState: () => {
      const [, forceRender] = useState(0);
      useEffect(() => {
        const listener = () => forceRender((n) => n + 1);
        dockListeners.add(listener);
        return () => {
          dockListeners.delete(listener);
        };
      }, []);
      return dockRef.current;
    },
    focusSurface: mockFocusSurface,
    popOut: mockPopOut,
    dockBack: mockDockBack,
  };
});

const CHAT_INBOX_MSG: MessageMeta = {
  id: 'INBOX1',
  subject: 'Inbox subject',
  from: 'KK4XYZ@winlink.org',
  to: [],
  date: '2026-05-19T14:00:00Z',
  unread: true,
  bodySize: 100,
  hasAttachments: false,
};

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd?: string) => {
    if (cmd === undefined) return undefined;
    if (cmd === 'routines_runs_list') return [];
    if (cmd === 'config_read') return null;
    if (cmd === 'backend_status') return null;
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'modem_get_status') {
      return {
        state: 'stopped', peer: null, mode: null, widthHz: null, pttBackend: null,
        snDb: null, vuDbfs: null, throughputBps: null,
        bytesRx: 0, bytesTx: 0, uptimeSec: 0,
        arqFlags: { busy: false, rx: false, tx: false }, lastError: null,
      };
    }
    if (cmd === 'packet_config_get') {
      return {
        ssid: 7, listenDefault: true, linkKind: null, btMac: null, tcpHost: null,
        tcpPort: null, serialDevice: null, serialBaud: null, txdelay: 30,
        persistence: 63, slotTime: 10, paclen: 128, maxframe: 4, t1Ms: 3000, n2Retries: 10,
      };
    }
    if (cmd === 'position_status') return { gps_ready: false, broadcast_grid: '', ui_grid: '' };
    if (cmd === 'tauri_search_list_saved') return [];
    if (cmd === 'tauri_search_list_recent') return [];
    if (cmd === 'contacts_read') return [];
    if (cmd === 'contacts_suggestions') return [];
    if (cmd === 'aprs_config_get') return { listenDefault: false };
    if (cmd === 'mailbox_list') return [CHAT_INBOX_MSG];
    if (cmd === 'routines_validate_draft') return [];
    if (cmd === 'routines_actions_list') return [];
    // bd tuxlink-mfssz: the Elmer drawer mounts in the arrival/pop-out cases.
    if (cmd === 'elmer_config_read') {
      return {
        agentEndpoint: 'http://localhost:11434',
        agentModel: 'test-model',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
    }
    if (cmd === 'egress_status') {
      return { armed: false, armedRemainingSecs: 0, tainted: false };
    }
    return undefined;
  }),
}));

vi.mock('react-virtuoso', () => ({
  Virtuoso: ({ data, itemContent }: { data: MessageMeta[]; itemContent: (i: number, m: MessageMeta) => unknown }) => (
    <div data-testid="virtuoso-mock">
      {data.map((m, i) => (<div key={m.id}>{itemContent(i, m) as ReactNode}</div>))}
    </div>
  ),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTitle: vi.fn(async () => {}) }),
}));

vi.mock('../map/basemapLeaflet', async () => {
  const L = (await import('leaflet')).default;
  return {
    buildBaseLayers: () => [L.layerGroup()],
    OSM_ATTRIBUTION: '© OpenStreetMap contributors',
    flavorBackground: () => '#34373d',
  };
});

import { AppShell } from './AppShell';
import { emit } from '@tauri-apps/api/event';

function renderShell() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

function clickMenu(top: string, item: RegExp) {
  const menubar = screen.getByRole('menubar');
  fireEvent.click(within(menubar).getByRole('button', { name: top }));
  const matches = within(menubar).getAllByRole('button', { name: item });
  fireEvent.click(matches[matches.length - 1]);
}

describe('AppShell dock wiring (task 8)', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
    // Clears call history on ALL mocks (including the module-level invoke mock)
    // without wiping their implementations — so per-test `shell_mounted` counts
    // don't accumulate across the suite.
    vi.clearAllMocks();
    dockRef.current = snapshot('docked');
  });

  it('while Routines is popped, Routines → Routines focuses the window instead of swapping the pane', async () => {
    dockRef.current = snapshot('popped');
    renderShell();
    await screen.findByTestId('folder-sidebar');

    // While popped, the top-level label reads "Routines ↗" (the pathway back).
    clickMenu('Routines ↗', /^Routines$/);
    // No inline pane swap — the mailbox stays and the popped window is focused.
    expect(screen.queryByTestId('routines-dashboard')).not.toBeInTheDocument();
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    await waitFor(() => expect(mockFocusSurface).toHaveBeenCalledWith('routines'));
  });

  it('while Routines is popped, New Routine… focuses the window and emits the new-routine intent', async () => {
    dockRef.current = snapshot('popped');
    renderShell();
    await screen.findByTestId('folder-sidebar');

    clickMenu('Routines ↗', /New Routine…/);
    expect(screen.queryByTestId('routine-designer')).not.toBeInTheDocument();
    await waitFor(() => expect(mockFocusSurface).toHaveBeenCalledWith('routines'));
    expect(emit).toHaveBeenCalledWith('dock:intent', { surface: 'routines', intent: 'new_routine' });
  });

  it('a foreground popped→docked arrival opens the inline surface on the token view', async () => {
    dockRef.current = snapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');

    // ⇤ Dock back (foreground) arrives: routines now docked, token foreground
    // with a designer view (fresh draft — no backend fetch).
    dockRef.current = snapshot('docked', {
      foreground: true,
      state: { view: { view: 'designer', routine: '', tab: 'design' } },
    });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    expect(await screen.findByTestId('routine-designer', {}, { timeout: 5000 })).toBeInTheDocument();
  });

  it('a NON-foreground popped→docked arrival leaves the mailbox pane alone (availability)', async () => {
    dockRef.current = snapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');

    dockRef.current = snapshot('docked', {
      foreground: false,
      state: { view: { view: 'dashboard' } },
    });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    // Availability semantics: no pane theft — the mailbox stays put.
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.queryByTestId('routines-dashboard')).not.toBeInTheDocument();
    expect(screen.queryByTestId('routine-designer')).not.toBeInTheDocument();
  });

  // --- Task 9: Tac Map pop-out wiring + positions snapshot handshake -------

  it("behavior 1 (AMD-3): ↗ Pop out lives on the MAP SURFACE — open the map, click its chip, the Tac Map pops out", async () => {
    renderShell();
    await screen.findByTestId('folder-sidebar');
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 });

    // tuxlink-w68mb: the dock row carries NO pop-out; the entry point is the
    // chip on the inline map surface (pop-out of what you are looking at).
    expect(screen.queryByTestId('aprs-map-popout')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('aprs-map-toggle'));
    const chip = await screen.findByTestId('aprs-map-popout', {}, { timeout: 5000 });
    expect(chip).toHaveTextContent('Pop out');
    fireEvent.click(chip);
    await waitFor(() =>
      expect(mockPopOut).toHaveBeenCalledWith('tac_map', { foreground: true, state: null }),
    );
  });

  it('behavior 2: while tac_map is popped, the toggle control shows the "in window" pathway; ⇤ dock back invokes dockBack', async () => {
    dockRef.current = tacMapSnapshot('popped');
    renderShell();
    await screen.findByTestId('folder-sidebar');
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 });

    // The Map toggle is gone — replaced by the compact focus/dock-back
    // pathway (spec §5 AMD-3: "Map ↗" text label + accessible-named ⇤ glyph).
    expect(screen.queryByTestId('aprs-map-toggle')).not.toBeInTheDocument();
    expect(screen.queryByTestId('aprs-map-popout')).not.toBeInTheDocument();
    const focus = screen.getByTestId('aprs-map-focus');
    expect(focus).toHaveTextContent('Map ↗');
    fireEvent.click(focus);
    await waitFor(() => expect(mockFocusSurface).toHaveBeenCalledWith('tac_map'));

    fireEvent.click(screen.getByTestId('aprs-map-dockback'));
    await waitFor(() =>
      expect(mockDockBack).toHaveBeenCalledWith('tac_map', { foreground: true, state: null }),
    );
  });

  it('behavior 2: a tac_map foreground popped→docked arrival opens the inline map (aprsOpen AND aprsMapOpen)', async () => {
    dockRef.current = tacMapSnapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');
    // No dock/map yet — the arrival below must open BOTH from scratch.
    expect(screen.queryByTestId('aprs-dock-surface')).not.toBeInTheDocument();

    dockRef.current = tacMapSnapshot('docked', { foreground: true, state: null });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    expect(await screen.findByTestId('aprs-dock-surface')).toBeInTheDocument();
    expect(await screen.findByTestId('aprs-positions-map', {}, { timeout: 5000 })).toBeInTheDocument();
  });

  // tuxlink-fh53x: when APRS owns the dock (no radio-mode panel mounted), the
  // dock surface itself hosts the tour's 'radio-dock' anchor so the tour can
  // spotlight the dock column instead of skipping the stop. (When the modem
  // tab shows a RadioPanel inside this surface, both elements carry the
  // anchor and querySelector resolves the outer surface — same spotlight.)
  it('tuxlink-fh53x: the APRS dock surface carries the radio-dock tour anchor', async () => {
    dockRef.current = tacMapSnapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');

    dockRef.current = tacMapSnapshot('docked', { foreground: true, state: null });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    const surface = await screen.findByTestId('aprs-dock-surface');
    expect(surface).toHaveAttribute('data-tour-anchor', 'radio-dock');
  });

  it('behavior 2: a NON-foreground tac_map popped→docked arrival changes neither aprsOpen nor aprsMapOpen', async () => {
    dockRef.current = tacMapSnapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');

    dockRef.current = tacMapSnapshot('docked', { foreground: false, state: null });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    // Availability semantics: no dock/map materializes from a non-foreground arrival.
    expect(screen.queryByTestId('aprs-dock-surface')).not.toBeInTheDocument();
    expect(screen.queryByTestId('aprs-positions-map')).not.toBeInTheDocument();
  });

  it('behavior 2: once tac_map becomes popped, the inline map never renders — even if aprsMapOpen was already true', async () => {
    dockRef.current = snapshot('docked'); // tac_map docked (default)
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 });
    fireEvent.click(screen.getByTestId('aprs-map-toggle'));
    expect(await screen.findByTestId('aprs-positions-map', {}, { timeout: 5000 })).toBeInTheDocument();

    // tac_map flips to popped (e.g. the ↗ affordance was used) — the inline
    // map disappears regardless of the still-true `aprsMapOpen` local state.
    dockRef.current = tacMapSnapshot('popped');
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    expect(screen.queryByTestId('aprs-positions-map')).not.toBeInTheDocument();
    // The chat dock itself is unaffected (aprsOpen stays true).
    expect(screen.getByTestId('aprs-chat-panel')).toBeInTheDocument();
  });

  it('regression (spec §5 AMD-2): ↗ pop-out clears aprsMapOpen so a NON-foreground dock-back does not spring the inline map back into the pane', async () => {
    // Inline map open, tac_map still docked.
    dockRef.current = snapshot('docked');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 });
    fireEvent.click(screen.getByTestId('aprs-map-toggle'));
    expect(await screen.findByTestId('aprs-positions-map', {}, { timeout: 5000 })).toBeInTheDocument();

    // ↗ pop the map out. The handler must clear `aprsMapOpen` (not just call
    // popOut) — otherwise nothing resets the flag while popped and it is
    // still true the instant the map docks back.
    fireEvent.click(screen.getByTestId('aprs-map-popout'));
    await waitFor(() =>
      expect(mockPopOut).toHaveBeenCalledWith('tac_map', { foreground: true, state: null }),
    );

    // Backend confirms the pop-out.
    dockRef.current = tacMapSnapshot('popped');
    let qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );
    expect(screen.queryByTestId('aprs-positions-map')).not.toBeInTheDocument();

    // A non-foreground popped→docked arrival — ✕ / Ctrl+W / WM close, per the
    // arrival effect's `foreground: false` early-return (availability
    // semantics; no pane theft). This is NOT the ⇤ restore path.
    dockRef.current = tacMapSnapshot('docked', { foreground: false, state: null });
    qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    // The inline map must NOT spring back and commandeer the reading pane —
    // the mailbox stays put.
    expect(screen.queryByTestId('aprs-positions-map')).not.toBeInTheDocument();
    // The chat dock itself is unaffected (aprsOpen stays true).
    expect(screen.getByTestId('aprs-chat-panel')).toBeInTheDocument();
  });

  // --- Task 10: APRS Chat pop-out entry point + placeholder + dock-aware flows

  it('entry point (spec §5): the docked APRS chat header shows ↗ Pop out; clicking pops the surface out', async () => {
    dockRef.current = snapshot('docked'); // aprs_chat docked (default)
    renderShell();
    await screen.findByTestId('folder-sidebar');
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 });

    // Text-labeled entry point in the chat panel header (never icon-only).
    const popout = screen.getByTestId('aprs-chat-popout');
    expect(popout).toHaveTextContent('Pop out');
    expect(popout).toHaveAccessibleName(/pop out/i);

    fireEvent.click(popout);
    await waitFor(() =>
      expect(mockPopOut).toHaveBeenCalledWith('aprs_chat', { foreground: true, state: null }),
    );
  });

  it('behavior 3: while aprs_chat is popped, the APRS tab body is a placeholder with focus + ⇤ dock-back', async () => {
    // Open the dock on the APRS tab while chat is still DOCKED.
    dockRef.current = snapshot('docked'); // aprs_chat docked (default)
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 });

    // The chat is popped out (↗ from the panel header) — the tab body swaps to a placeholder.
    dockRef.current = aprsChatSnapshot('popped');
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    const placeholder = await screen.findByTestId('aprs-chat-popped-placeholder');
    expect(placeholder).toHaveTextContent('APRS Chat ↗ — in its own window');
    expect(placeholder).toHaveTextContent('click to focus');
    expect(screen.queryByTestId('aprs-chat-panel')).not.toBeInTheDocument();
    // The panel (and thus its header ↗ Pop out entry point) is gone while
    // popped — no duplicate pop-out affordance in the placeholder.
    expect(screen.queryByTestId('aprs-chat-popout')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('aprs-chat-focus'));
    await waitFor(() => expect(mockFocusSurface).toHaveBeenCalledWith('aprs_chat'));

    fireEvent.click(screen.getByTestId('aprs-chat-dockback'));
    await waitFor(() =>
      expect(mockDockBack).toHaveBeenCalledWith('aprs_chat', { foreground: true, state: null }),
    );
  });

  it('behavior 3: an aprs_chat foreground popped→docked arrival opens the dock on the APRS tab', async () => {
    dockRef.current = aprsChatSnapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');
    expect(screen.queryByTestId('aprs-dock-surface')).not.toBeInTheDocument();

    dockRef.current = aprsChatSnapshot('docked', { foreground: true, state: null });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    // ⇤ activates the tab: the dock opens on APRS and the real panel renders (chat is docked now).
    expect(await screen.findByTestId('aprs-dock-surface')).toBeInTheDocument();
    expect(await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 })).toBeInTheDocument();
  });

  it('behavior 3: a NON-foreground aprs_chat popped→docked arrival opens neither the dock nor the tab', async () => {
    dockRef.current = aprsChatSnapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');

    dockRef.current = aprsChatSnapshot('docked', { foreground: false, state: null });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    expect(screen.queryByTestId('aprs-dock-surface')).not.toBeInTheDocument();
  });

  it('behavior 4: while aprs_chat is popped, the first-run listening switch focuses the window instead of opening the dock', async () => {
    // No radio configured (packet_config_get.linkKind === null) + not listening
    // ⇒ the ribbon control takes the first-run branch, which now routes on dock state.
    dockRef.current = aprsChatSnapshot('popped');
    renderShell();
    await screen.findByTestId('folder-sidebar');

    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    await waitFor(() => expect(mockFocusSurface).toHaveBeenCalledWith('aprs_chat'));
    // Behavior 4: it must NOT escort the operator to the in-dock placeholder.
    expect(screen.queryByTestId('aprs-dock-surface')).not.toBeInTheDocument();
  });

  // --- bd tuxlink-9obx2: Station Intelligence pop-out wiring ---------------

  it('entry point (spec §5): the docked Station Intelligence header shows ↗ Pop out; clicking pops the surface out', async () => {
    renderShell();
    await screen.findByTestId('folder-sidebar');
    clickMenu('Tools', /station intelligence/i);
    const popout = await screen.findByRole(
      'button',
      { name: /pop out station intelligence/i },
      { timeout: 10000 },
    );
    fireEvent.click(popout);
    await waitFor(() =>
      expect(mockPopOut).toHaveBeenCalledWith('station_intelligence', {
        foreground: true,
        state: null,
      }),
    );
  });

  it('while station_intelligence is popped, Tools → Station Intelligence focuses the window instead of opening a second copy', async () => {
    dockRef.current = stationIntelSnapshot('popped');
    renderShell();
    await screen.findByTestId('folder-sidebar');

    clickMenu('Tools', /station intelligence/i);
    expect(screen.queryByRole('dialog', { name: /station intelligence/i })).not.toBeInTheDocument();
    await waitFor(() => expect(mockFocusSurface).toHaveBeenCalledWith('station_intelligence'));
  });

  it('a station_intelligence foreground popped→docked arrival reopens the inline overlay', async () => {
    dockRef.current = stationIntelSnapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');
    expect(screen.queryByRole('dialog', { name: /station intelligence/i })).not.toBeInTheDocument();

    dockRef.current = stationIntelSnapshot('docked', { foreground: true, state: null });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    expect(
      await screen.findByRole('dialog', { name: /station intelligence/i }, { timeout: 10000 }),
    ).toBeInTheDocument();
  });

  it('a NON-foreground station_intelligence popped→docked arrival leaves the overlay closed (availability)', async () => {
    dockRef.current = stationIntelSnapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');

    dockRef.current = stationIntelSnapshot('docked', { foreground: false, state: null });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    // Availability semantics: no overlay theft, the mailbox stays put.
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: /station intelligence/i })).not.toBeInTheDocument();
  });

  it('once station_intelligence becomes popped, the inline overlay never renders, even if it was already open', async () => {
    dockRef.current = snapshot('docked'); // station_intelligence docked (default)
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');
    clickMenu('Tools', /station intelligence/i);
    expect(
      await screen.findByRole('dialog', { name: /station intelligence/i }, { timeout: 10000 }),
    ).toBeInTheDocument();

    // station_intelligence flips to popped (e.g. the ↗ affordance was used
    // from a second launch, or another window); the inline overlay
    // disappears regardless of the still-true `catalogBuilderOpen` local
    // state (the force-close effect).
    dockRef.current = stationIntelSnapshot('popped');
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );

    expect(screen.queryByRole('dialog', { name: /station intelligence/i })).not.toBeInTheDocument();
  });

  // Reviewer finding (Important, post-tuxlink-9obx2 review): the pre-mount-
  // popped test above ("while station_intelligence is popped, Tools ->
  // Station Intelligence focuses...") cannot discriminate a stale-closure
  // bug, since `handlers` is freshly built on the FIRST render and already
  // captures the right `stationIntelPopped` value with no prior render to go
  // stale relative to. This test starts DOCKED, renders, and only THEN
  // transitions to popped via `pushDockSnapshot` (a re-render of ONLY
  // AppShellInner, mirroring a real `dock:changed` event; see that helper's
  // doc comment for why the OTHER arrival tests' `rerender(<AppShell />)`
  // pattern cannot be reused here without masking the bug), so firing the
  // Tools menu action afterward exercises whatever closure the `handlers`
  // useMemo held across that transition. It would have caught the bug where
  // `stationIntelPopped` was missing from that memo's dependency array: the
  // memo never recomputed, so the menu action kept calling
  // `setCatalogBuilderOpen(true)` against a render guard that had already
  // flipped to false; a silent dead action.
  it('regression: popping station_intelligence mid-session (after mount) still makes Tools -> Station Intelligence focus the window, not a dead action', async () => {
    dockRef.current = snapshot('docked'); // station_intelligence docked (default)
    renderShell();
    await screen.findByTestId('folder-sidebar');

    pushDockSnapshot(stationIntelSnapshot('popped'));

    clickMenu('Tools', /station intelligence/i);
    await waitFor(() => expect(mockFocusSurface).toHaveBeenCalledWith('station_intelligence'));
    expect(screen.queryByRole('dialog', { name: /station intelligence/i })).not.toBeInTheDocument();
  });

  it('invokes shell_mounted once on mount (launch restoration signal, spec §3)', async () => {
    const core = await import('@tauri-apps/api/core');
    renderShell();
    await screen.findByTestId('folder-sidebar');
    const calls = (core.invoke as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(
      (c) => c[0] === 'shell_mounted',
    );
    expect(calls).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// bd tuxlink-mfssz: Elmer pop-out wiring
// ---------------------------------------------------------------------------

describe('AppShell Elmer dock wiring (bd tuxlink-mfssz)', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
    vi.clearAllMocks();
    dockRef.current = elmerSnapshot('docked');
  });

  function rerenderShell(rerender: (ui: ReactNode) => void) {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={qc}>
        <AppShell />
      </QueryClientProvider>,
    );
  }

  it('↗ in the drawer header pops Elmer out carrying the conversation token', async () => {
    renderShell();
    await screen.findByTestId('folder-sidebar');

    clickMenu('Tools', /Elmer \(AI assistant\)/);
    const popBtn = await screen.findByTestId('elmer-pop-out', {}, { timeout: 5000 });
    fireEvent.click(popBtn);

    await waitFor(() => expect(mockPopOut).toHaveBeenCalledTimes(1));
    const [surface, envelope] = mockPopOut.mock.calls[0] as unknown as [string, { foreground: boolean; state: { items: unknown[] } }];
    expect(surface).toBe('elmer');
    expect(envelope.foreground).toBe(true);
    // The pane reported its (empty) conversation before the click — the token
    // must carry the items array, never undefined.
    expect(Array.isArray(envelope.state.items)).toBe(true);
  });

  it('while Elmer is popped, Tools → Elmer focuses the window instead of opening the drawer', async () => {
    dockRef.current = elmerSnapshot('popped');
    renderShell();
    await screen.findByTestId('folder-sidebar');

    clickMenu('Tools', /Elmer \(AI assistant\)/);
    await waitFor(() => expect(mockFocusSurface).toHaveBeenCalledWith('elmer'));
    expect(screen.queryByTestId('elmer-pane')).not.toBeInTheDocument();
  });

  it("while Elmer is popped, Set up Elmer's model… focuses the window and forwards the open_model intent", async () => {
    dockRef.current = elmerSnapshot('popped');
    renderShell();
    await screen.findByTestId('folder-sidebar');

    clickMenu('Tools', /Set up Elmer's model/);
    await waitFor(() => expect(mockFocusSurface).toHaveBeenCalledWith('elmer'));
    expect(emit).toHaveBeenCalledWith('dock:intent', { surface: 'elmer', intent: 'open_model' });
    expect(screen.queryByTestId('elmer-pane')).not.toBeInTheDocument();
  });

  it('Dock Elmer back is hidden while docked, and while popped forwards the dock_back intent (never a main-side state:null dockBack)', async () => {
    dockRef.current = elmerSnapshot('popped');
    renderShell();
    await screen.findByTestId('folder-sidebar');

    clickMenu('Tools', /Dock Elmer back/);
    await waitFor(() =>
      expect(emit).toHaveBeenCalledWith('dock:intent', { surface: 'elmer', intent: 'dock_back' }),
    );
    // The conversation lives in the popped window — main must NOT dock back
    // with its own (null) state.
    expect(mockDockBack).not.toHaveBeenCalled();
  });

  it('Dock Elmer back is not rendered while Elmer is docked', async () => {
    renderShell();
    await screen.findByTestId('folder-sidebar');
    const menubar = screen.getByRole('menubar');
    fireEvent.click(within(menubar).getByRole('button', { name: 'Tools' }));
    expect(within(menubar).queryByRole('button', { name: /Dock Elmer back/ })).not.toBeInTheDocument();
  });

  it('a foreground popped→docked arrival opens the drawer with the adopted conversation', async () => {
    dockRef.current = elmerSnapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');

    dockRef.current = elmerSnapshot('docked', {
      foreground: true,
      state: {
        items: [{ kind: 'turn', id: 'popwin-0', role: 'user', text: 'carried across windows' }],
      },
    });
    rerenderShell(rerender);

    expect(await screen.findByTestId('elmer-pane', {}, { timeout: 5000 })).toBeInTheDocument();
    expect(await screen.findByText('carried across windows')).toBeInTheDocument();
  });

  it('adrev 2026-07-20 P1: a null/invalid dock-back token does NOT wipe the inline conversation (foreground still opens the drawer)', async () => {
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');

    // Build inline conversation state: open the drawer and send a message
    // (send appends the user turn locally, no backend round-trip needed).
    clickMenu('Tools', /Elmer \(AI assistant\)/);
    const input = await screen.findByPlaceholderText(/ask/i, {}, { timeout: 5000 });
    fireEvent.change(input, { target: { value: 'precious inline turn' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(await screen.findByText('precious inline turn')).toBeInTheDocument();

    // Pop out, then dock back with state:null — the host flushed before
    // ElmerPopped registered getContext (or a liveness-timeout stateless
    // dock-back). The inline mounted-hidden copy is the best remaining state.
    dockRef.current = elmerSnapshot('popped');
    rerenderShell(rerender);
    dockRef.current = elmerSnapshot('docked', { foreground: true, state: null });
    rerenderShell(rerender);

    // The conversation survives (no remount-wipe) and the drawer is open.
    expect(await screen.findByText('precious inline turn', {}, { timeout: 5000 })).toBeInTheDocument();
  });

  it('a NON-foreground popped→docked arrival keeps the drawer closed but STILL adopts the conversation', async () => {
    dockRef.current = elmerSnapshot('popped');
    const { rerender } = renderShell();
    await screen.findByTestId('folder-sidebar');

    dockRef.current = elmerSnapshot('docked', {
      foreground: false,
      state: {
        items: [{ kind: 'turn', id: 'popwin-0', role: 'user', text: 'adopted quietly' }],
      },
    });
    rerenderShell(rerender);

    // Availability semantics: no drawer theft.
    expect(screen.queryByText('adopted quietly')).not.toBeInTheDocument();

    // But the adoption is real (the deliberate asymmetry with routines): the
    // next open shows the popped window's conversation, not the pane's
    // diverged mounted-hidden copy.
    clickMenu('Tools', /Elmer \(AI assistant\)/);
    expect(await screen.findByText('adopted quietly', {}, { timeout: 5000 })).toBeInTheDocument();
  });
});
