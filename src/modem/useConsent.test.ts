import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useConsent } from './useConsent';

describe('useConsent', () => {
  it('starts with no token; granted() returns the token after grant', () => {
    const { result } = renderHook(() => useConsent());
    expect(result.current.token).toBeNull();
    act(() => { result.current.grant('abc123'); });
    expect(result.current.token).toBe('abc123');
  });

  it('clear() wipes the token', () => {
    const { result } = renderHook(() => useConsent());
    act(() => { result.current.grant('abc123'); });
    act(() => { result.current.clear(); });
    expect(result.current.token).toBeNull();
  });
});
