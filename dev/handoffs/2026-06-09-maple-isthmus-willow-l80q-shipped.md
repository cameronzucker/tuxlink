# 2026-06-09 maple-isthmus-willow — tuxlink-l80q SHIPPED (multi-select bulk actions). PR #515 merged.

## What shipped this session

**tuxlink-l80q** (P1 bug → closed) — Inbox multi-select bulk actions. The
read/unread multi-select (etxt, PR #497/#499) was visually present but
functionally read/unread-only; this completes it. **PR #515 MERGED** (merge
commit `7b235e2`), remote branch deleted, issue closed, worktree disposed.

Two commits on the branch:
- `dd750ee` feat — the feature.
- `24a7cc2` fix — the Codex P2 remediation.

### Scope delivered (Approach B from the design doc)
- **Selection-aware context menu** (OS convention): right-click a SELECTED row →
  acts on ALL selected ("N messages" header + "Acting on N selected messages"
  footer); right-click an UNSELECTED row → selection resets to that one row
  (highlights) and the menu acts single-target. selectionMode is snapshotted at
  open so the reset can't flip a live menu back to single mode.
- **Bulk Archive + Move ▾** in `MessageBulkBar` (Move reuses `MoveToButton`;
  Archive disabled while in Archive) AND in the selection-mode context menu,
  both driven by shared AppShell handlers `bulkMoveToFolder` / `bulkArchive`.
- **New Rust `message_move_bulk`** command (+ testable `move_bulk_with_backend`),
  registered in lib.rs, mirroring `message_set_read_state_bulk`.
- `selectionToFolderItems` (new `src/mailbox/bulkSelection.ts`) extracts the
  cross-folder id→folder mapping + stale-id filter (#499 Fix-3), now shared by
  all three bulk handlers; `dropId`/`dropIds` selection-set helpers added. All
  unit-tested.
- **DELETE remains OUT OF SCOPE** — deferred to **tuxlink-2tg5** (still OPEN, P3;
  it depends on this PR's bulk plumbing). Do NOT add Delete without its own
  brainstorm (retention/restore/empty-trash model is the crux).

### Discipline / gates (all green pre-merge + CI)
- TDD against `docs/design/2026-06-09-l80q-multiselect-bulk-actions.md` (the spec).
- `cargo test` (incl. self-move data-loss regression test) ✓
- `cargo clippy --all-targets` ✓ (0 warnings)
- full `pnpm vitest run` ✓ — 174 files / 1966 tests
- **Codex** parent-level adversarial round → 3× P2, ALL fixed in `24a7cc2`:
  1. **DATA LOSS**: a bulk self-move (folder == destination) fell through to
     `Mailbox::move_between` (write-dst-then-remove-src on the same path =
     delete). Guarded in `move_bulk_with_backend` AND the `move_between`
     primitive (the latter also hardens the single `mailbox_move` path, which
     had no backend guard). + regression test.
  2. Out-of-selection right-click reset to `{}` (lost row highlight) → now
     resets to `{clicked id}`; single move/archive handlers drop the moved id.
  3. Bulk move/archive left stale ids in the selection (stranded the bulk-bar
     count) → now drops the whole requested set.
- CI on #515: build-linux + verify, both arches, all PASS.

## Repo / worktree state at session end

- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs` (operator state;
  multiple other sessions live — dyop, 6c9y, slate-glade-sparrow, …). l80q work
  was done entirely in its worktree, now disposed.
- **l80q worktree DISPOSED** (ADR 0009 ritual): inventory showed nothing of mine
  to propagate (all merged/pushed); the 7 `git stash list` entries belong to
  OTHER branches/sessions (fl6e, task-amd-main-ui, main) — left untouched, NOT
  mine. `rm -rf` + `git worktree prune` done; registry clean.
- bd `tuxlink-l80q` CLOSED; verified visible from the main checkout (shared
  Dolt). Note: `bd dolt push` has no remote configured here — bd state is shared
  locally via the embedded Dolt; no separate push step was possible/needed.
- The raw Codex transcript (`dev/adversarial/...l80q-codex.md`) was local-only
  in the disposed worktree (`.gitignore`d per policy); findings are captured in
  `24a7cc2` + PR #515 + above.

## Pending / follow-ups (none block anything)

- **tuxlink-2tg5** (P3, OPEN) — Message Delete/Trash workflow (net-new). Needs
  office-hours brainstorm first (retention/restore/empty model), then plan+build;
  reuses this PR's bulk-command pattern.
- Carryover from the prior handoff (not touched this session): FZ-M1 on-device
  confirm for the mt73 fix (operator); `tuxlink-mxui` (wire converge_build_fixtures
  into CI); triage backlog dqte/yrby/ci3o/ii1z/77m9; P1s 0ja/7fr/9ky (RF, gated).

## Operator next-session starting prompt

```
Continue tuxlink. Last session shipped tuxlink-l80q (multi-select bulk
actions — PR #515 merged, worktree disposed). READ the handoff first:
dev/handoffs/2026-06-09-maple-isthmus-willow-l80q-shipped.md

Then pick the next item from `bd ready`. Notes on the obvious candidates:
- tuxlink-2tg5 (Message Delete/Trash) is net-new UX → REQUIRES office-hours
  brainstorm FIRST (retention/restore/empty-trash model), not straight-to-build.
  It reuses l80q's bulk-command pattern.
- RF items (0ja/7fr/9ky) are claimable/buildable but on-air execution is
  operator/RADIO-1-gated.
Any UI feature work needs a brainstorm before build. Match worktree-per-bd-issue
discipline; hooks block main-checkout writes while other sessions are live.
```
