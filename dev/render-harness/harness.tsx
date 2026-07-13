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
  | 'elmer' | 'sparkline' | 'ft8';
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
  const setupSlot = snapWithFlags ? (
    <Ft8SetupSurface snapshot={snapWithFlags} onStarted={() => undefined} onRetry={() => undefined} />
  ) : undefined;
  return (
    <div style={{ minHeight: '100vh', display: 'flex', flexDirection: 'column', justifyContent: 'flex-end', background: 'var(--bg)' }}>
      <LiveBandStrip
        snapshot={snapWithFlags}
        uiState={{ state, flags }}
        decodesRing={ring}
        blockingSessionMode={params.get('blocking') ?? undefined}
        setupSurface={setupSlot}
        onOpenFullSetup={() => undefined}
        nowMs={FT8_NOW_MS}
      />
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <QueryClientProvider client={queryClient}>
    {view === 'ft8' ? (
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
