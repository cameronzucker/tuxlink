/**
 * RunsTab — the run list, journal-truth Gantt monitor, verbatim step detail,
 * redacted bundle export, and take-the-radio UI (routines plan-5 Task 13,
 * `.superpowers/sdd/task-13-brief.md`, spec §12 flows 3/5/6).
 *
 * Layout is the approved mock verbatim (dev/scratch/routines-ui-mocks/
 * run-monitor.html): `.runlist` left rail + `.runmain` (header/gantt/step
 * detail). The mock's "#12" run numbering and "runs/<routine>/12.jsonl" path
 * are BOTH fiction (documented decision, task-13 brief binding constraint
 * 3/2): the real journal file is `<run_id>.jsonl` (journal.rs:92) and there is
 * no per-routine ordinal on the wire — the list shows the real (short) run id
 * instead, newest-first by start time.
 *
 * ---- ganttModel: what the journal ACTUALLY carries (read before editing) ----
 *
 * `RunEvent::StateChanged` (journal.rs:49-51) is `{ state: RunState }` ONLY —
 * no step id, no track, no rig, no delay length. This is true for EVERY
 * parked state (`waiting`, `awaiting_consent`, `awaiting_radio`), not just
 * the arbiter-refusal case the brief calls out explicitly. Two consequences
 * this module works around rather than fabricating data to paper over:
 *
 * 1. `Control::Delay` (executor.rs:450-475) journals ONLY
 *    `StateChanged{Waiting}` before the sleep and `StateChanged{Running}`
 *    after — no `StepIntent` at all for the delay itself (unlike
 *    `Control::Call`, which does journal an intent). There is no step id to
 *    hang a delay bar off of.
 * 2. An attended-mode consent park (`awaiting_consent`) most plausibly
 *    happens BEFORE the retried step's own `StepIntent` is written (intent-
 *    before-effect means intent logs right before invoking, and the consent
 *    gate blocks the invoke) — so at the moment parking begins there is
 *    typically no OPEN intent to attribute it to either.
 *
 * `closeParkedWindow()` below handles both cases with ONE mechanism: when a
 * parked state begins, snapshot whichever step intents are currently OPEN
 * (unresolved). If any are open, attribute the parked interval to each of
 * those steps' lanes (the step that's mid-invocation when it discovers it
 * needs the radio, say). If none are open (the common case for a bare delay,
 * and the common case for a pre-intent consent gate), attribute it instead to
 * the lane of the most recently CLOSED step — the last thing that actually
 * ran, which is the best-effort "where we are" anchor the journal supports.
 * If neither exists (parking at the very start of a run, before any step has
 * even opened), the interval is dropped rather than invented a lane for it.
 *
 * The same "no step id on state_changed" gap is why the arbiter-refusal
 * banner (binding constraint 7) can't read a `rig` field off the
 * `state_changed` entry directly — there isn't one. `radioAwaitRig()` below
 * instead reads the real `rig` param off the journal's most recent
 * `step_intent.resolved_params` (the actual per-step field a radio action
 * reads to target a lease, `actions/mod.rs:462-468`'s `rig_id_from_params`,
 * defaulting to `"default"` exactly as the Rust side does) — verbatim journal
 * data, just sourced from the adjacent `step_intent` rather than a
 * nonexistent field on `state_changed` itself.
 *
 * A third open-intent case — separate from the parked windows above — is a
 * `step_intent` with no `step_ok`/`step_err` AT ALL by the end of the
 * journal: on a live run this is the step currently executing; on a
 * terminated run it's the step the process died inside of (the interrupted
 * scan appends only a terminal `RunFinished`, never a synthetic `StepErr` —
 * journal.rs's `scan_interrupted`). `ganttModel` flushes every such intent
 * as an open-ended bar after the entry walk: kind `running` (to the
 * now-line) while live, kind `interrupted` (to the journal's last entry)
 * otherwise — without this flush the very step under investigation on a
 * crashed run would render no bar and vanish from the monitor.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import {
  listRuns,
  runJournal,
  runStatus,
  cancelRun,
  takeRadio,
  exportRunBundle,
  type JournalEntry,
  type RunEvent,
  type RunListEntry,
  type RunState,
  type RunStatus,
} from '../routinesApi';
import { listenRoutinesEvents } from '../routinesEvents';
import { formatRunState, formatStepErrorCause, formatUiError, formatUtc } from '../format';
import './RunsTab.css';

export interface RunsTabProps {
  routine: string;
  highlightRunId?: string | null;
}

/** Non-terminal `RunState`s — polling continues, Cancel is offered. Mirrors
 *  RoutinesDashboard.tsx's `LIVE_STATES` (each routines surface owns its own
 *  copy of this small set rather than sharing an import — established
 *  per-file convention in this directory). */
