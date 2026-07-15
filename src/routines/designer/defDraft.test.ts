/**
 * Tests for defDraft.ts — the pure routine-draft model (routines plan-5 Task
 * 9, `.superpowers/sdd/task-9-brief.md`).
 *
 * No Tauri, no React — every op here is a pure function over `RoutineDef`
 * (routinesApi.ts), so these are plain data-in/data-out assertions.
 */
import { describe, it, expect } from 'vitest';
import type { RoutineDef, ActionStep, ControlStep } from '../routinesApi';
import {
  createDraft,
  nextStepId,
  insertStep,
  insertStepIntoBranchArm,
  removeStep,
  updateStep,
  updateSettings,
  addTrack,
  removeTrack,
} from './defDraft';

function baseDef(): RoutineDef {
  return {
    routine: 'r1',
    schema_version: 1,
    transmit_mode: 'attended',
    triggers: [{ type: 'manual' }],
    tracks: [
      {
        name: 'track-1',
        steps: [
          { id: 's1', action: 'radio.connect', timeout_s: 30 },
          { id: 's2', control: 'end' },
        ],
      },
      {
        name: 'track-2',
        steps: [{ id: 's3', action: 'local.log' }],
      },
    ],
  };
}

describe('createDraft', () => {
  it('produces the minimal valid skeleton with a given name', () => {
    expect(createDraft('deployment-poll')).toEqual({
      routine: 'deployment-poll',
      schema_version: 1,
      transmit_mode: 'attended',
      triggers: [{ type: 'manual' }],
      tracks: [{ name: 'track-1', steps: [] }],
    });
  });

  it('defaults to an empty name when none is given', () => {
    expect(createDraft().routine).toBe('');
  });
});

describe('nextStepId', () => {
  it('returns s<max+1> across all tracks', () => {
    expect(nextStepId(baseDef())).toBe('s4');
  });

  it('returns s1 for an empty draft', () => {
    expect(nextStepId(createDraft('x'))).toBe('s1');
  });

  it('never collides after the highest-numbered step is removed', () => {
    const def = baseDef();
    // s3 is the max; remove it, then the next id must not collide with any
    // step still present (s1, s2) — recomputing from the live def, not a
    // stale counter, guarantees this.
    const afterRemoval = removeStep(def, 's3');
    const id = nextStepId(afterRemoval);
    const allIds = afterRemoval.tracks.flatMap((t) => t.steps.map((s) => s.id));
    expect(allIds).not.toContain(id);
    expect(id).toBe('s3');

    // Insert a step using that id, then ask again — must not repeat it.
    const withNew = insertStep(afterRemoval, 0, 's1', { id, action: 'local.notify' });
    const nextId = nextStepId(withNew);
    const allIds2 = withNew.tracks.flatMap((t) => t.steps.map((s) => s.id));
    expect(allIds2).not.toContain(nextId);
    expect(nextId).toBe('s4');
  });
});

describe('insertStep', () => {
  it('prepends when afterStepId is null', () => {
    const def = baseDef();
    const step: ActionStep = { id: 's4', action: 'local.notify' };
    const next = insertStep(def, 0, null, step);
    expect(next.tracks[0]!.steps[0]).toEqual(step);
    expect(next.tracks[0]!.steps.map((s) => s.id)).toEqual(['s4', 's1', 's2']);
  });

  it('inserts immediately after the named step in the given track', () => {
    const def = baseDef();
    const step: ActionStep = { id: 's4', action: 'local.notify' };
    const next = insertStep(def, 0, 's1', step);
    expect(next.tracks[0]!.steps.map((s) => s.id)).toEqual(['s1', 's4', 's2']);
  });

  it('leaves other tracks and untouched fields on the def unchanged (immutable)', () => {
    const def = baseDef();
    const step: ActionStep = { id: 's4', action: 'local.notify' };
    const next = insertStep(def, 0, 's1', step);

    expect(next).not.toBe(def);
    expect(next.tracks[0]).not.toBe(def.tracks[0]);
    expect(next.tracks[1]).toBe(def.tracks[1]); // untouched track — same reference
    expect(next.routine).toBe(def.routine);
    expect(next.schema_version).toBe(def.schema_version);
    expect(next.transmit_mode).toBe(def.transmit_mode);
    expect(next.triggers).toBe(def.triggers);
    // original untouched
    expect(def.tracks[0]!.steps).toHaveLength(2);
  });

  it('works on a control step and on a non-zero track index', () => {
    const def = baseDef();
    const step: ControlStep = { id: 's4', control: 'delay', delay: '5m' };
    const next = insertStep(def, 1, 's3', step);
    expect(next.tracks[1]!.steps.map((s) => s.id)).toEqual(['s3', 's4']);
    expect(next.tracks[0]).toBe(def.tracks[0]);
  });
});

