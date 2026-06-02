// Client-side sort for MessageList rows.
//
// bd issue: tuxlink-2x0l (MessageList sort UI — Phase 2 of mailbox-sort).
// Predecessor: tuxlink-mjc8 / PR #201 (backend deterministic ordering).
//
// Two orthogonal axes (operator iteration on PR #244): `SortKey` is what
// to sort by, `SortDirection` is which way. The popup renders them as
// two radio groups so the direction label can adapt to the active key
// ("Newest first" for date, "A→Z" for sender, "Smallest first" for size).

import type { MessageMeta } from './types';

export type SortKey = 'date' | 'sender' | 'recipient' | 'subject' | 'size';
export type SortDirection = 'asc' | 'desc';

export interface SortState {
  key: SortKey;
  direction: SortDirection;
}

/// Backend ordering after PR #201 is date-desc — match that as the default so
/// "no preference saved" renders the same order the backend produced.
export const DEFAULT_SORT_STATE: SortState = { key: 'date', direction: 'desc' };

export interface SortKeyOption {
  id: SortKey;
  label: string;
}

/// Order matches the popup radio group. Recipient sits next to Sender (both
/// are correspondent axes); Size sits at the end because the value-vs-string
/// comparison is a different mental model from the lexicographic three.
export const SORT_KEY_OPTIONS: SortKeyOption[] = [
  { id: 'date', label: 'Date' },
  { id: 'sender', label: 'Sender' },
  { id: 'recipient', label: 'Recipient' },
  { id: 'subject', label: 'Subject' },
  { id: 'size', label: 'Size' },
];

export interface DirectionLabel {
  desc: string;
  asc: string;
}

/// Direction labels adapt to the key — operator clarity: "Newest first" reads
/// naturally for date but is meaningless for size, etc. The popup renders
/// these in the second radio group based on the active key.
export const DIRECTION_LABELS: Record<SortKey, DirectionLabel> = {
  date: { desc: 'Newest first', asc: 'Oldest first' },
  sender: { desc: 'Z → A', asc: 'A → Z' },
  recipient: { desc: 'Z → A', asc: 'A → Z' },
  subject: { desc: 'Z → A', asc: 'A → Z' },
  size: { desc: 'Largest first', asc: 'Smallest first' },
};

export const SORT_STATE_STORAGE_KEY = 'tuxlink.messageList.sortState';

export function isSortKey(value: unknown): value is SortKey {
  return SORT_KEY_OPTIONS.some((o) => o.id === value);
}

export function isSortDirection(value: unknown): value is SortDirection {
  return value === 'asc' || value === 'desc';
}

export function isSortState(value: unknown): value is SortState {
  if (!value || typeof value !== 'object') return false;
  const v = value as { key?: unknown; direction?: unknown };
  return isSortKey(v.key) && isSortDirection(v.direction);
}

/// Read the persisted sort, falling back to the default for missing/garbage.
/// Old single-string format (PR #244, e.g. "date-desc") is intentionally not
/// migrated — it has been live for hours, the migration logic is more code
/// than it saves, and the worst case is one re-pick on first run.
export function loadSortState(): SortState {
  try {
    const stored = localStorage.getItem(SORT_STATE_STORAGE_KEY);
    if (!stored) return DEFAULT_SORT_STATE;
    const parsed = JSON.parse(stored) as unknown;
    return isSortState(parsed) ? parsed : DEFAULT_SORT_STATE;
  } catch {
    return DEFAULT_SORT_STATE;
  }
}

/// Persist the chosen sort. Best-effort — storage may be unavailable.
export function saveSortState(state: SortState): void {
  try {
    localStorage.setItem(SORT_STATE_STORAGE_KEY, JSON.stringify(state));
  } catch {
    /* storage unavailable — selection still applies for this session */
  }
}

/// Folder context for the sort. Matches the MailboxFolder union but kept as a
/// string literal here so this module stays free of mailbox-shell imports
/// beyond MessageMeta itself.
export type SortFolder = 'inbox' | 'outbox' | 'sent' | 'drafts' | 'deleted';

/// Lowercase + trim for case-insensitive locale compare. `localeCompare` already
/// handles diacritics; the lowercase normalizes "alpha" vs "Alpha" tie order.
function normalize(s: string): string {
  return s.trim().toLowerCase();
}

/// Sender column: the correspondent the row's "from" line shows. For Sent/
/// Outbox the visible value is the recipient (matching `correspondentLabel`
/// in MessageList.tsx), everywhere else the literal sender.
function senderKey(msg: MessageMeta, folder: SortFolder): string {
  if (folder === 'sent' || folder === 'outbox') {
    return msg.to.length > 0 ? msg.to[0] : msg.from;
  }
  return msg.from;
}

/// Recipient axis: explicit "who is this addressed to" regardless of folder.
/// Falls back to `from` only when `to` is empty (a bulletin-style message with
/// no explicit recipient) so the row still has a stable sort key.
function recipientKey(msg: MessageMeta): string {
  return msg.to.length > 0 ? msg.to[0] : msg.from;
}

/// Compare two messages under (key, direction). Stable secondary key: id
/// (ascending) so equal primary keys produce a deterministic order across
/// renders. The asc/desc flip is applied once at the end so per-key logic
/// stays direction-agnostic.
export function compareMessages(
  a: MessageMeta,
  b: MessageMeta,
  state: SortState,
  folder: SortFolder,
): number {
  let primary = 0;
  switch (state.key) {
    case 'date':
      primary = a.date.localeCompare(b.date);
      break;
    case 'sender':
      primary = normalize(senderKey(a, folder)).localeCompare(normalize(senderKey(b, folder)));
      break;
    case 'recipient':
      primary = normalize(recipientKey(a)).localeCompare(normalize(recipientKey(b)));
      break;
    case 'subject':
      primary = normalize(a.subject).localeCompare(normalize(b.subject));
      break;
    case 'size':
      primary = a.bodySize - b.bodySize;
      break;
  }
  if (primary !== 0) {
    return state.direction === 'asc' ? primary : -primary;
  }
  // Tiebreak by id ascending — stable across renders regardless of direction.
  return a.id.localeCompare(b.id);
}

/// Return a new array of messages sorted by `state`. Pure: input is not mutated.
export function sortMessages(
  messages: MessageMeta[],
  state: SortState,
  folder: SortFolder,
): MessageMeta[] {
  return [...messages].sort((a, b) => compareMessages(a, b, state, folder));
}
