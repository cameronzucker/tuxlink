// src/packet/packetStatus.test.ts
import { describe, it, expect } from 'vitest';
import {
  formatPacketConnection,
  formatPacketStatusBar,
  derivePacketUiState,
  type PacketUiState,
} from './packetStatus';
import type { StatusDto } from '../shell/useStatus';

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

describe('derivePacketUiState (tuxlink-orj — live backend status → indicator)', () => {
  it('Listening status → listening, and active even when the panel is not selected', () => {
    const s = derivePacketUiState({ kind: 'Listening', transport: 'Packet-7' }, false, 'N7CPZ-7');
    expect(s.listening).toBe(true);
    expect(s.connected).toBe(false);
    expect(s.active).toBe(true);
  });
  it('Connected with a packet transport → connected', () => {
    const s: StatusDto = { kind: 'Connected', transport: 'Packet-7', peer: 'W7AUX', since_iso: '' };
    expect(derivePacketUiState(s, false, 'N7CPZ-7').connected).toBe(true);
  });
  it('Connected with a CMS transport → NOT a packet connection', () => {
    const s: StatusDto = { kind: 'Connected', transport: 'CMS-CmsSsl', peer: 'cms', since_iso: '' };
    const ui = derivePacketUiState(s, false, 'N7CPZ-7');
    expect(ui.connected).toBe(false);
    expect(ui.active).toBe(false);
  });
  it('panel selected with no live packet state → active but not listening/connected', () => {
    expect(derivePacketUiState(null, true, 'N7CPZ-7')).toMatchObject({
      active: true,
      listening: false,
      connected: false,
      effectiveCall: 'N7CPZ-7',
    });
  });
  it('neither selected nor a live packet state → inactive', () => {
    expect(derivePacketUiState({ kind: 'Disconnected' }, false, 'N7CPZ-7').active).toBe(false);
  });
});
