// HintOverlay — the spotlight tour / single-hint popover (tuxlink-10bkw Task 6).
//
// Pure consumer of useHints(); renders nothing when `active` is null or
// `{kind:'offer'}` (OfferCard owns that surface). For 'tour' and 'single' it:
//   - finds the current entry's anchor element via `[data-tour-anchor=...]`,
//   - when found: 4 fixed dark panels + a transparent click-BLOCKING 5th div
//     exactly over the (padded) hole, plus a popover positioned beside it,
//   - when NOT found: `fallback:'center'` shows a centered popover (no panels,
//     no blocker); `fallback:'skip'` auto-advances the tour past this stop
//     (or auto-dismisses a single hint) rather than showing nothing forever.
//
// No SVG masks, no mix-blend-mode — both are unreliable under WebKitGTK
// (project convention; see CLAUDE.md WebKitGTK rendering notes). Geometry is
// plain `position:fixed` rects recomputed on resize/scroll/ResizeObserver/rAF.

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent } from 'react';
import { useHints } from './HintProvider';
import { TOUR_STOPS } from './tourRegistry';
import type { HintEntry } from './types';
import { findMountedAnchor } from './domAnchor';
import './HintOverlay.css';

interface Rect {
  top: number;
  left: number;
  width: number;
  height: number;
}

/** The gap between the spotlighted hole and the element it highlights. */
const HOLE_PADDING = 8;
/** Gap between the anchor hole and the popover; viewport clamp margin. */
const POPOVER_GAP = 12;
const VIEWPORT_MARGIN = 8;

function rectOf(el: Element): Rect {
  const r = el.getBoundingClientRect();
  return { top: r.top, left: r.left, width: r.width, height: r.height };
}

const FOCUSABLE_SELECTOR =
  'button:not(:disabled), [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

