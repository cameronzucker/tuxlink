import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { RequestCenter } from './RequestCenter';
import type { CatalogEntry } from '../catalog/types';

// Mock the Tauri invoke surface so the shell drives without a backend.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

function entry(category: string, filename: string, description = '', size_bytes = 0): CatalogEntry {
  return { category, filename, description, size_bytes };
}

const FIXTURE_ENTRIES: CatalogEntry[] = [
  entry('WL2K_RMS', 'PUB_PACKET', 'Packet Public Gateways Frequency List', 219867),
  entry('PROPAGATION', 'PROP_WWV', 'Daily WWV Solar Flux summary', 621),
];

// Default mock: catalog loads, config_read returns a grid.
function mockHappy(grid: string | null = 'CN87') {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
    if (cmd === 'config_read') return { grid };
    return null;
  });
}

describe('<RequestCenter>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders a dialog labelled "Request Center"', async () => {
    mockHappy();
    render(<RequestCenter onClose={() => {}} />);
    const dialog = await screen.findByRole('dialog', { name: 'Request Center' });
    expect(dialog).toBeInTheDocument();
  });

  it('renders the header chrome: location chip, search input, content + basket regions', async () => {
    mockHappy();
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    expect(screen.getByTestId('request-center-location')).toBeInTheDocument();
    expect(screen.getByTestId('request-search')).toBeInTheDocument();
    expect(screen.getByTestId('request-content')).toBeInTheDocument();
    expect(screen.getByTestId('request-basket')).toBeInTheDocument();
  });

  it('the location chip shows "Near CN87" once config_read resolves with a grid', async () => {
    mockHappy('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await waitFor(() =>
      expect(screen.getByTestId('request-center-location')).toHaveTextContent('Near CN87'),
    );
  });

  it('the Close button calls onClose', async () => {
    mockHappy();
    const onClose = vi.fn();
    render(<RequestCenter onClose={onClose} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    fireEvent.click(screen.getByTestId('request-close'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('ESC calls onClose', async () => {
    mockHappy();
    const onClose = vi.fn();
    render(<RequestCenter onClose={onClose} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  // --- Adrev #3: single catalog-load owner; shell renders loading/empty/error states ---

  it('renders a loading placeholder while the catalog fetches', () => {
    // Both calls return never-resolving promises so NO deferred setState fires
    // after the synchronous assertion (would warn "not wrapped in act(...)").
    vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));
    render(<RequestCenter onClose={() => {}} />);
    expect(screen.getByTestId('request-catalog-loading')).toBeInTheDocument();
  });

  it('renders an empty state when the catalog loads with zero entries', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return [];
      if (cmd === 'config_read') return { grid: 'CN87' };
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    expect(await screen.findByTestId('request-catalog-empty')).toBeInTheDocument();
    // Mutual exclusion: empty state must not coexist with home cards/sections
    // (guards the `entries.length > 0` home clause against a future drop).
    expect(screen.queryAllByTestId(/^request-card-/)).toHaveLength(0);
    expect(document.querySelector('.request-sections')).toBeNull();
  });

  it('renders an error message (no crash) when catalog_list rejects', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') throw new Error('catalog backend offline');
      if (cmd === 'config_read') return { grid: 'CN87' };
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    const err = await screen.findByTestId('request-catalog-error');
    expect(err).toHaveTextContent('catalog backend offline');
    // Dialog still renders — no crash.
    expect(screen.getByRole('dialog', { name: 'Request Center' })).toBeInTheDocument();
    // Mutual exclusion: error state must not coexist with home cards/sections
    // (guards the `!error` home clause against a future drop).
    expect(screen.queryAllByTestId(/^request-card-/)).toHaveLength(0);
    expect(document.querySelector('.request-sections')).toBeNull();
  });

  // --- initialView routing: grib seam renders its placeholder, not home ---

  it('initialView="grib" renders the grib placeholder and not the home sections', async () => {
    mockHappy();
    render(<RequestCenter onClose={() => {}} initialView="grib" />);
    expect(await screen.findByTestId('request-grib')).toBeInTheDocument();
    // Home sections must not render in the grib view.
    expect(document.querySelector('.request-sections')).toBeNull();
    expect(screen.queryAllByTestId(/^request-card-/)).toHaveLength(0);
  });

  // --- Adrev #9: config_read failure path → neutral location chip, never "Near null" ---

  it('shows a neutral location state when config_read resolves with no grid', async () => {
    mockHappy(null);
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    // config_read resolved with no grid → chip settles on the neutral label.
    await waitFor(() =>
      expect(screen.getByTestId('request-center-location')).toHaveTextContent('Location not set'),
    );
    const chip = screen.getByTestId('request-center-location');
    expect(chip.textContent).not.toMatch(/null|undefined/);
  });

  it('shows a neutral location state (no crash) when config_read rejects', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return FIXTURE_ENTRIES;
      if (cmd === 'config_read') throw new Error('config unreadable');
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByRole('dialog', { name: 'Request Center' });
    const chip = screen.getByTestId('request-center-location');
    expect(chip).toHaveTextContent('Location not set');
    expect(chip.textContent).not.toMatch(/null|undefined|Near/);
  });
});

// ===========================================================================
// Task C2 — request-first sections + cards.
//
// A richer catalog fixture so the geo + catalogMap resolvers have real entries
// to bind to. CN87 → lat≈47.5, lon≈-123.0 → state WA, sea-area WX_EASTPAC.
// ===========================================================================

const C2_ENTRIES: CatalogEntry[] = [
  // WA state forecast (non-tabular preferred by bestStateForecast).
  entry('WX_US_WA', 'WA_FOR_WA', 'State Forecast for Washington', 4096),
  // The four NATIONAL filenames in their categories.
  entry('PROPAGATION', 'PROP_3DAY', '3-Day Propagation Forecast', 800),
  entry('PROPAGATION', 'PROP_WWV', 'Daily WWV Solar Flux summary', 621),
  entry('PROPAGATION', 'AUR_TONIGHT', 'Aurora Forecast Tonight', 900),
  entry('INQUIRIES', 'INQUIRIES', 'Winlink Catalog Inquiries Help', 1200),
  // Two WL2K_RMS PUB_* gateway entries (by mode).
  entry('WL2K_RMS', 'PUB_PACKET', 'Packet Public Gateways Frequency List', 219867),
  entry('WL2K_RMS', 'PUB_VARA', 'VARA HF Public Gateways Frequency List', 180000),
  // Decoy entries that MUST NOT surface as cards (dropped per decisions §7).
  entry('WX_METAR', 'METAR_KSEA', 'METAR for Seattle-Tacoma', 300),
  entry('WX_HAZARD', 'HAZ_OUTLOOK', 'Hazardous Weather Outlook', 500),
];

function mockC2(grid: string | null = 'CN87') {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'catalog_list') return C2_ENTRIES;
    if (cmd === 'config_read') return { grid };
    return null;
  });
}

