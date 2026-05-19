// Local Drafts-folder source — STUB owned by Task 12, replaced by Task 14.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §2.2, §7
// bd issue: tuxlink-zsm (Task 12)
//
// The sidebar's Drafts count reads `listDraftIds()`. Drafts are a LOCAL
// (localStorage) store, NOT a backend folder (spec §2.2). Task 14 owns the
// real draft store (`src/compose/useDraft.ts`); per the spec §7 soft-
// dependency note, Task 12 ships this tiny stub so the sidebar builds and
// shows a correct (0 until Task 14 lands) Drafts count without a hard build
// dependency on Task 14. When Task 14 merges, the integration swaps this for
// `useDraft.listDraftIds`.
//
// Reads are defensive: localStorage may be unavailable (SSR/test) — return
// [] rather than throwing.

const DRAFT_INDEX_KEY = 'tuxlink.drafts.index';

/// Return the list of saved draft ids from localStorage. Empty when none /
/// unavailable / malformed. Task 14 replaces this with the real store; the
/// key + shape are kept compatible so the count is correct cross-task.
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
