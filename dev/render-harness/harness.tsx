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
// controls.css carries the shared `.tux-btn*` / `.tux-field` / `.tux-select` rules the
// Button/Select/Field wrappers emit (tuxlink-3m0vx). In the real app it loads globally via
// App.tsx; the harness doesn't mount App.tsx, so import it explicitly or every migrated
// control renders unstyled.
import '../../src/styles/controls.css';
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
// Sparkline token-migration comparison (tuxlink-ivzut): render the real
// <Sparkline> three times under variant-scoped wrappers so the CURRENT raw-hex
// candy gradient, Option A (token + color-mix subtle fade), and Option B (flat
// solid token) render side-by-side in the SAME WebKitGTK engine. The real
// Sparkline.css is untouched here — the variant backgrounds below override
// `.sparkline-bar` only inside `.spk-*` wrappers, so this stays a pre-approval
// visual probe, not the implementation.
import { Sparkline } from '../../src/radio/charts/Sparkline';
// FT-8 Station Intelligence D2 (tuxlink-b026z.4): mount LiveBandStrip per
// uiState with realistic fixtures — one PNG per state via snapshot.py. The
// strip is props-driven, so every state is drivable without a backend; the
// needs-setup/device-lost arms mount the real Ft8SetupSurface in the slot
// (its device list/meter/rig reads come from the canned shim below).
//   ?view=ft8&state=off|transitional|needs-setup|device-lost|wedged|yielded|
//                    waiting-first-slot|band-dead|decoding
//   &flags=clock|jt9    (overlay variants on any live state)
import { LiveBandStrip } from '../../src/ft8ui/LiveBandStrip';
import { Ft8SetupSurface } from '../../src/ft8ui/Ft8SetupSurface';
import { Ft8ListenerProvider } from '../../src/ft8ui/useFt8Listener';
import { StationFinderPanel } from '../../src/catalog/StationFinderPanel';
import { ContactsPanel } from '../../src/contacts/ContactsPanel';
import type {
  Ft8Snapshot,
  Ft8UiState,
  Ft8Flags,
  SlotRecord,
} from '../../src/ft8ui/ft8Types';
import type { RadioPanelMode } from '../../src/radio/types';
import type { CatalogEntry } from '../../src/catalog/types';
import type { StatusBarData } from '../../src/shell/useStatus';

const params = new URLSearchParams(location.search);
const grid = params.has('grid') ? params.get('grid') : 'CN87';
const view = (params.get('view') ?? 'home') as
  | 'home' | 'browse' | 'grib' | 'ribbon'
  | 'radio-ardop' | 'radio-vara' | 'radio-telnet'
  | 'elmer' | 'sparkline' | 'ft8' | 'finder'
  | 'contacts' | 'favorites';
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
  // --- FT-8 D2 fixtures (view=ft8). Setup surface + waterfall mount-time reads. ---
  ft8_list_devices: [
    { humanName: 'Digirig Mobile (USB Audio)', stableId: { kind: 'usb-path', value: 'usb-1.2' }, alsaHw: 'hw:1,0' },
    { humanName: 'DRA-100 (USB Audio CODEC)', stableId: { kind: 'usb-path', value: 'usb-1.3' }, alsaHw: 'hw:2,0' },
  ],
  ft8_device_meter: { rmsDbfs: -32.5, state: 'live' },
  ft8_cat_probe: { dialHz: 14074000, band: '20m' },
  ft8_waterfall_subscribe: { token: 1 },
  ft8_waterfall_unsubscribe: null,
  rig_list_models: [],
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

document.documentElement.dataset.theme = params.get('theme') ?? '';

