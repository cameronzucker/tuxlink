/**
 * RoutinesDashboard — the fleet-ops table (routines plan-5 Task 8, spec §12,
 * flows 1/3/6/7).
 *
 * Replaces the Task 7 compile stub. Layout is the approved mock verbatim
 * (dev/scratch/routines-ui-mocks/dashboard.html): a 7-column ops table
 * (Routine / Status / Trigger / Last result / Next fire / TX mode / row
 * actions) plus a fleet-check strip above the app statusbar. "↗ Pop out" is
 * omitted outright (Global Constraint 6) — never a disabled stub.
 *
 * ONE read path (`useRoutines`): every registry/scheduler value the table
 * needs — summaries, schedule status, next fires, per-routine findings,
 * per-routine `RoutineDef`s, the action catalog, and fleet findings — comes
 * from that hook. This component owns exactly one thing `useRoutines`
 * doesn't: live/terminal RUN state (`listRuns`), because the brief's
 * "Consumes" list scopes `listRuns`/`runStatus` to this component directly,
 * not the shared hook. Runs are re-fetched on mount and (debounced, mirroring
 * `useRoutines`'s own 150ms coalescing) on every run-progress routines event
 * this component's status chips and Last-result column care about.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  runRoutine,
  cancelRun,
  deleteRoutine,
  setEnabled,
  listRuns,
  runJournal,
  type RunListEntry,
  type RunState,
  type Finding,
  type RoutineDef,
  type RoutineSummary,
  type ActionInfo,
} from './routinesApi';
import { useRoutines } from './useRoutines';
import { listenRoutinesEvents } from './routinesEvents';
import {
  formatMissedCount,
  formatUtc,
  formatTrigger,
  formatIfMissed,
  formatRunState,
  formatStepErrorCause,
  formatUiError,
} from './format';
import { ImportJsonDialog } from './ImportJsonDialog';
import type { DesignerTab } from './RoutinesSurface';
import './RoutinesDashboard.css';

export interface RoutinesDashboardProps {
  onOpenDesigner: (routine: string, tab?: DesignerTab) => void;
  onNewRoutine: () => void;
  /** When provided, the dashboard header shows a text-labeled "↗ Pop out"
   *  affordance (tuxlink-dmwte task 8, spec §5) that pops the Routines surface
   *  to its own window. Absent inside the popped window itself. */
  onPopOut?: () => void;
  /** Return to the mailbox (tuxlink-9se1x). Provided by the inline host only;
   *  absent in the popped window, where there is no mailbox pane to return
   *  to. Renders the "← Mailbox" button, mirroring the designer's
   *  "← Routines" idiom. */
  onClose?: () => void;
}

/** Non-terminal `RunState`s — a routine with one of these is "live". */
const LIVE_STATES = new Set<RunState>([
  'pending',
  'running',
  'waiting',
  'awaiting_consent',
  'awaiting_radio',
]);

/** Coalesce window for run-progress-event-triggered `listRuns` re-fetches,
 *  mirroring `useRoutines.ts`'s `REFRESH_DEBOUNCE_MS`. */
const RUNS_REFRESH_DEBOUNCE_MS = 150;

function newestOf(entries: RunListEntry[]): RunListEntry | undefined {
  return entries.reduce<RunListEntry | undefined>(
    (best, e) => (!best || e.startedUnix > best.startedUnix ? e : best),
    undefined,
  );
}

/** Status chip precedence (task-8 brief): `awaiting consent` > `running` >
 *  `draft · N errors` > `enabled` > `disabled`. */
function statusChipFor(
  summary: RoutineSummary,
  findings: Finding[],
  liveRun: RunListEntry | undefined,
): { cls: string; text: string } {
  if (liveRun?.state === 'awaiting_consent') return { cls: 'consent', text: 'awaiting consent' };
  if (liveRun) return { cls: 'running', text: 'running' };
  const errCount = findings.filter((f) => f.severity === 'error').length;
  if (errCount > 0) {
    return { cls: 'draft', text: `draft · ${errCount} error${errCount === 1 ? '' : 's'}` };
  }
  if (summary.enabled) return { cls: 'enabled', text: 'enabled' };
  return { cls: 'disabled', text: 'disabled' };
}

