// FavoritesTabs + FavoriteRow tests (Task B5).
//
// Covers: per-mode tabs (M7 — ARDOP/packet get Favorites/Recent/Manual; VARA +
// telnet get Manual-only with NO tabs + NO Connect), FavoriteRow head/detail lines
// (gateway·band, freq·grid·distance), telnet head (gateway·transport, no
// freq/band, H7), C4 distance source (position_current_fix NOT position_status)
// + null-grid safety, star-to-promote, and the RADIO-1 purity of Connect
// (onPrefill only — never a *_connect / transmit / record-attempt invoke).

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createElement, type ReactNode } from 'react';

import { invoke } from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { FavoritesTabs } from './FavoritesTabs';
// Raw CSS for the active-tab styling assertion (tuxlink-fr0d), mirroring the
// AppShell.test.tsx pattern of pinning a style rule against regression.
const FAVORITES_TABS_CSS = Object.values(
  import.meta.glob('./FavoritesTabs.css', { query: '?raw', import: 'default', eager: true }),
)[0] as string;
import { FavoriteRow } from './FavoriteRow';
import type { Favorite, FavoriteDial, RadioMode, StationsFile } from './types';
import { FIXTURE_FAVORITES, FIXTURE_RECENTS, FIXTURE_ATTEMPTS } from './favorites-fixture';

const invokeMock = invoke as ReturnType<typeof vi.fn>;

// Routed mock: every command FavoritesTabs/FavoriteRow/ConnectionRecord touch
// MUST return a Promise. position_current_fix is the C4 operator-grid source.
function routeInvoke(opts: {
  favorites?: Favorite[];
  recents?: Favorite[];
  log?: StationsFile['log'];
  grid?: string | null;
} = {}) {
  const stations: StationsFile = {
    schema_version: 1,
    favorites: opts.favorites ?? FIXTURE_FAVORITES,
    log: opts.log ?? FIXTURE_ATTEMPTS,
  };
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'favorites_read') return Promise.resolve(stations);
    if (cmd === 'favorites_recents') return Promise.resolve(opts.recents ?? FIXTURE_RECENTS);
    if (cmd === 'position_current_fix')
      return Promise.resolve({ grid: opts.grid === undefined ? 'CN87us' : opts.grid });
    if (cmd === 'favorite_tod_hint') return Promise.resolve(null);
    if (cmd === 'favorite_star') return Promise.resolve(undefined);
    return Promise.resolve(undefined);
  });
}

function renderTabs(props: {
  mode: RadioMode;
  onPrefill?: (dial: FavoriteDial) => void;
  manualContent?: ReactNode;
}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
  return render(
    createElement(FavoritesTabs, {
      mode: props.mode,
      onPrefill: props.onPrefill ?? (() => {}),
      manualContent:
        props.manualContent ??
        createElement('div', { 'data-testid': 'manual-content' }, 'MANUAL FORM'),
    }),
    { wrapper },
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  routeInvoke();
});

