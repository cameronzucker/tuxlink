import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './UninstallCleanupDialog.css';

type CleanupMode = 'keep' | 'transient' | 'full';

type RemovalOutcome =
  | 'WouldRemove'
  | 'Removed'
  | 'Missing'
  | 'would_remove'
  | 'removed'
  | 'missing'
  | { Error: string }
  | { error: string };

interface PathRemoval {
  path: string;
  outcome: RemovalOutcome;
}

interface KeyringRemoval {
  service: string;
  account: string;
  outcome: RemovalOutcome;
}

interface CleanupReport {
  mode: CleanupMode;
  dry_run: boolean;
  paths: PathRemoval[];
  keyring: KeyringRemoval[];
  warnings: string[];
}

interface CleanupModeOption {
  id: CleanupMode;
  title: string;
  description: string;
  dryRunCommand: string;
  runCommand: string;
}

const CLEANUP_MODES: CleanupModeOption[] = [
  {
    id: 'keep',
    title: 'Keep user data',
    description: 'Normal package uninstall behavior. Messages, settings, logs, cache, and credentials remain in this Linux user profile.',
    dryRunCommand: 'tuxlink cleanup --keep --dry-run',
    runCommand: 'sudo apt remove tuxlink',
  },
  {
    id: 'transient',
    title: 'Remove transient state',
    description: 'Deletes cache, webview storage, map tiles, logs, window state, and stale pid files while preserving mailbox data and settings.',
    dryRunCommand: 'tuxlink cleanup --transient --dry-run',
    runCommand: 'tuxlink cleanup --transient',
  },
  {
    id: 'full',
    title: 'Remove all operator data',
    description: 'Deletes Tuxlink config, messages, drafts, contacts, stations, logs, cache, user-local launcher leftovers, and known keyring entries.',
    dryRunCommand: 'tuxlink cleanup --all --dry-run',
    runCommand: 'tuxlink cleanup --all',
  },
];

export interface UninstallCleanupDialogProps {
  open: boolean;
  onClose: () => void;
}

