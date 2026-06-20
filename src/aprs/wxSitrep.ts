// src/aprs/wxSitrep.ts
//
// APRS→Winlink local-area weather situation report (tuxlink-hepq). Pure: turns the
// heard WxStation[] into a transmittable text SITREP (subject + body) for the
// Winlink compose path. RF-honesty is the whole point — the report states ONLY
// what was actually heard over RF, with station count, heard window, and per-station
// age. Absent fields (every WX measurement is Option) render "—", never 0 or a guess.
//
// Format = the operator-approved "hybrid": plain-language ground-truth + what-to-
// expect first, then aggregate ranges, then a per-station detail table, then the
// RF-honesty footer. Units are ham-conventional (°F / mph / in / hPa). Per-station
// location is a Maidenhead grid (latLonToGrid) — never an invented place name.
// See docs/design/2026-06-20-aprs-wx-sitrep-design.md.

import type { ChannelKind, EnvStation } from './envStations';
import type { WxStation } from './wxStations';
import { latLonToGrid } from '../forms/position/maidenhead';

export interface WxSitrepMeta {
  /** Epoch-ms "now" the report is generated for (UTC formatting + freshness). */
  nowMs: number;
  /** Operator's Maidenhead grid, used for the area header when present. */
  operatorGrid?: string;
}

export interface WxSitrep {
  subject: string;
  body: string;
}

const COMPASS = [
  'N', 'NNE', 'NE', 'ENE', 'E', 'ESE', 'SE', 'SSE',
  'S', 'SSW', 'SW', 'WSW', 'W', 'WNW', 'NW', 'NNW',
];

/** Wind direction (degrees the weather comes FROM, true) → 16-point compass. */
export function degToCompass(deg: number): string {
  const i = Math.round((((deg % 360) + 360) % 360) / 22.5) % 16;
  return COMPASS[i];
}

/** Latest value of a weather channel on a station, or null when absent on the wire. */
function chan(env: EnvStation, kind: ChannelKind): number | null {
  const c = env.channels.find((ch) => ch.kind === kind);
  return c ? c.value : null;
}

/** Per-station barometric trend from the pressure channel's history (hPa delta over
 * the heard window). null when there is no pressure channel or <2 samples. */
function pressureTrend(env: EnvStation): number | null {
  const c = env.channels.find((ch) => ch.kind === 'pressure');
  if (!c || c.history.length < 2) return null;
  return c.history[c.history.length - 1].value - c.history[0].value;
}

function minMax(values: number[]): { min: number; max: number } | null {
  if (values.length === 0) return null;
  return { min: Math.min(...values), max: Math.max(...values) };
}

/** Collect the present (non-null) values of one channel kind across all stations. */
function present(stations: WxStation[], kind: ChannelKind): number[] {
  return stations.map((s) => chan(s.env, kind)).filter((v): v is number => v != null);
}

function presentRain(stations: WxStation[], key: 'in1h' | 'in24h'): number[] {
  return stations
    .map((s) => s.env.rain?.[key] ?? null)
    .filter((v): v is number => v != null);
}

/** "4m ago" / "2h ago" — coarse, honest freshness. */
function ageText(nowMs: number, atMs: number): string {
  const min = Math.max(0, Math.round((nowMs - atMs) / 60000));
  if (min < 60) return `${min}m`;
  const h = Math.floor(min / 60);
  return `${h}h${min % 60 ? ` ${min % 60}m` : ''}`;
}

function fmtTemp(v: number): string {
  return `${Math.round(v)}°F`;
}

/** 2-digit UTC. */
function pad(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

/** `2026-06-20 1715Z`. */
function utcStamp(ms: number): string {
  const d = new Date(ms);
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}${pad(d.getUTCMinutes())}Z`;
}

/** `201715Z` (DDHHMM) for the subject. */
function ddhhmm(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getUTCDate())}${pad(d.getUTCHours())}${pad(d.getUTCMinutes())}Z`;
}

/** Round to at most 1 decimal, dropping a trailing `.0`. */
function r1(v: number): string {
  return (Math.round(v * 10) / 10).toString();
}

function tempWord(maxF: number): string {
  if (maxF < 32) return 'cold';
  if (maxF < 50) return 'chilly';
  if (maxF < 66) return 'cool';
  if (maxF < 81) return 'mild';
  if (maxF < 91) return 'warm';
  return 'hot';
}

