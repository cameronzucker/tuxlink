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
import { baseCallsign } from '../catalog/stationModel';

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
   *  when it has never been observed. Bumps on FAILED attempts too — do NOT
   *  use it for a "reached / heard" claim. */
  lastSeen: string | null;
  /** Derived SUCCESS recency (T-F Part 0): the most recent `last_ok` across
   *  the contact's channels + endpoints, or `null` when nothing has ever
   *  completed. The only honest source for a "reached / heard" tier. */
  lastOk: string | null;
  channels: Channel[];
  endpoints: Endpoint[];
}

/** A peer's tac-chat map visual (spec §6): a color tier class + a dashed
 *  modifier. Structurally identical to `map/PeerLayer::PeerVisual`. */
export interface PeerTacTier {
  tierClass: string;
  dashed: boolean;
}

/**
 * Tac-chat map tier for a peer (spec §6), success-only (T-F Part 0). The color
 * axis is session OUTCOME: `live` (currently connected) → `reached`/`stale` by
 * the last SUCCESS instant (`lastOk`, NEVER a failed-attempt timestamp) →
 * `failed` (attempts exist, none succeeded) → `unknown`. `dashed` marks a
 * never-connected manual peer (origin `manual` with no success on record).
 *
 * The bug this fixes (T-F Part 0): keying the reached/stale branch on
 * `lastSeen` made `peer-pin--failed` unreachable, since a failed dial bumps
 * `lastSeen` and the reached branch then won. Keying on `lastOk` restores the
 * failed tier and keeps every non-live label a literal success claim.
 */
export function peerTacChatTier(
  peer: AggregatedPeer,
  opts: { livePeerBase: string | null; nowMs: number },
): PeerTacTier {
  const { livePeerBase, nowMs } = opts;
  const dashed = peer.origin === 'manual' && peer.lastOk == null;
  let tierClass: string;
  if (livePeerBase && baseCallsign(peer.callsign) === livePeerBase) {
    tierClass = 'peer-pin--live';
  } else if (peer.lastOk) {
    const ageMs = nowMs - Date.parse(peer.lastOk);
    tierClass = ageMs <= 3_600_000 ? 'peer-pin--reached' : 'peer-pin--stale';
  } else if (peer.channels.some((c) => c.counts.fail > 0)) {
    tierClass = 'peer-pin--failed';
  } else {
    tierClass = 'peer-pin--unknown';
  }
  return { tierClass, dashed };
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
      // Success-only recency (T-F Part 0): a failed dial bumps last_seen but
      // never last_ok, so this is the only honest "reached / heard" instant.
      const lastOk = latestOf([
        ...channels.map((ch) => ch.last_ok ?? ''),
        ...endpoints.map((e) => e.last_ok ?? ''),
      ]);
      return {
        id: c.id,
        callsign: c.callsign,
        origin: c.origin ?? 'unknown',
        tier: c.tier ?? 'confirmed',
        grid,
        mapPlaceable: Boolean(grid),
        lastSeen,
        lastOk,
        channels,
        endpoints,
      };
    });
}
