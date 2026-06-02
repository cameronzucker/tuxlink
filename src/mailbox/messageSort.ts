// Client-side sort for MessageList rows.
//
// bd issue: tuxlink-2x0l (MessageList sort UI — Phase 2 of mailbox-sort).
// Predecessor: tuxlink-mjc8 / PR #201 (backend deterministic ordering).
//
// The backend's mailbox_list already returns rows date-desc (newest first).
// This module sorts the already-fetched MessageMeta[] before Virtuoso receives
// it, so changing sort doesn't require a re-fetch.

import type { MessageMeta } from './types';

export type SortMode =
  | 'date-desc'
  | 'date-asc'
  | 'sender-asc'
  | 'sender-desc'
  | 'subject-asc'
  | 'subject-desc';

export const DEFAULT_SORT_MODE: SortMode = 'date-desc';

export interface SortOption {
  id: SortMode;
  label: string;
}

/// The fixed dropdown menu (order matches the bd issue spec).
export const SORT_OPTIONS: SortOption[] = [
  { id: 'date-desc', label: 'Newest first' },
  { id: 'date-asc', label: 'Oldest first' },
  { id: 'sender-asc', label: 'Sender A→Z' },
  { id: 'sender-desc', label: 'Sender Z→A' },
  { id: 'subject-asc', label: 'Subject A→Z' },
  { id: 'subject-desc', label: 'Subject Z→A' },
];

export const SORT_MODE_STORAGE_KEY = 'tuxlink.messageList.sortMode';

export function isSortMode(value: unknown): value is SortMode {
  return SORT_OPTIONS.some((o) => o.id === value);
}

/// Read the persisted sort, falling back to the default for missing/garbage.
export function loadSortMode(): SortMode {
  try {
    const stored = localStorage.getItem(SORT_MODE_STORAGE_KEY);
    return isSortMode(stored) ? stored : DEFAULT_SORT_MODE;
  } catch {
    return DEFAULT_SORT_MODE;
  }
}

/// Persist the chosen sort. Best-effort — storage may be unavailable.
export function saveSortMode(mode: SortMode): void {
  try {
    localStorage.setItem(SORT_MODE_STORAGE_KEY, mode);
  } catch {
    /* storage unavailable — selection still applies for this session */
  }
}

/// Sort key used for Sender. Folder-aware: Sent/Outbox sort by recipient (the
/// first `to` entry — what the row actually shows), everywhere else by sender.
/// Matches `correspondentLabel` in MessageList.tsx so the visible column and
/// the sort key never disagree.
function senderKey(msg: MessageMeta, folder: SortFolder): string {
  if (folder === 'sent' || folder === 'outbox') {
    return msg.to.length > 0 ? msg.to[0] : msg.from;
  }
  return msg.from;
}

/// Folder context for the sort. Matches the MailboxFolder union but kept as a
/// string literal here so this module stays free of mailbox-shell imports beyond
/// MessageMeta itself.
export type SortFolder = 'inbox' | 'outbox' | 'sent' | 'drafts' | 'deleted';

/// Lowercase + trim for case-insensitive locale compare. `localeCompare` already
/// handles diacritics; the lowercase normalizes "alpha" vs "Alpha" tie order.
function normalize(s: string): string {
  return s.trim().toLowerCase();
}

/// Compare two messages under `mode`. Stable secondary key: id (ascending) so
/// equal primary keys produce a deterministic order across renders.
export function compareMessages(
  a: MessageMeta,
  b: MessageMeta,
  mode: SortMode,
  folder: SortFolder,
): number {
  let primary = 0;
  switch (mode) {
    case 'date-desc':
      primary = b.date.localeCompare(a.date);
      break;
    case 'date-asc':
      primary = a.date.localeCompare(b.date);
      break;
    case 'sender-asc':
      primary = normalize(senderKey(a, folder)).localeCompare(normalize(senderKey(b, folder)));
      break;
    case 'sender-desc':
      primary = normalize(senderKey(b, folder)).localeCompare(normalize(senderKey(a, folder)));
      break;
    case 'subject-asc':
      primary = normalize(a.subject).localeCompare(normalize(b.subject));
      break;
    case 'subject-desc':
      primary = normalize(b.subject).localeCompare(normalize(a.subject));
      break;
  }
  if (primary !== 0) return primary;
  return a.id.localeCompare(b.id);
}

/// Return a new array of messages sorted by `mode`. Pure: input is not mutated.
export function sortMessages(
  messages: MessageMeta[],
  mode: SortMode,
  folder: SortFolder,
): MessageMeta[] {
  return [...messages].sort((a, b) => compareMessages(a, b, mode, folder));
}
