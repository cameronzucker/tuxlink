// Same-window prefill event for station-picker → radio-panel handoff.
//
// RADIO-1: this event only fills an existing modem form. It never invokes a
// connect/transmit command; the operator still clicks the panel's own action.

import type { FavoriteDial, RadioMode } from './types';

export const GATEWAY_PREFILL_EVENT = 'tuxlink:gateway-prefill';

// Retained prefill for arm-on-demand (tuxlink-s0r1). When Find-a-Station's
// "Use →" opens a modem panel that wasn't mounted yet, the live event below
// fires before that panel's listener registers, so it would be lost. We retain
// the most recent dial briefly; a panel consumes it on mount if it matches its
// mode. A short TTL keeps a stale dial from prefilling an unrelated panel the
// operator opens much later by hand.
const PENDING_TTL_MS = 4000;
let pending: { dial: FavoriteDial; atMs: number } | null = null;

export function emitGatewayPrefill(dial: FavoriteDial): void {
  if (typeof window === 'undefined') return;
  pending = { dial, atMs: Date.now() };
  window.dispatchEvent(new CustomEvent<FavoriteDial>(GATEWAY_PREFILL_EVENT, { detail: dial }));
}

export function listenGatewayPrefill(
  mode: RadioMode,
  onPrefill: (dial: FavoriteDial) => void,
): () => void {
  if (typeof window === 'undefined') return () => {};
  // Consume a fresh prefill emitted just before this panel mounted (arm-on-demand).
  if (pending && pending.dial.mode === mode && Date.now() - pending.atMs <= PENDING_TTL_MS) {
    const dial = pending.dial;
    pending = null;
    onPrefill(dial);
  }
  const onEvent = (event: Event) => {
    const dial = (event as CustomEvent<FavoriteDial>).detail;
    if (!dial || dial.mode !== mode) return;
    pending = null; // a live, already-mounted listener handled it
    onPrefill(dial);
  };
  window.addEventListener(GATEWAY_PREFILL_EVENT, onEvent);
  return () => window.removeEventListener(GATEWAY_PREFILL_EVENT, onEvent);
}
