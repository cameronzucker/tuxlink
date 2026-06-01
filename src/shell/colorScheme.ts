// Selectable color schemes + custom theme designer (tuxlink-8za, tuxlink-c22r,
// tuxlink-vgth).
//
// A scheme is purely presentational: it sets a `data-theme` attribute on <html>,
// and App.css's [data-theme] blocks override the primitive design tokens (the
// semantic --tux-* layer remaps automatically). The preference lives in
// localStorage — a UI preference, not validated app config — and is applied
// before React mounts (main.tsx) so there's no flash of the default theme.
//
// Light schemes (Daylight, High-Contrast Light, Paper) join the dark slate
// default + Night-Red + Grayscale set, motivated by outdoor / bright-sun
// readability. Each preset declares its mode via `color-scheme: light|dark`
// in the matching App.css block so WebKitGTK renders native form controls,
// scrollbars, and selection highlights in the correct mode.
//
// Custom themes (tuxlink-vgth) are a separate layer on top: the operator picks
// `Customize…` from View → Color Scheme, the ThemeDesigner panel writes a
// CustomTheme JSON to localStorage, and apply injects the tokens as an inline
// `style="--bg: #...;"` on <html>. Inline style beats the [data-theme]
// selector specificity, so the custom layer wins. Selecting any preset clears
// the inline style.

export type PresetScheme =
  | 'default'
  | 'daylight'
  | 'high-contrast-light'
  | 'paper'
  | 'night-red'
  | 'grayscale';

export type ColorScheme = PresetScheme | 'custom';

export interface ColorSchemeOption {
  id: PresetScheme;
  label: string;
  /** Affects native form controls / scrollbar; used by tests + the designer's
   *  "base from" preview to render the correct mode. */
  mode: 'light' | 'dark';
}

/** The selectable preset schemes, in menu order (dark default first, then
 *  light family, then specialty). */
export const COLOR_SCHEMES: ColorSchemeOption[] = [
  { id: 'default', label: 'Default (dark)', mode: 'dark' },
  { id: 'daylight', label: 'Daylight (light)', mode: 'light' },
  { id: 'high-contrast-light', label: 'High contrast (light)', mode: 'light' },
  { id: 'paper', label: 'Paper (warm light)', mode: 'light' },
  { id: 'night-red', label: 'Night / tactical (red)', mode: 'dark' },
  { id: 'grayscale', label: 'Grayscale', mode: 'dark' },
];

export const COLOR_SCHEME_STORAGE_KEY = 'tuxlink.colorScheme';
export const CUSTOM_THEME_STORAGE_KEY = 'tuxlink.customTheme';

/** The set of CSS custom properties the designer can override. The names match
 *  App.css primitive tokens 1:1; the `--tux-*` aliases follow automatically.
 *  Two literal `--tux-*` tokens (`tux-accent-fg`, `tux-danger-fg`) are
 *  included because they don't alias a primitive — they live in the literal
 *  block of each preset and need explicit values to keep accent-on-accent
 *  contrast legible in custom themes. */
export const CUSTOM_THEME_TOKENS = [
  'bg',
  'surface',
  'surface-2',
  'elevated',
  'border',
  'border-strong',
  'border-soft',
  'text',
  'text-dim',
  'text-faint',
  'accent',
  'accent-2',
  'unread-dot',
  'success',
  'error',
  'info',
  'form-tag',
  'tux-accent-fg',
  'tux-danger-fg',
] as const;

export type CustomThemeToken = (typeof CUSTOM_THEME_TOKENS)[number];

export interface CustomTheme {
  /** Operator-chosen label shown in the menu in place of the bare "Custom". */
  name: string;
  /** Light or dark — drives `color-scheme:` on <html> + the designer mode swatch. */
  mode: 'light' | 'dark';
  /** Token → CSS color string. Every token in CUSTOM_THEME_TOKENS must be set;
   *  the loader rejects partial entries to avoid a half-applied appearance. */
  tokens: Record<CustomThemeToken, string>;
}

export function isColorScheme(value: unknown): value is ColorScheme {
  if (value === 'custom') return true;
  return COLOR_SCHEMES.some((s) => s.id === value);
}

