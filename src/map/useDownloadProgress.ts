/**
 * useDownloadProgress (tuxlink-9n9t) — turns the backend's
 * `basemap:download-progress` / `basemap:download-done` event stream into a
 * render-ready snapshot for one active pack download.
 *
 * The backend (basemap/commands.rs) polls the in-progress `.part` size and emits
 * a throttled `{ packId, bytes, total }` progress event, plus a terminal
 * `{ packId, ok, error }` done event. This hook subscribes (mirroring
 * useStatus.ts's listen/unlisten effect) while a download is `active`. Because
 * the backend `install_lock` serializes downloads (only one runs machine-wide at
 * a time) and the UI disables every other pack's button during a run, the hook
 * latches onto whichever `packId` emits — the caller (OfflineMapsSettings) knows
 * its own busy key but not the backend's resolved pack id, so matching on id here
 * would be brittle. It derives:
 *   - percent  = clamp(bytes/total, 0..0.999) until done → 1
 *   - rateBps  = smoothed (EMA) bytes/sec over event-arrival deltas
 *   - etaSecs  = (total - bytes) / rateBps
 *   - status   = idle | downloading | done | error | cancelled
 *
 * Rate uses `performance.now()` (event arrival time), not the byte timestamps,
 * so a slow/bursty emitter still yields a usable rate.
 */
import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  DOWNLOAD_PROGRESS_EVENT,
  DOWNLOAD_DONE_EVENT,
  type DownloadProgress,
  type DownloadDone,
} from './offlineMaps';

export type DownloadStatus = 'idle' | 'downloading' | 'done' | 'error' | 'cancelled';

export interface DownloadProgressView {
  bytes: number;
  /**
   * Effective denominator for the bar: max(estimate, bytes) so a real extract
   * larger than the manifest `typical_bytes` estimate never pins the bar at 99%
   * or shows "1.4 GB / 1.0 GB" (C4). When bytes reach/exceed the estimate this
   * equals bytes, and `finishing` flips true to render the bar indeterminate.
   */
  total: number;
  /** 0..1; clamped below 1 while downloading, exactly 1 once done-ok. */
  percent: number;
  /**
   * True while downloading once bytes are within epsilon of (or past) the
   * estimate — render the bar indeterminate / "finishing…" rather than a stuck
   * 99%, since the real size is unknown until done (C4).
   */
  finishing: boolean;
  /** Smoothed transfer rate in bytes/sec; null until two samples seen. */
  rateBps: number | null;
  /** Estimated seconds remaining; null until a rate is known. */
  etaSecs: number | null;
  status: DownloadStatus;
  /** Failure reason when status === 'error'. */
  error: string | null;
  /** The backend pack id this run latched onto, for `basemap_cancel_download`. */
  trackedId: string | null;
}

/** EMA smoothing factor for the rate (higher = more weight on recent samples). */
const RATE_ALPHA = 0.3;

/** Within this fraction of the estimate, treat the bar as "finishing" (the real
 * extract size is unknown until the done event, so don't pin a stuck 99%). */
const FINISHING_EPSILON = 0.005;

const IDLE: DownloadProgressView = {
  bytes: 0,
  total: 0,
  percent: 0,
  finishing: false,
  rateBps: null,
  etaSecs: null,
  status: 'idle',
  error: null,
  trackedId: null,
};

/** The backend signals cancel via a done event whose error message is the
 * Cancelled variant's Display string. */
function isCancelled(error: string | null): boolean {
  return error === 'download cancelled';
}

/**
 * @param active a key identifying the in-flight download (the caller's busy key),
 *   or `null` when nothing is downloading. The value is opaque to the hook — it
 *   only gates the subscription and resets state on change; the hook latches onto
 *   whichever backend `packId` actually emits.
 */
export function useDownloadProgress(active: string | null): DownloadProgressView {
  const [view, setView] = useState<DownloadProgressView>(IDLE);
  // The backend pack id this run is tracking (latched from the first event).
  const trackedId = useRef<string | null>(null);
  // Last sample for rate computation: { bytes, at } where `at` = performance.now().
  const lastSample = useRef<{ bytes: number; at: number } | null>(null);
  const rateRef = useRef<number | null>(null);

  // Reset whenever the active download changes (a new download, or none).
  useEffect(() => {
    setView(IDLE);
    trackedId.current = null;
    lastSample.current = null;
    rateRef.current = null;
  }, [active]);

  useEffect(() => {
    if (!active) return;
    let mounted = true;
    const unlisteners: Array<() => void> = [];

    listen<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, (event) => {
      const p = event.payload;
      if (!mounted) return;
      // Latch onto the first packId we see; ignore any other concurrent emitter.
      if (trackedId.current == null) trackedId.current = p.packId;
      if (p.packId !== trackedId.current) return;
      const now = performance.now();
      const prev = lastSample.current;
      if (prev && now > prev.at && p.bytes >= prev.bytes) {
        const inst = ((p.bytes - prev.bytes) * 1000) / (now - prev.at); // bytes/sec
        rateRef.current =
          rateRef.current == null ? inst : RATE_ALPHA * inst + (1 - RATE_ALPHA) * rateRef.current;
      }
      lastSample.current = { bytes: p.bytes, at: now };

      const rate = rateRef.current;
      // C4: the estimate (p.total = manifest typical_bytes) can be smaller than
      // the real extract. Clamp the denominator UP so percent never exceeds 100%
      // and `current` never exceeds `total` ("1.4 GB / 1.0 GB"). When bytes meet
      // or pass the estimate, the true size is unknown — flag `finishing` so the
      // bar renders indeterminate rather than pinned at a stuck 99%.
      const estimate = p.total;
      const effectiveTotal = Math.max(estimate, p.bytes);
      const finishing = estimate > 0 && p.bytes >= estimate * (1 - FINISHING_EPSILON);
      const remaining = Math.max(0, effectiveTotal - p.bytes);
      const eta = !finishing && rate && rate > 0 ? remaining / rate : null;
      const percent = finishing
        ? 0.999
        : effectiveTotal > 0
          ? Math.min(p.bytes / effectiveTotal, 0.999)
          : 0;
      setView({
        bytes: p.bytes,
        total: effectiveTotal,
        percent,
        finishing,
        rateBps: rate,
        etaSecs: eta,
        status: 'downloading',
        error: null,
        trackedId: trackedId.current,
      });
    })
      .then((u) => (mounted ? unlisteners.push(u) : u()))
      .catch(() => {
        /* listen() unavailable (test env without the mock / no Tauri) — no-op. */
      });

    listen<DownloadDone>(DOWNLOAD_DONE_EVENT, (event) => {
      const d = event.payload;
      if (!mounted) return;
      // Accept the done for the tracked pack, or — if no progress event arrived
      // first (instant failure) — latch onto it here.
      if (trackedId.current == null) trackedId.current = d.packId;
      if (d.packId !== trackedId.current) return;
      setView((v) => {
        const base = { ...v, trackedId: trackedId.current };
        if (d.ok) {
          return { ...base, percent: 1, finishing: false, status: 'done' as const, error: null };
        }
        if (isCancelled(d.error)) {
          return { ...base, status: 'cancelled' as const, error: null };
        }
        return { ...base, status: 'error' as const, error: d.error };
      });
    })
      .then((u) => (mounted ? unlisteners.push(u) : u()))
      .catch(() => {
        /* see above */
      });

    return () => {
      mounted = false;
      for (const u of unlisteners) u();
    };
  }, [active]);

  return view;
}
