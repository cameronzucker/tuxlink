// Compose window — separate Tauri window per AMD-6 + spec §5.4.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.4
// bd issue: tuxlink-dm8 (Task 14 — compose window)
//
// This component is mounted at `/compose/:draftId` inside a separate Tauri
// window labeled `compose-<draftId>`. It is NOT a Radix Dialog inside the
// main shell — AMD-6 locked decision #2.
//
// Cc field disposition (tuxlink-h1km, 2026-06-01):
//   The Cc field is ENABLED end-to-end. The original v0.0.1-era rationale
//   was Pat 1.0.0's `send_message` dropping Cc silently; Pat is fully
//   stripped (project_pat_complete_strip_directive_2026_05_30) and the
//   native B2F path writes RFC 5322 `Cc:` headers in compose_message
//   (winlink/compose.rs L65-67). End-to-end verification trace:
//     Compose.tsx cc state → OutboundDraftDto.cc → ui_commands.rs
//     message_send (L741) → NativeBackend.send_message
//     (winlink_backend.rs L636) → compose_message_with_files (cc &[&str])
//     → compose_message → add_header("Cc", …) per recipient.
//
// Key behaviors (spec §5.4):
//   - Autosave to localStorage every 2s
//   - Restore on reopen (via draftId prop / URL param)
//   - Clear on successful send
//   - Close with unsaved changes → "Save draft / Discard / Cancel" dialog
//   - Ctrl+S → save; Ctrl+Enter → send
//   - message_send Ok(_) → "Posted to Outbox" success
//   - From / Send-as / Select-Template → disabled (deferred-feature tooltip)
//   - Attachments list + drop zone (stubbed — multipart attachment wiring
//     is deferred until the form-aware send path lands)
//
// DOES NOT import from AppShell.tsx or listen for menu:file:new (Codex F7:
// compose windows must not listen for that event — it would spawn nested
// compose windows).

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { clearDraft, loadDraft, saveDraft, splitAddrs } from './useDraft';
import { ComposeTitleBar } from './ComposeTitleBar';
import { ResizeHandles } from '../shell/chrome/ResizeHandles';
import { FormPicker, lookupForm, allForms } from '../forms';
import './Compose.css';

// ============================================================================
// Types
// ============================================================================

/** Attachment transferred over the Tauri IPC layer. `bytes` is base64-encoded
 * by serde-json's default Vec<u8> serialization on the Rust side. The
 * file-picker UI (HTML Forms, PR #151) is not yet built; pass [] until then. */
interface OutboundAttachmentDto {
  filename: string;
  bytes: number[];
}

interface OutboundDraftDto {
  to: string[];
  cc: string[];
  subject: string;
  body: string;
  /** P2.1 bridge: attachments threaded through IPC. Pass [] until the
   *  attachment-picker UI is built (HTML Forms PR #151). */
  attachments: OutboundAttachmentDto[];
}

type SendState = 'idle' | 'sending' | 'success' | 'error';

type FormMode =
  | { kind: 'plain' }
  | { kind: 'pick' }
  | { kind: 'form'; formId: string; values: Record<string, string> };

type CloseAction = 'close' | 'switch-to-form' | null;

interface ClosePromptState {
  open: boolean;
  action: CloseAction;
}

// ============================================================================
// Props
// ============================================================================

export interface ComposeProps {
  /// The stable draft id — provided via URL param `/compose/:draftId` or
  /// directly as a prop in tests. Drives localStorage keying.
  draftId: string;
}

// ============================================================================
// Component
// ============================================================================