// Click the add control inside a card identified by its stable id.
function clickCardAdd(id: string) {
  const card = screen.getByTestId(`request-card-${id}`);
  fireEvent.click(within(card).getByRole('button', { name: /add|open/i }));
}

describe('<RequestCenter> — C2 sections & cards', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('Weather: "State forecast" card adds the WA state-forecast cms item', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    const card = await screen.findByTestId('request-card-wx-state-forecast');
    expect(card).toBeInTheDocument();
    // Verify the visible label copy.
    expect(within(card).getByText('State forecast')).toBeInTheDocument();

    clickCardAdd('wx-state-forecast');

    const item = await screen.findByTestId('basket-item-cms:WA_FOR_WA');
    expect(item).toHaveTextContent('State forecast');
  });

  it('Weather: "Marine forecast" card navigates the browse view to WX_EASTPAC without touching the basket', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    const card = await screen.findByTestId('request-card-wx-marine-forecast');
    expect(card).toBeInTheDocument();
    expect(within(card).getByText('Marine forecast')).toBeInTheDocument();

    clickCardAdd('wx-marine-forecast');

    const browse = await screen.findByTestId('request-browse');
    expect(browse).toHaveAttribute('data-category', 'WX_EASTPAC');
    // Basket unchanged — openBrowse never adds an item.
    expect(screen.queryByTestId(/^basket-item-/)).not.toBeInTheDocument();
  });

  it('Propagation: renders exactly 3 national cards, each adding the right cms filename', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    const section = await screen.findByTestId('request-section-propagation');
    const cards = within(section).getAllByTestId(/^request-card-/);
    expect(cards).toHaveLength(3);

    clickCardAdd('prop-forecast');
    clickCardAdd('prop-solar');
    clickCardAdd('prop-aurora');

    expect(await screen.findByTestId('basket-item-cms:PROP_3DAY')).toBeInTheDocument();
    expect(screen.getByTestId('basket-item-cms:PROP_WWV')).toBeInTheDocument();
    expect(screen.getByTestId('basket-item-cms:AUR_TONIGHT')).toBeInTheDocument();
  });

  it('Nearby stations: gateway-lists card opens browse at WL2K_RMS; Winlink-info adds INQUIRIES', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-nearby');

    // Winlink info & how-to → addCms INQUIRIES.
    clickCardAdd('nearby-winlink-info');
    expect(await screen.findByTestId('basket-item-cms:INQUIRIES')).toBeInTheDocument();

    // Public gateway lists → openBrowse WL2K_RMS (navigates, no basket mutation).
    clickCardAdd('nearby-gateways');
    const browse = await screen.findByTestId('request-browse');
    expect(browse).toHaveAttribute('data-category', 'WL2K_RMS');
  });

  it('dropped cards are absent: no METAR card, no hazardous-weather card', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-nearby');
    expect(screen.queryByText(/METAR/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/hazardous/i)).not.toBeInTheDocument();
  });

  it('grid=null: geo cards (State / Marine forecast) are absent; national + nearby cards still render', async () => {
    mockC2(null);
    render(<RequestCenter onClose={() => {}} />);
    // National + nearby render regardless of location.
    expect(await screen.findByTestId('request-card-prop-forecast')).toBeInTheDocument();
    expect(screen.getByTestId('request-card-nearby-winlink-info')).toBeInTheDocument();
    // Geo-derived cards omitted when there is no grid.
    expect(screen.queryByTestId('request-card-wx-state-forecast')).not.toBeInTheDocument();
    expect(screen.queryByTestId('request-card-wx-marine-forecast')).not.toBeInTheDocument();
  });
});

