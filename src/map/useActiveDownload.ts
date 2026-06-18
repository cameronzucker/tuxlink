/**
 * useActiveDownload (tuxlink-8g28) — an APP-LEVEL subscription to the basemap
 * pack-download event stream, for the status bar's ambient progress indicator.
 *
 * Unlike `useDownloadProgress` (which the Offline-maps panel keys to its own busy
 * state and decorates with rate/eta), this hook is ALWAYS listening and tracks
 * whichever pack download is in flight — so progress stays visible after the
 * operator navigates away from Settings (flow 1: "show progress … on the status
 * bar"; flow 2: returning to a running download isn't blind). The backend
 * `install_lock` serialises downloads (one at a time machine-wide), so latching
 * onto whichever pack emits is unambiguous.
 *
 * Returns `null` when nothing is downloading; clears on the terminal done event
 * (ok / error / cancel alike — the panel owns the detailed error copy, the status
 * bar just stops showing the in-flight indicator).
 */
import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  DOWNLOAD_PROGRESS_EVENT,
  DOWNLOAD_DONE_EVENT,
  type DownloadProgress,
  type DownloadDone,
} from './offlineMaps';

export interface ActiveDownload {
  /** The backend pack id currently downloading. */
  packId: string;
  /** Bytes written to the in-progress `.part` so far. */
  bytes: number;
  /** Effective denominator: `max(estimate, bytes)` so the bar never exceeds 100%. */
  total: number;
  /** 0..1, clamped below 1 while running (exactly reported only at done, which clears). */
  percent: number;
  /** True once bytes reach/pass the estimate — true size unknown, render as "finishing". */
  finishing: boolean;
}

/** Within this fraction of the estimate, treat the download as "finishing" (the
 * real extract size is unknown until done — mirrors useDownloadProgress's C4). */
const FINISHING_EPSILON = 0.005;

export function useActiveDownload(): ActiveDownload | null {
  const [active, setActive] = useState<ActiveDownload | null>(null);

  useEffect(() => {
    let mounted = true;
    const unlisteners: Array<() => void> = [];
    // Defensive: `listen` is unavailable outside a Tauri webview (plain unit tests
    // without the mock); a sync throw or async reject must just leave the indicator
    // idle, never crash the shell.
    const sub = <T>(name: string, cb: (payload: T) => void) => {
      try {
        listen<T>(name, (e) => {
          if (mounted) cb(e.payload as T);
        })
          .then((u) => (mounted ? unlisteners.push(u) : u()))
          .catch(() => {});
      } catch {
        /* listen unavailable — no-op */
      }
    };

    sub<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, (p) => {
      const estimate = p.total;
      const effectiveTotal = Math.max(estimate, p.bytes);
      const finishing = estimate > 0 && p.bytes >= estimate * (1 - FINISHING_EPSILON);
      const percent = finishing
        ? 0.999
        : effectiveTotal > 0
          ? Math.min(p.bytes / effectiveTotal, 0.999)
          : 0;
      setActive({ packId: p.packId, bytes: p.bytes, total: effectiveTotal, percent, finishing });
    });

    sub<DownloadDone>(DOWNLOAD_DONE_EVENT, () => {
      // Terminal: clear the ambient indicator regardless of ok/error/cancel.
      setActive(null);
    });

    return () => {
      mounted = false;
      for (const u of unlisteners) u();
    };
  }, []);

  return active;
}
