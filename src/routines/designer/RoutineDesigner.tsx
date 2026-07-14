/**
 * RoutineDesigner — the routine designer shell (routines plan-5 Task 9,
 * `.superpowers/sdd/task-9-brief.md`, spec §12 flows 2/5).
 *
 * Replaces RoutinesSurface's Task 7 one-line placeholder. Layout is the
 * approved mock verbatim (dev/scratch/routines-ui-mocks/designer-canvas.html):
 * header (← Routines, name, state pill, unsaved dot, Design/Runs/Settings
 * tabs, Dry-run/Export JSON/Save actions) and the always-on `.valbar`
 * validation strip. The canvas (Task 10), settings form (Task 11), and runs
 * list (Task 13) mount points below are minimal inert placeholders — Tasks
 * 10-13 replace each one outright with the real tab body, using this file's
 * `draft` state and defDraft.ts's edit ops.
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
import { useCallback, useEffect, useState } from 'react';
import {
  getRoutine,
  saveRoutine,
  validateDraft,
  dryRunRoutine,
  type RoutineDef,
  type Finding,
} from '../routinesApi';
import { formatUiError } from '../format';
import { createDraft, addTrack } from './defDraft';
import type { DesignerTab } from '../RoutinesSurface';
import './RoutineDesigner.css';

export interface RoutineDesignerProps {
  /** Empty string means a fresh, unsaved draft (RoutinesSurface's "New
   *  Routine…" path) — the def is never fetched from the backend for it. */
  routine: string;
  tab: DesignerTab;
  onBack: () => void;
  onTabChange: (tab: DesignerTab) => void;
}

/** Debounce window for the always-on validation bar (spec §12 flow 2). */
const VALIDATE_DEBOUNCE_MS = 400;

const TAB_LABELS: Record<DesignerTab, string> = {
  design: 'Design',
  runs: 'Runs',
  settings: 'Settings',
};

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

/** Design tab mount point (Task 10 replaces this with the real canvas). The
 * one live affordance here — "＋ Add track" — is a real defDraft.ts op
 * (`addTrack`), not a fake control: it's how this task's own tests exercise
 * "an edit marks the draft dirty" without anticipating Task 10's canvas UI. */
function DesignTabPlaceholder({ onAddTrack }: { onAddTrack: () => void }) {
  return (
    <div className="tab-body-placeholder" data-testid="design-tab-placeholder">
      <p>Canvas (Task 10) mounts here.</p>
      <button type="button" className="btn" data-testid="test-add-track" onClick={onAddTrack}>
        ＋ Add track
      </button>
    </div>
  );
}

/** Runs tab mount point (Task 13). `highlightRunId` is threaded through from
 * this component's dry-run flow so Task 13's real runs list can scroll to /
 * highlight the run a dry-run just started. */
function RunsTabPlaceholder({ highlightRunId }: { highlightRunId: string | null }) {
  return (
    <div className="tab-body-placeholder" data-testid="runs-tab-placeholder">
      Runs (Task 13) mounts here.
      {highlightRunId && <span data-testid="highlight-run-id"> highlight: {highlightRunId}</span>}
    </div>
  );
}

/** Settings tab mount point (Task 11). */
function SettingsTabPlaceholder() {
  return (
    <div className="tab-body-placeholder" data-testid="settings-tab-placeholder">
      Settings (Task 11) mounts here.
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

export function RoutineDesigner({ routine, tab, onBack, onTabChange }: RoutineDesignerProps) {
  // Fixed at mount: whether this designer opened on a brand-new, unsaved
  // draft (empty `routine`) — the name field stays editable for the whole
  // session even after the operator types a name, since the routine isn't
  // considered "loaded from the backend" until a real Save happens.
  const [isNewDraft] = useState(() => routine === '');

  const [draft, setDraft] = useState<RoutineDef | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [findings, setFindings] = useState<Finding[]>([]);
  const [parseFailure, setParseFailure] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [dryRunning, setDryRunning] = useState(false);
  const [highlightRunId, setHighlightRunId] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);

  // Load the def once, per `routine`.
  useEffect(() => {
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
  }, [routine]);

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
        <span className="dstate">{draft.transmit_mode}</span>
        {dirty && <span className="unsaved" data-testid="unsaved-dot" title="unsaved changes" />}
        <span className="tabs">
          {(Object.keys(TAB_LABELS) as DesignerTab[]).map((t) => (
            <button
              key={t}
              type="button"
              className={`tab${tab === t ? ' active' : ''}`}
              onClick={() => onTabChange(t)}
            >
              {TAB_LABELS[t]}
            </button>
          ))}
        </span>
        <span className="dactions">
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
        {tab === 'design' && (
          <DesignTabPlaceholder
            onAddTrack={() => updateDraft((d) => addTrack(d, `track-${d.tracks.length + 1}`))}
          />
        )}
        {tab === 'runs' && <RunsTabPlaceholder highlightRunId={highlightRunId} />}
        {tab === 'settings' && <SettingsTabPlaceholder />}
      </div>

      <ValBar findings={findings} parseFailure={parseFailure} draft={draft} />

      {exportOpen && <ExportJsonDialog draft={draft} onClose={() => setExportOpen(false)} />}
    </div>
  );
}
