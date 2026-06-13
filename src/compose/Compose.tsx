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
import { clearDraft, expandGroupsAndDedup, findUnknownGroupTokens, loadDraft, saveDraft, splitAddrs, type DraftData } from './useDraft';
import { ComposeTitleBar } from './ComposeTitleBar';
import { ResizeHandles } from '../shell/chrome/ResizeHandles';
import { formatCallsign } from '../shell/useStatus';
import { lookupForm } from '../forms';
import { CatalogBrowser } from './CatalogBrowser';
import { WebviewFormHost, type ParsedBody } from './WebviewFormHost';
import { RecipientInput, type RecipientInputHandle } from '../contacts/RecipientInput';
import type { ContactsFile } from '../contacts/types';
import { useContacts } from '../contacts/useContacts';
import { useAttachments, type ImageOpts } from './useAttachments';
import { humanSize, airtimeEstimate, cmsStatus } from './attachmentFormat';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';
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
  | { kind: 'form'; formId: string; values: Record<string, string> }
  // P1 Task 10: webview-form mode is the entry for any catalog form whose
  // id has no native React Form in the registry. The WebviewFormHost owns
  // the in-flight form state inside the embedded webview, so this branch
  // carries no `values` — the form submits via the loopback POST and
  // round-trips a ParsedBody back through `handleWebviewSubmit`.
  | { kind: 'webview-form'; formId: string }
  // tuxlink-hhfx / G10: a SendReply reply session. Like webview-form (the
  // SendReply HTML renders + submits inside the embedded webview), but the
  // backend pre-binds the original form's `values` (+ `msgOriginalBody`) into
  // the page so the operator sees the message they're replying to. `formId` is
  // the ORIGINAL form id; the backend resolves the SendReply from it and hands
  // back the reply_template the submit threads to `send_webview_form`.
  | {
      kind: 'webview-reply';
      formId: string;
      values: Record<string, string>;
      msgOriginalBody: string;
    };

type CloseAction = 'close' | 'switch-to-form' | null;

/**
 * Collapse a ParsedBody (multi-value HTML form fields keyed by name)
 * into the single-string-per-field shape that `send_webview_form`
 * expects. The synthetic `Submit` button name is dropped — WLE
 * templates POST the submit button's value back and we don't want it
 * appearing as a "submit" field in the synthesized XML envelope.
 *
 * Multi-value collapse: single values → bare string; >1 value →
 * newline-joined. Per design §5.3 this matches WLE's expectation for
 * checkbox / multi-select groups.
 *
 * Exported for direct unit-testing of the conversion logic without
 * having to mount the full Compose component (handleWebviewSubmit is
 * a useCallback in Compose's component scope).
 */
export function parsedBodyToFieldValues(payload: ParsedBody): Record<string, string> {
  const fieldValues: Record<string, string> = {};
  for (const [k, vs] of Object.entries(payload.fields)) {
    if (k === 'Submit') continue;
    fieldValues[k] = vs.length === 1 ? vs[0] : vs.join('\n');
  }
  return fieldValues;
}

/**
 * Decide what the unsaved-changes close prompt should offer for a given
 * Compose form mode. Returns the dialog shape — primary message + which
 * action buttons appear — so the rendering branch is testable without
 * mounting the full Compose component (which requires a Tauri runtime).
 *
 * P1.1 (2026-06-04 Codex adrev): in `webview-form` mode the form
 * contents live inside the embedded child webview and Compose has no
 * IPC introspection into them. Offering "Save Draft" would persist only
 * the formId metadata while silently losing every field value the
 * operator typed. The dialog drops the Save button in that mode and
 * surfaces a sub-explainer that tells the operator how to recover
 * (Cancel back to the form → press its Send button).
 */
