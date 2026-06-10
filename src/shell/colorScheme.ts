// Selectable color schemes + custom theme designer (tuxlink-8za, tuxlink-c22r,
// tuxlink-vgth).
//
// A scheme is purely presentational: it sets a `data-theme` attribute on <html>,
// and App.css's [data-theme] blocks override the primitive design tokens (the
// semantic --tux-* layer remaps automatically). The preference lives in
// localStorage — a UI preference, not validated app config — and is applied
// before React mounts (main.tsx) so there's no flash of the default theme.
//
// Practical dark schemes (GitHub dark, Office dark), light schemes (Daylight,
// High-Contrast Light, Paper), and specialty schemes (Night-Red, Grayscale)
// join the dark slate default. Each preset declares its mode via
// `color-scheme: light|dark` in the matching App.css block so WebKitGTK
// renders native form controls, scrollbars, and selection highlights in the
// correct mode.
//
// Custom themes (tuxlink-vgth) are a separate layer on top: the operator picks
// `Customize…` from View → Color Scheme, the ThemeDesigner panel writes a
// CustomTheme JSON to localStorage, and apply injects the tokens as an inline
// `style="--bg: #...;"` on <html>. Inline style beats the [data-theme]
// selector specificity, so the custom layer wins. Selecting any preset clears
// the inline style.

export type PresetScheme =
  | 'default'
  | 'github-dark'
  | 'office-dark'
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

/** The selectable preset schemes, in menu order (practical dark schemes first,
 *  then light family, then specialty). */
export const COLOR_SCHEMES: ColorSchemeOption[] = [
  { id: 'default', label: 'Default (dark)', mode: 'dark' },
  { id: 'github-dark', label: 'GitHub dark', mode: 'dark' },
  { id: 'office-dark', label: 'Office dark', mode: 'dark' },
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
 *  contrast legible in custom themes.
 *
 *  tuxlink-2ief: the modem-accent family was added as a dedicated identity
 *  for the radio panel chrome, decoupled from the project accent. Themes
 *  saved before this token set existed are migrated on load by filling
 *  missing tokens with the default-dark fallback (see `loadCustomTheme`). */
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
  'modem-accent',
  'modem-accent-2',
  'modem-accent-soft',
  'modem-accent-fg',
  'tux-accent-fg',
  'tux-danger-fg',
] as const;

/** The required tokens — every saved theme must carry these (they predate
 *  the modem-accent split). New tokens added after this set fill in from
 *  DEFAULT_DARK_TOKENS during load, so an old saved theme keeps working. */
const REQUIRED_CUSTOM_TOKENS: readonly CustomThemeToken[] = [
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
];

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

/** Read the persisted custom theme, or null if absent / malformed.
 *
 *  Validation:
 *   - name + mode + tokens-is-object are hard requirements.
 *   - Every `REQUIRED_CUSTOM_TOKENS` entry must be a non-empty string.
 *   - Tokens added after the v1 set (e.g. the modem-accent family from
 *     tuxlink-2ief) are filled from DEFAULT_DARK_TOKENS if missing or
 *     empty. This keeps pre-existing custom themes loading after a
 *     token-set upgrade — the operator gets sensible defaults for the
 *     new tokens until they re-edit the theme. */
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
    const raw_tokens = obj.tokens as Record<string, unknown>;
    for (const t of REQUIRED_CUSTOM_TOKENS) {
      if (typeof raw_tokens[t] !== 'string' || !(raw_tokens[t] as string).trim()) {
        return null;
      }
    }
    // Fill any not-yet-present tokens (added after v1 of the schema) from
    // the default-dark snapshot so the theme stays applicable on upgrade.
    const tokens = {} as Record<CustomThemeToken, string>;
    for (const t of CUSTOM_THEME_TOKENS) {
      const v = raw_tokens[t];
      tokens[t] =
        typeof v === 'string' && v.trim()
          ? v
          : DEFAULT_DARK_TOKENS[t];
    }
    return {
      name: obj.name,
      mode: obj.mode,
      tokens,
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
 *  preset clears any prior inline custom-token style so themes don't leak.
 *
 *  tuxlink-0gsy (spec §8.2): when called from an operator-initiated path
 *  (menu flip, designer apply, startup), broadcasts the new scheme to any
 *  other webview windows (currently: help) via the theme_broadcast_scheme
 *  Tauri command. Fire-and-forget — silently swallows errors in test envs
 *  without a Tauri runtime.
 *
 *  tuxlink-och6 (2026-06-03): the broadcast MUST be skipped when this is
 *  called in response to a `color_scheme_changed` event from another
 *  window, otherwise the listener (useHelpTheme.ts) creates an infinite
 *  loop: receive → apply → broadcast → emit-to-all-windows-including-sender
 *  → receive → … Each iteration is one IPC round-trip + DOM mutation; the
 *  Pi5 sat at WebKit 70% + Rust 40% at idle until this guard landed. Pass
 *  `{ broadcast: false }` when applying in response to an event. */
export function applyColorScheme(
  scheme: ColorScheme,
  options?: { broadcast?: boolean },
): void {
  const root = document.documentElement;
  const broadcast = options?.broadcast !== false;
  if (broadcast) {
    // Broadcast first so other windows see the change at the same time the
    // local apply completes. The dynamic import keeps tauri/api/core out of
    // test-environment bundles that mock the module.
    void import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke('theme_broadcast_scheme', { scheme }))
      .catch(() => {});
  }
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
  'modem-accent': '#4ade80',
  'modem-accent-2': '#86efac',
  'modem-accent-soft': 'rgba(34, 197, 94, 0.10)',
  'modem-accent-fg': '#0d1b14',
  'tux-accent-fg': '#1a0e02',
  'tux-danger-fg': '#1a0e02',
};

