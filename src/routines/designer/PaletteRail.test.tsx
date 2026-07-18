/**
 * Tests for PaletteRail.tsx (routines plan-5 Task 11,
 * `.superpowers/sdd/task-11-brief.md`). PaletteRail fetches its LIBRARY
 * group's saved-routine list itself (`listRoutines()`), so `@tauri-apps/api/
 * core` is mocked at module scope, keyed by command name
 * (feedback_vitest_invoke_mock_cleanup_call — the no-arg teardown call must
 * be inert), mirroring RoutineDesigner.test.tsx's proven pattern.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import type { ActionInfo, RoutineDef, RoutineSummary, Step } from '../routinesApi';
import type { ArmedInsertPosition } from './CanvasTab';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

import { PaletteRail } from './PaletteRail';

const LIBRARY: RoutineSummary[] = [
  { routine: 'oregon-gateways', transmitMode: 'attended', enabled: true, triggers: [{ type: 'manual' }] },
];

function installInvokeMock(routines: RoutineSummary[] = LIBRARY) {
  mockInvoke.mockImplementation((cmd?: string) => {
    // Teardown pitfall: invoke mocks get called with NO args at cleanup.
    if (cmd === undefined) return Promise.resolve();
    if (cmd === 'routines_list') return Promise.resolve(routines);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  installInvokeMock();
});

const ACTIONS: ActionInfo[] = [
  { name: 'radio.connect', label: '', description: '', needsRadio: true, needsInternet: false, transmits: true },
  { name: 'local.notify', label: '', description: '', needsRadio: false, needsInternet: false, transmits: false },
  // A made-up action absent from any hardcoded name list — the RADIO/
  // INTERNET/LOCAL grouping must derive purely from its flags.
  { name: 'zzz.custom', label: '', description: '', needsRadio: false, needsInternet: true, transmits: false },
];

const EMPTY_DEF: RoutineDef = {
  routine: 'r',
  schema_version: 1,
  transmit_mode: 'attended',
  triggers: [{ type: 'manual' }],
  tracks: [{ name: 'track-1', steps: [] }],
};

function renderPalette(overrides: Partial<Parameters<typeof PaletteRail>[0]> = {}) {
  const onInsert = overrides.onInsert ?? vi.fn();
  const utils = render(
    <PaletteRail
      def={overrides.def ?? EMPTY_DEF}
      actions={overrides.actions ?? ACTIONS}
      armedInsert={overrides.armedInsert ?? null}
      onInsert={onInsert}
    />,
  );
  return { ...utils, onInsert };
}

describe('PaletteRail — grouping (proves no hardcoded action-name list)', () => {
  it('groups an unknown action name purely from its capability flags: zzz.custom (needsInternet) lands under INTERNET', async () => {
    renderPalette();
    const netGroup = await screen.findByText('INTERNET');
    const netSection = netGroup.parentElement as HTMLElement;
    expect(within(netSection).getByTestId('palette-item-zzz.custom')).toBeInTheDocument();
  });

  it('groups radio.connect under RADIO and local.notify under LOCAL', () => {
    renderPalette();
    expect(screen.getByText('RADIO')).toBeInTheDocument();
    expect(screen.getByTestId('palette-item-radio.connect')).toBeInTheDocument();
    expect(screen.getByText('LOCAL')).toBeInTheDocument();
    expect(screen.getByTestId('palette-item-local.notify')).toBeInTheDocument();
  });

  it('renders the static CONTROL FLOW group with all five control kinds, including retry', () => {
    renderPalette();
    expect(screen.getByText('CONTROL FLOW')).toBeInTheDocument();
    expect(screen.getByTestId('palette-control-branch')).toBeInTheDocument();
    expect(screen.getByTestId('palette-control-delay')).toBeInTheDocument();
    expect(screen.getByTestId('palette-control-retry')).toBeInTheDocument();
    expect(screen.getByTestId('palette-control-call')).toBeInTheDocument();
    expect(screen.getByTestId('palette-control-end')).toBeInTheDocument();
  });

  it('renders the LIBRARY group from listRoutines()', async () => {
    renderPalette();
    await screen.findByTestId('palette-library-oregon-gateways');
    expect(screen.getByText('LIBRARY')).toBeInTheDocument();
  });
});

describe('PaletteRail — filter', () => {
  it('narrows items to those matching the filter text (case-insensitive substring)', () => {
    renderPalette();
    expect(screen.getByTestId('palette-item-radio.connect')).toBeInTheDocument();
    expect(screen.getByTestId('palette-item-local.notify')).toBeInTheDocument();

    fireEvent.change(screen.getByTestId('palette-filter'), { target: { value: 'radio' } });

    expect(screen.getByTestId('palette-item-radio.connect')).toBeInTheDocument();
    expect(screen.queryByTestId('palette-item-local.notify')).not.toBeInTheDocument();
    expect(screen.queryByText('LOCAL')).not.toBeInTheDocument(); // the emptied group header also disappears
  });
});

describe('PaletteRail — click-with-armed-insert', () => {
  // tuxlink-7ewvq item 2: the palette used to render disabled (opacity .45,
  // unreadable, inert) until a ＋ was clicked on the canvas — it read as
  // broken. Items are ALWAYS live now: an unarmed click still builds the step
  // and hands it to onInsert (the owner appends it to the end of the track).
  it('an action item is enabled and calls onInsert even while nothing is armed', () => {
    const { onInsert } = renderPalette({ armedInsert: null, def: EMPTY_DEF });
    const btn = screen.getByTestId('palette-item-radio.connect');
    expect(btn).not.toBeDisabled();
    fireEvent.click(btn);
    expect(onInsert).toHaveBeenCalledWith({ id: 's1', action: 'radio.connect', params: {} });
  });

  it('the unarmed hint explains both paths in plain language (no "armed" jargon)', () => {
    renderPalette({ armedInsert: null });
    const hint = screen.getByTestId('palette-hint');
    expect(hint.textContent).toMatch(/add it to the end/i);
    expect(hint.textContent).toMatch(/＋ on the canvas/);
    expect(hint.textContent).not.toMatch(/arm/i);
  });

  it('the armed hint names the insertion spot by the step it follows, without "armed" jargon', () => {
    const def: RoutineDef = {
      ...EMPTY_DEF,
      tracks: [{ name: 'track-1', steps: [{ id: 's5', action: 'radio.connect', params: {} }] }],
    };
    renderPalette({ armedInsert: { trackIdx: 0, afterStepId: 's5' }, def });
    const hint = screen.getByTestId('palette-hint');
    expect(hint.textContent).toMatch(/adding after/i);
    expect(hint.textContent).not.toMatch(/\barmed\b/i);
  });

  it('clicking an action item with an armed insert point calls onInsert with an action step whose action matches, via nextStepId', () => {
    const armed: ArmedInsertPosition = { trackIdx: 0, afterStepId: null };
    const { onInsert } = renderPalette({ armedInsert: armed, def: EMPTY_DEF });
    fireEvent.click(screen.getByTestId('palette-item-radio.connect'));
    expect(onInsert).toHaveBeenCalledWith({ id: 's1', action: 'radio.connect', params: {} });
  });

  it('retry inserts the real 5-kind control shape, seeding `step` from the armed position\'s afterStepId', () => {
    const armed: ArmedInsertPosition = { trackIdx: 0, afterStepId: 's5' };
    const def: RoutineDef = {
      ...EMPTY_DEF,
      tracks: [{ name: 'track-1', steps: [{ id: 's5', action: 'radio.connect', params: {} }] }],
    };
    const { onInsert } = renderPalette({ armedInsert: armed, def });
    fireEvent.click(screen.getByTestId('palette-control-retry'));
    expect(onInsert).toHaveBeenCalledWith({
      id: 's6',
      control: 'retry',
      step: 's5',
      attempts: 3,
      backoff_s: 2,
    } satisfies Step);
  });

  it('a LIBRARY item inserts a {control: "call", routine: name} step', async () => {
    const armed: ArmedInsertPosition = { trackIdx: 0, afterStepId: null };
    const { onInsert } = renderPalette({ armedInsert: armed });
    await screen.findByTestId('palette-library-oregon-gateways');
    fireEvent.click(screen.getByTestId('palette-library-oregon-gateways'));
    expect(onInsert).toHaveBeenCalledWith({ id: 's1', control: 'call', routine: 'oregon-gateways' });
  });

  it('CONTROL FLOW\'s generic "call routine" item leaves routine blank for the inspector to fill in', () => {
    const armed: ArmedInsertPosition = { trackIdx: 0, afterStepId: null };
    const { onInsert } = renderPalette({ armedInsert: armed });
    fireEvent.click(screen.getByTestId('palette-control-call'));
    expect(onInsert).toHaveBeenCalledWith({ id: 's1', control: 'call', routine: '' });
  });
});
