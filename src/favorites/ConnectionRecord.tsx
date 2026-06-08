// ConnectionRecord — the per-unit connection record line + outcome strip + the
// gated time-of-day hint (Task B5).
//
// The design's internal framing for this surface is the "honest connection
// record" — it states the OBSERVED record plainly and NEVER predicts. That
// framing is fine HERE in a code comment; NO user-facing string may carry the
// word "honest" (VOICE). The ToD hint states observed counts, never a forecast.
//
// L2: the wall-clock comes from the `ts_local` OFFSET literal (the station's own
// clock) via `stationWallClock`; the "ago" delta uses the absolute instant via
// `relativeAgo`. See record-format.ts.

import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { ConnectionAttempt, TodHint } from './types';
import { relativeAgo, stationWallClock } from './record-format';

export interface ConnectionRecordProps {
  unitId: string;
  /** This unit's attempts (already filtered by unit_id). */
  attempts: ConnectionAttempt[];
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

export function ConnectionRecord({ unitId, attempts, now }: ConnectionRecordProps) {
  const ref = now ?? new Date();
  const ordered = sortedNewestFirst(attempts);

  // The gated hint. Rendered ONLY when the backend returns a non-null TodHint —
  // never synthesized, never a prediction. The backend already excludes
  // zero-success buckets (H2).
  const hintQuery = useQuery({
    queryKey: ['favorite_tod_hint', unitId],
    // Tauri auto-camelCases Rust snake_case command args on the JS wire:
    // `unit_id: String` in Rust → `unitId` key here (see PacketRadioPanel.tsx:79).
    // Snake_case keys silently fail to bind and the command no-ops at runtime.
    queryFn: () => invoke<TodHint | null>('favorite_tod_hint', { unitId }),
  });
  const hint = hintQuery.data ?? null;

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
