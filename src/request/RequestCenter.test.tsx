import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { RequestCenter } from './RequestCenter';
import type { CatalogEntry } from '../catalog/types';

// Mock the Tauri invoke surface so the shell drives without a backend.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

// Mock GridMapPicker at the module boundary — the GribForm view (Task D3)
// imports it, and the real picker pulls in Leaflet (no DOM map in jsdom). The
// mock exposes a button that fires onBoxChange, matching GribForm.test.tsx.
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

  // --- initialView routing: grib seam renders the GribForm, not home (D3) ---

  it('initialView="grib" renders the GribForm and not the home sections', async () => {
    mockHappy();
    render(<RequestCenter onClose={() => {}} initialView="grib" />);
    const grib = await screen.findByTestId('request-grib');
    expect(grib).toBeInTheDocument();
    // The GribForm fields are present (it's the real form, not a blank div).
    expect(screen.getByTestId('grib-subject')).toBeInTheDocument();
    expect(screen.getByTestId('grib-add')).toBeInTheDocument();
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

// ===========================================================================
// Task D2 — global header search: typing in `request-search` filters across
// ALL catalog items (filename / description / category, case-insensitive),
// overriding the current view; clearing the search returns to the view.
//
// Fixture has matches across >= 2 categories sharing the needle "forecast"
// (Aurora Forecast / 3-Day Propagation Forecast in PROPAGATION, State Forecast
// in WX_US_WA), so a single needle proves cross-category results.
// ===========================================================================

describe('<RequestCenter> — D2 global search', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('typing a needle shows global results across categories, overriding home', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    fireEvent.change(screen.getByTestId('request-search'), { target: { value: 'forecast' } });

    const results = await screen.findByTestId('catalog-search-results');
    expect(results).toBeInTheDocument();
    // Matches span multiple categories.
    expect(within(results).getByTestId('catalog-browse-item-PROP_3DAY')).toBeInTheDocument();
    expect(within(results).getByTestId('catalog-browse-item-WA_FOR_WA')).toBeInTheDocument();
    // Home sections no longer rendered (search overrides the view).
    expect(document.querySelector('.request-sections')).toBeNull();
  });

  it('clicking a global result Add adds a cms:<filename> basket item', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    fireEvent.change(screen.getByTestId('request-search'), { target: { value: 'WWV' } });
    const item = await screen.findByTestId('catalog-browse-item-PROP_WWV');
    fireEvent.click(within(item).getByRole('button', { name: /add/i }));

    // Same id scheme as cards/browse → dedup works across surfaces.
    expect(await screen.findByTestId('basket-item-cms:PROP_WWV')).toBeInTheDocument();
  });

  it('clearing the search returns to the home view', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    const search = screen.getByTestId('request-search');
    fireEvent.change(search, { target: { value: 'forecast' } });
    expect(await screen.findByTestId('catalog-search-results')).toBeInTheDocument();

    fireEvent.change(search, { target: { value: '' } });
    // Home sections render again; search results gone.
    expect(await screen.findByTestId('request-section-propagation')).toBeInTheDocument();
    expect(screen.queryByTestId('catalog-search-results')).toBeNull();
  });

  it('clearing search via the Clear-search control restores the underlying browse view (not home)', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    // Drop into the master-detail browse view.
    fireEvent.click(await screen.findByTestId('request-browse-reveal'));
    await screen.findByTestId('catalog-browse-nav');

    // Search overrides the view with flat results.
    fireEvent.change(screen.getByTestId('request-search'), { target: { value: 'forecast' } });
    expect(await screen.findByTestId('catalog-search-results')).toBeInTheDocument();
    expect(screen.queryByTestId('catalog-browse-nav')).toBeNull();

    // The search-mode Back control clears the search ONLY → the browse view
    // underneath returns (not the home sections).
    const clear = screen.getByTestId('catalog-browse-back');
    expect(clear).toHaveTextContent('Clear search');
    fireEvent.click(clear);

    // Browse master-detail is restored; search results gone; home not shown.
    expect(await screen.findByTestId('catalog-browse-nav')).toBeInTheDocument();
    expect(screen.getByTestId('request-browse')).toBeInTheDocument();
    expect(screen.queryByTestId('catalog-search-results')).toBeNull();
    expect(document.querySelector('.request-sections')).toBeNull();
  });

  it('search overrides the grib view too (search from anywhere)', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} initialView="grib" />);
    await screen.findByTestId('request-grib');

    fireEvent.change(screen.getByTestId('request-search'), { target: { value: 'forecast' } });
    expect(await screen.findByTestId('catalog-search-results')).toBeInTheDocument();
    expect(screen.queryByTestId('request-grib')).toBeNull();
  });
});

