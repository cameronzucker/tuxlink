/**
 * Display formatters for the routines operator UI (plan-5 Task 6).
 *
 * These are pure functions — no Tauri, no React — so they're trivially unit
 * tested and reusable across every routines surface (library list, run
 * status, schedule status).
 */
import type { RunState, Trigger } from './routinesApi';

/**
 * The scheduler's `ITERATION_GUARD` (spec §8) caps how many missed fires it
 * will ever compute for one routine, so the true count is bounded — but a
 * display value should never dangle right at that internal implementation
 * detail. Anything at or above 100k renders as the clamp string, never the
 * raw number, so the UI doesn't leak (or imply precision about) the guard's
 * exact value.
 */
const MISSED_COUNT_DISPLAY_CLAMP = 100_000;

/** `n >= 100_000` → `'100k+'`; otherwise the plain decimal string. */
export function formatMissedCount(n: number): string {
  return n >= MISSED_COUNT_DISPLAY_CLAMP ? '100k+' : String(n);
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

/**
 * `unix` (seconds) → `HH:MMZ` when the instant falls on today in UTC,
 * otherwise `MM-DD HH:MMZ`. "Today" is evaluated against the caller's system
 * clock at call time (tests pin it with `vi.setSystemTime`).
 */
export function formatUtc(unix: number): string {
  const d = new Date(unix * 1000);
  const now = new Date();
  const isToday =
    d.getUTCFullYear() === now.getUTCFullYear() &&
    d.getUTCMonth() === now.getUTCMonth() &&
    d.getUTCDate() === now.getUTCDate();
  const time = `${pad2(d.getUTCHours())}:${pad2(d.getUTCMinutes())}Z`;
  if (isToday) return time;
  return `${pad2(d.getUTCMonth() + 1)}-${pad2(d.getUTCDate())} ${time}`;
}

/**
 * `Trigger` → an operator-facing one-liner. `'manual'` triggers read as
 * `'manual'`. `'schedule'` triggers read as `'every 30m'`, plus `' · align
 * hour'` when `align` is set, plus `' · window HH:MM-HH:MM'` when `window`
 * is set.
 */
export function formatTrigger(t: Trigger): string {
  if (t.type === 'manual') return 'manual';
  let s = `every ${t.every}`;
  if (t.align) s += ` · align ${t.align}`;
  if (t.window) s += ` · window ${t.window}`;
  return s;
}

/** Human labels for all 9 `RunState` values (journal.rs:17-30). */
const RUN_STATE_LABELS: Record<RunState, string> = {
  pending: 'Pending',
  running: 'Running',
  waiting: 'Waiting',
  awaiting_consent: 'Awaiting consent',
  awaiting_radio: 'Awaiting radio',
  completed: 'Completed',
  failed: 'Failed',
  cancelled: 'Cancelled',
  interrupted: 'Interrupted',
};

export function formatRunState(s: RunState): string {
  return RUN_STATE_LABELS[s];
}
