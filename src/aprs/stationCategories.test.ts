import { describe, it, expect } from 'vitest';
import { CATEGORIES, categoryByKey } from './stationCategories';

describe('stationCategories', () => {
  it('all matches everything', () => {
    const all = categoryByKey('all');
    expect(all.matches({ call: 'X', isWeather: false })).toBe(true);
  });
  it('weather matches only weather stations', () => {
    const wx = categoryByKey('weather');
    expect(wx.matches({ call: 'X', isWeather: true })).toBe(true);
    expect(wx.matches({ call: 'X', isWeather: false })).toBe(false);
  });
  it('exposes weather as a selectable category', () => {
    expect(CATEGORIES.map((c) => c.key)).toContain('weather');
  });
  it('unknown key falls back to all', () => {
    expect(categoryByKey('nope').key).toBe('all');
  });
});
