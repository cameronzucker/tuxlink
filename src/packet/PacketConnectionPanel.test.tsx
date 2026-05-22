// src/packet/PacketConnectionPanel.test.tsx
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));

import { invoke } from '@tauri-apps/api/core';
import { PacketConnectionPanel } from './PacketConnectionPanel';
import type { PacketConfigDto } from './packetTypes';

const cfg: PacketConfigDto = {
  ssid: 7,
  listenDefault: true,
  linkKind: 'Tcp',
  tcpHost: '127.0.0.1',
  tcpPort: 8001,
  serialDevice: null,
  serialBaud: null,
  txdelay: 30,
  persistence: 63,
  slotTime: 10,
  paclen: 128,
  maxframe: 4,
  t1Ms: 3000,
  n2Retries: 10,
};

// ---------------------------------------------------------------------------
// Task 3: Header / skeleton
// ---------------------------------------------------------------------------
describe('<PacketConnectionPanel> — header', () => {
  it('renders the panel in a reading-pane root with title + 1200 baud badge', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    const root = screen.getByTestId('packet-panel-root');
    expect(root).toBeInTheDocument();
    expect(root.className).toContain('reading-pane');
    expect(screen.getByTestId('packet-panel-title')).toHaveTextContent('Packet (AX.25)');
    expect(screen.getByTestId('packet-panel-badge')).toHaveTextContent('1200 baud');
  });
});

// ---------------------------------------------------------------------------
// Task 4: Modem block
// ---------------------------------------------------------------------------
describe('<PacketConnectionPanel> — modem block', () => {
  it('shows three transport segments; TCP active for a tcp link with host:port inputs', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    expect(screen.getByTestId('modem-seg-tcp')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('modem-seg-usb')).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByTestId('modem-seg-bt')).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByTestId('modem-host')).toHaveValue('127.0.0.1');
    expect(screen.getByTestId('modem-port')).toHaveValue('8001');
  });

  it('selecting USB serial swaps host:port for a device input (no 127.0.0.1 leak)', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.click(screen.getByTestId('modem-seg-usb'));
    expect(screen.getByTestId('modem-seg-usb')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByTestId('modem-host')).toBeNull();
    const device = screen.getByTestId('modem-device');
    expect(device).toBeInTheDocument();
    // Regression (the bug the operator hit): the TCP host (127.0.0.1) must NOT
    // leak into the device field when switching transports. Controlled inputs.
    expect(device).toHaveValue('');
    expect(device).toHaveAttribute('placeholder', '/dev/ttyUSB0');
  });

  it('Bluetooth shows an rfcomm device path placeholder, not an IP', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.click(screen.getByTestId('modem-seg-bt'));
    const device = screen.getByTestId('modem-device');
    expect(device).toHaveValue('');
    expect(device).toHaveAttribute('placeholder', '/dev/rfcomm0');
  });

  it("shows a serial link's device when config.linkKind is Serial", () => {
    const serialCfg: PacketConfigDto = {
      ...cfg,
      linkKind: 'Serial',
      tcpHost: null,
      tcpPort: null,
      serialDevice: '/dev/ttyUSB0',
      serialBaud: 9600,
    };
    render(<PacketConnectionPanel config={serialCfg} baseCall="N7CPZ" />);
    expect(screen.getByTestId('modem-seg-tcp')).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByTestId('modem-device')).toHaveValue('/dev/ttyUSB0');
  });
});

// ---------------------------------------------------------------------------
// Task 5: My-station block
// ---------------------------------------------------------------------------
import { effectiveCall } from './packetConfig';

describe('<PacketConnectionPanel> — my station / SSID', () => {
  it('shows the base call from identity and SSID select reflecting config.ssid', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    expect(screen.getByTestId('station-base')).toHaveValue('N7CPZ');
    expect(screen.getByTestId('station-ssid')).toHaveValue('7');
  });

  it('displays the effective call N7CPZ-7 and updates it when SSID changes', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    expect(screen.getByTestId('station-effective')).toHaveTextContent(effectiveCall('N7CPZ', 7));
    fireEvent.change(screen.getByTestId('station-ssid'), { target: { value: '10' } });
    expect(screen.getByTestId('station-effective')).toHaveTextContent('N7CPZ-10');
  });

  it('offers all 16 SSID options (0..15)', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    const opts = screen.getByTestId('station-ssid').querySelectorAll('option');
    expect(opts).toHaveLength(16);
  });
});

