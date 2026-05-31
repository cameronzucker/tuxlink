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
