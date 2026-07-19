/**
 * CanvasTab — the Design tab's auto-laid-out routine canvas (routines
 * plan-5 Task 10, `.superpowers/sdd/task-10-brief.md`, spec §5/§12 flow 2).
 *
 * Pure DOM/flex rendering of `canvasModel.ts`'s `layoutCanvas(draft, actions)`
 * output — no canvas element, no SVG lib, no drag geometry (the engine owns
 * geometry; this component lays lanes/rows out with flexbox, never pixel
 * coordinates). Structure transplanted from the approved mock
 * (dev/scratch/routines-ui-mocks/designer-canvas.html): one `.lane` per
 * track, a `.flow` for the main row, a `.branch-out` of two `.path`s when a
 * branch fans the lane, `.node`/`.edge` per step/connector, `.dep-lbl` for a
 * cross-track dependency, `.anchor-rule` for delay/schedule anchors.
 *
 * Ownership (binding constraint 4): the armed-insert-point state
 * (`{trackIdx, afterStepId} | null`) is NOT local to this component — it
 * lives in `RoutineDesigner` (the single owner Task 11's PaletteRail will
 * also need to read/clear) and is threaded down as the controlled
 * `armedInsert` prop. Clicking any edge's ＋ calls `onInsertAt` with that
 * edge's position; `RoutineDesigner`'s `onInsertAt` handler is what actually
 * arms/toggles its `armedInsert` state — this component only renders
 * whichever position is currently armed as `.edge.armed` (amber) and never
 * mutates the draft itself.
 */
import { useEffect, useMemo } from 'react';
import type { ActionInfo, RoutineDef } from '../routinesApi';
import { layoutCanvas, type CanvasEdge, type CanvasNode, type CanvasLane, type CrossTrackDep } from './canvasModel';
import './CanvasTab.css';

/** The armed insert point — owned by `RoutineDesigner`, read here to render
 *  the matching edge amber; Task 11's PaletteRail consumes the same shape.
 *  `arm` is present only when an ARM insert edge was armed (see
 *  `CanvasEdge.arm`): RoutineDesigner routes such an insert through
 *  `defDraft.insertStepIntoBranchArm` so the step lands IN the branch arm
 *  (fan row + then/else list), not spliced-after-the-branch into the
 *  unplaced row. */
export interface ArmedInsertPosition {
  trackIdx: number;
  afterStepId: string | null;
  arm?: { branchId: string; which: 'then' | 'else' };
}

/** Arm-marker equality (both absent, or same branch + same arm). */
export function sameArm(a: ArmedInsertPosition['arm'], b: ArmedInsertPosition['arm']): boolean {
  if (!a || !b) return !a && !b;
  return a.branchId === b.branchId && a.which === b.which;
}

export interface CanvasTabProps {
  draft: RoutineDef;
  actions: ActionInfo[];
  selectedStepId: string | null;
  onSelect: (stepId: string | null) => void;
  armedInsert: ArmedInsertPosition | null;
  onInsertAt: (pos: ArmedInsertPosition) => void;
  onRemoveStep: (stepId: string) => void;
  onAddTrack: () => void;
}

