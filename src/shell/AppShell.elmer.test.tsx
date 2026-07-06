/**
 * AppShell — Elmer pane lazy-load and close-survives-mount tests.
 *
 * Asserts that:
 *   1. ElmerPane is NOT mounted when Elmer has never been opened (cold-start
 *      discipline: the module chunk must not be fetched until needed).
 *   2. ElmerPane IS mounted once the ribbon launcher is clicked.
 *   3. (tuxlink-9uat6) Closing the pane does NOT unmount it — the pane
 *      element remains in the DOM (hidden), preserving useElmer state +
 *      event-listeners + any in-flight inference run.
 *
 * Strategy: mount the REAL AppShell (same pattern as AppShell.aprs.test.tsx)
 * with the Tauri IPC layer mocked so jsdom can mount the shell. Drive the real
 * ribbon-elmer-launcher button (data-testid from ElmerAgentChip in
 * DashboardRibbon) to trigger the open path, then use findByTestId to wait
 * for the lazy ElmerPane chunk to resolve (same async pattern the APRS test
 * uses for aprs-chat-panel). The close button is data-testid="elmer-close"
 * (ElmerPane.tsx line 832).
 *
 * The KEY assertion (tuxlink-9uat6 proof): after clicking close, query
 * data-testid="elmer-pane" — if it returns null the pane was unmounted and
 * useElmer state was lost. It must be non-null AND have an ancestor with the
 * `hidden` attribute (the <div hidden={!elmerOpen}> wrapper in AppShell).
 */

import type { ReactNode } from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MessageMeta } from '../mailbox/types';

// ---------------------------------------------------------------------------
// Tauri IPC mocks — mirrors the pattern in AppShell.aprs.test.tsx, extended
// with elmer_config_read so useElmer's eager configRead call on mount resolves.
// ---------------------------------------------------------------------------

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'config_read') return null;
    if (cmd === 'backend_status') return null;
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'modem_get_status') {
      return {
        state: 'stopped',
        peer: null, mode: null, widthHz: null, pttBackend: null,
        snDb: null, vuDbfs: null, throughputBps: null,
        bytesRx: 0, bytesTx: 0, uptimeSec: 0,
        arqFlags: { busy: false, rx: false, tx: false },
        lastError: null,
      };
    }
    if (cmd === 'packet_config_get') {
      return {
        ssid: 7, listenDefault: true, linkKind: null, tcpHost: null,
        tcpPort: null, serialDevice: null, serialBaud: null, txdelay: 30,
        persistence: 63, slotTime: 10, paclen: 128, maxframe: 4,
        t1Ms: 3000, n2Retries: 10,
      };
    }
    if (cmd === 'position_status') return { gps_ready: false, broadcast_grid: '', ui_grid: '' };
    if (cmd === 'tauri_search_list_saved') return [];
    if (cmd === 'tauri_search_list_recent') return [];
    if (cmd === 'contacts_read') return { schema_version: 1, contacts: [], groups: [] };
    if (cmd === 'contacts_suggestions') return [];
    if (cmd === 'aprs_config_get') return { listenDefault: false };
    if (cmd === 'network_po_favorites_get') return [];
    if (cmd === 'mailbox_list') return [];
    // useElmer's eagerly-called configRead on ElmerPane mount.
    if (cmd === 'elmer_config_read') {
      return {
        agentEndpoint: '',
        agentModel: '',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
      };
    }
    return undefined;
  }),
}));

// listen: used by useMailboxChangeEvents, useElmer's event subscriptions, and
// the session log. Return a no-op unlisten function so subscriptions mount
// cleanly in jsdom without real IPC.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

// TitleBar + ResizeHandles call window controls (minimize / toggleMaximize / close).
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: 'main',
    setTitle: vi.fn(async () => {}),
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    startResizeDragging: vi.fn(async () => {}),
  }),
  ResizeDirection: {
    North: 'North', South: 'South', East: 'East', West: 'West',
    NorthEast: 'NorthEast', NorthWest: 'NorthWest',
    SouthEast: 'SouthEast', SouthWest: 'SouthWest',
  },
}));

