// Hook: rank visible stations by predicted reachability on the selected band at
// a fixed UTC hour (design §7 — "distance is not the ranking; reachability is").
// One point-to-point voacapl run per station on a representative band dial (§5
// option a). Degrades to distance-only when the engine is Unavailable: tiers
// empty, available=false, distances always present.
//
// useEffect + per-call .then/.catch (never-rejecting) rather than react-query —
// see useStationPrediction for why. A cancelled-guard prevents late setState.

import { useEffect, useMemo, useState } from 'react';
import { predictPath, isUnavailable } from './propagationApi';
import { relToTier, type ReachTier } from './reachability';
import { distanceFromGrids, kmToMi } from './distance';
import type { Band } from './bandPlan';
import type { Station } from './stationModel';

export function stationKey(s: Station): string {
  return `${s.baseCallsign}|${s.grid}`;
}

export interface ReachabilityMap {
  tiers: Map<string, ReachTier>;
  distances: Map<string, number>; // miles from operator grid
  available: boolean;
  loading: boolean;
}

/** Frequencies (kHz) a station offers on any of the selected HF bands. VHF/UHF
 *  is excluded — it has no propagation model (design §10), so it never tiers. */
function stationDials(station: Station, bands: Set<Band>): number[] {
  return station.channels
    .filter((c) => c.band != null && c.band !== 'vhf-uhf' && bands.has(c.band))
    .map((c) => c.frequencyKhz);
}

const EMPTY_TIERS: Map<string, ReachTier> = new Map();

// Max voacapl predictions in flight at once. Each run is a short CPU-bound
// process on a backend blocking thread; a small pool keeps the cores busy
// without oversubscribing the dev Pi (4 cores) when a band has many stations.
const PREDICT_CONCURRENCY = 6;

export function useReachabilityMap(
  operatorGrid: string,
  stations: Station[],
  bands: Set<Band>,
  utcHour: number,
): ReachabilityMap {
  const grid = operatorGrid.trim();
  const keys = stations.map(stationKey).join(',');
  // Stable dep key for the selected-band set (Set identity changes every render).
  const bandsKey = useMemo(() => [...bands].sort().join(','), [bands]);

  // Distances are pure + always available.
  const distances = useMemo(() => {
    const m = new Map<string, number>();
    for (const s of stations) {
      const km = grid ? distanceFromGrids(grid, s.grid) : null;
      if (km != null) m.set(stationKey(s), kmToMi(km));
    }
    return m;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [grid, keys]);

  const [data, setData] = useState<{ tiers: Map<string, ReachTier>; available: boolean; loading: boolean }>({
    tiers: EMPTY_TIERS,
    available: false,
    loading: false,
  });

  useEffect(() => {
    // Stations with at least one dial on a selected HF band (VHF/UHF excluded in
    // stationDials, so a VHF-only selection yields no tiers — distance-only).
    const onBand = stations.filter((s) => stationDials(s, bands).length > 0);
    const enabled = grid.length > 0 && onBand.length > 0;
    if (!enabled) {
      setData({ tiers: EMPTY_TIERS, available: false, loading: false });
      return;
    }

    let cancelled = false;
    setData((d) => ({ ...d, loading: true }));

    (async () => {
      const tiers = new Map<string, ReachTier>();
      let sawUnavailable = false;

      // Each prediction is an independent voacapl run; the Rust command runs on
      // a blocking thread, so the calls can overlap. Dispatch them through a
      // bounded worker pool rather than a serial `for await` (which made the map
      // fill in tier-by-tier over seconds). JS is single-threaded so the shared
      // `tiers`/flag writes are race-free — workers only yield at the `await`.
      let next = 0;
      const worker = async () => {
        while (!cancelled && !sawUnavailable) {
          const i = next++;
          if (i >= onBand.length) return;
          const s = onBand[i];
          const dials = stationDials(s, bands);
          // Predict all of the station's selected-band dials in one run; its tier
          // is the BEST band it can reach right now (multi-band selection).
          // Two-arg .then (single call, not .then().catch()) → no intermediate
          // rejected promise; the awaited result never rejects.
          const outcome = await predictPath(grid, s.grid, dials).then(
            (p) => ({ ok: true as const, p }),
            (err) => ({ ok: false as const, err }),
          );
          if (cancelled) return;
          if (outcome.ok) {
            const bestRel = outcome.p.channels.reduce(
              (best, ch) => Math.max(best, ch.relByHour[utcHour] ?? 0),
              0,
            );
            tiers.set(stationKey(s), relToTier(bestRel));
          } else if (isUnavailable(outcome.err)) {
            // Engine not bundled — every call would fail the same way; stop
            // dispatching and degrade to distance-only ranking.
            sawUnavailable = true;
          }
          // A single non-Unavailable failure is non-fatal: leave it untiered.
        }
      };
      const poolSize = Math.min(PREDICT_CONCURRENCY, onBand.length);
      await Promise.all(Array.from({ length: poolSize }, worker));

      if (!cancelled) {
        setData({
          tiers: sawUnavailable ? EMPTY_TIERS : tiers,
          available: !sawUnavailable,
          loading: false,
        });
      }
    })();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [grid, bandsKey, utcHour, keys]);

  return { tiers: data.tiers, distances, available: data.available, loading: data.loading };
}
