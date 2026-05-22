// src/packet/PacketConnectionPanel.tsx
// AX.25 packet connection panel — renders in the reading-pane slot when the
// sidebar's "Packet (AX.25)" connection is selected (NO pop-up window). Faithful
// to the locked mock `.superpowers/brainstorm/2163380-1779446277/content/
// packet-inline-v2.html`. Additive on the .layout-b shell.
//
// Props carry config + base call so tests inject synthetic data without a Tauri
// runtime (mirrors MessageViewLoaded). The container variant (PacketConnectionPanelContainer)
// does the invoke() wiring.
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ssidOptions, effectiveCall, pathPreview, withSsid } from './packetConfig';
import './PacketConnectionPanel.css';
import type { PacketConfigDto, PacketLinkKind } from './packetTypes';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ModemSegment = 'tcp' | 'usb' | 'bt';

/** Derive the initial UI segment from the config link kind. TCP → 'tcp'; any
 *  Serial link defaults to 'usb' (USB is the common case; operator switches to
 *  BT — both produce linkKind:'Serial'). null → 'tcp' (default). */
function initialSegment(config: PacketConfigDto | null): ModemSegment {
  return config?.linkKind === 'Serial' ? 'usb' : 'tcp';
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface PacketConnectionPanelProps {
  /** Current packet config (null while packet_config_get is in flight). */
  config: PacketConfigDto | null;
  /** Operator base callsign from identity; '' until loaded. */
  baseCall: string;
  /** Persist a new SSID (container wires this to packet_config_set). */
  onSsidPersist?: (ssid: number) => void;
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function PacketConnectionPanel({ config, baseCall, onSsidPersist }: PacketConnectionPanelProps) {
  // SSID state — seeded from config, re-synced when config loads from null
  const [ssid, setSsid] = useState<number>(config?.ssid ?? 0);
  useEffect(() => {
    if (config !== null) {
      setSsid(config.ssid);
    }
  }, [config?.ssid]);

  // Listen state — re-synced when config loads
  const [listening, setListening] = useState<boolean>(config?.listenDefault ?? true);
  useEffect(() => {
    if (config !== null) {
      setListening(config.listenDefault);
    }
  }, [config?.listenDefault]);

  // Connect state
  const [target, setTarget] = useState('');
  const [relays, setRelays] = useState<string[]>([]);

  const onToggleListen = () => {
    const next = !listening;
    setListening(next);
    void invoke('packet_set_listen', { enabled: next }).catch(() => {
      setListening((v) => !v);
    });
  };

  const onAddRelay = () => setRelays((r) => (r.length < 2 ? [...r, ''] : r));
  const onRelayChange = (i: number, v: string) =>
    setRelays((r) => r.map((x, idx) => (idx === i ? v : x)));
  const onRemoveRelay = (i: number) => setRelays((r) => r.filter((_, idx) => idx !== i));

  const onConnect = () => {
    const call = target.trim();
    if (!call) return;
    const path = relays.map((r) => r.trim()).filter(Boolean);
    void invoke('packet_connect', { call, path }).catch(() => {});
  };

  return (
    <div className="reading-pane packet-panel" data-testid="packet-panel-root">
      {/* Header */}
      <div className="packet-panel-head">
        <h2 className="packet-panel-title" data-testid="packet-panel-title">
          Packet (AX.25)
        </h2>
        <span className="packet-panel-badge" data-testid="packet-panel-badge">
          1200 baud
        </span>
      </div>
      <p className="packet-panel-sub">
        Connection setup — shown here in the reading pane; no separate window.
      </p>

      {/* Modem block */}
      <PacketModemBlock config={config} />

      {/* My station block */}
      <div className="packet-blk" data-testid="station-block">
        <div className="packet-blk-h">
          <span>My station</span>
          <span className="packet-new" data-testid="station-new-badge">NEW: SSID</span>
        </div>
        <div className="packet-ssidwrap">
          <label className="packet-f packet-base">
            Base call sign
            <input className="packet-inp" data-testid="station-base" value={baseCall} readOnly />
          </label>
          <label className="packet-f">
            SSID
            <select
              className="packet-ssidsel"
              data-testid="station-ssid"
              value={ssid}
              onChange={(e) => {
                const n = Number(e.target.value);
                setSsid(n);
                onSsidPersist?.(n);
              }}
            >
              {ssidOptions().map((n) => (
                <option key={n} value={n}>{`-${n}`}</option>
              ))}
            </select>
          </label>
        </div>
        <p className="packet-hint">
          Operating as <b data-testid="station-effective">{effectiveCall(baseCall, ssid)}</b>.
          SSID (0–15) is new to tuxlink — packet needs it (mobile -7, gateways -10, digis -1).
        </p>
      </div>

      {/* Status / listen toggle */}
      <div className="packet-blk" data-testid="status-block">
        <div className="packet-blk-h"><span>Status</span></div>
        <button
          type="button"
          className={`packet-listen ${listening ? 'on' : 'off'}`}
          data-testid="listen-switch"
          role="switch"
          aria-checked={listening}
          onClick={onToggleListen}
        >
          <span className={`packet-switch ${listening ? 'on' : ''}`} aria-hidden="true">
            <i />
          </span>
          <span className="packet-listen-text">
            <span className="packet-listen-title" data-testid="listen-label">
              {listening ? `Listening as ${effectiveCall(baseCall, ssid)}` : `Listen disabled`}
            </span>
            <span className="packet-listen-sub">
              Answers incoming packet calls when idle (default on)
            </span>
          </span>
        </button>
      </div>

      {/* Connect block */}
      <div className="packet-blk" data-testid="connect-block">
        <div className="packet-blk-h"><span>Connect</span></div>
        <label className="packet-f">
          Connect to
          <input
            className="packet-inp"
            data-testid="connect-to"
            value={target}
            placeholder="call sign (gateway or peer)"
            onChange={(e) => setTarget(e.target.value)}
          />
        </label>
        <p className="packet-hint">
          Gateway vs peer <b>auto-detected</b> — answers a login challenge only if one is sent.
        </p>

        <div className="packet-relays">
          <label className="packet-relays-label">
            Digipeater path <span className="packet-faint">(relays · 0–2)</span>
          </label>
          <div className="packet-chips" data-testid="relay-chips">
            {relays.map((r, i) => (
              <span className="packet-chip" key={i} data-testid={`relay-chip-${i}`}>
                {/* chip label (visible text) — toHaveTextContent asserts on this */}
                <span className="packet-chip-label">{r}</span>
                <input
                  className="packet-chip-input"
                  data-testid={`relay-input-${i}`}
                  value={r}
                  placeholder="W7RPT-1"
                  onChange={(e) => onRelayChange(i, e.target.value)}
                />
                <button
                  type="button"
                  className="packet-chip-x"
                  data-testid={`relay-remove-${i}`}
                  aria-label={`Remove relay ${i + 1}`}
                  onClick={() => onRemoveRelay(i)}
                >
                  ✕
                </button>
              </span>
            ))}
            {relays.length < 2 && (
              <button
                type="button"
                className="packet-chip packet-chip-add"
                data-testid="add-relay"
                onClick={onAddRelay}
              >
                + add relay
              </button>
            )}
          </div>
          <p className="packet-path" data-testid="path-preview">
            Path: <code>{pathPreview(baseCall, ssid, relays, target)}</code> · 0 relays = direct
          </p>
        </div>

        <button
          type="button"
          className="packet-connect-btn"
          data-testid="packet-connect-btn"
          onClick={onConnect}
        >
          {target.trim() ? `Connect to ${target.trim()}` : 'Connect'}
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Modem sub-component (Tasks 3-4)
// ---------------------------------------------------------------------------

function PacketModemBlock({ config }: { config: PacketConfigDto | null }) {
  const [segment, setSegment] = useState<ModemSegment>(() => initialSegment(config));
  const host = config?.linkKind === 'Tcp' ? (config.tcpHost ?? '127.0.0.1') : '127.0.0.1';
  const port = config?.linkKind === 'Tcp' ? String(config.tcpPort ?? 8001) : '8001';
  const device = config?.linkKind === 'Serial' ? (config.serialDevice ?? '') : '';

  return (
    <div className="packet-blk" data-testid="modem-block">
      <div className="packet-blk-h">
        <span>Modem connection</span>
        <button type="button" className="packet-change" data-testid="modem-change">
          Change ▾
        </button>
      </div>
      <div className="packet-seg" role="group" aria-label="Modem transport">
        {([
          ['tcp', 'Network (TCP)'],
          ['usb', 'USB serial'],
          ['bt', 'Bluetooth'],
        ] as [ModemSegment, string][]).map(([seg, label]) => (
          <button
            key={seg}
            type="button"
            className={`packet-seg-btn ${segment === seg ? 'on' : ''}`.trim()}
            data-testid={`modem-seg-${seg}`}
            aria-pressed={segment === seg}
            onClick={() => setSegment(seg)}
          >
            {label}
          </button>
        ))}
      </div>
      {segment === 'tcp' ? (
        <div className="packet-row2">
          <label className="packet-f">
            Host
            <input className="packet-inp" data-testid="modem-host" defaultValue={host} />
          </label>
          <label className="packet-f">
            Port
            <input className="packet-inp" data-testid="modem-port" defaultValue={port} />
          </label>
        </div>
      ) : (
        <div className="packet-f">
          <label>
            {segment === 'bt' ? 'Bluetooth device (RFCOMM)' : 'Serial device'}
            <input
              className="packet-inp"
              data-testid="modem-device"
              defaultValue={device}
              placeholder={segment === 'bt' ? '/dev/rfcomm0' : '/dev/ttyUSB0'}
            />
          </label>
        </div>
      )}
      <p className="packet-hint">
        {segment === 'tcp'
          ? 'KISS over TCP — Dire Wolf (default 8001) / SoundModem. Modem does AFSK + framing; tuxlink runs the AX.25 link layer.'
          : segment === 'bt'
            ? 'Pair + bind the BT TNC at the OS (e.g. /dev/rfcomm0); tuxlink opens it as a serial device.'
            : 'USB KISS TNC as a serial device. Host-link baud is separate from the 1200-baud over-air rate.'}
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Container (Task 8) — loads config on mount, persists SSID changes
// ---------------------------------------------------------------------------

/** Container: loads packet config on mount, persists SSID changes (global sticky). */
export function PacketConnectionPanelContainer({ baseCall }: { baseCall: string }) {
  const [config, setConfig] = useState<PacketConfigDto | null>(null);

  useEffect(() => {
    let mounted = true;
    void invoke<PacketConfigDto>('packet_config_get')
      .then((c) => { if (mounted) setConfig(c); })
      .catch(() => { /* pre-config — panel renders with defaults */ });
    return () => { mounted = false; };
  }, []);

  const onSsidPersist = (ssid: number) => {
    setConfig((c) => {
      if (!c) return c;
      const next = withSsid(c, ssid);
      void invoke('packet_config_set', { dto: next }).catch(() => {});
      return next;
    });
  };

  return <PacketConnectionPanel config={config} baseCall={baseCall} onSsidPersist={onSsidPersist} />;
}

// Suppress unused import warning — PacketLinkKind is referenced in comments / type cast
void (undefined as unknown as PacketLinkKind);
