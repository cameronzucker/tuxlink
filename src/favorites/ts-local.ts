// ts-local — the offset-bearing local timestamp for a connection record (M4).
//
// The connection log stores `ts_local` VERBATIM (H1): an ISO8601 string that
// carries the machine's UTC offset (e.g. "2026-06-08T21:42:05-07:00"). The
// offset is the load-bearing part — the time-of-day feature buckets attempts by
// LOCAL wall-clock, so an offset-stripped `Z` timestamp would silently shift
// every record into UTC and defeat the ToD-local bucketing.
//
// `Date.prototype.toISOString()` is FORBIDDEN here: it emits a `Z`-suffixed UTC
// string with no offset. This helper builds the string from LOCAL wall-clock
// getters plus the machine offset so the offset is preserved.

/**
 * Offset-bearing local ISO8601 timestamp (e.g. "2026-06-08T21:42:05-07:00").
 * Built from LOCAL wall-clock components + the machine offset — never
 * `toISOString()` (which emits `Z`, stripping the offset).
 *
 * Offset sign: `Date.prototype.getTimezoneOffset()` returns minutes BEHIND UTC
 * (positive for west, e.g. US Pacific = +420 / +480). The ISO offset for
 * west-of-UTC is NEGATIVE (`-07:00`), so the ISO offset minutes are
 * `-getTimezoneOffset()`.
 */
export function tsLocal(now: Date = new Date()): string {
  const pad = (n: number) => String(Math.trunc(Math.abs(n))).padStart(2, '0');
  const offMin = -now.getTimezoneOffset(); // minutes EAST of UTC (Pacific → -420)
  const sign = offMin >= 0 ? '+' : '-';
  const y = now.getFullYear();
  return (
    `${y}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}` +
    `T${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}` +
    `${sign}${pad(offMin / 60)}:${pad(offMin % 60)}`
  );
}
