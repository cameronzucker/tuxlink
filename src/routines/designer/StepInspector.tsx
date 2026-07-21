/**
 * StepInspector — the Design tab's per-step field editor (routines plan-5
 * Task 11, `.superpowers/sdd/task-11-brief.md`, spec §5/§12 flow 2).
 *
 * Renders below `PaletteRail` in the same right rail, only when
 * `RoutineDesigner` has a selected step (it does NOT render a "nothing
 * selected" placeholder itself — the caller renders it conditionally, same
 * as CanvasTab's `selectedStepId` gating). `RoutineDesigner` mounts one keyed
 * by `step.id` (`<StepInspector key={step.id} .../>`) so every local editing
 * state (the params textarea's raw text, its parse-error) resets cleanly on
 * selection change via a plain remount, instead of a `useEffect`
 * re-sync-on-id-change dance.
 *
 * `onChange(patch)` is a `defDraft.StepPatch` — the caller applies it via
 * `updateStep(draft, step.id, patch)`; this component never calls
 * `defDraft` itself, mirroring `PaletteRail`'s "build the value, let the
 * owner apply the op" split.
 *
 * Params editor (action steps only): a JSON textarea seeded with
 * `JSON.stringify(step.params, null, 2)`, applied on blur only when it
 * parses — an unparseable edit shows the error inline and leaves the field
 * editable without calling `onChange` (task brief Step 1 test).
 *
 * @-reference helper: params correctness stays with the validator's own
 * UNRESOLVED_REF finding (spec's valbar) — this row is assistance only. It
 * appears whenever the raw textarea text contains a quoted string starting
 * with `@` (checked against the raw text, not the parsed value, so it shows
 * up mid-edit even before the JSON is valid again) and offers every known
 * preset/station-set as a one-click completion inserted at the textarea's
 * cursor position.
 */
import { useEffect, useRef, useState } from 'react';
import {
  listPresets,
  listRoutines,
  listStationSets,
  type ActionInfo,
  type BusyPolicy,
  type RadioPreset,
  type RoutineSummary,
  type StationSet,
  type Step,
} from '../routinesApi';
import type { StepPatch } from './defDraft';
import { RadioConnectSection } from './RadioConnectSection';
import './StepInspector.css';

export interface StepInspectorProps {
  step: Step;
  actions: ActionInfo[];
  onChange: (patch: StepPatch) => void;
  onRemove: () => void;
}

/** A quoted JSON string value starting with `@`, e.g. `"@station-set:foo"` —
 *  matched against the raw textarea text (not a parsed value) so the helper
 *  row can appear mid-edit. */
const AT_REF_RE = /"@[^"]*"/;

