// Find a Gateway — location-aware station finder (bd-tuxlink-a2gd).
// Inline overlay panel (no pop-up window): a form column + a distance-sorted results column.
// Stations come from the direct HTTPS poll (catalog_fetch_stations); when the listing endpoint
// can't serve a mode, the station list is requestable by in-band message instead.
//
// tuxlink-6jpf: the by-message INFO-category requests (area weather / propagation / winlink info)
// that previously lived here have moved to Message → Request Center, which already lists the full
// bundled catalog (those entries — US.ALL, AUR_TONIGHT, INQUIRIES — are in it). This panel is now
// the station finder only.

import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { LISTING_MODES, type Gateway, type ListingMode } from './stationTypes';
import { useStations } from './useStations';
import { sendCatalogInquiry } from './useCatalog';
import { catalogErrorMessage } from './stationTypes';
import {
  StationResults,
  stationFavoriteKey,
  type GatewayFavoriteState,
} from './StationResults';
import { FAVORITES_QUERY_KEY } from '../favorites/useFavorites';
import { emitGatewayPrefill } from '../favorites/prefillEvent';
import type { Favorite, FavoriteDial, StationsFile } from '../favorites/types';
import './CatalogBuilderPanel.css';

export interface CatalogBuilderPanelProps {
  onClose: () => void;
  /** Active RF modem that can consume station-result prefill. VARA has no
   *  gateway target field yet, so it is intentionally absent here. */
  activePrefillMode?: 'packet' | 'ardop-hf';
}

const DEFAULT_RADIUS_MI = 300; // confirmed default (design §Builder UX)

// RMS-list inquiry filenames per mode (the by-message station-list fallback).
const MODE_INQUIRY_FILENAME: Record<ListingMode, string> = {
  'vara-hf': 'PUB_VARA',
  packet: 'PUB_PACKET',
  'ardop-hf': 'PUB_ARDOP',
  pactor: 'PUB_PACTOR',
  'robust-packet': 'PUB_ROBUST',
};

type QueueState =
  | { kind: 'idle' }
  | { kind: 'sending' }
  | { kind: 'done'; count: number }
  | { kind: 'error'; message: string };

type FavoritableListingMode = Extract<ListingMode, 'vara-hf' | 'ardop-hf' | 'packet'>;

function favoriteModeForListing(mode: string): FavoritableListingMode | null {
  if (mode === 'vara-hf' || mode === 'ardop-hf' || mode === 'packet') return mode;
  return null;
}

function frequencyMhz(g: Gateway): string | undefined {
  return g.frequenciesKhz.length ? (g.frequenciesKhz[0] / 1000).toFixed(3) : undefined;
}

function gatewayToDraftFavorite(g: Gateway, mode: ListingMode): Favorite | null {
  const favoriteMode = favoriteModeForListing(mode);
  if (!favoriteMode) return null;
  return {
    id: '', // empty → backend mints a uuid + stamps timestamps
    mode: favoriteMode,
    gateway: g.callsign,
    // Record-only metadata (never read back into a form, H8); use the same
    // MHz form the results row displays so a catalog-sourced favorite reads
    // consistently with what the operator saw.
    freq: frequencyMhz(g),
    grid: g.grid ?? undefined,
    starred: false, // backend forces false on create; star() below promotes it
    created_at: '',
    updated_at: '',
  };
}

function gatewayToPrefillDial(g: Gateway, mode: ListingMode): FavoriteDial | null {
  if (mode !== 'packet' && mode !== 'ardop-hf') return null;
  return {
    mode,
    gateway: g.callsign,
    freq: frequencyMhz(g),
    grid: g.grid ?? undefined,
  };
}

