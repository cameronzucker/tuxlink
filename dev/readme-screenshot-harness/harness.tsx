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
//   dock=vara|ardop|packet   (shell) — open that transport's radio modem dock
//   dock=aprs                (shell) — open the APRS tactical-chat dock + inject heard traffic
//   scheme=<color-scheme-id> (shell) — apply a color scheme before render

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

// Surface render errors visibly (the snapshot can't read the console).
class HarnessBoundary extends React.Component<{ children: React.ReactNode }, { err: string | null }> {
  state = { err: null as string | null };
  static getDerivedStateFromError(e: unknown) {
    return { err: e instanceof Error ? `${e.message}\n${e.stack ?? ''}` : String(e) };
  }
  render() {
    if (this.state.err) {
      return React.createElement(
        'pre',
        { style: { color: '#f88', padding: 16, fontSize: 14, whiteSpace: 'pre-wrap' } },
        `RENDER ERROR: ${this.state.err}`,
      );
    }
    return this.props.children as React.ReactElement;
  }
}

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

// Privacy-safe inbox: fictional callsigns, realistic EmComm traffic. Shape
// mirrors MessageMeta (src/mailbox/types.ts).
const MESSAGES = [
  { id: 'M1', folder: 'inbox', from: 'WX4MTL@winlink.org', to: ['W4PHS@winlink.org'], subject: 'ICS-213: Shelter status — Shelby County EOC', date: '2026-06-10T14:32:00Z', unread: true, bodySize: 1840, hasAttachments: false, formTag: 'ICS-213', preview: 'Bartlett HS shelter at 62 of 120 cots. Generator fuel ~18h. Requesting water resupply...' },
  { id: 'M2', folder: 'inbox', from: 'K4ARC@winlink.org', to: ['W4PHS@winlink.org'], subject: 'ARES net check-in roster — 0600Z', date: '2026-06-10T13:05:00Z', unread: true, bodySize: 920, hasAttachments: false, preview: '14 stations checked in. Net control K4ARC. Next net 1800Z on the county VARA gateway.' },
  { id: 'M3', folder: 'inbox', from: 'SERVICE', to: ['W4PHS@winlink.org'], subject: 'INQUIRY - https://tgftp.nws.noaa.gov/data/raw/fp/fpus65.kpsr.sft.psr.txt', date: '2026-06-10T12:18:00Z', unread: false, bodySize: 2228, hasAttachments: false, preview: 'Tabular State Forecast — Southwest Arizona. National Weather Service Phoenix AZ...' },
  { id: 'M4', folder: 'inbox', from: 'N4SAR@winlink.org', to: ['W4PHS@winlink.org'], subject: 'Welfare traffic — Hutchins family OK', date: '2026-06-10T11:47:00Z', unread: false, bodySize: 640, hasAttachments: false, preview: 'Please relay to requesting party: all four accounted for, no injuries, sheltering in place.' },
  { id: 'M5', folder: 'inbox', from: 'KK4OBN@winlink.org', to: ['W4PHS@winlink.org'], subject: 'ICS-213RR: Resource request — 6 cots, 200 MRE', date: '2026-06-10T10:22:00Z', unread: false, bodySize: 1510, hasAttachments: true, formTag: 'ICS-213RR', preview: 'Priority: routine. Deliver to Bartlett HS staging by 1600 local. Authorizing official...' },
  { id: 'M6', folder: 'inbox', from: 'W4EM@winlink.org', to: ['W4PHS@winlink.org'], subject: 'County EOC SITREP 06 — power restoration', date: '2026-06-10T09:10:00Z', unread: false, bodySize: 2040, hasAttachments: false, preview: 'MLGW reports 71% restored. Two shelters consolidating. Amateur traffic steady on 80m + VARA.' },
  { id: 'M7', folder: 'inbox', from: 'N0CALL@winlink.org', to: ['W4PHS@winlink.org'], subject: 'Test message — gateway reachability', date: '2026-06-09T22:40:00Z', unread: false, bodySize: 210, hasAttachments: false, preview: 'Confirming the post office is reachable over VARA HF. 73.' },
];

// Full reading-pane content for the opened message (ParsedMessage shape).
const MESSAGE_BODY = {
  id: 'M1',
  folder: 'inbox',
  subject: 'ICS-213: Shelter status — Shelby County EOC',
  from: 'WX4MTL@winlink.org',
  to: ['W4PHS@winlink.org'],
  date: '2026-06-10T14:32:00Z',
  isForm: false,
  hasAttachments: false,
  attachments: [],
  body: [
    'ICS-213 GENERAL MESSAGE',
    '',
    'TO:        Shelby County EOC / Logistics',
    'FROM:      WX4MTL — James, EC Shelby County',
    'DATE/TIME: 2026-06-10 14:32 UTC',
    'SUBJECT:   Shelter status — Bartlett HS',
    '',
    'MESSAGE:',
    'Bartlett HS shelter at 62 of 120 cots occupied. Generator fuel',
    'approximately 18 hours remaining. Requesting water resupply (20',
    'cases) and 6 additional cots by 1600 local. No injuries reported.',
    'Net control relaying this traffic via VARA HF to the county post',
    'office; acknowledge receipt on the 1800Z net.',
    '',
    'SIGNED: WX4MTL / MEMPHIS-ARES',
  ].join('\n'),
};

