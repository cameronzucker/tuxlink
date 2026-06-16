// src/connections/connectDispatch.ts
//
// tuxlink-vu97: shared dispatcher that lets the dashboard-ribbon Connect button
// fire the LAST-SELECTED transport's full send/receive (connect + mail
// exchange) in one click, with the right-hand radio pane CLOSED — for
// ARDOP / VARA / packet, not just Telnet-CMS.
//
// The per-mode panels (ArdopRadioPanel / VaraRadioPanel / PacketRadioPanel)
// remain the canonical interactive surface; this module replicates their EXACT
// backend invoke shapes (same commands, same args, same intent/transportKind)
// so a ribbon-driven connect is indistinguishable on the wire from the panel's
// Start → Send/Receive sequence. No consent token / modal — the RF panels fire
// directly (memory: no-tuxlink-added-safeguards), and the operator's click on
// the ribbon Connect button is the Part 97 consent (RADIO-1 governs the click,
// not a UI gate).
//
// Target persistence: each RF panel writes the operator-configured target to
// localStorage under `tuxlink.lastTarget.<protocol>` whenever the target input
// changes. connectFor reads that key back so the ribbon knows WHERE to dial.
// Telnet-CMS carries no target (cms_connect takes no args), so no key is read.

import { invoke } from '@tauri-apps/api/core';
import type { ConnectionKey, ProtocolId, SessionTypeId } from './sessionTypes';

/** localStorage key for a mode's last-configured dial target. The RF panels
 *  write it on target-input change; connectFor reads it. */
export function lastTargetKey(protocol: ProtocolId): string {
  return `tuxlink.lastTarget.${protocol}`;
}

/** Read the persisted dial target for a protocol; '' when none / unavailable. */
export function readLastTarget(protocol: ProtocolId): string {
  try {
    return (localStorage.getItem(lastTargetKey(protocol)) ?? '').trim();
  } catch {
    return '';
  }
}

/** Persist a protocol's dial target. Empty/whitespace clears the key so a
 *  cleared input doesn't leave a stale target the ribbon would dial. */
export function writeLastTarget(protocol: ProtocolId, target: string): void {
  try {
    const trimmed = target.trim();
    if (trimmed === '') localStorage.removeItem(lastTargetKey(protocol));
    else localStorage.setItem(lastTargetKey(protocol), trimmed);
  } catch {
    /* localStorage unavailable — in-memory panel state still drives the panel */
  }
}

/** Thrown when a ribbon connect is attempted for an RF mode with no persisted
 *  target. AppShell surfaces the message via its existing error/log path. */
export class MissingTargetError extends Error {
  constructor(protocol: ProtocolId) {
    super(
      `No saved target for this mode — open the ${protocol} panel, enter a ` +
        `gateway/peer call sign, then use Connect.`,
    );
    this.name = 'MissingTargetError';
  }
}

/** The intent string the panels thread into their invokes IS the ConnectionKey
 *  sessionType (cms / p2p / radio-only / post-office / network-po). The RF
 *  panels' RadioPanelMode.intent is a subset of those, set from the same
 *  sidebar selection. */
function intentOf(key: ConnectionKey): SessionTypeId {
  return key.sessionType;
}

/**
 * Fire the last-selected mode's FULL connect + exchange.
 *
 * Per-mode replication (verbatim invoke shapes from the panels):
 *  - Telnet-CMS  → cms_connect (no args).
 *  - ARDOP       → modem_ardop_connect{target}, THEN
 *                  modem_ardop_b2f_exchange{target, intent, transportKind:'ardop'}.
 *  - VARA HF/FM  → vara_open_session{intent, transportKind}, THEN
 *                  modem_vara_b2f_exchange{target, intent, transportKind}.
 *  - packet      → packet_connect{call, path} (single blocking connect→B2F).
 *
 * RF modes require a persisted target; a missing one throws MissingTargetError
 * BEFORE any backend invoke (no half-open transport on the missing-target path).
 */
export async function connectFor(key: ConnectionKey): Promise<void> {
  const { sessionType, protocol } = key;
  const intent = intentOf(key);

  // Telnet-CMS — unchanged: cms_connect takes no args, no target needed.
  if (sessionType === 'cms' && protocol === 'telnet') {
    await invoke('cms_connect');
    return;
  }

  if (protocol === 'ardop-hf') {
    const target = readLastTarget('ardop-hf');
    if (!target) throw new MissingTargetError('ardop-hf');
    // Connect (spawn ardopcf + dial the ARQ link), then run the B2F exchange.
    // The panel splits these across two operator clicks because the link takes
    // time; the ribbon one-click awaits the connect, then exchanges with the
    // SAME target it dialed (panel uses status.peer once connected — identical
    // callsign). transportKind:'ardop' mirrors ArdopRadioPanel.onSendReceiveClick.
    await invoke('modem_ardop_connect', { target });
    await invoke('modem_ardop_b2f_exchange', {
      target,
      intent,
      transportKind: 'ardop',
    });
    return;
  }

  if (protocol === 'vara-hf' || protocol === 'vara-fm') {
    const target = readLastTarget(protocol);
    if (!target) throw new MissingTargetError(protocol);
    // Open the TCP transport (no transmit), then the SINGLE blocking
    // connect→B2F→disconnect exchange. transportKind is the panel's mode.kind
    // ('vara-hf' / 'vara-fm') — mirrors VaraRadioPanel.openSession + onSendReceive.
    await invoke('vara_open_session', { intent, transportKind: protocol });
    await invoke('modem_vara_b2f_exchange', {
      target,
      intent,
      transportKind: protocol,
    });
    return;
  }

  if (protocol === 'packet') {
    const target = readLastTarget('packet');
    if (!target) throw new MissingTargetError('packet');
    // packet_connect is a single blocking connect→B2F. The panel also carries a
    // 0–2 relay path; the ribbon dials a direct path (no relays) since relays
    // are panel-local transient state, not a persisted per-target attribute.
    // Mirrors PacketRadioPanel.onConnect's invoke shape (path defaults to []).
    await invoke('packet_connect', { call: target, path: [] });
    return;
  }

  // Built-but-unhandled (e.g. a non-RF telnet intent reaching the ribbon).
  // Fall back to the CMS exchange shape only for telnet; otherwise refuse
  // rather than silently dial the wrong transport.
  if (protocol === 'telnet') {
    await invoke('cms_connect');
    return;
  }

  throw new Error(`Connect not supported for ${sessionType}/${protocol}`);
}

/**
 * Abort the last-selected mode's in-flight connect/exchange. Dispatches the
 * per-mode abort the panels use:
 *  - Telnet-CMS  → cms_abort
 *  - ARDOP       → modem_ardop_disconnect
 *  - VARA HF/FM  → vara_close_session
 *  - packet      → cms_abort (the shared session-abort the packet panel's
 *                  Listen "Stop" uses to unwind a blocked answer/connect).
 */
export async function abortFor(key: ConnectionKey): Promise<void> {
  const { protocol } = key;
  if (protocol === 'ardop-hf') {
    await invoke('modem_ardop_disconnect');
    return;
  }
  if (protocol === 'vara-hf' || protocol === 'vara-fm') {
    await invoke('vara_close_session');
    return;
  }
  // Telnet-CMS and packet both unwind through the shared session abort.
  await invoke('cms_abort');
}
