/**
 * canvasModel.ts — pure def→render-model for the Design tab canvas (routines
 * plan-5 Task 10, `.superpowers/sdd/task-10-brief.md`, spec §5: "the engine
 * owns geometry; the definition carries only logic, never coordinates").
 *
 * No Tauri, no React, no DOM: `layoutCanvas(def, actions)` is a pure function
 * from a `RoutineDef` (routinesApi.ts) + the `ActionInfo` registry to a small
 * render model (`CanvasLane[]` + `CanvasAnchor[]` + cross-track dep list) that
 * `CanvasTab.tsx` renders with plain DOM/flex — no canvas element, no SVG
 * lib, no drag/pixel geometry. Every step id is treated as globally unique
 * across the whole def (never re-scoped per track), matching how control
 * steps (`branch.then`/`branch.else`, `retry.step`) already reference other
 * steps by id regardless of which track they live in.
 */
import type { ActionInfo, ControlStep, RoutineDef, Step, Track, Trigger } from '../routinesApi';
import { formatTrigger } from '../format';

// ============================================================================
// Render model (task-10 brief's exported interfaces, extended per binding
// constraints: `kind` gains `'retry'` — the real `Step` union has FIVE
// control kinds (branch/delay/retry/call/end), not the four the plan prose
// lists — and `CanvasNode` gains `unknown` so an action absent from the
// ActionInfo registry can be flagged without inventing a sixth `category`).
// ============================================================================

export interface CanvasNode {
  id: string;
  kind: 'trigger' | 'action' | 'branch' | 'delay' | 'retry' | 'call' | 'end';
  title: string;
  bodyLines: string[];
  category: 'radio' | 'data' | 'local' | 'ctl';
  transmits: boolean;
  /** True only for an action step whose `action` name has no entry in the
   *  `ActionInfo[]` registry (the validator's own UNKNOWN_ACTION finding
   *  covers the error path — this model's job is only to never crash
   *  rendering one). Always `false` for trigger/control nodes. */
  unknown: boolean;
  /** True for a step the layout could not place in any flow row — a step
   *  sitting after a branch in `track.steps` that no branch arm references.
   *  Such steps are appended as the lane's FINAL row so the canvas never
   *  silently hides a step that IS in the def (a canvas that hides steps
   *  lies to the operator). The validator owns flagging WHY it's
   *  unreachable; this model's job is only to keep it visible. */
  unplaced: boolean;
  /** `end` nodes only: the end step's own `failed` flag (drives the ok/err
   *  end-node styling — the view must never re-derive it from title text).
   *  Absent on every other kind. */
  failed?: boolean;
}

export interface CanvasEdge {
  from: string;
  /** Target node id — or `''` for a DANGLING insert edge (a lone ＋ with no
   *  node after it): an empty track's only edge, a non-empty row's trailing
   *  append-at-end ＋, or an empty/extendable branch arm's ＋ — so no
   *  authoring position is ever unreachable from the canvas. */
  to: string;
  label?: 'ok' | 'err';
  insertPoint: boolean;
  /** What arming this edge's ＋ means for `defDraft.insertStep`: the step id
   *  to insert AFTER, or `null` for "prepend to the track" (`insertStep`'s
   *  documented `afterStepId === null` contract). `null` on the edge out of
   *  the trigger head — `'trigger-0'` is a synthetic node id, not a step id
   *  `insertStep` could ever find (its findIndex-miss APPENDS, the exact
   *  opposite of what inserting at the head of the flow means) — and on the
   *  synthetic head edge of a trigger-less lane. */
  insertAfter: string | null;
  /** Present only on an ARM insert edge (Task 11 authoring fix, Gap A):
   *  arming it routes the insert through
   *  `defDraft.insertStepIntoBranchArm(def, trackIdx, branchId, which, step,
   *  afterStepId)` — which splices storage adjacently AND inserts the new
   *  step's id into the branch's then/else list at the position
   *  `insertAfter` names — instead of the plain `insertStep` splice (which
   *  would land the step in the unplaced row). Emitted for an EMPTY arm (a
   *  dangling labeled ＋ straight out of the branch — otherwise a
   *  canvas-authored branch with `then:[], else:[]` has NO armable position
   *  that reaches either arm), on every INTRA-arm edge between two arm nodes
   *  (so a mid-arm ＋ inserts INTO the arm at that position, not unplaced),
   *  and as the TRAILING ＋ after a non-empty arm's last node (skipped when
   *  the arm ends in an `end` step — appending after end is meaningless). */
  arm?: { branchId: string; which: 'then' | 'else' };
}

