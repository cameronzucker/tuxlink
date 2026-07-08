import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { STOPPED, type ModemStatus } from './types';
import type { ConnectionKey } from '../connections/sessionTypes';
import type { RadioPanelMode } from '../radio/types';

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

/**
 * Map the operator's selected connection to the radio-panel mode it implies
 * (tuxlink-7ppfq, Contract 2). Only the radio protocols (`vara-hf` / `ardop-hf`)
 * have a panel mode; telnet / packet / sonde protocols return `null`. The panel
 * `kind` now tracks the SELECTED protocol rather than a hardcoded `ardop-hf`.
 */
export function connectionToPanelMode(conn: ConnectionKey): RadioPanelMode | null {
  const intent =
    conn.sessionType === 'p2p' || conn.sessionType === 'radio-only'
      ? conn.sessionType
      : 'cms';
  if (conn.protocol === 'vara-hf') return { kind: 'vara-hf', intent };
  if (conn.protocol === 'vara-fm') return { kind: 'vara-fm', intent };
  if (conn.protocol === 'ardop-hf') return { kind: 'ardop-hf', intent };
  return null;
}

/**
 * Panel mode for the active connection, but only while a modem is actually live
 * (tuxlink-7ppfq, Contract 2). Replaces the AppShell `activeModem` hardcode that
 * ALWAYS reported `ardop-hf` regardless of the real selection: now the SELECTED
 * radio protocol drives the panel (so a VARA selection surfaces the VARA panel).
 * When the selection is not a radio protocol, it falls back to `ardop-hf` —
 * `useModemIsActive` reflects the ARDOP modem being live, so ARDOP is the honest
 * default (this preserves "a running ARDOP modem surfaces its panel"). Deduped:
 * derives from `useModemIsActive` (which fires once per state transition, not at
 * the 4 Hz broadcaster cadence) and memoizes on `active`, so the shell does not
 * re-render at the modem-broadcaster tick.
 */
export function useActiveModemMode(active: ConnectionKey): RadioPanelMode | null {
  const isActive = useModemIsActive();
  return useMemo(
    () =>
      isActive
        ? connectionToPanelMode(active) ?? { kind: 'ardop-hf', intent: 'cms' }
        : null,
    [isActive, active.sessionType, active.protocol],
  );
}
