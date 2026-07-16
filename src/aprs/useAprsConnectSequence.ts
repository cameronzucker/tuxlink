// src/aprs/useAprsConnectSequence.ts
//
// The ONE composed APRS connect/disconnect sequence, shared by every surface
// that hosts the AprsConnectStrip: AppShell's dock header and the popped
// AprsChatSurface (src/dock/surfaceRegistry.tsx). Extracted from AppShell
// (bd-tuxlink-ckmb) so the transport-specific two-step + rollback + teardown
// lives in exactly one place instead of being copied per host (tuxlink-dmwte
// task 10, Rider A — the Task 7 copy in surfaceRegistry.tsx had drifted into a
// second maintenance site for the Codex 2026-06-14 P1 race fix).
//
//   - UvproNative: the engine rides the already-connected UV-Pro session, so
//     connect = uvpro_connect() THEN aprs_listen_start (two steps); disconnect
//     = aprs_listen_stop THEN uvpro_disconnect().
//   - KISS (Tcp/Serial/Bluetooth): the engine opens the link itself, so
//     connect = aprs_listen_start (one step); disconnect = aprs_listen_stop.
//
// The connect/disconnect sequence invokes the uvpro commands DIRECTLY (not via
// useUvproControl().connect(), which swallows errors into its own `error`
// state) so a failure REJECTS and the hosting connect strip surfaces it inline.
// Backend rejects (no active identity) propagate to the strip's inline alert.

import { useCallback, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ModemLinkFields } from '../radio/sections/ModemLinkSection';
import type { PacketLinkKind } from '../packet/packetTypes';

export interface UseAprsConnectSequence {
  /// True while a connect is in flight (shared "Connecting…" state for the
  /// hosting surfaces). Cleared when the connect settles, success or failure.
  connecting: boolean;
  /// Run the transport-appropriate connect, flipping `connecting` for its
  /// duration. Rejects (without swallowing) so callers — the status-bar toggle's
  /// connect-failure retry, the strip's inline alert — see the failure.
  connect: () => Promise<void>;
  /// Tear the listener (and, for UvproNative, the session) down. Keyed to the
  /// transport the listener actually came up on, not the editable picker.
  disconnect: () => Promise<void>;
  /// Persist a link-picker edit; the next `connect` awaits this persist before
  /// arming so the backend reads the JUST-PERSISTED link (Codex P1 race).
  onLinkChange: (fields: ModemLinkFields) => void;
}

export function useAprsConnectSequence(
  linkKind: PacketLinkKind | null,
  setLink: (fields: ModemLinkFields) => Promise<void>,
): UseAprsConnectSequence {
  const [connecting, setConnecting] = useState(false);
  // The most-recent link-persist promise. `connect` awaits it before
  // aprs_listen_start so the backend reads the JUST-PERSISTED link, not a stale
  // one (Codex adrev 2026-06-14 P1 race: setLink's packet_config_set is async).
  const linkPersist = useRef<Promise<void>>(Promise.resolve());
  // The transport the LIVE listener actually came up on (set on a successful
  // connect, cleared on disconnect). Teardown keys off THIS, not the editable
  // `linkKind` — otherwise changing the picker while listening would skip the
  // UV-Pro session cleanup (Codex adrev 2026-06-14 P1). null = not listening.
  const activeTransport = useRef<PacketLinkKind | null>(null);

  const onLinkChange = useCallback(
    (fields: ModemLinkFields) => {
      linkPersist.current = setLink(fields);
    },
    [setLink],
  );

  const onConnect = useCallback(async () => {
    // Wait for the picked link to actually persist before arming.
    await linkPersist.current;
    if (linkKind === 'UvproNative') {
      // Ride the native session: connect it first (rejects propagate), then arm
      // the listener. If arming fails (e.g. no active identity), roll the
      // session back so a failed connect never leaves the UV-Pro connected.
      await invoke('uvpro_connect', {});
      try {
        await invoke('aprs_listen_start');
      } catch (err) {
        await invoke('uvpro_disconnect').catch(() => undefined);
        throw err;
      }
    } else {
      await invoke('aprs_listen_start');
    }
    // Record the transport the listener actually came up on for teardown.
    activeTransport.current = linkKind;
  }, [linkKind]);

  const connect = useCallback(async () => {
    setConnecting(true);
    try {
      await onConnect();
    } finally {
      setConnecting(false);
    }
  }, [onConnect]);

  const disconnect = useCallback(async () => {
    // Ask the BACKEND which transport is actually live, AT CLICK TIME. The local
    // ref only knows about connects THIS hook instance performed — a popped-out or
    // remounted connect strip (AprsChatSurface) mounts a fresh instance whose ref
    // is null, so keying teardown off the ref alone would skip the UV-Pro session
    // cleanup and leak the live session (tuxlink-dmwte). aprs_status is the truth;
    // fall back to the local ref only if the query fails.
    let active: PacketLinkKind | null;
    try {
      const status = await invoke<{ listening: boolean; transport: PacketLinkKind | null }>(
        'aprs_status',
      );
      active = status.transport;
    } catch (err) {
      // Backend unreachable — degrade to the optimistic local ref (correct for the
      // window that performed the connect; null for a remount, which then does the
      // plain listen-stop path). Match the file's console.error logging style.
      console.error('aprs_status query failed; falling back to local transport ref', err);
      active = activeTransport.current;
    }
    try {
      await invoke('aprs_listen_stop');
    } finally {
      // Clean up the UV-Pro session even if stopping the engine threw — keyed to
      // the transport the BACKEND named (authoritative), not the (possibly edited)
      // picker and not this instance's ref.
      if (active === 'UvproNative') {
        await invoke('uvpro_disconnect').catch(() => undefined);
      }
      activeTransport.current = null;
    }
  }, []);

  return { connecting, connect, disconnect, onLinkChange };
}
