// Same-window prefill event for station-picker → radio-panel handoff.
//
// RADIO-1: this event only fills an existing modem form. It never invokes a
// connect/transmit command; the operator still clicks the panel's own action.

import type { FavoriteDial, RadioMode } from './types';

export const GATEWAY_PREFILL_EVENT = 'tuxlink:gateway-prefill';

/** The prefill payload: the primary dial plus an optional ordered candidate
 *  list (tuxlink-8fkkk Task B) the panel sends as `qsyCandidates` for the
 *  backend QSY-on-fail walk. `candidates` is the Find-a-Station ranked list for
 *  the used channel's station+mode; absent for a bare favorite/recent prefill. */
export interface GatewayPrefill {
  dial: FavoriteDial;
  candidates?: FavoriteDial[];
}

// Retained prefill for arm-on-demand (tuxlink-s0r1). When Find-a-Station's
// "Use →" opens a modem panel that wasn't mounted yet, the live event below
// fires before that panel's listener registers, so it would be lost. We retain
// the most recent prefill briefly; a panel consumes it on mount if it matches
// its mode. A short TTL keeps a stale prefill from filling an unrelated panel
// the operator opens much later by hand.
const PENDING_TTL_MS = 4000;
let pending: { prefill: GatewayPrefill; atMs: number } | null = null;

export function emitGatewayPrefill(
  dial: FavoriteDial,
  candidates?: FavoriteDial[],
): void {
  if (typeof window === 'undefined') return;
  const prefill: GatewayPrefill = { dial, candidates };
  pending = { prefill, atMs: Date.now() };
  window.dispatchEvent(
    new CustomEvent<GatewayPrefill>(GATEWAY_PREFILL_EVENT, { detail: prefill }),
  );
}

export function listenGatewayPrefill(
  mode: RadioMode,
  onPrefill: (dial: FavoriteDial, candidates?: FavoriteDial[]) => void,
): () => void {
  if (typeof window === 'undefined') return () => {};
  // Consume a fresh prefill emitted just before this panel mounted (arm-on-demand).
  if (
    pending &&
    pending.prefill.dial.mode === mode &&
    Date.now() - pending.atMs <= PENDING_TTL_MS
  ) {
    const { dial, candidates } = pending.prefill;
    pending = null;
    onPrefill(dial, candidates);
  }
  const onEvent = (event: Event) => {
    const prefill = (event as CustomEvent<GatewayPrefill>).detail;
    if (!prefill || !prefill.dial || prefill.dial.mode !== mode) return;
    pending = null; // a live, already-mounted listener handled it
    onPrefill(prefill.dial, prefill.candidates);
  };
  window.addEventListener(GATEWAY_PREFILL_EVENT, onEvent);
  return () => window.removeEventListener(GATEWAY_PREFILL_EVENT, onEvent);
}