const NON_TERMINAL = new Set<RunState>(['pending', 'running', 'waiting', 'awaiting_consent', 'awaiting_radio']);

const POLL_MS = 2000;

// ============================================================================
// ganttModel — pure helper, unit-tested directly against journal fixtures.
// ============================================================================

export interface GanttBar {
  /** `running`: a `step_intent` with no `step_ok`/`step_err` yet on a LIVE
   *  run — the currently-executing step, drawn open-ended to the now-line.
   *  `interrupted`: the same unclosed intent on a run that reached a terminal
   *  journal state anyway (crash-mid-step / interrupted recovery) — usually
   *  the exact step under investigation, drawn to the journal's last entry. */
  kind: 'ok' | 'fail' | 'delay' | 'consent' | 'running' | 'interrupted';
  /** The verbatim `RunState` for a parked bar (`delay`/`consent`) — one of
   *  `'waiting' | 'awaiting_consent' | 'awaiting_radio'`. Absent for ok/fail. */
  parkedState?: RunState;
  /** Absent only for a delay bar with no attributable step (see module doc). */
  stepId?: string;
  action?: string;
  t0: number;
  t1: number;
  /** The `step_intent` entry that opened this bar, when one exists. */
  intentEntry?: JournalEntry;
  /** The `step_ok`/`step_err` entry that closed this bar, when resolved. */
  resultEntry?: JournalEntry;
}

export interface GanttLane {
  track: string;
  bars: GanttBar[];
}

export interface GanttModel {
  lanes: GanttLane[];
  t0: number;
  t1: number;
  /** `true` when the journal has no `run_finished` entry yet — the caller
   *  draws the now-line and `t1` is extended to `now`. */
  live: boolean;
}

interface SnapshotStep {
  id?: unknown;
}
interface SnapshotTrack {
  name?: unknown;
  steps?: unknown;
}

function extractTracks(snapshot: unknown): { name: string; stepIds: string[] }[] {
  if (!snapshot || typeof snapshot !== 'object') return [];
  const tracks = (snapshot as { tracks?: unknown }).tracks;
  if (!Array.isArray(tracks)) return [];
  return tracks.map((t: SnapshotTrack, i: number) => {
    const name = typeof t?.name === 'string' ? t.name : `track-${i + 1}`;
    const steps = Array.isArray(t?.steps) ? (t.steps as SnapshotStep[]) : [];
    const stepIds = steps.map((s) => (typeof s?.id === 'string' ? s.id : '')).filter((id) => id.length > 0);
    return { name, stepIds };
  });
}

function stepIntentFields(event: RunEvent): { action: string; resolved_params: unknown } | null {
  return event.type === 'step_intent' ? { action: event.action, resolved_params: event.resolved_params } : null;
}

/** Pure: derives lane/bar geometry from a run's journal, verbatim (spec §11 /
 *  §15 "run monitor rendering from journal fixtures"). `now` (unix seconds)
 *  is injectable so this stays deterministic under test; defaults to the
 *  wall clock for real callers. */
