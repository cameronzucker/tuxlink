// useWeightsJob.ts — live view of the classifier-weights status + job
// (tuxlink-13ofm). One hook, consumed by the wizard step and the Elmer-panel
// gate, so both surfaces render the same job from the same payloads.

import { useCallback, useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  WEIGHTS_PROGRESS_EVENT,
  WEIGHTS_STATUS_EVENT,
  weightsStatus,
  type WeightsProgress,
  type WeightsStatus,
} from './weightsApi';

export interface WeightsJobView {
  /** Latest status; null until the first probe resolves. */
  status: WeightsStatus | null;
  /** Latest byte progress; null when nothing is streaming. */
  progress: WeightsProgress | null;
  /** Re-probe on demand (after a start/cancel invoke, surfaces pass the
   * returned status here instead to save a round trip). */
  refresh: () => Promise<void>;
  /** Push a status we already have (the return value of a start/cancel). */
  accept: (status: WeightsStatus) => void;
}

export function useWeightsJob(): WeightsJobView {
  const [status, setStatus] = useState<WeightsStatus | null>(null);
  const [progress, setProgress] = useState<WeightsProgress | null>(null);
  const alive = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const s = await weightsStatus();
      // Guard nullish payloads (a broken bridge / mocked invoke): a stored
      // `undefined` would slip past `!== null` checks downstream.
      if (alive.current && s != null) setStatus(s);
    } catch {
      // Status probe failing is not renderable state; keep what we have.
    }
  }, []);

  const accept = useCallback((s: WeightsStatus) => {
    if (s != null) setStatus(s);
  }, []);

  useEffect(() => {
    alive.current = true;
    void refresh();

    const subs: Promise<UnlistenFn>[] = [
      listen<WeightsStatus>(WEIGHTS_STATUS_EVENT, (e) => {
        if (!alive.current) return;
        setStatus(e.payload);
        // A terminal phase means no bytes are moving; drop the stale bar.
        const st = e.payload.job?.state;
        if (st !== 'downloading' && st !== 'verifying') setProgress(null);
      }),
      listen<WeightsProgress>(WEIGHTS_PROGRESS_EVENT, (e) => {
        if (alive.current) setProgress(e.payload);
      }),
    ];

    return () => {
      alive.current = false;
      for (const sub of subs) void sub.then((un) => un());
    };
  }, [refresh]);

  return { status, progress, refresh, accept };
}
