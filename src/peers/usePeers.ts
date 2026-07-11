// usePeers — the P2P Peers data layer, re-sourced by Task T-E onto Contacts
// (operator pivot: a peer IS a contact now; `peers_read`/`peers:changed` are
// gone — see `../contacts/store.rs` + `../contacts/reachability.rs`).
//
// `usePeers()` delegates straight to `useContacts()` (`../contacts/useContacts`)
// rather than running a second `useQuery` against the same data: TanStack
// Query's cache is keyed by `CONTACTS_QUERY_KEY`, so a second independent
// `useQuery` call with that same key would ALREADY dedupe the network fetch —
// but delegating avoids re-declaring the query + the `contacts:changed`
// listen()/invalidate effect a second time, and gives every `usePeers()` call
// site the exact same contract useContacts's own consumers get. The `.peers`
// field is the raw `Contact[]` (unfiltered) — `aggregatePeers` (`./peerModel`)
// is the view-model that filters to reachability-bearing records and derives
// the rail/map-facing shape.
//
// `useP2pCapabilities` is UNCHANGED by this task (still its own `useQuery`
// against `p2p_capabilities`, which now lives in `contacts/commands.rs` but
// kept its command name across T-B/T-D).

import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { useContacts } from '../contacts/useContacts';
import type { Contact } from '../contacts/types';
import type { P2pCapabilities } from './types';

/// Query key for the P2P capabilities bits.
export const P2P_CAPABILITIES_QUERY_KEY = ['p2p-capabilities'] as const;

export interface UsePeers {
  peers: Contact[];
  isLoading: boolean;
}

/// Re-sourced onto Contacts (T-E): `usePeers()` is now a thin projection of
/// `useContacts()` — same query key, same `contacts:changed` invalidation, no
/// second fetch. `aggregatePeers(usePeers().peers)` is the caller's job.
export function usePeers(): UsePeers {
  const { contacts, isLoading } = useContacts();
  return {
    peers: contacts,
    isLoading,
  };
}

export interface UseP2pCapabilities {
  capabilities: P2pCapabilities | undefined;
  isLoading: boolean;
}

/// The P2P integration-matrix capability bits (spec R5-8). Read-only,
/// informational query — the backend never mutates capabilities at runtime
/// (each bit flips only when its own task's binary lands), so this has no
/// change-event subscription. UNTOUCHED by Task T-E.
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
