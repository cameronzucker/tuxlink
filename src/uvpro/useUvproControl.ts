// useUvproControl — the always-live UV-Pro native control session, surfaced to the
// control strip (tuxlink-ve3j). Subscribes to the `uvpro:status` broadcast (the
// backend's ~2 s status poller) and exposes the connect / disconnect / channel
// commands. The session shares ONE Bluetooth connection with native APRS chat
// (tuxlink-7my9), so connecting here is the prerequisite for native APRS listening.
//
// RF-honesty: the strip never optimistically flips state — `status` reflects the
// backend snapshot (command return + broadcast), never a hopeful local guess.
import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  UVPRO_STATUS_EVENT,
  uvproErrorMessage,
  type UvproChannel,
  type UvproStatus,
} from './uvproTypes';

const DISCONNECTED: UvproStatus = {
  state: 'disconnected',
  isTx: false,
  isRx: false,
  squelchOpen: false,
  powerOn: false,
  gpsLocked: false,
};

export interface UseUvproControl {
  status: UvproStatus;
  channels: UvproChannel[];
  /** A command is in flight (connect / disconnect / channel change). */
  busy: boolean;
  /** Last command error, or null. Cleared at the start of each command. */
  error: string | null;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  setChannel: (channelId: number) => Promise<void>;
}

export function useUvproControl(): UseUvproControl {
  const [status, setStatus] = useState<UvproStatus>(DISCONNECTED);
  const [channels, setChannels] = useState<UvproChannel[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Initial snapshot + live subscription. The backend is the source of truth:
  // `state` flips when the broadcaster emits, never optimistically here.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void invoke<UvproStatus>('uvpro_get_status')
      .then((s) => {
        if (!cancelled && s) setStatus(s);
      })
      .catch(() => {
        /* no Tauri context (test env) — keep the disconnected default */
      });

    try {
      void listen<UvproStatus>(UVPRO_STATUS_EVENT, (event) => {
        if (!cancelled && event.payload) setStatus(event.payload);
      }).then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    } catch {
      /* listen() unavailable (test env) — the command returns still drive state */
    }

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  // Channel memories are only meaningful while connected; (re)load on connect,
  // clear on disconnect so a stale list never implies a live radio.
  useEffect(() => {
    if (status.state !== 'connected') {
      setChannels([]);
      return;
    }
    void invoke<UvproChannel[]>('uvpro_get_channels')
      .then((list) => setChannels(Array.isArray(list) ? list : []))
      .catch(() => setChannels([]));
  }, [status.state]);

  const run = useCallback(
    async (op: () => Promise<UvproStatus | undefined>) => {
      setBusy(true);
      setError(null);
      try {
        const next = await op();
        if (next) setStatus(next);
      } catch (err) {
        setError(uvproErrorMessage(err));
      } finally {
        setBusy(false);
      }
    },
    [],
  );

  const connect = useCallback(
    () => run(() => invoke<UvproStatus>('uvpro_connect', {})),
    [run],
  );
  const disconnect = useCallback(
    () => run(() => invoke<UvproStatus>('uvpro_disconnect')),
    [run],
  );
  const setChannel = useCallback(
    (channelId: number) =>
      run(() => invoke<UvproStatus>('uvpro_set_channel', { channelId })),
    [run],
  );

  return { status, channels, busy, error, connect, disconnect, setChannel };
}
