// AntennaControl — in-context operator-antenna + prediction-tuning row for the
// Find-a-Station panel (tuxlink-s0r1 #3c). Sits right above the map/forecast so
// the operator sets their OWN antenna where the forecast reacts to it — not
// buried in a Settings submenu (the WLE anti-pattern the operator called out).
//
// The operator's antenna drives the TX end of the VOACAP model; the gateway's
// parsed antenna drives the RX end (threaded per-station elsewhere). Required SNR
// and TX power are the other two prediction knobs. All three persist via
// propagation_prefs_write; changing one re-runs the forecast once the write lands.

import { ANTENNA_PRESET_OPTIONS, type AntennaPreset, type PropagationPrefs } from './propagationPrefs';

export interface AntennaControlProps {
  prefs: PropagationPrefs;
  onChange: (next: PropagationPrefs) => void;
  /** Inline error (e.g. a rejected save), shown beside the controls. */
  error?: string | null;
}

export function AntennaControl({ prefs, onChange, error }: AntennaControlProps) {
  return (
    <div className="station-finder__antenna" data-testid="antenna-control">
      <label className="station-finder__antenna-field station-finder__antenna-field--wide">
        <span className="station-finder__antenna-lab">Your antenna</span>
        <select
          className="station-finder__antenna-select"
          data-testid="antenna-select"
          value={prefs.antennaPreset}
          onChange={(e) => onChange({ ...prefs, antennaPreset: e.target.value as AntennaPreset })}
        >
          {ANTENNA_PRESET_OPTIONS.map((o) => (
            <option key={o.value} value={o.value} title={o.help}>
              {o.label}
            </option>
          ))}
        </select>
      </label>

      <label className="station-finder__antenna-field">
        <span className="station-finder__antenna-lab">Req SNR</span>
        <input
          className="station-finder__antenna-num"
          data-testid="req-snr-input"
          type="number"
          min={0}
          max={99}
          step={1}
          value={prefs.reqSnrDb}
          aria-label="Required SNR in dB-Hz"
          onChange={(e) => {
            const v = Number(e.target.value);
            // Persist only an in-range value; mid-typing junk just updates the
            // field without a doomed backend write (Rust enforces the same bound).
            if (Number.isFinite(v) && v >= 0 && v < 100) onChange({ ...prefs, reqSnrDb: v });
          }}
        />
        <span className="station-finder__antenna-unit">dB-Hz</span>
      </label>

      <label className="station-finder__antenna-field">
        <span className="station-finder__antenna-lab">TX power</span>
        <input
          className="station-finder__antenna-num"
          data-testid="tx-power-input"
          type="number"
          min={1}
          step={5}
          value={prefs.txPowerW}
          aria-label="TX power in watts"
          onChange={(e) => {
            const v = Number(e.target.value);
            if (Number.isFinite(v) && v > 0) onChange({ ...prefs, txPowerW: v });
          }}
        />
        <span className="station-finder__antenna-unit">W</span>
      </label>

      {error && (
        <span className="station-finder__antenna-err" role="alert">
          {error}
        </span>
      )}
    </div>
  );
}
