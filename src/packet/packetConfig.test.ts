// src/packet/packetConfig.test.ts
import { describe, it, expect } from 'vitest';
import { effectiveCall, ssidOptions, withSsid, withListen, withLinkKind } from './packetConfig';
import type { PacketConfigDto } from './packetTypes';

const baseDto: PacketConfigDto = {
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

describe('effectiveCall', () => {
  it('joins base + ssid as BASE-N (mock shows N7CPZ-7)', () => {
    expect(effectiveCall('N7CPZ', 7)).toBe('N7CPZ-7');
  });
  it('shows -0 as configured (no special-casing)', () => {
    expect(effectiveCall('N7CPZ', 0)).toBe('N7CPZ-0');
  });
  it('returns the bare base when base is empty (no dangling dash)', () => {
    expect(effectiveCall('', 7)).toBe('');
  });
});

describe('ssidOptions', () => {
  it('returns 0..15 inclusive (SSID range)', () => {
    expect(ssidOptions()).toEqual(Array.from({ length: 16 }, (_, i) => i));
  });
});

describe('immutable updaters', () => {
  it('withSsid returns a new dto with the ssid replaced, others intact', () => {
    const next = withSsid(baseDto, 10);
    expect(next.ssid).toBe(10);
    expect(next).not.toBe(baseDto);
    expect(next.tcpHost).toEqual(baseDto.tcpHost);
    expect(baseDto.ssid).toBe(7); // input not mutated
  });
  it('withListen toggles listenDefault without touching ssid/link', () => {
    const next = withListen(baseDto, false);
    expect(next.listenDefault).toBe(false);
    expect(next.ssid).toBe(7);
  });
  it('withLinkKind switches to Serial without mutating the original', () => {
    const next = withLinkKind(baseDto, 'Serial');
    expect(next.linkKind).toBe('Serial');
    expect(baseDto.linkKind).toBe('Tcp'); // original unchanged
  });
});