export function ganttModel(entries: JournalEntry[], now: number = Math.floor(Date.now() / 1000)): GanttModel {
  const started = entries.find((e) => e.event.type === 'run_started');
  if (!started || started.event.type !== 'run_started') {
    return { lanes: [], t0: now, t1: now, live: false };
  }

  const tracks = extractTracks(started.event.snapshot);
  const stepToTrack = new Map<string, number>();
  tracks.forEach((t, i) => t.stepIds.forEach((id) => stepToTrack.set(id, i)));
  const lanes: GanttLane[] = tracks.map((t) => ({ track: t.name, bars: [] }));

  const openByStep = new Map<string, JournalEntry>();
  let lastClosed: { stepId: string; ts: number } | null = null;
  let parked: { state: RunState; ts: number; openSnapshot: Map<string, JournalEntry> } | null = null;

  const pushBar = (bar: GanttBar, stepId: string | undefined) => {
    const idx = stepId !== undefined ? stepToTrack.get(stepId) : undefined;
    if (idx !== undefined) lanes[idx].bars.push(bar);
  };

  const closeParked = (ts: number) => {
    if (!parked) return;
    const { state, ts: t0, openSnapshot } = parked;
    const kind: GanttBar['kind'] = state === 'waiting' ? 'delay' : 'consent';
    if (openSnapshot.size > 0) {
      for (const [stepId, intentEntry] of openSnapshot) {
        const fields = stepIntentFields(intentEntry.event);
        pushBar(
          { kind, parkedState: state, stepId, action: fields?.action, t0, t1: ts, intentEntry },
          stepId,
        );
      }
    } else if (lastClosed) {
      pushBar({ kind, parkedState: state, stepId: lastClosed.stepId, t0, t1: ts }, lastClosed.stepId);
    }
    parked = null;
  };

  for (const entry of entries) {
    const ev = entry.event;
    switch (ev.type) {
      case 'run_started':
        break;
      case 'step_intent':
        openByStep.set(ev.step, entry);
        break;
      case 'step_ok':
      case 'step_err': {
        const intentEntry = openByStep.get(ev.step);
        if (intentEntry && intentEntry.event.type === 'step_intent') {
          pushBar(
            {
              kind: ev.type === 'step_ok' ? 'ok' : 'fail',
              stepId: ev.step,
              action: intentEntry.event.action,
              t0: intentEntry.ts_unix,
              t1: entry.ts_unix,
              intentEntry,
              resultEntry: entry,
            },
            ev.step,
          );
        }
        openByStep.delete(ev.step);
        lastClosed = { stepId: ev.step, ts: entry.ts_unix };
        break;
      }
      case 'state_changed':
        if (ev.state === 'waiting' || ev.state === 'awaiting_consent' || ev.state === 'awaiting_radio') {
          parked = { state: ev.state, ts: entry.ts_unix, openSnapshot: new Map(openByStep) };
        } else if (parked) {
          closeParked(entry.ts_unix);
        }
        break;
      case 'run_finished':
        if (parked) closeParked(entry.ts_unix);
        break;
    }
  }

  const finished = entries.find((e) => e.event.type === 'run_finished');
  const live = !finished;
  if (live && parked) closeParked(now);

  const t0 = started.ts_unix;
  const lastEntryTs = entries.reduce((max, e) => Math.max(max, e.ts_unix), t0);
  const t1 = live ? Math.max(lastEntryTs, now) : lastEntryTs;

  // Flush every still-open step_intent as an OPEN-ENDED bar: on a live run
  // it's the currently-executing step (drawn to the now-line); on a
  // terminated run it's a step the process died inside of (interrupted
  // recovery) — usually the exact step under investigation — drawn to the
  // journal's last entry. Without this, an in-progress or crashed-mid-step
  // step produces NO bar at all and simply vanishes from the monitor.
  for (const [stepId, intentEntry] of openByStep) {
    const fields = stepIntentFields(intentEntry.event);
    pushBar(
      {
        kind: live ? 'running' : 'interrupted',
        stepId,
        action: fields?.action,
        t0: intentEntry.ts_unix,
        t1,
        intentEntry,
      },
      stepId,
    );
  }

  return { lanes, t0, t1, live };
}

/** The rig a currently-parked `awaiting_radio` state pertains to. The
 *  journal's `state_changed` entry carries no `rig` field (module doc) — this
 *  reads the real `rig` param off the most recent `step_intent`, mirroring
 *  the Rust side's own default (`actions/mod.rs`'s `rig_id_from_params`,
 *  `DEFAULT_RIG_ID = "default"`). */
export function radioAwaitRig(entries: JournalEntry[]): string {
  for (let i = entries.length - 1; i >= 0; i--) {
    const ev = entries[i]!.event;
    if (ev.type === 'step_intent') {
      const params = ev.resolved_params;
      if (params && typeof params === 'object' && typeof (params as { rig?: unknown }).rig === 'string') {
        return (params as { rig: string }).rig;
      }
      return 'default';
    }
  }
  return 'default';
}

