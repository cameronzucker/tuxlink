import { describe, it, expect } from 'vitest';
import { catalogErrorMessage } from './stationTypes';

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
