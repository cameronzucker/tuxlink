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
}

type HintAction =
  | { type: 'config-loaded'; tourCompleted: boolean; tipsSeen: string[] }
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
};

function hintReducer(state: HintState, action: HintAction): HintState {
  switch (action.type) {
    case 'config-loaded':
      return {
        ...state,
        tourCompleted: action.tourCompleted,
        tipsSeen: action.tipsSeen,
        active: action.tourCompleted ? null : { kind: 'offer' },
      };
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

  // Bullet 1: seed from config_read on mount.
  useEffect(() => {
    let mounted = true;
    invoke<{ onboarding_tour_completed: boolean; onboarding_tips_seen: string[] }>('config_read')
      .then((c) => {
        if (!mounted) return;
        dispatch({
          type: 'config-loaded',
          tourCompleted: c.onboarding_tour_completed,
          tipsSeen: c.onboarding_tips_seen,
        });
      })
      .catch((err) => {
        console.error('[onboarding] config_read failed', err);
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

  function requestFirstOpenTip(id: string): void {
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
    const anchorMounted = document.querySelector(`[data-tour-anchor="${entry.anchor}"]`) !== null;
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
  useEffect(() => {
    const passthroughKeys = new Set(['Escape', 'ArrowRight', 'ArrowLeft', 'Enter', 'Tab']);

    function onKeyDown(e: KeyboardEvent): void {
      const active = stateRef.current.active;
      if (!isOverlayCapturing(active)) return; // inactive or offer card: no-op passthrough

      if (!passthroughKeys.has(e.key)) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      // Tab is left alone entirely — the focus trap lives in the Task 6
      // overlay component.
      if (e.key === 'Tab') return;

      if (e.key === 'Escape') {
        if (active.kind === 'tour') skipTour();
        else dismissSingle();
        return;
      }
      if (active.kind !== 'tour') return; // advance/back only meaningful for the tour
      if (e.key === 'ArrowRight' || e.key === 'Enter') advance();
      else if (e.key === 'ArrowLeft') back();
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
    requestFirstOpenTip,
    registerProbe,
    overlayActive,
  };

  return <HintContext.Provider value={value}>{children}</HintContext.Provider>;
}
