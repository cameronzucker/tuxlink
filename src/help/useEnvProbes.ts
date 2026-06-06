/**
 * useEnvProbes — subscribes to the backend's env-probe push events and
 * exposes the current snapshot list + a rerun trigger.
 *
 * On mount:
 *   1. Establishes the push-event listener FIRST (canonical listen-before-fetch
 *      pattern from useSessionLog) so no push event can be lost between subscribe
 *      and initial fetch.
 *   2. Inside the listen() resolve callback (i.e., only after the listener is
 *      guaranteed registered), fetches the current snapshot via
 *      logging_env_probes_snapshot. If a push event has already arrived and
 *      filled snapshots, the initial fetch is a no-op (prev.length check).
 *
 * Cancelled flag: guards against both (a) unmount-before-listen-resolves (C2)
 * and (b) state updates on an already-unmounted component.
 *
 * Returns { snapshots, lastUpdated, rerun } where:
 *   - snapshots: ProbeSnapshot[]
 *   - lastUpdated: ISO string of the last time snapshots were refreshed
 *   - rerun: async fn that invokes logging_env_probes_rerun and updates state
 *
 * tuxlink-qjgx alpha-logging plan Task 7.6 / spec §8.8.
 */
import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface ProbeSnapshot {
  probe: string;
  timestamp: string;
  trigger: string;
  result: Record<string, unknown>;
}

export function useEnvProbes() {
  const [snapshots, setSnapshots] = useState<ProbeSnapshot[]>([]);
  const [lastUpdated, setLastUpdated] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;

    listen<ProbeSnapshot[]>('logging://probes/snapshot-updated', (e) => {
      if (cancelled) return;
      setSnapshots(e.payload);
      setLastUpdated(new Date().toISOString());
    }).then((u) => {
      // If we already unmounted while listen() was resolving, immediately
      // unlisten and skip the snapshot fetch.
      if (cancelled) {
        u();
        return;
      }
      unlisten = u;

      // Now that the listener is registered, fetch the initial snapshot.
      // Push events that arrive between this point and the snapshot resolving
      // win the race (correctly), because they're handled by the listener
      // before this then() resolves.
      invoke<ProbeSnapshot[]>('logging_env_probes_snapshot')
        .then((s) => {
          if (cancelled) return;
          // Only set if we haven't already received a push event with newer data.
          // We use the listener's setSnapshots as the source of truth — if it
          // already fired, lastUpdated is non-null and this initial fetch
          // shouldn't clobber. Simple guard: if lastUpdated is null, set;
          // otherwise the push already won.
          setSnapshots((prev) => prev.length === 0 ? s : prev);
          setLastUpdated((prev) => prev ?? new Date().toISOString());
        })
        .catch(() => { /* backend unavailable or degraded; silent */ });
    });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  const rerun = useCallback(async () => {
    const fresh = await invoke<ProbeSnapshot[]>('logging_env_probes_rerun');
    setSnapshots(fresh);
    setLastUpdated(new Date().toISOString());
  }, []);

  return { snapshots, lastUpdated, rerun };
}
