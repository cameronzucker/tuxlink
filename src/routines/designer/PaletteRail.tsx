/**
 * PaletteRail — the Design tab's right-rail step-insertion palette (routines
 * plan-5 Task 11, `.superpowers/sdd/task-11-brief.md`, spec §5/§12 flow 2).
 *
 * Pure controlled component: `RoutineDesigner` owns `def` (the live draft),
 * the fetched `actions` registry, and the armed-insert-point state
 * (`ArmedInsertPosition | null`, Task 10's seam) and passes them down. This
 * component never calls `defDraft.insertStep` itself — it only *builds* the
 * `Step` value (via `defDraft.nextStepId` for the id) and hands it to
 * `onInsert`; `RoutineDesigner`'s `onInsert` handler is what actually splices
 * it into the draft at the armed position via `insertStep`. Mirrors
 * CanvasTab's ownership split (geometry/rendering here, edit ops upstream).
 *
 * Groups, per the approved mock (dev/scratch/routines-ui-mocks/
 * designer-canvas.html's `.palette`) and the task brief:
 *   - RADIO / INTERNET / LOCAL — derived from each `ActionInfo`'s capability
 *     flags using the SAME mapping as `canvasModel.ts`'s `actionCategory`
 *     (needsRadio → RADIO, else needsInternet → INTERNET, else LOCAL) — never
 *     a hardcoded action-name list (Global Constraint 3). An action absent
 *     from every known name (e.g. a future `zzz.custom`) still lands in the
 *     right group purely from its flags.
 *   - CONTROL FLOW — static language constructs, not registry actions: the
 *     real `Step` union has FIVE control kinds (branch/delay/retry/call/end,
 *     routinesApi.ts's `ControlStep` — the plan prose's four-item list omits
 *     retry, which the wire type does define).
 *   - LIBRARY — saved routines from `listRoutines()` (spec §7 composition),
 *     each inserted as a `{control:'call', routine: name}` step distinct from
 *     CONTROL FLOW's generic "call routine" (routine left blank, filled in
 *     via StepInspector's dropdown).
 *
 * Drag-from-palette (the mock's secondary affordance) is DELIBERATELY
 * SKIPPED — click-arm-then-pick (this component's actual interaction) is the
 * required flow per the task brief and fully satisfies step insertion; HTML5
 * DnD would add real drag-state/drop-target plumbing that isn't "trivial on
 * the same handler," so it's left out rather than half-built. Noted in the
 * Task 11 PR body.
 */
import { useEffect, useState, type ReactNode } from 'react';
import { listRoutines, type ActionInfo, type RoutineDef, type RoutineSummary, type Step } from '../routinesApi';
import { nextStepId } from './defDraft';
import type { ArmedInsertPosition } from './CanvasTab';
import './PaletteRail.css';

export interface PaletteRailProps {
  /** The live draft — read-only here, needed only for `nextStepId`'s
   *  max-across-every-track scan. */
  def: RoutineDef;
  actions: ActionInfo[];
  armedInsert: ArmedInsertPosition | null;
  onInsert: (step: Step) => void;
}

type ControlKind = 'branch' | 'delay' | 'retry' | 'call' | 'end';

const CONTROL_FLOW_ITEMS: ReadonlyArray<{ kind: ControlKind; label: string }> = [
  { kind: 'branch', label: 'branch' },
  { kind: 'delay', label: 'delay' },
  { kind: 'retry', label: 'retry (wrap step)' },
  { kind: 'call', label: 'call routine' },
  { kind: 'end', label: 'end' },
];

/** Builds the real control-step shape for each CONTROL FLOW item, with
 *  sensible seed values (task brief: "sensible seed values" for retry).
 *  `wrapStepId` seeds retry's `step` field from the armed position's
 *  `afterStepId` — "retry (wrap step)" wraps the step immediately before the
 *  insert point, which is exactly what's armed. */
function buildControlStep(kind: ControlKind, id: string, wrapStepId: string | null): Step {
  switch (kind) {
    case 'branch':
      return { id, control: 'branch', on: '', then: [], else: [] };
    case 'delay':
      return { id, control: 'delay', delay: '1m' };
    case 'retry':
      return { id, control: 'retry', step: wrapStepId ?? '', attempts: 3, backoff_s: 2 };
    case 'call':
      return { id, control: 'call', routine: '' };
    case 'end':
      return { id, control: 'end', failed: false };
  }
}

