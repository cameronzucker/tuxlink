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

/** Ribbon Connection item label + dot tone; null when packet inactive.
 *  Honest by construction: "Listening"/"Connected" are claimed ONLY when the
 *  backend actually reports them (`listening`/`connected`). Selecting the panel
 *  is not a live state, so the default is an honest "not connected" (idle tone),
 *  never a green "Listening" we can't back up. */
export function formatPacketConnection(s: PacketUiState): { label: string; tone: StatusTone } | null {
  if (!s.active) return null;
  if (s.connected) return { label: 'Connected · Packet 1200', tone: 'good' };
  if (s.listening) return { label: 'Listening · Packet 1200', tone: 'good' };
  return { label: 'Packet 1200 · not connected', tone: 'idle' };
}

/** Status-bar label + dot tone; null when packet inactive. Honest default: no
 *  "Listening"/"Connected" claim unless the backend reports it. */
export function formatPacketStatusBar(s: PacketUiState): { label: string; tone: StatusTone } | null {
  if (!s.active) return null;
  if (!s.connected && !s.listening) {
    return { label: 'Packet 1200 · not connected', tone: 'idle' };
  }
  const verb = s.connected ? 'Connected' : 'Listening';
  const parts = ['Packet 1200', `${verb} as ${s.effectiveCall}`];
  if (s.linkLabel) parts.push(s.linkLabel);
  return { label: parts.join(' · '), tone: 'good' };
}
