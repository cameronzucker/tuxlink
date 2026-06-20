import { describe, it, expect } from 'vitest';
import { composeWxSitrep, assessConditions, degToCompass } from './wxSitrep';
import type { WxStation } from './wxStations';
import type { EnvChannel, EnvStation, RainTotals, ChannelKind } from './envStations';

const NOW = Date.UTC(2026, 5, 20, 17, 15, 0); // 2026-06-20 1715Z

function ch(kind: ChannelKind, value: number, history?: number[]): EnvChannel {
  return {
    key: `wx:${kind}`,
    label: kind,
    unit: '',
    kind,
    value,
    scaled: true,
    history: (history ?? [value]).map((v, i) => ({ value: v, at: NOW - (history ? (history.length - i) * 60000 : 0) })),
  };
}

function station(
  call: string,
  ageMin: number,
  fields: {
    lat?: number;
    lon?: number;
    temp?: number;
    windDir?: number;
    windSpeed?: number;
    windGust?: number;
    humidity?: number;
    pressure?: number;
    pressureHist?: number[];
    rain?: Partial<RainTotals>;
    snow?: number;
  },
): WxStation {
  const channels: EnvChannel[] = [];
  if (fields.temp != null) channels.push(ch('temperature', fields.temp));
  if (fields.windDir != null) channels.push(ch('wind_dir', fields.windDir));
  if (fields.windSpeed != null) channels.push(ch('wind_speed', fields.windSpeed));
  if (fields.windGust != null) channels.push(ch('wind_gust', fields.windGust));
  if (fields.humidity != null) channels.push(ch('humidity', fields.humidity));
  if (fields.pressure != null) channels.push(ch('pressure', fields.pressure, fields.pressureHist));
  if (fields.snow != null) channels.push(ch('snow', fields.snow));
  const env: EnvStation = {
    call,
    project: '',
    seq: null,
    channels,
    bits: [],
    rain: fields.rain
      ? { in1h: fields.rain.in1h ?? null, in24h: fields.rain.in24h ?? null, sinceMidnight: fields.rain.sinceMidnight ?? null }
      : null,
    wxStatus: 'readings',
    rawWx: '',
    lastHeard: NOW - ageMin * 60000,
  };
  return { call, lat: fields.lat ?? 35.6, lon: fields.lon ?? -82.5, ambiguity: 0, env, at: env.lastHeard };
}

// A realistic post-storm set: cool, wet, gusty NE, pressure falling.
const STORM: WxStation[] = [
  station('W4ABC-13', 4, { lat: 35.62, lon: -82.32, temp: 58, windDir: 45, windSpeed: 12, windGust: 24, humidity: 96, pressure: 1003, pressureHist: [1006, 1003], rain: { in1h: 1.1, in24h: 3.8 } }),
  station('N4WX-1', 7, { temp: 56, windDir: 45, windSpeed: 15, windGust: 31, humidity: 94, pressure: 1006, pressureHist: [1008, 1006], rain: { in1h: 0.9 } }),
  station('KM4DD', 9, { temp: 59, windDir: 40, windSpeed: 10, windGust: 19, humidity: 90, rain: { in1h: 1.2 } }),
  station('W4GSO-13', 15, { temp: 57, windDir: 50, windSpeed: 9, humidity: 78, pressure: 1007, pressureHist: [1009, 1007] }),
  station('KK4XYZ', 12, { temp: 61, windDir: 45, windSpeed: 8, humidity: 88, rain: { in1h: 0.4 } }),
  station('AE4RP', 22, { temp: 54, humidity: 91, rain: { in1h: 0.6 } }), // calm: no wind channels
];

describe('degToCompass', () => {
  it('maps degrees to the 16-point compass', () => {
    expect(degToCompass(0)).toBe('N');
    expect(degToCompass(45)).toBe('NE');
    expect(degToCompass(90)).toBe('E');
    expect(degToCompass(360)).toBe('N');
    expect(degToCompass(-45)).toBe('NW');
  });
});

