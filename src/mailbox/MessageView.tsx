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
//   5. attachment strip  — names + sizes + Save / image Preview
//
// The reply→compose wiring (replyActions.ts) is unchanged — it is sound; only
// the markup/labels are reshaped to the mock. State (empty/loading/not-found/
// parse-error) renders inside a centered `.reading-pane` so the pane bg/padding
// is consistent.
//
// Exported sub-components are exposed for unit tests that inject synthetic data
// without the full hook + QueryClientProvider.

import './MessageView.css';
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ContactEditor, emptyContact } from '../contacts/ContactEditor';
import { useContacts } from '../contacts/useContacts';
import type { Contact } from '../contacts/types';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { MessageViewEmpty } from './MessageViewEmpty';
import type {
  ParsedMessage,
  AttachmentMeta,
  AttachmentPreview,
  MailboxFolderRef,
  UserFolder,
} from './types';
import type { FormPayload } from '../forms/types';
import { useMessage, type MessageSelection } from './useMessage';
import { asUiError, isNotConfigured } from './types';
import { openReplyWindow, hasReplyWithFormSupport, type ReplyMode } from './replyActions';
import { sanitizeAttachmentName } from './sanitize';
import { devFormMeta } from './devFixture';
import { lookupForm, KeyValueView } from '../forms';
import { MoveToButton } from './MoveToButton';
import { WebviewFormViewer } from './WebviewFormViewer';
import { CatalogReplyView } from '../catalog/CatalogReplyView';

/// tuxlink-a2gd: a received catalog INQUIRY reply (From: SERVICE, Subject "INQUIRY - <url>").
function isCatalogReply(m: ParsedMessage): boolean {
  return (m.from ?? '').toUpperCase().includes('SERVICE') && (m.subject ?? '').startsWith('INQUIRY - ');
}

// ============================================================================
// Exported constants (used by tests)
// ============================================================================

// tuxlink-djnl: MessageViewEmpty + its copy live in MessageViewEmpty.tsx so
// AppShell can import the empty state eagerly without pulling MessageView's
// forms-registry dependency graph. Re-exported here for backward compat
// with existing tests that import { SELECT_MESSAGE_COPY, MessageViewEmpty }
// from './MessageView'.
export { MessageViewEmpty, SELECT_MESSAGE_COPY } from './MessageViewEmpty';
export const NOT_FOUND_COPY = 'Message not found. It may have been deleted or moved.';
export const PARSE_ERROR_PREFIX = 'This message could not be parsed';
export const FORM_PLACEHOLDER = 'Form rendering coming soon.';

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

// ============================================================================
// State sub-components — rendered inside a centered reading-pane
// ============================================================================

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

function isPreviewableImageName(filename: string): boolean {
  return /\.(?:jpe?g|png|gif|webp|bmp)$/i.test(filename);
}

function attachmentErrorDetail(e: unknown): string {
  const uiError = asUiError(e);
  if (!uiError) {
    return e instanceof Error ? e.message : String(e);
  }

  switch (uiError.kind) {
    case 'NotConfigured':
    case 'NotFound':
    case 'Rejected':
      return uiError.detail;
    case 'AuthFailed':
    case 'Transport':
    case 'Unavailable':
      return uiError.detail.reason;
    case 'Internal':
      return uiError.detail.detail;
    case 'Cancelled':
      return 'Cancelled';
  }
}

/** Format a UTC ISO-8601 date-time to the mock's "<date> <HH:MM> UTC · <HH:MM tz>"
 *  (UTC primary for emcomm + the operator's local time, as the mock shows). */
