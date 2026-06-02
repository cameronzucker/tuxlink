// src/radio/useVaraConfig.ts
//
// Shared VARA-config hook (mirrors usePacketConfig exactly). The
// VaraRadioPanel reads/writes the persisted VARA settings (host, cmd_port,
// data_port, bandwidth) through this hook; the StatusBar / DashboardRibbon
// can later subscribe to the same hook for status indicators without
// duplicating the load + persist plumbing.
//
// Pattern (verbatim from usePacketConfig): the hook starts with the struct
// default; `config_get_vara` overwrites it when the load completes. No
// `loading` state — callers either don't need one, or check via a stable
// signal of their choosing (e.g., comparing config to the default). The
// prior version (tuxlink-6dzo) carried a `loading: boolean` that wired into
// the panel's `disabled` prop, which created a UI-locking failure mode if
// loading ever stayed true (and on the Pi it did, for reasons still under
// investigation — Strict Mode race vs. invoke channel vs. JS exception
// before .finally). The hook now matches usePacketConfig's posture so the
// same bug class is impossible by construction.
//
// Race window: if the operator types into a field BEFORE config_get_vara
// resolves AND the load returns a value different from the default, the
// load overwrites the operator's edit. In practice the load completes in
// milliseconds, so this race is vanishingly rare; the simpler hook wins.

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

/** Mirror of Rust's `VaraUiConfig`. Field names are snake_case (the Rust
 *  struct does not use `#[serde(rename_all = "camelCase")]`). */
export interface VaraUiConfig {
  host: string;
  cmd_port: number;
  data_port: number;
  /** None on the wire = field absent (skip_serializing_if). The hook
   *  surfaces this as `null` for ergonomic TS use. */
  bandwidth_hz: number | null;
}

/** Same-window broadcast for cross-component sync. Future-proofing for a
 *  StatusBar / ribbon indicator that wants live config changes without
 *  remounting the panel. */
const VARA_CONFIG_LOCAL_EVENT = 'tuxlink:vara-config:change';

/** Default the hook holds before `config_get_vara` returns (or when it
 *  rejects, pre-wizard). Matches the Rust `VaraUiConfig::default()` so
 *  the displayed values agree with what `config_get_vara` would return
 *  after the first persist. */
export const VARA_DEFAULT_CONFIG: VaraUiConfig = {
  host: '127.0.0.1',
  cmd_port: 8300,
  data_port: 8301,
  bandwidth_hz: null,
};

export interface UseVaraConfig {
  /** Currently-loaded config. Always non-null — the hook substitutes
   *  the struct default before the first load completes and on load error. */
  config: VaraUiConfig;
  /** Persist a new config. Optimistic local update + backend write +
   *  same-window broadcast. Errors surface in the session log via the
   *  backend; the hook holds the optimistic value regardless. */
  setConfig: (next: VaraUiConfig) => void;
}

export function useVaraConfig(): UseVaraConfig {
  const [config, setConfigState] = useState<VaraUiConfig>(VARA_DEFAULT_CONFIG);

  useEffect(() => {
    let cancelled = false;

    const onLocalChange = (e: Event) => {
      if (cancelled) return;
      const detail = (e as CustomEvent<VaraUiConfig>).detail;
      if (detail) setConfigState(detail);
    };
    if (typeof window !== 'undefined') {
      window.addEventListener(VARA_CONFIG_LOCAL_EVENT, onLocalChange);
    }

    invoke<VaraUiConfig>('config_get_vara')
      .then((c) => {
        if (cancelled) return;
        // Normalize an absent (undefined) bandwidth_hz to null for ergonomic TS use.
        // The Rust side skips serializing None, so the field can be absent on the wire.
        setConfigState({
          host: c.host ?? VARA_DEFAULT_CONFIG.host,
          cmd_port: c.cmd_port ?? VARA_DEFAULT_CONFIG.cmd_port,
          data_port: c.data_port ?? VARA_DEFAULT_CONFIG.data_port,
          bandwidth_hz: c.bandwidth_hz ?? null,
        });
      })
      .catch(() => {
        // Pre-wizard / config absent — keep the default. (No loading state
        // to clear; the hook is already serving usable defaults.)
      });

    return () => {
      cancelled = true;
      if (typeof window !== 'undefined') {
        window.removeEventListener(VARA_CONFIG_LOCAL_EVENT, onLocalChange);
      }
    };
  }, []);

  const setConfig = useCallback((next: VaraUiConfig) => {
    setConfigState(next);
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new CustomEvent(VARA_CONFIG_LOCAL_EVENT, { detail: next }));
    }
    void invoke('config_set_vara', { value: next }).catch(() => {
      // Persist errors surface in the session log via the backend. The
      // optimistic local update stands; the operator can retry by editing
      // the field again.
    });
  }, []);

  return { config, setConfig };
}
