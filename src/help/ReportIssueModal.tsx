/**
 * ReportIssueModal — inline overlay for Help → Report Issue (tuxlink-qjgx).
 *
 * State machine:
 *   idle → choosing-path (Save As opens) → exporting → success | canceled | error
 *
 * Spec §8.5: each failure path (Save As canceled, export error, no-browser) is
 * explicitly handled with copy-to-clipboard fallbacks.
 *
 * The modal is controller-driven: the parent passes `open` + `onClose`, and calls
 * `controller.start()` (via the exported `useReportIssueController` hook) to trigger
 * the Save As dialog and begin the flow.
 *
 * NOT a separate OS window — inline overlay per feedback_inline_ui_no_window_clutter.
 */

import { useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import type { Dispatch, SetStateAction } from 'react';

// ── State machine ──────────────────────────────────────────────────────────────

export type ReportIssueState =
  | { kind: 'idle' }
  | { kind: 'choosing-path' }
  | { kind: 'exporting'; path: string }
  | { kind: 'success'; archivePath: string; archiveSizeBytes: number; githubUrl: string; browserOpened: boolean }
  | { kind: 'canceled' }
  | { kind: 'error'; message: string; archivePath?: string; githubUrl?: string };

export interface ReportIssueResult {
  archive_path: string;
  archive_size_bytes: number;
  github_url: string;
  browser_opened: boolean;
  correlation_id: string | null;
}

// ── Controller ref pattern ─────────────────────────────────────────────────────

/**
 * A handle the AppShell places into a ref and passes to dispatchMenuAction so
 * `menu:help:report_issue` can trigger the flow without lifting state up further.
 */
export interface ReportIssueController {
  start: () => void;
}

// ── Component ─────────────────────────────────────────────────────────────────

export interface ReportIssueModalProps {
  state: ReportIssueState;
  onClose: () => void;
}

export function ReportIssueModal({ state, onClose }: ReportIssueModalProps) {
  // Esc closes (matches SettingsPanel, ThemeDesigner, AboutDialog).
  useEffect(() => {
    if (state.kind === 'idle') return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [state.kind, onClose]);

  if (state.kind === 'idle') return null;

  async function copyText(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* clipboard may be unavailable in some sandbox configurations */
    }
  }

  function openBrowserFallback(url: string) {
    // webview open — same mechanism as the backend's ShellExt::open but from
    // the frontend. Fire-and-forget; if the webview blocks this, the operator
    // can use the Copy URL button.
    window.open(url, '_blank');
  }

  return (
    <div
      className="tux-about-backdrop"
      data-testid="report-issue-backdrop"
      onClick={onClose}
    >
      <div
        className="tux-about-panel tux-report-issue-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Report Issue"
        data-testid="report-issue-panel"
        onClick={(e) => e.stopPropagation()}
        style={{ width: 'min(520px, calc(100vw - 48px))' }}
      >
        {/* Header */}
        <div className="tux-about-header">
          <h2 className="tux-about-title">Report Issue</h2>
          <button
            type="button"
            className="tux-about-close"
            data-testid="report-issue-close"
            aria-label="Close Report Issue dialog"
            onClick={onClose}
          >
            ×
          </button>
        </div>

        {/* Body */}
        <div className="tux-about-body" style={{ minHeight: 80 }}>
          {state.kind === 'choosing-path' && (
            <p style={{ margin: 0, fontSize: 13, color: 'var(--text-dim)' }}>
              Opening Save As dialog…
            </p>
          )}

          {state.kind === 'exporting' && (
            <p style={{ margin: 0, fontSize: 13, color: 'var(--text-dim)' }}>
              Exporting logs to <code style={{ fontFamily: 'var(--mono)', wordBreak: 'break-all' }}>{state.path}</code>…
            </p>
          )}

          {state.kind === 'canceled' && (
            <p
              style={{ margin: 0, fontSize: 13, color: 'var(--text-dim)' }}
              role="status"
              data-testid="report-issue-canceled-msg"
            >
              Report Issue canceled — no archive produced.
            </p>
          )}

          {state.kind === 'success' && (
            <div>
              <p style={{ margin: '0 0 10px', fontSize: 13 }}>
                Log archive saved to:{' '}
                <code
                  style={{ fontFamily: 'var(--mono)', wordBreak: 'break-all', fontSize: 12 }}
                  data-testid="report-issue-archive-path"
                >
                  {state.archivePath}
                </code>
                {' '}({formatBytes(state.archiveSizeBytes)})
              </p>

              {state.browserOpened ? (
                <p style={{ margin: '0 0 10px', fontSize: 13, color: 'var(--text-dim)' }}>
                  GitHub Issues opened in your browser. Drag the archive file into the
                  comment box to attach it.
                </p>
              ) : (
                <div>
                  <p style={{ margin: '0 0 8px', fontSize: 13, color: 'var(--text-dim)' }}>
                    Browser could not be opened automatically. Copy the URL below and
                    paste it in your browser.
                  </p>
                  <textarea
                    readOnly
                    value={state.githubUrl}
                    data-testid="report-issue-url-textarea"
                    style={{
                      width: '100%',
                      height: 60,
                      boxSizing: 'border-box',
                      fontFamily: 'var(--mono)',
                      fontSize: 11,
                      resize: 'none',
                      background: 'var(--surface-raised, #1e2030)',
                      border: '1px solid var(--border)',
                      color: 'var(--text)',
                      borderRadius: 4,
                      padding: '6px 8px',
                    }}
                  />
                </div>
              )}
            </div>
          )}

          {state.kind === 'error' && (
            <div>
              <p
                style={{ margin: '0 0 10px', fontSize: 13, color: 'var(--warn, #e89a9a)' }}
                role="status"
                data-testid="report-issue-error-msg"
              >
                {state.message}
              </p>
              {state.archivePath && (
                <p style={{ margin: '0 0 6px', fontSize: 12, color: 'var(--text-dim)' }}>
                  Archive (may be partial):{' '}
                  <code style={{ fontFamily: 'var(--mono)', fontSize: 11, wordBreak: 'break-all' }}>
                    {state.archivePath}
                  </code>
                </p>
              )}
              {state.githubUrl && (
                <p style={{ margin: '0 0 6px', fontSize: 12, color: 'var(--text-dim)' }}>
                  Open this URL manually to file the issue without an attached archive:
                </p>
              )}
              {state.githubUrl && (
                <textarea
                  readOnly
                  value={state.githubUrl}
                  data-testid="report-issue-error-url-textarea"
                  style={{
                    width: '100%',
                    height: 60,
                    boxSizing: 'border-box',
                    fontFamily: 'var(--mono)',
                    fontSize: 11,
                    resize: 'none',
                    background: 'var(--surface-raised, #1e2030)',
                    border: '1px solid var(--border)',
                    color: 'var(--text)',
                    borderRadius: 4,
                    padding: '6px 8px',
                  }}
                />
              )}
            </div>
          )}
        </div>

        {/* Actions */}
        <div
          className="tux-about-actions"
          style={{ gap: 8, flexWrap: 'wrap', justifyContent: 'flex-end' }}
        >
          {state.kind === 'success' && !state.browserOpened && (
            <>
              <button
                type="button"
                className="tux-about-button"
                data-testid="report-issue-open-browser-btn"
                onClick={() => openBrowserFallback(state.githubUrl)}
                style={{ background: 'none', color: 'var(--accent)', border: '1px solid var(--accent)' }}
              >
                Open in browser
              </button>
              <button
                type="button"
                className="tux-about-button"
                data-testid="report-issue-copy-url-btn"
                onClick={() => void copyText(state.githubUrl)}
                style={{ background: 'none', color: 'var(--text)', border: '1px solid var(--border)' }}
              >
                Copy URL
              </button>
            </>
          )}

          {state.kind === 'success' && state.browserOpened && (
            // Spec §8.5 step 3: clipboard fallback always present, regardless of
            // browser success — even if the URL opened automatically, give the
            // operator an explicit way to copy it (e.g., for filing on a
            // different machine that has the log archive).
            <button
              type="button"
              className="tux-about-button"
              data-testid="report-issue-copy-url-btn-success"
              onClick={() => void copyText(state.githubUrl)}
              style={{ background: 'none', color: 'var(--text)', border: '1px solid var(--border)' }}
            >
              Copy URL
            </button>
          )}

          {state.kind === 'success' && (
            <button
              type="button"
              className="tux-about-button"
              data-testid="report-issue-copy-path-btn"
              onClick={() => void copyText(state.archivePath)}
              style={{ background: 'none', color: 'var(--text)', border: '1px solid var(--border)' }}
            >
              Copy archive path
            </button>
          )}

          {state.kind === 'error' && state.githubUrl && (
            <button
              type="button"
              className="tux-about-button"
              data-testid="report-issue-error-copy-url-btn"
              onClick={() => void copyText(state.githubUrl!)}
              style={{ background: 'none', color: 'var(--text)', border: '1px solid var(--border)' }}
            >
              Copy URL
            </button>
          )}

          {state.kind === 'error' && state.archivePath && (
            <button
              type="button"
              className="tux-about-button"
              data-testid="report-issue-error-copy-path-btn"
              onClick={() => void copyText(state.archivePath!)}
              style={{ background: 'none', color: 'var(--text)', border: '1px solid var(--border)' }}
            >
              Copy archive path
            </button>
          )}

          <button
            type="button"
            className="tux-about-button"
            data-testid="report-issue-close-btn"
            onClick={onClose}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Controller hook ────────────────────────────────────────────────────────────

/**
 * Returns a `start` function that, when called, runs the full Report Issue flow
 * (Save As → export → GitHub URL → browser open) and drives the modal's state.
 *
 * The returned `start` function is stable across re-renders (memoised internally
 * so it is safe to embed in AppShell's `handlers` useMemo).
 */
export function useReportIssueController(
  setState: Dispatch<SetStateAction<ReportIssueState>>,
): ReportIssueController {
  const stateSetterRef = useRef(setState);
  stateSetterRef.current = setState;

  const start = useCallback(() => {
    void (async () => {
      const set = stateSetterRef.current;

      // Step 1: Show the dialog prompt state, then open Save As.
      set({ kind: 'choosing-path' });

      const ts = new Date().toISOString().replace(/[:.]/g, '-');
      const defaultName = `tuxlink-issue-${ts}.tar.zst`;

      const filePath = await saveDialog({
        defaultPath: defaultName,
        filters: [{ name: 'Tuxlink Log Archive', extensions: ['tar.zst'] }],
      });

      if (!filePath) {
        set({ kind: 'canceled' });
        return;
      }

      // Step 2: Export + build URL.
      set({ kind: 'exporting', path: filePath });

      try {
        const result = await invoke<ReportIssueResult>('report_issue_flow', {
          outputPath: filePath,
        });

        set({
          kind: 'success',
          archivePath: result.archive_path,
          archiveSizeBytes: result.archive_size_bytes,
          githubUrl: result.github_url,
          browserOpened: result.browser_opened,
        });
      } catch (e) {
        set({
          kind: 'error',
          message: `Report Issue failed: ${e}`,
        });
      }
    })();
  }, []);

  return { start };
}

// ── Helpers ────────────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units: string[] = ['KB', 'MB', 'GB'];
  let n = bytes / 1024;
  for (const u of units) {
    if (n < 1024) return `${n.toFixed(1)} ${u}`;
    n /= 1024;
  }
  return `${n.toFixed(1)} TB`;
}
