// WebKitGTK render harness for the Request Center and DashboardRibbon
// (diagnostic scratch — NOT shipped).
//
// Mounts components in a plain browser/WebKitGTK context by shimming the
// Tauri v2 IPC (window.__TAURI_INTERNALS__.invoke) with canned responses, so the
// real component + CSS render in the exact WebKitGTK engine Tauri uses, with no
// Rust build and no menu-driving. Drive which view / grid via URL query:
//   ?grid=CN87        4-char grid (working case)
//   ?grid=CN87uo      6-char grid (reproduces the gridToLatLon null gap)
//   ?grid=            no grid ("Location not set")
//   ?view=home|browse|grib|ribbon
import React from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '../../src/App.css';
// AppShell.css carries the ribbon's `.layout-b .dashboard .dash-*` rules; without
// it (and the `.layout-b` wrapper below) DashboardRibbon renders unstyled. Only
// this file is needed — StatusBar.css has no dash-* rules and compactShell.css is
// narrow-viewport overrides that would distort the desktop ribbon snapshot.
import '../../src/shell/AppShell.css';
import { RequestCenter } from '../../src/request/RequestCenter';
import { DashboardRibbon } from '../../src/shell/DashboardRibbon';
// Radio panes (tuxlink-zj9se): mount each pane standalone in the dock layout so
// its CSS (RadioPanel.css + per-mode + section CSS, imported by the components
// themselves) renders in the real WebKitGTK engine for the token-migration
// before/after diff. The panes default to disconnected/empty state under the
// shim; deeper live-data sections degrade gracefully (null/STOPPED defaults).
import { ArdopRadioPanel } from '../../src/radio/modes/ArdopRadioPanel';
import { VaraRadioPanel } from '../../src/radio/modes/VaraRadioPanel';
import { TelnetRadioPanel } from '../../src/radio/modes/TelnetRadioPanel';
// Elmer model-access picker (tuxlink-wpqwy): mount ElmerPane with an
// onboarded=false config shim so the ModelTilePicker renders as the first-run
// surface (tier headers, tiles, GetKeyCard) for the WebKitGTK layout smoke.
import { ElmerPane } from '../../src/elmer/ElmerPane';
import type { RadioPanelMode } from '../../src/radio/types';
import type { CatalogEntry } from '../../src/catalog/types';
import type { StatusBarData } from '../../src/shell/useStatus';

const params = new URLSearchParams(location.search);
const grid = params.has('grid') ? params.get('grid') : 'CN87';
const view = (params.get('view') ?? 'home') as
  | 'home' | 'browse' | 'grib' | 'ribbon'
  | 'radio-ardop' | 'radio-vara' | 'radio-telnet'
  | 'elmer';
// ?running=1 drives a connected modem / open VARA transport so the running-state
// footers render: ARDOP/VARA `Send/Receive` (primary) + the red `Stop`
// (`radio-panel-btn-bad`) button. Without it the fixture pins state to STOPPED, so
// ONLY the `Start` button is reachable and a footer review covers half the button
// set (the tuxlink-ppnui review gap). Telnet renders Start+Stop unconditionally.
const running = params.get('running') === '1';

// Representative catalog: zone forecast + radar for CN87uo (Seattle), EASTPAC
// marine entries, propagation, and gateway lists — enough categories that the
// full location hero, browse, and chip grids populate realistically.
// Use ?grid=CN87uo to exercise the exact-zone hero at City of Seattle / WAZ315.
const CATALOG: CatalogEntry[] = [
  // Zone forecast for CN87uo → WAZ315 "City of Seattle"
  { category: 'WX_US_WA', filename: 'WA_ZON_SEA', description: 'City of Seattle Washington Zone Forecast', size_bytes: 2500 },
  { category: 'WX_US_WA', filename: 'WA_FCST', description: 'Washington — state forecast (NWS)', size_bytes: 4200 },
  // Radar for Puget Sound & SJDF (resolves for CN87/CN87uo)
  { category: 'WX_US_RAD', filename: 'US.RAD.PSND', description: 'SNAPSHOT CURRENT RADAR U.S. PUGET SOUND & SJDF', size_bytes: 20799 },
  // Marine: EASTPAC entries
  { category: 'WX_EASTPAC', filename: 'EPAC_HIGH', description: 'NE Pacific — high seas forecast', size_bytes: 9100 },
  { category: 'WX_EASTPAC', filename: 'EPAC_COASTAL', description: 'NE Pacific — coastal waters', size_bytes: 7300 },
  { category: 'PROPAGATION', filename: 'PROP_3DAY', description: '3-day HF propagation outlook', size_bytes: 1800 },
  { category: 'PROPAGATION', filename: 'PROP_WWV', description: 'WWV solar-terrestrial summary', size_bytes: 900 },
  { category: 'PROPAGATION', filename: 'AUR_TONIGHT', description: 'Aurora Forecast Tonight', size_bytes: 900 },
  { category: 'WL2K_RMS', filename: 'PUB_VARA', description: 'Public VARA HF RMS gateways', size_bytes: 11000 },
  { category: 'WL2K_RMS', filename: 'PUB_ARDOP', description: 'Public ARDOP RMS gateways', size_bytes: 9000 },
  { category: 'INQUIRIES', filename: 'INQUIRIES', description: 'Catalog inquiries help & how-to', size_bytes: 2200 },
  { category: 'BULLETINS', filename: 'B_ARRL', description: 'ARRL bulletins', size_bytes: 3400 },
];

