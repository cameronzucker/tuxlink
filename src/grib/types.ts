// GRIB-request DTOs mirroring src-tauri/src/grib/composer.rs::GribRequest.
// Wire spec: https://saildocs.com/gribinfo (recap in
// docs/design/2026-06-02-cms-request-protocol-grounding.md).

export type GribMode = 'send' | 'sub';
export type GribDirection = 'N' | 'S' | 'E' | 'W';

export interface Latitude {
  /// 0..90, whole degrees only.
  degrees: number;
  /// Must be 'N' or 'S'.
  dir: GribDirection;
}

export interface Longitude {
  /// 0..180, whole degrees only.
  degrees: number;
  /// Must be 'E' or 'W'.
  dir: GribDirection;
}

/// Either a literal forecast hour (`24`) or a range (`{ start: 12, end: 96 }`
/// rendered as `12..96`).
export type ForecastTime = { Hour: number } | { Range: { start: number; end: number } };

export type GribParameter = 'PRMSL' | 'WIND' | 'HGT' | 'SEATMP' | 'AIRTMP' | 'WAVES';

export const ALL_GRIB_PARAMETERS: readonly GribParameter[] = [
  'PRMSL',
  'WIND',
  'HGT',
  'SEATMP',
  'AIRTMP',
  'WAVES',
] as const;

export interface GribRequest {
  mode: GribMode;
  lat0: Latitude;
  lat1: Latitude;
  lon0: Longitude;
  lon1: Longitude;
  /// (dlat, dlon) in degrees. Saildocs default 2,2.
  grid: [number, number];
  /// Empty → Saildocs default [24, 48, 72] applies.
  times: ForecastTime[];
  /// Empty → Saildocs default [PRESS, WIND] applies.
  params: GribParameter[];
  /// `sub` mode: subscription length in days.
  sub_days: number | null;
  /// `sub` mode: daily delivery time in HH:MM UTC.
  sub_time: string | null;
  /// Operator-editable label. Default "GRIB request".
  subject: string;
}

/// Default request — minimal canonical Saildocs example
/// `send gfs:40N,60N,140W,120W`. The operator-facing form starts here
/// (US west coast, all defaults).
export const DEFAULT_GRIB_REQUEST: GribRequest = {
  mode: 'send',
  lat0: { degrees: 40, dir: 'N' },
  lat1: { degrees: 60, dir: 'N' },
  lon0: { degrees: 140, dir: 'W' },
  lon1: { degrees: 120, dir: 'W' },
  grid: [2, 2],
  times: [],
  params: [],
  sub_days: null,
  sub_time: null,
  subject: 'GRIB request',
};

/// Parse a UI string of comma-separated forecast times into
/// `ForecastTime[]`. Returns `{ ok: true, value }` on success or
/// `{ ok: false, error }` on bad syntax. Empty string → empty array
/// (Saildocs default applies). Accepts `24`, `24,48,72`, `6,12..96`.
export function parseForecastTimes(
  raw: string,
): { ok: true; value: ForecastTime[] } | { ok: false; error: string } {
  const trimmed = raw.trim();
  if (trimmed === '') return { ok: true, value: [] };
  const out: ForecastTime[] = [];
  for (const piece of trimmed.split(',')) {
    const segment = piece.trim();
    if (segment === '') {
      return { ok: false, error: 'empty forecast-time segment' };
    }
    if (segment.includes('..')) {
      const [s, e] = segment.split('..', 2);
      const start = Number(s);
      const end = Number(e);
      if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end < 0) {
        return { ok: false, error: `invalid range "${segment}" — expected start..end (non-negative integers)` };
      }
      if (end <= start) {
        return { ok: false, error: `range "${segment}" — end must be greater than start` };
      }
      out.push({ Range: { start, end } });
    } else {
      const hour = Number(segment);
      if (!Number.isInteger(hour) || hour < 0) {
        return { ok: false, error: `invalid forecast hour "${segment}" — expected non-negative integer` };
      }
      out.push({ Hour: hour });
    }
  }
  return { ok: true, value: out };
}
