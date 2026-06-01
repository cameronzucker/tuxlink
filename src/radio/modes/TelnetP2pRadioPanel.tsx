// src/radio/modes/TelnetP2pRadioPanel.tsx
//
// Telnet P2P right-hand radio panel (tuxlink-0pnb), structurally mirroring
// TelnetRadioPanel.tsx per the 2026-06-01 operator-flagged regression:
// the prior implementation built a one-off control scheme and never wired
// into the connection-status pipeline, so the StatusBar stayed idle and
// operators lost control parity with the CMS panel.
//
// Structural mirror of TelnetRadioPanel:
//   - Same RadioPanel chrome wrapper (mode, state, sub, onClose).
//   - Same radio-panel-sec / radio-panel-input-row / radio-panel-chip
//     class system — no new CSS classes invented.
//   - Same radio-panel-btn-primary (Connect) + radio-panel-btn-bad (Stop)
//     button pair in a radio-panel-act section.
//   - Same SessionLogSection + useSessionLog for live progress.
//   - Same config_read pattern for my_callsign + locator on mount.
//   - Backend calls: telnet_p2p_connect (mirrors cms_connect) +
//     telnet_p2p_abort (mirrors cms_abort).
//   - telnet_p2p_connect emits backend_status:change events at each phase
//     transition, so the StatusBar reflects Connecting / Connected /
//     Disconnected for P2P sessions without polling WinlinkBackend.
//
// Differences from TelnetRadioPanel that ARE warranted:
//   - Peer callsign input (CMS callsign is fixed in config; P2P callsign
//     is per-session and set at dial time).
//   - Peer password Set/Clear (keyring-backed; CMS uses its own auth path).
//   - Host quick-pick chip shows 127.0.0.1 (local Winlink Express default
//     for P2P; CMS quick-picks are remote server names).
//   - No port input exposed: WLE P2P is plaintext 8772 only (no TLS).
//     Surfacing a port field would imply TLS is an option — it is not.
//   - No transport radio group: WLE P2P is plaintext-only per decompile
//     (spec §4.3). Hiding transport choice is honest, not an omission.
//
// Tauri commands used:
//   config_read()                               → { callsign, grid, ... }
//   telnet_p2p_connect({ req: P2pDialReq })     → { sent_count, received_count }
//   telnet_p2p_abort()                          → void (best-effort cancel)
//   p2p_peer_password_status(callsign)          → "Set" | "NotSet"
//   p2p_peer_password_set(callsign, password)   → void
//   p2p_peer_password_clear(callsign)           → void

