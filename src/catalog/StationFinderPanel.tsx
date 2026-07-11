// Find a Station — propagation-aware station finder (design §7, Mock-D).
// Supersedes CatalogBuilderPanel. Inline overlay (no pop-up window). Owns the
// operator grid (from config_read), band (default 40 m), mode filter (default
// all three prefillable modes), and the selected station. Offline-first: U2
// seeds the station cache so the list shows immediately; reachability + the
// per-path forecast light up when U1 prediction is available and degrade to
// distance-only otherwise. RADIO-1: nothing here transmits.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { FAVORITES_QUERY_KEY } from '../favorites/useFavorites';
import { dialToNewFavorite, favoriteKey } from '../favorites/dialToFavorite';
import type { Favorite, StationsFile } from '../favorites/types';
import { useStations } from './useStations';
import { aggregateStations, stationMatchesBandMode, type Station } from './stationModel';
import { useReachabilityMap, stationKey } from './useReachabilityMap';
import { useDebouncedCommit } from './useDebouncedCommit';
import { useStationPrediction } from './useStationPrediction';
import { distanceFromGrids, kmToMi } from './distance';
import { StationFinderControls, type FilterMode, type StationType } from './StationFinderControls';
import { useP2pCapabilities, usePeers } from '../peers/usePeers';
import { aggregatePeers } from '../peers/peerModel';
import { ServiceCodesField } from './ServiceCodesField';
import { StationFinderMap } from './StationFinderMap';
import { StationRail } from './StationRail';
import { AntennaControl } from './AntennaControl';
import {
  readPropagationPrefs,
  writePropagationPrefs,
  DEFAULT_PROPAGATION_PREFS,
  type PropagationPrefs,
} from './propagationPrefs';
import { HF_BANDS, type Band } from './bandPlan';
import type { ListingMode } from './stationTypes';
import type { RadioMode, FavoriteDial } from '../favorites/types';
import './StationFinderPanel.css';

export interface StationFinderPanelProps {
  onClose: () => void;
  /** The open modem that can consume a channel prefill (Use →). */
  activePrefillMode?: RadioMode;
  /** Arm-on-demand handler for "Use →" (AppShell opens the modem + prefills).
   *  `candidates` is the ranked QSY-on-fail list for the used channel's
   *  station+mode (tuxlink-8fkkk Task B). */
  onUse?: (dial: FavoriteDial, candidates?: FavoriteDial[]) => void;
}

const FILTER_MODES: FilterMode[] = ['vara-hf', 'ardop-hf', 'packet'];
// Task 23 (spec §5): station-type filter, both on by default. Forced to
// {'gateway'} when useP2pCapabilities().finder_peers is false (R5-8) — see
// `effectiveTypes` below.
const STATION_TYPES: StationType[] = ['gateway', 'peer'];

// Coalesce an antenna-control gesture (a height-slider drag, SNR/power typing)
// into ONE persist + ONE reachability re-sweep once the operator settles, rather
// than one full N-station voacapl sweep per onChange event (tuxlink-ziyu). 300 ms
// is below the threshold of feeling laggy yet long enough to swallow a drag.
const PREFS_COMMIT_DEBOUNCE_MS = 300;

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

// tuxlink-liqs9: persist the operator's finder VIEW (search + band/mode filters
// + radius + selection) across a close/reopen — the panel used to reset
// entirely. localStorage, validated on read so cross-version / hand-edited data
// degrades to defaults. The MAP viewport persists separately (usePersistedViewport).
const FINDER_VIEW_KEY = 'tuxlink:station-finder:view';

interface PersistedFinderView {
  search: string;
  bands: Band[];
  modes: FilterMode[];
  types: StationType[];
  radiusMi: number | null;
  selectedKey: string | null;
}

