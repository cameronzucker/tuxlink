// src/aprs/AprsConnectStrip.tsx
//
// The APRS connect surface — a COMPACT status-strip control for the APRS dock
// header (the dead header space above the chat). Connection does NOT live in the
// chat panel (settled design): the operator picks a transport + radio and
// connects/disconnects from here.
//
// Compact row, always visible:
//   📡 APRS · <radio-label | "no link">  ● <state>  [Connect|Disconnect]  [⚙]
//
//   state ∈ { "not listening" (off), "connecting…", "listening" }
//     - `listening` is BACKEND TRUTH, injected as a prop (flips on the
//       aprs-listening:change event the parent owns). The strip NEVER
//       optimistically flips it.
//     - `connecting` is local: true while the injected onConnect promise is in
//       flight. A reject surfaces inline (role="alert") and clears connecting
//       WITHOUT flipping to listening.
//
// The ⚙ caret discloses an inline ModemLinkSection (TCP / USB / BT / UV-Pro) to
// pick transport+radio. It AUTO-EXPANDS when no link is configured (fresh
// install) so the operator immediately sees the picker.
//
// PRESENTATIONAL: the strip injects `listening`, `linkKind`, `radioLabel`, and
// three composed callbacks (`onConnect`, `onDisconnect`, `onLinkChange`). The
// PARENT (AppShell) composes the transport-specific connect sequence — for
// UvproNative it does uvpro.connect() THEN aprs_listen_start; for KISS it just
// invokes aprs_listen_start. The strip is unit-testable with plain mocks, no
// live backend.

import { useState } from 'react';
import { ModemLinkSection } from '../radio/sections/ModemLinkSection';
import type { ModemLinkFields } from '../radio/sections/ModemLinkSection';
import type { PacketLinkKind } from '../packet/packetTypes';
import './AprsConnectStrip.css';

export interface AprsConnectStripProps {
  /** Whether the backend listener is armed (BACKEND TRUTH — flips on the
   *  aprs-listening:change event the parent subscribes to). Drives the
   *  listening/not-listening state + the Connect⇄Disconnect button. */
  listening: boolean;
  /** Current persisted link kind, or null when no link is configured (fresh
   *  install). null auto-expands the setup picker. */
  linkKind: PacketLinkKind | null;
  /** Human-readable radio/device label for the configured link (e.g.
   *  "127.0.0.1:8001", "/dev/ttyUSB0", a BT MAC), or null when no link. */
  radioLabel: string | null;
  /** Offer the native UV-Pro segment in the picker (VHF/packet context). */
  allowUvproNative?: boolean;
  /** Persisted address fields, threaded into ModemLinkSection so the picker
   *  seeds from the SAVED link instead of blanks. Without these, opening setup
   *  on a configured link and tapping a segment emits a null address (e.g.
   *  btMac: null), corrupting the live link (tuxlink-hoi1 B2). */
  tcpHost?: string;
  tcpPort?: number;
  serialDevice?: string;
  serialBaud?: number;
  btMac?: string;
  /** Composed connect sequence. For UvproNative the parent does
   *  uvpro.connect() then aprs_listen_start; for KISS just aprs_listen_start.
   *  Reject ⇒ inline error, no optimistic listening flip. */
  onConnect: () => Promise<void>;
  /** Composed disconnect sequence (aprs_listen_stop; + uvpro.disconnect() for
   *  the native path). */
  onDisconnect: () => Promise<void>;
  /** Persist the picked transport+radio (parent merges + persists via
   *  usePacketConfig.setLink). */
  onLinkChange: (fields: ModemLinkFields) => void;
}

/** Segment props for ModemLinkSection require a concrete kind; default a null
 *  (unconfigured) link to the USB segment so the picker has a sensible start. */
function pickerKind(
  linkKind: PacketLinkKind | null,
): 'Tcp' | 'Serial' | 'Bluetooth' | 'UvproNative' {
  if (linkKind === 'Tcp') return 'Tcp';
  if (linkKind === 'Bluetooth') return 'Bluetooth';
  if (linkKind === 'UvproNative') return 'UvproNative';
  return 'Serial';
}

