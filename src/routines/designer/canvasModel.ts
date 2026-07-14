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
}

export interface CanvasEdge {
  from: string;
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
      };
  }
}

// ============================================================================
// Chain builder (used both for a track's main row and a branch's then/else)
// ============================================================================

function buildChain(
  ids: string[],
  stepsById: Map<string, Step>,
  actionsByName: Map<string, ActionInfo>,
): { nodes: CanvasNode[]; edges: CanvasEdge[] } {
  const nodes: CanvasNode[] = [];
  const edges: CanvasEdge[] = [];
  let prevId: string | null = null;
  for (const id of ids) {
    const step = stepsById.get(id);
    if (!step) continue; // dangling then/else reference — the validator's own finding covers this; never crash here
    const node = toNode(step, actionsByName);
    nodes.push(node);
    if (prevId) edges.push({ from: prevId, to: node.id, insertPoint: true, insertAfter: prevId });
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

      if ('control' in step && step.control === 'branch') {
        branchStep = step;
        break; // steps past the branch are reached via branchStep.then/else below, not main-row order
      }
    }

    const rows: CanvasNode[][] = [mainRow];

    if (branchStep) {
      const thenChain = buildChain(branchStep.then, stepsById, actionsByName);
      const elseChain = buildChain(branchStep.else, stepsById, actionsByName);
      for (const n of thenChain.nodes) placed.add(n.id);
      for (const n of elseChain.nodes) placed.add(n.id);
      rows.push(thenChain.nodes);
      rows.push(elseChain.nodes);
      if (thenChain.nodes[0]) {
        edges.push({
          from: branchStep.id,
          to: thenChain.nodes[0].id,
          label: 'ok',
          insertPoint: true,
          insertAfter: branchStep.id,
        });
      }
      edges.push(...thenChain.edges);
      if (elseChain.nodes[0]) {
        edges.push({
          from: branchStep.id,
          to: elseChain.nodes[0].id,
          label: 'err',
          insertPoint: true,
          insertAfter: branchStep.id,
        });
      }
      edges.push(...elseChain.edges);
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
