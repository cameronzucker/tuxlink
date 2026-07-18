/**
 * RoutineDesigner — the routine designer shell (routines plan-5 Task 9,
 * `.superpowers/sdd/task-9-brief.md`, spec §12 flows 2/5).
 *
 * Replaces RoutinesSurface's Task 7 one-line placeholder. Layout is the
 * approved mock verbatim (dev/scratch/routines-ui-mocks/designer-canvas.html):
 * header (← Routines, name, state pill, unsaved dot, Design/Runs/Settings
 * tabs, Dry-run/Export JSON/Save actions) and the always-on `.valbar`
 * validation strip. The Design tab mounts the real `CanvasTab` (Task 10,
 * `./CanvasTab.tsx`) plus, in its right rail, `PaletteRail` and
 * `StepInspector` (Task 11, `./PaletteRail.tsx` / `./StepInspector.tsx`) —
 * RoutineDesigner owns `draft`, the fetched `actions` registry,
 * `selectedStepId`, and the armed-insert-point state, and passes them down
 * as controlled props; none of the three ever call `defDraft.ts` or the
 * Tauri surface directly, only this shell does. The settings form and runs
 * list (Task 13) mount points below are still minimal inert placeholders —
 * that task replaces each outright with the real tab body.
 *
 * Load: an existing routine (`routine !== ''`) is fetched with `getRoutine`;
 * a fresh draft (`routine === ''`) is never sent to the backend — binding
 * constraint 6 — `createDraft()` supplies the skeleton and the name becomes
 * an editable field (this component is the only place a routine's name is
 * ever typed by hand).
 *
 * Validation (binding constraint 2 / flow 2 "updates continuously"): every
 * time `draft` changes — including the initial load — a 400ms debounced
 * `validateDraft(draft)` call refreshes the valbar. A rejected `invoke` (a
 * parse failure on the backend, `UiError.kind === 'Rejected'`) renders its
 * verbatim message as a single error line via `formatUiError` rather than
 * throwing or clearing the bar.
 *
 * Save (binding constraint 3 / Global Constraint 7): `saveRoutine(draft)`
 * NEVER blocks — its `SaveResult.findings` replace the valbar's content and
 * `dirty` is cleared unconditionally, even when `blocked: true`. No modal,
 * no thrown exception on a blocked save; only a genuine backend/parse error
 * (a rejected promise) surfaces as the valbar's single-error-line state.
 *
 * Dry-run (binding constraint 4 / flow 5): an implicit save (dry-run always
 * runs the STORED def — prompt-free auto-save is correct because save never
 * blocks), then `dryRunRoutine(draft.routine, {})`, then `onTabChange('runs')`
 * with the returned `runId` threaded through as `highlightRunId` to the runs
 * tab mount point (Task 13 wires the actual highlight behavior).
 *
 * Export JSON (binding constraint 5): a read-only dialog showing
 * `JSON.stringify(draft, null, 2)` + a Copy button (`navigator.clipboard`).
 * No fs-write plugin exists yet, so copy — not file-write — is the honest v1
 * (the storage format IS the export format, spec §14).
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  getRoutine,
  saveRoutine,
  validateDraft,
  dryRunRoutine,
  listActions,
  type RoutineDef,
  type Finding,
  type ActionInfo,
  type Step,
} from '../routinesApi';
import { formatUiError } from '../format';
import {
  createDraft,
  addTrack,
  removeStep,
  insertStep,
  insertStepIntoBranchArm,
  updateStep,
  updateSettings,
  type StepPatch,
} from './defDraft';
import { CanvasTab, sameArm, type ArmedInsertPosition } from './CanvasTab';
import { PaletteRail } from './PaletteRail';
import { StepInspector } from './StepInspector';
import { SettingsTab } from './SettingsTab';
import { RunsTab } from './RunsTab';
import type { DesignerTab } from '../RoutinesSurface';
import './RoutineDesigner.css';

export interface RoutineDesignerProps {
  /** Empty string means a fresh, unsaved draft (RoutinesSurface's "New
   *  Routine…" path) — the def is never fetched from the backend for it. */
  routine: string;
  tab: DesignerTab;
  onBack: () => void;
  onTabChange: (tab: DesignerTab) => void;
  /** Seed the designer from a continuity token's in-progress draft
   *  (tuxlink-dmwte task 8, spec §7). When present the designer mounts on this
   *  exact draft and SKIPS its `getRoutine` fetch — so popping from / docking
   *  back to the designer preserves the operator's unsaved canvas edits. */
  initialDraft?: RoutineDef;
  /** Reports the live draft upward on every change so a host (the popped
   *  window's registry Component, or AppShell inline) can collect it into the
   *  continuity token at pop-out / dock-back time (tuxlink-dmwte task 8). */
  onDraftChange?: (draft: RoutineDef) => void;
  /** When provided, the designer header shows a text-labeled "↗ Pop out"
   *  affordance (spec §5) that pops the Routines surface to its own window
   *  carrying THIS designer view + draft. Absent inside the popped window
   *  itself (there is nothing to pop out to). */
  onPopOut?: () => void;
}

