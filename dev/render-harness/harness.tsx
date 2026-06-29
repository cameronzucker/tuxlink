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
import { RequestCenter } from '../../src/request/RequestCenter';
import { DashboardRibbon } from '../../src/shell/DashboardRibbon';
import type { CatalogEntry } from '../../src/catalog/types';
import type { StatusBarData } from '../../src/shell/useStatus';

const params = new URLSearchParams(location.search);
const grid = params.has('grid') ? params.get('grid') : 'CN87';
const view = (params.get('view') ?? 'home') as 'home' | 'browse' | 'grib' | 'ribbon';

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
  config_read: { grid, review_inbound_before_download: false },
  catalog_list: CATALOG,
  catalog_send_inquiry: 'MID-TEST-0001',
  // DashboardRibbon's GridEdit write paths:
  config_set_grid: grid,
  position_set_source: null,
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
};

const queryClient = new QueryClient();

createRoot(document.getElementById('root')!).render(
  <QueryClientProvider client={queryClient}>
    {view === 'ribbon' ? (
      <DashboardRibbon data={ribbonData} onConnect={() => undefined} />
    ) : (
      <RequestCenter initialView={view} onClose={() => undefined} />
    )}
  </QueryClientProvider>,
);
