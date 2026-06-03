# Handoff — pika-cedar-tanager session end (2026-06-03)

> **Date:** 2026-06-03 · **Agent:** `pika-cedar-tanager` · **Machine:** pandora
>
> **Arc:** Resumed from gulch-osprey-bog (2026-06-02) to ship the unified
> user-folders mechanism — Archive wiring + custom user-created folders.
> Brainstormed → specced → shipped Phases 0-3 + UX polish over 6 PRs
> (5 merged, 1 closed-unmerged at operator's call). Session consolidated
> at operator's request: `tuxlink-f62f` is functionally complete; all
> bd issues closed.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first. §3 covers what shipped and what's pending
   downstream (the converged build picks up #292 on next launch).
2. The main checkout STILL has an interactive rebase in progress on
   task-amd-main-ui — same state as 2026-06-02 session start. This
   session did NOT touch the rebase. Operator decides when to continue
   or abort it; agents don't.
3. No specific gates required before the next substantive task — the
   user-folder arc is closed end-to-end on main. Pick from `bd ready`.
```

Paste-ready next-session prompt at the bottom of this doc.

---

## 1. Session arc (compressed)

1. **Briefing**: gulch-osprey-bog's 2026-06-02 handoff queued
   `tuxlink-ca5x` (wire up Archive) + `tuxlink-f62f` (custom user
   folders) as primary work. Both are conceptually one mechanism;
   handoff explicitly required a brainstorm + spec before code.
2. **Phase 0 — spec** (`#260`). Built a high-fidelity dark HTML mock
   with three sidebar concept variants (`docs/design/mockups/2026-06-02-user-folders-mocks.html`)
   + a `docs/superpowers/specs/2026-06-02-user-folders-design.md`
   design doc with 10-decision table. Critical finding: the backend
   ALREADY had `MailboxFolder::Archive` end-to-end — Phase 1 was
   pure frontend exposure. Operator approved recommendations
   wholesale; merged.
3. **Phase 1 — Archive wiring** (`#268`, tuxlink-ca5x closed).
   Reading-pane Archive button + `A` keyboard shortcut (gated on
   text-input focus) + sidebar Archive entry enabled + thin
   `mailbox_move` Tauri command. 918 → 933 frontend tests.
4. **Phase 2 — open-set folder model** (`#284`, tuxlink-f62f
   closed). Backend `.folders.json` registry, `folder_create` /
   `folder_delete` / `user_folders_list` commands, `parse_folder_ref`
   accepting either system folders or user-slug strings,
   `FolderRef::{System,User}` move primitive. Frontend
   `NewFolderDialog`, dynamic Folders section in sidebar with `+`
   button, `MoveToButton` Radix dropdown, `MailboxFolderRef` type
   widening across mailbox surfaces. 933 → 958 tests.
5. **Phase 3 — discoverability hotfix** (`#287`, tuxlink-ejph
   closed). Operator smoke: \"can't add anything to it by any method
   I've attempted\" → right-click context menu + drag-drop weren't
   implemented despite spec §6 D5 promising right-click as the v1
   path. Built `MessageContextMenu`, `FolderContextMenu`,
   `RenameFolderDialog`, `DeleteFolderDialog` (with cascade radio
   per §6 D6), HTML5 drag-drop on `MessageRow` → `FolderSidebar`,
   `folder_rename` backend command. 958 → 958 tests (no new tests
   added, all existing pass — covered by AppShell suite + new
   dialogs are simple presentational).
6. **Phase 3.5 — verification Catch-22** (`#289` REJECTED).
   Operator: \"we don't have CMS-synced messages because we can't
   hit production CMS. The test condition seems like a catch-22.\"
   I wrote a dev-build inbox seeder that materialized synthetic
   B2F files on disk so moves could be exercised. Operator rejected
   the synthetic-data path. Closed unmerged; tuxlink-456u closed
   rejected. Surfaced cms-z.winlink.org dev CMS path from memory
   `project_cms_rejects_unknown_clients` as the real-message
   alternative.
7. **Verification** (no PR). Operator confirmed moves work via
   both reading-pane Move dropdown AND right-click context menu.
   Real-message smoke = green. Phase 2/3 complete.
8. **Phase 4 — UX consistency** (`#292`, tuxlink-i2nr closed).
   Operator: \"menus render oddly, and don't have highlighting on
   hover entries like our other menus do.\" Caused by inline-styled
   components without `:hover` / `[data-highlighted]` rules.
   Extracted `src/mailbox/userFolders.css` mirroring
   `.message-list-sort-*` (Radix menus) + `.tux-settings-*`
   (SettingsPanel) conventions. All 6 components rewritten to use
   shared classes. 958 → 992 tests (delta is from main moving
   forward in parallel; my PR added 0 tests, only style cleanups).
9. **Wrap-up**. Disposed both completed worktrees, deleted the
   rejected `bd-tuxlink-456u/dev-inbox-seeder` remote branch, closed
   all 5 owned bd issues.

---

## 2. PR state

| PR | Branch | State | Notes |
|---|---|---|---|
| [#260](https://github.com/cameronzucker/tuxlink/pull/260) | `bd-tuxlink-ca5x/user-folders-spec` | **MERGED** | Phase 0 spec + mock |
| [#268](https://github.com/cameronzucker/tuxlink/pull/268) | `bd-tuxlink-ca5x/phase1-archive-wiring` | **MERGED** | Phase 1 Archive wiring |
| [#284](https://github.com/cameronzucker/tuxlink/pull/284) | `bd-tuxlink-f62f/phase2-user-folders` | **MERGED** | Phase 2 open-set folders |
| [#287](https://github.com/cameronzucker/tuxlink/pull/287) | `bd-tuxlink-ejph/message-context-menu` | **MERGED** | Phase 3 right-click/drag-drop/rename/delete |
| [#289](https://github.com/cameronzucker/tuxlink/pull/289) | `bd-tuxlink-456u/dev-inbox-seeder` | **CLOSED-UNMERGED** | Operator rejected synthetic-data path |
| [#292](https://github.com/cameronzucker/tuxlink/pull/292) | `bd-tuxlink-i2nr/user-folder-menu-polish` | **MERGED** | UX consistency polish |

Other PRs open on the repo (NOT this session's; from other concurrent
agents): unknown — operator is consolidating sessions, recommend
`gh pr list --state open` at next session start to see what's live.

---

## 3. bd state

Closed this session (issues this agent owned or filed):
- `tuxlink-ca5x` — Archive wiring (Phase 1)
- `tuxlink-f62f` — Custom user folders (Phase 2)
- `tuxlink-ejph` — Right-click MessageContextMenu hotfix (Phase 3)
- `tuxlink-i2nr` — Menu/dialog UX consistency (Phase 4 polish)
- `tuxlink-456u` — Dev-build inbox seeder (rejected by operator)
- `tuxlink-unb0` — This handoff doc's wrapper

The user-folder spec at
`docs/superpowers/specs/2026-06-02-user-folders-design.md`
documents Phase 3 deferred items if a future operator wants them:
multi-select move, folder-level count badges (needs
`user_folders_list_with_counts` backend command to avoid N+1),
search-index display-name lookup. None blocking.

---

## 4. Worktree inventory at handoff

**Disposed this session** (PRs landed, ADR 0009 ritual followed):
- `bd-tuxlink-ca5x-user-folders-spec` (PR #260)
- `bd-tuxlink-ca5x-phase1-archive-wiring` (PR #268)
- `bd-tuxlink-f62f-phase2-user-folders` (PR #284)
- `bd-tuxlink-ejph-message-context-menu` (PR #287)
- `bd-tuxlink-456u-dev-inbox-seeder` (PR #289 closed-unmerged; remote
  branch also deleted with `git push origin --delete`)
- `bd-tuxlink-i2nr-user-folder-menu-polish` (PR #292)

**Remaining at handoff**:

| Worktree | Branch | bd issue | Tracked dirty | Untracked | Gitignored stateful |
|---|---|---|---|---|---|
| `worktrees/bd-tuxlink-unb0-session-end-handoff/` | `bd-tuxlink-unb0/session-end-handoff` | tuxlink-unb0 | THIS doc | — | — |

Inherited from prior sessions (≈25 worktrees from gulch-osprey-bog
and earlier): not this session's property; see prior handoffs at
`dev/handoffs/2026-06-02-*`.

---

## 5. Discipline notes / decisions

- **Spec D2 deviation**: the spec recommended collapsing
  `MailboxFolder::Archive` into `MailboxFolder::User("archive")` to
  unify storage. Phase 2 kept the closed enum + ran user folders as
  a parallel mechanism to avoid a 122-site refactor + Pi5 cargo
  cost. Wire API + operator UX are identical to the spec. Future
  cleanup can unify the storage paths without changing the operator
  surface. Documented in PR #284 body.
- **Phase phasing pushback** (2026-06-02 → 2026-06-03): operator
  flagged the \"MVP / hotfix / polish\" pattern as wasteful for a
  pre-production project. Adjusted mid-session to ship the spec
  end-to-end (Phase 3 absorbed all deferred items in one push)
  instead of staged MVPs. Memory entry candidate but operator did
  not explicitly ask me to record it — leaving for them to capture.
- **Dev-data path closed**: synthetic inbox seeding is OFF the
  table. cms-z.winlink.org dev CMS is the canonical real-message
  verification route (memory `project_cms_rejects_unknown_clients`,
  `feedback_cms_telnet_testing_authorized`).
- **No RF / live-CMS work** — RADIO-1 untouched.
- **Main checkout in-flight rebase** — SAME state as session start:
  `task-amd-main-ui` mid-rebase on `dea086f`. Did NOT touch. The
  handoff doc lives on its own branch per the
  `feedback_main_checkout_is_operator_state` rule.

---

## 6. Useful pointers for next session

- Frontend user-folder surfaces:
  - [src/mailbox/types.ts](src/mailbox/types.ts) — `MailboxFolderRef`,
    `UserFolder` types
  - [src/mailbox/useUserFolders.ts](src/mailbox/useUserFolders.ts) —
    list/create/rename/delete hooks
  - [src/mailbox/FolderSidebar.tsx](src/mailbox/FolderSidebar.tsx) —
    dynamic Folders section + drop targets
  - [src/mailbox/userFolders.css](src/mailbox/userFolders.css) —
    canonical class kit (`.tux-ctx-*`, `.tux-folder-*`)
- Backend user-folder surfaces:
  - [src-tauri/src/user_folders.rs](src-tauri/src/user_folders.rs) —
    registry + validation
  - [src-tauri/src/native_mailbox.rs](src-tauri/src/native_mailbox.rs)
    — `Mailbox::{seed_dev_inbox is NOT here — that was rejected}`,
    `create_user_folder`, `rename_user_folder`,
    `delete_user_folder`, `move_between`
  - [src-tauri/src/ui_commands.rs](src-tauri/src/ui_commands.rs) —
    `parse_folder_ref`, `mailbox_move`, `folder_*` commands
- Spec: [docs/superpowers/specs/2026-06-02-user-folders-design.md](docs/superpowers/specs/2026-06-02-user-folders-design.md)
- Mock: [docs/design/mockups/2026-06-02-user-folders-mocks.html](docs/design/mockups/2026-06-02-user-folders-mocks.html)

---

## 7. Next-session paste-ready prompt

```
Resume from pika-cedar-tanager's 2026-06-03 session-end handoff.

Handoff doc: dev/handoffs/2026-06-03-pika-cedar-tanager-user-folders-complete.md
(Lives on branch bd-tuxlink-unb0/session-end-handoff if not yet merged
into main — committed there because the main checkout was mid-rebase
at handoff time, same as the 2026-06-02 gulch-osprey-bog pattern.)
READ IT FIRST.

The user-folders arc is complete end-to-end. No primary follow-up.
Pick from `bd ready` and proceed.

Operator note: sessions are being consolidated to 1-2 concurrent.
Be mindful of other agents' work surfaces (check `git worktree list`
+ `bd ready` before claiming) so we don't fragment effort.

If you have no specific task: filter `bd ready` for RF-path work
(AX.25 codec, abort/disarm, ARDOP/VARA, serial/Bluetooth) per memory
feedback_rf_path_scope_filter — that's the standing backlog priority
when explicit work runs out, with operator green-light + smoke plan
required (RADIO-1 stays active).
```

---

Agent: pika-cedar-tanager