import { useEffect, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';

export interface TelnetP2pRadioPanelProps {
  onClose: () => void;
}

const DEFAULT_HOST = '127.0.0.1';
const P2P_PORT = 8772; // WLE P2P is plaintext 8772 only — no TLS option.

type PasswordStatus = 'Set' | 'NotSet';

interface DialResult {
  sent_count: number;
  received_count: number;
}

interface ConfigSlice {
  callsign?: string;
  grid?: string;
}

// Quick-pick chips for the peer host input. 127.0.0.1 is the canonical
// "dial into a local Winlink Express instance" target. The operator can type
// any host manually; the chip is a convenience only.
const QUICK_PICKS: { host: string; label: string }[] = [
  { host: '127.0.0.1', label: 'localhost' },
];

export function TelnetP2pRadioPanel({ onClose }: TelnetP2pRadioPanelProps) {
  const [busy, setBusy] = useState(false);
  const [host, setHost] = useState<string>(DEFAULT_HOST);
  const [peerCallsign, setPeerCallsign] = useState<string>('');
  const [myCallsign, setMyCallsign] = useState<string>('');
  const [locator, setLocator] = useState<string>('');
  const [passwordStatus, setPasswordStatus] = useState<PasswordStatus>('NotSet');
  const [result, setResult] = useState<DialResult | null>(null);
  const [connectError, setConnectError] = useState<string | null>(null);
  const { entries: logEntries, clear: clearLog } = useSessionLog();

  // Load my_callsign + locator from config on mount (same pattern as
  // TelnetRadioPanel's host/transport fetch — one call, cancelled on unmount).
  useEffect(() => {
    let cancelled = false;
    invoke<ConfigSlice>('config_read')
      .then((c) => {
        if (cancelled) return;
        if (c.callsign) setMyCallsign(c.callsign);
        if (c.grid) setLocator(c.grid);
      })
      .catch(() => {
        // Pre-wizard / config absent — my_callsign + locator stay empty;
        // the backend will reject with a meaningful error if needed.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Refresh password status whenever the peer callsign changes (debounce not
  // needed — the backend lookup is a fast keyring read).
  useEffect(() => {
    if (!peerCallsign) {
      setPasswordStatus('NotSet');
      return;
    }
    let cancelled = false;
    invoke<PasswordStatus>('p2p_peer_password_status', { callsign: peerCallsign })
      .then((s) => {
        if (!cancelled) setPasswordStatus(s);
      })
      .catch(() => {
        if (!cancelled) setPasswordStatus('NotSet');
      });
    return () => {
      cancelled = true;
    };
  }, [peerCallsign]);

  const commitHost = () => {
    const trimmed = host.trim();
    if (trimmed && trimmed !== host) setHost(trimmed);
  };

  const onHostKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
  };

  const pickHost = (picked: string) => {
    setHost(picked);
  };

  const onPeerCallsignChange = (raw: string) => {
    setPeerCallsign(raw.toUpperCase());
  };

  const handleSetPassword = async () => {
    // v0.1: use window.prompt — secret never stored in component state.
    const pw = window.prompt(`Enter password for ${peerCallsign} (blank = cancel):`);
    if (pw === null || pw === '') return;
    try {
      await invoke('p2p_peer_password_set', { callsign: peerCallsign, password: pw });
      setPasswordStatus('Set');
    } catch {
      // Backend errors surface in the session log.
    }
  };

  const handleClearPassword = async () => {
    try {
      await invoke('p2p_peer_password_clear', { callsign: peerCallsign });
      setPasswordStatus('NotSet');
    } catch {
      // Session log carries any backend error.
    }
  };

  // Connect — mirrors TelnetRadioPanel's `start()` pattern.
  // telnet_p2p_connect drives session-log events + status transitions.
  const start = async () => {
    if (busy) return;
    setBusy(true);
    setResult(null);
    setConnectError(null);
    try {
      const res = await invoke<DialResult>('telnet_p2p_connect', {
        req: {
          host: host.trim() || DEFAULT_HOST,
          port: P2P_PORT,
          peer_callsign: peerCallsign,
          my_callsign: myCallsign,
          locator,
        },
      });
      setResult(res);
    } catch (e) {
      setConnectError(String(e));
    } finally {
      setBusy(false);
    }
  };

  // Stop — mirrors TelnetRadioPanel's `stop()` pattern.
  const stop = () => {
    void invoke('telnet_p2p_abort').catch(() => {});
  };

  const subText = peerCallsign
    ? `${peerCallsign} @ ${host.trim() || DEFAULT_HOST}:${P2P_PORT}`
    : `${host.trim() || DEFAULT_HOST}:${P2P_PORT}`;

  return (
    <RadioPanel
      mode={{ kind: 'telnet', intent: 'p2p' }}
      state={busy ? 'connecting' : 'disconnected'}
      sub={subText}
      onClose={onClose}
    >
      {/* Peer Station section — mirrors TelnetRadioPanel's Server section */}
      <section className="radio-panel-sec">
        <h5>Peer Station</h5>
        <label className="radio-panel-input-row">
          <span>Host</span>
          <input
            type="text"
            className="radio-panel-input"
            data-testid="p2p-host-input"
            value={host}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="127.0.0.1"
            onChange={(e) => setHost(e.target.value)}
            onBlur={commitHost}
            onKeyDown={onHostKey}
          />
        </label>
        {/* Port is not an editable input: WLE P2P is plaintext 8772 only.
            Exposing a port field would imply TLS is an option — it is not.
            The port is shown in the sub-header line for operator awareness. */}
        <label className="radio-panel-input-row">
          <span>Callsign</span>
          <input
            type="text"
            className="radio-panel-input"
            data-testid="p2p-peer-callsign-input"
            value={peerCallsign}
            spellCheck={false}
            autoCapitalize="characters"
            autoCorrect="off"
            placeholder="W7AUX"
            onChange={(e) => onPeerCallsignChange(e.target.value)}
          />
        </label>
        <div className="radio-panel-chip-row">
          {QUICK_PICKS.map((q) => (
            <button
              key={q.host}
              type="button"
              className="radio-panel-chip"
              data-testid={`p2p-pick-${q.host}`}
              onClick={() => pickHost(q.host)}
            >
              {q.label}
            </button>
          ))}
        </div>
      </section>

      {/* Transport note — mirrors TelnetRadioPanel's Transport section.
          WLE P2P is plaintext-only (no TLS). The section is present so the
          operator knows this is not an oversight, and to maintain visual
          parity with the CMS panel's section count. */}
      <section className="radio-panel-sec">
        <h5>Transport</h5>
        <p className="radio-panel-radio-help">
          Plaintext · port {P2P_PORT}. WLE P2P does not support TLS — the
          transport is fixed.
        </p>
      </section>

      {/* Peer Password section — P2P-specific; no CMS equivalent.
          The per-peer password is keyring-backed and never shown in the UI.
          Set/Clear chips mirror the chip styling of the Host quick-picks. */}
      <section className="radio-panel-sec">
        <h5>Peer Password</h5>
        <div className="radio-panel-chip-row">
          <span
            className="radio-panel-sub"
            data-testid="p2p-password-status"
          >
            {passwordStatus === 'Set' ? 'Set' : 'Not set'}
          </span>
          <button
            type="button"
            className="radio-panel-chip"
            data-testid="p2p-password-set-btn"
            disabled={!peerCallsign}
            onClick={handleSetPassword}
          >
            Set…
          </button>
          <button
            type="button"
            className="radio-panel-chip"
            data-testid="p2p-password-clear-btn"
            disabled={!peerCallsign || passwordStatus !== 'Set'}
            onClick={handleClearPassword}
          >
            Clear
          </button>
        </div>
        <p className="radio-panel-radio-help">
          Optional. Sent only if the peer challenges for a password.
          Stored in OS keyring — never in config or logs.
        </p>
      </section>

      <SessionLogSection entries={logEntries} onClear={clearLog} />

      {/* Result / error feedback — displayed below the session log so the
          operator sees the outcome inline without a separate modal. */}
      {result && (
        <section className="radio-panel-sec">
          <p className="radio-panel-radio-help" data-testid="p2p-result">
            Sent {result.sent_count}, received {result.received_count}.
          </p>
        </section>
      )}
      {connectError && (
        <section className="radio-panel-sec">
          <p className="radio-panel-radio-help" data-testid="p2p-error"
             style={{ color: 'var(--error, #f87171)' }}>
            {connectError}
          </p>
        </section>
      )}

      {/* Actions — mirrors TelnetRadioPanel's Start/Stop section exactly:
          primary button (Connect / Connecting…) + bad button (Stop). */}
      <section className="radio-panel-sec radio-panel-act">
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-primary"
          data-testid="p2p-connect-btn"
          disabled={busy}
          onClick={start}
        >
          {busy ? 'Connecting…' : 'Connect'}
        </button>
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-bad"
          data-testid="p2p-stop-btn"
          onClick={stop}
        >
          Stop
        </button>
      </section>
    </RadioPanel>
  );
}
