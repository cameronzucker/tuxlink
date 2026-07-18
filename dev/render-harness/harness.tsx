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
import React, { useEffect, useRef, useState } from 'react';
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
// tuxlink-fh53x (?state=stop4dock): the real drawer->panel chain, so the tour's
// relocated 'radio-dock' anchor is layout-verified in WebKitGTK — the drawer is
// display:contents on desktop (zero rect), the panel root is the boxed anchor.
import { RadioDrawer } from '../../src/shell/RadioDrawer';
import { RadioPanel } from '../../src/radio/RadioPanel';
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
// Routines operator UI (tuxlink-3awm9): the post-PR-#1118 WebKitGTK smoke.
// RoutinesSurface is the inline full-pane view-switch AppShell mounts in
// place of the mailbox panes; ConsentGate is the always-mounted Part 97
// consent modal. Both are driven here purely off canned runs_list/journal
// reads — ConsentGate's own launch-recovery path builds its parks from
// those, so no Tauri event synthesis is needed.
import { RoutinesSurface } from '../../src/routines/RoutinesSurface';
import type { RoutinesView, DesignerTab } from '../../src/routines/RoutinesSurface';
import { ConsentGate } from '../../src/routines/ConsentGate';
import { ContactsPanel } from '../../src/contacts/ContactsPanel';
// Onboarding first-run tour fixtures (tuxlink-10bkw Task 7): the REAL
// HintProvider/HintOverlay/OfferCard mounted over a fake-but-realistic shell
// anchor layout built from the REAL DashboardRibbon/FolderSidebar/MessageList
// (each already carries the production `data-tour-anchor` the tour targets).
import { HintProvider, useHints } from '../../src/onboarding/HintProvider';
import { HintOverlay } from '../../src/onboarding/HintOverlay';
import { OfferCard } from '../../src/onboarding/OfferCard';
import { FolderSidebar } from '../../src/mailbox/FolderSidebar';
import { MessageList } from '../../src/mailbox/MessageList';
import { MessageViewEmpty } from '../../src/mailbox/MessageViewEmpty';
import { useViewport } from '../../src/shell/useViewport';
import type {
  Ft8Snapshot,
  Ft8UiState,
  Ft8Flags,
  SlotRecord,
} from '../../src/ft8ui/ft8Types';
import type { RadioPanelMode } from '../../src/radio/types';
import type { CatalogEntry } from '../../src/catalog/types';
import type { StatusBarData } from '../../src/shell/useStatus';
import type { MessageMeta } from '../../src/mailbox/types';
// Dockable surfaces (tuxlink-dmwte task 11, spec §4/§5/§10): the REAL popped
// window shell + the docked-side pathway affordances, smoked on WebKitGTK.
//   ?view=pop-routines | pop-tacmap | pop-aprschat   — the popped OS window
//   ?view=vacated-routines | vacated-tacmap | vacated-aprschat — main-shell traces
//   ?view=header-routines | header-tacmap | header-aprschat — docked ↗ affordances
import { PoppedSurfaceHost } from '../../src/dock/PoppedSurfaceHost';
import type { SurfaceId } from '../../src/dock/dockState';
import { MenuBar } from '../../src/shell/chrome/MenuBar';
import { AprsDockTabs } from '../../src/aprs/AprsDockTabs';
import { AprsChatPanel } from '../../src/aprs/AprsChatPanel';
import type { AprsConfigDto, ChannelMessage, HeardPosition } from '../../src/aprs/aprsTypes';
// AprsPositionsMap (mounted inside the popped Tac Map surface) renders through
// Leaflet; its stylesheet is otherwise pulled in only by LeafletMap.tsx, which
// this harness does not mount. Import it directly so the popped map's controls
// + tile pane are styled exactly as shipped.
import 'leaflet/dist/leaflet.css';

const params = new URLSearchParams(location.search);
const grid = params.has('grid') ? params.get('grid') : 'CN87';
const view = (params.get('view') ?? 'home') as
  | 'home' | 'browse' | 'grib' | 'ribbon'
  | 'radio-ardop' | 'radio-vara' | 'radio-telnet'
  | 'elmer' | 'sparkline' | 'ft8' | 'finder'
  | 'contacts' | 'favorites' | 'onboarding' | 'routines'
  | 'pop-routines' | 'pop-tacmap' | 'pop-aprschat'
  | 'vacated-routines' | 'vacated-tacmap' | 'vacated-aprschat'
  | 'header-routines' | 'header-tacmap' | 'header-aprschat';

// tuxlink-dmwte task 11: the dockable-surfaces fixture families. `ROUTINES_FAMILY`
// gates the big routines-data block below (the popped/header/vacated Routines
// fixtures reuse the plan-5 routines_* canned reads); `DOCK_FAMILY` gates the
// dock-surface shim additions (dock_state_get, aprs/packet/position reads, the
// snapshot-handshake host answers).
const ROUTINES_FAMILY =
  view === 'routines' || view === 'pop-routines' || view === 'header-routines' || view === 'vacated-routines';
const DOCK_FAMILY = view.startsWith('pop-') || view.startsWith('vacated-') || view.startsWith('header-');
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
    // Onboarding first-run tour (tuxlink-10bkw Task 7, view=onboarding):
    // HintProvider seeds from these two on mount. false/[] reproduces a
    // fresh install — the offer card is the config-loaded default when the
    // tour has never completed. Harmless extra fields for every other view.
    onboarding_tour_completed: false,
    onboarding_tips_seen: [] as string[],
  },
  // HintProvider's whole-section persistence write (skipTour/declineOffer/
  // dismissSingle) — resolves with no return value, matching the real
  // Tauri command's `()` result.
  config_set_onboarding: null,
  // HintProvider's point-at ack (view=onboarding&state=pointat replays the
  // real 'onboarding:point-at' event — see capturedEventHandlers below —
  // which fires this on the 'shown' path).
  onboarding_point_at_ack: null,
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

// Captured `listen()` registrations, keyed by event name (onboarding fixture,
// view=onboarding&state=pointat). `transformCallback` below is the identity
// function, so @tauri-apps/api/event's listen() passes its OWN handler
// closure straight through as invoke('plugin:event|listen', {handler}) —
// see node_modules/@tauri-apps/api/event.js `listen()`. Capturing it here
// lets a fixture synthesize the backend event a `listen()` call is waiting
// on (e.g. HintProvider's 'onboarding:point-at') with no real Tauri event
// bus. Harmless for every other view: they never look this map up.
//
// A Set per event (not a single slot): a popped surface can have two live
// subscribers to the SAME event — e.g. the popped APRS window mounts BOTH the
// chat panel's `useAprsChat` and the status strip's `useAprsChat`, each
// listening 'aprs-chat:snapshot'. A single-slot map would seed only the last
// registrant and leave the other empty (tuxlink-dmwte task 11).
type CapturedHandler = (event: { payload: unknown }) => void;
const capturedEventHandlers = new Map<string, Set<CapturedHandler>>();

