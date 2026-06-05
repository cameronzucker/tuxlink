// Reply / Reply-All / Forward actions for the reading pane.
//
// bd issue: tuxlink-cbz (reading-pane action bar, operator decision 2026-05-20).
// mock-d shows amber Reply / Reply All / Forward atop the reading pane. The
// Task-13 spec (§5.6) did not include them; this adds them, wired to the
// existing compose window.
//
// Mechanism: there is no dedicated "reply" IPC. We reuse the established draft
// seam — seed a prefilled draft into the localStorage store (the same store the
// compose window restores from on mount, useDraft.ts) under a fresh draftId,
// then open a compose window for that id via `compose_window_open`. That
// command is gated to the MAIN window (compose_window.rs); the reading pane
// lives in the main window, so this is authorized.

import { invoke } from '@tauri-apps/api/core';
import { saveDraft } from '../compose/useDraft';
import type { ParsedMessage } from './types';
import { sanitizeAttachmentName } from './sanitize';

export type ReplyMode = 'reply' | 'replyAll' | 'forward' | 'replyWithForm';

export interface DraftPrefill {
  /// Semicolon-separated recipients (matches the compose To field input format
  /// that `splitAddrs` parses at send time).
  to: string;
  subject: string;
  body: string;
  /** When set, opens compose in form-mode pre-populated with these fields. */
  formId?: string;
  formFields?: Record<string, string>;
}

const RE_PREFIX = /^re:\s*/i;
const FWD_PREFIX = /^fwd:\s*/i;

/// Compact UTC label for quote/forward attribution. Mirrors
/// MessageList.formatListDate intentionally — kept local so this module stays
/// free of the list/virtuoso import graph.
function formatUtc(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  return (
    `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}Z`
  );
}

/// Trim, drop empties, dedupe (order-preserving), and join with "; ".
function uniqueJoin(addrs: string[]): string {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of addrs) {
    const a = raw.trim();
    if (a && !seen.has(a)) {
      seen.add(a);
      out.push(a);
    }
  }
  return out.join('; ');
}

// The reader hides Winlink form payloads behind a placeholder and never shows
// raw XML; a reply/forward must not expose or transmit it either (Codex P2).
const FORM_QUOTE_PLACEHOLDER =
  '[Winlink form — the original form content is not included in this draft.]';

/// The text to quote/forward for a message: its body, EXCEPT for form messages
/// whose body is raw XML — those substitute a safe placeholder so neither the
/// reply quote nor the forward leaks/transmits the hidden payload.
function quoteSource(message: ParsedMessage): string {
  return message.isForm ? FORM_QUOTE_PLACEHOLDER : message.body;
}

/// A visible note for a forward that cannot carry the original attachments.
/// The compose path has no attachment-send wiring yet, so a forward silently
/// dropping them would violate the project's "never silently drop user data"
/// rule — we name them instead. Empty string when there are no attachments.
function attachmentsOmittedNote(message: ParsedMessage): string {
  if (message.attachments.length === 0) return '';
  const n = message.attachments.length;
  const names = message.attachments.map((a) => sanitizeAttachmentName(a.filename)).join(', ');
  return `\n\n[${n} attachment${n === 1 ? '' : 's'} from the original message ${
    n === 1 ? 'was' : 'were'
  } not carried into this forward (attachment forwarding not yet supported): ${names}]`;
}

function replyBody(message: ParsedMessage): string {
  const quoted = quoteSource(message)
    .split('\n')
    .map((line) => `> ${line}`)
    .join('\n');
  return `\n\nOn ${formatUtc(message.date)}, ${message.from} wrote:\n${quoted}\n`;
}

function forwardBody(message: ParsedMessage): string {
  const header = [
    '--- Forwarded message ---',
    `From: ${message.from}`,
    `Date: ${formatUtc(message.date)}`,
    `Subject: ${message.subject}`,
    message.to.length > 0 ? `To: ${message.to.join(', ')}` : null,
  ]
    .filter((l): l is string => l !== null)
    .join('\n');
  return `\n\n${header}\n\n${quoteSource(message)}\n${attachmentsOmittedNote(message)}`;
}

// ============================================================================
// Per-form reply-with-form support map (Codex r2 P2 #1)
// ============================================================================
//
// Only forms with explicit field-mapping logic in buildReplyDraft below are
// safe to expose via the "Reply with form…" action — otherwise clicking the
// button on, say, a Position Report opens a blank-ish form with no useful
// pre-population. MessageView's button visibility consults this set.
//
// Intentionally NOT included:
// - Position_Report: position reports are broadcast (no recipient field +
//   no meaningful operator-text-content beyond the message — the reply
//   value-add is nil over composing a new Position Report).
// - Form-309_Initial: comms logs aren't reply-shaped (replying to a log
//   has no meaningful semantics in WLE either).
// - Winlink_Check-In: check-ins are fire-and-forget status updates; WLE
//   has no _SendReply.0 template for them.
// - Damage_Assessment_Initial: damage assessments are reports, not
//   conversation threads.
const REPLY_WITH_FORM_SUPPORTED: ReadonlySet<string> = new Set([
  'ICS213_Initial',
  'Bulletin_Initial',
]);

