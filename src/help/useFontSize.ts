/**
 * Text-size hook for the help window. Persists the operator's chosen
 * preset in localStorage and writes the resulting px to a global CSS
 * variable (--help-font-size on <html>) that ReadingPane.css consumes.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §7.
 */

import { useState, useEffect, useCallback } from 'react';

export const FONT_PRESETS = ['Normal', 'Large', 'X-Large', 'Huge'] as const;
export type FontPreset = (typeof FONT_PRESETS)[number];

export const FONT_PX: Record<FontPreset, number> = {
  'Normal': 18,
  'Large': 20,
  'X-Large': 22,
  'Huge': 24,
};

export const FONT_SIZE_STORAGE_KEY = 'tuxlink.help.fontSize';
export const DEFAULT_FONT_PRESET: FontPreset = 'Normal';

function isFontPreset(value: unknown): value is FontPreset {
  return typeof value === 'string' && (FONT_PRESETS as readonly string[]).includes(value);
}

/** Step `current` up or down the preset list; saturates at both ends. */
export function stepFontSize(current: FontPreset, dir: 'up' | 'down'): FontPreset {
  const i = FONT_PRESETS.indexOf(current);
  const next = dir === 'up' ? i + 1 : i - 1;
  if (next < 0) return FONT_PRESETS[0];
  if (next >= FONT_PRESETS.length) return FONT_PRESETS[FONT_PRESETS.length - 1];
  return FONT_PRESETS[next];
}

function loadPersisted(): FontPreset {
  try {
    const raw = localStorage.getItem(FONT_SIZE_STORAGE_KEY);
    if (isFontPreset(raw)) return raw;
  } catch {
    // localStorage may throw in private-browsing-class environments; treat as default.
  }
  return DEFAULT_FONT_PRESET;
}

export function useFontSize() {
  const [preset, setPresetState] = useState<FontPreset>(() => loadPersisted());

  useEffect(() => {
    document.documentElement.style.setProperty('--help-font-size', `${FONT_PX[preset]}px`);
    try {
      localStorage.setItem(FONT_SIZE_STORAGE_KEY, preset);
    } catch {
      // ignore — UI still updates, just won't persist.
    }
  }, [preset]);

  const setPreset = useCallback((p: FontPreset) => setPresetState(p), []);

  return { preset, setPreset, presets: FONT_PRESETS };
}
