// AppShell ⇄ dock-registry wiring (tuxlink-dmwte task 8, spec §5/§6).
// The dock-state module is mocked so this suite controls the snapshot the
// shell sees: Routines docked vs popped, and a popped→docked foreground
// arrival. Mock scaffold copied from AppShell.routines.test.tsx (that file
// knows which backends a real AppShell mount needs).
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, within, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import type { MessageMeta } from '../mailbox/types';
import type { DockSnapshot, DockMode } from '../dock/dockState';

// --- dock-state module mock ------------------------------------------------
// `useDockState` returns the mutable `dockRef.current`; the transition test
// reassigns it to a fresh object and `rerender`s so the shell's `dock:changed`
// effect (keyed on the snapshot) fires. Spies + the mutable ref go through
// `vi.hoisted` since the `vi.mock` factory is hoisted above module top-level.
const { mockFocusSurface, mockPopOut, mockDockBack, dockRef } = vi.hoisted(() => ({
  mockFocusSurface: vi.fn(async () => {}),
  mockPopOut: vi.fn(async () => {}),
  mockDockBack: vi.fn(async () => {}),
  dockRef: { current: null as DockSnapshot | null },
}));

function snapshot(routines: DockMode, context: unknown = null): DockSnapshot {
  return {
    surfaces: { routines, tac_map: 'docked', aprs_chat: 'docked' },
    context: { routines: context, tac_map: null, aprs_chat: null },
  };
}

// tuxlink-dmwte task 9: same shape as `snapshot()` above, but flips tac_map's
// mode/context instead of routines' — routines stays docked/null throughout
// (task 9 does not touch it).
function tacMapSnapshot(tac_map: DockMode, context: unknown = null): DockSnapshot {
  return {
    surfaces: { routines: 'docked', tac_map, aprs_chat: 'docked' },
    context: { routines: null, tac_map: context, aprs_chat: null },
  };
}

// tuxlink-dmwte task 10: flips aprs_chat's mode/context; routines + tac_map
// stay docked/null throughout (task 10 does not touch them).
function aprsChatSnapshot(aprs_chat: DockMode, context: unknown = null): DockSnapshot {
  return {
    surfaces: { routines: 'docked', tac_map: 'docked', aprs_chat },
    context: { routines: null, tac_map: null, aprs_chat: context },
  };
}

vi.mock('../dock/dockState', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../dock/dockState')>();
  return {
    ...actual, // keep the real consentHostWindow + SURFACE_WINDOW_LABEL
    useDockState: () => dockRef.current,
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

  it('behavior 1: clicking ↗ in the map header controls pops the Tac Map out', async () => {
    renderShell();
    await screen.findByTestId('folder-sidebar');
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 });

    fireEvent.click(screen.getByTestId('aprs-map-popout'));
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

    // The Map toggle + pop-out are gone — replaced by the focus/dock-back pathway.
    expect(screen.queryByTestId('aprs-map-toggle')).not.toBeInTheDocument();
    expect(screen.queryByTestId('aprs-map-popout')).not.toBeInTheDocument();
    const focus = screen.getByTestId('aprs-map-focus');
    expect(focus).toHaveTextContent('Tac Map ↗ — in window');
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

  // --- Task 10: APRS Chat pop-out placeholder + dock-aware flows -----------

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
