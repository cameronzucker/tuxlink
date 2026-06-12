// RequestCenter.app.test.tsx — App-level production-mount + invoke-failure
// tests (Task E3, bd-tuxlink-eymu).
//
// WHY THIS FILE EXISTS (the tuxlink-n4hz discipline):
// RequestCenter.test.tsx renders <RequestCenter> DIRECTLY — it hand-provides the
// props and never touches AppShell. A unit test like that can PASS while
// production CRASHES, because the unit silently supplies context the real mount
// path fails to provide (the tuxlink-n4hz post-merge crash: HelpView's useQuery
// threw "No QueryClient set" only on the production path). E3 mounts the Request
// Center through the EXACT production path:
//   menuModel → MenuBar → onMenuAction → dispatchMenuAction → openRequestCenter
//   → AppShell state → lazy Suspense mount of <RequestCenter>.
// It fires the REAL menu-dispatch action (NOT a synthetic open, NOT setState),
// asserts the dialog mounts with its providers (no missing-context crash), and
// covers the three adrev-#9 invoke-failure paths.
//
// The render harness MIRRORS AppShell.test.tsx exactly (renderShell →
// QueryClientProvider wrapping <AppShell/>) so the providers under test are the
// production providers, not a hand-rolled wrapper.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  render,
  screen,
  fireEvent,
  waitFor,
  within,
} from '@testing-library/react';
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { MessageMeta } from '../mailbox/types';
import type { CatalogEntry } from '../catalog/types';
// Eager-import the resolver so its bundled NWS-zone geometry (a large JSON) is
// transformed at COLLECT time, not lazily inside the timed findByRole when the
// React.lazy RequestCenter chunk first loads — the on-demand transform otherwise
// blows the dialog-open timeout (tuxlink-z1b7).
import './geo';

// ---------------------------------------------------------------------------
// Tauri IPC mocks. AppShell mounts a lot of chrome (TitleBar, MenuBar, ribbon,
// sidebar, list, status bar, session log). The base mock below returns
// shape-correct values for every command the shell fires so it mounts cleanly
// under jsdom; the Request-Center-specific commands (catalog_list, config_read,
// catalog_send_inquiry, grib_send_request) are overridden per-test via
// `routeRequest()` so each scenario controls the load + send outcomes.
// ---------------------------------------------------------------------------
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

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
    NorthEast: 'NorthEast', NorthWest: 'NorthWest', SouthEast: 'SouthEast', SouthWest: 'SouthWest',
  },
}));

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

// GribForm (the Request Center's 'grib' view) imports GridMapPicker, which pulls
// in Leaflet — no DOM map under jsdom. Mock at the module boundary, matching
// RequestCenter.test.tsx / GribForm.test.tsx so the GRIB deep-link mounts.
vi.mock('../map/GridMapPicker', () => ({
  GridMapPicker: ({
    onBoxChange,
  }: {
    onBoxChange?: (a: { lat: number; lon: number }, b: { lat: number; lon: number }) => void;
  }) => (
    <button
      type="button"
      data-testid="mock-box-drag"
      onClick={() => onBoxChange?.({ lat: 60.2, lon: -120.9 }, { lat: 40.8, lon: -140.1 })}
    >
      fire box
    </button>
  ),
}));

// AppShell mounts useMailbox / useUserFolders; mock them to keep the sidebar +
// list inert (the Request Center is what's under test, not mailbox wiring).
const inboxMsgs: MessageMeta[] = [];
vi.mock('../mailbox/useMailbox', () => ({
  useMailboxChangeEvents: () => {},
  useMailbox: () => ({ messages: inboxMsgs, isLoading: false, isError: false, error: null }),
  isBackendFolder: (f: string) => f === 'inbox' || f === 'outbox' || f === 'sent',
  isUserFolderSlug: (s: string) => /^[a-z0-9-]+$/.test(s) && !s.startsWith('-') && !s.endsWith('-'),
}));
vi.mock('../mailbox/useUserFolders', () => ({
  useUserFolders: () => ({ folders: [], isLoading: false, isError: false, error: null }),
  useCreateUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useDeleteUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRenameUserFolder: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useMoveUserFolder: () => ({ mutate: vi.fn(), mutateAsync: vi.fn(), isPending: false }),
  USER_FOLDERS_QUERY_KEY: ['userFolders'],
}));

import { invoke } from '@tauri-apps/api/core';
import { AppShell } from '../shell/AppShell';

