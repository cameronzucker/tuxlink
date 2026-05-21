// Selectable color schemes (tuxlink-8za).
//
// A scheme is purely presentational: it sets a `data-theme` attribute on <html>,
// and App.css's [data-theme] blocks override the primitive design tokens (the
// semantic --tux-* layer remaps automatically). The preference lives in
// localStorage — a UI preference, not validated app config — and is applied
// before React mounts (main.tsx) so there's no flash of the default theme.

export type ColorScheme = 'default' | 'night-red' | 'grayscale';

export interface ColorSchemeOption {
  id: ColorScheme;
  label: string;
}

/** The selectable schemes, in menu order (default first). */
export const COLOR_SCHEMES: ColorSchemeOption[] = [
  { id: 'default', label: 'Default' },
  { id: 'night-red', label: 'Night / tactical (red)' },
  { id: 'grayscale', label: 'Grayscale' },
];

export const COLOR_SCHEME_STORAGE_KEY = 'tuxlink.colorScheme';

export function isColorScheme(value: unknown): value is ColorScheme {
  return COLOR_SCHEMES.some((s) => s.id === value);
}

/** Read the persisted scheme, falling back to 'default' for missing/garbage. */
export function loadColorScheme(): ColorScheme {
  try {
    const stored = localStorage.getItem(COLOR_SCHEME_STORAGE_KEY);
    return isColorScheme(stored) ? stored : 'default';
  } catch {
    return 'default';
  }
}

/** Persist the chosen scheme. Best-effort — storage may be unavailable. */
export function saveColorScheme(scheme: ColorScheme): void {
  try {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, scheme);
  } catch {
    /* storage unavailable — the scheme still applies for this session */
  }
}

/** Apply a scheme: set <html> data-theme, or clear it for the default. */
export function applyColorScheme(scheme: ColorScheme): void {
  if (scheme === 'default') {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = scheme;
  }
}
