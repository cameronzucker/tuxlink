// AntennaControl — in-context operator-antenna + prediction-tuning row for the
// Find-a-Station panel (tuxlink-s0r1 #3c; antenna picker tuxlink-bl01 Group E).
// Sits right above the map/forecast so the operator sets their OWN antenna where
// the forecast reacts to it — not buried in a Settings submenu (the WLE
// anti-pattern the operator called out).
//
// The operator's antenna drives the TX end of the VOACAP model; the gateway's
// parsed antenna drives the RX end (threaded per-station elsewhere). Required SNR
// and TX power are the other two prediction knobs. All persist via
// propagation_prefs_write; changing one re-runs the forecast once the write lands.
//
// The antenna + height resolve to a precomputed NEC pattern; the polar preview
// shows that pattern's elevation lobe live (a read-only projection of the same
// data the forecast uses). Horizontal antennas snap to a four-stop height grid;
// ground-mounted verticals have a fixed pattern (no height control).

import { useEffect, useState } from 'react';

import { PolarPattern } from './PolarPattern';
import {
  ANTENNA_PRESET_OPTIONS,
  GROUND_TYPE_OPTIONS,
  HEIGHT_GRID_M,
  NOISE_ENVIRONMENT_OPTIONS,
  TX_POWER_OPTIONS_W,
  isHeightVariable,
  readAntennaPreview,
  type AntennaPreset,
  type AntennaPreview,
  type GroundType,
  type NoiseEnvironment,
  type PropagationPrefs,
} from './propagationPrefs';

export interface AntennaControlProps {
  prefs: PropagationPrefs;
  onChange: (next: PropagationPrefs) => void;
  /** Inline error (e.g. a rejected save), shown beside the controls. */
  error?: string | null;
}

/** Index of the grid stop nearest a height (metres). */
function nearestGridIndex(heightM: number): number {
  let best = 0;
  let bestDist = Infinity;
  HEIGHT_GRID_M.forEach((v, i) => {
    const d = Math.abs(v - heightM);
    if (d < bestDist) {
      bestDist = d;
      best = i;
    }
  });
  return best;
}

export function AntennaControl({ prefs, onChange, error }: AntennaControlProps) {
  const heightVariable = isHeightVariable(prefs.antennaPreset);
  const [preview, setPreview] = useState<AntennaPreview | null>(null);

  // Live elevation-pattern preview. Reads the committed precomputed library via
  // the backend (no engine/network), debounced so dragging the slider does not
  // spam the command. Failures degrade to no preview rather than an error.
  useEffect(() => {
    let active = true;
    const handle = setTimeout(() => {
      readAntennaPreview(prefs.antennaPreset, prefs.antennaHeightM)
        .then((p) => {
          if (active) setPreview(p);
        })
        .catch(() => {
          if (active) setPreview(null);
        });
    }, 120);
    return () => {
      active = false;
      clearTimeout(handle);
    };
  }, [prefs.antennaPreset, prefs.antennaHeightM]);

  return (
    <div className="station-finder__antenna" data-testid="antenna-control">
      <span className="station-finder__grouplab">Station setup</span>
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

      {heightVariable ? (
        <label className="station-finder__antenna-field">
          <span className="station-finder__antenna-lab">
            Height {HEIGHT_GRID_M[nearestGridIndex(prefs.antennaHeightM)]} m
          </span>
          <input
            className="station-finder__antenna-slider"
            data-testid="antenna-height-slider"
            type="range"
            min={0}
            max={HEIGHT_GRID_M.length - 1}
            step={1}
            value={nearestGridIndex(prefs.antennaHeightM)}
            aria-label="Antenna apex height above ground"
            onChange={(e) => {
              const idx = Number(e.target.value);
              const snapped = HEIGHT_GRID_M[idx];
              if (snapped !== undefined) onChange({ ...prefs, antennaHeightM: snapped });
            }}
          />
        </label>
      ) : (
        <div className="station-finder__antenna-field" data-testid="antenna-ground-mounted">
          <span className="station-finder__antenna-lab">Mounting</span>
          <span className="station-finder__antenna-fixed">Ground-mounted — height fixed</span>
        </div>
      )}

      <label className="station-finder__antenna-field">
        <span className="station-finder__antenna-lab">Ground</span>
        <select
          className="station-finder__antenna-select"
          data-testid="ground-select"
          value={prefs.groundType}
          onChange={(e) => onChange({ ...prefs, groundType: e.target.value as GroundType })}
        >
          {GROUND_TYPE_OPTIONS.map((o) => (
            <option key={o.value} value={o.value} title={o.help}>
              {o.label}
            </option>
          ))}
        </select>
      </label>

      <label className="station-finder__antenna-field">
        <span className="station-finder__antenna-lab">Noise</span>
        <select
          className="station-finder__antenna-select"
          data-testid="noise-select"
          value={prefs.noiseEnvironment}
          onChange={(e) => onChange({ ...prefs, noiseEnvironment: e.target.value as NoiseEnvironment })}
        >
          {NOISE_ENVIRONMENT_OPTIONS.map((o) => (
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
        <span className="station-finder__antenna-lab">TX W</span>
        <select
          className="station-finder__antenna-select"
          data-testid="tx-power-input"
          value={String(prefs.txPowerW)}
          aria-label="TX power in watts"
          onChange={(e) => {
            const v = Number(e.target.value);
            if (Number.isFinite(v) && v > 0) onChange({ ...prefs, txPowerW: v });
          }}
        >
          {/* Preserve a persisted off-list value as a one-off option so it is
              never silently changed; otherwise just the standard power steps. */}
          {!TX_POWER_OPTIONS_W.includes(prefs.txPowerW as (typeof TX_POWER_OPTIONS_W)[number]) && (
            <option value={String(prefs.txPowerW)}>{prefs.txPowerW} W</option>
          )}
          {TX_POWER_OPTIONS_W.map((w) => (
            <option key={w} value={String(w)}>
              {w} W
            </option>
          ))}
        </select>
      </label>

      <div className="station-finder__antenna-field station-finder__antenna-preview" data-testid="antenna-preview">
        <span className="station-finder__antenna-lab">
          Pattern{preview ? ` — peak ${preview.peakElevationDeg}°` : ''}
        </span>
        {preview ? (
          <PolarPattern gainsDbi={preview.gainsDbi} peakElevationDeg={preview.peakElevationDeg} />
        ) : (
          <span className="station-finder__antenna-fixed">…</span>
        )}
      </div>

      <span className="station-finder__antenna-note">
        Patterns model poor / dry-desert ground regardless of the Ground selection.
      </span>

      {error && (
        <span className="station-finder__antenna-err" role="alert">
          {error}
        </span>
      )}
    </div>
  );
}
