import { describe, it, expect } from 'vitest';
import type { ModemStatus } from './types';
import { STOPPED } from './types';

describe('ModemStatus wire contract', () => {
  it('STOPPED matches the documented stopped shape from modem_status.rs', () => {
    expect(STOPPED.state).toBe('stopped');
    expect(STOPPED.peer).toBeNull();
    expect(STOPPED.arqFlags).toEqual({ busy: false, rx: false, tx: false });
  });

  it('accepts a sample connected-irs payload from the Rust serialization fixture', () => {
    const wire = {
      state: 'connected-irs',
      peer: 'W7RMS-10',
      mode: '4FSK 500',
      widthHz: 500,
      pttBackend: 'rts',
      snDb: 8.4, vuDbfs: -18.0, throughputBps: 540,
      bytesRx: 4128, bytesTx: 982, uptimeSec: 222,
      arqFlags: { busy: true, rx: true, tx: false },
      lastError: null,
    } as ModemStatus;
    expect(wire.state).toBe('connected-irs');
    expect(wire.peer).toBe('W7RMS-10');
    // Reviewer-requested explicit camelCase rename coverage:
    expect(wire.widthHz).toBe(500);
    expect(wire.snDb).toBeCloseTo(8.4, 5);
    expect(wire.bytesRx).toBe(4128);
  });
});