const VARA_CONFIG = { host: '127.0.0.1', cmdPort: 8300, dataPort: 8301, bandwidthHz: 2300 };
const VARA_STATUS_OPEN = { state: 'open', lastError: null, boundHost: '127.0.0.1', boundCmdPort: 8300 };
const PLATFORM_INFO = { arch: 'aarch64', os: 'linux', varaSupported: true };

// FULL/tactical identity (post multi-identity refactor the ribbon callsign comes
// from identity_active, not config.callsign).
const ACTIVE_IDENTITY = { mycall: 'W4PHS', address_as: 'W4PHS', is_tactical: false };
const IDENTITY_LIST = {
  full: [{ callsign: 'W4PHS', label: null, has_cms_account: true, cms_registered: true, needs_auth: false }],
  tactical: [],
  last_selected: 'W4PHS',
};

// ARDOP HF dock fixtures.
const ARDOP_CONFIG = {
  binary: 'ardopcf', capture_device: 'plughw:1,0', playback_device: 'plughw:1,0',
  ptt_serial_path: '/dev/ttyUSB0', cmd_port: 8515, bandwidth_hz: 2000, webgui_port: null,
};
const ARDOP_DEVICES = {
  captures: [{ name: 'plughw:1,0', description: 'USB Audio CODEC', isHardware: true }],
  playbacks: [{ name: 'plughw:1,0', description: 'USB Audio CODEC', isHardware: true }],
};
const SERIAL_DEVICES = [{ path: '/dev/ttyUSB0', kind: 'usb', label: 'CP210x USB-UART' }];
const BT_DEVICES = [{ name: 'UV-PRO 5C', address: '38:D2:00:01:55:5C' }];

// APRS tactical chat fixtures (privacy-safe). AprsConfigDto + a few inbound
// InboundMsgDto payloads injected over the `aprs-message:new` event.
const APRS_CONFIG = { sourceSsid: 7, tocall: 'APTUX1', path: 'WIDE1-1,WIDE2-1' };
const APRS_INBOUND = [
  { sender: 'K4ARC', addressee: '', text: 'Net opens 1800Z on the county VARA gateway — QNI welcome.', msgid: '42' },
  { sender: 'WX4MTL', addressee: 'W4PHS', text: 'Bartlett HS staging confirmed, ETA 1545L with 6 cots.', msgid: '17' },
  { sender: 'N4SAR', addressee: '', text: 'Welfare: Hutchins family OK, sheltering in place.', msgid: '88' },
];

let callbackId = 1;
const callbacks = new Map<number, (...args: unknown[]) => unknown>();
// event-name -> registered callback id (populated as components call listen()).
const eventHandlers = new Map<string, number>();
let eventListenerId = 1000;

/** Fire a backend event into the app, exactly as the Tauri event bus would. */
function emitTauriEvent(name: string, payload: unknown) {
  const id = eventHandlers.get(name);
  if (id == null) return;
  callbacks.get(id)?.({ event: name, id, payload });
}