export function UninstallCleanupDialog({ open, onClose }: UninstallCleanupDialogProps) {
  const [mode, setMode] = useState<CleanupMode>('transient');
  const [previewRefresh, setPreviewRefresh] = useState(0);
  const [report, setReport] = useState<CleanupReport | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [runError, setRunError] = useState<string | null>(null);
  const [ranCleanup, setRanCleanup] = useState(false);
  const [transientConfirmed, setTransientConfirmed] = useState(false);
  const [fullConfirm, setFullConfirm] = useState('');

  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  useEffect(() => {
    if (!open) return;
    let canceled = false;
    setPreviewLoading(true);
    setPreviewError(null);
    setRunError(null);
    setRanCleanup(false);
    invoke<CleanupReport>('uninstall_cleanup_preview', { mode })
      .then((nextReport) => {
        if (!canceled) setReport(nextReport);
      })
      .catch((err) => {
        if (!canceled) {
          setReport(null);
          setPreviewError(errorMessage(err));
        }
      })
      .finally(() => {
        if (!canceled) setPreviewLoading(false);
      });
    return () => {
      canceled = true;
    };
  }, [open, mode, previewRefresh]);

  const option = CLEANUP_MODES.find((item) => item.id === mode) ?? CLEANUP_MODES[1];
  const summary = useMemo(() => summarizeReport(report), [report]);
  const canExecute =
    mode === 'transient'
      ? transientConfirmed
      : mode === 'full'
        ? fullConfirm === 'DELETE'
        : false;

  if (!open) return null;

  function chooseMode(nextMode: CleanupMode) {
    setMode(nextMode);
    setTransientConfirmed(false);
    setFullConfirm('');
  }

  async function executeCleanup() {
    if (!canExecute || mode === 'keep') return;
    setRunning(true);
    setRunError(null);
    try {
      const nextReport = await invoke<CleanupReport>('uninstall_cleanup_execute', { mode });
      setReport(nextReport);
      setRanCleanup(true);
      setTransientConfirmed(false);
      setFullConfirm('');
    } catch (err) {
      setRunError(errorMessage(err));
    } finally {
      setRunning(false);
    }
  }

  return (
    <div
      className="tux-cleanup-backdrop"
      data-testid="uninstall-cleanup-backdrop"
      onClick={onClose}
    >
      <div
        className="tux-cleanup-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Uninstall Cleanup"
        data-testid="uninstall-cleanup-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-cleanup-header">
          <div>
            <h2 className="tux-cleanup-title">Uninstall Cleanup</h2>
            <p className="tux-cleanup-subtitle">
              Remove Tuxlink data for the signed-in Linux user.
            </p>
          </div>
          <button
            type="button"
            className="tux-cleanup-close"
            data-testid="uninstall-cleanup-close"
            aria-label="Close Uninstall Cleanup dialog"
            onClick={onClose}
          >
            x
          </button>
        </div>

        <div className="tux-cleanup-body">
          <p className="tux-cleanup-note" role="note">
            Package removal such as <code>sudo apt remove tuxlink</code> keeps user data.
            Run cleanup from this user account before uninstalling, or reinstall Tuxlink
            and run cleanup afterward if the package is already gone.
          </p>

          <fieldset className="tux-cleanup-options">
            <legend>Cleanup mode</legend>
            {CLEANUP_MODES.map((item) => (
              <label
                key={item.id}
                className={`tux-cleanup-option${mode === item.id ? ' selected' : ''}`}
              >
                <input
                  type="radio"
                  name="cleanup-mode"
                  value={item.id}
                  checked={mode === item.id}
                  onChange={() => chooseMode(item.id)}
                />
                <span>
                  <strong>{item.title}</strong>
                  <small>{item.description}</small>
                </span>
              </label>
            ))}
          </fieldset>

          <div className="tux-cleanup-command-grid">
            <div>
              <span>Preview command</span>
              <code>{option.dryRunCommand}</code>
            </div>
            <div>
              <span>Uninstall command</span>
              <code>{option.runCommand}</code>
            </div>
          </div>

          <div className="tux-cleanup-preview-header">
            <h3>{ranCleanup ? 'Cleanup Result' : 'Preview'}</h3>
            <button
              type="button"
              className="tux-cleanup-secondary"
              data-testid="uninstall-cleanup-refresh"
              onClick={() => setPreviewRefresh((n) => n + 1)}
              disabled={previewLoading || running}
            >
              Refresh
            </button>
          </div>

          {previewLoading && (
            <p className="tux-cleanup-muted" role="status">
              Checking current user paths...
            </p>
          )}

          {previewError && (
            <p className="tux-cleanup-error" role="alert" data-testid="uninstall-cleanup-preview-error">
              {previewError}
            </p>
          )}

          {runError && (
            <p className="tux-cleanup-error" role="alert" data-testid="uninstall-cleanup-run-error">
              {runError}
            </p>
          )}

          {report && !previewLoading && (
            <div className="tux-cleanup-report" data-testid="uninstall-cleanup-report">
              <div className="tux-cleanup-summary" aria-label="Cleanup summary">
                <span><strong>{summary.wouldRemove}</strong> would remove</span>
                <span><strong>{summary.removed}</strong> removed</span>
                <span><strong>{summary.missing}</strong> missing</span>
                <span className={summary.errors > 0 ? 'has-errors' : ''}>
                  <strong>{summary.errors}</strong> errors
                </span>
              </div>

              {report.paths.length === 0 && report.keyring.length === 0 ? (
                <p className="tux-cleanup-muted">No Tuxlink data is selected for removal.</p>
              ) : (
                <>
                  {renderPaths(report.paths)}
                  {renderKeyring(report.keyring)}
                </>
              )}

              {report.warnings.length > 0 && (
                <div className="tux-cleanup-warnings">
                  <h4>Warnings</h4>
                  <ul>
                    {report.warnings.map((warning) => (
                      <li key={warning}>{warning}</li>
                    ))}
                  </ul>
                </div>
              )}

              {ranCleanup && (
                <p className="tux-cleanup-success" role="status" data-testid="uninstall-cleanup-success">
                  Cleanup finished. Close Tuxlink before removing or reinstalling the package.
                </p>
              )}
            </div>
          )}

          {mode === 'transient' && (
            <label className="tux-cleanup-confirm">
              <input
                type="checkbox"
                checked={transientConfirmed}
                onChange={(e) => setTransientConfirmed(e.currentTarget.checked)}
              />
              <span>I understand this will remove transient Tuxlink state for this user.</span>
            </label>
          )}

          {mode === 'full' && (
            <label className="tux-cleanup-delete-confirm">
              <span>Type DELETE to remove all Tuxlink operator data for this user.</span>
              <input
                value={fullConfirm}
                onChange={(e) => setFullConfirm(e.currentTarget.value)}
                data-testid="uninstall-cleanup-delete-confirm"
                autoCapitalize="off"
                autoCorrect="off"
              />
            </label>
          )}
        </div>

        <div className="tux-cleanup-actions">
          <button type="button" className="tux-cleanup-secondary" onClick={onClose}>
            Close
          </button>
          <button
            type="button"
            className="tux-cleanup-danger"
            data-testid="uninstall-cleanup-execute"
            onClick={() => void executeCleanup()}
            disabled={!canExecute || previewLoading || running}
          >
            {executeLabel(mode, running)}
          </button>
        </div>
      </div>
    </div>
  );
}

