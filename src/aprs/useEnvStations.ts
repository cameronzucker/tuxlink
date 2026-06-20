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

import { useEffect, useMemo, useRef, useState } from 'react';
import { listen, emit } from '@tauri-apps/api/event';
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

/// Cross-window snapshot handshake events (tuxlink-hzwc bug #4). A freshly
/// opened window — notably the Station Data pop-out — starts with an empty
/// accumulator and would otherwise read "no station data" until the next beacon
/// (minutes, on a sparse channel). The pop-out (client) requests a snapshot on
/// mount; the main shell (host) answers with its current per-station view-models
/// (including the from-launch history rings), so the new window shows the live
/// roster immediately and keeps updating from the shared event stream.
const SNAPSHOT_REQUEST = 'aprs-env:request-snapshot';
const SNAPSHOT_REPLY = 'aprs-env:snapshot';

export interface UseEnvStationsOptions {
  /// `'host'` (the main shell, mounted from launch) answers snapshot requests.
  /// `'client'` (a pop-out window) requests + seeds from a snapshot on mount.
  /// Omitted ⇒ neither (a standalone instance, e.g. a unit test).
  snapshotRole?: 'host' | 'client';
}

export interface UseEnvStations {
  /// Heard stations, most-recently-heard first (a live feed reads top-down).
  stations: EnvStation[];
}

export function useEnvStations(opts?: UseEnvStationsOptions): UseEnvStations {
  const role = opts?.snapshotRole;
  // Keyed by callsign so weather and telemetry from one station merge, and a
  // re-beacon updates the same view-model (appending to its history rings).
  const [byCall, setByCall] = useState<Map<string, EnvStation>>(new Map());
  // Latest accumulator, read by the host's snapshot responder without making the
  // subscription effect depend on `byCall`.
  const byCallRef = useRef(byCall);
  byCallRef.current = byCall;

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

    if (role === 'host') {
      // Answer a new window's request with the current roster.
      listen(SNAPSHOT_REQUEST, () => {
        if (!mounted) return;
        void emit(SNAPSHOT_REPLY, [...byCallRef.current.values()]).catch(() => {});
      })
        .then((un) => {
          if (!mounted) un();
          else unlisteners.push(un);
        })
        .catch(() => {});
    }

    if (role === 'client') {
      // Register the reply listener FIRST, then request — so the host's answer
      // can't arrive before we're listening.
      listen<EnvStation[]>(SNAPSHOT_REPLY, (e) => {
        if (!mounted) return;
        const incoming = e.payload ?? [];
        setByCall((prev) => {
          const next = new Map(prev);
          for (const s of incoming) {
            const existing = next.get(s.call);
            // Don't clobber a fresher locally-heard frame with an older snapshot.
            if (!existing || existing.lastHeard < s.lastHeard) next.set(s.call, s);
          }
          return pruneStations(next, Date.now());
        });
      })
        .then((un) => {
          if (!mounted) {
            un();
            return;
          }
          unlisteners.push(un);
          void emit(SNAPSHOT_REQUEST).catch(() => {});
        })
        .catch(() => {});
    }

    const sweep = setInterval(() => {
      if (!mounted) return;
      setByCall((prev) => pruneStations(prev, Date.now()));
    }, PRUNE_INTERVAL_MS);

    return () => {
      mounted = false;
      for (const un of unlisteners) un();
      clearInterval(sweep);
    };
  }, [role]);

  // tuxlink-xsv5: stable reference across renders (see useAprsPositions). A fresh
  // sorted array every render made `wx = joinWxStations(envStations, positions)`
  // change identity every render → the WX-badge GeoJSON `setData` re-fired every
  // render, part of the "drunk map" re-tile storm. Memoized ⇒ recomputes only
  // when `byCall` changes.
  const stations = useMemo(
    () => [...byCall.values()].sort((a, b) => b.lastHeard - a.lastHeard),
    [byCall],
  );
  return { stations };
}
