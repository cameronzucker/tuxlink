# Handoff — 2026-05-19 willow-cypress-heron

**From agent:** `willow-cypress-heron`
**Session arc:** Closed the operator-side smoke gap on PR #70 (Task 7 menu bar): retroactively smoked, found File→Quit broken, attempted two wrong fixes (custom + on_menu_event with exit() that lost diagnosis to phantom symptom; PredefinedMenuItem::quit which is silently Linux-unsupported), then ran bug-hunt-cycle skill (3 hunters + Codex adrev), found canonical Linux Quit pattern via 4-way source verification, reverted to it, operator smoke confirmed, PR #71 merged. Session-end batch cleanup: closed 7 stale in_progress bd issues with merged PRs, disposed 14 stale worktrees per ADR 0009, dropped redundant stash, filed bd issue for the webkit2gtk TV-static rendering bug observed during smoke.
**Status:** pushed; integration branch `feat/v0.0.1` at `971cfbb`.

---

## Next session's starting prompt

> I'm resuming the tuxlink project. `willow-cypress-heron` handed off 2026-05-19. Read these before doing anything:
>
> 1. `dev/handoffs/2026-05-19-willow-cypress-heron-session-end.md` — this handoff.
> 2. `CLAUDE.md` — full project rules. Pay attention to `## Tool referee`, `## Documentation propagation contract`, `## Session Completion`, and the worktree sections (ADR 0008 + ADR 0009).
> 3. `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` — the v0.0.1 plan.
> 4. `docs/pitfalls/implementation-pitfalls.md` and `docs/pitfalls/testing-pitfalls.md`.
> 5. Memory entries to internalize before substantive work:
>    - `[[no-atomic-decisions-to-operator]]` — has a 2026-05-19 recurrence note. Read it.
>    - `[[no-ceremony-spiral-on-small-fixes]]` — NEW today. The bug-hunt-cycle skill's later phases were over-applied to a 5-minute fix; operator pushed back hard. Read it.
>    - `[[main-checkout-is-operator-state]]` — NEW today. Don't `git checkout` in the main checkout; reads use `git show`/`git log`, writes use worktrees.
>
> Once read:
>
> - Generate a fresh moniker via `python3 .claude/scripts/get_agent_moniker.py`.
> - Run `bd ready` for available work. Likely candidates: tuxlink-cs7 (Task 17 AppImage, has a pre-impl decision pending), tuxlink-zsm (Task 12 Inbox/Sent), tuxlink-69z (Task 15 Session log pane), tuxlink-nk7 (Task 6 Live-CMS smoke, operator-only).
> - **DO NOT** spend hours debugging the webkit2gtk TV-static rendering symptom — it's filed as `tuxlink-wfw` P3 with mitigation env vars enumerated. Either test one of the mitigations IF the operator wants this resolved before more UI work, or defer it past v0.0.1.
> - Take time on the work; quality over speed.

---

## What landed in this session

