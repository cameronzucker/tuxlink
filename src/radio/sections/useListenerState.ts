// src/radio/sections/useListenerState.ts
//
// Shared hook that fetches + maintains a transport's allowed-stations
// list and exposes mutation helpers wired to the transport's Tauri
// commands. Each transport has its own command-name set (telnet_*,
// packet_*, ardop_*), so the caller passes a small descriptor object
// rather than the hook hardcoding any transport. Keeps the three panel
// implementations identical above the parameterisation.
//
// Also tracks armed-state + a derived "minutes remaining" countdown
// computed from the moment the arm call resolved + the TTL the operator
// chose. The exact ttl from the backend isn't surfaced via a Tauri get
// call for Packet/ARDOP, so we use a UI-side default (60 minutes) for
// those; Telnet's `telnet_listen_config_get` gives the real TTL and
// `useListenerState`'s `ttlSecs` parameter lets the Telnet panel feed
// that in.

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface AllowedStations {
  allowAll: boolean;
  callsigns: string[];
  ips: string[];
}

export interface ListenerCommandSet {
  /** Tauri command name for `*_listen()` — arms the listener. */
  listen: string;
  /** Tauri command name for `*_set_listen({ enabled })` — toggle. */
  setListen: string;
  /** Tauri command name for `*_allowed_stations_get()` — returns the
   *  allowlist. The wire shape differs slightly per transport (Packet
   *  has no `ips` field); the response normaliser handles that. */
  allowedGet: string;
  /** Tauri command name + arg-key for adding a callsign. The arg-key
   *  varies (Telnet uses `callsign`, Packet uses `callsign`); kept as a
   *  parameter for forward compatibility. */
  allowedAddCallsign: string;
  allowedAddCallsignArgKey: string;
  /** Tauri command name + arg-key for removing a callsign. */
  allowedRemoveCallsign: string;
  allowedRemoveCallsignArgKey: string;
  /** Set-allow-all command + arg-key. Telnet's wire shape uses
   *  `{ enabled }`; Packet + ARDOP use `{ allow_all }`. */
  allowedSetAllowAll: string;
  allowedSetAllowAllArgKey: string;
  /** Optional IP-pattern commands (Telnet only). */
  allowedAddIp?: string;
  allowedAddIpArgKey?: string;
  allowedRemoveIp?: string;
  allowedRemoveIpArgKey?: string;
}

export interface UseListenerStateOptions {
  commands: ListenerCommandSet;
  /** TTL in seconds (drives the countdown indicator). Default 3600. */
  ttlSecs?: number;
}

export interface UseListenerStateReturn {
  /** TRUE when the operator's last call to `arm()` resolved without
   *  error; flips back to FALSE on disarm or on disarm-confirmed event.
   *  This is a best-effort armed indicator — the backend's arms record
   *  is authoritative. */
  armed: boolean;
  /** Approximate minutes remaining until TTL expiry. null when not
   *  armed or before the first arm completes. */
  minutesRemaining: number | null;
  /** Allowlist as last fetched. Mutates immediately on local edits +
   *  re-fetched after each backend mutation to stay in sync. */
  allowed: AllowedStations;
  /** TRUE during an in-flight arm or disarm call. */
  busy: boolean;
  /** Last error string from any of the calls; cleared on next attempt. */
  error: string | null;
  arm: () => Promise<void>;
  disarm: () => Promise<void>;
  addCallsign: (callsign: string) => Promise<void>;
  removeCallsign: (callsign: string) => Promise<void>;
  addIp: (pattern: string) => Promise<void>;
  removeIp: (pattern: string) => Promise<void>;
  setAllowAll: (enabled: boolean) => Promise<void>;
  refresh: () => Promise<void>;
}

/** Wire shape for `*_allowed_stations_get`. Packet's response lacks
 *  `ips`; we coerce it to an empty array on read so consumers always
 *  see the same shape. The backend uses snake_case for `allow_all`. */
interface AllowedStationsWire {
  allow_all?: boolean;
  allowAll?: boolean;
  callsigns?: string[];
  ips?: string[];
}

