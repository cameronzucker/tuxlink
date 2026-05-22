// src/packet/packetStatus.ts
// Pure formatters for the packet transport indicator (ribbon + status bar).
// Returns null when packet is inactive so host components fall back to the
// existing CMS connection labels. `tone` reuses useStatus.ts's StatusTone.
import type { StatusTone } from '../shell/useStatus';

export interface PacketUiState {
  /** Packet selected/configured as the active transport. */
  active: boolean;
  /** Listening (armed to answer) when idle. */
  listening: boolean;
  /** A connect/exchange is in progress or established. */
  connected: boolean;
  /** Effective AX.25 call, e.g. N7CPZ-7. */
  effectiveCall: string;
  /** Link label, e.g. "KISS-TCP Dire Wolf" or "" when unknown. */
  linkLabel: string;
}

/** Ribbon Connection item label + dot tone; null when packet inactive. */
export function formatPacketConnection(s: PacketUiState): { label: string; tone: StatusTone } | null {
  if (!s.active) return null;
  const verb = s.connected ? 'Connected' : 'Listening';
  return { label: `${verb} · Packet 1200`, tone: 'good' };
}

/** Status-bar label + dot tone; null when packet inactive. */
export function formatPacketStatusBar(s: PacketUiState): { label: string; tone: StatusTone } | null {
  if (!s.active) return null;
  const verb = s.connected ? 'Connected' : 'Listening';
  const parts = ['Packet 1200', `${verb} as ${s.effectiveCall}`];
  if (s.linkLabel) parts.push(s.linkLabel);
  return { label: parts.join(' · '), tone: 'good' };
}
