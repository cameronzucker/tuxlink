/**
 * useEnvProbes — subscribes to the backend's env-probe push events and
 * exposes the current snapshot list + a rerun trigger.
 *
 * On mount:
 *   1. Fetches the current snapshot via logging_env_probes_snapshot.
 *   2. Subscribes to the 'logging://probes/snapshot-updated' Tauri event.
 *      Each emission replaces the full snapshot list.
 *
 * Returns { snapshots, lastUpdated, rerun } where:
 *   - snapshots: ProbeSnapshot[]
 *   - lastUpdated: ISO string of the last time snapshots were refreshed
 *   - rerun: async fn that invokes logging_env_probes_rerun and updates state
 *
 * tuxlink-qjgx alpha-logging plan Task 7.6 / spec §8.8.
 */
import { useEffect, useState, useCallback } from 'react';
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
    let unlisten: UnlistenFn | undefined;

    // Fetch initial snapshot on mount.
    invoke<ProbeSnapshot[]>('logging_env_probes_snapshot')
      .then((s) => {
        setSnapshots(s);
        setLastUpdated(new Date().toISOString());
      })
      .catch(() => {/* backend not yet ready; wait for push event */});

    // Subscribe to push events.
    listen<ProbeSnapshot[]>('logging://probes/snapshot-updated', (e) => {
      setSnapshots(e.payload);
      setLastUpdated(new Date().toISOString());
    }).then((un) => {
      unlisten = un;
    });

    return () => {
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