| Item | What | PR # | Status |
|---|---|---|---|
| 1 | Task 7 menu bar retroactive smoke (operator-side) | (no PR; smoke against PR #70) | smoke completed |
| 2 | Quit menu fix — canonical Linux pattern (custom MenuItemBuilder + on_menu_event → app.exit(0)) | #71 | **merged** (commits 40a7f1d → 4a0b19a → 888b957, no-squash) |
| 3 | 4-way bug-hunt-cycle: 3 hunters (exploratory/holistic/multipass) + Codex adrev | committed under `dev/bug-hunts/` + `dev/adversarial/` | committed |
| 4 | Consolidated bug-hunt findings doc | `dev/bug-hunts/2026-05-19-tuxlink-r21-quit-menu-consolidated.md` | committed |
| 5 | bd issue `tuxlink-r21` (Quit fix) | — | closed |
| 6 | bd issue `tuxlink-6vi` (Task 7 menu bar parent) | — | closed |
| 7 | Closed 7 stale in_progress bd issues from earlier-session merged PRs: 4mt, 756, 9pb, ln3, 4p2, cvs, ttp | — | all closed |
| 8 | Disposed 14 stale worktrees per ADR 0009 ritual (including the freshly-merged r21 worktree) | — | done |
| 9 | Archived adrev transcripts from ln3 + mib worktrees before disposal | `.claude/worktree-archives/` (per-machine, gitignored) | done |
| 10 | Filed bd `tuxlink-wfw` for the webkit2gtk TV-static rendering bug (Pi 5 / Wayland / Mesa V3D first-frame buffer) | — | open, P3 |
| 11 | Three new feedback memories saved: `[[no-ceremony-spiral-on-small-fixes]]`, `[[main-checkout-is-operator-state]]`, plus a 2026-05-19 recurrence note appended to `[[no-atomic-decisions-to-operator]]` | `~/.claude/projects/-home-administrator-Code-tuxlink/memory/` | persisted |

---

## State at pause

### What's pushed to origin

```
feat/v0.0.1  971cfbb  (merge of PR #71 — fast-forwarded from d9f7d14 + the 3 r21 commits)
main         86ddd3d  (unchanged this session — ancient; feat/v0.0.1 is the active integration branch)
task-amd-main-ui (local-only; remote ref was deleted some time before this session; harmless)
```

### Working-tree state

Run from the main checkout (`/home/administrator/Code/tuxlink`):

- `git status --short` — `MM .beads/issues.jsonl` (bd auto-export from this session's close operations; will be picked up by `bd dolt push` at the end of this handoff commit batch)
- `git ls-files --others --exclude-standard` — `dev/scratch/` (gitignored research notes from earlier sessions; intentional)
- `git stash list` — empty (the willow-cypress-heron stash with redundant SCOPE-1 dups was dropped this session after verifying redundancy against feat/v0.0.1 commit `9b8d138`)

### In-flight worktrees (per ADR 0009)

**No worktrees in flight.** All 14 prior worktrees disposed this session:

```
git worktree list
# /home/administrator/Code/tuxlink  3b8f5ac [task-amd-main-ui]
```

Disposal-ritual content disposition for the 14 worktrees:
- **4mt, 6vi, 756, r21, z5f**: 4000-7000 gitignored files each (cargo target/ + node_modules/ build artifacts; reproducible from the lockfiles and source; discarded without archive).
- **4p2, z5f**: 1 untracked file each = `dev/scratch/` (duplicate of main-checkout's; intentional discard).
- **ln3, mib**: 9-12 gitignored adrev transcripts in `dev/adversarial/`; ARCHIVED to `.claude/worktree-archives/bd-tuxlink-{ln3,mib}-adversarial-<TS>.tar.gz` before disposal (operator-machine-local, per ADR 0009).
- **54p, 9pb, cvs, cyy, gdo, ttp**: clean (no tracked-dirty, no untracked, no non-build gitignored, no stashes); discarded without archive.

### bd state

```
$ bd stats
(refresh at end of session — see "Final push + verify clean" step)
```

In-progress issues claimed by this session: NONE (tuxlink-r21 was claimed by this session via `new_tuxlink_worktree.py`; closed earlier in the session post-merge).

Ready-for-pickup issues (`bd ready`) for the next session:

| Issue ID | Title | Notes |
|---|---|---|
| tuxlink-cs7 | Task 17 AppImage packaging | Has a pre-impl decision pending: fetch fork's Pat binary (post-cred-refactor) vs la5nta upstream. Surface to operator. |
| tuxlink-zsm | Task 12 Inbox/Sent tabbed view | UI design ceremony; blocks Task 13 + Task 14. |
| tuxlink-69z | Task 15 Session log pane | UI; consumes z5f's PatBackend.stream_log surface. |
| tuxlink-nk7 | Task 6 Live-CMS smoke | Operator-only per Part 97 consent gate; needs adaptation against PR #68's PatSpawnOptions.tuxlink_config. |
| tuxlink-wfw | webkit2gtk TV-static (NEW) | P3 polish; mitigation env vars enumerated in issue body. |

---

## Open decisions for the next agent or Cameron

1. **tuxlink-cs7 pre-impl Pat-binary decision** — fetch tuxlink-pat fork's post-cred-refactor binary, or la5nta upstream pat. Context: ADR 0011 forked Pat to enable cred-refactor; the fork's binary is the production target. Surface to operator at task start; do NOT default-decide this.

2. **tuxlink-wfw priority** — does this block UI work (front-load the env-var mitigation), or defer past v0.0.1 (cosmetic; release config can bake in `WEBKIT_DISABLE_DMABUF_RENDERER=1`)? Surface to operator IF the next agent starts more UI work (Task 12/13/14/15/16); skip the question if the next session goes to Task 17 or Task 19 (non-UI).

---

## Plan amendments queued

None this session. The Quit-menu fix path doesn't amend any plan section — Task 7's plan body was specific enough that the fix is just "implement the canonical Tauri 2 Linux Quit pattern that Tauri's own docs example uses."

---

## Reminders for the next agent

- bd directives in `<!-- BEGIN BEADS INTEGRATION -->` are overridden by `## Tool referee` in CLAUDE.md (per ADR 0006). Use TodoWrite for in-turn working memory; auto-memory at `~/.claude/projects/...` for cross-session knowledge.
- Per-task-branch wrap: branch off `feat/v0.0.1` → commit → push → PR (`gh pr create --base feat/v0.0.1`) → `gh pr merge --merge --delete-branch` (NOT `--squash`) → `git pull --ff-only origin feat/v0.0.1` → `bd close` if a bd issue was claimed.
- **DO NOT `git checkout <branch>` in the main checkout.** Reads use `git show <branch>:<file>`, `git log <branch>`, `git diff <branch>...HEAD`. Writes use a worktree spawned via `.claude/scripts/new_tuxlink_worktree.py --issue <bd-id> --slug <slug>`. Then `EnterWorktree path=worktrees/<id>` to actually move the session cwd (a bare `cd` in a bash command doesn't update Claude Code's session state, and the `block-main-checkout-race.sh` hook reads session cwd). See `[[main-checkout-is-operator-state]]`.
- **PredefinedMenuItem on Linux is a footgun.** 12 of the 16 PredefinedMenuItem variants are `Linux: Unsupported` (Quit, Hide, Minimize, etc.) and silently no-op via muda's GTK allowlist. For any new menu item, use custom `MenuItemBuilder::with_id(...)` + on_menu_event handler. Operator-side `pnpm tauri dev` smoke is the ONLY adequate verification — `cargo test` + `cargo build` will both pass for menu items that don't render. The closest analog memory: `[[browser-smoke-before-ship]]`, extended to native menus.
- **No ceremony spiral on small fixes** (`[[no-ceremony-spiral-on-small-fixes]]`): when a bug-hunt-cycle identifies root cause + a clear single-function canonical fix, just apply the fix. Don't write a 400-line remediation plan + new pitfall docs + introspection-test scaffolding for a 5-minute code change. Skills calibrate in proportion to work scope.
- **The webkit2gtk TV-static is real but separate** (tuxlink-wfw): every Tauri window launch on this Pi shows uninitialized GPU memory ("TV static") until first interaction. Has been there since the Tauri scaffold (commit 52d4181); not introduced by recent work. If you notice it during smoke, don't spiral — just note + click into the window to dismiss.

---

**If something in this handoff looks wrong tomorrow:** the previous-session agent (`willow-cypress-heron`) made multiple wrong assumptions early in the session (branch state, runtime behavior of `app.exit(0)`, PredefinedMenuItem cross-platform support) before the bug-hunt-cycle delivered ground truth. Flag any claim that doesn't smell right and verify it before acting. Source of truth for any rule that this handoff restates: ADRs and CLAUDE.md per the propagation contract.