/** Honest, conservative plain-language assessment derived ONLY from the readings.
 * Returns the "on the ground" sentence and the "for arrivals" guidance. */
export function assessConditions(stations: WxStation[]): { ground: string; arrivals: string } {
  const temps = present(stations, 'temperature');
  const gusts = present(stations, 'wind_gust');
  const winds = present(stations, 'wind_speed');
  const rain1h = presentRain(stations, 'in1h');
  const rain24h = presentRain(stations, 'in24h');
  const trends = stations.map((s) => pressureTrend(s.env)).filter((v): v is number => v != null);

  const tmm = minMax(temps);
  const maxGust = gusts.length ? Math.max(...gusts) : null;
  const maxWind = winds.length ? Math.max(...winds) : null;
  const wetNow = rain1h.some((v) => v > 0);
  const recentRain = rain24h.some((v) => v >= 1);
  const falling = trends.length > 0 && trends.filter((t) => t < -0.5).length > trends.length / 2;

  const groundParts: string[] = [];
  if (tmm) groundParts.push(`${tempWord(tmm.max)}`);
  if (wetNow) groundParts.push('rain falling');
  else if (recentRain) groundParts.push('recent rain');
  if (maxGust != null && maxGust >= 25) groundParts.push('gusty wind');
  else if (maxWind != null && maxWind >= 20) groundParts.push('windy');
  if (falling) groundParts.push('pressure falling');

  const ground = groundParts.length
    ? `${groundParts.join(', ').replace(/^./, (c) => c.toUpperCase())}.`
    : 'Limited data heard; see readings below.';

  const advice: string[] = [];
  if (wetNow || recentRain) advice.push('rain gear');
  if (tmm && tmm.min < 50) advice.push('layers/warm clothing');
  if ((maxGust != null && maxGust >= 25) || (maxWind != null && maxWind >= 20)) {
    advice.push('expect wind chill and blowing debris');
  }
  if (recentRain) advice.push('watch for localized flooding and slick roads');
  const arrivals = advice.length
    ? `For arrivals: ${advice.join('; ')}.`
    : 'For arrivals: conditions appear unremarkable from data heard.';

  return { ground, arrivals };
}

/** Render a `min–max unit` range, or a single value when min==max, or "" when empty. */
function range(mm: { min: number; max: number } | null, fmt: (n: number) => string): string {
  if (!mm) return '';
  return mm.min === mm.max ? fmt(mm.min) : `${fmt(mm.min)}–${fmt(mm.max)}`;
}

/** Area label for the header/subject: operator grid (4-char field+square) when known. */
function areaLabel(stations: WxStation[], operatorGrid?: string): string {
  if (operatorGrid && operatorGrid.length >= 4) return `grid ${operatorGrid.slice(0, 4).toUpperCase()}`;
  if (stations.length) {
    const lat = stations.reduce((a, s) => a + s.lat, 0) / stations.length;
    const lon = stations.reduce((a, s) => a + s.lon, 0) / stations.length;
    return `grid ${latLonToGrid(lat, lon).slice(0, 4).toUpperCase()}`;
  }
  return 'local area';
}

/** Per-station detail row: `CALL  GRID  58°F  NE 12 G24  96%  1.1"/h   4m`. */
function stationRow(s: WxStation, nowMs: number): string {
  const env = s.env;
  const grid = latLonToGrid(s.lat, s.lon).slice(0, 6);
  const parts: string[] = [];
  const t = chan(env, 'temperature');
  parts.push(t != null ? fmtTemp(t) : '—');
  const dir = chan(env, 'wind_dir');
  const spd = chan(env, 'wind_speed');
  const gst = chan(env, 'wind_gust');
  if (spd != null || dir != null || gst != null) {
    let w = '';
    if (dir != null) w += `${degToCompass(dir)} `;
    w += spd != null ? `${Math.round(spd)}` : '—';
    if (gst != null) w += ` G${Math.round(gst)}`;
    parts.push(w.trim());
  } else {
    parts.push('—');
  }
  const h = chan(env, 'humidity');
  parts.push(h != null ? `${Math.round(h)}%` : '—');
  const r1h = env.rain?.in1h ?? null;
  parts.push(r1h != null ? `${r1(r1h)}"/h` : '—');
  return `  ${s.call.padEnd(9)} ${grid.padEnd(7)} ${parts.join('  ')}   ${ageText(nowMs, s.at)} ago`;
}

