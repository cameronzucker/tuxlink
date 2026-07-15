/**
 * Tests for canvasModel.ts (routines plan-5 Task 10,
 * `.superpowers/sdd/task-10-brief.md`) — the pure, fast, load-bearing test of
 * this task. No mocks: `layoutCanvas` is Tauri-free and React-free.
 */
import { describe, it, expect } from 'vitest';
import type { ActionInfo, RoutineDef } from '../routinesApi';
import { layoutCanvas } from './canvasModel';

/** A fake registry deliberately shaped so `category`/`transmits` can only be
 *  correct if the model reads THIS, not the action's name: `data.spacewx_wwv`
 *  needs the radio (not the network its name suggests), and `aprs.send`
 *  transmits despite having no "radio." prefix. */
const FAKE_ACTIONS: ActionInfo[] = [
  { name: 'radio.connect', needsRadio: true, needsInternet: false, transmits: true },
  { name: 'data.read', needsRadio: false, needsInternet: true, transmits: false },
  { name: 'local.notify', needsRadio: false, needsInternet: false, transmits: false },
  { name: 'aprs.send', needsRadio: true, needsInternet: false, transmits: true },
  { name: 'data.spacewx_wwv', needsRadio: true, needsInternet: false, transmits: false },
  { name: 'local.compose', needsRadio: false, needsInternet: false, transmits: false },
];

/** Mirrors designer-canvas.html's shape: track 1 is a connect-cycle
 *  (schedule → radio.connect → branch → ok-path (2 actions + end) / err-path
 *  (1 action + end)); track 2 is a spacewx-post with a delay and a
 *  cross-track `params` reference into track 1's `s1`. */
const FIXTURE_DEF: RoutineDef = {
  routine: 'deployment-poll',
  schema_version: 1,
  transmit_mode: 'attended',
  triggers: [
    { type: 'schedule', every: '30m', align: 'hour', window: '06:00-22:00' },
    { type: 'schedule', every: '6h' },
  ],
  tracks: [
    {
      name: 'track-1',
      steps: [
        { id: 's1', action: 'radio.connect', params: { stations: '@station-set:or-gateways' } },
        { id: 's2', control: 'branch', on: 's1.connected', then: ['s3', 's4', 's10'], else: ['s5', 's11'] },
        { id: 's3', action: 'data.read', params: { scope: 'inbox' } },
        { id: 's4', action: 'local.notify', params: { message: 'poll ok' } },
        { id: 's10', control: 'end', failed: false },
        { id: 's5', action: 'aprs.send', params: { status: 'HF poll failed' } },
        { id: 's11', control: 'end', failed: true },
      ],
    },
    {
      name: 'track-2',
      steps: [
        { id: 's6', action: 'data.spacewx_wwv', params: { freq: '10MHz' } },
        { id: 's7', action: 'local.compose', params: { template: '@template:wx-tabular' } },
        { id: 's8', control: 'delay', delay: '5m' },
        { id: 's9', action: 'radio.connect', params: { station: 's1.last_heard_gateway' } },
        { id: 's12', control: 'end', failed: false },
      ],
    },
  ],
};

