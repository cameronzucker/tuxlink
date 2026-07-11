// recentStatus — the per-row honest reachability label for the ContactsPanel
// "Recent" section + contact-detail reachability block (Task T-F).
//
// Operator-dictated label semantics (spec §AMENDMENT pt. 7; the operator
// personally rejected the prior design at the mock gate — label LITERALNESS is
// the point):
//   - A row with a COMPLETED session (any `last_ok`) carries the "Heard"
//     distinction. "Heard" is the INCOMING flavor ("literally heard, dialed
//     into my station"); "reached" is the OUTGOING flavor ("I reached them").
//     THE ROW, not the section, makes the RF claim — so the flavor comes from
//     the specific successful observation, not the record's overall origin.
//   - A row with attempts but NO success reads "dialed · not reached yet".
//   - "worked" is BANNED; "verified/confirmed" is CURATION, never identity
//     authentication, and never appears here.
//
// Truth rests entirely on `last_ok` + `last_ok_direction` (success-only, T-F
// Part 0 + review Finding 1): a failed dial bumps `last_seen` AND mutates
// `Channel.direction`, but never touches the `last_ok*` pair — so neither the
// timestamp nor the verb can be contaminated by a failure. When a success has
// no recorded direction (pre-T-F record, forward-compat `unknown`), the verb
// degrades to the direction-neutral "connected" — a completed session is
// literally a connection — rather than guessing a direction.

import type { Channel, ChannelTransport, Contact, Endpoint } from '../contacts/types';
import { relativeAgo } from '../favorites/record-format';

/** The verb a success carries: "heard" (incoming), "reached" (outgoing), or
 *  the direction-neutral "connected" when the success's direction is unknown. */
type Flavor = 'heard' | 'reached' | 'connected';

/** The honest status of a reachability-bearing contact row. */
export type RecentStatus =
  | { kind: 'heard'; when: string; summary: string | null }
  | { kind: 'reached'; when: string; summary: string | null }
  | { kind: 'connected'; when: string; summary: string | null }
  | { kind: 'dialed-not-reached' }
  | { kind: 'none' };

const TRANSPORT_LABEL: Record<ChannelTransport, string> = {
  packet: 'Packet',
  ardop: 'ARDOP HF',
  'vara-hf': 'VARA HF',
  'vara-fm': 'VARA FM',
  unknown: 'Unknown',
};

/** e.g. "VARA HF · 7.101 MHz" (freq omitted when the channel has none). */
export function channelSummary(ch: Channel): string {
  const label = TRANSPORT_LABEL[ch.transport] ?? ch.transport;
  if (ch.freq_hz == null) return label;
  return `${label} · ${(ch.freq_hz / 1_000_000).toFixed(3)} MHz`;
}

/** The verb for a channel's success. Keys on `last_ok_direction` — the
 *  direction captured atomically WITH the success — NEVER on
 *  `Channel.direction`, which mutates on failures too (review Finding 1: an
 *  incoming success followed by an outgoing failed dial must still read
 *  "heard"). Absent/unknown → direction-neutral, no guessing. */
function channelFlavor(ch: Channel): Flavor {
  if (ch.last_ok_direction === 'incoming') return 'heard';
  if (ch.last_ok_direction === 'outgoing') return 'reached';
  return 'connected';
}

/** The verb for an endpoint's success. Provenance is part of the endpoint
 *  dedup key (never failure-mutated), so it is a truthful flavor source:
 *  `observed-incoming` = they dialed us; `operator` = we dialed them.
 *  Unknown provenance → direction-neutral. */
function endpointFlavor(ep: Endpoint): Flavor {
  if (ep.provenance === 'observed-incoming') return 'heard';
  if (ep.provenance === 'operator') return 'reached';
  return 'connected';
}

/**
 * Derive the honest reachability status of a contact from its channels +
 * endpoints. Picks the MOST RECENT success (`last_ok`, by absolute instant) and
 * reports its flavor + a transport/freq summary; falls back to
 * "dialed · not reached yet" when attempts exist without a success, and to
 * `none` when the contact carries no reachability rows at all.
 */
export function deriveRecentStatus(contact: Contact): RecentStatus {
  const channels = contact.channels ?? [];
  const endpoints = contact.endpoints ?? [];

  let best: { when: string; ms: number; flavor: Flavor; summary: string | null } | null = null;
  const consider = (when: string | null, flavor: Flavor, summary: string | null) => {
    if (!when) return;
    const ms = Date.parse(when);
    if (Number.isNaN(ms)) return;
    if (!best || ms > best.ms) best = { when, ms, flavor, summary };
  };

  for (const ch of channels) {
    consider(ch.last_ok, channelFlavor(ch), channelSummary(ch));
  }
  for (const ep of endpoints) {
    consider(ep.last_ok, endpointFlavor(ep), 'telnet');
  }

  if (best) {
    const b = best as { when: string; flavor: Flavor; summary: string | null };
    return { kind: b.flavor, when: b.when, summary: b.summary };
  }
  if (channels.length > 0 || endpoints.length > 0) return { kind: 'dialed-not-reached' };
  return { kind: 'none' };
}

/** A single RF channel's honest status line: "reached 3 h ago" / "heard …" /
 *  "connected …" when it has a success, else "dialed · not reached yet". No
 *  transport/freq summary — the channel row already shows those. */
export function channelStatusLine(ch: Channel, now: Date = new Date()): string {
  if (ch.last_ok) {
    const verb = channelFlavor(ch);
    const rel = relativeAgo(ch.last_ok, now);
    return rel ? `${verb} ${rel}` : verb;
  }
  return 'dialed · not reached yet';
}

/** A single telnet endpoint's honest status line (same rule as
 *  [`channelStatusLine`]). */
export function endpointStatusLine(ep: Endpoint, now: Date = new Date()): string {
  if (ep.last_ok) {
    const verb = endpointFlavor(ep);
    const rel = relativeAgo(ep.last_ok, now);
    return rel ? `${verb} ${rel}` : verb;
  }
  return 'dialed · not reached yet';
}

/**
 * The one-line label a Recent row / detail shows, rendered from a
 * [`RecentStatus`]. "heard 3 h ago · VARA HF · 7.101 MHz" (incoming),
 * "reached 3 h ago · …" (outgoing), "connected 3 h ago · …"
 * (direction-unknown success), or the honest "dialed · not reached yet".
 * `none` renders empty (the caller omits the line).
 */
export function recentStatusLine(status: RecentStatus, now: Date = new Date()): string {
  switch (status.kind) {
    case 'heard':
    case 'reached':
    case 'connected': {
      const verb = status.kind; // all three are literal claims
      const rel = relativeAgo(status.when, now);
      const base = rel ? `${verb} ${rel}` : verb;
      return status.summary ? `${base} · ${status.summary}` : base;
    }
    case 'dialed-not-reached':
      return 'dialed · not reached yet';
    case 'none':
      return '';
  }
}
