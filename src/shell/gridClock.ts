import tzLookup from 'tz-lookup';
import { gridToLatLon } from '../forms/position/maidenhead';

export type ClockTimeSource = 'grid' | 'device';

export interface ClockParts {
  utc: string;
  local: string;
  localTitle: string;
  source: ClockTimeSource;
  timeZone: string | null;
}

export function timeZoneForGrid(grid: string | null): string | null {
  const normalized = grid?.trim();
  if (!normalized) return null;

  const point = gridToLatLon(normalized);
  if (!point) return null;

  try {
    return tzLookup(point.lat, point.lon);
  } catch {
    return null;
  }
}

export function formatUtcClock(now: Date): string {
  return `${pad(now.getUTCHours())}:${pad(now.getUTCMinutes())}z`;
}

export function formatLocalClock(now: Date, timeZone?: string): string {
  const options: Intl.DateTimeFormatOptions = {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
    timeZoneName: 'short',
  };
  if (timeZone) options.timeZone = timeZone;
  return new Intl.DateTimeFormat('en-US', options).format(now);
}

export function formatGridClock(now: Date, grid: string | null): ClockParts {
  const normalized = grid?.trim() || null;
  const timeZone = timeZoneForGrid(normalized);
  if (normalized && timeZone) {
    const precisionNote = normalized.length === 4
      ? ' Approximate from the 4-character grid center.'
      : '';
    return {
      utc: formatUtcClock(now),
      local: formatLocalClock(now, timeZone),
      localTitle: `Local time from grid ${normalized.toUpperCase()} (${timeZone}).${precisionNote}`,
      source: 'grid',
      timeZone,
    };
  }

  const fallbackReason = normalized
    ? `Grid ${normalized.toUpperCase()} is not a valid 4- or 6-character Maidenhead locator.`
    : 'No grid is configured or detected.';
  return {
    utc: formatUtcClock(now),
    local: formatLocalClock(now),
    localTitle: `${fallbackReason} Local time uses this device's timezone.`,
    source: 'device',
    timeZone: null,
  };
}

function pad(n: number): string {
  return String(n).padStart(2, '0');
}