function formatHeaderDate(isoDate: string): string {
  try {
    const d = new Date(isoDate);
    if (isNaN(d.getTime())) return isoDate;
    const pad = (n: number) => String(n).padStart(2, '0');
    const utc =
      `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
      `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())} UTC`;
    const local = d.toLocaleTimeString(undefined, {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
      timeZoneName: 'short',
    });
    return `${utc} · ${local}`;
  } catch {
    return isoDate;
  }
}

/** Split an RFC5322-style address into display name + bare address.
 *  "Mike / Net Control <K0SWE@winlink.org>" → { name, addr }; a bare address
 *  → { name: '', addr }. Used so the reading pane can show the mock's
 *  "K0SWE@winlink.org · Mike / Net Control". */
function parseAddress(s: string): { name: string; addr: string } {
  const m = s.match(/^\s*(.*?)\s*<([^>]+)>\s*$/);
  if (m) return { name: m[1].replace(/^"|"$/g, '').trim(), addr: m[2].trim() };
  return { name: '', addr: s.trim() };
}

/// Derive the bare callsign to prefill an "Add to contacts" editor from a
/// sender address (G1). Strips a trailing `@winlink.org` (the Winlink email
/// form is the bare callsign) but preserves any other SMTP address verbatim and
/// keeps the SSID — the callsign is the SSID-bearing identity.
export function senderCallsign(from: string): string {
  const { addr } = parseAddress(from);
  const at = addr.indexOf('@');
  if (at >= 0 && addr.slice(at + 1).toLowerCase() === 'winlink.org') {
    return addr.slice(0, at);
  }
  return addr;
}

interface TextMatch {
  start: number;
  end: number;
}

function findTextMatches(text: string, query: string): TextMatch[] {
  const needle = query.trim();
  if (!needle) return [];
  const haystack = text.toLowerCase();
  const lowerNeedle = needle.toLowerCase();
  const matches: TextMatch[] = [];
  let at = 0;
  while (at < haystack.length) {
    const found = haystack.indexOf(lowerNeedle, at);
    if (found === -1) break;
    matches.push({ start: found, end: found + lowerNeedle.length });
    at = found + lowerNeedle.length;
  }
  return matches;
}

function highlightedText(text: string, matches: TextMatch[], activeIndex: number): ReactNode {
  if (matches.length === 0) return text;
  const nodes: ReactNode[] = [];
  let cursor = 0;
  matches.forEach((match, index) => {
    if (match.start > cursor) nodes.push(text.slice(cursor, match.start));
    const active = index === activeIndex;
    nodes.push(
      <mark
        key={`${match.start}-${match.end}`}
        className={`message-find-match${active ? ' active' : ''}`}
        data-testid="message-find-match"
        data-active={active ? 'true' : 'false'}
        data-message-find-active={active ? 'true' : undefined}
      >
        {text.slice(match.start, match.end)}
      </mark>,
    );
    cursor = match.end;
  });
  if (cursor < text.length) nodes.push(text.slice(cursor));
  return nodes;
}

// ============================================================================
// Form-body rendering (Tasks 13 + 11)
// ============================================================================

/**
 * Render the body region for a message whose `isForm` is true and whose
 * payload parsed successfully. Routing precedence (per spec §8.3):
 *
 *   1. If `lookupForm(formId)` returns a `FormRegistryEntry` (native view —
 *      ICS-213, ICS-309, Bulletin, Position, Damage Assessment),
 *      use the entry's `View` component. This is the highest-fidelity
 *      rendering and remains the canonical path for the 5 bundled forms.
 *   2. Otherwise (catalog / custom / unknown formId), mount a
 *      `WebviewFormViewer`. The Rust side serves the WLE
 *      `*_Viewer.html` template with the parsed payload's field values
 *      bound into both `{var X}` placeholders and `[name="X"]` inputs.
 *   3. If the WebviewFormViewer fails (resolved Viewer template missing
 *      on disk — catalog drift or a custom form without a companion
 *      `_Viewer.html`), fall through to `KeyValueView`. This guarantees
 *      something always renders for the operator.
 *
 * Step (3) is gated by local state (`viewerFailed`) so a transient
 * open-error from the Rust side doesn't permanently bury the message
 * behind KeyValueView for the rest of the session — the next time the
 * operator selects this message, MessageView remounts with fresh state
 * and the viewer attempt re-runs.
 */
function FormMessageBody({
  formId,
  payload,
  bodyText,
  radioDrawerOpen = false,
}: {
  formId: string;
  payload: FormPayload;
  bodyText: string;
  radioDrawerOpen?: boolean;
}) {
  // Track viewer-fallback failure so we can fall through to KeyValueView.
  // Reset semantics: state lives for the lifetime of this component
  // instance (one per message-selection). Selecting a different message
  // remounts, clearing the flag.
  const [viewerFailed, setViewerFailed] = useState(false);

  // Native-View path: registered forms render via their dedicated React
  // component (preserves existing behavior for ICS-213 et al.).
  const entry = lookupForm(formId);
  if (entry) {
    const ViewComponent = entry.View;
    return (
      <div className="form-attached" data-testid="message-form-rendered">
        <ViewComponent payload={payload} />
      </div>
    );
  }

  // KeyValueView fallback: either the viewer reported failure, OR an
  // operator-flagged escape hatch. Renders the raw field/value pairs from
  // the parsed payload alongside the original body text. This is the
  // safety net that guarantees the message stays readable even if every
  // upstream path fails.
  if (viewerFailed) {
    return (
      <div className="form-attached" data-testid="message-form-unknown">
        <KeyValueView payload={payload} bodyText={bodyText} />
      </div>
    );
  }

  // Viewer-mode fallback (P1 Task 11): convert the payload's
  // `[fieldId, value]` pairs into a plain object the Tauri command can
  // marshal into a HashMap. The Rust side handles {var X} substitution
  // + JS injection for `[name="X"]` inputs.
  const fieldValues: Record<string, string> = {};
  for (const [k, v] of payload.fields) {
    fieldValues[k] = v;
  }

  return (
    <div className="form-attached" data-testid="message-form-viewer">
      <WebviewFormViewer
        formId={formId}
        fieldValues={fieldValues}
        onClose={() => setViewerFailed(true)}
        onFallback={() => setViewerFailed(true)}
        suppressed={radioDrawerOpen}
      />
      <div
        className="message-form-print-fallback"
        data-testid="message-form-print-fallback"
        aria-hidden="true"
      >
        <KeyValueView payload={payload} bodyText={bodyText} />
      </div>
    </div>
  );
}

// ============================================================================
// Loaded view
// ============================================================================

/**
 * Fully-loaded message view (Mock B `.reading-pane`). Accepts a `ParsedMessage`
 * directly so tests can inject synthetic data without a Tauri runtime.
 *
 * `onArchive` (tuxlink-ca5x) renders an Archive button in the action bar when
 * supplied. AppShell builds the closure and omits it when the open message
 * is already in Archive (where archiving is a no-op).
 */
export function MessageViewLoaded({
  message,
  onArchive,
  currentFolder,
  userFolders,
  onMove,
  onEditDraft,
  contacts,
  onAddContact,
  radioDrawerOpen = false,
}: {
  message: ParsedMessage;
  onArchive?: () => void;
  /// Current folder of the open message (used by `MoveToButton` to disable
  /// the self-target row and gate the Archive button). Optional so existing
  /// tests that inject a bare `ParsedMessage` keep working — the move +
  /// archive UI is suppressed when this is absent.
  currentFolder?: MailboxFolderRef;
  /// Operator's user folders, shown in the Move-to dropdown. tuxlink-f62f.
  userFolders?: UserFolder[];
  /// Move-to-folder callback. When supplied + `currentFolder` is present,
  /// the reading-pane toolbar renders a "Move ▾" dropdown alongside Archive.
  onMove?: (to: MailboxFolderRef) => void;
  /// Drafts are local-only; editing is an explicit reading-pane action.
  onEditDraft?: () => void;
  /// G1 (Task A8) — the operator's saved contacts. When supplied (with
  /// `onAddContact`), the action bar renders an "Add to contacts" button for a
  /// sender that is NOT already a contact; clicking it opens an inline
  /// ContactEditor prefilled with the sender callsign. Omitted by the
  /// presentational unit tests, which keeps the contacts UI off and avoids the
  /// QueryClient dependency.
  contacts?: Contact[];
  /// Persist an added-from-sender contact (routes through `contact_upsert`).
  onAddContact?: (contact: Contact) => Promise<void> | void;
  /// When true, any open form-viewer webview is hidden while the radio
  /// drawer is open. Threaded from AppShell via MessageView (tuxlink-813d).
  radioDrawerOpen?: boolean;
}) {
  const paneRef = useRef<HTMLDivElement | null>(null);
  const findInputRef = useRef<HTMLInputElement | null>(null);
  const from = parseAddress(message.from);
  const toAddrs = message.to.map(parseAddress);
  // G1 — add-from-sender. Active only when contacts state + handler are wired.
  const [addingContact, setAddingContact] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState('');
  const [findActiveIndex, setFindActiveIndex] = useState(0);
  const senderCs = senderCallsign(message.from);
  const alreadyContact =
    !!contacts &&
    contacts.some((c) => c.callsign.toLowerCase() === senderCs.toLowerCase());
  const canAddContact = !!contacts && !!onAddContact && senderCs.length > 0 && !alreadyContact;
  // Form metadata (the Mock B "Form" row + form-attached box). Dev-only today;
  // ParsedMessage carries `isForm` but not the form kind/payload yet.
  const formMeta = message.isForm ? devFormMeta(message.id) : null;
  const [formCode, ...formRest] = (formMeta?.formKind ?? '').split(' · ');
  const isDraft = currentFolder === 'drafts';
  const findNeedle = findQuery.trim();
  const findMatches = useMemo(
    () => findTextMatches(message.body, findNeedle),
    [message.body, findNeedle],
  );
  const findCountText =
    findNeedle && findMatches.length > 0
      ? `${findActiveIndex + 1}/${findMatches.length}`
      : '0/0';
  const bodyWithFind = findOpen && findNeedle
    ? highlightedText(message.body, findMatches, findActiveIndex)
    : message.body;
  const showCatalogFindRaw = findOpen && findNeedle && isCatalogReply(message);
  const moveFind = useCallback((delta: number) => {
    setFindActiveIndex((cur) => {
      if (findMatches.length === 0) return 0;
      return (cur + delta + findMatches.length) % findMatches.length;
    });
  }, [findMatches.length]);

  useEffect(() => {
    setFindActiveIndex(0);
  }, [findNeedle, message.id]);

  useEffect(() => {
    if (findMatches.length > 0 && findActiveIndex >= findMatches.length) {
      setFindActiveIndex(0);
    }
  }, [findMatches.length, findActiveIndex]);

  useEffect(() => {
    if (!findOpen) return;
    findInputRef.current?.focus();
  }, [findOpen]);

  useEffect(() => {
    const active = paneRef.current?.querySelector('[data-message-find-active="true"]');
    if (active instanceof HTMLElement && typeof active.scrollIntoView === 'function') {
      active.scrollIntoView({ block: 'center', inline: 'nearest' });
    }
  }, [findActiveIndex, findNeedle]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        setFindOpen(true);
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  return (
    <div className="reading-pane" data-testid="message-view-loaded" ref={paneRef}>
      {/* 1 — action bar (Mock B: Reply primary amber · Reply All · Forward) */}
      <div className="actions" role="group" aria-label="Message actions">
        {isDraft ? (
          <button
            type="button"
            className="action-btn primary"
            data-testid="edit-draft-btn"
            onClick={onEditDraft}
          >
            Edit Draft
          </button>
        ) : (
          <>
            <button
              type="button"
              className="action-btn primary"
              data-testid="reply-btn"
              onClick={() => fireReply(message, 'reply')}
            >
              Reply (Ctrl+R)
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
            {message.isForm
              && message.formId
              && lookupForm(message.formId)
              && hasReplyWithFormSupport(message.formId) && (
                <button
                  type="button"
                  className="action-btn"
                  data-testid="reply-with-form-btn"
                  title="Reply with the same form type, pre-populated with sender↔recipient swap"
                  onClick={() => fireReply(message, 'replyWithForm')}
                >
                  Reply with form…
                </button>
              )}
          </>
        )}
        <button
          type="button"
          className="action-btn"
          data-testid="message-find-btn"
          title="Find in message (Ctrl+Shift+F)"
          onClick={() => setFindOpen(true)}
        >
          Find
        </button>
        {!isDraft && onArchive && (
          <button
            type="button"
            className="action-btn"
            data-testid="archive-btn"
            title="Archive (A)"
            onClick={onArchive}
          >
            Archive
          </button>
        )}
        {!isDraft && onMove && currentFolder && (
          <MoveToButton
            currentFolder={currentFolder}
            userFolders={userFolders ?? []}
            onMove={onMove}
          />
        )}
        {/* G1 — Add the sender to contacts (suggest-only counterpart for an
            individual message). Hidden when the sender is already a contact. */}
        {!isDraft && canAddContact && !addingContact && (
          <button
            type="button"
            className="action-btn"
            data-testid="add-to-contacts-btn"
            title={`Add ${senderCs} to contacts`}
            onClick={() => setAddingContact(true)}
          >
            Add to contacts
          </button>
        )}
      </div>

      {findOpen && (
        <div className="message-find-bar" data-testid="message-find-bar" role="search">
          <input
            ref={findInputRef}
            className="message-find-input"
            data-testid="message-find-input"
            aria-label="Find in message"
            value={findQuery}
            onChange={(e) => setFindQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                moveFind(e.shiftKey ? -1 : 1);
              } else if (e.key === 'Escape') {
                e.preventDefault();
                setFindOpen(false);
              }
            }}
          />
          <span className="message-find-count" data-testid="message-find-count" aria-live="polite">
            {findCountText}
          </span>
          <button
            type="button"
            className="action-btn"
            data-testid="message-find-prev"
            disabled={findMatches.length < 2}
            onClick={() => moveFind(-1)}
          >
            Prev
          </button>
          <button
            type="button"
            className="action-btn"
            data-testid="message-find-next"
            disabled={findMatches.length < 2}
            onClick={() => moveFind(1)}
          >
            Next
          </button>
          <button
            type="button"
            className="action-btn"
            data-testid="message-find-close"
            onClick={() => setFindOpen(false)}
          >
            Close
          </button>
        </div>
      )}

      {/* G1 — inline ContactEditor prefilled with the sender callsign. Inline
          (no popup window); replaces nothing — sits below the action bar. */}
      {!isDraft && addingContact && onAddContact && (
        <div className="message-add-contact" data-testid="message-add-contact">
          <ContactEditor
            contact={emptyContact(senderCs)}
            onSave={async (c) => {
              await onAddContact(c);
              setAddingContact(false);
            }}
            onCancel={() => setAddingContact(false)}
          />
        </div>
      )}

      <div className="message-print-header">
        {/* 2 — subject heading */}
        <h1 className="subject-line" data-testid="message-subject">
          {message.subject}
        </h1>

        {/* 3 — From / To / Date (+ Form when the message is a Winlink form) */}
        <dl className="msg-meta">
          <dt>From</dt>
          <dd>
            <span className="addr" data-testid="message-from">
              {from.addr}
            </span>
            {from.name && <span className="from-name"> · {from.name}</span>}
          </dd>

          <dt>To</dt>
          <dd data-testid="message-to">
            {toAddrs.length === 0
              ? '—'
              : toAddrs.map((a, i) => (
                  <span key={i}>
                    {i > 0 && ', '}
                    <span className="addr">{a.addr}</span>
                  </span>
                ))}
          </dd>

          <dt>Date</dt>
          <dd data-testid="message-date">{formatHeaderDate(message.date)}</dd>

          {/* Winlink message ID — tuxlink-gtno. Surfaced for support / forensics
              / log-correlation workflows where the operator needs to quote the
              ID in a thread or grep wl2k-go logs. Mono so paste-targets line up. */}
          <dt>ID</dt>
          <dd data-testid="message-id">
            <span className="msg-id">{message.id}</span>
          </dd>

          {message.isForm && (
            <>
              <dt>Form</dt>
              <dd data-testid="message-form-kind">
                {formMeta ? (
                  <>
                    <span className="form-kind-code">{formCode}</span>
                    {formRest.length > 0 && ` · ${formRest.join(' · ')}`}
                  </>
                ) : (
                  'Winlink form'
                )}
              </dd>
            </>
          )}
        </dl>
      </div>

      {/* 4 — body. Form messages dispatch to a registered View component
          (e.g., Ics213View). If the form_id is not registered, the Viewer-
          mode webview fallback (P1 Task 11) tries to render the WLE
          `_Viewer.html` template with the parsed payload bound; if that
          fails (template missing on disk), KeyValueView renders the raw
          field/value pairs as a final fallback. If isForm is true but
          there's no parsed payload (parse failed), fall back to the
          legacy "form attached" placeholder. Plain messages render the
          decoded body. */}
      {message.isForm && message.formId && message.formPayload ? (
        <FormMessageBody
          key={message.id}
          formId={message.formId}
          payload={message.formPayload}
          bodyText={message.body}
          radioDrawerOpen={radioDrawerOpen}
        />
      ) : message.isForm ? (
        // isForm true but no payload — parse failed server-side or message
        // is a form by attachment-name but XML couldn't be parsed.
        <div className="form-attached" data-testid="message-form-placeholder">
          <strong className="form-attached-title">Winlink form attached.</strong>{' '}
          {FORM_PLACEHOLDER}
          {formMeta && (
            <div className="form-attached-meta">
              Form: {formMeta.formCode} · payload: {formMeta.payloadBytes} B XML
            </div>
          )}
        </div>
      ) : isCatalogReply(message) ? (
        // tuxlink-a2gd: catalog INQUIRY replies (From: SERVICE, Subject: "INQUIRY - <url>")
        // render via parse-with-fallback — area weather structured, everything else raw.
        showCatalogFindRaw ? (
          <pre className="catalog-reply__raw" data-testid="message-body">
            {bodyWithFind}
          </pre>
        ) : (
          <CatalogReplyView subject={message.subject} body={message.body} />
        )
      ) : (
        <pre className="msg-body" data-testid="message-body">
          {bodyWithFind}
        </pre>
      )}

      {/* 5 — attachment strip — names + sizes + Save / image Preview */}
      {message.attachments.length > 0 && (
        <AttachmentStrip
          attachments={message.attachments}
          messageId={message.id}
          folder={currentFolder}
        />
      )}
    </div>
  );
}

// ============================================================================
// Attachment strip (tuxlink-0fyj — Save As; tuxlink-ewtb — image Preview)
// ============================================================================

/**
 * Per-attachment download status. Cleared after a few seconds so the UI
 * never shows a stale success/failure label.
 */
type AttachStatus =
  | { kind: 'idle' }
  | { kind: 'saving' }
  | { kind: 'saved'; path: string }
  | { kind: 'error'; detail: string };

type PreviewStatus =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'shown'; filename: string; mimeType: string; dataUrl: string }
  | { kind: 'error'; detail: string };

/**
 * Click-to-save attachment strip. Each item is a button that opens the
 * native Save As dialog, then routes through `message_attachment_save`
 * to write the decoded bytes to the chosen path. Common image files also
 * expose an on-demand preview that fetches bytes through
 * `message_attachment_preview` only after the operator asks for them.
 *
 * Disabled when `folder` is undefined (tests injecting bare ParsedMessage
 * without selection context). The Save button is also suppressed for
 * dev/legacy callers that omit it.
 */
export function AttachmentStrip({
  attachments,
  messageId,
  folder,
}: {
  attachments: AttachmentMeta[];
  messageId: string;
  folder: MailboxFolderRef | undefined;
}) {
  const [status, setStatus] = useState<Record<number, AttachStatus>>({});
  const [preview, setPreview] = useState<Record<number, PreviewStatus>>({});

  async function handleSave(index: number, a: AttachmentMeta) {
    if (!folder) return;
    setStatus((s) => ({ ...s, [index]: { kind: 'saving' } }));
    try {
      const destPath = await saveDialog({
        defaultPath: sanitizeAttachmentName(a.filename),
        title: `Save ${a.filename}`,
      });
      if (!destPath) {
        setStatus((s) => ({ ...s, [index]: { kind: 'idle' } }));
        return;
      }
      await invoke('message_attachment_save', {
        folder,
        id: messageId,
        filename: a.filename,
        destPath,
      });
      setStatus((s) => ({ ...s, [index]: { kind: 'saved', path: destPath } }));
      // Auto-clear after 4s so the row returns to the actionable state.
      setTimeout(() => {
        setStatus((s) => {
          if (s[index]?.kind !== 'saved') return s;
          const next = { ...s };
          delete next[index];
          return next;
        });
      }, 4000);
    } catch (e) {
      setStatus((s) => ({
        ...s,
        [index]: { kind: 'error', detail: attachmentErrorDetail(e) },
      }));
    }
  }

  async function handlePreview(index: number, a: AttachmentMeta) {
    if (!folder) return;
    if (preview[index]?.kind === 'shown') {
      setPreview((s) => {
        const next = { ...s };
        delete next[index];
        return next;
      });
      return;
    }

    setPreview((s) => ({ ...s, [index]: { kind: 'loading' } }));
    try {
      const result = await invoke<AttachmentPreview>('message_attachment_preview', {
        folder,
        id: messageId,
        filename: a.filename,
      });
      setPreview((s) => ({
        ...s,
        [index]: {
          kind: 'shown',
          filename: result.filename,
          mimeType: result.mimeType,
          dataUrl: `data:${result.mimeType};base64,${result.dataBase64}`,
        },
      }));
    } catch (e) {
      setPreview((s) => ({
        ...s,
        [index]: { kind: 'error', detail: attachmentErrorDetail(e) },
      }));
    }
  }

  return (
    <div className="msg-attachments" data-testid="message-attachments">
      <span className="msg-attachments-label">Attachments:</span>
      <ul className="msg-attachment-list">
        {attachments.map((a: AttachmentMeta, i: number) => {
          const st = status[i] ?? { kind: 'idle' };
          const previewStatus = preview[i] ?? { kind: 'idle' };
          const safeName = sanitizeAttachmentName(a.filename);
          const canPreview = folder && isPreviewableImageName(a.filename);
          return (
            <li key={i} className="msg-attachment-item">
              <div className="msg-attachment-row">
                <span className="msg-attachment-name">{safeName}</span>
                <span className="msg-attachment-size">{formatAttachSize(a.size)}</span>
                {canPreview && (
                  <button
                    type="button"
                    className="msg-attachment-preview"
                    data-testid={`attachment-preview-${i}`}
                    disabled={previewStatus.kind === 'loading'}
                    onClick={() => handlePreview(i, a)}
                    title={`${previewStatus.kind === 'shown' ? 'Hide' : 'Preview'} ${safeName}`}
                  >
                    {previewStatus.kind === 'loading'
                      ? 'Loading...'
                      : previewStatus.kind === 'shown'
                        ? 'Hide'
                        : 'Preview'}
                  </button>
                )}
                {folder && (
                  <button
                    type="button"
                    className="msg-attachment-save"
                    data-testid={`attachment-save-${i}`}
                    disabled={st.kind === 'saving'}
                    onClick={() => handleSave(i, a)}
                    title={`Save ${safeName} to disk`}
                  >
                    {st.kind === 'saving' ? 'Saving…' : 'Save'}
                  </button>
                )}
                {st.kind === 'saved' && (
                  <span
                    className="msg-attachment-status msg-attachment-status--ok"
                    data-testid={`attachment-status-${i}`}
                  >
                    ✓ Saved
                  </span>
                )}
                {st.kind === 'error' && (
                  <span
                    className="msg-attachment-status msg-attachment-status--err"
                    data-testid={`attachment-status-${i}`}
                    title={st.detail}
                  >
                    ✗ Failed
                  </span>
                )}
              </div>
              {previewStatus.kind === 'shown' && (
                <div className="msg-attachment-preview-frame" data-testid={`attachment-preview-frame-${i}`}>
                  <img
                    className="msg-attachment-preview-image"
                    data-testid={`attachment-preview-image-${i}`}
                    src={previewStatus.dataUrl}
                    alt={previewStatus.filename}
                  />
                </div>
              )}
              {previewStatus.kind === 'error' && (
                <span
                  className="msg-attachment-status msg-attachment-status--err"
                  data-testid={`attachment-preview-status-${i}`}
                  title={previewStatus.detail}
                >
                  Preview failed
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

// ============================================================================
// Main component
// ============================================================================

export interface MessageViewProps {
  /** The selected message (folder + id). Null when nothing is selected. */
  selectedMessage: MessageSelection | null;
  /** Move-to-Archive callback (tuxlink-ca5x). When supplied, the loaded
   *  reading pane renders an Archive button. AppShell omits this when the
   *  open message is already in Archive. */
  onArchive?: () => void;
  /** Operator's user folders, shown in the Move-to dropdown (tuxlink-f62f). */
  userFolders?: UserFolder[];
  /** Move-to-folder callback. When supplied, the reading pane renders a
   *  "Move ▾" dropdown that lists system folders + user folders. */
  onMove?: (to: MailboxFolderRef) => void;
  /** Open a selected local draft in the compose editor. */
  onEditDraft?: (draftId: string) => void;
  /** When true, any open form-viewer child webview is hidden while the
   *  radio drawer is open (compact-mode overlay coexistence, tuxlink-813d).
   *  Defaults to false so existing call sites that omit it keep working. */
  radioDrawerOpen?: boolean;
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
export default function MessageView({
  selectedMessage,
  onArchive,
  userFolders,
  onMove,
  onEditDraft,
  radioDrawerOpen = false,
}: MessageViewProps) {
  const { data, isLoading, isError, error } = useMessage(selectedMessage);
  // G1 (Task A8) — wire the real contacts state so a sender that isn't already
  // a contact gets an "Add to contacts" action in the loaded reading pane.
  const { contacts, upsertContact } = useContacts();

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

  return (
    <MessageViewLoaded
      message={data}
      onArchive={onArchive}
      currentFolder={selectedMessage.folder}
      userFolders={userFolders}
      onMove={onMove}
      onEditDraft={
        selectedMessage.folder === 'drafts' && onEditDraft
          ? () => onEditDraft(selectedMessage.id)
          : undefined
      }
      contacts={contacts}
      onAddContact={upsertContact}
      radioDrawerOpen={radioDrawerOpen}
    />
  );
}