/** Synthesize a backend event to every live subscriber (see above). */
function deliverEvent(name: string, payload: unknown): void {
  const set = capturedEventHandlers.get(name);
  if (!set) return;
  for (const handler of set) handler({ payload });
}

// The popped-window label `getCurrentWindow()` resolves to (spec §3 wire
// table). PopTitleBar reads `__TAURI_INTERNALS__.metadata.currentWindow.label`
// at mount via getCurrentWindow(); without it the pop fixtures throw before
// first paint. The min/max handlers only fire on click, so the label just has
// to exist and be plausible.
const popWindowLabel =
  view === 'pop-tacmap' ? 'pop-tacmap' : view === 'pop-aprschat' ? 'pop-aprschat' : view === 'pop-routines' ? 'pop-routines' : 'main';

// Tauri v2 routes invoke() through window.__TAURI_INTERNALS__.invoke(cmd, args).
(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  metadata: { currentWindow: { label: popWindowLabel } },
  invoke: (cmd: string, args?: Record<string, unknown>) =>
    new Promise((resolve, reject) => {
      if (cmd === 'plugin:event|listen' && typeof args?.event === 'string' && typeof args.handler === 'function') {
        const ev = args.event;
        let set = capturedEventHandlers.get(ev);
        if (!set) {
          set = new Set<CapturedHandler>();
          capturedEventHandlers.set(ev, set);
        }
        set.add(args.handler as CapturedHandler);
        resolve(0);
        return;
      }
      if (cmd === 'plugin:event|emit' && typeof args?.event === 'string') {
        // Snapshot-handshake host answer (spec §7): the harness plays the main
        // shell, so a freshly-popped CLIENT window (Tac Map / APRS Chat) that
        // requests a seed roster gets a populated feed back — the same
        // request/answer path `useEnvStations` already ships. Without this the
        // pop fixtures render an empty map + empty chat and the crush cases
        // (long feed, dense pins) never appear.
        if (args.event === 'aprs-positions:request-snapshot') {
          setTimeout(() => deliverEvent('aprs-positions:snapshot', SEED_POSITIONS), 0);
        } else if (args.event === 'aprs-chat:request-snapshot') {
          setTimeout(() => deliverEvent('aprs-chat:snapshot', SEED_CHAT), 0);
        }
        resolve(null);
        return;
      }
      if (cmd in RESPONSES) {
        // A function value is an args-aware fixture (routines_get by name,
        // routines_journal by runId, …) — call it; anything else is the
        // canned literal, unchanged behavior for every existing view.
        const canned = RESPONSES[cmd];
        setTimeout(
          () => resolve(typeof canned === 'function' ? (canned as (a?: Record<string, unknown>) => unknown)(args) : canned),
          0,
        );
      } else reject(new Error(`harness: no canned response for '${cmd}'`));
    }),
  transformCallback: (cb: unknown) => cb,
};

// ---------------------------------------------------------------------------
// Dockable surfaces (tuxlink-dmwte task 11) — seed rosters + shim reads.
//
// SEED_* are delivered as the host's snapshot-handshake answer (see the emit
// branch above) so a popped Tac Map / APRS Chat window renders a live map +
// populated feed — the crush cases (long feed rows, dense pins) that an empty
// fixture would never surface. Run/fix data is realistic for the operator's
// Seattle-area net (callsigns match the routines fixtures).
// ---------------------------------------------------------------------------
const DOCK_NOW_MS = Date.now();
const SEED_POSITIONS: HeardPosition[] = [
  { call: 'N0DAJ-9', lat: 47.6205, lon: -122.3493, symbolTable: '/', symbolCode: '>', comment: 'Mobile — I-5 NB', at: DOCK_NOW_MS - 42_000, ambiguity: 0, via: [] },
  { call: 'KD7SSB', lat: 47.6512, lon: -122.3010, symbolTable: '/', symbolCode: '-', comment: 'Home QTH', at: DOCK_NOW_MS - 138_000, ambiguity: 0, via: [] },
  { call: 'W7RMS-1', lat: 47.6039, lon: -122.3301, symbolTable: '/', symbolCode: '#', comment: 'Capitol Hill digi', at: DOCK_NOW_MS - 305_000, ambiguity: 0, via: [] },
  { call: 'N7CPZ-7', lat: 47.5990, lon: -122.3350, symbolTable: '/', symbolCode: '[', comment: 'HT — walking', at: DOCK_NOW_MS - 20_000, ambiguity: 1, via: [] },
];
const SEED_CHAT: ChannelMessage[] = [
  { id: '27', direction: 'in', from: 'N0DAJ', to: null, text: 'Net starts 1900 local on the 2m machine — check in early.', kind: 'message', msgid: '27', at: DOCK_NOW_MS - 640_000 },
  { id: '28', direction: 'in', from: 'KD7SSB', to: 'N7CPZ', text: 'Copy — will check in from the north end of the county.', kind: 'message', msgid: '28', at: DOCK_NOW_MS - 470_000 },
  { id: 'b12', direction: 'out', from: 'N7CPZ', to: null, text: 'Good morning net — N7CPZ mobile, monitoring the channel.', kind: 'message', msgid: 'b12', at: DOCK_NOW_MS - 300_000, state: 'acked', ackedAt: DOCK_NOW_MS - 296_000 },
  { id: '29', direction: 'in', from: 'W7RMS', to: null, text: 'QSL, 59 into Seattle. Digi on Capitol Hill is hot today.', kind: 'message', msgid: '29', at: DOCK_NOW_MS - 118_000 },
  { id: 'd3', direction: 'out', from: 'N7CPZ', to: 'KD7SSB', text: 'See you at the ARES meeting Thursday.', kind: 'message', msgid: 'd3', at: DOCK_NOW_MS - 28_000, state: 'sent' },
];

