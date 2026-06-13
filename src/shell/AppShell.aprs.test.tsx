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
      return {
        ssid: 7, listenDefault: true, linkKind: 'Tcp', tcpHost: '127.0.0.1',
        tcpPort: 8001, serialDevice: null, serialBaud: null, txdelay: 30,
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
    expect(await screen.findByTestId('aprs-chat-panel')).toBeInTheDocument();
    expect(screen.getByTestId('aprs-dock-tabs')).toBeInTheDocument();
  });
});