/** Debounce window for the always-on validation bar (spec §12 flow 2). */
const VALIDATE_DEBOUNCE_MS = 400;

/* tuxlink-7ewvq: two tabs, not three. 'runs' KEEPS its wire value (routing +
 * continuity tokens store it) but reads 'History' — 'Runs' was ambiguous
 * (the verb? the history?). 'settings' is gone as a tab: its sections render
 * inline below the canvas in the Design view, so a stored tab === 'settings'
 * simply lands on Design. */
const VISIBLE_TABS: ReadonlyArray<{ tab: DesignerTab; label: string }> = [
  { tab: 'design', label: 'Design' },
  { tab: 'runs', label: 'History' },
];

/** Plain-language explanation for the header's transmit-mode chip. */
function transmitModeTooltip(mode: string): string {
  if (mode === 'attended') {
    return 'Transmit mode: attended — this routine only transmits while you are at the radio as control operator. Change it under Routine settings, below the canvas.';
  }
  if (mode === 'automatic') {
    return 'Transmit mode: automatic — this routine may transmit unattended (scheduled runs). Change it under Routine settings, below the canvas.';
  }
  return `Transmit mode: ${mode}. Change it under Routine settings, below the canvas.`;
}

function stepCountOf(def: RoutineDef): number {
  return def.tracks.reduce((n, t) => n + t.steps.length, 0);
}

/** The always-on validation bar (task-9 brief binding constraint 2,
 * transplanted from designer-canvas.html's `.valbar`). A non-null
 * `parseFailure` (a rejected `validateDraft`/`saveRoutine` call) renders its
 * verbatim message as a single error line instead of the error/warning
 * counts — there's no finding list to summarize when the def itself didn't
 * parse. */
function ValBar({
  findings,
  parseFailure,
  draft,
}: {
  findings: Finding[];
  parseFailure: string | null;
  draft: RoutineDef;
}) {
  const trackCount = draft.tracks.length;
  const stepCount = stepCountOf(draft);
  const rightMeta = `schema v${draft.schema_version} · ${stepCount} step${stepCount === 1 ? '' : 's'} · ${trackCount} track${trackCount === 1 ? '' : 's'}`;

  if (parseFailure) {
    return (
      <div className="valbar" data-testid="valbar">
        <span className="err">{parseFailure}</span>
        <span className="right">{rightMeta}</span>
      </div>
    );
  }

  const errors = findings.filter((f) => f.severity === 'error');
  const warnings = findings.filter((f) => f.severity === 'warning');
  const first = findings[0];

  return (
    <div className="valbar" data-testid="valbar">
      <span className={errors.length > 0 ? 'err' : 'ok'}>
        ✓ {errors.length} error{errors.length === 1 ? '' : 's'}
      </span>
      <span className="warn">
        ⚠ {warnings.length} warning{warnings.length === 1 ? '' : 's'}
      </span>
      {first && (
        <span className="msg">
          <span className="code">{first.code}</span> — {first.message}
        </span>
      )}
      <span className="right">{rightMeta}</span>
    </div>
  );
}

