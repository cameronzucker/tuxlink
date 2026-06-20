import { describe, it, expect } from 'vitest';
import {
  applyWeather,
  applyTelemetry,
  isStale,
  HISTORY_CAP,
  STALE_AFTER_MS,
  type EnvStation,
} from './envStations';
import type { WeatherReportDto, InboundTelemetryDto } from './aprsTypes';

function wx(partial: Partial<WeatherReportDto>): WeatherReportDto {
  return {
    station: 'KE7ABC-13',
    windDirectionDeg: null,
    windSpeedMph: null,
    windGustMph: null,
    temperatureF: null,
    humidityPct: null,
    pressureHpa: null,
    rain1hIn: null,
    rain24hIn: null,
    rainSinceMidnightIn: null,
    luminosityWm2: null,
    snowIn: null,
    comment: '',
    status: 'readings',
    rawWx: '',
    ...partial,
  };
}

function tlm(partial: Partial<InboundTelemetryDto>): InboundTelemetryDto {
  return {
    station: 'W7DIGI-2',
    seq: null,
    analog: [],
    digital: [],
    project: '',
    comment: '',
    ...partial,
  };
}

describe('applyWeather — channel derivation (source-reactive)', () => {
  it('derives only the WX fields present on the wire (absent → no channel, never a fabricated 0)', () => {
    const s = applyWeather(undefined, wx({ temperatureF: 52, humidityPct: 78 }), 1000);
    const kinds = s.channels.map((c) => c.kind).sort();
    expect(kinds).toEqual(['humidity', 'temperature']);
    // pressure/wind/etc. absent on the wire → not present as channels
    expect(s.channels.find((c) => c.kind === 'pressure')).toBeUndefined();
  });

  it('tags each WX field with its kind, ham-conventional unit, and label', () => {
    const s = applyWeather(
      undefined,
      wx({ windDirectionDeg: 270, windSpeedMph: 8, windGustMph: 15, temperatureF: 52, pressureHpa: 1013.2 }),
      1000,
    );
    const byKind = Object.fromEntries(s.channels.map((c) => [c.kind, c]));
    expect(byKind.wind_dir).toMatchObject({ value: 270, unit: '°' });
    expect(byKind.wind_speed).toMatchObject({ value: 8, unit: 'mph' });
    expect(byKind.wind_gust).toMatchObject({ value: 15, unit: 'mph' });
    expect(byKind.temperature).toMatchObject({ value: 52, unit: '°F', label: 'Temp' });
    expect(byKind.pressure).toMatchObject({ value: 1013.2, unit: 'hPa' });
  });

  it('extracts rain totals into a dedicated block (not a time-series channel)', () => {
    const s = applyWeather(undefined, wx({ rain1hIn: 0.04, rain24hIn: 0.12 }), 1000);
    // rain is deliberately NOT a ChannelKind — it renders as a totals block, not
    // a time-series channel. Cast to compare against the off-type literal.
    expect(s.channels.find((c) => (c.kind as string) === 'rain')).toBeUndefined();
    expect(s.rain).toEqual({ in1h: 0.04, in24h: 0.12, sinceMidnight: null });
  });

  it('weather channels are always scaled (real engineering units)', () => {
    const s = applyWeather(undefined, wx({ temperatureF: 52 }), 1000);
    expect(s.channels[0].scaled).toBe(true);
  });
});

describe('applyTelemetry — self-named channels + bits', () => {
  it('maps analog channels to generic kind, carrying raw-vs-scaled honesty', () => {
    const s = applyTelemetry(
      undefined,
      tlm({
        seq: 207,
        project: 'Ridge Digipeater',
        analog: [
          { name: 'Vbat', unit: 'V', raw: 220, value: 13.6, scaled: true },
          { name: 'A1', unit: '', raw: 199, value: 199, scaled: false },
        ],
      }),
      1000,
    );
    expect(s.channels).toHaveLength(2);
    expect(s.channels[0]).toMatchObject({ label: 'Vbat', unit: 'V', value: 13.6, kind: 'generic', scaled: true });
    expect(s.channels[1]).toMatchObject({ label: 'A1', value: 199, scaled: false });
    expect(s.seq).toBe(207);
    expect(s.project).toBe('Ridge Digipeater');
  });

  it('maps digital channels to bits with their BITS sense', () => {
    const s = applyTelemetry(
      undefined,
      tlm({ digital: [{ name: 'Fan', value: true, sense: true }, { name: 'Door', value: false, sense: true }] }),
      1000,
    );
    expect(s.bits).toEqual([
      { key: 'bit:Fan', label: 'Fan', value: true, sense: true },
      { key: 'bit:Door', label: 'Door', value: false, sense: true },
    ]);
  });
});

