// src/packet/PacketConnectionPanel.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
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

  it('selecting USB serial swaps host:port for a device input', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.click(screen.getByTestId('modem-seg-usb'));
    expect(screen.getByTestId('modem-seg-usb')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByTestId('modem-host')).toBeNull();
    expect(screen.getByTestId('modem-device')).toBeInTheDocument();
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
// Task 6: Listen toggle
// ---------------------------------------------------------------------------
describe('<PacketConnectionPanel> — listen toggle', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it('reflects listenDefault and shows the effective call in the listen label', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    const sw = screen.getByTestId('listen-switch');
    expect(sw).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('listen-label')).toHaveTextContent('Listening as N7CPZ-7');
  });

  it('toggling off calls packet_set_listen(false)', () => {
    render(<PacketConnectionPanel config={cfg} baseCall="N7CPZ" />);
    fireEvent.click(screen.getByTestId('listen-switch'));
    expect(screen.getByTestId('listen-switch')).toHaveAttribute('aria-checked', 'false');
    expect(invoke).toHaveBeenCalledWith('packet_set_listen', { enabled: false });
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
    expect(screen.getByTestId('relay-chip-0')).toHaveTextContent('W7RPT-1');
    expect(screen.getByTestId('relay-chip-1')).toHaveTextContent('W7XYZ-2');
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