// ===========================================================================
// Task D1 — the home "Browse full catalog by category" reveal mounts the real
// master-detail CatalogBrowse, and the C2 openBrowse deep-links still resolve
// against it (data-category preserved on the browse root).
// ===========================================================================

describe('<RequestCenter> — D1 catalog browse reveal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('the home browse-reveal switches to the browse view and mounts CatalogBrowse', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    // Home reveal control is present.
    const reveal = await screen.findByTestId('request-browse-reveal');
    expect(reveal).toBeInTheDocument();

    fireEvent.click(reveal);

    // Browse pane mounts; CatalogBrowse's nav lists real categories.
    const browse = await screen.findByTestId('request-browse');
    expect(browse).toBeInTheDocument();
    expect(screen.getByTestId('catalog-browse-nav')).toBeInTheDocument();
    // No deep-link category → defaults to first category (WX_US_WA in C2 order).
    expect(screen.getByTestId('catalog-browse-cat-WL2K_RMS')).toBeInTheDocument();
    // Home sections no longer rendered.
    expect(document.querySelector('.request-sections')).toBeNull();
  });

  it('adding an item from browse puts a cms BasketItem in the shared basket (cms:<filename> id)', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    fireEvent.click(await screen.findByTestId('request-browse-reveal'));
    await screen.findByTestId('request-browse');
    // Select WL2K_RMS, then add PUB_PACKET from its item list.
    fireEvent.click(screen.getByTestId('catalog-browse-cat-WL2K_RMS'));
    const item = screen.getByTestId('catalog-browse-item-PUB_PACKET');
    fireEvent.click(within(item).getByRole('button', { name: /add/i }));
    // Same id scheme as the C2 home cards → dedup works across surfaces.
    expect(await screen.findByTestId('basket-item-cms:PUB_PACKET')).toBeInTheDocument();
  });

  it('Back from browse returns to the home view', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    fireEvent.click(await screen.findByTestId('request-browse-reveal'));
    await screen.findByTestId('request-browse');
    fireEvent.click(screen.getByTestId('catalog-browse-back'));
    // Home sections render again.
    expect(await screen.findByTestId('request-section-propagation')).toBeInTheDocument();
    expect(screen.queryByTestId('request-browse')).toBeNull();
  });

  it('openBrowse deep-link from a card preselects the target category on the browse root', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    // Marine-forecast card deep-links to WX_EASTPAC.
    const card = await screen.findByTestId('request-card-wx-marine-forecast');
    fireEvent.click(within(card).getByRole('button', { name: /open/i }));
    const browse = await screen.findByTestId('request-browse');
    expect(browse).toHaveAttribute('data-category', 'WX_EASTPAC');
  });
});