export function Compose({ draftId }: ComposeProps) {
  // Field state — restored from localStorage on mount
  const [to, setTo] = useState('');
  const [cc, setCc] = useState('');
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');
  const [requestAck, setRequestAck] = useState(false);

  // Form mode state (T6.1)
  const [formMode, setFormMode] = useState<FormMode>({ kind: 'plain' });
  const [callsign, setCallsign] = useState<string>('');
  const [grid, setGrid] = useState<string>('');

  // Send + close state
  const [sendState, setSendState] = useState<SendState>('idle');
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [closePrompt, setClosePrompt] = useState<ClosePromptState>({
    open: false,
    action: null,
  });

  // Attachment stub (v0.0.1 DONE_WITH_CONCERNS: Pat multipart wiring deferred)
  const [attachments, _setAttachments] = useState<string[]>([]);

  // Track the "clean" snapshot so we can detect unsaved changes on close
  const savedSnapshotRef = useRef({ to: '', cc: '', subject: '', body: '', requestAck: false });
  // Set to true after a successful send — gates the autosave interval so it
  // cannot recreate the draft that was intentionally cleared (Codex P1).
  const sentRef = useRef(false);
  // Track if the user has interacted (only prompt on genuine changes)
  const isDirty = useCallback(() => {
    const s = savedSnapshotRef.current;
    // Form mode is "dirty" iff there are any non-empty field values
    if (formMode.kind === 'form') {
      return Object.values(formMode.values).some((v) => v.trim().length > 0);
    }
    return (
      to !== s.to ||
      cc !== s.cc ||
      subject !== s.subject ||
      body !== s.body ||
      requestAck !== s.requestAck
    );
  }, [to, cc, subject, body, requestAck, formMode]);

  // ============================================================================
  // Restore on mount
  // ============================================================================

  useEffect(() => {
    const draft = loadDraft(draftId);
    if (draft) {
      setTo(draft.to);
      // `cc` is optional on the DraftData shape for back-compat with drafts
      // saved before tuxlink-h1km landed; default to ''.
      setCc(draft.cc ?? '');
      setSubject(draft.subject);
      setBody(draft.body);
      setRequestAck(draft.requestAck);
      savedSnapshotRef.current = {
        to: draft.to,
        cc: draft.cc ?? '',
        subject: draft.subject,
        body: draft.body,
        requestAck: draft.requestAck,
      };
      if (draft.formId) {
        setFormMode({
          kind: 'form',
          formId: draft.formId,
          values: draft.formFields ?? {},
        });
      }
    }
  }, [draftId]);

  // Fetch config to populate callsign + grid for send_form (T6.1)
  useEffect(() => {
    invoke<{ callsign?: string; grid?: string }>('config_read')
      .then((cfg) => {
        setCallsign(cfg.callsign ?? '');
        setGrid(cfg.grid ?? '');
      })
      .catch(() => {
        // pre-wizard launch — leave empty, send_form will still build XML
        // with empty senders_callsign/grid_square; operator-pending verification
      });
  }, []);

  // ============================================================================
  // Autosave every 2s (spec §5.4)
  // ============================================================================

  useEffect(() => {
    const interval = setInterval(() => {
      // Do NOT autosave after a successful send — the draft was intentionally
      // cleared and the interval must not recreate it (Codex P1 fix).
      if (!sentRef.current) {
        saveDraft({
          draftId, to, cc, subject, body, requestAck,
          formId: formMode.kind === 'form' ? formMode.formId : undefined,
          formFields: formMode.kind === 'form' ? formMode.values : undefined,
        });
      }
    }, 2000);
    return () => clearInterval(interval);
  }, [draftId, to, cc, subject, body, requestAck, formMode]);

  // ============================================================================
  // Keyboard shortcuts
  // ============================================================================

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        handleSaveDraft();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        handleSend();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [to, cc, subject, body, requestAck, draftId]);

  // ============================================================================
  // Save draft
  // ============================================================================

  const handleSaveDraft = useCallback(() => {
    saveDraft({
      draftId, to, cc, subject, body, requestAck,
      formId: formMode.kind === 'form' ? formMode.formId : undefined,
      formFields: formMode.kind === 'form' ? formMode.values : undefined,
    });
    savedSnapshotRef.current = { to, cc, subject, body, requestAck };
    setSendState('idle');
  }, [draftId, to, cc, subject, body, requestAck, formMode]);

  // ============================================================================
  // Send
  // ============================================================================

  const handleSend = useCallback(async () => {
    if (sendState === 'sending') return;
    // P1 #2 fix: in form mode, the global send is invalid — the form has its
    // own Send button (which routes to handleFormSubmit + send_form IPC).
    if (formMode.kind !== 'plain') return;
    setSendState('sending');
    setErrorMsg(null);

    const dto: OutboundDraftDto = {
      to: splitAddrs(to),
      cc: splitAddrs(cc),
      subject,
      body,
      // P2.1 bridge: attachment-picker not yet built (HTML Forms PR #151); pass []
      // to preserve current behavior while the IPC bridge is wired up.
      attachments: [],
    };

    try {
      // Returns String (MID). NativeBackend returns a real MID; PatBackend
      // (deleted in P9) returns an empty string as a transitional placeholder.
      // Treat any Ok(_) uniformly as success (spec §3.2 / §5.4); do not
      // display the MID directly — it may be empty.
      await invoke<string>('message_send', { draft: dto });
      // Gate autosave BEFORE clearing the draft so the interval cannot win a
      // race between the flag set and the next 2s tick (Codex P1 fix).
      sentRef.current = true;
      setSendState('success');
      clearDraft(draftId);
      savedSnapshotRef.current = { to: '', cc: '', subject: '', body: '', requestAck: false };
    } catch (err: unknown) {
      setSendState('error');
      if (err && typeof err === 'object' && 'detail' in err) {
        const detail = (err as { detail: unknown }).detail;
        setErrorMsg(typeof detail === 'string' ? detail : JSON.stringify(detail));
      } else {
        setErrorMsg(String(err));
      }
    }
  }, [sendState, to, cc, subject, body, draftId, formMode.kind]);

  // ============================================================================
  // Form submit (T6.1)
  // ============================================================================

  const handleFormSubmit = useCallback(async (formId: string, values: Record<string, string>) => {
    if (sendState === 'sending') return;
    setSendState('sending');
    setErrorMsg(null);
    try {
      await invoke<string>('send_form', {
        formId,
        fieldValues: values,
        to: splitAddrs(to),
        cc: splitAddrs(cc),
        sendersCallsign: callsign,
        gridSquare: grid,
      });
      sentRef.current = true;
      setSendState('success');
      clearDraft(draftId);
      savedSnapshotRef.current = { to: '', cc: '', subject: '', body: '', requestAck: false };
    } catch (err: unknown) {
      setSendState('error');
      if (err && typeof err === 'object' && 'detail' in err) {
        const detail = (err as { detail: unknown }).detail;
        setErrorMsg(typeof detail === 'string' ? detail : JSON.stringify(detail));
      } else {
        setErrorMsg(String(err));
      }
    }
  }, [sendState, to, cc, draftId, callsign, grid]);

  // ============================================================================
  // Form picker (T6.1)
  // ============================================================================

  const handleOpenFormPicker = useCallback(() => {
    // T6.2: if body has unsaved content, prompt first
    if (body.trim().length > 0 || subject.trim().length > 0) {
      setClosePrompt({ open: true, action: 'switch-to-form' });
      return;
    }
    setFormMode({ kind: 'pick' });
  }, [body, subject]);

  // ============================================================================
  // Close handling (spec §5.4: unsaved changes → prompt)
  // ============================================================================

  // Wire the native window-close event (titlebar X / Alt-F4) so it goes
  // through the same unsaved-changes path as the in-app close button.
  // Without this, native close would bypass the prompt and silently discard
  // edits newer than the last autosave (Codex P1 fix — native close path).
  //
  // Strategy: intercept the close request, prevent it, then either show the
  // prompt (dirty) or perform a clean programmatic close (not dirty). The
  // success state (sentRef.current) always passes as clean — the send already
  // cleared the draft, so no prompt is needed.
  useEffect(() => {
    // Late-resolution guard (mirrors App.tsx's menu listener + SessionLog's
    // listener): the dynamic import + onCloseRequested() registration is async,
    // so a fast unmount can run cleanup BEFORE the listener handle resolves.
    // Without the `mounted` flag we would only `unlisten` an already-assigned
    // handle and leak the listener registered after cleanup (Codex integration
    // round P3). The flag causes the late `.then()` to immediately release it.
    let mounted = true;
    let unlisten: (() => void) | undefined;
    import('@tauri-apps/api/window').then(({ getCurrentWindow }) => {
      getCurrentWindow()
        .onCloseRequested((event) => {
          // tuxlink-h2y: route EVERY close through compose_close_self (a
          // self-only Rust destroy), so compose.json needs no window-class JS
          // grants. ALWAYS block the native close, then either close via the
          // command (clean / already-sent) or show the unsaved-changes prompt.
          event.preventDefault();
          if (sentRef.current || !isDirty()) {
            invoke('compose_close_self').catch(() => {/* ignore */});
            return;
          }
          // There are unsaved changes: show the in-app Save/Discard/Cancel dialog.
          setClosePrompt({ open: true, action: 'close' });
        })
        .then((fn) => {
          if (mounted) {
            unlisten = fn;
          } else {
            // Cleanup already ran before the listener resolved — release it
            // immediately so it does not leak / fire on a dead component.
            fn();
          }
        });
    });
    return () => {
      mounted = false;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isDirty]);

  const handleRequestClose = useCallback(() => {
    if (isDirty()) {
      setClosePrompt({ open: true, action: 'close' });
    } else {
      // No unsaved changes — close via Tauri window API
      closeWindow();
    }
  }, [isDirty]);

  const closeWindow = () => {
    // tuxlink-h2y: close via the self-only Rust command (compose_close_self
    // destroys ONLY the calling window) instead of the window-class JS
    // window.close(), so compose.json can drop allow-close/allow-destroy.
    invoke('compose_close_self').catch(() => {/* ignore */});
  };

  const handleSaveAndProceed = useCallback(() => {
    saveDraft({
      draftId, to, cc, subject, body, requestAck,
      formId: formMode.kind === 'form' ? formMode.formId : undefined,
      formFields: formMode.kind === 'form' ? formMode.values : undefined,
    });
    const action = closePrompt.action;
    setClosePrompt({ open: false, action: null });
    if (action === 'switch-to-form') {
      setFormMode({ kind: 'pick' });
    } else {
      closeWindow();
    }
  }, [draftId, to, cc, subject, body, requestAck, closePrompt.action, formMode]);

  const handleDiscardAndProceed = useCallback(() => {
    // Clear body content
    setTo('');
    setCc('');
    setSubject('');
    setBody('');
    setRequestAck(false);
    const action = closePrompt.action;
    setClosePrompt({ open: false, action: null });
    if (action === 'switch-to-form') {
      setFormMode({ kind: 'pick' });
    } else {
      clearDraft(draftId);
      closeWindow();
    }
  }, [draftId, closePrompt.action]);

  const handleCancelClose = useCallback(() => {
    setClosePrompt({ open: false, action: null });
  }, []);

  // ============================================================================
  // Drag-and-drop attachment stub (v0.0.1 — presence only)
  // ============================================================================

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    // v0.0.1 DONE_WITH_CONCERNS: Pat multipart attachment API wiring is
    // deferred. The drop zone accepts files and lists their names, but the
    // send path does NOT include them (they are silently omitted with a
    // visible warning in the UI rather than silently dropped without notice).
    const names = Array.from(e.dataTransfer.files).map((f) => f.name);
    if (names.length > 0) {
      // We intentionally do not call _setAttachments here — this is the stub
      // that shows we accepted the event without wiring send. A real v0.1
      // implementation populates attachments state and sends multipart form
      // data.
      console.warn('Attachment UI stub: attach-send is not wired in v0.0.1', names);
    }
  };

  // ============================================================================
  // Render
  // ============================================================================

  if (sendState === 'success') {
    return (
      <div className="compose-success" data-testid="compose-success">
        <p className="compose-success__msg">Posted to Outbox</p>
        <p className="compose-success__sub">
          Your message has been queued. It will be sent on the next CMS connection.
        </p>
        <button
          className="compose-btn compose-btn--primary"
          onClick={closeWindow}
        >
          Close
        </button>
      </div>
    );
  }

  return (
    <div className="compose-root" data-testid="compose-root">
      {/* Borderless-window resize affordances (decorations:false leaves no
          native grips on labwc / Wayland). Mirrors AppShell. */}
      <ResizeHandles />

      {/* ------------------------------------------------------------------ */}
      {/* Custom title bar (tuxlink-ng3: decorations:false, closes msr)      */}
      {/* ------------------------------------------------------------------ */}
      <ComposeTitleBar onClose={handleRequestClose} />

      {/* ------------------------------------------------------------------ */}
      {/* Fields (the duplicate in-form header was removed — ComposeTitleBar  */}
      {/* is the single title bar + close, tuxlink-ng3 smoke #4)              */}
      {/* ------------------------------------------------------------------ */}
      <div className="compose-fields">

        {/* From — disabled (single callsign, v0.1 multi-callsign) */}
        <div className="compose-field-row">
          <label htmlFor="compose-from" className="compose-label">From</label>
          <input
            id="compose-from"
            className="compose-input compose-input--disabled"
            type="text"
            value=""
            readOnly
            disabled
            aria-describedby="compose-from-hint"
            title="Callsign selection arrives in v0.1"
          />
          <span id="compose-from-hint" className="compose-hint">
            v0.1 — multi-callsign support
          </span>
        </div>

        {/* Send as — disabled (v0.1) */}
        <div className="compose-field-row">
          <label htmlFor="compose-send-as" className="compose-label">Send as</label>
          <input
            id="compose-send-as"
            className="compose-input compose-input--disabled"
            type="text"
            value="Winlink Message"
            readOnly
            disabled
            title="Message type selection arrives in v0.1"
          />
        </div>

        {/* To */}
        <div className="compose-field-row">
          <label htmlFor="compose-to" className="compose-label">
            To <span className="compose-label__req" aria-hidden="true">*</span>
          </label>
          <input
            id="compose-to"
            className="compose-input"
            type="text"
            value={to}
            onChange={(e) => setTo(e.target.value)}
            placeholder="W6ABC@winlink.org; W7DEF@winlink.org"
            aria-label="Recipients (semicolon-separated callsigns)"
            data-testid="compose-to"
          />
        </div>

        {/* Cc — enabled end-to-end per tuxlink-h1km. */}
        <div className="compose-field-row">
          <label htmlFor="compose-cc" className="compose-label">Cc</label>
          <input
            id="compose-cc"
            className="compose-input"
            type="text"
            value={cc}
            onChange={(e) => setCc(e.target.value)}
            placeholder="W6ABC@winlink.org; W7DEF@winlink.org"
            aria-label="Cc recipients (semicolon-separated callsigns)"
            data-testid="compose-cc"
          />
        </div>

        {/* Subject */}
        <div className="compose-field-row">
          <label htmlFor="compose-subject" className="compose-label">Subject</label>
          <input
            id="compose-subject"
            className="compose-input"
            type="text"
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
            placeholder="Message subject"
            data-testid="compose-subject"
          />
        </div>

        {/* Select Template — disabled */}
        <div className="compose-field-row">
          <label htmlFor="compose-template" className="compose-label compose-label--muted">
            Template
          </label>
          <button
            id="compose-template"
            className="compose-btn compose-btn--ghost compose-btn--disabled"
            disabled
            title="Template selection arrives in v0.1"
          >
            Select Template…
          </button>
        </div>

      </div>

      {/* ------------------------------------------------------------------ */}
      {/* Body                                                                */}
      {/* ------------------------------------------------------------------ */}
      <div className="compose-body-area">
        <label htmlFor="compose-body" className="compose-label compose-label--sr-only">
          Message body
        </label>
        {formMode.kind === 'plain' && (
          <textarea
            id="compose-body"
            className="compose-textarea"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="Type your message here…"
            data-testid="compose-body"
          />
        )}
        {formMode.kind === 'pick' && (
          <FormPicker
            forms={allForms().map((f) => ({ id: f.id, name: f.name }))}
            onPick={(id) => setFormMode({ kind: 'form', formId: id, values: {} })}
            onCancel={() => setFormMode({ kind: 'plain' })}
          />
        )}
        {formMode.kind === 'form' && (() => {
          const entry = lookupForm(formMode.formId);
          if (!entry) {
            // Unknown form ID (shouldn't happen since picker shows registered only)
            setFormMode({ kind: 'plain' });
            return null;
          }
          const FormComponent = entry.Form;
          return (
            <FormComponent
              initialValues={formMode.values}
              onChange={(values) => setFormMode({ kind: 'form', formId: formMode.formId, values })}
              onSubmit={(values) => handleFormSubmit(formMode.formId, values)}
              onCancel={() => setFormMode({ kind: 'plain' })}
            />
          );
        })()}
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* Attachments (v0.0.1 stub — drop zone only, send not wired)         */}
      {/* ------------------------------------------------------------------ */}
      <div
        className="compose-attachments"
        onDragOver={handleDragOver}
        onDrop={handleDrop}
        data-testid="compose-attachments-zone"
      >
        {attachments.length === 0 ? (
          <span className="compose-attachments__hint">
            Drop files here to attach (v0.0.1: attachment send not wired)
          </span>
        ) : (
          <ul className="compose-attachments__list">
            {attachments.map((name, i) => (
              <li key={i} className="compose-attachments__item">{name}</li>
            ))}
          </ul>
        )}
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* Request-ack checkbox                                                */}
      {/* ------------------------------------------------------------------ */}
      <div className="compose-options">
        <label className="compose-checkbox-label">
          <input
            type="checkbox"
            checked={requestAck}
            onChange={(e) => setRequestAck(e.target.checked)}
            data-testid="compose-request-ack"
          />
          Request read receipt
        </label>
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* Error banner                                                        */}
      {/* ------------------------------------------------------------------ */}
      {sendState === 'error' && errorMsg && (
        <div className="compose-error" role="alert" data-testid="compose-error">
          <strong>Send failed:</strong> {errorMsg}
        </div>
      )}

      {/* ------------------------------------------------------------------ */}
      {/* Action bar                                                          */}
      {/* ------------------------------------------------------------------ */}
      <div className="compose-actions">
        <button
          className="compose-btn compose-btn--primary"
          onClick={handleSend}
          disabled={sendState === 'sending' || formMode.kind !== 'plain'}
          title={formMode.kind !== 'plain'
            ? "Use the form's Send button to submit a form"
            : 'Send (Ctrl+Enter)'}
          data-testid="compose-send-btn"
        >
          {sendState === 'sending' ? 'Sending…' : 'Post to Outbox'}
        </button>
        <button
          className="compose-btn compose-btn--secondary"
          onClick={handleSaveDraft}
          title="Save draft (Ctrl+S)"
          data-testid="compose-save-draft-btn"
        >
          Save Draft
        </button>
        <button
          className="compose-btn compose-btn--secondary"
          onClick={handleOpenFormPicker}
          disabled={formMode.kind !== 'plain'}
          data-testid="compose-form-picker-btn"
        >
          Compose form…
        </button>
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* Unsaved-changes close prompt (spec §5.4)                           */}
      {/* ------------------------------------------------------------------ */}
      {closePrompt.open && (
        <div
          className="compose-overlay"
          role="dialog"
          aria-modal="true"
          aria-label="Unsaved changes"
          data-testid="compose-close-prompt"
        >
          <div className="compose-dialog">
            <p className="compose-dialog__msg">
              {closePrompt.action === 'switch-to-form'
                ? 'Save changes before switching to a form?'
                : 'This draft has unsaved changes.'}
            </p>
            <div className="compose-dialog__actions">
              <button
                className="compose-btn compose-btn--primary"
                onClick={handleSaveAndProceed}
                data-testid="compose-close-save"
              >
                Save Draft
              </button>
              <button
                className="compose-btn compose-btn--danger"
                onClick={handleDiscardAndProceed}
                data-testid="compose-close-discard"
              >
                Discard
              </button>
              <button
                className="compose-btn compose-btn--ghost"
                onClick={handleCancelClose}
                data-testid="compose-close-cancel"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