// Sparkline regression fixture (tuxlink-ivzut). Renders the real <Sparkline>
// in both its shipped configurations so the token palette can be snapshot-
// checked in real WebKitGTK across themes (?theme=night-red exercises the
// night-vision monochrome collapse the raw-hex version used to punch through):
//   - S/N trace: warnBelow/badBelow thresholds → good/warn/bad palettes in one
//     trace, as the Signal section draws it (0–15 dB range).
//   - Throughput trace: no thresholds → all-good, as the Live section draws it.
const SN_TRACE = [
  14, 13, 12, 11, 9, 7, 5, 4, 3, 2, 4, 7, 10, 12, 14, 13, 11, 8, 5, 3, 1, 2, 5,
  8, 11, 13, 14, 12, 10, 6, 4, 2, 3, 6, 9, 12,
];
const THROUGHPUT_TRACE = [
  120, 180, 240, 300, 280, 340, 420, 480, 460, 380, 300, 260, 320, 400, 440,
  480, 500, 460, 420, 360, 300, 280, 340, 420,
];
const SPARK_FIXTURE_CSS = `
  .spk-page { padding: 24px; font-family: var(--sans); color: var(--text); }
  .spk-page h1 { font-size: 15px; margin: 0 0 4px; }
  .spk-page .sub { font-size: 12px; color: var(--text-dim); margin: 0 0 20px; }
  .spk-grid { display: grid; grid-template-columns: 150px 1fr; gap: 16px 20px; align-items: center; max-width: 720px; }
  .spk-grid .label { font-size: 12px; color: var(--text-dim); }
  .spk-grid .label b { display: block; color: var(--text); font-size: 12.5px; }
  .spk-card { background: var(--surface); border: 1px solid var(--border); border-radius: 6px; padding: 10px 12px; }
`;

