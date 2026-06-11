// Top conditions/band/mode bar for Find-a-Station (design §7). Presentational:
// the parent owns band + mode-filter state. SSN provenance is shown from the
// prediction (F12); SFI/K-index are shown only when a value is supplied — never
// fabricated (amateur-radio-reliability discipline: display only what we have).

import { HF_BANDS, bandLabel, type Band } from './bandPlan';

export type FilterMode = 'vara-hf' | 'ardop-hf' | 'packet';

const FILTER_MODES: { mode: FilterMode; label: string }[] = [
  { mode: 'vara-hf', label: 'VARA HF' },
  { mode: 'ardop-hf', label: 'ARDOP HF' },
  { mode: 'packet', label: 'Packet' },
];

export interface StationFinderControlsProps {
  band: Band;
  onBandChange: (band: Band) => void;
  enabledModes: Set<FilterMode>;
  onToggleMode: (mode: FilterMode) => void;
  utcHour: number;
  localTime: string;
  ssn: number | null;
  ssnAgeDays: number | null;
  sfi?: number | null;
  kIndex?: number | null;
  predictionAvailable: boolean;
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
        {!props.predictionAvailable && (
          <span className="station-finder__stale">no forecast — distance only</span>
        )}
      </div>

      <div className="station-finder__bandbar">
        <span className="station-finder__lab">Reachability on</span>
        {HF_BANDS.map((b) => (
          <button
            key={b}
            type="button"
            className={`station-finder__bandtab${props.band === b ? ' on' : ''}`}
            aria-pressed={props.band === b}
            onClick={() => props.onBandChange(b)}
          >
            {bandLabel(b)}
          </button>
        ))}
        <button
          type="button"
          className="station-finder__bandtab"
          disabled
          aria-disabled
          title="No propagation model for VHF/UHF"
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

        {props.listFetchedAtMs != null && (
          <span className="station-finder__listage" data-testid="list-age">
            {listAgeLabel(props.listFetchedAtMs)}
          </span>
        )}
        <button
          type="button"
          className="station-finder__refresh"
          onClick={props.onRefresh}
          disabled={props.refreshing}
        >
          {props.refreshing ? 'Checking…' : 'Check for newer list'}
        </button>
      </div>

      <div className="station-finder__filterbar">
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
      </div>
    </div>
  );
}