/** TX-mode column: `automatic` / `attended` / `—` (no step in the routine's
 *  own tracks ever transmits, per the action catalog's `transmits` flag — no
 *  hardcoded action names) / `auto·no-ack` (automatic + `AUTO_TX_UNACKED`). */
function txModeFor(
  summary: RoutineSummary,
  def: RoutineDef | undefined,
  actionsByName: Record<string, ActionInfo>,
  findings: Finding[],
): { cls: string; text: string } {
  const steps = def ? def.tracks.flatMap((t) => t.steps) : [];
  const transmitsAny = steps.some((step) => 'action' in step && actionsByName[step.action]?.transmits);
  if (!transmitsAny) return { cls: '', text: '—' };
  if (summary.transmitMode === 'attended') return { cls: 'attended', text: 'attended' };
  const hasAutoTxUnacked = findings.some((f) => f.code === 'AUTO_TX_UNACKED');
  return hasAutoTxUnacked ? { cls: 'noack', text: 'auto·no-ack' } : { cls: 'auto', text: 'automatic' };
}

function fleetTagText(findings: Finding[]): string {
  const errors = findings.filter((f) => f.severity === 'error').length;
  const warnings = findings.filter((f) => f.severity === 'warning').length;
  const parts: string[] = [];
  if (errors > 0) parts.push(`${errors} ERROR${errors === 1 ? '' : 'S'}`);
  if (warnings > 0) parts.push(`${warnings} WARNING${warnings === 1 ? '' : 'S'}`);
  return `FLEET CHECK · ${parts.join(', ')}`;
}