describe('<FavoritesTabs> — per-mode chrome (M7)', () => {
  it('ardop-hf renders Favorites/Recent/Manual triggers; Recent shows recents; Manual shows manualContent', async () => {
    renderTabs({ mode: 'ardop-hf' });

    // All three triggers present.
    expect(await screen.findByRole('tab', { name: 'Favorites' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Recent' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Manual' })).toBeInTheDocument();

    // Default tab = Favorites → a starred favorite is visible.
    expect(await screen.findByTestId('favorite-row-fav-w7dxg')).toBeInTheDocument();

    // Switch to Recent → a recent row is visible. (Radix Tabs selects on
    // mouseDown with button 0 — fireEvent.click alone does not switch it.)
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Recent' }), { button: 0 });
    expect(await screen.findByTestId('favorite-row-rec-w6drz')).toBeInTheDocument();

    // Switch to Manual → the passthrough content shows.
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Manual' }), { button: 0 });
    expect(await screen.findByTestId('manual-content')).toBeInTheDocument();
  });

  it('packet gets the three-tab chrome', async () => {
    routeInvoke({ favorites: FIXTURE_FAVORITES });
    renderTabs({ mode: 'packet' });
    expect(await screen.findByRole('tab', { name: 'Favorites' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Recent' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Manual' })).toBeInTheDocument();
  });

  // tuxlink-fr0d: telnet connects to a FIXED CMS host — there is no nearby-
  // station choice to favorite or redial, so it is Manual-only like VARA
  // (operator converged-build smoke: Telnet Winlink CMS doesn't need this).
  it('telnet is Manual-only — NO Favorites/Recent tabs (tuxlink-fr0d)', async () => {
    routeInvoke({ favorites: FIXTURE_FAVORITES });
    renderTabs({ mode: 'telnet' });
    expect(await screen.findByTestId('manual-content')).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Favorites' })).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Recent' })).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Manual' })).not.toBeInTheDocument();
  });

  it('VARA (vara-hf) renders manualContent and NO Favorites/Recent triggers and NO Connect button', async () => {
    renderTabs({ mode: 'vara-hf' });

    // Manual content is shown directly.
    expect(await screen.findByTestId('manual-content')).toBeInTheDocument();

    // No tab chrome at all.
    expect(screen.queryByRole('tab', { name: 'Favorites' })).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Recent' })).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Manual' })).not.toBeInTheDocument();

    // No FavoriteRow, hence no Connect button, anywhere.
    expect(screen.queryByText('Connect')).not.toBeInTheDocument();
  });

  it('VARA (vara-fm) is also Manual-only', async () => {
    renderTabs({ mode: 'vara-fm' });
    expect(await screen.findByTestId('manual-content')).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Favorites' })).not.toBeInTheDocument();
  });
});

// tuxlink-fgc1: the first Contacts+Favorites tab chrome used generic web tabs
// and global app accent actions. Pin it to the radio-panel segmented/action
// language so the selector feels native to the modem pane.
describe('FavoritesTabs.css radio-panel styling (tuxlink-fgc1)', () => {
  it('renders the tab list as a bounded segmented control, not an underline tab strip', () => {
    const start = FAVORITES_TABS_CSS.indexOf('.favorites-tabs-list');
    expect(start).toBeGreaterThan(-1);
    const block = FAVORITES_TABS_CSS.slice(start, FAVORITES_TABS_CSS.indexOf('}', start) + 1);
    expect(block).toMatch(/display:\s*grid/);
    expect(block).toMatch(/grid-template-columns:\s*repeat\(3,\s*minmax\(0,\s*1fr\)\)/);
    expect(block).toMatch(/border:\s*1px solid var\(--border\)/);
    expect(block).toMatch(/border-radius:\s*6px/);
    expect(block).not.toMatch(/border-bottom/);
  });

  it('uses button-like triggers instead of border-bottom tabs', () => {
    const start = FAVORITES_TABS_CSS.indexOf('.favorites-tab-trigger {');
    expect(start).toBeGreaterThan(-1);
    const block = FAVORITES_TABS_CSS.slice(start, FAVORITES_TABS_CSS.indexOf('}', start) + 1);
    expect(block).toMatch(/border:\s*1px solid transparent/);
    expect(block).toMatch(/border-radius:\s*4px/);
    expect(block).not.toMatch(/border-bottom/);
  });

  it('uses the radio-panel modem-accent (not the global app accent) for the active tab', () => {
    const start = FAVORITES_TABS_CSS.indexOf(".favorites-tab-trigger[data-state='active']");
    expect(start).toBeGreaterThan(-1);
    const block = FAVORITES_TABS_CSS.slice(start, FAVORITES_TABS_CSS.indexOf('}', start) + 1);
    expect(block).toMatch(/color:\s*var\(--modem-accent\)/);
    expect(block).toMatch(/background:\s*var\(--modem-accent-soft\)/);
    expect(block).toMatch(/border-color:\s*color-mix\(in srgb,\s*var\(--modem-accent\) 35%, transparent\)/);
    expect(block).not.toMatch(/var\(--accent\)/);
    expect(block).not.toMatch(/border-bottom/);
  });

  it('uses the radio-panel modem-accent for stars, records, and Connect', () => {
    const starStart = FAVORITES_TABS_CSS.indexOf('.favorite-star--on');
    const connectStart = FAVORITES_TABS_CSS.indexOf('.favorite-connect {');
    const glyphStart = FAVORITES_TABS_CSS.indexOf('.favorite-record-glyph--ok');
    expect(starStart).toBeGreaterThan(-1);
    expect(connectStart).toBeGreaterThan(-1);
    expect(glyphStart).toBeGreaterThan(-1);

    const starBlock = FAVORITES_TABS_CSS.slice(
      starStart,
      FAVORITES_TABS_CSS.indexOf('}', starStart) + 1,
    );
    const connectBlock = FAVORITES_TABS_CSS.slice(
      connectStart,
      FAVORITES_TABS_CSS.indexOf('}', connectStart) + 1,
    );
    const glyphBlock = FAVORITES_TABS_CSS.slice(
      glyphStart,
      FAVORITES_TABS_CSS.indexOf('}', glyphStart) + 1,
    );

    expect(starBlock).toMatch(/color:\s*var\(--modem-accent\)/);
    expect(connectBlock).toMatch(/color:\s*var\(--modem-accent\)/);
    expect(connectBlock).toMatch(/background:\s*var\(--modem-accent-soft\)/);
    expect(connectBlock).toMatch(/border:\s*1px solid color-mix\(in srgb,\s*var\(--modem-accent\) 35%, transparent\)/);
    expect(glyphBlock).toMatch(/color:\s*var\(--modem-accent\)/);
    expect(`${starBlock}\n${connectBlock}\n${glyphBlock}`).not.toMatch(/var\(--accent\)/);
  });
});

