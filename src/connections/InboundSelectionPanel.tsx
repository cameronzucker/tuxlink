// Inbound pending-message selection panel — WLE "Review Pending Messages"
// parity (tuxlink-bsiy). Inline overlay panel (no pop-up window). The overlay
// STRUCTURE (fixed-position backdrop, role="dialog", design-token CSS, lazy +
// Suspense mount) is modeled on CatalogBuilderPanel. ESC-to-close is an
// improvement this panel adds; CatalogBuilderPanel has neither ESC-close nor
// backdrop-click-close. There is no backdrop onClick here either — accidental
// dismissal of a modal selection dialog is undesirable.
//
// Columns are sender / subject / MID / uncompressed / compressed. Sender and
// subject come from the CMS `;PM:` manifest (tuxlink-9u07u) — the whole pending
// list is reviewed at once, matching WLE. They may be blank for an `FC`-only
// path (no manifest), in which case only MID + sizes show. All rows are
// PRE-CHECKED on open (download-everything is the common case + the WLE
// default); the operator unchecks what they want to hold/delete. The
// Hold/Delete radio controls the disposition of the UNCHECKED messages.
//
// The countdown is COSMETIC. The backend's recv_timeout (45s) is the single
// source of truth for the timeout — on reaching 0 the panel auto-submits the
// current checkbox state, and the backend's own timeout independently covers
// the case where this UI is gone.

import { useEffect, useRef, useState } from 'react';
import { formatSize } from '../mailbox/MessageList';
import type { InboundSelection, PendingProposalDto, UnselectedDisposition } from './sessionTypes';
import './InboundSelectionPanel.css';

export interface InboundSelectionPanelProps {
  proposals: PendingProposalDto[];
  onSubmit: (selection: InboundSelection) => void;
  onClose: () => void;
  /// Cosmetic countdown start (seconds). Defaults to 45 to mirror the backend
  /// recv_timeout; the backend timeout — not this — is authoritative.
  countdownSeconds?: number;
}

const DEFAULT_COUNTDOWN_SECONDS = 45;

export function InboundSelectionPanel({
  proposals,
  onSubmit,
  onClose,
  countdownSeconds = DEFAULT_COUNTDOWN_SECONDS,
}: InboundSelectionPanelProps) {
  // Selection initialized to ALL mids (pre-checked).
  const [checked, setChecked] = useState<Set<string>>(
    () => new Set(proposals.map((p) => p.mid)),
  );
  const [disposition, setDisposition] = useState<UnselectedDisposition>('hold');
  const [remaining, setRemaining] = useState(countdownSeconds);

  const submitWith = (sel: Set<string>, disp: UnselectedDisposition) =>
    onSubmit({ selected_mids: [...sel], disposition: disp });

  // Latest selection snapshot for the auto-submit-at-zero path. The countdown
  // interval is set up ONCE (stable under fake timers); reading current state
  // through a ref avoids re-subscribing the interval on every checkbox toggle
  // (which would otherwise reset the timer on each click).
  const latest = useRef({ checked, disposition });
  latest.current = { checked, disposition };
  // Guard so the auto-submit fires exactly once even though the interval may
  // tick `remaining` below 0 before the clear-on-unmount runs.
  const autoSubmitted = useRef(false);

  // ESC closes (an improvement over CatalogBuilderPanel, which has no keyboard dismiss).
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  // Cosmetic countdown: a single interval ticks `remaining` down once per
  // second. Set up once so it advances cleanly under fake timers (a chained
  // self-rescheduling setTimeout doesn't re-arm reliably across React state
  // commits under jsdom fake timers). The functional updater floors at 0 so a
  // late tick can't drive the displayed countdown negative.
  useEffect(() => {
    const id = setInterval(() => setRemaining((r) => (r > 0 ? r - 1 : 0)), 1000);
    return () => clearInterval(id);
  }, []);

  // Auto-submit when the countdown reaches 0. Reads the LATEST checkbox state
  // via the ref and fires exactly once (the autoSubmitted guard). The backend
  // recv_timeout is authoritative — this is a courtesy that dismisses the panel
  // before the backend's own timeout fires.
  useEffect(() => {
    if (remaining <= 0 && !autoSubmitted.current) {
      autoSubmitted.current = true;
      submitWith(latest.current.checked, latest.current.disposition);
    }
    // submitWith/latest are intentionally omitted: fire exactly once on the
    // remaining<=0 transition. eslint-disable-next-line react-hooks/exhaustive-deps
  }, [remaining]); // eslint-disable-line react-hooks/exhaustive-deps

  const toggle = (mid: string) =>
    setChecked((prev) => {
      const next = new Set(prev);
      next.has(mid) ? next.delete(mid) : next.add(mid);
      return next;
    });

  const selectAll = () => setChecked(new Set(proposals.map((p) => p.mid)));
  const deselectAll = () => setChecked(new Set());

  const checkedCount = checked.size;

  return (
    <div
      className="inbound-selection-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Review Pending Messages"
    >
      <div className="inbound-selection">
        <header className="inbound-selection__header">
          <h2>Review Pending Messages</h2>
          <span
            className="inbound-selection__countdown"
            data-testid="inbound-countdown"
            aria-label={`Auto-download in ${remaining} seconds`}
          >
            {remaining}s
          </span>
          <button
            className="inbound-selection__close"
            onClick={onClose}
            aria-label="Close"
          >
            ×
          </button>
        </header>

        <div className="inbound-selection__toolbar">
          <button type="button" onClick={selectAll}>
            Select All
          </button>
          <button type="button" onClick={deselectAll}>
            Deselect All
          </button>
          <span className="inbound-selection__count">
            {checkedCount} of {proposals.length} selected
          </span>
        </div>

        <div className="inbound-selection__col-head" aria-hidden="true">
          <span />
          <span>From</span>
          <span>Subject</span>
          <span>Message ID</span>
          <span>Size</span>
          <span>Compressed</span>
        </div>

        <ul className="inbound-selection__list">
          {proposals.map((p) => (
            <li key={p.mid} className="inbound-selection__row">
              <label>
                <input
                  type="checkbox"
                  aria-label={`Select message ${p.mid}`}
                  checked={checked.has(p.mid)}
                  onChange={() => toggle(p.mid)}
                />
              </label>
              <span className="inbound-selection__sender" title={p.sender}>
                {p.sender || '—'}
              </span>
              <span className="inbound-selection__subject" title={p.subject}>
                {p.subject || '(no subject)'}
              </span>
              <span className="inbound-selection__mid" title={p.mid}>
                {p.mid}
              </span>
              <span className="inbound-selection__size">
                {formatSize(p.uncompressed_size)}
              </span>
              <span className="inbound-selection__size">
                {formatSize(p.compressed_size)}
              </span>
            </li>
          ))}
        </ul>

        <footer className="inbound-selection__footer">
          <fieldset className="inbound-selection__disposition">
            <legend>Unchecked:</legend>
            <label>
              <input
                type="radio"
                name="inbound-disposition"
                aria-label="Hold"
                checked={disposition === 'hold'}
                onChange={() => setDisposition('hold')}
              />
              Hold
            </label>
            <label>
              <input
                type="radio"
                name="inbound-disposition"
                aria-label="Delete"
                checked={disposition === 'delete'}
                onChange={() => setDisposition('delete')}
              />
              Delete
            </label>
          </fieldset>
          <button
            type="button"
            className="inbound-selection__go"
            onClick={() => submitWith(checked, disposition)}
          >
            Download {checkedCount} Checked
          </button>
        </footer>
      </div>
    </div>
  );
}