export function RoutinesDashboard({ onOpenDesigner, onNewRoutine, onPopOut, onClose }: RoutinesDashboardProps) {
  const {
    summaries,
    scheduleStatus,
    nextFires,
    findingsByRoutine,
    fleetFindings,
    defsByRoutine,
    actionsByName,
    loaded,
    refresh,
  } = useRoutines();

  const [runs, setRuns] = useState<RunListEntry[]>([]);
  const [runRefusal, setRunRefusal] = useState<string | null>(null);
  const [enableBlocked, setEnableBlocked] = useState<Finding[] | null>(null);
  const [openMenuFor, setOpenMenuFor] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [failureCauseByRunId, setFailureCauseByRunId] = useState<Record<string, string>>({});

  const mountedRef = useRef(true);
  const inFlightJournalRef = useRef<Set<string>>(new Set());

  const loadRuns = useCallback(async () => {
    try {
      const result = await listRuns();
      if (mountedRef.current) setRuns(result);
    } catch {
      // No Tauri runtime (test/dev harness) or the command failed — leave the
      // last-known runs in place.
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    let debounceHandle: ReturnType<typeof setTimeout> | null = null;

    const scheduleLoadRuns = () => {
      if (debounceHandle) clearTimeout(debounceHandle);
      debounceHandle = setTimeout(() => {
        debounceHandle = null;
        void loadRuns();
      }, RUNS_REFRESH_DEBOUNCE_MS);
    };

    listenRoutinesEvents((event) => {
      if (!mountedRef.current) return;
      switch (event.kind) {
        case 'runStarted':
        case 'stateChanged':
        case 'stepCompleted':
        case 'awaitingConsent':
        case 'runFinished':
        case 'scheduledFire':
        case 'scheduleSkipped':
        case 'scheduleRefused':
          scheduleLoadRuns();
          break;
        default:
          break; // libraryChanged/missedFires: useRoutines's own concern.
      }
    })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {
        // No Tauri runtime in some tests/dev harnesses.
      });

    void loadRuns();

    return () => {
      mountedRef.current = false;
      disposed = true;
      if (debounceHandle) clearTimeout(debounceHandle);
      if (unlisten) unlisten();
    };
    // Mount-once subscription; `loadRuns` is stable (useCallback([])).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loadRuns]);

  // Fetch the step_err cause for the newest FAILED terminal run of each
  // routine, on demand, cached by runId (task-8 brief binding constraint 6:
  // "fetch on demand for the newest failed run only; cache").
  useEffect(() => {
    for (const s of summaries) {
      const terminal = runs.filter((r) => r.routine === s.routine && !LIVE_STATES.has(r.state));
      const newest = newestOf(terminal);
      if (
        newest &&
        newest.state === 'failed' &&
        !(newest.runId in failureCauseByRunId) &&
        !inFlightJournalRef.current.has(newest.runId)
      ) {
        const runId = newest.runId;
        inFlightJournalRef.current.add(runId);
        runJournal(runId)
          .then((entries) => {
            let cause = '';
            for (const entry of entries) {
              if (entry.event.type === 'step_err') cause = formatStepErrorCause(entry.event.error);
            }
            if (mountedRef.current) setFailureCauseByRunId((prev) => ({ ...prev, [runId]: cause }));
          })
          .catch(() => {
            if (mountedRef.current) setFailureCauseByRunId((prev) => ({ ...prev, [runId]: '' }));
          })
          .finally(() => inFlightJournalRef.current.delete(runId));
      }
    }
  }, [summaries, runs, failureCauseByRunId]);

  const handleRun = useCallback(
    async (routine: string) => {
      try {
        await runRoutine(routine, {});
        setRunRefusal(null);
        void loadRuns();
      } catch (err) {
        setRunRefusal(formatUiError(err));
      }
    },
    [loadRuns],
  );

  const handleStop = useCallback(
    async (runId: string) => {
      try {
        await cancelRun(runId);
      } finally {
        void loadRuns();
      }
    },
    [loadRuns],
  );

  const handleToggleEnabled = useCallback(
    async (routine: string, enabled: boolean) => {
      setOpenMenuFor(null);
      try {
        const result = await setEnabled(routine, !enabled);
        if (result.blocked) {
          setEnableBlocked(result.findings);
        } else {
          setEnableBlocked(null);
          void refresh();
        }
      } catch (err) {
        setRunRefusal(formatUiError(err));
      }
    },
    [refresh],
  );

  const handleDeleteConfirmed = useCallback(
    async (routine: string) => {
      setPendingDelete(null);
      setOpenMenuFor(null);
      try {
        await deleteRoutine(routine);
        void refresh();
      } catch (err) {
        setRunRefusal(formatUiError(err));
      }
    },
    [refresh],
  );

  return (
    <div className="surface" data-testid="routines-dashboard">
      <div className="surface-head">
        {onClose && (
          <button
            type="button"
            className="back"
            data-testid="routines-dashboard-close"
            title="Back to the mailbox (Esc)"
            onClick={onClose}
          >
            ← Mailbox
          </button>
        )}
        <span className="surface-title">Routines</span>
        <span className="surface-sub">
          {summaries.length} routine{summaries.length === 1 ? '' : 's'}
        </span>
        <span className="head-actions">
          {onPopOut && (
            <button
              type="button"
              className="btn btn-ghost"
              data-testid="routines-dashboard-popout"
              title="Open Routines in its own window"
              onClick={onPopOut}
            >
              ↗ Pop out
            </button>
          )}
          {/* item 10 (bd tuxlink-iizmk): the operator imports a ROUTINE; the
              file happening to be JSON is an implementation detail. */}
          <button type="button" className="btn btn-ghost" onClick={() => setImportOpen(true)}>
            Import routine…
          </button>
          <button type="button" className="btn btn-accent" onClick={onNewRoutine}>
            ＋ New Routine
          </button>
        </span>
      </div>

      {runRefusal && (
        <div className="refusal-strip" role="alert">
          <span>{runRefusal}</span>
          <button
            type="button"
            className="dismiss"
            aria-label="Dismiss refusal"
            onClick={() => setRunRefusal(null)}
          >
            ×
          </button>
        </div>
      )}

      {/* Calm empty state, MessageList's EMPTY_FOLDER_COPY idiom: a fresh
          install opening Routines rendered a bare void under the column
          headers (tuxlink-3awm9 WebKitGTK smoke) — say what this surface is
          and point at the two ways in. Gated on `loaded` (Codex P2): until
          the first refresh settles, summaries is only its initial [] and
          "No routines yet" would be a false statement to an operator whose
          library is still loading — render the bare table shell meanwhile. */}
      {loaded && summaries.length === 0 ? (
        <div className="ops-empty" data-testid="routines-dashboard-empty">
          No routines yet. Click ＋ New Routine to build one on the canvas, or Import routine… to
          load a shared definition.
        </div>
      ) : (
      <div className="ops-wrap">
        <table className="ops">
          <thead>
            <tr>
              <th style={{ width: '20%' }}>Routine</th>
              <th style={{ width: '18%' }}>Status</th>
              <th style={{ width: '17%' }}>Trigger</th>
              <th style={{ width: '19%' }}>Last result</th>
              <th style={{ width: '8%' }}>Next fire</th>
              <th style={{ width: '8%' }}>TX mode</th>
              {/* Controls column is FIXED-px: at 1024px wide, its former 5%
                  (51px) flex-crushed the run/stop button to a sliver beside
                  the ⋯ menu (tuxlink-3awm9 WebKitGTK smoke) — controls don't
                  scale with viewport. Slack taken from Last result (24→19%). */}
              <th style={{ width: 72 }} />
            </tr>
          </thead>
          <tbody>
            {summaries.map((s) => {
              const findings = findingsByRoutine[s.routine] ?? [];
              const status = scheduleStatus.find((ss) => ss.routine === s.routine);
              const def = defsByRoutine[s.routine];
              const routineRuns = runs.filter((r) => r.routine === s.routine);
              const liveRun = newestOf(routineRuns.filter((r) => LIVE_STATES.has(r.state)));
              const newestTerminal = newestOf(routineRuns.filter((r) => !LIVE_STATES.has(r.state)));
              const chip = statusChipFor(s, findings, liveRun);
              const tx = txModeFor(s, def, actionsByName, findings);
              const trigger = s.triggers[0];
              const nextFireAt = nextFires[s.routine];
              const trackCount = def?.tracks.length ?? 0;
              const stepCount = def ? def.tracks.reduce((n, t) => n + t.steps.length, 0) : 0;
              const menuOpen = openMenuFor === s.routine;

              return (
                <tr
                  key={s.routine}
                  onDoubleClick={() =>
                    // Final whole-branch review, Fix 3: flow 3 "investigate a
                    // failed run" lands directly on the Runs tab when the
                    // row's own last-result column already reads FAILED — no
                    // reason to make the operator re-navigate from Design.
                    // Every other row keeps the prior default (no tab arg —
                    // RoutinesSurface's own default).
                    onOpenDesigner(s.routine, newestTerminal?.state === 'failed' ? 'runs' : undefined)
                  }
                >
                  <td>
                    <div className="rname">{s.routine}</div>
                    {def && (
                      <div className="rmeta">
                        {trackCount} track{trackCount === 1 ? '' : 's'} · {stepCount} step
                        {stepCount === 1 ? '' : 's'}
                      </div>
                    )}
                  </td>
                  <td>
                    <div className="statuscell">
                      <span className={`chip ${chip.cls}`}>
                        <span className="d" />
                        {chip.text}
                      </span>
                      {status && status.missed > 0 && (
                        <span className="badge-miss">
                          ⚠ missed {formatMissedCount(status.missed)} fire(s)
                        </span>
                      )}
                      {status?.lastRefusal && (
                        <div className="refusal-note">
                          last fire refused: {status.lastRefusal.reason}
                        </div>
                      )}
                      {status?.lastSkip && (
                        <div className="skip-note">last fire skipped: {status.lastSkip.reason}</div>
                      )}
                    </div>
                  </td>
                  <td>
                    <div className="trig">
                      {trigger ? formatTrigger(trigger) : '—'}
                      {trigger?.type === 'schedule' && trigger.if_missed && (
                        <div className="win">{formatIfMissed(trigger.if_missed)}</div>
                      )}
                    </div>
                  </td>
                  <td>
                    {!newestTerminal ? (
                      <div className="res">
                        <span className="t">never run</span>
                      </div>
                    ) : newestTerminal.state === 'completed' ? (
                      <div className="res">
                        <span className="ok">✓ ok</span>{' '}
                        <span className="t">
                          {formatUtc(newestTerminal.finishedUnix ?? newestTerminal.startedUnix)}
                        </span>
                      </div>
                    ) : newestTerminal.state === 'failed' ? (
                      <div className="res">
                        <span className="fail">✕ failed</span>{' '}
                        <span className="t">
                          {formatUtc(newestTerminal.finishedUnix ?? newestTerminal.startedUnix)}
                        </span>
                        {failureCauseByRunId[newestTerminal.runId] && (
                          <div className="detail">{failureCauseByRunId[newestTerminal.runId]}</div>
                        )}
                      </div>
                    ) : (
                      <div className="res">
                        <span className="t">{formatRunState(newestTerminal.state)}</span>
                      </div>
                    )}
                  </td>
                  <td>
                    <div className="nextfire">
                      {nextFireAt !== undefined ? formatUtc(nextFireAt) : <span className="none">—</span>}
                    </div>
                  </td>
                  <td>
                    <span className={`txmode ${tx.cls}`}>{tx.text}</span>
                  </td>
                  {/* item 8 (bd tuxlink-iizmk): `actcell` lifts `.ops td`'s
                      overflow:hidden so the row menu (absolutely positioned
                      inside `.rowact` below) isn't clipped by its own cell. */}
                  <td className="actcell">
                    <div className="rowact">
                      {liveRun ? (
                        <button
                          type="button"
                          className="ibtn stop"
                          aria-label={`Stop ${s.routine}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleStop(liveRun.runId);
                          }}
                        >
                          ■
                        </button>
                      ) : chip.cls === 'draft' ? (
                        <button
                          type="button"
                          className="ibtn"
                          aria-label={`Edit ${s.routine}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            onOpenDesigner(s.routine);
                          }}
                        >
                          ✎
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="ibtn run"
                          aria-label={`Run ${s.routine}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleRun(s.routine);
                          }}
                        >
                          ▶
                        </button>
                      )}
                      <button
                        type="button"
                        className="ibtn"
                        aria-label={`Actions for ${s.routine}`}
                        onClick={(e) => {
                          e.stopPropagation();
                          setPendingDelete(null);
                          setOpenMenuFor(menuOpen ? null : s.routine);
                        }}
                      >
                        ⋯
                      </button>
                      {/* item 8 (bd tuxlink-iizmk): the menu MUST live inside
                          `.rowact` — the row's only position:relative anchor.
                          As a td-level sibling its position:absolute resolved
                          against the app window itself, painting the menu at
                          the window's top-right corner underneath the chrome
                          ("opens behind the main window"). */}
                      {menuOpen && (
                        <div className="rowmenu" role="menu">
                          <button
                            type="button"
                            role="menuitem"
                            onClick={() => {
                              setOpenMenuFor(null);
                              onOpenDesigner(s.routine);
                            }}
                          >
                            Edit
                          </button>
                          <button
                            type="button"
                            role="menuitem"
                            onClick={() => void handleToggleEnabled(s.routine, s.enabled)}
                          >
                            {s.enabled ? 'Disable' : 'Enable'}
                          </button>
                          {pendingDelete === s.routine ? (
                            <button
                              type="button"
                              role="menuitem"
                              className="danger"
                              onClick={() => void handleDeleteConfirmed(s.routine)}
                            >
                              Confirm delete
                            </button>
                          ) : (
                            <button type="button" role="menuitem" onClick={() => setPendingDelete(s.routine)}>
                              Delete
                            </button>
                          )}
                        </div>
                      )}
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      )}

      {enableBlocked && enableBlocked.length > 0 && (
        <div className="fleet-bar err">
          <span className="tag">ENABLE BLOCKED</span>
          <span className="msgs">
            {enableBlocked.map((f, i) => (
              <span key={`${f.code}-${i}`}>
                <span className="code">{f.code}</span> — {f.message}
              </span>
            ))}
          </span>
          <button
            type="button"
            className="dismiss"
            aria-label="Dismiss enable-blocked findings"
            onClick={() => setEnableBlocked(null)}
          >
            ×
          </button>
        </div>
      )}

      {fleetFindings.length > 0 && (
        <div className="fleet-bar">
          <span className="tag">{fleetTagText(fleetFindings)}</span>
          <span className="msgs">
            {fleetFindings.map((f, i) => (
              <span key={`${f.code}-${i}`}>
                <span className="code">{f.code}</span> — {f.message}
              </span>
            ))}
          </span>
        </div>
      )}

      {importOpen && (
        <ImportJsonDialog onClose={() => setImportOpen(false)} onSaved={() => void refresh()} />
      )}
    </div>
  );
}