function splitIds(text: string): string[] {
  return text
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** tuxlink-7ewvq item 9: display text for a param value — strings raw,
 *  anything else as compact JSON. */
function valueToText(v: unknown): string {
  return typeof v === 'string' ? v : JSON.stringify(v);
}

/** Inverse of `valueToText` at commit time: text that parses as JSON becomes
 *  the parsed value (numbers, booleans, arrays, nested objects); anything
 *  else — callsigns, @refs, plain words — stays a string. */
function textToValue(text: string): unknown {
  const t = text.trim();
  if (t === '') return '';
  try {
    return JSON.parse(t) as unknown;
  } catch {
    return text;
  }
}

export function StepInspector({ step, actions, onChange, onRemove }: StepInspectorProps) {
  const isAction = 'action' in step;
  const info = isAction ? actions.find((a) => a.name === step.action) : undefined;

  // ---- params editor (action steps only) ----
  // Default surface is the key/value grid (tuxlink-7ewvq item 9) — no
  // operator hand-types JSON to configure an action. The raw JSON textarea
  // survives behind the "edit as JSON" toggle for nested shapes.
  const [paramsMode, setParamsMode] = useState<'kv' | 'json'>('kv');
  const [paramsText, setParamsText] = useState(() =>
    isAction ? JSON.stringify(step.params ?? {}, null, 2) : '',
  );
  const [paramsError, setParamsError] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  function commitParams() {
    try {
      const parsed: unknown = JSON.parse(paramsText);
      setParamsError(null);
      onChange({ params: parsed });
    } catch (e) {
      setParamsError(e instanceof Error ? e.message : String(e));
    }
  }

  // ---- key/value grid state ----
  // Committed rows derive from `step.params` every render; only the field
  // being edited holds local text (`fieldEdits`), so a parent update never
  // fights a stale local copy. Rows added via ＋ live in `newRows` until the
  // parent's params actually contain their key.
  const committedParams: Record<string, unknown> =
    isAction && step.params && typeof step.params === 'object' && !Array.isArray(step.params)
      ? (step.params as Record<string, unknown>)
      : {};
  const [fieldEdits, setFieldEdits] = useState<Record<string, string>>({});
  const [newRows, setNewRows] = useState<Array<{ uid: number; key: string; value: string }>>([]);
  const newRowUid = useRef(0);
  const lastFocusedRef = useRef<string | null>(null);

  const visibleNewRows = newRows.filter((r) => !(r.key in committedParams));

  function displayValue(key: string): string {
    return fieldEdits[`v:${key}`] ?? valueToText(committedParams[key]);
  }

  /** Rebuild the full params object from committed keys (with any in-flight
   *  field edit applied) plus the keyed new rows, and commit it. */
  function commitGrid(overrides?: { renamedFrom?: string; renamedTo?: string; removeKey?: string }) {
    const next: Record<string, unknown> = {};
    for (const key of Object.keys(committedParams)) {
      if (overrides?.removeKey === key) continue;
      const outKey = overrides?.renamedFrom === key ? (overrides.renamedTo ?? key) : key;
      if (outKey === '') continue;
      next[outKey] = textToValue(displayValue(key));
    }
    for (const row of visibleNewRows) {
      if (row.key.trim() === '') continue;
      next[row.key.trim()] = textToValue(row.value);
    }
    onChange({ params: next });
  }

  // ---- @-reference helper data ----
  const [presets, setPresets] = useState<RadioPreset[]>([]);
  const [stationSets, setStationSets] = useState<StationSet[]>([]);
  useEffect(() => {
    if (!isAction) return;
    let cancelled = false;
    listPresets()
      .then((l) => {
        if (!cancelled) setPresets(Array.isArray(l) ? l : []);
      })
      .catch(() => {
        if (!cancelled) setPresets([]);
      });
    listStationSets()
      .then((l) => {
        if (!cancelled) setStationSets(Array.isArray(l) ? l : []);
      })
      .catch(() => {
        if (!cancelled) setStationSets([]);
      });
    return () => {
      cancelled = true;
    };
    // Deliberately `[]` — fetched once per mount (this component is remounted
    // wholesale on step selection change, per the header comment), not
    // re-fetched on every keystroke in the params textarea.
  }, []);

  /** KV mode: the row a completion chip should fill — the last-focused value
   *  field if its text starts with '@', else the first '@'-prefixed row. */
  function atRefTargetKey(): string | null {
    const focused = lastFocusedRef.current;
    if (focused && displayValue(focused).startsWith('@')) return focused;
    for (const key of Object.keys(committedParams)) {
      if (displayValue(key).startsWith('@')) return key;
    }
    return null;
  }

  // tuxlink-fg0em: radio.connect's kv surface is the RadioConnectSection —
  // there is no grid field to insert a completion into, so the helper only
  // offers refs in JSON mode there.
  const showRefHelper =
    isAction &&
    (paramsMode === 'json'
      ? AT_REF_RE.test(paramsText)
      : step.action !== 'radio.connect' && atRefTargetKey() !== null);

  function insertRef(ref: string) {
    if (paramsMode === 'kv') {
      const target = atRefTargetKey();
      if (target) setFieldEdits((e) => ({ ...e, [`v:${target}`]: ref }));
      return;
    }
    const el = textareaRef.current;
    if (!el) {
      setParamsText((t) => t + ref);
      return;
    }
    const start = el.selectionStart ?? paramsText.length;
    const end = el.selectionEnd ?? paramsText.length;
    setParamsText(paramsText.slice(0, start) + ref + paramsText.slice(end));
  }

  // ---- routine dropdown (call steps only) ----
  const [routines, setRoutines] = useState<RoutineSummary[]>([]);
  useEffect(() => {
    if (isAction || step.control !== 'call') return;
    let cancelled = false;
    listRoutines()
      .then((l) => {
        if (!cancelled) setRoutines(Array.isArray(l) ? l : []);
      })
      .catch(() => {
        if (!cancelled) setRoutines([]);
      });
    return () => {
      cancelled = true;
    };
    // Deliberately `[]` — `isAction`/`step.control` are stable for this
    // component's whole lifetime (it remounts on step-selection change, per
    // the header comment), so there's nothing to re-run this effect on.
  }, []);

  return (
    <div className="inspector" data-testid="step-inspector">
      <div className="insp-head">
        <span className="insp-id mono" data-testid="inspector-step-id">
          {step.id}
        </span>
        <button
          type="button"
          className="insp-remove"
          data-testid="inspector-remove"
          aria-label={`Remove ${step.id}`}
          onClick={onRemove}
        >
          Delete
        </button>
      </div>

      {isAction && (
        <>
          <div className="insp-row">
            <span className="insp-label">action</span>
            {/* tuxlink-5lfxk: human label first; the raw id stays visible as
                mono secondary (it is what params/journals reference). */}
            {info?.label && <span className="insp-value">{info.label}</span>}
            <span className="insp-value mono">{step.action}</span>
            <span className="flags">
              {info?.needsRadio && <span className="flag rig">RIG</span>}
              {info?.transmits && <span className="flag tx">TX</span>}
              {info?.needsInternet && <span className="flag net">NET</span>}
              {/* D5/E3: the config-write consent class, keyed on writesConfig. */}
              {info?.writesConfig && <span className="flag writes">WRITES</span>}
            </span>
          </div>

          {/* E3: the action's one-line human description (tuxlink-5lfxk),
              surfaced beneath the action row. Hidden when the registry copy
              is empty. */}
          {info?.description && (
            <div className="insp-desc" data-testid="inspector-description">
              {info.description}
            </div>
          )}

          <div className="insp-field">
            <span className="insp-label">
              parameters
              <button
                type="button"
                className="insp-json-toggle"
                data-testid="params-json-toggle"
                onClick={() => {
                  setParamsMode((m) => (m === 'kv' ? 'json' : 'kv'));
                  // Entering JSON mode always shows the CURRENT params; leaving
                  // it drops any uncommitted text (blur already committed the
                  // valid edits).
                  setParamsText(JSON.stringify(step.params ?? {}, null, 2));
                  setParamsError(null);
                  setFieldEdits({});
                }}
              >
                {paramsMode === 'kv' ? 'edit as JSON' : 'edit as fields'}
              </button>
            </span>
            {paramsMode === 'kv' && step.action === 'radio.connect' ? (
              /* tuxlink-fg0em: radio.connect gets its dedicated section over
                 the REAL selection surfaces instead of the generic grid; the
                 JSON toggle above stays the escape hatch for any shape the
                 section cannot express. */
              <RadioConnectSection
                params={committedParams}
                onChange={(p) => onChange({ params: p })}
              />
            ) : paramsMode === 'kv' ? (
              <div className="param-grid" data-testid="param-grid">
                {Object.keys(committedParams).map((key) => (
                  <div className="param-row" data-testid={`param-row-${key}`} key={key}>
                    <input
                      className="param-key mono"
                      data-testid={`param-key-${key}`}
                      aria-label={`Parameter name ${key}`}
                      value={fieldEdits[`k:${key}`] ?? key}
                      onChange={(e) => setFieldEdits((ed) => ({ ...ed, [`k:${key}`]: e.target.value }))}
                      onBlur={(e) => {
                        const renamed = e.target.value.trim();
                        setFieldEdits(({ [`k:${key}`]: _drop, ...rest }) => rest);
                        if (renamed !== key) commitGrid({ renamedFrom: key, renamedTo: renamed });
                      }}
                    />
                    <input
                      className="param-value mono"
                      data-testid={`param-value-${key}`}
                      aria-label={`Value for ${key}`}
                      value={displayValue(key)}
                      onFocus={() => {
                        lastFocusedRef.current = key;
                      }}
                      onChange={(e) => setFieldEdits((ed) => ({ ...ed, [`v:${key}`]: e.target.value }))}
                      onBlur={() => {
                        commitGrid();
                        setFieldEdits(({ [`v:${key}`]: _drop, ...rest }) => rest);
                      }}
                    />
                    <button
                      type="button"
                      className="param-remove"
                      data-testid={`param-remove-${key}`}
                      aria-label={`Remove parameter ${key}`}
                      onClick={() => commitGrid({ removeKey: key })}
                    >
                      ×
                    </button>
                  </div>
                ))}
                {visibleNewRows.map((row, i) => (
                  <div className="param-row" key={row.uid}>
                    <input
                      className="param-key mono"
                      data-testid={row.key.trim() === '' ? `param-key-new-${i}` : `param-key-${row.key.trim()}`}
                      aria-label="New parameter name"
                      placeholder="name"
                      value={row.key}
                      onChange={(e) =>
                        setNewRows((rs) => rs.map((r) => (r.uid === row.uid ? { ...r, key: e.target.value } : r)))
                      }
                      onBlur={() => {
                        if (row.key.trim() !== '') commitGrid();
                      }}
                    />
                    <input
                      className="param-value mono"
                      data-testid={row.key.trim() === '' ? `param-value-new-${i}` : `param-value-${row.key.trim()}`}
                      aria-label={`Value for ${row.key.trim() || 'new parameter'}`}
                      placeholder="value"
                      value={row.value}
                      onChange={(e) =>
                        setNewRows((rs) => rs.map((r) => (r.uid === row.uid ? { ...r, value: e.target.value } : r)))
                      }
                      onBlur={() => {
                        if (row.key.trim() !== '') commitGrid();
                      }}
                    />
                    <button
                      type="button"
                      className="param-remove"
                      aria-label="Remove new parameter"
                      onClick={() => setNewRows((rs) => rs.filter((r) => r.uid !== row.uid))}
                    >
                      ×
                    </button>
                  </div>
                ))}
                <button
                  type="button"
                  className="param-add"
                  data-testid="param-add"
                  onClick={() =>
                    setNewRows((rs) => [...rs, { uid: newRowUid.current++, key: '', value: '' }])
                  }
                >
                  ＋ add parameter
                </button>
              </div>
            ) : (
              <textarea
                ref={textareaRef}
                data-testid="inspector-params"
                className="insp-textarea mono"
                rows={8}
                value={paramsText}
                onChange={(e) => setParamsText(e.target.value)}
                onBlur={commitParams}
              />
            )}
          </div>
          {paramsError && (
            <div className="insp-error" data-testid="inspector-params-error">
              {paramsError}
            </div>
          )}

          {showRefHelper && (presets.length > 0 || stationSets.length > 0) && (
            <div className="insp-ref-helper" data-testid="inspector-ref-helper">
              {presets.map((p) => (
                <button
                  key={`preset:${p.name}`}
                  type="button"
                  className="ref-chip"
                  data-testid={`ref-chip-preset-${p.name}`}
                  onClick={() => insertRef(`@preset:${p.name}`)}
                >
                  @preset:{p.name}
                </button>
              ))}
              {stationSets.map((s) => (
                <button
                  key={`station-set:${s.name}`}
                  type="button"
                  className="ref-chip"
                  data-testid={`ref-chip-station-set-${s.name}`}
                  onClick={() => insertRef(`@station-set:${s.name}`)}
                >
                  @station-set:{s.name}
                </button>
              ))}
            </div>
          )}

          <label className="insp-field">
            <span className="insp-label">timeout_s</span>
            <input
              type="number"
              data-testid="inspector-timeout"
              value={step.timeout_s ?? ''}
              onChange={(e) =>
                onChange({ timeout_s: e.target.value === '' ? undefined : Number(e.target.value) })
              }
            />
          </label>

          <label className="insp-field">
            <span className="insp-label">on_radio_busy</span>
            <select
              data-testid="inspector-on-radio-busy"
              value={step.on_radio_busy ?? 'wait'}
              onChange={(e) => onChange({ on_radio_busy: e.target.value as BusyPolicy })}
            >
              <option value="wait">wait</option>
              <option value="fail">fail</option>
            </select>
          </label>
        </>
      )}

      {!isAction && step.control === 'branch' && (
        <>
          <label className="insp-field">
            <span className="insp-label">on</span>
            <input
              data-testid="inspector-branch-on"
              value={step.on}
              onChange={(e) => onChange({ on: e.target.value })}
            />
          </label>
          <label className="insp-field">
            <span className="insp-label">then (comma-separated step ids)</span>
            <input
              data-testid="inspector-branch-then"
              value={step.then.join(', ')}
              onChange={(e) => onChange({ then: splitIds(e.target.value) })}
            />
          </label>
          <label className="insp-field">
            <span className="insp-label">else (comma-separated step ids)</span>
            <input
              data-testid="inspector-branch-else"
              value={step.else.join(', ')}
              onChange={(e) => onChange({ else: splitIds(e.target.value) })}
            />
          </label>
        </>
      )}

      {!isAction && step.control === 'delay' && (
        <label className="insp-field">
          <span className="insp-label">delay (e.g. 5m)</span>
          <input
            data-testid="inspector-delay"
            value={step.delay}
            onChange={(e) => onChange({ delay: e.target.value })}
          />
        </label>
      )}

      {!isAction && step.control === 'retry' && (
        <>
          <label className="insp-field">
            <span className="insp-label">step (id being retried)</span>
            <input
              data-testid="inspector-retry-step"
              value={step.step}
              onChange={(e) => onChange({ step: e.target.value })}
            />
          </label>
          <label className="insp-field">
            <span className="insp-label">attempts</span>
            <input
              type="number"
              data-testid="inspector-retry-attempts"
              value={step.attempts}
              onChange={(e) => onChange({ attempts: Number(e.target.value) })}
            />
          </label>
          <label className="insp-field">
            <span className="insp-label">backoff_s</span>
            <input
              type="number"
              data-testid="inspector-retry-backoff"
              value={step.backoff_s ?? ''}
              onChange={(e) =>
                onChange({ backoff_s: e.target.value === '' ? undefined : Number(e.target.value) })
              }
            />
          </label>
        </>
      )}

      {!isAction && step.control === 'call' && (
        <label className="insp-field">
          <span className="insp-label">routine</span>
          <select
            data-testid="inspector-call-routine"
            value={step.routine}
            onChange={(e) => onChange({ routine: e.target.value })}
          >
            <option value="">— select —</option>
            {routines.map((r) => (
              <option key={r.routine} value={r.routine}>
                {r.routine}
              </option>
            ))}
          </select>
        </label>
      )}

      {!isAction && step.control === 'end' && (
        <label className="insp-field insp-checkbox">
          <input
            type="checkbox"
            data-testid="inspector-end-failed"
            checked={step.failed === true}
            onChange={(e) => onChange({ failed: e.target.checked })}
          />
          <span className="insp-label">failed</span>
        </label>
      )}
    </div>
  );
}
