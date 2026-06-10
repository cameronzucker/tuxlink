// Tests for tuxlink-8za / tuxlink-c22r / tuxlink-vgth — selectable color
// schemes + custom theme designer model.
//
// The scheme model is a tiny presentational-preference layer: a fixed set of
// presets, a localStorage round-trip, and applying the choice as a `data-theme`
// attribute on <html> (the CSS does the rest via [data-theme] token overrides).
// The custom theme layer extends this with operator-edited tokens applied as
// inline `style="--bg: …"` properties.

import { describe, it, expect, beforeEach } from 'vitest';
import {
  COLOR_SCHEMES,
  CUSTOM_THEME_TOKENS,
  DAYLIGHT_TOKENS,
  DEFAULT_DARK_TOKENS,
  GITHUB_DARK_TOKENS,
  HIGH_CONTRAST_LIGHT_TOKENS,
  OFFICE_DARK_TOKENS,
  isColorScheme,
  isPresetScheme,
  loadColorScheme,
  saveColorScheme,
  applyColorScheme,
  loadCustomTheme,
  saveCustomTheme,
  clearCustomTheme,
  tokensForBase,
  COLOR_SCHEME_STORAGE_KEY,
  CUSTOM_THEME_STORAGE_KEY,
  type CustomTheme,
  type CustomThemeToken,
} from './colorScheme';

const APP_CSS_MODULES = import.meta.glob('../App.css', {
  eager: true,
  query: '?raw',
  import: 'default',
});
const APP_CSS = Object.values(APP_CSS_MODULES)[0] as string;

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  for (const t of CUSTOM_THEME_TOKENS) {
    document.documentElement.style.removeProperty(`--${t}`);
  }
  document.documentElement.style.removeProperty('color-scheme');
});

/** A complete custom theme fixture — every token in CUSTOM_THEME_TOKENS set
 *  to a unique color so the inline-style override can be asserted per-token. */
function makeFixtureCustomTheme(overrides: Partial<CustomTheme> = {}): CustomTheme {
  const tokens = {} as Record<CustomThemeToken, string>;
  for (const t of CUSTOM_THEME_TOKENS) {
    tokens[t] = DEFAULT_DARK_TOKENS[t];
  }
  return {
    name: 'My field theme',
    mode: 'dark',
    tokens,
    ...overrides,
  };
}

type Rgb = [number, number, number];

function cssBlock(selector: string): string {
  const start = APP_CSS.indexOf(selector);
  expect(start).toBeGreaterThan(-1);
  return APP_CSS.slice(start, APP_CSS.indexOf('}', start) + 1);
}

function cssTokens(selector: string): Record<string, string> {
  const block = cssBlock(selector);
  const tokens: Record<string, string> = {};
  for (const match of block.matchAll(/--([a-z0-9-]+):\s*([^;]+);/gi)) {
    tokens[match[1]] = match[2].trim();
  }
  return tokens;
}

function parseHexColor(value: string): Rgb {
  const match = /^#([0-9a-f]{6})$/i.exec(value.trim());
  if (!match) throw new Error(`Expected 6-digit hex color, got ${value}`);
  const hex = match[1];
  return [
    Number.parseInt(hex.slice(0, 2), 16),
    Number.parseInt(hex.slice(2, 4), 16),
    Number.parseInt(hex.slice(4, 6), 16),
  ];
}

