import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { STOPPED, type ModemStatus } from './types';

export const MODEM_STATUS_EVENT = 'modem:status';

export function useModemStatus() {
  const [status, setStatus] = useState<ModemStatus>(STOPPED as ModemStatus);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;

    // Initial snapshot — for the case where the dock mounts mid-session.
    invoke<ModemStatus>('modem_get_status')
      .then((s) => { if (!cancelled) setStatus(s); })
      .catch((e) => { if (!cancelled) setError(String(e)); })
      .finally(() => { if (!cancelled) setLoading(false); });

    // Subscribe to live updates.
    listen<ModemStatus>(MODEM_STATUS_EVENT, (e) => {
      if (!cancelled) setStatus(e.payload);
    }).then((u) => {
      if (cancelled) u();      // already unmounted — tear down immediately
      else unsubscribe = u;
    });

    return () => { cancelled = true; unsubscribe?.(); };
  }, []);

  return { status, loading, error };
}

/**
 * Focused selector for consumers that only need to know whether the modem is
 * currently running. The Rust modem broadcaster emits at 4 Hz; the full
 * [`useModemStatus`] hook re-renders its consumer on every tick. That's
 * desirable for the live-meter panels (sparklines + byte counters) but a
 * pointless render-storm at the AppShell level, which only uses
 * `state !== 'stopped'` to decide whether the radio panel should mount.
 *
 * `useModemIsActive` subscribes to the same event stream but only calls
 * `setState` when the boolean it returns actually changes — once per state
 * transition rather than 4× per second. tuxlink-sndh.
 */
export function useModemIsActive(): boolean {
  const [isActive, setIsActive] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;
    let last: boolean | null = null;

    const apply = (s: ModemStatus) => {
      const next = s.state !== 'stopped';
      // Dedupe: only fire setState on actual change. React 18+ already bails
      // on identical primitive setState, but the explicit gate makes the
      // dedup contract obvious and saves the comparison work.
      if (next !== last) {
        last = next;
        if (!cancelled) setIsActive(next);
      }
    };

    invoke<ModemStatus>('modem_get_status')
      .then((s) => { if (!cancelled) apply(s); })
      .catch(() => { /* swallow — AppShell renders the modem-stopped state regardless */ });

    listen<ModemStatus>(MODEM_STATUS_EVENT, (e) => {
      if (!cancelled) apply(e.payload);
    }).then((u) => {
      if (cancelled) u();
      else unsubscribe = u;
    });

    return () => { cancelled = true; unsubscribe?.(); };
  }, []);

  return isActive;
}
