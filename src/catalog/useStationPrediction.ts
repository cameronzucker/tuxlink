// Hook: predict the selected station's HF path and expose a discriminated
// status so the UI can light up the propagation panel when 'ok', show "set your
// location" when 'no-location', and silently fall back to distance-only when
// 'unavailable' (engine not bundled — the U1 degrade path).
//
// Implemented with useEffect + a non-rejecting .then/.catch (the useModemStatus
// pattern) rather than react-query: routing a rejected `invoke` through a
// react-query queryFn leaves a phantom unhandled rejection under the test runner
// even when fully caught. A cancelled-guard prevents setState after unmount.

import { useEffect, useState } from 'react';
import { predictPath, isUnavailable, type PathPrediction } from './propagationApi';
import type { Station } from './stationModel';

export type PredictionStatus = 'idle' | 'no-location' | 'loading' | 'ok' | 'unavailable' | 'error';

export interface StationPredictionResult {
  prediction: PathPrediction | null;
  status: PredictionStatus;
}

/** Distinct HF dials for a station, ascending, capped to the engine's 11. */
export function hfDials(station: Station): number[] {
  const set = new Set<number>();
  for (const ch of station.channels) {
    if (ch.band && ch.band !== 'vhf-uhf') set.add(ch.frequencyKhz);
  }
  return [...set].sort((a, b) => a - b).slice(0, 11);
}

export function useStationPrediction(
  operatorGrid: string,
  station: Station | null,
): StationPredictionResult {
  const grid = operatorGrid.trim();
  const stationCall = station?.baseCallsign ?? null;
  const stationGrid = station?.grid ?? null;
  const stationAntenna = station?.gatewayAntenna ?? null;
  const dialsKey = station ? hfDials(station).join(',') : '';

  const [state, setState] = useState<StationPredictionResult>({ prediction: null, status: 'idle' });

  useEffect(() => {
    if (!station) {
      setState({ prediction: null, status: 'idle' });
      return;
    }
    if (grid.length === 0) {
      setState({ prediction: null, status: 'no-location' });
      return;
    }
    const dials = hfDials(station);
    if (dials.length === 0) {
      // No HF dials to forecast (e.g. a packet-only station) — nothing to
      // predict; the rail degrades to channels-without-reliability.
      setState({ prediction: null, status: 'unavailable' });
      return;
    }

    let cancelled = false;
    setState({ prediction: null, status: 'loading' });
    // Compute the outcome in .then/.catch but COMMIT it in .finally — the
    // terminal link. Committing terminally (and waiting on it in tests) gives
    // the runtime's rejection bookkeeping enough slack to clear a transient
    // unhandled-rejection flag the test runner would otherwise report, mirroring
    // the repo's useModemStatus pattern.
    let next: StationPredictionResult = { prediction: null, status: 'error' };
    predictPath(grid, station.grid, dials, station.gatewayAntenna)
      .then((prediction) => {
        next = { prediction, status: 'ok' };
      })
      .catch((err) => {
        next = { prediction: null, status: isUnavailable(err) ? 'unavailable' : 'error' };
      })
      .finally(() => {
        if (!cancelled) setState(next);
      });
    return () => {
      cancelled = true;
    };
    // Inputs are fully captured by the primitive deps below (station's identity
    // is represented by call+grid+dialsKey); `station` itself is intentionally
    // not a dep to avoid redundant re-predicts on unrelated re-renders.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [grid, stationCall, stationGrid, stationAntenna, dialsKey]);

  return state;
}
