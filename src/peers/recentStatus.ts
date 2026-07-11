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
// Truth rests entirely on `last_ok` (success-only, T-F Part 0): a failed dial
// bumps `last_seen` but never `last_ok`, so nothing here can mislabel a failure
// as a success.

import type { Channel, ChannelTransport, Contact, Endpoint, Origin } from '../contacts/types';
import { relativeAgo } from '../favorites/record-format';

/** Whether a success was an INCOMING ("heard") or OUTGOING ("reached") event. */
type Flavor = 'heard' | 'reached';

/** The honest status of a reachability-bearing contact row. */
export type RecentStatus =
  | { kind: 'heard'; when: string; summary: string | null }
  | { kind: 'reached'; when: string; summary: string | null }
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

/** Fall back to the record's origin when a success carries no direction of its
 *  own (a forward-compat `unknown` direction, or a telnet endpoint). */
function flavorFromOrigin(origin: Origin | undefined): Flavor {
  return origin === 'incoming' ? 'heard' : 'reached';
}

function channelFlavor(ch: Channel, origin: Origin | undefined): Flavor {
  if (ch.direction === 'incoming') return 'heard';
  if (ch.direction === 'outgoing') return 'reached';
  return flavorFromOrigin(origin);
}

function endpointFlavor(ep: Endpoint, origin: Origin | undefined): Flavor {
  if (ep.provenance === 'observed-incoming') return 'heard';
  if (ep.provenance === 'operator') return 'reached';
  return flavorFromOrigin(origin);
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
    consider(ch.last_ok, channelFlavor(ch, contact.origin), channelSummary(ch));
  }
  for (const ep of endpoints) {
    consider(ep.last_ok, endpointFlavor(ep, contact.origin), 'telnet');
  }

  if (best) {
    const b = best as { when: string; flavor: Flavor; summary: string | null };
    return { kind: b.flavor, when: b.when, summary: b.summary };
  }
  if (channels.length > 0 || endpoints.length > 0) return { kind: 'dialed-not-reached' };
  return { kind: 'none' };
}

/** A single RF channel's honest status line: "reached 3 h ago" / "heard …"
 *  when it has a success, else "dialed · not reached yet". No transport/freq
 *  summary — the channel row already shows those. */
export function channelStatusLine(ch: Channel, origin?: Origin, now: Date = new Date()): string {
  if (ch.last_ok) {
    const verb = channelFlavor(ch, origin);
    const rel = relativeAgo(ch.last_ok, now);
    return rel ? `${verb} ${rel}` : verb;
  }
  return 'dialed · not reached yet';
}

/** A single telnet endpoint's honest status line (same rule as
 *  [`channelStatusLine`]). */
export function endpointStatusLine(ep: Endpoint, origin?: Origin, now: Date = new Date()): string {
  if (ep.last_ok) {
    const verb = endpointFlavor(ep, origin);
    const rel = relativeAgo(ep.last_ok, now);
    return rel ? `${verb} ${rel}` : verb;
  }
  return 'dialed · not reached yet';
}

/**
 * The one-line label a Recent row / detail shows, rendered from a
 * [`RecentStatus`]. "heard 3 h ago · VARA HF · 7.101 MHz" (incoming),
 * "reached 3 h ago · …" (outgoing), or the honest "dialed · not reached yet".
 * `none` renders empty (the caller omits the line).
 */
export function recentStatusLine(status: RecentStatus, now: Date = new Date()): string {
  switch (status.kind) {
    case 'heard':
    case 'reached': {
      const verb = status.kind; // 'heard' | 'reached' — both are literal claims
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
