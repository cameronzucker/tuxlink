import { describe, it, expect } from 'vitest';
import { asUiError, isNotConfigured, type UiError } from './types';

describe('asUiError', () => {
  it('narrows a discriminated-union-shaped value to UiError', () => {
    const e: UiError = { kind: 'AuthFailed', detail: { reason: '401' } };
    expect(asUiError(e)).toBe(e);
  });

  it('returns null for non-UiError values', () => {
    expect(asUiError('boom')).toBeNull();
    expect(asUiError(new Error('x'))).toBeNull();
    expect(asUiError(null)).toBeNull();
    expect(asUiError(42)).toBeNull();
  });
});

describe('isNotConfigured', () => {
  it('is true only for the NotConfigured kind', () => {
    expect(isNotConfigured({ kind: 'NotConfigured', detail: 'offline' })).toBe(true);
    expect(isNotConfigured({ kind: 'Transport', detail: { reason: 'x' } })).toBe(false);
    expect(isNotConfigured(new Error('x'))).toBe(false);
    expect(isNotConfigured(null)).toBe(false);
  });
});