// tuxlink-sm22: ARDOP mounts <FavoritesTabs> as a BARE flex child of the
// .radio-panel-body column (Packet/Telnet wrap it in a .radio-panel-sec, which
// has implicit min-height:auto and refuses to shrink). .favorites-tabs sets
// min-height:0 with no overflow clip, so a long favorites list let flexbox
// shrink the surface below its content height — and the overflowing rows
// painted OVER the Radio + Start sections that flow below it (the panel body
// never scrolled because flex "fit" everything by shrinking). flex-shrink:0
// keeps the surface at content height so the body's overflow-y:auto scrolls
// instead. No-op where FavoritesTabs is a block child of a section.
describe('FavoritesTabs.css flow containment (tuxlink-sm22)', () => {
  it('refuses flex shrink so a long list cannot overflow and overlap sibling sections', () => {
    const start = FAVORITES_TABS_CSS.indexOf('.favorites-tabs {');
    expect(start).toBeGreaterThan(-1);
    const block = FAVORITES_TABS_CSS.slice(start, FAVORITES_TABS_CSS.indexOf('}', start) + 1);
    expect(block).toMatch(/flex-shrink:\s*0/);
  });
});

describe('<FavoritesTabs> — C4 distance source', () => {
  it('invokes position_current_fix and NOT position_status for the operator grid', async () => {
    renderTabs({ mode: 'ardop-hf' });
    await waitFor(() => {
      expect(
        invokeMock.mock.calls.some(([cmd]) => cmd === 'position_current_fix'),
      ).toBe(true);
    });
    expect(
      invokeMock.mock.calls.some(([cmd]) => cmd === 'position_status'),
    ).toBe(false);
  });

  it('does not crash and shows no distance segment when the operator grid is null (C4)', async () => {
    routeInvoke({ grid: null });
    renderTabs({ mode: 'ardop-hf' });
    const row = await screen.findByTestId('favorite-row-fav-w7dxg');
    // freq + grid still show; distance segment absent (no "km", no "null").
    const detail = within(row).getByTestId('favorite-detail-fav-w7dxg');
    expect(detail.textContent).toContain('14105.0');
    expect(detail.textContent).toContain('CN87');
    expect(detail.textContent).not.toContain('km');
    expect(detail.textContent?.toLowerCase()).not.toContain('null');
  });
});

describe('<FavoritesTabs> — star-to-promote', () => {
  it('clicking a Recent row star invokes favorite_star with {id, starred:true}', async () => {
    renderTabs({ mode: 'ardop-hf' });
    fireEvent.mouseDown(await screen.findByRole('tab', { name: 'Recent' }), { button: 0 });

    const star = await screen.findByTestId('favorite-star-rec-w6drz');
    fireEvent.click(star);

    await waitFor(() => {
      const call = invokeMock.mock.calls.find(([cmd]) => cmd === 'favorite_star');
      expect(call).toBeTruthy();
      const args = call?.[1] as { id: string; starred: boolean };
      expect(args.id).toBe('rec-w6drz');
      expect(args.starred).toBe(true);
    });
  });
});