export type ClosePromptShape = {
  primary: string;
  sub?: string;
  buttons: readonly ('save' | 'discard' | 'cancel')[];
};
export function closePromptShape(
  formModeKind: 'plain' | 'pick' | 'form' | 'webview-form' | 'webview-reply',
  action: 'close' | 'switch-to-form' | null,
): ClosePromptShape {
  if (formModeKind === 'webview-form' || formModeKind === 'webview-reply') {
    return {
      primary: "Form contents can't be saved as a draft. Submit it now, or discard.",
      sub:
        "The form's field values live inside the embedded form window, " +
        "where Compose can't reach them. Cancel to return to the form and " +
        'press its Send button — otherwise the field contents are lost.',
      buttons: ['discard', 'cancel'] as const,
    };
  }
  return {
    primary:
      action === 'switch-to-form'
        ? 'Save changes before switching to a form?'
        : 'This draft has unsaved changes.',
    buttons: ['save', 'discard', 'cancel'] as const,
  };
}

/**
 * Decide whether the manual "Save Draft" affordance (toolbar button +
 * Ctrl+S keyboard shortcut) is available for a given form mode.
 *
 * P1.1 (2026-06-04 Codex adrev): false in `webview-form` mode because
 * Save Draft would only persist formId metadata while silently dropping
 * the operator's typed field values. Autosave still runs in webview-form
 * mode but only persists the formId so a restored draft picks up the
 * same picker mode.
 */
export function isSaveDraftAvailable(
  formModeKind: 'plain' | 'pick' | 'form' | 'webview-form' | 'webview-reply',
): boolean {
  // webview-reply, like webview-form, keeps its in-flight field values inside
  // the embedded webview where Compose can't reach them — manual Save Draft
  // would silently drop the operator's typed reply (tuxlink-hhfx / G10).
  return formModeKind !== 'webview-form' && formModeKind !== 'webview-reply';
}

