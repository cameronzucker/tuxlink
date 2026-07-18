/**
 * ImportJsonDialog — the dashboard's "Import routine…" header action (routines
 * plan-5 Task 8; relabeled from "Import JSON" per bd tuxlink-iizmk item 10 —
 * the operator imports a routine, JSON is the carrier format).
 *
 * Textarea paste → `JSON.parse` → `saveRoutine(parsed)` → findings shown.
 * Save NEVER blocks (spec §10 / task-8 brief binding constraint 7): a draft
 * with error-severity findings still imports — `saveRoutine` always returns
 * a `SaveResult` (never rejects on validation), so the only rejection path
 * here is a parse failure (bad JSON) or a genuine backend error (disk I/O,
 * name conflict), both shown inline rather than closing the dialog out from
 * under the operator.
 *
 * Styling mirrors the project's inline-overlay convention
 * (NewFolderDialog.tsx / SettingsPanel.tsx): a `position: fixed` backdrop,
 * Esc closes, click-outside closes. Rules live in RoutinesDashboard.css
 * (`.import-*`) — the brief's Files list creates no separate stylesheet for
 * this component.
 */
import { useEffect, useState } from 'react';
import { saveRoutine, type RoutineDef, type SaveResult } from './routinesApi';
import { formatUiError } from './format';

export interface ImportJsonDialogProps {
  onClose: () => void;
  onSaved: (result: SaveResult) => void;
}

export function ImportJsonDialog({ onClose, onSaved }: ImportJsonDialogProps) {
  const [text, setText] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [findings, setFindings] = useState<SaveResult['findings'] | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  async function handleImport() {
    setError(null);
    let parsed: RoutineDef;
    try {
      parsed = JSON.parse(text) as RoutineDef;
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Invalid JSON.');
      return;
    }
    setSaving(true);
    try {
      const result = await saveRoutine(parsed);
      setFindings(result.findings);
      onSaved(result);
    } catch (e) {
      setError(formatUiError(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div
      className="import-backdrop"
      role="presentation"
      data-testid="import-json-backdrop"
      onClick={onClose}
    >
      <div
        className="import-dialog"
        role="dialog"
        aria-label="Import routine"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="import-head">
          {/* item 10 (bd tuxlink-iizmk): "Import routine", not "Import JSON" —
              the operator imports a routine; JSON is the carrier format. */}
          <span>Import routine</span>
          <button type="button" className="dismiss" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        <textarea
          className="import-textarea"
          data-testid="import-json-textarea"
          rows={14}
          placeholder="Paste a routine definition JSON…"
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setError(null);
            setFindings(null);
          }}
        />
        {error && <div className="import-error">{error}</div>}
        {findings && (
          <div className="import-findings">
            {findings.length === 0 ? (
              <div className="import-ok">Saved — no validation findings.</div>
            ) : (
              findings.map((f, i) => (
                <div key={`${f.code}-${i}`} className={`import-finding ${f.severity}`}>
                  <span className="code">{f.code}</span> — {f.message}
                </div>
              ))
            )}
          </div>
        )}
        <div className="import-actions">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            Close
          </button>
          <button
            type="button"
            className="btn btn-accent"
            disabled={saving || text.trim().length === 0}
            onClick={() => void handleImport()}
          >
            {saving ? 'Importing…' : 'Import'}
          </button>
        </div>
      </div>
    </div>
  );
}
