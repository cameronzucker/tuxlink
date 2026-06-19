// App-level dock-integration test (production mount path). tuxlink-2f2n Plan 2:
// the APRS chat now lives in the shared right dock, opened from the status-strip
// control in the DashboardRibbon. This test mounts the REAL AppShell behind its
// production provider (QueryClientProvider) — mirroring src/shell/AppShell.test.tsx
// — so the wiring AppShell.test.tsx scaffolds piecemeal is exercised end-to-end:
// click `dash-aprs-control` → the dock paints `aprs-chat-panel` + `aprs-dock-tabs`.
//
// The modem mock returns `stopped`, so radioPanelMode is null and the dock
// appears purely from aprsOpen — proving the re-home path, not a modem path.
// fireEvent (not @testing-library/user-event, which is not installed) per plan.
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'config_read') return null;
    if (cmd === 'backend_status') return null;
    if (cmd === 'session_log_snapshot') return [];
    if (cmd === 'modem_get_status') {
      // STOPPED keeps the radio dock unmounted, so the dock that appears on
      // open is the APRS dock alone (radioPanelMode === null).
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
      // No link configured (linkKind: null) so clicking the status-strip control
      // deterministically opens the dock via the first-run setup path. Post-a1j3
      // the control is pure on/off and only opens the dock when nothing is
      // configured yet; with a configured link these tests would race the config
      // query (dock-open vs start-listening) — tuxlink-28o0.
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
    if (cmd === 'contacts_read') return [];
    if (cmd === 'contacts_suggestions') return [];
    if (cmd === 'aprs_config_get') return { listenDefault: false };
    return undefined;
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTitle: vi.fn(async () => {}) }),
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

describe('APRS dock integration', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
  });

  it('opens the chat in the right dock from the status-strip control', async () => {
    renderShell();
    // No dock APRS panel until opened.
    expect(screen.queryByTestId('aprs-chat-panel')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    // The lazy AprsChatPanel resolves; the dock tab row mounts alongside it.
    expect(await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 })).toBeInTheDocument();
    expect(screen.getByTestId('aprs-dock-tabs')).toBeInTheDocument();
  });

  // tuxlink-iehg regression: the tab row + chat panel MUST be wrapped in ONE
  // `.aprs-dock-surface` container. The drawer body is `display:contents` on
  // desktop, so two bare children get promoted to separate grid items and the
  // second overflows the single dock column (the bug: chat escaped to the
  // bottom-left, tabs stretched the right column). jsdom has no layout engine,
  // so the only catchable invariant is structural — both must descend from the
  // single surface element. Without the wrapper this test cannot find the
  // surface (and the children are bare siblings), so it fails pre-fix.
  it('wraps the dock tabs and chat panel in a single dock-surface container', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    const chat = await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 });
    const tabs = screen.getByTestId('aprs-dock-tabs');
    const surface = screen.getByTestId('aprs-dock-surface');
    // Both the tab row and the chat panel live inside the ONE dock surface —
    // not as bare siblings that the grid would scatter.
    expect(surface).toContainElement(tabs);
    expect(surface).toContainElement(chat);
    // And the tab row sits directly inside the surface, above the body.
    expect(tabs.parentElement).toBe(surface);
  });

  // tuxlink-iehg wire-walk flow 6: the chat opened one-way (no way to close it /
  // free the reading pane). The dock close control must dismiss the whole APRS
  // surface. With no radio session active (the mock returns a stopped modem),
  // closing collapses the dock entirely.
  it('closes the dock from the close control, freeing the reading pane', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    expect(await screen.findByTestId('aprs-chat-panel', {}, { timeout: 5000 })).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('aprs-dock-close'));
    // Dock surface + chat panel are gone (no radioPanelMode → no dock at all).
    expect(screen.queryByTestId('aprs-dock-surface')).not.toBeInTheDocument();
    expect(screen.queryByTestId('aprs-chat-panel')).not.toBeInTheDocument();
  });

  // tuxlink-6vgt: the Map toggle expands the heard-positions map into the
  // reading-pane region (left of the chat dock); toggling it off restores the
  // normal reading pane. The map is lazy-loaded (findByTestId awaits the chunk).
  it('toggles the heard-positions map into the reading pane', async () => {
    renderShell();
    fireEvent.click(screen.getByTestId('dash-aprs-control'));
    await screen.findByTestId('aprs-chat-panel');
    // Map not shown until toggled on; the chat dock stays put.
    expect(screen.queryByTestId('aprs-positions-map')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('aprs-map-toggle'));
    expect(await screen.findByTestId('aprs-positions-map')).toBeInTheDocument();
    // Chat dock remains docked on the right alongside the expanded map.
    expect(screen.getByTestId('aprs-chat-panel')).toBeInTheDocument();
    // Toggling off collapses the map back to the normal reading pane.
    fireEvent.click(screen.getByTestId('aprs-map-toggle'));
    expect(screen.queryByTestId('aprs-positions-map')).not.toBeInTheDocument();
  });
});
