/**
 * Tests for CanvasTab.tsx (routines plan-5 Task 10,
 * `.superpowers/sdd/task-10-brief.md`). No Tauri mock needed — CanvasTab is
 * a pure renderer of `canvasModel.ts`'s output over the `draft`/`actions`
 * props it's given (RoutineDesigner owns all fetching/edit-op wiring).
 */
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { ActionInfo, RoutineDef } from '../routinesApi';
import { CanvasTab, type ArmedInsertPosition } from './CanvasTab';

const ACTIONS: ActionInfo[] = [
  { name: 'radio.connect', needsRadio: true, needsInternet: false, transmits: true },
  { name: 'local.notify', needsRadio: false, needsInternet: false, transmits: false },
];

const DEF: RoutineDef = {
  routine: 'r',
  schema_version: 1,
  transmit_mode: 'attended',
  triggers: [{ type: 'manual' }],
  tracks: [
    {
      name: 'track-1',
      steps: [
        { id: 's1', action: 'radio.connect', params: {} },
        { id: 's2', action: 'local.notify', params: {} },
        { id: 's3', action: 'ghost.action', params: {} }, // absent from ACTIONS
      ],
    },
  ],
};

function renderTab(overrides: Partial<Parameters<typeof CanvasTab>[0]> = {}) {
  const onSelect = overrides.onSelect ?? vi.fn();
  const onInsertAt = overrides.onInsertAt ?? vi.fn();
  const onRemoveStep = overrides.onRemoveStep ?? vi.fn();
  const onAddTrack = overrides.onAddTrack ?? vi.fn();
  const utils = render(
    <CanvasTab
      draft={overrides.draft ?? DEF}
      actions={overrides.actions ?? ACTIONS}
      selectedStepId={overrides.selectedStepId ?? null}
      onSelect={onSelect}
      armedInsert={overrides.armedInsert ?? null}
      onInsertAt={onInsertAt}
      onRemoveStep={onRemoveStep}
      onAddTrack={onAddTrack}
    />,
  );
  return { ...utils, onSelect, onInsertAt, onRemoveStep, onAddTrack };
}

describe('CanvasTab — rendering', () => {
  it('renders one node per step (plus the trigger)', () => {
    renderTab();
    expect(screen.getByTestId('node-s1')).toBeInTheDocument();
    expect(screen.getByTestId('node-s2')).toBeInTheDocument();
    expect(screen.getByTestId('node-s3')).toBeInTheDocument();
    expect(screen.getByTestId('node-trigger-0')).toBeInTheDocument();
  });

  it('renders an unknown marker for an action absent from the registry, without crashing', () => {
    renderTab();
    const ghostNode = screen.getByTestId('node-s3');
    expect(ghostNode).toHaveTextContent(/unknown/i);
    expect(ghostNode.className).toContain('node-unknown');
    // The known-action nodes do NOT carry the marker.
    expect(screen.getByTestId('node-s1')).not.toHaveTextContent(/unknown/i);
  });

  it('shows a tx-dot only on a node whose registry entry transmits', () => {
    renderTab();
    expect(screen.getByTestId('tx-dot-s1')).toBeInTheDocument(); // radio.connect: transmits:true
    expect(screen.queryByTestId('tx-dot-s2')).not.toBeInTheDocument(); // local.notify: transmits:false
  });
});

describe('CanvasTab — selection', () => {
  it('clicking a node calls onSelect with its id, and a selectedStepId match renders the selected class', () => {
    const { onSelect, rerender, onInsertAt, onRemoveStep, onAddTrack } = renderTab();
    fireEvent.click(screen.getByTestId('node-s1'));
    expect(onSelect).toHaveBeenCalledWith('s1');

    rerender(
      <CanvasTab
        draft={DEF}
        actions={ACTIONS}
        selectedStepId="s1"
        onSelect={onSelect}
        armedInsert={null}
        onInsertAt={onInsertAt}
        onRemoveStep={onRemoveStep}
        onAddTrack={onAddTrack}
      />,
    );
    expect(screen.getByTestId('node-s1').className).toContain('selected');
    expect(screen.getByTestId('node-s2').className).not.toContain('selected');
  });

  it('clicking the ⌫ affordance on a selected node calls onRemoveStep', () => {
    const { onRemoveStep } = renderTab({ selectedStepId: 's1' });
    fireEvent.click(screen.getByTestId('delete-s1'));
    expect(onRemoveStep).toHaveBeenCalledWith('s1');
  });

  it('pressing Backspace with a node selected calls onRemoveStep', () => {
    const { onRemoveStep } = renderTab({ selectedStepId: 's2' });
    fireEvent.keyDown(window, { key: 'Backspace' });
    expect(onRemoveStep).toHaveBeenCalledWith('s2');
  });
});

