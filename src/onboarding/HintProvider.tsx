// HintProvider — the onboarding state machine (tuxlink-10bkw Task 5).
//
// Owns exactly one "active" surface at a time: the first-run offer card, the
// guided tour (TOUR_STOPS), or a single hint (a discretionary tip from
// HINTS, or a backend-driven point-at from Elmer/MCP). The overlay/offer
// card itself is Task 6 — this file is pure state + persistence + the
// capture-phase keyboard policy + the point-at event bridge.
//
// Persistence model: `config_read` seeds `onboarding_tour_completed` +
// `onboarding_tips_seen` on mount. Completing/skipping/declining the tour
// writes `tourCompleted: true` with the CURRENT tipsSeen via
// `config_set_onboarding` (whole-section set, matching the backend's
// last-write-wins posture — see ui_commands.rs config_set_onboarding).
// Dismissing a tip persists the updated tipsSeen the same way. A failed
// write is logged and NOT retried on a timer — the in-memory state still
// advances (session flag), and the next real state change naturally
// attempts another write.
//
// Latest-ref pattern: the capture-phase keydown listener and the
// `onboarding:point-at` event listener are each registered ONCE (empty
// dependency effects) so they survive across every re-render without
// churning `addEventListener`/`listen`. Because they only fire well after
// mount, they read current state through `stateRef` (mutated unconditionally
// every render, mirroring the pattern used for "latest ref" callbacks
// elsewhere in this codebase) rather than closing over a stale `state`.