// ---------------------------------------------------------------------------
// Task 6: real Listen action + honest armed state
// ---------------------------------------------------------------------------
describe('<PacketConnectionPanel> — Listen action', () => {
  beforeEach(() => {
    // A controllable packet_listen: resolves only when we let it, so the armed
    // state is observable while the call is "in flight" (cms_abort/complete clear it).
    vi.mocked(invoke).mockImplementation(async () => undefined);
  });
  afterEach(() => {
    vi.mocked(invoke).mockImplementation(async () => undefined);
  });

  it('is NOT listening by default — the label never claims "Listening" until armed', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    const action = screen.getByTestId('listen-action');
    expect(action).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByTestId('listen-label')).toHaveTextContent('Listen for an incoming call');
    // The honest-state guard: idle must NOT say "Listening as ...".
    expect(screen.getByTestId('listen-label')).not.toHaveTextContent(/Listening as/);
    expect(screen.getByTestId('listen-label')).not.toHaveTextContent(/Waiting for a call/);
  });

  it('clicking arm invokes packet_listen and reflects the armed/waiting state', async () => {
    // Hold packet_listen pending so we can observe the armed state.
    let resolveListen!: () => void;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_listen') {
        return new Promise<void>((res) => {
          resolveListen = () => res(undefined);
        });
      }
      return undefined;
    });
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.click(screen.getByTestId('listen-action'));
    expect(invoke).toHaveBeenCalledWith('packet_listen');
    await waitFor(() => {
      expect(screen.getByTestId('listen-action')).toHaveAttribute('aria-pressed', 'true');
      expect(screen.getByTestId('listen-label')).toHaveTextContent(
        'Waiting for a call as N7CPZ-7 — Stop',
      );
    });
    // Completing the exchange disarms (honest: no longer waiting).
    resolveListen();
    await waitFor(() => {
      expect(screen.getByTestId('listen-action')).toHaveAttribute('aria-pressed', 'false');
    });
  });

  it('clicking Stop while armed invokes the abort command (cms_abort) and disarms', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_listen') {
        return new Promise<void>(() => {}); // never resolves on its own
      }
      return undefined;
    });
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.click(screen.getByTestId('listen-action')); // arm
    await waitFor(() =>
      expect(screen.getByTestId('listen-action')).toHaveAttribute('aria-pressed', 'true'),
    );
    fireEvent.click(screen.getByTestId('listen-action')); // Stop
    expect(invoke).toHaveBeenCalledWith('cms_abort');
    await waitFor(() =>
      expect(screen.getByTestId('listen-action')).toHaveAttribute('aria-pressed', 'false'),
    );
  });

  it('listenDefault is a preference (auto-arm on startup), not a live-state claim', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    const pref = screen.getByTestId('listen-default-toggle');
    expect(pref).toBeChecked(); // cfg.listenDefault = true
    fireEvent.click(pref);
    expect(invoke).toHaveBeenCalledWith('packet_set_listen', { enabled: false });
    // Toggling the preference must NOT arm the live listener.
    expect(screen.getByTestId('listen-action')).toHaveAttribute('aria-pressed', 'false');
  });
});

// ---------------------------------------------------------------------------
// Task 7: Connect block
// ---------------------------------------------------------------------------
describe('<PacketConnectionPanel> — connect block', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it('Connect button names the target call entered in Connect to', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.change(screen.getByTestId('connect-to'), { target: { value: 'W7AUX-10' } });
    expect(screen.getByTestId('packet-connect-btn')).toHaveTextContent('Connect to W7AUX-10');
  });

  it('adds up to 2 relay chips then hides the add affordance at the cap', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.click(screen.getByTestId('add-relay'));
    fireEvent.change(screen.getByTestId('relay-input-0'), { target: { value: 'W7RPT-1' } });
    fireEvent.click(screen.getByTestId('add-relay'));
    fireEvent.change(screen.getByTestId('relay-input-1'), { target: { value: 'W7XYZ-2' } });
    // The value lives in the chip's <input> (no duplicate label span — that
    // double-printed the operator's text). Assert on the input value.
    expect(screen.getByTestId('relay-input-0')).toHaveValue('W7RPT-1');
    expect(screen.getByTestId('relay-input-1')).toHaveValue('W7XYZ-2');
    expect(screen.queryByTestId('add-relay')).toBeNull(); // capped at 2
  });

  it('Connect fires packet_connect(call, path) with the relay path', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.change(screen.getByTestId('connect-to'), { target: { value: 'W7AUX-10' } });
    fireEvent.click(screen.getByTestId('add-relay'));
    fireEvent.change(screen.getByTestId('relay-input-0'), { target: { value: 'W7RPT-1' } });
    fireEvent.click(screen.getByTestId('packet-connect-btn'));
    expect(invoke).toHaveBeenCalledWith('packet_connect', { call: 'W7AUX-10', path: ['W7RPT-1'] });
  });
});

