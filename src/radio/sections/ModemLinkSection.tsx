// src/radio/sections/ModemLinkSection.tsx
//
// Modem-link section for any TNC-mediated mode (Packet today; ARDOP +
// VARA reuse it once those panels migrate). Per spec §5.2.
//
// Densified from `src/packet/PacketModemBlock` for the 360 px right-
// panel column. Renders a 3-way segmented picker (TCP / USB / BT) over
// the matching field set, and emits a flat ModemLinkFields object to
// the parent on every persist trigger (segment switch + blur on text
// fields, dropdown select). The parent owns the actual persist call
// (e.g. `invoke('packet_config_set', { dto })`).
//
// tuxlink-mqu3: USB and BT segments now ALSO load the available devices
// from the backend and render dropdowns + Refresh + manual-text fallback
// — restoring the picker UX the legacy `PacketConnectionPanel` shipped
// (commit 7c30135) and the original migration ("densified for 360 px"
// = e2ca267) dropped. The BT segment emits `linkKind: 'Bluetooth'` +
// `btMac` — the in-app RFCOMM-socket transport (tuxlink-nx2) that bypasses
// the broken `/dev/rfcommN` serialport TTY path.

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './ModemLinkSection.css';

/** UI-segment identity. Each maps to a distinct wire `linkKind`: TCP →
 *  `Tcp`, USB → `Serial`, BT → `Bluetooth`. The UI segment is the
 *  authoritative source for the kind; the field set follows. */
type ModemSegment = 'tcp' | 'usb' | 'bt';

/** A USB-class serial device returned by `packet_list_serial_devices`. */
interface SerialDeviceDto {
  path: string;
  kind: 'usb' | 'bluetooth' | 'uart';
  label: string;
}

/** A paired Bluetooth radio returned by `packet_list_bluetooth_devices`. */
interface BluetoothDeviceDto {
  mac: string;
  name: string;
}

/** The flat field set emitted by `onChange`. Mirrors the PacketConfigDto
 *  subset that the modem editor owns; parent merges this into its config
 *  DTO and persists via `packet_config_set`. */
export interface ModemLinkFields {
  linkKind: 'Tcp' | 'Serial' | 'Bluetooth';
  tcpHost: string | null;
  tcpPort: number | null;
  serialDevice: string | null;
  serialBaud: number | null;
  /** Bluetooth radio MAC (non-null when `linkKind === 'Bluetooth'`). The
   *  dial side hands this to `KissLinkConfig::Bluetooth { mac }` — the
   *  RFCOMM-socket transport. Restored 2026-06-02 per tuxlink-mqu3. */
  btMac: string | null;
}

export interface ModemLinkSectionProps {
  /** Current link kind from the persisted config. Drives the active
   *  segment on first render + when the config reloads underneath. */
  kind: 'Tcp' | 'Serial' | 'Bluetooth';
  /** TCP host (used when kind='Tcp'). */
  host?: string;
  /** TCP port (used when kind='Tcp'). */
  port?: number;
  /** Serial device path (used when kind='Serial'). */
  serialDevice?: string;
  /** Serial host-link baud (used when kind='Serial'). */
  serialBaud?: number;
  /** Bluetooth radio MAC (used when kind='Bluetooth'). */
  btMac?: string;
  /** Emit the flat field set after every persist trigger. */
  onChange: (fields: ModemLinkFields) => void;
}

const DEFAULT_TCP_HOST = '127.0.0.1';
const DEFAULT_TCP_PORT = 8001;
// AX.25 host-link baud (TNC-to-host KISS link, NOT over-air). Operator smoke
// 2026-05-31: default should be 1200 — that's what almost every TNC ships
// with, and the prior 9600 default was wrong for the common case. The
// selector lists the full standard ladder; the operator picks per radio.
const DEFAULT_SERIAL_BAUD = 1200;

/** Standard TNC host-link bauds. 1200 is the common default for KISS TNCs;
 *  9600 is also common; the higher rates exist for KISS-over-USB-CDC and
 *  Bluetooth SPP adapters. Wire field on PacketConfigDto = serialBaud. */
const SERIAL_BAUD_OPTIONS = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200] as const;

function initialSegment(kind: 'Tcp' | 'Serial' | 'Bluetooth'): ModemSegment {
  if (kind === 'Tcp') return 'tcp';
  if (kind === 'Bluetooth') return 'bt';
  return 'usb';
}

