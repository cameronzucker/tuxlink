// Top conditions/band/mode bar for Find-a-Station (design §7). Presentational:
// the parent owns band + mode-filter state. SSN provenance is shown from the
// prediction (F12); SFI/K-index are shown only when a value is supplied — never
// fabricated (amateur-radio-reliability discipline: display only what we have).

import type { ReactNode } from 'react';
import { HF_BANDS, bandLabel, type Band } from './bandPlan';
import { WwvOffairControl } from '../wwv/WwvOffairControl';

export type FilterMode = 'vara-hf' | 'ardop-hf' | 'packet';

const FILTER_MODES: { mode: FilterMode; label: string }[] = [
  { mode: 'vara-hf', label: 'VARA HF' },
  { mode: 'ardop-hf', label: 'ARDOP HF' },
  { mode: 'packet', label: 'Packet' },
];

/// Station-type filter (Task 23, spec §5): a new dimension on the existing
/// finder, orthogonal to band/mode. Gateway = the RMS-catalog stations this
/// panel already showed; Peer = the P2P roster (Task 22's usePeers/
/// aggregatePeers). Both on by default.
export type StationType = 'gateway' | 'peer';

const STATION_TYPES: { type: StationType; label: string }[] = [
  { type: 'gateway', label: 'Gateway' },
  { type: 'peer', label: 'Peer' },
];

export interface StationFinderControlsProps {
  /** Selected bands — a multi-select FILTER: a station shows only if it has a
   *  channel on one of these bands (∩ enabledModes). Includes 'vhf-uhf' when the
   *  operator opts in to line-of-sight packet (never propagation-ranked). */
  enabledBands: Set<Band>;
  onToggleBand: (band: Band) => void;
  enabledModes: Set<FilterMode>;
  onToggleMode: (mode: FilterMode) => void;
  /** Station-type filter state (Task 23): Gateway / Peer. */
  enabledTypes: Set<StationType>;
  onToggleType: (type: StationType) => void;
  /** Whether the Peer type + its chip render at all — gated on
   *  `useP2pCapabilities().finder_peers` [R5-8]. `false` HIDES the whole type
   *  chip cluster (not just the Peer chip): with peers unavailable, showing a
   *  single-option Gateway toggle would be confusing chrome for no benefit. */
  showPeerType: boolean;
  utcHour: number;
  localTime: string;
  ssn: number | null;
  ssnAgeDays: number | null;
  sfi?: number | null;
  kIndex?: number | null;
  predictionAvailable: boolean;
  /** Reachability tiers are recomputing (a prefs change kicked off a re-sweep).
   *  Surfaced so the re-coloring map doesn't read as frozen (tuxlink-ziyu). */
  recomputing?: boolean;
  /** Freshest station-list fetch stamp (Unix ms) for the "updated N ago"
   *  caption — the U2 last-known-good freshness surface (design §6). */
  listFetchedAtMs?: number | null;
  /** Search radius in miles from the operator location; null = no limit (All).
   *  Disabled when the operator has no home grid set. */
  radiusMi: number | null;
  onRadiusChange: (mi: number | null) => void;
  hasOperatorGrid: boolean;
  /** Callsign substring filter (design §7 search). */
  search: string;
  onSearchChange: (q: string) => void;
  onRefresh: () => void;
  refreshing: boolean;
  /** Extra controls rendered inline in the Search row (the service-code field +
   *  presets + Apply), so they share the filter line instead of a row of their
   *  own (tuxlink-obpa compaction). */
  filterExtra?: ReactNode;
}

/** Radius options (miles) for the search-radius selector; null = All. */
const RADIUS_OPTIONS: { mi: number | null; label: string }[] = [
  { mi: 250, label: '250 mi' },
  { mi: 500, label: '500 mi' },
  { mi: 1000, label: '1000 mi' },
  { mi: 2500, label: '2500 mi' },
  { mi: null, label: 'All' },
];

/** Compact relative-age label for a Unix-ms timestamp ("updated 3 min ago"). */
function listAgeLabel(fetchedAtMs: number): string {
  const ageMin = Math.max(0, Math.round((Date.now() - fetchedAtMs) / 60_000));
  if (ageMin < 1) return 'stations updated just now';
  if (ageMin < 60) return `stations updated ${ageMin} min ago`;
  const ageHr = Math.round(ageMin / 60);
  if (ageHr < 24) return `stations updated ${ageHr} h ago`;
  return `stations updated ${Math.round(ageHr / 24)} d ago`;
}

