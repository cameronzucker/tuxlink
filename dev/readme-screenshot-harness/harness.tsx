// README screenshot harness.
//
// Mounts real frontend components in WebKitGTK with canned, privacy-safe
// Tauri IPC responses. This keeps README imagery grounded in the running app
// UI without requiring a Rust/Tauri build or exposing a real station identity.
//
// Usage:
//   VITE_TUXLINK_FIXTURE=1 pnpm dev -- --host 127.0.0.1
//   python3 dev/render-harness/snapshot.py \
//     "http://127.0.0.1:1420/dev/readme-screenshot-harness/harness.html?view=shell" \
//     docs/readme/images/tuxlink-mailbox.png 1920 1080 15000
//
// Query params:
//   view=shell|wizard|request

import React, { Suspense } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '../../src/App.css';
import type { CatalogEntry } from '../../src/catalog/types';

type TauriInternals = {
  invoke: (cmd: string, args?: unknown) => Promise<unknown>;
  transformCallback: (cb: (...args: unknown[]) => unknown, once?: boolean) => number;
  unregisterCallback: (id: number) => void;
};

const params = new URLSearchParams(location.search);
const view = params.get('view') ?? 'shell';

const CATALOG: CatalogEntry[] = [
  { category: 'WX_US_WA', filename: 'WA_FCST', description: 'Washington - state forecast (NWS)', size_bytes: 4200 },
  { category: 'WX_US_WA', filename: 'WA_ZONE', description: 'Washington - zone forecasts', size_bytes: 6100 },
  { category: 'WX_EASTPAC', filename: 'EPAC_HIGH', description: 'NE Pacific - high seas forecast', size_bytes: 9100 },
  { category: 'WX_EASTPAC', filename: 'EPAC_COASTAL', description: 'NE Pacific - coastal waters', size_bytes: 7300 },
  { category: 'PROPAGATION', filename: 'PROP_3DAY', description: '3-day HF propagation outlook', size_bytes: 1800 },
  { category: 'PROPAGATION', filename: 'PROP_WWV', description: 'WWV solar-terrestrial summary', size_bytes: 900 },
  { category: 'WL2K_RMS', filename: 'PUB_VARA', description: 'Public VARA HF RMS gateways', size_bytes: 11000 },
  { category: 'WL2K_RMS', filename: 'PUB_ARDOP', description: 'Public ARDOP RMS gateways', size_bytes: 9000 },
  { category: 'INQUIRIES', filename: 'INQUIRIES', description: 'Catalog inquiries help and how-to', size_bytes: 2200 },
  { category: 'BULLETINS', filename: 'B_ARRL', description: 'ARRL bulletins', size_bytes: 3400 },
];

const CONFIG = {
  connect_to_cms: true,
  transport: 'CmsSsl',
  host: 'cms-z.winlink.org',
  callsign: 'W4PHS',
  identifier: null,
  grid: 'EM75xx',
  gps_state: 'BroadcastAtPrecision',
  position_precision: 'FourCharGrid',
  position_source: 'Gps',
  review_inbound_before_download: true,
};

const MODEM_STOPPED = {
  state: 'stopped',
  peer: null,
  mode: null,
  widthHz: null,
  pttBackend: null,
  snDb: null,
  vuDbfs: null,
  throughputBps: null,
  bytesRx: 0,
  bytesTx: 0,
  uptimeSec: 0,
  arqFlags: { busy: false, rx: false, tx: false },
  lastError: null,
};

const PACKET_CONFIG = {
  ssid: 7,
  listenDefault: true,
  linkKind: 'Tcp',
  tcpHost: '127.0.0.1',
  tcpPort: 8001,
  serialDevice: null,
  serialBaud: null,
  txdelay: 30,
  persistence: 63,
  slotTime: 10,
  paclen: 128,
  maxframe: 4,
  t1Ms: 3000,
  n2Retries: 10,
};