// ===========================================================================
// Task D3 — the home "More: GRIB by area" reveal mounts the GribForm; adding
// a GRIB request puts a `saildocs` BasketItem in the shared basket (NOT an
// immediate send); Back returns to home.
// ===========================================================================

describe('<RequestCenter> — D3 GRIB-by-area reveal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('the home GRIB reveal switches to the grib view and mounts GribForm', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    const reveal = await screen.findByTestId('request-grib-reveal');
    expect(reveal).toBeInTheDocument();

    fireEvent.click(reveal);

    expect(await screen.findByTestId('request-grib')).toBeInTheDocument();
    expect(screen.getByTestId('grib-subject')).toBeInTheDocument();
    // Home sections no longer rendered.
    expect(document.querySelector('.request-sections')).toBeNull();
  });

  it('clicking Add to request adds a saildocs basket item carrying the request', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} initialView="grib" />);
    await screen.findByTestId('request-grib');

    fireEvent.click(screen.getByTestId('grib-add'));

    // A saildocs basket item appears, labelled by the request subject. The id
    // scheme is `saildocs:<json>`, so query by the label rather than the id.
    const basket = screen.getByTestId('request-basket');
    expect(within(basket).getByText('GRIB request')).toBeInTheDocument();
    expect(within(basket).getByTestId(/^basket-item-saildocs:/)).toBeInTheDocument();
    // Exactly one basket item, and no stray cms extras alongside the saildocs item.
    expect(within(basket).getAllByTestId(/^basket-item-/)).toHaveLength(1);
  });

  it('Back from the GribForm returns to the home view', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    fireEvent.click(await screen.findByTestId('request-grib-reveal'));
    await screen.findByTestId('request-grib');

    fireEvent.click(screen.getByTestId('grib-back'));

    expect(await screen.findByTestId('request-section-propagation')).toBeInTheDocument();
    expect(screen.queryByTestId('request-grib')).toBeNull();
  });
});

// ===========================================================================
// Task E1 — basket right-rail UI + Send all.
//
// The basket lists added items with a remove (✕) control; a per-rail footer
// summarizes counts ("N requests · 1 inquiry message to the CMS · N Saildocs
// request(s)"); "Send all" dispatches via dispatchBasket. Partial-failure
// (adrev #4): KEEP the failed rail's items, clear only the SUCCEEDED rail,
// surface a per-rail error. Empty basket (adrev #5): Send is disabled.
// ===========================================================================

// Adds N cms items + M saildocs items to the basket by driving the UI:
//   - cms via the home Propagation cards (prop-forecast/prop-solar/prop-aurora)
//   - saildocs via the GRIB reveal + add (one per call), back to home each time.
// Returns once the basket holds the requested items.
async function seedBasket({ cms = 0, saildocs = 0 }: { cms?: number; saildocs?: number }) {
  const cmsCards = ['prop-forecast', 'prop-solar', 'prop-aurora'];
  for (let i = 0; i < cms; i++) {
    clickCardAdd(cmsCards[i]);
  }
  for (let i = 0; i < saildocs; i++) {
    fireEvent.click(screen.getByTestId('request-grib-reveal'));
    await screen.findByTestId('request-grib');
    // The mock box-drag sets a unique-enough request; vary the subject so each
    // saildocs item gets a distinct id (saildocs:<json>) and is not deduped.
    fireEvent.change(screen.getByTestId('grib-subject'), {
      target: { value: `GRIB request ${i}` },
    });
    fireEvent.click(screen.getByTestId('grib-add'));
    fireEvent.click(screen.getByTestId('grib-back'));
    await screen.findByTestId('request-section-propagation');
  }
}

