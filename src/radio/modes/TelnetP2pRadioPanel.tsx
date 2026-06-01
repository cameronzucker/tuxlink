// src/radio/modes/TelnetP2pRadioPanel.tsx
//
// Telnet P2P right-panel implementation per spec §4.5 and the right-panel
// architecture established by TelnetRadioPanel (P2), PacketRadioPanel (P3),
// and ArdopRadioPanel (P4).
//
// Path decision (Task 6): path (a) — matches the established right-panel
// pattern where the reading pane falls back to MessageView and all
// connection controls live in the right-hand radio panel. The plan's
// `src/connections/TelnetP2pPanel.tsx` location pre-dated discovery that
// every other built mode uses this right-panel architecture; the right-panel
// path gives the operator a consistent UX surface for all connection types.
//
// Controls (Dial mode only — PR 1 scope):
//   - Peer host input (default 127.0.0.1)
//   - Port input (default 8772, WLE parity — no TLS for P2P)
//   - Peer callsign input (uppercased on input)
//   - Password status + Set/Clear buttons (secret never displayed)
//   - Connect button → telnet_p2p_dial Tauri command
//   - Result display: "Sent N, received M." on success; error string on fail
//   - Session log (shared surface via useSessionLog)
//
// my_callsign + locator sourced from config_read (same as TelnetRadioPanel's
// host/transport config — one fetch on mount, cancelled on unmount).
//
// Tauri commands used:
//   config_read()                               → { callsign, grid, ... }
//   telnet_p2p_dial({ req: P2pDialReq })        → { sent_count, received_count }
//   p2p_peer_password_status(callsign)           → "Set" | "NotSet"
//   p2p_peer_password_set(callsign, password)    → void
//   p2p_peer_password_clear(callsign)            → void

import { useEffect, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import './TelnetP2pRadioPanel.css';

export interface TelnetP2pRadioPanelProps {
  onClose: () => void;
}

const DEFAULT_HOST = '127.0.0.1';
const DEFAULT_PORT = 8772;

type PasswordStatus = 'Set' | 'NotSet';

interface DialResult {
  sent_count: number;
  received_count: number;
}

interface ConfigSlice {
  callsign?: string;
  grid?: string;
}

export function TelnetP2pRadioPanel({ onClose }: TelnetP2pRadioPanelProps) {
  const [busy, setBusy] = useState(false);
  const [host, setHost] = useState<string>(DEFAULT_HOST);
  const [port, setPort] = useState<number>(DEFAULT_PORT);
  const [peerCallsign, setPeerCallsign] = useState<string>('');
  const [myCallsign, setMyCallsign] = useState<string>('');
  const [locator, setLocator] = useState<string>('');
  const [passwordStatus, setPasswordStatus] = useState<PasswordStatus>('NotSet');
  const [result, setResult] = useState<DialResult | null>(null);
  const [dialError, setDialError] = useState<string | null>(null);
  const { entries: logEntries, clear: clearLog } = useSessionLog();

  // Load my_callsign + locator from config on mount.
  useEffect(() => {
    let cancelled = false;
    invoke<ConfigSlice>('config_read')
      .then((c) => {
        if (cancelled) return;
        if (c.callsign) setMyCallsign(c.callsign);
        if (c.grid) setLocator(c.grid);
      })
      .catch(() => {
        // Pre-wizard / config absent — keep defaults.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Refresh password status whenever the peer callsign changes.
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

  const onPeerCallsignChange = (raw: string) => {
    setPeerCallsign(raw.toUpperCase());
  };

  const onPortKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
  };

  const handleSetPassword = async () => {
    // v0.1: use window.prompt for password entry; no secret ever stored in state.
    const pw = window.prompt(`Enter password for ${peerCallsign} (leave blank to cancel):`);
    if (pw === null || pw === '') return;
    try {
      await invoke('p2p_peer_password_set', { callsign: peerCallsign, password: pw });
      setPasswordStatus('Set');
    } catch {
      // Backend errors propagate via session log; swallowed here.
    }
  };

  const handleClearPassword = async () => {
    try {
      await invoke('p2p_peer_password_clear', { callsign: peerCallsign });
      setPasswordStatus('NotSet');
    } catch {
      // Swallowed; session log carries any backend error.
    }
  };

  const connect = async () => {
    if (busy) return;
    setBusy(true);
    setResult(null);
    setDialError(null);
    try {
      const res = await invoke<DialResult>('telnet_p2p_dial', {
        req: {
          host: host.trim() || DEFAULT_HOST,
          port,
          peer_callsign: peerCallsign,
          my_callsign: myCallsign,
          locator,
        },
      });
      setResult(res);
    } catch (e) {
      setDialError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const subText = peerCallsign
    ? `${peerCallsign} @ ${host.trim() || DEFAULT_HOST}:${port}`
    : `${host.trim() || DEFAULT_HOST}:${port}`;

  return (
    <RadioPanel
      mode={{ kind: 'telnet', intent: 'p2p' }}
      state={busy ? 'connecting' : 'disconnected'}
      sub={subText}
      onClose={onClose}
    >
      {/* Peer endpoint */}
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
          />
        </label>
        <label className="radio-panel-input-row">
          <span>Port</span>
          <input
            type="number"
            className="radio-panel-input radio-panel-input--narrow"
            data-testid="p2p-port-input"
            value={port}
            min={1}
            max={65535}
            onChange={(e) => setPort(parseInt(e.target.value, 10) || DEFAULT_PORT)}
            onKeyDown={onPortKey}
          />
        </label>
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
      </section>

      {/* Peer password */}
      <section className="radio-panel-sec">
        <h5>Peer Password</h5>
        <div className="p2p-password-row">
          <span
            className="p2p-password-status"
            data-testid="p2p-password-status"
          >
            {passwordStatus === 'Set' ? '<set>' : '<not set>'}
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
        <p className="radio-panel-help">
          Optional. Sent only if the peer station challenges for a password.
          Stored in OS keyring — never in config or logs.
        </p>
      </section>

      {/* Session log */}
      <SessionLogSection entries={logEntries} onClear={clearLog} />

      {/* Result / error feedback */}
      {result && (
        <section className="radio-panel-sec">
          <p
            className="p2p-result"
            data-testid="p2p-result"
          >
            Sent {result.sent_count}, received {result.received_count}.
          </p>
        </section>
      )}
      {dialError && (
        <section className="radio-panel-sec">
          <p
            className="p2p-error"
            data-testid="p2p-error"
          >
            {dialError}
          </p>
        </section>
      )}

      {/* Actions */}
      <section className="radio-panel-sec radio-panel-act">
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-primary"
          data-testid="p2p-connect-btn"
          disabled={busy}
          onClick={connect}
        >
          {busy ? 'Connecting…' : 'Connect'}
        </button>
      </section>
    </RadioPanel>
  );
}