// ============================================================================
// Small pure display helpers
// ============================================================================

function formatElapsed(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const hh = Math.floor(s / 3600);
  const mm = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  const p = (n: number) => n.toString().padStart(2, '0');
  return `${p(hh)}:${p(mm)}:${p(ss)}`;
}

function pct(t: number, t0: number, t1: number): number {
  if (t1 <= t0) return 0;
  return ((t - t0) / (t1 - t0)) * 100;
}

function axisTicks(t0: number, t1: number): number[] {
  if (t1 <= t0) return [t0];
  const span = t1 - t0;
  const rawStep = span / 8;
  const step = Math.max(60, Math.round(rawStep / 60) * 60);
  const ticks: number[] = [];
  for (let t = t0; t < t1; t += step) ticks.push(t);
  ticks.push(t1);
  return ticks;
}

interface StateBadge {
  cls: string;
  text: string;
}

const STATE_BADGE: Record<RunState, StateBadge> = {
  pending: { cls: 'run', text: 'pending' },
  running: { cls: 'run', text: 'running' },
  waiting: { cls: 'run', text: 'waiting' },
  awaiting_consent: { cls: 'run', text: 'awaiting consent' },
  awaiting_radio: { cls: 'run', text: 'awaiting radio' },
  completed: { cls: 'ok', text: '✓ completed' },
  failed: { cls: 'fail', text: '✕ failed' },
  cancelled: { cls: 'cxl', text: '⊘ cancelled' },
  interrupted: { cls: 'intr', text: '⚡ interrupted' },
};

function badgeFor(entry: RunListEntry): StateBadge {
  if (entry.dryRun) {
    if (entry.state === 'completed') return { cls: 'dry', text: 'dry-run ✓' };
    if (entry.state === 'failed') return { cls: 'dry', text: 'dry-run ✕' };
    return { cls: 'dry', text: `dry-run · ${formatRunState(entry.state).toLowerCase()}` };
  }
  return STATE_BADGE[entry.state];
}

/** Compact run-id label. Backend ids are `run-<unixsecs>-<NNNN>`
 * (executor.rs's `format!("run-{}-{n:04}", now)`), so the first characters
 * are IDENTICAL for every run started within the same ~11-day window — the
 * head-slice this shipped with rendered every rail row and the detail header
 * as the same `run-176845…` label (tuxlink-3awm9 WebKitGTK smoke). Keep the
 * TAIL: the timestamp's low digits + the per-second counter are the
 * discriminating part. */
function shortRunId(runId: string): string {
  return runId.length > 10 ? `…${runId.slice(-9)}` : runId;
}

// ============================================================================
// Component
// ============================================================================

