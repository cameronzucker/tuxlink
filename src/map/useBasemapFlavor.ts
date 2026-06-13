/**
 * useBasemapFlavor — follow the app's color scheme (tuxlink-ndi4 phase 3).
 *
 * The basemap light/dark flavor tracks the app theme automatically (operator
 * decision 2026-06-13): a dark color scheme → dark map, light scheme → light map,
 * no separate map control. Resolved from the DOM — `applyColorScheme` sets
 * `data-theme` on `<html>` (and an inline `color-scheme` for the custom theme) —
 * via a MutationObserver, deliberately WITHOUT the `color_scheme_changed` Tauri
 * event, so the map components stay free of Tauri-event test coupling.
 */
import { useEffect, useState } from 'react';
import { COLOR_SCHEMES } from '../shell/colorScheme';
import type { BasemapFlavor } from './basemapStyle';

/** Mode of the 'default' scheme (applied when no `data-theme` is set). */
const DEFAULT_MODE: BasemapFlavor =
  COLOR_SCHEMES.find((s) => s.id === 'default')?.mode === 'dark' ? 'dark' : 'light';

/** Resolve the current basemap flavor from the document's theme state. */
export function resolveBasemapFlavor(): BasemapFlavor {
  if (typeof document === 'undefined') return 'light';
  const root = document.documentElement;
  const theme = root.dataset.theme; // undefined ('default') | 'custom' | scheme id
  if (!theme) return DEFAULT_MODE;
  if (theme === 'custom') return root.style.colorScheme === 'dark' ? 'dark' : 'light';
  return COLOR_SCHEMES.find((s) => s.id === theme)?.mode === 'dark' ? 'dark' : 'light';
}

/** The basemap flavor for the active app color scheme, updating on theme change. */
export function useBasemapFlavor(): BasemapFlavor {
  const [flavor, setFlavor] = useState<BasemapFlavor>(resolveBasemapFlavor);
  useEffect(() => {
    const update = () => setFlavor(resolveBasemapFlavor());
    const observer = new MutationObserver(update);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme', 'style'],
    });
    update(); // re-sync in case the theme changed between initial render and effect
    return () => observer.disconnect();
  }, []);
  return flavor;
}
