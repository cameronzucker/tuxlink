/**
 * Tests for ConsentGate.tsx — the Part 97 transmit-consent moment (routines
 * plan-5 Task 14, `.superpowers/sdd/task-14-brief.md`, spec §12).
 *
 * `@tauri-apps/api/core` is mocked at module scope, keyed by command name
 * (feedback_vitest_invoke_mock_cleanup_call — the no-arg teardown call must
 * be inert). `@tauri-apps/api/event`'s `listen` is mocked to capture the
 * registered `routines:event` handler for manual dispatch, mirroring
 * `useRoutines.test.tsx`'s proven pattern.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act, waitFor } from '@testing-library/react';
import type { RoutinesEvent } from './routinesEvents';
import type { RunStatus, RunListEntry, JournalEntry } from './routinesApi';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

let routinesEventHandler: ((e: { payload: unknown }) => void) | null = null;
const unlistenFn = vi.fn();
const { mockListen } = vi.hoisted(() => ({ mockListen: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }));

import { ConsentGate, type ParkedRun } from './ConsentGate';

// Raw stylesheet for the z-index pin (mirrors MenuBar.test.tsx's proven
// import.meta.glob '?raw' pattern — jsdom applies no real stacking, so the
// occlusion invariant is pinned at the stylesheet source, not computed style).
const CONSENT_CSS_MODULES = import.meta.glob('./ConsentGate.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const consentCss = CONSENT_CSS_MODULES['./ConsentGate.css'];

const RUN_ID = 'run-12';
const STEP_ID = 's4';
const ROUTINE = 'Net-opening checklist';

const RUN_STATUS: RunStatus = { runId: RUN_ID, routine: ROUTINE, dryRun: false, state: 'awaiting_consent' };

const JOURNAL_FIXTURE: JournalEntry[] = [
  { ts_unix: 1000, run_id: RUN_ID, seq: 1, event: { type: 'run_started', routine: ROUTINE, snapshot: {}, dry_run: false } },
  { ts_unix: 1001, run_id: RUN_ID, seq: 2, event: { type: 'state_changed', state: 'running' } },
  {
    ts_unix: 1002,
    run_id: RUN_ID,
    seq: 3,
    event: {
      type: 'step_intent',
      step: STEP_ID,
      action: 'radio.connect',
      resolved_params: { gateway: 'W7BO-10', freqHz: 7_103_500 },
    },
  },
  { ts_unix: 1003, run_id: RUN_ID, seq: 4, event: { type: 'state_changed', state: 'awaiting_consent' } },
];

function callsFor(cmd: string) {
  return mockInvoke.mock.calls.filter((c) => c[0] === cmd);
}

function emit(payload: RoutinesEvent) {
  act(() => {
    routinesEventHandler?.({ payload });
  });
}

/** Default: nothing live at launch, one known run + journal available on
 *  demand. Individual tests override `runsListResult` for launch recovery. */
let runsListResult: RunListEntry[] = [];