export interface CanvasLane {
  track: string;
  /** `rows[0]` is the track's main sequential flow (the routine-level trigger
   *  heads — FIRST lane only — plus steps up to and including a branch, if
   *  any). A branch step fans the lane into two more rows: the `then` (ok)
   *  chain, then the `else` (err) chain (mock's `.branch-out` > two
   *  `.path`s). If any of the track's steps were placed in NO row (after a
   *  branch, referenced by no arm), one final all-`unplaced` row carries
   *  them. No branch, nothing unplaced → `rows.length === 1`. */
  rows: CanvasNode[][];
  edges: CanvasEdge[];
}

export interface CanvasAnchor {
  label: string;
}

export interface CrossTrackDep {
  fromTrack: string;
  toTrack: string;
  variable: string;
}

export interface CanvasModel {
  lanes: CanvasLane[];
  anchors: CanvasAnchor[];
  crossTrackDeps: CrossTrackDep[];
}

// ============================================================================
// Node builders
// ============================================================================

/** `idx` is the trigger's index in `def.triggers` (routine-level — triggers
 *  fire the whole routine, they are NOT per-track). */
function triggerNode(trigger: Trigger, idx: number): CanvasNode {
  return {
    id: `trigger-${idx}`,
    kind: 'trigger',
    title: trigger.type,
    bodyLines: trigger.type === 'schedule' ? [formatTrigger(trigger)] : [],
    category: 'ctl',
    transmits: false,
    unknown: false,
    unplaced: false,
  };
}

function actionCategory(info: ActionInfo | undefined): CanvasNode['category'] {
  if (!info) return 'local'; // unknown action — safe fallback, flagged separately via `unknown`
  if (info.needsRadio) return 'radio';
  if (info.needsInternet) return 'data';
  return 'local';
}

/** One line per top-level `params` key, e.g. `"stations @station-set:or-gateways"`. */
function summarizeParams(params: unknown): string[] {
  if (!params || typeof params !== 'object') return [];
  return Object.entries(params as Record<string, unknown>).map(
    ([key, value]) => `${key} ${typeof value === 'string' ? value : JSON.stringify(value)}`,
  );
}

const DELAY_UNIT_LABEL: Record<string, string> = { s: 'sec', m: 'min', h: 'hr' };

/** `"5m"` → `"5 min"`; a delay string that doesn't match the `<n><unit>`
 *  shape renders verbatim rather than throwing (an imported def's delay
 *  string isn't this module's to validate — that's the validator's job). */
function formatDelay(delay: string): string {
  const m = /^(\d+)([smh])$/.exec(delay);
  if (!m) return delay;
  return `${m[1]} ${DELAY_UNIT_LABEL[m[2] as 's' | 'm' | 'h']}`;
}

function toNode(step: Step, actionsByName: Map<string, ActionInfo>): CanvasNode {
  if ('action' in step) {
    const info = actionsByName.get(step.action);
    return {
      id: step.id,
      kind: 'action',
      title: `${step.id} ${step.action}`,
      bodyLines: summarizeParams(step.params),
      category: actionCategory(info),
      transmits: info?.transmits ?? false,
      unknown: !info,
      unplaced: false,
    };
  }
  return controlNode(step);
}

function controlNode(step: ControlStep): CanvasNode {
  const base = {
    id: step.id,
    category: 'ctl' as const,
    transmits: false,
    unknown: false,
    unplaced: false,
  };
  switch (step.control) {
    case 'branch':
      return { ...base, kind: 'branch', title: `${step.id} branch`, bodyLines: [`on ${step.on}`] };
    case 'delay':
      return { ...base, kind: 'delay', title: `${step.id} delay`, bodyLines: [`+${formatDelay(step.delay)}`] };
    case 'retry':
      return {
        ...base,
        kind: 'retry',
        title: `${step.id} retry ${step.step}`,
        bodyLines: [
          `attempts ${step.attempts}${step.backoff_s !== undefined ? ` · backoff ${step.backoff_s}s` : ''}`,
        ],
      };
    case 'call':
      return { ...base, kind: 'call', title: `${step.id} call ${step.routine}`, bodyLines: [] };
    case 'end':
      return {
        ...base,
        kind: 'end',
        title: `${step.id} end · ${step.failed ? 'failed' : 'complete'}`,
        bodyLines: step.reason ? [step.reason] : [],
        failed: step.failed === true,
      };
  }
}

// ============================================================================
// Chain builder — a branch's then/else fan rows
// ============================================================================

