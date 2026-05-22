// src/packet/packetTypes.test.ts
// Contract test: constructing a valid PacketConfigDto with all P3 fields.
// If a field name drifts from the P3 wire contract, this fails to compile
// and `vitest run` errors.
import { describe, it, expect } from 'vitest';
import type { PacketConfigDto } from './packetTypes';

describe('packetTypes — P3 contract mirror (flat camelCase)', () => {
  it('constructs a valid PacketConfigDto with all P3 fields (TCP link)', () => {
    const dto: PacketConfigDto = {
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
    expect(dto.ssid).toBe(7);
    expect(dto.linkKind).toBe('Tcp');
    expect(dto.listenDefault).toBe(true);
    expect(dto.tcpHost).toBe('127.0.0.1');
    expect(dto.tcpPort).toBe(8001);
  });

  it('accepts a Serial link (USB or Bluetooth-RFCOMM both use linkKind:"Serial")', () => {
    const dto: PacketConfigDto = {
      ssid: 3,
      listenDefault: false,
      linkKind: 'Serial',
      tcpHost: null,
      tcpPort: null,
      serialDevice: '/dev/rfcomm0',
      serialBaud: 9600,
      txdelay: 30,
      persistence: 63,
      slotTime: 10,
      paclen: 128,
      maxframe: 4,
      t1Ms: 3000,
      n2Retries: 10,
    };
    expect(dto.linkKind).toBe('Serial');
    expect(dto.serialDevice).toBe('/dev/rfcomm0');
    expect(dto.serialBaud).toBe(9600);
  });

  it('accepts null link fields when no link is configured', () => {
    const dto: PacketConfigDto = {
      ssid: 0,
      listenDefault: true,
      linkKind: null,
      tcpHost: null,
      tcpPort: null,
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
    expect(dto.linkKind).toBeNull();
  });
});
