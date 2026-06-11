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
  onRefresh: () => void;
  refreshing: boolean;
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

        <button
          type="button"
          className="station-finder__refresh"
          onClick={props.onRefresh}
          disabled={props.refreshing}
        >
          {props.refreshing ? 'Checking…' : 'Check for newer list'}
        </button>
      </div>
    </div>
  );
}
