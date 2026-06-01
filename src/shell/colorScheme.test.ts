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

describe('color scheme model', () => {
  it('offers default, three light presets, night-red, grayscale (in menu order)', () => {
    expect(COLOR_SCHEMES.map((s) => s.id)).toEqual([
      'default',
      'daylight',
      'high-contrast-light',
      'paper',
      'night-red',
      'grayscale',
    ]);
    expect(COLOR_SCHEMES.every((s) => s.label.length > 0)).toBe(true);
  });

  it('declares mode (light/dark) for each preset — light family is light, others dark', () => {
    const modeFor = (id: string) => COLOR_SCHEMES.find((s) => s.id === id)?.mode;
    expect(modeFor('default')).toBe('dark');
    expect(modeFor('daylight')).toBe('light');
    expect(modeFor('high-contrast-light')).toBe('light');
    expect(modeFor('paper')).toBe('light');
    expect(modeFor('night-red')).toBe('dark');
    expect(modeFor('grayscale')).toBe('dark');
  });

  it('isColorScheme accepts known preset ids and the "custom" sentinel', () => {
    expect(isColorScheme('default')).toBe(true);
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
    expect(isPresetScheme('daylight')).toBe(true);
    expect(isPresetScheme('custom')).toBe(false);
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
});

describe('tokensForBase', () => {
  it('returns the dark tokens for the default preset', () => {
    expect(tokensForBase('default')).toEqual(DEFAULT_DARK_TOKENS);
  });

  it('returns the daylight tokens for the daylight preset', () => {
    expect(tokensForBase('daylight')).toEqual(DAYLIGHT_TOKENS);
  });

  it('falls back to dark tokens for presets without a bundled snapshot', () => {
    // night-red / grayscale / high-contrast-light / paper are valid bases for
    // the designer but the loader doesn't carry their snapshots — falling
    // back to the dark tokens lets the designer start from a known-good base.
    expect(tokensForBase('night-red')).toEqual(DEFAULT_DARK_TOKENS);
    expect(tokensForBase('paper')).toEqual(DEFAULT_DARK_TOKENS);
  });
});
