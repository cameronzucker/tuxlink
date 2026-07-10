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
import type { FavoriteDial, RadioMode } from '../favorites/types';
import { tsLocal } from '../favorites/ts-local';

/**
 * tuxlink-ypz3 (3b): record a ribbon Connect's empirical outcome into the per-mode
 * Recent list, mirroring the in-panel `recordAttempt` (useFavorites) so a
 * ribbon-driven dial appears in the mode's Recent tab exactly like an in-panel
 * dial. Without this the status-bar Connect — the PRIMARY connect surface since
 * vu97 (pane closed) — left Recent permanently empty.
 *
 * - RF modes only. Telnet-CMS has NO Recent surface (FavoritesTabs.isManualOnly
 *   ('telnet')) and connectFor carries no host/transport to build its dial, so
 *   the telnet branch records nothing — parity with the hidden surface.
 * - Fire-and-forget + swallow errors (Cross-cutting §1): a recording failure must
 *   never mask or delay the connect outcome.
 * - The ribbon runs with the pane CLOSED (vu97), so there is no live QueryClient
 *   to invalidate; the recents query refetches when the operator next opens the
 *   mode's panel (useFavorites mounts fresh), which is exactly the pane-closed
 *   workflow vu97 designed for.
 * - One outcome per click: `reached` iff the full per-mode connect+exchange
 *   sequence resolves; `failed` iff it throws at an ON-AIR step. Pre-air bails
 *   (missing target — guarded before any invoke; VARA transport-open failure)
 *   record nothing, matching the panels' honest-outcome rule.
 * - [R5-7] P2P sessions are NOT recorded here — the backend peer recorder is
 *   authoritative for P2P recents, bridged into this same favorites/Recents log
 *   via the ONE `bridge_to_favorites` writer (`peers/recorder.rs`). Recording
 *   here too would double-count a P2P attempt, so `connectFor` skips this call
 *   entirely for `sessionType === 'p2p'` (see the `isP2p` guard below).
 */
function recordRibbonAttempt(
  mode: RadioMode,
  gateway: string,
  outcome: 'reached' | 'failed',
): void {
  const dial: FavoriteDial = { mode, gateway };
  // Rust `ts_local: String` → Tauri camelCases the wire key to `tsLocal`
  // (matches useFavorites.recordAttempt).
  void invoke('favorite_record_attempt', { dial, outcome, tsLocal: tsLocal() }).catch(
    () => {},
  );
}

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
  // [R5-7] the backend peer-recorder is authoritative for p2p recents —
  // recording here too would double-count the attempt (bridge_to_favorites is
  // the sole writer for a P2P outcome).
  const isP2p = sessionType === 'p2p';

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
    //
    // Honest-outcome recording (tuxlink-ypz3 3b, Codex P2): modem_ardop_connect
    // can reject PRE-AIR — missing identity/backend, unconfigured audio devices,
    // a busy-channel guard, or ardopcf spawn/init failure — before any RF dial.
    // The ARDOP panel records NOTHING for those Start failures (it records
    // `reached` only on the connected-* status transition and `failed` only in
    // the later B2F catch), so the ribbon must not either: otherwise a
    // saved-target Connect with no audio config would pollute Recent with a
    // bogus `failed`. A connect RESOLVE means the link reached connected-* — an
    // honest on-air `reached`; a B2F throw after that is an honest `failed`
    // (reached-at-link-up + failed-at-exchange are distinct empirical facts, as
    // ArdopRadioPanel records them).
    await invoke('modem_ardop_connect', { target });
    if (!isP2p) recordRibbonAttempt('ardop-hf', target, 'reached');
    try {
      await invoke('modem_ardop_b2f_exchange', {
        target,
        intent,
        transportKind: 'ardop',
      });
    } catch (e) {
      if (!isP2p) recordRibbonAttempt('ardop-hf', target, 'failed');
      throw e;
    }
    return;
  }

  if (protocol === 'vara-hf' || protocol === 'vara-fm') {
    const target = readLastTarget(protocol);
    if (!target) throw new MissingTargetError(protocol);
    // Open the TCP transport (no transmit), then the SINGLE blocking
    // connect→B2F→disconnect exchange. transportKind is the panel's mode.kind
    // ('vara-hf' / 'vara-fm') — mirrors VaraRadioPanel.openSession + onSendReceive.
    // vara_open_session installs the TCP transport — PRE-AIR. A failure here
    // never transmitted, so it is NOT recorded (it propagates as-is). Only the
    // on-air exchange records an outcome (tuxlink-ypz3 3b).
    await invoke('vara_open_session', { intent, transportKind: protocol });
    try {
      await invoke('modem_vara_b2f_exchange', {
        target,
        intent,
        transportKind: protocol,
      });
    } catch (e) {
      // A "session not open" bail (transport vanished between open and exchange)
      // never went on-air — skip it, matching VaraRadioPanel.onSendReceive's
      // pre-air exclusion. Any other throw is an honest on-air `failed`.
      if (!/session not open/i.test(String(e)) && !isP2p) {
        recordRibbonAttempt(protocol, target, 'failed');
      }
      throw e;
    }
    if (!isP2p) recordRibbonAttempt(protocol, target, 'reached');
    return;
  }

  if (protocol === 'packet') {
    const target = readLastTarget('packet');
    if (!target) throw new MissingTargetError('packet');
    // packet_connect is a single blocking connect→B2F. The panel also carries a
    // 0–2 relay path; the ribbon dials a direct path (no relays) since relays
    // are panel-local transient state, not a persisted per-target attribute.
    // Mirrors PacketRadioPanel.onConnect's invoke shape (path defaults to []).
    // packet_connect is a single blocking connect→B2F: resolve = honest reach,
    // reject = honest fail (tuxlink-ypz3 3b).
    try {
      await invoke('packet_connect', { call: target, path: [] });
    } catch (e) {
      if (!isP2p) recordRibbonAttempt('packet', target, 'failed');
      throw e;
    }
    if (!isP2p) recordRibbonAttempt('packet', target, 'reached');
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
