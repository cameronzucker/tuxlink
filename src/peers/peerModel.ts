// aggregatePeers — distinct P2P-peer aggregation. Task 22.
//
// Distinct from catalog/aggregateStations [R4-8]: keys on `canonical_base`
// alone, tolerates a missing grid, and never drops gridless/telnet-only peers
// so the rail can render them untiered (rail-only, not map-placeable).

import type { Peer } from './types';

export interface AggregatedPeer {
  id: string;
  canonicalBase: string;
  presentedCallsigns: string[];
  origin: Peer['origin'];
  grid?: string; // undefined when the peer has no grid
  mapPlaceable: boolean; // false ⇒ rail-only, untiered
  lastConnectedAt: string | null;
  channels: Peer['channels'];
  endpoints: Peer['endpoints'];
}

/**
 * Distinct from catalog/aggregateStations [R4-8]: keys on canonical_base,
 * tolerates a missing grid, and never drops gridless/telnet-only peers.
 */
export function aggregatePeers(peers: Peer[]): AggregatedPeer[] {
  if (!Array.isArray(peers)) return [];
  return peers.map((p) => {
    const grid = p.grid?.value?.trim() || undefined;
    return {
      id: p.id,
      canonicalBase: p.canonical_base,
      presentedCallsigns: p.presented_callsigns,
      origin: p.origin,
      grid,
      mapPlaceable: Boolean(grid),
      lastConnectedAt: p.last_connected_at,
      channels: p.channels,
      endpoints: p.endpoints,
    };
  });
}
