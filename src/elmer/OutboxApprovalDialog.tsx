/**
 * OutboxApprovalDialog — literal staged-outbox manifest for operator review
 * before Elmer connects to transmit (AC-3, AC-10, AC-12).
 *
 * Displays the FULL content of every staged message (to/cc/subject/body
 * verbatim) — ground-truth rendering, never model prose (AC-12). The header
 * reads "Connecting transmits ALL N messages" so the operator knows the exact
 * scope before arming.
 *
 * Flow (two-step to satisfy the digest-gate):
 *   1. On open: call `outbox_staged_list` to fetch the manifest.
 *   2. Call `elmer_prepare_outbox_approval` to freeze staging and get the
 *      approval token.
 *   3. On operator confirm: call `elmer_connect({ approval })` with the token.
 *   4. On digest mismatch error: surface "outbox changed since you reviewed —
 *      re-review" and re-fetch from step 1.
 *   5. On dismiss: close without connecting.
 *
 * Per-row Remove: calls `onRemove(mid)` — the parent is responsible for
 * calling the appropriate backend command (remove is v2; v1 is whole-set or
 * dismiss). In v1 the Remove button calls onRemove for UI completeness even if
 * the backend does not yet support mid-level removal.
 *
 * Security: the component never shows model prose — only the verbatim
 * `StagedRecordView` fields from `outbox_staged_list`. The approval token
 * is opaque to the UI (digest/epoch/expiry are round-tripped, not parsed).
 */

import { memo, useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

// ---------------------------------------------------------------------------
// DTOs — mirror src-tauri/src/elmer/commands.rs (serde camelCase)
// ---------------------------------------------------------------------------

/** Verbatim record from outbox_staged_list (StagedRecordView in Rust). */
export interface StagedRecordView {
  mid: string;
  to: string[];
  cc: string[];
  subject: string;
  body: string;
}

/** Opaque approval token from elmer_prepare_outbox_approval (OutboxApprovalDto). */
export interface OutboxApprovalDto {
  approvalId: string;
  digest: string;
  sessionEpoch: number;
  expiresUnix: number;
}

// ---------------------------------------------------------------------------
// Phase
// ---------------------------------------------------------------------------

type DialogPhase =
  | 'loading'   // fetching manifest + approval
  | 'review'    // operator is reviewing the manifest
  | 'sending'   // elmer_connect in flight
  | 'mismatch'  // digest mismatch — outbox changed; must re-review
  | 'error';    // unclassified error

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface OutboxApprovalDialogProps {
  /** Called when the operator dismisses without approving. */
  onClose: () => void;
  /** Called when elmer_connect succeeds. */
  onConnected: () => void;
  /** Called when the operator clicks Remove on a record (v1: UI-only; v2: backend). */
  onRemove?: (mid: string) => void;
}

export const OutboxApprovalDialog = memo(function OutboxApprovalDialog({
  onClose,
  onConnected,
  onRemove,
}: OutboxApprovalDialogProps) {
  const [phase, setPhase] = useState<DialogPhase>('loading');
  const [records, setRecords] = useState<StagedRecordView[]>([]);
  const [approval, setApproval] = useState<OutboxApprovalDto | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Step 1+2: fetch the manifest and the approval token.
  const fetchManifest = useCallback(async () => {
    setPhase('loading');
    setErrorMsg(null);
    try {
      const recs = await invoke<StagedRecordView[]>('outbox_staged_list');
      setRecords(recs);

      const token = await invoke<OutboxApprovalDto>('elmer_prepare_outbox_approval');
      setApproval(token);
      setPhase('review');
    } catch (e) {
      setErrorMsg(String(e));
      setPhase('error');
    }
  }, []);

  useEffect(() => {
    void fetchManifest();
  }, [fetchManifest]);

  // Step 3: operator arms-to-send.
  const handleConnect = useCallback(async () => {
    if (!approval) return;
    setPhase('sending');
    try {
      await invoke('elmer_connect', { approval });
      onConnected();
    } catch (e) {
      const msg = String(e);
      // The backend returns a distinct error string for digest mismatch.
      if (msg.includes('outbox changed') || msg.includes('digest')) {
        setPhase('mismatch');
      } else {
        setErrorMsg(msg);
        setPhase('error');
      }
    }
  }, [approval, onConnected]);

  // Step 4: re-review after a mismatch.
  const handleReReview = useCallback(() => {
    void fetchManifest();
  }, [fetchManifest]);

  return (
    <div
      className="outbox-approval-dialog"
      data-testid="outbox-approval-dialog"
      role="dialog"
      aria-label="Review staged outbox before connecting"
    >
      <div className="obd-header">
        {phase === 'loading' ? (
          <span data-testid="obd-loading">Loading staged messages…</span>
        ) : phase === 'mismatch' ? (
          <span data-testid="obd-mismatch-header">
            Outbox changed since you reviewed — re-review before connecting.
          </span>
        ) : (
          <span data-testid="obd-manifest-header">
            Connecting transmits ALL {records.length} message{records.length !== 1 ? 's' : ''}
          </span>
        )}
      </div>

      {/* Manifest table */}
      {(phase === 'review' || phase === 'sending') && records.length > 0 && (
        <div className="obd-manifest" data-testid="obd-manifest">
          {records.map((rec) => (
            <div
              key={rec.mid}
              className="obd-record"
              data-testid="obd-record"
              data-mid={rec.mid}
            >
              <div className="obd-record-header">
                <span className="obd-record-to" data-testid="obd-record-to">
                  To: {rec.to.join(', ')}
                </span>
                {rec.cc.length > 0 && (
                  <span className="obd-record-cc" data-testid="obd-record-cc">
                    Cc: {rec.cc.join(', ')}
                  </span>
                )}
                <span className="obd-record-subject" data-testid="obd-record-subject">
                  Subject: {rec.subject}
                </span>
              </div>
              <pre className="obd-record-body" data-testid="obd-record-body">
                {rec.body}
              </pre>
              {onRemove && (
                <button
                  type="button"
                  className="obd-record-remove"
                  data-testid={`obd-remove-${rec.mid}`}
                  onClick={() => onRemove(rec.mid)}
                >
                  Remove
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Mismatch re-review callout */}
      {phase === 'mismatch' && (
        <div className="obd-mismatch" data-testid="obd-mismatch">
          <p>The outbox changed while you were reviewing. Re-review to see the current contents.</p>
          <button
            type="button"
            className="obd-re-review-button"
            data-testid="obd-re-review"
            onClick={handleReReview}
          >
            Re-review
          </button>
        </div>
      )}

      {/* Error state */}
      {phase === 'error' && errorMsg && (
        <div className="obd-error" data-testid="obd-error" role="alert">
          {errorMsg}
        </div>
      )}

      {/* Action row */}
      <div className="obd-actions" data-testid="obd-actions">
        {(phase === 'review') && (
          <button
            type="button"
            className="obd-connect-button obd-connect-button--primary"
            data-testid="obd-connect"
            onClick={() => void handleConnect()}
          >
            Arm to send ({records.length} message{records.length !== 1 ? 's' : ''})
          </button>
        )}
        {phase === 'sending' && (
          <span data-testid="obd-sending">Connecting…</span>
        )}
        <button
          type="button"
          className="obd-cancel-button"
          data-testid="obd-cancel"
          onClick={onClose}
          disabled={phase === 'sending'}
        >
          Cancel
        </button>
      </div>
    </div>
  );
});