/** Resolve localStorage defensively — the getter itself can throw. */
function finderStorage(): Storage | null {
  try {
    if (typeof window === 'undefined') return null;
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

function readFinderView(): Partial<PersistedFinderView> {
  const storage = finderStorage();
  if (!storage) return {};
  try {
    const raw = storage.getItem(FINDER_VIEW_KEY);
    if (!raw) return {};
    const v = JSON.parse(raw) as Record<string, unknown>;
    const out: Partial<PersistedFinderView> = {};
    if (typeof v.search === 'string') out.search = v.search;
    if (Array.isArray(v.bands)) out.bands = v.bands.filter((b) => typeof b === 'string') as Band[];
    if (Array.isArray(v.modes)) {
      out.modes = v.modes.filter((m) => typeof m === 'string') as FilterMode[];
    }
    if (Array.isArray(v.types)) {
      out.types = v.types.filter((t) => t === 'gateway' || t === 'peer') as StationType[];
    }
    if (typeof v.radiusMi === 'number' || v.radiusMi === null) {
      out.radiusMi = v.radiusMi as number | null;
    }
    if (typeof v.selectedKey === 'string' || v.selectedKey === null) {
      out.selectedKey = v.selectedKey as string | null;
    }
    return out;
  } catch {
    return {};
  }
}

function writeFinderView(view: PersistedFinderView): void {
  const storage = finderStorage();
  if (!storage) return;
  try {
    storage.setItem(FINDER_VIEW_KEY, JSON.stringify(view));
  } catch {
    /* storage full / disabled — skip persistence, keep the in-memory view. */
  }
}

export function StationFinderPanel({ onClose, activePrefillMode, onUse }: StationFinderPanelProps) {
  // tuxlink-liqs9: seed the finder view from the operator's last session so a
  // close/reopen restores where they left off. Read ONCE at mount (lazy);
  // re-persisted by the effect below.
  const [persisted0] = useState(readFinderView);
  const [grid, setGrid] = useState('');
  // Band picker is a multi-select FILTER (tuxlink-hlas). Default: all HF bands
  // on (show the operator's full HF options), VHF/UHF off (line-of-sight packet
  // is opt-in). A station shows only if it has a channel on a selected band.
  const [enabledBands, setEnabledBands] = useState<Set<Band>>(
    () => new Set(persisted0.bands ?? HF_BANDS),
  );
  const [enabledModes, setEnabledModes] = useState<Set<FilterMode>>(
    () => new Set(persisted0.modes ?? FILTER_MODES),
  );
  // Task 23 (spec §5): the operator's raw type-filter preference. This is NOT
  // necessarily what's rendered/filtered — `effectiveTypes` below forces
  // gateway-only when the finder_peers capability bit is off, without
  // clobbering the persisted preference (so it takes effect the moment the
  // capability flips on).
  const [enabledTypes, setEnabledTypes] = useState<Set<StationType>>(
    () => new Set(persisted0.types ?? STATION_TYPES),
  );
  const [selectedKey, setSelectedKey] = useState<string | null>(persisted0.selectedKey ?? null);
  const [utcHour] = useState(currentUtcHour);
  // `null` is a valid "no radius" choice, so distinguish it from "not persisted".
  const [radiusMi, setRadiusMi] = useState<number | null>(
    persisted0.radiusMi !== undefined ? persisted0.radiusMi : 500,
  );
  const [search, setSearch] = useState(persisted0.search ?? '');
  // Operator propagation prefs (own antenna / SNR / power). Loaded once on open;
  // `predictReload` is bumped AFTER a save persists so the forecast re-runs with
  // the new TX model (the backend reads these prefs fresh each prediction).
  const [prefs, setPrefs] = useState<PropagationPrefs | null>(null);
  const [prefsError, setPrefsError] = useState<string | null>(null);
  const [predictReload, setPredictReload] = useState(0);

  // tuxlink-liqs9: persist the finder view on every change so a close/reopen
  // restores it. Sets serialize as arrays. Cheap (localStorage write of a small
  // object); fires once on mount writing the seeded values back (a no-op).
  useEffect(() => {
    writeFinderView({
      search,
      bands: [...enabledBands],
      modes: [...enabledModes],
      types: [...enabledTypes],
      radiusMi,
      selectedKey,
    });
  }, [search, enabledBands, enabledModes, enabledTypes, radiusMi, selectedKey]);
  const stations = useStations();

  // Task 23 (spec §5): the P2P capability-matrix bit gating this dimension,
  // and the peer roster to aggregate into rail rows. `finder_peers` false
  // HIDES the type chips + peer rows entirely (R5-8) — it does not merely
  // disable them. Undefined (capabilities still loading) is treated as
  // false — see `finderPeersEnabled` — so there is no flash of the chip
  // before the read resolves.
  const p2pCapabilities = useP2pCapabilities();
  const finderPeersEnabled = p2pCapabilities.capabilities?.finder_peers === true;
  const peersData = usePeers();
  const aggregatedPeers = useMemo(() => aggregatePeers(peersData.peers), [peersData.peers]);
  // The EFFECTIVE type set used for filtering: forces gateway-only when the
  // capability is off, regardless of the operator's persisted preference.
  const effectiveTypes = useMemo(
    () => (finderPeersEnabled ? enabledTypes : new Set<StationType>(['gateway'])),
    [finderPeersEnabled, enabledTypes],
  );

  // Resolve the operator's EFFECTIVE location the way the rest of the app does
  // (CheckInForm / PositionFormV2): the PositionArbiter (`position_current_fix`)
  // returns the live grid — GPS-derived when `position_source` is GPS (the
  // default), or the manually-pinned grid otherwise. Fall back to the persisted
  // manual `config.grid` only when the arbiter has no fix.
  //
  // tuxlink-q1tm regression: reading `config_read().grid` ALONE (the manual
  // grid) left GPS operators — the default — with no location here, even though
  // the status bar showed their position. That blanked the aiming/bearing hero
  // AND silenced HF prediction (both gated on this grid).
  useEffect(() => {
    let cancelled = false;
    (async () => {
      let resolved: string | null = null;
      try {
        const fix = await invoke<{ grid: string | null }>('position_current_fix');
        resolved = fix?.grid ?? null;
      } catch {
        // PositionArbiter unavailable — fall through to the persisted grid.
      }
      if (!resolved) {
        try {
          const c = await invoke<{ grid: string | null }>('config_read');
          resolved = c?.grid ?? null;
        } catch {
          // No persisted grid either — leave blank (the "set your location" path).
        }
      }
      if (!cancelled && resolved) setGrid(resolved);
    })();
    return () => {
      cancelled = true;
    };
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
  // Band + mode FILTER (tuxlink-hlas), evaluated at the CHANNEL level: a station
  // shows only if it has a channel whose band is selected AND whose mode is
  // enabled. This is why 145 MHz packet (band='vhf-uhf') disappears when only HF
  // bands are selected — that channel matches no selected band.
  const bandModeVisible = useMemo(
    () => allStations.filter((s) => stationMatchesBandMode(s, enabledBands, enabledModes)),
    [allStations, enabledBands, enabledModes],
  );

  // Callsign search + radius filter (design §7). Radius needs a home grid;
  // without one it is a no-op (the whole list shows, and the controls disable
  // the selector + prompt the operator to set their location).
  const visible = useMemo(() => {
    // Task 23: the Gateway type toggle hides every gateway station/pin (a
    // Peer-only view) rather than merely dimming them.
    if (!effectiveTypes.has('gateway')) return [];
    const q = search.trim().toUpperCase();
    return bandModeVisible.filter((s) => {
      if (q && !s.baseCallsign.includes(q)) return false;
      if (radiusMi != null && grid) {
        const km = distanceFromGrids(grid, s.grid);
        if (km != null && kmToMi(km) > radiusMi) return false;
      }
      return true;
    });
  }, [bandModeVisible, search, radiusMi, grid, effectiveTypes]);

  // Task 23 (spec §5): peer rows, shown only when Peer is enabled (and
  // finder_peers is on — effectiveTypes already reflects that). Peers do NOT
  // reuse aggregateStations/stationMatchesBandMode [R4-8] — aggregatePeers
  // tolerates a missing grid and never drops a gridless/telnet-only peer, so
  // it renders untiered in the rail rather than disappearing. The callsign
  // search applies for parity with the gateway list; band/mode/radius do not
  // (peer channels aren't filtered by the gateway catalog's band-plan model).
  const visiblePeers = useMemo(() => {
    if (!effectiveTypes.has('peer')) return [];
    const q = search.trim().toUpperCase();
    if (!q) return aggregatedPeers;
    return aggregatedPeers.filter(
      (p) =>
        p.canonicalBase.toUpperCase().includes(q) ||
        p.presentedCallsigns.some((c) => c.toUpperCase().includes(q)),
    );
  }, [aggregatedPeers, effectiveTypes, search]);

  // `predictReload` (bumped after a prefs save persists) also re-runs the map
  // tiers, so changing power / antenna / height / ground / noise / SNR refreshes
  // reachability — not just the selected-station forecast.
  const reach = useReachabilityMap(grid, visible, enabledBands, utcHour, predictReload);
  const selected: Station | null = useMemo(
    () => visible.find((s) => stationKey(s) === selectedKey) ?? null,
    [visible, selectedKey],
  );
  const pred = useStationPrediction(grid, selected, predictReload);

  // Save-to-favorites (tuxlink-5016). Read the whole stations file (shared
  // ['favorites'] query key with useFavorites, so a save here refreshes the
  // radio panels' Favorites tabs and vice-versa) and index every unit by
  // mode+gateway. Saving a discovered channel STARS the matching unit, minting
  // it first if no unit exists yet — never a duplicate of a recents unit.
  const qc = useQueryClient();
  const favFile = useQuery({
    queryKey: FAVORITES_QUERY_KEY,
    queryFn: () => invoke<StationsFile>('favorites_read'),
  });
  const favByKey = useMemo(() => {
    const m = new Map<string, Favorite>();
    for (const f of favFile.data?.favorites ?? []) m.set(favoriteKey(f), f);
    return m;
  }, [favFile.data]);
  const isSaved = useCallback(
    (dial: FavoriteDial) => favByKey.get(favoriteKey(dial))?.starred ?? false,
    [favByKey],
  );
  const onSaveFavorite = useCallback(
    async (dial: FavoriteDial) => {
      const existing = favByKey.get(favoriteKey(dial));
      if (existing) {
        // Toggle the star on the existing unit (star-to-save / unstar). This is
        // the ONLY writer of `starred`; a non-starred recents unit becomes a
        // favorite without duplicating it.
        await invoke('favorite_star', { id: existing.id, starred: !existing.starred }).catch(
          () => {},
        );
      } else {
        const created = await invoke<Favorite>('favorite_upsert', {
          favorite: dialToNewFavorite(dial),
        }).catch(() => null);
        if (created?.id) {
          await invoke('favorite_star', { id: created.id, starred: true }).catch(() => {});
        }
      }
      await qc.invalidateQueries({ queryKey: FAVORITES_QUERY_KEY });
    },
    [favByKey, qc],
  );

  const ssnAgeDays = pred.prediction ? ssnAge(pred.prediction.year, pred.prediction.month) : null;
  // Freshest station-list fetch stamp across loaded listings (U2 freshness).
  const listFetchedAtMs = useMemo(() => {
    const stamps = (stations.listings ?? [])
      .map((l) => l.fetchedAtMs)
      .filter((t): t is number => t != null);
    return stamps.length ? Math.max(...stamps) : null;
  }, [stations.listings]);

  // Load the operator's propagation prefs once on mount (defaults if the read
  // fails — a fresh install has no prefs file and the backend returns defaults).
  useEffect(() => {
    let mounted = true;
    readPropagationPrefs()
      .then((p) => mounted && setPrefs(p))
      .catch(() => {
        if (!mounted) return;
        setPrefs(DEFAULT_PROPAGATION_PREFS);
        setPrefsError('Could not load antenna settings; showing defaults.');
      });
    return () => {
      mounted = false;
    };
  }, []);

  // Debounced commit: persist the prefs change, then bump the reload key so the
  // forecast re-runs with the new TX model — only AFTER the write resolves (the
  // backend reads the prefs file fresh each prediction, so re-running before the
  // write would read stale). Debouncing collapses a slider drag / typing burst
  // into a single persist + single N-station re-sweep (tuxlink-ziyu). On unmount
  // a still-pending value is persisted WITHOUT the reload bump (the component is
  // gone — no setState), so a final drag is not silently lost.
  const commitPrefs = useDebouncedCommit<PropagationPrefs>(
    (next) => {
      writePropagationPrefs(next)
        .then(() => setPredictReload((v) => v + 1))
        .catch(() => setPrefsError('Could not save antenna settings.'));
    },
    PREFS_COMMIT_DEBOUNCE_MS,
    (next) => {
      void writePropagationPrefs(next).catch(() => {});
    },
  );

  const handlePrefsChange = (next: PropagationPrefs) => {
    setPrefs(next); // live UI follows the slider immediately
    setPrefsError(null);
    commitPrefs(next); // debounced persist + recompute
  };

  const toggleMode = (m: FilterMode) =>
    setEnabledModes((prev) => {
      const next = new Set(prev);
      if (next.has(m)) next.delete(m);
      else next.add(m);
      return next;
    });

  const toggleBand = (b: Band) =>
    setEnabledBands((prev) => {
      const next = new Set(prev);
      if (next.has(b)) next.delete(b);
      else next.add(b);
      return next;
    });

  const toggleType = (t: StationType) =>
    setEnabledTypes((prev) => {
      const next = new Set(prev);
      if (next.has(t)) next.delete(t);
      else next.add(t);
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
          enabledBands={enabledBands}
          onToggleBand={toggleBand}
          enabledModes={enabledModes}
          onToggleMode={toggleMode}
          enabledTypes={enabledTypes}
          onToggleType={toggleType}
          showPeerType={finderPeersEnabled}
          utcHour={utcHour}
          localTime={localTimeLabel()}
          ssn={pred.prediction?.ssn ?? null}
          ssnAgeDays={ssnAgeDays}
          predictionAvailable={reach.available || pred.status === 'ok'}
          recomputing={reach.loading}
          listFetchedAtMs={listFetchedAtMs}
          radiusMi={radiusMi}
          onRadiusChange={setRadiusMi}
          hasOperatorGrid={grid.trim().length > 0}
          search={search}
          onSearchChange={setSearch}
          onRefresh={() => stations.fetch(FILTER_MODES as ListingMode[])}
          refreshing={stations.loading}
          filterExtra={<ServiceCodesField onApplied={() => stations.fetch(FILTER_MODES as ListingMode[])} />}
        />

        {prefs && (
          <AntennaControl prefs={prefs} onChange={handlePrefsChange} error={prefsError} />
        )}

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
            onUse={onUse}
            onSaveFavorite={onSaveFavorite}
            isSaved={isSaved}
            peers={visiblePeers}
          />
        </div>
      </div>
    </div>
  );
}
