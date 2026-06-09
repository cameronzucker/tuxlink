// Find a Gateway — location-aware station finder (bd-tuxlink-a2gd).
// Inline overlay panel (no pop-up window): a form column + a distance-sorted results column.
// Stations come from the direct HTTPS poll (catalog_fetch_stations); when the listing endpoint
// can't serve a mode, the station list is requestable by in-band message instead.
//
// tuxlink-6jpf: the by-message INFO-category requests (area weather / propagation / winlink info)
// that previously lived here have moved to Message → Request Center, which already lists the full
// bundled catalog (those entries — US.ALL, AUR_TONIGHT, INQUIRIES — are in it). This panel is now
// the station finder only.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { LISTING_MODES, type ListingMode } from './stationTypes';
import { useStations } from './useStations';
import { sendCatalogInquiry } from './useCatalog';
import { catalogErrorMessage } from './stationTypes';
import { StationResults } from './StationResults';
import './CatalogBuilderPanel.css';

export interface CatalogBuilderPanelProps {
  onClose: () => void;
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

export function CatalogBuilderPanel({ onClose }: CatalogBuilderPanelProps) {
  const [grid, setGrid] = useState('');
  const [modes, setModes] = useState<Set<ListingMode>>(new Set());
  const [radiusMi, setRadiusMi] = useState(DEFAULT_RADIUS_MI);
  const [queueState, setQueueState] = useState<QueueState>({ kind: 'idle' });
  const stations = useStations();

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
