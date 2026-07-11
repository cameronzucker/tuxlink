// aggregatePeers — the reachability view-model over Contact records. Task 22,
// re-sourced by Task T-E (operator pivot: a peer IS a contact; the separate
// peers.json entity died — spec §AMENDMENT).
//
// Distinct from catalog/aggregateStations [R4-8]: keys on the EXACT contact
// callsign (no base-normalization merging — the pivot's routing rule), never
// drops a contact just because it has no grid, and includes ONLY contacts
// with at least one channel or endpoint (a contact with neither has no
// reachability to aggregate — it is address-book-only, not a "peer").

import type { Channel, Contact, ContactTier, Endpoint, Origin } from '../contacts/types';

export interface AggregatedPeer {
  id: string;
  /** The contact's EXACT presented callsign (SSID-bearing, never stripped). */
  callsign: string;
  origin: Origin;
  tier: ContactTier;
  grid?: string; // undefined when the contact has no grid
  mapPlaceable: boolean; // false ⇒ rail-only, untiered
  /** Derived recency (Contact carries no `last_connected_at`): the most
   *  recent `last_seen` across the contact's channels + endpoints, or `null`
   *  when it has never been observed. */
  lastSeen: string | null;
  channels: Channel[];
  endpoints: Endpoint[];
}

/** The most recent of a list of ISO timestamp strings (by actual instant, not
 *  lexicographic order — offsets can differ), or `null` if none parse. */
function latestOf(timestamps: string[]): string | null {
  let best: string | null = null;
  let bestMs = -Infinity;
  for (const ts of timestamps) {
    const ms = Date.parse(ts);
    if (!Number.isNaN(ms) && ms > bestMs) {
      bestMs = ms;
      best = ts;
    }
  }
  return best;
}

/**
 * Reachability view-model over Contact records (spec §AMENDMENT). Keys on the
 * exact callsign (no merging), tolerates a missing grid, and includes only
 * contacts with ≥1 channel or endpoint — a contact with neither is
 * address-book-only, not surfaced as a peer.
 */
export function aggregatePeers(contacts: Contact[]): AggregatedPeer[] {
  if (!Array.isArray(contacts)) return [];
  return contacts
    .filter((c) => (c.channels?.length ?? 0) > 0 || (c.endpoints?.length ?? 0) > 0)
    .map((c) => {
      const channels = c.channels ?? [];
      const endpoints = c.endpoints ?? [];
      const grid = c.grid?.value?.trim() || undefined;
      const lastSeen = latestOf([
        ...channels.map((ch) => ch.last_seen),
        ...endpoints.map((e) => e.last_seen),
      ]);
      return {
        id: c.id,
        callsign: c.callsign,
        origin: c.origin ?? 'unknown',
        tier: c.tier ?? 'confirmed',
        grid,
        mapPlaceable: Boolean(grid),
        lastSeen,
        channels,
        endpoints,
      };
    });
}
