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
    expect(screen.getByDisplayValue('9600')).toBeInTheDocument();
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
