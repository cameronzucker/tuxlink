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
import { render, screen, fireEvent, act, within } from '@testing-library/react';
import type { RoutineDef, Finding } from '../routinesApi';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

// The Settings tab (Task 12's SettingsTab) reads the active callsign off
// useStatusData for the Acknowledge button's label — mocked directly
// (StationRail.test.tsx's established pattern) rather than standing up a
// real QueryClientProvider ancestor this shell doesn't otherwise need.
vi.mock('../../shell/useStatus', () => ({ useStatusData: () => ({ callsign: '' }) }));

import { RoutineDesigner, slugifyRoutineName } from './RoutineDesigner';

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
        return Promise.resolve({ revision: 'rev-1', def: EXISTING_DEF });
      case 'routines_validate_draft':
        return Promise.resolve([]);
      case 'routines_save':
        return Promise.resolve({ routine: 'deployment-poll', revision: 'rev-2', findings: [], blocked: false });
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
      {...props}
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
  // tuxlink-7ewvq items 5+8: 'Runs' was ambiguous (Run? Run history?) — the
  // tab reads 'History'; the Settings tab is GONE (its sections render inline
  // below the canvas in the Design view instead of behind a third tab).
  it('loads an existing routine and renders its name + the Design/History tabs (no Settings tab)', async () => {
    renderDesigner({ routine: 'deployment-poll' });
    await screen.findByText('deployment-poll');
    expect(screen.getByRole('button', { name: 'Design' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'History' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Runs' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Settings' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: '← Routines' })).toBeInTheDocument();

    const getCalls = callsFor('routines_get');
    expect(getCalls).toHaveLength(1);
    expect(getCalls[0]?.[1]).toEqual({ name: 'deployment-poll' });
  });

  // tuxlink-7ewvq item 6: the bare transmit_mode value ('attended') rendered
  // as an unexplained chip next to the title. It reads as a labeled chip with
  // a plain-language tooltip now.
  it('renders the transmit-mode chip labeled and with an explanatory tooltip', async () => {
    renderDesigner({ routine: 'deployment-poll' });
    await screen.findByText('deployment-poll');
    const chip = screen.getByTestId('transmit-mode-chip');
    expect(chip).toHaveTextContent('TX: attended');
    expect(chip.getAttribute('title')).toMatch(/transmit/i);
  });

  it('a fresh/new draft (empty routine name) renders an editable name field and skips routines_get', async () => {
    renderDesigner({ routine: '' });
    await screen.findByTestId('designer-name-input');
    expect(callsFor('routines_get')).toHaveLength(0);
  });

  // bd tuxlink-iizmk item 7: the operator types a HUMAN name ("Test Routine
  // 1"); the kebab-case wire id is DERIVED by slugification and stored in the
  // draft — Save must never see the human text and reject it.
  it('typing a human name keeps the display text, derives the wire id into the draft, and shows it as fine print', async () => {
    renderDesigner({ routine: '' });
    const input = (await screen.findByTestId('designer-name-input')) as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'Test Routine 1' } });

    expect(input.value).toBe('Test Routine 1');
    expect(screen.getByTestId('derived-name')).toHaveTextContent('saves as test-routine-1');

    // The DRAFT carries the wire id — proof via the Export routine dialog.
    fireEvent.click(screen.getByRole('button', { name: 'Export routine' }));
    const textarea = (await screen.findByTestId('export-json-textarea')) as HTMLTextAreaElement;
    expect((JSON.parse(textarea.value) as RoutineDef).routine).toBe('test-routine-1');
  });

  it('the derived-id fine print stays hidden when the typed text already IS the wire id', async () => {
    renderDesigner({ routine: '' });
    const input = await screen.findByTestId('designer-name-input');
    fireEvent.change(input, { target: { value: 'test-routine-1' } });
    expect(screen.queryByTestId('derived-name')).not.toBeInTheDocument();
  });

  it('an all-symbols name derives nothing: the draft name stays empty and no fine print renders', async () => {
    renderDesigner({ routine: '' });
    const input = await screen.findByTestId('designer-name-input');
    fireEvent.change(input, { target: { value: '!!! ***' } });
    expect(screen.queryByTestId('derived-name')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Export routine' }));
    const textarea = (await screen.findByTestId('export-json-textarea')) as HTMLTextAreaElement;
    expect((JSON.parse(textarea.value) as RoutineDef).routine).toBe('');
  });

  // tuxlink-iizmk round 2: the header carries three always-visible fact
  // chips (transmit mode, schedule summary, enabled state), each a button
  // that jumps to its settings section under the canvas.
  it('renders the schedule fact-chip with a compact cadence summary, or "manual" without a schedule', async () => {
    installInvokeMock({
      routines_get: () => ({
        revision: 'rev-1',
        def: {
          ...EXISTING_DEF,
          triggers: [{ type: 'schedule', every: '30m', window: '07:00-09:00', if_missed: 'skip' }],
        },
      }),
    });
    renderDesigner({ routine: 'deployment-poll' });
    await screen.findByText('deployment-poll');
    expect(screen.getByTestId('schedule-chip')).toHaveTextContent('every 30m · 07:00-09:00');
  });

  it('renders "manual" on the schedule chip for a manual-only routine', async () => {
    renderDesigner({ routine: 'deployment-poll' });
    await screen.findByText('deployment-poll');
    expect(screen.getByTestId('schedule-chip')).toHaveTextContent('manual');
  });

  it('renders the enabled fact-chip from routines_list and clicking it from History lands on Design', async () => {
    installInvokeMock({
      routines_list: () => [
        { routine: 'deployment-poll', transmitMode: 'attended', enabled: true, triggers: [{ type: 'manual' }] },
      ],
      routines_runs_list: () => [],
    });
    const onTabChange = vi.fn();
    renderDesigner({ routine: 'deployment-poll', tab: 'runs', onTabChange });
    await screen.findByText('deployment-poll');
    const chip = await screen.findByTestId('enabled-chip');
    await vi.waitFor(() => expect(chip).toHaveTextContent('enabled'));

    fireEvent.click(chip);
    expect(onTabChange).toHaveBeenCalledWith('design');
  });

  it('clicking the schedule chip on the Design view scrolls its settings section into view', async () => {
    const scrollSpy = vi.fn();
    (window.Element.prototype as unknown as { scrollIntoView: () => void }).scrollIntoView = scrollSpy;
    const onTabChange = vi.fn();
    renderDesigner({ routine: 'deployment-poll', tab: 'design', onTabChange });
    await screen.findByTestId('settings-schedule-section');

    fireEvent.click(screen.getByTestId('schedule-chip'));
    // Already on Design: no tab flip, just the deferred scroll.
    expect(onTabChange).not.toHaveBeenCalled();
    await vi.waitFor(() => expect(scrollSpy).toHaveBeenCalled());
  });

  it('clicking ← Routines calls onBack', async () => {
    const { onBack } = renderDesigner();
    await screen.findByText('deployment-poll');
    fireEvent.click(screen.getByRole('button', { name: '← Routines' }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it('clicking the History tab calls onTabChange with the wire value "runs"', async () => {
    const { onTabChange } = renderDesigner();
    await screen.findByText('deployment-poll');
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
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

    // Scoped to the valbar — the inline settings sections (tuxlink-7ewvq
    // item 8) render the same finding codes in the Design view too.
    const valbar = within(screen.getByTestId('valbar'));
    expect(valbar.getByText('MULTIPLE_SCHEDULES')).toBeInTheDocument();
    expect(
      valbar.getByText('2 schedules declared; one cadence per routine.', { exact: false }),
    ).toBeInTheDocument();
    expect(valbar.getByText(/⚠ 1 warning/)).toBeInTheDocument();
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

describe('RoutineDesigner — revision CAS (spec D7, adrev round 2)', () => {
  it('a loaded routine saves with the expectedRevision from routines_get, and updates it from the save result', async () => {
    installInvokeMock();
    renderDesigner();
    await screen.findByText('deployment-poll');
    fireEvent.click(screen.getByTestId('add-track-btn'));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await vi.waitFor(() => {
      expect(callsFor('routines_save')).toHaveLength(1);
    });
    expect(callsFor('routines_save')[0]?.[1]).toMatchObject({ expectedRevision: 'rev-1' });
  });

  it('a token-seeded designer saves with the revision the continuity token carried', async () => {
    installInvokeMock();
    renderDesigner({
      initialDraft: EXISTING_DEF,
      initialRevision: 'rev-token-7',
    });
    await screen.findByText('deployment-poll');
    // Token-seeded: no fetch happened, so the ONLY possible source of the
    // revision is the token (adrev round 2 P1 — pop-out/dock-back must not
    // shed the CAS protection).
    expect(callsFor('routines_get')).toHaveLength(0);
    fireEvent.click(screen.getByTestId('add-track-btn'));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await vi.waitFor(() => {
      expect(callsFor('routines_save')).toHaveLength(1);
    });
    expect(callsFor('routines_save')[0]?.[1]).toMatchObject({ expectedRevision: 'rev-token-7' });
  });
});

describe('RoutineDesigner — Save never blocks (c)', () => {
  it('invokes routines_save with the draft body and clears the unsaved dot even when blocked:true, with no modal/exception', async () => {
    const finding = {
      code: 'AUTO_TX_UNACKED',
      severity: 'error',
      routine: 'deployment-poll',
      message: 'transmits under automatic control but has no recorded acknowledgment',
    } satisfies Finding;
    installInvokeMock({
      routines_save: () => ({
        routine: 'deployment-poll',
        blocked: true,
        findings: [finding],
      }),
      // The 400ms debounced revalidate fires after the save on a slow runner;
      // returning the same finding keeps the valbar stable for the
      // assertions below instead of racing them against an empty [] refresh.
      routines_validate_draft: () => [finding],
    });
    renderDesigner();
    await screen.findByText('deployment-poll');
    fireEvent.click(screen.getByTestId('add-track-btn'));
    expect(screen.getByTestId('unsaved-dot')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    // Scoped to the valbar (the inline settings can surface findings too).
    // 3s timeout: the Design view mounts the inline settings sections now
    // (tuxlink-7ewvq item 8), and the default 1s is marginal on a loaded Pi.
    await within(screen.getByTestId('valbar')).findByText('AUTO_TX_UNACKED', {}, { timeout: 3000 });
    expect(screen.queryByTestId('unsaved-dot')).not.toBeInTheDocument();
    expect(within(screen.getByTestId('valbar')).getByText(/✓ 1 error/)).toBeInTheDocument();
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
    fireEvent.click(screen.getByRole('button', { name: 'Export routine' }));
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

  it('MID-ARM insert: the ＋ between two arm nodes lands the step between them in the fan row (not unplaced), id ordered in the arm list', async () => {
    installInvokeMock({
      routines_get: () =>
        ({
          revision: 'rev-1',
          def: {
          routine: 'deployment-poll',
          schema_version: 1,
          transmit_mode: 'attended',
          triggers: [{ type: 'manual' }],
          tracks: [
            {
              name: 'track-1',
              steps: [
                { id: 's1', action: 'radio.connect' },
                { id: 's2', control: 'branch', on: 's1.connected', then: [], else: ['s3', 's4'] },
                { id: 's3', action: 'radio.connect' },
                { id: 's4', control: 'end', failed: true },
              ],
            },
          ],
          } satisfies RoutineDef,
        }),
      routines_actions_list: () => [
        { name: 'radio.connect', needsRadio: true, needsInternet: false, transmits: true },
      ],
      routines_list: () => [],
    });
    renderDesigner({ routine: 'deployment-poll' });
    await screen.findByTestId('palette-item-radio.connect');

    // Arm the intra-arm ＋ between s3 and s4 (the err arm), insert an action.
    fireEvent.click(screen.getByTestId('insert-s3'));
    fireEvent.click(screen.getByTestId('palette-item-radio.connect'));

    // The new step (s5) renders between s3 and s4 IN the fan row — no
    // unplaced row, no unplaced marker.
    const s5 = screen.getByTestId('node-s5');
    expect(s5).not.toHaveTextContent(/unplaced/i);
    expect(screen.queryByTestId('unplaced-row-0')).not.toBeInTheDocument();
    const path = s5.closest('.path') as HTMLElement;
    expect(path).not.toBeNull();
    const rowIds = Array.from(path.querySelectorAll('[data-testid^="node-"]')).map((el) =>
      el.getAttribute('data-testid'),
    );
    expect(rowIds).toEqual(['node-s3', 'node-s5', 'node-s4']);

    // And the id is ordered correctly in the arm list + storage.
    fireEvent.click(screen.getByRole('button', { name: 'Export routine' }));
    const textarea = (await screen.findByTestId('export-json-textarea')) as HTMLTextAreaElement;
    const exported = JSON.parse(textarea.value) as RoutineDef;
    const branch = exported.tracks[0]!.steps.find((s) => s.id === 's2') as {
      control: 'branch';
      then: string[];
      else: string[];
    };
    expect(branch.else).toEqual(['s3', 's5', 's4']);
    expect(exported.tracks[0]!.steps.map((s) => s.id)).toEqual(['s1', 's2', 's3', 's5', 's4']);
  });

  it('ARM-LEAD insert: the ＋ between the branch and a populated arm\'s first node prepends INTO that arm (not unplaced)', async () => {
    installInvokeMock({
      routines_get: () =>
        ({
          revision: 'rev-1',
          def: {
          routine: 'deployment-poll',
          schema_version: 1,
          transmit_mode: 'attended',
          triggers: [{ type: 'manual' }],
          tracks: [
            {
              name: 'track-1',
              steps: [
                { id: 's1', action: 'radio.connect' },
                { id: 's2', control: 'branch', on: 's1.connected', then: [], else: ['s3'] },
                { id: 's3', action: 'radio.connect' },
              ],
            },
          ],
          } satisfies RoutineDef,
        }),
      routines_actions_list: () => [
        { name: 'radio.connect', needsRadio: true, needsInternet: false, transmits: true },
      ],
      routines_list: () => [],
    });
    renderDesigner({ routine: 'deployment-poll' });
    await screen.findByTestId('palette-item-radio.connect');

    // Arm the err LEAD ＋ (between the branch and s3), insert an action.
    fireEvent.click(screen.getByTestId('insert-s2-err'));
    fireEvent.click(screen.getByTestId('palette-item-radio.connect'));

    // The new step (s4) renders BEFORE s3 in the err fan row — no unplaced
    // row, no unplaced marker.
    const s4 = screen.getByTestId('node-s4');
    expect(s4).not.toHaveTextContent(/unplaced/i);
    expect(screen.queryByTestId('unplaced-row-0')).not.toBeInTheDocument();
    const path = s4.closest('.path') as HTMLElement;
    expect(path).not.toBeNull();
    const rowIds = Array.from(path.querySelectorAll('[data-testid^="node-"]')).map((el) =>
      el.getAttribute('data-testid'),
    );
    expect(rowIds).toEqual(['node-s4', 'node-s3']);

    // And the id landed at the FRONT of the else list, storage adjacent to
    // the branch.
    fireEvent.click(screen.getByRole('button', { name: 'Export routine' }));
    const textarea = (await screen.findByTestId('export-json-textarea')) as HTMLTextAreaElement;
    const exported = JSON.parse(textarea.value) as RoutineDef;
    const branch = exported.tracks[0]!.steps.find((s) => s.id === 's2') as {
      control: 'branch';
      then: string[];
      else: string[];
    };
    expect(branch.else).toEqual(['s4', 's3']);
    expect(exported.tracks[0]!.steps.map((s) => s.id)).toEqual(['s1', 's2', 's4', 's3']);
  });
});

describe('RoutineDesigner — inline settings wiring (Task 12 seam, tuxlink-7ewvq item 8)', () => {
  it('the Design view renders the settings sections inline below the canvas, and editing a setting flows through onChange -> updateSettings -> the draft', async () => {
    installInvokeMock({
      routines_actions_list: () => [],
      routines_list: () => [{ routine: 'deployment-poll', transmitMode: 'attended', enabled: false, triggers: [{ type: 'manual' }] }],
    });
    renderDesigner({ tab: 'design' });
    await screen.findByTestId('settings-tab');
    // The canvas and the settings sections co-exist in the same view.
    expect(screen.getByTestId('canvas-tab')).toBeInTheDocument();

    // Editing on_interrupted routes through RoutineDesigner's updateDraft ->
    // defDraft.updateSettings -> marks the draft dirty, same seam every other
    // tab's edits use.
    expect(screen.queryByTestId('unsaved-dot')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('settings-interrupted-resume'));
    expect(screen.getByTestId('unsaved-dot')).toBeInTheDocument();

    // The updated field is reflected in the Export JSON dialog — proof the
    // patch actually landed in the shell's `draft`, not just a local
    // SettingsTab-only state.
    fireEvent.click(screen.getByRole('button', { name: 'Export routine' }));
    const textarea = (await screen.findByTestId('export-json-textarea')) as HTMLTextAreaElement;
    const exported = JSON.parse(textarea.value) as RoutineDef;
    expect(exported.on_interrupted).toBe('resume');
  });

  // Continuity-token compat: a stored tab === 'settings' (from before the tab
  // was removed) renders the Design view — the settings live there now.
  it('tab="settings" renders the Design view (compat for stored tokens)', async () => {
    renderDesigner({ tab: 'settings' });
    await screen.findByTestId('canvas-tab');
    expect(screen.getByTestId('settings-tab')).toBeInTheDocument();
  });
});

// tuxlink-7ewvq item 2: with no insert point chosen on the canvas, clicking a
// palette action appends it to the END of the routine — the palette is
// directly usable without the ＋-first dance.
describe('RoutineDesigner — palette append-to-end fallback', () => {
  it('an unarmed palette click appends the step to the end of the track', async () => {
    installInvokeMock({
      routines_actions_list: () => [
        { name: 'local.log', label: 'Log', description: '', needsRadio: false, transmits: false, needsInternet: false },
      ],
    });
    renderDesigner();
    await screen.findByText('deployment-poll');
    await screen.findByTestId('palette-item-local.log');

    fireEvent.click(screen.getByTestId('palette-item-local.log'));

    // Proof via Export JSON: the new step is the LAST step of track 1.
    fireEvent.click(screen.getByRole('button', { name: 'Export routine' }));
    const textarea = (await screen.findByTestId('export-json-textarea')) as HTMLTextAreaElement;
    const exported = JSON.parse(textarea.value) as RoutineDef;
    const steps = exported.tracks[0]!.steps;
    const last = steps[steps.length - 1]!;
    expect('action' in last && last.action).toBe('local.log');
  });
});

describe('slugifyRoutineName — wire-id derivation (bd tuxlink-iizmk item 7)', () => {
  it.each([
    ['Test Routine 1', 'test-routine-1'],
    ['  Morning  Sweep  ', 'morning-sweep'],
    ['snake_case_name', 'snake-case-name'],
    ['3rd Watch', 'rd-watch'], // leading non-a-z stripped until a letter
    ['123', ''], // no usable chars at all → empty, Save stays blocked
    ['---', ''],
    ["What's Up? (v2)", 'whats-up-v2'],
    ['ALL CAPS', 'all-caps'],
  ])('derives %j → %j', (input, expected) => {
    expect(slugifyRoutineName(input)).toBe(expected);
  });

  it('trims to 64 chars and never ends with a dash', () => {
    const slug = slugifyRoutineName(`${'a'.repeat(63)}-tail`);
    expect(slug.length).toBeLessThanOrEqual(64);
    expect(slug.endsWith('-')).toBe(false);
    expect(slug).toBe('a'.repeat(63)); // char 64 would be the dash — dropped
  });
});

describe('RoutineDesigner — Export routine dialog', () => {
  it('opens a read-only dialog with JSON.stringify(draft, null, 2) and a Copy button', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    renderDesigner();
    await screen.findByText('deployment-poll');
    fireEvent.click(screen.getByRole('button', { name: 'Export routine' }));

    const dialog = await screen.findByRole('dialog');
    const textarea = screen.getByTestId('export-json-textarea') as HTMLTextAreaElement;
    expect(JSON.parse(textarea.value)).toEqual(EXISTING_DEF);

    fireEvent.click(screen.getByRole('button', { name: /Copy/ }));
    expect(writeText).toHaveBeenCalledWith(JSON.stringify(EXISTING_DEF, null, 2));

    expect(dialog).toBeInTheDocument();
    // No fs write — only clipboard.
    expect(callsFor('routines_export_run_artifact')).toHaveLength(0);
  });
});
