// src/packet/packetStatus.test.ts
import { describe, it, expect } from 'vitest';
import { formatPacketConnection, formatPacketStatusBar, type PacketUiState } from './packetStatus';

const listening: PacketUiState = {
  active: true, listening: true, connected: false,
  effectiveCall: 'N7CPZ-7', linkLabel: 'KISS-TCP Dire Wolf',
};
const connected: PacketUiState = { ...listening, connected: true };
const idle: PacketUiState = {
  active: false, listening: false, connected: false,
  effectiveCall: 'N7CPZ-7', linkLabel: '',
};

describe('formatPacketConnection (ribbon)', () => {
  it('listening → "Listening · Packet 1200" with good tone', () => {
    expect(formatPacketConnection(listening)).toEqual({ label: 'Listening · Packet 1200', tone: 'good' });
  });
  it('connected → "Connected · Packet 1200" with good tone', () => {
    expect(formatPacketConnection(connected)).toEqual({ label: 'Connected · Packet 1200', tone: 'good' });
  });
  it('inactive → null (ribbon falls back to the CMS connection string)', () => {
    expect(formatPacketConnection(idle)).toBeNull();
  });
});

describe('formatPacketStatusBar', () => {
  it('listening → "Packet 1200 · Listening as N7CPZ-7 · KISS-TCP Dire Wolf"', () => {
    expect(formatPacketStatusBar(listening)).toEqual({
      label: 'Packet 1200 · Listening as N7CPZ-7 · KISS-TCP Dire Wolf', tone: 'good',
    });
  });
  it('connected → "Packet 1200 · Connected as N7CPZ-7 · KISS-TCP Dire Wolf"', () => {
    expect(formatPacketStatusBar(connected)).toEqual({
      label: 'Packet 1200 · Connected as N7CPZ-7 · KISS-TCP Dire Wolf', tone: 'good',
    });
  });
  it('inactive → null', () => {
    expect(formatPacketStatusBar(idle)).toBeNull();
  });
});
