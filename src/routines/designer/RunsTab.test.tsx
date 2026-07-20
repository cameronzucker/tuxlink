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

import { RunsTab, ganttModel, radioAwaitRig, stepListModel } from './RunsTab';

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
      case 'routines_export_run_artifact':
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

  it('attributes a parked window to the exact step named on state_changed, not the heuristic', () => {
    const snapshot = {
      tracks: [
        {
          name: 'net-control',
          steps: [
            { id: 's1', action: 'cat.apply_preset' },
            { id: 'd1', control: 'delay', delay: '+5m' },
            { id: 's2', action: 'radio.connect' },
          ],
        },
      ],
    };
    const journal: JournalEntry[] = [
      { ts_unix: T, run_id: 'run-e', seq: 0, event: { type: 'run_started', routine: 'r', snapshot, dry_run: false } },
      { ts_unix: T + 1, run_id: 'run-e', seq: 1, event: { type: 'step_intent', step: 's1', action: 'cat.apply_preset', resolved_params: {} } },
      { ts_unix: T + 2, run_id: 'run-e', seq: 2, event: { type: 'step_ok', step: 's1', output: {} } },
      // Enriched delay park: names d1. The legacy heuristic would have
      // attributed this window to s1 (most recently CLOSED step).
      { ts_unix: T + 3, run_id: 'run-e', seq: 3, event: { type: 'state_changed', state: 'waiting', step: 'd1' } },
      { ts_unix: T + 303, run_id: 'run-e', seq: 4, event: { type: 'state_changed', state: 'running', step: 'd1' } },
      { ts_unix: T + 400, run_id: 'run-e', seq: 5, event: { type: 'run_finished', state: 'completed', reason: null } },
    ];
    const model = ganttModel(journal);
    const bars = model.lanes[0]!.bars;
    const delayBar = bars.find((b) => b.kind === 'delay');
    expect(delayBar).toBeDefined();
    expect(delayBar!.stepId).toBe('d1');
    expect(delayBar!.t0).toBe(T + 3);
    expect(delayBar!.t1).toBe(T + 303);
    // Exactly one bar for the window — no duplicate heuristic attribution.
    expect(bars.filter((b) => b.kind === 'delay')).toHaveLength(1);
  });

  it('attributes an exact consent park to the named step and attaches its open intent, not the heuristic', () => {
    const snapshot = {
      tracks: [
        {
          name: 'net-control',
          steps: [
            { id: 's1', action: 'cat.apply_preset' },
            { id: 's2', action: 'radio.tx' },
          ],
        },
      ],
    };
    const journal: JournalEntry[] = [
      { ts_unix: T, run_id: 'run-c', seq: 0, event: { type: 'run_started', routine: 'r', snapshot, dry_run: false } },
      { ts_unix: T + 1, run_id: 'run-c', seq: 1, event: { type: 'step_intent', step: 's1', action: 'cat.apply_preset', resolved_params: {} } },
      { ts_unix: T + 2, run_id: 'run-c', seq: 2, event: { type: 'step_ok', step: 's1', output: {} } },
      // s2's intent opens and stays open across the park — a consent park,
      // unlike the delay-control-step case, has an open step_intent to
      // attach.
      { ts_unix: T + 3, run_id: 'run-c', seq: 3, event: { type: 'step_intent', step: 's2', action: 'radio.tx', resolved_params: { rig: 'g90' } } },
      { ts_unix: T + 4, run_id: 'run-c', seq: 4, event: { type: 'state_changed', state: 'awaiting_consent', step: 's2', rig: 'g90' } },
      { ts_unix: T + 304, run_id: 'run-c', seq: 5, event: { type: 'state_changed', state: 'running', step: 's2' } },
      { ts_unix: T + 400, run_id: 'run-c', seq: 6, event: { type: 'run_finished', state: 'completed', reason: null } },
    ];
    const model = ganttModel(journal);
    const bars = model.lanes[0]!.bars;
    const consentBar = bars.find((b) => b.kind === 'consent');
    expect(consentBar).toBeDefined();
    expect(consentBar).toMatchObject({ kind: 'consent', stepId: 's2', action: 'radio.tx', t0: T + 4, t1: T + 304 });
    expect(consentBar!.intentEntry).toBeDefined();
    expect(consentBar!.intentEntry!.event.type).toBe('step_intent');
    // Exactly one consent bar for the window — no duplicate/heuristic
    // attribution alongside the exact one.
    expect(bars.filter((b) => b.kind === 'consent')).toHaveLength(1);
  });

  it('falls back to the legacy heuristic when state_changed carries no step (old journals)', () => {
    const snapshot = {
      tracks: [
        { name: 'net-control', steps: [{ id: 's1', action: 'cat.apply_preset' }] },
      ],
    };
    const journal: JournalEntry[] = [
      { ts_unix: T, run_id: 'run-l', seq: 0, event: { type: 'run_started', routine: 'r', snapshot, dry_run: false } },
      { ts_unix: T + 1, run_id: 'run-l', seq: 1, event: { type: 'step_intent', step: 's1', action: 'cat.apply_preset', resolved_params: {} } },
      { ts_unix: T + 2, run_id: 'run-l', seq: 2, event: { type: 'step_ok', step: 's1', output: {} } },
      { ts_unix: T + 3, run_id: 'run-l', seq: 3, event: { type: 'state_changed', state: 'waiting' } },
      { ts_unix: T + 63, run_id: 'run-l', seq: 4, event: { type: 'state_changed', state: 'running' } },
      { ts_unix: T + 70, run_id: 'run-l', seq: 5, event: { type: 'run_finished', state: 'completed', reason: null } },
    ];
    const model = ganttModel(journal);
    const delayBar = model.lanes[0]!.bars.find((b) => b.kind === 'delay');
    // Legacy behavior preserved verbatim: attributed to the most recently
    // closed step.
    expect(delayBar).toBeDefined();
    expect(delayBar!.stepId).toBe('s1');
  });
});