if (DOCK_FAMILY) {
  // The dock registry snapshot PoppedSurfaceHost consumes at mount (spec §3).
  // The named surface is `popped`; the Routines context carries the continuity
  // token (spec §7) so a pop-routines&rview=designer fixture opens the designer
  // with the token's view, exactly as a real pop-from-designer would.
  const dockRoutineParam = params.get('routine') ?? '';
  const routinesTokenState =
    params.get('rview') === 'designer'
      ? { view: { view: 'designer', routine: dockRoutineParam, tab: params.get('rtab') ?? 'design' } }
      : { view: { view: 'dashboard' } };
  const popped = (s: string) => (view === `pop-${s}` || view === `vacated-${s}` ? 'popped' : 'docked');
  RESPONSES.dock_state_get = {
    surfaces: { routines: popped('routines'), tac_map: popped('tacmap'), aprs_chat: popped('aprschat') },
    context: { routines: { foreground: false, state: routinesTokenState }, tac_map: null, aprs_chat: null },
  };
  // Surface-hook mount-time reads (all disconnected/idle — action calls fire
  // only on click and reject harmlessly).
  RESPONSES.aprs_config_get = { sourceSsid: 7, tocall: 'APTUX0', path: 'WIDE1-1,WIDE2-1' } satisfies AprsConfigDto;
  RESPONSES.aprs_config_set = null;
  RESPONSES.packet_config_get = {
    ssid: 7,
    listenDefault: true,
    linkKind: 'Serial',
    tcpHost: null,
    tcpPort: null,
    serialDevice: '/dev/ttyUSB0',
    serialBaud: 9600,
    btMac: null,
  };
  RESPONSES.packet_config_set = null;
  RESPONSES.aprs_listen_start = null;
  RESPONSES.aprs_listen_stop = null;
  RESPONSES.uvpro_connect = null;
  RESPONSES.uvpro_disconnect = null;
  RESPONSES.backend_status = null;
  RESPONSES.position_status = {
    gps_state: 'BroadcastAtPrecision',
    position_source: 'Gps',
    broadcast_grid: grid ?? '',
    ui_grid: grid ?? '',
    active_connection: null,
  };
  RESPONSES.basemap_list_packs = { packs: [], total_bytes: 0 };
}

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

// ?theme=light maps to the app's real light preset. The app never calls a
// scheme "light" — colorScheme.ts's PresetScheme id is 'daylight' ("Daylight
// (light)", mode:'light'); applyColorScheme() just does
// `root.dataset.theme = scheme`, so stamping 'daylight' directly reproduces
// exactly what Settings → Appearance does. Every other ?theme= value (e.g.
// night-red, used by other views) passes through unchanged.
const themeParam = params.get('theme');
document.documentElement.dataset.theme = themeParam === 'light' ? 'daylight' : (themeParam ?? '');

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

// ---------------------------------------------------------------------------
// view=onboarding — the REAL HintProvider + HintOverlay + OfferCard mounted
// over a fake-but-realistic shell anchor layout (tuxlink-10bkw Task 7). Every
// anchor except 'find-a-station' is the REAL production element (imported
// from src/): DashboardRibbon carries `ribbon-connect` + `elmer`,
// FolderSidebar carries `contacts`, MessageList carries `mailbox` + the new
// `compose` toolbar button. 'find-a-station' has no persistent chrome in the
// real app (its anchor is the whole StationFinderPanel, opened from a
// message-menu action) — the harness-only stand-in below exists solely so
// `?state=tip` has something to spotlight.
//
//   ?view=onboarding&state=offer     first-run offer card, bottom-right
//   ?view=onboarding&state=stop1     tour step 1 — ribbon-connect spotlight
//   ?view=onboarding&state=stop4     tour step 4 — radio-dock NOT mounted →
//                                    fallback:'skip' auto-advances to stop 5
//                                    (Elmer, centered); tuxlink-fh53x replaced
//                                    the old 'center' fallback that rendered a
//                                    contentless card about an absent surface
//   ?view=onboarding&state=stop4dock tour step 4 with the dock MOUNTED (real
//                                    RadioDrawer->RadioPanel chain) → real
//                                    spotlight on the panel's boxed root
//   ?view=onboarding&state=tip       discretionary tip on find-a-station
//   ?view=onboarding&state=pointat   backend point-at, single-hint mode, on
//                                    ribbon-connect
//   &theme=light                    the app's real 'daylight' preset
//
// The offer/tour/tip/point-at states are reached by driving the REAL
// HintProvider state machine (startTour/advance/declineOffer/
// requestFirstOpenTip) and — for point-at — replaying the REAL
// 'onboarding:point-at' Tauri event through the captured `listen()` handler
// (see capturedEventHandlers above), rather than reaching into HintProvider's
// internals. Tour advances are EVENT-DRIVEN: each `advance()` waits for the
// effect to observe the reducer's new stepIndex before issuing the next
// (see OnboardingFixtureController). Fixed-interval chains are load-fragile:
// HintProvider's "latest ref" (stateRef, mutated once per render) must have
// observed the PRIOR dispatch before the next call reads it, and a busy
// machine can delay a render past any fixed spacing (tuxlink-fh53x).
// ---------------------------------------------------------------------------

const ONBOARDING_MESSAGES: MessageMeta[] = [
  {
    id: 'ob-1',
    subject: 'Net check-in reminder',
    from: 'W7RMS',
    to: ['N7CPZ'],
    date: new Date(Date.now() - 3_600_000).toISOString(),
    unread: true,
    bodySize: 812,
    hasAttachments: false,
    preview: 'Reminder: Tuesday net at 1900Z on 7103.5.',
  },
  {
    id: 'ob-2',
    subject: 'RE: propagation outlook',
    from: 'K7HTZ',
    to: ['N7CPZ'],
    date: new Date(Date.now() - 86_400_000).toISOString(),
    unread: true,
    bodySize: 2100,
    hasAttachments: true,
    preview: '3-day HF outlook attached — 20m looking good after 1400Z.',
  },
  {
    id: 'ob-3',
    subject: 'Welcome to the ARES net',
    from: 'N0DAJ',
    to: ['N7CPZ'],
    date: new Date(Date.now() - 3 * 86_400_000).toISOString(),
    unread: false,
    bodySize: 640,
    hasAttachments: false,
    preview: 'Glad to have you aboard — see you Tuesday.',
  },
];

/** Imperatively drives HintProvider's real state machine to the fixture's
 *  requested `?state=`, then renders nothing itself — HintOverlay/OfferCard
 *  (siblings under the same HintProvider) render the result.
 *
 *  Tour states (stop1/stop4/stop4dock) drive EVENT-DRIVEN, not on a fixed
 *  timer: each `advance()` is issued only after the effect observes the
 *  reducer's new `active.stepIndex` (tuxlink-fh53x). The old fixed-40ms chain
 *  assumed a render commits between calls; on a loaded machine renders lag,
 *  the "latest ref" reads a stale stepIndex, and the tour lands anywhere
 *  (observed: stuck at stop 1, or driven past Finish to completion). Note
 *  this driver relies on every stop BELOW the target having a mounted
 *  anchor — a skip-fallback stop before the target would race the overlay's
 *  own auto-advance and can overshoot; our fixtures mount all of them. */