/// True iff `replyWithForm` produces a meaningfully-populated draft for the
/// given form ID. Used by MessageView to gate the "Reply with form…" button.
export function hasReplyWithFormSupport(formId: string | null | undefined): boolean {
  return !!formId && REPLY_WITH_FORM_SUPPORTED.has(formId);
}

/// Pure: derive the To / Subject / Body prefill for a reply, reply-all,
/// forward, or replyWithForm off a parsed message. No I/O.
export function buildReplyDraft(message: ParsedMessage, mode: ReplyMode): DraftPrefill {
  if (mode === 'forward') {
    return {
      to: '',
      subject: FWD_PREFIX.test(message.subject) ? message.subject : `Fwd: ${message.subject}`,
      body: forwardBody(message),
    };
  }

  if (mode === 'replyWithForm') {
    // Only valid for messages that already carry a form payload AND have a
    // per-form mapping below. Unsupported forms (Position, ICS-309,
    // Check-In, Damage Assessment — see REPLY_WITH_FORM_SUPPORTED for the
    // rationale) fall back to a plain reply rather than producing a half-
    // populated form draft (Codex r2 P2 #1).
    if (
      !message.isForm ||
      !message.formId ||
      !message.formPayload ||
      !hasReplyWithFormSupport(message.formId)
    ) {
      return buildReplyDraft(message, 'reply');
    }
    const origFields: Record<string, string> = Object.fromEntries(message.formPayload.fields);
    const carrySubject = (raw: string | undefined): string => {
      if (!raw) return '';
      return RE_PREFIX.test(raw) ? raw : `Re: ${raw}`;
    };

    let formFields: Record<string, string>;
    switch (message.formId) {
      case 'ICS213_Initial':
        // Sender↔recipient swap: original fm_name → new to_name; preserve
        // subjectline + inc_name + isexercise. Don't carry approval /
        // message body — those are response-specific.
        formFields = {
          to_name: origFields['fm_name'] ?? '',
          inc_name: origFields['inc_name'] ?? '',
          subjectline: carrySubject(origFields['subjectline']),
          isexercise: origFields['isexercise'] ?? '',
        };
        break;
      case 'Bulletin_Initial':
        // Bulletin reply semantics: the original sender (from_name)
        // becomes the new bulletin's recipient (`name` = "For"); leave
        // bulletin number, datetime, and message blank for the operator
        // to fill — they're per-bulletin volatile fields. Carry the
        // precedence level + title so an acknowledgment bulletin matches
        // the original's urgency + identity. `from_name` left blank so
        // the operator's config / chrome callsign supplies it on send.
        formFields = {
          name: origFields['from_name'] ?? '',
          from_name: '',
          level: origFields['level'] ?? '',
          title: origFields['title'] ?? '',
          subjectline: carrySubject(origFields['subjectline']),
          bullnr: '',
          activitydatetime1: '',
          message: '',
        };
        break;
      default:
        // hasReplyWithFormSupport gates entry, so this branch is
        // unreachable in practice. Belt-and-suspenders: a stray formId
        // (e.g. operator manually edited the registry) falls back to a
        // plain reply rather than crashing.
        return buildReplyDraft(message, 'reply');
    }

    return {
      to: message.from,
      subject: RE_PREFIX.test(message.subject) ? message.subject : `Re: ${message.subject}`,
      body: '',
      formId: message.formId,
      formFields,
    };
  }

  const to =
    mode === 'replyAll'
      ? uniqueJoin([message.from, ...message.to, ...message.cc])
      : message.from;

  return {
    to,
    subject: RE_PREFIX.test(message.subject) ? message.subject : `Re: ${message.subject}`,
    body: replyBody(message),
  };
}

/// Fresh draft id for a reply/forward compose window. Mirrors App.tsx's
/// newDraftId so reply drafts key the same way new-message drafts do.
function newReplyDraftId(): string {
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  const rand = Math.random().toString(36).slice(2, 8);
  return `draft-${ts}-${rand}`;
}

/// Seed a prefilled draft and open a compose window for it. The compose window
/// restores the draft by id on mount (useDraft.loadDraft). Returns the promise
/// of the window-open IPC so callers can surface failures if they wish.
export async function openReplyWindow(message: ParsedMessage, mode: ReplyMode): Promise<void> {
  const prefill = buildReplyDraft(message, mode);
  const draftId = newReplyDraftId();
  saveDraft({
    draftId,
    to: prefill.to,
    subject: prefill.subject,
    body: prefill.body,
    requestAck: false,
    formId: prefill.formId,
    formFields: prefill.formFields,
  });
  await invoke('compose_window_open', { draftId });
}