// react-virtuoso renders into a zero-height scroller under jsdom (no layout
// engine), so rows never paint. Mirror the flat-render stub AppShell.test.tsx
// uses so the shell mounts without layout-dependent crashes.
vi.mock('react-virtuoso', () => ({
  Virtuoso: ({
    data,
    itemContent,
  }: {
    data: MessageMeta[];
    itemContent: (i: number, m: MessageMeta) => unknown;
  }) => (
    <div data-testid="virtuoso-mock">
      {data.map((m, i) => (
        <div key={m.id}>{itemContent(i, m) as ReactNode}</div>
      ))}
    </div>
  ),
}));

import { AppShell } from './AppShell';

function renderShell() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('ElmerPane lazy-load on real AppShell (tuxlink-9uat6)', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
  });

  it('ElmerPane is NOT in the DOM before the ribbon launcher is clicked (cold-start discipline)', () => {
    renderShell();
    // elmerEverOpened is false on cold start — the pane must not be mounted.
    expect(screen.queryByTestId('elmer-pane')).toBeNull();
  });

  it('ElmerPane IS in the DOM after the ribbon launcher is clicked', async () => {
    renderShell();
    // Trigger the real open affordance — DashboardRibbon's ElmerAgentChip.
    fireEvent.click(screen.getByTestId('ribbon-elmer-launcher'));
    // ElmerPane is lazy (React.lazy + Suspense); await the chunk to resolve.
    expect(
      await screen.findByTestId('elmer-pane', {}, { timeout: 5000 }),
    ).toBeInTheDocument();
  });

  it('(tuxlink-9uat6) closing the pane does NOT unmount it — elmer-pane stays in the DOM', async () => {
    renderShell();

    // Step 1: open the real Elmer pane via the ribbon launcher.
    fireEvent.click(screen.getByTestId('ribbon-elmer-launcher'));

    // Step 2: wait for the lazy ElmerPane chunk to resolve and mount.
    const pane = await screen.findByTestId('elmer-pane', {}, { timeout: 5000 });
    expect(pane).toBeInTheDocument();

    // Step 3: close the pane via ElmerPane's own close button (data-testid="elmer-close").
    // This calls onClose → setElmerOpen(false) in AppShell.
    fireEvent.click(screen.getByTestId('elmer-close'));

    // Step 4 (KEY ASSERTION — proves tuxlink-9uat6 fix): the pane element MUST
    // still be in the DOM after close. If it were null, useElmer state and
    // event-listeners would have been torn down, losing any in-flight run.
    const paneAfterClose = screen.queryByTestId('elmer-pane');
    expect(paneAfterClose).not.toBeNull();

    // Step 5: an ancestor of the pane must carry the `hidden` attribute,
    // confirming the <div hidden={!elmerOpen}> wrapper in AppShell is hiding
    // the pane (display:none) — NOT destroying it.
    let el: HTMLElement | null = paneAfterClose;
    let hiddenAncestorFound = false;
    while (el) {
      if (el.hasAttribute('hidden')) {
        hiddenAncestorFound = true;
        break;
      }
      el = el.parentElement;
    }
    expect(hiddenAncestorFound).toBe(true);
  });

  it('reopening a closed pane reveals it (hidden attribute removed from the wrapper)', async () => {
    renderShell();

    // Open → close → reopen cycle on the REAL AppShell.
    fireEvent.click(screen.getByTestId('ribbon-elmer-launcher'));
    await screen.findByTestId('elmer-pane', {}, { timeout: 5000 });

    fireEvent.click(screen.getByTestId('elmer-close'));

    // After close: pane is still mounted but hidden.
    await waitFor(() => {
      const pane = screen.queryByTestId('elmer-pane');
      expect(pane).not.toBeNull();
      let el: HTMLElement | null = pane;
      let found = false;
      while (el) {
        if (el.hasAttribute('hidden')) { found = true; break; }
        el = el.parentElement;
      }
      expect(found).toBe(true);
    });

    // Reopen via the ribbon launcher again (toggle: closed→open).
    fireEvent.click(screen.getByTestId('ribbon-elmer-launcher'));

    // After reopen: the wrapper's hidden attribute is gone — pane is visible.
    await waitFor(() => {
      const pane = screen.queryByTestId('elmer-pane');
      expect(pane).not.toBeNull();
      let el: HTMLElement | null = pane;
      let found = false;
      while (el) {
        if (el.hasAttribute('hidden')) { found = true; break; }
        el = el.parentElement;
      }
      expect(found).toBe(false);
    });
  });
});
