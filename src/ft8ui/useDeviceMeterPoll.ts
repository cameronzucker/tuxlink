// src/ft8ui/useDeviceMeterPoll.ts
//
// Extracted from Ft8SetupSurface.tsx (Task C9a/C9b) so the compact in-strip
// setup form (Ft8StripSetup, Task 1 of the Station Intelligence usability
// series) can share the same live-meter polling primitive without importing
// the full-panel surface. Logic is verbatim from the original inline hook:
// same METER_POLL_MS, same enabled-dependency resume contract, same
// stopAndAwait race-safety handover.
//
// Per-row live meter, polls `ft8_device_meter` at ~2 Hz while `enabled`.
// Exposes `stopAndAwait`, the race-safety handover primitive (§FirstRun
// "Meter/start handover"): stops future polls immediately AND awaits any
// poll already in flight, so the device handle is guaranteed released before
// the caller proceeds to `ft8_set_device`. The `enabled` prop doubles as the
// RESUME signal: once the caller's busy flag drops back to false, this
// effect's `enabled` dependency flips true again and polling restarts on its
// own, no separate resume primitive needed.

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Ft8CmdError, MeterDto, StableAudioId } from './ft8Types';

/** ~2 Hz per §FirstRun Step 1 ("live level meter … poll ~2 Hz while the
 *  picker is visible"). */
const METER_POLL_MS = 500;

function isFt8CmdError(e: unknown): e is Ft8CmdError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'detail' in e;
}

function cmdErrorMessage(e: unknown): string {
  if (isFt8CmdError(e)) return e.detail;
  if (e instanceof Error) return e.message;
  return 'Something went wrong, try again.';
}

export interface DeviceMeterState {
  meter: MeterDto | null;
  error: Ft8CmdError | null;
  stopAndAwait: () => Promise<void>;
}

export function useDeviceMeterPoll(stableId: StableAudioId, enabled: boolean): DeviceMeterState {
  const [meter, setMeter] = useState<MeterDto | null>(null);
  const [error, setError] = useState<Ft8CmdError | null>(null);

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const inFlightRef = useRef<Promise<void> | null>(null);
  const stoppedRef = useRef(false);
  const mountedRef = useRef(true);
  const idRef = useRef(stableId);
  idRef.current = stableId;

  const poll = useCallback(() => {
    if (stoppedRef.current) return;
    const id = idRef.current;
    const p = invoke<MeterDto>('ft8_device_meter', { stableId: id })
      .then((m) => {
        if (!mountedRef.current || stoppedRef.current) return;
        setMeter(m);
        setError(null);
      })
      .catch((e: unknown) => {
        if (!mountedRef.current || stoppedRef.current) return;
        // ft8_device_meter's real error kinds: device-not-found |
        // device-reserved | internal-error (never device-in-use, a busy
        // device is the Ok state:'in-use' value, handled by the caller).
        setError(isFt8CmdError(e) ? e : { kind: 'internal-error', detail: cmdErrorMessage(e) });
      })
      .finally(() => {
        inFlightRef.current = null;
      });
    inFlightRef.current = p;
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    stoppedRef.current = false;
    if (!enabled) return undefined;

    poll(); // immediate first read, then ~2 Hz
    timerRef.current = setInterval(poll, METER_POLL_MS);

    return () => {
      mountedRef.current = false;
      stoppedRef.current = true;
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
    };
    // `stableId` changes are handled via the `key`-remounted caller, not an
    // effect dependency here; idRef.current always tracks the latest.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, poll]);

  const stopAndAwait = useCallback(async () => {
    stoppedRef.current = true;
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    if (inFlightRef.current) {
      await inFlightRef.current;
    }
  }, []);

  return { meter, error, stopAndAwait };
}
