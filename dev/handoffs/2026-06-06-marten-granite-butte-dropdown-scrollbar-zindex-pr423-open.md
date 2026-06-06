# Handoff - 2026-06-06 dropdown scrollbar z-index PR #423 open

**From agent:** `marten-granite-butte`
**Session arc:** Fixed bd issue `tuxlink-bqq4` by making top-app dropdown layers paint above the message-list scroll/content layer, then opened PR #423.
**Status:** pushed; PR #423 open; bd issue closed.

---

## Next Session's Starting Prompt

```
I'm resuming tuxlink after marten-granite-butte fixed tuxlink-bqq4 and opened PR #423.
Read dev/handoffs/2026-06-06-marten-granite-butte-dropdown-scrollbar-zindex-pr423-open.md first.
Then read CLAUDE.md/AGENTS.md workflow sections for worktrees, destructive-git ban, bd, and session completion.
Generate a fresh moniker with python3 .claude/scripts/get_agent_moniker.py.
First action: check PR #423 status with gh pr checks 423.
If CI is green, review/merge via merge commit only; do not squash.
If CI is red, fix forward on bd-tuxlink-bqq4/dropdown-scrollbar-zindex and rerun the failing gate.
RF/on-air code was not touched; keep RADIO-1 consent rules in force.
After merge, dispose worktrees/bd-tuxlink-bqq4-dropdown-scrollbar-zindex per ADR 0009.
```

---

## What Landed

| Item | What | PR # | Status |
|---|---|---|---|
| `tuxlink-bqq4` | Isolated `.layout-b .panes` at the base content layer, lifted the ribbon/search and menubar chrome layers above it, and pinned the CSS contract in `MenuBar.test.tsx`. | #423 | open |

Implementation files:

- `src/shell/AppShell.css`
- `src/shell/chrome/chrome.css`
- `src/shell/chrome/MenuBar.test.tsx`

Session/admin file:

- `dev/handoffs/2026-06-06-marten-granite-butte-dropdown-scrollbar-zindex-pr423-open.md`

RF/on-air code was not touched.

---

## Verification

Passed locally:

```bash
pnpm vitest run src/shell/AppShell.test.tsx src/shell/chrome/MenuBar.test.tsx src/mailbox/MessageList.test.tsx
pnpm typecheck
git diff --check
```

Targeted Vitest result: 3 files passed, 66 tests passed.

`pnpm install --offline` was needed because the new worktree had no `node_modules/`. It completed successfully after sandbox approval for pnpm's local store metadata.

---

## Branch and PR State

Branch:

```text
bd-tuxlink-bqq4/dropdown-scrollbar-zindex
```

Commits pushed before this handoff:

```text
2cab9dc28aac65334badf8542d14929b385c7758  fix(shell): keep top menus above mailbox scrollbars
```

PR:

```text
https://github.com/cameronzucker/tuxlink/pull/423
```

This handoff file is expected to be committed and pushed as a small follow-up after the implementation commit.

---

## Working-Tree State

Current issue worktree:

```text
/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-bqq4-dropdown-scrollbar-zindex
```

At handoff creation time:

- Branch: `bd-tuxlink-bqq4/dropdown-scrollbar-zindex`
- Tracked dirty: this handoff file only, pending follow-up commit
- Untracked: none before this handoff file was added
- Gitignored/restorable: `node_modules/` and `node_modules/.vite/vitest/results.json` from the offline install/test run; `node_modules/` is documented in `dev/state-paths.md` as restorable with `pnpm install`
- Stashes: no session stash created; pre-existing global stash entries remain untouched

Main checkout was not used for edits. It already had unrelated dirty state:

```text
## bd-tuxlink-xygm/recover-handoffs...origin/bd-tuxlink-xygm/recover-handoffs
M  .beads/issues.jsonl
?? dev/handoffs/2026-06-05-magpie-isthmus-gorge-gps-foundation-tasks-1-5.md
?? docs/design/2026-06-05-gps-setup-ux-design-addendum-r2-r4.md
?? docs/design/2026-06-05-gps-setup-ux-design.md
?? docs/design/mockups/2026-06-04-gps-setup-mocks.html
?? docs/superpowers/plans/2026-06-05-gps-setup-bd-1-plan-v2.md
?? docs/superpowers/plans/2026-06-05-gps-setup-bd-1-plan.md
```

There are many pre-existing worktrees in this checkout. This session created and modified only:

```text
/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-bqq4-dropdown-scrollbar-zindex  [bd-tuxlink-bqq4/dropdown-scrollbar-zindex]
```

After PR #423 merges, dispose this worktree via ADR 0009 inventory/archive/remove/prune ritual. Do not use `git worktree remove`.

---

## bd State

`bd show tuxlink-bqq4`:

```text
tuxlink-bqq4  CLOSED
Close reason: Fixed in 2cab9dc: isolated content panes below top-app chrome so message-list scrollbars cannot overlay dropdowns.
```

`bd stats` at session end:

```text
Total Issues: 484
Open: 137
In Progress: 158
Blocked: 32
Closed: 189
Ready to Work: 105
```

`bd dolt push` was attempted per session protocol. No Dolt remote is configured, so bd reported "No remote is configured - skipping."

---

## Open Decisions

No open implementation decisions. PR #423 needs normal review/CI handling.

---

## Reminders

- PR #423 should merge as a merge commit only; squash merge remains banned by ADR 0010.
- The fix intentionally avoids `AppShell.tsx` and `AppShell.test.tsx` because active PRs #417 and #419 are touching those areas.
- Active PRs checked before editing: #416, #417, #418, #419, #420. The implementation stayed in `AppShell.css`, `chrome.css`, and `MenuBar.test.tsx`.
- RADIO-1 remains in force: no automation or tests may initiate transmissions without explicit scoped consent.