// ---------------------------------------------------------------------------
// A real-ish catalog fixture. Includes the four national filenames the home
// sections resolve (PROP_3DAY / PROP_WWV / AUR_TONIGHT / INQUIRIES), an
// INQUIRIES entry, a couple of WL2K_RMS PUB_* entries, and a WA state-forecast
// entry (so the geo-derived Weather section has data when grid=CN87 → WA).
// ---------------------------------------------------------------------------
function entry(category: string, filename: string, description = '', size_bytes = 0): CatalogEntry {
  return { category, filename, description, size_bytes };
}
const CATALOG_FIXTURE: CatalogEntry[] = [
  entry('PROPAGATION', 'PROP_3DAY', '3-day HF propagation outlook', 800),
  entry('PROPAGATION', 'PROP_WWV', 'Daily WWV Solar Flux summary', 621),
  entry('PROPAGATION', 'AUR_TONIGHT', 'Auroral activity forecast', 500),
  entry('INFO', 'INQUIRIES', 'Catalog inquiries help & getting-started guides', 1200),
  entry('WL2K_RMS', 'PUB_PACKET', 'Packet Public Gateways Frequency List', 219867),
  entry('WL2K_RMS', 'PUB_VARA', 'VARA HF Public Gateways Frequency List', 198432),
  entry('WX_US_WA', 'WX_WA_FORECAST', 'Washington State forecast', 4096),
];

// Augmented fixture for the location-hero production-path test (Task 13).
// CN87uo → WAZ315 "City of Seattle" → WA_ZON_SEA; radar US.RAD.PSND; sea WX_EASTPAC.
const CATALOG_FIXTURE_WITH_LOCATION: CatalogEntry[] = [
  ...CATALOG_FIXTURE,
  entry('WX_US_WA', 'WA_ZON_SEA', 'City of Seattle Washington Zone Forecast', 2500),
  entry('WX_US_RAD', 'US.RAD.PSND', 'SNAPSHOT CURRENT RADAR U.S. PUGET SOUND & SJDF', 20799),
  entry('WX_EASTPAC', 'EPAC_COASTAL', 'NE Pacific coastal waters', 7300),
];

// Per-test routing of the Request-Center-relevant commands. Everything else the
// shell fires gets a shape-correct default so AppShell mounts identically across
// scenarios. A custom mockImplementation fully replaces the factory, so every
// command the production mount path touches is routed here.
interface RouteOpts {
  catalog?: () => Promise<unknown>;
  config?: () => Promise<unknown>;
  catalogSend?: () => Promise<unknown>;
  gribSend?: () => Promise<unknown>;
  /** position_status — drives RequestCenter's grid via useStatusData (ui_grid),
   *  the live GPS-aware source (tuxlink-fnzr). Default: no fix, no grid. */
  position?: () => Promise<unknown>;
}
function routeRequest(opts: RouteOpts = {}) {
  const catalog = opts.catalog ?? (async () => CATALOG_FIXTURE);
  const config = opts.config ?? (async () => ({ grid: 'CN87' }));
  const catalogSend = opts.catalogSend ?? (async () => 'MID-DEFAULT');
  const gribSend = opts.gribSend ?? (async () => 'GRIB-MID');
  const position = opts.position ?? (async () => ({ gps_ready: false, broadcast_grid: '', ui_grid: '' }));

  vi.mocked(invoke).mockImplementation((cmd: string): Promise<unknown> => {
    switch (cmd) {
      // --- Request Center commands (per-test controlled) ---
      case 'catalog_list':
        return catalog();
      case 'config_read':
        return config();
      case 'catalog_send_inquiry':
        return catalogSend();
      case 'grib_send_request':
        return gribSend();
      // --- Shell-chrome defaults so AppShell mounts cleanly ---
      case 'backend_status':
        return Promise.resolve(null);
      case 'session_log_snapshot':
        return Promise.resolve([]);
      case 'modem_get_status':
        return Promise.resolve({
          state: 'stopped',
          peer: null, mode: null, widthHz: null, pttBackend: null,
          snDb: null, vuDbfs: null, throughputBps: null,
          bytesRx: 0, bytesTx: 0, uptimeSec: 0,
          arqFlags: { busy: false, rx: false, tx: false },
          lastError: null,
        });
      case 'position_status':
        return position();
      case 'tauri_search_list_saved':
        return Promise.resolve([]);
      case 'tauri_search_list_recent':
        return Promise.resolve([]);
      case 'contacts_read':
        return Promise.resolve({ schema_version: 1, contacts: [], groups: [] });
      case 'contacts_suggestions':
        return Promise.resolve([]);
      default:
        return Promise.resolve(undefined);
    }
  });
}

// Production render harness — MIRRORS AppShell.test.tsx's renderShell so the
// providers under test are the production providers (QueryClientProvider wrapping
// the real AppShell), NOT a hand-rolled wrapper (the n4hz lesson).
function renderShell() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AppShell />
    </QueryClientProvider>,
  );
}

