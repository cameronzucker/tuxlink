// src/radio/useSampleHistory.ts
//
// Rolling fixed-length sample buffer for the ARDOP panel's S/N and
// throughput sparklines. Reads the latest value out of a React ref so
// the interval-driven tick always sees the freshest reading without
// re-creating the timer on every render.
//
// Spec §5.3 — 60-sample buffer at 1-second cadence produces a 60-second
// rolling window in the sparkline. Smaller lengths / faster intervals
// fall out for ad-hoc charts.

import { useEffect, useRef, useState } from 'react';

/**
 * Maintain a rolling buffer of length `length`. On each `intervalMs`
 * tick, push the latest value of `current` onto the back of the buffer
 * and drop the oldest sample off the front. `current` is read out of a
 * ref so a parent re-render with a new value is picked up by the next
 * tick without restarting the interval.
 *
 * Initial buffer is zero-filled so consumers can render a sparkline
 * before any real samples have arrived (the bars render as faint
 * 2%-height nubs rather than a missing chart).
 *
 * @param current latest sample value (or null = "no reading yet", treated as 0).
 * @param length  number of samples to hold; sparklines typically use 60.
 * @param intervalMs how often to tick. Default 1000 ms.
 */
export function useSampleHistory(
  current: number | null,
  length: number,
  intervalMs: number = 1000,
): number[] {
  // Lazy init: don't allocate a fresh zero-array on every render.
  const [samples, setSamples] = useState<number[]>(() =>
    new Array(length).fill(0),
  );
  // Track the latest value via a ref so the interval callback always
  // reads the freshest reading without requiring an `intervalMs`-derived
  // restart of the timer on every value change.
  const latest = useRef(current);
  latest.current = current;

  useEffect(() => {
    const id = setInterval(() => {
      setSamples((prev) => [...prev.slice(1), latest.current ?? 0]);
    }, intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  return samples;
}