const qs = (testid: string) => document.querySelector<HTMLElement>(`[data-testid="${testid}"]`);
const pinRadioPanel = () =>
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'm', ctrlKey: true, shiftKey: true, bubbles: true }));

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
      if (cmd === 'mailbox_list') {
        const folder = (args as { folder?: string } | undefined)?.folder;
        return folder === 'inbox' ? MESSAGES : [];
      }
      if (cmd === 'message_read') return MESSAGE_BODY;
      if (cmd === 'message_set_read_state') return null;
      // Active VARA HF radio dock (for the hero — shows the modem dock connected).
      if (cmd === 'config_get_vara') return VARA_CONFIG;
      if (cmd === 'vara_status') return VARA_STATUS_OPEN;
      if (cmd === 'vara_open_session') return VARA_STATUS_OPEN;
      if (cmd === 'platform_info') return PLATFORM_INFO;
      if (cmd === 'session_log_snapshot') {
        // LogLineDto[] (src/session/logProjection.ts): timestampIso/level/source/message.
        return [
          { timestampIso: '2026-06-10T14:32:14Z', level: 'info', source: 'backend', message: '*** Connecting to W4XYZ via VARA HF (2300 Hz)' },
          { timestampIso: '2026-06-10T14:32:18Z', level: 'info', source: 'transport', message: 'VARA modem bound 127.0.0.1:8300 — listening for link' },
          { timestampIso: '2026-06-10T14:32:24Z', level: 'info', source: 'transport', message: 'Link established with W4XYZ — SNR 14 dB, 1200 bps' },
          { timestampIso: '2026-06-10T14:32:31Z', level: 'info', source: 'backend', message: 'Receiving message 2 of 3 from WX4MTL@winlink.org' },
          { timestampIso: '2026-06-10T14:32:48Z', level: 'info', source: 'backend', message: '*** Session complete — 3 received, 1 sent, 41s' },
        ];
      }
      if (cmd === 'modem_get_status') return MODEM_STOPPED;
      if (cmd === 'packet_config_get') return PACKET_CONFIG;
      if (cmd === 'identity_active') return ACTIVE_IDENTITY;
      if (cmd === 'identity_list') return IDENTITY_LIST;
      if (cmd === 'config_get_ardop') return ARDOP_CONFIG;
      if (cmd === 'ardop_list_audio_devices') return ARDOP_DEVICES;
      if (cmd === 'packet_list_serial_devices') return SERIAL_DEVICES;
      if (cmd === 'packet_list_bluetooth_devices') return BT_DEVICES;
      if (cmd === 'aprs_config_get') return APRS_CONFIG;
      if (cmd === 'contacts_read') return CONTACTS;
      if (cmd === 'contacts_suggestions') return [];
      if (cmd === 'user_folders_list') return [];
      if (cmd === 'tauri_search_list_saved') return [];
      if (cmd === 'tauri_search_list_recent') return [];
      if (cmd === 'network_po_favorites_get') return [];
      if (cmd === 'catalog_list') return CATALOG;
      if (cmd === 'catalog_send_inquiry') return 'MID-README-0001';

      // Capture event subscriptions so the dock drivers can inject backend
      // events (e.g. APRS chat traffic) after the components mount.
      if (cmd === 'plugin:event|listen') {
        const a = args as { event?: string; handler?: number } | undefined;
        if (a?.event && typeof a.handler === 'number') eventHandlers.set(a.event, a.handler);
        return eventListenerId++;
      }
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

  // Optional color scheme (e.g. ?scheme=night-red, ?scheme=daylight). Applied
  // before render so the whole shell paints in the chosen palette.
  const scheme = params.get('scheme');
  if (scheme) {
    try {
      localStorage.setItem('tuxlink.colorScheme', scheme);
      const { applyColorScheme } = await import('../../src/shell/colorScheme');
      applyColorScheme(scheme as never);
    } catch {
      /* colorScheme module shape drift — ignore in the harness */
    }
  }

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
    <HarnessBoundary>
      <QueryClientProvider client={queryClient}>
        <Suspense fallback={<div data-testid="readme-harness-loading" />}>
          <AppShell />
        </Suspense>
      </QueryClientProvider>
    </HarnessBoundary>,
  );

  // Open a transport radio dock by driving the sidebar (expand the Winlink/CMS
  // accordion, then select the protocol). The proto row must exist before the
  // accordion auto-expands, so expand explicitly. dock=vara|ardop|packet.
  const dock = params.get('dock');
  const PROTO: Record<string, string> = {
    vara: 'proto-cms-vara-hf',
    ardop: 'proto-cms-ardop-hf',
    packet: 'proto-cms-packet',
  };
  if (dock && PROTO[dock]) {
    window.setTimeout(() => {
      qs('sess-cms')?.click();
      window.setTimeout(() => {
        // Selecting the protocol sets the active connection (persists in the ribbon).
        qs(PROTO[dock])?.click();
        // Pin the radio panel (Ctrl+Shift+M) so the dock stays open even with a
        // message selected, then open a message so the reading pane has content.
        window.setTimeout(() => {
          pinRadioPanel();
          window.setTimeout(() => qs('message-row-M1')?.click(), 500);
        }, 500);
      }, 600);
    }, 1500);
  }

  // APRS tactical-chat dock: toggle it open from the dashboard ribbon, mark the
  // channel listening, inject a few heard messages over the event bus, and open
  // a mailbox message so the Winlink reading pane has content — illustrating the
  // simultaneous HF Winlink + VHF APRS workspace in one window.
  if (dock === 'aprs') {
    window.setTimeout(() => {
      qs('dash-aprs-control')?.click();
      window.setTimeout(() => {
        emitTauriEvent('aprs-listening:change', true);
        APRS_INBOUND.forEach((m, i) =>
          window.setTimeout(() => emitTauriEvent('aprs-message:new', m), 250 + i * 200),
        );
        window.setTimeout(() => qs('message-row-M1')?.click(), 700);
      }, 800);
    }, 1500);
  }
}

void main().catch((err) => {
  console.error('README screenshot harness failed:', err);
  const root = document.getElementById('root');
  if (root) {
    root.textContent = err instanceof Error ? err.message : String(err);
  }
});
