// src/packet/packetStatus.ts
// Pure formatters for the packet transport indicator (ribbon + status bar).
// Returns null when packet is inactive so host components fall back to the
// existing CMS connection labels. `tone` reuses useStatus.ts's StatusTone.
import type { StatusTone, StatusDto } from '../shell/useStatus';

/**
 * Derive the packet indicator state from the LIVE backend status (tuxlink-orj).
 *
 * Replaces the prior hard-coded `listening:false, connected:false` placeholder
 * that the UI used because there was no real feed. `Listening` is a packet-only
 * backend state (CMS never listens), so it maps unambiguously. `Connected` is
 * shared with CMS, so it counts as a *packet* connection only when the transport
 * string is a packet one ("Packet-7"). `active` is true when the operator has
 * the packet panel selected OR there is a live packet state — so an armed Listen
 * shows honestly in the ribbon even while another panel is in view.
 */
export function derivePacketUiState(
  status: StatusDto | null,
  panelSelected: boolean,
  effectiveCall: string,
): PacketUiState {
  const listening = status?.kind === 'Listening';
  const connected = status?.kind === 'Connected' && status.transport.startsWith('Packet');
  return {
    active: panelSelected || listening || connected,
    listening,
    connected,
    effectiveCall,
    linkLabel: '',
  };
}

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
