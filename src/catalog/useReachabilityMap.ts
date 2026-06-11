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

/** First channel frequency a station offers on `band`, or null. */
function bandDial(station: Station, band: Band): number | null {
  const ch = station.channels.find((c) => c.band === band);
  return ch ? ch.frequencyKhz : null;
}

const EMPTY_TIERS: Map<string, ReachTier> = new Map();

export function useReachabilityMap(
  operatorGrid: string,
  stations: Station[],
  band: Band,
  utcHour: number,
): ReachabilityMap {
  const grid = operatorGrid.trim();
  const keys = stations.map(stationKey).join(',');

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
    const onBand = stations.filter((s) => bandDial(s, band) != null);
    const enabled = grid.length > 0 && band !== 'vhf-uhf' && onBand.length > 0;
    if (!enabled) {
      setData({ tiers: EMPTY_TIERS, available: false, loading: false });
      return;
    }

    let cancelled = false;
    setData((d) => ({ ...d, loading: true }));

    (async () => {
      const tiers = new Map<string, ReachTier>();
      let sawUnavailable = false;
      for (const s of onBand) {
        const dial = bandDial(s, band)!;
        // Two-arg .then (single call, not .then().catch()) → no intermediate
        // rejected promise; the awaited result never rejects.
        const outcome = await predictPath(grid, s.grid, [dial]).then(
          (p) => ({ ok: true as const, p }),
          (err) => ({ ok: false as const, err }),
        );
        if (cancelled) return;
        if (outcome.ok) {
          const rel = outcome.p.channels[0]?.relByHour[utcHour] ?? 0;
          tiers.set(stationKey(s), relToTier(rel));
        } else if (isUnavailable(outcome.err)) {
          sawUnavailable = true;
          break;
        }
        // A single non-Unavailable failure is non-fatal: leave it untiered.
      }
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
  }, [grid, band, utcHour, keys]);

  return { tiers: data.tiers, distances, available: data.available, loading: data.loading };
}
