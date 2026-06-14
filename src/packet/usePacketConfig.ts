// src/packet/usePacketConfig.ts
//
// Shared packet-config hook so the SSID (and any future shared fields)
// stay coherent between the DashboardRibbon (status pane) and the
// PacketRadioPanel. The pre-fix path hardcoded `effectiveCall(call, 0)`
// in AppShell.tsx — the ribbon callsign never reflected the configured
// SSID. Operator smoke 2026-05-31.
//
// Layout: this hook owns the source-of-truth `PacketConfigDto`, exposes
// a setSsid action that persists via packet_config_set, and broadcasts
// writes via a same-window CustomEvent so multiple consumers (the ribbon
// + the radio panel) stay in sync without a round-trip through the
// backend. Pre-wizard (no config yet) the hook holds `null` and setSsid
// is a no-op (we can't merge into a DTO we never read).
//
// Cross-window sync (e.g., a wizard window updating config) would still
// need a backend-emitted event; that is out of scope for this fix.

/** Same-window broadcast name. All `usePacketConfig` instances within
 *  the same WebView listen for this and re-seed their local state from
 *  the event detail, so a write in one panel propagates to others. */
const PACKET_CONFIG_LOCAL_EVENT = 'tuxlink:packet-config:change';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { PacketConfigDto } from './packetTypes';
import type { ModemLinkFields } from '../radio/sections/ModemLinkSection';

export interface UsePacketConfig {
  /** Loaded config, or null when not yet loaded (pre-wizard / load error). */
  config: PacketConfigDto | null;
  /** Effective SSID — `config.ssid` when loaded, else 0 (UI default). */
  ssid: number;
  /** Persist a new SSID. No-op when config is unloaded. */
  setSsid: (n: number) => void;
  /** Persist the transport/radio link fields (read-modify-write of the full
   *  DTO; merges the ModemLinkSection field set in). No-op when config is
   *  unloaded — we cannot merge into a DTO we never read. Mirrors setSsid:
   *  optimistic local update + same-window CustomEvent broadcast + persist via
   *  packet_config_set. Persisting is what makes `config.linkKind` exist on a
   *  fresh install.
   *
   *  RETURNS the persist promise (resolves once packet_config_set settles; never
   *  rejects) so the connect flow can AWAIT it before arming the listener —
   *  aprs_listen_start reads the PERSISTED backend config, not JS state, so
   *  connecting before the write lands would read a stale/absent link (Codex
   *  adrev 2026-06-14 P1 race). */
  setLink: (fields: ModemLinkFields) => Promise<void>;
}

/**
 * Subscribe to packet config. Loads on mount via packet_config_get; listens
 * for a `packet_config:change` event so any panel's write triggers a refresh
 * elsewhere. The backend SHOULD emit `packet_config:change` on every persist
 * — if it doesn't, this hook still works locally (the writing component sees
 * its own update via the optimistic setConfig in setSsid).
 */
export function usePacketConfig(): UsePacketConfig {
  const [config, setConfig] = useState<PacketConfigDto | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    const load = () => {
      invoke<PacketConfigDto>('packet_config_get')
        .then((c) => {
          if (!cancelled) setConfig(c);
        })
        .catch(() => {
          /* pre-wizard / no config — leave null, UI uses default 0 */
        });
    };

    // Same-window broadcast: re-seed from any other component's write.
    const onLocalChange = (e: Event) => {
      if (cancelled) return;
      const detail = (e as CustomEvent<PacketConfigDto>).detail;
      if (detail) setConfig(detail);
    };
    if (typeof window !== 'undefined') {
      window.addEventListener(PACKET_CONFIG_LOCAL_EVENT, onLocalChange);
    }

    // Backend-emitted change event — present when/if the backend chooses
    // to emit it. Future-proofing only; safe no-op if it never fires.
    listen<PacketConfigDto>('packet_config:change', (event) => {
      if (!cancelled) setConfig(event.payload);
    })
      .then((u) => {
        if (cancelled) {
          u();
          return;
        }
        unlisten = u;
      })
      .catch(() => {
        /* listen unavailable (test env) — load-only is fine */
      });

    load();
    return () => {
      cancelled = true;
      if (typeof window !== 'undefined') {
        window.removeEventListener(PACKET_CONFIG_LOCAL_EVENT, onLocalChange);
      }
      if (unlisten) unlisten();
    };
  }, []);

  const setSsid = useCallback(
    (n: number) => {
      if (!config) return;
      const next = { ...config, ssid: n };
      // Optimistic local update.
      setConfig(next);
      // Broadcast to peer hooks within the same window so they re-seed
      // without waiting for the backend (which doesn't emit a change
      // event today).
      if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(PACKET_CONFIG_LOCAL_EVENT, { detail: next }));
      }
      void invoke('packet_config_set', { dto: next }).catch(() => {
        /* persist errors surface via the session log */
      });
    },
    [config],
  );

  const setLink = useCallback(
    (fields: ModemLinkFields): Promise<void> => {
      if (!config) return Promise.resolve();
      // Merge the link field set into the persisted DTO. The ModemLinkFields
      // subset (linkKind + per-transport address fields) overrides; every other
      // AX.25 / SSID field is preserved.
      const next: PacketConfigDto = { ...config, ...fields };
      // Optimistic local update.
      setConfig(next);
      // Broadcast to peer hooks within the same window so they re-seed without
      // waiting for the backend (which doesn't emit a change event today).
      if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(PACKET_CONFIG_LOCAL_EVENT, { detail: next }));
      }
      // Return the persist promise so the connect flow can await it (P1 race);
      // swallow the error so awaiting never throws (persist errors surface via
      // the session log).
      return invoke('packet_config_set', { dto: next })
        .then(() => undefined)
        .catch(() => undefined);
    },
    [config],
  );

  return {
    config,
    ssid: config?.ssid ?? 0,
    setSsid,
    setLink,
  };
}
