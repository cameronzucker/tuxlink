// Find a Station — propagation-aware station finder (design §7, Mock-D).
// Supersedes CatalogBuilderPanel. Inline overlay (no pop-up window). Owns the
// operator grid (from config_read), band (default 40 m), mode filter (default
// all three prefillable modes), and the selected station. Offline-first: U2
// seeds the station cache so the list shows immediately; reachability + the
// per-path forecast light up when U1 prediction is available and degrade to
// distance-only otherwise. RADIO-1: nothing here transmits.

import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useStations } from './useStations';
import { aggregateStations, type Station } from './stationModel';
import { useReachabilityMap, stationKey } from './useReachabilityMap';
import { useStationPrediction } from './useStationPrediction';
import { StationFinderControls, type FilterMode } from './StationFinderControls';
import { StationFinderMap } from './StationFinderMap';
import { StationRail } from './StationRail';
import type { Band } from './bandPlan';
import type { ListingMode } from './stationTypes';
import type { RadioMode } from '../favorites/types';
import './StationFinderPanel.css';

export interface StationFinderPanelProps {
  onClose: () => void;
  /** The open modem that can consume a channel prefill (Use →). */
  activePrefillMode?: RadioMode;
}

const FILTER_MODES: FilterMode[] = ['vara-hf', 'ardop-hf', 'packet'];

// UTC hour is captured once on open (not a live clock) to keep ranking stable.
function currentUtcHour(): number {
  return new Date().getUTCHours();
}
function localTimeLabel(): string {
  return new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

/** Whole months between (year, month) and now, rendered as a rough day-count. */
function ssnAge(year: number, month: number): number {
  const now = new Date();
  const months = (now.getUTCFullYear() - year) * 12 + (now.getUTCMonth() + 1 - month);
  return Math.max(0, months) * 30;
}

export function StationFinderPanel({ onClose, activePrefillMode }: StationFinderPanelProps) {
  const [grid, setGrid] = useState('');
  const [band, setBand] = useState<Band>('40m');
  const [enabledModes, setEnabledModes] = useState<Set<FilterMode>>(new Set(FILTER_MODES));
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [utcHour] = useState(currentUtcHour);
  const stations = useStations();

  useEffect(() => {
    invoke<{ grid: string | null }>('config_read')
      .then((c) => {
        if (c?.grid) setGrid(c.grid);
      })
      .catch(() => {});
  }, []);

  // Offline-first: fetch the three prefillable modes on open (U2 seeds cache).
  useEffect(() => {
    stations.fetch(FILTER_MODES as ListingMode[]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  const allStations = useMemo(() => aggregateStations(stations.listings), [stations.listings]);
  const visible = useMemo(
    () =>
      allStations.filter((s) =>
        s.modes.some((m) => (FILTER_MODES as ListingMode[]).includes(m) && enabledModes.has(m as FilterMode)),
      ),
    [allStations, enabledModes],
  );

  const reach = useReachabilityMap(grid, visible, band, utcHour);
  const selected: Station | null = useMemo(
    () => visible.find((s) => stationKey(s) === selectedKey) ?? null,
    [visible, selectedKey],
  );
  const pred = useStationPrediction(grid, selected);

  const ssnAgeDays = pred.prediction ? ssnAge(pred.prediction.year, pred.prediction.month) : null;
  // Freshest station-list fetch stamp across loaded listings (U2 freshness).
  const listFetchedAtMs = useMemo(() => {
    const stamps = stations.listings.map((l) => l.fetchedAtMs).filter((t): t is number => t != null);
    return stamps.length ? Math.max(...stamps) : null;
  }, [stations.listings]);

  const toggleMode = (m: FilterMode) =>
    setEnabledModes((prev) => {
      const next = new Set(prev);
      if (next.has(m)) next.delete(m);
      else next.add(m);
      return next;
    });

  return (
    <div
      className="station-finder-overlay"
      data-testid="station-finder-overlay"
      role="dialog"
      aria-label="Find a Station"
      onClick={onClose}
    >
      <div className="station-finder" onClick={(e) => e.stopPropagation()}>
        <header className="station-finder__header">
          <h2>Find a Station</h2>
          <button className="station-finder__close" onClick={onClose} aria-label="Close">
            ×
          </button>
        </header>

        <StationFinderControls
          band={band}
          onBandChange={setBand}
          enabledModes={enabledModes}
          onToggleMode={toggleMode}
          utcHour={utcHour}
          localTime={localTimeLabel()}
          ssn={pred.prediction?.ssn ?? null}
          ssnAgeDays={ssnAgeDays}
          predictionAvailable={reach.available || pred.status === 'ok'}
          listFetchedAtMs={listFetchedAtMs}
          onRefresh={() => stations.fetch(FILTER_MODES as ListingMode[])}
          refreshing={stations.loading}
        />

        <div className="station-finder__body">
          <StationFinderMap
            stations={visible}
            operatorGrid={grid}
            tiers={reach.tiers}
            selectedKey={selectedKey}
            onSelect={(s) => setSelectedKey(stationKey(s))}
          />
          <StationRail
            station={selected}
            prediction={pred.prediction}
            predictionStatus={pred.status}
            operatorGrid={grid}
            utcHour={utcHour}
            activePrefillMode={activePrefillMode}
          />
        </div>
      </div>
    </div>
  );
}
