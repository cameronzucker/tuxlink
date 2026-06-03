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
//   - Port input IS exposed (default 8772): WLE itself surfaces txtPort
//     in TelnetP2PSetup.cs:392, and operators run WLE on non-default
//     ports for NAT / SSH-tunnel / multi-instance setups. Port choice is
//     independent of TLS — WLE P2P is plaintext regardless of port.
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
import { useQueryClient } from '@tanstack/react-query';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import { AllowedStationsEditor } from '../sections/AllowedStationsEditor';
import { ListenArmButton } from '../sections/ListenArmButton';
import { useListenerState } from '../sections/useListenerState';
import '../sections/ListenSection.css';

export interface TelnetP2pRadioPanelProps {
  onClose: () => void;
}

const DEFAULT_HOST = '127.0.0.1';
const DEFAULT_PORT = 8772; // WLE P2P listener default — operator can override per setup.
const MIN_PORT = 1;
const MAX_PORT = 65535;

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

// Telnet listener config wire shape — `telnet_listen_config_get` returns
// the bound port + bind address + TTL the next arm() will use. Mirrors
// the Rust DTO at ui_commands.rs.
interface TelnetListenConfig {
  port: number;
  bind_addr: string;
  ttl_secs: number;
}

// TTL preset options surfaced in the Listener setup expander. The
// backend accepts any positive value; these are operator-convenience
// presets that mirror the spec §1.3 layout. The first preset (15 min)
// is the safest default for "I'm just smoking it"; 1 hour matches the
// mock's selected default.
const TTL_PRESETS: { label: string; secs: number }[] = [
  { label: '15 min', secs: 15 * 60 },
  { label: '30 min', secs: 30 * 60 },
  { label: '1 hour', secs: 60 * 60 },
  { label: '4 hours', secs: 4 * 60 * 60 },
];

// Station password status — mirrors `p2p_peer_password_status`'s wire
// shape (the Telnet station password and Telnet P2P peer password use
// the same Set/NotSet enum).
type StationPasswordStatus = 'Set' | 'NotSet';

