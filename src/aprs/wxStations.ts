// src/aprs/wxStations.ts
//
// Pure join + classification for the on-map weather overlay (ni5b). Weather data
// lives in EnvStation.channels (keyed `wx:<kind>`) + EnvStation.rain; positions
// live in HeardPosition. A WxStation exists only when a station has BOTH a heard
// weather reading and a position. RF-honesty: the badge shows only what was heard
// — temperature-led, a condition glyph only when a real field supports it.

import type { EnvStation } from './envStations';
import type { HeardPosition } from './aprsTypes';

/// A station counts as "weather" when it emitted any weather channel or rain —
/// distinct from a telemetry-only station (channels keyed `tlm:`).
export function hasWeather(env: EnvStation): boolean {
  return env.rain != null || env.channels.some((c) => c.key.startsWith('wx:'));
}

export interface WxStation {
  call: string;
  lat: number;
  lon: number;
  env: EnvStation;
  /// Local epoch-ms of the latest frame (from EnvStation.lastHeard).
  at: number;
}

/// Inner-join weather stations with their positions by callsign. Excludes
/// telemetry-only stations and weather stations with no heard position.
export function joinWxStations(env: EnvStation[], positions: HeardPosition[]): WxStation[] {
  const posByCall = new Map(positions.map((p) => [p.call, p]));
  const out: WxStation[] = [];
  for (const e of env) {
    if (!hasWeather(e)) continue;
    const p = posByCall.get(e.call);
    if (!p) continue;
    out.push({ call: e.call, lat: p.lat, lon: p.lon, env: e, at: e.lastHeard });
  }
  return out;
}

/// The compact badge: a temperature-led primary string + an optional condition
/// glyph derived ONLY from real fields. Never assumes a sky condition.
export function badgeContent(env: EnvStation): { primary: string; glyph: string | null } {
  const temp = env.channels.find((c) => c.kind === 'temperature');
  const wind = env.channels.find((c) => c.kind === 'wind_speed');
  const rain1h = env.rain?.in1h ?? null;

  let primary: string;
  if (temp) primary = `${Math.round(temp.value)}°F`;
  else if (wind) primary = `${Math.round(wind.value)} mph`;
  else {
    const first = env.channels[0];
    primary = first ? `${Math.round(first.value)} ${first.unit}`.trim() : '—';
  }

  let glyph: string | null = null;
  if (rain1h != null && rain1h > 0) glyph = '🌧';
  else if (wind && wind.value >= 20) glyph = '💨';
  return { primary, glyph };
}
