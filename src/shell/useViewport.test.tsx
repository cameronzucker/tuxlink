import { describe, it, expect, vi, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useViewport, COMPACT_MEDIA_QUERY } from './useViewport';

// jsdom has no real matchMedia; install a controllable mock.
function installMatchMedia(initialMatches: boolean) {
  const listeners = new Set<(e: MediaQueryListEvent) => void>();
  const mql = {
    matches: initialMatches,
    media: COMPACT_MEDIA_QUERY,
    addEventListener: (_: string, cb: (e: MediaQueryListEvent) => void) => listeners.add(cb),
    removeEventListener: (_: string, cb: (e: MediaQueryListEvent) => void) => listeners.delete(cb),
  };
  vi.stubGlobal('matchMedia', (q: string) => {
    expect(q).toBe(COMPACT_MEDIA_QUERY); // the hook MUST use the shared constant
    return mql;
  });
  return {
    fire(matches: boolean) {
      mql.matches = matches;
      listeners.forEach((cb) => cb({ matches } as MediaQueryListEvent));
    },
  };
}

afterEach(() => vi.unstubAllGlobals());

describe('useViewport', () => {
  it('exports the canonical compact media query string (strictly below the 1366px desktop floor, gated on a touch pointer)', () => {
    // tuxlink-mt73: gated on `any-pointer: coarse` so a narrow desktop window
    // (the 1100px Compose webview) does not enter FZ-M1 touch mode.
    expect(COMPACT_MEDIA_QUERY).toBe('(max-width: 1365px) and (any-pointer: coarse)');
  });

  it('reports compact=true when the media query matches at mount', () => {
    installMatchMedia(true);
    const { result } = renderHook(() => useViewport());
    expect(result.current.isCompact).toBe(true);
  });

  it('reports compact=false above the breakpoint', () => {
    installMatchMedia(false);
    const { result } = renderHook(() => useViewport());
    expect(result.current.isCompact).toBe(false);
  });

  it('updates when the media query changes (resize across the breakpoint)', () => {
    const ctl = installMatchMedia(false);
    const { result } = renderHook(() => useViewport());
    expect(result.current.isCompact).toBe(false);
    act(() => ctl.fire(true));
    expect(result.current.isCompact).toBe(true);
  });
});