function OnboardingFixtureController({ state }: { state: string }) {
  const hints = useHints();
  const startedRef = useRef(false);
  const tourDoneRef = useRef(false);
  const tourTarget =
    state === 'stop1' ? 0 : state === 'stop4' || state === 'stop4dock' ? 3 : null; // TOUR_STOPS[3] === 'radio-dock' (1-indexed stop 4)
  useEffect(() => {
    // Waits for config_read to resolve — HintProvider's config-loaded action
    // sets `active: {kind:'offer'}` (the canned onboarding_tour_completed is
    // false, reproducing a fresh install). Before that resolves, active is
    // the reducer's safe initial `null` and there is nothing to drive yet.
    if (!startedRef.current) {
      if (hints.active === null) return;
      startedRef.current = true;
      if (tourTarget !== null) {
        hints.startTour();
        return;
      }
      switch (state) {
        case 'tip':
          hints.declineOffer(); // clears the offer so requestFirstOpenTip sees active===null
          setTimeout(() => hints.requestFirstOpenTip('find-a-station'), 40);
          break;
        case 'pointat':
          hints.declineOffer();
          setTimeout(() => {
            deliverEvent('onboarding:point-at', { request_id: 1, anchor_id: 'ribbon-connect' });
          }, 40);
          break;
        default:
          // 'offer' (or an unrecognized state): the offer card is already the
          // config-loaded default — nothing further to drive.
          break;
      }
      return;
    }
    // Tour states, post-start: one advance per OBSERVED step change until the
    // target stop is reached.
    if (tourTarget === null || tourDoneRef.current) return;
    if (hints.active?.kind !== 'tour') return;
    if (hints.active.stepIndex >= tourTarget) {
      tourDoneRef.current = true;
      return;
    }
    const t = setTimeout(() => hints.advance(), 40);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hints.active, state]);
  return null;
}

/** The fake-but-realistic shell frame the tour anchors live in: ribbon strip
 *  (Connect + Elmer), sidebar (Contacts row), mailbox rows-pane (+ Compose),
 *  reading pane. All REAL components; `useViewport` drives `compact` exactly
 *  as AppShell does, so a narrow render (1024×768) reflows the same way the
 *  shipped shell would (subject to the FZ-M1 rail's own `any-pointer: coarse`
 *  gate — a non-touch WebKitGTK render stays in desktop mode at any width,
 *  same as every other harness view). */
function OnboardingShell({ state }: { state: string }) {
  const { isCompact } = useViewport();
  // tuxlink-fh53x: stop4dock mounts the dock exactly as AppShell does — the
  // `.panes--with-dock` 4-column grid, then RadioDrawer (display:contents on
  // desktop) wrapping a RadioPanel whose boxed root hosts the relocated
  // 'radio-dock' tour anchor. This is the layout jsdom cannot verify (no
  // layout engine): the spotlight only works if the anchor's rect is nonzero
  // through the display:contents chain in the real WebKitGTK engine.
  const dockMounted = state === 'stop4dock';
  return (
    <div className="layout-b" style={{ height: '100vh' }}>
      {/* `.layout-b`'s CSS is a fixed 5-row grid (titlebar/menubar/
          ribbon-with-search/panes/statusbar — see AppShell.css). This
          fixture mounts only 2 of those 5 rows (no titlebar/menubar/
          statusbar chrome), and neither `.ribbon-with-search` nor `.panes`
          carries an explicit `grid-row` in AppShell.css — production relies
          on ALL FIVE siblings being present so DOM-order auto-placement
          lands them correctly. Pin the rows explicitly so `.panes` actually
          gets the 1fr track (full remaining height) instead of collapsing
          to an `auto` (content-height) track vacated by the missing
          titlebar/menubar rows. */}
      {/* No `.ribbon-with-search`/`.search-zone` wrapper here (contrast
          view=ribbon above): that wrapper's `.search-zone` reserves a
          560px-shrinking-to-240px flex basis, which starves `.dashboard`'s
          own `overflow:hidden` row once its content — even this fixture's
          minimal Callsign/Grid/clock/Connection/Elmer/Connect set — plus
          the reserved search width exceeds 1366px, clipping the
          right-pinned Connect button (`margin-left:auto`) off-canvas. This
          fixture has no SearchBar to reproduce and no long GPS-fallback/
          packet text to guard against (see the 'ribbon' view's own comment
          on THAT overlap risk) — mounting `.dashboard` bare gives it the
          full row width, which is what the flagship ribbon-connect spotlight
          renders need. */}
      <div style={{ gridRow: 3 }}>
        <DashboardRibbon
          data={ribbonData}
          onConnect={() => undefined}
          connecting={false}
          onOpenElmer={() => undefined}
          elmerOpen={false}
        />
      </div>
      <div className={`panes${dockMounted ? ' panes--with-dock' : ''}`} style={{ gridRow: 4 }}>
        <FolderSidebar
          compact={isCompact}
          selectedFolder="inbox"
          onSelectFolder={() => undefined}
          counts={{ inbox: 2 }}
          contactsCount={6}
          favoritesCount={4}
        />
        <MessageList
          folder="inbox"
          messages={ONBOARDING_MESSAGES}
          selectedId={null}
          onSelect={() => undefined}
          onCompose={() => undefined}
        />
        <MessageViewEmpty />
        {dockMounted && (
          <RadioDrawer open onToggle={() => undefined} sessionState="disconnected">
            <RadioPanel mode={{ kind: 'telnet', intent: 'cms' }} onClose={() => undefined}>
              <div style={{ padding: 12, color: 'var(--text-dim)' }}>
                Canned panel body — the tour spotlights the panel chrome, not this content.
              </div>
            </RadioPanel>
          </RadioDrawer>
        )}
      </div>
      {/* Harness-only stand-in (?state=tip only): the real 'find-a-station'
          anchor is the whole StationFinderPanel, opened via a message-menu
          action rather than persistent chrome, so there is nothing
          lightweight to mount here. Fixed position — independent of the
          shell's grid/flex so it never perturbs the real components' layout;
          z-index below the overlay's panels/blocker (1200+) so the spotlight
          hole still paints correctly around it. */}
      {state === 'tip' && (
        <button
          type="button"
          className="hint-overlay__btn"
          data-tour-anchor="find-a-station"
          style={{ position: 'fixed', left: 24, bottom: 24, zIndex: 50 }}
        >
          🔎 Find a station
        </button>
      )}
    </div>
  );
}