/** `arm` is stamped on every intra-chain edge, so a mid-arm ＋ (between two
 *  arm nodes) routes through `insertStepIntoBranchArm` exactly like the
 *  empty-arm and trailing-arm ＋s do — the edge's `insertAfter` (the
 *  preceding arm step) then positions the new id WITHIN the then/else list.
 *  Without the marker, a mid-arm insert took the plain `insertStep` splice
 *  and minted an unplaced step: two ＋s in the same fan row behaved
 *  differently. */
function buildChain(
  ids: string[],
  stepsById: Map<string, Step>,
  actionsByName: Map<string, ActionInfo>,
  arm: { branchId: string; which: 'then' | 'else' },
): { nodes: CanvasNode[]; edges: CanvasEdge[] } {
  const nodes: CanvasNode[] = [];
  const edges: CanvasEdge[] = [];
  let prevId: string | null = null;
  for (const id of ids) {
    const step = stepsById.get(id);
    if (!step) continue; // dangling then/else reference — the validator's own finding covers this; never crash here
    const node = toNode(step, actionsByName);
    nodes.push(node);
    if (prevId) edges.push({ from: prevId, to: node.id, insertPoint: true, insertAfter: prevId, arm });
    prevId = node.id;
  }
  return { nodes, edges };
}

// ============================================================================
// Cross-track dependency detection (binding constraint 1): scan every step's
// serialized form for `s<digits>.<field>` references and resolve the leading
// `s<digits>` against the real step-id → track-name map. Mirrors the
// validator's own `stepId.output` variable walk; a plain regex is sufficient
// here per the brief. Scanning the whole serialized step (not just `params`)
// also catches a branch's `on` and a call's `args` for free, at zero extra
// cost — same-track references (the overwhelmingly common case, e.g. a
// branch's `on: "s1.connected"` where s1 is in the same track) are filtered
// out, since only a reference to a step living in ANOTHER track is a
// cross-track dependency.
// ============================================================================

const CROSS_TRACK_REF_RE = /\bs\d+\.[a-z_]+\b/g;

