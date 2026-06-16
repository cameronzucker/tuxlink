// FavoritesPanel tests (bd-tuxlink-kiaa) — the shell-level, cross-mode Favorites
// home (Address section). Distinct from FavoritesTabs (per-mode, inside a modem
// panel): this surface reads the WHOLE StationsFile, groups by mode, and a row's
// Connect opens+arms the matching modem via the onConnect handler — never a
// transmit (RADIO-1; FavoriteRow Connect is pure onPrefill).

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createElement, type ReactNode } from 'react';

import { invoke } from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { FavoritesPanel } from './FavoritesPanel';
import type { Favorite, FavoriteDial, StationsFile } from './types';

const invokeMock = invoke as ReturnType<typeof vi.fn>;

function fav(over: Partial<Favorite>): Favorite {
  return {
    id: 'x',
    mode: 'ardop-hf',
    gateway: 'W7X',
    starred: true,
    created_at: '2026-06-01T00:00:00-07:00',
    updated_at: '2026-06-01T00:00:00-07:00',
    ...over,
  };
}

function route(favorites: Favorite[], grid: string | null = 'CN87us') {
  const stations: StationsFile = { schema_version: 1, favorites, log: [] };
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'favorites_read') return Promise.resolve(stations);
    if (cmd === 'position_current_fix') return Promise.resolve({ grid });
    if (cmd === 'favorite_tod_hint') return Promise.resolve(null);
    return Promise.resolve(undefined);
  });
}

function renderPanel(onConnect: (d: FavoriteDial) => void = () => {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
  return render(createElement(FavoritesPanel, { onConnect }), { wrapper });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe('<FavoritesPanel> — cross-mode Favorites home', () => {
  it('Favorites tab groups STARRED favorites by mode (cross-mode, one home)', async () => {
    route([
      fav({ id: 'a', mode: 'ardop-hf', gateway: 'W7ARD', starred: true }),
      fav({ id: 'b', mode: 'packet', gateway: 'W7PKT', starred: true }),
      fav({ id: 'c', mode: 'vara-hf', gateway: 'W7VAR', starred: true }),
    ]);
    renderPanel();

    expect(await screen.findByTestId('favorite-row-a')).toBeInTheDocument();
    expect(screen.getByTestId('favorite-row-b')).toBeInTheDocument();
    expect(screen.getByTestId('favorite-row-c')).toBeInTheDocument();
    // Per-mode group headers.
    expect(screen.getByTestId('favorites-group-ardop-hf')).toBeInTheDocument();
    expect(screen.getByTestId('favorites-group-packet')).toBeInTheDocument();
    expect(screen.getByTestId('favorites-group-vara-hf')).toBeInTheDocument();
  });

  it('default tab is Favorites: non-starred rows are NOT shown until Recent is selected', async () => {
    route([
      fav({ id: 'star1', starred: true }),
      fav({ id: 'rec1', starred: false }),
    ]);
    renderPanel();

    expect(await screen.findByTestId('favorite-row-star1')).toBeInTheDocument();
    expect(screen.queryByTestId('favorite-row-rec1')).toBeNull();
  });

  it('Recent tab shows NON-starred favorites, most-recent first', async () => {
    route([
      fav({ id: 'older', mode: 'ardop-hf', starred: false, last_attempt_at: '2026-06-01T00:00:00-07:00' }),
      fav({ id: 'newer', mode: 'ardop-hf', starred: false, last_attempt_at: '2026-06-10T00:00:00-07:00' }),
    ]);
    renderPanel();

    fireEvent.click(await screen.findByTestId('favorites-panel-tab-recent'));

    const rows = await screen.findAllByTestId(/^favorite-row-/);
    const ids = rows.map((r) => r.getAttribute('data-testid'));
    expect(ids).toEqual(['favorite-row-newer', 'favorite-row-older']);
  });

  it('shows an empty state when there are no favorites at all', async () => {
    route([]);
    renderPanel();
    expect(await screen.findByTestId('favorites-panel-empty')).toBeInTheDocument();
  });

  it('a row Connect calls onConnect with the dial (open+arm; NO transmit — RADIO-1)', async () => {
    route([fav({ id: 'a', mode: 'ardop-hf', gateway: 'W7ARD', freq: '14105.0', band: '20m', grid: 'CN87', starred: true })]);
    const onConnect = vi.fn();
    renderPanel(onConnect);

    fireEvent.click(await screen.findByTestId('favorite-connect-a'));

    expect(onConnect).toHaveBeenCalledTimes(1);
    expect(onConnect).toHaveBeenCalledWith({
      mode: 'ardop-hf',
      gateway: 'W7ARD',
      freq: '14105.0',
      transport: undefined,
      band: '20m',
      grid: 'CN87',
    });
    // RADIO-1: the panel never fires a connect/transmit/record command.
    const forbidden = invokeMock.mock.calls.filter(([cmd]) =>
      /_connect$|connect_|transmit|record_attempt/.test(cmd as string),
    );
    expect(forbidden).toEqual([]);
  });

  it('clicking a row star invokes favorite_star', async () => {
    route([fav({ id: 'a', starred: true })]);
    renderPanel();
    fireEvent.click(await screen.findByTestId('favorite-star-a'));
    await waitFor(() => {
      const call = invokeMock.mock.calls.find(([cmd]) => cmd === 'favorite_star');
      expect(call).toBeTruthy();
      expect((call?.[1] as { id: string }).id).toBe('a');
    });
  });

  it('shows a filter box when a list exceeds 8 rows and narrows on input', async () => {
    const many = Array.from({ length: 10 }, (_, i) =>
      fav({ id: `g${i}`, mode: 'ardop-hf', gateway: `W7G${String(i).padStart(2, '0')}`, grid: 'CN87', starred: true }),
    );
    route(many);
    renderPanel();

    const filter = await screen.findByTestId('favorites-panel-filter-input');
    expect(screen.getByTestId('favorite-row-g0')).toBeInTheDocument();
    expect(screen.getByTestId('favorite-row-g9')).toBeInTheDocument();
    fireEvent.change(filter, { target: { value: 'G09' } });
    expect(screen.getByTestId('favorite-row-g9')).toBeInTheDocument();
    expect(screen.queryByTestId('favorite-row-g0')).toBeNull();
  });
});
