// Same-window prefill event for a PEER (P2P) channel/endpoint → radio-panel
// handoff — the peer-selection sibling of `favorites/prefillEvent.ts`'s
// GATEWAY_PREFILL_EVENT (docs/design/mockups/2026-07-11-station-intel-l3/
// station-intel-l3-peer-selected.html, the operator-approved peers↔L3
// reconciliation).
//
// A SEPARATE event rather than reusing GatewayPrefill/FavoriteDial: a peer
// dial isn't shaped like a FavoriteDial (a telnet peer endpoint needs
// host/port, which FavoriteDial has no fields for), and — the more important
// reason — dispatching an event literally named "gateway prefill" for a peer
// would misname what happened for anyone reading the wire log later
// (explicit-referents discipline). The MECHANICS are otherwise identical to
// GatewayPrefill (arm-on-demand pending/TTL, same-window CustomEvent,
// mode-filtered subscription) — this module mirrors that one verbatim rather
// than sharing implementation, so each event stays simple and independently
// auditable, and the gateway path stays completely untouched.
//
// RADIO-1: this event only fills an existing modem form's target/freq/host/
// port fields. It NEVER invokes a connect/transmit command — the operator
// still clicks the panel's own Connect / Start / Send-Receive. THIS IS THE
// CORRECTION over the shipped-then-dropped peers epic's `connectPeerChannel`/
// `connectPeerEndpoint` (`./connectPeer.ts`), which dialed RF directly from
// the finder with the pane closed. Those functions still exist for
// ContactsPanel's own reachability block (a different, non-finder surface);
// the finder never calls them.

import type { RadioMode } from '../favorites/types';

export const PEER_PREFILL_EVENT = 'tuxlink:peer-prefill';

/**
 * The peer prefill payload. `mode` is the RadioMode the target panel filters
 * on (mirrors GatewayPrefill's `dial.mode`). `target` is the peer's callsign
 * — `Channel.target_callsign` for an RF channel dial, or the peer's own
 * callsign for a telnet endpoint dial (fills the Telnet-P2P panel's peer
 * Callsign field). `freqHz` (RF channels) and `host`/`port` (telnet
 * endpoints) are mutually exclusive in practice — a panel reads only the
 * fields relevant to its own transport and ignores the rest. `contactId`
 * carries the `Contact.id` owning the dialed channel/endpoint (mirrors
 * `FavoriteDial.contact_id`'s documented purpose: "links this favorite to a
 * P2P roster entry"), for a panel that wants to thread it into a resulting
 * attempt record.
 */
export interface PeerPrefill {
  mode: RadioMode;
  target: string;
  freqHz?: number;
  /** Digipeater / relay path for a packet channel (`Channel.via`) — the packet
   *  pane's relay chips consume it. Absent for a direct (no-relay) channel. */
  via?: string[];
  host?: string;
  port?: number;
  contactId?: string;
}

// Retained prefill for arm-on-demand (mirrors GATEWAY_PREFILL_EVENT's
// `pending`, `favorites/prefillEvent.ts`): when a peer channel/endpoint click
// opens a modem panel that wasn't mounted yet, this event fires before that
// panel's listener registers, so it would otherwise be lost. A short TTL
// keeps a stale prefill from filling an unrelated panel the operator opens
// much later by hand.
const PENDING_TTL_MS = 4000;
let pending: { prefill: PeerPrefill; atMs: number } | null = null;

export function emitPeerPrefill(prefill: PeerPrefill): void {
  if (typeof window === 'undefined') return;
  pending = { prefill, atMs: Date.now() };
  window.dispatchEvent(new CustomEvent<PeerPrefill>(PEER_PREFILL_EVENT, { detail: prefill }));
}

export function listenPeerPrefill(
  mode: RadioMode,
  onPrefill: (prefill: PeerPrefill) => void,
): () => void {
  if (typeof window === 'undefined') return () => {};
  // Consume a fresh prefill emitted just before this panel mounted (arm-on-demand).
  if (pending && pending.prefill.mode === mode && Date.now() - pending.atMs <= PENDING_TTL_MS) {
    const { prefill } = pending;
    pending = null;
    onPrefill(prefill);
  }
  const onEvent = (event: Event) => {
    const prefill = (event as CustomEvent<PeerPrefill>).detail;
    if (!prefill || prefill.mode !== mode) return;
    pending = null; // a live, already-mounted listener handled it
    onPrefill(prefill);
  };
  window.addEventListener(PEER_PREFILL_EVENT, onEvent);
  return () => window.removeEventListener(PEER_PREFILL_EVENT, onEvent);
}