export function StationFinderControls(props: StationFinderControlsProps) {
  const utcLabel = `${String(props.utcHour).padStart(2, '0')}:00Z`;
  return (
    <div className="station-finder__controls">
      {/* Row 1 (directly under the title): data-update actions on the left,
          live conditions/time pushed to the right. The off-air WWV control
          (Task 15) lives here beside "Update station list" — a self-contained
          component that owns its own hook, so no propagation state is
          threaded through this presentational panel's props. */}
      <div className="station-finder__topbar">
        <div className="station-finder__actions">
          <button
            type="button"
            className="station-finder__refresh"
            onClick={props.onRefresh}
            disabled={props.refreshing}
          >
            {props.refreshing ? 'Updating…' : 'Update station list'}
          </button>
          {props.listFetchedAtMs != null && (
            <span className="station-finder__listage" data-testid="list-age">
              {listAgeLabel(props.listFetchedAtMs)}
            </span>
          )}
          <WwvOffairControl />
        </div>
        <div className="station-finder__cond" data-testid="conditions">
          <span>
            {props.localTime} local · <b>{utcLabel}</b>
          </span>
          {props.sfi != null && (
            <span>
              SFI <b>{props.sfi}</b>
            </span>
          )}
          {props.ssn != null && (
            <span>
              SSN <b>{props.ssn}</b>
            </span>
          )}
          {props.kIndex != null && (
            <span>
              K <b>{props.kIndex}</b>
            </span>
          )}
          {props.ssnAgeDays != null && (
            <span className="station-finder__stale">solar data {props.ssnAgeDays}d old</span>
          )}
          {props.recomputing && (
            <span className="station-finder__recomputing" data-testid="reach-recomputing" role="status">
              updating reachability…
            </span>
          )}
          {!props.predictionAvailable && (
            <span className="station-finder__stale">no forecast — distance only</span>
          )}
        </div>
      </div>

      <div className="station-finder__bandbar">
        <span className="station-finder__lab">Bands</span>
        {HF_BANDS.map((b) => (
          <button
            key={b}
            type="button"
            className={`station-finder__bandtab${props.enabledBands.has(b) ? ' on' : ' off'}`}
            aria-pressed={props.enabledBands.has(b)}
            onClick={() => props.onToggleBand(b)}
          >
            {bandLabel(b)}
          </button>
        ))}
        {/* VHF/UHF is a selectable filter (line-of-sight packet) but is never
            propagation-ranked — no terrain model (design §10). */}
        <button
          type="button"
          className={`station-finder__bandtab${props.enabledBands.has('vhf-uhf') ? ' on' : ' off'}`}
          aria-pressed={props.enabledBands.has('vhf-uhf')}
          title="Line-of-sight packet — shown when selected, never propagation-ranked"
          onClick={() => props.onToggleBand('vhf-uhf')}
        >
          VHF/UHF
        </button>

        <span className="station-finder__modes">
          {FILTER_MODES.map(({ mode, label }) => (
            <button
              key={mode}
              type="button"
              className={`station-finder__chip${props.enabledModes.has(mode) ? ' on' : ' off'}`}
              aria-pressed={props.enabledModes.has(mode)}
              onClick={() => props.onToggleMode(mode)}
            >
              <span className={`station-finder__sw station-finder__sw--${mode}`} />
              {label}
            </button>
          ))}
        </span>

        {/* Station-type filter (Task 23, spec §5): Gateway/Peer. Hidden — not
            disabled — when the P2P finder_peers capability is off (R5-8). */}
        {props.showPeerType && (
          <span className="station-finder__types" data-testid="type-chips">
            {STATION_TYPES.map(({ type, label }) => (
              <button
                key={type}
                type="button"
                data-testid={`type-chip-${type}`}
                className={`station-finder__chip${props.enabledTypes.has(type) ? ' on' : ' off'}`}
                aria-pressed={props.enabledTypes.has(type)}
                onClick={() => props.onToggleType(type)}
              >
                {label}
              </button>
            ))}
          </span>
        )}

        {/* Search group merged onto the band-bar row (reclaims the old filter
            row); pushed to the right of the bands/modes via margin-left:auto. */}
        <span className="station-finder__searchgroup">
          <span className="station-finder__grouplab">Search</span>
          <input
            type="search"
            className="station-finder__search"
            aria-label="Filter stations by callsign"
            placeholder="Filter by callsign…"
            value={props.search}
            onChange={(e) => props.onSearchChange(e.target.value)}
          />
          <label className="station-finder__radius">
            <span className="station-finder__lab">Within</span>
            <select
              aria-label="Search radius"
              value={props.radiusMi == null ? 'all' : String(props.radiusMi)}
              disabled={!props.hasOperatorGrid}
              title={props.hasOperatorGrid ? undefined : 'Set your location in the status bar to filter by distance'}
              onChange={(e) => props.onRadiusChange(e.target.value === 'all' ? null : Number(e.target.value))}
            >
              {RADIUS_OPTIONS.map((o) => (
                <option key={o.label} value={o.mi == null ? 'all' : String(o.mi)}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
          {!props.hasOperatorGrid && (
            <span className="station-finder__stale">set your location (status bar) for distance + bearing</span>
          )}
          {props.filterExtra}
        </span>
      </div>
    </div>
  );
}
