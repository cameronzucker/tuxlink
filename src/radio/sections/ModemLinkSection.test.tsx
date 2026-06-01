// src/radio/sections/ModemLinkSection.test.tsx
//
// Spec §5.2 — modem-link section for any TNC-mediated mode. The Packet
// panel is the first consumer; ARDOP / VARA will reuse this section in
// future phases. The section densifies the existing PacketModemBlock
// content for the 360 px right-panel column and emits flat fields via
// onChange (the parent persists via packet_config_set).

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ModemLinkSection } from './ModemLinkSection';

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
    expect(screen.getByDisplayValue('/dev/ttyUSB0')).toBeInTheDocument();
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

  it('switches TCP → Serial and emits null tcpHost/tcpPort', () => {
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
    });
  });
});