function SparklineFixtureView() {
  return (
    <div className="spk-page">
      <style>{SPARK_FIXTURE_CSS}</style>
      <h1>Sparkline palette (tuxlink-ivzut)</h1>
      <p className="sub">
        Real &lt;Sparkline&gt;, same WebKitGTK engine. Theme:{' '}
        {params.get('theme') || 'default (cool-slate dark)'}.
      </p>
      <div className="spk-grid">
        <div className="label">
          <b>S/N trace</b>
          warnBelow 6 · badBelow 3
        </div>
        <div className="spk-card">
          <Sparkline samples={SN_TRACE} min={0} max={15} warnBelow={6} badBelow={3} height={48} />
        </div>

        <div className="label">
          <b>Throughput trace</b>
          no thresholds (all good)
        </div>
        <div className="spk-card">
          <Sparkline samples={THROUGHPUT_TRACE} min={0} height={48} />
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// FT-8 D2 fixtures — one falsifiable render per uiState (tuxlink-b026z.4).
// ---------------------------------------------------------------------------
const FT8_NOW_MS = 1_767_225_600_000; // fixed "now" so stripStats is deterministic

function ft8Snapshot(over: Partial<Ft8Snapshot> = {}): Ft8Snapshot {
  return {
    service: { axis: 'listening' },
    flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
    slotPhase: 'decoded',
    band: '20m',
    dialHz: 14_074_000,
    bandSource: 'cat-confirmed',
    bandLabelConfirmedUtcMs: FT8_NOW_MS - 90_000,
    sweep: { mode: 'inactive', bandIdx: null, dwellProgress: null },
    engineVersion: 'jt9 2.6.1',
    nConsecutive: 4,
    kConsecutive: 0,
    lastSlotUtcMs: FT8_NOW_MS - 15_000,
    lastFailure: null,
    availableDevices: null,
    ringTail: [],
    sweepConfig: { enabled: false, bands: ['20m'], dwellSlots: 4 },
    configuredDeviceName: 'Digirig Mobile',
    ...over,
  } as Ft8Snapshot;
}

const FT8_RING: SlotRecord[] = [0, 1, 2, 3].map((i): SlotRecord => ({
  slotUtcMs: FT8_NOW_MS - (4 - i) * 15_000,
  band: '20m',
  dialHz: 14_074_000,
  bandSource: 'cat-confirmed',
  bandLabelConfirmedUtcMs: FT8_NOW_MS - 90_000,
  outcome: { kind: 'decoded' },
  decodes: [
    { slotUtcMs: FT8_NOW_MS - (4 - i) * 15_000, snrDb: -4 - i, dtS: 0.2, freqHz: 1240 + i * 180, message: `CQ W7GTE DM34`, fromCall: 'W7GTE', toCall: null, grid: 'DM34', partial: false },
    { slotUtcMs: FT8_NOW_MS - (4 - i) * 15_000, snrDb: -13, dtS: 0.1, freqHz: 688, message: 'K5MDX N7CPZ DM43', fromCall: 'N7CPZ', toCall: 'K5MDX', grid: 'DM43', partial: false },
  ],
}));

/** Per-state snapshot + uiState (flags folded in via ?flags=). */
function ft8StateFixture(state: Ft8UiState, flags: Ft8Flags): { snapshot: Ft8Snapshot | null; ring: SlotRecord[] } {
  switch (state) {
    case 'off':
      return { snapshot: ft8Snapshot({ service: { axis: 'stopped' }, slotPhase: 'waiting-first-slot', lastSlotUtcMs: null, nConsecutive: 0 }), ring: [] };
    case 'transitional':
      return { snapshot: ft8Snapshot({ service: { axis: 'starting' }, slotPhase: 'waiting-first-slot', lastSlotUtcMs: null }), ring: [] };
    case 'needs-setup':
      return {
        snapshot: ft8Snapshot({
          service: { axis: 'blocked', reason: 'needs-device-selection' },
          slotPhase: 'waiting-first-slot',
          configuredDeviceName: null,
          availableDevices: [
            { humanName: 'Digirig Mobile (USB Audio)', stableId: { kind: 'usb-path', value: 'usb-1.2' }, alsaHw: 'hw:1,0' },
            { humanName: 'DRA-100 (USB Audio CODEC)', stableId: { kind: 'usb-path', value: 'usb-1.3' }, alsaHw: 'hw:2,0' },
          ],
        }),
        ring: [],
      };
    case 'device-lost':
      return { snapshot: ft8Snapshot({ service: { axis: 'blocked', reason: 'device-absent' }, slotPhase: 'waiting-first-slot' }), ring: FT8_RING };
    case 'wedged':
      return { snapshot: ft8Snapshot({ service: { axis: 'blocked', reason: 'capture-wedged' }, slotPhase: 'waiting-first-slot' }), ring: FT8_RING };
    case 'yielded':
      return { snapshot: ft8Snapshot({ service: { axis: 'yielded' } }), ring: FT8_RING };
    case 'waiting-first-slot':
      return { snapshot: ft8Snapshot({ slotPhase: 'waiting-first-slot', lastSlotUtcMs: null, nConsecutive: 0 }), ring: [] };
    case 'band-dead':
      return { snapshot: ft8Snapshot({ slotPhase: 'band-dead', nConsecutive: 0, kConsecutive: 9 }), ring: [] };
    case 'decoding':
    default:
      return { snapshot: ft8Snapshot(), ring: FT8_RING };
  }
}

// ---------------------------------------------------------------------------
// view=finder — the WHOLE StationFinderPanel (QA round-3 render gate). Drives
// the real panel + Ft8ListenerProvider against canned mount-time reads:
//   ?view=finder                 → map+rail+strip, listener decoding (F5/F7/F8)
//   ?view=finder&state=setup     → needs-setup → FULL-BODY setup surface (F2)
//   ?view=finder&state=<Ft8UiState> → any other listener state
// The ft8 ring is anchored at the REAL Date.now() (unlike the strip fixtures'
// fixed FT8_NOW_MS) so live decodes/min figures — the strip stats and the
// rail tab's si-count badge — render non-zero in the snapshot.
// ---------------------------------------------------------------------------
if (view === 'finder') {
  const stateParam = params.get('state') ?? 'decoding';
  const finderState = (stateParam === 'setup' ? 'needs-setup' : stateParam) as Ft8UiState;
  const nowMs = Date.now();
  const liveRing: SlotRecord[] = [0, 1, 2, 3].map((i): SlotRecord => ({
    slotUtcMs: nowMs - (4 - i) * 15_000,
    band: '20m',
    dialHz: 14_074_000,
    bandSource: 'cat-confirmed',
    bandLabelConfirmedUtcMs: nowMs - 90_000,
    outcome: { kind: 'decoded' },
    decodes: [
      { slotUtcMs: nowMs - (4 - i) * 15_000, snrDb: -4 - i, dtS: 0.2, freqHz: 1240 + i * 180, message: 'CQ W7GTE DM34', fromCall: 'W7GTE', toCall: null, grid: 'DM34', partial: false },
      { slotUtcMs: nowMs - (4 - i) * 15_000, snrDb: -13, dtS: 0.1, freqHz: 688, message: 'K5MDX N7CPZ DM43', fromCall: 'N7CPZ', toCall: 'K5MDX', grid: 'DM43', partial: false },
    ],
  }));
  const { snapshot } = ft8StateFixture(finderState, {
    clockUnsynced: false,
    jt9Degraded: false,
    catFixedBand: false,
  });
  RESPONSES.ft8_listener_snapshot = snapshot
    ? { ...snapshot, lastSlotUtcMs: nowMs - 15_000, bandLabelConfirmedUtcMs: nowMs - 90_000, ringTail: finderState === 'needs-setup' ? [] : liveRing }
    : null;
  RESPONSES.ft8_listener_start = null;

  const gw = (callsign: string, g: string, location: string, freqs: number[]) => ({
    channel: `${callsign} ${g}`,
    callsign,
    sysopName: null,
    grid: g,
    location,
    frequenciesKhz: freqs,
    lastUpdate: null,
    email: null,
    homepage: null,
    antenna: null,
  });
  RESPONSES.catalog_fetch_stations = [
    {
      mode: 'vara-hf',
      title: 'Public VARA HF RMS gateways',
      gateways: [
        gw('N0DAJ', 'DM34oa', 'Wickenburg, AZ', [3590, 7103.5, 14103.5]),
        gw('KD7SSB', 'DM33', 'Phoenix, AZ', [7101.5, 10145.5]),
        gw('K7HTZ', 'CN85', 'Portland, OR', [7104, 14105]),
      ],
      raw: '',
      parsedOk: true,
      fetchedAtMs: nowMs - 18 * 60_000,
    },
    {
      mode: 'ardop-hf',
      title: 'Public ARDOP RMS gateways',
      gateways: [gw('W7RMS', 'DM43', 'Mesa, AZ', [7102, 14105.5]), gw('N0DAJ', 'DM34oa', 'Wickenburg, AZ', [7103.5])],
      raw: '',
      parsedOk: true,
      fetchedAtMs: nowMs - 18 * 60_000,
    },
  ];
  RESPONSES.propagation_predict_path = {
    bearingDeg: 57,
    distanceKm: 1397,
    ssn: 108,
    year: 2026,
    month: 7,
    channels: [3590, 7101.5, 7102, 7103.5, 7104, 10145.5, 14103.5, 14105, 14105.5].map((frequencyKhz) => ({
      frequencyKhz,
      voacapMhz: frequencyKhz / 1000,
      relByHour: Array(24).fill(frequencyKhz < 10000 ? 0.82 : 0.51),
      snrByHour: Array(24).fill(12),
      mufdayByHour: Array(24).fill(0.9),
    })),
  };
  RESPONSES.propagation_prefs_read = {
    antenna_preset: 'low-nvis-wire',
    req_snr_db: 38,
    tx_power_w: 50,
    antenna_height_m: 2.5,
    ground_type: 'poor-rocky',
    noise_environment: 'city-industrial',
  };
  RESPONSES.propagation_prefs_write = null;
  // Peers (finding 8): ONE grid-bearing peer with a channel → a map diamond +
  // the PEERS legend row; capabilities all-on → the Peers layer pill renders.
  RESPONSES.p2p_capabilities = {
    peer_store: true,
    finder_peers: true,
    map_peers: true,
    agent_find_peers: true,
    vara_engine_split: true,
    favorites_contact_link: true,
  };
  RESPONSES.contacts_read = {
    schema_version: 2,
    contacts: [
      {
        id: 'peer-ka0zis',
        name: 'Dennis Hess',
        callsign: 'KA0ZIS',
        tier: 'confirmed',
        origin: 'incoming',
        grid: { value: 'EM17gq', source: 'manual' },
        channels: [
          {
            transport: 'vara-hf',
            target_callsign: 'KA0ZIS',
            via: [],
            freq_hz: 7101500,
            bandwidth: null,
            direction: 'incoming',
            counts: { ok: 2, fail: 0 },
            last_seen: new Date(nowMs - 3600_000).toISOString(),
            last_ok: new Date(nowMs - 3600_000).toISOString(),
          },
        ],
        endpoints: [],
        created_at: new Date(nowMs - 86_400_000).toISOString(),
        updated_at: new Date(nowMs - 3600_000).toISOString(),
      },
    ],
    groups: [],
  };
  RESPONSES.position_current_fix = { grid: 'DM33wp' };
  RESPONSES.position_status = {
    gps_ready: false,
    broadcast_grid: 'DM33wp',
    ui_grid: 'DM33wp',
  };
  RESPONSES.backend_status = null;
  RESPONSES.magnetic_declination = { declDeg: 9.7, modelEpoch: 'WMM2025', validUntil: '2029-01-01' };
  RESPONSES.basemap_list_packs = { packs: [], total_bytes: 0 };
  RESPONSES.catalog_get_service_codes = 'PUBLIC';
  RESPONSES.catalog_set_service_codes = null;
  RESPONSES.wwv_offair_snapshot_read = null;
  RESPONSES.wwv_offair_cat_configured = true;
  // Seed the persisted map viewport (Arizona at regional zoom) so gateway
  // pins spread out instead of stacking under the operator dot at world
  // zoom. `?worldview=1` skips the seed for a first-open render.
  if (params.get('worldview') !== '1') {
    try {
      localStorage.setItem(
        'tuxlink:map-viewport:station-finder',
        JSON.stringify({ center: { lat: 33.4, lon: -112.0 }, zoom: 7 }),
      );
    } catch {
      /* storage disabled — the world-zoom fallback still renders */
    }
  }
}

function FinderFixtureView() {
  return (
    <Ft8ListenerProvider>
      <StationFinderPanel onClose={() => undefined} />
    </Ft8ListenerProvider>
  );
}

// ---------------------------------------------------------------------------
// view=contacts / view=favorites — the Contacts outline + the Favorites panel
// on ONE representative dataset (tuxlink-sbf03 consolidation pass): groups,
// grouped + ungrouped contacts across tiers/origins, heard-but-unconfirmed
// suggestions, starred favorites + recents, and a shared contact↔favorite
// link — so the current visual (in)coherence of the three surfaces renders
// side-by-side for critique.
// ---------------------------------------------------------------------------
if (view === 'contacts' || view === 'favorites') {
  const nowMs = Date.now();
  const iso = (agoMs: number) => new Date(nowMs - agoMs).toISOString();
  const ch = (
    transport: string,
    target: string,
    freqHz: number | null,
    dir: string,
    ok: number,
    fail: number,
    agoMs: number,
    okAgoMs: number | null,
  ) => ({
    transport,
    target_callsign: target,
    via: [],
    freq_hz: freqHz,
    bandwidth: null,
    direction: dir,
    counts: { ok, fail },
    last_seen: iso(agoMs),
    last_ok: okAgoMs === null ? null : iso(okAgoMs),
  });
  const contact = (
    id: string,
    callsign: string,
    name: string,
    over: Record<string, unknown> = {},
  ) => ({
    id,
    callsign,
    name,
    tier: 'confirmed',
    origin: 'manual',
    grid: null,
    channels: [],
    endpoints: [],
    created_at: iso(30 * 86_400_000),
    updated_at: iso(86_400_000),
    ...over,
  });
  RESPONSES.contacts_read = {
    schema_version: 2,
    contacts: [
      contact('c-ka0zis', 'KA0ZIS', 'Dennis Hess', {
        grid: { value: 'EM17gq', source: 'manual' },
        origin: 'incoming',
        channels: [ch('vara-hf', 'KA0ZIS', 7101500, 'incoming', 3, 1, 3600_000, 3600_000)],
      }),
      contact('c-n0daj', 'N0DAJ', 'Doug Jarmuth', {
        grid: { value: 'DM34oa', source: 'manual' },
        channels: [
          ch('vara-hf', 'N0DAJ', 7103500, 'outgoing', 5, 2, 7200_000, 7200_000),
          ch('ardop', 'N0DAJ', 7103500, 'outgoing', 1, 4, 86_400_000, 259_200_000),
        ],
      }),
      contact('c-w7gte', 'W7GTE', '', {
        tier: 'unconfirmed',
        origin: 'incoming',
        grid: { value: 'DM34', source: 'aprs' },
        channels: [ch('packet', 'W7GTE', 145710000, 'incoming', 2, 0, 1800_000, 1800_000)],
      }),
      contact('c-k7htz', 'K7HTZ', 'Portland Gateway', {
        grid: { value: 'CN85', source: 'manual' },
        channels: [ch('vara-hf', 'K7HTZ', 14105000, 'outgoing', 0, 3, 43_200_000, null)],
      }),
      contact('c-smtp', 'SMTP:cameronzucker@gmail.com', 'Cameron (email)', {}),
      contact('c-kj7abc', 'KJ7ABC', 'Riley Park', {
        grid: { value: 'DM33', source: 'manual' },
      }),
    ],
    groups: [
      {
        id: 'g-ares',
        name: 'ARES Net',
        members: [
          { type: 'contact', contact_id: 'c-n0daj' },
          { type: 'contact', contact_id: 'c-kj7abc' },
          { type: 'raw', callsign: 'W7XYZ' },
        ],
        created_at: iso(20 * 86_400_000),
        updated_at: iso(2 * 86_400_000),
      },
      {
        id: 'g-family',
        name: 'Family',
        members: [{ type: 'contact', contact_id: 'c-smtp' }],
        created_at: iso(20 * 86_400_000),
        updated_at: iso(10 * 86_400_000),
      },
    ],
  };
  RESPONSES.contacts_suggestions = [
    { callsign: 'K5MDX', message_count: 4 },
    { callsign: 'N7CPZ-1', message_count: 2 },
  ];
  const fav = (
    id: string,
    mode: string,
    gateway: string,
    freq: string,
    starred: boolean,
    over: Record<string, unknown> = {},
  ) => ({
    id,
    mode,
    gateway,
    freq,
    starred,
    created_at: iso(20 * 86_400_000),
    updated_at: iso(86_400_000),
    ...over,
  });
  RESPONSES.favorites_read = {
    schema_version: 1,
    favorites: [
      fav('f-1', 'vara-hf', 'N0DAJ', '7103.5', true, { band: '40m', grid: 'DM34oa', contact_id: 'c-n0daj', last_attempt_at: iso(7200_000) }),
      fav('f-2', 'vara-hf', 'K7HTZ', '14105.0', true, { band: '20m', grid: 'CN85', last_attempt_at: iso(43_200_000) }),
      fav('f-3', 'ardop-hf', 'W7RMS', '7102.0', true, { band: '40m', grid: 'DM43' }),
      fav('f-4', 'telnet', 'CMS', '', true, { transport: 'CmsSsl' }),
    ],
    log: [
      { unit_id: 'f-1', ts_local: new Date(nowMs - 7200_000).toISOString(), freq: '7103.5', outcome: 'reached' },
      { unit_id: 'f-2', ts_local: new Date(nowMs - 43_200_000).toISOString(), freq: '14105.0', outcome: 'failed' },
    ],
  };
  RESPONSES.favorites_recents = [
    fav('r-1', 'vara-hf', 'KD7SSB', '7101.5', false, { band: '40m', grid: 'DM33', last_attempt_at: iso(10_800_000) }),
    fav('r-2', 'packet', 'W7GTE-10', '145.710', false, { last_attempt_at: iso(1800_000) }),
  ];
  RESPONSES.position_current_fix = { grid: 'DM33wp' };
}

function ContactsFixtureView() {
  return (
    <div className="layout-b" style={{ height: '100vh' }}>
      <div style={{ height: '100%', display: 'flex' }}>
        <ContactsPanel />
      </div>
    </div>
  );
}

function FavoritesFixtureView() {
  // tuxlink-sbf03: Favorites is a SCOPE of ContactsPanel now (FavoritesPanel
  // retired) — this view renders the pseudo-folder's exact mount.
  return (
    <div className="layout-b" style={{ height: '100vh' }}>
      <div style={{ height: '100%', display: 'flex' }}>
        <ContactsPanel initialScope="favorites" onConnectFavorite={() => undefined} />
      </div>
    </div>
  );
}

function Ft8StripFixtureView() {
  const state = (params.get('state') ?? 'decoding') as Ft8UiState;
  const flagsParam = params.get('flags');
  const flags: Ft8Flags = {
    clockUnsynced: flagsParam === 'clock',
    jt9Degraded: flagsParam === 'jt9',
    catFixedBand: params.get('catfixed') === '1',
  };
  const { snapshot, ring } = ft8StateFixture(state, flags);
  const snapWithFlags = snapshot
    ? { ...snapshot, flags, lastFailure: flags.jt9Degraded ? 'jt9 exited 137 (SIGKILL) — decode timeout' : snapshot.lastFailure }
    : null;
  // QA round-3 finding 2: the strip no longer nests Ft8SetupSurface — the
  // full surface renders standalone below the strip here so the D2 per-state
  // fixtures still cover BOTH the strip arm and the surface itself. In the
  // product it is StationFinderPanel's full BODY (view=finder&state=setup).
  const setupStandalone =
    (state === 'needs-setup' || state === 'device-lost') && snapWithFlags ? (
      <Ft8SetupSurface snapshot={snapWithFlags} onStarted={() => undefined} onRetry={() => undefined} />
    ) : null;
  return (
    <div style={{ minHeight: '100vh', display: 'flex', flexDirection: 'column', justifyContent: 'flex-end', background: 'var(--bg)' }}>
      {setupStandalone}
      <LiveBandStrip
        snapshot={snapWithFlags}
        uiState={{ state, flags }}
        decodesRing={ring}
        blockingSessionMode={params.get('blocking') ?? undefined}
        onOpenFullSetup={() => undefined}
        nowMs={FT8_NOW_MS}
      />
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <QueryClientProvider client={queryClient}>
    {view === 'contacts' ? (
      <ContactsFixtureView />
    ) : view === 'favorites' ? (
      <FavoritesFixtureView />
    ) : view === 'finder' ? (
      <FinderFixtureView />
    ) : view === 'ft8' ? (
      <Ft8StripFixtureView />
    ) : view === 'sparkline' ? (
      <SparklineFixtureView />
    ) : view === 'ribbon' ? (
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
          // QA round-3 finding 4 render: drive the FT-8 chip's raw uiState via
          // ?ft8state= (e.g. yielded → "Paused", needs-setup → "Needs setup").
          // Default 'decoding' keeps the pre-existing ribbon snapshot lively.
          ft8={{
            uiState: (params.get('ft8state') ?? 'decoding') as Ft8UiState,
            band: '20m',
            decodesPerMin: 4.5,
            onOpen: () => undefined,
          }}
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
