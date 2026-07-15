/**
 * defDraft.ts — the pure routine-draft model (routines plan-5 Task 9,
 * `.superpowers/sdd/task-9-brief.md`).
 *
 * No Tauri, no React: every export here is a pure function over a
 * `RoutineDef` (routinesApi.ts) — the caller (RoutineDesigner.tsx today,
 * Tasks 10-12's CanvasTab/RunsTab/SettingsTab tomorrow) owns state and
 * re-validates after each call. Every edit op is IMMUTABLE — it returns a new
 * `RoutineDef`; it never mutates its argument. Only the branch of the tree
 * actually touched gets a new object/array — untouched tracks/steps keep
 * their original references, which is asserted by defDraft.test.ts and lets
 * a caller cheaply diff "what changed" if it ever wants to.
 */
import type { RoutineDef, Track, Step, ActionStep, ControlStep } from '../routinesApi';

/** Minimal valid skeleton for a fresh, unsaved routine (task-9 brief
 *  "Produces" list, verbatim). `name` defaults to `''` — RoutineDesigner
 *  treats an empty `routine` as a new, not-yet-named draft (binding
 *  constraint 6) and renders an editable name field for it. */
export function createDraft(name?: string): RoutineDef {
  return {
    routine: name ?? '',
    schema_version: 1,
    transmit_mode: 'attended',
    triggers: [{ type: 'manual' }],
    tracks: [{ name: 'track-1', steps: [] }],
  };
}

const STEP_ID_RE = /^s(\d+)$/;

/** `s<max+1>` across every step id in every track. Pure and stateless — it
 * recomputes the max from the live `def` on every call rather than
 * remembering a counter, so an id freed by `removeStep` is available again
 * without ever colliding with an id still in use (defDraft.test.ts's
 * "never collides after removals" case). Step ids that don't match the
 * `s<n>` shape (never produced by this module, but tolerated on an imported
 * def) are ignored for the max computation rather than throwing. */
export function nextStepId(def: RoutineDef): string {
  let max = 0;
  for (const track of def.tracks) {
    for (const step of track.steps) {
      const m = STEP_ID_RE.exec(step.id);
      if (m) {
        const n = Number(m[1]);
        if (n > max) max = n;
      }
    }
  }
  return `s${max + 1}`;
}

/** Insert `step` into `def.tracks[trackIdx]`, immediately after the step
 * whose id is `afterStepId`, or at the front of the track when
 * `afterStepId` is `null`. An `afterStepId` that isn't found in the track
 * appends to the end rather than silently dropping the step. Only the
 * targeted track gets a new object; every other track keeps its original
 * reference. */
export function insertStep(
  def: RoutineDef,
  trackIdx: number,
  afterStepId: string | null,
  step: Step,
): RoutineDef {
  const tracks = def.tracks.map((track, i): Track => {
    if (i !== trackIdx) return track;
    const steps = track.steps.slice();
    if (afterStepId === null) {
      steps.unshift(step);
    } else {
      const idx = steps.findIndex((s) => s.id === afterStepId);
      steps.splice(idx === -1 ? steps.length : idx + 1, 0, step);
    }
    return { ...track, steps };
  });
  return { ...def, tracks };
}

/** Insert `step` INTO a branch arm (Task 11 authoring fix, Gap A): both
 * splices the step into `def.tracks[trackIdx].steps` — immediately after the
 * arm's last step already present in that track, or immediately after the
 * branch itself when the arm is empty — AND appends `step.id` to that
 * branch's `then`/`else` id list, so the step lands IN the arm (rendered in
 * the fan row, reachable), never in the unplaced row. No new engine
 * semantics: a branch arm IS its then/else id list (types.rs Branch); this
 * op just performs both existing edits atomically. A `branchStepId` that
 * doesn't resolve to a branch control step in that track is a no-op (fresh,
 * equal-by-value def — same contract as `removeStep`'s missing id). */
