# 2026-07-17 — fox-cypress-pika: v0.92.1 hotfix, Routines fidelity pass, overnight demo polish

## What happened (one session, three phases)

**Phase 1 — P0 launch crash (tuxlink-j1f30, CLOSED).** v0.92.0 did not launch
anywhere: `RoutinesScheduler::spawn` did a bare `tokio::spawn` and its one
production caller is `lib.rs` `.setup()` (plain main thread, no ambient
runtime). Fixed with a `Handle::try_current()` probe falling back to
`tauri::async_runtime` (the 18 `start_paused` scheduler tests keep their
virtual-time runtimes). Regression test committed red-first; red/green proven
on R2. PR #1132 merged, released as the **v0.92.1 nightly pre-release**
(release-merge.yml dispatched off-cadence, operator-approved), deb installed
on the R2 (checksum-verified). Only v0.92.0 carried the bug.

**Phase 2 — Routines fidelity pass (operator-requested).** Compared shipped
Routines against the approved mocks (`dev/scratch/routines-ui-mocks/`, PNGs =
approved artifacts). Report + before/after captures:
`dev/scratch/routines-fidelity-2026-07-17/REPORT.md`. Score: 2 findings fully
real (type scale collapsed, navigation dead-end), 1 half-real (accent colors
were the operator's theme, but literal alpha derivations didn't re-theme and
`--danger-surface` doesn't exist), 1 retracted (consent ambient layer exists —
tuxlink-ecijj filed and closed-invalid by this session), 1 reframed (raw
palette ids are IN the approved mock — 5lfxk is a design change, not drift).

**Phase 3 — overnight demo polish (operator-authorized "on your own
recognizance").** PR #1138, branch `bd-tuxlink-9se1x/routines-demo-polish`,
7 commits (incl. handoff), closing:

- **tuxlink-9se1x**: "← Mailbox" header button, Escape (dashboard-only,
  guarded against `[role="dialog"]` / `[role="menu"]` / typing surfaces;
  deliberately inert in the designer to protect unsaved drafts), Routines →
  "Back to Mailbox" menu item (MenuBar-gated like dockback), titlebar +
  window title track the surface.
- **tuxlink-5lfxk**: `label` + `description` on `ActionDescriptor` (17
  production impls, copy grounded in each action's doc comment) → `ActionInfo`
  DTO → palette (label + mono id subtext + tooltip; filter matches either) →
  step inspector → canvas node titles. Per-module descriptor tests assert
  non-empty copy on shipped actions.
- **tuxlink-h82wg**: all 9 Routines stylesheets retokenized (124 sizes →
  `--type-*` by role, 61 colors → token `color-mix`); fixed failed-run gantt
  bars referencing nonexistent `--danger-surface`.

## Verification (provenance)

- R2 (rustup 1.96, `--locked`, detached at branch head): 282 routines tests
  green, clippy `--all-targets -D warnings` clean.
- Full local vitest 4530 green at HEAD (one mid-run-edit artifact and one
  contended-Pi flake investigated and cleared; targeted suites 7/7, 255/255).
- Live on R2 display :1, isolated `$HOME`, seeded 4-routine fleet, embedded
  debug build (`pnpm tauri build --debug --no-bundle`): every fix verified on
  screen (captures `20-22*.png`).
- **Trap for future agents:** a plain `cargo build` debug binary does NOT
  embed `../dist` — it loads `devUrl` (localhost:1420) and will silently
  render whatever vite happens to be listening (here: the operator's
  converge-build worktree = origin/main). Provenance-check renders via a UI
  affordance unique to your branch.
- Codex adversarial round (GPT-5.5, stdin pattern): one P3 (Escape vs open
  row menu), fixed + regression-tested. Transcript local:
  `dev/adversarial/2026-07-17-routines-demo-polish-codex.md`.
- Formal wire-walk with operator-supplied greenfield flows was NOT run
  (operator asleep); the operator's three complaints served as the flow set,
  each traced to file:line and verified live on screen. Flag if this
  disposition is unsatisfying.

## State at handoff

- **PR #1138**: CI green on both arches except one amd64 `verify` job that
  died as a runner-infra failure (every step `conclusion: null`) and was
  re-run; merge attempted on green (see PR for final state). Merge may have
  required the operator (auto-mode classifier blocks self-merge; the earlier
  P0 merge needed an explicit operator answer).
- **bd**: tuxlink-j1f30 closed. 9se1x / 5lfxk / h82wg close on merge.
  tuxlink-ecijj closed invalid.
- **Worktrees**: `worktrees/bd-tuxlink-9se1x-routines-demo-polish` (this
  branch; disposable per ADR 0009 after merge — only node_modules is
  gitignored-on-disk). The j1f30 worktree was disposed. Other live sessions'
  worktrees (niiug, 7raoe, ant8s main-checkout state, d8f3l) untouched.
- **Repo-wide stashes**: 7 old stashes from May–June sessions exist
  (`git stash list`) — other sessions' state, not cleared.
- **Local branch** `bd-tuxlink-j1f30/fix-scheduler-spawn` is merged but
  undeletable from this session (branch ops on the main checkout are
  hook-blocked while other sessions live); remote already deleted.
- **R2**: `~/Code/tuxlink` left detached at the polish branch head;
  `/tmp/fidelity*` scratch (captures, drive scripts, seeded HOME) is
  disposable. tuxlink 0.92.1 deb installed. The operator's converge vite +
  debug instance were not touched.
- **Not in scope, still open**: the deeper constructor UX redesign (palette
  grouping/discoverability beyond labels), the mock's ambient running-count
  statusbar item (noted in REPORT.md F4, not filed), radio.rs/cat.rs have 6
  production descriptors with no descriptor()-asserting tests (label guard
  covers 11 of 17).

## Operator decisions pending

1. Merge PR #1138 if the classifier blocked the overnight attempt.
2. Whether to cut a 0.92.2 nightly (off-cadence) with the polish, or let the
   14:00 UTC cron batch it.
3. Whether the constructor needs the deeper design round (mocks first) beyond
   tonight's label/size/navigation work.