function OnboardingFixtureView() {
  const state = params.get('state') ?? 'offer';
  return (
    <HintProvider>
      <OnboardingShell state={state} />
      <OnboardingFixtureController state={state} />
      <HintOverlay />
      <OfferCard />
    </HintProvider>
  );
}

// ---------------------------------------------------------------------------
// view=routines — the plan-5 operator UI (tuxlink-3awm9 WebKitGTK smoke).
// PR #1118 shipped the whole surface having only ever rendered in
// vitest/jsdom; this fixture mounts the REAL RoutinesSurface (dashboard +
// designer canvas/palette/inspector/runs/settings) and the REAL ConsentGate
// in the production engine, in AppShell's exact frame (ribbon row above,
// surface in the 1fr panes row — same explicit grid-row pinning as
// view=onboarding, and same reason).
//
//   ?view=routines                                  → dashboard, populated
//   ?view=routines&empty=1                          → dashboard, empty library
//   ?view=routines&rview=designer&routine=<name>
//        &rtab=design|runs|settings                 → designer on <name>
//   ?view=routines&rview=designer&routine=          → fresh unsaved draft
//   ?view=routines&consent=1                        → ConsentGate modal over
//        the dashboard, TWO parks (the "1 of N" pip), recovered from the
//        canned runs_list/journal exactly like a cold launch would.
//
// Wire casing per routinesApi.ts's header table: RoutineDef/Track/Step/
// Trigger/Finding/JournalEntry as-written snake_case; RoutineSummary/
// ScheduleStatus/NextFire/RunListEntry/RunStatus/ActionInfo/RadioPreset/
// StationSet camelCase; RunEvent tags snake_case.
// ---------------------------------------------------------------------------
if (ROUTINES_FAMILY) {
  const nowS = Math.floor(Date.now() / 1000);
  const emptyLibrary = params.get('empty') === '1';
  const consentMode = params.get('consent') === '1';
  // The popped Routines host ALWAYS mounts ConsentGate (spec §6), which auto-
  // modals over the oldest parked run. `&park=0` drops the awaiting_consent
  // runs so the dashboard/designer chrome renders unobscured — the modal-up
  // state is its own fixture (default pop-routines, or &consent=1).
  const suppressParks = params.get('park') === '0';

  // --- Definitions (snake_case; the export format IS the storage format) ---
  const DEF_MORNING = {
    routine: 'Morning Winlink Check',
    schema_version: 1,
    transmit_mode: 'attended',
    on_interrupted: 'resume',
    triggers: [
      { type: 'schedule', every: '30m', align: 'hour', window: '07:00-09:00', if_missed: 'run_once_on_launch' },
    ],
    tracks: [
      {
        name: 'main',
        steps: [
          { id: 's1', action: 'rig.apply_preset', params: { preset: '@preset:40m-vara' }, timeout_s: 30 },
          { id: 's2', action: 'radio.connect', params: { target: 'N0DAJ', freq_hz: 7103500 }, timeout_s: 180, on_radio_busy: 'wait' },
          { id: 's3', action: 'local.notify', params: { message: 'Morning check complete' } },
        ],
      },
    ],
  };
  const DEF_BEACON = {
    routine: 'APRS Position Beacon',
    schema_version: 1,
    transmit_mode: 'automatic',
    transmit_ack: { by: 'N7CPZ', at: '2026-07-10T16:20:00Z' },
    triggers: [{ type: 'schedule', every: '15m', align: null, window: null, if_missed: 'skip' }],
    tracks: [
      {
        name: 'main',
        steps: [{ id: 'b1', action: 'radio.aprs_send', params: { comment: 'Tuxlink routine beacon' }, timeout_s: 60 }],
      },
    ],
  };
  // The control-flow showcase: branch arms, a retry, a delay, an end — the
  // canvas's hardest render (task-10/11's review rounds were all here).
  const DEF_PREAMBLE = {
    routine: 'Net Preamble',
    schema_version: 1,
    transmit_mode: 'attended',
    inputs: [{ name: 'net_name', required: true }],
    triggers: [{ type: 'manual' }],
    tracks: [
      {
        name: 'main',
        steps: [
          { id: 'p1', action: 'local.compose', params: { template: 'preamble' } },
          { id: 'p2', control: 'branch', on: 'connected', then: ['p3'], else: ['p4', 'p5'] },
          { id: 'p3', action: 'radio.connect', params: { target: 'W7RMS' }, timeout_s: 120 },
          { id: 'p4', control: 'retry', step: 'p3', attempts: 3, backoff_s: 30 },
          { id: 'p5', control: 'delay', delay: '2m' },
          { id: 'p6', control: 'end', failed: false },
        ],
      },
    ],
  };
  const DEFS: Record<string, unknown> = {
    'Morning Winlink Check': DEF_MORNING,
    'APRS Position Beacon': DEF_BEACON,
    'Net Preamble': DEF_PREAMBLE,
  };

  const SUMMARIES = emptyLibrary
    ? []
    : [
        { routine: 'Morning Winlink Check', transmitMode: 'attended', enabled: true, triggers: DEF_MORNING.triggers },
        { routine: 'APRS Position Beacon', transmitMode: 'automatic', enabled: true, triggers: DEF_BEACON.triggers },
        { routine: 'Net Preamble', transmitMode: 'attended', enabled: false, triggers: DEF_PREAMBLE.triggers },
      ];

  const FINDINGS_BY_ROUTINE: Record<string, unknown[]> = {
    'Morning Winlink Check': [],
    'APRS Position Beacon': [],
    'Net Preamble': [
      {
        code: 'unreachable_step',
        severity: 'warning',
        routine: 'Net Preamble',
        track: 'main',
        step: 'p5',
        message: 'Step p5 is reachable only through the else arm; the 2m delay may exceed the manual-run window',
      },
    ],
  };

  // --- Runs. One awaiting_consent park is ALWAYS present (the dashboard's
  //     live rail + badge render it); consent=1 adds a second park so the
  //     modal's "1 of N" pip renders. Newest terminal run of the beacon is a
  //     FAILURE so the dashboard's failure-cause line (journal-fetched,
  //     cached) renders too. ---
  const RUNS = [
    { runId: 'run-1768456705-0007', routine: 'Morning Winlink Check', dryRun: false, startedUnix: nowS - 95, state: 'awaiting_consent', finishedUnix: null },
    ...(consentMode
      ? [{ runId: 'run-1768456770-0008', routine: 'Net Preamble', dryRun: false, startedUnix: nowS - 30, state: 'awaiting_consent', finishedUnix: null }]
      : []),
    { runId: 'run-1768456780-0009', routine: 'Net Preamble', dryRun: true, startedUnix: nowS - 20, state: 'running', finishedUnix: null },
    { runId: 'run-1768453200-0003', routine: 'Morning Winlink Check', dryRun: false, startedUnix: nowS - 3600, state: 'completed', finishedUnix: nowS - 3540 },
    { runId: 'run-1768449600-0001', routine: 'APRS Position Beacon', dryRun: false, startedUnix: nowS - 7200, state: 'completed', finishedUnix: nowS - 7180 },
    { runId: 'run-1768455900-0006', routine: 'APRS Position Beacon', dryRun: false, startedUnix: nowS - 900, state: 'failed', finishedUnix: nowS - 880 },
    { runId: 'run-1768370400-0002', routine: 'Net Preamble', dryRun: false, startedUnix: nowS - 86400, state: 'cancelled', finishedUnix: nowS - 86300 },
  ].filter((r) => !emptyLibrary && !(suppressParks && r.state === 'awaiting_consent'));

  const j = (runId: string, seq: number, agoS: number, event: unknown) => ({
    ts_unix: nowS - agoS,
    run_id: runId,
    seq,
    event,
  });
  const JOURNALS: Record<string, unknown[]> = {
    'run-1768456705-0007': [
      j('run-1768456705-0007', 0, 95, { type: 'run_started', routine: 'Morning Winlink Check', snapshot: DEF_MORNING, dry_run: false }),
      j('run-1768456705-0007', 1, 95, { type: 'state_changed', state: 'running' }),
      j('run-1768456705-0007', 2, 94, { type: 'step_intent', step: 's1', action: 'rig.apply_preset', resolved_params: { preset: '40m-vara' } }),
      j('run-1768456705-0007', 3, 92, { type: 'step_ok', step: 's1', output: { applied: true } }),
      j('run-1768456705-0007', 4, 91, { type: 'step_intent', step: 's2', action: 'radio.connect', resolved_params: { target: 'N0DAJ', freq_hz: 7103500 } }),
      j('run-1768456705-0007', 5, 90, { type: 'state_changed', state: 'awaiting_consent' }),
    ],
    'run-1768456770-0008': [
      j('run-1768456770-0008', 0, 30, { type: 'run_started', routine: 'Net Preamble', snapshot: DEF_PREAMBLE, dry_run: false }),
      j('run-1768456770-0008', 1, 30, { type: 'state_changed', state: 'running' }),
      j('run-1768456770-0008', 2, 29, { type: 'step_intent', step: 'p3', action: 'radio.connect', resolved_params: { target: 'W7RMS' } }),
      j('run-1768456770-0008', 3, 28, { type: 'state_changed', state: 'awaiting_consent' }),
    ],
    'run-1768456780-0009': [
      j('run-1768456780-0009', 0, 20, { type: 'run_started', routine: 'Net Preamble', snapshot: DEF_PREAMBLE, dry_run: true }),
      j('run-1768456780-0009', 1, 20, { type: 'state_changed', state: 'running' }),
      j('run-1768456780-0009', 2, 19, { type: 'step_intent', step: 'p1', action: 'local.compose', resolved_params: { template: 'preamble' } }),
      j('run-1768456780-0009', 3, 18, { type: 'step_ok', step: 'p1', output: null }),
    ],
    'run-1768453200-0003': [
      j('run-1768453200-0003', 0, 3600, { type: 'run_started', routine: 'Morning Winlink Check', snapshot: DEF_MORNING, dry_run: false }),
      j('run-1768453200-0003', 1, 3598, { type: 'step_intent', step: 's1', action: 'rig.apply_preset', resolved_params: { preset: '40m-vara' } }),
      j('run-1768453200-0003', 2, 3595, { type: 'step_ok', step: 's1', output: { applied: true } }),
      j('run-1768453200-0003', 3, 3590, { type: 'step_intent', step: 's2', action: 'radio.connect', resolved_params: { target: 'N0DAJ' } }),
      j('run-1768453200-0003', 4, 3560, { type: 'step_ok', step: 's2', output: { connected: true } }),
      j('run-1768453200-0003', 5, 3540, { type: 'run_finished', state: 'completed', reason: null }),
    ],
    'run-1768449600-0001': [
      j('run-1768449600-0001', 0, 7200, { type: 'run_started', routine: 'APRS Position Beacon', snapshot: DEF_BEACON, dry_run: false }),
      j('run-1768449600-0001', 1, 7185, { type: 'step_ok', step: 'b1', output: null }),
      j('run-1768449600-0001', 2, 7180, { type: 'run_finished', state: 'completed', reason: null }),
    ],
    'run-1768455900-0006': [
      j('run-1768455900-0006', 0, 900, { type: 'run_started', routine: 'APRS Position Beacon', snapshot: DEF_BEACON, dry_run: false }),
      j('run-1768455900-0006', 1, 899, { type: 'step_intent', step: 'b1', action: 'radio.aprs_send', resolved_params: { comment: 'Tuxlink routine beacon' } }),
      j('run-1768455900-0006', 2, 885, {
        type: 'step_err',
        step: 'b1',
        error: { kind: 'action', detail: { action: 'radio.aprs_send', cause: 'PTT keying failed: rig not responding on /dev/ttyUSB0' } },
      }),
      j('run-1768455900-0006', 3, 880, { type: 'run_finished', state: 'failed', reason: 'step b1 failed' }),
    ],
    'run-1768370400-0002': [
      j('run-1768370400-0002', 0, 86400, { type: 'run_started', routine: 'Net Preamble', snapshot: DEF_PREAMBLE, dry_run: false }),
      j('run-1768370400-0002', 1, 86310, { type: 'state_changed', state: 'cancelled' }),
      j('run-1768370400-0002', 2, 86300, { type: 'run_finished', state: 'cancelled', reason: 'operator cancelled' }),
    ],
  };

  // The real registry's 17 actions (src-tauri/src/routines/actions/*.rs),
  // flags per each module's descriptor intent: radio.* transmit (listen is
  // RX-only), rig.* need the radio but never key it, data.* fetch over the
  // internet, local.* are inert.
  const ACTIONS = [
    { name: 'local.log', needsRadio: false, transmits: false, needsInternet: false },
    { name: 'local.notify', needsRadio: false, transmits: false, needsInternet: false },
    { name: 'local.compose', needsRadio: false, transmits: false, needsInternet: false },
    { name: 'local.compose_catalog_request', needsRadio: false, transmits: false, needsInternet: false },
    { name: 'local.set_identity', needsRadio: false, transmits: false, needsInternet: false },
    { name: 'data.read', needsRadio: false, transmits: false, needsInternet: false },
    { name: 'data.spacewx_swpc', needsRadio: false, transmits: false, needsInternet: true },
    { name: 'data.spacewx_wwv', needsRadio: true, transmits: false, needsInternet: false },
    { name: 'data.stationlist_update', needsRadio: false, transmits: false, needsInternet: true },
    { name: 'radio.connect', needsRadio: true, transmits: true, needsInternet: false },
    { name: 'radio.listen', needsRadio: true, transmits: false, needsInternet: false },
    { name: 'radio.aprs_send', needsRadio: true, transmits: true, needsInternet: false },
    { name: 'rig.apply_preset', needsRadio: true, transmits: false, needsInternet: false },
    { name: 'rig.read_state', needsRadio: true, transmits: false, needsInternet: false },
    { name: 'rig.switch_vfo', needsRadio: true, transmits: false, needsInternet: false },
    { name: 'rig.tune_atu', needsRadio: true, transmits: true, needsInternet: false },
    { name: 'rig.validate_preset', needsRadio: false, transmits: false, needsInternet: false },
  ];

  RESPONSES.routines_list = SUMMARIES;
  RESPONSES.routines_get = (args?: Record<string, unknown>) => DEFS[String(args?.name)] ?? DEF_MORNING;
  RESPONSES.routines_validate = (args?: Record<string, unknown>) => FINDINGS_BY_ROUTINE[String(args?.name)] ?? [];
  RESPONSES.routines_validate_draft = [];
  RESPONSES.routines_missed_fires = emptyLibrary
    ? []
    : [
        {
          routine: 'Morning Winlink Check',
          missed: 2,
          lastFireUnix: nowS - 5400,
          lastRefusal: { at: nowS - 1800, reason: 'radio busy: ARDOP session active' },
          lastSkip: null,
        },
      ];
  RESPONSES.routines_next_fires = emptyLibrary
    ? []
    : [
        { routine: 'Morning Winlink Check', at: nowS + 1260 },
        { routine: 'APRS Position Beacon', at: nowS + 340 },
      ];
  RESPONSES.routines_fleet_check = emptyLibrary
    ? []
    : [
        {
          code: 'schedule_overlap',
          severity: 'warning',
          routine: 'APRS Position Beacon',
          track: null,
          step: null,
          message: 'Beacon fires inside Morning Winlink Check’s 07:00–09:00 window; both need the radio',
        },
      ];
  RESPONSES.routines_actions_list = ACTIONS;
  RESPONSES.routines_runs_list = (args?: Record<string, unknown>) =>
    typeof args?.routine === 'string' && args.routine !== '' ? RUNS.filter((r) => r.routine === args.routine) : RUNS;
  RESPONSES.routines_run_status = (args?: Record<string, unknown>) => {
    const run = RUNS.find((r) => r.runId === args?.runId);
    return run ? { runId: run.runId, routine: run.routine, dryRun: run.dryRun, state: run.state } : null;
  };
  RESPONSES.routines_journal = (args?: Record<string, unknown>) => JOURNALS[String(args?.runId)] ?? [];
  RESPONSES.routines_presets_list = [
    { name: '40m-vara', frequencyHz: 7103500, mode: 'VARA', powerW: 30, atu: true },
    { name: '20m-ardop', frequencyHz: 14105000, mode: 'ARDOP', powerW: 50, atu: false },
  ];
  RESPONSES.routines_station_sets_list = [{ name: 'ares-net', callsigns: ['N0DAJ', 'KD7SSB', 'W7RMS'] }];
}

