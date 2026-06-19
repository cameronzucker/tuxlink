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
  /// APRS position-ambiguity level (0–4) carried from the heard position, so the
  /// badge plots at the SAME honest cell-centre as the pin (never the false-exact
  /// low corner of a masked fix).
  ambiguity: number;
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
    out.push({ call: e.call, lat: p.lat, lon: p.lon, ambiguity: p.ambiguity, env: e, at: e.lastHeard });
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
    // Fallback must stay WEATHER-only: a mixed station's `channels` can carry
    // `tlm:` channels (e.g. battery voltage) ahead of the weather data, so pick a
    // present `wx:` channel — never the first merged channel regardless of source.
    const firstWx = env.channels.find((c) => c.key.startsWith('wx:'));
    if (firstWx) primary = `${Math.round(firstWx.value)} ${firstWx.unit}`.trim();
    else if (rain1h != null) primary = `${rain1h}" rain`;
    else primary = '—';
  }

  let glyph: string | null = null;
  if (rain1h != null && rain1h > 0) glyph = '🌧';
  else if (wind && wind.value >= 20) glyph = '💨';
  return { primary, glyph };
}
