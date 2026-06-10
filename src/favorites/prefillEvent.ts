// Same-window prefill event for station-picker → radio-panel handoff.
//
// RADIO-1: this event only fills an existing modem form. It never invokes a
// connect/transmit command; the operator still clicks the panel's own action.

import type { FavoriteDial, RadioMode } from './types';

export const GATEWAY_PREFILL_EVENT = 'tuxlink:gateway-prefill';

export function emitGatewayPrefill(dial: FavoriteDial): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new CustomEvent<FavoriteDial>(GATEWAY_PREFILL_EVENT, { detail: dial }));
}

export function listenGatewayPrefill(
  mode: RadioMode,
  onPrefill: (dial: FavoriteDial) => void,
): () => void {
  if (typeof window === 'undefined') return () => {};
  const onEvent = (event: Event) => {
    const dial = (event as CustomEvent<FavoriteDial>).detail;
    if (!dial || dial.mode !== mode) return;
    onPrefill(dial);
  };
  window.addEventListener(GATEWAY_PREFILL_EVENT, onEvent);
  return () => window.removeEventListener(GATEWAY_PREFILL_EVENT, onEvent);
}
