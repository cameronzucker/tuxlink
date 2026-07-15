/**
 * Tests for RoutineDesigner.tsx — the routine designer shell (routines
 * plan-5 Task 9, `.superpowers/sdd/task-9-brief.md`).
 *
 * `@tauri-apps/api/core` is mocked at module scope, keyed by command name
 * (feedback_vitest_invoke_mock_cleanup_call — the no-arg teardown call must
 * be inert). Fake timers drive the 400ms validation debounce
 * (vi.useFakeTimers + advanceTimersByTimeAsync, mirroring
 * useRoutines.test.tsx's proven pattern).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import type { RoutineDef, Finding } from '../routinesApi';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

import { RoutineDesigner } from './RoutineDesigner';

const EXISTING_DEF: RoutineDef = {
  routine: 'deployment-poll',
  schema_version: 1,
  transmit_mode: 'attended',
  triggers: [{ type: 'manual' }],
  tracks: [{ name: 'track-1', steps: [{ id: 's1', action: 'radio.connect' }] }],
};

type InvokeOverrides = Partial<Record<string, (args: unknown) => unknown>>;

function installInvokeMock(overrides: InvokeOverrides = {}) {
  mockInvoke.mockImplementation((cmd?: string, args?: unknown) => {
    // Teardown pitfall: invoke mocks get called with NO args at cleanup.
    if (cmd === undefined) return Promise.resolve();
    if (cmd in overrides) return Promise.resolve(overrides[cmd]!(args));
    switch (cmd) {
      case 'routines_get':
        return Promise.resolve(EXISTING_DEF);
      case 'routines_validate_draft':
        return Promise.resolve([]);
      case 'routines_save':
        return Promise.resolve({ routine: 'deployment-poll', findings: [], blocked: false });
      case 'routines_dry_run':
        return Promise.resolve({ runId: 'run-dry-1', findings: [] });
      case 'routines_actions_list':
        return Promise.resolve([]);
      default:
        return Promise.resolve(undefined);
    }
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  installInvokeMock();
});

afterEach(() => {
  vi.useRealTimers();
});

function renderDesigner(props: Partial<Parameters<typeof RoutineDesigner>[0]> = {}) {
  const onBack = props.onBack ?? vi.fn();
  const onTabChange = props.onTabChange ?? vi.fn();
  const utils = render(
    <RoutineDesigner
      routine={props.routine ?? 'deployment-poll'}
      tab={props.tab ?? 'design'}
      onBack={onBack}
      onTabChange={onTabChange}
    />,
  );
  return { ...utils, onBack, onTabChange };
}

function callsFor(cmd: string) {
  return mockInvoke.mock.calls.filter((c) => c[0] === cmd);
}

describe('RoutineDesigner — load + header (a)', () => {
  it('loads an existing routine and renders its name + all three tabs', async () => {
    renderDesigner({ routine: 'deployment-poll' });
    await screen.findByText('deployment-poll');
    expect(screen.getByRole('button', { name: 'Design' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Runs' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Settings' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '← Routines' })).toBeInTheDocument();

    const getCalls = callsFor('routines_get');
    expect(getCalls).toHaveLength(1);
    expect(getCalls[0]?.[1]).toEqual({ name: 'deployment-poll' });
  });

  it('a fresh/new draft (empty routine name) renders an editable name field and skips routines_get', async () => {
    renderDesigner({ routine: '' });
    await screen.findByTestId('designer-name-input');
    expect(callsFor('routines_get')).toHaveLength(0);
  });

  it('clicking ← Routines calls onBack', async () => {
    const { onBack } = renderDesigner();
    await screen.findByText('deployment-poll');
    fireEvent.click(screen.getByRole('button', { name: '← Routines' }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it('clicking a tab calls onTabChange with that tab', async () => {
    const { onTabChange } = renderDesigner();
    await screen.findByText('deployment-poll');
    fireEvent.click(screen.getByRole('button', { name: 'Runs' }));
    expect(onTabChange).toHaveBeenCalledWith('runs');
  });
});

describe('RoutineDesigner — always-on validation bar (b, flow 2)', () => {
  it('editing the draft marks it dirty and, after the 400ms debounce, invokes routines_validate_draft with the current draft and renders findings verbatim', async () => {
    vi.useFakeTimers();
    render(
      <RoutineDesigner routine="deployment-poll" tab="design" onBack={vi.fn()} onTabChange={vi.fn()} />,
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(400); // flush the initial-load validate
    });
    mockInvoke.mockClear();

    installInvokeMock({
      routines_validate_draft: () => [
        {
          code: 'MULTIPLE_SCHEDULES',
          severity: 'warning',
          routine: 'deployment-poll',
          message: '2 schedules declared; one cadence per routine.',
        } satisfies Finding,
      ],
    });

    expect(screen.queryByTestId('unsaved-dot')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('add-track-btn'));
    expect(screen.getByTestId('unsaved-dot')).toBeInTheDocument();

    // Not yet — inside the debounce window.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(200);
    });
    expect(callsFor('routines_validate_draft')).toHaveLength(0);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(200);
    });

    const calls = callsFor('routines_validate_draft');
    expect(calls).toHaveLength(1);
    const sentDef = JSON.parse((calls[0]![1] as { defJson: string }).defJson) as RoutineDef;
    expect(sentDef.tracks).toHaveLength(2); // the added track is in the sent body

    expect(screen.getByText('MULTIPLE_SCHEDULES')).toBeInTheDocument();
    expect(
      screen.getByText('2 schedules declared; one cadence per routine.', { exact: false }),
    ).toBeInTheDocument();
    expect(screen.getByText(/⚠ 1 warning/)).toBeInTheDocument();
  });

  it('a Rejected parse failure from validateDraft renders its verbatim message as a single error line', async () => {
    vi.useFakeTimers();
    installInvokeMock({
      routines_validate_draft: () => {
        throw { kind: 'Rejected', detail: 'invalid def: missing tracks' };
      },
    });
    render(
      <RoutineDesigner routine="deployment-poll" tab="design" onBack={vi.fn()} onTabChange={vi.fn()} />,
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0); // flush routines_get's microtask
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(400); // flush the 400ms validate debounce
    });
    expect(screen.getByText('invalid def: missing tracks')).toBeInTheDocument();
  });
});

describe('RoutineDesigner — Save never blocks (c)', () => {
  it('invokes routines_save with the draft body and clears the unsaved dot even when blocked:true, with no modal/exception', async () => {
    installInvokeMock({
      routines_save: () => ({
        routine: 'deployment-poll',
        blocked: true,
        findings: [
          {
            code: 'AUTO_TX_UNACKED',
            severity: 'error',
            routine: 'deployment-poll',
            message: 'transmits under automatic control but has no recorded acknowledgment',
          } satisfies Finding,
        ],
      }),
    });
    renderDesigner();
    await screen.findByText('deployment-poll');
    fireEvent.click(screen.getByTestId('add-track-btn'));
    expect(screen.getByTestId('unsaved-dot')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await screen.findByText('AUTO_TX_UNACKED');
    expect(screen.queryByTestId('unsaved-dot')).not.toBeInTheDocument();
    expect(screen.getByText(/✓ 1 error/)).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    const saveCalls = callsFor('routines_save');
    expect(saveCalls).toHaveLength(1);
    const sentDef = JSON.parse((saveCalls[0]![1] as { defJson: string }).defJson) as RoutineDef;
    expect(sentDef.tracks).toHaveLength(2);
  });
});

describe('RoutineDesigner — Dry-run implicit save (d, flow 5)', () => {
  it('invokes routines_save then routines_dry_run and switches to the runs tab with the runId highlighted', async () => {
    const onTabChange = vi.fn();
    renderDesigner({ onTabChange });
    await screen.findByText('deployment-poll');

    fireEvent.click(screen.getByRole('button', { name: 'Dry-run' }));

    await act(async () => {});
    const saveCalls = callsFor('routines_save');
    const dryRunCalls = callsFor('routines_dry_run');
    expect(saveCalls).toHaveLength(1);
    expect(dryRunCalls).toHaveLength(1);
    expect(dryRunCalls[0]?.[1]).toEqual({ name: 'deployment-poll', args: {} });

    // routines_save happened before routines_dry_run.
    const saveIdx = mockInvoke.mock.calls.findIndex((c) => c[0] === 'routines_save');
    const dryRunIdx = mockInvoke.mock.calls.findIndex((c) => c[0] === 'routines_dry_run');
    expect(saveIdx).toBeLessThan(dryRunIdx);

    expect(onTabChange).toHaveBeenCalledWith('runs');
  });
});

describe('RoutineDesigner — flow-2 authoring trace through the real seam (Task 11 fix)', () => {
  it('new draft → insert action → trailing ＋ → insert branch → err arm ＋ → insert action INTO the arm: renders in the err fan row (not unplaced) and its id lands in branch.else', async () => {
    installInvokeMock({
      routines_actions_list: () => [
        { name: 'radio.connect', needsRadio: true, needsInternet: false, transmits: true },
      ],
      routines_list: () => [],
    });
    renderDesigner({ routine: '' });

    // Fresh draft: the lone dangling ＋ off the trigger head. Wait for the
    // palette's action item (the actions fetch is async).
    await screen.findByTestId('palette-item-radio.connect');

    // 1. Arm the head ＋, insert radio.connect → s1.
    fireEvent.click(screen.getByTestId('insert-trigger-0'));
    fireEvent.click(screen.getByTestId('palette-item-radio.connect'));
    expect(screen.getByTestId('node-s1')).toBeInTheDocument();

    // 2. The trailing append ＋ after s1 exists (Gap B) — arm it, insert a
    //    branch → s2 with empty arms.
    fireEvent.click(screen.getByTestId('insert-s1'));
    fireEvent.click(screen.getByTestId('palette-control-branch'));
    expect(screen.getByTestId('node-s2')).toBeInTheDocument();

    // 3. The err arm ＋ is visible on the canvas-authored branch (Gap A) —
    //    arm it, insert radio.connect INTO the arm → s3.
    fireEvent.click(screen.getByTestId('insert-s2-err'));
    fireEvent.click(screen.getByTestId('palette-item-radio.connect'));

    // 4. The inserted step renders as a placed node in the err fan row —
    //    NOT in the unplaced row.
    const s3 = screen.getByTestId('node-s3');
    expect(s3).not.toHaveTextContent(/unplaced/i);
    expect(screen.queryByTestId('unplaced-row-0')).not.toBeInTheDocument();
    expect(s3.closest('.path')).not.toBeNull(); // fan row, not the main flow

    // 5. …and its id landed in the branch's else list: verify through the
    //    draft itself via the Export JSON dialog.
    fireEvent.click(screen.getByRole('button', { name: 'Export JSON' }));
    const textarea = (await screen.findByTestId('export-json-textarea')) as HTMLTextAreaElement;
    const exported = JSON.parse(textarea.value) as RoutineDef;
    const branch = exported.tracks[0]!.steps.find((s) => s.id === 's2') as {
      control: 'branch';
      then: string[];
      else: string[];
    };
    expect(branch.else).toEqual(['s3']);
    expect(branch.then).toEqual([]);
    // Storage splice landed adjacently: s1, s2 (branch), s3.
    expect(exported.tracks[0]!.steps.map((s) => s.id)).toEqual(['s1', 's2', 's3']);
  });
});

describe('RoutineDesigner — Export JSON dialog', () => {
  it('opens a read-only dialog with JSON.stringify(draft, null, 2) and a Copy button', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    renderDesigner();
    await screen.findByText('deployment-poll');
    fireEvent.click(screen.getByRole('button', { name: 'Export JSON' }));

    const dialog = await screen.findByRole('dialog');
    const textarea = screen.getByTestId('export-json-textarea') as HTMLTextAreaElement;
    expect(JSON.parse(textarea.value)).toEqual(EXISTING_DEF);

    fireEvent.click(screen.getByRole('button', { name: /Copy/ }));
    expect(writeText).toHaveBeenCalledWith(JSON.stringify(EXISTING_DEF, null, 2));

    expect(dialog).toBeInTheDocument();
    // No fs write — only clipboard.
    expect(callsFor('routines_export_run_bundle')).toHaveLength(0);
  });
});
