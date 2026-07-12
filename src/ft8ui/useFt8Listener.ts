/**
 * useFt8Listener.ts — the race-safe FT8 hydration hook + Ft8ListenerProvider
 * (Task B1, plan tuxlink-b026z.4 §Frontend data layer).
 *
 * The provider owns ONE subscription (a single pair of `listen` calls) and the
 * hydration state machine; `useFt8Listener()` reads the derived value from
 * context. Every Phase-C component consumes this hook.
 *
 * Hydration contract (§Frontend data layer, 5 steps):
 *   1. Register BOTH listeners FIRST, so events that fire before the snapshot
 *      resolves are captured, not dropped.
 *   2. THEN invoke('ft8_listener_snapshot').
 *   3. Replay + dedupe by `slotUtcMs`: the live slot listener commits every
 *      slot to the ring immediately (deduped); the snapshot's `ringTail` is
 *      merged in, also deduped — so a slot seen both live AND in the tail
 *      appears exactly once.
 *   4. A monotonic generation token gates every async snapshot commit: an
 *      unmounted provider commits nothing, and an older snapshot resolving
 *      after a newer one is discarded.
 *   5. Any `ft8-listening:change` (a) overlays its live axis/flags/phase/band/
 *      sweep onto the exposed snapshot immediately (so uiState reacts without
 *      waiting) and (b) schedules a coalesced (~150 ms debounce) re-hydrate to
 *      refresh the non-event snapshot fields (sweepConfig, availableDevices,
 *      counters, ringTail).
 *
 * `.catch(() => {})` on `listen`/`invoke` keeps jsdom / no-Tauri contexts quiet.
 */

import {
  createContext,
  createElement,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  FT8_LISTENING_EVENT,
  FT8_SLOT_EVENT,
  FT8_SNAPSHOT_COMMAND,
  type BandDot,
  type Ft8Flags,
  type Ft8ListeningChange,
  type Ft8Snapshot,
  type Ft8UiState,
  type SlotRecord,
} from './ft8Types';
import { deriveUiState } from './deriveUiState';
import { deriveBandActivity } from './deriveBandActivity';

/** Ring capacity — oldest evicted past this (§Frontend data layer). */
const RING_CAPACITY = 240;
/** Coalesce window for :change-triggered re-hydration (§Frontend data layer). */
const REHYDRATE_DEBOUNCE_MS = 150;

export interface Ft8ListenerValue {
  /** Full listener state; `null` until the first snapshot resolves. */
  snapshot: Ft8Snapshot | null;
  /** The decode ring, oldest→newest, capped at 240. */
  decodesRing: SlotRecord[];
  /** Derived UI state + flags (Task B2 owns `deriveUiState`; see stub below). */
  uiState: { state: Ft8UiState; flags: Ft8Flags };
  /** Per-band activity (Task B3 owns `deriveBandActivity`; see stub below). */
  bandActivity: Map<string, BandDot>;
}

const Ft8ListenerContext = createContext<Ft8ListenerValue | null>(null);

// ---------------------------------------------------------------------------
// Loading-window default: before the first snapshot resolves, there is no
// service state to derive. B2's deriveUiState is total over a real snapshot;
// the null case is the pre-hydrate loading window (uiState 'off', all flags
// clear). Consumers treat snapshot===null as loading (see the hook docs).
const LOADING_UI_STATE: { state: Ft8UiState; flags: Ft8Flags } = {
  state: 'off',
  flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
};

// ---------------------------------------------------------------------------
// Ring helpers — pure, dedupe by slotUtcMs, chronological, bounded at 240.
// ---------------------------------------------------------------------------

function insertSlot(ring: SlotRecord[], rec: SlotRecord): SlotRecord[] {
  if (ring.some((r) => r.slotUtcMs === rec.slotUtcMs)) return ring; // dedupe: unchanged
  return capRing([...ring, rec].sort((a, b) => a.slotUtcMs - b.slotUtcMs));
}

function mergeTail(ring: SlotRecord[], tail: SlotRecord[]): SlotRecord[] {
  const seen = new Set(ring.map((r) => r.slotUtcMs));
  const next = ring.slice();
  let added = false;
  for (const rec of tail) {
    if (!seen.has(rec.slotUtcMs)) {
      seen.add(rec.slotUtcMs);
      next.push(rec);
      added = true;
    }
  }
  if (!added) return ring; // nothing new: unchanged reference
  return capRing(next.sort((a, b) => a.slotUtcMs - b.slotUtcMs));
}

