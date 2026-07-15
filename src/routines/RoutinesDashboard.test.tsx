/**
 * Tests for RoutinesDashboard.tsx — the fleet-ops dashboard (routines plan-5
 * Task 8, `.superpowers/sdd/task-8-brief.md`).
 *
 * `@tauri-apps/api/core` is mocked at module scope, keyed by command name
 * (feedback_vitest_invoke_mock_cleanup_call — the no-arg teardown call must
 * be inert). `@tauri-apps/api/event`'s `listen` is mocked to resolve an
 * unlisten fn; this suite never needs to dispatch a live event (`useRoutines`
 * and the dashboard's own run-progress listener are exercised by their own
 * unit tests), so the handler is never invoked here.
 *
 * Seed fleet: 5 routines exercising every status-chip precedence level, plus
 * a 6th (`unacked-routine`) isolated to the TX-mode `auto·no-ack` case so it
 * doesn't perturb the chip-precedence assertions.
 *   - morning-sweep   (automatic, enabled, no live run)               -> chip 'enabled'
 *                      missed=100000 -> '100k+' badge; lastRefusal verbatim
 *                      terminal run COMPLETED -> "✓ ok" last result
 *   - net-opening     (attended, enabled, LIVE awaiting_consent run)  -> chip 'awaiting consent'
 *   - wwv-capture     (automatic, enabled, LIVE running run)          -> chip 'running'
 *                      its action never transmits -> TX mode '—'
 *   - deployment-poll (automatic, DISABLED, no live run)              -> chip 'disabled'
 *                      lastSkip present -> skip row renders despite disabled
 *                      terminal run FAILED -> "✕ failed" + verbatim step_err cause
 *   - ics309          (automatic, enabled, 2 error findings)          -> chip 'draft · 2 errors'
 *   - unacked-routine (automatic, enabled, AUTO_TX_UNACKED finding)   -> TX mode 'auto·no-ack'
 *
 * fleetFindings seeds one SCHEDULE_COLLISION finding for assertion (e).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, within, waitFor } from '@testing-library/react';
import type { RoutineDef, RunListEntry, Finding, ScheduleStatus, RoutineSummary, ActionInfo } from './routinesApi';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

const { mockListen } = vi.hoisted(() => ({ mockListen: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }));

import { RoutinesDashboard } from './RoutinesDashboard';

const SUMMARIES: RoutineSummary[] = [
  {
    routine: 'morning-sweep',
    transmitMode: 'automatic',
    enabled: true,
    triggers: [{ type: 'schedule', every: '2h', align: 'hour' }],
  },
  {
    routine: 'net-opening',
    transmitMode: 'attended',
    enabled: true,
    triggers: [{ type: 'manual' }],
  },
  {
    routine: 'wwv-capture',
    transmitMode: 'automatic',
    enabled: true,
    triggers: [{ type: 'schedule', every: '30m' }],
  },
  {
    routine: 'deployment-poll',
    transmitMode: 'automatic',
    enabled: false,
    triggers: [{ type: 'schedule', every: '30m', if_missed: 'run_once_on_launch' }],
  },
  {
    routine: 'ics309',
    transmitMode: 'automatic',
    enabled: true,
    triggers: [{ type: 'schedule', every: '2h', align: 'hour' }],
  },
  {
    routine: 'unacked-routine',
    transmitMode: 'automatic',
    enabled: true,
    triggers: [{ type: 'manual' }],
  },
];

const SCHEDULE_STATUS: ScheduleStatus[] = [
  {
    routine: 'morning-sweep',
    missed: 100_000,
    lastFireUnix: 1000,
    lastRefusal: { at: 900, reason: 'Refused: G90 already held by "wwv-capture"' },
    lastSkip: null,
  },
  { routine: 'net-opening', missed: 0, lastFireUnix: 0, lastRefusal: null, lastSkip: null },
  { routine: 'wwv-capture', missed: 0, lastFireUnix: 0, lastRefusal: null, lastSkip: null },
  {
    routine: 'deployment-poll',
    missed: 1,
    lastFireUnix: 500,
    lastRefusal: null,
    lastSkip: { at: 480, reason: 'skipped: previous run of "deployment-poll" still active' },
  },
  { routine: 'ics309', missed: 0, lastFireUnix: 0, lastRefusal: null, lastSkip: null },
  { routine: 'unacked-routine', missed: 0, lastFireUnix: 0, lastRefusal: null, lastSkip: null },
];

const DEFS: Record<string, RoutineDef> = {
  'morning-sweep': {
    routine: 'morning-sweep',
    schema_version: 1,
    transmit_mode: 'automatic',
    triggers: [{ type: 'schedule', every: '2h', align: 'hour' }],
    tracks: [
      { name: 't1', steps: [{ id: 's1', action: 'radio.connect' }, { id: 's2', action: 'mailbox.check' }] },
      { name: 't2', steps: [{ id: 's3', action: 'radio.connect' }] },
    ],
  },
  'net-opening': {
    routine: 'net-opening',
    schema_version: 1,
    transmit_mode: 'attended',
    triggers: [{ type: 'manual' }],
    tracks: [{ name: 't1', steps: [{ id: 's1', action: 'radio.connect' }] }],
  },
  'wwv-capture': {
    routine: 'wwv-capture',
    schema_version: 1,
    transmit_mode: 'automatic',
    triggers: [{ type: 'schedule', every: '30m' }],
    tracks: [{ name: 't1', steps: [{ id: 's1', action: 'wwv.decode' }] }],
  },
  'deployment-poll': {
    routine: 'deployment-poll',
    schema_version: 1,
    transmit_mode: 'automatic',
    triggers: [{ type: 'schedule', every: '30m', if_missed: 'run_once_on_launch' }],
    tracks: [{ name: 't1', steps: [{ id: 's1', action: 'radio.connect' }] }],
  },
  ics309: {
    routine: 'ics309',
    schema_version: 1,
    transmit_mode: 'automatic',
    triggers: [{ type: 'schedule', every: '2h', align: 'hour' }],
    tracks: [{ name: 't1', steps: [{ id: 's1', action: 'ics309.build' }] }],
  },
  'unacked-routine': {
    routine: 'unacked-routine',
    schema_version: 1,
    transmit_mode: 'automatic',
    triggers: [{ type: 'manual' }],
    tracks: [{ name: 't1', steps: [{ id: 's1', action: 'radio.connect' }] }],
  },
};

const FINDINGS_BY_ROUTINE: Record<string, Finding[]> = {
  'morning-sweep': [],
  'net-opening': [],
  'wwv-capture': [],
  'deployment-poll': [],
  ics309: [
    { code: 'MISSING_STEP', severity: 'error', routine: 'ics309', message: 'track "t1" ends without an End step' },
    { code: 'BAD_TRIGGER', severity: 'error', routine: 'ics309', message: 'trigger window is malformed' },
  ],
  'unacked-routine': [
    {
      code: 'AUTO_TX_UNACKED',
      severity: 'error',
      routine: 'unacked-routine',
      message: 'transmits under automatic control but has no recorded acknowledgment',
    },
  ],
};

const ACTIONS: ActionInfo[] = [
  { name: 'radio.connect', needsRadio: true, transmits: true, needsInternet: false },
  { name: 'mailbox.check', needsRadio: false, transmits: false, needsInternet: true },
  { name: 'wwv.decode', needsRadio: true, transmits: false, needsInternet: false },
  { name: 'ics309.build', needsRadio: false, transmits: false, needsInternet: false },
];

const RUNS: RunListEntry[] = [
  { runId: 'run-wwv-live', routine: 'wwv-capture', dryRun: false, startedUnix: 500, state: 'running', finishedUnix: null },
  { runId: 'run-net-live', routine: 'net-opening', dryRun: false, startedUnix: 1000, state: 'awaiting_consent', finishedUnix: null },
  { runId: 'run-sweep-ok', routine: 'morning-sweep', dryRun: false, startedUnix: 700, state: 'completed', finishedUnix: 800 },
  { runId: 'run-poll-fail', routine: 'deployment-poll', dryRun: false, startedUnix: 600, state: 'failed', finishedUnix: 650 },
];

const FLEET_FINDINGS: Finding[] = [
  {
    code: 'SCHEDULE_COLLISION',
    severity: 'warning',
    routine: 'morning-sweep',
    message:
      '"morning-sweep" and "wwv-capture" both take rig G90 at 16:00Z; runs will serialize (first-come-first-served).',
  },
];

const JOURNAL_POLL_FAIL = [
  {
    ts_unix: 640,
    run_id: 'run-poll-fail',
    seq: 1,
    event: { type: 'run_started' as const, routine: 'deployment-poll', snapshot: {}, dry_run: false },
  },
  {
    ts_unix: 645,
    run_id: 'run-poll-fail',
    seq: 2,
    event: {
      type: 'step_err' as const,
      step: 's1',
      error: {
        kind: 'action' as const,
        detail: { action: 'radio.connect', cause: 'VARA HF: DISCONNECTED — link timeout 90 s' },
      },
    },
  },
  {
    ts_unix: 650,
    run_id: 'run-poll-fail',
    seq: 3,
    event: { type: 'run_finished' as const, state: 'failed' as const, reason: null },
  },
];

type InvokeOverrides = Partial<Record<string, (args: unknown) => unknown>>;

function installInvokeMock(overrides: InvokeOverrides = {}) {
  mockInvoke.mockImplementation((cmd?: string, args?: unknown) => {
    // Teardown pitfall: invoke mocks get called with NO args at cleanup.
    if (cmd === undefined) return Promise.resolve();
    if (cmd in overrides) return Promise.resolve(overrides[cmd]!(args));
    switch (cmd) {
      case 'routines_list':
        return Promise.resolve(SUMMARIES);
      case 'routines_missed_fires':
        return Promise.resolve(SCHEDULE_STATUS);
      case 'routines_next_fires':
        return Promise.resolve([{ routine: 'morning-sweep', at: 1_752_400_000 }]);
      case 'routines_fleet_check':
        return Promise.resolve(FLEET_FINDINGS);
      case 'routines_actions_list':
        return Promise.resolve(ACTIONS);
      case 'routines_validate': {
        const name = (args as { name: string }).name;
        return Promise.resolve(FINDINGS_BY_ROUTINE[name] ?? []);
      }
      case 'routines_get': {
        const name = (args as { name: string }).name;
        return Promise.resolve(DEFS[name]);
      }
      case 'routines_runs_list':
        return Promise.resolve(RUNS);
      case 'routines_journal': {
        const runId = (args as { runId: string }).runId;
        return Promise.resolve(runId === 'run-poll-fail' ? JOURNAL_POLL_FAIL : []);
      }
      case 'routines_run':
        return Promise.resolve('new-run-id');
      case 'routines_cancel':
        return Promise.resolve(true);
      case 'routines_set_enabled':
        return Promise.resolve({ routine: (args as { name: string }).name, enabled: true, blocked: false, findings: [] });
      case 'routines_delete':
        return Promise.resolve(undefined);
      case 'routines_save':
        return Promise.resolve({ routine: 'x', findings: [], blocked: false });
      default:
        return Promise.resolve([]);
    }
  });
}

function rowFor(routine: string): HTMLElement {
  const cell = screen.getByText(routine);
  const row = cell.closest('tr');
  if (!row) throw new Error(`no <tr> ancestor for routine "${routine}"`);
  return row;
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockListen.mockReset();
  mockListen.mockImplementation(() => Promise.resolve(vi.fn()));
  installInvokeMock();
});

function renderDashboard(props: Partial<Parameters<typeof RoutinesDashboard>[0]> = {}) {
  const onOpenDesigner = props.onOpenDesigner ?? vi.fn();
  const onNewRoutine = props.onNewRoutine ?? vi.fn();
  const utils = render(<RoutinesDashboard onOpenDesigner={onOpenDesigner} onNewRoutine={onNewRoutine} />);
  return { ...utils, onOpenDesigner, onNewRoutine };
}

describe('RoutinesDashboard — status chip precedence (a)', () => {
  it('renders every precedence level with the right chip text', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');

    expect(within(rowFor('net-opening')).getByText('awaiting consent')).toBeInTheDocument();
    expect(within(rowFor('wwv-capture')).getByText('running')).toBeInTheDocument();
    expect(within(rowFor('ics309')).getByText('draft · 2 errors')).toBeInTheDocument();
    expect(within(rowFor('morning-sweep')).getByText('enabled')).toBeInTheDocument();
    expect(within(rowFor('deployment-poll')).getByText('disabled')).toBeInTheDocument();
  });
});

describe('RoutinesDashboard — missed-fire badge (b)', () => {
  it('clamps a 100000 missed count to "100k+"', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    expect(within(rowFor('morning-sweep')).getByText(/⚠ missed 100k\+ fire\(s\)/)).toBeInTheDocument();
  });
});

describe('RoutinesDashboard — refusal/skip visibility (c, flow 7)', () => {
  it('renders the last-fire-refused reason VERBATIM', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    expect(
      within(rowFor('morning-sweep')).getByText(
        'last fire refused: Refused: G90 already held by "wwv-capture"',
      ),
    ).toBeInTheDocument();
  });

  it('renders the last-fire-skipped reason VERBATIM even on a DISABLED routine', async () => {
    renderDashboard();
    await screen.findByText('deployment-poll');
    expect(within(rowFor('deployment-poll')).getByText('disabled')).toBeInTheDocument();
    expect(
      within(rowFor('deployment-poll')).getByText(
        'last fire skipped: skipped: previous run of "deployment-poll" still active',
      ),
    ).toBeInTheDocument();
  });
});

describe('RoutinesDashboard — Run action (d)', () => {
  it('invokes routines_run with the routine name when ▶ Run is clicked', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    fireEvent.click(within(rowFor('morning-sweep')).getByRole('button', { name: 'Run morning-sweep' }));

    await waitFor(() => {
      const call = mockInvoke.mock.calls.find((c) => c[0] === 'routines_run');
      expect(call?.[1]).toEqual({ name: 'morning-sweep', args: {} });
    });
  });

  it('disabled routines still show a Run button (not gated by enabled=false)', async () => {
    renderDashboard();
    await screen.findByText('deployment-poll');
    expect(within(rowFor('deployment-poll')).getByRole('button', { name: 'Run deployment-poll' })).toBeInTheDocument();
  });

  it('a LIVE routine shows Stop instead of Run', async () => {
    renderDashboard();
    await screen.findByText('wwv-capture');
    expect(within(rowFor('wwv-capture')).getByRole('button', { name: 'Stop wwv-capture' })).toBeInTheDocument();
    expect(within(rowFor('wwv-capture')).queryByRole('button', { name: 'Run wwv-capture' })).not.toBeInTheDocument();
  });

  it('a DRAFT (error findings) routine shows Edit instead of Run', async () => {
    renderDashboard();
    await screen.findByText('ics309');
    expect(within(rowFor('ics309')).getByRole('button', { name: 'Edit ics309' })).toBeInTheDocument();
    expect(within(rowFor('ics309')).queryByRole('button', { name: 'Run ics309' })).not.toBeInTheDocument();
  });
});

describe('RoutinesDashboard — fleet bar (e)', () => {
  it('shows the fleet finding code (mono) and message VERBATIM', async () => {
    renderDashboard();
    await screen.findByText('SCHEDULE_COLLISION');
    expect(
      screen.getByText(
        '"morning-sweep" and "wwv-capture" both take rig G90 at 16:00Z; runs will serialize (first-come-first-served).',
        { exact: false },
      ),
    ).toBeInTheDocument();
  });

  it('is hidden when there are no fleet findings', async () => {
    installInvokeMock({ routines_fleet_check: () => [] });
    renderDashboard();
    await screen.findByText('morning-sweep');
    expect(screen.queryByText('SCHEDULE_COLLISION')).not.toBeInTheDocument();
  });
});

describe('RoutinesDashboard — double-click opens the designer (f)', () => {
  it('calls onOpenDesigner(routine) with no tab on row double-click when the last result is NOT failed', async () => {
    const { onOpenDesigner } = renderDashboard();
    await screen.findByText('morning-sweep');
    // morning-sweep's newest terminal run is COMPLETED (RUNS fixture) —
    // default designer landing, no tab argument.
    fireEvent.doubleClick(rowFor('morning-sweep'));
    expect(onOpenDesigner).toHaveBeenCalledWith('morning-sweep', undefined);
  });

  // Final whole-branch review, Fix 3: `onOpenDesigner(routine, tab?)` was
  // threaded through RoutinesSurface but never called with a `tab` — flow 3
  // "investigate a failed run" now lands directly on the Runs tab.
  it('calls onOpenDesigner(routine, "runs") on row double-click when the last result is FAILED', async () => {
    const { onOpenDesigner } = renderDashboard();
    await screen.findByText('deployment-poll');
    // deployment-poll's newest terminal run is FAILED (run-poll-fail, RUNS fixture).
    fireEvent.doubleClick(rowFor('deployment-poll'));
    expect(onOpenDesigner).toHaveBeenCalledWith('deployment-poll', 'runs');
  });
});

describe('RoutinesDashboard — arbiter refusal plainness (g, flow 6)', () => {
  it('shows the rejected run UiError message VERBATIM in a dismissible strip', async () => {
    installInvokeMock({
      routines_run: () => {
        throw { kind: 'Rejected', detail: 'Refused: consent required before an automatic transmit' };
      },
    });
    renderDashboard();
    await screen.findByText('deployment-poll');
    fireEvent.click(within(rowFor('deployment-poll')).getByRole('button', { name: 'Run deployment-poll' }));

    const strip = await screen.findByRole('alert');
    expect(within(strip).getByText('Refused: consent required before an automatic transmit')).toBeInTheDocument();

    fireEvent.click(within(strip).getByRole('button', { name: 'Dismiss refusal' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});

describe('RoutinesDashboard — Last result column', () => {
  it('shows the newest terminal run as "✓ ok" with its finish time', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    expect(within(rowFor('morning-sweep')).getByText('✓ ok')).toBeInTheDocument();
  });

  it('shows "✕ failed" and fetches the verbatim step_err cause from the journal', async () => {
    renderDashboard();
    await screen.findByText('deployment-poll');
    expect(within(rowFor('deployment-poll')).getByText('✕ failed')).toBeInTheDocument();
    await within(rowFor('deployment-poll')).findByText('VARA HF: DISCONNECTED — link timeout 90 s');

    const journalCalls = mockInvoke.mock.calls.filter((c) => c[0] === 'routines_journal');
    expect(journalCalls).toHaveLength(1); // fetched once, cached — not re-fetched on re-render
  });

  it('shows "never run" when a routine has no run history', async () => {
    installInvokeMock({ routines_runs_list: () => [] });
    renderDashboard();
    await screen.findByText('ics309');
    expect(within(rowFor('ics309')).getByText('never run')).toBeInTheDocument();
  });
});

describe('RoutinesDashboard — TX mode column', () => {
  it('renders "—" for a routine whose action never transmits', async () => {
    renderDashboard();
    await screen.findByText('wwv-capture');
    expect(
      within(rowFor('wwv-capture')).getByText('—', { selector: '.txmode' }),
    ).toBeInTheDocument();
  });

  it('renders "automatic" and "attended" for transmitting routines', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    expect(within(rowFor('morning-sweep')).getByText('automatic')).toBeInTheDocument();
    expect(within(rowFor('net-opening')).getByText('attended')).toBeInTheDocument();
  });

  it('renders "auto·no-ack" when AUTO_TX_UNACKED is present', async () => {
    renderDashboard();
    await screen.findByText('unacked-routine');
    expect(within(rowFor('unacked-routine')).getByText('auto·no-ack')).toBeInTheDocument();
  });
});

describe('RoutinesDashboard — routine meta line', () => {
  it('shows «N tracks · M steps» from the fetched RoutineDef', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    expect(within(rowFor('morning-sweep')).getByText('2 tracks · 3 steps')).toBeInTheDocument();
  });
});

describe('RoutinesDashboard — header actions', () => {
  it('calls onNewRoutine from the header button', async () => {
    const { onNewRoutine } = renderDashboard();
    await screen.findByText('morning-sweep');
    fireEvent.click(screen.getByRole('button', { name: '＋ New Routine' }));
    expect(onNewRoutine).toHaveBeenCalledTimes(1);
  });

  it('opens the Import JSON dialog', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    fireEvent.click(screen.getByRole('button', { name: 'Import JSON…' }));
    expect(screen.getByTestId('import-json-textarea')).toBeInTheDocument();
  });
});

describe('RoutinesDashboard — row menu: enable/disable, delete', () => {
  it('Enable/Disable invokes setEnabled with the flipped enabled value', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    fireEvent.click(within(rowFor('morning-sweep')).getByRole('button', { name: 'Actions for morning-sweep' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Disable' }));

    await waitFor(() => {
      const call = mockInvoke.mock.calls.find((c) => c[0] === 'routines_set_enabled');
      expect(call?.[1]).toEqual({ name: 'morning-sweep', enabled: false });
    });
  });

  it('surfaces setEnabled blocked findings in the fleet-bar area, styled as errors', async () => {
    installInvokeMock({
      routines_set_enabled: () => ({
        routine: 'morning-sweep',
        enabled: true,
        blocked: true,
        findings: [{ code: 'AUTO_TX_UNACKED', severity: 'error', routine: 'morning-sweep', message: 'no ack on record' }],
      }),
    });
    renderDashboard();
    await screen.findByText('morning-sweep');
    fireEvent.click(within(rowFor('morning-sweep')).getByRole('button', { name: 'Actions for morning-sweep' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Disable' }));

    await screen.findByText('ENABLE BLOCKED');
    expect(screen.getByText('no ack on record', { exact: false })).toBeInTheDocument();
  });

  it('Delete requires a confirm click before calling deleteRoutine', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    fireEvent.click(within(rowFor('morning-sweep')).getByRole('button', { name: 'Actions for morning-sweep' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Delete' }));
    expect(mockInvoke.mock.calls.some((c) => c[0] === 'routines_delete')).toBe(false);

    fireEvent.click(screen.getByRole('menuitem', { name: 'Confirm delete' }));
    await waitFor(() => {
      const call = mockInvoke.mock.calls.find((c) => c[0] === 'routines_delete');
      expect(call?.[1]).toEqual({ name: 'morning-sweep' });
    });
  });
});

describe('RoutinesDashboard — ImportJsonDialog', () => {
  it('shows a parse error inline for invalid JSON', async () => {
    renderDashboard();
    await screen.findByText('morning-sweep');
    fireEvent.click(screen.getByRole('button', { name: 'Import JSON…' }));
    fireEvent.change(screen.getByTestId('import-json-textarea'), { target: { value: 'not json' } });
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));

    expect(await screen.findByText(/./, { selector: '.import-error' })).toBeInTheDocument();
    expect(mockInvoke.mock.calls.some((c) => c[0] === 'routines_save')).toBe(false);
  });

  it('saves a valid draft even when the backend returns error findings (save never blocks)', async () => {
    installInvokeMock({
      routines_save: () => ({
        routine: 'imported',
        findings: [{ code: 'MISSING_STEP', severity: 'error', routine: 'imported', message: 'no End step' }],
        blocked: true,
      }),
    });
    renderDashboard();
    await screen.findByText('morning-sweep');
    fireEvent.click(screen.getByRole('button', { name: 'Import JSON…' }));

    const def = {
      routine: 'imported',
      schema_version: 1,
      transmit_mode: 'manual',
      triggers: [{ type: 'manual' }],
      tracks: [],
    };
    fireEvent.change(screen.getByTestId('import-json-textarea'), { target: { value: JSON.stringify(def) } });
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));

    await waitFor(() => {
      expect(mockInvoke.mock.calls.some((c) => c[0] === 'routines_save')).toBe(true);
    });
    expect(await screen.findByText('MISSING_STEP')).toBeInTheDocument();
    expect(screen.getByText('no End step', { exact: false })).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Empty library (tuxlink-3awm9 WebKitGTK smoke): a fresh install rendered a
// bare void under the column headers — the calm empty state must say what
// this surface is and point at ＋ New Routine / Import JSON….
// ---------------------------------------------------------------------------

describe('RoutinesDashboard — empty library', () => {
  it('renders the empty-state copy instead of the ops table when routines_list is empty', async () => {
    installInvokeMock({
      routines_list: () => [],
      routines_runs_list: () => [],
      routines_missed_fires: () => [],
      routines_next_fires: () => [],
      routines_fleet_check: () => [],
    });
    renderDashboard();
    expect(await screen.findByTestId('routines-dashboard-empty')).toHaveTextContent(/No routines yet/);
    expect(screen.queryByText('Routine', { selector: 'th' })).not.toBeInTheDocument();
  });

  it('does NOT claim an empty library while the first refresh is still in flight (Codex P2)', async () => {
    // Every routines read hangs: summaries stays its initial [] but loaded
    // stays false — the table shell renders, never the "No routines yet" copy.
    mockInvoke.mockImplementation((cmd?: string) => {
      if (cmd === undefined) return Promise.resolve();
      return new Promise(() => undefined);
    });
    renderDashboard();
    expect(await screen.findByText('Routine', { selector: 'th' })).toBeInTheDocument();
    expect(screen.queryByTestId('routines-dashboard-empty')).not.toBeInTheDocument();
  });
});