export function HintOverlay() {
  const hints = useHints();
  const { active } = hints;

  const mode: 'tour' | 'single' | null =
    active?.kind === 'tour' ? 'tour' : active?.kind === 'single' ? 'single' : null;

  const stepIndex = active?.kind === 'tour' ? active.stepIndex : -1;
  const entry: HintEntry | null =
    active?.kind === 'tour' ? (TOUR_STOPS[active.stepIndex] ?? null) : active?.kind === 'single' ? active.entry : null;

  // Anchor geometry, keyed by entry id so a step/tip change re-checks fresh
  // rather than showing the PREVIOUS entry's stale rect for one frame.
  const [geom, setGeom] = useState<{ id: string; rect: Rect | null } | null>(null);
  const resolved = geom !== null && entry !== null && geom.id === entry.id;
  const rect = resolved ? geom.rect : null;

  // Tracking: window resize + scroll (capture, so ancestor scrolls are
  // caught) + ResizeObserver on the target + one rAF retry after mount (late
  // layout — e.g. a lazy panel whose chunk is still evaluating).
  useLayoutEffect(() => {
    if (!entry) {
      setGeom(null);
      return;
    }
    const id = entry.id;
    const anchorAttr = entry.anchor;

    function reposition() {
      const el = findMountedAnchor(anchorAttr);
      setGeom({ id, rect: el ? rectOf(el) : null });
    }
    reposition();

    window.addEventListener('resize', reposition);
    window.addEventListener('scroll', reposition, true);

    // Finding #1: a zero-rect anchor (e.g. RadioDrawer's `display:contents`
    // root at desktop widths) is treated as anchor-missing by findMountedAnchor
    // above, so `el` here is null too — nothing to ResizeObserver.
    const el = findMountedAnchor(anchorAttr);
    let ro: ResizeObserver | undefined;
    if (el && typeof ResizeObserver !== 'undefined') {
      ro = new ResizeObserver(reposition);
      ro.observe(el);
    }
    const raf = requestAnimationFrame(reposition);

    return () => {
      window.removeEventListener('resize', reposition);
      window.removeEventListener('scroll', reposition, true);
      ro?.disconnect();
      cancelAnimationFrame(raf);
    };
  }, [entry?.id, entry?.anchor]);

  // fallback:'skip' + anchor not found: auto-advance the tour past this stop,
  // or auto-dismiss a single hint, rather than leaving the overlay invisible
  // but still capturing keyboard. Guarded by id so it fires exactly once per
  // entry (hints.advance/dismissSingle are fresh closures every render).
  const handledSkipRef = useRef<string | null>(null);
  useEffect(() => {
    if (!entry || !resolved || rect) return;
    if (entry.fallback !== 'skip') return;
    if (handledSkipRef.current === entry.id) return;
    handledSkipRef.current = entry.id;
    if (mode === 'tour') hints.advance();
    // Finding #5: a single hint (tip or point-at) whose anchor is missing at
    // fire time must be abandoned WITHOUT persisting — dismissSingle() would
    // mark a tip permanently "seen" even though the operator never saw it.
    // abandonSingle() clears the active state and leaves tipsSeen untouched
    // (suppressed-not-consumed), matching the busy-suppression semantics
    // requestFirstOpenTip already uses elsewhere.
    else if (mode === 'single') hints.abandonSingle();
  }, [entry, resolved, rect, mode, hints]);

  const showSpotlight = entry !== null && resolved && rect !== null;
  const showCentered = entry !== null && resolved && rect === null && entry.fallback === 'center';
  const popoverVisible = showSpotlight || showCentered;

  // Popover measure-then-position: render once at a default spot, then
  // measure its own size and flip above/clamp horizontally if it wouldn't fit.
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  useLayoutEffect(() => {
    if (!showSpotlight || !rect) {
      setPos(null);
      return;
    }
    const pop = popoverRef.current;
    const popW = pop?.offsetWidth || 320;
    const popH = pop?.offsetHeight || 160;
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    let top = rect.top + rect.height + POPOVER_GAP;
    if (top + popH > vh - VIEWPORT_MARGIN) {
      // Flip above; if that ALSO doesn't fit, clamp to the top margin —
      // still readable, just no longer perfectly adjacent to the hole.
      top = Math.max(VIEWPORT_MARGIN, rect.top - popH - POPOVER_GAP);
    }
    let left = rect.left;
    if (left + popW > vw - VIEWPORT_MARGIN) left = vw - popW - VIEWPORT_MARGIN;
    if (left < VIEWPORT_MARGIN) left = VIEWPORT_MARGIN;

    setPos({ top, left });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showSpotlight, rect?.top, rect?.left, rect?.width, rect?.height, entry?.id]);

  // Focus management: capture the pre-overlay focus target once when the
  // popover FIRST becomes visible; restore it once the popover fully closes.
  const prevFocusRef = useRef<HTMLElement | null>(null);
  const wasVisibleRef = useRef(false);
  useEffect(() => {
    if (popoverVisible && !wasVisibleRef.current) {
      prevFocusRef.current = document.activeElement as HTMLElement | null;
    } else if (!popoverVisible && wasVisibleRef.current) {
      prevFocusRef.current?.focus?.();
      prevFocusRef.current = null;
    }
    wasVisibleRef.current = popoverVisible;
  }, [popoverVisible]);

  // Focus the primary action (Next/Finish or Got it) whenever the popover
  // opens, or moves to a new step/tip while already open.
  const primaryBtnRef = useRef<HTMLButtonElement | null>(null);
  useEffect(() => {
    if (!popoverVisible) return;
    primaryBtnRef.current?.focus();
  }, [popoverVisible, entry?.id]);

  function onPopoverKeyDown(e: ReactKeyboardEvent<HTMLDivElement>) {
    if (e.key !== 'Tab') return;
    const root = popoverRef.current;
    if (!root) return;
    const focusables = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const activeEl = document.activeElement;
    if (e.shiftKey) {
      if (activeEl === first || !root.contains(activeEl)) {
        e.preventDefault();
        last.focus();
      }
    } else if (activeEl === last || !root.contains(activeEl)) {
      e.preventDefault();
      first.focus();
    }
  }

  // Finding #4: the dim panels sit over live app chrome; a click on one must
  // not fall through to whatever control is underneath (the hole blocker
  // already covers the hole itself, so this only ever swallows clicks OUTSIDE
  // the spotlighted hole).
  function swallowPanelClick(e: ReactMouseEvent<HTMLDivElement>) {
    e.stopPropagation();
  }

  if (!entry || !resolved || !popoverVisible) return null;

  const titleId = 'hint-overlay-title';
  const bodyId = 'hint-overlay-desc';
  const total = TOUR_STOPS.length;
  const liveText = mode === 'tour' ? `Step ${stepIndex + 1} of ${total}: ${entry.title}` : entry.title;

  const hole: Rect | null = showSpotlight && rect
    ? {
        top: rect.top - HOLE_PADDING,
        left: rect.left - HOLE_PADDING,
        width: rect.width + HOLE_PADDING * 2,
        height: rect.height + HOLE_PADDING * 2,
      }
    : null;

  return (
    <>
      {hole && (
        <>
          <div
            className="hint-overlay__panel"
            data-testid="hint-overlay-panel"
            onClick={swallowPanelClick}
            style={{ top: 0, left: 0, width: '100vw', height: Math.max(0, hole.top) }}
          />
          <div
            className="hint-overlay__panel"
            data-testid="hint-overlay-panel"
            onClick={swallowPanelClick}
            style={{
              top: hole.top + hole.height,
              left: 0,
              width: '100vw',
              height: Math.max(0, window.innerHeight - (hole.top + hole.height)),
            }}
          />
          <div
            className="hint-overlay__panel"
            data-testid="hint-overlay-panel"
            onClick={swallowPanelClick}
            style={{ top: hole.top, left: 0, width: Math.max(0, hole.left), height: hole.height }}
          />
          <div
            className="hint-overlay__panel"
            data-testid="hint-overlay-panel"
            onClick={swallowPanelClick}
            style={{
              top: hole.top,
              left: hole.left + hole.width,
              width: Math.max(0, window.innerWidth - (hole.left + hole.width)),
              height: hole.height,
            }}
          />
          <div
            className="hint-overlay__blocker"
            data-testid="hint-overlay-blocker"
            style={{ top: hole.top, left: hole.left, width: hole.width, height: hole.height }}
            onClick={(e) => e.stopPropagation()}
          />
        </>
      )}
      <div
        ref={popoverRef}
        className={`hint-overlay__popover${showCentered ? ' hint-overlay__popover--center' : ''}`}
        data-testid="hint-overlay-popover"
        role="dialog"
        aria-modal="false"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        style={showSpotlight && pos ? { top: pos.top, left: pos.left } : undefined}
        onKeyDown={onPopoverKeyDown}
      >
        <div className="hint-overlay__live" role="status" aria-live="polite" data-testid="hint-overlay-live">
          {liveText}
        </div>
        <h3 id={titleId} className="hint-overlay__title">
          {entry.title}
        </h3>
        <p id={bodyId} className="hint-overlay__body">
          {entry.body}
        </p>
        <div className="hint-overlay__actions">
          {mode === 'tour' ? (
            <>
              {stepIndex > 0 && (
                <button
                  type="button"
                  className="hint-overlay__btn"
                  data-testid="hint-overlay-back"
                  onClick={hints.back}
                >
                  Back
                </button>
              )}
              <button
                type="button"
                ref={primaryBtnRef}
                className="hint-overlay__btn hint-overlay__btn--primary"
                data-testid="hint-overlay-next"
                onClick={hints.advance}
              >
                {stepIndex === total - 1 ? 'Finish' : 'Next'}
              </button>
              <button
                type="button"
                className="hint-overlay__btn hint-overlay__btn--ghost"
                data-testid="hint-overlay-skip"
                onClick={hints.skipTour}
              >
                Skip tour
              </button>
            </>
          ) : (
            <button
              type="button"
              ref={primaryBtnRef}
              className="hint-overlay__btn hint-overlay__btn--primary"
              data-testid="hint-overlay-gotit"
              onClick={hints.dismissSingle}
            >
              Got it
            </button>
          )}
        </div>
      </div>
    </>
  );
}
