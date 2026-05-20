// Local Drafts-folder source — Task-12 stub, REPLACED by Task 14's real store
// in the orchestrator integration commit (spec §4.3 / §7 soft-dependency note).
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §2.2, §7
// bd issue: tuxlink-zsm (Task 12) → tuxlink-8zg (integration)
//
// Originally a standalone localStorage reader so the sidebar's Drafts count
// built without a hard dependency on Task 14. Now that Task 14's draft store
// (`src/compose/useDraft.ts`) has merged, this module re-exports the canonical
// `listDraftIds` so there is ONE source of truth for the draft index. The
// key + shape were kept compatible across tasks, so existing FolderSidebar
// imports (`import { listDraftIds } from './draftIds'`) keep working unchanged
// while now reading the real store the compose window writes to.

export { listDraftIds } from '../compose/useDraft';
