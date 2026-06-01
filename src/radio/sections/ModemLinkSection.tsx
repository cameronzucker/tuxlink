// src/radio/sections/ModemLinkSection.tsx
//
// Modem-link section for any TNC-mediated mode (Packet today; ARDOP +
// VARA reuse it once those panels migrate). Per spec §5.2.
//
// Densified from `src/packet/PacketModemBlock` for the 360 px right-
// panel column. Renders a 3-way segmented picker (TCP / USB / BT) over
// the matching field set, and emits a flat ModemLinkFields object to
// the parent on every persist trigger (segment switch + blur on text
// fields). The parent owns the actual persist call (e.g.
// `invoke('packet_config_set', { dto })`).
//
// Shape note: serial USB and Bluetooth both produce `linkKind: 'Serial'`
// in the wire DTO — the segment is a UI affordance, not a wire field.
// The BT segment exists so the next iteration (BT-as-RFCOMM-MAC, see
// `packetTypes.ts` PacketLinkKind) can split out without re-shaping the
// section's props.

import { useEffect, useState } from 'react';
import './ModemLinkSection.css';

/** UI-segment identity. Bluetooth currently maps to the same `Serial`
 *  wire kind as USB; we keep the segment distinct for the picker
 *  affordance (and so the wire shape can split to `Bluetooth` later
 *  without re-wiring the section). */
type ModemSegment = 'tcp' | 'usb' | 'bt';

/** The flat field set emitted by `onChange`. Mirrors the PacketConfigDto
 *  subset that the modem editor owns; parent merges this into its config
 *  DTO and persists via `packet_config_set`. */
export interface ModemLinkFields {
  linkKind: 'Tcp' | 'Serial';
  tcpHost: string | null;
  tcpPort: number | null;
  serialDevice: string | null;
  serialBaud: number | null;
}

export interface ModemLinkSectionProps {
  /** Current link kind from the persisted config. `'Tcp'` shows the
   *  host+port editor; `'Serial'` shows the serial device+baud editor.
   *  The UI segment defaults to 'usb' when kind='Serial' (USB is the
   *  common case; the operator switches the segment to 'bt' explicitly). */
  kind: 'Tcp' | 'Serial';
  /** TCP host (used when kind='Tcp'). */
  host?: string;
  /** TCP port (used when kind='Tcp'). */
  port?: number;
  /** Serial device path (used when kind='Serial'). */
  serialDevice?: string;
  /** Serial host-link baud (used when kind='Serial'). */
  serialBaud?: number;
  /** Emit the flat field set after every persist trigger. */
  onChange: (fields: ModemLinkFields) => void;
}

const DEFAULT_TCP_HOST = '127.0.0.1';
const DEFAULT_TCP_PORT = 8001;
const DEFAULT_SERIAL_BAUD = 9600;

function initialSegment(kind: 'Tcp' | 'Serial'): ModemSegment {
  return kind === 'Tcp' ? 'tcp' : 'usb';
}

export function ModemLinkSection({
  kind,
  host,
  port,
  serialDevice,
  serialBaud,
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
  const baud = serialBaud ?? DEFAULT_SERIAL_BAUD;

  // Re-seed when props change underneath (parent loaded a new config).
  useEffect(() => {
    setSegment(initialSegment(kind));
    setHostInput(host ?? DEFAULT_TCP_HOST);
    setPortInput(String(port ?? DEFAULT_TCP_PORT));
    setDeviceInput(serialDevice ?? '');
  }, [kind, host, port, serialDevice]);

  const emit = (seg: ModemSegment) => {
    if (seg === 'tcp') {
      onChange({
        linkKind: 'Tcp',
        tcpHost: hostInput.trim() || DEFAULT_TCP_HOST,
        tcpPort: Number(portInput) || DEFAULT_TCP_PORT,
        serialDevice: null,
        serialBaud: null,
      });
    } else {
      onChange({
        linkKind: 'Serial',
        tcpHost: null,
        tcpPort: null,
        serialDevice: deviceInput.trim() || (serialDevice ?? null),
        serialBaud: baud,
      });
    }
  };

  const selectSegment = (seg: ModemSegment) => {
    setSegment(seg);
    emit(seg);
  };

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
      ) : (
        <>
          <label className="radio-panel-input-row">
            <span>{segment === 'bt' ? 'BT dev' : 'Device'}</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="modem-device"
              value={deviceInput}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder={segment === 'bt' ? '/dev/rfcomm0' : '/dev/ttyUSB0'}
              onChange={(e) => setDeviceInput(e.target.value)}
              onBlur={() => emit(segment)}
            />
          </label>
          <label className="radio-panel-input-row">
            <span>Baud</span>
            <input
              type="text"
              inputMode="numeric"
              className="radio-panel-input"
              data-testid="modem-baud"
              value={String(baud)}
              readOnly
            />
          </label>
        </>
      )}
    </section>
  );
}
