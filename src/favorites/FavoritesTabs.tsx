// FavoritesTabs — the per-mode Favorites / Recent / Manual surface (Task B5).
//
// Mounts INSIDE a radio mode panel body (B6). Inline only — no popup.
//
// M7 — per-mode chrome:
//   · ardop-hf / packet / telnet → Radix Tabs: Favorites | Recent | Manual.
//   · vara-hf / vara-fm          → Manual content ONLY (no tabs, no Favorites/
//     Recent lists, no FavoriteRow, no Connect button). VARA has no
//     favorites/recents Connect by design.
//
// C4 — distance source: the operator grid comes from `position_current_fix`
// (FULL precision), NEVER `position_status`/`useStatus` (those are
// precision-reduced for broadcast).
//
// RADIO-1: `onPrefill` is the ONLY connect-related callback this surface exposes.
// FavoriteRow's Connect drops a dial into the host form; the operator clicks the
// panel's own Send/Receive (the Part 97 consent click).

import { useMemo } from 'react';
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

/** VARA modes have no favorites/recents surface (M7). */
function isVara(mode: RadioMode): boolean {
  return mode === 'vara-hf' || mode === 'vara-fm';
}

export function FavoritesTabs({ mode, onPrefill, manualContent }: FavoritesTabsProps) {
  const { favorites, recents, star } = useFavorites(mode);

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

  // VARA: Manual content only — no tabs, no rows, no Connect.
  if (isVara(mode)) {
    return <div className="favorites-tabs favorites-tabs--manual-only">{manualContent}</div>;
  }

  const renderRows = (list: typeof favorites) =>
    list.length === 0 ? (
      <p className="favorites-empty">No stations</p>
    ) : (
      <div className="favorites-list">
        {list.map((f) => (
          <FavoriteRow
            key={f.id}
            favorite={f}
            operatorGrid={operatorGrid}
            onPrefill={onPrefill}
            onToggleStar={star}
            attempts={log.filter((a) => a.unit_id === f.id)}
          />
        ))}
      </div>
    );

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