// ---- FavoriteRow focused tests ----

const RF_FAV: Favorite = {
  id: 'rf1',
  mode: 'ardop-hf',
  gateway: 'W7DXG',
  freq: '14105.0',
  band: '20m',
  grid: 'CN87',
  starred: true,
  created_at: '2026-06-01T00:00:00-07:00',
  updated_at: '2026-06-01T00:00:00-07:00',
};

const TELNET_FAV: Favorite = {
  id: 'tn1',
  mode: 'telnet',
  gateway: 'cms.winlink.org',
  transport: 'CmsSsl',
  starred: false,
  created_at: '2026-06-01T00:00:00-07:00',
  updated_at: '2026-06-01T00:00:00-07:00',
};

function renderRow(props: {
  favorite: Favorite;
  operatorGrid: string | null;
  onPrefill?: (d: FavoriteDial) => void;
  onToggleStar?: (id: string, s: boolean) => void;
  onUpsert?: (f: Favorite) => void;
  onDelete?: (id: string) => void;
}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
  return render(
    createElement(FavoriteRow, {
      favorite: props.favorite,
      operatorGrid: props.operatorGrid,
      onPrefill: props.onPrefill ?? (() => {}),
      onToggleStar: props.onToggleStar ?? (() => {}),
      onUpsert: props.onUpsert,
      onDelete: props.onDelete,
    }),
    { wrapper },
  );
}

describe('<FavoriteRow>', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'favorite_tod_hint') return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
  });

  it('RF row shows gateway · band and freq · grid · distance with a valid grid pair', async () => {
    renderRow({ favorite: RF_FAV, operatorGrid: 'CN85nx' });
    expect(screen.getByText('W7DXG')).toBeInTheDocument();
    expect(screen.getByText(/20m/)).toBeInTheDocument();
    const detail = screen.getByTestId('favorite-detail-rf1');
    expect(detail.textContent).toContain('14105.0');
    expect(detail.textContent).toContain('CN87');
    expect(detail.textContent).toMatch(/\d+ km/); // a distance string appears
  });

  it('RF row OMITS distance (no crash, no "null") when favorite.grid is absent (C4)', async () => {
    const noGrid: Favorite = { ...RF_FAV, grid: undefined };
    renderRow({ favorite: noGrid, operatorGrid: 'CN85nx' });
    const detail = screen.getByTestId('favorite-detail-rf1');
    expect(detail.textContent).toContain('14105.0');
    expect(detail.textContent).not.toContain('km');
    expect(detail.textContent?.toLowerCase()).not.toContain('null');
  });

  it('RF row OMITS distance when operatorGrid is null (C4)', async () => {
    renderRow({ favorite: RF_FAV, operatorGrid: null });
    const detail = screen.getByTestId('favorite-detail-rf1');
    expect(detail.textContent).not.toContain('km');
  });

  it('telnet row shows gateway · transport and NO freq/band/distance (H7)', async () => {
    renderRow({ favorite: TELNET_FAV, operatorGrid: 'CN85nx' });
    expect(screen.getByText('cms.winlink.org')).toBeInTheDocument();
    expect(screen.getByText(/CmsSsl/)).toBeInTheDocument();
    // No RF detail line at all for telnet.
    expect(screen.queryByTestId('favorite-detail-tn1')).not.toBeInTheDocument();
  });

  it('clicking the star calls onToggleStar(id, !starred)', async () => {
    const onToggleStar = vi.fn();
    renderRow({ favorite: TELNET_FAV, operatorGrid: null, onToggleStar });
    fireEvent.click(screen.getByTestId('favorite-star-tn1'));
    expect(onToggleStar).toHaveBeenCalledWith('tn1', true); // was unstarred → true
  });

  it('Connect calls onPrefill with the dial and invokes NO connect/transmit/record-attempt command (RADIO-1)', async () => {
    const onPrefill = vi.fn();
    renderRow({ favorite: RF_FAV, operatorGrid: 'CN85nx', onPrefill });
    fireEvent.click(screen.getByTestId('favorite-connect-rf1'));

    expect(onPrefill).toHaveBeenCalledTimes(1);
    const dial = onPrefill.mock.calls[0][0] as FavoriteDial;
    expect(dial).toEqual({
      mode: 'ardop-hf',
      gateway: 'W7DXG',
      freq: '14105.0',
      transport: undefined,
      band: '20m',
      grid: 'CN87',
    });

    // RADIO-1: the row must never have invoked a connect/transmit/record path.
    const forbidden = invokeMock.mock.calls.filter(([cmd]) =>
      /_connect$|connect_|transmit|record_attempt/.test(cmd),
    );
    expect(forbidden).toEqual([]);
  });
});