function renderPaths(paths: PathRemoval[]) {
  if (paths.length === 0) return null;
  return (
    <details className="tux-cleanup-details" open>
      <summary>Paths ({paths.length})</summary>
      <ul>
        {paths.map((item) => (
          <li key={item.path}>
            <span className={`tux-cleanup-outcome ${outcomeKind(item.outcome)}`}>
              {outcomeLabel(item.outcome)}
            </span>
            <code>{item.path}</code>
          </li>
        ))}
      </ul>
    </details>
  );
}

function renderKeyring(keyring: KeyringRemoval[]) {
  if (keyring.length === 0) return null;
  return (
    <details className="tux-cleanup-details">
      <summary>Known keyring entries ({keyring.length})</summary>
      <ul>
        {keyring.map((item) => (
          <li key={`${item.service}:${item.account}`}>
            <span className={`tux-cleanup-outcome ${outcomeKind(item.outcome)}`}>
              {outcomeLabel(item.outcome)}
            </span>
            <code>{item.service}:{item.account}</code>
          </li>
        ))}
      </ul>
    </details>
  );
}

function summarizeReport(report: CleanupReport | null) {
  const summary = { wouldRemove: 0, removed: 0, missing: 0, errors: 0 };
  if (!report) return summary;
  for (const outcome of [
    ...report.paths.map((item) => item.outcome),
    ...report.keyring.map((item) => item.outcome),
  ]) {
    const kind = outcomeKind(outcome);
    if (kind === 'would-remove') summary.wouldRemove += 1;
    if (kind === 'removed') summary.removed += 1;
    if (kind === 'missing') summary.missing += 1;
    if (kind === 'error') summary.errors += 1;
  }
  return summary;
}

function outcomeKind(outcome: RemovalOutcome): 'would-remove' | 'removed' | 'missing' | 'error' {
  if (typeof outcome !== 'string') return 'error';
  switch (outcome) {
    case 'WouldRemove':
    case 'would_remove':
      return 'would-remove';
    case 'Removed':
    case 'removed':
      return 'removed';
    case 'Missing':
    case 'missing':
      return 'missing';
    default:
      return 'error';
  }
}

function outcomeLabel(outcome: RemovalOutcome): string {
  if (typeof outcome !== 'string') {
    if ('Error' in outcome) return outcome.Error;
    if ('error' in outcome) return outcome.error;
    return 'error';
  }
  switch (outcomeKind(outcome)) {
    case 'would-remove':
      return 'would remove';
    case 'removed':
      return 'removed';
    case 'missing':
      return 'missing';
    case 'error':
      return 'error';
  }
}

function executeLabel(mode: CleanupMode, running: boolean): string {
  if (running) return 'Running...';
  if (mode === 'full') return 'Remove All Data';
  if (mode === 'transient') return 'Remove Transient State';
  return 'No Cleanup To Run';
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
