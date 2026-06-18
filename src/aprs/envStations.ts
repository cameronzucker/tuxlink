// src/aprs/envStations.ts
//
// Pure data-shaping core for the source-reactive APRS environmental panel
// (tuxlink-2phz). The engine emits point-in-time DTOs — `aprs-weather:new`
// (fixed WX fields) and `aprs-telemetry:new` (self-named T# channels). This
// module merges them BY CALLSIGN into one per-station view-model whose cards
// AUTO-COMPOSE from the channels each station actually emits — no weather /
// telemetry mode, no per-card kind. A station sending BOTH appears once, with
// both sets of channels.
//
// RF-honesty is carried straight through: a WX field absent from the wire is
// never fabricated as 0 (it simply yields no channel); a telemetry channel with
// no EQNS is shown as a raw count (`scaled: false`); the per-channel history
// ring is buffered FRONTEND-side (the engine emits points, the graph needs the
// series), bounded so a long-lived session cannot grow unbounded.
//
// This file is pure (no React, no Tauri) so the merge / ring / staleness logic
// is unit-tested directly; `useEnvStations` is a thin event-subscription shell.

import type { WeatherReportDto, InboundTelemetryDto } from './aprsTypes';

/// How a channel is rendered. The renderer is a `kind → component` map: angular
/// (`wind_dir` → compass), `pressure` → graded chart + rise/fall trend, and
/// everything else → a graded X/Y-grid chart. `generic` is a self-named T#
/// channel. Rain is NOT a channel kind — it is a totals block (see
/// [`EnvStation.rain`]).
export type ChannelKind =
  | 'wind_dir'
  | 'wind_speed'
  | 'wind_gust'
  | 'temperature'
  | 'humidity'
  | 'pressure'
  | 'luminosity'
  | 'snow'
  | 'generic';

/// One point in a channel's frontend-buffered history ring. `at` is local
/// epoch-ms when the frame was heard (an honest client-stamp, not a wire time).
export interface ChannelSample {
  value: number;
  at: number;
}

/// A single rendered channel on a station card. `history` is the bounded ring
/// the graded chart plots; `value` is the latest reading. `scaled` is the
/// telemetry raw-vs-scaled honesty flag (always true for WX engineering units).
export interface EnvChannel {
  /// Stable identity within a station, so re-heard frames append to the right
  /// ring (`wx:<kind>` for weather fields, `tlm:<name>` for telemetry channels).
  key: string;
  label: string;
  unit: string;
  kind: ChannelKind;
  value: number;
  scaled: boolean;
  history: ChannelSample[];
}

/// A digital telemetry bit, rendered as an LED pill. `sense` is the channel's
/// defined active sense from `BITS.` — the decoded `value` is never inverted.
export interface EnvBit {
  key: string;
  label: string;
  value: boolean;
  sense: boolean;
}

/// Rain totals decoded from a WX report. Rendered as totals + a fill bar, NOT a
/// time-series chart (APRS reports running accumulations, not instantaneous
/// rate). A null sub-total is absent on the wire, not zero rainfall.
export interface RainTotals {
  in1h: number | null;
  in24h: number | null;
  sinceMidnight: number | null;
}

/// One heard station's merged environmental view-model. `channels` and `bits`
/// auto-compose from whatever the station emits across both sources.
export interface EnvStation {
  call: string;
  /// Best available human descriptor: telemetry project name, else the WX/TLM
  /// comment (station/software id). "" when none heard.
  project: string;
  /// Latest telemetry sequence number, or null for a weather-only station.
  seq: number | null;
  channels: EnvChannel[];
  bits: EnvBit[];
  rain: RainTotals | null;
  /// Local epoch-ms of the most recent frame from this station (either source).
  lastHeard: number;
}

/// Per-channel history cap. APRS WX stations beacon every few minutes and
/// telemetry stations more often; 180 points covers hours of either without
/// letting a day-long session grow the ring without bound.
export const HISTORY_CAP = 180;

/// A station is dimmed as stale once no frame (either source) has arrived within
/// this window — ~2–4 missed beacons for a typical WX station.
export const STALE_AFTER_MS = 30 * 60 * 1000;

/// A station is dropped from the panel after this much silence, so a station
/// that goes off the air eventually clears rather than lingering dimmed forever.
export const STATION_TTL_MS = 2 * 60 * 60 * 1000;

/// True when `station` has not been heard within [`STALE_AFTER_MS`] of `now`.
export function isStale(station: EnvStation, now: number): boolean {
  return now - station.lastHeard > STALE_AFTER_MS;
}

/// Append a sample to a ring, keeping at most [`HISTORY_CAP`] (oldest dropped).
function pushSample(history: ChannelSample[], value: number, at: number): ChannelSample[] {
  const next = [...history, { value, at }];
  return next.length > HISTORY_CAP ? next.slice(next.length - HISTORY_CAP) : next;
}