export function insertStepIntoBranchArm(
  def: RoutineDef,
  trackIdx: number,
  branchStepId: string,
  arm: 'then' | 'else',
  step: Step,
): RoutineDef {
  const tracks = def.tracks.map((track, i): Track => {
    if (i !== trackIdx) return track;
    const branchIdx = track.steps.findIndex(
      (s) => s.id === branchStepId && 'control' in s && s.control === 'branch',
    );
    if (branchIdx === -1) return { ...track }; // no such branch here — no-op
    const branch = track.steps[branchIdx] as ControlStep & { control: 'branch' };
    const armIds = branch[arm];
    // Storage splice position: right after the LAST of the arm's steps that
    // actually lives in this track (arm ids can dangle — validator's
    // problem, not ours), falling back to right after the branch itself.
    let insertIdx = branchIdx + 1;
    for (const id of armIds) {
      const idx = track.steps.findIndex((s) => s.id === id);
      if (idx !== -1 && idx + 1 > insertIdx) insertIdx = idx + 1;
    }
    const steps = track.steps.slice();
    steps.splice(insertIdx, 0, step);
    // insertIdx is always > branchIdx, so the branch's own index is stable.
    steps[branchIdx] = { ...branch, [arm]: [...armIds, step.id] };
    return { ...track, steps };
  });
  return { ...def, tracks };
}

/** Remove the step with `stepId`, searching every track (a caller doesn't
 * need to know which track a step lives in — control steps like `retry`
 * reference other steps by id across the whole def, not just their own
 * track). A missing id is a no-op (still returns a fresh, equal-by-value
 * def, never mutates or throws). */
export function removeStep(def: RoutineDef, stepId: string): RoutineDef {
  const tracks = def.tracks.map((track): Track => {
    if (!track.steps.some((s) => s.id === stepId)) return track; // untouched track keeps its reference
    return { ...track, steps: track.steps.filter((s) => s.id !== stepId) };
  });
  return { ...def, tracks };
}

/** Patch fields on the step with `stepId`, wherever it lives, preserving
 * every untouched field (both the step's own and its siblings'). `patch` is
 * an intersection of both step shapes' optional fields so a caller can patch
 * either an action step or a control step through one signature without a
 * type-narrowing dance — the real shape stays whatever `step.action` /
 * `step.control` already discriminates. */
export type StepPatch = Partial<ActionStep> & Partial<ControlStep>;

export function updateStep(def: RoutineDef, stepId: string, patch: StepPatch): RoutineDef {
  const tracks = def.tracks.map((track): Track => {
    const idx = track.steps.findIndex((s) => s.id === stepId);
    if (idx === -1) return track; // untouched track keeps its reference
    const steps = track.steps.slice();
    steps[idx] = { ...steps[idx], ...patch } as Step;
    return { ...track, steps };
  });
  return { ...def, tracks };
}

/** Patch top-level routine settings (everything except `tracks`, which has
 * its own dedicated ops). `schema_version` is deliberately excluded — it is
 * a storage-format version this module doesn't renegotiate. */
export type SettingsPatch = Partial<
  Pick<RoutineDef, 'routine' | 'transmit_mode' | 'transmit_ack' | 'on_interrupted' | 'inputs' | 'triggers'>
>;

export function updateSettings(def: RoutineDef, patch: SettingsPatch): RoutineDef {
  return { ...def, ...patch };
}

/** Append a new, empty track named `name`. */
export function addTrack(def: RoutineDef, name: string): RoutineDef {
  return { ...def, tracks: [...def.tracks, { name, steps: [] }] };
}

/** Remove the track at `trackIdx` (and every step it held — control steps
 * elsewhere that referenced one of those step ids become dangling
 * references; validateDraft's own findings surface that, this op doesn't
 * try to police it). */
export function removeTrack(def: RoutineDef, trackIdx: number): RoutineDef {
  return { ...def, tracks: def.tracks.filter((_, i) => i !== trackIdx) };
}
