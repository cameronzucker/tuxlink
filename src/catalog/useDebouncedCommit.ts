import { useCallback, useEffect, useRef } from 'react';

// Coalesce a burst of rapid calls into a single trailing invocation.
//
// Motivation (tuxlink-ziyu): the Find-a-Station antenna controls fire onChange
// continuously — the height `type="range"` slider on every grid-index crossing
// mid-drag, the SNR/power `type="number"` inputs on every keystroke. Each event
// previously persisted prefs AND bumped the reachability reload key, kicking off
// a full N-station voacapl re-sweep. A single drag thus launched many overlapping
// sweeps that oversubscribed the Pi's cores and made the map churn for seconds
// (the measured voacapl per-call cost is ~10 ms, so the cost was the firing
// storm, not the compute). Debouncing the commit collapses a gesture into ONE
// persist + ONE recompute once the operator settles.
//
// `commit` runs `delayMs` after the last `schedule(value)` call, with the latest
// value. On unmount a still-pending value is flushed via `flush` (default:
// `commit`) so a final drag/keystroke is not silently dropped; callers pass a
// setState-free `flush` when the trailing `commit` would otherwise touch
// unmounted component state.
export function useDebouncedCommit<T>(
  commit: (value: T) => void,
  delayMs: number,
  flush?: (value: T) => void,
): (value: T) => void {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<{ value: T } | null>(null);
  // Hold the latest callbacks in refs so `schedule` stays referentially stable
  // (safe to pass as a prop / use in deps) yet always invokes the current closure.
  const commitRef = useRef(commit);
  const flushRef = useRef(flush);
  commitRef.current = commit;
  flushRef.current = flush;

  const schedule = useCallback(
    (value: T) => {
      pendingRef.current = { value };
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        const pending = pendingRef.current;
        pendingRef.current = null;
        if (pending) commitRef.current(pending.value);
      }, delayMs);
    },
    [delayMs],
  );

  useEffect(() => {
    return () => {
      if (!timerRef.current) return;
      clearTimeout(timerRef.current);
      timerRef.current = null;
      const pending = pendingRef.current;
      pendingRef.current = null;
      if (pending) (flushRef.current ?? commitRef.current)(pending.value);
    };
  }, []);

  return schedule;
}
