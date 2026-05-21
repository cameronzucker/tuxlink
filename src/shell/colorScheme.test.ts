// Tests for tuxlink-8za — selectable color schemes.
//
// The scheme model is a tiny presentational-preference layer: a fixed set of
// schemes, a localStorage round-trip, and applying the choice as a `data-theme`
// attribute on <html> (the CSS does the rest via [data-theme] token overrides).

import { describe, it, expect, beforeEach } from 'vitest';
import {
  COLOR_SCHEMES,
  isColorScheme,
  loadColorScheme,
  saveColorScheme,
  applyColorScheme,
  COLOR_SCHEME_STORAGE_KEY,
} from './colorScheme';

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
});

describe('color scheme model', () => {
  it('offers default, night-red, and grayscale (default first)', () => {
    expect(COLOR_SCHEMES.map((s) => s.id)).toEqual(['default', 'night-red', 'grayscale']);
    // Every scheme carries a human label for the menu.
    expect(COLOR_SCHEMES.every((s) => s.label.length > 0)).toBe(true);
  });

  it('isColorScheme accepts known ids and rejects anything else', () => {
    expect(isColorScheme('night-red')).toBe(true);
    expect(isColorScheme('grayscale')).toBe(true);
    expect(isColorScheme('default')).toBe(true);
    expect(isColorScheme('chartreuse')).toBe(false);
    expect(isColorScheme('')).toBe(false);
    expect(isColorScheme(null)).toBe(false);
  });
});

describe('loadColorScheme', () => {
  it('defaults to "default" when nothing is stored', () => {
    expect(loadColorScheme()).toBe('default');
  });

  it('returns a previously stored valid scheme', () => {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'night-red');
    expect(loadColorScheme()).toBe('night-red');
  });

  it('falls back to "default" when the stored value is garbage', () => {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, 'neon-banana');
    expect(loadColorScheme()).toBe('default');
  });
});

describe('saveColorScheme', () => {
  it('round-trips through storage', () => {
    saveColorScheme('grayscale');
    expect(loadColorScheme()).toBe('grayscale');
  });
});

describe('applyColorScheme', () => {
  it('sets data-theme on <html> for a non-default scheme', () => {
    applyColorScheme('night-red');
    expect(document.documentElement.dataset.theme).toBe('night-red');
  });

  it('removes data-theme for the default scheme (clean :root)', () => {
    applyColorScheme('grayscale');
    applyColorScheme('default');
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});