export function CatalogBuilderPanel({ onClose, activePrefillMode }: CatalogBuilderPanelProps) {
  const [grid, setGrid] = useState('');
  const [modes, setModes] = useState<Set<ListingMode>>(new Set());
  const [radiusMi, setRadiusMi] = useState(DEFAULT_RADIUS_MI);
  const [queueState, setQueueState] = useState<QueueState>({ kind: 'idle' });
  const stations = useStations();
  const qc = useQueryClient();
  const favoritesQuery = useQuery({
    queryKey: FAVORITES_QUERY_KEY,
    queryFn: () => invoke<StationsFile>('favorites_read'),
  });

  const favoriteStates = useMemo(() => {
    const map = new Map<string, GatewayFavoriteState>();
    for (const favorite of favoritesQuery.data?.favorites ?? []) {
      const mode = favoriteModeForListing(favorite.mode);
      if (!mode) continue;
      const gateway: Gateway = {
        channel: favorite.id,
        callsign: favorite.gateway,
        sysopName: null,
        grid: favorite.grid ?? null,
        location: null,
        frequenciesKhz: [],
        lastUpdate: null,
        email: null,
        homepage: null,
      };
      const key = stationFavoriteKey(mode, gateway);
      const previous = map.get(key);
      if (!previous || favorite.starred) {
        map.set(key, { id: favorite.id, starred: favorite.starred });
      }
    }
    return map;
  }, [favoritesQuery.data]);

  // tuxlink-29zx: Escape closes the panel — the keyboard dismiss path alongside
  // the × button and backdrop click. Document-level so it fires regardless of
  // which element inside the panel currently holds focus.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  useEffect(() => {
    // Full-precision home grid for distance origin (NOT the precision-reduced status-bar grid).
    invoke<{ grid: string | null }>('config_read')
      .then((c) => {
        if (c?.grid) setGrid(c.grid);
      })
      .catch(() => {});
  }, []);

  const toggleMode = (m: ListingMode) =>
    setModes((prev) => {
      const next = new Set(prev);
      next.has(m) ? next.delete(m) : next.add(m);
      return next;
    });

  const onGetStations = () => stations.fetch([...modes]);

  // ☆/★ on a result row → toggle the gateway's persisted favorite state.
  // The favorites store is star-to-promote: `favorite_upsert` mints the record
  // but forces `starred:false` (an unstarred "recent"), then `favorite_star`
  // promotes it so it lands in the Favorites tab. Existing starred records can
  // now be unstarred from this same list (tuxlink-0ml3).
  const onToggleFavorite = async (
    g: Gateway,
    mode: ListingMode,
    state: GatewayFavoriteState | null,
  ) => {
    try {
      if (state) {
        await invoke('favorite_star', { id: state.id, starred: !state.starred });
      } else {
        const draft = gatewayToDraftFavorite(g, mode);
        if (!draft) return;
        const stored = await invoke<Favorite>('favorite_upsert', { favorite: draft });
        await invoke('favorite_star', { id: stored.id, starred: true });
      }
      // Refresh the shared ['favorites'] cache so the radio dock's FavoritesTabs
      // reflects the new entry (prefix-match also refetches recents).
      await qc.invalidateQueries({ queryKey: FAVORITES_QUERY_KEY });
    } catch {
      // Non-blocking — persistence failures surface in the backend session log.
    }
  };

  const onSelectGateway = (g: Gateway, mode: ListingMode) => {
    if (mode !== activePrefillMode) return;
    const dial = gatewayToPrefillDial(g, mode);
    if (!dial) return;
    emitGatewayPrefill(dial);
    onClose();
  };

  // Direct-poll failed → offer the station list by in-band message (PUB_<mode> inquiry).
  const onRequestStationsByMessage = async () => {
    const filenames = [...modes].map((m) => MODE_INQUIRY_FILENAME[m]);
    if (filenames.length === 0) return;
    setQueueState({ kind: 'sending' });
    try {
      await sendCatalogInquiry(filenames);
      setQueueState({ kind: 'done', count: filenames.length });
    } catch (e) {
      setQueueState({ kind: 'error', message: catalogErrorMessage(e) });
    }
  };

  return (
    <div
      className="catalog-builder-overlay"
      data-testid="catalog-builder-overlay"
      role="dialog"
      aria-label="Find a Gateway"
      onClick={onClose}
    >
      {/* Stop backdrop-dismiss from firing on clicks inside the panel itself. */}
      <div className="catalog-builder" onClick={(e) => e.stopPropagation()}>
        <header className="catalog-builder__header">
          <h2>Find a Gateway</h2>
          <button className="catalog-builder__close" onClick={onClose} aria-label="Close">
            ×
          </button>
        </header>
        <div className="catalog-builder__body">
          <form
            className="catalog-builder__form"
            onSubmit={(e) => {
              e.preventDefault();
              onGetStations();
            }}
          >
            <label className="catalog-field">
              <span>Your location</span>
              <input
                aria-label="Your location"
                value={grid}
                onChange={(e) => setGrid(e.target.value)}
                placeholder="Set your location"
              />
            </label>

            <fieldset className="catalog-field">
              <legend>Station modes</legend>
              {LISTING_MODES.map(({ mode, label }) => (
                <label key={mode} className="catalog-check">
                  <input type="checkbox" aria-label={label} checked={modes.has(mode)} onChange={() => toggleMode(mode)} />
                  {label}
                </label>
              ))}
            </fieldset>

            <label className="catalog-field">
              <span>Within</span>
              <input
                aria-label="within (miles)"
                type="range"
                min={50}
                max={3000}
                step={50}
                value={radiusMi}
                onChange={(e) => setRadiusMi(Number(e.target.value))}
              />
              <output>{radiusMi} mi</output>
            </label>

            <button type="submit" className="catalog-builder__go" disabled={modes.size === 0 || stations.loading}>
              {stations.loading ? 'Fetching…' : 'Get stations →'}
            </button>
          </form>

          <div className="catalog-builder__results">
            <StationResults
              listings={stations.listings}
              error={stations.error}
              originGrid={grid}
              radiusMi={radiusMi}
              onRequestByMessage={modes.size > 0 ? onRequestStationsByMessage : undefined}
              onToggleFavorite={onToggleFavorite}
              favoriteStates={favoriteStates}
              selectableMode={activePrefillMode}
              onSelectGateway={activePrefillMode ? onSelectGateway : undefined}
            />
          </div>
        </div>

        {queueState.kind !== 'idle' && (
          <footer className="catalog-builder__footer">
            {queueState.kind === 'sending' && <p className="catalog-builder__confirm">Queuing…</p>}
            {queueState.kind === 'done' && (
              <p className="catalog-builder__confirm">
                Queued {queueState.count} request{queueState.count > 1 ? 's' : ''} — they'll arrive in your Inbox after the next connect.
              </p>
            )}
            {queueState.kind === 'error' && <p className="catalog-results--error">{queueState.message}</p>}
          </footer>
        )}
      </div>
    </div>
  );
}