const RESPONSES: Record<string, unknown> = {
  config_read: {
    grid,
    review_inbound_before_download: false,
    // Telnet pane reads host + transport from config_read.
    host: 'cms.winlink.org',
    transport: 'CmsSsl',
  },
  catalog_list: CATALOG,
  catalog_send_inquiry: 'MID-TEST-0001',
  // Elmer model-access picker (tuxlink-wpqwy): onboarded=false → ElmerPane shows
  // the ModelTilePicker as the first-run surface. key-status map empty (no saved
  // keys) so no badges; the layout smoke is about tiers/tiles/no-h-scroll.
  elmer_config_read: {
    agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
    agentModel: '',
    keyStatus: 'absent',
    agentTurnTimeoutSecs: 900,
    onboarded: false,
  },
  elmer_key_status_for_origins: {},
  // DashboardRibbon's GridEdit write paths:
  config_set_grid: grid,
  position_set_source: null,
  // --- Radio-pane mount-time reads (tuxlink-zj9se). Representative
  //     disconnected/empty state; action calls (cms_connect, modem_*_connect,
  //     vara_open_session, config_set_*, favorite_*, identity_*) fire only on
  //     click and may reject harmlessly. ModemStatus uses the STOPPED shape. ---
  modem_get_status: running
    ? {
        // Connected (ISS) so `!isStopped` → Send/Receive + red Stop + Open WebGUI
        // render, and the SIGNAL sparklines have real samples to draw.
        state: 'connected-iss', peer: 'W7RMS', mode: 'ARQ', widthHz: 500, pttBackend: 'cat',
        snDb: 12, vuDbfs: -18, throughputBps: 480,
        bytesRx: 4096, bytesTx: 2048, uptimeSec: 42,
        arqFlags: { busy: true, rx: false, tx: true },
        lastError: null, quality: 88, rigFreqHz: 14105000,
      }
    : {
        state: 'stopped', peer: null, mode: null, widthHz: null, pttBackend: null,
        snDb: null, vuDbfs: null, throughputBps: null,
        bytesRx: 0, bytesTx: 0, uptimeSec: 0,
        arqFlags: { busy: false, rx: false, tx: false },
        lastError: null, quality: null, rigFreqHz: null,
      },
  platform_info: { arch: 'aarch64', os: 'linux', varaSupported: true },
  vara_status: running
    ? { state: 'open', lastError: null, boundHost: '127.0.0.1', boundCmdPort: 8300 }
    : { state: 'closed', lastError: null, boundHost: null, boundCmdPort: null },
  config_get_vara: { host: '127.0.0.1', cmd_port: 8300, data_port: 8301, bandwidth_hz: null },
  // Representative ArdopFullConfig so the default-open Radio & audio section
  // renders real values (an empty {} makes String(c.cmd_port) print "undefined").
  config_get_ardop: {
    binary: 'ardopcf',
    capture_device: '',
    playback_device: '',
    ptt_method: 'vox',
    ptt_serial_path: null,
    cat_key_cmd: 'TX1;',
    cat_unkey_cmd: 'TX0;',
    cat_bridge_port: 4532,
    cmd_port: 8515,
    bandwidth_hz: null,
    webgui_port: null,
    connect_attempts: 5,
    listen_ttl_minutes: 0,
  },
  config_get_rig: {},
  ardop_list_audio_devices: [],
  packet_list_serial_devices: [],
  identity_active: null,
  identity_list: [],
  // favorites_read returns the whole StationsFile (object, NOT an array) — the
  // hook does data?.favorites.filter(...), so .favorites must exist.
  favorites_read: { schema_version: 1, favorites: [], log: [] },
  favorites_recents: [],
  session_log_snapshot: [],
  // Tauri event API: listen() routes through invoke('plugin:event|listen') and
  // resolves to an event id; unlisten through 'plugin:event|unlisten'. Resolve
  // both so useModemStatus's listen() effect settles (the stream never emits, so
  // the modem holds its STOPPED default — a valid disconnected render).
  'plugin:event|listen': 0,
  'plugin:event|unlisten': null,
  // AllowedStationsEditor fetches the allow-list on mount (per transport). Default
  // allow_all=true (project convention allowed-stations-default-true).
  ardop_allowed_stations_get: { allow_all: true, callsigns: [], ips: [] },
  vara_allowed_stations_get: { allow_all: true, callsigns: [], ips: [] },
  packet_allowed_stations_get: { allow_all: true, callsigns: [], ips: [] },
  telnet_allowed_stations_get: { allow_all: true, callsigns: [], ips: [] },
};

