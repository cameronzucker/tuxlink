import { useCallback, useRef, useState } from 'react';
import type { RefObject } from 'react';

/** A container counts as "at the bottom" within this many pixels of the end. */
const BOTTOM_THRESHOLD_PX = 24;

export interface StreamAutoFollow {
  /** Wire to the scroll container's `onScroll`. Updates the pinned state. */
  onScroll: () => void;
  /** True while the user is pinned to (near) the bottom. */
  atBottom: boolean;
  /** Scroll to the bottom ONLY if currently pinned (call after content grows). */
  followIfPinned: () => void;
  /** Force-scroll to the bottom and re-pin (the "Jump to live" action). */
  jumpToLive: () => void;
}

/**
 * Pin-to-bottom auto-follow for a scroll container. Auto-scroll only happens
 * when the user is already at the bottom; scrolling up releases the pin so the
 * viewport is never yanked away mid-read (tuxlink-06v9s).
 */
export function useStreamAutoFollow(ref: RefObject<HTMLElement | null>): StreamAutoFollow {
  const [atBottom, setAtBottom] = useState(true);
  // Mirror in a ref so followIfPinned reads the latest value without being
  // re-created on every pin change (it is called from a content-change effect).
  const atBottomRef = useRef(true);

  const computeAtBottom = useCallback((): boolean => {
    const el = ref.current;
    if (!el) return true;
    return el.scrollHeight - el.scrollTop - el.clientHeight <= BOTTOM_THRESHOLD_PX;
  }, [ref]);

  const onScroll = useCallback(() => {
    const now = computeAtBottom();
    atBottomRef.current = now;
    setAtBottom(now);
  }, [computeAtBottom]);

  const scrollToBottom = useCallback(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [ref]);

  const followIfPinned = useCallback(() => {
    if (atBottomRef.current) scrollToBottom();
  }, [scrollToBottom]);

  const jumpToLive = useCallback(() => {
    scrollToBottom();
    atBottomRef.current = true;
    setAtBottom(true);
  }, [scrollToBottom]);

  return { onScroll, atBottom, followIfPinned, jumpToLive };
}
