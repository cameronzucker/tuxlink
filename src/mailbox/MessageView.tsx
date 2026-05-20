// Message reading pane — the right pane of the Mock D shell.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.3, §4.2
// bd issue: tuxlink-y5c (Task 13); rebuilt to Mock D under tuxlink-yd4.
//
// DOM matches the approved mock's `.reading-pane`, ported verbatim and in order:
//   1. `.actions`        — Reply (amber primary) · Reply All · Forward · Print
//   2. `h1.subject-line` — the subject
//   3. `dl.msg-meta`     — From / To / Date (+ Via when routing is known)
//   4. `pre.msg-body`    — the decoded body (form → placeholder box)
//   5. attachment strip  — names + sizes only (v0.0.1; open/preview is v0.1)
//
// The reply→compose wiring (replyActions.ts) is unchanged — it is sound; only
// the markup/labels are reshaped to the mock. State (empty/loading/not-found/
// parse-error) renders inside a centered `.reading-pane` so the pane bg/padding
// is consistent.
//
// Exported sub-components are exposed for unit tests that inject synthetic data
// without the full hook + QueryClientProvider.

import './MessageView.css';
import type { ParsedMessage, AttachmentMeta } from './types';
import { useMessage, type MessageSelection } from './useMessage';
import { asUiError, isNotConfigured } from './types';
import { openReplyWindow, type ReplyMode } from './replyActions';

// ============================================================================
// Exported constants (used by tests)
// ============================================================================

export const SELECT_MESSAGE_COPY = 'Select a message to read.';
export const NOT_FOUND_COPY = 'Message not found. It may have been deleted or moved.';
export const PARSE_ERROR_PREFIX = 'This message could not be parsed';
export const FORM_PLACEHOLDER =
  'This message contains a Winlink form. Form rendering arrives in v0.1.';

/**
 * Open a reply / reply-all / forward compose window. Window-open failure is
 * non-fatal: openReplyWindow has already seeded the prefilled draft into the
 * store, so it appears in Drafts even if the IPC to spawn the window rejects.
 */
function fireReply(message: ParsedMessage, mode: ReplyMode): void {
  openReplyWindow(message, mode).catch(() => {
    /* non-fatal — surfaced via Rust logs; draft is saved */
  });
}

/** Print the current message via the webview's native print (Ctrl/Cmd+P parity). */
function firePrint(): void {
  try {
    window.print?.();
  } catch {
    /* print unavailable (headless/test) — no-op */
  }
}

// ============================================================================
// State sub-components — rendered inside a centered reading-pane
// ============================================================================

/** Shown when no message is selected. */
export function MessageViewEmpty() {
  return (
    <div
      className="reading-pane reading-pane--center"
      data-testid="message-view-empty"
    >
      {SELECT_MESSAGE_COPY}
    </div>
  );
}

/** Shown when the backend returns UiError::NotFound (deleted / moved message). */
export function MessageViewNotFound() {
  return (
    <div
      className="reading-pane reading-pane--center"
      data-testid="message-view-not-found"
    >
      {NOT_FOUND_COPY}
    </div>
  );
}

/** Shown when the Rust command returns a parse error (UiError::Internal). */
export function MessageViewParseError({ rawSize }: { rawSize?: number }) {
  const sizeNote = rawSize !== undefined ? ` (raw size ${rawSize} bytes)` : '';
  return (
    <div
      className="reading-pane reading-pane--center reading-pane--error"
      data-testid="message-parse-error"
    >
      {PARSE_ERROR_PREFIX}
      {sizeNote}.
    </div>
  );
}

