import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import {
  useFontSize,
  FONT_SIZE_STORAGE_KEY,
  FONT_PRESETS,
  FONT_PX,
  stepFontSize,
} from './useFontSize';

beforeEach(() => {
  localStorage.clear();
  document.documentElement.style.removeProperty('--help-font-size');
});
afterEach(() => {
  localStorage.clear();
  document.documentElement.style.removeProperty('--help-font-size');
});

describe('FONT_PRESETS / FONT_PX', () => {
  it('exposes the four preset names', () => {
    expect(FONT_PRESETS).toEqual(['Normal', 'Large', 'X-Large', 'Huge']);
  });
  it('maps each preset to the documented px value', () => {
    expect(FONT_PX).toEqual({ 'Normal': 18, 'Large': 20, 'X-Large': 22, 'Huge': 24 });
  });
});

describe('stepFontSize', () => {
  it('steps up through the presets', () => {
    expect(stepFontSize('Normal', 'up')).toBe('Large');
    expect(stepFontSize('Large', 'up')).toBe('X-Large');
    expect(stepFontSize('X-Large', 'up')).toBe('Huge');
    expect(stepFontSize('Huge', 'up')).toBe('Huge'); // saturates
  });
  it('steps down through the presets', () => {
    expect(stepFontSize('Huge', 'down')).toBe('X-Large');
    expect(stepFontSize('Large', 'down')).toBe('Normal');
    expect(stepFontSize('Normal', 'down')).toBe('Normal'); // saturates
  });
});

describe('useFontSize', () => {
  it('defaults to Normal when localStorage is empty', () => {
    const { result } = renderHook(() => useFontSize());
    expect(result.current.preset).toBe('Normal');
  });

  it('reads a persisted preset from localStorage', () => {
    localStorage.setItem(FONT_SIZE_STORAGE_KEY, 'Large');
    const { result } = renderHook(() => useFontSize());
    expect(result.current.preset).toBe('Large');
  });

  it('falls back to Normal when localStorage holds an unknown value', () => {
    localStorage.setItem(FONT_SIZE_STORAGE_KEY, 'GIANT');
    const { result } = renderHook(() => useFontSize());
    expect(result.current.preset).toBe('Normal');
  });

  it('applies --help-font-size on mount', () => {
    renderHook(() => useFontSize());
    expect(document.documentElement.style.getPropertyValue('--help-font-size')).toBe('18px');
  });

  it('persists + applies a new preset on setPreset', () => {
    const { result } = renderHook(() => useFontSize());
    act(() => result.current.setPreset('X-Large'));
    expect(localStorage.getItem(FONT_SIZE_STORAGE_KEY)).toBe('X-Large');
    expect(document.documentElement.style.getPropertyValue('--help-font-size')).toBe('22px');
  });
});