beforeEach(() => {
  mockInvoke.mockReset();
  mockListen.mockReset();
  unlistenFn.mockReset();
  routinesEventHandler = null;
  runsListResult = [];

  mockListen.mockImplementation((event: string, handler: (e: { payload: unknown }) => void) => {
    if (event === 'routines:event') routinesEventHandler = handler;
    return Promise.resolve(unlistenFn);
  });

  // Teardown pitfall: invoke mocks get called with NO args at cleanup — the
  // `cmd === undefined` branch must be inert, never throw.
  mockInvoke.mockImplementation((cmd?: string, args?: Record<string, unknown>) => {
    if (cmd === undefined) return Promise.resolve();
    switch (cmd) {
      case 'routines_runs_list':
        return Promise.resolve(runsListResult);
      case 'routines_run_status':
        return Promise.resolve(args?.runId === RUN_ID ? RUN_STATUS : null);
      case 'routines_journal':
        return Promise.resolve(args?.runId === RUN_ID ? JOURNAL_FIXTURE : []);
      case 'routines_consent_grant':
        return Promise.resolve(true);
      case 'routines_cancel':
        return Promise.resolve(true);
      default:
        return Promise.resolve([]);
    }
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe('ConsentGate — (a) awaitingConsent event parks + surfaces the modal', () => {
  it('names the routine + step in the modal and reports the parked list upward', async () => {
    const onParkedChange = vi.fn();
    render(<ConsentGate onParkedChange={onParkedChange} />);
    await act(async () => {});

    expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();

    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await act(async () => {});

    expect(await screen.findByTestId('consent-gate-modal')).toBeInTheDocument();
    expect(screen.getByTestId('consent-gate-routine')).toHaveTextContent(ROUTINE);
    expect(screen.getByTestId('consent-gate-run-step')).toHaveTextContent(RUN_ID);
    expect(screen.getByTestId('consent-gate-run-step')).toHaveTextContent(STEP_ID);

    // The resolved step_intent params render VERBATIM (task-14 brief binding
    // constraint 4) — no invented message-staging readout.
    expect(screen.getByTestId('consent-gate-txbox')).toHaveTextContent('W7BO-10');
    expect(screen.getByTestId('consent-gate-txbox')).toHaveTextContent('7103500');

    // The upward callback is what AppShell would mirror into the MenuBar
    // badge (count) and the StatusBar item (naming the routine).
    const last = onParkedChange.mock.calls.at(-1)?.[0] as ParkedRun[];
    expect(last).toHaveLength(1);
    expect(last[0]).toMatchObject({ runId: RUN_ID, stepId: STEP_ID, routine: ROUTINE });
  });

  it('has NO Skip button — the engine has no skip-this-step outcome to call', async () => {
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');

    expect(screen.queryByRole('button', { name: /skip/i })).not.toBeInTheDocument();
    // The two outcomes that DO exist:
    expect(screen.getByRole('button', { name: /Confirm transmit/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Cancel run/ })).toBeInTheDocument();
  });
});

describe('ConsentGate — (b) Confirm', () => {
  it('invokes routines_consent_grant with {runId, stepId} and closes on true', async () => {
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');

    screen.getByTestId('consent-gate-confirm').click();
    await waitFor(() => {
      expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
    });

    const call = callsFor('routines_consent_grant').at(-1);
    expect(call?.[1]).toEqual({ runId: RUN_ID, stepId: STEP_ID });
  });

  it('a false grant (the park vanished) still removes the entry and closes', async () => {
    mockInvoke.mockImplementation((cmd?: string, args?: Record<string, unknown>) => {
      if (cmd === undefined) return Promise.resolve();
      if (cmd === 'routines_runs_list') return Promise.resolve(runsListResult);
      if (cmd === 'routines_run_status') return Promise.resolve(args?.runId === RUN_ID ? RUN_STATUS : null);
      if (cmd === 'routines_journal') return Promise.resolve(args?.runId === RUN_ID ? JOURNAL_FIXTURE : []);
      if (cmd === 'routines_consent_grant') return Promise.resolve(false);
      return Promise.resolve([]);
    });

    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');

    screen.getByTestId('consent-gate-confirm').click();
    await waitFor(() => {
      expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
    });
  });
});

describe('ConsentGate — (c) Cancel run', () => {
  it('invokes routines_cancel and closes the modal', async () => {
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');

    screen.getByTestId('consent-gate-cancel').click();
    await waitFor(() => {
      expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
    });

    const call = callsFor('routines_cancel').at(-1);
    expect(call?.[1]).toEqual({ runId: RUN_ID });
  });
});

describe('ConsentGate — (d) runFinished clears the park', () => {
  it('closes the modal and reports an empty parked list for the parked run', async () => {
    const onParkedChange = vi.fn();
    render(<ConsentGate onParkedChange={onParkedChange} />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');

    emit({ kind: 'runFinished', runId: RUN_ID, state: 'cancelled' });
    await waitFor(() => {
      expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
    });
    expect(onParkedChange.mock.calls.at(-1)?.[0]).toEqual([]);
  });

  it('a runFinished for a DIFFERENT run does not clear the park', async () => {
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');

    emit({ kind: 'runFinished', runId: 'some-other-run', state: 'completed' });
    await act(async () => {});
    expect(screen.getByTestId('consent-gate-modal')).toBeInTheDocument();
  });
});

describe('ConsentGate — (e) launch recovery', () => {
  it('surfaces the modal from listRuns + the journal fixture with NO event fired', async () => {
    runsListResult = [
      { runId: RUN_ID, routine: ROUTINE, dryRun: false, startedUnix: 1000, state: 'awaiting_consent', finishedUnix: null },
    ];

    render(<ConsentGate />);
    // Deliberately no `emit(...)` call — recovery must not depend on an event.
    expect(await screen.findByTestId('consent-gate-modal')).toBeInTheDocument();
    expect(screen.getByTestId('consent-gate-routine')).toHaveTextContent(ROUTINE);
    expect(screen.getByTestId('consent-gate-run-step')).toHaveTextContent(STEP_ID);
  });

  it('recovers nothing when no run is live in awaiting_consent', async () => {
    runsListResult = [
      { runId: 'other-run', routine: 'Some other routine', dryRun: false, startedUnix: 1000, state: 'running', finishedUnix: null },
    ];
    render(<ConsentGate />);
    await act(async () => {});
    expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
  });

  // Reviewer fix 3: a parked run whose journal carries NO step_intent must
  // still surface (cannot hide) — step rendered as unknown, Confirm disabled
  // (granting needs a real stepId), Cancel run still available.
  it('parks a run with no step_intent in the journal: modal shows, Confirm disabled, Cancel enabled', async () => {
    const NO_INTENT_RUN = 'run-no-intent';
    runsListResult = [
      { runId: NO_INTENT_RUN, routine: 'Journal-less routine', dryRun: false, startedUnix: 1000, state: 'awaiting_consent', finishedUnix: null },
    ];
    mockInvoke.mockImplementation((cmd?: string, args?: Record<string, unknown>) => {
      if (cmd === undefined) return Promise.resolve();
      if (cmd === 'routines_runs_list') return Promise.resolve(runsListResult);
      if (cmd === 'routines_run_status') {
        return Promise.resolve(
          args?.runId === NO_INTENT_RUN
            ? { runId: NO_INTENT_RUN, routine: 'Journal-less routine', dryRun: false, state: 'awaiting_consent' }
            : null,
        );
      }
      // Journal with no step_intent entry at all.
      if (cmd === 'routines_journal') {
        return Promise.resolve([
          { ts_unix: 1000, run_id: NO_INTENT_RUN, seq: 1, event: { type: 'state_changed', state: 'awaiting_consent' } },
        ]);
      }
      if (cmd === 'routines_cancel') return Promise.resolve(true);
      return Promise.resolve([]);
    });

    render(<ConsentGate />);
    expect(await screen.findByTestId('consent-gate-modal')).toBeInTheDocument();
    expect(screen.getByTestId('consent-gate-routine')).toHaveTextContent('Journal-less routine');
    expect(screen.getByTestId('consent-gate-run-step')).toHaveTextContent('step unknown — see run journal');
    expect(screen.getByTestId('consent-gate-confirm')).toBeDisabled();
    expect(screen.getByTestId('consent-gate-cancel')).not.toBeDisabled();

    // Cancel run remains a working exit.
    screen.getByTestId('consent-gate-cancel').click();
    await waitFor(() => {
      expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
    });
    expect(callsFor('routines_cancel').at(-1)?.[1]).toEqual({ runId: NO_INTENT_RUN });
  });
});

describe('ConsentGate — multiple parked runs', () => {
  it('shows the OLDEST parked run with a "1 of N" pip', async () => {
    mockInvoke.mockImplementation((cmd?: string, args?: Record<string, unknown>) => {
      if (cmd === undefined) return Promise.resolve();
      if (cmd === 'routines_runs_list') return Promise.resolve(runsListResult);
      if (cmd === 'routines_run_status') {
        if (args?.runId === RUN_ID) return Promise.resolve(RUN_STATUS);
        if (args?.runId === 'run-99') {
          return Promise.resolve({ runId: 'run-99', routine: 'Second routine', dryRun: false, state: 'awaiting_consent' });
        }
        return Promise.resolve(null);
      }
      if (cmd === 'routines_journal') return Promise.resolve(args?.runId === RUN_ID ? JOURNAL_FIXTURE : []);
      return Promise.resolve([]);
    });

    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');
    emit({ kind: 'awaitingConsent', runId: 'run-99', stepId: 's1' });
    await act(async () => {});

    // Still the FIRST (oldest) run's detail...
    expect(screen.getByTestId('consent-gate-routine')).toHaveTextContent(ROUTINE);
    // ...with a pip showing the queue depth.
    expect(screen.getByTestId('consent-gate-pip')).toHaveTextContent('1 of 2');
  });

  it('shows no pip when only one run is parked', async () => {
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');
    expect(screen.queryByTestId('consent-gate-pip')).not.toBeInTheDocument();
  });
});

describe('ConsentGate — live duration + reconciliation poll', () => {
  it('ticks the "Parked" duration display every second', async () => {
    vi.useFakeTimers();
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await vi.advanceTimersByTimeAsync(0);
    expect(screen.getByTestId('consent-gate-modal')).toBeInTheDocument();

    const before = screen.getByTestId('consent-gate-parked').textContent;
    await vi.advanceTimersByTimeAsync(5000);
    const after = screen.getByTestId('consent-gate-parked').textContent;
    expect(after).not.toBe(before);
  });

  it('a poll that finds the run left awaiting_consent removes the park', async () => {
    vi.useFakeTimers();
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await vi.advanceTimersByTimeAsync(0);
    expect(screen.getByTestId('consent-gate-modal')).toBeInTheDocument();

    // The run advanced past awaiting_consent from elsewhere (e.g. another
    // operator window) WITHOUT a runFinished event reaching this instance.
    mockInvoke.mockImplementation((cmd?: string, args?: Record<string, unknown>) => {
      if (cmd === undefined) return Promise.resolve();
      if (cmd === 'routines_runs_list') return Promise.resolve(runsListResult);
      if (cmd === 'routines_run_status') {
        return Promise.resolve(
          args?.runId === RUN_ID ? { ...RUN_STATUS, state: 'running' } : null,
        );
      }
      if (cmd === 'routines_journal') return Promise.resolve(args?.runId === RUN_ID ? JOURNAL_FIXTURE : []);
      return Promise.resolve([]);
    });

    await vi.advanceTimersByTimeAsync(5000);
    expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
  });

  // Reviewer fix 2: a poll tick resolving null is UNKNOWN, not "gone" — the
  // park is retained and retried next tick; only an explicit
  // non-awaiting_consent state removes it (asserted by the test above).
  it('a poll tick resolving null keeps the park (unknown ≠ gone)', async () => {
    vi.useFakeTimers();
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await vi.advanceTimersByTimeAsync(0);
    expect(screen.getByTestId('consent-gate-modal')).toBeInTheDocument();

    // runStatus now answers null for the parked run (registry rotation /
    // read racing a backend restart) — the run may still be parked.
    mockInvoke.mockImplementation((cmd?: string) => {
      if (cmd === undefined) return Promise.resolve();
      if (cmd === 'routines_runs_list') return Promise.resolve(runsListResult);
      if (cmd === 'routines_run_status') return Promise.resolve(null);
      if (cmd === 'routines_journal') return Promise.resolve(JOURNAL_FIXTURE);
      return Promise.resolve([]);
    });

    // Several poll ticks — the park must survive every null read.
    await vi.advanceTimersByTimeAsync(15_000);
    expect(screen.getByTestId('consent-gate-modal')).toBeInTheDocument();
  });
});

// Final whole-branch review, Fix 1 (Important): the consent modal locked the
// entire app (TitleBar controls, mailbox, Runs monitor all behind the
// backdrop) with no way out short of granting/cancelling. "Keep parked" hides
// the MODAL only — the park, badge, and statusbar item all persist.
describe('ConsentGate — "Keep parked" defer affordance', () => {
  it('hides the modal without granting/cancelling; the parked list is unchanged', async () => {
    const onParkedChange = vi.fn();
    render(<ConsentGate onParkedChange={onParkedChange} />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');

    onParkedChange.mockClear();
    act(() => {
      screen.getByTestId('consent-gate-keepparked').click();
    });

    expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
    // No engine call — neither grant nor cancel fired.
    expect(callsFor('routines_consent_grant')).toHaveLength(0);
    expect(callsFor('routines_cancel')).toHaveLength(0);
    // The parked list itself is untouched — onParkedChange never fires from
    // a Keep-parked click (nothing about `parked` changed).
    expect(onParkedChange).not.toHaveBeenCalled();
  });

  it('a NEW park re-shows the modal even after Keep parked dismissed the first one', async () => {
    // A second, distinct run must resolve a real (non-null) runStatus for
    // useParkedRuns' addParked to actually track it — mirrors the "multiple
    // parked runs" describe block's own mockInvoke override below.
    mockInvoke.mockImplementation((cmd?: string, args?: Record<string, unknown>) => {
      if (cmd === undefined) return Promise.resolve();
      if (cmd === 'routines_runs_list') return Promise.resolve(runsListResult);
      if (cmd === 'routines_run_status') {
        if (args?.runId === RUN_ID) return Promise.resolve(RUN_STATUS);
        if (args?.runId === 'run-99') {
          return Promise.resolve({ runId: 'run-99', routine: 'Second routine', dryRun: false, state: 'awaiting_consent' });
        }
        return Promise.resolve(null);
      }
      if (cmd === 'routines_journal') return Promise.resolve(args?.runId === RUN_ID ? JOURNAL_FIXTURE : []);
      return Promise.resolve([]);
    });

    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');
    act(() => {
      screen.getByTestId('consent-gate-keepparked').click();
    });
    expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();

    emit({ kind: 'awaitingConsent', runId: 'run-99', stepId: 's1' });
    await waitFor(() => {
      expect(screen.getByTestId('consent-gate-modal')).toBeInTheDocument();
    });
  });

  it('a reopenSignal bump re-shows the modal after Keep parked dismissed it', async () => {
    const { rerender } = render(<ConsentGate reopenSignal={0} />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');
    act(() => {
      screen.getByTestId('consent-gate-keepparked').click();
    });
    expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();

    rerender(<ConsentGate reopenSignal={1} />);
    expect(await screen.findByTestId('consent-gate-modal')).toBeInTheDocument();
  });

  it('Confirm/Cancel/no-Skip behavior is unchanged with Keep parked present', async () => {
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');

    expect(screen.queryByRole('button', { name: /skip/i })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Confirm transmit/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Cancel run/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Keep parked/ })).toBeInTheDocument();

    screen.getByTestId('consent-gate-confirm').click();
    await waitFor(() => {
      expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
    });
    expect(callsFor('routines_consent_grant').at(-1)?.[1]).toEqual({ runId: RUN_ID, stepId: STEP_ID });
  });
});

// Codex adrev P1: parked entries are keyed by the (runId, stepId) PAIR, not
// runId alone. An attended routine with two transmitting steps can emit step
// 2's awaitingConsent BEFORE the async grant path removes step 1's entry —
// runId-only keying dropped that second park entirely (add-dedupe
// early-returned on the still-present runId, then the grant's removal
// deleted it: no modal, no badge, invisible until app restart).
describe('ConsentGate — pair-keyed parks (Codex adrev P1)', () => {
  it('the exact race: awaitingConsent(run, s2) fired while step 1 grant is in flight → s2 park survives', async () => {
    // A grant whose resolution the test controls, so the s2 event can fire
    // strictly BEFORE step 1's removal settles.
    let resolveGrant: (v: boolean) => void = () => {};
    mockInvoke.mockImplementation((cmd?: string, args?: Record<string, unknown>) => {
      if (cmd === undefined) return Promise.resolve();
      if (cmd === 'routines_runs_list') return Promise.resolve(runsListResult);
      if (cmd === 'routines_run_status') {
        return Promise.resolve(args?.runId === RUN_ID ? RUN_STATUS : null);
      }
      if (cmd === 'routines_journal') return Promise.resolve(args?.runId === RUN_ID ? JOURNAL_FIXTURE : []);
      if (cmd === 'routines_consent_grant') {
        return new Promise<boolean>((res) => {
          resolveGrant = res;
        });
      }
      return Promise.resolve([]);
    });

    const onParkedChange = vi.fn();
    render(<ConsentGate onParkedChange={onParkedChange} />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');

    // Operator confirms step 1 — the grant is now IN FLIGHT (unresolved).
    screen.getByTestId('consent-gate-confirm').click();

    // Backend resumes, reaches step 2, and emits its park BEFORE the grant
    // promise resolves (the Codex race). The pair-keyed add must insert it
    // even though (RUN_ID, s4) is still tracked.
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: 's7' });
    await act(async () => {});

    // Now the grant settles true → removes ONLY (RUN_ID, s4).
    act(() => {
      resolveGrant(true);
    });
    await act(async () => {});

    // s7's park exists: modal shows step s7, and the badge feed reports
    // exactly one parked entry (count 1).
    expect(screen.getByTestId('consent-gate-modal')).toBeInTheDocument();
    expect(screen.getByTestId('consent-gate-run-step')).toHaveTextContent('step s7');
    const last = onParkedChange.mock.calls.at(-1)?.[0] as ParkedRun[];
    expect(last).toHaveLength(1);
    expect(last[0]).toMatchObject({ runId: RUN_ID, stepId: 's7' });
  });

  it('runFinished removes BOTH pairs of the same run when two are parked', async () => {
    const onParkedChange = vi.fn();
    render(<ConsentGate onParkedChange={onParkedChange} />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await screen.findByTestId('consent-gate-modal');
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: 's7' });
    await act(async () => {});

    // Two pair-keyed entries for the one run.
    expect(onParkedChange.mock.calls.at(-1)?.[0]).toHaveLength(2);

    emit({ kind: 'runFinished', runId: RUN_ID, state: 'cancelled' });
    await waitFor(() => {
      expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
    });
    expect(onParkedChange.mock.calls.at(-1)?.[0]).toEqual([]);
  });

  it('a real awaitingConsent upgrades the unknown-step sentinel park in place (Confirm becomes enabled)', async () => {
    const NO_INTENT_RUN = 'run-no-intent';
    runsListResult = [
      { runId: NO_INTENT_RUN, routine: 'Journal-less routine', dryRun: false, startedUnix: 1000, state: 'awaiting_consent', finishedUnix: null },
    ];
    mockInvoke.mockImplementation((cmd?: string, args?: Record<string, unknown>) => {
      if (cmd === undefined) return Promise.resolve();
      if (cmd === 'routines_runs_list') return Promise.resolve(runsListResult);
      if (cmd === 'routines_run_status') {
        return Promise.resolve(
          args?.runId === NO_INTENT_RUN
            ? { runId: NO_INTENT_RUN, routine: 'Journal-less routine', dryRun: false, state: 'awaiting_consent' }
            : null,
        );
      }
      // Journal still has no step_intent — the s4 name arrives via the event.
      if (cmd === 'routines_journal') {
        return Promise.resolve([
          { ts_unix: 1000, run_id: NO_INTENT_RUN, seq: 1, event: { type: 'state_changed', state: 'awaiting_consent' } },
        ]);
      }
      return Promise.resolve([]);
    });

    const onParkedChange = vi.fn();
    render(<ConsentGate onParkedChange={onParkedChange} />);
    // Launch recovery parks under the unknown-step sentinel: Confirm disabled.
    expect(await screen.findByTestId('consent-gate-modal')).toBeInTheDocument();
    expect(screen.getByTestId('consent-gate-run-step')).toHaveTextContent('step unknown — see run journal');
    expect(screen.getByTestId('consent-gate-confirm')).toBeDisabled();

    // The real event for the same run names the step → the sentinel entry is
    // REPLACED (single entry, not a second park), and Confirm enables.
    emit({ kind: 'awaitingConsent', runId: NO_INTENT_RUN, stepId: 's4' });
    await waitFor(() => {
      expect(screen.getByTestId('consent-gate-run-step')).toHaveTextContent('step s4');
    });
    expect(screen.getByTestId('consent-gate-confirm')).not.toBeDisabled();
    expect(screen.queryByTestId('consent-gate-pip')).not.toBeInTheDocument(); // single entry — no "1 of 2"
    const last = onParkedChange.mock.calls.at(-1)?.[0] as ParkedRun[];
    expect(last).toHaveLength(1);
    expect(last[0]).toMatchObject({ runId: NO_INTENT_RUN, stepId: 's4' });
  });
});

// Reviewer fix 1 (CRITICAL): the consent overlay must render ABOVE every
// other fixed layer — the inset:0 overlay panels (SettingsPanel /
// StationFinderPanel / RequestCenter / InboundSelectionPanel, all z-1000)
// and the guided-tour HintOverlay stack (1200-1202). jsdom computes no real
// stacking contexts, so the invariant is pinned at the stylesheet source
// (the same technique MenuBar.test.tsx uses for its dropdown z-order pin).
describe('ConsentGate — cannot hide (z-order pin)', () => {
  it('the overlay z-index beats the z-1000 panels and the 1200-1202 hint stack', () => {
    const match = consentCss.match(/\.tux-consent-overlay\s*\{[^}]*z-index:\s*(\d+)/);
    expect(match).not.toBeNull();
    const overlayZ = Number(match![1]);
    expect(overlayZ).toBeGreaterThan(1000); // SettingsPanel & friends
    expect(overlayZ).toBeGreaterThan(1202); // HintOverlay stack ceiling
  });
});

// tuxlink-dmwte task 8, behavior 6 (spec §6): the ConsentGate splits along the
// data/modal seam it already has. `renderModal={false}` (the main window while
// Routines is popped) must keep the data hook + `onParkedChange` mirroring
// running — the amber badge and StatusBar item never move — while rendering NO
// modal in that window (the popped host renders it instead).
describe('ConsentGate — renderModal split (behavior 6)', () => {
  it('renderModal={false} renders no modal but still reports the parked list upward', async () => {
    const onParkedChange = vi.fn();
    render(<ConsentGate renderModal={false} onParkedChange={onParkedChange} />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await waitFor(() => {
      const last = onParkedChange.mock.calls.at(-1)?.[0] as ParkedRun[] | undefined;
      expect(last?.length).toBe(1);
    });
    // The data half kept running: the parked list was reported for the badge.
    const last = onParkedChange.mock.calls.at(-1)?.[0] as ParkedRun[];
    expect(last[0]).toMatchObject({ runId: RUN_ID, stepId: STEP_ID, routine: ROUTINE });
    // ...but this window renders no modal (the popped host owns it).
    expect(screen.queryByTestId('consent-gate-modal')).not.toBeInTheDocument();
  });

  it('renderModal defaults to true — the modal renders as before', async () => {
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    expect(await screen.findByTestId('consent-gate-modal')).toBeInTheDocument();
  });

  it('renderModal={true} renders the modal (the popped host case)', async () => {
    render(<ConsentGate renderModal={true} />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    expect(await screen.findByTestId('consent-gate-modal')).toBeInTheDocument();
  });
});

// tuxlink-dmwte task 8, behavior 8 (spec §6, adrev R2-F8): a launch-recovered
// park seeds its "Parked HH:MM:SS" duration from the journal's `step_intent`
// timestamp, NOT this UI instance's learn-time — so opening the app (or moving
// the modal to a different host window) cannot reset a Part 97 surface's
// asserted duration. Live-event parks (below) keep Date.now(): the event IS the
// park moment.
describe('ConsentGate — journal-seeded park duration (behavior 8)', () => {
  it('a launch-recovered park counts from the journal step_intent timestamp, not learn-time', async () => {
    vi.useFakeTimers();
    // "Now" is 2000s; the step_intent journal entry is stamped 1002s (see
    // JOURNAL_FIXTURE) — so the park is (2000 - 1002) = 998s = 00:16:38 old,
    // NOT ~00:00:00 as a learn-time seed would show.
    vi.setSystemTime(2_000_000);
    runsListResult = [
      { runId: RUN_ID, routine: ROUTINE, dryRun: false, startedUnix: 1000, state: 'awaiting_consent', finishedUnix: null },
    ];
    render(<ConsentGate />);
    await vi.advanceTimersByTimeAsync(0);
    expect(screen.getByTestId('consent-gate-modal')).toBeInTheDocument();
    // 998 seconds → 00:16:38.
    expect(screen.getByTestId('consent-gate-parked')).toHaveTextContent('00:16:38');
  });

  it('a live-event park counts from Date.now() (the event is the park moment)', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(2_000_000);
    render(<ConsentGate />);
    emit({ kind: 'awaitingConsent', runId: RUN_ID, stepId: STEP_ID });
    await vi.advanceTimersByTimeAsync(0);
    expect(screen.getByTestId('consent-gate-modal')).toBeInTheDocument();
    // Just parked — near-zero elapsed, NOT the journal's 998s.
    expect(screen.getByTestId('consent-gate-parked')).toHaveTextContent('00:00:00');
  });
});