/** Format bytes to a human-readable size string (e.g. "1.2 KB"). */
function formatAttachSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Format a UTC ISO-8601 date-time string to a compact UTC display label. */
function formatHeaderDate(isoDate: string): string {
  try {
    const d = new Date(isoDate);
    if (isNaN(d.getTime())) return isoDate;
    const pad = (n: number) => String(n).padStart(2, '0');
    return (
      `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
      `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())} UTC`
    );
  } catch {
    return isoDate;
  }
}

// ============================================================================
// Loaded view
// ============================================================================

/**
 * Fully-loaded message view (Mock D `.reading-pane`). Accepts a `ParsedMessage`
 * directly so tests can inject synthetic data without a Tauri runtime.
 */
export function MessageViewLoaded({ message }: { message: ParsedMessage }) {
  return (
    <div className="reading-pane" data-testid="message-view-loaded">
      {/* 1 — action bar (Reply primary amber · Reply All · Forward · Print) */}
      <div className="actions" role="group" aria-label="Message actions">
        <button
          type="button"
          className="action-btn primary"
          data-testid="reply-btn"
          onClick={() => fireReply(message, 'reply')}
        >
          Reply
        </button>
        <button
          type="button"
          className="action-btn"
          data-testid="reply-all-btn"
          onClick={() => fireReply(message, 'replyAll')}
        >
          Reply All
        </button>
        <button
          type="button"
          className="action-btn"
          data-testid="forward-btn"
          onClick={() => fireReply(message, 'forward')}
        >
          Forward
        </button>
        <button
          type="button"
          className="action-btn"
          data-testid="print-btn"
          onClick={firePrint}
        >
          Print
        </button>
      </div>

      {/* 2 — subject heading */}
      <h1 className="subject-line" data-testid="message-subject">
        {message.subject}
      </h1>

      {/* 3 — From / To / Date (+ Via when routing is known) */}
      <dl className="msg-meta">
        <dt>From</dt>
        <dd>
          <span className="addr" data-testid="message-from">
            {message.from}
          </span>
        </dd>

        <dt>To</dt>
        <dd data-testid="message-to">
          {message.to.length > 0
            ? message.to.map((addr, i) => (
                <span key={i}>
                  {i > 0 && ', '}
                  <span className="addr">{addr}</span>
                </span>
              ))
            : '—'}
        </dd>

        <dt>Date</dt>
        <dd data-testid="message-date">{formatHeaderDate(message.date)}</dd>

        {message.routing !== null && message.routing !== undefined && (
          <>
            <dt>Via</dt>
            <dd data-testid="message-routing">{message.routing}</dd>
          </>
        )}
      </dl>

      {/* 4 — body (form payloads render a placeholder, never raw XML) */}
      {message.isForm ? (
        <div className="form-placeholder" data-testid="message-form-placeholder">
          {FORM_PLACEHOLDER}
        </div>
      ) : (
        <pre className="msg-body" data-testid="message-body">
          {message.body}
        </pre>
      )}

      {/* 5 — attachment strip — names + sizes only (no download/preview in v0.0.1) */}
      {message.attachments.length > 0 && (
        <div className="msg-attachments" data-testid="message-attachments">
          <span className="msg-attachments-label">Attachments:</span>
          <ul className="msg-attachment-list">
            {message.attachments.map((a: AttachmentMeta, i: number) => (
              <li key={i} className="msg-attachment-item">
                <span className="msg-attachment-name">{a.filename}</span>
                <span className="msg-attachment-size">{formatAttachSize(a.size)}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Main component
// ============================================================================

export interface MessageViewProps {
  /** The selected message (folder + id). Null when nothing is selected. */
  selectedMessage: MessageSelection | null;
}

/**
 * Parse the raw byte count from a `UiError::Internal` detail string
 * ("message too large to parse (N bytes; cap is M bytes)"). Returns undefined
 * when the detail carries no size (e.g. an RFC5322 parse failure).
 */
function parseRawSizeFromDetail(detail: string | undefined): number | undefined {
  if (!detail) return undefined;
  const m = detail.match(/\((\d+)\s+bytes/);
  if (!m) return undefined;
  const n = parseInt(m[1], 10);
  return isNaN(n) ? undefined : n;
}

/**
 * Reading pane — the right pane of the `.panes` grid. Delegates fetching to
 * `useMessage`; renders one of five states (empty / loading / not-found /
 * parse-error / loaded). Selection comes from AppShell's `selectedMessage`.
 */
export default function MessageView({ selectedMessage }: MessageViewProps) {
  const { data, isLoading, isError, error } = useMessage(selectedMessage);

  if (!selectedMessage) {
    return <MessageViewEmpty />;
  }

  if (isLoading) {
    return (
      <div
        className="reading-pane reading-pane--center"
        data-testid="message-view-loading"
        aria-label="Loading message..."
      />
    );
  }

  if (isError || !data) {
    const uiErr = asUiError(error);

    // NotConfigured → "not connected" empty state (not an error toast).
    if (isNotConfigured(error)) {
      return <MessageViewEmpty />;
    }

    // NotFound → message was deleted or moved; show distinct state.
    if (uiErr?.kind === 'NotFound') {
      return <MessageViewNotFound />;
    }

    // Internal (parse failure or oversized message) → parse-error state.
    const detail =
      uiErr?.kind === 'Internal' ? (uiErr.detail as { detail: string }).detail : undefined;
    const rawSize = parseRawSizeFromDetail(detail);
    return <MessageViewParseError rawSize={rawSize} />;
  }

  return <MessageViewLoaded message={data} />;
}