describe('history ring — frontend-buffered series, bounded', () => {
  it('appends a sample per channel on each frame, latest value wins', () => {
    let s = applyWeather(undefined, wx({ temperatureF: 50 }), 1000);
    s = applyWeather(s, wx({ temperatureF: 52 }), 2000);
    s = applyWeather(s, wx({ temperatureF: 51 }), 3000);
    const temp = s.channels.find((c) => c.kind === 'temperature')!;
    expect(temp.value).toBe(51);
    expect(temp.history).toEqual([
      { value: 50, at: 1000 },
      { value: 52, at: 2000 },
      { value: 51, at: 3000 },
    ]);
  });

  it('bounds the ring at HISTORY_CAP, dropping the oldest', () => {
    let s: EnvStation | undefined;
    for (let i = 0; i < HISTORY_CAP + 25; i++) {
      s = applyWeather(s, wx({ temperatureF: i }), 1000 + i);
    }
    const temp = s!.channels.find((c) => c.kind === 'temperature')!;
    expect(temp.history).toHaveLength(HISTORY_CAP);
    expect(temp.history[0].value).toBe(25); // first 25 dropped
    expect(temp.history[HISTORY_CAP - 1].value).toBe(HISTORY_CAP + 24);
  });
});

describe('merge by callsign — one station, two sources auto-composed', () => {
  it('a station sending BOTH weather and telemetry shows both in one card', () => {
    let s = applyWeather(undefined, wx({ temperatureF: 52, humidityPct: 78 }), 1000);
    s = applyTelemetry(s, tlm({ analog: [{ name: 'Vbat', unit: 'V', raw: 220, value: 13.6, scaled: true }] }), 1100);
    const kinds = s.channels.map((c) => c.kind).sort();
    expect(kinds).toEqual(['generic', 'humidity', 'temperature']);
    expect(s.lastHeard).toBe(1100);
  });

  it('a previously-heard channel persists (keeps its history) when a later frame omits it', () => {
    let s = applyWeather(undefined, wx({ temperatureF: 52, pressureHpa: 1013 }), 1000);
    s = applyWeather(s, wx({ temperatureF: 53 }), 2000); // pressure absent this frame
    const pressure = s.channels.find((c) => c.kind === 'pressure');
    expect(pressure).toBeDefined();
    expect(pressure!.value).toBe(1013);
    expect(s.channels.find((c) => c.kind === 'temperature')!.value).toBe(53);
  });
});

describe('applyWeather — honest no-data state (tuxlink-vnm5)', () => {
  it('carries the readings status + raw run for a valid report', () => {
    const s = applyWeather(undefined, wx({ temperatureF: 55, status: 'readings', rawWx: '180/010t055' }), 1000);
    expect(s.wxStatus).toBe('readings');
    expect(s.rawWx).toBe('180/010t055');
  });

  it('rosters a sensors-offline station: no channels, but the raw run preserved', () => {
    // Backend dropped the impossible readings (767deg/200F/0hPa); the DTO arrives
    // all-null with status sensorsOffline and the raw run kept for inspection.
    const s = applyWeather(
      undefined,
      wx({ status: 'sensorsOffline', rawWx: '767/255g255t200r000p000P000h00b00000' }),
      1000,
    );
    expect(s.wxStatus).toBe('sensorsOffline');
    expect(s.channels).toHaveLength(0); // nothing rendered as a real reading
    expect(s.rawWx).toContain('767/255'); // …but the raw is there to inspect
  });

  it('rosters a position-only weather-symbol beacon (name, no readings)', () => {
    const s = applyWeather(
      undefined,
      wx({ station: 'KA7WSB-2', status: 'positionOnly', comment: 'NPS_003_Chiminea' }),
      1000,
    );
    expect(s.wxStatus).toBe('positionOnly');
    expect(s.channels).toHaveLength(0);
    expect(s.project).toBe('NPS_003_Chiminea');
  });
});

describe('staleness', () => {
  it('a station is stale once no frame has arrived within STALE_AFTER_MS', () => {
    const s = applyWeather(undefined, wx({ temperatureF: 52 }), 1000);
    expect(isStale(s, 1000 + STALE_AFTER_MS - 1)).toBe(false);
    expect(isStale(s, 1000 + STALE_AFTER_MS + 1)).toBe(true);
  });
});