// ---------------------------------------------------------------------------
// Device picker (USB / Bluetooth) — enumerates real devices from the backend,
// never an IP. Regression coverage for the "no real device selector" gap.
// ---------------------------------------------------------------------------
describe('<PacketConnectionPanel> — device picker', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === 'packet_list_serial_devices'
        ? [
            { path: '/dev/ttyUSB0', kind: 'usb', label: 'USB serial' },
            { path: '/dev/rfcomm0', kind: 'bluetooth', label: 'Bluetooth (RFCOMM)' },
          ]
        : undefined,
    );
  });
  afterEach(() => {
    vi.mocked(invoke).mockImplementation(async () => undefined);
  });

  it('USB tab lists only USB/serial devices (labeled), not the Bluetooth one', async () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.click(screen.getByTestId('modem-seg-usb'));
    expect(invoke).toHaveBeenCalledWith('packet_list_serial_devices');
    await waitFor(() => {
      expect(
        screen.getByRole('option', { name: /\/dev\/ttyUSB0 — USB serial/ }),
      ).toBeInTheDocument();
    });
    // No conflation: the Bluetooth device must NOT appear under the USB tab.
    expect(screen.queryByRole('option', { name: /rfcomm0/ })).toBeNull();
  });

  it('Bluetooth tab lists only RFCOMM devices; selecting one persists a Serial link', async () => {
    const onLinkPersist = vi.fn();
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" onLinkPersist={onLinkPersist} />);
    fireEvent.click(screen.getByTestId('modem-seg-bt'));
    await waitFor(() => screen.getByRole('option', { name: /\/dev\/rfcomm0/ }));
    // No conflation: the USB device must NOT appear under the Bluetooth tab.
    expect(screen.queryByRole('option', { name: /ttyUSB0/ })).toBeNull();
    fireEvent.change(screen.getByTestId('modem-device-select'), {
      target: { value: '/dev/rfcomm0' },
    });
    expect(onLinkPersist).toHaveBeenCalledWith(
      expect.objectContaining({ linkKind: 'Serial', serialDevice: '/dev/rfcomm0' }),
    );
  });
});

// ---------------------------------------------------------------------------
// Task 8: Container
// ---------------------------------------------------------------------------
import { withSsid } from './packetConfig';

describe('PacketConnectionPanelContainer — config IPC', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it('loads config via packet_config_get and seeds the SSID select', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === 'packet_config_get' ? cfg : undefined,
    );
    const { PacketConnectionPanelContainer } = await import('./PacketConnectionPanel');
    render(<PacketConnectionPanelContainer baseCall="N7CPZ" />);
    await waitFor(() => expect(screen.getByTestId('station-ssid')).toHaveValue('7'));
    expect(invoke).toHaveBeenCalledWith('packet_config_get');
  });

  it('persists an SSID change via packet_config_set (global sticky)', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === 'packet_config_get' ? cfg : undefined,
    );
    const { PacketConnectionPanelContainer } = await import('./PacketConnectionPanel');
    render(<PacketConnectionPanelContainer baseCall="N7CPZ" />);
    await waitFor(() => expect(screen.getByTestId('station-ssid')).toHaveValue('7'));
    fireEvent.change(screen.getByTestId('station-ssid'), { target: { value: '10' } });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('packet_config_set', { dto: withSsid(cfg, 10) }),
    );
  });
});
