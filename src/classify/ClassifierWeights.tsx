// ClassifierWeights.tsx — the shared classifier-weights acquisition surface
// (tuxlink-13ofm), rendered by the first-run wizard step (variant="wizard")
// and the Elmer panel gate (variant="panel"). One component, one job, one
// status: whatever the user is looking at shows the same truth.
//
// Operator-decided flow: the download is a first-class persistent job. The
// wizard shows it INLINE with an explicit "Continue setup while it downloads"
// act; the panel keeps showing it (with retry / switch source / cancel) until
// weights are ready; completion flips the surface and fires a desktop
// notification (Rust side). Sideload exists because digest pinning makes a
// folder exactly as verified as the download.

import { useCallback, useState } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import {
  formatMb,
  weightsDownloadCancel,
  weightsDownloadStart,
  weightsSideloadImport,
  type WeightsStatus,
} from './weightsApi';
import { useWeightsJob } from './useWeightsJob';
import './ClassifierWeights.css';

export interface ClassifierWeightsProps {
  variant: 'wizard' | 'panel';
  /** Wizard: advance the step. Fired by "Continue" (ready) and by
   * "Continue setup while it downloads". */
  onContinue?: () => void;
  /** Wizard: skip without starting anything. */
  onSkip?: () => void;
}

export function ClassifierWeights({ variant, onContinue, onSkip }: ClassifierWeightsProps) {
  const { status, progress, accept } = useWeightsJob();
  const [customSource, setCustomSource] = useState('');
  const [showSource, setShowSource] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const act = useCallback(
    async (fn: () => Promise<WeightsStatus>) => {
      setBusy(true);
      setActionError(null);
      try {
        accept(await fn());
      } catch (e) {
        setActionError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [accept],
  );

  const startDownload = useCallback(
    () => act(() => weightsDownloadStart(showSource ? customSource : undefined)),
    [act, customSource, showSource],
  );

  const cancelDownload = useCallback(() => act(() => weightsDownloadCancel()), [act]);

  const pickFolder = useCallback(async () => {
    setActionError(null);
    let dir: string | string[] | null = null;
    try {
      dir = await openDialog({ directory: true, multiple: false });
    } catch (e) {
      setActionError(`folder picker unavailable: ${String(e)}`);
      return;
    }
    if (typeof dir === 'string' && dir !== '') {
      await act(() => weightsSideloadImport(dir as string));
    }
  }, [act]);

  if (status == null) {
    return (
      <div className="cw-root" data-testid="classifier-weights">
        <p className="cw-dim">Checking classifier model…</p>
      </div>
    );
  }

  const job = status.job;
  const jobActive =
    job !== null &&
    (job.state === 'waiting' || job.state === 'downloading' || job.state === 'verifying');
  const jobFailed = job !== null && job.state === 'failed';
  const sizeLabel = formatMb(status.totalBytes);

  // ---- ready ----
  if (status.ready && !jobActive) {
    return (
      <div className="cw-root" data-testid="classifier-weights">
        <p className="cw-ok" data-testid="cw-ready">
          Classifier model installed
          {status.integrity === 'digest-pinned'
            ? ' — every file digest-verified against this release'
            : ''}
          .
        </p>
        {status.location !== null && <p className="cw-dim cw-path">{status.location}</p>}
        {variant === 'wizard' && (
          <div className="cw-actions">
            <button type="button" className="cw-btn cw-btn-primary" onClick={onContinue}>
              Continue
            </button>
          </div>
        )}
      </div>
    );
  }

  // ---- job running ----
  if (jobActive) {
    const pct =
      progress !== null && progress.total > 0
        ? Math.min(100, Math.round((progress.got / progress.total) * 100))
        : null;
    const phaseLabel =
      job.state === 'waiting'
        ? (job.detail ?? 'Waiting…')
        : job.state === 'verifying'
          ? `Verifying ${job.file ?? progress?.file ?? ''}…`
          : `Downloading ${job.file ?? progress?.file ?? ''}`;
    return (
      <div className="cw-root" data-testid="classifier-weights">
        <p className="cw-line" data-testid="cw-job-phase">
          {phaseLabel}
        </p>
        {progress !== null && (
          <>
            <div
              className="cw-bar"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={pct ?? undefined}
            >
              <div className="cw-bar-fill" style={{ width: `${pct ?? 0}%` }} />
            </div>
            <p className="cw-dim" data-testid="cw-job-bytes">
              {formatMb(progress.got)} of {formatMb(progress.total)}
              {job.filesDone.length > 0 && ` · ${job.filesDone.length} of 3 files done`}
            </p>
          </>
        )}
        <p className="cw-dim cw-path">from {job.source}</p>
        <div className="cw-actions">
          {variant === 'wizard' && (
            <button
              type="button"
              className="cw-btn cw-btn-primary"
              data-testid="cw-continue-while-downloading"
              onClick={onContinue}
            >
              Continue setup while it downloads
            </button>
          )}
          <button
            type="button"
            className="cw-btn"
            data-testid="cw-cancel"
            disabled={busy}
            onClick={() => void cancelDownload()}
          >
            Cancel
          </button>
        </div>
        {actionError !== null && <p className="cw-err">{actionError}</p>}
      </div>
    );
  }

  // ---- failed / idle ----
  const failureGuidance =
    jobFailed && job.errorClass === 'digest-mismatch'
      ? 'The file the source served is NOT what this release vouches for, so it was refused and removed. Retry re-downloads; if it repeats, the source is serving different content.'
      : jobFailed && job.errorClass === 'source'
        ? 'This source does not have usable files. Switch the source, or install from a folder.'
        : null;

  return (
    <div className="cw-root" data-testid="classifier-weights">
      {variant === 'wizard' && !jobFailed && (
        <>
          <p className="cw-line">
            Tuxlink can run a small on-device classifier model ({sizeLabel}). Downloading it
            now, while you have internet, keeps everything working offline later — in the
            field it can be installed from a folder instead.
          </p>
          <p className="cw-dim">
            Every file is verified against digests pinned in this release before it is
            installed, whatever the source.
          </p>
        </>
      )}
      {variant === 'panel' && !jobFailed && (
        <p className="cw-line">
          The on-device classifier model ({sizeLabel}) is not installed yet.
        </p>
      )}

      {jobFailed && (
        <>
          <p className="cw-err" data-testid="cw-job-error">
            {job.detail ?? 'The weights job failed.'}
          </p>
          {failureGuidance !== null && <p className="cw-dim">{failureGuidance}</p>}
        </>
      )}

      <div className="cw-actions">
        <button
          type="button"
          className="cw-btn cw-btn-primary"
          data-testid="cw-download"
          disabled={busy}
          onClick={() => void startDownload()}
        >
          {jobFailed
            ? job.errorClass === 'cancelled' || job.errorClass === 'network'
              ? 'Resume download'
              : 'Retry download'
            : `Download (${sizeLabel})`}
        </button>
        <button
          type="button"
          className="cw-btn"
          data-testid="cw-sideload"
          disabled={busy}
          onClick={() => void pickFolder()}
        >
          Install from a folder…
        </button>
        {variant === 'wizard' && (
          <button
            type="button"
            className="cw-btn cw-btn-quiet"
            data-testid="cw-skip"
            onClick={onSkip}
          >
            Skip for now
          </button>
        )}
      </div>

      <button
        type="button"
        className="cw-source-toggle"
        data-testid="cw-source-toggle"
        onClick={() => setShowSource((v) => !v)}
      >
        {showSource ? 'Use the default source' : 'Download from a different source…'}
      </button>
      {showSource && (
        <input
          className="cw-source-input"
          data-testid="cw-source-input"
          type="text"
          placeholder={status.defaultSource}
          value={customSource}
          onChange={(e) => setCustomSource(e.target.value)}
          spellCheck={false}
        />
      )}

      {variant === 'wizard' && (
        <p className="cw-dim">
          You can do this any time later from the Elmer panel.
        </p>
      )}
      {actionError !== null && <p className="cw-err">{actionError}</p>}
    </div>
  );
}