describe('composeWxSitrep — hybrid format', () => {
  const { subject, body } = composeWxSitrep(STORM, { nowMs: NOW, operatorGrid: 'DM35xa' });

  it('subject carries area + DDHHMMZ', () => {
    expect(subject).toBe('WX SITREP grid DM35 — 201715Z');
  });

  it('leads with the plain-language ground truth + arrivals guidance', () => {
    const onGround = body.split('\n').find((l) => l.startsWith('ON THE GROUND:'))!;
    expect(onGround).toContain('rain falling');
    expect(onGround).toContain('gusty wind'); // max gust 31 >= 25
    expect(onGround).toContain('pressure falling'); // majority of trends negative
    expect(onGround).toContain('For arrivals:');
    expect(onGround.toLowerCase()).toContain('rain gear');
  });

  it('reports ranges across stations in ham units', () => {
    expect(body).toContain('Temp      54–61°F');
    expect(body).toContain('Humidity  78–96 %');
    expect(body).toContain('gusts to 31');
    expect(body).toContain('Pressure  1003–1007 hPa, falling');
    expect(body).toContain('Rain      0.4–1.2 in/1h (max 3.8 in/24h)');
    expect(body).toContain('Snow      none reported');
  });

  it('lists per-station detail, newest first, callsign + grid (never place names)', () => {
    const lines = body.split('\n');
    const start = lines.findIndex((l) => l.startsWith('STATIONS'));
    const rows = lines.slice(start + 1).filter((l) => l.startsWith('  ') && /\d/.test(l));
    // newest (W4ABC-13, 4m) before oldest (AE4RP, 22m)
    expect(rows[0]).toContain('W4ABC-13');
    expect(rows[rows.length - 1]).toContain('AE4RP');
    // grid present, no invented town name
    expect(rows[0]).toMatch(/[A-R]{2}\d{2}[a-x]{2}/);
    expect(body).not.toMatch(/Black Mtn|Asheville|Swannanoa/);
  });

  it('renders absent fields as "—", never 0 (RF honesty)', () => {
    const calmRow = body.split('\n').find((l) => l.includes('AE4RP'))!;
    // AE4RP has no wind and no humidity-as-rain; wind shows "—", rain present (0.6)
    expect(calmRow).toContain('—'); // wind absent
    expect(calmRow).toContain('0.6"/h');
  });

  it('states station count, heard window, and oldest age in the footer', () => {
    expect(body).toContain('6 stations heard over RF, last 22 min');
    expect(body).toMatch(/RF honesty: only stations actually heard over RF are listed \(6 stations, last 22 min\); oldest reading 22m ago/);
  });
});

describe('composeWxSitrep — honesty edge cases', () => {
  it('empty list → valid "nothing to report" message, never throws', () => {
    const { subject, body } = composeWxSitrep([], { nowMs: NOW });
    expect(subject).toContain('WX SITREP');
    expect(body).toContain('No weather stations heard over RF');
  });

  it('a temperature-only station omits ranges it has no data for', () => {
    const one = [station('W1AW', 3, { temp: 70 })];
    const { body } = composeWxSitrep(one, { nowMs: NOW });
    expect(body).toContain('Temp      70°F');
    expect(body).not.toContain('Humidity');
    expect(body).not.toContain('Pressure');
    expect(body).toContain('Snow      none reported');
    expect(body).toContain('1 station heard over RF');
  });

  it('does not invent a pressure trend without history', () => {
    const noHist = [station('W1AW', 3, { temp: 50, pressure: 1010 })]; // single sample
    const { body } = composeWxSitrep(noHist, { nowMs: NOW });
    expect(body).toContain('Pressure  1010 hPa');
    expect(body).not.toContain('falling');
    expect(body).not.toContain('rising');
  });
});

describe('assessConditions', () => {
  it('stays conservative with thin data', () => {
    const { ground, arrivals } = assessConditions([station('W1AW', 3, { temp: 72 })]);
    expect(ground).toContain('Mild');
    expect(arrivals).toBeTruthy();
  });
});