/** GitHub dark inspired preset. Mirrored from App.css's github-dark block. */
export const GITHUB_DARK_TOKENS: Record<CustomThemeToken, string> = {
  'bg': '#0d1117',
  'surface': '#161b22',
  'surface-2': '#21262d',
  'elevated': '#30363d',
  'border': '#30363d',
  'border-strong': '#484f58',
  'border-soft': '#21262d',
  'text': '#e6edf3',
  'text-dim': '#8b949e',
  'text-faint': '#6e7681',
  'accent': '#58a6ff',
  'accent-2': '#79c0ff',
  'unread-dot': '#f2cc60',
  'success': '#3fb950',
  'error': '#ff7b72',
  'info': '#58a6ff',
  'form-tag': '#bc8cff',
  'modem-accent': '#3fb950',
  'modem-accent-2': '#56d364',
  'modem-accent-soft': 'rgba(63, 185, 80, 0.14)',
  'modem-accent-fg': '#0d1117',
  'tux-accent-fg': '#0d1117',
  'tux-danger-fg': '#0d1117',
};

/** Office/Outlook dark inspired preset. Mirrored from App.css's office-dark block. */
export const OFFICE_DARK_TOKENS: Record<CustomThemeToken, string> = {
  'bg': '#111217',
  'surface': '#1b1c21',
  'surface-2': '#24262d',
  'elevated': '#2c2f37',
  'border': '#343741',
  'border-strong': '#4b5563',
  'border-soft': '#252832',
  'text': '#f2f3f5',
  'text-dim': '#c7cbd1',
  'text-faint': '#8a9099',
  'accent': '#4f9cf9',
  'accent-2': '#7bbcff',
  'unread-dot': '#ffcc4d',
  'success': '#4ecb8f',
  'error': '#ff6b6b',
  'info': '#6db9ff',
  'form-tag': '#d8a7ff',
  'modem-accent': '#44c08a',
  'modem-accent-2': '#5ed99e',
  'modem-accent-soft': 'rgba(68, 192, 138, 0.14)',
  'modem-accent-fg': '#08130e',
  'tux-accent-fg': '#08121f',
  'tux-danger-fg': '#180607',
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
  'modem-accent': '#0a6d3b',
  'modem-accent-2': '#137a47',
  'modem-accent-soft': 'rgba(10, 109, 59, 0.10)',
  'modem-accent-fg': '#ffffff',
  'tux-accent-fg': '#ffffff',
  'tux-danger-fg': '#ffffff',
};

/** High-contrast light preset token values. Mirrored from App.css so the
 *  designer can start from the stronger harsh-sun palette instead of falling
 *  back to dark tokens. */
export const HIGH_CONTRAST_LIGHT_TOKENS: Record<CustomThemeToken, string> = {
  'bg': '#f2f2f2',
  'surface': '#ffffff',
  'surface-2': '#e8e8e8',
  'elevated': '#d4d4d4',
  'border': '#4f4f4f',
  'border-strong': '#121212',
  'border-soft': '#8c8c8c',
  'text': '#050505',
  'text-dim': '#1f1f1f',
  'text-faint': '#333333',
  'accent': '#6f2b00',
  'accent-2': '#4a1b00',
  'unread-dot': '#5f2600',
  'success': '#00491f',
  'error': '#740014',
  'info': '#00285d',
  'form-tag': '#3d116e',
  'modem-accent': '#003f1f',
  'modem-accent-2': '#004d26',
  'modem-accent-soft': 'rgba(0, 63, 31, 0.18)',
  'modem-accent-fg': '#ffffff',
  'tux-accent-fg': '#ffffff',
  'tux-danger-fg': '#ffffff',
};

/** Map a base scheme id to its token values for the designer's "base from"
 *  picker. Only presets bundled with token snapshots are listed; the rest
 *  fall through to DEFAULT_DARK_TOKENS for safety. */
export function tokensForBase(base: PresetScheme): Record<CustomThemeToken, string> {
  if (base === 'github-dark') return GITHUB_DARK_TOKENS;
  if (base === 'office-dark') return OFFICE_DARK_TOKENS;
  if (base === 'daylight') return DAYLIGHT_TOKENS;
  if (base === 'high-contrast-light') return HIGH_CONTRAST_LIGHT_TOKENS;
  return DEFAULT_DARK_TOKENS;
}
