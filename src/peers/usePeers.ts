// usePeers — the P2P Peers data layer (TanStack Query) + cross-window
// `peers:changed` invalidation. Task 22.
//
// Mirrors `src/contacts/useContacts.ts` (query + invoke + the H9 cross-window
// listen()/invalidate effect). This task is read-only on the frontend (no
// mutation commands land here — those are later tasks), so the hook exposes
// just the roster + a loading flag, plus a sibling `useP2pCapabilities` for
// the integration-matrix bits (`p2p_capabilities`).
//
// Contract:
//   - `peers_read` returns the whole `PeersFile`; the hook exposes `.peers`
//     (defaulting to []) and `.schemaVersion`.
//   - A `useEffect` subscribes to the app-level `peers:changed` event
//     (`src-tauri/src/peers/commands.rs::PEERS_CHANGED_EVENT` — the exact
//     string is a frontend contract per that file's doc comment) and
//     invalidates `['peers']` on fire, so any window's peers mutation
//     propagates here. Tolerates a missing Tauri runtime (test/dev harness)
//     via `.catch` — the query's own refetch remains the fallback.

import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { P2pCapabilities, PeersFile } from './types';

/// Query key for the whole peers file.
export const PEERS_QUERY_KEY = ['peers'] as const;

/// App-level Tauri event the Rust command layer emits after every peers
/// mutation. Mirrors `PEERS_CHANGED_EVENT` in `src-tauri/src/peers/commands.rs`.
export const PEERS_CHANGED_EVENT = 'peers:changed';

/// Query key for the P2P capabilities bits.
export const P2P_CAPABILITIES_QUERY_KEY = ['p2p-capabilities'] as const;

export interface UsePeers {
  peers: PeersFile['peers'];
  schemaVersion: number | undefined;
  isLoading: boolean;
}

export function usePeers(): UsePeers {
  const qc = useQueryClient();

  const query = useQuery({
    queryKey: PEERS_QUERY_KEY,
    queryFn: () => invoke<PeersFile>('peers_read'),
  });

  // Cross-window propagation. Subscribe once; invalidate on fire. Mirrors
  // useContacts's H9 handling (the `cancelled` flag guards an unmount before
  // the listen() promise resolves).
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    listen<void>(PEERS_CHANGED_EVENT, () => {
      void qc.invalidateQueries({ queryKey: PEERS_QUERY_KEY });
    })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch(() => {
        // No Tauri runtime here — the query's own refetch remains the fallback.
      });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [qc]);

  return {
    peers: query.data?.peers ?? [],
    schemaVersion: query.data?.schema_version,
    isLoading: query.isLoading,
  };
}

export interface UseP2pCapabilities {
  capabilities: P2pCapabilities | undefined;
  isLoading: boolean;
}

/// The P2P integration-matrix capability bits (spec R5-8). Read-only,
/// informational query — the backend never mutates capabilities at runtime
/// (each bit flips only when its own task's binary lands), so this has no
/// change-event subscription.
export function useP2pCapabilities(): UseP2pCapabilities {
  const query = useQuery({
    queryKey: P2P_CAPABILITIES_QUERY_KEY,
    queryFn: () => invoke<P2pCapabilities>('p2p_capabilities'),
  });

  return {
    capabilities: query.data,
    isLoading: query.isLoading,
  };
}