function flagsFor(info: ActionInfo): Array<'RIG' | 'TX' | 'NET'> {
  const flags: Array<'RIG' | 'TX' | 'NET'> = [];
  if (info.needsRadio) flags.push('RIG');
  if (info.transmits) flags.push('TX');
  if (info.needsInternet) flags.push('NET');
  return flags;
}

function PaletteItem({
  label,
  testId,
  flags,
  lib,
  sub,
  desc,
  onClick,
}: {
  label: string;
  testId: string;
  flags?: Array<'RIG' | 'TX' | 'NET'>;
  lib?: boolean;
  /** Raw registry id, rendered as mono subtext under a human label
   *  (tuxlink-5lfxk). Omitted when the label IS the id. */
  sub?: string;
  /** One-line human description — surfaces as the hover tooltip. */
  desc?: string;
  onClick: () => void;
}) {
  // tuxlink-7ewvq item 2: NEVER disabled. The old disabled-until-armed
  // palette rendered dim and inert, and read as broken; an unarmed click now
  // appends to the end of the routine (the owner resolves the position).
  return (
    <button
      type="button"
      className={`pal-item${lib ? ' lib' : ''}`}
      data-testid={testId}
      title={desc}
      onClick={onClick}
    >
      <span className="pal-label">
        <span>{label}</span>
        {sub && <span className="pal-sub">{sub}</span>}
      </span>
      {flags && flags.length > 0 && (
        <span className="flags">
          {flags.map((f) => (
            <span key={f} className={`flag ${f.toLowerCase()}`}>
              {f}
            </span>
          ))}
        </span>
      )}
    </button>
  );
}

