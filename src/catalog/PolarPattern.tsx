// PolarPattern — a compact quarter-polar elevation-lobe preview for the
// Find-a-Station antenna picker (tuxlink-bl01 Group E). Plots the 91-point
// gain-vs-elevation slice returned by `antenna_pattern_preview`: the horizon
// (0°) runs along the bottom edge, the zenith (90°) straight up, and the lobe
// radius is the gain normalized to the pattern's own peak over a fixed dynamic
// range. A flat pattern (the neutral `unknown` model) renders a uniform arc
// with a "not modeled" label rather than a misleading lobe.
//
// Dark-theme-friendly (CSS vars / currentColor), no external deps.

import { useMemo } from 'react';

export interface PolarPatternProps {
  /** gainsDbi[i] = gain (dBi) at elevation i° (0 = horizon, 90 = zenith). */
  gainsDbi: number[];
  /** Elevation (deg) of the peak gain — marks the main-lobe takeoff angle. */
  peakElevationDeg: number;
  /** Dynamic range (dB) shown from the peak inward. Default 36 dB. */
  rangeDb?: number;
}

const W = 120;
const H = 74;
const PAD = 6;
const FLAT_EPS = 0.01;
const FLAT_RADIUS_FRAC = 0.6;

export function PolarPattern({ gainsDbi, peakElevationDeg, rangeDb = 36 }: PolarPatternProps) {
  // Origin at the bottom-left corner; the quarter-circle opens up-and-right.
  const ox = PAD;
  const oy = H - PAD;
  const R = Math.min(W - 2 * PAD, H - 2 * PAD);

  const { lobePath, peak, flat } = useMemo(() => {
    const n = gainsDbi.length;
    const peakVal = n ? Math.max(...gainsDbi) : 0;
    const minVal = n ? Math.min(...gainsDbi) : 0;
    const isFlat = peakVal - minVal < FLAT_EPS;
    const floor = peakVal - rangeDb;
    const norm = (g: number): number => {
      if (isFlat) return FLAT_RADIUS_FRAC;
      const t = (g - floor) / (peakVal - floor);
      return Math.max(0, Math.min(1, t));
    };

    const ringPoint = (elevDeg: number, frac: number) => {
      const rad = (elevDeg * Math.PI) / 180;
      const r = frac * R;
      return { x: ox + r * Math.cos(rad), y: oy - r * Math.sin(rad) };
    };

    // Closed lobe path: origin → each elevation sample → back to origin.
    let d = `M ${ox.toFixed(1)},${oy.toFixed(1)}`;
    for (let i = 0; i < n; i++) {
      const elevDeg = (i / (n - 1)) * 90;
      const p = ringPoint(elevDeg, norm(gainsDbi[i]));
      d += ` L ${p.x.toFixed(1)},${p.y.toFixed(1)}`;
    }
    d += ' Z';

    const peakPt = ringPoint(peakElevationDeg, isFlat ? FLAT_RADIUS_FRAC : 1);
    return { lobePath: d, peak: peakPt, flat: isFlat };
  }, [gainsDbi, peakElevationDeg, rangeDb, ox, oy, R]);

  return (
    <svg
      className="station-finder__polar"
      viewBox={`0 0 ${W} ${H}`}
      width={W}
      height={H}
      role="img"
      aria-label="Antenna elevation pattern (horizon at left, zenith up)"
    >
      {/* Reference frame: horizon (0°) axis, zenith (90°) axis, quarter arc. */}
      <line className="station-finder__polar-axis" x1={ox} y1={oy} x2={ox + R} y2={oy} />
      <line className="station-finder__polar-axis" x1={ox} y1={oy} x2={ox} y2={oy - R} />
      <path
        className="station-finder__polar-arc"
        d={`M ${ox + R} ${oy} A ${R} ${R} 0 0 0 ${ox} ${oy - R}`}
        fill="none"
      />
      <path className="station-finder__polar-lobe" data-testid="lobe" d={lobePath} />
      {flat ? (
        <text
          className="station-finder__polar-flat"
          data-testid="polar-flat"
          x={ox + R * 0.34}
          y={oy - R * 0.5}
        >
          not modeled
        </text>
      ) : (
        <circle
          className="station-finder__polar-peak"
          data-testid="polar-peak"
          cx={peak.x}
          cy={peak.y}
          r={2.5}
        />
      )}
    </svg>
  );
}