describe('CanvasTab — insert points', () => {
  it('clicking an edge\'s ＋ calls onInsertAt with the right {trackIdx, afterStepId}', () => {
    const { onInsertAt } = renderTab();
    fireEvent.click(screen.getByTestId('insert-s1'));
    expect(onInsertAt).toHaveBeenCalledWith({ trackIdx: 0, afterStepId: 's1' });
  });

  it('the trigger→first-step edge arms a PREPEND ({trackIdx, afterStepId: null}), never the synthetic trigger id', () => {
    // 'trigger-0' is not a step id — defDraft.insertStep's findIndex would
    // miss it and APPEND. The trigger edge must arm the documented
    // afterStepId:null prepend contract instead.
    const { onInsertAt } = renderTab();
    fireEvent.click(screen.getByTestId('insert-trigger-0'));
    expect(onInsertAt).toHaveBeenCalledWith({ trackIdx: 0, afterStepId: null });
  });

  it('the insert point matching armedInsert renders the armed (amber) class', () => {
    const armed: ArmedInsertPosition = { trackIdx: 0, afterStepId: 's1' };
    renderTab({ armedInsert: armed });
    const edge = screen.getByTestId('edge-s1-s2');
    expect(edge.className).toContain('armed');
    const otherEdge = screen.getByTestId('edge-s2-s3');
    expect(otherEdge.className).not.toContain('armed');
  });
});

describe('CanvasTab — branch fan-out + anchors + deps', () => {
  const BRANCH_DEF: RoutineDef = {
    routine: 'r',
    schema_version: 1,
    transmit_mode: 'attended',
    triggers: [{ type: 'schedule', every: '30m' }, { type: 'schedule', every: '6h' }],
    tracks: [
      {
        name: 'track-1',
        steps: [
          { id: 's1', action: 'radio.connect', params: {} },
          { id: 's2', control: 'branch', on: 's1.connected', then: ['s3'], else: ['s4'] },
          { id: 's3', control: 'end', failed: false },
          { id: 's4', control: 'end', failed: true },
        ],
      },
      {
        name: 'track-2',
        steps: [
          { id: 's5', control: 'delay', delay: '5m' },
          { id: 's6', action: 'radio.connect', params: { station: 's1.last_heard_gateway' } },
        ],
      },
    ],
  };

  it('renders both branch-out paths with ok/err labels', () => {
    renderTab({ draft: BRANCH_DEF });
    expect(screen.getByText('ok')).toBeInTheDocument();
    expect(screen.getByText('err')).toBeInTheDocument();
    expect(screen.getByTestId('node-s3')).toBeInTheDocument();
    expect(screen.getByTestId('node-s4')).toBeInTheDocument();
  });

  it('renders BOTH routine-level triggers on the first lane; track 2 gets no fabricated trigger', () => {
    renderTab({ draft: BRANCH_DEF });
    const lane1 = screen.getByTestId('lane-0');
    expect(lane1).toContainElement(screen.getByTestId('node-trigger-0'));
    expect(lane1).toContainElement(screen.getByTestId('node-trigger-1'));
    // Track 2 renders headless (its lane-tag is the head label) but keeps a
    // prepend insert point via the synthetic head edge.
    const lane2 = screen.getByTestId('lane-1');
    expect(lane2.querySelector('.node.trigger')).toBeNull();
    expect(lane2).toContainElement(screen.getByTestId('insert-head-1'));
  });

  it('the two branch lead-edge insert points have distinct testids and both arm after the branch step', () => {
    const { onInsertAt } = renderTab({ draft: BRANCH_DEF });
    const okInsert = screen.getByTestId('insert-s2-ok');
    const errInsert = screen.getByTestId('insert-s2-err');
    expect(okInsert).not.toBe(errInsert);
    fireEvent.click(okInsert);
    expect(onInsertAt).toHaveBeenLastCalledWith({ trackIdx: 0, afterStepId: 's2' });
    fireEvent.click(errInsert);
    expect(onInsertAt).toHaveBeenLastCalledWith({ trackIdx: 0, afterStepId: 's2' });
  });

  it('renders the delay anchor and the cross-track dependency chip', () => {
    renderTab({ draft: BRANCH_DEF });
    expect(screen.getByTestId('anchor-rule')).toHaveTextContent('+5 min');
    expect(screen.getByTestId('dep-1-0')).toHaveTextContent('s1.last_heard_gateway');
  });

  it('surfaces a post-branch step no arm references with a visible unplaced marker', () => {
    const draft: RoutineDef = {
      ...BRANCH_DEF,
      tracks: [
        {
          ...BRANCH_DEF.tracks[0]!,
          steps: [
            ...BRANCH_DEF.tracks[0]!.steps,
            { id: 's9', action: 'local.notify', params: { message: 'orphan' } },
          ],
        },
        BRANCH_DEF.tracks[1]!,
      ],
    };
    renderTab({ draft });
    const orphan = screen.getByTestId('node-s9');
    expect(orphan).toHaveTextContent(/unplaced/i);
    expect(orphan.className).toContain('node-unplaced');
    expect(screen.getByTestId('unplaced-row-0')).toContainElement(orphan);
  });
});

describe('CanvasTab — add track', () => {
  it('the Add track button calls onAddTrack', () => {
    const { onAddTrack } = renderTab();
    fireEvent.click(screen.getByTestId('add-track-btn'));
    expect(onAddTrack).toHaveBeenCalledTimes(1);
  });
});