export function AprsConnectStrip({
  listening,
  linkKind,
  radioLabel,
  allowUvproNative = false,
  tcpHost,
  tcpPort,
  serialDevice,
  serialBaud,
  btMac,
  onConnect,
  onDisconnect,
  onLinkChange,
}: AprsConnectStripProps) {
  const noLink = linkKind == null;
  // Auto-expand when no link is configured (fresh install). Operator-toggled
  // afterward via the ⚙ caret.
  const [setupOpen, setSetupOpen] = useState(noLink);
  const [connecting, setConnecting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const state: 'off' | 'connecting' | 'listening' = connecting
    ? 'connecting'
    : listening
      ? 'listening'
      : 'off';
  const stateLabel =
    state === 'listening' ? 'Listening' : state === 'connecting' ? 'Connecting…' : 'Not listening';

  const handleConnect = async () => {
    if (connecting || busy) return;
    setError(null);
    setConnecting(true);
    try {
      await onConnect();
    } catch (err) {
      // Backend is truth: surface the failure inline, never optimistically flip
      // to listening.
      setError(err instanceof Error ? err.message : 'Could not connect — check the radio link.');
    } finally {
      setConnecting(false);
    }
  };

  const handleDisconnect = async () => {
    if (busy) return;
    setError(null);
    setBusy(true);
    try {
      await onDisconnect();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not disconnect.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="aprs-connect-strip" data-testid="aprs-connect-strip" aria-label="APRS connection">
      <div className="aprs-connect-row">
        <span className="aprs-connect-id">
          {/* tuxlink-rypw #4a: the "APRS" label was the third stacked "APRS" in the
              dock header (the tab "APRS Chat" + the panel "APRS Channel" already
              name it), so it's dropped — the radio status is the useful info here. */}
          <span className="aprs-connect-radio" data-testid="aprs-connect-radio">
            {radioLabel ?? 'no link'}
          </span>
        </span>

        <span
          className={`aprs-connect-state aprs-connect-state-${state}`}
          data-testid="aprs-connect-state"
          data-state={state}
        >
          <span className="aprs-connect-dot" aria-hidden="true" />
          {stateLabel}
        </span>

        {listening ? (
          <button
            type="button"
            className="aprs-connect-btn aprs-connect-btn-ghost"
            data-testid="aprs-disconnect-btn"
            disabled={busy}
            onClick={handleDisconnect}
          >
            Disconnect
          </button>
        ) : (
          <button
            type="button"
            className="aprs-connect-btn"
            data-testid="aprs-connect-btn"
            disabled={connecting || noLink}
            onClick={handleConnect}
          >
            {connecting ? 'Connecting…' : 'Connect'}
          </button>
        )}

        <button
          type="button"
          className={`aprs-connect-caret ${setupOpen ? 'aprs-connect-caret-open' : ''}`}
          data-testid="aprs-connect-setup-toggle"
          aria-expanded={setupOpen && !listening}
          aria-label="Transport and radio setup"
          // No link edits while listening: changing the transport/radio under a
          // live listener would leave the engine on the OLD link (and could
          // orphan a UV-Pro session). Disconnect first to re-pick (Codex adrev
          // 2026-06-14 P1).
          disabled={listening}
          title={listening ? 'Disconnect to change transport or radio' : undefined}
          onClick={() => setSetupOpen((v) => !v)}
        >
          ⚙
        </button>
      </div>

      {setupOpen && !listening && (
        <div className="aprs-connect-setup" data-testid="aprs-connect-setup">
          <ModemLinkSection
            kind={pickerKind(linkKind)}
            host={tcpHost}
            port={tcpPort}
            serialDevice={serialDevice}
            serialBaud={serialBaud}
            btMac={btMac}
            allowUvproNative={allowUvproNative}
            onChange={onLinkChange}
          />
        </div>
      )}

      {error && (
        <p className="aprs-connect-error" data-testid="aprs-connect-error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
