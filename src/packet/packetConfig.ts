// src/packet/packetConfig.ts
// Pure helpers for the packet connection panel. No I/O, no React — the prime
// unit-test targets (mirrors useStatus.ts's pure-formatter posture).
import type { PacketConfigDto, PacketLinkKind } from './packetTypes';

/** Effective AX.25 display call: `${base}-${ssid}` (e.g. N7CPZ-7). Empty base →
 *  empty string (no dangling dash before identity loads). -0 shown as configured. */
export function effectiveCall(base: string, ssid: number): string {
  if (!base) return '';
  return `${base}-${ssid}`;
}

/** Selectable SSID values: 0..15 inclusive. */
export function ssidOptions(): number[] {
  return Array.from({ length: 16 }, (_, i) => i);
}

/** Immutable: replace the (global sticky) SSID. */
export function withSsid(dto: PacketConfigDto, ssid: number): PacketConfigDto {
  return { ...dto, ssid };
}

/** Immutable: set the default-on listen flag. */
export function withListen(dto: PacketConfigDto, listenDefault: boolean): PacketConfigDto {
  return { ...dto, listenDefault };
}

/** Immutable: change the link kind (TCP ↔ Serial). */
export function withLinkKind(dto: PacketConfigDto, linkKind: PacketLinkKind | null): PacketConfigDto {
  return { ...dto, linkKind };
}

/** Render the digipeater path: source(SSID) → [relays...] → target. Empty target
 *  shows a "(call sign)" placeholder. 0 relays = direct. */
export function pathPreview(base: string, ssid: number, relays: string[], target: string): string {
  const src = effectiveCall(base, ssid);
  const dst = target.trim() || '(call sign)';
  return [src, ...relays.map((r) => r.trim()).filter(Boolean), dst].join(' → ');
}