function relativeLuminance([r, g, b]: Rgb): number {
  const linear = (channel: number) => {
    const s = channel / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

function contrastRatio(a: Rgb, b: Rgb): number {
  const [lighter, darker] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return (lighter + 0.05) / (darker + 0.05);
}

function cssContrast(foreground: string, background: string): number {
  return contrastRatio(parseHexColor(foreground), parseHexColor(background));
}

function compositeRgbaOverSurface(value: string, surface: string): Rgb {
  const match = /^rgba\(\s*(\d+),\s*(\d+),\s*(\d+),\s*([0-9.]+)\s*\)$/i.exec(value.trim());
  if (!match) throw new Error(`Expected rgba() color, got ${value}`);
  const foreground: Rgb = [
    Number(match[1]),
    Number(match[2]),
    Number(match[3]),
  ];
  const alpha = Number(match[4]);
  const background = parseHexColor(surface);
  return [
    Math.round(foreground[0] * alpha + background[0] * (1 - alpha)),
    Math.round(foreground[1] * alpha + background[1] * (1 - alpha)),
    Math.round(foreground[2] * alpha + background[2] * (1 - alpha)),
  ];
}

function highContrastLightCssTokens(): Record<string, string> {
  return cssTokens(":root[data-theme='high-contrast-light']");
}

describe('color scheme model', () => {
  it('offers practical dark, light, and specialty presets in menu order', () => {
    expect(COLOR_SCHEMES.map((s) => s.id)).toEqual([
      'default',
      'github-dark',
      'office-dark',
      'daylight',
      'high-contrast-light',
      'paper',
      'night-red',
      'grayscale',
    ]);
    expect(COLOR_SCHEMES.find((s) => s.id === 'github-dark')?.label).toBe('Repository Dark');
    expect(COLOR_SCHEMES.every((s) => s.label.length > 0)).toBe(true);
  });

  it('declares mode (light/dark) for each preset — light family is light, others dark', () => {
    const modeFor = (id: string) => COLOR_SCHEMES.find((s) => s.id === id)?.mode;
    expect(modeFor('default')).toBe('dark');
    expect(modeFor('github-dark')).toBe('dark');
    expect(modeFor('office-dark')).toBe('dark');
    expect(modeFor('daylight')).toBe('light');
    expect(modeFor('high-contrast-light')).toBe('light');
    expect(modeFor('paper')).toBe('light');
    expect(modeFor('night-red')).toBe('dark');
    expect(modeFor('grayscale')).toBe('dark');
  });

  it('isColorScheme accepts known preset ids and the "custom" sentinel', () => {
    expect(isColorScheme('default')).toBe(true);
    expect(isColorScheme('github-dark')).toBe(true);
    expect(isColorScheme('office-dark')).toBe(true);
    expect(isColorScheme('daylight')).toBe(true);
    expect(isColorScheme('high-contrast-light')).toBe(true);
    expect(isColorScheme('paper')).toBe(true);
    expect(isColorScheme('night-red')).toBe(true);
    expect(isColorScheme('grayscale')).toBe(true);
    expect(isColorScheme('custom')).toBe(true);
    expect(isColorScheme('chartreuse')).toBe(false);
    expect(isColorScheme('')).toBe(false);
    expect(isColorScheme(null)).toBe(false);
  });

  it('isPresetScheme rejects "custom" — the designer is not a preset', () => {
    expect(isPresetScheme('default')).toBe(true);
    expect(isPresetScheme('github-dark')).toBe(true);
    expect(isPresetScheme('office-dark')).toBe(true);
    expect(isPresetScheme('daylight')).toBe(true);
    expect(isPresetScheme('custom')).toBe(false);
  });
});

describe('high-contrast light preset tokens (tuxlink-msoy)', () => {
  it('mirrors the shipped CSS preset into the designer base tokens', () => {
    const tokens = highContrastLightCssTokens();
    for (const token of CUSTOM_THEME_TOKENS) {
      expect(tokens[token]).toBe(HIGH_CONTRAST_LIGHT_TOKENS[token]);
    }
    expect(tokensForBase('high-contrast-light')).toEqual(HIGH_CONTRAST_LIGHT_TOKENS);
  });

  it('uses a visible surface ladder and stronger border contrast', () => {
    const tokens = highContrastLightCssTokens();
    expect(tokens.bg).not.toBe(tokens.surface);
    expect(cssContrast(tokens['border-soft'], tokens.surface)).toBeGreaterThanOrEqual(3);
    expect(cssContrast(tokens.border, tokens.surface)).toBeGreaterThanOrEqual(7);
    expect(cssContrast(tokens['border-strong'], tokens.surface)).toBeGreaterThanOrEqual(15);
  });

  it('keeps text tokens AAA-readable across the light filled surfaces', () => {
    const tokens = highContrastLightCssTokens();
    for (const foreground of ['text', 'text-dim', 'text-faint']) {
      for (const background of ['bg', 'surface', 'surface-2', 'elevated']) {
        expect(cssContrast(tokens[foreground], tokens[background])).toBeGreaterThanOrEqual(7);
      }
    }
  });

  it('keeps semantic and radio accents readable on light controls', () => {
    const tokens = highContrastLightCssTokens();
    const accents = [
      'accent',
      'accent-2',
      'modem-accent',
      'modem-accent-2',
      'unread-dot',
      'success',
      'error',
      'info',
      'form-tag',
    ];
    for (const foreground of accents) {
      expect(cssContrast(tokens[foreground], tokens.surface)).toBeGreaterThanOrEqual(7);
      expect(cssContrast(tokens[foreground], tokens['surface-2'])).toBeGreaterThanOrEqual(7);
    }
  });

  it('makes low-alpha selected/status fills visible without becoming solid blocks', () => {
    const tokens = highContrastLightCssTokens();
    for (const fillToken of ['accent-soft', 'modem-accent-soft', 'tux-danger-surface']) {
      const composite = compositeRgbaOverSurface(tokens[fillToken], tokens.surface);
      const ratio = contrastRatio(composite, parseHexColor(tokens.surface));
      expect(ratio).toBeGreaterThanOrEqual(1.3);
      expect(ratio).toBeLessThanOrEqual(1.8);
    }
  });
});

describe('practical dark preset tokens (tuxlink-pfhw + tuxlink-23ck)', () => {
  it('keeps the renamed Repository Dark CSS preset mirrored into the designer tokens', () => {
    expect(cssTokens(":root[data-theme='github-dark']")['unread-dot']).toBe(GITHUB_DARK_TOKENS['unread-dot']);
    expect(GITHUB_DARK_TOKENS['unread-dot']).toBe(GITHUB_DARK_TOKENS.accent);
  });

  it('uses theme-native unread indicators for Office dark instead of the default amber dot', () => {
    expect(cssTokens(":root[data-theme='office-dark']")['unread-dot']).toBe(OFFICE_DARK_TOKENS['unread-dot']);
    expect(OFFICE_DARK_TOKENS['unread-dot']).toBe(OFFICE_DARK_TOKENS.accent);
  });
});

describe('loadColorScheme', () => {
  it('defaults to "default" when nothing is stored', () => {
    expect(loadColorScheme()).toBe('default');
  });

  it('returns a previously stored valid preset', () => {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'night-red');
    expect(loadColorScheme()).toBe('night-red');
  });

  it('accepts each new light preset', () => {
    for (const id of ['daylight', 'high-contrast-light', 'paper'] as const) {
      localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, id);
      expect(loadColorScheme()).toBe(id);
    }
  });

  it('accepts each practical dark preset', () => {
    for (const id of ['github-dark', 'office-dark'] as const) {
      localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, id);
      expect(loadColorScheme()).toBe(id);
    }
  });

  it('accepts the "custom" sentinel', () => {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'custom');
    expect(loadColorScheme()).toBe('custom');
  });

  it('falls back to "default" when the stored value is garbage', () => {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'neon-banana');
    expect(loadColorScheme()).toBe('default');
  });
});

