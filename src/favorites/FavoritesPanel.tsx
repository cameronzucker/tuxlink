// FavoritesPanel — the shell-level, cross-mode Favorites home (bd-tuxlink-kiaa).
//
// Mounts in the Address section (selectedFolder === 'favorites'), spanning the
// content area like ContactsPanel. Distinct from FavoritesTabs (per-mode, inside
// a modem panel): this surface reads the WHOLE StationsFile once and shows every
// saved station grouped by mode, so favorites are reachable without opening a
// radio panel first.
//
// RADIO-1: a row's Connect is pure prefill (FavoriteRow calls onPrefill only).
// `onConnect` here opens + arms the matching modem panel (AppShell wires it to
// onSelectConnection + emitGatewayPrefill, mirroring Find-a-Station's
// handleStationUse); the operator then clicks the panel's own Send/Receive.
// This panel NEVER invokes a connect/transmit/record command.
//
// C4: operator grid comes from `position_current_fix` (FULL precision), never
// `position_status` (precision-reduced for broadcast).

import { useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import './FavoritesPanel.css';
import type { Favorite, FavoriteDial, RadioMode, StationsFile } from './types';
import { FAVORITES_QUERY_KEY } from './useFavorites';
import { FavoriteRow } from './FavoriteRow';

export interface FavoritesPanelProps {
  /** RADIO-1: open + arm the matching modem for this dial. NEVER transmits. */
  onConnect: (dial: FavoriteDial) => void;
}

/** Above this row count a tab shows a filter box (matches FavoritesTabs). */
const FILTER_THRESHOLD = 8;

/** Stable display order + labels for the per-mode group headers. */
const MODE_ORDER: readonly { mode: RadioMode; label: string }[] = [
  { mode: 'vara-hf', label: 'VARA HF' },
  { mode: 'vara-fm', label: 'VARA FM' },
  { mode: 'ardop-hf', label: 'ARDOP HF' },
  { mode: 'packet', label: 'Packet' },
  { mode: 'telnet', label: 'Telnet' },
];

function matchesFilter(f: Favorite, q: string): boolean {
  const needle = q.trim().toLowerCase();
  if (!needle) return true;
  return (
    f.gateway.toLowerCase().includes(needle) ||
    (f.grid ?? '').toLowerCase().includes(needle) ||
    (f.note ?? '').toLowerCase().includes(needle)
  );
}

/** Recents sort: most-recently-attempted first; never-attempted last. */
function byRecency(a: Favorite, b: Favorite): number {
  const at = a.last_attempt_at ?? '';
  const bt = b.last_attempt_at ?? '';
  if (at === bt) return 0;
  if (!at) return 1;
  if (!bt) return -1;
  return at < bt ? 1 : -1;
}

export function FavoritesPanel({ onConnect }: FavoritesPanelProps) {
  const qc = useQueryClient();
  const [tab, setTab] = useState<'favorites' | 'recent'>('favorites');
  const [filter, setFilter] = useState('');

  const stationsQuery = useQuery({
    queryKey: FAVORITES_QUERY_KEY,
    queryFn: () => invoke<StationsFile>('favorites_read'),
  });
  const allFavorites = useMemo(
    () => stationsQuery.data?.favorites ?? [],
    [stationsQuery.data],
  );
  const log = useMemo(() => stationsQuery.data?.log ?? [], [stationsQuery.data]);

  // C4: full-precision operator grid for the per-row distance. Fetched once.
  const fixQuery = useQuery({
    queryKey: ['position_current_fix'],
    queryFn: () => invoke<{ grid: string | null }>('position_current_fix'),
  });
  const operatorGrid = fixQuery.data?.grid ?? null;

  // Mutations mirror useFavorites: invoke, swallow errors (Cross-cutting §1),
  // then prefix-invalidate ['favorites'] to refetch this panel and any open tab.
  const invalidate = () => qc.invalidateQueries({ queryKey: FAVORITES_QUERY_KEY });
  const star = async (id: string, starred: boolean) => {
    await invoke('favorite_star', { id, starred }).catch(() => {});
    await invalidate();
  };
  const upsert = async (favorite: Favorite) => {
    await invoke('favorite_upsert', { favorite }).catch(() => {});
    await invalidate();
  };
  const remove = async (id: string) => {
    await invoke('favorite_delete', { id }).catch(() => {});
    await invalidate();
  };

  const list = useMemo(() => {
    const base =
      tab === 'favorites'
        ? allFavorites.filter((f) => f.starred)
        : allFavorites.filter((f) => !f.starred).sort(byRecency);
    return base;
  }, [allFavorites, tab]);

  const showFilter = list.length > FILTER_THRESHOLD;
  const shown = showFilter ? list.filter((f) => matchesFilter(f, filter)) : list;

  // Group the shown rows by mode, preserving MODE_ORDER and each mode's own
  // ordering within the list (recency for Recent, file order for Favorites).
  const groups = useMemo(
    () =>
      MODE_ORDER.map(({ mode, label }) => ({
        mode,
        label,
        rows: shown.filter((f) => f.mode === mode),
      })).filter((g) => g.rows.length > 0),
    [shown],
  );

  const emptyMessage =
    tab === 'favorites'
      ? 'No saved stations yet — star one from a radio panel or Find a Station.'
      : 'No recent connections yet.';

  return (
    <div className="favorites-panel" data-testid="favorites-panel">
      <div className="favorites-panel-tabs" role="tablist" aria-label="Favorites or recent stations">
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'favorites'}
          data-testid="favorites-panel-tab-favorites"
          className={`favorites-panel-tab${tab === 'favorites' ? ' favorites-panel-tab--active' : ''}`}
          onClick={() => setTab('favorites')}
        >
          Favorites
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'recent'}
          data-testid="favorites-panel-tab-recent"
          className={`favorites-panel-tab${tab === 'recent' ? ' favorites-panel-tab--active' : ''}`}
          onClick={() => setTab('recent')}
        >
          Recent
        </button>
      </div>

      {showFilter && (
        <div className="favorites-panel-filter">
          <input
            type="text"
            data-testid="favorites-panel-filter-input"
            placeholder="Filter… (call / grid / note)"
            value={filter}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            aria-label="Filter favorites"
            onChange={(e) => setFilter(e.target.value)}
          />
          <span className="favorites-panel-filter-count" data-testid="favorites-panel-filter-count">
            {shown.length}/{list.length}
          </span>
        </div>
      )}

      {shown.length === 0 ? (
        <p className="favorites-panel-empty" data-testid="favorites-panel-empty">
          {list.length === 0 ? emptyMessage : 'No matches.'}
        </p>
      ) : (
        <div className="favorites-panel-groups">
          {groups.map((g) => (
            <section key={g.mode} className="favorites-panel-group">
              <h3 className="favorites-panel-group-head" data-testid={`favorites-group-${g.mode}`}>
                <span className="favorites-panel-group-pill">{g.label}</span>
              </h3>
              <div className="favorites-panel-list">
                {g.rows.map((f) => (
                  <FavoriteRow
                    key={f.id}
                    favorite={f}
                    operatorGrid={operatorGrid}
                    onPrefill={onConnect}
                    onToggleStar={star}
                    attempts={log.filter((a) => a.unit_id === f.id)}
                    onUpsert={upsert}
                    onDelete={remove}
                  />
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
