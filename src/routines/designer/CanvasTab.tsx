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
 *  the matching edge amber; Task 11's PaletteRail consumes the same shape. */
export interface ArmedInsertPosition {
  trackIdx: number;
  afterStepId: string | null;
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
  if (node.kind === 'end') classes.push('endnode', node.title.includes('failed') ? 'err' : 'ok');
  if (selected) classes.push('selected');
  if (node.unknown) classes.push('node-unknown');

  return (
    <div
      className={classes.join(' ')}
      data-testid={`node-${node.id}`}
      onClick={() => onSelect(node.id)}
    >
      <div className="node-head">
        {node.transmits && <span className="tx-dot" data-testid={`tx-dot-${node.id}`} />}
        <span>{node.title}</span>
        {node.unknown && <span className="unknown-badge">⚠ unknown</span>}
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
            ⌫
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
  const armed = !!armedInsert && armedInsert.trackIdx === trackIdx && armedInsert.afterStepId === edge.from;
  return (
    <div className={`edge${armed ? ' armed' : ''}`} data-testid={`edge-${edge.from}-${edge.to}`}>
      {edge.label && <span className={`lbl ${edge.label}`}>{edge.label}</span>}
      {edge.insertPoint && (
        <button
          type="button"
          className="plus"
          aria-label={`Insert step after ${edge.from}`}
          data-testid={`insert-${edge.from}`}
          onClick={() => onInsertAt({ trackIdx, afterStepId: edge.from })}
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
  const fanRows = lane.rows.slice(1);
  const branchNode = mainRow.find((n) => n.kind === 'branch');

  return (
    <div className="lane" data-testid={`lane-${trackIdx}`}>
      <span className="lane-tag">
        TRACK {trackIdx + 1} · {lane.track.toUpperCase()}
      </span>
      {deps.map((dep, i) => (
        <span key={i} className="dep-lbl" data-testid={`dep-${trackIdx}-${i}`}>
          ⇠ consumes {dep.variable} ({dep.toTrack})
        </span>
      ))}
      <div className="flow">
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
      </div>
      {fanRows.length > 0 && (
        <div className="branch-out">
          {fanRows.map((row, rowIdx) => {
            const label = rowIdx === 0 ? 'ok' : 'err';
            const leadEdge = branchNode
              ? lane.edges.find((e) => e.from === branchNode.id && e.label === label)
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
              </div>
            );
          })}
        </div>
      )}
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
      <button type="button" className="btn add-track-btn" data-testid="add-track-btn" onClick={onAddTrack}>
        ＋ Add track
      </button>
    </div>
  );
}