function capRing(ring: SlotRecord[]): SlotRecord[] {
  return ring.length > RING_CAPACITY ? ring.slice(ring.length - RING_CAPACITY) : ring;
}

/** Overlay a listening-change's live fields onto the base snapshot. */
function applyChange(base: Ft8Snapshot, change: Ft8ListeningChange): Ft8Snapshot {
  return {
    ...base,
    service: change.service,
    flags: change.flags,
    slotPhase: change.slotPhase,
    band: change.band,
    dialHz: change.dialHz,
    sweep: change.sweep,
  };
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export function Ft8ListenerProvider({ children }: { children: ReactNode }) {
  const [snapshot, setSnapshot] = useState<Ft8Snapshot | null>(null);
  const [change, setChange] = useState<Ft8ListeningChange | null>(null);
  const [ring, setRing] = useState<SlotRecord[]>([]);

  // Mutable machinery (avoids stale closures in async/listener callbacks).
  const mountedRef = useRef(true);
  const generationRef = useRef(0);
  const ringRef = useRef<SlotRecord[]>([]);
  const rehydrateTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    mountedRef.current = true;

    const commitRing = (next: SlotRecord[]) => {
      if (next === ringRef.current) return; // helper returned same ref = no change
      ringRef.current = next;
      setRing(next);
    };

    // Step 4: generation-gated snapshot fetch.
    const hydrate = () => {
      const gen = (generationRef.current += 1);
      invoke<Ft8Snapshot>(FT8_SNAPSHOT_COMMAND)
        .then((snap) => {
          if (!mountedRef.current) return; // unmounted → commit nothing
          if (gen !== generationRef.current) return; // stale (newer in flight) → discard
          setSnapshot(snap);
          commitRing(mergeTail(ringRef.current, snap.ringTail)); // Step 3: replay + dedupe
        })
        .catch(() => {
          // jsdom / no-Tauri / command unavailable — poll-less, listeners still live.
        });
    };

    const scheduleRehydrate = () => {
      if (rehydrateTimerRef.current) clearTimeout(rehydrateTimerRef.current);
      rehydrateTimerRef.current = setTimeout(() => {
        rehydrateTimerRef.current = null;
        hydrate();
      }, REHYDRATE_DEBOUNCE_MS);
    };

    let unlistenSlot: (() => void) | null = null;
    let unlistenChange: (() => void) | null = null;
    let disposed = false;

    // Step 1: register BOTH listeners FIRST (buffer early events into the ring).
    listen<SlotRecord>(FT8_SLOT_EVENT, (e) => {
      if (!mountedRef.current) return;
      commitRing(insertSlot(ringRef.current, e.payload)); // live slot: dedupe + append
    })
      .then((u) => {
        if (disposed) u();
        else unlistenSlot = u;
      })
      .catch(() => {});

    listen<Ft8ListeningChange>(FT8_LISTENING_EVENT, (e) => {
      if (!mountedRef.current) return;
      setChange(e.payload); // Step 5a: overlay live fields immediately
      scheduleRehydrate(); // Step 5b: coalesced re-hydrate
    })
      .then((u) => {
        if (disposed) u();
        else unlistenChange = u;
      })
      .catch(() => {});

    // Step 2: fetch the snapshot AFTER the listeners are registering.
    hydrate();

    return () => {
      mountedRef.current = false;
      disposed = true;
      if (rehydrateTimerRef.current) {
        clearTimeout(rehydrateTimerRef.current);
        rehydrateTimerRef.current = null;
      }
      if (unlistenSlot) unlistenSlot();
      if (unlistenChange) unlistenChange();
    };
    // Mount-once: a single subscription per provider (no re-subscribe churn).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const value = useMemo<Ft8ListenerValue>(() => {
    // The latest listening-change overlays the snapshot's live fields; the
    // snapshot supplies the non-event fields (sweepConfig, devices, counters).
    const merged = snapshot && change ? applyChange(snapshot, change) : snapshot;
    return {
      snapshot: merged,
      decodesRing: ring,
      uiState: merged ? deriveUiState(merged) : LOADING_UI_STATE,
      bandActivity: deriveBandActivity(ring, Date.now()),
    };
  }, [snapshot, change, ring]);

  return createElement(Ft8ListenerContext.Provider, { value }, children);
}

/**
 * Read the FT8 listener value. Must be called within an `Ft8ListenerProvider`.
 */
export function useFt8Listener(): Ft8ListenerValue {
  const ctx = useContext(Ft8ListenerContext);
  if (!ctx) {
    throw new Error('useFt8Listener must be used within an Ft8ListenerProvider');
  }
  return ctx;
}
