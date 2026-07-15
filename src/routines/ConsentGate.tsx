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
  /** `''` (UNKNOWN_STEP_ID) when launch recovery found the run parked but its
   *  journal carried no `step_intent` to name the step — the park still
   *  surfaces (spec §12: cannot hide), with Confirm disabled (granting
   *  consent needs a real stepId) and Cancel run available. */
  stepId: string;
  routine: string;
  /** `Date.now()` at the moment THIS UI instance learned of the park — used
   *  only for the live "parked HH:MM:SS" readout, not persisted anywhere. */
  parkedAtMs: number;
}

/** Sentinel for a launch-recovered park whose journal named no step. */
export const UNKNOWN_STEP_ID = '';

export interface UseParkedRunsResult {
  /** Oldest-first — the modal always shows `parked[0]`. Entries are keyed by
   *  the `(runId, stepId)` PAIR, not runId alone (Codex adrev P1): an
   *  attended routine with two transmitting steps can emit step 2's
   *  `awaitingConsent` BEFORE the async grant path finishes removing step
   *  1's entry — runId-only keying dropped that second park entirely (the
   *  add-dedupe early-returned on the still-present runId, then the grant's
   *  removal deleted the entry, leaving the backend parked with no modal, no
   *  badge, and nothing to re-add until app restart). */
  parked: ParkedRun[];
  /** Grants consent for the named run/step. `false` means the park vanished
   *  (the run moved on, or was resolved from elsewhere) between the operator
   *  opening the modal and clicking Confirm — the stale entry is removed
   *  either way, matching the brief's "refresh state and close". Removes
   *  ONLY the `(runId, stepId)` pair — a newer park of the same run
   *  survives. */
  confirm(runId: string, stepId: string): Promise<boolean>;
  /** Cancels the run outright; the engine journals it as operator-cancelled.
   *  Removes every parked entry for the runId — the whole run is gone. */
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
 * step's intent while the run sits in this state. Returns `UNKNOWN_STEP_ID`
 * (`''`) if no `step_intent` is found — the run is still parked and MUST
 * still surface (spec §12: a cannot-hide surface never silently drops a live
 * park); the modal renders the step as unknown, disables Confirm (granting
 * needs a real stepId), and leaves Cancel run available.
 */
async function recoverParkedStepId(runId: string): Promise<string> {
  const entries = await runJournal(runId);
  for (let i = entries.length - 1; i >= 0; i--) {
    const ev = entries[i].event;
    if (ev.type === 'step_intent') return ev.step;
  }
  return UNKNOWN_STEP_ID;
}

export function useParkedRuns(): UseParkedRunsResult {
  const [parked, setParked] = useState<ParkedRun[]>([]);
  const mountedRef = useRef(true);
  const parkedRef = useRef<ParkedRun[]>([]);
  useEffect(() => {
    parkedRef.current = parked;
  }, [parked]);

  /** Removes EVERY parked entry for the runId — the run-terminal paths
   *  (`runFinished`, cancel, the poll's explicit non-awaiting_consent read):
   *  when the whole run is gone, no step of it can still be parked. */
  const removeRun = useCallback((runId: string) => {
    setParked((cur) => cur.filter((p) => p.runId !== runId));
  }, []);

  /** Removes ONLY the `(runId, stepId)` pair — the grant path: consenting to
   *  step 1 must not delete a step-2 park that raced in while the grant was
   *  in flight (Codex adrev P1). */
  const removePair = useCallback((runId: string, stepId: string) => {
    setParked((cur) => cur.filter((p) => !(p.runId === runId && p.stepId === stepId)));
  }, []);

  const addParked = useCallback((runId: string, stepId: string) => {
    void (async () => {
      // Pair-keyed dedupe (Codex adrev P1): a second transmitting step of the
      // SAME run must insert even while the first step's entry still exists
      // (its removal by the async grant path may not have settled yet).
      if (parkedRef.current.some((p) => p.runId === runId && p.stepId === stepId)) return;
      try {
        const status = await runStatus(runId);
        if (!mountedRef.current || !status) return;
        setParked((cur) => {
          if (cur.some((p) => p.runId === runId && p.stepId === stepId)) return cur;
          // A real event for a run parked under the unknown-step sentinel
          // (launch recovery found no step_intent) UPGRADES the sentinel in
          // place — same underlying park, now with a grantable stepId, so
          // Confirm becomes available. parkedAtMs is kept: the park started
          // when we first learned of it, not when the step got named.
          const sentinel = cur.find((p) => p.runId === runId && p.stepId === UNKNOWN_STEP_ID);
          if (sentinel) {
            return sortByParkedAt(cur.map((p) => (p === sentinel ? { ...p, stepId } : p)));
          }
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
            if (disposed || !mountedRef.current) continue;
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
        // One status read per RUN (entries are keyed by (runId, stepId), but
        // run state is per-run — a run that left awaiting_consent invalidates
        // every parked pair it still has here).
        const runIds = [...new Set(parkedRef.current.map((p) => p.runId))];
        for (const runId of runIds) {
          try {
            const status = await runStatus(runId);
            if (!mountedRef.current) return;
            // A resolved `null` is UNKNOWN, not "gone" — a registry rotation
            // or a read racing a backend restart can answer null for a run
            // that is still parked. Dropping the park on null would HIDE a
            // live consent moment (spec §12: cannot hide). Remove only on an
            // explicit non-awaiting_consent state; null keeps the park and
            // retries next tick.
            if (status && status.state !== 'awaiting_consent') removeRun(runId);
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
      // ONLY this (runId, stepId) pair (Codex adrev P1): the resumed run may
      // already have parked its NEXT transmitting step and that entry —
      // possibly inserted while this grant was in flight — must survive.
      removePair(runId, stepId);
      return granted;
    },
    [removePair],
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
  /** Bumped (any change, value itself is meaningless) to request the modal
   *  reopen after the operator dismissed it via "Keep parked" — wired from
   *  AppShell to the StatusBar consent item's onClick. Consent cannot hide
   *  the PARK (badge/statusbar item stay put regardless); this only asks the
   *  MODAL to come back. */
  reopenSignal?: number;
}

interface StepIntentInfo {
  action: string;
  resolvedParams: unknown;
}

export function ConsentGate({ onParkedChange, reopenSignal }: ConsentGateProps) {
  const { parked, confirm, cancelParked } = useParkedRuns();
  const [stepIntent, setStepIntent] = useState<StepIntentInfo | null>(null);
  const [busy, setBusy] = useState(false);
  // "Keep parked" (reviewer fix: consent modal defer affordance) hides the
  // MODAL only — the park itself stays `awaiting_consent` in the engine, and
  // the MenuBar badge / StatusBar item (driven by `parked` via
  // `onParkedChange`, below, not by this flag) stay visible throughout.
  // "Consent cannot hide" still holds: what can hide is the dialog the
  // operator has already acknowledged is there, not the park's own presence.
  const [hidden, setHidden] = useState(false);
  // Forces a re-render every second so the "Parked HH:MM:SS" readout ticks
  // without needing to store the formatted string in state.
  const [, setTick] = useState(0);

  const onParkedChangeRef = useRef(onParkedChange);
  onParkedChangeRef.current = onParkedChange;
  useEffect(() => {
    onParkedChangeRef.current?.(parked);
  }, [parked]);

  // A brand-new park (a (runId, stepId) PAIR this instance hasn't seen before
  // — a fresh `awaitingConsent` event, a freshly-recovered launch park, or a
  // later transmitting step of a run whose earlier step was already handled)
  // always re-surfaces the modal, even if an EARLIER park was dismissed via
  // "Keep parked". Pair-keyed to match the parked list itself (Codex adrev
  // P1): step 2 of the same run is a NEW consent moment and must not stay
  // hidden behind step 1's dismissal. Removals (the parked set shrinking)
  // never touch `hidden`.
  const knownParkKeysRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    const keyOf = (p: ParkedRun) => `${p.runId} ${p.stepId}`;
    const hasNewPark = parked.some((p) => !knownParkKeysRef.current.has(keyOf(p)));
    knownParkKeysRef.current = new Set(parked.map(keyOf));
    if (hasNewPark) setHidden(false);
  }, [parked]);

  // The statusbar consent item's onClick (wired through AppShell) bumps this
  // to bring a dismissed modal back without waiting for a new park.
  const prevReopenSignalRef = useRef(reopenSignal);
  useEffect(() => {
    if (reopenSignal !== undefined && reopenSignal !== prevReopenSignalRef.current) {
      setHidden(false);
    }
    prevReopenSignalRef.current = reopenSignal;
  }, [reopenSignal]);

  const oldest = parked[0] ?? null;

  // Resolve the parked step's action + resolved params VERBATIM from the
  // journal's `step_intent` entry — never invent a message-staging readout
  // the backend doesn't expose (task-14 brief binding constraint 4).
  useEffect(() => {
    if (!oldest || oldest.stepId === UNKNOWN_STEP_ID) {
      // No step to look up: either nothing is parked, or launch recovery
      // found no `step_intent` in the journal (the unknown-step park).
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

  if (!oldest || hidden) return null;

  const total = parked.length;
  // Launch recovery couldn't name the parked step (no step_intent in the
  // journal). The park still shows — cannot hide — but consent can't be
  // granted without a real stepId, so Confirm is disabled with a plain
  // explanation; Cancel run stays available.
  const stepUnknown = oldest.stepId === UNKNOWN_STEP_ID;

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
  // Display-only: hides the modal, touches nothing else. The park stays
  // `awaiting_consent`; the badge/statusbar item (driven by `parked`, not by
  // `hidden`) keep reporting it. No grant, no deny, no skip — the engine
  // never hears about this click.
  const onKeepParked = () => {
    setHidden(true);
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
                run {oldest.runId} ·{' '}
                {stepUnknown ? 'step unknown — see run journal' : `step ${oldest.stepId}`}
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
            {stepUnknown
              ? `The run journal names no transmit step for this park, so consent cannot be granted from here. Cancel ends run ${oldest.runId} as cancelled (journaled: cancelled by operator).`
              : `Confirm keys the radio now. Cancel ends run ${oldest.runId} as cancelled (journaled: cancelled by operator).`}
          </div>
        </div>
        <div className="tux-consent-foot">
          <button
            type="button"
            className="tux-consent-btn-confirm"
            data-testid="consent-gate-confirm"
            disabled={busy || stepUnknown}
            title={stepUnknown ? 'The parked step is unknown — consent cannot be granted; cancel the run instead' : undefined}
            onClick={onConfirm}
          >
            Confirm transmit
          </button>
          <button
            type="button"
            className="tux-consent-btn-keepparked"
            data-testid="consent-gate-keepparked"
            title="Hide this dialog — the run stays parked; the menubar/statusbar badges stay visible"
            onClick={onKeepParked}
          >
            Keep parked
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