/// Merge an incoming reading for one channel into a station's channel list,
/// appending the frame's sample to the matching ring (by `key`) or creating the
/// channel fresh. `template` carries the latest label/unit/kind/scaled; the ring
/// accumulates across frames, bounded by [`HISTORY_CAP`].
function mergeChannel(
  channels: EnvChannel[],
  template: Omit<EnvChannel, 'history'>,
  at: number,
): EnvChannel[] {
  const idx = channels.findIndex((c) => c.key === template.key);
  const prior = idx === -1 ? [] : channels[idx].history;
  const merged: EnvChannel = { ...template, history: pushSample(prior, template.value, at) };
  if (idx === -1) return [...channels, merged];
  const next = [...channels];
  next[idx] = merged;
  return next;
}

/// One WX field → channel descriptor (label/unit/kind), or null when the field
/// is absent on the wire (so no fabricated channel is created).
interface WxFieldSpec {
  key: string;
  label: string;
  unit: string;
  kind: ChannelKind;
  read: (dto: WeatherReportDto) => number | null;
}

const WX_FIELDS: WxFieldSpec[] = [
  { key: 'wx:wind_dir', label: 'Wind', unit: '°', kind: 'wind_dir', read: (d) => d.windDirectionDeg },
  { key: 'wx:wind_speed', label: 'Wind speed', unit: 'mph', kind: 'wind_speed', read: (d) => d.windSpeedMph },
  { key: 'wx:wind_gust', label: 'Gust', unit: 'mph', kind: 'wind_gust', read: (d) => d.windGustMph },
  { key: 'wx:temperature', label: 'Temp', unit: '°F', kind: 'temperature', read: (d) => d.temperatureF },
  { key: 'wx:humidity', label: 'Humidity', unit: '%', kind: 'humidity', read: (d) => d.humidityPct },
  { key: 'wx:pressure', label: 'Pressure', unit: 'hPa', kind: 'pressure', read: (d) => d.pressureHpa },
  { key: 'wx:luminosity', label: 'Luminosity', unit: 'W/m²', kind: 'luminosity', read: (d) => d.luminosityWm2 },
  { key: 'wx:snow', label: 'Snow', unit: 'in', kind: 'snow', read: (d) => d.snowIn },
];

function blankStation(call: string): EnvStation {
  return { call, project: '', seq: null, channels: [], bits: [], rain: null, lastHeard: 0 };
}

/// Pick the best human descriptor, preferring a non-empty new value but keeping
/// the prior one rather than blanking it when a later frame carries none.
function pickProject(prev: string, ...candidates: string[]): string {
  for (const c of candidates) if (c && c.trim()) return c.trim();
  return prev;
}

/// Apply a heard `aprs-weather:new` frame to a station's prior view-model
/// (or create it), appending the frame's readings to each present field's ring.
export function applyWeather(
  prev: EnvStation | undefined,
  dto: WeatherReportDto,
  at: number,
): EnvStation {
  const base = prev ?? blankStation(dto.station);
  let channels = base.channels;
  for (const f of WX_FIELDS) {
    const v = f.read(dto);
    if (v === null) continue;
    channels = mergeChannel(
      channels,
      { key: f.key, label: f.label, unit: f.unit, kind: f.kind, value: v, scaled: true },
      at,
    );
  }
  const rain =
    dto.rain1hIn === null && dto.rain24hIn === null && dto.rainSinceMidnightIn === null
      ? base.rain
      : { in1h: dto.rain1hIn, in24h: dto.rain24hIn, sinceMidnight: dto.rainSinceMidnightIn };
  return {
    ...base,
    channels,
    rain,
    project: pickProject(base.project, dto.comment),
    lastHeard: at,
  };
}

/// Apply a heard `aprs-telemetry:new` frame to a station's prior view-model
/// (or create it): self-named analog channels accumulate as `generic` rings;
/// digital channels become LED-pill bits.
export function applyTelemetry(
  prev: EnvStation | undefined,
  dto: InboundTelemetryDto,
  at: number,
): EnvStation {
  const base = prev ?? blankStation(dto.station);
  let channels = base.channels;
  for (const a of dto.analog) {
    channels = mergeChannel(
      channels,
      { key: `tlm:${a.name}`, label: a.name, unit: a.unit, kind: 'generic', value: a.value, scaled: a.scaled },
      at,
    );
  }
  const bits: EnvBit[] = dto.digital.map((b) => ({
    key: `bit:${b.name}`,
    label: b.name,
    value: b.value,
    sense: b.sense,
  }));
  return {
    ...base,
    channels,
    bits: bits.length > 0 ? bits : base.bits,
    seq: dto.seq ?? base.seq,
    project: pickProject(base.project, dto.project, dto.comment),
    lastHeard: at,
  };
}

/// Drop stations not heard within [`STATION_TTL_MS`] of `now`. Returns the same
/// map reference when nothing expired so callers can skip a needless re-render.
export function pruneStations(
  byCall: Map<string, EnvStation>,
  now: number,
): Map<string, EnvStation> {
  let expired = false;
  for (const s of byCall.values()) {
    if (now - s.lastHeard > STATION_TTL_MS) {
      expired = true;
      break;
    }
  }
  if (!expired) return byCall;
  const next = new Map(byCall);
  for (const [call, s] of next) {
    if (now - s.lastHeard > STATION_TTL_MS) next.delete(call);
  }
  return next;
}