describe('layoutCanvas — 2-track fixture', () => {
  const model = layoutCanvas(FIXTURE_DEF, FAKE_ACTIONS);

  it('produces one lane per track', () => {
    expect(model.lanes).toHaveLength(2);
    expect(model.lanes.map((l) => l.track)).toEqual(['track-1', 'track-2']);
  });

  it('track-1: main row is ALL routine-level triggers → action → branch, in order', () => {
    // Triggers are routine-level (they fire the whole routine, never one
    // track), so BOTH schedule triggers head the FIRST lane — neither is
    // dropped, neither is fabricated onto track 2.
    const [lane1] = model.lanes;
    expect(lane1!.rows).toHaveLength(3); // main + ok fan-out + err fan-out
    expect(lane1!.rows[0]!.map((n) => n.kind)).toEqual(['trigger', 'trigger', 'action', 'branch']);
    expect(lane1!.rows[0]!.map((n) => n.id)).toEqual(['trigger-0', 'trigger-1', 's1', 's2']);
  });

  it('the edge between two trigger heads carries no insert point; the trigger→first-step edge arms a PREPEND (insertAfter null)', () => {
    const [lane1] = model.lanes;
    const betweenTriggers = lane1!.edges.find((e) => e.from === 'trigger-0' && e.to === 'trigger-1');
    expect(betweenTriggers).toEqual({
      from: 'trigger-0',
      to: 'trigger-1',
      insertPoint: false,
      insertAfter: null,
    });
    const headEdge = lane1!.edges.find((e) => e.from === 'trigger-1' && e.to === 's1');
    // 'trigger-1' is not a step id defDraft.insertStep could ever find (its
    // findIndex-miss APPENDS) — the trigger edge must arm the documented
    // prepend contract instead.
    expect(headEdge).toEqual({ from: 'trigger-1', to: 's1', insertPoint: true, insertAfter: null });
  });

  it('branch fans the lane into ok (2 actions + end) and err (1 action + end) rows', () => {
    const [lane1] = model.lanes;
    expect(lane1!.rows[1]!.map((n) => n.kind)).toEqual(['action', 'action', 'end']);
    expect(lane1!.rows[1]!.map((n) => n.id)).toEqual(['s3', 's4', 's10']);
    expect(lane1!.rows[2]!.map((n) => n.kind)).toEqual(['action', 'end']);
    expect(lane1!.rows[2]!.map((n) => n.id)).toEqual(['s5', 's11']);
  });

  it('the branch emits ok/err labeled edges into each fan-out row, every step edge is an insert point', () => {
    const [lane1] = model.lanes;
    const okEdge = lane1!.edges.find((e) => e.from === 's2' && e.label === 'ok');
    const errEdge = lane1!.edges.find((e) => e.from === 's2' && e.label === 'err');
    expect(okEdge).toEqual({ from: 's2', to: 's3', label: 'ok', insertPoint: true, insertAfter: 's2' });
    expect(errEdge).toEqual({ from: 's2', to: 's5', label: 'err', insertPoint: true, insertAfter: 's2' });
    // Every edge into a STEP is an insert point; only the connectors between
    // two trigger heads (nothing insertable between triggers) are not.
    const stepEdges = lane1!.edges.filter((e) => !e.to.startsWith('trigger-'));
    expect(stepEdges.length).toBeGreaterThan(0);
    expect(stepEdges.every((e) => e.insertPoint)).toBe(true);
  });

  it('transmits is driven by the ActionInfo registry, NOT by the action name', () => {
    const [lane1, lane2] = model.lanes;
    const s1 = lane1!.rows[0]!.find((n) => n.id === 's1')!;
    const s5 = lane1!.rows[2]!.find((n) => n.id === 's5')!;
    const s3 = lane1!.rows[1]!.find((n) => n.id === 's3')!;
    expect(s1.transmits).toBe(true); // radio.connect: transmits:true in registry
    expect(s5.transmits).toBe(true); // aprs.send: transmits:true despite no "radio." prefix
    expect(s3.transmits).toBe(false); // data.read: transmits:false

    const s6 = lane2!.rows[0]!.find((n) => n.id === 's6')!;
    expect(s6.category).toBe('radio'); // data.spacewx_wwv needsRadio:true — category is NOT name-derived
    expect(s6.transmits).toBe(false);
  });

  it('categories come from needsRadio/needsInternet, control steps are always ctl', () => {
    const [lane1] = model.lanes;
    const s1 = lane1!.rows[0]!.find((n) => n.id === 's1')!;
    const s3 = lane1!.rows[1]!.find((n) => n.id === 's3')!;
    const s4 = lane1!.rows[1]!.find((n) => n.id === 's4')!;
    const s2 = lane1!.rows[0]!.find((n) => n.id === 's2')!;
    expect(s1.category).toBe('radio');
    expect(s3.category).toBe('data');
    expect(s4.category).toBe('local');
    expect(s2.category).toBe('ctl');
  });

  it('a delay control step contributes a "+{delay}" anchor', () => {
    expect(model.anchors).toEqual([{ label: '+5 min' }]);
  });

  it('detects the cross-track dependency from track-2\'s params into track-1\'s s1', () => {
    expect(model.crossTrackDeps).toEqual([
      { fromTrack: 'track-2', toTrack: 'track-1', variable: 's1.last_heard_gateway' },
    ]);
  });

  it('does NOT flag the branch\'s same-track "on" reference as a cross-track dep', () => {
    expect(model.crossTrackDeps.some((d) => d.variable === 's1.connected')).toBe(false);
  });

  it('track-2 renders headless (no fabricated trigger) with a synthetic prepend insert point', () => {
    const [, lane2] = model.lanes;
    expect(lane2!.rows).toHaveLength(1);
    // No trigger node — triggers are routine-level and all head lane 1; the
    // lane-tag (TRACK 2 · NAME) is the parallel-track head label.
    expect(lane2!.rows[0]!.map((n) => n.kind)).toEqual([
      'action',
      'action',
      'delay',
      'action',
      'end',
    ]);
    // The headless lane still offers a prepend insert point via a synthetic
    // head edge (insertAfter null = defDraft.insertStep's prepend contract).
    const headEdge = lane2!.edges.find((e) => e.from === 'head-1');
    expect(headEdge).toEqual({ from: 'head-1', to: 's6', insertPoint: true, insertAfter: null });
  });

  it('an added (empty) track renders as a labeled, headless lane with a dangling prepend insert point — never a crash, never uninsertable', () => {
    // The exact shape this task's own Add-track button produces: one more
    // trigger-less, step-less track than there are triggers.
    const def: RoutineDef = {
      ...FIXTURE_DEF,
      tracks: [...FIXTURE_DEF.tracks, { name: 'track-3', steps: [] }],
    };
    const model3 = layoutCanvas(def, FAKE_ACTIONS);
    expect(model3.lanes).toHaveLength(3);
    const lane3 = model3.lanes[2]!;
    expect(lane3.track).toBe('track-3');
    expect(lane3.rows).toEqual([[]]); // headless AND empty — no undefined node, no crash
    // A step-less track must never be uninsertable: exactly one DANGLING
    // insert edge (no target node) arming the prepend contract.
    expect(lane3.edges).toEqual([
      { from: 'head-2', to: '', insertPoint: true, insertAfter: null },
    ]);
  });

  it('a createDraft-shaped def (one trigger, empty track) has exactly one insert edge, arming a prepend from the trigger head', () => {
    // The "New Routine…" first flow: createDraft() → manual trigger + one
    // empty track. Without a dangling insert edge the canvas would render a
    // lone trigger with zero ＋ anywhere — Task 11's palette dead-ends.
    const def: RoutineDef = {
      routine: '',
      schema_version: 1,
      transmit_mode: 'attended',
      triggers: [{ type: 'manual' }],
      tracks: [{ name: 'track-1', steps: [] }],
    };
    const model1 = layoutCanvas(def, []);
    const lane = model1.lanes[0]!;
    expect(lane.rows[0]!.map((n) => n.id)).toEqual(['trigger-0']);
    const insertEdges = lane.edges.filter((e) => e.insertPoint);
    expect(insertEdges).toEqual([
      { from: 'trigger-0', to: '', insertPoint: true, insertAfter: null },
    ]);
  });

  it('end nodes carry the step\'s own failed flag (view never re-derives it from title text)', () => {
    const [lane1] = model.lanes;
    const okEnd = lane1!.rows[1]!.find((n) => n.id === 's10')!;
    const errEnd = lane1!.rows[2]!.find((n) => n.id === 's11')!;
    expect(okEnd.failed).toBe(false);
    expect(errEnd.failed).toBe(true);
    // Non-end nodes don't carry the flag at all.
    const s1 = lane1!.rows[0]!.find((n) => n.id === 's1')!;
    expect(s1.failed).toBeUndefined();
  });
});

