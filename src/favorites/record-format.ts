// record-format — pure formatters for the connection record line (Task B5).
//
// L2 (the subtle correctness point): a connection attempt's `ts_local` is an
// offset-bearing ISO8601 string stored VERBATIM (the station's own clock). The
// displayed wall-clock MUST be the literal HH:MM from that string — the clock
// the station observed — NOT a re-projection into the viewer's timezone
// (`new Date(...).getHours()`) NOR into UTC (`.getUTCHours()`). Both of those
// are WRONG for the station wall-clock. The relative "ago" delta is a different
// quantity: it legitimately uses the ABSOLUTE instant (an offset-aware parse).

/**
 * Extract the literal `HH:MM` from an offset-bearing ISO8601 timestamp — the
 * STATION wall-clock. e.g. `2026-06-07T21:42:00-07:00` → `'21:42'`, regardless
 * of the viewer's timezone. Returns `null` for an unparseable string.
 */
export function stationWallClock(tsLocal: string): string | null {
  const m = /T(\d{2}):(\d{2})/.exec(tsLocal);
  return m ? `${m[1]}:${m[2]}` : null;
}

/**
 * Relative-age label using the ABSOLUTE instant: `just now` (<1 min),
 * `N min ago`, `N h ago`, `N d ago`. The parse is offset-aware (`new Date(...)`
 * honors the offset), so this is the true elapsed time — distinct from the
 * station wall-clock literal above.
 */
export function relativeAgo(tsLocal: string, now: Date = new Date()): string {
  const then = new Date(tsLocal).getTime();
  if (Number.isNaN(then)) return '';
  const ms = now.getTime() - then;
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  return `${days} d ago`;
}
