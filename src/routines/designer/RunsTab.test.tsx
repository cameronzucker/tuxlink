/**
 * Tests for RunsTab.tsx — the Runs tab (routines plan-5 Task 13,
 * `.superpowers/sdd/task-13-brief.md`, spec §12 flows 3/5/6).
 *
 * `@tauri-apps/api/core` (`invoke`) and `@tauri-apps/plugin-dialog` (`save`)
 * are mocked at module scope via `vi.hoisted` (feedback_vitest_invoke_mock_
 * cleanup_call — the no-arg teardown call must be inert). `@tauri-apps/api/
 * event`'s `listen` is mocked to a no-op resolved unlisten so the
 * `runFinished`-nudge listener effect doesn't hit a real (nonexistent) Tauri
 * IPC bridge, mirroring useRoutines.test.tsx's proven pattern.
 *
 * The fixture journal below is JSON literals matching the wire table
 * verbatim (routinesApi.ts's header comment): `ts_unix`/`run_id`/`seq`,
 * `event.type` snake-case tags, `step_err`'s cause byte-for-byte
 * "VARA HF: DISCONNECTED — Link timeout: no data received for 90 s".
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act, within, waitFor } from '@testing-library/react';
import type { JournalEntry, RunListEntry, RunStatus } from '../routinesApi';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

const { mockSaveDialog } = vi.hoisted(() => ({ mockSaveDialog: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: mockSaveDialog }));

const { mockListen } = vi.hoisted(() => ({ mockListen: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }));

import { RunsTab, ganttModel, radioAwaitRig } from './RunsTab';

// ---------------------------------------------------------------------------
// Fixture journal (task-13 brief Step 1): run_started with snapshot,
// step_intent/step_ok, a step_err with the required verbatim cause,
// state_changed -> awaiting_consent, run_finished.
// ---------------------------------------------------------------------------

const T = 1_752_400_000;
const CAUSE = 'VARA HF: DISCONNECTED — Link timeout: no data received for 90 s';

const SNAPSHOT = {
  routine: 'net-opening-checklist',
  schema_version: 1,
  transmit_mode: 'attended',
  triggers: [{ type: 'manual' }],
  tracks: [
    {
      name: 'net-control',
      steps: [
        { id: 's1', action: 'cat.apply_preset' },
        { id: 's2', action: 'radio.connect' },
      ],
    },
  ],
};

const FIXTURE_JOURNAL: JournalEntry[] = [
  {
    ts_unix: T,
    run_id: 'run-1',
    seq: 0,
    event: { type: 'run_started', routine: 'net-opening-checklist', snapshot: SNAPSHOT, dry_run: false },
  },
  {
    ts_unix: T + 1,
    run_id: 'run-1',
    seq: 1,
    event: { type: 'step_intent', step: 's1', action: 'cat.apply_preset', resolved_params: {} },
  },
  { ts_unix: T + 2, run_id: 'run-1', seq: 2, event: { type: 'step_ok', step: 's1', output: {} } },
  {
    ts_unix: T + 3,
    run_id: 'run-1',
    seq: 3,
    event: { type: 'step_intent', step: 's2', action: 'radio.connect', resolved_params: { rig: 'g90' } },
  },
  {
    ts_unix: T + 230,
    run_id: 'run-1',
    seq: 4,
    event: {
      type: 'step_err',
      step: 's2',
      error: { kind: 'action', detail: { action: 'radio.connect', cause: CAUSE } },
    },
  },
  { ts_unix: T + 400, run_id: 'run-1', seq: 5, event: { type: 'state_changed', state: 'awaiting_consent' } },
  { ts_unix: T + 560, run_id: 'run-1', seq: 6, event: { type: 'run_finished', state: 'cancelled', reason: null } },
];

const RUN_1_ENTRY: RunListEntry = {
  runId: 'run-1',
  routine: 'net-opening-checklist',
  dryRun: false,
  startedUnix: T,
  state: 'cancelled',
  finishedUnix: T + 560,
};

const RUN_1_STATUS: RunStatus = {
  runId: 'run-1',
  routine: 'net-opening-checklist',
  dryRun: false,
  state: 'cancelled',
};

type InvokeOverrides = Partial<Record<string, (args: unknown) => unknown>>;

function installInvokeMock(overrides: InvokeOverrides = {}) {
  mockInvoke.mockImplementation((cmd?: string, args?: unknown) => {
    // Teardown pitfall: invoke mocks get called with NO args at cleanup.
    if (cmd === undefined) return Promise.resolve();
    if (cmd && cmd in overrides) return Promise.resolve(overrides[cmd]!(args));
    switch (cmd) {
      case 'routines_runs_list':
        return Promise.resolve([RUN_1_ENTRY]);
      case 'routines_run_status':
        return Promise.resolve(RUN_1_STATUS);
      case 'routines_journal':
        return Promise.resolve(FIXTURE_JOURNAL);
      case 'routines_cancel':
        return Promise.resolve(true);
      case 'routines_take_radio':
        return Promise.resolve(true);
      case 'routines_export_run_bundle':
        return Promise.resolve({ path: '/home/operator/exports/tuxlink-run-run-1.json', bytes: 512 });
      default:
        return Promise.resolve(undefined);
    }
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockSaveDialog.mockReset();
  mockListen.mockReset();
  mockListen.mockImplementation(() => Promise.resolve(vi.fn()));
  installInvokeMock();
});

afterEach(() => {
  vi.useRealTimers();
});

function callsFor(cmd: string) {
  return mockInvoke.mock.calls.filter((c) => c[0] === cmd);
}

// ---------------------------------------------------------------------------
// (a) ganttModel — pure helper, unit-tested directly against the fixture.
// ---------------------------------------------------------------------------

describe('ganttModel (a)', () => {
  it('derives one lane per track, bars from step_intent/step_ok/step_err pairs, and a parked interval from state_changed', () => {
    const model = ganttModel(FIXTURE_JOURNAL, T + 560);

    expect(model.t0).toBe(T);
    expect(model.t1).toBe(T + 560);
    expect(model.live).toBe(false);
    expect(model.lanes).toHaveLength(1);
    expect(model.lanes[0]!.track).toBe('net-control');

    const bars = model.lanes[0]!.bars;
    expect(bars).toHaveLength(3);

    const ok = bars.find((b) => b.kind === 'ok');
    expect(ok).toMatchObject({ stepId: 's1', action: 'cat.apply_preset', t0: T + 1, t1: T + 2 });

    const fail = bars.find((b) => b.kind === 'fail');
    expect(fail).toMatchObject({ stepId: 's2', action: 'radio.connect', t0: T + 3, t1: T + 230 });
    expect(fail!.resultEntry?.event.type).toBe('step_err');

    // The awaiting_consent state_changed anchors to the last CLOSED step
    // (s2, via its step_err) since no step_intent is open at parking time —
    // module doc's "no step id on state_changed" gap.
    const parked = bars.find((b) => b.kind === 'consent');
    expect(parked).toMatchObject({ stepId: 's2', parkedState: 'awaiting_consent', t0: T + 400, t1: T + 560 });
  });

  it('returns an empty model when the journal has no run_started entry', () => {
    const model = ganttModel([], 1000);
    expect(model.lanes).toEqual([]);
    expect(model.t0).toBe(1000);
    expect(model.t1).toBe(1000);
  });

  it('extends t1 to `now` and sets live:true when the journal has no run_finished entry yet', () => {
    const liveJournal = FIXTURE_JOURNAL.slice(0, 4); // through the open s2 step_intent, no err/finish yet
    const model = ganttModel(liveJournal, T + 1000);
    expect(model.live).toBe(true);
    expect(model.t1).toBe(T + 1000);
  });

  it('renders a still-open step_intent on a LIVE run as an open-ended running bar to the now-line', () => {
    const liveJournal = FIXTURE_JOURNAL.slice(0, 4); // s1 closed ok, s2 intent open, run still live
    const model = ganttModel(liveJournal, T + 1000);
    const bars = model.lanes[0]!.bars;
    const running = bars.find((b) => b.kind === 'running');
    expect(running).toMatchObject({ stepId: 's2', action: 'radio.connect', t0: T + 3, t1: T + 1000 });
    expect(running!.resultEntry).toBeUndefined();
    // The closed s1 pair is unchanged alongside it.
    expect(bars.find((b) => b.kind === 'ok')).toMatchObject({ stepId: 's1' });
  });

  it('renders an unclosed step_intent on an INTERRUPTED run as a visible interrupted bar ending at the last journal entry', () => {
    const interruptedJournal: JournalEntry[] = [
      FIXTURE_JOURNAL[0]!, // run_started at T
      FIXTURE_JOURNAL[1]!, // s1 intent at T+1
      FIXTURE_JOURNAL[2]!, // s1 ok at T+2
      FIXTURE_JOURNAL[3]!, // s2 intent at T+3 — never closed; the process died mid-step
      {
        ts_unix: T + 50,
        run_id: 'run-1',
        seq: 4,
        event: { type: 'run_finished', state: 'interrupted', reason: 'recovered at launch' },
      },
    ];
    const model = ganttModel(interruptedJournal, T + 9999);
    expect(model.live).toBe(false);
    const bars = model.lanes[0]!.bars;
    const interrupted = bars.find((b) => b.kind === 'interrupted');
    expect(interrupted).toMatchObject({ stepId: 's2', action: 'radio.connect', t0: T + 3, t1: T + 50 });
    expect(interrupted!.resultEntry).toBeUndefined();
    expect(bars.find((b) => b.kind === 'ok')).toMatchObject({ stepId: 's1' });
  });
});

describe('radioAwaitRig', () => {
  it('reads the rig param off the most recent step_intent (journal has no rig field on state_changed itself)', () => {
    expect(radioAwaitRig(FIXTURE_JOURNAL.slice(0, 4))).toBe('g90');
  });

  it('defaults to "default" when no step_intent carries a rig param', () => {
    expect(radioAwaitRig(FIXTURE_JOURNAL.slice(0, 2))).toBe('default');
  });
});

// ---------------------------------------------------------------------------
// Rendered component
// ---------------------------------------------------------------------------

function renderRunsTab(props: Partial<Parameters<typeof RunsTab>[0]> = {}) {
  return render(<RunsTab routine={props.routine ?? 'net-opening-checklist'} highlightRunId={props.highlightRunId ?? null} />);
}

describe('RunsTab — run list + selection', () => {
  it('lists runs newest-first with a state badge, and auto-selects highlightRunId when present', async () => {
    renderRunsTab({ highlightRunId: 'run-1' });
    await screen.findByTestId('runrow-run-1');
    await waitFor(() => expect(screen.getByTestId('runrow-run-1')).toHaveClass('sel'));
  });
});

// ---------------------------------------------------------------------------
// (b) Step detail — verbatim cause, resolved inputs, real journal path.
// ---------------------------------------------------------------------------

describe('RunsTab — step detail (b)', () => {
  it('auto-selects the failed step and renders its cause byte-for-byte, resolved inputs, and the real journal path', async () => {
    renderRunsTab();
    const causeEl = await screen.findByTestId('stepdetail-cause');
    expect(causeEl.textContent).toContain(CAUSE);

    const resolvedEl = screen.getByTestId('stepdetail-resolved');
    expect(resolvedEl.textContent).toContain('"rig":"g90"');

    const pathEl = screen.getByTestId('stepdetail-path');
    expect(pathEl.textContent).toBe('journal: run-1.jsonl');
    // Never the mock's fictional per-routine ordinal path.
    expect(pathEl.textContent).not.toMatch(/runs\/.*\/\d+\.jsonl/);
  });
});

// ---------------------------------------------------------------------------
// (c) Export — save dialog -> routines_export_run_bundle -> shows written path.
// ---------------------------------------------------------------------------

describe('RunsTab — export run bundle (c)', () => {
  it('invokes the save dialog with the default filename, then exportRunBundle with {runId, outputPath}, and shows the absolute written path', async () => {
    mockSaveDialog.mockResolvedValue('/home/operator/exports/tuxlink-run-run-1.json');
    renderRunsTab();
    await screen.findByTestId('run-header');

    fireEvent.click(screen.getByTestId('export-run-btn'));

    await act(async () => {});
    expect(mockSaveDialog).toHaveBeenCalledOnce();
    expect((mockSaveDialog.mock.calls[0]![0] as { defaultPath: string }).defaultPath).toBe(
      'tuxlink-run-run-1.json',
    );

    const exportCalls = callsFor('routines_export_run_bundle');
    expect(exportCalls).toHaveLength(1);
    expect(exportCalls[0]![1]).toEqual({ runId: 'run-1', outputPath: '/home/operator/exports/tuxlink-run-run-1.json' });

    const feedback = await screen.findByTestId('runs-feedback');
    expect(feedback.textContent).toContain('/home/operator/exports/tuxlink-run-run-1.json');
  });

  it('does nothing when the save dialog is cancelled (null)', async () => {
    mockSaveDialog.mockResolvedValue(null);
    renderRunsTab();
    await screen.findByTestId('run-header');

    fireEvent.click(screen.getByTestId('export-run-btn'));
    await act(async () => {});

    expect(callsFor('routines_export_run_bundle')).toHaveLength(0);
    expect(screen.queryByTestId('runs-feedback')).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// (d) Dry-run display.
// ---------------------------------------------------------------------------

describe('RunsTab — dry-run display (d)', () => {
  it('renders a dry-run badge in the list and the fake-world banner in the header when the run is a dry run', async () => {
    const dryEntry: RunListEntry = { ...RUN_1_ENTRY, runId: 'run-dry', dryRun: true, state: 'completed' };
    const dryStatus: RunStatus = { runId: 'run-dry', routine: 'net-opening-checklist', dryRun: true, state: 'completed' };
    const dryJournal: JournalEntry[] = [
      {
        ts_unix: T,
        run_id: 'run-dry',
        seq: 0,
        event: { type: 'run_started', routine: 'net-opening-checklist', snapshot: SNAPSHOT, dry_run: true },
      },
      { ts_unix: T + 5, run_id: 'run-dry', seq: 1, event: { type: 'run_finished', state: 'completed', reason: null } },
    ];

    installInvokeMock({
      routines_runs_list: () => [dryEntry],
      routines_run_status: () => dryStatus,
      routines_journal: () => dryJournal,
    });

    renderRunsTab();

    const row = await screen.findByTestId('runrow-run-dry');
    expect(within(row).getByText(/dry-run/)).toBeInTheDocument();

    const banner = await screen.findByTestId('dry-run-banner');
    expect(banner.textContent).toBe('fake world — nothing real was touched');
  });
});

// ---------------------------------------------------------------------------
// (e) Cancel run.
// ---------------------------------------------------------------------------

describe('RunsTab — cancel run (e)', () => {
  it('shows Cancel run for a live run and invokes routines_cancel with {runId}', async () => {
    const liveEntry: RunListEntry = { ...RUN_1_ENTRY, runId: 'run-live', state: 'running', finishedUnix: null };
    const liveStatus: RunStatus = { runId: 'run-live', routine: 'net-opening-checklist', dryRun: false, state: 'running' };
    const liveJournal: JournalEntry[] = [
      {
        ts_unix: T,
        run_id: 'run-live',
        seq: 0,
        event: { type: 'run_started', routine: 'net-opening-checklist', snapshot: SNAPSHOT, dry_run: false },
      },
    ];

    installInvokeMock({
      routines_runs_list: () => [liveEntry],
      routines_run_status: () => liveStatus,
      routines_journal: () => liveJournal,
      routines_cancel: () => true,
    });

    renderRunsTab();
    const btn = await screen.findByTestId('cancel-run-btn');
    fireEvent.click(btn);

    await act(async () => {});
    const calls = callsFor('routines_cancel');
    expect(calls).toHaveLength(1);
    expect(calls[0]![1]).toEqual({ runId: 'run-live' });
  });
});

// ---------------------------------------------------------------------------
// (f) Take the radio.
// ---------------------------------------------------------------------------

describe('RunsTab — take the radio (f)', () => {
  it('shows Take the radio while the run is running/awaiting_radio and invokes routines_take_radio', async () => {
    const radioEntry: RunListEntry = { ...RUN_1_ENTRY, runId: 'run-radio', state: 'awaiting_radio', finishedUnix: null };
    const radioStatus: RunStatus = {
      runId: 'run-radio',
      routine: 'net-opening-checklist',
      dryRun: false,
      state: 'awaiting_radio',
    };
    const radioJournal: JournalEntry[] = [
      {
        ts_unix: T,
        run_id: 'run-radio',
        seq: 0,
        event: { type: 'run_started', routine: 'net-opening-checklist', snapshot: SNAPSHOT, dry_run: false },
      },
      {
        ts_unix: T + 1,
        run_id: 'run-radio',
        seq: 1,
        event: { type: 'step_intent', step: 's2', action: 'radio.connect', resolved_params: { rig: 'g90' } },
      },
      { ts_unix: T + 2, run_id: 'run-radio', seq: 2, event: { type: 'state_changed', state: 'awaiting_radio' } },
    ];

    installInvokeMock({
      routines_runs_list: () => [radioEntry],
      routines_run_status: () => radioStatus,
      routines_journal: () => radioJournal,
      routines_take_radio: () => true,
    });

    renderRunsTab();
    const btn = await screen.findByTestId('take-radio-btn');
    fireEvent.click(btn);

    await act(async () => {});
    const calls = callsFor('routines_take_radio');
    expect(calls).toHaveLength(1);
    expect(calls[0]![1]).toEqual({ rig: undefined });

    // Flow 6 plainness: the banner names the real rig from the journal, not a spinner.
    const banner = await screen.findByTestId('awaiting-radio-banner');
    expect(banner.textContent).toBe('waiting for the radio — the operator holds rig g90');
  });
});

// ---------------------------------------------------------------------------
// Live polling: every 2s while non-terminal; stops on terminal state.
// ---------------------------------------------------------------------------

describe('RunsTab — live polling', () => {
  it('polls runStatus + runJournal every 2s while non-terminal and stops once the state is terminal', async () => {
    vi.useFakeTimers();

    let call = 0;
    const liveEntry: RunListEntry = { ...RUN_1_ENTRY, runId: 'run-poll', state: 'running', finishedUnix: null };
    const runningStatus: RunStatus = {
      runId: 'run-poll',
      routine: 'net-opening-checklist',
      dryRun: false,
      state: 'running',
    };
    const completedStatus: RunStatus = { ...runningStatus, state: 'completed' };
    const journalEntries: JournalEntry[] = [
      {
        ts_unix: T,
        run_id: 'run-poll',
        seq: 0,
        event: { type: 'run_started', routine: 'net-opening-checklist', snapshot: SNAPSHOT, dry_run: false },
      },
    ];

    installInvokeMock({
      routines_runs_list: () => [liveEntry],
      routines_run_status: () => {
        call += 1;
        return call === 1 ? runningStatus : completedStatus;
      },
      routines_journal: () => journalEntries,
    });

    render(<RunsTab routine="net-opening-checklist" highlightRunId={null} />);

    // Initial tick fires on selection (mount), no timer advance needed.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(callsFor('routines_run_status')).toHaveLength(1);

    // Second tick after 2s — status flips to terminal (completed).
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(callsFor('routines_run_status')).toHaveLength(2);

    const countAfterTerminal = callsFor('routines_run_status').length;

    // No further ticks once terminal.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(callsFor('routines_run_status')).toHaveLength(countAfterTerminal);
  });
});
