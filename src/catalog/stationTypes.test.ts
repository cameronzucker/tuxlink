import { describe, it, expect } from 'vitest';
import { catalogErrorMessage, bandwidthClass } from './stationTypes';

// catalogErrorMessage is the real error-extraction logic; the hooks just call setError(it).
// Unit-test every UiError wire shape ({ kind, content="detail" } per ui_commands.rs).
describe('catalogErrorMessage', () => {
  it('reads .detail.reason for Transport/Unavailable/AuthFailed', () => {
    expect(catalogErrorMessage({ kind: 'Transport', detail: { reason: 'boom' } })).toBe('boom');
    expect(catalogErrorMessage({ kind: 'Unavailable', detail: { reason: 'down' } })).toBe('down');
    expect(catalogErrorMessage({ kind: 'AuthFailed', detail: { reason: 'no' } })).toBe('no');
  });

  it('reads the string detail for NotConfigured/NotFound/Rejected', () => {
    expect(catalogErrorMessage({ kind: 'NotConfigured', detail: 'offline' })).toBe('offline');
    expect(catalogErrorMessage({ kind: 'Rejected', detail: 'nope' })).toBe('nope');
  });

  it('reads the nested .detail.detail for Internal', () => {
    expect(catalogErrorMessage({ kind: 'Internal', detail: { detail: 'oops' } })).toBe('oops');
  });

  it('falls back to Error.message / String for non-UiError throws', () => {
    expect(catalogErrorMessage(new Error('plain'))).toBe('plain');
    expect(catalogErrorMessage('a string')).toBe('a string');
  });
});

describe('bandwidthClass (Task 9)', () => {
  it('classifies the three known VARA bandwidths', () => {
    expect(bandwidthClass(500)).toBe('500');
    expect(bandwidthClass(2300)).toBe('2300');
    expect(bandwidthClass(2750)).toBe('2750');
  });

  it('returns null for null/undefined (the unknown-bandwidth case)', () => {
    expect(bandwidthClass(null)).toBeNull();
    expect(bandwidthClass(undefined)).toBeNull();
  });

  it('returns null for a known-but-unclassified bandwidth (e.g. ARDOP 1000/2000 Hz)', () => {
    expect(bandwidthClass(1000)).toBeNull();
    expect(bandwidthClass(2000)).toBeNull();
    expect(bandwidthClass(0)).toBeNull();
  });
});
