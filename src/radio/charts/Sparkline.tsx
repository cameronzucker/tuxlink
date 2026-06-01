// src/radio/charts/Sparkline.tsx
//
// Spec §5.3 — 60-second rolling-history sparkline. Pure DOM bars rather
// than an SVG path so per-bar threshold colors are trivial via class
// names; CSS handles the gradient + warn/bad palette swap. Used by:
//
//   - the Live section's throughput trace (samples = throughput-history
//     buffer in bits per second; warnAbove/badAbove disabled),
//   - the Signal section's S/N trace (samples = S/N-history buffer in
//     dB; warnBelow / badBelow color low-S/N samples).
//
// Sample buffer convention: oldest → newest. Most callers pass a fixed-
// length buffer (e.g. 60 entries for 60 seconds at the 1-second
// useSampleHistory tick); shorter buffers render with fewer bars.

import './Sparkline.css';

export interface SparklineProps {
  /** Samples ordered oldest → newest. ~60 entries for a 60s rolling view. */
  samples: number[];
  /** Min value of the displayed range. Defaults to 0. */
  min?: number;
  /**
   * Max value of the displayed range. Defaults to `max(...samples, 1)` so an
   * idle buffer of zeros doesn't divide-by-zero.
   */
  max?: number;
  /** Samples above this threshold render with the `warn` palette. */
  warnAbove?: number;
  /** Samples below this threshold render with the `warn` palette (low-is-bad). */
  warnBelow?: number;
  /** Samples above this threshold render with the `bad` palette (takes priority over warn). */
  badAbove?: number;
  /** Samples below this threshold render with the `bad` palette (low-is-bad). */
  badBelow?: number;
  /** Container height in pixels. Default 42. */
  height?: number;
}

export function Sparkline({
  samples,
  min = 0,
  max,
  warnAbove,
  warnBelow,
  badAbove,
  badBelow,
  height = 42,
}: SparklineProps) {
  const computedMax = max ?? Math.max(...samples, 1);
  const span = computedMax - min || 1;

  return (
    <div className="sparkline" style={{ height }} data-testid="sparkline">
      {samples.map((s, i) => {
        const pct = ((s - min) / span) * 100;
        // bad checks run BEFORE warn so the bad palette wins when both
        // thresholds match (e.g. a 0 dB sample is both warn-below-3 and
        // bad-below-0 in the S/N trace).
        let cls = '';
        if (badAbove !== undefined && s > badAbove) cls = 'bad';
        else if (badBelow !== undefined && s < badBelow) cls = 'bad';
        else if (warnAbove !== undefined && s > warnAbove) cls = 'warn';
        else if (warnBelow !== undefined && s < warnBelow) cls = 'warn';
        return (
          <div
            key={i}
            className={`sparkline-bar ${cls}`}
            style={{ height: `${Math.max(2, pct)}%` }}
          />
        );
      })}
    </div>
  );
}
