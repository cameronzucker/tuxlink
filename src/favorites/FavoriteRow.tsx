// FavoriteRow — one favorite/recent station row (Task B5).
//
// RADIO-1: the Connect button NEVER invokes a connect/transmit command. It
// ONLY calls `onPrefill(toDial(favorite))`, dropping the dial into the host
// panel's hand-entry form; the operator then clicks the panel's own
// Send/Receive (the Part 97 consent click). This row stays pure: no
// `invoke('*_connect')`, no `recordAttempt`.
//
// H7: telnet rows show `gateway · transport` (CmsSsl/Telnet) — no freq, band,
// or RF distance (telnet has no RF path).
// C4: the RF distance segment is OMITTED (never "null", never a crash) when the
// operator grid is null OR the favorite grid is absent/malformed.

import type { Favorite, FavoriteDial } from './types';
import { distanceBetweenGrids } from '../forms/position/distance';
import { ConnectionRecord } from './ConnectionRecord';
import type { ConnectionAttempt } from './types';

export interface FavoriteRowProps {
  favorite: Favorite;
  operatorGrid: string | null;
  onPrefill: (dial: FavoriteDial) => void;
  onToggleStar: (id: string, starred: boolean) => void;
  /** This unit's attempts (filtered upstream by FavoritesTabs from the shared log). */
  attempts?: ConnectionAttempt[];
}

/** The record-path DTO this row would hand the form on Connect (H3/Codex#8). */
function toDial(f: Favorite): FavoriteDial {
  return {
    mode: f.mode,
    gateway: f.gateway,
    freq: f.freq,
    transport: f.transport,
    band: f.band,
    grid: f.grid,
  };
}

/** Round a km distance to a thousands-grouped integer, e.g. 1240.7 → "1,240 km". */
function formatDistance(km: number): string {
  return `${Math.round(km).toLocaleString('en-US')} km`;
}

export function FavoriteRow({
  favorite,
  operatorGrid,
  onPrefill,
  onToggleStar,
  attempts = [],
}: FavoriteRowProps) {
  const isTelnet = favorite.mode === 'telnet';

  // Distance is RF-only and only when BOTH grids resolve (C4).
  const distanceKm = isTelnet
    ? null
    : distanceBetweenGrids(operatorGrid, favorite.grid);

  // Detail segments: telnet has no RF detail line beyond the head; RF modes show
  // freq · grid · distance (each present only when it has a value).
  const detailSegments: string[] = [];
  if (!isTelnet) {
    if (favorite.freq) detailSegments.push(`${favorite.freq}`);
    if (favorite.grid) detailSegments.push(favorite.grid);
    if (distanceKm != null) detailSegments.push(formatDistance(distanceKm));
  }

  // Head line: telnet → gateway · transport; RF → gateway · band.
  const headSub = isTelnet ? favorite.transport ?? '' : favorite.band ?? '';

  return (
    <div className="favorite-row" data-testid={`favorite-row-${favorite.id}`}>
      <button
        type="button"
        className={`favorite-star${favorite.starred ? ' favorite-star--on' : ''}`}
        data-testid={`favorite-star-${favorite.id}`}
        aria-pressed={favorite.starred}
        aria-label={favorite.starred ? 'Unstar' : 'Star'}
        title={favorite.starred ? 'Unstar' : 'Star'}
        onClick={() => onToggleStar(favorite.id, !favorite.starred)}
      >
        {favorite.starred ? '★' : '☆'}
      </button>

      <div className="favorite-row-body">
        <div className="favorite-row-head">
          <span className="favorite-row-gateway">{favorite.gateway}</span>
          {headSub && <span className="favorite-row-sub"> · {headSub}</span>}
        </div>

        {detailSegments.length > 0 && (
          <div className="favorite-row-detail" data-testid={`favorite-detail-${favorite.id}`}>
            {detailSegments.join(' · ')}
          </div>
        )}

        <ConnectionRecord unitId={favorite.id} attempts={attempts} />
      </div>

      <button
        type="button"
        className="favorite-connect"
        data-testid={`favorite-connect-${favorite.id}`}
        onClick={() => onPrefill(toDial(favorite))}
      >
        Connect
      </button>
    </div>
  );
}