/** Export JSON's read-only dialog (binding constraint 5). Mirrors the
 * inline-overlay convention (ImportJsonDialog.tsx) without reusing its CSS
 * classes — this component owns its own copy (established convention: every
 * routines surface file owns its own `.surface`/dialog rules rather than
 * assuming another file's stylesheet is loaded alongside it). */
function ExportJsonDialog({ draft, onClose }: { draft: RoutineDef; onClose: () => void }) {
  const [copied, setCopied] = useState(false);
  const json = JSON.stringify(draft, null, 2);

  return (
    <div className="dlg-backdrop" role="presentation" data-testid="export-json-backdrop" onClick={onClose}>
      <div
        className="dlg"
        role="dialog"
        aria-label="Export routine JSON"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="dlg-head">
          <span>Export JSON</span>
          <button type="button" className="dismiss" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        <textarea
          className="dlg-textarea"
          data-testid="export-json-textarea"
          readOnly
          rows={18}
          value={json}
        />
        <div className="dlg-actions">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            Close
          </button>
          <button
            type="button"
            className="btn btn-accent"
            onClick={() => {
              void navigator.clipboard.writeText(json).then(() => {
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              });
            }}
          >
            {copied ? 'Copied ✓' : 'Copy'}
          </button>
        </div>
      </div>
    </div>
  );
}

