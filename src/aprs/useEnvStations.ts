// src/aprs/useEnvStations.ts
//
// React hook backing the source-reactive environmental panel (tuxlink-2phz).
// APRS is an OPEN CHANNEL: every station's weather + telemetry beacons are heard
// by all. This hook subscribes to BOTH backend emit seams — `aprs-weather:new`
// (fixed WX fields) and `aprs-telemetry:new` (self-named T# channels) — and
// merges them BY CALLSIGN into one per-station view-model (see envStations.ts).
//
// The engine emits point-in-time DTOs; the graded charts need a SERIES, so the
// per-channel history ring is buffered HERE, frontend-side, from the moment the
// hook mounts. Mount it once at the shell top level (like useAprsPositions) so
// history accumulates from app launch regardless of whether the panel tab is
// open — opening the tab later then shows the buffered series, not an empty
// graph that only starts filling on first view.

import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { WeatherReportDto, InboundTelemetryDto } from './aprsTypes';
import {
  applyWeather,
  applyTelemetry,
  pruneStations,
  type EnvStation,
} from './envStations';

/// How often the silent-station sweep runs, so a station drops after
/// `STATION_TTL_MS` of silence even when no further traffic arrives to trigger
/// the per-frame prune.
const PRUNE_INTERVAL_MS = 60 * 1000;

export interface UseEnvStations {
  /// Heard stations, most-recently-heard first (a live feed reads top-down).
  stations: EnvStation[];
}

export function useEnvStations(): UseEnvStations {
  // Keyed by callsign so weather and telemetry from one station merge, and a
  // re-beacon updates the same view-model (appending to its history rings).
  const [byCall, setByCall] = useState<Map<string, EnvStation>>(new Map());

  useEffect(() => {
    let mounted = true;
    const unlisteners: Array<() => void> = [];

    const subscribe = <T>(event: string, apply: (prev: EnvStation | undefined, dto: T, at: number) => EnvStation) => {
      listen<T>(event, (e) => {
        if (!mounted) return;
        const dto = e.payload;
        const call = (dto as { station: string }).station;
        const at = Date.now();
        setByCall((prev) => {
          const next = new Map(prev);
          next.set(call, apply(prev.get(call), dto, at));
          // Sweep on every frame too, so a busy channel stays trimmed without
          // waiting for the interval tick.
          return pruneStations(next, at);
        });
      })
        .then((un) => {
          if (!mounted) un();
          else unlisteners.push(un);
        })
        .catch(() => {
          // listen() unavailable (jsdom without Tauri — mocked in tests).
        });
    };

    subscribe<WeatherReportDto>('aprs-weather:new', applyWeather);
    subscribe<InboundTelemetryDto>('aprs-telemetry:new', applyTelemetry);

    const sweep = setInterval(() => {
      if (!mounted) return;
      setByCall((prev) => pruneStations(prev, Date.now()));
    }, PRUNE_INTERVAL_MS);

    return () => {
      mounted = false;
      for (const un of unlisteners) un();
      clearInterval(sweep);
    };
  }, []);

  const stations = [...byCall.values()].sort((a, b) => b.lastHeard - a.lastHeard);
  return { stations };
}
