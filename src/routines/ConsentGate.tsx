/**
 * ConsentGate — the Part 97 transmit-consent moment (routines plan-5 Task 14,
 * spec §12, flow 4). "Consent cannot hide": an attended-transmit-mode routine
 * that reaches a transmitting step parks the run (`awaiting_consent`) instead
 * of keying the radio, and the operator MUST see it — from cold launch, from
 * any surface, without hunting for it.
 *
 * Mounted ALWAYS at AppShell level (like `CloseBehaviorPrompt`) — self-
 * managing, no parent state. It tracks every parked run via `useParkedRuns`
 * (exported below) and renders a modal for the OLDEST one; a multi-park
 * queue shows a "1 of N" pip rather than stacking modals. The menubar/
 * statusbar badges are chrome AppShell owns, so ConsentGate reports its
 * parked list upward through `onParkedChange` (task-14 brief: "self-managing
 * … exports `useParkedRuns()` if the badge is rendered by MenuBar via AppShell
 * prop" — this is that wiring: ONE hook instance, here, and AppShell mirrors
 * its output into `<MenuBar badges>` + `<StatusBar consent>` rather than
 * re-subscribing a second instance).
 *
 * The engine's ConsentPort has exactly two outcomes — grant or teardown-as-
 * cancelled — so the footer is Confirm transmit / Cancel run. There is
 * deliberately NO Skip button; the mock this was transplanted from
 * (dev/scratch/routines-ui-mocks/consent-dialog.html) shows one, but the
 * decision (task-14 brief binding constraint 1) drops it — the backend has no
 * skip-this-step outcome to call.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  runStatus,
  runJournal,
  listRuns,
  grantConsent,
  cancelRun,
  type RunListEntry,
} from './routinesApi';
import { listenRoutinesEvents } from './routinesEvents';
import { formatParkedDuration } from './format';
import './ConsentGate.css';

/** A single run parked awaiting operator transmit consent. */
export interface ParkedRun {
  runId: string;
  stepId: string;
  routine: string;
  /** `Date.now()` at the moment THIS UI instance learned of the park — used
   *  only for the live "parked HH:MM:SS" readout, not persisted anywhere. */
  parkedAtMs: number;
}

export interface UseParkedRunsResult {
  /** Oldest-first — the modal always shows `parked[0]`. */
  parked: ParkedRun[];
  /** Grants consent for the named run/step. `false` means the park vanished
   *  (the run moved on, or was resolved from elsewhere) between the operator
   *  opening the modal and clicking Confirm — the stale entry is removed
   *  either way, matching the brief's "refresh state and close". */
  confirm(runId: string, stepId: string): Promise<boolean>;
  /** Cancels the run outright; the engine journals it as operator-cancelled. */
  cancelParked(runId: string): Promise<void>;
}

/** Reconciliation poll: catches a park clearing WITHOUT a `runFinished` event
 *  reaching this instance (task-14 brief binding constraint 2's third removal
 *  path — "when a poll shows the run left awaiting_consent"). */
const CONSENT_POLL_MS = 5000;

function sortByParkedAt(list: ParkedRun[]): ParkedRun[] {
  return [...list].sort((a, b) => a.parkedAtMs - b.parkedAtMs);
}

/**
 * Launch recovery (spec §12, "consent cannot hide"): a run already sitting in
 * `awaiting_consent` when this hook mounts (parked at 03:00, operator opens
 * the app at 08:00) has no fresh `awaitingConsent` event to park it from — the
 * event fired hours ago, to nobody. Its journal's last `step_intent` names the
 * step that hasn't reached `step_ok`/`step_err`: the engine parks BEFORE
 * executing the transmit, so no completion entry ever follows the parked
 * step's intent while the run sits in this state. Returns `null` if no
 * `step_intent` is found (defensive — the modal simply won't recover that run
 * rather than throwing).
 */
async function recoverParkedStepId(runId: string): Promise<string | null> {
  const entries = await runJournal(runId);
  for (let i = entries.length - 1; i >= 0; i--) {
    const ev = entries[i].event;
    if (ev.type === 'step_intent') return ev.step;
  }
  return null;
}