/**
 * Compose the local-area weather SITREP from the heard WX stations.
 * Returns the Winlink subject + plain-text body. Caller is responsible for there
 * being at least one station (the control is disabled with none — nothing honest
 * to report); called with an empty list it still returns a valid "no stations
 * heard" report rather than throwing.
 */
export function composeWxSitrep(stations: WxStation[], meta: WxSitrepMeta): WxSitrep {
  const { nowMs, operatorGrid } = meta;
  const area = areaLabel(stations, operatorGrid);
  const subject = `WX SITREP ${area} — ${ddhhmm(nowMs)}`;

  if (stations.length === 0) {
    const body = `LOCAL WX GROUND TRUTH — ${area}\n${utcStamp(nowMs)}\n\nNo weather stations heard over RF. Nothing to report.\n`;
    return { subject, body };
  }

  const oldest = Math.max(...stations.map((s) => nowMs - s.at));
  const windowMin = Math.max(1, Math.ceil(oldest / 60000));
  const { ground, arrivals } = assessConditions(stations);

  const tmm = minMax(present(stations, 'temperature'));
  const hmm = minMax(present(stations, 'humidity'));
  const pmm = minMax(present(stations, 'pressure'));
  const winds = present(stations, 'wind_speed');
  const gusts = present(stations, 'wind_gust');
  const dirs = present(stations, 'wind_dir');
  const rain1 = presentRain(stations, 'in1h');
  const rain24 = presentRain(stations, 'in24h');
  const snow = present(stations, 'snow');
  const trends = stations.map((s) => pressureTrend(s.env)).filter((v): v is number => v != null);
  const falling = trends.length > 0 && trends.filter((t) => t < -0.5).length > trends.length / 2;
  const rising = trends.length > 0 && trends.filter((t) => t > 0.5).length > trends.length / 2;
  const trendWord = falling ? ', falling' : rising ? ', rising' : '';

  const lines: string[] = [];
  lines.push(`LOCAL WX GROUND TRUTH — ${area}`);
  lines.push(`${utcStamp(nowMs)} · ${stations.length} station${stations.length === 1 ? '' : 's'} heard over RF, last ${windowMin} min`);
  lines.push('');
  lines.push(`ON THE GROUND: ${ground} ${arrivals}`);
  lines.push('');
  lines.push('RANGES (across stations heard)');
  if (tmm) lines.push(`  Temp      ${range(tmm, (v) => `${Math.round(v)}`)}°F`);
  if (winds.length || gusts.length || dirs.length) {
    const dirText = dirs.length ? `, from ${degToCompass(dirs[Math.floor(dirs.length / 2)])}` : '';
    const windPart = winds.length ? `to ${Math.max(...winds)} mph` : 'calm';
    const gustPart = gusts.length ? `, gusts to ${Math.max(...gusts)}` : '';
    lines.push(`  Wind      ${windPart}${gustPart}${dirText}`);
  }
  if (hmm) lines.push(`  Humidity  ${range(hmm, (v) => `${Math.round(v)}`)} %`);
  if (pmm) lines.push(`  Pressure  ${range(pmm, (v) => r1(v))} hPa${trendWord}`);
  if (rain1.length) {
    const max24 = rain24.length ? ` (max ${r1(Math.max(...rain24))} in/24h)` : '';
    lines.push(`  Rain      ${range(minMax(rain1), (v) => `${r1(v)}`)} in/1h${max24}`);
  } else if (rain24.length) {
    lines.push(`  Rain      up to ${r1(Math.max(...rain24))} in/24h`);
  }
  lines.push(`  Snow      ${snow.some((v) => v > 0) ? `up to ${r1(Math.max(...snow))} in/24h` : 'none reported'}`);
  lines.push('');
  lines.push('STATIONS (callsign · grid · temp · wind · humidity · rain/h · heard)');
  for (const s of [...stations].sort((a, b) => b.at - a.at)) {
    lines.push(stationRow(s, nowMs));
  }
  lines.push('');
  lines.push(
    `RF honesty: only stations actually heard over RF are listed (${stations.length} station${stations.length === 1 ? '' : 's'}, last ${windowMin} min); oldest reading ${ageText(nowMs, nowMs - oldest)} ago. Fields absent on the wire show "—".`,
  );

  return { subject, body: lines.join('\n') + '\n' };
}