import {
  createContext,
  useContext,
  useEffect,
  useReducer,
  useRef,
} from 'react';
import type { ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { HintEntry, PanelStateProbe } from './types';
import { TOUR_STOPS } from './tourRegistry';
import { HINTS } from './hintRegistry';
import { MENU_POINT_AT_ENTRIES } from './menuAnchors';
import { isTipSeen, markTipSeen } from './tipLogic';
import { findMountedAnchor } from './domAnchor';

/** Every known anchor, for point-at lookup: tour stops, discretionary tips,
 *  and menu chrome (top-level menu buttons + items inside open menus —
 *  point-at-only, tuxlink-10bkw Task 9). Discretionary first-open tip
 *  scheduling (`requestFirstOpenTip`) is driven exclusively by literal HINTS
 *  ids passed from call sites (useFirstOpenTip('settings'|'compose'|
 *  'find-a-station'|'aprs')) — it never iterates this combined list — so
 *  menu entries cannot leak into tip scheduling. */
const ALL_ENTRIES: HintEntry[] = [...TOUR_STOPS, ...HINTS, ...MENU_POINT_AT_ENTRIES];

function findEntry(id: string): HintEntry | undefined {
  return ALL_ENTRIES.find((e) => e.id === id);
}

/** Ack outcomes exactly as the backend's `PointAtAck.outcome` expects. */
type PointAtOutcome = 'shown' | 'unknown-anchor' | 'anchor-unmounted' | 'overlay-busy';

type ActiveHint =
  | null
  | { kind: 'offer' }
  | { kind: 'tour'; stepIndex: number }
  | { kind: 'single'; entry: HintEntry; source: 'tip' | 'point-at' };

/**
 * The overlay is capturing keyboard/UI: a tour or a single hint. The offer
 * card is deliberately NOT overlay-active (it never blocks typing). Single
 * source of truth for both the context's `overlayActive` and the capture
 * keydown policy. Type guard so callers get the narrowed tour/single union.
 */
function isOverlayCapturing(
  active: ActiveHint,
): active is Exclude<ActiveHint, null | { kind: 'offer' }> {
  return active !== null && active.kind !== 'offer';
}

export interface HintContextValue {
  active: ActiveHint;
  /** Offer accept + Help replay — (re)starts the tour from stop 0. */
  startTour(): void;
  advance(): void;
  back(): void;
  /** ESC / Skip during the tour — persists tour_completed. */
  skipTour(): void;
  /** Declining the first-run offer card — persists tour_completed. */
  declineOffer(): void;
  /** Tip: marks seen + persists. Point-at: no mutation. */
  dismissSingle(): void;
  /** Auto-skip (anchor missing/zero-rect at fire time): clears the active
   *  single hint WITHOUT persisting — suppressed-not-consumed, so a tip whose
   *  anchor was hidden stays eligible for a future request. Distinct from
   *  dismissSingle(), which is the user-clicked "Got it" path and persists
   *  tips_seen for the 'tip' source. */
  abandonSingle(): void;
  /** Fired by useFirstOpenTip on mount; shows the tip iff unseen + idle. */
  requestFirstOpenTip(id: string): void;
  registerProbe(key: string, probe: PanelStateProbe): () => void;
  overlayActive: boolean;
}

const HintContext = createContext<HintContextValue | null>(null);

export function useHints(): HintContextValue {
  const ctx = useContext(HintContext);
  if (!ctx) throw new Error('useHints must be used inside <HintProvider>');
  return ctx;
}

/** Fires requestFirstOpenTip(id) once on mount (id change re-fires). */
export function useFirstOpenTip(id: string): void {
  const { requestFirstOpenTip } = useHints();
  useEffect(() => {
    requestFirstOpenTip(id);
    // requestFirstOpenTip's identity churns every render, but its behavior
    // only depends on the latest state (read via ref inside the provider),
    // so it is intentionally left out of the dependency array.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);
}

interface HintState {
  active: ActiveHint;
  tourCompleted: boolean;
  tipsSeen: string[];
  /** Finding #6: config_read hasn't resolved yet. requestFirstOpenTip queues
   *  ids while this is false instead of consulting the not-yet-real
   *  tourCompleted:true / tipsSeen:[] defaults, which could show (and
   *  persist) a tip before the real config lands on a slow cold start. */
  hydrated: boolean;
}

type HintAction =
  | { type: 'config-loaded'; tourCompleted: boolean; tipsSeen: string[]; pendingTipIds: string[] }
  | { type: 'start-tour' }
  | { type: 'set-tour-step'; stepIndex: number }
  | { type: 'complete-onboarding' }
  | { type: 'show-single'; entry: HintEntry; source: 'tip' | 'point-at' }
  | { type: 'dismiss-single'; tipsSeen: string[] };

const initialState: HintState = {
  active: null,
  // Safe default until config_read resolves (or fails): never force a tour
  // an operator already has recorded a preference about.
  tourCompleted: true,
  tipsSeen: [],
  hydrated: false,
};

function hintReducer(state: HintState, action: HintAction): HintState {
  switch (action.type) {
    case 'config-loaded': {
      const { tourCompleted, tipsSeen, pendingTipIds } = action;
      // Guard against clobbering an operator-initiated tour/single that
      // started WHILE config_read was still in flight (a slow read, or —
      // per the catch-fallback dispatch below — one that failed outright).
      // Only the untouched pre-hydration idle state (active === null) is
      // ours to decide; if the operator already clicked "Replay tour" or a
      // tip already surfaced, hydration completing must not yank it away.
      let active = state.active;
      if (active === null) {
        // Bullet 1 default: tourCompleted:false shows the offer, which wins
        // over any queued tip request (offer/tour always outrank a single hint).
        active = tourCompleted ? null : { kind: 'offer' };
        if (active === null) {
          // Finding #6: drain requestFirstOpenTip calls that arrived before
          // hydration, checked against the REAL freshly-loaded tipsSeen (not
          // the initialState placeholder) — same eligibility rule
          // requestFirstOpenTip itself uses. Only the first eligible queued
          // id shows; a single hint can only hold one entry, so the rest
          // simply remain unconsumed (they never got a "shown" this cycle,
          // so nothing was suppressed-and-lost — a future request re-checks).
          for (const id of pendingTipIds) {
            if (isTipSeen(tipsSeen, id)) continue;
            const entry = findEntry(id);
            if (!entry) continue;
            active = { kind: 'single', entry, source: 'tip' };
            break;
          }
        }
      }
      return { ...state, tourCompleted, tipsSeen, hydrated: true, active };
    }
    case 'start-tour':
      return { ...state, active: { kind: 'tour', stepIndex: 0 } };
    case 'set-tour-step':
      return state.active?.kind === 'tour'
        ? { ...state, active: { kind: 'tour', stepIndex: action.stepIndex } }
        : state;
    case 'complete-onboarding':
      return { ...state, active: null, tourCompleted: true };
    case 'show-single':
      return { ...state, active: { kind: 'single', entry: action.entry, source: action.source } };
    case 'dismiss-single':
      return { ...state, active: null, tipsSeen: action.tipsSeen };
    default:
      return state;
  }
}

/** Whole-section config_set_onboarding write; logs, never throws. */
function persistOnboarding(tourCompleted: boolean, tipsSeen: string[]): void {
  invoke('config_set_onboarding', { tourCompleted, tipsSeen }).catch((err) => {
    console.error('[onboarding] failed to persist onboarding state', err);
  });
}

function ackPointAt(
  requestId: number,
  outcome: PointAtOutcome,
  extra?: { validIds?: string[]; openHint?: string },
): void {
  invoke('onboarding_point_at_ack', {
    requestId,
    outcome,
    validIds: extra?.validIds,
    openHint: extra?.openHint,
  }).catch((err) => {
    console.error('[onboarding] point-at ack failed', err);
  });
}

export function HintProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(hintReducer, initialState);
  const probesRef = useRef<Map<string, PanelStateProbe>>(new Map());
  // "Latest ref" for the mount-once listeners below — see file header.
  const stateRef = useRef(state);
  stateRef.current = state;
  // Finding #6: requestFirstOpenTip ids that arrived before config_read
  // resolved. Drained atomically into the 'config-loaded' dispatch so they're
  // checked against the REAL tipsSeen, never the initialState placeholder.
  const pendingTipIdsRef = useRef<string[]>([]);

  // Bullet 1: seed from config_read on mount.
  useEffect(() => {
    let mounted = true;
    invoke<{ onboarding_tour_completed: boolean; onboarding_tips_seen: string[] }>('config_read')
      .then((c) => {
        if (!mounted) return;
        // Read the (possibly malformed — `config_read` is a shared IPC
        // command; some callers legitimately see a null/different shape)
        // response fields FIRST, before touching pendingTipIdsRef. If this
        // throws, it must throw here — before the ref is captured/reset —
        // so the .catch below (which also drains pendingTipIdsRef) still
        // sees every id queued so far instead of an already-cleared ref.
        const tourCompleted = c.onboarding_tour_completed;
        const tipsSeen = c.onboarding_tips_seen;
        const pendingTipIds = pendingTipIdsRef.current;
        pendingTipIdsRef.current = [];
        dispatch({ type: 'config-loaded', tourCompleted, tipsSeen, pendingTipIds });
      })
      .catch((err) => {
        console.error('[onboarding] config_read failed', err);
        if (!mounted) return;
        // Finding #6 corollary: hydration must complete even when config_read
        // rejects (or returns a malformed shape — e.g. a shared IPC command
        // that legitimately answers `null` for a different caller). Without
        // this, requestFirstOpenTip calls queued before the failure would
        // wait forever and never drain. Fall back to the same safe defaults
        // initialState already used pre-hydration: never force a tour, but a
        // still-unseen tip may show.
        const pendingTipIds = pendingTipIdsRef.current;
        pendingTipIdsRef.current = [];
        dispatch({ type: 'config-loaded', tourCompleted: true, tipsSeen: [], pendingTipIds });
      });
    return () => {
      mounted = false;
    };
  }, []);

  function startTour(): void {
    dispatch({ type: 'start-tour' });
  }

  function advance(): void {
    const active = stateRef.current.active;
    if (active?.kind !== 'tour') return;
    const nextIndex = active.stepIndex + 1;
    if (nextIndex >= TOUR_STOPS.length) {
      // Bullet 2: finishing the last stop persists tour_completed.
      dispatch({ type: 'complete-onboarding' });
      persistOnboarding(true, stateRef.current.tipsSeen);
    } else {
      dispatch({ type: 'set-tour-step', stepIndex: nextIndex });
    }
  }

  function back(): void {
    const active = stateRef.current.active;
    if (active?.kind !== 'tour') return;
    dispatch({ type: 'set-tour-step', stepIndex: Math.max(0, active.stepIndex - 1) });
  }

  function skipTour(): void {
    dispatch({ type: 'complete-onboarding' });
    persistOnboarding(true, stateRef.current.tipsSeen);
  }

  function declineOffer(): void {
    dispatch({ type: 'complete-onboarding' });
    persistOnboarding(true, stateRef.current.tipsSeen);
  }

  function dismissSingle(): void {
    const active = stateRef.current.active;
    if (active?.kind !== 'single') return;
    if (active.source === 'tip') {
      const nextTipsSeen = markTipSeen(stateRef.current.tipsSeen, active.entry.id);
      dispatch({ type: 'dismiss-single', tipsSeen: nextTipsSeen });
      persistOnboarding(stateRef.current.tourCompleted, nextTipsSeen);
    } else {
      // point-at: session-only, no persistence.
      dispatch({ type: 'dismiss-single', tipsSeen: stateRef.current.tipsSeen });
    }
  }

  function abandonSingle(): void {
    const active = stateRef.current.active;
    if (active?.kind !== 'single') return;
    // Never marks tips_seen and never persists — see the interface doc
    // comment. Identical shape to dismissSingle's point-at branch, but used
    // for BOTH sources: the auto-skip-fallback path in HintOverlay calls this
    // regardless of whether the single hint came from a tip or a point-at.
    dispatch({ type: 'dismiss-single', tipsSeen: stateRef.current.tipsSeen });
  }

  function requestFirstOpenTip(id: string): void {
    // Finding #6: config_read hasn't resolved yet — queue instead of
    // consulting the initialState placeholder (tourCompleted:true,
    // tipsSeen:[]), which could show (and then persist) a tip before the
    // real onboarding config is known. Drained by the 'config-loaded'
    // dispatch above once the real config lands.
    if (!stateRef.current.hydrated) {
      pendingTipIdsRef.current.push(id);
      return;
    }
    // Suppressed-not-consumed: busy leaves the tip eligible for next time.
    if (stateRef.current.active !== null) return;
    if (isTipSeen(stateRef.current.tipsSeen, id)) return;
    const entry = findEntry(id);
    if (!entry) return;
    dispatch({ type: 'show-single', entry, source: 'tip' });
  }

  function registerProbe(key: string, probe: PanelStateProbe): () => void {
    probesRef.current.set(key, probe);
    return () => {
      if (probesRef.current.get(key) === probe) probesRef.current.delete(key);
    };
  }

  function handlePointAt(requestId: number, anchorId: string): void {
    const entry = findEntry(anchorId);
    if (!entry) {
      ackPointAt(requestId, 'unknown-anchor', { validIds: ALL_ENTRIES.map((e) => e.id) });
      return;
    }
    // Overlay-busy wins over mount-state: while a modal offer/tour is
    // capturing the UI, an 'anchor-unmounted' ack with openHint navigation
    // guidance would be actively wrong (the operator can't navigate anywhere).
    // A point-at may replace an earlier point-at/tip (single), but never a
    // running offer or tour.
    const activeNow = stateRef.current.active;
    if (activeNow !== null && (activeNow.kind === 'offer' || activeNow.kind === 'tour')) {
      ackPointAt(requestId, 'overlay-busy');
      return;
    }
    const probe = entry.requiredPanelState ? probesRef.current.get(entry.requiredPanelState) : undefined;
    const probeOk = entry.requiredPanelState ? Boolean(probe?.()) : true;
    // Finding #1: a zero-rect anchor (present in the DOM but laid out with no
    // box — e.g. RadioDrawer's `display:contents` root at desktop widths) is
    // exactly as unusable as "not found"; ack anchor-unmounted for it too.
    const anchorMounted = findMountedAnchor(entry.anchor) !== null;
    if (!probeOk || !anchorMounted) {
      ackPointAt(requestId, 'anchor-unmounted', { openHint: entry.openHint });
      return;
    }
    dispatch({ type: 'show-single', entry, source: 'point-at' });
    ackPointAt(requestId, 'shown');
  }

  // Bullet 6: onboarding:point-at listener — registered once (canonical
  // mounted/unlisten cleanup pattern from useInboundSelection.ts).
  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | undefined;

    listen<{ request_id: number; anchor_id: string }>('onboarding:point-at', (event) => {
      if (!mounted) return;
      handlePointAt(event.payload.request_id, event.payload.anchor_id);
    })
      .then((un) => {
        if (!mounted) {
          un();
          return;
        }
        unlisten = un;
      })
      .catch(() => {
        // listen() unavailable (test env without Tauri — mocked separately).
      });

    return () => {
      mounted = false;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Bullet 5: capture-phase keydown policy — registered once on mount.
  //
  // Findings #2/#3 (fixwave): this handler used to treat Enter as a
  // "passthrough" key — synthesizing advance() AND letting the event
  // continue toward the focused Next button, whose own onClick ALSO calls
  // advance() via the button's native Enter-activates-click behavior. That
  // double-fired every Enter press (and made Back+Enter a net no-op). Space
  // wasn't in the passthrough set at all, so it was unconditionally swallowed
  // — a focused button could never be Space-activated. Fix: when Enter/Space
  // targets an element inside the popover dialog, this handler does NOT
  // synthesize anything and does NOT call preventDefault/stopPropagation —
  // the popover's own real DOM buttons own activation entirely. Arrow keys
  // have no native button-activation behavior, so they always synthesize
  // regardless of focus location (no double-fire risk — this is the tested
  // "keyboard shortcut works even while a button has focus" behavior).
  // Escape stays global (synthesized regardless of focus location) since
  // Skip/Got-it must work even when focus is elsewhere in the dialog. Every
  // key this handler DOES synthesize for (Escape, Arrows, non-popover Enter)
  // is consumed (preventDefault + stopPropagation) so a document-level
  // listener underneath the overlay doesn't ALSO react to it (finding #3 —
  // e.g. Escape dismissing a tip must not also close an app dialog beneath).
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent): void {
      const active = stateRef.current.active;
      if (!isOverlayCapturing(active)) return; // inactive or offer card: no-op passthrough

      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        if (active.kind === 'tour') skipTour();
        else dismissSingle();
        return;
      }

      // Tab is left alone entirely — the focus trap lives in the Task 6
      // overlay component.
      if (e.key === 'Tab') return;

      // Arrow keys don't natively activate a focused button, so they can
      // always synthesize the tour shortcut regardless of focus location —
      // no double-fire risk, and this is the tested "works while a button
      // has focus" keyboard-shortcut behavior.
      if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
        e.preventDefault();
        e.stopPropagation();
        if (active.kind === 'tour') {
          if (e.key === 'ArrowLeft') back();
          else advance();
        }
        return;
      }

      if (e.key === 'Enter' || e.key === ' ') {
        const target = e.target;
        const insidePopover = target instanceof Element && target.closest('[role="dialog"]') !== null;
        if (insidePopover) {
          // The focused button's own native Enter/Space click-activation
          // handles it — synthesizing on top would double-fire Enter (two
          // advances per press) and Space was previously swallowed outright
          // regardless of focus, blocking Space-activation of any focused
          // button entirely.
          return;
        }
        e.preventDefault();
        e.stopPropagation();
        if (active.kind === 'tour' && e.key === 'Enter') advance();
        return;
      }

      // Any other key while the overlay captures the UI: swallow it so it
      // doesn't leak into the app beneath.
      e.preventDefault();
      e.stopPropagation();
    }

    window.addEventListener('keydown', onKeyDown, { capture: true });
    return () => window.removeEventListener('keydown', onKeyDown, { capture: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const overlayActive = isOverlayCapturing(state.active);

  const value: HintContextValue = {
    active: state.active,
    startTour,
    advance,
    back,
    skipTour,
    declineOffer,
    dismissSingle,
    abandonSingle,
    requestFirstOpenTip,
    registerProbe,
    overlayActive,
  };

  return <HintContext.Provider value={value}>{children}</HintContext.Provider>;
}
