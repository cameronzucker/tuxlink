# 2026-06-09 slate-glade-sparrow — handoff: fix tuxlink-n3hw (drafts reorder on read)

## TL;DR for the next session

Pick up **`tuxlink-n3hw`** (P2 bug, **already `in_progress`/claimed**) — "Drafts reorder when opened for reading; only edits should affect draft recency." **The root cause is already found** (below), so go **straight to TDD** — this is a contained single-function guard, no data-loss risk; skip the build-robust-features ceremony per the `discipline_triage_rule` memory.

## Root cause (verified by reading the code)

- Drafts live in localStorage. `saveDraft()` stamps `savedAt = new Date().toISOString()` — [src/compose/useDraft.ts:69-70](../../src/compose/useDraft.ts#L69-L70).
- The Compose **autosave interval fires every 2s and calls `saveDraft()` UNCONDITIONALLY**, guarded only by `!sentRef.current` — [src/compose/Compose.tsx:187-201](../../src/compose/Compose.tsx#L187-L201).
- So **opening a draft for reading** mounts Compose → starts the 2s timer → within 2s re-stamps `savedAt = now` → the Drafts list re-sorts that draft to the top. Reading a draft for >2s reorders it. That's the bug.

## The fix (suggested — TDD it)

Gate the autosave's `saveDraft` on **actual content change**, so an unedited open never re-stamps `savedAt`:

1. There is already a `savedSnapshotRef` set in `handleSaveDraft` ([Compose.tsx ~L227](../../src/compose/Compose.tsx#L227)) — the dirty-check primitive. **Initialize it from the loaded draft on mount** (where `loadDraft(draftId)` runs, [Compose.tsx:144](../../src/compose/Compose.tsx#L144)) so a freshly-opened draft starts "clean".
2. In the **2s autosave interval**, before calling `saveDraft`, deep-compare the current `{to, cc, subject, body, requestAck, formId, formFields}` against `savedSnapshotRef.current`; **skip `saveDraft` when unchanged**. Update the snapshot whenever you do persist.
3. Keep the existing `!sentRef.current` guard (post-send no-recreate, a prior Codex P1 fix — don't regress it).

**Still to confirm (do this first):** where the Drafts-folder list actually orders by `savedAt`. A grep of `src/shell/AppShell.tsx` for `savedAt`/`draftMessages` found nothing, so the ordering may be `listDraftIds()` index order ([useDraft.ts:44](../../src/compose/useDraft.ts#L44)) rather than a `savedAt` sort — trace `draftMessages` from `AppShell` into how the Drafts folder renders. The fix target is "what determines order"; if it's the id-index order and `saveDraft` reorders the index, the guard in step 2 still fixes it (no re-save → no reorder), but verify before writing the test so the test asserts the real observable (draft order unchanged after open-without-edit, changed after edit).

## TDD shape

- **Failing test first** (likely `src/compose/draft.test.ts` or a new `Compose` test): open/load an existing draft, advance timers past the autosave interval *without changing content*, assert `savedAt` (and list order) is unchanged; then change a field, advance timers, assert `savedAt` updates. Use `vi.useFakeTimers()` to drive the 2s interval deterministically.
- Implement the dirty-guard, verify green.
- Gate before push: `cargo`-side unaffected (TS-only change); run the affected vitest file(s). **Heads-up: the full `pnpm vitest run` crashes on this Pi under load — run scoped per-file (`pnpm vitest run src/compose/...`) and reap zombies (`pkill -9 -f vitest`) after.** CI runs the full suite on real hardware as the gate.

## Scope guardrails

- Scope n3hw to **ordering/recency only**. `tuxlink-2l66` ("clicking a draft opens it for editing instead of viewing") is a *related but separate* issue about read-vs-edit open mode — don't fold it in unless trivially coupled; note it if your fix touches the same open path.
- No RF, no RADIO-1 gate, no operator decision needed.

## Repo state at handoff

- **ka3z (#498) is DONE and CI-GREEN, awaiting merge.** Branch `bd-tuxlink-ka3z/nested-folders` @ `7337832` (merged `origin/main` 0.39.1 in). All 4 CI checks pass (verify + build, both arches). Nested user folders shipped end-to-end. If you merge it before starting n3hw, n3hw branches off the updated main cleanly. n3hw does **not** touch folders, so no conflict either way.
- **Main checkout:** operator branch `bd-tuxlink-xygm/recover-handoffs`; `.beads/issues.jsonl` is bd's Dolt state (don't commit the worktree JSONL). Other live sessions active (dyop-lan-tiles, 6c9y-telnet-post-office, eymu-request-center) → **worktrees mandatory** for write work (ADR 0008).
- **Avoided for n3hw:** `tuxlink-c61v` (sidebar Inbox-glyph bug) overlaps ka3z's `FolderSidebar` changes — defer it until #498 merges to avoid a conflict.

## Next-session starting prompt

```
Fix tuxlink-n3hw (drafts reorder when opened for reading; only edits should
affect draft recency). It's already in_progress/claimed. FIRST read the full
handoff: dev/handoffs/2026-06-09-slate-glade-sparrow-n3hw-drafts-recency.md

Root cause is ALREADY found (Compose.tsx 2s autosave calls saveDraft
unconditionally → re-stamps savedAt on open). Per discipline_triage_rule this is
a contained single-function guard with no data-loss risk: go STRAIGHT TO TDD
(superpowers:test-driven-development), skip build-robust-features ceremony.

GATE/setup: create a worktree (other sessions are live — worktrees mandatory):
  python3 .claude/scripts/new_tuxlink_worktree.py --slug n3hw-drafts --issue tuxlink-n3hw --moniker <your-moniker>
Confirm the Drafts list ordering path before writing the test. Run vitest
SCOPED per-file (full suite OOM-crashes on this Pi) + reap zombies after. The bd
issue note has the full root cause + fix sketch.
```
