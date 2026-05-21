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

export type ReplyMode = 'reply' | 'replyAll' | 'forward';

export interface DraftPrefill {
  /// Semicolon-separated recipients (matches the compose To field input format
  /// that `splitAddrs` parses at send time).
  to: string;
  subject: string;
  body: string;
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
/// v0.0.1 compose has no attachment-send path, so a forward silently dropping
/// them would violate the project's "never silently drop user data" rule — we
/// name them instead. Empty string when there are no attachments.
function attachmentsOmittedNote(message: ParsedMessage): string {
  if (message.attachments.length === 0) return '';
  const n = message.attachments.length;
  const names = message.attachments.map((a) => a.filename).join(', ');
  return `\n\n[${n} attachment${n === 1 ? '' : 's'} from the original message ${
    n === 1 ? 'was' : 'were'
  } not carried into this forward (attachment forwarding arrives in v0.1): ${names}]`;
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

/// Pure: derive the To / Subject / Body prefill for a reply, reply-all, or
/// forward off a parsed message. No I/O.
export function buildReplyDraft(message: ParsedMessage, mode: ReplyMode): DraftPrefill {
  if (mode === 'forward') {
    return {
      to: '',
      subject: FWD_PREFIX.test(message.subject) ? message.subject : `Fwd: ${message.subject}`,
      body: forwardBody(message),
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
  });
  await invoke('compose_window_open', { draftId });
}