// ---- FavoriteRow edit/delete (tuxlink-oi1g) ----
describe('<FavoriteRow> edit/delete (oi1g)', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'favorite_tod_hint') return Promise.resolve(null);
      return Promise.resolve(undefined);
    });
  });

  it('renders NO overflow menu when no edit handlers are provided (view-only)', () => {
    renderRow({ favorite: RF_FAV, operatorGrid: 'CN85nx' });
    expect(screen.queryByTestId('favorite-menu-rf1')).toBeNull();
  });

  it('shows the overflow menu with Edit + Delete when edit handlers are provided', () => {
    renderRow({ favorite: RF_FAV, operatorGrid: 'CN85nx', onUpsert: () => {}, onDelete: () => {} });
    fireEvent.click(screen.getByTestId('favorite-menu-rf1'));
    expect(screen.getByTestId('favorite-edit-rf1')).toBeInTheDocument();
    expect(screen.getByTestId('favorite-delete-rf1')).toBeInTheDocument();
  });

  it('Edit reveals the inline form pre-filled with current values', () => {
    renderRow({ favorite: RF_FAV, operatorGrid: 'CN85nx', onUpsert: () => {}, onDelete: () => {} });
    fireEvent.click(screen.getByTestId('favorite-menu-rf1'));
    fireEvent.click(screen.getByTestId('favorite-edit-rf1'));
    expect((screen.getByTestId('favorite-edit-gateway-rf1') as HTMLInputElement).value).toBe('W7DXG');
    expect((screen.getByTestId('favorite-edit-band-rf1') as HTMLInputElement).value).toBe('20m');
    expect((screen.getByTestId('favorite-edit-grid-rf1') as HTMLInputElement).value).toBe('CN87');
    expect((screen.getByTestId('favorite-edit-freq-rf1') as HTMLInputElement).value).toBe('14105.0');
  });

  it('Save calls onUpsert with the merged favorite (edited fields applied, id + mode preserved)', () => {
    const onUpsert = vi.fn();
    renderRow({ favorite: RF_FAV, operatorGrid: 'CN85nx', onUpsert, onDelete: () => {} });
    fireEvent.click(screen.getByTestId('favorite-menu-rf1'));
    fireEvent.click(screen.getByTestId('favorite-edit-rf1'));
    fireEvent.change(screen.getByTestId('favorite-edit-gateway-rf1'), { target: { value: 'W7NEW' } });
    fireEvent.change(screen.getByTestId('favorite-edit-band-rf1'), { target: { value: '40m' } });
    fireEvent.click(screen.getByTestId('favorite-edit-save-rf1'));
    expect(onUpsert).toHaveBeenCalledTimes(1);
    const saved = onUpsert.mock.calls[0][0] as Favorite;
    expect(saved.id).toBe('rf1');
    expect(saved.mode).toBe('ardop-hf');
    expect(saved.gateway).toBe('W7NEW');
    expect(saved.band).toBe('40m');
    expect(saved.grid).toBe('CN87'); // untouched field preserved
  });

  it('Cancel collapses the edit form without calling onUpsert', () => {
    const onUpsert = vi.fn();
    renderRow({ favorite: RF_FAV, operatorGrid: 'CN85nx', onUpsert, onDelete: () => {} });
    fireEvent.click(screen.getByTestId('favorite-menu-rf1'));
    fireEvent.click(screen.getByTestId('favorite-edit-rf1'));
    fireEvent.click(screen.getByTestId('favorite-edit-cancel-rf1'));
    expect(onUpsert).not.toHaveBeenCalled();
    expect(screen.queryByTestId('favorite-edit-gateway-rf1')).toBeNull();
  });

  it('Delete requires an inline confirm before calling onDelete(id)', () => {
    const onDelete = vi.fn();
    renderRow({ favorite: RF_FAV, operatorGrid: 'CN85nx', onUpsert: () => {}, onDelete });
    fireEvent.click(screen.getByTestId('favorite-menu-rf1'));
    fireEvent.click(screen.getByTestId('favorite-delete-rf1'));
    // Not deleted yet — a confirm affordance appears first.
    expect(onDelete).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId('favorite-delete-confirm-rf1'));
    expect(onDelete).toHaveBeenCalledWith('rf1');
  });
});