/// The form-related draft fields to persist for a given form mode (tuxlink-hhfx
/// / G10 made this shared so the three persistence sites — autosave, manual
/// save, close handler — stay in lock-step). Native `form` persists its live
/// values; `webview-form` persists only the formId (its values live in the
/// webview); `webview-reply` persists the original values + the reply markers
/// so a restored draft re-opens the SendReply page pre-bound.
export function persistedFormDraft(
  formMode: FormMode,
): Pick<DraftData, 'formId' | 'formFields' | 'formReply' | 'msgOriginalBody'> {
  switch (formMode.kind) {
    case 'form':
      return { formId: formMode.formId, formFields: formMode.values };
    case 'webview-form':
      return { formId: formMode.formId };
    case 'webview-reply':
      return {
        formId: formMode.formId,
        formFields: formMode.values,
        formReply: true,
        msgOriginalBody: formMode.msgOriginalBody,
      };
    default:
      return {};
  }
}

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

  // Contacts/groups for the To/Cc autocomplete + send-time group expansion.
  // Compose is a SEPARATE Tauri window, so this is its own useContacts
  // instance; the A4 `contacts:changed` listener keeps it fresh when the main
  // window edits a contact/group (H9 — so an in-flight draft expands the
  // UPDATED membership at send time, not a stale snapshot).
  const { contacts, groups } = useContacts();

  // Send + close state
  const [sendState, setSendState] = useState<SendState>('idle');
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [closePrompt, setClosePrompt] = useState<ClosePromptState>({
    open: false,
    action: null,
  });

  // Attachment stub (multipart UI wiring deferred — see drop-zone comment below)
  const attach = useAttachments();

  // Track the "clean" snapshot so we can detect unsaved changes on close
  const savedSnapshotRef = useRef({ to: '', cc: '', subject: '', body: '', requestAck: false });
  // Set to true after a successful send — gates the autosave interval so it
  // cannot recreate the draft that was intentionally cleared (Codex P1).
  const sentRef = useRef(false);
  // Issue #648 — handles to flush each RecipientInput's in-progress (un-Entered)
  // buffer at send time, so a recipient typed without pressing Enter is committed
  // before buildRecipients reads it. Every send path goes through buildRecipients.
  const toRef = useRef<RecipientInputHandle>(null);
  const ccRef = useRef<RecipientInputHandle>(null);
  // Track if the user has interacted (only prompt on genuine changes)
  const isDirty = useCallback(() => {
    const s = savedSnapshotRef.current;
    // Form mode is "dirty" iff there are any non-empty field values
    if (formMode.kind === 'form') {
      return Object.values(formMode.values).some((v) => v.trim().length > 0);
    }
    // Webview-form mode: the form state lives inside the embedded child
    // webview (we have no introspection into its inputs across the IPC
    // boundary). Conservatively treat it as dirty whenever the form is
    // open — the operator has potentially significant work in flight,
    // and the close-gate ("really close?") is the right behavior for a
    // false-positive dirty signal (the alternative — silently closing
    // a form with unsaved field data — is the failure we are guarding
    // against). Important #4 from the P1 Task 10 code review.
    // webview-reply (tuxlink-hhfx / G10) shares webview-form's blind-spot: the
    // operator's reply lives inside the embedded SendReply webview, so treat an
    // open reply session as dirty too.
    if (formMode.kind === 'webview-form' || formMode.kind === 'webview-reply') {
      return true;
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
        if (draft.formReply) {
          // tuxlink-hhfx / G10: a SendReply reply draft re-opens the original
          // form's SendReply page pre-bound with the saved original values.
          setFormMode({
            kind: 'webview-reply',
            formId: draft.formId,
            values: draft.formFields ?? {},
            msgOriginalBody: draft.msgOriginalBody ?? '',
          });
        } else {
          // Restore to whichever form mode matches: native form (with values)
          // if the React registry has a Form for this id; webview-form
          // otherwise. This mirrors CatalogBrowser's pick routing so a
          // restored draft picks up the same UI path. Important #3 from
          // the P1 Task 10 code review: previously, webview-form drafts saved
          // with formId: undefined and silently restored as plain-text.
          const entry = lookupForm(draft.formId);
          if (entry?.Form) {
            setFormMode({
              kind: 'form',
              formId: draft.formId,
              values: draft.formFields ?? {},
            });
          } else {
            setFormMode({ kind: 'webview-form', formId: draft.formId });
          }
        }
      }
    }
  }, [draftId]);

  // Fetch config to populate callsign + grid for send_form (T6.1)
  useEffect(() => {
    invoke<{
      connect_to_cms?: boolean;
      callsign?: string | null;
      identifier?: string | null;
      grid?: string;
    }>('config_read')
      .then((cfg) => {
        // Resolve the From identity the same way the ribbon does (spec §5.6):
        // callsign for CMS installs, falling back to identifier for offline-path
        // operators who have no callsign. Reading cfg.callsign alone left the
        // field blank for the offline audience (smoke-walk item 39 gap).
        setCallsign(
          formatCallsign({
            connect_to_cms: cfg.connect_to_cms ?? false,
            callsign: cfg.callsign ?? null,
            identifier: cfg.identifier ?? null,
          }),
        );
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
      if (sentRef.current) return;

      // Persist formId in BOTH native form and webview-form modes so a
      // restored draft picks up the same picker mode (Important #3 from
      // the P1 Task 10 code review: previously, webview-form drafts saved
      // with formId: undefined and silently restored as plain-text).
      // formFields is only populated in native form mode — the webview's
      // in-flight state lives in the embedded webview, not in Compose's
      // React state, so we cannot snapshot it from this side.
      const pf = persistedFormDraft(formMode);

      // tuxlink-n3hw: re-stamp savedAt ONLY on a genuine edit. savedAt drives
      // the Drafts-list sort (draftMailbox.draftToMessageMeta → `date`), so an
      // unconditional autosave bumped a draft to the top of the list merely by
      // being opened for reading. Compare the editable content against what is
      // already on disk and skip the save when nothing changed — leaving
      // savedAt (and thus list order) untouched. localStorage is the source of
      // truth, so there is no separate "clean snapshot" ref to keep in sync
      // across the manual-save / send paths.
      const existing = loadDraft(draftId);
      if (
        existing &&
        existing.to === to &&
        (existing.cc ?? '') === cc &&
        existing.subject === subject &&
        existing.body === body &&
        existing.requestAck === requestAck &&
        existing.formId === pf.formId &&
        JSON.stringify(existing.formFields ?? null) === JSON.stringify(pf.formFields ?? null) &&
        (existing.formReply ?? false) === (pf.formReply ?? false) &&
        (existing.msgOriginalBody ?? undefined) === pf.msgOriginalBody
      ) {
        return;
      }

      saveDraft({
        draftId, to, cc, subject, body, requestAck,
        ...pf,
      });
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
        // P1.1 (2026-06-04 Codex adrev): Save Draft in webview-form mode
        // can't capture the form's in-flight contents (they live inside
        // the embedded webview). No-op the Ctrl+S so we don't pretend to
        // save something we can't. Autosave already persists the formId
        // for mode restoration.
        if (!isSaveDraftAvailable(formMode.kind)) return;
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
  }, [to, cc, subject, body, requestAck, draftId, formMode.kind]);

  // ============================================================================
  // Save draft
  // ============================================================================

  const handleSaveDraft = useCallback(() => {
    // Persist the form-mode draft fields (native form values, or webview
    // formId / reply context). Shared with autosave + close (tuxlink-hhfx).
    saveDraft({
      draftId, to, cc, subject, body, requestAck,
      ...persistedFormDraft(formMode),
    });
    savedSnapshotRef.current = { to, cc, subject, body, requestAck };
    setSendState('idle');
  }, [draftId, to, cc, subject, body, requestAck, formMode]);

  // ============================================================================
  // Recipient build — the SINGLE send-time expansion point (Task A6)
  // ============================================================================
  //
  // Expand `group:<id>` sentinels to member callsigns and wire-key-dedup, for
  // BOTH To and Cc. Cc is seeded against the EXPANDED To so a recipient in both
  // is not double-sent (Codex#6). Expansion happens ONLY here, at send — the
  // `to`/`cc` state stays the raw semicolon string with sentinels for autosave.
  //
  // Factored as one helper so all THREE send paths (message_send / send_form /
  // send_webview_form) produce IDENTICAL recipient lists — no path can drift.
  //
  // C2-P1 / Codex#5: fetch FRESH contacts at send so a separate Compose window
  // cannot expand a STALE group after a main-window edit (the cached useContacts
  // value can lag an in-flight contacts:changed refetch). Falls back to the
  // cached hook values if the fresh read fails (offline / no backend) so send
  // still works without a Tauri runtime.
  const buildRecipients = useCallback(async (): Promise<{ to: string[]; cc: string[]; unknownGroups: string[] }> => {
    // Issue #648 — flush any un-Entered text in the To/Cc inputs FIRST so a
    // recipient typed without pressing Enter is committed before we read it.
    // flush() returns the up-to-date string synchronously; the `to`/`cc` state
    // set by its onChange would not yet be visible to this closure. Falls back
    // to the committed state when a ref is unmounted (e.g. non-plain form modes).
    const toStr = toRef.current?.flush() ?? to;
    const ccStr = ccRef.current?.flush() ?? cc;
    let freshContacts = contacts;
    let freshGroups = groups;
    try {
      const file = await invoke<ContactsFile>('contacts_read');
      if (file) {
        freshContacts = file.contacts ?? [];
        freshGroups = file.groups ?? [];
      }
    } catch {
      // keep the cached hook value
    }
    const rawTo = splitAddrs(toStr);
    const rawCc = splitAddrs(ccStr);
    const unknownGroups = findUnknownGroupTokens([...rawTo, ...rawCc], freshGroups);
    const expandedTo = expandGroupsAndDedup(rawTo, freshContacts, freshGroups);
    const expandedCc = expandGroupsAndDedup(rawCc, freshContacts, freshGroups, expandedTo);
    return { to: expandedTo, cc: expandedCc, unknownGroups };
  }, [to, cc, contacts, groups]);

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

    // Expand groups + wire-key-dedup at send (Task A6). No `group:<id>` token
    // reaches the wire (H5); To/Cc dedup with Cc seeded from To (Codex#6).
    // C2-P1: await the async fresh-fetch so stale-cache group expansion is
    // impossible even when a contacts:changed refetch is in flight.
    const { to: toAddrs, cc: ccAddrs, unknownGroups } = await buildRecipients();
    if (unknownGroups.length > 0) {
      setSendState('error');
      setErrorMsg('A distribution group in your recipients no longer exists. Remove the group and re-add its members before sending.');
      return;
    }
    const dto: OutboundDraftDto = {
      to: toAddrs,
      cc: ccAddrs,
      subject,
      body,
      // tuxlink-mg4s: real attachments from the picker/drop zone (images
      // resized/transcoded at attach time). The backend message_send maps
      // these into the outbound B2F message.
      attachments: attach.toDto(),
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
  }, [sendState, buildRecipients, subject, body, draftId, formMode.kind, attach.toDto]);

  // ============================================================================
  // Form submit (T6.1)
  // ============================================================================

  const handleFormSubmit = useCallback(async (formId: string, values: Record<string, string>) => {
    if (sendState === 'sending') return;
    setSendState('sending');
    setErrorMsg(null);
    // Expand groups + wire-key-dedup at send (Task A6) — same helper as
    // message_send, so the form path produces an IDENTICAL recipient list.
    // C2-P1: await the async fresh-fetch (same rationale as handleSend).
    const { to: toAddrs, cc: ccAddrs, unknownGroups } = await buildRecipients();
    if (unknownGroups.length > 0) {
      setSendState('error');
      setErrorMsg('A distribution group in your recipients no longer exists. Remove the group and re-add its members before sending.');
      return;
    }
    try {
      await invoke<string>('send_form', {
        formId,
        fieldValues: values,
        to: toAddrs,
        cc: ccAddrs,
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
  }, [sendState, buildRecipients, draftId, callsign, grid]);

  // ============================================================================
  // Webview-form submit (T10)
  // ============================================================================
  //
  // The embedded WLE form POSTs back through the loopback http_server, which
  // round-trips a ParsedBody (multi-value string fields keyed by HTML name)
  // through the `form-submitted` event. We collapse the multi-value shape
  // into the single-string-per-field `fieldValues` that send_webview_form
  // expects, then mirror handleFormSubmit's post-send cleanup so the success
  // banner + draft clear behave identically across native and webview entries.
  //
  // Routes to `send_webview_form` (NOT `send_form`) because send_form only
  // knows the 5 native BUNDLED_FORMS templates; ~245 catalog forms need the
  // webview-aware command that synthesizes the XML envelope from
  // field_values + WLE filename conventions. Critical #1 from the P1 Task 10
  // code review — without this, the entire P1 catalog-picker path fails at
  // submit time with "unknown form: <id>".

  const handleWebviewSubmit = useCallback(async (formId: string, payload: ParsedBody) => {
    if (sendState === 'sending') return;
    setSendState('sending');
    setErrorMsg(null);
    // Convert ParsedBody (multi-value fields) → fieldValues (single string
    // per name). The exported helper at module scope is unit-tested
    // independently — see Compose.test.tsx.
    const fieldValues = parsedBodyToFieldValues(payload);
    // Expand groups + wire-key-dedup at send (Task A6) — same helper as the
    // other two send paths, so the webview-form path is IDENTICAL.
    // C2-P1: await the async fresh-fetch (same rationale as handleSend).
    const { to: toAddrs, cc: ccAddrs, unknownGroups } = await buildRecipients();
    if (unknownGroups.length > 0) {
      setSendState('error');
      setErrorMsg('A distribution group in your recipients no longer exists. Remove the group and re-add its members before sending.');
      return;
    }
    try {
      await invoke<string>('send_webview_form', {
        formId,
        fieldValues,
        to: toAddrs,
        cc: ccAddrs,
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
  }, [sendState, buildRecipients, draftId, callsign, grid]);

  // ============================================================================
  // Webview-reply submit (tuxlink-hhfx / G10)
  // ============================================================================
  //
  // Identical to handleWebviewSubmit (collapse ParsedBody → fieldValues, expand
  // groups, send via send_webview_form), with two reply-specific extras:
  //   - `replyTemplate` (the SendReply `.0`, handed back by open_webview_reply)
  //     so the backend renders To:/Subject:/Msg: from the SendReply, not the
  //     original form's .txt.
  //   - `subjectHint` (the compose "Re: <original>" subject) so the reply gets a
  //     meaningful subject — SendReply `.0`s carry no Subject: directive.
  const handleWebviewReplySubmit = useCallback(
    async (formId: string, payload: ParsedBody, replyTemplate: string | undefined) => {
      if (sendState === 'sending') return;
      setSendState('sending');
      setErrorMsg(null);
      const fieldValues = parsedBodyToFieldValues(payload);
      const { to: toAddrs, cc: ccAddrs, unknownGroups } = await buildRecipients();
      if (unknownGroups.length > 0) {
        setSendState('error');
        setErrorMsg('A distribution group in your recipients no longer exists. Remove the group and re-add its members before sending.');
        return;
      }
      try {
        await invoke<string>('send_webview_form', {
          formId,
          fieldValues,
          to: toAddrs,
          cc: ccAddrs,
          sendersCallsign: callsign,
          gridSquare: grid,
          replyTemplate,
          subjectHint: subject,
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
    },
    [sendState, buildRecipients, draftId, callsign, grid, subject],
  );

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
    // Persist the form-mode draft fields (shared with autosave + manual save).
    saveDraft({
      draftId, to, cc, subject, body, requestAck,
      ...persistedFormDraft(formMode),
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
  // Drag-and-drop attachment stub (UI presence only — wire-up deferred)
  // ============================================================================

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    // Tauri's webview drag-drop exposes OS paths via the dropped File objects'
    // `path` on desktop; route each through prepare_attachment (images are
    // resized/transcoded, other files passed through). If a build delivers no
    // path here, the "Attach files…" picker (handlePickFiles) is the reliable
    // entry point.
    const paths = Array.from(e.dataTransfer.files)
      .map((f) => (f as File & { path?: string }).path)
      .filter((p): p is string => !!p);
    for (const p of paths) {
      await attach.addPath(p, { resize: 'medium', format: 'jpeg' });
    }
  };

  const handlePickFiles = async () => {
    const selected = await openFileDialog({ multiple: true });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    for (const p of paths) {
      await attach.addPath(p, { resize: 'medium', format: 'jpeg' });
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

        {/* From — read-only configured callsign; multi-callsign selection deferred */}
        <div className="compose-field-row">
          <label htmlFor="compose-from" className="compose-label">From</label>
          <input
            id="compose-from"
            className="compose-input compose-input--disabled"
            type="text"
            value={callsign}
            readOnly
            disabled
            aria-describedby="compose-from-hint"
            title="Multi-callsign selection not yet wired"
          />
          <span id="compose-from-hint" className="compose-hint">
            Multi-callsign — coming soon
          </span>
        </div>

        {/* To — chips + contacts autocomplete (Task A6). The `to` STATE stays a
            semicolon string with `group:<id>` sentinels so draft autosave is
            unchanged; group expansion happens only at send (buildRecipients). */}
        <div className="compose-field-row">
          <label htmlFor="compose-to" className="compose-label">
            To <span className="compose-label__req" aria-hidden="true">*</span>
          </label>
          <RecipientInput
            ref={toRef}
            id="compose-to"
            value={to}
            onChange={setTo}
            contacts={contacts}
            groups={groups}
            placeholder="W6ABC@winlink.org; W7DEF@winlink.org"
            aria-label="Recipients (semicolon-separated callsigns)"
          />
        </div>

        {/* Cc — enabled end-to-end per tuxlink-h1km. */}
        <div className="compose-field-row">
          <label htmlFor="compose-cc" className="compose-label">Cc</label>
          <RecipientInput
            ref={ccRef}
            id="compose-cc"
            value={cc}
            onChange={setCc}
            contacts={contacts}
            groups={groups}
            placeholder="W6ABC@winlink.org; W7DEF@winlink.org"
            aria-label="Cc recipients (semicolon-separated callsigns)"
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
            title="Template selection not yet wired"
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
          <CatalogBrowser
            onPick={(id) => {
              // Native registry takes precedence: forms with a compose-
              // side React `Form` component route into native form mode
              // (ICS-213, Bulletin in P0). Everything else (the bulk
              // of the WLE catalog + the operator's custom forms)
              // routes into webview-form mode via WebviewFormHost.
              const entry = lookupForm(id);
              if (entry?.Form) {
                setFormMode({ kind: 'form', formId: id, values: {} });
              } else {
                setFormMode({ kind: 'webview-form', formId: id });
              }
            }}
            onCancel={() => setFormMode({ kind: 'plain' })}
          />
        )}
        {formMode.kind === 'form' && (() => {
          const entry = lookupForm(formMode.formId);
          if (!entry || !entry.Form) {
            // Unknown form ID, or view-only entry with no compose-side Form.
            // The picker routes view-only ids to 'webview-form' mode, so this
            // branch should only fire on a stale draft restored from
            // localStorage whose formId no longer maps to a native entry.
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
        {formMode.kind === 'webview-form' && (
          <WebviewFormHost
            formId={formMode.formId}
            onSubmit={(payload) => handleWebviewSubmit(formMode.formId, payload)}
            onCancel={() => setFormMode({ kind: 'plain' })}
          />
        )}
        {formMode.kind === 'webview-reply' && (
          <WebviewFormHost
            formId={formMode.formId}
            replyPrefill={{
              fieldValues: formMode.values,
              msgOriginalBody: formMode.msgOriginalBody,
            }}
            onSubmit={(payload, meta) =>
              handleWebviewReplySubmit(formMode.formId, payload, meta?.replyTemplate)
            }
            onCancel={() => setFormMode({ kind: 'plain' })}
          />
        )}
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* Attachments — picker + drop zone; images resized at attach time    */}
      {/* ------------------------------------------------------------------ */}
      <div
        className="compose-attachments"
        onDragOver={handleDragOver}
        onDrop={handleDrop}
        data-testid="compose-attachments-zone"
      >
        <div className="compose-attachments__header">
          <button
            type="button"
            className="compose-attachments__add"
            onClick={handlePickFiles}
            disabled={attach.busy}
            data-testid="compose-attach-add"
          >
            Attach files…
          </button>
          {attach.totalBytes > 0 && (
            <span className="compose-attachments__total" data-testid="compose-attach-total">
              {humanSize(attach.totalBytes)} · {airtimeEstimate(attach.totalBytes)}
              {cmsStatus(attach.totalBytes) === 'over' && (
                <span className="compose-attachments__cms-over" data-testid="compose-attach-cms-over">
                  {' '}· exceeds Winlink CMS ~120 KB limit — resize smaller
                </span>
              )}
              {cmsStatus(attach.totalBytes) === 'near' && (
                <span className="compose-attachments__cms-near">
                  {' '}· near the CMS ~120 KB limit
                </span>
              )}
            </span>
          )}
        </div>
        {attach.error && <div className="compose-attachments__error">{attach.error}</div>}
        {attach.items.length === 0 ? (
          <span className="compose-attachments__hint">
            Drop files here or use “Attach files…”.
          </span>
        ) : (
          <ul className="compose-attachments__list">
            {attach.items.map((a, i) => (
              <li key={`${a.filename}-${i}`} className="compose-attachments__item">
                <span className="compose-attachments__name">{a.filename}</span>
                <span className="compose-attachments__size" data-testid={`compose-attach-size-${i}`}>
                  {humanSize(a.newLen)}
                  {a.kind === 'image' && a.newLen < a.originalLen && (
                    <span className="compose-attachments__resized"> (from {humanSize(a.originalLen)})</span>
                  )}
                </span>
                {a.kind === 'image' && (
                  <span className="compose-attachments__opts">
                    <select
                      className="compose-attachments__resize"
                      value={a.opts.resize}
                      disabled={attach.busy}
                      aria-label={`Resize for ${a.filename}`}
                      data-testid={`compose-attach-resize-${i}`}
                      onChange={(e) =>
                        attach.setOptions(i, { ...a.opts, resize: e.target.value as ImageOpts['resize'] })
                      }
                    >
                      <option value="original">Original size</option>
                      <option value="small">Small (480px)</option>
                      <option value="medium">Medium (640px)</option>
                      <option value="large">Large (800px)</option>
                    </select>
                    <select
                      className="compose-attachments__format"
                      value={a.opts.format}
                      disabled={attach.busy}
                      aria-label={`Format for ${a.filename}`}
                      data-testid={`compose-attach-format-${i}`}
                      onChange={(e) =>
                        attach.setOptions(i, { ...a.opts, format: e.target.value as ImageOpts['format'] })
                      }
                    >
                      <option value="original">Original format</option>
                      <option value="jpeg">JPEG</option>
                      <option value="webp">WebP (smaller)</option>
                    </select>
                  </span>
                )}
                {a.kind === 'file' && a.newLen > 256 * 1024 && (
                  <span className="compose-attachments__warn">
                    {airtimeEstimate(a.newLen)}
                  </span>
                )}
                <button
                  type="button"
                  className="compose-attachments__remove"
                  onClick={() => attach.remove(i)}
                  data-testid={`compose-attach-remove-${i}`}
                >
                  Remove
                </button>
              </li>
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
        {/* Post to Outbox + Compose form… only apply to plain-text mode. In
            form mode the form's own Send button handles submission; in pick
            mode neither applies. Hiding them removes the "why are these
            greyed out?" confusion (operator feedback 2026-06-01). */}
        {formMode.kind === 'plain' && (
          <button
            className="compose-btn compose-btn--primary"
            onClick={handleSend}
            disabled={sendState === 'sending'}
            title="Send (Ctrl+Enter)"
            data-testid="compose-send-btn"
          >
            {sendState === 'sending' ? 'Sending…' : 'Post to Outbox'}
          </button>
        )}
        {/* P1.1 (2026-06-04 Codex adrev): Save Draft only makes sense when
            Compose owns the form state. In webview-form mode the form
            contents live inside the embedded child webview, and Compose
            has no IPC introspection into them — Save Draft would only
            persist the formId metadata while silently losing every
            field value the operator typed. Hide the button entirely
            rather than offer a confusing "save" that drops content. */}
        {isSaveDraftAvailable(formMode.kind) && (
          <button
            className="compose-btn compose-btn--secondary"
            onClick={handleSaveDraft}
            title="Save draft (Ctrl+S)"
            data-testid="compose-save-draft-btn"
          >
            Save Draft
          </button>
        )}
        {formMode.kind === 'plain' && (
          <button
            className="compose-btn compose-btn--secondary"
            onClick={handleOpenFormPicker}
            data-testid="compose-form-picker-btn"
          >
            Compose form…
          </button>
        )}
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* Unsaved-changes close prompt (spec §5.4)                           */}
      {/*                                                                    */}
      {/* P1.1 (2026-06-04 Codex adrev): In webview-form mode the form       */}
      {/* contents live inside the embedded child webview — Compose has no   */}
      {/* IPC introspection into them. Offering "Save Draft" here would      */}
      {/* persist only the formId metadata while silently losing every       */}
      {/* field value the operator typed. Show a clearer message and offer   */}
      {/* only Discard + Cancel; the operator can return to the form and     */}
      {/* press its own Send button to submit.                               */}
      {/* ------------------------------------------------------------------ */}
      {closePrompt.open && (() => {
        const shape = closePromptShape(formMode.kind, closePrompt.action);
        return (
          <div
            className="compose-overlay"
            role="dialog"
            aria-modal="true"
            aria-label="Unsaved changes"
            data-testid="compose-close-prompt"
          >
            <div className="compose-dialog">
              <p className="compose-dialog__msg">{shape.primary}</p>
              {shape.sub && (
                <p
                  className="compose-dialog__sub"
                  data-testid="compose-close-sub"
                >
                  {shape.sub}
                </p>
              )}
              <div className="compose-dialog__actions">
                {shape.buttons.includes('save') && (
                  <button
                    className="compose-btn compose-btn--primary"
                    onClick={handleSaveAndProceed}
                    data-testid="compose-close-save"
                  >
                    Save Draft
                  </button>
                )}
                {shape.buttons.includes('discard') && (
                  <button
                    className="compose-btn compose-btn--danger"
                    onClick={handleDiscardAndProceed}
                    data-testid="compose-close-discard"
                  >
                    Discard
                  </button>
                )}
                {shape.buttons.includes('cancel') && (
                  <button
                    className="compose-btn compose-btn--ghost"
                    onClick={handleCancelClose}
                    data-testid="compose-close-cancel"
                  >
                    Cancel
                  </button>
                )}
              </div>
            </div>
          </div>
        );
      })()}
    </div>
  );
}