describe('layoutCanvas — retry control step', () => {
  it('renders a retry step as a ctl node without crashing', () => {
    const def: RoutineDef = {
      routine: 'r',
      schema_version: 1,
      transmit_mode: 'attended',
      triggers: [{ type: 'manual' }],
      tracks: [
        {
          name: 'track-1',
          steps: [
            { id: 's1', action: 'radio.connect', params: {} },
            { id: 's2', control: 'retry', step: 's1', attempts: 3, backoff_s: 2 },
            { id: 's3', control: 'end', failed: false },
          ],
        },
      ],
    };

    const model = layoutCanvas(def, [{ name: 'radio.connect', needsRadio: true, needsInternet: false, transmits: true }]);

    const retryNode = model.lanes[0]!.rows[0]!.find((n) => n.id === 's2');
    expect(retryNode).toBeDefined();
    expect(retryNode!.kind).toBe('retry');
    expect(retryNode!.category).toBe('ctl');
    expect(retryNode!.transmits).toBe(false);
    expect(retryNode!.title).toContain('s1'); // titled from its real fields: wraps step s1
    expect(retryNode!.bodyLines.join(' ')).toContain('3');
  });
});

describe('layoutCanvas — unknown action', () => {
  it('flags an action absent from the registry as unknown, and never crashes', () => {
    const def: RoutineDef = {
      routine: 'r',
      schema_version: 1,
      transmit_mode: 'attended',
      triggers: [{ type: 'manual' }],
      tracks: [{ name: 'track-1', steps: [{ id: 's1', action: 'ghost.action', params: {} }] }],
    };

    expect(() => layoutCanvas(def, [])).not.toThrow();
    const node = layoutCanvas(def, []).lanes[0]!.rows[0]!.find((n) => n.id === 's1')!;
    expect(node.unknown).toBe(true);
    expect(node.transmits).toBe(false);
    expect(node.category).toBe('local');
  });
});