export function ModemLinkSection({
  kind,
  host,
  port,
  serialDevice,
  serialBaud,
  btMac,
  onChange,
}: ModemLinkSectionProps) {
  // Controlled local state so editing TCP host / port doesn't bounce
  // through onChange on every keystroke — only on blur and segment-switch.
  // Mirrors the PacketModemBlock idiom that prevents the TCP host from
  // leaking into the device field when the segment swaps (the original
  // "127.0.0.1 for USB/Bluetooth" bug, fixed in the legacy panel).
  const [segment, setSegment] = useState<ModemSegment>(() => initialSegment(kind));
  const [hostInput, setHostInput] = useState(host ?? DEFAULT_TCP_HOST);
  const [portInput, setPortInput] = useState(String(port ?? DEFAULT_TCP_PORT));
  const [deviceInput, setDeviceInput] = useState(serialDevice ?? '');
  const [btMacInput, setBtMacInput] = useState(btMac ?? '');
  // Track baud as local state so changes apply immediately on the new value
  // (rather than the stale prop) when the operator selects from the dropdown.
  const [baudInput, setBaudInput] = useState<number>(serialBaud ?? DEFAULT_SERIAL_BAUD);

  // Device enumeration state (tuxlink-mqu3). Loaded on segment-activation
  // and on Refresh; an empty list is normal (no TNC plugged in / no paired
  // radio) and surfaced as "no devices found — refresh after plugging in".
  const [serialDevices, setSerialDevices] = useState<SerialDeviceDto[]>([]);
  const [btDevices, setBtDevices] = useState<BluetoothDeviceDto[]>([]);

  // Re-seed when props change underneath (parent loaded a new config).
  useEffect(() => {
    setSegment(initialSegment(kind));
    setHostInput(host ?? DEFAULT_TCP_HOST);
    setPortInput(String(port ?? DEFAULT_TCP_PORT));
    setDeviceInput(serialDevice ?? '');
    setBtMacInput(btMac ?? '');
    setBaudInput(serialBaud ?? DEFAULT_SERIAL_BAUD);
  }, [kind, host, port, serialDevice, serialBaud, btMac]);

  const loadSerialDevices = useCallback(() => {
    // tuxlink-61yg: defend against the Tauri invoke resolving to undefined
    // (mock setups that don't define this command return undefined, which
    // would set state to undefined and crash the next render at the
    // `serialDevices.filter(...)` line below).
    void invoke<SerialDeviceDto[]>('packet_list_serial_devices')
      .then((list) => setSerialDevices(Array.isArray(list) ? list : []))
      .catch(() => setSerialDevices([]));
  }, []);

  const loadBluetoothDevices = useCallback(() => {
    void invoke<BluetoothDeviceDto[]>('packet_list_bluetooth_devices')
      .then((list) => setBtDevices(Array.isArray(list) ? list : []))
      .catch(() => setBtDevices([]));
  }, []);

  // Auto-load the active segment's device list when it becomes visible.
  // Refresh is operator-driven (a button); this gets the first list on
  // segment-switch without forcing them to click Refresh first.
  useEffect(() => {
    if (segment === 'usb') loadSerialDevices();
    else if (segment === 'bt') loadBluetoothDevices();
  }, [segment, loadSerialDevices, loadBluetoothDevices]);

  const emit = (
    seg: ModemSegment,
    overrides?: { baud?: number; device?: string; mac?: string },
  ) => {
    if (seg === 'tcp') {
      onChange({
        linkKind: 'Tcp',
        tcpHost: hostInput.trim() || DEFAULT_TCP_HOST,
        tcpPort: Number(portInput) || DEFAULT_TCP_PORT,
        serialDevice: null,
        serialBaud: null,
        btMac: null,
      });
    } else if (seg === 'usb') {
      const device = (overrides?.device ?? deviceInput).trim();
      onChange({
        linkKind: 'Serial',
        tcpHost: null,
        tcpPort: null,
        // Empty input is a no-op rather than wiping the persisted device —
        // segment-switches shouldn't clear an existing path when the
        // operator hasn't typed anything yet.
        serialDevice: device || (serialDevice ?? null),
        serialBaud: overrides?.baud ?? baudInput,
        btMac: null,
      });
    } else {
      // bt segment
      const mac = (overrides?.mac ?? btMacInput).trim();
      onChange({
        linkKind: 'Bluetooth',
        tcpHost: null,
        tcpPort: null,
        serialDevice: null,
        serialBaud: null,
        btMac: mac || (btMac ?? null),
      });
    }
  };

  const selectSegment = (seg: ModemSegment) => {
    setSegment(seg);
    emit(seg);
  };

  // Filter to USB-class entries for the USB segment. The backend also
  // returns UART (`ttyAMA0`, `ttyS0`) for completeness; the legacy picker
  // surfaced only USB serial adapters here. UART devices are accessible via
  // the manual-fallback input for the rare GPIO-KISS case.
  const usbList = serialDevices.filter((d) => d.kind === 'usb');

  return (
    <section className="radio-panel-sec" data-testid="modem-link-section">
      <h5>Modem link</h5>
      <div className="radio-panel-segmented" role="group" aria-label="Modem transport">
        <button
          type="button"
          className={segment === 'tcp' ? 'active' : ''}
          aria-pressed={segment === 'tcp'}
          data-testid="modem-seg-tcp"
          onClick={() => selectSegment('tcp')}
        >
          TCP
        </button>
        <button
          type="button"
          className={segment === 'usb' ? 'active' : ''}
          aria-pressed={segment === 'usb'}
          data-testid="modem-seg-usb"
          onClick={() => selectSegment('usb')}
        >
          USB
        </button>
        <button
          type="button"
          className={segment === 'bt' ? 'active' : ''}
          aria-pressed={segment === 'bt'}
          data-testid="modem-seg-bt"
          onClick={() => selectSegment('bt')}
        >
          BT
        </button>
      </div>

      {segment === 'tcp' ? (
        <>
          <label className="radio-panel-input-row">
            <span>Host</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="modem-host"
              value={hostInput}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              onChange={(e) => setHostInput(e.target.value)}
              onBlur={() => emit('tcp')}
            />
          </label>
          <label className="radio-panel-input-row">
            <span>Port</span>
            <input
              type="text"
              inputMode="numeric"
              className="radio-panel-input"
              data-testid="modem-port"
              value={portInput}
              onChange={(e) => setPortInput(e.target.value)}
              onBlur={() => emit('tcp')}
            />
          </label>
        </>
      ) : segment === 'usb' ? (
        <>
          <label className="radio-panel-input-row">
            <span>Device</span>
            <select
              className="radio-panel-input"
              data-testid="modem-usb-select"
              value={usbList.some((d) => d.path === deviceInput) ? deviceInput : ''}
              onChange={(e) => {
                const next = e.target.value;
                setDeviceInput(next);
                emit('usb', { device: next });
              }}
            >
              <option value="" disabled>
                {usbList.length === 0 ? 'No USB serial devices found' : 'Choose USB device…'}
              </option>
              {usbList.map((d) => (
                <option key={d.path} value={d.path}>
                  {d.path} — {d.label}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="radio-panel-btn-sm"
              data-testid="modem-usb-refresh"
              onClick={loadSerialDevices}
              aria-label="Refresh USB device list"
            >
              ↻
            </button>
          </label>
          <label className="radio-panel-input-row">
            <span>Manual</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="modem-device"
              value={deviceInput}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder="/dev/ttyUSB0 (unlisted)"
              onChange={(e) => setDeviceInput(e.target.value)}
              onBlur={() => emit('usb')}
            />
          </label>
          <label className="radio-panel-input-row">
            <span>Serial baud</span>
            <select
              className="radio-panel-input"
              data-testid="modem-baud"
              aria-describedby="modem-baud-help"
              value={baudInput}
              onChange={(e) => {
                const next = Number(e.target.value);
                setBaudInput(next);
                emit('usb', { baud: next });
              }}
            >
              {SERIAL_BAUD_OPTIONS.map((b) => (
                <option key={b} value={b}>{b}</option>
              ))}
            </select>
          </label>
          <p className="modem-link-help" id="modem-baud-help" data-testid="modem-baud-help">
            USB/KISS host-link rate, not the AX.25 over-air packet rate (1200 baud).
          </p>
        </>
      ) : (
        <>
          <label className="radio-panel-input-row">
            <span>BT dev</span>
            <select
              className="radio-panel-input"
              data-testid="modem-bt-select"
              value={btDevices.some((d) => d.mac === btMacInput) ? btMacInput : ''}
              onChange={(e) => {
                const next = e.target.value;
                setBtMacInput(next);
                emit('bt', { mac: next });
              }}
            >
              <option value="" disabled>
                {btDevices.length === 0 ? 'No paired devices — pair a radio in BlueZ' : 'Choose paired device…'}
              </option>
              {btDevices.map((d) => (
                <option key={d.mac} value={d.mac}>
                  {d.name || d.mac} — {d.mac}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="radio-panel-btn-sm"
              data-testid="modem-bt-refresh"
              onClick={loadBluetoothDevices}
              aria-label="Refresh Bluetooth device list"
            >
              ↻
            </button>
          </label>
          <label className="radio-panel-input-row">
            <span>Manual</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="modem-bt-mac"
              value={btMacInput}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder="AA:BB:CC:DD:EE:FF (unpaired)"
              onChange={(e) => setBtMacInput(e.target.value)}
              onBlur={() => emit('bt')}
            />
          </label>
        </>
      )}
    </section>
  );
}
