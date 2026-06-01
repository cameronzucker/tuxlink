// localStorage draft store — single source of truth for outbound drafts.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.4
// bd issue: tuxlink-dm8 (Task 14 — compose window)
//
// ALSO used by Task 12's FolderSidebar: `listDraftIds()` is the Drafts-folder
// source. The stub at `src/mailbox/draftIds.ts` will be replaced by this
// module in the integration commit (spec §7, soft-dependency note).
//
// Key layout:
//   tuxlink.drafts.index          — JSON-encoded string[] of draftIds
//   tuxlink.drafts.<draftId>      — JSON-encoded DraftData for one draft
//
// The key format is stable across tasks: draftIds.ts uses the same
// DRAFT_INDEX_KEY so counts are correct before Task 14 merges.

export const DRAFT_INDEX_KEY = 'tuxlink.drafts.index';
const draftKey = (id: string) => `tuxlink.drafts.${id}`;

/// Shape of a saved draft. All fields optional (user may not have filled
/// them yet). `savedAt` is the ISO timestamp of the last autosave.
export interface DraftData {
  draftId: string;
  to: string;      // raw semicolon-separated input string (split on send)
  subject: string;
  body: string;
  requestAck: boolean;
  /** Form ID when this draft is a form-mode draft (T6.3). Optional. */
  formId?: string;
  /** Form field values when this draft is a form-mode draft (T6.3). Optional. */
  formFields?: Record<string, string>;
  savedAt: string; // ISO 8601 UTC
}

// ============================================================================
// Index helpers
// ============================================================================

/// Return the list of saved draft ids from localStorage. Defensive: returns
/// [] on any parse/access failure.
export function listDraftIds(): string[] {
  try {
    const raw = globalThis.localStorage?.getItem(DRAFT_INDEX_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

function persistIndex(ids: string[]): void {
  try {
    globalThis.localStorage?.setItem(DRAFT_INDEX_KEY, JSON.stringify(ids));
  } catch {
    // localStorage unavailable (e.g. SSR, quota full) — silently ignore
  }
}

// ============================================================================
// CRUD
// ============================================================================

/// Persist (create or update) a draft. Adds the id to the index if new.
/// Returns the saved `DraftData` (with `savedAt` stamped to now).
export function saveDraft(data: Omit<DraftData, 'savedAt'>): DraftData {
  const saved: DraftData = { ...data, savedAt: new Date().toISOString() };
  try {
    globalThis.localStorage?.setItem(draftKey(data.draftId), JSON.stringify(saved));
    const ids = listDraftIds();
    if (!ids.includes(data.draftId)) {
      persistIndex([...ids, data.draftId]);
    }
  } catch {
    // localStorage unavailable — best-effort
  }
  return saved;
}

/// Load a draft by id. Returns `null` when the id is unknown or storage is
/// unavailable. Does NOT throw.
export function loadDraft(draftId: string): DraftData | null {
  try {
    const raw = globalThis.localStorage?.getItem(draftKey(draftId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (
      parsed &&
      typeof parsed === 'object' &&
      'draftId' in parsed &&
      'to' in parsed &&
      'subject' in parsed &&
      'body' in parsed
    ) {
      return parsed as DraftData;
    }
    return null;
  } catch {
    return null;
  }
}

/// Remove a draft and its index entry. No-op when the id is not present.
export function clearDraft(draftId: string): void {
  try {
    globalThis.localStorage?.removeItem(draftKey(draftId));
    persistIndex(listDraftIds().filter((id) => id !== draftId));
  } catch {
    // localStorage unavailable — best-effort
  }
}

// ============================================================================
// Address splitting (spec §5.4 / §6 test 4)
// ============================================================================

/// Split a semicolon-separated address string into a trimmed, non-empty
/// string array. Used for the To field at send time.
///
/// @example splitAddrs('W6ABC ; W7DEF;') // => ['W6ABC', 'W7DEF']
export function splitAddrs(raw: string): string[] {
  return raw
    .split(';')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}
