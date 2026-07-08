import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useRef } from 'react';
import { useStreamAutoFollow } from './useStreamAutoFollow';

// jsdom does no layout, so scroll geometry must be defined by hand.
function makeScrollEl(scrollHeight: number, clientHeight: number, scrollTop: number): HTMLElement {
  const el = document.createElement('div');
  Object.defineProperty(el, 'scrollHeight', { value: scrollHeight, configurable: true });
  Object.defineProperty(el, 'clientHeight', { value: clientHeight, configurable: true });
  let top = scrollTop;
  Object.defineProperty(el, 'scrollTop', {
    get: () => top,
    set: (v: number) => { top = v; },
    configurable: true,
  });
  return el;
}

function useHarness(el: HTMLElement) {
  const ref = useRef<HTMLElement | null>(el);
  return useStreamAutoFollow(ref);
}

describe('useStreamAutoFollow', () => {
  it('starts pinned to the bottom', () => {
    const el = makeScrollEl(1000, 200, 800); // 1000-800-200 = 0 <= 24
    const { result } = renderHook(() => useHarness(el));
    expect(result.current.atBottom).toBe(true);
  });

  it('releases the pin once the user scrolls up', () => {
    const el = makeScrollEl(1000, 200, 800);
    const { result } = renderHook(() => useHarness(el));
    el.scrollTop = 100; // 1000-100-200 = 700 > 24
    act(() => { result.current.onScroll(); });
    expect(result.current.atBottom).toBe(false);
  });

  it('followIfPinned scrolls to bottom only when pinned', () => {
    const el = makeScrollEl(1000, 200, 100); // scrolled up
    const { result } = renderHook(() => useHarness(el));
    act(() => { result.current.onScroll(); }); // register scrolled-up state
    expect(result.current.atBottom).toBe(false);
    act(() => { result.current.followIfPinned(); });
    expect(el.scrollTop).toBe(100); // NOT moved — pin released

    el.scrollTop = 800;
    act(() => { result.current.onScroll(); }); // back at bottom
    el.scrollTop = 700; // pretend new content arrived above the fold
    act(() => { result.current.followIfPinned(); });
    expect(el.scrollTop).toBe(1000); // snapped to scrollHeight
  });

  it('jumpToLive snaps to bottom and re-pins', () => {
    const el = makeScrollEl(1000, 200, 100);
    const { result } = renderHook(() => useHarness(el));
    act(() => { result.current.onScroll(); });
    expect(result.current.atBottom).toBe(false);
    act(() => { result.current.jumpToLive(); });
    expect(el.scrollTop).toBe(1000);
    expect(result.current.atBottom).toBe(true);
  });
});
