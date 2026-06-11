// FavoriteRow — one favorite/recent station row (Task B5; edit/delete oi1g).
//
// RADIO-1: the Connect button NEVER invokes a connect/transmit command. It
// ONLY calls `onPrefill(toDial(favorite))`, dropping the dial into the host
// panel's hand-entry form; the operator then clicks the panel's own
// Send/Receive (the Part 97 consent click). This row stays pure: no
// `invoke('*_connect')`, no `recordAttempt`. Editing is local config only —
// the row hands an updated Favorite to `onUpsert`; it never touches the radio.
//
// oi1g: when `onUpsert`/`onDelete` are provided the row renders a `⋯` overflow
// menu → Edit (inline form: gateway/band/grid/freq/note → onUpsert with the
// merged favorite) / Delete (inline confirm → onDelete(id)). With no handlers
// the row is view-only (star + Connect), preserving recents/read-only callers.
//
// H7: telnet rows show `gateway · transport` (CmsSsl/Telnet) — no freq, band,
// or RF distance (telnet has no RF path).
// C4: the RF distance segment is OMITTED (never "null", never a crash) when the
// operator grid is null OR the favorite grid is absent/malformed.

import { useState } from 'react';
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
  /** oi1g: provided in editable contexts (RF favorites). Receives the merged
   *  favorite on Save; the backend `favorite_upsert` merges on the existing id. */
  onUpsert?: (favorite: Favorite) => void;
  /** oi1g: provided in editable contexts. Receives the favorite id on confirmed Delete. */
  onDelete?: (id: string) => void;
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
  onUpsert,
  onDelete,
}: FavoriteRowProps) {
  const isTelnet = favorite.mode === 'telnet';
  const editable = Boolean(onUpsert || onDelete);

  const [menuOpen, setMenuOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  // Edit-form field mirrors. Seeded from the favorite on each edit-open so a
  // Cancel-then-reopen starts from the persisted values, not a stale draft.
  const [gatewayInput, setGatewayInput] = useState(favorite.gateway);
  const [bandInput, setBandInput] = useState(favorite.band ?? '');
  const [gridInput, setGridInput] = useState(favorite.grid ?? '');
  const [freqInput, setFreqInput] = useState(favorite.freq ?? '');
  const [noteInput, setNoteInput] = useState(favorite.note ?? '');

  const openEdit = () => {
    setGatewayInput(favorite.gateway);
    setBandInput(favorite.band ?? '');
    setGridInput(favorite.grid ?? '');
    setFreqInput(favorite.freq ?? '');
    setNoteInput(favorite.note ?? '');
    setEditing(true);
    setMenuOpen(false);
    setConfirmingDelete(false);
  };

  const saveEdit = () => {
    const blankToUndef = (s: string) => {
      const t = s.trim();
      return t === '' ? undefined : t;
    };
    // Spread preserves id + mode + starred + timestamps; only editable fields
    // are overwritten. The backend merges by id (M12).
    onUpsert?.({
      ...favorite,
      gateway: gatewayInput.trim(),
      band: blankToUndef(bandInput),
      grid: blankToUndef(gridInput),
      freq: blankToUndef(freqInput),
      note: blankToUndef(noteInput),
    });
    setEditing(false);
  };

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

      <div className="favorite-row-acts">
        <button
          type="button"
          className="favorite-connect"
          data-testid={`favorite-connect-${favorite.id}`}
          onClick={() => onPrefill(toDial(favorite))}
        >
          Connect
        </button>

        {editable && (
          <span className="favorite-menu-wrap">
            <button
              type="button"
              className="favorite-menu-btn"
              data-testid={`favorite-menu-${favorite.id}`}
              aria-label="Edit or delete this favorite"
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              title="Edit or delete"
              onClick={() => {
                setMenuOpen((v) => !v);
                setConfirmingDelete(false);
              }}
            >
              ⋯
            </button>
            {menuOpen && (
              <div className="favorite-menu-pop" role="menu">
                {onUpsert && (
                  <button
                    type="button"
                    role="menuitem"
                    data-testid={`favorite-edit-${favorite.id}`}
                    onClick={openEdit}
                  >
                    ✎ Edit
                  </button>
                )}
                {onDelete && !confirmingDelete && (
                  <button
                    type="button"
                    role="menuitem"
                    className="favorite-menu-del"
                    data-testid={`favorite-delete-${favorite.id}`}
                    onClick={() => setConfirmingDelete(true)}
                  >
                    🗑 Delete
                  </button>
                )}
                {onDelete && confirmingDelete && (
                  <button
                    type="button"
                    role="menuitem"
                    className="favorite-menu-del"
                    data-testid={`favorite-delete-confirm-${favorite.id}`}
                    onClick={() => {
                      onDelete(favorite.id);
                      setMenuOpen(false);
                      setConfirmingDelete(false);
                    }}
                  >
                    Delete {favorite.gateway}? — confirm
                  </button>
                )}
              </div>
            )}
          </span>
        )}
      </div>

      {editing && (
        <div className="favorite-edit" data-testid={`favorite-edit-form-${favorite.id}`}>
          <div className="favorite-edit-row">
            <label htmlFor={`fe-gw-${favorite.id}`}>Call</label>
            <input
              id={`fe-gw-${favorite.id}`}
              type="text"
              data-testid={`favorite-edit-gateway-${favorite.id}`}
              value={gatewayInput}
              spellCheck={false}
              autoCapitalize="characters"
              autoCorrect="off"
              onChange={(e) => setGatewayInput(e.target.value)}
            />
          </div>
          <div className="favorite-edit-row">
            <label htmlFor={`fe-band-${favorite.id}`}>Band</label>
            <input
              id={`fe-band-${favorite.id}`}
              type="text"
              data-testid={`favorite-edit-band-${favorite.id}`}
              value={bandInput}
              onChange={(e) => setBandInput(e.target.value)}
            />
            <label htmlFor={`fe-freq-${favorite.id}`}>Freq</label>
            <input
              id={`fe-freq-${favorite.id}`}
              type="text"
              inputMode="decimal"
              data-testid={`favorite-edit-freq-${favorite.id}`}
              value={freqInput}
              onChange={(e) => setFreqInput(e.target.value)}
            />
          </div>
          <div className="favorite-edit-row">
            <label htmlFor={`fe-grid-${favorite.id}`}>Grid</label>
            <input
              id={`fe-grid-${favorite.id}`}
              type="text"
              data-testid={`favorite-edit-grid-${favorite.id}`}
              value={gridInput}
              spellCheck={false}
              autoCapitalize="characters"
              autoCorrect="off"
              onChange={(e) => setGridInput(e.target.value)}
            />
            <label htmlFor={`fe-note-${favorite.id}`}>Note</label>
            <input
              id={`fe-note-${favorite.id}`}
              type="text"
              data-testid={`favorite-edit-note-${favorite.id}`}
              value={noteInput}
              onChange={(e) => setNoteInput(e.target.value)}
            />
          </div>
          <div className="favorite-edit-actions">
            <button
              type="button"
              className="favorite-edit-save"
              data-testid={`favorite-edit-save-${favorite.id}`}
              disabled={gatewayInput.trim() === ''}
              onClick={saveEdit}
            >
              Save
            </button>
            <button
              type="button"
              className="favorite-edit-cancel"
              data-testid={`favorite-edit-cancel-${favorite.id}`}
              onClick={() => setEditing(false)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