function NodeView({
  node,
  selected,
  onSelect,
  onRemoveStep,
}: {
  node: CanvasNode;
  selected: boolean;
  onSelect: (stepId: string | null) => void;
  onRemoveStep: (stepId: string) => void;
}) {
  const classes = ['node', `cat-${node.category}`];
  if (node.kind === 'trigger') classes.push('trigger');
  if (node.kind === 'end') classes.push('endnode', node.failed ? 'err' : 'ok');
  if (selected) classes.push('selected');
  if (node.unknown) classes.push('node-unknown');
  if (node.unplaced) classes.push('node-unplaced');

  // tuxlink-iizmk round 2 (mock node-head): the human label leads, the step
  // id rides as a mono badge. canvasModel titles step nodes "<id> <label>";
  // split that prefix off for display rather than reshaping the model.
  const hasIdPrefix = node.title.startsWith(`${node.id} `);
  const headLabel = hasIdPrefix ? node.title.slice(node.id.length + 1) : node.title;

  return (
    <div
      className={classes.join(' ')}
      data-testid={`node-${node.id}`}
      onClick={() => onSelect(node.id)}
    >
      <div className="node-head">
        {node.transmits && <span className="tx-dot" data-testid={`tx-dot-${node.id}`} />}
        <span>{headLabel}</span>
        {hasIdPrefix && <span className="node-id">{node.id}</span>}
        {node.unknown && <span className="unknown-badge">⚠ unknown</span>}
        {node.unplaced && <span className="unknown-badge">⚠ unplaced</span>}
        {selected && node.kind !== 'trigger' && (
          <button
            type="button"
            className="node-delete"
            data-testid={`delete-${node.id}`}
            aria-label={`Delete ${node.id}`}
            onClick={(e) => {
              e.stopPropagation();
              onRemoveStep(node.id);
            }}
          >
            {/* item 5 (bd tuxlink-iizmk): × replaces ⌫ — the erase-left glyph
                fell back to a substituted font on WebKitGTK and rendered as a
                few clipped pixels at the node-head's meta font size. */}
            ×
          </button>
        )}
      </div>
      {node.bodyLines.length > 0 && (
        <div className="node-body">
          {node.bodyLines.map((line, i) => (
            <div key={i}>{line}</div>
          ))}
        </div>
      )}
    </div>
  );
}

function EdgeView({
  edge,
  trackIdx,
  armedInsert,
  onInsertAt,
}: {
  edge: CanvasEdge;
  trackIdx: number;
  armedInsert: ArmedInsertPosition | null;
  onInsertAt: (pos: ArmedInsertPosition) => void;
}) {
  // Arm/highlight from `edge.insertAfter` + the arm marker — the model-owned
  // insert semantics (`null` = prepend for trigger/head edges; `arm` =
  // branch-arm insert), never `edge.from` ('trigger-0' would findIndex-miss
  // in defDraft.insertStep and APPEND). The arm marker participates in the
  // match so an arm ＋ and the plain splice ＋ that share an `insertAfter`
  // (both anchored on the branch id) never both light up.
  const armed =
    edge.insertPoint &&
    !!armedInsert &&
    armedInsert.trackIdx === trackIdx &&
    armedInsert.afterStepId === edge.insertAfter &&
    sameArm(armedInsert.arm, edge.arm);
  // The two branch lead edges share `from` (the branch id) — the label
  // suffix keeps their testids unique (insert-s2-ok / insert-s2-err).
  const insertTestId = `insert-${edge.from}${edge.label ? `-${edge.label}` : ''}`;
  // A dangling edge (`to: ''` — empty track, trailing append, empty arm) has
  // no target node — render the lone ＋ without the arrowhead pointing at
  // nothing.
  const dangling = edge.to === '';
  const ariaLabel = edge.arm
    ? `Insert step into ${edge.arm.branchId}'s ${edge.arm.which === 'then' ? 'ok' : 'err'} arm`
    : edge.insertAfter === null
      ? 'Insert step at the start of the track'
      : `Insert step after ${edge.insertAfter}`;
  return (
    <div
      className={`edge${armed ? ' armed' : ''}${dangling ? ' dangling' : ''}`}
      data-testid={`edge-${edge.from}-${edge.to}`}
    >
      {edge.label && <span className={`lbl ${edge.label}`}>{edge.label}</span>}
      {edge.insertPoint && (
        <button
          type="button"
          className="plus"
          aria-label={ariaLabel}
          data-testid={insertTestId}
          onClick={() =>
            onInsertAt(
              edge.arm
                ? { trackIdx, afterStepId: edge.insertAfter, arm: edge.arm }
                : { trackIdx, afterStepId: edge.insertAfter },
            )
          }
        >
          ＋
        </button>
      )}
    </div>
  );
}

/** One node, plus its leading edge (the connector from the PREVIOUS node in
 *  this row) when there is one — `prevId === null` for a row's first node,
 *  since a fan-out row's own leading edge (the labeled ok/err connector out
 *  of the branch) is rendered by `Lane` itself, not here. */
