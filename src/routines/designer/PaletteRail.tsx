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
import { useEffect, useState } from 'react';
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
  armed,
  lib,
  onClick,
}: {
  label: string;
  testId: string;
  flags?: Array<'RIG' | 'TX' | 'NET'>;
  armed: boolean;
  lib?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`pal-item${lib ? ' lib' : ''}`}
      data-testid={testId}
      disabled={!armed}
      onClick={onClick}
    >
      <span>{label}</span>
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

  const radioActions = actions.filter((a) => a.needsRadio && matches(a.name));
  const internetActions = actions.filter((a) => !a.needsRadio && a.needsInternet && matches(a.name));
  const localActions = actions.filter((a) => !a.needsRadio && !a.needsInternet && matches(a.name));
  const controlItems = CONTROL_FLOW_ITEMS.filter((c) => matches(c.label));
  const libraryItems = routines.filter((r) => matches(r.routine));

  const armed = armedInsert !== null;

  function insertAction(name: string) {
    if (!armedInsert) return;
    onInsert({ id: nextStepId(def), action: name, params: {} });
  }

  function insertControl(kind: ControlKind) {
    if (!armedInsert) return;
    onInsert(buildControlStep(kind, nextStepId(def), armedInsert.afterStepId));
  }

  function insertLibraryCall(routineName: string) {
    if (!armedInsert) return;
    onInsert({ id: nextStepId(def), control: 'call', routine: routineName });
  }

  return (
    <div className="palette" data-testid="palette-rail">
      <div className="pal-head">INSERT STEP</div>
      <input
        className="pal-search"
        data-testid="palette-filter"
        placeholder="Filter actions…"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      <div className="pal-hint" data-testid="palette-hint">
        {armed ? (
          <>
            Insert point <b>armed</b>. Pick an action below.
          </>
        ) : (
          'Arm an insert point (＋ on the canvas) first.'
        )}
      </div>
      <div className="pal-scroll">
        {radioActions.length > 0 && (
          <>
            <div className="pal-group radio">RADIO</div>
            {radioActions.map((a) => (
              <PaletteItem
                key={a.name}
                label={a.name}
                testId={`palette-item-${a.name}`}
                flags={flagsFor(a)}
                armed={armed}
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
                label={a.name}
                testId={`palette-item-${a.name}`}
                flags={flagsFor(a)}
                armed={armed}
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
                label={a.name}
                testId={`palette-item-${a.name}`}
                flags={flagsFor(a)}
                armed={armed}
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
                armed={armed}
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
                armed={armed}
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