function scanCrossTrackDeps(
  step: Step,
  fromTrack: string,
  stepTrack: Map<string, string>,
  out: CrossTrackDep[],
  seen: Set<string>,
): void {
  const matches = JSON.stringify(step).match(CROSS_TRACK_REF_RE);
  if (!matches) return;
  for (const variable of matches) {
    const refStepId = variable.slice(0, variable.indexOf('.'));
    const toTrack = stepTrack.get(refStepId);
    if (!toTrack || toTrack === fromTrack) continue;
    const key = `${fromTrack}|${toTrack}|${variable}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ fromTrack, toTrack, variable });
  }
}

// ============================================================================
// layoutCanvas
// ============================================================================

export function layoutCanvas(def: RoutineDef, actions: ActionInfo[]): CanvasModel {
  const actionsByName = new Map(actions.map((a) => [a.name, a]));
  const stepsById = new Map<string, Step>();
  const stepTrack = new Map<string, string>();
  for (const track of def.tracks) {
    for (const step of track.steps) {
      stepsById.set(step.id, step);
      stepTrack.set(step.id, track.name);
    }
  }

  const anchors: CanvasAnchor[] = [];
  const crossTrackDeps: CrossTrackDep[] = [];
  const seenDeps = new Set<string>();
  for (const track of def.tracks) {
    for (const step of track.steps) {
      if ('control' in step && step.control === 'delay') {
        anchors.push({ label: `+${formatDelay(step.delay)}` });
      }
      scanCrossTrackDeps(step, track.name, stepTrack, crossTrackDeps, seenDeps);
    }
  }

  const lanes: CanvasLane[] = def.tracks.map((track: Track, trackIdx: number): CanvasLane => {
    const edges: CanvasEdge[] = [];
    const mainRow: CanvasNode[] = [];
    const placed = new Set<string>();
    let prevId: string | null = null;

    // Triggers are ROUTINE-level (they fire the whole routine; the wire model
    // has no per-track trigger), so ALL of `def.triggers` head the FIRST lane
    // only. Secondary lanes render headless — their `.lane-tag` (TRACK N ·
    // NAME) is the parallel-track head label; fabricating a duplicate trigger
    // chip per lane (as the static mock happens to draw) would misstate the
    // model. Edges BETWEEN trigger heads carry no insert point (a step can't
    // go between two triggers); the edge out of the last trigger into the
    // first step arms `insertAfter: null` — defDraft.insertStep's documented
    // prepend contract ('trigger-N' is not a step id it could ever find).
    if (trackIdx === 0) {
      def.triggers.forEach((trigger, i) => {
        const node = triggerNode(trigger, i);
        if (prevId) edges.push({ from: prevId, to: node.id, insertPoint: false, insertAfter: null });
        mainRow.push(node);
        prevId = node.id;
      });
    }

    let branchStep: (ControlStep & { control: 'branch' }) | null = null;
    let lastMainStep: Step | null = null;
    let isFirstStep = true;
    for (const step of track.steps) {
      const node = toNode(step, actionsByName);
      mainRow.push(node);
      placed.add(node.id);
      if (prevId) {
        edges.push({
          from: prevId,
          to: node.id,
          insertPoint: true,
          insertAfter: isFirstStep ? null : prevId,
        });
      } else {
        // Headless lane (no trigger heads): a synthetic head edge so the
        // track's prepend position stays insertable from the canvas.
        edges.push({ from: `head-${trackIdx}`, to: node.id, insertPoint: true, insertAfter: null });
      }
      isFirstStep = false;
      prevId = node.id;
      lastMainStep = step;

      if ('control' in step && step.control === 'branch') {
        branchStep = step;
        break; // steps past the branch are reached via branchStep.then/else below, not main-row order
      }
    }

    // Trailing append-at-end insert point (Task 11 authoring fix, Gap B):
    // without it, a non-empty main row only has ＋s leading INTO nodes — a
    // single-step lane has no armable position AFTER its step, dead-ending
    // sequential authoring (flow 2's primary motion). Skipped when the row
    // ends in an `end` step (appending after end is meaningless) or in the
    // branch (the arms below carry the continuation — a plain trailing ＋
    // there would arm the same splice-after-branch position as the arm lead
    // edges and mint unplaced steps).
    if (
      lastMainStep !== null &&
      branchStep === null &&
      !('control' in lastMainStep && lastMainStep.control === 'end')
    ) {
      edges.push({
        from: lastMainStep.id,
        to: '',
        insertPoint: true,
        insertAfter: lastMainStep.id,
      });
    }

    // An EMPTY track would otherwise render with no insert point at all —
    // exactly the "New Routine…" (createDraft → empty track-1) and Add-track
    // first flows Task 11's palette starts from. Emit one DANGLING insert
    // edge (`to: ''` — no target node) arming the prepend contract: out of
    // the last trigger head on lane 0, out of the synthetic lane head on a
    // headless lane.
    if (track.steps.length === 0) {
      edges.push({
        from: prevId ?? `head-${trackIdx}`,
        to: '',
        insertPoint: true,
        insertAfter: null,
      });
    }

    const rows: CanvasNode[][] = [mainRow];

    if (branchStep) {
      const branchId = branchStep.id;
      const armChains: Array<{ which: 'then' | 'else'; label: 'ok' | 'err'; ids: string[] }> = [
        { which: 'then', label: 'ok', ids: branchStep.then },
        { which: 'else', label: 'err', ids: branchStep.else },
      ];
      for (const { which, label, ids } of armChains) {
        const chain = buildChain(ids, stepsById, actionsByName, { branchId, which });
        for (const n of chain.nodes) placed.add(n.id);
        rows.push(chain.nodes);
        const first = chain.nodes[0];
        const last = chain.nodes[chain.nodes.length - 1];
        if (first) {
          edges.push({
            from: branchId,
            to: first.id,
            label,
            insertPoint: true,
            insertAfter: branchId,
          });
        } else {
          // EMPTY arm (Gap A): a canvas-authored branch starts `then:[],
          // else:[]` — without this dangling ARM insert edge there is no
          // armable position that reaches either arm at all. Arming it
          // routes through insertStepIntoBranchArm (splice + arm-list
          // append), so the inserted step lands IN the arm, not unplaced.
          edges.push({
            from: branchId,
            to: '',
            label,
            insertPoint: true,
            insertAfter: branchId,
            arm: { branchId, which },
          });
        }
        edges.push(...chain.edges);
        // Trailing ARM append ＋ after a non-empty arm's last node (Gap B for
        // arms — sequential authoring inside an arm), skipped when the arm
        // already ends in an `end` node.
        if (last && last.kind !== 'end') {
          edges.push({
            from: last.id,
            to: '',
            insertPoint: true,
            insertAfter: last.id,
            arm: { branchId, which },
          });
        }
      }
    }

    // Any step of this track placed in NO row (after the first branch,
    // referenced by no arm) is appended as one final all-`unplaced` row —
    // surfaced, never silently dropped.
    const unplacedNodes = track.steps
      .filter((s) => !placed.has(s.id))
      .map((s): CanvasNode => ({ ...toNode(s, actionsByName), unplaced: true }));
    if (unplacedNodes.length > 0) rows.push(unplacedNodes);

    return { track: track.name, rows, edges };
  });

  return { lanes, anchors, crossTrackDeps };
}