function FlowSegment({
  node,
  prevId,
  edges,
  trackIdx,
  selected,
  armedInsert,
  onSelect,
  onInsertAt,
  onRemoveStep,
}: {
  node: CanvasNode;
  prevId: string | null;
  edges: CanvasEdge[];
  trackIdx: number;
  selected: boolean;
  armedInsert: ArmedInsertPosition | null;
  onSelect: (stepId: string | null) => void;
  onInsertAt: (pos: ArmedInsertPosition) => void;
  onRemoveStep: (stepId: string) => void;
}) {
  const edge = prevId ? edges.find((e) => e.from === prevId && e.to === node.id) : undefined;
  return (
    <>
      {edge && <EdgeView edge={edge} trackIdx={trackIdx} armedInsert={armedInsert} onInsertAt={onInsertAt} />}
      <NodeView node={node} selected={selected} onSelect={onSelect} onRemoveStep={onRemoveStep} />
    </>
  );
}

function Lane({
  lane,
  trackIdx,
  deps,
  selectedStepId,
  armedInsert,
  onSelect,
  onInsertAt,
  onRemoveStep,
}: {
  lane: CanvasLane;
  trackIdx: number;
  deps: CrossTrackDep[];
  selectedStepId: string | null;
  armedInsert: ArmedInsertPosition | null;
  onSelect: (stepId: string | null) => void;
  onInsertAt: (pos: ArmedInsertPosition) => void;
  onRemoveStep: (stepId: string) => void;
}) {
  const mainRow = lane.rows[0] ?? [];
  const extraRows = lane.rows.slice(1);
  // The final row is all-`unplaced` when the layout couldn't place some steps
  // (canvasModel appends it after the branch fan-out rows); a fan row's first
  // node is never unplaced, so the flag on `row[0]` separates the two kinds.
  const fanRows = extraRows.filter((row) => !row[0]?.unplaced);
  const unplacedRows = extraRows.filter((row) => row[0]?.unplaced === true);
  const branchNode = mainRow.find((n) => n.kind === 'branch');
  // A headless lane (no trigger heads — every lane but the first) gets a
  // synthetic prepend edge from the model; FlowSegment only renders edges
  // BETWEEN nodes, so the head edge is rendered here, before the first node.
  const headEdge =
    mainRow.length > 0
      ? lane.edges.find((e) => e.from === `head-${trackIdx}` && e.to === mainRow[0]!.id)
      : undefined;
  // The main row's DANGLING insert edge (`to: ''`, no arm marker — arm
  // dangling edges render inside their fan paths below): an empty track's
  // lone ＋ (New Routine… / Add track) or a non-empty row's trailing
  // append-at-end ＋. Rendered after whatever the main flow holds.
  const danglingEdge = lane.edges.find((e) => e.to === '' && !e.arm);

  return (
    <div className="lane" data-testid={`lane-${trackIdx}`}>
      {/* tuxlink-7ewvq: a generic auto-name (track-1, track-2, …) is display
          noise — 'TRACK 1 · TRACK-1' explained nothing. Only a name the
          operator actually chose earns a spot next to the number. */}
      <span
        className="lane-tag"
        title="Tracks run in parallel when the routine starts; steps inside a track run in order."
      >
        {/^track-?\d+$/i.test(lane.track)
          ? `TRACK ${trackIdx + 1}`
          : `TRACK ${trackIdx + 1} · ${lane.track.toUpperCase()}`}
      </span>
      {deps.map((dep, i) => (
        <span key={i} className="dep-lbl" data-testid={`dep-${trackIdx}-${i}`}>
          ⇠ consumes {dep.variable} ({dep.toTrack})
        </span>
      ))}
      <div className="flow">
        {headEdge && (
          <EdgeView edge={headEdge} trackIdx={trackIdx} armedInsert={armedInsert} onInsertAt={onInsertAt} />
        )}
        {mainRow.map((node, i) => (
          <FlowSegment
            key={node.id}
            node={node}
            prevId={i > 0 ? (mainRow[i - 1]?.id ?? null) : null}
            edges={lane.edges}
            trackIdx={trackIdx}
            selected={node.id === selectedStepId}
            armedInsert={armedInsert}
            onSelect={onSelect}
            onInsertAt={onInsertAt}
            onRemoveStep={onRemoveStep}
          />
        ))}
        {danglingEdge && (
          <EdgeView edge={danglingEdge} trackIdx={trackIdx} armedInsert={armedInsert} onInsertAt={onInsertAt} />
        )}
      </div>
      {fanRows.length > 0 && (
        <div className="branch-out">
          {fanRows.map((row, rowIdx) => {
            const label = rowIdx === 0 ? 'ok' : 'err';
            const which = rowIdx === 0 ? 'then' : 'else';
            // Lead edge into the arm's first node — or, for an EMPTY arm, the
            // dangling arm insert ＋ straight out of the branch (same `from`
            // + `label`, `to: ''`, arm marker set), found by the same query.
            const leadEdge = branchNode
              ? lane.edges.find((e) => e.from === branchNode.id && e.label === label)
              : undefined;
            // Trailing arm append ＋ after the arm's last node (absent when
            // the arm is empty — the lead edge IS the arm ＋ then — or ends
            // in an `end` node).
            const trailingArmEdge =
              branchNode && row.length > 0
                ? lane.edges.find(
                    (e) => e.to === '' && e.arm?.branchId === branchNode.id && e.arm.which === which,
                  )
                : undefined;
            return (
              <div className="path" key={label}>
                {leadEdge && (
                  <EdgeView edge={leadEdge} trackIdx={trackIdx} armedInsert={armedInsert} onInsertAt={onInsertAt} />
                )}
                {row.map((node, i) => (
                  <FlowSegment
                    key={node.id}
                    node={node}
                    prevId={i > 0 ? (row[i - 1]?.id ?? null) : null}
                    edges={lane.edges}
                    trackIdx={trackIdx}
                    selected={node.id === selectedStepId}
                    armedInsert={armedInsert}
                    onSelect={onSelect}
                    onInsertAt={onInsertAt}
                    onRemoveStep={onRemoveStep}
                  />
                ))}
                {trailingArmEdge && (
                  <EdgeView
                    edge={trailingArmEdge}
                    trackIdx={trackIdx}
                    armedInsert={armedInsert}
                    onInsertAt={onInsertAt}
                  />
                )}
              </div>
            );
          })}
        </div>
      )}
      {unplacedRows.map((row, i) => (
        <div className="flow unplaced-row" key={`unplaced-${i}`} data-testid={`unplaced-row-${trackIdx}`}>
          {row.map((node) => (
            <NodeView
              key={node.id}
              node={node}
              selected={node.id === selectedStepId}
              onSelect={onSelect}
              onRemoveStep={onRemoveStep}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

export function CanvasTab({
  draft,
  actions,
  selectedStepId,
  onSelect,
  armedInsert,
  onInsertAt,
  onRemoveStep,
  onAddTrack,
}: CanvasTabProps) {
  const model = useMemo(() => layoutCanvas(draft, actions), [draft, actions]);

  // ⌫ / Delete / Backspace on a selected node removes it (binding constraint
  // 3) — a window-level listener so the key works regardless of which node
  // element last had DOM focus; guarded against hijacking an input/textarea
  // elsewhere on the page (e.g. the designer's name field, the Export JSON
  // dialog's textarea).
  useEffect(() => {
    if (!selectedStepId) return;
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key !== 'Delete' && e.key !== 'Backspace') return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')) return;
      e.preventDefault();
      onRemoveStep(selectedStepId as string);
    }
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selectedStepId, onRemoveStep]);

  return (
    <div className="canvas" data-testid="canvas-tab">
      {model.anchors.length > 0 && (
        <div className="anchor-rule" data-testid="anchor-rule">
          {model.anchors.map((anchor, i) => (
            <span key={i} className="anchor-lbl">
              ⚓ {anchor.label}
            </span>
          ))}
        </div>
      )}
      {model.lanes.map((lane, trackIdx) => (
        <Lane
          key={lane.track}
          lane={lane}
          trackIdx={trackIdx}
          deps={model.crossTrackDeps.filter((d) => d.fromTrack === lane.track)}
          selectedStepId={selectedStepId}
          armedInsert={armedInsert}
          onSelect={onSelect}
          onInsertAt={onInsertAt}
          onRemoveStep={onRemoveStep}
        />
      ))}
      <button
        type="button"
        className="btn add-track-btn"
        data-testid="add-track-btn"
        title="Adds a second track that runs in parallel with this one when the routine starts."
        onClick={onAddTrack}
      >
        ＋ Add parallel track
      </button>
    </div>
  );
}