export function isPresetScheme(value: unknown): value is PresetScheme {
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

/** Read the persisted custom theme, or null if absent / malformed. The loader
 *  is strict: a malformed or partial entry returns null so the designer can
 *  detect first-use vs edit-existing without a separate flag. */
export function loadCustomTheme(): CustomTheme | null {
  try {
    const raw = localStorage.getItem(CUSTOM_THEME_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== 'object') return null;
    const obj = parsed as Partial<CustomTheme>;
    if (typeof obj.name !== 'string' || !obj.name) return null;
    if (obj.mode !== 'light' && obj.mode !== 'dark') return null;
    if (!obj.tokens || typeof obj.tokens !== 'object') return null;
    const tokens = obj.tokens as Record<string, unknown>;
    for (const t of CUSTOM_THEME_TOKENS) {
      if (typeof tokens[t] !== 'string' || !(tokens[t] as string).trim()) {
        return null;
      }
    }
    return {
      name: obj.name,
      mode: obj.mode,
      tokens: tokens as Record<CustomThemeToken, string>,
    };
  } catch {
    return null;
  }
}

/** Persist a custom theme (replaces any prior entry). */
export function saveCustomTheme(theme: CustomTheme): void {
  try {
    localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, JSON.stringify(theme));
  } catch {
    /* storage unavailable — applies for this session only */
  }
}

/** Delete the persisted custom theme. The currently-applied scheme is
 *  unaffected; the next `applyColorScheme('custom')` after a delete falls
 *  back to the default preset (no custom tokens to inject). */
export function clearCustomTheme(): void {
  try {
    localStorage.removeItem(CUSTOM_THEME_STORAGE_KEY);
  } catch {
    /* no-op */
  }
}

/** Apply a scheme. Sets <html data-theme> for presets; injects custom tokens
 *  as inline `style` properties when scheme is 'custom'. Switching to a
 *  preset clears any prior inline custom-token style so themes don't leak. */
export function applyColorScheme(scheme: ColorScheme): void {
  const root = document.documentElement;
  // Always strip prior inline custom-token style before applying — a stale
  // override from the last 'custom' selection would otherwise bleed through
  // any new preset. Presets live in CSS; the designer lives in inline style.
  for (const t of CUSTOM_THEME_TOKENS) {
    root.style.removeProperty(`--${t}`);
  }
  // color-scheme is inline only for 'custom' — clear it on every transition so
  // a prior custom selection doesn't pin the wrong native-control mode.
  root.style.removeProperty('color-scheme');

  if (scheme === 'default') {
    delete root.dataset.theme;
    return;
  }

  if (scheme === 'custom') {
    const custom = loadCustomTheme();
    if (!custom) {
      // No saved custom theme — fall back to default so the operator doesn't
      // land on an undefined state (e.g., after clearing localStorage).
      delete root.dataset.theme;
      return;
    }
    root.dataset.theme = 'custom';
    for (const t of CUSTOM_THEME_TOKENS) {
      root.style.setProperty(`--${t}`, custom.tokens[t]);
    }
    root.style.colorScheme = custom.mode;
    return;
  }

  root.dataset.theme = scheme;
}

/** The default dark token set, used as the designer's starting base when the
 *  operator opens Customize for the first time. Mirrored from App.css's
 *  `:root` block — keep in sync. */
export const DEFAULT_DARK_TOKENS: Record<CustomThemeToken, string> = {
  'bg': '#0d1318',
  'surface': '#141c23',
  'surface-2': '#1a2330',
  'elevated': '#1e2832',
  'border': '#1f2832',
  'border-strong': '#2c3744',
  'border-soft': '#1a2028',
  'text': '#e4ebf2',
  'text-dim': '#94a0ad',
  'text-faint': '#5d6975',
  'accent': '#f59f3c',
  'accent-2': '#ffba6e',
  'unread-dot': '#ffd166',
  'success': '#5dd6a0',
  'error': '#ee6b6b',
  'info': '#6bb8ee',
  'form-tag': '#c084fc',
  'tux-accent-fg': '#1a0e02',
  'tux-danger-fg': '#1a0e02',
};

/** The "Daylight" preset's token values — exposed so the designer can offer
 *  "Start from Daylight" as a light-mode base. Mirrored from App.css. */
export const DAYLIGHT_TOKENS: Record<CustomThemeToken, string> = {
  'bg': '#f4f6f9',
  'surface': '#ffffff',
  'surface-2': '#eef1f5',
  'elevated': '#e6eaef',
  'border': '#d4dae2',
  'border-strong': '#9ea7b3',
  'border-soft': '#e6eaef',
  'text': '#101820',
  'text-dim': '#3d4753',
  'text-faint': '#6c7682',
  'accent': '#a04a00',
  'accent-2': '#7a3700',
  'unread-dot': '#a04a00',
  'success': '#0a6d3b',
  'error': '#a3171e',
  'info': '#0b4f9c',
  'form-tag': '#5b21b6',
  'tux-accent-fg': '#ffffff',
  'tux-danger-fg': '#ffffff',
};

/** Map a base scheme id to its token values for the designer's "base from"
 *  picker. Only presets bundled with token snapshots are listed; the rest
 *  fall through to DEFAULT_DARK_TOKENS for safety. */
export function tokensForBase(base: PresetScheme): Record<CustomThemeToken, string> {
  if (base === 'daylight') return DAYLIGHT_TOKENS;
  return DEFAULT_DARK_TOKENS;
}
