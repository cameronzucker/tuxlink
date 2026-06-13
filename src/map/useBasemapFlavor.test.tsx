/**
 * Tests for useBasemapFlavor (tuxlink-ndi4 phase 3) — the basemap follows the
 * app color scheme via the <html> data-theme attribute.
 */
import { describe, it, expect, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { resolveBasemapFlavor, useBasemapFlavor } from './useBasemapFlavor';

afterEach(() => {
  delete document.documentElement.dataset.theme;
  document.documentElement.style.removeProperty('color-scheme');
});

describe('resolveBasemapFlavor', () => {
  it('defaults to dark when no theme is set (the "default" scheme is dark)', () => {
    delete document.documentElement.dataset.theme;
    expect(resolveBasemapFlavor()).toBe('dark');
  });

  it('maps a light preset scheme to light', () => {
    document.documentElement.dataset.theme = 'daylight';
    expect(resolveBasemapFlavor()).toBe('light');
  });

  it('maps a dark preset scheme to dark', () => {
    document.documentElement.dataset.theme = 'github-dark';
    expect(resolveBasemapFlavor()).toBe('dark');
  });

  it('reads the custom theme mode from inline color-scheme', () => {
    document.documentElement.dataset.theme = 'custom';
    document.documentElement.style.colorScheme = 'light';
    expect(resolveBasemapFlavor()).toBe('light');
    document.documentElement.style.colorScheme = 'dark';
    expect(resolveBasemapFlavor()).toBe('dark');
  });
});

describe('useBasemapFlavor', () => {
  it('updates when the document theme changes', async () => {
    document.documentElement.dataset.theme = 'daylight'; // light
    const { result } = renderHook(() => useBasemapFlavor());
    expect(result.current).toBe('light');
    await act(async () => {
      document.documentElement.dataset.theme = 'github-dark'; // dark
      // let the MutationObserver microtask flush
      await Promise.resolve();
    });
    expect(result.current).toBe('dark');
  });
});