export function useParkedRuns(): UseParkedRunsResult {
  const [parked, setParked] = useState<ParkedRun[]>([]);
  const mountedRef = useRef(true);
  const parkedRef = useRef<ParkedRun[]>([]);
  useEffect(() => {
    parkedRef.current = parked;
  }, [parked]);

  const removeRun = useCallback((runId: string) => {
    setParked((cur) => cur.filter((p) => p.runId !== runId));
  }, []);

  const addParked = useCallback((runId: string, stepId: string) => {
    void (async () => {
      if (parkedRef.current.some((p) => p.runId === runId)) return;
      try {
        const status = await runStatus(runId);
        if (!mountedRef.current || !status) return;
        setParked((cur) => {
          if (cur.some((p) => p.runId === runId)) return cur;
          return sortByParkedAt([
            ...cur,
            { runId, stepId, routine: status.routine, parkedAtMs: Date.now() },
          ]);
        });
      } catch {
        // Best-effort — no Tauri runtime, or the run vanished before
        // runStatus resolved. Nothing to park.
      }
    })();
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    let disposed = false;
    let unlisten: (() => void) | null = null;

    // Launch recovery runs once, at mount, independent of any event.
    void (async () => {
      try {
        const runs: RunListEntry[] = await listRuns(undefined);
        const live = runs.filter((r) => r.state === 'awaiting_consent');
        for (const r of live) {
          if (disposed) return;
          try {
            const stepId = await recoverParkedStepId(r.runId);
            if (disposed || !mountedRef.current || !stepId) continue;
            setParked((cur) => {
              if (cur.some((p) => p.runId === r.runId)) return cur;
              return sortByParkedAt([
                ...cur,
                { runId: r.runId, stepId, routine: r.routine, parkedAtMs: Date.now() },
              ]);
            });
          } catch {
            // This run's journal read failed — skip it; other parked runs
            // still recover independently.
          }
        }
      } catch {
        // No Tauri runtime (test/dev harness) or listRuns failed — nothing
        // to recover; live awaitingConsent events still park normally.
      }
    })();

    listenRoutinesEvents((event) => {
      if (!mountedRef.current) return;
      switch (event.kind) {
        case 'awaitingConsent':
          addParked(event.runId, event.stepId);
          break;
        case 'runFinished':
          removeRun(event.runId);
          break;
        default:
          break; // not this gate's concern
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
      mountedRef.current = false;
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [addParked, removeRun]);

  // Periodic reconciliation while anything is parked. Reads current parked
  // runs off a ref (not the `parked` state itself) so a change in WHO is
  // parked doesn't reset the interval's cadence — only the has-any/has-none
  // transition does.
  const hasParked = parked.length > 0;
  useEffect(() => {
    if (!hasParked) return;
    const id = setInterval(() => {
      void (async () => {
        for (const p of parkedRef.current) {
          try {
            const status = await runStatus(p.runId);
            if (!mountedRef.current) return;
            if (!status || status.state !== 'awaiting_consent') removeRun(p.runId);
          } catch {
            // Transient read failure — leave the entry parked, retry next tick.
          }
        }
      })();
    }, CONSENT_POLL_MS);
    return () => clearInterval(id);
  }, [hasParked, removeRun]);

  const confirm = useCallback(
    async (runId: string, stepId: string): Promise<boolean> => {
      const granted = await grantConsent(runId, stepId);
      // Either outcome removes the tracked entry: a true grant means the step
      // is proceeding (no longer parked); a false grant means the park
      // vanished out from under us — the brief's "refresh state and close".
      removeRun(runId);
      return granted;
    },
    [removeRun],
  );

  const cancelParked = useCallback(
    async (runId: string): Promise<void> => {
      await cancelRun(runId);
      removeRun(runId);
    },
    [removeRun],
  );

  return { parked, confirm, cancelParked };
}

export interface ConsentGateProps {
  /** Fires with the current parked list (oldest-first) whenever it changes,
   *  so AppShell can mirror the count/oldest-routine into the MenuBar badge
   *  and StatusBar item without ConsentGate depending on any parent state. */
  onParkedChange?: (parked: ParkedRun[]) => void;
}

interface StepIntentInfo {
  action: string;
  resolvedParams: unknown;
}

export function ConsentGate({ onParkedChange }: ConsentGateProps) {
  const { parked, confirm, cancelParked } = useParkedRuns();
  const [stepIntent, setStepIntent] = useState<StepIntentInfo | null>(null);
  const [busy, setBusy] = useState(false);
  // Forces a re-render every second so the "Parked HH:MM:SS" readout ticks
  // without needing to store the formatted string in state.
  const [, setTick] = useState(0);

  const onParkedChangeRef = useRef(onParkedChange);
  onParkedChangeRef.current = onParkedChange;
  useEffect(() => {
    onParkedChangeRef.current?.(parked);
  }, [parked]);

  const oldest = parked[0] ?? null;

  // Resolve the parked step's action + resolved params VERBATIM from the
  // journal's `step_intent` entry — never invent a message-staging readout
  // the backend doesn't expose (task-14 brief binding constraint 4).
  useEffect(() => {
    if (!oldest) {
      setStepIntent(null);
      return;
    }
    let cancelled = false;
    void runJournal(oldest.runId)
      .then((entries) => {
        if (cancelled) return;
        for (let i = entries.length - 1; i >= 0; i--) {
          const ev = entries[i].event;
          if (ev.type === 'step_intent' && ev.step === oldest.stepId) {
            setStepIntent({ action: ev.action, resolvedParams: ev.resolved_params });
            return;
          }
        }
        setStepIntent(null);
      })
      .catch(() => {
        if (!cancelled) setStepIntent(null);
      });
    return () => {
      cancelled = true;
    };
  }, [oldest?.runId, oldest?.stepId]);

  useEffect(() => {
    if (!oldest) return;
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, [oldest?.runId]);

  if (!oldest) return null;

  const total = parked.length;

  // `.catch(() => {})` guards against an unhandled rejection if the backend
  // call itself throws (as opposed to `confirm`'s `false` return, which is
  // the modeled "park vanished" outcome, not an error) — a transient IPC
  // failure should re-enable the buttons, not crash the modal or the test
  // harness with an unhandled promise rejection.
  const onConfirm = () => {
    if (busy) return;
    setBusy(true);
    void confirm(oldest.runId, oldest.stepId)
      .catch(() => {})
      .finally(() => setBusy(false));
  };
  const onCancel = () => {
    if (busy) return;
    setBusy(true);
    void cancelParked(oldest.runId)
      .catch(() => {})
      .finally(() => setBusy(false));
  };

  return (
    <div className="tux-consent-overlay" data-testid="consent-gate-overlay">
      <div
        className="tux-consent-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="tux-consent-title"
        data-testid="consent-gate-modal"
      >
        <div className="tux-consent-head">
          <span className="tux-consent-warnicon" aria-hidden="true">
            ⚠
          </span>
          <span className="tux-consent-title" id="tux-consent-title">
            Transmit consent — attended routine
          </span>
          <span className="tux-consent-sub">Part 97 §97.109 · you are the control operator</span>
        </div>
        <div className="tux-consent-body">
          <div className="tux-consent-row">
            <span className="tux-consent-k">Routine</span>
            <span className="tux-consent-v">
              <span className="tux-consent-big" data-testid="consent-gate-routine">
                {oldest.routine}
              </span>
              <br />
              <span className="tux-consent-dim" data-testid="consent-gate-run-step">
                run {oldest.runId} · step {oldest.stepId}
                {stepIntent ? ` — ${stepIntent.action}` : ''}
              </span>
            </span>
          </div>
          <div className="tux-consent-row">
            <span className="tux-consent-k">Transmit step</span>
            <span className="tux-consent-v">
              <div className="tux-consent-txbox" data-testid="consent-gate-txbox">
                <span className="tux-consent-txbox-h">RESOLVED PARAMS</span>
                <br />
                {stepIntent ? JSON.stringify(stepIntent.resolvedParams, null, 2) : '—'}
              </div>
            </span>
          </div>
          <div className="tux-consent-row">
            <span className="tux-consent-k">Parked</span>
            <span className="tux-consent-v tux-consent-parked" data-testid="consent-gate-parked">
              {formatParkedDuration(Date.now() - oldest.parkedAtMs)}
            </span>
          </div>
          {total > 1 && (
            <div className="tux-consent-pip" data-testid="consent-gate-pip">
              1 of {total}
            </div>
          )}
          <div className="tux-consent-p97">
            Confirm keys the radio now. Cancel ends run {oldest.runId} as cancelled
            (journaled: cancelled by operator).
          </div>
        </div>
        <div className="tux-consent-foot">
          <button
            type="button"
            className="tux-consent-btn-confirm"
            data-testid="consent-gate-confirm"
            disabled={busy}
            onClick={onConfirm}
          >
            Confirm transmit
          </button>
          <button
            type="button"
            className="tux-consent-btn-cancelrun"
            data-testid="consent-gate-cancel"
            disabled={busy}
            onClick={onCancel}
          >
            Cancel run
          </button>
        </div>
      </div>
    </div>
  );
}
