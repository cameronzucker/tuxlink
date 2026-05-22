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
import type { PacketConfigDto } from './packetTypes';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ModemSegment = 'tcp' | 'usb' | 'bt';

/** The flat link fields the modem editor writes back into PacketConfigDto. */
export type ModemLinkFields = Pick<
  PacketConfigDto,
  'linkKind' | 'tcpHost' | 'tcpPort' | 'serialDevice' | 'serialBaud'
>;

/** A discovered serial/RFCOMM device from the packet_list_serial_devices command.
 *  `kind` ("usb" | "bluetooth" | "uart") lets the picker show USB and Bluetooth
 *  devices in their own tabs instead of one undifferentiated list. */
interface SerialDevice {
  path: string;
  kind: string;
  label: string;
}

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
  /** Persist the modem link fields (container wires this to packet_config_set). */
  onLinkPersist?: (fields: ModemLinkFields) => void;
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function PacketConnectionPanel({ config, baseCall, onSsidPersist, onLinkPersist }: PacketConnectionPanelProps) {
  // SSID state — seeded from config, re-synced when config loads from null
  const [ssid, setSsid] = useState<number>(config?.ssid ?? 0);
  useEffect(() => {
    if (config !== null) {
      setSsid(config.ssid);
    }
  }, [config?.ssid]);

  // Armed = a real `packet_listen` is in flight (the station is actually waiting
  // to answer an inbound call). This is LIVE state, NOT a preference — it only
  // becomes true after `packet_listen` is invoked, and clears on Stop/complete.
  const [armed, setArmed] = useState<boolean>(false);

  // listenDefault is a *preference* ("auto-arm on startup"), not a live-state
  // claim — re-synced when config loads. Persisted via packet_set_listen.
  const [listenDefault, setListenDefault] = useState<boolean>(config?.listenDefault ?? true);
  useEffect(() => {
    if (config !== null) {
      setListenDefault(config.listenDefault);
    }
  }, [config?.listenDefault]);

  // Connect state
  const [target, setTarget] = useState('');
  const [relays, setRelays] = useState<string[]>([]);

  // Real Listen action: arm → invoke packet_listen (which blocks in ax25::answer
  // until a call arrives or we abort). The promise resolves when the exchange
  // completes or the connection is aborted; either way we disarm. Clicking again
  // while armed invokes the backend abort (cms_abort) to stop listening.
  const onListen = () => {
    if (armed) {
      // Stop: shut the link so a blocked answer() unwinds (Cancelled).
      void invoke('cms_abort').catch(() => {});
      setArmed(false);
      return;
    }
    setArmed(true);
    void invoke('packet_listen')
      .catch(() => {})
      .finally(() => {
        // Whether the call was answered, the exchange completed, or it was
        // stopped, we're no longer waiting.
        setArmed(false);
      });
  };

  const onToggleListenDefault = () => {
    const next = !listenDefault;
    setListenDefault(next);
    void invoke('packet_set_listen', { enabled: next }).catch(() => {
      setListenDefault((v) => !v);
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
      <PacketModemBlock config={config} onPersistLink={onLinkPersist} />

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

      {/* Status / real Listen action */}
      <div className="packet-blk" data-testid="status-block">
        <div className="packet-blk-h"><span>Status</span></div>
        {/* The real Listen control: idle = "not listening"; armed = "waiting for
            a call". The label NEVER claims "Listening" until packet_listen has
            actually been invoked (armed). Clicking while armed aborts (Stop). */}
        <button
          type="button"
          className={`packet-listen-btn ${armed ? 'armed' : 'idle'}`}
          data-testid="listen-action"
          aria-pressed={armed}
          onClick={onListen}
        >
          <span className="packet-listen-text">
            <span className="packet-listen-title" data-testid="listen-label">
              {armed
                ? `Waiting for a call as ${effectiveCall(baseCall, ssid)} — Stop`
                : 'Listen for an incoming call'}
            </span>
            <span className="packet-listen-sub" data-testid="listen-sub">
              {armed
                ? 'Armed — will auto-answer the next inbound packet call (transmits a UA under your call).'
                : 'Not listening. Arm the station to answer an inbound packet call.'}
            </span>
          </span>
        </button>

        {/* listenDefault is a PREFERENCE (auto-arm on startup), clearly distinct
            from the live armed state above — it does not imply live listening. */}
        <label className="packet-listen-pref" data-testid="listen-default-pref">
          <input
            type="checkbox"
            data-testid="listen-default-toggle"
            checked={listenDefault}
            onChange={onToggleListenDefault}
          />
          <span>Auto-arm Listen on startup (preference, not a live state)</span>
        </label>
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

function PacketModemBlock({
  config,
  onPersistLink,
}: {
  config: PacketConfigDto | null;
  onPersistLink?: (fields: ModemLinkFields) => void;
}) {
  // Controlled state, seeded from config. (Uncontrolled `defaultValue` inputs let
  // React reuse a DOM node across the TCP↔serial swap, leaking the TCP host into
  // the device field — that was the "127.0.0.1 for USB/Bluetooth" bug.)
  const [segment, setSegment] = useState<ModemSegment>(() => initialSegment(config));
  const [host, setHost] = useState(config?.tcpHost ?? '127.0.0.1');
  const [port, setPort] = useState(String(config?.tcpPort ?? 8001));
  const [device, setDevice] = useState(config?.serialDevice ?? '');
  const [expanded, setExpanded] = useState(true);
  const baud = config?.serialBaud ?? 9600;

  // Re-seed when config loads (null → loaded) or changes underneath us.
  useEffect(() => {
    if (!config) return;
    setSegment(initialSegment(config));
    setHost(config.tcpHost ?? '127.0.0.1');
    setPort(String(config.tcpPort ?? 8001));
    setDevice(config.serialDevice ?? '');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config?.linkKind, config?.tcpHost, config?.tcpPort, config?.serialDevice]);

  // Discovered serial/RFCOMM devices for the USB/Bluetooth picker, from the
  // packet_list_serial_devices backend command. Loaded when those transports are
  // selected; the operator can Refresh after plugging in / binding a device.
  // Promise.resolve() tolerates a non-promise mock in unit tests.
  const [devices, setDevices] = useState<SerialDevice[]>([]);
  const loadDevices = () => {
    void Promise.resolve(invoke<SerialDevice[]>('packet_list_serial_devices'))
      .then((list) => setDevices(Array.isArray(list) ? list : []))
      .catch(() => setDevices([]));
  };
  useEffect(() => {
    if (segment === 'usb' || segment === 'bt') loadDevices();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [segment]);
  // Show only devices for the selected transport — USB tab → USB serial +
  // on-board UART; Bluetooth tab → RFCOMM. Never conflate USB and Bluetooth.
  const segDevices = devices.filter((d) =>
    segment === 'bt' ? d.kind === 'bluetooth' : d.kind === 'usb' || d.kind === 'uart',
  );

  // Build + persist the KISS link for a given transport from current fields.
  const persist = (seg: ModemSegment) => {
    if (!onPersistLink) return;
    if (seg === 'tcp') {
      onPersistLink({
        linkKind: 'Tcp',
        tcpHost: host.trim() || '127.0.0.1',
        tcpPort: Number(port) || 8001,
        serialDevice: null,
        serialBaud: null,
      });
    } else {
      onPersistLink({
        linkKind: 'Serial',
        serialDevice: device.trim(),
        serialBaud: baud,
        tcpHost: null,
        tcpPort: null,
      });
    }
  };

  const selectSegment = (seg: ModemSegment) => {
    setSegment(seg);
    persist(seg);
  };

  const summary =
    segment === 'tcp'
      ? `Network (TCP) · ${host || '127.0.0.1'}:${port || '8001'}`
      : segment === 'bt'
        ? `Bluetooth · ${device || '(no device set)'}`
        : `USB serial · ${device || '(no device set)'}`;

  return (
    <div className="packet-blk" data-testid="modem-block">
      <div className="packet-blk-h">
        <span>Modem connection</span>
        <button
          type="button"
          className="packet-change"
          data-testid="modem-change"
          aria-expanded={expanded}
          onClick={() => setExpanded((v) => !v)}
        >
          {expanded ? 'Hide ▴' : 'Change ▾'}
        </button>
      </div>

      {!expanded ? (
        <p className="packet-hint" data-testid="modem-summary">
          {summary}
        </p>
      ) : (
        <>
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
                onClick={() => selectSegment(seg)}
              >
                {label}
              </button>
            ))}
          </div>
          {segment === 'tcp' ? (
            <div className="packet-row2">
              <label className="packet-f">
                Host
                <input
                  className="packet-inp"
                  data-testid="modem-host"
                  value={host}
                  onChange={(e) => setHost(e.target.value)}
                  onBlur={() => persist('tcp')}
                />
              </label>
              <label className="packet-f">
                Port
                <input
                  className="packet-inp"
                  data-testid="modem-port"
                  value={port}
                  inputMode="numeric"
                  onChange={(e) => setPort(e.target.value)}
                  onBlur={() => persist('tcp')}
                />
              </label>
            </div>
          ) : (
            <div className="packet-f">
              <label className="packet-device-pick">
                {segment === 'bt' ? 'Bluetooth device (RFCOMM)' : 'Serial device'}
                <div className="packet-device-row">
                  <select
                    className="packet-inp"
                    data-testid="modem-device-select"
                    value={segDevices.some((d) => d.path === device) ? device : ''}
                    onChange={(e) => {
                      const v = e.target.value;
                      setDevice(v);
                      if (onPersistLink && v) {
                        onPersistLink({
                          linkKind: 'Serial',
                          serialDevice: v,
                          serialBaud: baud,
                          tcpHost: null,
                          tcpPort: null,
                        });
                      }
                    }}
                  >
                    <option value="">
                      {segDevices.length
                        ? '— select a device —'
                        : segment === 'bt'
                          ? '— no Bluetooth devices (pair + bind one at the OS, then Refresh) —'
                          : '— no USB/serial devices (plug one in, then Refresh) —'}
                    </option>
                    {segDevices.map((d) => (
                      <option key={d.path} value={d.path}>
                        {d.path} — {d.label}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="packet-change"
                    data-testid="modem-device-refresh"
                    onClick={loadDevices}
                  >
                    Refresh
                  </button>
                </div>
              </label>
              <label className="packet-device-manual">
                or enter a path manually
                <input
                  className="packet-inp"
                  data-testid="modem-device"
                  value={device}
                  placeholder={segment === 'bt' ? '/dev/rfcomm0' : '/dev/ttyUSB0'}
                  onChange={(e) => setDevice(e.target.value)}
                  onBlur={() => persist(segment)}
                />
              </label>
            </div>
          )}
          <p className="packet-hint">
            {segment === 'tcp'
              ? 'KISS over TCP — Dire Wolf (default 8001) / SoundModem. The software modem listens on a LOCAL TCP socket (127.0.0.1); this is not the internet.'
              : segment === 'bt'
                ? 'Pair + bind the BT TNC at the OS first (e.g. /dev/rfcomm0), then enter its device path here; tuxlink opens it as a serial device.'
                : 'USB KISS TNC as a serial device (e.g. /dev/ttyUSB0). Host-link baud is separate from the 1200-baud over-air rate.'}
          </p>
        </>
      )}
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

  const onLinkPersist = (fields: ModemLinkFields) => {
    setConfig((c) => {
      if (!c) return c;
      const next = { ...c, ...fields };
      void invoke('packet_config_set', { dto: next }).catch(() => {});
      return next;
    });
  };

  return (
    <PacketConnectionPanel
      config={config}
      baseCall={baseCall}
      onSsidPersist={onSsidPersist}
      onLinkPersist={onLinkPersist}
    />
  );
}

