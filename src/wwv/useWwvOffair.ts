// Hook: drive one off-air WWV decode arm/capture cycle from the UI (a manual
// action, not an effect — WWV bulletins are minute-aligned and the operator
// picks the moment, so there is no auto-run-on-mount path here).
//
// Task 16 extends the Task 14 shape with real window scheduling: the backend
// `wwv_offair_refresh` captures IMMEDIATELY when invoked (arecord -d 70 now),
// but the space-weather bulletin only airs at :18 (WWV) / :45 (WWVH) past the
// hour. `arm()` no longer calls the backend directly — it computes the next
// capture window via window.ts's pure `nextCapture()`, goes 'armed', and
// schedules a one-shot `setTimeout` that fires the actual capture at (or
// immediately inside) the window. A `no_copy` outcome auto-retries once by
// re-arming for the following window; `cancel()` clears the pending timer.

import { useCallback, useEffect, useRef, useState } from 'react';
import { nextCapture } from './window';
import {
  catConfigured as catConfiguredApi,
  manualIngest as manualIngestApi,
  readSnapshot,
  refreshOffair,
  type SolarSnapshot,
  type WwvRefreshOutcome,
} from './wwvApi';

export type WwvOffairStatus = 'idle' | 'armed' | 'capturing' | 'done' | 'nocopy' | 'error';

export interface UseWwvOffairResult {
  status: WwvOffairStatus;
  result: WwvRefreshOutcome | null;
  snapshot: SolarSnapshot | null;
  windowLabel: string | null;
  wavPath: string | null;
  catConfigured: boolean | null;
  arm(nowMs: number): void;
  cancel(): void;
  refreshSnapshot(): Promise<void>;
  refreshCat(): Promise<void>;
  manualIngest(sfi: number, aIndex: number | null, kIndex: number | null): Promise<void>;
}

export function useWwvOffair(): UseWwvOffairResult {
  const [status, setStatus] = useState<WwvOffairStatus>('idle');
  const [result, setResult] = useState<WwvRefreshOutcome | null>(null);
  const [snapshot, setSnapshot] = useState<SolarSnapshot | null>(null);
  const [windowLabel, setWindowLabel] = useState<string | null>(null);
  const [wavPath, setWavPath] = useState<string | null>(null);
  const [catConfigured, setCatConfigured] = useState<boolean | null>(null);

  // Pending setTimeout handle for the armed capture — cleared on cancel(),
  // on a fresh arm(), and on unmount.
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Tracks whether the current arm cycle has already used its one no-copy
  // retry. Reset to false at the start of every USER-initiated arm() (not
  // the internal auto-retry re-arm), so a fresh manual arm always gets a
  // clean retry budget.
  const retriedRef = useRef(false);
  // Holds the latest armInternal so fireCapture (defined before armInternal
  // below) can call it for the auto-retry re-arm without a textual forward
  // reference. Populated by the effect right after armInternal is created.
  const armInternalRef = useRef<(nowMs: number, isRetry: boolean) => void>(() => {});
  // Tracks whether the hook is still mounted. fireCapture's async chain
  // (await refreshOffair, await refreshSnapshot) can still be in flight when
  // the component unmounts; every setState after an await checks this guard
  // before dispatching, so a late-resolving promise never touches state on
  // an unmounted hook (React dev "setState after unmount" warning + wasted
  // update). Set false in the unmount effect below.
  const mountedRef = useRef(true);

  const refreshSnapshot = useCallback(async () => {
    const snap = await readSnapshot();
    if (!mountedRef.current) return;
    setSnapshot(snap);
  }, []);

  // Fetched once (mount effect in the component), same swallow-errors shape
  // as refreshSnapshot: this hits Tauri's invoke, which throws outside a real
  // Tauri webview, and a failed background prefetch must never crash the
  // host control.
  const refreshCat = useCallback(async () => {
    const configured = await catConfiguredApi();
    if (!mountedRef.current) return;
    setCatConfigured(configured);
  }, []);

  const manualIngest = useCallback(
    async (sfi: number, aIndex: number | null, kIndex: number | null) => {
      try {
        await manualIngestApi(sfi, aIndex, kIndex, Date.now());
        if (!mountedRef.current) return;
        setStatus('done');
        await refreshSnapshot();
      } catch {
        if (!mountedRef.current) return;
        setStatus('error');
      }
    },
    [refreshSnapshot],
  );

  const clearTimer = useCallback(() => {
    if (timeoutRef.current != null) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
  }, []);

  // Fires when the armed timer elapses: does the actual capture call and
  // resolves the outcome into a status. On no_copy, auto-retries once by
  // re-arming for the next window via armInternalRef.
  const fireCapture = useCallback(async () => {
    timeoutRef.current = null;
    setStatus('capturing');
    try {
      const outcome = await refreshOffair(Date.now());
      if (!mountedRef.current) return;
      setResult(outcome);
      setWavPath(outcome.wav_path);
      if (outcome.no_copy) {
        // Retry-once default: mirrors the backend's
        // WwvOffairConfig.auto_retry_next_window (default true). The config
        // field itself isn't consulted here yet — this is a hardcoded
        // retry-once, matching that default. Wiring the real config value
        // through is left for a follow-up.
        if (!retriedRef.current) {
          retriedRef.current = true;
          armInternalRef.current(Date.now(), true);
        } else {
          setStatus('nocopy');
        }
      } else {
        setStatus('done');
      }
    } catch {
      if (!mountedRef.current) return;
      setResult(null);
      setStatus('error');
    }
    // Refresh the snapshot regardless of outcome: even a no_copy/error
    // capture doesn't change persisted state, but keeping the read on the
    // same path as the write keeps the panel's displayed data honest with
    // whatever's actually on disk. Skip entirely once unmounted — no point
    // firing the IPC read just to discard the result.
    if (!mountedRef.current) return;
    await refreshSnapshot();
  }, [refreshSnapshot]);

  const armInternal = useCallback(
    (nowMs: number, isRetry: boolean) => {
      if (!isRetry) {
        retriedRef.current = false;
        // A fresh user-initiated arm starts a new capture cycle — any clip
        // from a prior cycle is stale, so drop the reference. The internal
        // auto-retry re-arm (isRetry === true) deliberately keeps it: there's
        // no new clip yet, and the retry is still resolving the same user
        // gesture.
        setWavPath(null);
      }
      clearTimer();
      const next = nextCapture(nowMs);
      setWindowLabel(next.label);
      setStatus('armed');
      timeoutRef.current = setTimeout(() => {
        void fireCapture();
      }, next.delayMs);
    },
    [clearTimer, fireCapture],
  );

  useEffect(() => {
    armInternalRef.current = armInternal;
  }, [armInternal]);

  const arm = useCallback(
    (nowMs: number) => {
      armInternal(nowMs, false);
    },
    [armInternal],
  );

  const cancel = useCallback(() => {
    clearTimer();
    setStatus('idle');
    setWindowLabel(null);
  }, [clearTimer]);

  // Clear any pending timer on unmount so a fire-after-unmount never
  // dispatches state updates on an unmounted component, and flip
  // mountedRef so fireCapture's async chain — already in flight and past
  // the timer entirely — also stops short of calling setState.
  useEffect(
    () => () => {
      clearTimer();
      mountedRef.current = false;
    },
    [clearTimer],
  );

  return {
    status,
    result,
    snapshot,
    windowLabel,
    wavPath,
    catConfigured,
    arm,
    cancel,
    refreshSnapshot,
    refreshCat,
    manualIngest,
  };
}
