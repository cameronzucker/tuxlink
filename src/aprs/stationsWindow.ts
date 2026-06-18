// src/aprs/stationsWindow.ts
//
// Open the Station Data panel in its own window (tuxlink-2phz), reusing the
// project's second-window pattern (compose / help / logging). The Rust command
// `stations_window_open` is single-instance: re-invoking focuses the existing
// window. The popped-out window builds its OWN history ring from the moment it
// opens — the from-launch buffer lives in the main window's hook — so a fresh
// pop-out starts with live values and fills its graphs as new frames arrive.

import { invoke } from '@tauri-apps/api/core';

export function openStationsWindow(): Promise<void> {
  return invoke<void>('stations_window_open').catch((err) => {
    // Non-fatal: the in-dock panel remains available if the window fails to open.
    console.error('stations_window_open failed', err);
  });
}