export function PaletteRail({ def, actions, armedInsert, onInsert }: PaletteRailProps) {
  const [filter, setFilter] = useState('');

  // The LIBRARY group's saved-routine list — fetched here (not threaded down
  // as a prop) since it's this group's own concern, mirroring how
  // RoutineDesigner fetches `actions` once for the whole designer.
  const [routines, setRoutines] = useState<RoutineSummary[]>([]);
  useEffect(() => {
    let cancelled = false;
    listRoutines()
      .then((list) => {
        // Defensive: a test harness or misconfigured mock that doesn't stub
        // this command resolves `undefined`, which must never propagate into
        // `.filter()` below and crash the render.
        if (!cancelled) setRoutines(Array.isArray(list) ? list : []);
      })
      .catch(() => {
        if (!cancelled) setRoutines([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const q = filter.trim().toLowerCase();
  const matches = (label: string) => q === '' || label.toLowerCase().includes(q);

  // tuxlink-5lfxk: the filter matches the human label OR the raw id, so
  // "compose" and "local.compose" both find the same entry.
  const matchesAction = (a: ActionInfo) => matches(a.name) || matches(a.label);
  const radioActions = actions.filter((a) => a.needsRadio && matchesAction(a));
  const internetActions = actions.filter((a) => !a.needsRadio && a.needsInternet && matchesAction(a));
  const localActions = actions.filter((a) => !a.needsRadio && !a.needsInternet && matchesAction(a));
  const controlItems = CONTROL_FLOW_ITEMS.filter((c) => matches(c.label));
  const libraryItems = routines.filter((r) => matches(r.routine));

  // tuxlink-7ewvq item 2: items are always live. With a position chosen on
  // the canvas the step lands there; without one, the owner appends it to the
  // end of the track. This component still only builds the Step value.
  function insertAction(name: string) {
    onInsert({ id: nextStepId(def), action: name, params: {} });
  }

  function insertControl(kind: ControlKind) {
    // Retry's wrap-step seed is the chosen position's predecessor — EXCEPT on
    // an empty arm's ＋, whose `afterStepId` is the branch's own id (not a
    // step retry could sensibly wrap), and when no position is chosen at all:
    // leave `step: ''` for the operator to fill in via the inspector.
    const wrapStepId =
      !armedInsert || (armedInsert.arm && armedInsert.afterStepId === armedInsert.arm.branchId)
        ? null
        : armedInsert.afterStepId;
    onInsert(buildControlStep(kind, nextStepId(def), wrapStepId));
  }

  function insertLibraryCall(routineName: string) {
    onInsert({ id: nextStepId(def), control: 'call', routine: routineName });
  }

  /** Human name for the step a chosen insert position follows — the hint
   *  says "Adding after Connect radio", never a bare step id. */
  function stepTitleOf(stepId: string): string {
    const step = def.tracks.flatMap((t) => t.steps).find((s) => s.id === stepId);
    if (!step) return stepId;
    if ('action' in step) {
      const info = actions.find((a) => a.name === step.action);
      return info?.label || step.action;
    }
    return step.control;
  }

  function hintText(): ReactNode {
    if (!armedInsert) {
      return (
        <>
          Click an action to add it to the end of the routine — or click a ＋ on the canvas
          first to choose the exact spot.
        </>
      );
    }
    if (armedInsert.arm) {
      return (
        <>
          Adding into the <b>{armedInsert.arm.which === 'then' ? 'ok' : 'err'}</b> path — pick an
          action.
        </>
      );
    }
    if (armedInsert.afterStepId === null) {
      return (
        <>
          Adding at the <b>start</b> — pick an action.
        </>
      );
    }
    return (
      <>
        Adding after <b>{stepTitleOf(armedInsert.afterStepId)}</b> — pick an action.
      </>
    );
  }

  return (
    <div className="palette" data-testid="palette-rail">
      {/* tuxlink-iizmk round 2 (mock .rail-head): a sentence-case sans title,
          not a mono micro tag; hint above the filter, mock order. */}
      <div className="pal-head">Add a step</div>
      <div className="pal-hint" data-testid="palette-hint">
        {hintText()}
      </div>
      <input
        className="pal-search"
        data-testid="palette-filter"
        placeholder="Filter actions…"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      <div className="pal-scroll">
        {radioActions.length > 0 && (
          <>
            <div className="pal-group radio">RADIO</div>
            {radioActions.map((a) => (
              <PaletteItem
                key={a.name}
                label={a.label || a.name}
                sub={a.label ? a.name : undefined}
                desc={a.description || undefined}
                testId={`palette-item-${a.name}`}
                flags={flagsFor(a)}
                onClick={() => insertAction(a.name)}
              />
            ))}
          </>
        )}
        {internetActions.length > 0 && (
          <>
            <div className="pal-group net">INTERNET</div>
            {internetActions.map((a) => (
              <PaletteItem
                key={a.name}
                label={a.label || a.name}
                sub={a.label ? a.name : undefined}
                desc={a.description || undefined}
                testId={`palette-item-${a.name}`}
                flags={flagsFor(a)}
                onClick={() => insertAction(a.name)}
              />
            ))}
          </>
        )}
        {localActions.length > 0 && (
          <>
            <div className="pal-group local">LOCAL</div>
            {localActions.map((a) => (
              <PaletteItem
                key={a.name}
                label={a.label || a.name}
                sub={a.label ? a.name : undefined}
                desc={a.description || undefined}
                testId={`palette-item-${a.name}`}
                flags={flagsFor(a)}
                onClick={() => insertAction(a.name)}
              />
            ))}
          </>
        )}
        {controlItems.length > 0 && (
          <>
            <div className="pal-group ctl">CONTROL FLOW</div>
            {controlItems.map((c) => (
              <PaletteItem
                key={c.kind}
                label={c.label}
                testId={`palette-control-${c.kind}`}
                onClick={() => insertControl(c.kind)}
              />
            ))}
          </>
        )}
        {libraryItems.length > 0 && (
          <>
            <div className="pal-group ctl">LIBRARY</div>
            {libraryItems.map((r) => (
              <PaletteItem
                key={r.routine}
                label={`★ ${r.routine}`}
                testId={`palette-library-${r.routine}`}
                lib
                onClick={() => insertLibraryCall(r.routine)}
              />
            ))}
          </>
        )}
      </div>
    </div>
  );
}
