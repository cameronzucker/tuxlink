import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { applyColorScheme, type ColorScheme } from '../shell/colorScheme';

/**
 * Inherits the main window's color scheme on mount + re-applies on
 * `color_scheme_changed` events broadcast by the main window.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §8.
 */
export function useHelpTheme() {
  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | null = null;

    // Initial: read whatever the main window last broadcast.
    invoke<string | null>('theme_get_scheme')
      .then((scheme) => {
        if (cancelled) return;
        if (scheme && typeof scheme === 'string') {
          applyColorScheme(scheme as ColorScheme);
        }
      })
      .catch(() => {});  // ignore — theme falls back to defaults

    // Live: re-apply on broadcast events from the main window.
    listen<string>('color_scheme_changed', (e) => {
      applyColorScheme(e.payload as ColorScheme);
    }).then((unfn) => {
      if (cancelled) {
        unfn();
      } else {
        unlisten = unfn;
      }
    }).catch(() => {});

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);
}
