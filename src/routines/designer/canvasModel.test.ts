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

  it('track-1: main row is trigger → action → branch, in order', () => {
    const [lane1] = model.lanes;
    expect(lane1!.rows).toHaveLength(3); // main + ok fan-out + err fan-out
    expect(lane1!.rows[0]!.map((n) => n.kind)).toEqual(['trigger', 'action', 'branch']);
    expect(lane1!.rows[0]!.map((n) => n.id)).toEqual(['trigger-0', 's1', 's2']);
  });

  it('branch fans the lane into ok (2 actions + end) and err (1 action + end) rows', () => {
    const [lane1] = model.lanes;
    expect(lane1!.rows[1]!.map((n) => n.kind)).toEqual(['action', 'action', 'end']);
    expect(lane1!.rows[1]!.map((n) => n.id)).toEqual(['s3', 's4', 's10']);
    expect(lane1!.rows[2]!.map((n) => n.kind)).toEqual(['action', 'end']);
    expect(lane1!.rows[2]!.map((n) => n.id)).toEqual(['s5', 's11']);
  });

  it('the branch emits ok/err labeled edges into each fan-out row, every edge is an insert point', () => {
    const [lane1] = model.lanes;
    const okEdge = lane1!.edges.find((e) => e.from === 's2' && e.label === 'ok');
    const errEdge = lane1!.edges.find((e) => e.from === 's2' && e.label === 'err');
    expect(okEdge).toEqual({ from: 's2', to: 's3', label: 'ok', insertPoint: true });
    expect(errEdge).toEqual({ from: 's2', to: 's5', label: 'err', insertPoint: true });
    expect(lane1!.edges.every((e) => e.insertPoint)).toBe(true);
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

  it('track-2 has a single row (no branch) covering trigger → …→ end in order', () => {
    const [, lane2] = model.lanes;
    expect(lane2!.rows).toHaveLength(1);
    expect(lane2!.rows[0]!.map((n) => n.kind)).toEqual([
      'trigger',
      'action',
      'action',
      'delay',
      'action',
      'end',
    ]);
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