export function RunsTab({ routine, highlightRunId }: RunsTabProps) {
  const [runs, setRuns] = useState<RunListEntry[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [status, setStatus] = useState<RunStatus | null>(null);
  const [journal, setJournal] = useState<JournalEntry[]>([]);
  const [selectedBar, setSelectedBar] = useState<GanttBar | null>(null);
  const [exportFeedback, setExportFeedback] = useState<string | null>(null);
  const [actionFeedback, setActionFeedback] = useState<string | null>(null);

  const runsSorted = useMemo(() => [...runs].sort((a, b) => b.startedUnix - a.startedUnix), [runs]);

  // Final whole-branch review, Fix 2: the left rail contradicted the live
  // run-detail pane beside it — `listRuns` was fetched exactly once per
  // routine/mount and never again, so a row's badge (e.g. "running") went
  // stale the instant that run actually finished. `loadRuns` is now a
  // reusable callback: one effect fetches it on mount/routine-change (below),
  // a second re-fetches on live run-progress events (mirrors
  // RoutinesDashboard.tsx's own `listRuns` + `listenRoutinesEvents` pairing).
  const loadRuns = useCallback(async () => {
    try {
      const list = await listRuns(routine);
      setRuns(Array.isArray(list) ? list : []);
    } catch {
      setRuns([]);
    }
  }, [routine]);

  // Load the run list once per routine/mount.
  useEffect(() => {
    void loadRuns();
  }, [loadRuns]);

  // Re-fetch on `runStarted` (a new row the rail hasn't seen yet) and
  // `runFinished` (the terminating run's own badge — completed/failed/
  // cancelled/interrupted — replacing whatever live badge it last painted;
  // this is also the "selected run reaches terminal state" refresh, since a
  // runFinished for the selected run is exactly that moment). Both event
  // kinds fire for every routine, not just this pane's `routine` prop, but
  // `listRuns(routine)` is already server-side scoped, so an out-of-scope
  // event just costs one harmless extra fetch.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    listenRoutinesEvents((event) => {
      if (event.kind === 'runStarted' || event.kind === 'runFinished') {
        void loadRuns();
      }
    })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {
        // No Tauri runtime in some tests/dev harnesses.
      });
    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [loadRuns]);

  // Default selection: the highlighted run (a just-started dry-run) if it's
  // in the list, otherwise the newest run. Only ever applied once — an
  // operator's own later selection is never overridden by a runs re-fetch.
  useEffect(() => {
    if (selectedRunId !== null || runsSorted.length === 0) return;
    const wantHighlight = highlightRunId && runsSorted.some((r) => r.runId === highlightRunId);
    setSelectedRunId(wantHighlight ? (highlightRunId as string) : runsSorted[0]!.runId);
  }, [runsSorted, highlightRunId, selectedRunId]);

  const fetchStatusAndJournal = useCallback(async (runId: string) => {
    const [s, j] = await Promise.all([runStatus(runId), runJournal(runId)]);
    return [s, j] as const;
  }, []);

  // Live polling (binding constraint 5): every 2s while non-terminal; stops
  // on terminal state or unmount/reselection. A fetch failure retries on the
  // same schedule rather than giving up (a transient invoke hiccup shouldn't
  // permanently stop the monitor for a still-live run).
  useEffect(() => {
    if (!selectedRunId) {
      setStatus(null);
      setJournal([]);
      return;
    }
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const loop = async () => {
      try {
        const [s, j] = await fetchStatusAndJournal(selectedRunId);
        if (cancelled) return;
        setStatus(s);
        setJournal(j);
        if (s && NON_TERMINAL.has(s.state)) {
          timer = setTimeout(() => {
            void loop();
          }, POLL_MS);
        }
      } catch {
        if (!cancelled) {
          timer = setTimeout(() => {
            void loop();
          }, POLL_MS);
        }
      }
    };
    void loop();

    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [selectedRunId, fetchStatusAndJournal]);

  // `runFinished` nudge (binding constraint 5): one immediate extra refresh
  // on top of the poll loop, so the terminal frame shows up without waiting
  // for the next 2s tick.
  useEffect(() => {
    if (!selectedRunId) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    listenRoutinesEvents((event) => {
      if (event.kind === 'runFinished' && event.runId === selectedRunId) {
        fetchStatusAndJournal(selectedRunId)
          .then(([s, j]) => {
            setStatus(s);
            setJournal(j);
          })
          .catch(() => {});
      }
    })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [selectedRunId, fetchStatusAndJournal]);

  const model = useMemo(() => ganttModel(journal), [journal]);

  // Auto-select a step to inspect (flow 3 "investigate failed run"): the
  // first failing bar if any, else the first parked bar, else nothing. Reset
  // whenever the run selection changes.
  useEffect(() => {
    setSelectedBar(null);
  }, [selectedRunId]);
  useEffect(() => {
    if (selectedBar) return;
    const allBars = model.lanes.flatMap((l) => l.bars);
    const fail = allBars.find((b) => b.kind === 'fail');
    const parked = allBars.find((b) => b.kind === 'consent');
    if (fail) setSelectedBar(fail);
    else if (parked) setSelectedBar(parked);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [model]);

  const selectedEntry = runsSorted.find((r) => r.runId === selectedRunId);
  const dryRun = status?.dryRun ?? selectedEntry?.dryRun ?? false;
  const isLive = status ? NON_TERMINAL.has(status.state) : false;
  const canTakeRadio = status ? status.state === 'running' || status.state === 'awaiting_radio' : false;
  const nowUnix = Math.floor(Date.now() / 1000);
  const started = selectedEntry?.startedUnix;
  const finished = selectedEntry?.finishedUnix ?? (status && !isLive ? nowUnix : null);
  const elapsed = started !== undefined ? (finished ?? nowUnix) - started : null;

  const handleExport = useCallback(async () => {
    if (!selectedRunId) return;
    setExportFeedback(null);
    const defaultPath = `tuxlink-run-${selectedRunId}.json`;
    const path = await saveDialog({
      defaultPath,
      filters: [{ name: 'Tuxlink Run Bundle', extensions: ['json'] }],
    });
    if (!path) return; // dialog cancel — no-op
    try {
      const result = await exportRunBundle(selectedRunId, path);
      setExportFeedback(`Saved to ${result.path}`);
    } catch (e) {
      setExportFeedback(`Export failed: ${formatUiError(e)}`);
    }
  }, [selectedRunId]);

  const handleCancel = useCallback(async () => {
    if (!selectedRunId) return;
    setActionFeedback(null);
    try {
      await cancelRun(selectedRunId);
    } catch (e) {
      setActionFeedback(`Cancel failed: ${formatUiError(e)}`);
    }
  }, [selectedRunId]);

  const handleTakeRadio = useCallback(async () => {
    setActionFeedback(null);
    try {
      await takeRadio();
    } catch (e) {
      setActionFeedback(`Take the radio failed: ${formatUiError(e)}`);
    }
  }, []);

  return (
    <div className="runs-body" data-testid="runs-tab">
      <div className="runlist" data-testid="runlist">
        <div className="runlist-head">RUNS · JOURNAL</div>
        {runsSorted.map((r) => {
          const badge = badgeFor(r);
          return (
            <div
              key={r.runId}
              className={`runrow${r.runId === selectedRunId ? ' sel' : ''}`}
              data-testid={`runrow-${r.runId}`}
              onClick={() => {
                // Clear the step-detail selection in the SAME event as the
                // run change — the effect keyed on selectedRunId also clears
                // it, but one frame later, which paints a stale detail card
                // for the outgoing run against the incoming run's header.
                setSelectedRunId(r.runId);
                setSelectedBar(null);
              }}
              role="button"
              tabIndex={0}
            >
              <span className="id mono" title={r.runId}>
                {shortRunId(r.runId)}
              </span>
              <span className={`rbadge ${badge.cls}`}>{badge.text}</span>
              <span className="when">{formatUtc(r.startedUnix)}</span>
            </div>
          );
        })}
        {runsSorted.length === 0 && <div className="runlist-empty">No runs yet.</div>}
      </div>

      <div className="runmain">
        {!selectedRunId && <div className="runmain-empty">Select a run to inspect.</div>}
        {selectedRunId && (
          <>
            <div className="runhead" data-testid="run-header">
              <span className="runtitle">
                Run {shortRunId(selectedRunId)} — {status ? formatRunState(status.state) : '…'}
              </span>
              <span className="runmeta">
                {started !== undefined && <>started {formatUtc(started)}</>}
                {elapsed !== null && <> · elapsed {formatElapsed(elapsed)}</>}
              </span>
              {canTakeRadio && (
                <button
                  type="button"
                  className="btn"
                  data-testid="take-radio-btn"
                  onClick={() => void handleTakeRadio()}
                >
                  Take the radio
                </button>
              )}
              {isLive && (
                <button type="button" className="btn" data-testid="cancel-run-btn" onClick={() => void handleCancel()}>
                  Cancel run
                </button>
              )}
              <button
                type="button"
                className="btn btn-accent"
                data-testid="export-run-btn"
                onClick={() => void handleExport()}
              >
                ⇩ Export run bundle
              </button>
            </div>

            {(exportFeedback || actionFeedback) && (
              <div className="runs-feedback" data-testid="runs-feedback" role="status">
                {exportFeedback}
                {exportFeedback && actionFeedback && ' · '}
                {actionFeedback}
              </div>
            )}

            {dryRun && (
              <div className="dry-banner" data-testid="dry-run-banner">
                fake world — nothing real was touched
              </div>
            )}

            {status?.state === 'awaiting_radio' && (
              <div className="radio-banner" data-testid="awaiting-radio-banner">
                waiting for the radio — the operator holds rig {radioAwaitRig(journal)}
              </div>
            )}

            <div className="gantt" data-testid="gantt">
              <div className="axis">
                {axisTicks(model.t0, model.t1).map((t) => (
                  <span key={t} className="t" style={{ left: `${pct(t, model.t0, model.t1)}%` }}>
                    {formatUtc(t)}
                  </span>
                ))}
              </div>
              {model.lanes.map((lane, li) => (
                <div className="lanerow" key={`${lane.track}-${li}`} data-testid={`lane-${li}`}>
                  <div className="lanelabel">
                    <b>TRACK {li + 1}</b>
                    {lane.track}
                  </div>
                  <div className="track">
                    {lane.bars.map((bar, bi) => {
                      const left = pct(bar.t0, model.t0, model.t1);
                      const width = Math.max(pct(bar.t1, model.t0, model.t1) - left, 0.4);
                      const clickable = bar.kind !== 'delay' || bar.intentEntry !== undefined;
                      return (
                        <div
                          key={`${lane.track}-${bi}-${bar.t0}`}
                          className={`bar ${bar.kind}`}
                          style={{ left: `${left}%`, width: `${width}%`, top: 6 }}
                          data-testid={`bar-${bar.stepId ?? 'delay'}-${bar.kind}-${bi}`}
                          title={bar.action ? `${bar.stepId} ${bar.action}` : bar.stepId}
                          onClick={clickable ? () => setSelectedBar(bar) : undefined}
                        >
                          {bar.stepId ?? formatRunState(bar.parkedState ?? 'waiting')}
                        </div>
                      );
                    })}
                    {model.live && (
                      <div
                        className="nowline"
                        data-testid="nowline"
                        style={{ left: `${pct(model.t1, model.t0, model.t1)}%` }}
                      />
                    )}
                  </div>
                </div>
              ))}
            </div>

            {selectedBar && (
              <div className="stepdetail" data-testid="stepdetail">
                <div className="sd-head">
                  <span className="st">
                    {selectedBar.kind === 'fail'
                      ? '✕ FAILED'
                      : selectedBar.kind === 'ok'
                        ? '✓ OK'
                        : selectedBar.kind === 'running'
                          ? '▶ RUNNING'
                          : selectedBar.kind === 'interrupted'
                            ? '⚡ INTERRUPTED'
                            : `⏸ ${formatRunState(selectedBar.parkedState ?? 'waiting').toUpperCase()}`}
                  </span>
                  <span className="act mono">
                    {selectedBar.stepId} {selectedBar.action}
                  </span>
                  <span className="right mono" data-testid="stepdetail-path">
                    journal: {selectedRunId}.jsonl
                  </span>
                </div>
                <div className="sd-body">
                  {selectedBar.intentEntry && selectedBar.intentEntry.event.type === 'step_intent' && (
                    <div data-testid="stepdetail-resolved">
                      {formatUtc(selectedBar.intentEntry.ts_unix)} intent{' '}
                      {JSON.stringify(selectedBar.intentEntry.event.resolved_params)}
                    </div>
                  )}
                  {selectedBar.resultEntry && selectedBar.resultEntry.event.type === 'step_err' && (
                    <div className="err" data-testid="stepdetail-cause">
                      {formatUtc(selectedBar.resultEntry.ts_unix)} err{' '}
                      {formatStepErrorCause(selectedBar.resultEntry.event.error)}
                    </div>
                  )}
                  {selectedBar.resultEntry && selectedBar.resultEntry.event.type === 'step_ok' && (
                    <div data-testid="stepdetail-output">
                      {formatUtc(selectedBar.resultEntry.ts_unix)} ok{' '}
                      {JSON.stringify(selectedBar.resultEntry.event.output)}
                    </div>
                  )}
                  {!selectedBar.resultEntry && selectedBar.kind === 'running' && (
                    <div className="dim" data-testid="stepdetail-running">
                      started {formatUtc(selectedBar.t0)} — still running, no result journaled yet
                    </div>
                  )}
                  {!selectedBar.resultEntry && selectedBar.kind === 'interrupted' && (
                    <div className="dim" data-testid="stepdetail-interrupted">
                      started {formatUtc(selectedBar.t0)} — never closed: the run ended before this
                      step journaled a result
                    </div>
                  )}
                  {!selectedBar.resultEntry &&
                    selectedBar.kind !== 'running' &&
                    selectedBar.kind !== 'interrupted' && (
                      <div className="dim" data-testid="stepdetail-parked">
                        parked {formatUtc(selectedBar.t0)}
                        {model.live ? ' — still parked' : ` — ${formatUtc(selectedBar.t1)}`}
                      </div>
                    )}
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
