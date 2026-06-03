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
    expect(READING_WIDTH_PX).toEqual({ Narrow: '720px', Wide: '980px' });
  });
});

describe('useReadingWidth', () => {
  it('defaults to Narrow when localStorage is empty', () => {
    const { result } = renderHook(() => useReadingWidth());
    expect(result.current.width).toBe('Narrow');
  });

  it('reads a persisted preset from localStorage', () => {
    localStorage.setItem(READING_WIDTH_STORAGE_KEY, 'Wide');
    const { result } = renderHook(() => useReadingWidth());
    expect(result.current.width).toBe('Wide');
  });

  it('falls back to Narrow when localStorage holds an unknown value', () => {
    localStorage.setItem(READING_WIDTH_STORAGE_KEY, 'EXTRAWIDE');
    const { result } = renderHook(() => useReadingWidth());
    expect(result.current.width).toBe('Narrow');
  });

  it('applies --help-reading-max-width on mount', () => {
    renderHook(() => useReadingWidth());
    expect(
      document.documentElement.style.getPropertyValue('--help-reading-max-width'),
    ).toBe('720px');
  });

  it('toggle flips Narrow ↔ Wide and persists', () => {
    const { result } = renderHook(() => useReadingWidth());
    act(() => result.current.toggle());
    expect(result.current.width).toBe('Wide');
    expect(localStorage.getItem(READING_WIDTH_STORAGE_KEY)).toBe('Wide');
    expect(
      document.documentElement.style.getPropertyValue('--help-reading-max-width'),
    ).toBe('980px');

    act(() => result.current.toggle());
    expect(result.current.width).toBe('Narrow');
    expect(
      document.documentElement.style.getPropertyValue('--help-reading-max-width'),
    ).toBe('720px');
  });

  it('setWidth applies + persists explicitly', () => {
    const { result } = renderHook(() => useReadingWidth());
    act(() => result.current.setWidth('Wide'));
    expect(result.current.width).toBe('Wide');
    expect(localStorage.getItem(READING_WIDTH_STORAGE_KEY)).toBe('Wide');
  });
});
