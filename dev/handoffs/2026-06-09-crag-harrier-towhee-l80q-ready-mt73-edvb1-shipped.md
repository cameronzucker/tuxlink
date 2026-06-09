# 2026-06-09 crag-harrier-towhee — START HERE: implement tuxlink-l80q (multi-select bulk actions). Design done. + edvb.1/mt73 shipped.

## 🚀 IMMEDIATE NEXT TASK — tuxlink-l80q implementation (zero ramp needed)

**Everything is decided. Go straight to build.** The investigation + office-hours
brainstorm + design doc are DONE and the worktree is already claimed.

- **Issue:** `tuxlink-l80q` (P1, in_progress) — Inbox multi-select bulk actions.
- **Worktree (already exists, claimed, deps installed):**
  `worktrees/bd-tuxlink-l80q-multiselect-bulk-actions`, branch
  `bd-tuxlink-l80q/multiselect-bulk-actions` (pushed; one commit `e93169b` = the design doc).
- **READ FIRST:** `docs/design/2026-06-09-l80q-multiselect-bulk-actions.md` (on that
  branch). It has the locked scope, premises, UX specifics, success criteria, and a
  numbered task list. The mock is `dev/scratch/l80q-multiselect-mock.html` (gitignored).
- **Locked scope:** selection-aware context menu (right-click a SELECTED row acts on
  ALL selected, OS convention; right-click an unselected row resets to that one) +
  bulk **Archive** + bulk **Move** in `MessageBulkBar` AND the context menu, via shared
  handlers + a new Rust `message_move_bulk` (mirror `message_set_read_state_bulk`,
  `ui_commands.rs:1349` / `lib.rs:379`). Read/unread bulk already ships.
- **DELETE IS DEFERRED** to `tuxlink-2tg5` (message delete/trash does NOT exist today —
  net-new). Do NOT add Delete in this PR.
- **Discipline:** the design doc IS the spec — TDD against it (per discipline-triage,
  this is bulk-ification of existing patterns, not a hard-to-undo arch decision; skip
  the heavy cross-provider adrev ceremony). Run a parent-level Codex round on the result
  as the independent gate (codex-post-subagent-review). Tests: bulk move/archive handler
  (incl. cross-folder id→folder mapping + stale-id filter, the #499 Fix-3 pattern),
  context-menu selection-vs-single branch, Rust `message_move_bulk`. Gate before push:
  `cargo clippy --all-targets` (re-run till exit 0) + full `pnpm vitest run` (the
  `message-view-loaded` flake fails only under shared-Pi load — confirm in isolation).
- **Key files:** `AppShell.tsx` (selectedIds:307, bulkSetReadState:739, context-menu
  by-id handlers:682-730, `<MessageList>` props:1025-1040), `MessageBulkBar.tsx`,
  `MessageContextMenu.tsx`, `MessageList.tsx` (right-click trigger ~188), `ui_commands.rs`.

## What shipped this session (crag-harrier-towhee)

- **tuxlink-edvb.1** — cross-vendor Codex adrev → **PR #502 MERGED** (`3023ed5`). Headline:
  PR #428 was reverted 35 min post-merge by b68017a; reviewed b68017a's live restoration.
  TDD-fixed an ignore-rule-timing gap in converge-build's root-`target/` refusal (fixture
  08). Issue closed, worktree disposed. Filed `tuxlink-mxui` (P3): converge_build_fixtures
  modes 2-6 rotted post-PR#207 + suite NOT CI-wired (main green despite 5/8 failing).
- **tuxlink-mt73** — investigated → root cause: Compose is a fixed 1100px webview tripping
  the width-only `@media (max-width: 1365px)` FZ-M1 breakpoint → fix A: append
  `and (any-pointer: coarse)` across 19 CSS files + `useViewport` + new
  `compactBreakpoint.test.tsx` guard. **PR #509 MERGED** (`167c23d`). Issue closed,
  worktree disposed.
  - ⚠️ **OPERATOR POST-MERGE STEP (unverifiable on dev Pi):** confirm on the real FZ-M1
    that it STILL renders compact (fix assumes WebKitGTK reports `any-pointer: coarse`).
    If regressed → swap to the screen-size alternative (documented in the PR/issue) or
    revert. No data-loss risk; fix-forward.
- **6 issues filed** (operator request): `tuxlink-77m9` (Winlink-over-APRS/APRSLink, net-new
  RF → brainstorm+RADIO-1), `tuxlink-ci3o` (import legacy WLE catalog as test data),
  `tuxlink-dqte` (station-lookup favorites broken — bug), `tuxlink-ii1z` (catalog parser:
  PRE-INVESTIGATED — CatalogReplyView EXISTS + wired at MessageView.tsx:515 but gated by
  `isCatalogReply` = From~SERVICE + Subject "INQUIRY - "; structured view = area-weather
  only), `tuxlink-yrby` (VARA FM missing from Find-a-gateway mode list — bug).
- **tuxlink-2tg5** (P3) — Message Delete/Trash workflow (net-new), split out of l80q.
- **Warm-up:** recovered 7 orphaned handoffs (commit `10db6c3` on this branch); disposed
  spent etxt+kuhk worktrees (~30 GB); closed tuxlink-kuhk after #499 already-merged.

## Repo / worktree state at session end

- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`, in sync with origin (before this
  handoff commit). Working tree had `.beads/issues.jsonl` (this session's bd activity) +
  an untracked handoff from a CONCURRENT session (`slate-glade-sparrow-n3hw-drafts-recency`)
  — left untouched (theirs).
- **My active worktree:** `bd-tuxlink-l80q-multiselect-bulk-actions` (design doc pushed;
  node_modules installed; gitignored `dev/scratch/l80q-multiselect-mock.html`). This is the
  immediate-start worktree above. Disposed this session: edvb.1, kuhk, etxt, mt73.
- **Other sessions are live** (dyop, eymu, 6c9y, slate-glade-sparrow, ...). Main-checkout
  history ops are hook-gated — use the l80q worktree for all l80q write work; `git push`
  (no local-HEAD mutation) works from the main checkout for clean fast-forwards.

## Pending decisions / follow-ups (none block l80q)

- FZ-M1 on-device confirm for the mt73 fix (operator, above).
- `tuxlink-mxui` — wire the converge_build_fixtures suite into CI + fix rotted modes 2-6.
- Triage backlog: the 5 other new bugs/features (dqte, yrby, ci3o, ii1z, 77m9) + the prior
  P1s (0ja abort-disarm, 7fr AX.25, 9ky UV-Pro BT). RF items stay operator/RADIO-1-gated;
  net-new features (77m9) need office-hours first.