export function RoutineDesigner({
  routine,
  tab,
  onBack,
  onTabChange,
  initialDraft,
  onDraftChange,
  onPopOut,
}: RoutineDesignerProps) {
  // Fixed at mount: whether this designer opened on a brand-new, unsaved
  // draft (empty `routine`) — the name field stays editable for the whole
  // session even after the operator types a name, since the routine isn't
  // considered "loaded from the backend" until a real Save happens.
  const [isNewDraft] = useState(() => routine === '');

  // A continuity-token draft (spec §7) seeds the designer at mount and SUPPRESSES
  // the `getRoutine` fetch below — captured at mount so a later prop-identity
  // change (there is none per current navigation) can't re-trigger a fetch.
  const [seededFromToken] = useState(() => initialDraft != null);
  const [draft, setDraft] = useState<RoutineDef | null>(() => initialDraft ?? null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [findings, setFindings] = useState<Finding[]>([]);
  const [parseFailure, setParseFailure] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [dryRunning, setDryRunning] = useState(false);
  const [highlightRunId, setHighlightRunId] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);

  // The action registry (Task 10's canvas needs it to derive category/
  // transmits per node — never from the action's name). Fetched once per
  // mount, independent of `routine`/`draft` — the registry doesn't change
  // per routine.
  const [actions, setActions] = useState<ActionInfo[]>([]);
  useEffect(() => {
    let cancelled = false;
    listActions()
      .then((list) => {
        if (!cancelled) setActions(list);
      })
      .catch(() => {
        // A registry fetch failure shouldn't block the designer from
        // rendering — the canvas degrades to every action rendering as
        // "unknown" (category 'local', transmits false), same as any
        // genuinely-unregistered action name.
        if (!cancelled) setActions([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Canvas selection + armed-insert-point state (task-10 brief binding
  // constraint 4): RoutineDesigner is the single owner so PaletteRail can
  // read/clear the same `armedInsert` value CanvasTab renders as the amber
  // edge.
  const [selectedStepId, setSelectedStepId] = useState<string | null>(null);
  const [armedInsert, setArmedInsert] = useState<ArmedInsertPosition | null>(null);

  /** CanvasTab's ＋ click handler: toggles the armed insert point (clicking
   *  the same one again disarms it; clicking a different one re-arms at the
   *  new position). PaletteRail's `onInsert` (below) performs the actual
   *  `insertStep` call once an action is chosen against this armed position —
   *  this component only tracks WHERE, never WHAT, is being inserted. */
  const handleInsertAt = useCallback((pos: ArmedInsertPosition) => {
    setArmedInsert((prev) =>
      prev &&
      prev.trackIdx === pos.trackIdx &&
      prev.afterStepId === pos.afterStepId &&
      sameArm(prev.arm, pos.arm)
        ? null
        : pos,
    );
  }, []);

  // Report the live draft upward for continuity-token collection (spec §7):
  // the host reads the LATEST reported draft at pop-out / dock-back time. Kept
  // in a ref so a changing `onDraftChange` identity doesn't re-fire the effect.
  const onDraftChangeRef = useRef(onDraftChange);
  onDraftChangeRef.current = onDraftChange;
  useEffect(() => {
    if (draft) onDraftChangeRef.current?.(draft);
  }, [draft]);

  // Load the def once, per `routine`. A token-seeded designer already has its
  // draft (set at mount) and must NOT fetch — that would clobber the operator's
  // in-progress edits with the last-saved def.
  useEffect(() => {
    if (seededFromToken) return;
    let cancelled = false;
    if (routine === '') {
      setDraft(createDraft());
      setLoadError(null);
      return;
    }
    getRoutine(routine)
      .then((def) => {
        if (!cancelled) {
          setDraft(def);
          setLoadError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) setLoadError(formatUiError(e));
      });
    return () => {
      cancelled = true;
    };
  }, [routine, seededFromToken]);

  // Always-on validation (flow 2): re-validate 400ms after every draft
  // change, including the initial load.
  useEffect(() => {
    if (!draft) return;
    const handle = setTimeout(() => {
      validateDraft(draft)
        .then((result) => {
          setFindings(result);
          setParseFailure(null);
        })
        .catch((e) => {
          setFindings([]);
          setParseFailure(formatUiError(e));
        });
    }, VALIDATE_DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [draft]);

  /** Apply an edit op and mark the draft dirty. Every edit path in this
   *  component (and, eventually, Tasks 10-12's tab bodies) funnels through
   *  this so "editing marks dirty" is a single code path, not one per op. */
  const updateDraft = useCallback((updater: (d: RoutineDef) => RoutineDef) => {
    setDraft((prev) => (prev ? updater(prev) : prev));
    setDirty(true);
  }, []);

  /** CanvasTab's ⌫/Delete/Backspace handler. `defDraft.removeStep` itself
   *  scrubs the removed id from every branch's then/else arm list (so a
   *  recycled `nextStepId` can never phantom-attach a later step); here we
   *  additionally clear the UI state anchored on it — the selection, and an
   *  armed insert point whose `afterStepId` or arm's branch is the removed
   *  step — so the canvas never points at a step id that's no longer in the
   *  draft. */
  const handleRemoveStep = useCallback(
    (stepId: string) => {
      updateDraft((d) => removeStep(d, stepId));
      setSelectedStepId((prev) => (prev === stepId ? null : prev));
      setArmedInsert((prev) =>
        prev && (prev.afterStepId === stepId || prev.arm?.branchId === stepId) ? null : prev,
      );
    },
    [updateDraft],
  );

  /** PaletteRail's click-with-armed-insert handler: PaletteRail builds the
   *  `Step` value (its own action/control shape, `nextStepId`-assigned id);
   *  this is where it's actually spliced into the draft at the armed
   *  position. A position carrying an `arm` marker (any branch-arm ＋ —
   *  empty-arm, mid-arm, or trailing) routes through
   *  `defDraft.insertStepIntoBranchArm`, with the armed `afterStepId`
   *  positioning the new id WITHIN the then/else list (append when it's the
   *  branch's own id, i.e. an empty arm) — so the step lands IN the arm at
   *  the clicked position; every other position uses the plain
   *  `defDraft.insertStep` splice. Disarms afterward (one insert per arm —
   *  re-arming for a second insert at the same spot is a deliberate extra
   *  click, not implicit) and selects the new step so its fields are
   *  immediately editable in `StepInspector`. A stale call with no armed
   *  position (shouldn't happen — PaletteRail disables its items while
   *  unarmed) is a no-op rather than a crash. */
  const handleInsert = useCallback(
    (step: Step) => {
      // tuxlink-7ewvq item 2: no chosen position is a first-class path now —
      // the step appends to the END of the current track (the one holding
      // the selection, else the first), so the palette is directly usable
      // without arming a ＋ first.
      const pos = armedInsert;
      if (!pos) {
        setDraft((d) => {
          if (!d) return d;
          const selTrackIdx = selectedStepId
            ? d.tracks.findIndex((t) => t.steps.some((s) => s.id === selectedStepId))
            : -1;
          const trackIdx = selTrackIdx >= 0 ? selTrackIdx : 0;
          const steps = d.tracks[trackIdx]?.steps ?? [];
          const afterStepId = steps.length > 0 ? steps[steps.length - 1]!.id : null;
          return insertStep(d, trackIdx, afterStepId, step);
        });
        setDirty(true);
        setSelectedStepId(step.id);
        return;
      }
      updateDraft((d) =>
        pos.arm
          ? insertStepIntoBranchArm(d, pos.trackIdx, pos.arm.branchId, pos.arm.which, step, pos.afterStepId)
          : insertStep(d, pos.trackIdx, pos.afterStepId, step),
      );
      setArmedInsert(null);
      setSelectedStepId(step.id);
    },
    [armedInsert, selectedStepId, updateDraft],
  );

  /** StepInspector's field-edit handler: patches whichever step is currently
   *  selected. A `null` `selectedStepId` (StepInspector is only ever mounted
   *  when one is selected, but this guards a stray call after a selection
   *  clears mid-flight) is a no-op. */
  const handleStepChange = useCallback(
    (patch: StepPatch) => {
      const stepId = selectedStepId;
      if (!stepId) return;
      updateDraft((d) => updateStep(d, stepId, patch));
    },
    [selectedStepId, updateDraft],
  );

  const handleSave = useCallback(async () => {
    if (!draft) return null;
    setSaving(true);
    try {
      const result = await saveRoutine(draft);
      // Save NEVER blocks (Global Constraint 7): dirty clears and the
      // findings replace the valbar's content regardless of `blocked`.
      setFindings(result.findings);
      setParseFailure(null);
      setDirty(false);
      return result;
    } catch (e) {
      // A thrown value here is a genuine backend/parse error — saveRoutine
      // itself never rejects on validation findings.
      setParseFailure(formatUiError(e));
      return null;
    } finally {
      setSaving(false);
    }
  }, [draft]);

  const handleDryRun = useCallback(async () => {
    if (!draft) return;
    setDryRunning(true);
    try {
      const saveResult = await handleSave();
      if (!saveResult) return; // save itself failed (parse/backend error) — don't dry-run a def that didn't save
      const started = await dryRunRoutine(draft.routine, {});
      setHighlightRunId(started.runId);
      onTabChange('runs');
    } catch (e) {
      setParseFailure(formatUiError(e));
    } finally {
      setDryRunning(false);
    }
  }, [draft, handleSave, onTabChange]);

  if (loadError) {
    return (
      <div className="surface" data-testid="routine-designer">
        <div className="designer-head">
          <button type="button" className="back" onClick={onBack}>
            ← Routines
          </button>
        </div>
        <div className="load-error">Failed to load routine: {loadError}</div>
      </div>
    );
  }

  if (!draft) {
    return (
      <div className="surface" data-testid="routine-designer">
        Loading…
      </div>
    );
  }

  // The selected node's own Step value, for StepInspector — looked up fresh
  // from `draft` every render (never cached) so an edit that changes the
  // step's own shape is reflected immediately, and a selection that no
  // longer resolves (a removed step id — handleRemoveStep already clears
  // `selectedStepId` in the common path, but this is the defensive fallback)
  // renders no inspector rather than a stale one.
  const selectedStep = selectedStepId
    ? (draft.tracks.flatMap((t) => t.steps).find((s) => s.id === selectedStepId) ?? null)
    : null;

  // 'settings' survives in the DesignerTab type for continuity-token compat
  // (a token stored before the tab was removed) but renders the Design view —
  // the settings sections live inline there now.
  const effectiveTab: DesignerTab = tab === 'settings' ? 'design' : tab;

  return (
    <div className="surface" data-testid="routine-designer">
      <div className="designer-head">
        <button type="button" className="back" onClick={onBack}>
          ← Routines
        </button>
        {isNewDraft ? (
          <input
            className="dname-input"
            data-testid="designer-name-input"
            placeholder="Untitled routine"
            value={draft.routine}
            onChange={(e) => {
              const name = e.target.value;
              updateDraft((d) => ({ ...d, routine: name }));
            }}
          />
        ) : (
          <span className="dname">{draft.routine}</span>
        )}
        <span
          className="dstate"
          data-testid="transmit-mode-chip"
          title={transmitModeTooltip(draft.transmit_mode)}
        >
          TX: {draft.transmit_mode}
        </span>
        {dirty && <span className="unsaved" data-testid="unsaved-dot" title="unsaved changes" />}
        <span className="tabs">
          {VISIBLE_TABS.map(({ tab: t, label }) => (
            <button
              key={t}
              type="button"
              className={`tab${effectiveTab === t ? ' active' : ''}`}
              onClick={() => onTabChange(t)}
            >
              {label}
            </button>
          ))}
        </span>
        <span className="dactions">
          {onPopOut && (
            <button
              type="button"
              className="btn"
              data-testid="routines-designer-popout"
              title="Open Routines in its own window"
              onClick={onPopOut}
            >
              ↗ Pop out
            </button>
          )}
          <button type="button" className="btn" disabled={dryRunning} onClick={() => void handleDryRun()}>
            Dry-run
          </button>
          <button type="button" className="btn" onClick={() => setExportOpen(true)}>
            Export JSON
          </button>
          <button
            type="button"
            className="btn btn-accent"
            disabled={saving}
            onClick={() => void handleSave()}
          >
            Save
          </button>
        </span>
      </div>

      <div className="design-body">
        {effectiveTab === 'design' && (
          <>
            <div className="design-main" data-testid="design-main">
              <CanvasTab
                draft={draft}
                actions={actions}
                selectedStepId={selectedStepId}
                onSelect={setSelectedStepId}
                armedInsert={armedInsert}
                onInsertAt={handleInsertAt}
                onRemoveStep={handleRemoveStep}
                onAddTrack={() => updateDraft((d) => addTrack(d, `track-${d.tracks.length + 1}`))}
              />
              {/* tuxlink-7ewvq item 8: settings live HERE, in the canvas
                  column's otherwise-empty space, not behind a third tab. */}
              <div className="inline-settings" data-testid="inline-settings">
                <div className="inline-settings-head">ROUTINE SETTINGS</div>
                <SettingsTab
                  key={draft.routine}
                  draft={draft}
                  findings={findings}
                  onChange={(patch) => updateDraft((d) => updateSettings(d, patch))}
                  onSaved={handleSave}
                />
              </div>
            </div>
            <div className="design-rail" data-testid="design-rail">
              <PaletteRail def={draft} actions={actions} armedInsert={armedInsert} onInsert={handleInsert} />
              {selectedStep && (
                <StepInspector
                  key={selectedStep.id}
                  step={selectedStep}
                  actions={actions}
                  onChange={handleStepChange}
                  onRemove={() => handleRemoveStep(selectedStep.id)}
                />
              )}
            </div>
          </>
        )}
        {effectiveTab === 'runs' && <RunsTab routine={draft.routine} highlightRunId={highlightRunId} />}
      </div>

      <ValBar findings={findings} parseFailure={parseFailure} draft={draft} />

      {exportOpen && <ExportJsonDialog draft={draft} onClose={() => setExportOpen(false)} />}
    </div>
  );
}
