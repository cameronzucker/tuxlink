// Distance-sorted station results column for the Catalog Builder.
// Rows dim (not vanish) beyond the radius (design §Builder UX). Star controls
// reflect persisted favorite state; Use is prefill-only for active modem modes.

import { useMemo } from 'react';
import { distanceFromGrids, kmToMi } from './distance';
import type { Gateway, ListingMode, StationListing } from './stationTypes';

// CF's favorite RadioMode union (vara-hf|vara-fm|ardop-hf|packet|telnet) — no pactor/robust-packet.
const FAVORITABLE_MODES: ReadonlySet<ListingMode> = new Set<ListingMode>(['vara-hf', 'ardop-hf', 'packet']);

interface Props {
  listings: StationListing[];
  error: string | null;
  originGrid: string;
  radiusMi: number;
  onRequestByMessage?: () => void; // direct-poll failure → offer the message-request fallback
  onToggleFavorite?: (
    g: Gateway,
    mode: ListingMode,
    state: GatewayFavoriteState | null,
  ) => void;
  favoriteStates?: ReadonlyMap<string, GatewayFavoriteState>;
  /** Active modem mode that can consume a prefill-only station selection. */
  selectableMode?: ListingMode;
  onSelectGateway?: (g: Gateway, mode: ListingMode) => void;
}

export interface GatewayFavoriteState {
  id: string;
  starred: boolean;
}

interface Row {
  g: Gateway;
  mode: ListingMode;
  distMi: number | null;
}

export function stationFavoriteKey(mode: ListingMode, gateway: Gateway): string {
  return `${mode}:${gateway.callsign.trim().toUpperCase()}`;
}

function StaleCaption({ listings }: { listings: StationListing[] }) {
  const stamps = listings.map((l) => l.fetchedAtMs).filter((t): t is number => t != null);
  if (stamps.length === 0) return null;
  const oldest = Math.min(...stamps);
  const ageMin = (Date.now() - oldest) / 60_000;
  const when = new Date(oldest).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  return (
    <p className="catalog-results__asof">
      as of {when}
      {ageMin > 30 ? ' (cached — may be stale)' : ''}
    </p>
  );
}

export function StationResults({
  listings,
  error,
  originGrid,
  radiusMi,
  onRequestByMessage,
  onToggleFavorite,
  favoriteStates,
  selectableMode,
  onSelectGateway,
}: Props) {
  const rows = useMemo<Row[]>(() => {
    const all: Row[] = listings.flatMap((l) =>
      l.gateways.map((g) => {
        const km = g.grid ? distanceFromGrids(originGrid, g.grid) : null;
        return { g, mode: l.mode, distMi: km == null ? null : kmToMi(km) };
      }),
    );
    return all.sort((a, b) => (a.distMi ?? Infinity) - (b.distMi ?? Infinity));
  }, [listings, originGrid]);

  if (error) {
    return (
      <div className="catalog-results catalog-results--error">
        <p>{error}</p>
        <p className="catalog-results__fallback">
          Couldn't reach the listing service —{' '}
          <button type="button" className="catalog-link" onClick={onRequestByMessage} disabled={!onRequestByMessage}>
            request by message instead?
          </button>
        </p>
      </div>
    );
  }

  if (rows.length === 0) {
    return <div className="catalog-results catalog-results--empty">No stations yet — pick modes and Get stations.</div>;
  }

  return (
    <div className="catalog-results">
      <StaleCaption listings={listings} />
      <ul className="catalog-results__list">
        {rows.map(({ g, mode, distMi }) => {
          const dim = distMi != null && distMi > radiusMi;
          const favoritable = FAVORITABLE_MODES.has(mode);
          const favoriteState = favoriteStates?.get(stationFavoriteKey(mode, g)) ?? null;
          const selectable = Boolean(onSelectGateway && selectableMode === mode);
          return (
            <li key={`${mode}:${g.channel}`} data-testid="gateway-row" className={`catalog-row${dim ? ' is-dim' : ''}`}>
              <span className="catalog-row__badge">{mode}</span>
              <span className="catalog-row__call">{g.callsign}</span>
              <span className="catalog-row__freq">
                {g.frequenciesKhz.length ? `${g.frequenciesKhz.map((f) => (f / 1000).toFixed(3)).join(', ')} MHz` : '—'}
              </span>
              <span className="catalog-row__grid">{g.grid ?? '—'}</span>
              <span className="catalog-row__dist">{distMi == null ? '—' : `${Math.round(distMi)} mi`}</span>
              <span className="catalog-row__actions">
                {onSelectGateway && (
                  <button
                    type="button"
                    className="catalog-row__use"
                    disabled={!selectable}
                    title={
                      selectable
                        ? 'Use this station in the active modem panel'
                        : 'Open the matching Packet or ARDOP modem panel to use this station'
                    }
                    onClick={() => onSelectGateway(g, mode)}
                  >
                    Use
                  </button>
                )}
                <button
                  type="button"
                  className={`catalog-row__star${favoriteState?.starred ? ' catalog-row__star--on' : ''}`}
                  aria-pressed={favoriteState?.starred ?? false}
                  aria-label={
                    favoriteState?.starred
                      ? `Remove ${g.callsign} from ${mode} favorites`
                      : `Add ${g.callsign} to ${mode} favorites`
                  }
                  disabled={!onToggleFavorite || !favoritable}
                  title={
                    !favoritable
                      ? 'Favorites for this mode are not yet supported'
                      : favoriteState?.starred
                      ? 'Remove from favorites'
                      : 'Add to favorites'
                  }
                  onClick={() => onToggleFavorite?.(g, mode, favoriteState)}
                >
                  {favoriteState?.starred ? '★' : '☆'}
                </button>
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