function normalize(raw: AllowedStationsWire | null | undefined): AllowedStations {
  const r = raw ?? {};
  return {
    allowAll: r.allow_all ?? r.allowAll ?? true,
    callsigns: r.callsigns ?? [],
    ips: r.ips ?? [],
  };
}

export function useListenerState(options: UseListenerStateOptions): UseListenerStateReturn {
  const { commands, ttlSecs = 3600 } = options;
  const [armed, setArmed] = useState(false);
  const [armedAt, setArmedAt] = useState<number | null>(null);
  const [tick, setTick] = useState(0);
  const [allowed, setAllowed] = useState<AllowedStations>({
    allowAll: true,
    callsigns: [],
    ips: [],
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Bump tick once per second while armed so the countdown re-renders.
  useEffect(() => {
    if (!armed) return;
    const id = setInterval(() => setTick((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, [armed]);

  const minutesRemaining = (() => {
    if (!armed || armedAt === null) return null;
    // tick is referenced here so the effect re-runs each second; the
    // value itself doesn't matter beyond forcing the recompute.
    void tick;
    const elapsedSec = (Date.now() - armedAt) / 1000;
    const remainingSec = Math.max(0, ttlSecs - elapsedSec);
    return Math.floor(remainingSec / 60);
  })();

  const refresh = useCallback(async () => {
    try {
      const raw = await invoke<AllowedStationsWire>(commands.allowedGet);
      setAllowed(normalize(raw));
    } catch (e) {
      // Soft-failure: pre-config errors leave the local state as the
      // backend default (allow_all=true, no lists).
      setError(String(e));
    }
  }, [commands.allowedGet]);

  // Initial fetch.
  useEffect(() => {
    void refresh();
  }, [refresh]);

  const arm = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await invoke(commands.listen);
      setArmed(true);
      setArmedAt(Date.now());
    } catch (e) {
      setError(String(e));
      setArmed(false);
      setArmedAt(null);
    } finally {
      setBusy(false);
    }
  }, [busy, commands.listen]);

  const disarm = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await invoke(commands.setListen, { enabled: false });
      setArmed(false);
      setArmedAt(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [busy, commands.setListen]);

  const addCallsign = useCallback(
    async (callsign: string) => {
      setError(null);
      try {
        await invoke(commands.allowedAddCallsign, {
          [commands.allowedAddCallsignArgKey]: callsign,
        });
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [commands.allowedAddCallsign, commands.allowedAddCallsignArgKey, refresh],
  );

  const removeCallsign = useCallback(
    async (callsign: string) => {
      setError(null);
      try {
        await invoke(commands.allowedRemoveCallsign, {
          [commands.allowedRemoveCallsignArgKey]: callsign,
        });
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [commands.allowedRemoveCallsign, commands.allowedRemoveCallsignArgKey, refresh],
  );

  const addIp = useCallback(
    async (pattern: string) => {
      if (!commands.allowedAddIp || !commands.allowedAddIpArgKey) return;
      setError(null);
      try {
        await invoke(commands.allowedAddIp, {
          [commands.allowedAddIpArgKey]: pattern,
        });
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [commands.allowedAddIp, commands.allowedAddIpArgKey, refresh],
  );

  const removeIp = useCallback(
    async (pattern: string) => {
      if (!commands.allowedRemoveIp || !commands.allowedRemoveIpArgKey) return;
      setError(null);
      try {
        await invoke(commands.allowedRemoveIp, {
          [commands.allowedRemoveIpArgKey]: pattern,
        });
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [commands.allowedRemoveIp, commands.allowedRemoveIpArgKey, refresh],
  );

  const setAllowAll = useCallback(
    async (enabled: boolean) => {
      setError(null);
      try {
        await invoke(commands.allowedSetAllowAll, {
          [commands.allowedSetAllowAllArgKey]: enabled,
        });
        setAllowed((prev) => ({ ...prev, allowAll: enabled }));
      } catch (e) {
        setError(String(e));
      }
    },
    [commands.allowedSetAllowAll, commands.allowedSetAllowAllArgKey],
  );

  return {
    armed,
    minutesRemaining,
    allowed,
    busy,
    error,
    arm,
    disarm,
    addCallsign,
    removeCallsign,
    addIp,
    removeIp,
    setAllowAll,
    refresh,
  };
}
