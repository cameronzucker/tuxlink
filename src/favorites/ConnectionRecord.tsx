// ConnectionRecord — the connection record line + outcome strip + the gated
// time-of-day hint (Task B5; props-lifted for tuxlink-je5d).
//
// The design's internal framing for this surface is the "honest connection
// record" — it states the OBSERVED record plainly and NEVER predicts. That
// framing is fine HERE in a code comment; NO user-facing string may carry the
// word "honest" (VOICE). The ToD hint states observed counts, never a forecast.
//
// PROPS-DRIVEN (tuxlink-je5d): this component renders `attempts` + `hint` passed
// in by its caller. It no longer fetches `favorite_tod_hint` itself — that lets
// the same render serve BOTH the favorites caller (FavoriteRow, which fetches
// the per-favorite hint) and the contacts caller (ContactDetail, which fetches
// the aggregate-by-callsign record). Lifting the query out is the only behavior
// change; the strip / record line / hint / empty-state markup is unchanged.
//
// L2: the wall-clock comes from the `ts_local` OFFSET literal (the station's own
// clock) via `stationWallClock`; the "ago" delta uses the absolute instant via
// `relativeAgo`. See record-format.ts.

import type { ConnectionAttempt, TodHint } from './types';
import { relativeAgo, stationWallClock } from './record-format';

export interface ConnectionRecordProps {
  /** The attempts to render (already scoped to the unit/callsign by the caller). */
  attempts: ConnectionAttempt[];
  /** The gated time-of-day hint, or null when the caller has none to show. */
  hint: TodHint | null;
  /** Test-only injection point for a deterministic "now"; defaults to live clock. */
  now?: Date;
}

/** Most-recent-first copy of the attempts (does not mutate the input). */
function sortedNewestFirst(attempts: ConnectionAttempt[]): ConnectionAttempt[] {
  return [...attempts].sort(
    (a, b) => new Date(b.ts_local).getTime() - new Date(a.ts_local).getTime(),
  );
}

/** Plain, non-predictive labels for each ToD bucket. */
const BUCKET_LABEL: Record<TodHint['bucket'], string> = {
  dawn: 'dawn',
  day: 'daytime',
  dusk: 'dusk',
  night: 'night',
};

export function ConnectionRecord({ attempts, hint, now }: ConnectionRecordProps) {
  const ref = now ?? new Date();
  const ordered = sortedNewestFirst(attempts);

  const reached = ordered.filter((a) => a.outcome === 'reached');
  const failed = ordered.filter((a) => a.outcome === 'failed');

  let line: string;
  if (reached.length > 0) {
    const last = reached[0];
    const wall = stationWallClock(last.ts_local);
    const ago = relativeAgo(last.ts_local, ref);
    line = wall
      ? `reached ${ago} · ${wall} local`
      : `reached ${ago}`;
  } else if (failed.length > 0) {
    const last = failed[0];
    const ago = relativeAgo(last.ts_local, ref);
    const noun = failed.length === 1 ? 'attempt' : 'attempts';
    line = `no successful connect yet · ${failed.length} ${noun} failed, last ${ago}`;
  } else {
    line = 'no connection attempts yet';
  }

  // Up to 5 glyphs, most-recent first.
  const glyphs = ordered.slice(0, 5).map((a) => (a.outcome === 'reached' ? '✓' : '✗'));

  return (
    <div className="favorite-record" data-testid="connection-record">
      {glyphs.length > 0 && (
        <span
          className="favorite-record-strip"
          data-testid="connection-record-strip"
          aria-label="recent connection outcomes"
        >
          {glyphs.map((g, i) => (
            <span
              key={i}
              className={`favorite-record-glyph favorite-record-glyph--${
                g === '✓' ? 'ok' : 'fail'
              }`}
            >
              {g}
            </span>
          ))}
        </span>
      )}
      <span className="favorite-record-line" data-testid="connection-record-line">
        {line}
      </span>
      {hint && (
        <span className="favorite-record-hint" data-testid="connection-record-hint">
          Reached most at {BUCKET_LABEL[hint.bucket]} · {hint.successes} of {hint.attempts}
        </span>
      )}
    </div>
  );
}