function RoutinesFixtureView() {
  const routineParam = params.get('routine') ?? '';
  const initial: RoutinesView =
    params.get('rview') === 'designer'
      ? { view: 'designer', routine: routineParam, tab: (params.get('rtab') ?? 'design') as DesignerTab }
      : { view: 'dashboard' };
  // Real state so snap-click.py can drive dashboard→designer navigation the
  // way AppShell's setRoutinesView would.
  const [rv, setRv] = useState<RoutinesView>(initial);
  return (
    <div className="layout-b" style={{ height: '100vh' }}>
      {/* Same explicit grid-row pinning as OnboardingShell (see its comment):
          only 2 of .layout-b's 5 rows are mounted, so without the pins the
          surface would auto-place into an `auto` track and collapse. */}
      <div style={{ gridRow: 3 }}>
        <DashboardRibbon
          data={ribbonData}
          onConnect={() => undefined}
          connecting={false}
          onOpenElmer={() => undefined}
          elmerOpen={false}
        />
      </div>
      <div style={{ gridRow: 4, minHeight: 0 }}>
        <RoutinesSurface view={rv} onNavigate={setRv} />
      </div>
      {/* ConsentGate mounts only under ?consent=1: the canned data always
          carries an awaiting_consent run (the dashboard's live rail must
          render it), and an always-mounted gate would modal over every other
          snapshot — in the real app it would too; the param just picks which
          truth this PNG tells. */}
      {params.get('consent') === '1' && <ConsentGate />}
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

// ===========================================================================
// Dockable surfaces (tuxlink-dmwte task 11, spec §4/§5/§10)
// ===========================================================================

// Family 1 — the popped OS window: the REAL PoppedSurfaceHost per surface. It
// fills the viewport itself (`.pop-surface-host { height: 100vh }`), so no
// wrapper. The Tauri-IPC shim above feeds dock_state_get / the routines reads /
// the snapshot-handshake answers, so this is the shipped window, not a mock.
//
// HintProvider mirrors App.tsx's pop-branch wrapper (App.tsx renders every
// /pop/<surface> webview inside one): AprsChatPanel calls useFirstOpenTip('aprs'),
// whose useHints() throws with no provider. Without this the popped APRS Chat
// window renders BLANK — the defect this smoke exists to catch (tuxlink-dmwte
// task 11). Wrapping here keeps the fixture faithful to the fixed shipped tree.
function PopSurfaceFixtureView({ surface }: { surface: SurfaceId }) {
  return (
    <HintProvider>
      <PoppedSurfaceHost surface={surface} />
    </HintProvider>
  );
}

// A representative right-dock column: the vacated-slot + docked-header dock
// affordances live in AppShell's fixed-width right dock, and their crush cases
// (spec §10) only reproduce at that bounded width. 400px mirrors the shipped
// dock column (AprsDockTabs.css documents the "~400px dock width"); the border
// makes the bound visible in the shot.
function DockColumn({ children, label }: { children: React.ReactNode; label: string }) {
  return (
    <div style={{ minHeight: '100vh', background: 'var(--bg)', padding: 24, boxSizing: 'border-box' }}>
      <div style={{ fontSize: 12, color: 'var(--text-faint)', marginBottom: 8, fontFamily: 'sans-serif' }}>{label}</div>
      <div
        style={{
          width: 400,
          border: '1px solid var(--border)',
          borderRadius: 8,
          overflow: 'hidden',
          display: 'flex',
          flexDirection: 'column',
          background: 'var(--surface)',
        }}
      >
        {children}
      </div>
    </div>
  );
}

// Family 2 — vacated-slot main-shell traces (spec §5). Each is the REAL
// affordance component in its popped-surface state.
function VacatedFixtureView() {
  if (view === 'vacated-routines') {
    // The Routines menu reads "Routines ↗" and the dropdown gains "Dock
    // Routines back". Closed shows the pathway label; snap-click.py on
    // [data-tour-anchor="menu:routines"] opens the dropdown for the item shot.
    return (
      <div style={{ minHeight: '100vh', background: 'var(--bg)' }}>
        <div className="tux-titlebar" style={{ borderBottom: '1px solid var(--border)' }}>
          <span className="tux-app-name">Tuxlink</span>
        </div>
        <MenuBar onAction={() => undefined} badges={{ routines: 1 }} dockPopped />
      </div>
    );
  }
  if (view === 'vacated-tacmap') {
    // The map toggle slot SWAPS to the "Tac Map ↗ — in window" pathway + a
    // "⇤ dock back" action (AprsDockTabs, mapPopped).
    return (
      <DockColumn label="APRS dock — Tac Map popped (vacated slot)">
        <AprsDockTabs
          active="aprs"
          unread={3}
          modemEnabled
          stationCount={4}
          onSelect={() => undefined}
          onClose={() => undefined}
          mapPopped
          onFocusMap={() => undefined}
          onDockBackMap={() => undefined}
        />
      </DockColumn>
    );
  }
  // vacated-aprschat: the dock keeps its other tabs; the APRS tab body is the
  // focus-me placeholder. This mirrors AppShell.tsx:2501-2526 (the placeholder
  // is inline JSX there, not a component) so its `.aprs-chat-popped-*` classes
  // — from AppShell.css, already imported — render on real WebKitGTK.
  return (
    <DockColumn label="APRS dock — APRS Chat popped (vacated tab body)">
      <AprsDockTabs
        active="aprs"
        unread={0}
        modemEnabled
        stationCount={4}
        onSelect={() => undefined}
        onClose={() => undefined}
        mapOpen={false}
        onToggleMap={() => undefined}
      />
      <div className="aprs-chat-popped-placeholder" data-testid="aprs-chat-popped-placeholder">
        <button type="button" className="aprs-chat-popped-focus" title="Focus the APRS Chat window">
          <span className="aprs-chat-popped-title">APRS Chat ↗ — in its own window</span>
          <span className="aprs-chat-popped-sub">click to focus</span>
        </button>
        <button type="button" className="aprs-chat-popped-dockback" aria-label="Dock APRS Chat back inline" title="Dock APRS Chat back inline">
          ⇤ dock back
        </button>
      </div>
    </DockColumn>
  );
}

// Family 3 — the docked-state headers carrying the ↗ Pop out affordance (spec
// §5 entry points; the affordance is a WebKitGTK flex-crush candidate, R5-F18).
function HeaderFixtureView() {
  if (view === 'header-routines') {
    // Dashboard (default) or designer header, both showing "↗ Pop out". Reuses
    // the plan-5 routines skeleton (DashboardRibbon + RoutinesSurface in a
    // pinned .layout-b grid), now with onPopOut wired.
    const initial: RoutinesView =
      params.get('rview') === 'designer'
        ? { view: 'designer', routine: params.get('routine') ?? '', tab: (params.get('rtab') ?? 'design') as DesignerTab }
        : { view: 'dashboard' };
    return <HeaderRoutines initial={initial} />;
  }
  if (view === 'header-tacmap') {
    // The Tac Map header control: the Map toggle + the "↗ Pop out" button
    // beside it (AprsDockTabs docked, onPopOutMap provided).
    return (
      <DockColumn label="APRS dock — Tac Map docked (↗ Pop out affordance)">
        <AprsDockTabs
          active="aprs"
          unread={3}
          modemEnabled
          stationCount={4}
          onSelect={() => undefined}
          onClose={() => undefined}
          mapOpen={false}
          onToggleMap={() => undefined}
          onPopOutMap={() => undefined}
        />
      </DockColumn>
    );
  }
  // header-aprschat: the APRS chat panel header carries "↗ Pop out". Real
  // AprsChatPanel with a populated feed (SEED_CHAT) so the header sits above
  // real traffic, not an empty pane. HintProvider because AprsChatPanel's
  // useFirstOpenTip('aprs') needs it (in the app it comes from AppShell).
  return (
    <DockColumn label="APRS dock — APRS Chat docked (↗ Pop out affordance)">
      <HintProvider>
        <div style={{ height: 620, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
          <AprsChatPanel
            messages={SEED_CHAT}
            send={async () => 'b99'}
            getConfig={async () => RESPONSES.aprs_config_get as AprsConfigDto}
            setConfig={async () => undefined}
            onPopOut={() => undefined}
          />
        </div>
      </HintProvider>
    </DockColumn>
  );
}

function HeaderRoutines({ initial }: { initial: RoutinesView }) {
  const [rv, setRv] = useState<RoutinesView>(initial);
  return (
    <div className="layout-b" style={{ height: '100vh' }}>
      <div style={{ gridRow: 3 }}>
        <DashboardRibbon data={ribbonData} onConnect={() => undefined} connecting={false} onOpenElmer={() => undefined} elmerOpen={false} />
      </div>
      <div style={{ gridRow: 4, minHeight: 0 }}>
        <RoutinesSurface view={rv} onNavigate={setRv} onPopOut={() => undefined} />
      </div>
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
    ) : view === 'routines' ? (
      <RoutinesFixtureView />
    ) : view === 'pop-routines' ? (
      <PopSurfaceFixtureView surface="routines" />
    ) : view === 'pop-tacmap' ? (
      <PopSurfaceFixtureView surface="tac_map" />
    ) : view === 'pop-aprschat' ? (
      <PopSurfaceFixtureView surface="aprs_chat" />
    ) : view.startsWith('vacated-') ? (
      <VacatedFixtureView />
    ) : view.startsWith('header-') ? (
      <HeaderFixtureView />
    ) : view === 'onboarding' ? (
      <OnboardingFixtureView />
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
