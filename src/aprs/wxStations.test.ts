import { describe, it, expect } from 'vitest';
import { hasWeather, joinWxStations, badgeContent } from './wxStations';
import type { EnvStation } from './envStations';
import type { HeardPosition } from './aprsTypes';

function env(
  call: string,
  ch: Array<{ key: string; kind: string; value: number; unit?: string }>,
  rain: EnvStation['rain'] = null,
): EnvStation {
  return {
    call,
    project: '',
    seq: null,
    bits: [],
    rain,
    lastHeard: 100,
    channels: ch.map((c) => ({
      key: c.key,
      label: c.kind,
      unit: c.unit ?? '',
      kind: c.kind as never,
      value: c.value,
      scaled: true,
      history: [],
    })),
  };
}
function pos(call: string, lat: number, lon: number): HeardPosition {
  return { call, lat, lon, symbolTable: '/', symbolCode: '_', comment: '', at: 1, ambiguity: 0 };
}

describe('hasWeather', () => {
  it('true for a wx channel', () => {
    expect(hasWeather(env('W7WX', [{ key: 'wx:temperature', kind: 'temperature', value: 72 }]))).toBe(true);
  });
  it('true for rain totals only', () => {
    expect(hasWeather(env('W7WX', [], { in1h: 0.1, in24h: null, sinceMidnight: null }))).toBe(true);
  });
  it('false for telemetry-only station', () => {
    expect(hasWeather(env('N0T', [{ key: 'tlm:Vbat', kind: 'generic', value: 13 }]))).toBe(false);
  });
});

describe('joinWxStations', () => {
  it('includes only weather stations that also have a position', () => {
    const stations = [
      env('W7WX', [{ key: 'wx:temperature', kind: 'temperature', value: 72 }]),
      env('NOPOS', [{ key: 'wx:temperature', kind: 'temperature', value: 60 }]),
      env('N0T', [{ key: 'tlm:Vbat', kind: 'generic', value: 13 }]),
    ];
    const positions = [pos('W7WX', 47, -122), pos('N0T', 40, -100)];
    const out = joinWxStations(stations, positions);
    expect(out.map((w) => w.call)).toEqual(['W7WX']); // NOPOS has no position; N0T has no weather
    expect(out[0].lat).toBe(47);
  });
});

describe('badgeContent (RF-honesty)', () => {
  it('temperature-led, no glyph when only temp', () => {
    expect(badgeContent(env('W', [{ key: 'wx:temperature', kind: 'temperature', value: 71.6 }]))).toEqual({
      primary: '72°F',
      glyph: null,
    });
  });
  it('rain glyph only when actually raining', () => {
    const e = env('W', [{ key: 'wx:temperature', kind: 'temperature', value: 60 }], {
      in1h: 0.2,
      in24h: null,
      sinceMidnight: null,
    });
    expect(badgeContent(e)).toEqual({ primary: '60°F', glyph: '🌧' });
  });
  it('wind glyph when wind is notable and no rain', () => {
    const e = env('W', [
      { key: 'wx:temperature', kind: 'temperature', value: 60 },
      { key: 'wx:wind_speed', kind: 'wind_speed', value: 22, unit: 'mph' },
    ]);
    expect(badgeContent(e)).toEqual({ primary: '60°F', glyph: '💨' });
  });
  it('falls back to wind reading when no temperature', () => {
    expect(badgeContent(env('W', [{ key: 'wx:wind_speed', kind: 'wind_speed', value: 12, unit: 'mph' }]))).toEqual({
      primary: '12 mph',
      glyph: null,
    });
  });
  it('never shows a telemetry channel as the badge primary (mixed station)', () => {
    // A rain-only weather station that also emits telemetry (battery voltage).
    const e = env(
      'W',
      [{ key: 'tlm:Vbat', kind: 'generic', value: 13, unit: 'V' }],
      { in1h: 0.3, in24h: null, sinceMidnight: null },
    );
    const b = badgeContent(e);
    expect(b.primary).not.toContain('13');
    expect(b.primary).toContain('rain');
    expect(b.glyph).toBe('🌧');
  });
});
