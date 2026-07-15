/**
 * Display formatters for the routines operator UI (plan-5 Task 6).
 *
 * These are pure functions — no Tauri, no React — so they're trivially unit
 * tested and reusable across every routines surface (library list, run
 * status, schedule status).
 */
import type { IfMissed, RunState, StepError, Trigger } from './routinesApi';
import { asUiError } from '../mailbox/types';

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

/**
 * Milliseconds elapsed → `HH:MM:SS` clock (the consent modal's live "parked
 * NN:NN:NN" readout, task-14 brief). Negative/NaN input clamps to `00:00:00`
 * rather than rendering a negative or garbage duration (a clock drift or a
 * `parkedAtMs` set at the instant of read should never show as negative).
 */
export function formatParkedDuration(ms: number): string {
  const totalSeconds = Number.isFinite(ms) ? Math.max(0, Math.floor(ms / 1000)) : 0;
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = totalSeconds % 60;
  return `${pad2(h)}:${pad2(m)}:${pad2(s)}`;
}

/**
 * The `if_missed` schedule policy → an operator-facing one-liner (dashboard
 * Task 8's Trigger column). `'skip'` and `'run_once_on_launch'` are the only
 * two `IfMissed` values (routinesApi.ts).
 */
export function formatIfMissed(im: IfMissed): string {
  return im === 'skip' ? 'missed: skip' : 'missed: run once on launch';
}

/**
 * The operator-facing cause text out of a failed step's `StepError`
 * (dashboard Task 8's Last-result column). `'action'`'s `detail.cause` and
 * `'unset_variable'`'s `detail` are already operator-facing text produced by
 * the backend — returned VERBATIM, never re-worded (task-8 brief binding
 * constraint 4). `'timeout'` and `'cancelled'` carry no message of their own
 * on the wire, so this synthesizes a short label for those two.
 */
export function formatStepErrorCause(e: StepError): string {
  switch (e.kind) {
    case 'action':
      return e.detail.cause;
    case 'unset_variable':
      return e.detail;
    case 'timeout':
      return `timeout after ${e.detail.seconds}s`;
    case 'cancelled':
      return 'cancelled';
  }
}

/**
 * The operator-facing message out of a rejected `invoke()` call — the
 * dashboard's arbiter-refusal strip (flow 6) and the import dialog's
 * save-failure text both need this. Mirrors `UiError`'s Rust-side
 * `refusal_reason` mapping (src-tauri/src/routines/scheduler.rs:1016-1028)
 * in TypeScript: `NotConfigured` / `NotFound` / `Rejected` carry their
 * operator-facing text directly in `detail` and are returned VERBATIM, never
 * re-worded (task-8 brief binding constraint 4/9). Falls back to the raw
 * thrown value's own message when it isn't the `UiError` discriminated-union
 * shape (e.g. a plain string or `Error` from a non-Tauri test harness).
 */
export function formatUiError(e: unknown): string {
  const ui = asUiError(e);
  if (!ui) return e instanceof Error ? e.message : typeof e === 'string' ? e : String(e);
  switch (ui.kind) {
    case 'NotConfigured':
    case 'NotFound':
    case 'Rejected':
      return ui.detail;
    case 'AuthFailed':
    case 'Transport':
    case 'Unavailable':
      return ui.detail.reason;
    case 'Internal':
      return ui.detail.detail;
    case 'Cancelled':
      return 'cancelled';
  }
}
