// Task 13 — Message reading pane.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.3, §4.1
// bd issue: tuxlink-y5c
//
// Renders the right-hand reading pane of the main shell. The component is
// wired into AppShell's `reader` region by the orchestrator integration
// commit (spec §4.3); until then it is independently unit-testable.
//
// State: none beyond the `useMessage(selection)` query; selection comes from
// AppShell's `selectedMessage: { folder, id } | null` prop.
//
// Spec §5.3 behaviours:
//   - No selection → "Select a message to read." empty state.
//   - Loading → spinner (no flicker on fast responses).
//   - Error (UiError::NotFound) → "message not found" state.
//   - Error (UiError::Internal from a parse failure) → "could not parse" state.
//   - Loaded:
//       Header strip: sender · UTC sent · routing (omit when null).
//       Body: `<pre>` with word-wrap.
//       Form payload (isForm) → placeholder text (no raw XML).
//       Attachments: names + sizes only; no download/preview (v0.1).
//
// Exported sub-components (`MessageViewLoaded`, `MessageViewEmpty`,
// `MessageViewNotFound`, `MessageViewParseError`) are exposed for unit tests
// that inject synthetic data without going through the full hook +
// QueryClientProvider.

import './MessageView.css';
import type { ParsedMessage, AttachmentMeta } from './types';
import { useMessage, type MessageSelection } from './useMessage';
import { asUiError, isNotConfigured } from './types';

// ============================================================================
// Exported constants (used by tests)
// ============================================================================

export const SELECT_MESSAGE_COPY = 'Select a message to read.';
export const NOT_FOUND_COPY = 'Message not found. It may have been deleted or moved.';
export const PARSE_ERROR_PREFIX = 'This message could not be parsed';
export const FORM_PLACEHOLDER =
  'This message contains a Winlink form. Form rendering arrives in v0.1.';

// ============================================================================
// Sub-components
// ============================================================================

/** Shown when no message is selected. */
export function MessageViewEmpty() {
  return (
    <div className="message-view message-view--empty" data-testid="message-view-empty">
      {SELECT_MESSAGE_COPY}
    </div>
  );
}

/** Shown when the backend returns UiError::NotFound (deleted / moved message). */
export function MessageViewNotFound() {
  return (
    <div className="message-view message-view--not-found" data-testid="message-view-not-found">
      {NOT_FOUND_COPY}
    </div>
  );
}

/** Shown when the Rust command returns a parse error (UiError::Internal). */
export function MessageViewParseError({ rawSize }: { rawSize?: number }) {
  const sizeNote = rawSize !== undefined ? ` (raw size ${rawSize} bytes)` : '';
  return (
    <div className="message-view message-view--error" data-testid="message-parse-error">
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

/** Format a UTC ISO-8601 date-time string to a compact display label. */
function formatHeaderDate(isoDate: string): string {
  try {
    const d = new Date(isoDate);
    if (isNaN(d.getTime())) return isoDate;
    const pad = (n: number) => String(n).padStart(2, '0');
    return (
      `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
      `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}Z`
    );
  } catch {
    return isoDate;
  }
}

/**
 * Fully-loaded message view. Accepts a `ParsedMessage` directly so tests
 * can inject synthetic data without requiring a Tauri runtime.
 */
export function MessageViewLoaded({ message }: { message: ParsedMessage }) {
  return (
    <div className="message-view message-view--loaded">
      {/* Header strip */}
      <header className="message-view__header">
        <div className="message-view__subject" data-testid="message-subject">
          {message.subject}
        </div>
        <div className="message-view__meta">
          <span className="message-view__from" data-testid="message-from">
            {message.from}
          </span>
          <span className="message-view__date" data-testid="message-date">
            {formatHeaderDate(message.date)}
          </span>
          {message.routing !== null && message.routing !== undefined && (
            <span className="message-view__routing" data-testid="message-routing">
              {message.routing}
            </span>
          )}
        </div>
      </header>

      {/* Body / form */}
      <div className="message-view__body-area">
        {message.isForm ? (
          <div
            className="message-view__form-placeholder"
            data-testid="message-form-placeholder"
          >
            {FORM_PLACEHOLDER}
          </div>
        ) : (
          <pre className="message-view__body" data-testid="message-body">
            {message.body}
          </pre>
        )}
      </div>

      {/* Attachment strip — v0.0.1: names + sizes only; no download/preview */}
      {message.attachments.length > 0 && (
        <div className="message-view__attachments" data-testid="message-attachments">
          <span className="message-view__attachments-label">Attachments:</span>
          <ul className="message-view__attachment-list">
            {message.attachments.map((a: AttachmentMeta, i: number) => (
              <li key={i} className="message-view__attachment-item">
                <span className="message-view__attachment-name">{a.filename}</span>
                <span className="message-view__attachment-size">
                  {formatAttachSize(a.size)}
                </span>
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
 * Parse the raw byte count from a `UiError::Internal` detail string.
 *
 * The Rust `parse_raw_rfc5322` function emits:
 *   "message too large to parse (N bytes; cap is M bytes)"
 * Extract N via a simple regex; return `undefined` if the detail doesn't
 * contain a size (e.g., an RFC5322 parse failure rather than the cap check).
 */
function parseRawSizeFromDetail(detail: string | undefined): number | undefined {
  if (!detail) return undefined;
  const m = detail.match(/\((\d+)\s+bytes/);
  if (!m) return undefined;
  const n = parseInt(m[1], 10);
  return isNaN(n) ? undefined : n;
}

/**
 * Message reading pane — the `reader` region of the AppShell grid.
 *
 * Delegates data-fetching to `useMessage`; renders one of five states:
 *   1. Empty        — no selection.
 *   2. Loading      — query in flight.
 *   3. Not-found    — UiError::NotFound (deleted / moved message).
 *   4. Parse-error  — UiError::Internal from the Rust parse boundary.
 *   5. Loaded       — full parsed message.
 *
 * Spec §4.3: wired into AppShell's reader region by the orchestrator
 * integration commit; this component does NOT import or reference AppShell.
 */
export default function MessageView({ selectedMessage }: MessageViewProps) {
  const { data, isLoading, isError, error } = useMessage(selectedMessage);

  if (!selectedMessage) {
    return <MessageViewEmpty />;
  }

  if (isLoading) {
    return (
      <div
        className="message-view message-view--loading"
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

    // Internal (parse failure or oversized message) → show parse-error state
    // with raw byte count when available in the detail string.
    const detail =
      uiErr?.kind === 'Internal' ? (uiErr.detail as { detail: string }).detail : undefined;
    const rawSize = parseRawSizeFromDetail(detail);
    return <MessageViewParseError rawSize={rawSize} />;
  }

  return <MessageViewLoaded message={data} />;
}
