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
 * splices the step into `def.tracks[trackIdx].steps` AND inserts `step.id`
 * into that branch's `then`/`else` id list, so the step lands IN the arm
 * (rendered in the fan row, reachable), never in the unplaced row. No new
 * engine semantics: a branch arm IS its then/else id list (types.rs Branch);
 * this op just performs both existing edits atomically.
 *
 * `afterStepId` positions the insert WITHIN the arm (the armed arm ＋'s
 * `insertAfter`): when it is the branch's OWN id (the ＋ straight out of the
 * branch — an empty arm's dangling ＋ or the lead ＋ into a populated arm's
 * first node), the new id goes to the FRONT of the then/else list and
 * storage is spliced right after the branch (arm-position-0; for an empty
 * arm prepend ≡ append, so this subsumes the old empty-arm behavior). When
 * it names a step already in the arm's id list, the new id goes into the
 * list immediately after it and storage is spliced immediately after that
 * step. Otherwise — `null` or an id not in the arm — the id is APPENDED to
 * the arm list and storage is spliced after the arm's last step present in
 * this track (right after the branch when the arm is empty). Arm ids that
 * dangle out of the track are the validator's problem, not ours — this op
 * never throws on them.
 *
 * A `branchStepId` that doesn't resolve to a branch control step in that
 * track is a no-op (fresh, equal-by-value def — same contract as
 * `removeStep`'s missing id). */
export function insertStepIntoBranchArm(
  def: RoutineDef,
  trackIdx: number,
  branchStepId: string,
  arm: 'then' | 'else',
  step: Step,
  afterStepId: string | null = null,
): RoutineDef {
  const tracks = def.tracks.map((track, i): Track => {
    if (i !== trackIdx) return track;
    const branchIdx = track.steps.findIndex(
      (s) => s.id === branchStepId && 'control' in s && s.control === 'branch',
    );
    if (branchIdx === -1) return { ...track }; // no such branch here — no-op
    const branch = track.steps[branchIdx] as ControlStep & { control: 'branch' };
    const armIds = branch[arm];

    let newArmIds: string[];
    let insertIdx = branchIdx + 1;
    if (afterStepId === branchStepId) {
      // Arm-position-0: the ＋ straight out of the branch (empty arm's
      // dangling ＋, or the lead ＋ into a populated arm's first node) —
      // front of the arm list, storage right after the branch.
      newArmIds = [step.id, ...armIds];
    } else {
      const armPos = afterStepId === null ? -1 : armIds.indexOf(afterStepId);
      // Arm-list position: right after afterStepId when it's in the arm,
      // appended otherwise.
      newArmIds =
        armPos === -1
          ? [...armIds, step.id]
          : [...armIds.slice(0, armPos + 1), step.id, ...armIds.slice(armPos + 1)];
      // Storage splice position: adjacent to the arm step being inserted
      // after; for an append, after the LAST of the arm's steps that
      // actually lives in this track, falling back to right after the
      // branch itself.
      if (armPos !== -1) {
        const idx = track.steps.findIndex((s) => s.id === afterStepId);
        if (idx !== -1) insertIdx = idx + 1;
      } else {
        for (const id of armIds) {
          const idx = track.steps.findIndex((s) => s.id === id);
          if (idx !== -1 && idx + 1 > insertIdx) insertIdx = idx + 1;
        }
      }
    }

    const steps = track.steps.slice();
    steps.splice(insertIdx, 0, step);
    // insertIdx is always > branchIdx, so the branch's own index is stable.
    steps[branchIdx] = { ...branch, [arm]: newArmIds };
    return { ...track, steps };
  });
  return { ...def, tracks };
}

/** Remove the step with `stepId`, searching every track (a caller doesn't
 * need to know which track a step lives in — control steps like `retry`
 * reference other steps by id across the whole def, not just their own
 * track), AND scrub the removed id from every branch's `then`/`else` arm
 * list across ALL tracks. The scrub is load-bearing, not cosmetic: because
 * `nextStepId` deliberately recycles freed ids, a dangling arm entry would
 * silently attach the next UNRELATED step that happens to get the recycled
 * id (delete s3 from `then:['s3']` → insert anywhere → new step is s3 →
 * phantom arm membership, double-render). Arms can only reference same-track
 * steps today, but the scrub walks every track — it's the same walk, and it
 * keeps the invariant global. A missing id is a no-op (still returns a
 * fresh, equal-by-value def, never mutates or throws); tracks with neither
 * the step nor a referencing branch keep their references, as do branches
 * that don't reference the id. */
export function removeStep(def: RoutineDef, stepId: string): RoutineDef {
  const referencesId = (s: Step): boolean =>
    'control' in s && s.control === 'branch' && (s.then.includes(stepId) || s.else.includes(stepId));
  const tracks = def.tracks.map((track): Track => {
    const holdsStep = track.steps.some((s) => s.id === stepId);
    const holdsRef = track.steps.some(referencesId);
    if (!holdsStep && !holdsRef) return track; // untouched track keeps its reference
    const steps = track.steps
      .filter((s) => s.id !== stepId)
      .map((s): Step => {
        if (!referencesId(s)) return s; // untouched step keeps its reference
        const branch = s as ControlStep & { control: 'branch' };
        return {
          ...branch,
          then: branch.then.filter((id) => id !== stepId),
          else: branch.else.filter((id) => id !== stepId),
        };
      });
    return { ...track, steps };
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
  Pick<
    RoutineDef,
    'routine' | 'transmit_mode' | 'transmit_ack' | 'write_ack' | 'on_interrupted' | 'inputs' | 'triggers'
  >
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
