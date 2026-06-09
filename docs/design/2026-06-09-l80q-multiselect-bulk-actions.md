# Design: Inbox multi-select — selection-aware actions + bulk Archive/Move

Generated via office-hours (builder mode) on 2026-06-09
Branch: bd-tuxlink-l80q/multiselect-bulk-actions
Issue: tuxlink-l80q
Status: APPROVED (scope decisions locked with operator)
Agent: crag-harrier-towhee

## Problem statement

The read/unread multi-select shipped in tuxlink-etxt (Tasks 8–13, PR #497/#499)
is visually present but functionally incomplete. The operator can Ctrl/Shift+click
to build a selection, but actions don't apply to it:

- The **right-click context menu** acts on the right-clicked row ONLY, by design
  (`AppShell.tsx:684`; handlers `moveByIdToFolder` / `archiveByIdAndFolder` /
  `setMessageReadState` each take a single id) — it ignores the selection set.
- The **bulk action bar** (`MessageBulkBar`) — the ONLY selection-aware control —
  carries just Mark read / Mark unread / Clear. No bulk Archive, Move, or Delete.
- **Reading-pane / toolbar** controls act on the single open message.

Net: multi-select is a read/unread-only feature behind one easily-missed bar
(it replaces the sort control in the list header, `MessageList.tsx:404`), while
every other action and the entire context menu are single-target. This violates
the "use OS conventions" rule (right-click on a selection acts on the whole
selection) and the alpha "complete feature, not a partial slice" bar.

## What makes this right

Bring multi-select to OS-standard behavior with the smallest honest surface:
every action that EXISTS as a single-message action becomes selection-aware,
reachable from both the bulk bar and the context menu, with one shared set of
bulk handlers. No half-built actions.

## Constraints (design within; do NOT relitigate)

- Selection state = `selectedIds: Set<string>` in `AppShell.tsx:307`, cleared on
  folder change (308). Bulk read/unread already works via `bulkSetReadState`
  (`AppShell.tsx:739`) → Rust `message_set_read_state_bulk` (registered
  `lib.rs:379`).
- UI stays INLINE — no popup windows (operator pet peeve; Compose is the lone
  window exception). The bulk Move picker is an inline dropdown/submenu.
- OS conventions, Ctrl-first modifier hints.
- Existing single-message actions: Mark read/unread, Move-to (Inbox/Sent/user
  folders), Archive. (`MessageContextMenu.tsx`.)

## Premises (confirmed with operator)

1. Right-click a row that IS in the selection → action applies to ALL selected.
   Right-click a row that is NOT in the selection → selection resets to that one
   row and acts on it. (Windows Explorer / mail-client convention.)
2. The bulk bar gains **Archive** + **Move ▾** (inline destination dropdown)
   alongside the existing Mark read / Mark unread. Bar and context menu drive
   the SAME bulk handlers.
3. New Rust `message_move_bulk` mirrors `message_set_read_state_bulk` (one
   command, `{items:[{folder,id}], to}`); Archive = move-to-archive. Read/unread
   bulk already exists and is reused.
4. **Delete is OUT OF SCOPE** for this work and DEFERRED to its own issue.
   Rationale: single-message Delete does not exist anywhere in tuxlink today —
   no Trash/Deleted folder, no delete command, and `MessageContextMenu.tsx:37`
   deliberately excludes "Deleted" as a destination ("unimplemented"). "Bulk
   Delete" would be a net-new delete feature (Deleted folder + command + confirm
   + retention/restore semantics), not a bulk-ification. Designing it properly
   is its own effort; bolting a half-version onto this PR would violate the alpha
   vettedness bar.

## Approaches considered

### Approach A: Bulk bar only (minimal) — REJECTED
Add Archive + Move ▾ to `MessageBulkBar`; leave the context menu single-target.
- Effort: S. Risk: Low.
- Pro: smallest diff; closes the "no bulk archive/move" half.
- Con: does NOT fix the operator's primary complaint (right-click on a selection
  acts on one row). Violates OS convention. Rejected by operator.

### Approach B: Selection-aware menu + bulk bar Archive/Move (RECOMMENDED, chosen)
Context menu honors the selection (premise 1) AND the bulk bar gains Archive +
Move ▾. Both entry points call shared bulk handlers (`bulkSetReadState`,
new `bulkMoveToFolder`/`bulkArchive`). New Rust `message_move_bulk`.
- Effort: M. Risk: Low–Med (touches the context-menu trigger path + a new Rust
  command + frontend wiring).
