import { describe, it, expect } from 'vitest';
import { deriveDrawerSessionState } from './drawerSessionState';

describe('deriveDrawerSessionState (F2/F13 — honest grip tick)', () => {
  it('connecting flag wins over everything', () => {
    expect(
      deriveDrawerSessionState({ connecting: true, status: { kind: 'Connected' }, modemIsActive: true }),
    ).toBe('connecting');
  });

  it('maps each transport status kind', () => {
    const base = { connecting: false, modemIsActive: false };
    expect(deriveDrawerSessionState({ ...base, status: { kind: 'Connecting' } })).toBe('connecting');
    // Listening (armed) surfaces amber, NOT a safe green
    expect(deriveDrawerSessionState({ ...base, status: { kind: 'Listening' } })).toBe('connecting');
    expect(deriveDrawerSessionState({ ...base, status: { kind: 'Disconnecting' } })).toBe('disconnecting');
    expect(deriveDrawerSessionState({ ...base, status: { kind: 'Error' } })).toBe('error');
    expect(deriveDrawerSessionState({ ...base, status: { kind: 'Connected' } })).toBe('connected');
  });

  it('an active modem with unknown status reads amber (cautious), never a safe green', () => {
    expect(
      deriveDrawerSessionState({ connecting: false, status: null, modemIsActive: true }),
    ).toBe('connecting');
    expect(
      deriveDrawerSessionState({ connecting: false, status: { kind: 'Weird' }, modemIsActive: true }),
    ).toBe('connecting');
  });

  it('idle with no signal is disconnected', () => {
    expect(
      deriveDrawerSessionState({ connecting: false, status: { kind: 'Disconnected' }, modemIsActive: false }),
    ).toBe('disconnected');
    expect(
      deriveDrawerSessionState({ connecting: false, status: undefined, modemIsActive: false }),
    ).toBe('disconnected');
  });
});
