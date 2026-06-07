// src/radio/sections/ModemLinkSection.test.tsx
//
// Spec §5.2 — modem-link section for any TNC-mediated mode. The Packet
// panel is the first consumer; ARDOP / VARA will reuse this section in
// future phases. The section densifies the existing PacketModemBlock
// content for the 360 px right-panel column and emits flat fields via
// onChange (the parent persists via packet_config_set).
//
// tuxlink-mqu3: USB and BT segments load device lists from the backend
// (`packet_list_serial_devices`, `packet_list_bluetooth_devices`) and
// render dropdowns. Tauri `invoke` is mocked module-wide; per-test
// `mockResolvedValueOnce` shapes the response for the segment under test.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ModemLinkSection } from './ModemLinkSection';

// Mock Tauri so the device-discovery commands resolve to empty by default;
// per-test overrides via `mockResolvedValueOnce` shape the response.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => []),
}));
import { invoke } from '@tauri-apps/api/core';

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue([]);
});

describe('<ModemLinkSection>', () => {
  it('renders the TCP/USB/BT segmented picker', () => {
    render(
      <ModemLinkSection
        kind="Tcp"
        host="127.0.0.1"
        port={8001}
        onChange={() => {}}
      />,
    );
    expect(screen.getByRole('button', { name: /TCP/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /USB/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /BT/ })).toBeInTheDocument();
  });

  it('fires onChange with the new kind when a segment is clicked', () => {
    const onChange = vi.fn();
    render(
      <ModemLinkSection
        kind="Tcp"
        host="127.0.0.1"
        port={8001}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /USB/ }));
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ linkKind: 'Serial' }),
    );
  });

  it('shows TCP host + port when kind=Tcp', () => {
    render(
      <ModemLinkSection
        kind="Tcp"
        host="127.0.0.1"
        port={8001}
        onChange={() => {}}
      />,
    );
    expect(screen.getByDisplayValue('127.0.0.1')).toBeInTheDocument();
    expect(screen.getByDisplayValue('8001')).toBeInTheDocument();
  });

  it('shows serial device + baud when kind=Serial', () => {
    render(
      <ModemLinkSection
        kind="Serial"
        serialDevice="/dev/ttyUSB0"
        serialBaud={9600}
        onChange={() => {}}
      />,
    );
    // The manual-fallback input echoes the persisted path.
    expect(screen.getByTestId('modem-device')).toHaveValue('/dev/ttyUSB0');
    expect(screen.getByText('Serial baud')).toBeInTheDocument();
    expect(screen.getByTestId('modem-baud-help')).toHaveTextContent(
      /host-link rate, not the AX\.25 over-air packet rate/i,
    );
    // Baud is now a <select> — the selected option's value is reflected in
    // the select element's displayValue.
    const baudSelect = screen.getByTestId('modem-baud') as HTMLSelectElement;
    expect(baudSelect.value).toBe('9600');
  });

  it('baud select defaults to 1200 (the common TNC default) when no serialBaud is provided', () => {
    render(
      <ModemLinkSection
        kind="Serial"
        serialDevice="/dev/ttyUSB0"
        onChange={() => {}}
      />,
    );
    const baudSelect = screen.getByTestId('modem-baud') as HTMLSelectElement;
    expect(baudSelect.value).toBe('1200');
  });

  it('baud select exposes the standard TNC baud ladder', () => {
    render(
      <ModemLinkSection
        kind="Serial"
        serialDevice="/dev/ttyUSB0"
        onChange={() => {}}
      />,
    );
    const baudSelect = screen.getByTestId('modem-baud') as HTMLSelectElement;
    const values = Array.from(baudSelect.options).map((o) => o.value);
    expect(values).toEqual(['1200', '2400', '4800', '9600', '19200', '38400', '57600', '115200']);
  });

  it('changing baud fires onChange with the new serialBaud immediately', () => {
    const onChange = vi.fn();
    render(
      <ModemLinkSection
        kind="Serial"
        serialDevice="/dev/ttyUSB0"
        serialBaud={1200}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByTestId('modem-baud'), { target: { value: '9600' } });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        linkKind: 'Serial',
        serialBaud: 9600,
      }),
    );
  });

  it('persists TCP host edits via onChange on blur', () => {
    const onChange = vi.fn();
    render(
      <ModemLinkSection
        kind="Tcp"
        host="127.0.0.1"
        port={8001}
        onChange={onChange}
      />,
    );
    const hostInput = screen.getByDisplayValue('127.0.0.1') as HTMLInputElement;
    fireEvent.change(hostInput, { target: { value: '10.0.0.5' } });
    fireEvent.blur(hostInput);
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        linkKind: 'Tcp',
        tcpHost: '10.0.0.5',
        tcpPort: 8001,
      }),
    );
  });

  it('switches TCP → Serial and emits null tcpHost/tcpPort + null btMac', () => {
    const onChange = vi.fn();
    render(
      <ModemLinkSection
        kind="Tcp"
        host="127.0.0.1"
        port={8001}
        serialDevice="/dev/ttyUSB0"
        serialBaud={9600}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /USB/ }));
    expect(onChange).toHaveBeenLastCalledWith({
      linkKind: 'Serial',
      tcpHost: null,
      tcpPort: null,
      serialDevice: '/dev/ttyUSB0',
      serialBaud: 9600,
      btMac: null,
    });
  });

  // ─────────────────────────────────────────────────────────────────────
  // tuxlink-mqu3: picker UX restoration. USB segment populates a dropdown
  // from `packet_list_serial_devices` (USB-class entries only); BT segment
  // populates from `packet_list_bluetooth_devices` (paired devices). Each
  // segment carries a Refresh button + a manual-text fallback. The BT
  // segment emits `linkKind: 'Bluetooth'` + `btMac` (not serialDevice).

  it('USB segment fetches packet_list_serial_devices on activation and shows USB-class devices', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      { path: '/dev/ttyUSB0', kind: 'usb', label: 'USB serial' },
      // UART entries are returned by the backend but the USB segment filters
      // them out — the manual fallback is the escape hatch for GPIO-KISS.
      { path: '/dev/ttyAMA0', kind: 'uart', label: 'On-board UART' },
      { path: '/dev/ttyACM0', kind: 'usb', label: 'USB serial' },
    ]);
    render(
      <ModemLinkSection
        kind="Serial"
        onChange={() => {}}
      />,
    );
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('packet_list_serial_devices');
    });
    const usbSelect = await screen.findByTestId('modem-usb-select') as HTMLSelectElement;
    const values = Array.from(usbSelect.options).map((o) => o.value);
    expect(values).toContain('/dev/ttyUSB0');
    expect(values).toContain('/dev/ttyACM0');
    expect(values).not.toContain('/dev/ttyAMA0');
  });

  it('selecting a USB device from the dropdown emits the path via onChange', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      { path: '/dev/ttyUSB0', kind: 'usb', label: 'USB serial' },
    ]);
    const onChange = vi.fn();
    render(
      <ModemLinkSection
        kind="Serial"
        serialBaud={1200}
        onChange={onChange}
      />,
    );
    const usbSelect = await screen.findByTestId('modem-usb-select') as HTMLSelectElement;
    await waitFor(() => {
      expect(Array.from(usbSelect.options).map((o) => o.value)).toContain('/dev/ttyUSB0');
    });
    fireEvent.change(usbSelect, { target: { value: '/dev/ttyUSB0' } });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        linkKind: 'Serial',
        serialDevice: '/dev/ttyUSB0',
        btMac: null,
      }),
    );
  });

  it('USB Refresh re-invokes packet_list_serial_devices', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    render(
      <ModemLinkSection
        kind="Serial"
        onChange={() => {}}
      />,
    );
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('packet_list_serial_devices');
    });
    const callsBefore = vi.mocked(invoke).mock.calls.filter(
      (c) => c[0] === 'packet_list_serial_devices',
    ).length;
    fireEvent.click(screen.getByTestId('modem-usb-refresh'));
    await waitFor(() => {
      const callsAfter = vi.mocked(invoke).mock.calls.filter(
        (c) => c[0] === 'packet_list_serial_devices',
      ).length;
      expect(callsAfter).toBe(callsBefore + 1);
    });
  });

  it('BT segment fetches packet_list_bluetooth_devices on activation and shows paired devices', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      { mac: '38:D2:00:01:55:5C', name: 'UV-PRO' },
      { mac: 'AA:BB:CC:DD:EE:FF', name: 'Some Other Radio' },
    ]);
    render(
      <ModemLinkSection
        kind="Bluetooth"
        onChange={() => {}}
      />,
    );
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('packet_list_bluetooth_devices');
    });
    const btSelect = await screen.findByTestId('modem-bt-select') as HTMLSelectElement;
    const values = Array.from(btSelect.options).map((o) => o.value);
    expect(values).toContain('38:D2:00:01:55:5C');
    expect(values).toContain('AA:BB:CC:DD:EE:FF');
  });

  it('selecting a Bluetooth device emits linkKind=Bluetooth + btMac (NOT serialDevice)', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      { mac: '38:D2:00:01:55:5C', name: 'UV-PRO' },
    ]);
    const onChange = vi.fn();
    render(
      <ModemLinkSection
        kind="Bluetooth"
        onChange={onChange}
      />,
    );
    const btSelect = await screen.findByTestId('modem-bt-select') as HTMLSelectElement;
    await waitFor(() => {
      expect(Array.from(btSelect.options).map((o) => o.value)).toContain('38:D2:00:01:55:5C');
    });
    fireEvent.change(btSelect, { target: { value: '38:D2:00:01:55:5C' } });
    // This is the tuxlink-mqu3 win: BT is its own wire kind, not a Serial
    // mis-conflation, and the MAC is on btMac (not serialDevice).
    expect(onChange).toHaveBeenCalledWith({
      linkKind: 'Bluetooth',
      tcpHost: null,
      tcpPort: null,
      serialDevice: null,
      serialBaud: null,
      btMac: '38:D2:00:01:55:5C',
    });
  });

  it('switching to BT segment emits linkKind=Bluetooth and clears serial fields', async () => {
    const onChange = vi.fn();
    render(
      <ModemLinkSection
        kind="Tcp"
        host="127.0.0.1"
        port={8001}
        serialDevice="/dev/ttyUSB0"
        serialBaud={9600}
        btMac="38:D2:00:01:55:5C"
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /BT/ }));
    expect(onChange).toHaveBeenLastCalledWith({
      linkKind: 'Bluetooth',
      tcpHost: null,
      tcpPort: null,
      serialDevice: null,
      serialBaud: null,
      btMac: '38:D2:00:01:55:5C',
    });
  });

  it('BT segment renders manual-text fallback for unpaired devices', () => {
    render(
      <ModemLinkSection
        kind="Bluetooth"
        btMac="38:D2:00:01:55:5C"
        onChange={() => {}}
      />,
    );
    // The manual MAC input shows the current persisted value so the operator
    // can edit it directly when a paired device isn't enumerated.
    const manual = screen.getByTestId('modem-bt-mac') as HTMLInputElement;
    expect(manual.value).toBe('38:D2:00:01:55:5C');
  });

  it('BT segment empty list shows the pair-a-radio hint', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    render(
      <ModemLinkSection
        kind="Bluetooth"
        onChange={() => {}}
      />,
    );
    const btSelect = await screen.findByTestId('modem-bt-select') as HTMLSelectElement;
    await waitFor(() => {
      expect(btSelect.options[0].textContent).toMatch(/pair a radio/i);
    });
  });
});