describe('insertStepIntoBranchArm', () => {
  /** track-1: action s1 → branch s2 (then:[s3], else:[]) → s3 end. */
  function branchDef(): RoutineDef {
    return {
      routine: 'r1',
      schema_version: 1,
      transmit_mode: 'attended',
      triggers: [{ type: 'manual' }],
      tracks: [
        {
          name: 'track-1',
          steps: [
            { id: 's1', action: 'radio.connect' },
            { id: 's2', control: 'branch', on: 's1.connected', then: ['s3'], else: [] },
            { id: 's3', control: 'end', failed: false },
          ],
        },
        { name: 'track-2', steps: [{ id: 's4', action: 'local.log' }] },
      ],
    };
  }

  it('EMPTY arm: splices the step right after the branch and appends its id to that arm', () => {
    const def = branchDef();
    const step: ActionStep = { id: 's5', action: 'aprs.send' };
    const next = insertStepIntoBranchArm(def, 0, 's2', 'else', step);
    // Storage: right after the branch (else is empty).
    expect(next.tracks[0]!.steps.map((s) => s.id)).toEqual(['s1', 's2', 's5', 's3']);
    // Arm list: appended.
    const branch = next.tracks[0]!.steps[1] as ControlStep & { control: 'branch' };
    expect(branch.else).toEqual(['s5']);
    // Untouched branch fields preserved.
    expect(branch.on).toBe('s1.connected');
    expect(branch.then).toEqual(['s3']);
  });

  it('NON-EMPTY arm: splices after the arm\'s last step and appends to the arm (order preserved)', () => {
    const def = branchDef();
    const step: ActionStep = { id: 's5', action: 'local.notify' };
    const next = insertStepIntoBranchArm(def, 0, 's2', 'then', step);
    expect(next.tracks[0]!.steps.map((s) => s.id)).toEqual(['s1', 's2', 's3', 's5']);
    const branch = next.tracks[0]!.steps[1] as ControlStep & { control: 'branch' };
    expect(branch.then).toEqual(['s3', 's5']);
    expect(branch.else).toEqual([]);
  });

  it('is immutable: original def untouched, other tracks keep their references', () => {
    const def = branchDef();
    const step: ActionStep = { id: 's5', action: 'aprs.send' };
    const next = insertStepIntoBranchArm(def, 0, 's2', 'else', step);
    expect(next).not.toBe(def);
    expect(next.tracks[0]).not.toBe(def.tracks[0]);
    expect(next.tracks[1]).toBe(def.tracks[1]); // untouched track — same reference
    // Original untouched: 3 steps, empty else.
    expect(def.tracks[0]!.steps).toHaveLength(3);
    expect((def.tracks[0]!.steps[1] as ControlStep & { control: 'branch' }).else).toEqual([]);
  });

  it('is a no-op (equal-by-value, never throws) when the branch id does not resolve to a branch in that track', () => {
    const def = branchDef();
    const step: ActionStep = { id: 's5', action: 'aprs.send' };
    expect(insertStepIntoBranchArm(def, 0, 'does-not-exist', 'then', step)).toEqual(def);
    // s1 exists but is not a branch control step.
    expect(insertStepIntoBranchArm(def, 0, 's1', 'then', step)).toEqual(def);
  });
});

describe('removeStep', () => {
  it('removes the step wherever it lives, across any track', () => {
    const def = baseDef();
    const next = removeStep(def, 's3');
    expect(next.tracks[1]!.steps).toHaveLength(0);
    expect(next.tracks[0]!.steps).toHaveLength(2); // untouched track content
  });

  it('is a no-op (new object, same content) when the id is not found', () => {
    const def = baseDef();
    const next = removeStep(def, 'does-not-exist');
    expect(next).toEqual(def);
    expect(next).not.toBe(def);
  });
});

describe('updateStep', () => {
  it('merges the patch and preserves untouched fields on that step', () => {
    const def = baseDef();
    const next = updateStep(def, 's1', { timeout_s: 90 });
    const updated = next.tracks[0]!.steps[0] as ActionStep;
    expect(updated.timeout_s).toBe(90);
    expect(updated.action).toBe('radio.connect');
    expect(updated.id).toBe('s1');
  });

  it('leaves other steps and tracks untouched (immutable, reference-stable where unaffected)', () => {
    const def = baseDef();
    const next = updateStep(def, 's1', { timeout_s: 90 });
    expect(next.tracks[0]!.steps[1]).toBe(def.tracks[0]!.steps[1]);
    expect(next.tracks[1]).toBe(def.tracks[1]);
  });

  it('can patch a control step', () => {
    const def = baseDef();
    const next = updateStep(def, 's2', { failed: true, reason: 'aborted' });
    const updated = next.tracks[0]!.steps[1] as ControlStep;
    expect(updated).toEqual({ id: 's2', control: 'end', failed: true, reason: 'aborted' });
  });
});

describe('updateSettings', () => {
  it('merges top-level settings fields, preserving tracks', () => {
    const def = baseDef();
    const next = updateSettings(def, { transmit_mode: 'automatic', on_interrupted: 'resume' });
    expect(next.transmit_mode).toBe('automatic');
    expect(next.on_interrupted).toBe('resume');
    expect(next.tracks).toBe(def.tracks);
    expect(next.routine).toBe(def.routine);
  });

  it('can rename the routine', () => {
    const def = baseDef();
    const next = updateSettings(def, { routine: 'renamed' });
    expect(next.routine).toBe('renamed');
    expect(def.routine).toBe('r1');
  });
});

describe('addTrack', () => {
  it('appends a new empty track', () => {
    const def = baseDef();
    const next = addTrack(def, 'track-3');
    expect(next.tracks).toHaveLength(3);
    expect(next.tracks[2]).toEqual({ name: 'track-3', steps: [] });
    expect(next.tracks[0]).toBe(def.tracks[0]);
  });
});

describe('removeTrack', () => {
  it('removes the track at the given index', () => {
    const def = baseDef();
    const next = removeTrack(def, 0);
    expect(next.tracks).toHaveLength(1);
    expect(next.tracks[0]!.name).toBe('track-2');
  });
});