- Pro: completes multi-select for every existing action from both entry points;
  matches OS convention; alpha-complete; bounded (Delete deferred).
- Con: the context-menu trigger needs the "right-clicked row in selection?"
  branch + a count-aware menu header/footer ("N messages").
- Reuses: `selectedIds`, `MessageBulkBar`, `MessageContextMenu`, the by-id
  handlers (looped), the `message_set_read_state_bulk` pattern.

### Approach C: Unified selection model everywhere (lateral) — REJECTED as over-scope
Make the reading-pane/toolbar controls also operate on the selection.
- Effort: L. Risk: Med.
- Con: the reading pane shows ONE open message; making its controls act on a
  hidden N-selection is a footgun and contradicts "plain-click opens one
  message + clears selection." Out of scope.

## Recommended approach

**Approach B.** See the locked mock at
`dev/scratch/l80q-multiselect-mock.html` (rendered 2026-06-09): extended bulk
bar (`3 selected · Mark read · Mark unread · Archive · Move ▾ · ✕`) + a
selection-aware context menu whose header reads "N messages" and footer reads
"Acting on N selected messages", offering Mark read/unread + Move-to
(Inbox/Sent/Archive/user folders) — the single-message menu's destinations,
applied to the whole selection.

### UX specifics (locked)
- **Right-click in-selection vs out:** if the right-clicked row's id ∈
  `selectedIds`, the menu is selection-mode (header "N messages", acts on all).
  Else, reset `selectedIds` to `{that id}` (or empty + single-target) and act on
  the one row — standard convention.
- **Menu header/footer:** single-target keeps today's subject footer; selection
  mode shows "N messages" header + "Acting on N selected messages" footer.
- **Bulk Move ▾:** inline dropdown anchored under the Move button, same
  destination list as the context menu (system + user folders); current folder
  disabled.
- **Archive while in Archive / Move to current folder:** disabled, as today.
- **Post-action:** selection is retained after bulk read/unread (existing
  behavior); after a bulk MOVE/ARCHIVE the moved rows leave the current view, so
  clear `selectedIds` of moved ids (or all, since they left). Mirror the single
  handlers that clear `selectedMessage` when the open row moved.
- **Cross-folder search view:** bulk handlers map each id to its own folder
  (the `byId` map already in `bulkSetReadState`), filtering ids missing from the
  visible list (the Fix-3 pattern from #499).

## Open questions
- Bulk Move ▾ placement on the FZ-M1 compact bulk bar (the bar is width-tight in
  compact). Likely the dropdown is fine; verify against the compact header slot.
- Whether the context-menu "Folders" submenu (▸) should flatten in bulk mode for
  fewer clicks. Default: keep parity with single-message menu.

## Success criteria
- With N messages selected: bulk Mark read/unread, Archive, and Move-to-folder
  all apply to all N, from BOTH the bulk bar and the context menu.
- Right-click a selected row → acts on all selected; right-click an unselected
  row → acts on that one (selection resets).
- Reading-pane single-message behavior unchanged; plain-click still opens one +
  clears selection.
- Tests: bulk move/archive handler unit tests (incl. cross-folder id→folder
  mapping + stale-id filter); context-menu selection-vs-single branch test;
  Rust `message_move_bulk` test. Full `pnpm vitest run` + `cargo clippy
  --all-targets` green.

## Distribution
Ships in the normal tuxlink app (Tauri) build/release pipeline. No new artifact.

## Next steps (implementation)
1. Rust: `message_move_bulk(items:[{folder,id}], to)` in `ui_commands.rs`,
   register in `lib.rs` invoke_handler; test mirroring `message_set_read_state_bulk`.
2. AppShell: `bulkMoveToFolder(ids, to)` + `bulkArchive(ids)` handlers (loop/
   batch via the new command); wire to `MessageBulkBar` + the context menu.
3. `MessageBulkBar`: add Archive button + Move ▾ dropdown (destination list).
4. `MessageContextMenu` + the `MessageList` right-click trigger: selection-aware
   branch (in-selection → N-mode header/footer + bulk handlers; out → reset +
   single). 
5. Tests (frontend + Rust) per success criteria; verify compact bulk bar.
6. File the deferred **Delete/Trash** issue (Deleted folder + command + confirm
   + retention/restore; single + bulk).

## Deferred follow-up
- **Message Delete / Trash** — net-new feature (see premise 4). File as a
  separate bd issue; design its retention/restore/empty semantics in its own
  brainstorm.
