// Inline import sheet for the Catalog Browser (Forms-push G5+G6, tuxlink-z0le).
// Source pick → validate-before-write preview report → confirm overwrites →
// commit. No pop-up windows; mounted inline in CatalogBrowser.

import { useCallback, useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import {
  importPreview,
  importCommit,
  importCancel,
  type ImportPlan,
  type ImportEntry,
  type ImportResult,
} from './importApi';
import './ImportSheet.css';

type Step = 'choose' | 'previewing' | 'report' | 'committing' | 'result' | 'error';

export interface ImportSheetProps {
  /** Fired after a successful commit with the realized result. */
  onDone: (result: ImportResult) => void;
  /** Fired when the operator backs out without committing. */
  onCancel: () => void;
}

/** Short human label per classification. */
const KIND_LABEL: Record<ImportEntry['kind'], string> = {
  added: 'New',
  update: 'Replaces your form',
  overridesStandard: 'Replaces a standard form',
  companion: 'Supporting file',
  skip: 'Skipped',
  reject: 'Rejected',
};

const NO_VIEWER_NOTE =
  'Sends fine. Receiving stations see raw data, not a formatted view. Import your group’s viewer file alongside to fix.';

export function ImportSheet({ onDone, onCancel }: ImportSheetProps) {
  const [step, setStep] = useState<Step>('choose');
  const [plan, setPlan] = useState<ImportPlan | null>(null);
  const [approved, setApproved] = useState<Set<string>>(new Set());
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const committedRef = useRef(false);
  const tokenRef = useRef<string | null>(null);
  useEffect(() => {
    tokenRef.current = plan?.stagingToken ?? null;
  }, [plan]);

  // Cancel the staging dir on unmount if a token is live + not committed.
  useEffect(() => {
    return () => {
      if (tokenRef.current && !committedRef.current) {
        void importCancel(tokenRef.current);
      }
    };
  }, []);

  const runPreview = useCallback(async (sources: string[]) => {
    setStep('previewing');
    try {
      const p = await importPreview(sources);
      setPlan(p);
      setApproved(new Set());
      setStep('report');
    } catch (e) {
      setErrorMsg(describeError(e));
      setStep('error');
    }
  }, []);

  const pickZip = useCallback(async () => {
    const sel = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Zip archive', extensions: ['zip'] }],
    });
    if (typeof sel === 'string') void runPreview([sel]);
  }, [runPreview]);

  const pickFolder = useCallback(async () => {
    const sel = await open({ directory: true, multiple: false });
    if (typeof sel === 'string') void runPreview([sel]);
  }, [runPreview]);

  const pickFile = useCallback(async () => {
    const sel = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Winlink form', extensions: ['html', 'htm'] }],
    });
    if (typeof sel === 'string') void runPreview([sel]);
  }, [runPreview]);

  const toggleApprove = (id: string) => {
    setApproved((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const commit = useCallback(async () => {
    if (!plan) return;
    setStep('committing');
    try {
      const result = await importCommit(plan.stagingToken, [...approved]);
      committedRef.current = true;
      tokenRef.current = null;
      setStep('result');
      onDone(result);
    } catch (e) {
      setErrorMsg(describeError(e));
      setStep('error');
    }
  }, [plan, approved, onDone]);

  if (step === 'choose') {
    return (
      <div className="import-sheet" data-testid="import-sheet">
        <h3 className="import-sheet__title">Import custom forms</h3>
        <p className="import-sheet__hint">
          Bring in your organization&rsquo;s custom Winlink forms. Most groups distribute a ZIP.
        </p>
        <div className="import-sheet__choices">
          <button type="button" className="import-sheet__choice import-sheet__choice--primary"
            data-testid="import-choose-zip" onClick={() => void pickZip()}>
            Choose ZIP&hellip;
          </button>
          <button type="button" className="import-sheet__choice"
            data-testid="import-choose-folder" onClick={() => void pickFolder()}>
            Choose folder&hellip;
          </button>
          <button type="button" className="import-sheet__choice"
            data-testid="import-choose-file" onClick={() => void pickFile()}>
            Choose single file&hellip;
          </button>
        </div>
        <button type="button" className="import-sheet__ghost" onClick={onCancel}>
          Cancel
        </button>
      </div>
    );
  }

  if (step === 'previewing' || step === 'committing') {
    return (
      <div className="import-sheet" data-testid="import-sheet">
        <p className="import-sheet__busy">
          {step === 'previewing' ? 'Checking your forms…' : 'Installing…'}
        </p>
      </div>
    );
  }

  if (step === 'error') {
    return (
      <div className="import-sheet" data-testid="import-sheet">
        <p className="import-sheet__error" role="alert">{errorMsg}</p>
        <div className="import-sheet__actions">
          <button type="button" onClick={() => setStep('choose')}>Try again</button>
          <button type="button" className="import-sheet__ghost" onClick={onCancel}>Close</button>
        </div>
      </div>
    );
  }

  if (step === 'result' && plan) {
    return (
      <div className="import-sheet" data-testid="import-sheet">
        <p className="import-sheet__done" role="status">
          Imported {plan.summary.added + plan.summary.overridesStandard + approved.size} form(s).
        </p>
        <button type="button" onClick={onCancel}>Done</button>
      </div>
    );
  }

  // step === 'report'
  if (!plan) return null;
  const rows = plan.entries.filter((e) => e.kind !== 'companion');
  const companionCount = plan.summary.companions;
  const hasUpdates = rows.some((e) => e.kind === 'update');

  return (
    <div className="import-sheet" data-testid="import-sheet">
      <h3 className="import-sheet__title">Review import</h3>
      <ul className="import-sheet__report">
        {rows.map((e) => (
          <li key={`${e.folder}/${e.id}`} className="import-sheet__row" data-kind={e.kind}
            data-testid={`import-row-${e.id}`}>
            <span className="import-sheet__row-kind">{KIND_LABEL[e.kind]}</span>
            <span className="import-sheet__row-id">{e.id}</span>
            {e.folder && <span className="import-sheet__row-folder">{e.folder}</span>}
            {e.kind === 'update' && (
              <label className="import-sheet__confirm">
                <input
                  type="checkbox"
                  data-testid={`import-approve-${e.id}`}
                  checked={approved.has(e.id)}
                  onChange={() => toggleApprove(e.id)}
                />
                Replace my existing form
              </label>
            )}
            {e.kind === 'overridesStandard' && (
              <span className="import-sheet__warn" data-testid={`import-warn-override-${e.id}`}>
                {e.reason ?? `Replaces the standard ${e.id}`}
              </span>
            )}
            {(e.kind === 'added' || e.kind === 'update' || e.kind === 'overridesStandard') &&
              !e.hasViewer && (
                <span className="import-sheet__warn" data-testid={`import-warn-noviewer-${e.id}`}>
                  {NO_VIEWER_NOTE}
                </span>
              )}
            {(e.kind === 'skip' || e.kind === 'reject') && e.reason && (
              <span className="import-sheet__reason">{e.reason}</span>
            )}
          </li>
        ))}
      </ul>
      {companionCount > 0 && (
        <p className="import-sheet__companions">+ {companionCount} supporting file(s)</p>
      )}
      {hasUpdates && (
        <p className="import-sheet__updates-note">
          Forms you already have are kept unless you check &ldquo;Replace&rdquo;.
        </p>
      )}
      <div className="import-sheet__actions">
        <button type="button" className="import-sheet__commit" data-testid="import-commit"
          onClick={() => void commit()}>
          Import
        </button>
        <button type="button" className="import-sheet__ghost" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}

function describeError(e: unknown): string {
  if (e && typeof e === 'object' && 'kind' in e) {
    const k = (e as { kind: string }).kind;
    if (k === 'tokenExpired') return 'This import expired. Please choose your forms again.';
    if (k === 'commitConflict') return 'Your forms changed during import. Please preview again.';
    const reason = (e as { reason?: string }).reason;
    return reason ? `Import failed: ${reason}` : 'Import failed.';
  }
  return typeof e === 'string' ? e : 'Import failed.';
}