// Drive a menu action through the rendered <MenuBar>: open the top menu, then
// click the leaf item — mirrors AppShell.test.tsx's clickMenu so the action
// flows the real menuModel → MenuBar → onMenuAction → dispatchMenuAction path.
// Scoped to the menubar so leaf labels don't collide with reading-pane buttons.
function clickMenu(top: string, item: RegExp) {
  const menubar = screen.getByRole('menubar');
  fireEvent.click(within(menubar).getByRole('button', { name: top }));
  fireEvent.click(within(menubar).getByRole('button', { name: item }));
}

// Open the Request Center via the REAL menu and wait for the lazy Suspense mount
// + effects to resolve.
async function openRequestCenter() {
  clickMenu('Message', /^Request Center…$/);
  return screen.findByRole('dialog', { name: 'Request Center' }, { timeout: 10000 });
}

describe('<RequestCenter> — App-level production mount path (E3 / tuxlink-eymu)', () => {
  beforeEach(() => {
    globalThis.localStorage?.clear?.();
    vi.clearAllMocks();
  });
  afterEach(() => {
    vi.mocked(invoke).mockReset();
  });

  // (1) Opens via the real menu + mounts with providers (no crash). If a missing
  // provider would crash the lazy RequestCenter (the n4hz failure class), this
  // render throws and the test fails — that is the regression this guards.
  it('opens via Message → Request Center… and mounts with its providers (no missing-context crash)', async () => {
    routeRequest();
    renderShell();

    const dialog = await openRequestCenter();
    expect(dialog).toBeInTheDocument();

    // The location chip resolves through the production config_read path.
    const chip = within(dialog).getByTestId('request-center-location');
    await waitFor(() => expect(chip).toHaveTextContent('Near CN87'));

    // The home sections rendered through the production catalog_list path.
    expect(await within(dialog).findByTestId('request-section-propagation')).toBeInTheDocument();
  });

  // (2) Add a national card → Send all invokes the right command with the right
  // payload, and the success result region + arrival note render.
  it('adds the Propagation forecast card and Send all invokes catalog_send_inquiry with the national filename', async () => {
    routeRequest({ catalogSend: async () => 'MID-PROP-42' });
    renderShell();
    const dialog = await openRequestCenter();

    // Click the Propagation forecast card's Add control. Its aria-label is
    // "Add Propagation forecast to request" (sections.ts → RequestCenter card).
    const addBtn = await within(dialog).findByRole('button', {
      name: 'Add Propagation forecast to request',
    });
    fireEvent.click(addBtn);

    // The basket now holds the cms:PROP_3DAY item.
    expect(await within(dialog).findByTestId('basket-item-cms:PROP_3DAY')).toBeInTheDocument();

    vi.mocked(invoke).mockClear();
    fireEvent.click(within(dialog).getByTestId('request-basket-send'));

    // catalog_send_inquiry invoked once with the national filename.
    await waitFor(() => {
      const inquiryCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === 'catalog_send_inquiry');
      expect(inquiryCalls).toHaveLength(1);
      expect(inquiryCalls[0][1]).toEqual({ filenames: ['PROP_3DAY'] });
    });

    // Success result region + the arrival note render.
    const result = await within(dialog).findByTestId('request-basket-result');
    expect(result).toHaveTextContent('Queued 1 inquiry message to the CMS');
    expect(result).toHaveTextContent('MID-PROP-42');
    expect(result).toHaveTextContent(
      'Responses arrive in your Inbox after the next connect.',
    );
  });

  // (3) GRIB deep-link via the real menu — validates the E2 repoint end-to-end:
  // grib_request → openRequestCenter('grib') opens the center at the GRIB form.
  it('opens Message → GRIB File Request… at the GRIB form view through the production path', async () => {
    routeRequest();
    renderShell();

    clickMenu('Message', /^GRIB File Request…$/);
    const dialog = await screen.findByRole(
      'dialog',
      { name: 'Request Center' },
      { timeout: 10000 },
    );

    // The GRIB form is the active view: its subject + add controls are present.
    expect(await within(dialog).findByTestId('grib-subject')).toBeInTheDocument();
    expect(within(dialog).getByTestId('grib-add')).toBeInTheDocument();
  });

  // (4) adrev #9 — config_read returns no grid → neutral chip, no "Near
  // null/undefined", no crash. Two sub-cases: null grid and a rejecting read.
  it('shows a neutral "Location not set" chip when config_read returns no grid (no crash)', async () => {
    routeRequest({ config: async () => ({ grid: null }) });
    renderShell();
    const dialog = await openRequestCenter();

    const chip = within(dialog).getByTestId('request-center-location');
    expect(chip).toHaveTextContent('Location not set');
    expect(chip).not.toHaveTextContent(/Near (null|undefined)/);
  });

  it('shows a neutral "Location not set" chip when config_read rejects (no crash)', async () => {
    routeRequest({ config: async () => Promise.reject(new Error('no config file')) });
    renderShell();
    const dialog = await openRequestCenter();

    const chip = within(dialog).getByTestId('request-center-location');
    // Let any rejection settle; the chip must stay neutral.
    await waitFor(() => expect(chip).toHaveTextContent('Location not set'));
    expect(chip).not.toHaveTextContent(/Near (null|undefined)/);
    // The center still mounted its content region (no crash).
    expect(within(dialog).getByTestId('request-content')).toBeInTheDocument();
  });

  // (5) adrev #9 — catalog_list rejects → the catalog error state renders, no crash.
  it('renders the catalog error state when catalog_list rejects (no crash)', async () => {
    routeRequest({ catalog: async () => Promise.reject(new Error('catalog read failed')) });
    renderShell();
    const dialog = await openRequestCenter();

    const errState = await within(dialog).findByTestId('request-catalog-error');
    expect(errState).toHaveTextContent('Failed to load catalog');
    expect(errState).toHaveTextContent('catalog read failed');
  });

  // (6) adrev #9 — send-rail error surfaced: with a cms item in the basket,
  // catalog_send_inquiry rejects → the error is surfaced in the result region.
  it('surfaces a CMS send-rail failure in the basket result region (no crash)', async () => {
    routeRequest({
      catalogSend: async () => Promise.reject(new Error('CMS unreachable')),
    });
    renderShell();
    const dialog = await openRequestCenter();

    const addBtn = await within(dialog).findByRole('button', {
      name: 'Add Propagation forecast to request',
    });
    fireEvent.click(addBtn);
    expect(await within(dialog).findByTestId('basket-item-cms:PROP_3DAY')).toBeInTheDocument();

    fireEvent.click(within(dialog).getByTestId('request-basket-send'));

    const result = await within(dialog).findByTestId('request-basket-result');
    expect(result).toHaveTextContent('CMS failed: CMS unreachable');
    // The failed item is KEPT in the basket (adrev #4 keep/clear semantics).
    expect(within(dialog).getByTestId('basket-item-cms:PROP_3DAY')).toBeInTheDocument();
  });

  // (7) Task 13 — App-level production-path test for the resolved location hero
  // (tuxlink-n4hz discipline: unit tests can pass while the production mount path
  // crashes due to missing context; this test exercises config_read → grid →
  // buildSections → all three location cards rendered, NO hand-injected section
  // props — the exact path that runs when the operator opens Request Center with
  // a saved grid).
  it('renders the resolved location hero from the live GPS grid + catalog (production path)', async () => {
    // Grid comes from the position subsystem's ui_grid (live GPS fix), not a
    // pinned config grid — the default config (source=Gps, identity.grid=null).
    routeRequest({
      config: async () => ({ grid: null }),
      position: async () => ({ gps_ready: true, broadcast_grid: 'CN87', ui_grid: 'CN87uo' }),
      catalog: async () => CATALOG_FIXTURE_WITH_LOCATION,
    });
    renderShell();
    const dialog = await openRequestCenter();

    // Zone forecast card (primary hero — "City of Seattle" via WAZ315 → WA_ZON_SEA)
    expect(await within(dialog).findByTestId('request-card-loc-zone-forecast')).toBeInTheDocument();
    // Radar card (US.RAD.PSND — Puget Sound)
    expect(within(dialog).getByTestId('request-card-loc-radar')).toBeInTheDocument();
    // Marine card (WX_EASTPAC sea area)
    expect(within(dialog).getByTestId('request-card-loc-marine')).toBeInTheDocument();
  });

  // (8) tuxlink-fnzr regression: under the DEFAULT config (position_source=Gps,
  // identity.grid=null) the hero must resolve from the live GPS fix (ui_grid),
  // NOT show "Location not set". This is the exact bug — the hero read the static
  // config_read().grid (null under GPS) instead of the live position subsystem.
  it('resolves the location chip from the live GPS fix when no config grid is pinned (tuxlink-fnzr)', async () => {
    routeRequest({
      config: async () => ({ grid: null }), // GPS source, no pinned grid
      position: async () => ({ gps_ready: true, broadcast_grid: 'CN87', ui_grid: 'CN87uo' }),
    });
    renderShell();
    const dialog = await openRequestCenter();
    const chip = within(dialog).getByTestId('request-center-location');
    await waitFor(() => expect(chip).toHaveTextContent('Near CN87uo'));
    expect(chip).not.toHaveTextContent('Location not set');
  });
});