// ---- FavoritesTabs filter + edit wiring (tuxlink-oi1g) ----
function makeFavs(n: number, prefix = 'W'): Favorite[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `gen-${i}`,
    mode: 'ardop-hf' as const,
    gateway: `${prefix}7G${String(i).padStart(2, '0')}`,
    band: '20m',
    grid: 'CN87',
    starred: true,
    created_at: '2026-06-01T00:00:00-07:00',
    updated_at: '2026-06-01T00:00:00-07:00',
  }));
}

describe('<FavoritesTabs> filter + edit (oi1g)', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('does NOT show the filter box when the favorites list is short (<= 8)', async () => {
    routeInvoke({ favorites: makeFavs(3), recents: [] });
    renderTabs({ mode: 'ardop-hf' });
    expect(await screen.findByTestId('favorite-row-gen-0')).toBeInTheDocument();
    expect(screen.queryByTestId('favorites-filter-input')).toBeNull();
  });

  it('shows the filter box when the favorites list is long (> 8) and narrows on input', async () => {
    routeInvoke({ favorites: makeFavs(10), recents: [] });
    renderTabs({ mode: 'ardop-hf' });
    const filter = await screen.findByTestId('favorites-filter-input');
    expect(filter).toBeInTheDocument();
    // All 10 present initially.
    expect(screen.getByTestId('favorite-row-gen-0')).toBeInTheDocument();
    expect(screen.getByTestId('favorite-row-gen-9')).toBeInTheDocument();
    // Filter to a single gateway substring.
    fireEvent.change(filter, { target: { value: 'G09' } });
    expect(screen.getByTestId('favorite-row-gen-9')).toBeInTheDocument();
    expect(screen.queryByTestId('favorite-row-gen-0')).toBeNull();
  });

  it('editing a favorite from the row invokes favorite_upsert', async () => {
    routeInvoke({ favorites: makeFavs(3), recents: [] });
    renderTabs({ mode: 'ardop-hf' });
    await screen.findByTestId('favorite-row-gen-0');
    fireEvent.click(screen.getByTestId('favorite-menu-gen-0'));
    fireEvent.click(screen.getByTestId('favorite-edit-gen-0'));
    fireEvent.change(screen.getByTestId('favorite-edit-band-gen-0'), { target: { value: '40m' } });
    fireEvent.click(screen.getByTestId('favorite-edit-save-gen-0'));
    await waitFor(() => {
      const calls = invokeMock.mock.calls.filter(([cmd]) => cmd === 'favorite_upsert');
      expect(calls).toHaveLength(1);
      expect((calls[0][1] as { favorite: Favorite }).favorite.band).toBe('40m');
      expect((calls[0][1] as { favorite: Favorite }).favorite.id).toBe('gen-0');
    });
  });

  it('deleting a favorite from the row invokes favorite_delete after confirm', async () => {
    routeInvoke({ favorites: makeFavs(3), recents: [] });
    renderTabs({ mode: 'ardop-hf' });
    await screen.findByTestId('favorite-row-gen-1');
    fireEvent.click(screen.getByTestId('favorite-menu-gen-1'));
    fireEvent.click(screen.getByTestId('favorite-delete-gen-1'));
    expect(invokeMock.mock.calls.filter(([c]) => c === 'favorite_delete')).toHaveLength(0);
    fireEvent.click(screen.getByTestId('favorite-delete-confirm-gen-1'));
    await waitFor(() => {
      const calls = invokeMock.mock.calls.filter(([cmd]) => cmd === 'favorite_delete');
      expect(calls).toHaveLength(1);
      expect((calls[0][1] as { id: string }).id).toBe('gen-1');
    });
  });
});
