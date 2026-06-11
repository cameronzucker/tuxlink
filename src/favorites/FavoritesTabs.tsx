// FavoritesTabs — the per-mode Favorites / Recent / Manual surface (Task B5).
//
// Mounts INSIDE a radio mode panel body (B6). Inline only — no popup.
//
// M7 — per-mode chrome (amended tuxlink-fr0d; VARA exclusion retired tuxlink-xglf):
//   · ardop-hf / packet / vara-hf / vara-fm → Radix Tabs: Favorites | Recent |
//     Manual. These are the genuine pre-dial modes — the operator picks among
//     many nearby gateways, so favoriting / redialing is meaningful. VARA HF
//     dials RMS gateways exactly like ARDOP HF; it was Manual-only at M7 only
//     because it had no working dial yet (a favorite's Connect would have been
//     dead). tuxlink-xglf wired modem_vara_b2f_exchange to the pane, so VARA
//     now gets the full chrome.
//   · telnet → Manual content ONLY (no tabs, no Favorites/Recent lists, no
//     FavoriteRow, no Connect button). Telnet connects to a FIXED CMS host —
//     there is no nearby-station choice to favorite (tuxlink-fr0d operator
//     smoke: Telnet Winlink CMS doesn't need this).
//
// C4 — distance source: the operator grid comes from `position_current_fix`
// (FULL precision), NEVER `position_status`/`useStatus` (those are
// precision-reduced for broadcast).
//
// RADIO-1: `onPrefill` is the ONLY connect-related callback this surface exposes.
// FavoriteRow's Connect drops a dial into the host form; the operator clicks the
// panel's own Send/Receive (the Part 97 consent click).

import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import * as Tabs from '@radix-ui/react-tabs';
import './FavoritesTabs.css';
import type { FavoriteDial, RadioMode, StationsFile } from './types';
import { useFavorites, FAVORITES_QUERY_KEY } from './useFavorites';
import { FavoriteRow } from './FavoriteRow';

export interface FavoritesTabsProps {
  mode: RadioMode;
  /** RADIO-1: the ONLY connect-related callback — drops a dial into the host form. */
  onPrefill: (dial: FavoriteDial) => void;
  /** The host panel's existing hand-entry connect form (passthrough). */
  manualContent: React.ReactNode;
}

/**
 * Modes with NO favorites/recents surface: only telnet (fixed CMS host — no
 * nearby-station choice to favorite). It renders the Manual content only — no
 * tabs, no FavoriteRow, no Connect. VARA's exclusion was retired in tuxlink-xglf
 * once its dial path landed (see the M7 note above).
 */
function isManualOnly(mode: RadioMode): boolean {
  return mode === 'telnet';
}

/** oi1g: above this row count a tab shows a filter box. Short lists stay clean. */
const FILTER_THRESHOLD = 8;

function matchesFavoriteFilter(f: { gateway: string; grid?: string; note?: string }, q: string): boolean {
  const needle = q.trim().toLowerCase();
  if (!needle) return true;
  return (
    f.gateway.toLowerCase().includes(needle) ||
    (f.grid ?? '').toLowerCase().includes(needle) ||
    (f.note ?? '').toLowerCase().includes(needle)
  );
}

export function FavoritesTabs({ mode, onPrefill, manualContent }: FavoritesTabsProps) {
  const { favorites, recents, star, upsert, remove } = useFavorites(mode);

  // oi1g: client-side filter over the rendered list (gateway / grid / note),
  // shown only when a tab's list exceeds FILTER_THRESHOLD. One shared input
  // narrows whichever tab is active.
  const [filter, setFilter] = useState('');

  // C4: full-precision operator grid for distance. Fetched ONCE; shared down.
  const fixQuery = useQuery({
    queryKey: ['position_current_fix'],
    queryFn: () => invoke<{ grid: string | null }>('position_current_fix'),
  });
  const operatorGrid = fixQuery.data?.grid ?? null;

  // The connection log lives on the StationsFile. Reading it under the SAME
  // query key useFavorites uses (['favorites']) means react-query DEDUPES — this
  // is the same cached fetch, not a second network call. We slice it per-unit
  // for each row (useFavorites intentionally does not expose the log).
  const stationsQuery = useQuery({
    queryKey: FAVORITES_QUERY_KEY,
    queryFn: () => invoke<StationsFile>('favorites_read'),
  });
  const log = useMemo(() => stationsQuery.data?.log ?? [], [stationsQuery.data]);

  // Telnet: Manual content only — no tabs, no rows, no Connect.
  if (isManualOnly(mode)) {
    return <div className="favorites-tabs favorites-tabs--manual-only">{manualContent}</div>;
  }

  const renderRows = (list: typeof favorites) => {
    const showFilter = list.length > FILTER_THRESHOLD;
    const shown = showFilter ? list.filter((f) => matchesFavoriteFilter(f, filter)) : list;
    return (
      <>
        {showFilter && (
          <div className="favorites-filter">
            <input
              type="text"
              data-testid="favorites-filter-input"
              placeholder="Filter… (call / grid / note)"
              value={filter}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              aria-label="Filter favorites"
              onChange={(e) => setFilter(e.target.value)}
            />
            <span className="favorites-filter-count" data-testid="favorites-filter-count">
              {shown.length}/{list.length}
            </span>
          </div>
        )}
        {shown.length === 0 ? (
          <p className="favorites-empty">{list.length === 0 ? 'No stations' : 'No matches'}</p>
        ) : (
          <div className="favorites-list">
            {shown.map((f) => (
              <FavoriteRow
                key={f.id}
                favorite={f}
                operatorGrid={operatorGrid}
                onPrefill={onPrefill}
                onToggleStar={star}
                attempts={log.filter((a) => a.unit_id === f.id)}
                onUpsert={upsert}
                onDelete={remove}
              />
            ))}
          </div>
        )}
      </>
    );
  };

  return (
    <div className="favorites-tabs">
      <Tabs.Root defaultValue="favorites" className="favorites-tabs-root">
        <Tabs.List
          className="favorites-tabs-list"
          aria-label="Favorites, recents, or manual entry"
        >
          <Tabs.Trigger className="favorites-tab-trigger" value="favorites">
            Favorites
          </Tabs.Trigger>
          <Tabs.Trigger className="favorites-tab-trigger" value="recent">
            Recent
          </Tabs.Trigger>
          <Tabs.Trigger className="favorites-tab-trigger" value="manual">
            Manual
          </Tabs.Trigger>
        </Tabs.List>

        <Tabs.Content className="favorites-tab-content" value="favorites">
          {renderRows(favorites)}
        </Tabs.Content>
        <Tabs.Content className="favorites-tab-content" value="recent">
          {renderRows(recents)}
        </Tabs.Content>
        <Tabs.Content className="favorites-tab-content" value="manual">
          {manualContent}
        </Tabs.Content>
      </Tabs.Root>
    </div>
  );
}
