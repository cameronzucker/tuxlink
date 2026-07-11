// Hook: drive one off-air WWV decode arm/capture cycle from the UI (a manual
// action, not an effect — WWV bulletins are minute-aligned and the operator
// picks the moment, so there is no auto-run-on-mount path here). Kept
// deliberately simple: `arm()` calls the backend, maps the outcome to a
// status, and refreshes the persisted snapshot so the propagation panel picks
// up the new indices. The fuller no-copy retry / rearm flow lands in Task 16.

import { useCallback, useState } from 'react';
import { readSnapshot, refreshOffair, type SolarSnapshot, type WwvRefreshOutcome } from './wwvApi';

export type WwvOffairStatus = 'idle' | 'capturing' | 'done' | 'nocopy' | 'error';

export interface UseWwvOffairResult {
  status: WwvOffairStatus;
  result: WwvRefreshOutcome | null;
  snapshot: SolarSnapshot | null;
  arm(nowMs: number): Promise<void>;
  refreshSnapshot(): Promise<void>;
}

export function useWwvOffair(): UseWwvOffairResult {
  const [status, setStatus] = useState<WwvOffairStatus>('idle');
  const [result, setResult] = useState<WwvRefreshOutcome | null>(null);
  const [snapshot, setSnapshot] = useState<SolarSnapshot | null>(null);

  const refreshSnapshot = useCallback(async () => {
    const snap = await readSnapshot();
    setSnapshot(snap);
  }, []);

  const arm = useCallback(
    async (nowMs: number) => {
      setStatus('capturing');
      try {
        const outcome = await refreshOffair(nowMs);
        setResult(outcome);
        setStatus(outcome.no_copy ? 'nocopy' : 'done');
      } catch {
        setResult(null);
        setStatus('error');
      }
      // Refresh the snapshot regardless of outcome: even a no_copy/error
      // capture doesn't change persisted state, but keeping the read on the
      // same path as the write keeps the panel's displayed data honest with
      // whatever's actually on disk (e.g. a stale SWPC/RF snapshot from
      // before this capture attempt).
      await refreshSnapshot();
    },
    [refreshSnapshot],
  );

  return { status, result, snapshot, arm, refreshSnapshot };
}