describe('saveColorScheme', () => {
  it('round-trips a preset through storage', () => {
    saveColorScheme('grayscale');
    expect(loadColorScheme()).toBe('grayscale');
  });

  it('round-trips "custom" through storage', () => {
    saveColorScheme('custom');
    expect(loadColorScheme()).toBe('custom');
  });
});

describe('applyColorScheme', () => {
  it('sets data-theme on <html> for a non-default preset', () => {
    applyColorScheme('night-red');
    expect(document.documentElement.dataset.theme).toBe('night-red');
  });

  it('sets data-theme for each light preset', () => {
    applyColorScheme('daylight');
    expect(document.documentElement.dataset.theme).toBe('daylight');
    applyColorScheme('high-contrast-light');
    expect(document.documentElement.dataset.theme).toBe('high-contrast-light');
    applyColorScheme('paper');
    expect(document.documentElement.dataset.theme).toBe('paper');
  });

  it('sets data-theme for each practical dark preset', () => {
    applyColorScheme('github-dark');
    expect(document.documentElement.dataset.theme).toBe('github-dark');
    applyColorScheme('office-dark');
    expect(document.documentElement.dataset.theme).toBe('office-dark');
  });

  it('removes data-theme for the default scheme (clean :root)', () => {
    applyColorScheme('grayscale');
    applyColorScheme('default');
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it('strips any prior inline custom-token style when switching to a preset', () => {
    saveCustomTheme(
      makeFixtureCustomTheme({ tokens: { ...DEFAULT_DARK_TOKENS, bg: '#abcdef' } }),
    );
    applyColorScheme('custom');
    expect(document.documentElement.style.getPropertyValue('--bg')).toBe('#abcdef');

    // Switching to a preset must clear the inline tokens — otherwise the
    // custom --bg would override the preset's --bg via specificity.
    applyColorScheme('daylight');
    expect(document.documentElement.style.getPropertyValue('--bg')).toBe('');
    expect(document.documentElement.style.colorScheme).toBe('');
    expect(document.documentElement.dataset.theme).toBe('daylight');
  });

  it('applying "custom" with no saved theme falls back to default (no orphan data-theme)', () => {
    applyColorScheme('custom');
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it('applying "custom" with a saved theme writes every token + color-scheme inline', () => {
    const theme = makeFixtureCustomTheme({ mode: 'light' });
    saveCustomTheme(theme);

    applyColorScheme('custom');
    expect(document.documentElement.dataset.theme).toBe('custom');
    for (const t of CUSTOM_THEME_TOKENS) {
      expect(document.documentElement.style.getPropertyValue(`--${t}`)).toBe(theme.tokens[t]);
    }
    expect(document.documentElement.style.colorScheme).toBe('light');
  });
});

describe('custom theme persistence', () => {
  it('loadCustomTheme returns null when nothing is stored', () => {
    expect(loadCustomTheme()).toBeNull();
  });

  it('round-trips a complete CustomTheme through storage', () => {
    const theme = makeFixtureCustomTheme();
    saveCustomTheme(theme);
    expect(loadCustomTheme()).toEqual(theme);
  });

  it('rejects malformed JSON', () => {
    localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, '{not json');
    expect(loadCustomTheme()).toBeNull();
  });

  it('rejects a theme with missing mode', () => {
    const theme = makeFixtureCustomTheme();
    const { mode: _drop, ...partial } = theme;
    void _drop;
    localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, JSON.stringify(partial));
    expect(loadCustomTheme()).toBeNull();
  });

  it('rejects a theme with missing tokens', () => {
    const theme = makeFixtureCustomTheme();
    const partial = { ...theme, tokens: { ...theme.tokens } };
    delete (partial.tokens as Partial<Record<CustomThemeToken, string>>)['accent'];
    localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, JSON.stringify(partial));
    expect(loadCustomTheme()).toBeNull();
  });

  it('rejects a theme with an empty-string token', () => {
    const theme = makeFixtureCustomTheme({
      tokens: { ...DEFAULT_DARK_TOKENS, accent: '' },
    });
    localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, JSON.stringify(theme));
    expect(loadCustomTheme()).toBeNull();
  });

  it('clearCustomTheme removes the entry', () => {
    saveCustomTheme(makeFixtureCustomTheme());
    expect(loadCustomTheme()).not.toBeNull();
    clearCustomTheme();
    expect(loadCustomTheme()).toBeNull();
  });

  // tuxlink-2ief: schema-upgrade migration. A theme saved before the
  // modem-accent token family existed is still loadable; the loader
  // fills the new tokens from DEFAULT_DARK_TOKENS so the radio dock
  // still has a sensible identity until the operator re-edits the theme.
  it('migrates a saved theme missing post-v1 tokens by filling from DEFAULT_DARK_TOKENS', () => {
    const theme = makeFixtureCustomTheme();
    const v1Tokens = { ...theme.tokens };
    // Strip the modem-accent family (added post-v1).
    delete (v1Tokens as Partial<Record<CustomThemeToken, string>>)['modem-accent'];
    delete (v1Tokens as Partial<Record<CustomThemeToken, string>>)['modem-accent-2'];
    delete (v1Tokens as Partial<Record<CustomThemeToken, string>>)['modem-accent-soft'];
    delete (v1Tokens as Partial<Record<CustomThemeToken, string>>)['modem-accent-fg'];
    const partial = { ...theme, tokens: v1Tokens };
    localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, JSON.stringify(partial));

    const loaded = loadCustomTheme();
    expect(loaded).not.toBeNull();
    expect(loaded!.tokens['modem-accent']).toBe(DEFAULT_DARK_TOKENS['modem-accent']);
    expect(loaded!.tokens['modem-accent-2']).toBe(DEFAULT_DARK_TOKENS['modem-accent-2']);
    expect(loaded!.tokens['modem-accent-soft']).toBe(DEFAULT_DARK_TOKENS['modem-accent-soft']);
    expect(loaded!.tokens['modem-accent-fg']).toBe(DEFAULT_DARK_TOKENS['modem-accent-fg']);
    // The unmigrated tokens round-trip unchanged.
    expect(loaded!.tokens.accent).toBe(theme.tokens.accent);
  });
});

describe('tokensForBase', () => {
  it('returns the dark tokens for the default preset', () => {
    expect(tokensForBase('default')).toEqual(DEFAULT_DARK_TOKENS);
  });

  it('returns the daylight tokens for the daylight preset', () => {
    expect(tokensForBase('daylight')).toEqual(DAYLIGHT_TOKENS);
  });

  it('returns bundled token snapshots for the practical dark presets', () => {
    expect(tokensForBase('github-dark')).toEqual(GITHUB_DARK_TOKENS);
    expect(tokensForBase('office-dark')).toEqual(OFFICE_DARK_TOKENS);
    expect(tokensForBase('high-contrast-light')).toEqual(HIGH_CONTRAST_LIGHT_TOKENS);
  });

  it('falls back to dark tokens for presets without a bundled snapshot', () => {
    // night-red / grayscale / paper are valid bases for the designer but the
    // loader doesn't carry their snapshots — falling back to the dark tokens
    // lets the designer start from a known-good base.
    expect(tokensForBase('night-red')).toEqual(DEFAULT_DARK_TOKENS);
    expect(tokensForBase('paper')).toEqual(DEFAULT_DARK_TOKENS);
  });
});
