// Distance-sorted station results column for the Catalog Builder.
// Rows dim (not vanish) beyond the radius (design §Builder UX). The ★ is a forward hook:
// disabled until CF's favorite_upsert lands (threaded via onAddFavorite), and additionally
// disabled for pactor/robust-packet (CF's favorite RadioMode union lacks those — bd-tuxlink-raez).

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
  onAddFavorite?: (g: Gateway, mode: ListingMode) => void; // CF-owned consumer; gated until it lands
}

interface Row {
  g: Gateway;
  mode: ListingMode;
  distMi: number | null;
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

export function StationResults({ listings, error, originGrid, radiusMi, onRequestByMessage, onAddFavorite }: Props) {
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
          return (
            <li key={`${mode}:${g.channel}`} data-testid="gateway-row" className={`catalog-row${dim ? ' is-dim' : ''}`}>
              <span className="catalog-row__badge">{mode}</span>
              <span className="catalog-row__call">{g.callsign}</span>
              <span className="catalog-row__freq">
                {g.frequenciesKhz.length ? `${g.frequenciesKhz.map((f) => (f / 1000).toFixed(3)).join(', ')} MHz` : '—'}
              </span>
              <span className="catalog-row__grid">{g.grid ?? '—'}</span>
              <span className="catalog-row__dist">{distMi == null ? '—' : `${Math.round(distMi)} mi`}</span>
              <button
                type="button"
                className="catalog-row__star"
                aria-label={`Add ${g.callsign} to ${mode} favorites`}
                disabled={!onAddFavorite || !favoritable}
                title={!favoritable ? 'Favorites for this mode are not yet supported' : 'Add to favorites'}
                onClick={() => onAddFavorite?.(g, mode)}
              >
                ★
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