describe('radioAwaitRig', () => {
  it('reads the rig param off the most recent step_intent (journal has no rig field on state_changed itself)', () => {
    expect(radioAwaitRig(FIXTURE_JOURNAL.slice(0, 4))).toBe('g90');
  });

  it('defaults to "default" when no step_intent carries a rig param', () => {
    expect(radioAwaitRig(FIXTURE_JOURNAL.slice(0, 2))).toBe('default');
  });

  it('prefers the rig named on a state_changed entry over the adjacent-intent heuristic', () => {
    const journal: JournalEntry[] = [
      { ts_unix: T, run_id: 'run-r', seq: 0, event: { type: 'step_intent', step: 's1', action: 'radio.connect', resolved_params: { rig: 'ft710' } } },
      { ts_unix: T + 1, run_id: 'run-r', seq: 1, event: { type: 'state_changed', state: 'awaiting_radio', step: 's1', rig: 'g90' } },
    ];
    // The exact field wins over the (different) rig on the adjacent intent.
    expect(radioAwaitRig(journal)).toBe('g90');
  });

  it('a rig-less state_changed does not shadow the intent fallback', () => {
    const journal: JournalEntry[] = [
      { ts_unix: T, run_id: 'run-r2', seq: 0, event: { type: 'step_intent', step: 's1', action: 'radio.connect', resolved_params: { rig: 'ft710' } } },
      { ts_unix: T + 1, run_id: 'run-r2', seq: 1, event: { type: 'state_changed', state: 'running', step: 's1' } },
    ];
    expect(radioAwaitRig(journal)).toBe('ft710');
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
// (c) Export — save dialog -> routines_export_run_artifact -> shows written path.
// ---------------------------------------------------------------------------

describe('RunsTab — export run artifact (c)', () => {
  it('invokes the save dialog with the default filename, then exportRunArtifact with {runId, outputPath}, and shows the absolute written path', async () => {
    mockSaveDialog.mockResolvedValue('/home/operator/exports/tuxlink-run-run-1.json');
    renderRunsTab();
    await screen.findByTestId('run-header');

    fireEvent.click(screen.getByTestId('export-run-btn'));

    await act(async () => {});
    expect(mockSaveDialog).toHaveBeenCalledOnce();
    expect((mockSaveDialog.mock.calls[0]![0] as { defaultPath: string }).defaultPath).toBe(
      'tuxlink-run-run-1.json',
    );

    const exportCalls = callsFor('routines_export_run_artifact');
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

    expect(callsFor('routines_export_run_artifact')).toHaveLength(0);
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
// Final whole-branch review, Fix 2: the left rail never refreshed after its
// one per-routine/mount `listRuns` fetch — a runFinished event now triggers
// a re-fetch so the rail's badge matches the live pane beside it.
// ---------------------------------------------------------------------------

describe('RunsTab — left rail refreshes on run-progress events (Fix 2)', () => {
  it('a runFinished event re-fetches the run list, flipping a rail badge from running to completed', async () => {
    let routinesEventHandler: ((e: { payload: unknown }) => void) | null = null;
    mockListen.mockImplementation((event: string, handler: (e: { payload: unknown }) => void) => {
      if (event === 'routines:event') routinesEventHandler = handler;
      return Promise.resolve(vi.fn());
    });

    let listResult: RunListEntry[] = [
      { ...RUN_1_ENTRY, runId: 'run-live2', state: 'running', finishedUnix: null },
    ];
    const liveJournal: JournalEntry[] = [
      {
        ts_unix: T,
        run_id: 'run-live2',
        seq: 0,
        event: { type: 'run_started', routine: 'net-opening-checklist', snapshot: SNAPSHOT, dry_run: false },
      },
    ];

    installInvokeMock({
      routines_runs_list: () => listResult,
      routines_run_status: () => ({
        runId: 'run-live2',
        routine: 'net-opening-checklist',
        dryRun: false,
        state: 'running',
      }),
      routines_journal: () => liveJournal,
    });

    renderRunsTab();
    const row = await screen.findByTestId('runrow-run-live2');
    expect(within(row).getByText('running')).toBeInTheDocument();

    const runsListCallsBefore = callsFor('routines_runs_list').length;

    // The run actually finished — the backend's next `listRuns` answer
    // reflects it; a `runFinished` event (no routine field on the wire —
    // module doc) is the nudge that tells this rail to go re-fetch.
    listResult = [{ ...RUN_1_ENTRY, runId: 'run-live2', state: 'completed', finishedUnix: T + 10 }];
    act(() => {
      routinesEventHandler?.({ payload: { kind: 'runFinished', runId: 'run-live2', state: 'completed' } });
    });

    await waitFor(() => {
      expect(within(screen.getByTestId('runrow-run-live2')).getByText(/✓ completed/)).toBeInTheDocument();
    });
    // The refresh happened via a genuine re-fetch, not incidental re-render —
    // filtered by command name (feedback_vitest_invoke_mock_cleanup_call's
    // sibling concern: assert on the real invoke call, not on rendered text
    // alone).
    expect(callsFor('routines_runs_list').length).toBeGreaterThan(runsListCallsBefore);
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

// ---------------------------------------------------------------------------
// Compact run-id labels stay distinguishable (tuxlink-3awm9 WebKitGTK smoke).
// Backend ids are `run-<unixsecs>-<NNNN>`, so a head-slice rendered EVERY
// rail row (and the detail header) as the same `run-176845…` label — the
// discriminating tail (timestamp low digits + counter) is what must survive
// the shortening.
// ---------------------------------------------------------------------------

describe('RunsTab — rail labels for realistic backend run ids', () => {
  it('renders distinct labels for two runs sharing the run-<unixsecs> prefix', async () => {
    const runA: RunListEntry = { ...RUN_1_ENTRY, runId: 'run-1768456705-0007', startedUnix: T + 60 };
    const runB: RunListEntry = { ...RUN_1_ENTRY, runId: 'run-1768456705-0008', startedUnix: T };
    installInvokeMock({
      routines_runs_list: () => [runA, runB],
      routines_run_status: () => ({
        runId: runA.runId,
        routine: 'net-opening-checklist',
        dryRun: false,
        state: 'cancelled',
      }),
      routines_journal: () => [],
    });

    renderRunsTab();

    // Both compact labels exist and differ — the tail (timestamp low digits
    // + counter) survives the shortening. findAll: the selected run's id
    // also renders in the detail header, legitimately.
    const labelsA = await screen.findAllByText('…6705-0007');
    const labelsB = await screen.findAllByText('…6705-0008');
    expect(labelsA.length).toBeGreaterThan(0);
    expect(labelsB.length).toBeGreaterThan(0);
  });
});

// ---------------------------------------------------------------------------
// stepListModel — the chronological step list (observability decree O5/O6/O7,
// wire-walk 2026-07-18). Pure-model tests over a journal carrying the two
// decree events (branch_taken, step_skipped) plus the pre-existing kinds.
// ---------------------------------------------------------------------------

describe('stepListModel (decree O5/O6/O7)', () => {
  const j = (seq: number, ts: number, event: JournalEntry['event']): JournalEntry => ({
    ts_unix: ts,
    run_id: 'run-sl',
    seq,
    event,
  });

  const DECREE_JOURNAL: JournalEntry[] = [
    j(0, T, { type: 'run_started', routine: 'probe', snapshot: SNAPSHOT, dry_run: false }),
    j(1, T, { type: 'step_intent', step: 's1', action: 'data.read', resolved_params: { source: 'grid' } }),
    j(2, T + 1, { type: 'step_ok', step: 's1', output: { grid: 'DM33wp' } }),
    j(3, T + 1, { type: 'branch_taken', step: 's2', on: 's1.grid', value: 'DM33wp', took_then: true, target: 's3' }),
    j(4, T + 1, { type: 'step_intent', step: 's3', action: 'local.log', resolved_params: { message: 'DM33wp' } }),
    j(5, T + 2, { type: 'step_err', step: 's3', error: { kind: 'action', detail: { action: 'local.log', cause: CAUSE } } }),
    j(6, T + 2, { type: 'step_skipped', step: 's4', reason: "not run: the run failed at step 's3'" }),
    j(7, T + 2, { type: 'run_finished', state: 'failed', reason: `action 'local.log' failed: ${CAUSE}` }),
  ];

  it('lists every executed step with post-resolution params and output', () => {
    const rows = stepListModel(DECREE_JOURNAL);
    const ok = rows.find((r) => r.kind === 'ok');
    expect(ok?.stepId).toBe('s1');
    expect(ok?.params).toEqual({ source: 'grid' });
    expect(ok?.output).toEqual({ grid: 'DM33wp' });
    expect(ok?.durationS).toBe(1);
  });

  it('carries the branch decision: resolved value, arm, target', () => {
    const rows = stepListModel(DECREE_JOURNAL);
    const branch = rows.find((r) => r.kind === 'branch');
    expect(branch?.stepId).toBe('s2');
    expect(branch?.branch).toEqual({ on: 's1.grid', value: 'DM33wp', tookThen: true, target: 's3' });
  });

  it('carries the verbatim failure cause and the skipped step with its reason', () => {
    const rows = stepListModel(DECREE_JOURNAL);
    const fail = rows.find((r) => r.kind === 'fail');
    expect(fail?.stepId).toBe('s3');
    expect(fail?.cause).toContain(CAUSE);
    const skipped = rows.find((r) => r.kind === 'skipped');
    expect(skipped?.stepId).toBe('s4');
    expect(skipped?.reason).toBe("not run: the run failed at step 's3'");
  });

  it('orders rows by the primary entry (a step appears where it started) and ends with the terminal row', () => {
    const rows = stepListModel(DECREE_JOURNAL);
    expect(rows.map((r) => r.kind)).toEqual(['ok', 'branch', 'fail', 'skipped', 'finished']);
    const finished = rows[rows.length - 1]!;
    expect(finished.state).toBe('failed');
    expect(finished.reason).toContain(CAUSE);
  });

  it('flushes an unclosed intent as running on a live journal', () => {
    const live = DECREE_JOURNAL.slice(0, 5); // ends after s3's intent, no result, no run_finished
    const rows = stepListModel(live);
    const running = rows.find((r) => r.kind === 'running');
    expect(running?.stepId).toBe('s3');
    expect(running?.params).toEqual({ message: 'DM33wp' });
  });

  it('renders the step list rows in the component', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'routines_runs_list')
        return Promise.resolve([
          { runId: 'run-sl', routine: 'probe', dryRun: false, startedUnix: T, state: 'failed', finishedUnix: T + 2 },
        ] satisfies RunListEntry[]);
      if (cmd === 'routines_run_status')
        return Promise.resolve({ runId: 'run-sl', routine: 'probe', dryRun: false, state: 'failed' } satisfies RunStatus);
      if (cmd === 'routines_journal') return Promise.resolve(DECREE_JOURNAL);
      return Promise.resolve(null);
    });
    mockListen.mockResolvedValue(() => {});
    render(<RunsTab routine="probe" />);
    await waitFor(() => expect(screen.getByTestId('steplist')).toBeInTheDocument());
    expect(screen.getByTestId('slrow-s4-skipped')).toHaveTextContent("not run: the run failed at step 's3'");
    expect(screen.getByTestId('slrow-s2-branch')).toHaveTextContent('then arm (s3)');
    expect(screen.getByTestId('slrow-cause-s3')).toHaveTextContent(CAUSE);
    expect(screen.getByTestId('slrow-params-s1')).toHaveTextContent('{"source":"grid"}');
    expect(screen.getByTestId('slrow-output-s1')).toHaveTextContent('DM33wp');
  });
});

// ---------------------------------------------------------------------------
// stepListModel — B4 O3/O4: call rows (child-run navigation edges), end rows,
// park-kind, finished-reason suppression. Pure-model tests over synthetic
// journals feeding stepListModel directly (same harness as the O5/O6/O7 block).
// ---------------------------------------------------------------------------

describe('stepListModel (B4 O3/O4 — call/end/park-kind)', () => {
  const j = (seq: number, ts: number, event: JournalEntry['event']): JournalEntry => ({
    ts_unix: ts,
    run_id: 'run-b4',
    seq,
    event,
  });

  it('emits a call row from call_child, routine parsed from the paired step_intent action', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'parent', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'step_intent', step: 'c1', action: 'call:sub-routine', resolved_params: { x: 1 } }),
      j(2, T + 1, { type: 'call_child', step: 'c1', child_run_id: 'run-1768456705-0042' }),
      j(3, T + 3, { type: 'step_ok', step: 'c1', output: { completed: true, run_id: 'run-1768456705-0042' } }),
      j(4, T + 3, { type: 'run_finished', state: 'completed', reason: null }),
    ];
    const rows = stepListModel(journal);
    const call = rows.find((r) => r.kind === 'call');
    expect(call).toBeDefined();
    expect(call?.stepId).toBe('c1');
    expect(call?.childRoutine).toBe('sub-routine');
    expect(call?.childRunId).toBe('run-1768456705-0042');
    // The call's own step_ok still renders (every executed step is listed).
    expect(rows.find((r) => r.kind === 'ok' && r.stepId === 'c1')).toBeDefined();
    // The call row sorts at the call's start (before its own outcome row).
    const kinds = rows.map((r) => r.kind);
    expect(kinds.indexOf('call')).toBeLessThan(kinds.indexOf('ok'));
  });

  it('falls back to the raw intent action when it is not the call:<routine> shape', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'parent', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'step_intent', step: 'c1', action: 'weird', resolved_params: {} }),
      j(2, T + 1, { type: 'call_child', step: 'c1', child_run_id: 'run-x' }),
    ];
    const rows = stepListModel(journal);
    expect(rows.find((r) => r.kind === 'call')?.childRoutine).toBe('weird');
  });

  it('emits an end row from a failed end_reached carrying failed + reason', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'r', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'end_reached', step: 'e1', failed: true, reason: 'no gateway answered' }),
      j(2, T + 1, { type: 'run_finished', state: 'failed', reason: 'no gateway answered' }),
    ];
    const rows = stepListModel(journal);
    expect(rows.find((r) => r.kind === 'end')).toMatchObject({
      stepId: 'e1',
      failed: true,
      reason: 'no gateway answered',
    });
  });

  it('emits an end row from a success end_reached with failed:false and no reason', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'r', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'end_reached', step: 'e1', failed: false }),
      j(2, T + 1, { type: 'run_finished', state: 'completed', reason: null }),
    ];
    const rows = stepListModel(journal);
    const end = rows.find((r) => r.kind === 'end');
    expect(end).toMatchObject({ stepId: 'e1', failed: false });
    expect(end?.reason).toBeUndefined();
  });

  it('carries park_kind:"write" onto the park row', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'r', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'step_intent', step: 'w1', action: 'config.set_ardop', resolved_params: {} }),
      j(2, T + 2, { type: 'state_changed', state: 'awaiting_consent', step: 'w1', park_kind: 'write' }),
      j(3, T + 3, { type: 'state_changed', state: 'running', step: 'w1' }),
      j(4, T + 4, { type: 'step_ok', step: 'w1', output: {} }),
      j(5, T + 5, { type: 'run_finished', state: 'completed', reason: null }),
    ];
    const rows = stepListModel(journal);
    expect(rows.find((r) => r.kind === 'park')?.parkKind).toBe('write');
  });

  it('carries park_kind:"transmit" onto a transmit park row', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'r', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'state_changed', state: 'awaiting_consent', step: 't1', park_kind: 'transmit' }),
    ];
    const rows = stepListModel(journal);
    expect(rows.find((r) => r.kind === 'park')?.parkKind).toBe('transmit');
  });

  it('leaves park_kind undefined for a legacy park row (no park_kind on the wire)', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'r', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'state_changed', state: 'awaiting_consent', step: 't1' }),
    ];
    const rows = stepListModel(journal);
    const park = rows.find((r) => r.kind === 'park');
    expect(park).toBeDefined();
    expect(park?.parkKind).toBeUndefined();
  });

  it('suppresses the finished-row reason when it equals the winning end-row reason', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'r', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'end_reached', step: 'e1', failed: true, reason: 'link timeout' }),
      j(2, T + 1, { type: 'run_finished', state: 'failed', reason: 'link timeout' }),
    ];
    const rows = stepListModel(journal);
    // The end row still carries it; the finished row does not repeat it.
    expect(rows.find((r) => r.kind === 'end')?.reason).toBe('link timeout');
    expect(rows.find((r) => r.kind === 'finished')?.reason).toBeUndefined();
  });

  it('keeps the finished-row reason when no end row shares it', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'r', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'run_finished', state: 'failed', reason: 'some other reason' }),
    ];
    const rows = stepListModel(journal);
    expect(rows.find((r) => r.kind === 'finished')?.reason).toBe('some other reason');
  });

  it('skips an opaque (unknown future) event without producing a row', () => {
    const journal: JournalEntry[] = [
      j(0, T, { type: 'run_started', routine: 'r', snapshot: SNAPSHOT, dry_run: false }),
      j(1, T + 1, { type: 'opaque', raw: { type: 'from_the_future', x: 1 } }),
      j(2, T + 2, { type: 'run_finished', state: 'completed', reason: null }),
    ];
    const rows = stepListModel(journal);
    expect(rows.map((r) => r.kind)).toEqual(['finished']);
  });
});

