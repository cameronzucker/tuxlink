import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import {
  useReadingWidth,
  READING_WIDTH_STORAGE_KEY,
  READING_WIDTHS,
  READING_WIDTH_PX,
} from './useReadingWidth';

beforeEach(() => {
  localStorage.clear();
  document.documentElement.style.removeProperty('--help-reading-max-width');
});
afterEach(() => {
  localStorage.clear();
  document.documentElement.style.removeProperty('--help-reading-max-width');
});

describe('READING_WIDTHS / READING_WIDTH_PX', () => {
  it('exposes the two preset names', () => {
    expect(READING_WIDTHS).toEqual(['Narrow', 'Wide']);
  });

  it('maps each preset to the documented px value', () => {
    // tuxlink-d7a7: Wide bumped 980→1280 for better 1080p use.
    expect(READING_WIDTH_PX).toEqual({ Narrow: '720px', Wide: '1280px' });
  });
});

describe('useReadingWidth', () => {
  it('defaults to Wide when localStorage is empty (tuxlink-d7a7)', () => {
    const { result } = renderHook(() => useReadingWidth());
    expect(result.current.width).toBe('Wide');
  });

  it('reads a persisted preset from localStorage', () => {
    localStorage.setItem(READING_WIDTH_STORAGE_KEY, 'Narrow');
    const { result } = renderHook(() => useReadingWidth());
    expect(result.current.width).toBe('Narrow');
  });

  it('falls back to Wide when localStorage holds an unknown value', () => {
    localStorage.setItem(READING_WIDTH_STORAGE_KEY, 'EXTRAWIDE');
    const { result } = renderHook(() => useReadingWidth());
    expect(result.current.width).toBe('Wide');
  });

  it('applies --help-reading-max-width on mount (defaults to Wide → 1280px)', () => {
    renderHook(() => useReadingWidth());
    expect(
      document.documentElement.style.getPropertyValue('--help-reading-max-width'),
    ).toBe('1280px');
  });

  it('toggle flips Wide ↔ Narrow and persists', () => {
    const { result } = renderHook(() => useReadingWidth());
    // Default is now Wide, so first toggle → Narrow (was Narrow → Wide).
    act(() => result.current.toggle());
    expect(result.current.width).toBe('Narrow');
    expect(localStorage.getItem(READING_WIDTH_STORAGE_KEY)).toBe('Narrow');
    expect(
      document.documentElement.style.getPropertyValue('--help-reading-max-width'),
    ).toBe('720px');

    act(() => result.current.toggle());
    expect(result.current.width).toBe('Wide');
    expect(
      document.documentElement.style.getPropertyValue('--help-reading-max-width'),
    ).toBe('1280px');
  });

  it('setWidth applies + persists explicitly', () => {
    const { result } = renderHook(() => useReadingWidth());
    act(() => result.current.setWidth('Wide'));
    expect(result.current.width).toBe('Wide');
    expect(localStorage.getItem(READING_WIDTH_STORAGE_KEY)).toBe('Wide');
  });
});