describe('layoutCanvas — steps no branch arm references', () => {
  it('surfaces a post-branch step referenced by no arm as a final unplaced row — never silently dropped', () => {
    const def: RoutineDef = {
      routine: 'r',
      schema_version: 1,
      transmit_mode: 'attended',
      triggers: [{ type: 'manual' }],
      tracks: [
        {
          name: 'track-1',
          steps: [
            { id: 's1', action: 'radio.connect', params: {} },
            { id: 's2', control: 'branch', on: 's1.connected', then: ['s3'], else: ['s4'] },
            { id: 's3', control: 'end', failed: false },
            { id: 's4', control: 'end', failed: true },
            // Trailing step no arm references — the old layout dropped it.
            { id: 's5', action: 'local.notify', params: { message: 'orphan' } },
          ],
        },
      ],
    };

    const model = layoutCanvas(def, [
      { name: 'radio.connect', needsRadio: true, needsInternet: false, transmits: true },
      { name: 'local.notify', needsRadio: false, needsInternet: false, transmits: false },
    ]);

    const lane = model.lanes[0]!;
    expect(lane.rows).toHaveLength(4); // main + ok + err + unplaced
    const unplacedRow = lane.rows[3]!;
    expect(unplacedRow.map((n) => n.id)).toEqual(['s5']);
    expect(unplacedRow[0]!.unplaced).toBe(true);
    // Every step of the track appears SOMEWHERE in the lane's rows.
    const allRenderedIds = lane.rows.flat().map((n) => n.id);
    for (const step of def.tracks[0]!.steps) {
      expect(allRenderedIds).toContain(step.id);
    }
    // Placed nodes are never marked unplaced.
    expect(lane.rows[0]!.every((n) => !n.unplaced)).toBe(true);
    expect(lane.rows[1]!.every((n) => !n.unplaced)).toBe(true);
    expect(lane.rows[2]!.every((n) => !n.unplaced)).toBe(true);
  });
});