const CONTACTS = {
  schema_version: 1,
  contacts: [
    {
      id: 'c-wx4mtl',
      callsign: 'WX4MTL',
      name: 'James - EC Shelby County',
      email: 'WX4MTL@winlink.org',
      tactical: 'MEMPHIS-ARES',
      notes: 'Weather coordination',
      updatedAt: '2026-06-10T00:00:00Z',
    },
  ],
  groups: [
    {
      id: 'g-ares',
      name: 'Memphis ARES',
      memberIds: ['c-wx4mtl'],
      notes: '',
      updatedAt: '2026-06-10T00:00:00Z',
    },
  ],
};

let callbackId = 1;
const callbacks = new Map<number, (...args: unknown[]) => unknown>();

function installTauriShim() {
  const w = window as unknown as { __TAURI_INTERNALS__: TauriInternals & { metadata: unknown } };

  w.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: 'main' },
      currentWebview: { label: 'main' },
    },
    invoke: async (cmd: string, args?: unknown) => {
      if (cmd === 'get_wizard_completed') return true;
      if (cmd === 'emit_first_paint_complete') return null;
      if (cmd === 'config_read') return CONFIG;
      if (cmd === 'backend_status') return null;
      if (cmd === 'position_status') return { gps_ready: true, broadcast_grid: 'EM75', ui_grid: 'EM75xx' };
      if (cmd === 'mailbox_list') return [];
      if (cmd === 'message_read') return null;
      if (cmd === 'message_set_read_state') return null;
      if (cmd === 'session_log_snapshot') {
        return [
          { seq: 1, ts: '14:32:14', line: 'Connecting to Winlink CMS via telnet...' },
          { seq: 2, ts: '14:32:15', line: 'Connected to CMS gateway 1235-2.cms.winlink.org' },
          { seq: 3, ts: '14:32:18', line: 'Receiving message 4 of 4 from WX4MTL@winlink.org' },
          { seq: 4, ts: '14:32:19', line: 'Session complete - 4 received - 1 sent - 7s' },
        ];
      }
      if (cmd === 'modem_get_status') return MODEM_STOPPED;
      if (cmd === 'packet_config_get') return PACKET_CONFIG;
      if (cmd === 'contacts_read') return CONTACTS;
      if (cmd === 'contacts_suggestions') return [];
      if (cmd === 'user_folders_list') return [];
      if (cmd === 'tauri_search_list_saved') return [];
      if (cmd === 'tauri_search_list_recent') return [];
      if (cmd === 'network_po_favorites_get') return [];
      if (cmd === 'catalog_list') return CATALOG;
      if (cmd === 'catalog_send_inquiry') return 'MID-README-0001';

      if (cmd.includes('plugin:event') || cmd.includes('event')) return null;
      if (cmd.includes('plugin:window') || cmd.includes('window')) return null;
      if (cmd.includes('plugin:webview') || cmd.includes('webview')) return null;

      console.warn('README screenshot harness: unhandled invoke', cmd, args);
      return null;
    },
    transformCallback: (cb, once = false) => {
      const id = callbackId++;
      callbacks.set(id, (...args: unknown[]) => {
        const result = cb(...args);
        if (once) callbacks.delete(id);
        return result;
      });
      return id;
    },
    unregisterCallback: (id) => {
      callbacks.delete(id);
    },
  };
}

async function main() {
  installTauriShim();

  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false },
    },
  });

  const root = createRoot(document.getElementById('root')!);

  if (view === 'wizard') {
    const { Wizard } = await import('../../src/wizard/Wizard');
    root.render(<Wizard />);
    return;
  }

  if (view === 'request') {
    const { RequestCenter } = await import('../../src/request/RequestCenter');
    root.render(
      <QueryClientProvider client={queryClient}>
        <RequestCenter initialView="home" onClose={() => undefined} />
      </QueryClientProvider>,
    );
    return;
  }

  const { AppShell } = await import('../../src/shell/AppShell');
  root.render(
    <QueryClientProvider client={queryClient}>
      <Suspense fallback={<div data-testid="readme-harness-loading" />}>
        <AppShell />
      </Suspense>
    </QueryClientProvider>,
  );
}

void main().catch((err) => {
  console.error('README screenshot harness failed:', err);
  const root = document.getElementById('root');
  if (root) {
    root.textContent = err instanceof Error ? err.message : String(err);
  }
});