export function TelnetP2pRadioPanel({ onClose }: TelnetP2pRadioPanelProps) {
  const [busy, setBusy] = useState(false);
  const [host, setHost] = useState<string>(DEFAULT_HOST);
  const [port, setPort] = useState<number>(DEFAULT_PORT);
  const [peerCallsign, setPeerCallsign] = useState<string>('');
  const [myCallsign, setMyCallsign] = useState<string>('');
  const [locator, setLocator] = useState<string>('');
  const [passwordStatus, setPasswordStatus] = useState<PasswordStatus>('NotSet');
  const [result, setResult] = useState<DialResult | null>(null);
  const [connectError, setConnectError] = useState<string | null>(null);
  const { entries: logEntries, clear: clearLog } = useSessionLog();
  const queryClient = useQueryClient();

  // Listener config (bind addr / port / TTL). Loaded on mount; edits
  // persist via `telnet_listen_config_set`. Defaults match the backend
  // (`127.0.0.1`, port 8774, 1h TTL — see ui_commands.rs).
  const [listenConfig, setListenConfig] = useState<TelnetListenConfig>({
    port: 8774,
    bind_addr: '127.0.0.1',
    ttl_secs: 3600,
  });

  // Station-password status — driven by `telnet_station_password_is_set`
  // on mount + after each Set/Clear. Mirrors the Peer Password pattern
  // above but lives in the listener section.
  const [stationPasswordStatus, setStationPasswordStatus] =
    useState<StationPasswordStatus>('NotSet');

  // Listener arms + allowlist plumbing via the shared hook. The Telnet
  // commands take `enabled`/`callsign`/`pattern` args by name.
  const listener = useListenerState({
    commands: {
      listen: 'telnet_listen',
      setListen: 'telnet_set_listen',
      allowedGet: 'telnet_allowed_stations_get',
      allowedAddCallsign: 'telnet_allowed_stations_add_callsign',
      allowedAddCallsignArgKey: 'callsign',
      allowedRemoveCallsign: 'telnet_allowed_stations_remove_callsign',
      allowedRemoveCallsignArgKey: 'callsign',
      allowedSetAllowAll: 'telnet_allowed_stations_set_allow_all',
      allowedSetAllowAllArgKey: 'enabled',
      allowedAddIp: 'telnet_allowed_stations_add_ip',
      allowedAddIpArgKey: 'pattern',
      allowedRemoveIp: 'telnet_allowed_stations_remove_ip',
      allowedRemoveIpArgKey: 'pattern',
    },
    ttlSecs: listenConfig.ttl_secs,
  });

  // Load listener config + station-password status on mount.
  useEffect(() => {
    let cancelled = false;
    invoke<TelnetListenConfig>('telnet_listen_config_get')
      .then((c) => {
        if (cancelled) return;
        if (c) setListenConfig(c);
      })
      .catch(() => {
        // Backend default applies — keep the local fallback.
      });
    // Codex review 2026-06-03 [P2] (tuxlink-7vea): the backend returns
    // `StationPasswordStatus` (serialized as the literal string "Set" or
    // "NotSet"), NOT a bool. The prior bool coercion was always truthy on
    // a fresh install and showed "Set" when no password existed.
    invoke<'Set' | 'NotSet'>('telnet_station_password_is_set')
      .then((status) => {
        if (cancelled) return;
        setStationPasswordStatus(status);
      })
      .catch(() => {
        if (!cancelled) setStationPasswordStatus('NotSet');
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Persist listener config edits. Merge-then-write so partial edits
  // (e.g., changing only port) don't clobber the other fields.
  // Codex review 2026-06-03 [P2] (tuxlink-7vea): the backend command takes
  // a single `req: TelnetListenConfigDto` parameter; the prior call passed
  // the DTO fields at the top level so Tauri rejected the invoke as
  // missing `req` and the catch swallowed the error.
  const persistListenConfig = (patch: Partial<TelnetListenConfig>) => {
    const next = { ...listenConfig, ...patch };
    setListenConfig(next);
    void invoke('telnet_listen_config_set', { req: next }).catch(() => {
      // Persist errors surface in the session log via the backend.
    });
  };

  const handleSetStationPassword = async () => {
    // v0.1: window.prompt — matches the Peer Password Set flow above
    // so operators don't see two different password-entry idioms in the
    // same panel. The secret never enters component state.
    const pw = window.prompt(
      'Enter station password (sent to inbound peers as a challenge; blank = cancel):',
    );
    if (pw === null || pw === '') return;
    try {
      await invoke('telnet_station_password_set', { password: pw });
      setStationPasswordStatus('Set');
    } catch {
      // Backend errors surface in the session log.
    }
  };

  const handleClearStationPassword = async () => {
    try {
      await invoke('telnet_station_password_clear');
      setStationPasswordStatus('NotSet');
    } catch {
      // Session log carries any backend error.
    }
  };

  // Summary chip counts — drive the expander-count text without
  // recomputing on every render mid-key.
  const allowedSummary = (() => {
    const c = listener.allowed.callsigns.length;
    const i = listener.allowed.ips.length;
    if (listener.allowed.allowAll) return 'allow any';
    if (c === 0 && i === 0) return 'restrict to none';
    const parts: string[] = [];
    if (c > 0) parts.push(`${c} callsign${c === 1 ? '' : 's'}`);
    if (i > 0) parts.push(`${i} IP${i === 1 ? '' : 's'}`);
    return parts.join(' · ');
  })();

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
          port,
          peer_callsign: peerCallsign,
          my_callsign: myCallsign,
          locator,
        },
      });
      setResult(res);
      // tuxlink-l55l: sent messages moved Outbox→Sent and received messages
      // landed in Inbox during the exchange. Refresh both views so the
      // operator sees them without waiting for the 10s refetch.
      await queryClient.invalidateQueries({ queryKey: ['mailbox'] });
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
    ? `${peerCallsign} @ ${host.trim() || DEFAULT_HOST}:${port}`
    : `${host.trim() || DEFAULT_HOST}:${port}`;

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
        <label className="radio-panel-input-row">
          <span>Port</span>
          <input
            type="number"
            className="radio-panel-input"
            data-testid="p2p-port-input"
            value={port}
            min={MIN_PORT}
            max={MAX_PORT}
            onChange={(e) => {
              const n = parseInt(e.target.value, 10);
              if (!Number.isNaN(n) && n >= MIN_PORT && n <= MAX_PORT) setPort(n);
            }}
            onKeyDown={onHostKey}
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
          Plaintext only. WLE P2P does not support TLS at any port — TLS
          (CmsSsl, port 8773) exists for CMS only. Set the port in the
          Peer Station section above.
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

      {/* Listen (Accept Inbound) — listener arms + allowlist + station
          password. Replaces the prior backend-only listener affordances
          per spec 2026-06-03-listener-ui-design.md §1.3. */}
      <section
        className="radio-panel-sec"
        data-testid="telnet-listen-section"
      >
        <h5>Listen (Accept Inbound)</h5>

        <ListenArmButton
          armed={listener.armed}
          minutesRemaining={listener.minutesRemaining}
          busy={listener.busy}
          helpText={`Accepts inbound Telnet P2P sessions on ${listenConfig.bind_addr}:${listenConfig.port} until disarmed or the TTL expires.`}
          onArm={listener.arm}
          onDisarm={listener.disarm}
          testIdPrefix="telnet-listen"
        />
        {listener.error && (
          <p
            className="radio-panel-radio-help"
            data-testid="telnet-listen-error"
            style={{ color: 'var(--error, #f87171)' }}
          >
            {listener.error}
          </p>
        )}

        {/* Listener setup — bind address + port + TTL. Closed by
            default since most operators take the backend defaults. */}
        <details
          className="expander"
          data-testid="telnet-listen-setup-expander"
        >
          <summary className="expander-summary">
            Listener setup
            <span className="expander-count" data-testid="telnet-listen-setup-count">
              {`${listenConfig.port} · ${listenConfig.bind_addr === '127.0.0.1' ? 'loopback' : listenConfig.bind_addr}`}
            </span>
          </summary>
          <div className="expander-body">
            <label className="radio-panel-input-row">
              <span>Bind</span>
              <input
                type="text"
                className="radio-panel-input"
                data-testid="telnet-listen-bind-input"
                value={listenConfig.bind_addr}
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
                onChange={(e) =>
                  setListenConfig((c) => ({ ...c, bind_addr: e.target.value }))
                }
                onBlur={() =>
                  persistListenConfig({ bind_addr: listenConfig.bind_addr.trim() })
                }
              />
            </label>
            <label className="radio-panel-input-row">
              <span>Port</span>
              <input
                type="number"
                className="radio-panel-input"
                data-testid="telnet-listen-port-input"
                value={listenConfig.port}
                min={MIN_PORT}
                max={MAX_PORT}
                onChange={(e) => {
                  const n = parseInt(e.target.value, 10);
                  if (!Number.isNaN(n) && n >= MIN_PORT && n <= MAX_PORT) {
                    persistListenConfig({ port: n });
                  }
                }}
              />
            </label>
            <label className="radio-panel-input-row">
              <span>TTL</span>
              <select
                className="radio-panel-input"
                data-testid="telnet-listen-ttl-select"
                value={listenConfig.ttl_secs}
                onChange={(e) =>
                  persistListenConfig({ ttl_secs: parseInt(e.target.value, 10) })
                }
              >
                {TTL_PRESETS.map((p) => (
                  <option key={p.secs} value={p.secs}>
                    {p.label}
                  </option>
                ))}
              </select>
            </label>
            <p className="radio-panel-help">
              Loopback binds only on the local machine. LAN binding opens
              the listener to every device on the network.
            </p>
          </div>
        </details>

        {/* Allowed stations — callsigns + IP patterns. */}
        <details className="expander" data-testid="telnet-allowed-expander">
          <summary className="expander-summary">
            Allowed stations
            <span className="expander-count" data-testid="telnet-allowed-count">
              {allowedSummary}
            </span>
          </summary>
          <AllowedStationsEditor
            allowAll={listener.allowed.allowAll}
            callsigns={listener.allowed.callsigns}
            ips={listener.allowed.ips}
            helpText="Match logic: callsign-allow OR IP-allow. A peer is admitted if either list matches; when Allow-any-peer is ON, both lists are advisory."
            onSetAllowAll={listener.setAllowAll}
            onAddCallsign={listener.addCallsign}
            onRemoveCallsign={listener.removeCallsign}
            onAddIp={listener.addIp}
            onRemoveIp={listener.removeIp}
            testIdPrefix="telnet-allowed"
          />
        </details>

        {/* Station Password — keyring-backed; sent to peers as
            CR-terminated challenge before B2F per WLE wire parity. */}
        <details className="expander" data-testid="telnet-station-pw-expander">
          <summary className="expander-summary">
            Station Password
            <span className="expander-count" data-testid="telnet-station-pw-count">
              {stationPasswordStatus === 'Set' ? 'set in keyring' : 'not set'}
            </span>
          </summary>
          <div className="expander-body">
            <div className="radio-panel-chip-row">
              <button
                type="button"
                className="radio-panel-chip"
                data-testid="telnet-station-pw-set-btn"
                onClick={handleSetStationPassword}
              >
                {stationPasswordStatus === 'Set' ? 'Change…' : 'Set…'}
              </button>
              <button
                type="button"
                className="radio-panel-chip"
                data-testid="telnet-station-pw-clear-btn"
                disabled={stationPasswordStatus !== 'Set'}
                onClick={handleClearStationPassword}
              >
                Clear
              </button>
            </div>
            <p className="radio-panel-help">
              Stored in OS keyring — never in config or logs. Sent to every
              inbound peer as a CR-terminated challenge before B2F begins.
            </p>
          </div>
        </details>
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
