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
});