describe('<RequestCenter> — E1 basket UI + Send all', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('lists added items with a remove control that removes the item', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    await seedBasket({ cms: 1 });
    const basket = screen.getByTestId('request-basket');
    expect(within(basket).getByTestId('basket-item-cms:PROP_3DAY')).toBeInTheDocument();

    const remove = within(basket).getByTestId('basket-remove-cms:PROP_3DAY');
    fireEvent.click(remove);

    expect(within(basket).queryByTestId('basket-item-cms:PROP_3DAY')).toBeNull();
  });

  it('footer summary reflects per-rail counts (cms collapses to one inquiry message)', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    // 2 cms + 1 saildocs → "3 requests · 1 inquiry message to the CMS · 1 Saildocs request"
    await seedBasket({ cms: 2, saildocs: 1 });
    const summary = screen.getByTestId('request-basket-summary');
    expect(summary).toHaveTextContent('3 requests');
    expect(summary).toHaveTextContent('1 inquiry message to the CMS');
    expect(summary).toHaveTextContent('1 Saildocs request');
    expect(summary.textContent).not.toMatch(/Saildocs requests\b/); // singular
  });

  it('footer summary pluralizes Saildocs and omits absent rails', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    // 0 cms + 2 saildocs → no "inquiry message to the CMS" clause; "2 Saildocs requests"
    await seedBasket({ saildocs: 2 });
    const summary = screen.getByTestId('request-basket-summary');
    expect(summary).toHaveTextContent('2 requests');
    expect(summary).toHaveTextContent('2 Saildocs requests');
    expect(summary.textContent).not.toMatch(/inquiry message to the CMS/);
  });

  it('Send all (cms + saildocs) calls each command and clears the basket on success', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return C2_ENTRIES;
      if (cmd === 'config_read') return { grid: 'CN87' };
      if (cmd === 'catalog_send_inquiry') return 'MID-CMS-1';
      if (cmd === 'grib_send_request') return 'MID-GRIB-1';
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    await seedBasket({ cms: 2, saildocs: 1 });
    vi.mocked(invoke).mockClear();

    fireEvent.click(screen.getByTestId('request-basket-send'));

    // Result region shows the per-rail summary + the next-connect line.
    const result = await screen.findByTestId('request-basket-result');
    expect(result).toHaveTextContent('Responses arrive in your Inbox after the next connect.');

    // catalog_send_inquiry called ONCE with both cms filenames in insertion order.
    const cmsCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'catalog_send_inquiry');
    expect(cmsCalls).toHaveLength(1);
    expect(cmsCalls[0][1]).toEqual({ filenames: ['PROP_3DAY', 'PROP_WWV'] });

    // grib_send_request called once per saildocs item.
    const gribCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'grib_send_request');
    expect(gribCalls).toHaveLength(1);
    // PAYLOAD, not just count: the dispatched request must carry the operator's
    // composed request, NOT DEFAULT_GRIB_REQUEST. seedBasket set a distinctive
    // subject ("GRIB request 0", vs the default "GRIB request"), so a regression
    // that sent the default body would fail here.
    expect(gribCalls[0][1]).toMatchObject({ request: { subject: 'GRIB request 0' } });

    // Basket cleared (both rails ok).
    const basket = screen.getByTestId('request-basket');
    expect(within(basket).queryAllByTestId(/^basket-item-/)).toHaveLength(0);
  });

  it('Send all (cms only): calls catalog_send_inquiry, not grib; CMS summary only; basket cleared', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return C2_ENTRIES;
      if (cmd === 'config_read') return { grid: 'CN87' };
      if (cmd === 'catalog_send_inquiry') return 'MID-CMS-1';
      if (cmd === 'grib_send_request') return 'MID-GRIB-1';
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    await seedBasket({ cms: 2 });
    vi.mocked(invoke).mockClear();

    fireEvent.click(screen.getByTestId('request-basket-send'));

    const result = await screen.findByTestId('request-basket-result');
    // CMS summary + next-connect line present.
    expect(result).toHaveTextContent('Queued 1 inquiry message to the CMS');
    expect(result).toHaveTextContent('Responses arrive in your Inbox after the next connect.');
    // No Saildocs clause at all (no saildocs items dispatched).
    expect(result.textContent).not.toMatch(/Saildocs/);

    // catalog_send_inquiry called once with both cms filenames; grib never called.
    const cmsCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'catalog_send_inquiry');
    expect(cmsCalls).toHaveLength(1);
    expect(cmsCalls[0][1]).toEqual({ filenames: ['PROP_3DAY', 'PROP_WWV'] });
    expect(vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'grib_send_request')).toHaveLength(0);

    // Basket cleared on success.
    const basket = screen.getByTestId('request-basket');
    expect(within(basket).queryAllByTestId(/^basket-item-/)).toHaveLength(0);
  });

  it('Send all (saildocs only): calls grib_send_request, not cms; Saildocs summary only; basket cleared', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return C2_ENTRIES;
      if (cmd === 'config_read') return { grid: 'CN87' };
      if (cmd === 'catalog_send_inquiry') return 'MID-CMS-1';
      if (cmd === 'grib_send_request') return 'MID-GRIB-1';
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    await seedBasket({ saildocs: 1 });
    vi.mocked(invoke).mockClear();

    fireEvent.click(screen.getByTestId('request-basket-send'));

    const result = await screen.findByTestId('request-basket-result');
    // Saildocs summary + next-connect line present.
    expect(result).toHaveTextContent('Queued 1 Saildocs request');
    expect(result).toHaveTextContent('Responses arrive in your Inbox after the next connect.');
    // No CMS clause at all (no cms items dispatched), and no failure text.
    expect(result.textContent).not.toMatch(/inquiry message to the CMS/);
    expect(result.textContent).not.toMatch(/CMS failed/);

    // grib_send_request called once; catalog_send_inquiry never called.
    const gribCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'grib_send_request');
    expect(gribCalls).toHaveLength(1);
    expect(vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'catalog_send_inquiry')).toHaveLength(
      0,
    );

    // Basket cleared on success.
    const basket = screen.getByTestId('request-basket');
    expect(within(basket).queryAllByTestId(/^basket-item-/)).toHaveLength(0);
  });

  it('retry after partial failure: failed saildocs kept, then a second Send (only saildocs) succeeds and clears', async () => {
    // First send: cms ok, saildocs rejects. Toggle gribFails to flip the
    // grib_send_request behavior between the two Send clicks.
    let gribFails = true;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return C2_ENTRIES;
      if (cmd === 'config_read') return { grid: 'CN87' };
      if (cmd === 'catalog_send_inquiry') return 'MID-CMS-1';
      if (cmd === 'grib_send_request') {
        if (gribFails) throw new Error('saildocs offline');
        return 'MID-GRIB-1';
      }
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    await seedBasket({ cms: 1, saildocs: 1 });
    const basket = screen.getByTestId('request-basket');

    // --- First Send: cms succeeds, saildocs fails ---
    fireEvent.click(screen.getByTestId('request-basket-send'));

    const result = await screen.findByTestId('request-basket-result');
    await waitFor(() => expect(result).toHaveTextContent('Saildocs failed: saildocs offline'));
    // cms item cleared (succeeded rail); saildocs item kept (failed rail).
    await waitFor(() =>
      expect(within(basket).queryByTestId('basket-item-cms:PROP_3DAY')).toBeNull(),
    );
    expect(within(basket).getByTestId(/^basket-item-saildocs:/)).toBeInTheDocument();

    // --- Fix the backend, then re-send the remaining saildocs item ---
    gribFails = false;
    vi.mocked(invoke).mockClear();

    fireEvent.click(screen.getByTestId('request-basket-send'));

    // Second send dispatches only the remaining saildocs item.
    await waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'grib_send_request'),
      ).toHaveLength(1),
    );
    // No cms items remain → catalog_send_inquiry not called the second time.
    expect(vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'catalog_send_inquiry')).toHaveLength(
      0,
    );

    // Saildocs item now clears; basket empty.
    await waitFor(() =>
      expect(within(basket).queryAllByTestId(/^basket-item-/)).toHaveLength(0),
    );

    // Result region shows saildocs success + next-connect, no stale failure text.
    const result2 = screen.getByTestId('request-basket-result');
    expect(result2).toHaveTextContent('Queued 1 Saildocs request');
    expect(result2).toHaveTextContent('Responses arrive in your Inbox after the next connect.');
    expect(result2.textContent).not.toMatch(/Saildocs failed/);
  });

  it('partial failure (cms ok / saildocs fail): saildocs items remain, cms cleared, error shown', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return C2_ENTRIES;
      if (cmd === 'config_read') return { grid: 'CN87' };
      if (cmd === 'catalog_send_inquiry') return 'MID-CMS-1';
      if (cmd === 'grib_send_request') throw new Error('saildocs offline');
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    await seedBasket({ cms: 1, saildocs: 1 });
    fireEvent.click(screen.getByTestId('request-basket-send'));

    const result = await screen.findByTestId('request-basket-result');
    expect(result).toHaveTextContent('saildocs offline');

    const basket = screen.getByTestId('request-basket');
    // cms item gone; saildocs item remains.
    await waitFor(() =>
      expect(within(basket).queryByTestId('basket-item-cms:PROP_3DAY')).toBeNull(),
    );
    expect(within(basket).getByTestId(/^basket-item-saildocs:/)).toBeInTheDocument();
  });

  it('both fail: nothing is cleared, errors shown', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'catalog_list') return C2_ENTRIES;
      if (cmd === 'config_read') return { grid: 'CN87' };
      if (cmd === 'catalog_send_inquiry') throw new Error('cms offline');
      if (cmd === 'grib_send_request') throw new Error('saildocs offline');
      return null;
    });
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    await seedBasket({ cms: 1, saildocs: 1 });
    fireEvent.click(screen.getByTestId('request-basket-send'));

    const result = await screen.findByTestId('request-basket-result');
    expect(result).toHaveTextContent('cms offline');
    expect(result).toHaveTextContent('saildocs offline');

    const basket = screen.getByTestId('request-basket');
    expect(within(basket).getByTestId('basket-item-cms:PROP_3DAY')).toBeInTheDocument();
    expect(within(basket).getByTestId(/^basket-item-saildocs:/)).toBeInTheDocument();
  });

  it('Send all is disabled when the basket is empty and invokes nothing', async () => {
    mockC2('CN87');
    render(<RequestCenter onClose={() => {}} />);
    await screen.findByTestId('request-section-propagation');

    const send = screen.getByTestId('request-basket-send');
    expect(send).toBeDisabled();

    vi.mocked(invoke).mockClear();
    fireEvent.click(send);
    expect(vi.mocked(invoke)).not.toHaveBeenCalled();
  });
});