// Tauri v2 routes invoke() through window.__TAURI_INTERNALS__.invoke(cmd, args).
(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  invoke: (cmd: string) =>
    new Promise((resolve, reject) => {
      if (cmd in RESPONSES) setTimeout(() => resolve(RESPONSES[cmd]), 0);
      else reject(new Error(`harness: no canned response for '${cmd}'`));
    }),
  transformCallback: (cb: unknown) => cb,
};

// Realistic fixture data for the ribbon view (operator-verified callsign N7CPZ).
const ribbonData: StatusBarData = {
  callsign: 'N7CPZ',
  grid: grid ?? null,
  gridTooltip: null,
  state: { label: 'Idle', tone: 'idle' },
  connection: 'Idle · CMS-SSL',
  position_source: 'Gps',
  // GPS has a usable fix. Without this, GridEdit shows source=Gps && !gpsReady →
  // the verbose "GPS no fix · broadcasting fallback ▸ Set manually" affordance
  // (the widest-case grid cell, tuxlink-813d), which overflowed the cell and
  // jumbled the ribbon snapshot. A GPS-with-fix station is the realistic default
  // for this audience and keeps the radius-review render clean (tuxlink-ppnui).
  gpsReady: true,
};

const queryClient = new QueryClient();

// Radio panes mount inside the dock layout (`.radio-panel` is a direct child of
// `.panes--with-dock`, the wrapper that places it in the 4th grid column).
const VARA_MODE: RadioPanelMode = { kind: 'vara-hf', intent: 'cms' };

// Drive the ribbon's connecting state via ?connecting=1 so the Abort control
// (rendered only while connecting) can be snapshotted for the token diff.
const ribbonConnecting = params.get('connecting') === '1';

createRoot(document.getElementById('root')!).render(
  <QueryClientProvider client={queryClient}>
    {view === 'ribbon' ? (
      <div className="layout-b">
        {/* The real app wraps DashboardRibbon in `.ribbon-with-search` beside a
            `.search-zone` (flex 0 1 560px). That wrapper is what gives `.dashboard`
            its true `flex:1 1 auto; min-width:0` context (AppShell.css:204) so its
            cells shrink + ellipsize. Mounting `.dashboard` bare under `.layout-b`
            (the prior harness) gave it the FULL window width with the default
            `min-width:auto`, so the flex items couldn't shrink and the
            GPS-fallback / clock / connection cells overlapped — reproduced as
            jumbled text in the review snapshots. Reproduce the wrapper so the
            snapshot reflects the shipped layout (tuxlink-ppnui). The `<SearchBar>`
            itself isn't needed for layout fidelity — only its flex basis is. */}
        <div className="ribbon-with-search">
          <div className="search-zone">
            <input
              className="harness-search-stub"
              placeholder="Search mail…"
              readOnly
              style={{ width: '100%', background: 'transparent', border: 0, color: 'inherit' }}
            />
          </div>
          <DashboardRibbon
          data={ribbonData}
          onConnect={() => undefined}
          connecting={ribbonConnecting}
          onAbort={() => undefined}
          // Render the full set of non-enumerated dash-* controls (tuxlink-zj9se
          // migration targets): the Review|Download seg, the APRS control + unread
          // badge, and the merged Elmer×Agent-send chip in its armed state.
          reviewInbound={true}
          onReviewInboundChange={() => undefined}
          aprs={{ listening: true, unread: 3, onOpen: () => undefined }}
          egress={{
            status: { armed: true, armedRemainingSecs: 540, tainted: false },
            onArm: () => undefined,
            onDisarm: () => undefined,
          }}
          onOpenElmer={() => undefined}
          elmerOpen={false}
          />
        </div>
      </div>
    ) : view.startsWith('radio-') ? (
      <div className="layout-b">
        <div className="panes panes--with-dock">
          {view === 'radio-ardop' && (
            <ArdopRadioPanel onClose={() => undefined} onFindGateway={() => undefined} />
          )}
          {view === 'radio-vara' && (
            <VaraRadioPanel mode={VARA_MODE} onClose={() => undefined} onFindGateway={() => undefined} />
          )}
          {view === 'radio-telnet' && <TelnetRadioPanel onClose={() => undefined} />}
        </div>
      </div>
    ) : view === 'elmer' ? (
      // ElmerPane renders the ModelTilePicker in place of the message list when
      // the (shimmed) config reports onboarded=false. Mount it full-viewport so
      // the single-column min(420px,92vw) layout + the picker's scroll-within-
      // .elmer-messages behavior render as shipped for the narrow-width smoke.
      <div style={{ height: '100vh', display: 'flex', flexDirection: 'column' }}>
        <ElmerPane onClose={() => undefined} />
      </div>
    ) : (
      <RequestCenter initialView={view} onClose={() => undefined} />
    )}
  </QueryClientProvider>,
);