// ---------------------------------------------------------------------------
// RunsTab — B4 rendered rows + child-run navigation (O3/O4). A parent run
// whose journal carries a call_child edge, a write park, and an end row; the
// child run is a DIFFERENT routine so it is absent from the (routine-scoped)
// run list, driving the "foreign run" context strip.
// ---------------------------------------------------------------------------

describe('RunsTab — B4 History rows + child navigation (O3/O4)', () => {
  const PARENT_SNAPSHOT = {
    tracks: [
      {
        name: 'main',
        steps: [
          { id: 'c1', control: 'call', routine: 'sub-routine' },
          { id: 'w1', action: 'config.set_ardop' },
          { id: 'e1', control: 'end' },
        ],
      },
    ],
  };

  const PARENT_JOURNAL: JournalEntry[] = [
    { ts_unix: T, run_id: 'run-parent', seq: 0, event: { type: 'run_started', routine: 'net-opening-checklist', snapshot: PARENT_SNAPSHOT, dry_run: false } },
    { ts_unix: T + 1, run_id: 'run-parent', seq: 1, event: { type: 'step_intent', step: 'c1', action: 'call:sub-routine', resolved_params: {} } },
    { ts_unix: T + 1, run_id: 'run-parent', seq: 2, event: { type: 'call_child', step: 'c1', child_run_id: 'run-1768456705-0099' } },
    { ts_unix: T + 2, run_id: 'run-parent', seq: 3, event: { type: 'step_ok', step: 'c1', output: { completed: true, run_id: 'run-1768456705-0099' } } },
    { ts_unix: T + 3, run_id: 'run-parent', seq: 4, event: { type: 'step_intent', step: 'w1', action: 'config.set_ardop', resolved_params: { drive_level: 80 } } },
    { ts_unix: T + 4, run_id: 'run-parent', seq: 5, event: { type: 'state_changed', state: 'awaiting_consent', step: 'w1', park_kind: 'write' } },
    { ts_unix: T + 5, run_id: 'run-parent', seq: 6, event: { type: 'state_changed', state: 'running', step: 'w1' } },
    { ts_unix: T + 6, run_id: 'run-parent', seq: 7, event: { type: 'step_ok', step: 'w1', output: { field: 'drive_level', old: 0, new: 80 } } },
    { ts_unix: T + 7, run_id: 'run-parent', seq: 8, event: { type: 'end_reached', step: 'e1', failed: false, reason: 'net complete' } },
    { ts_unix: T + 8, run_id: 'run-parent', seq: 9, event: { type: 'run_finished', state: 'completed', reason: 'net complete' } },
  ];

  const CHILD_JOURNAL: JournalEntry[] = [
    { ts_unix: T + 1, run_id: 'run-1768456705-0099', seq: 0, event: { type: 'run_started', routine: 'sub-routine', snapshot: { tracks: [{ name: 's', steps: [{ id: 'x1', action: 'data.read' }] }] }, dry_run: false } },
    { ts_unix: T + 1, run_id: 'run-1768456705-0099', seq: 1, event: { type: 'step_intent', step: 'x1', action: 'data.read', resolved_params: { source: 'grid' } } },
    { ts_unix: T + 2, run_id: 'run-1768456705-0099', seq: 2, event: { type: 'step_ok', step: 'x1', output: { grid: 'DM33wp' } } },
    { ts_unix: T + 2, run_id: 'run-1768456705-0099', seq: 3, event: { type: 'run_finished', state: 'completed', reason: null } },
  ];

  const PARENT_ENTRY: RunListEntry = {
    runId: 'run-parent',
    routine: 'net-opening-checklist',
    dryRun: false,
    startedUnix: T,
    state: 'completed',
    finishedUnix: T + 8,
  };

  function installNavMock() {
    installInvokeMock({
      routines_runs_list: () => [PARENT_ENTRY],
      routines_run_status: (args) => {
        const id = (args as { runId: string }).runId;
        return id === 'run-1768456705-0099'
          ? { runId: id, routine: 'sub-routine', dryRun: false, state: 'completed' }
          : { runId: 'run-parent', routine: 'net-opening-checklist', dryRun: false, state: 'completed' };
      },
      routines_journal: (args) => {
        const id = (args as { runId: string }).runId;
        return id === 'run-1768456705-0099' ? CHILD_JOURNAL : PARENT_JOURNAL;
      },
    });
  }

  it('renders the call row, the write-park annotation, and the end row; the finished row does not repeat the end reason', async () => {
    installNavMock();
    renderRunsTab();
    await screen.findByTestId('steplist');

    // Call row: call:<routine> + short child id, and it is clickable.
    const callRow = screen.getByTestId('slrow-c1-call');
    expect(callRow.textContent).toContain('call:sub-routine');
    expect(within(callRow).getByTestId('slrow-call-link-run-1768456705-0099')).toBeInTheDocument();

    // Write park annotation.
    expect(screen.getByTestId('slrow-w1-park')).toHaveTextContent('awaiting consent (config write)');

    // End row.
    expect(screen.getByTestId('slrow-e1-end')).toHaveTextContent('ended at e1: complete, net complete');

    // Finished row shows the state but not the (duplicated) end reason.
    const finishedRow = screen.getByTestId('slrow-finished-finished');
    expect(finishedRow).toHaveTextContent('Completed');
    expect(finishedRow.textContent).not.toContain('net complete');
  });

  it('clicking the call link navigates to the child run and shows the context strip, and the back link returns', async () => {
    installNavMock();
    renderRunsTab();
    await screen.findByTestId('steplist');

    // No strip while viewing the parent (it is in the run list).
    expect(screen.queryByTestId('run-context-strip')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('slrow-call-link-run-1768456705-0099'));

    // The child run is foreign (a different routine, absent from the rail) —
    // the context strip names the child routine and offers a back link.
    const strip = await screen.findByTestId('run-context-strip');
    await waitFor(() => expect(strip.textContent).toContain('Viewing a run of sub-routine'));
    expect(strip.textContent).toContain('back to run');

    // The header now reflects the child run.
    await waitFor(() =>
      expect(screen.getByTestId('run-header').textContent).toContain('…6705-0099'),
    );

    // Back link returns to the parent; strip disappears, parent re-selected.
    fireEvent.click(screen.getByTestId('run-context-back'));
    await waitFor(() => expect(screen.queryByTestId('run-context-strip')).not.toBeInTheDocument());
    await waitFor(() => expect(screen.getByTestId('runrow-run-parent')).toHaveClass('sel'));
  });
});
