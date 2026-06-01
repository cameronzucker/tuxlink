# Handoff — 2026-05-31 — peregrine-maple-thistle — HTML Forms v0.1 (PR #177) + 3 follow-up PRs + 3 bd-hygiene closes

> Date: 2026-05-31 · Agent: peregrine-maple-thistle · bd: tuxlink-v1p (primary) · Machine: pandora · Worktrees: 3 active this session (see §5)

## 0. TL;DR

Picked up the HTML Forms v0.1 work mid-execution (per the prior `shoal-isthmus-swallow` handoff), shipped the whole plan including Phase 9 (which the prior session deferred) once the operator pointed me at the gitignored WLE install archive. **3 PRs landed for review, 3 bd issues closed via hygiene sweep (silently-fixed-but-never-closed).**

| Artifact | Status |
|---|---|
| PR [#177](https://github.com/cameronzucker/tuxlink/pull/177) — HTML Forms v0.1 | OPEN — ready for review |
| PR [#178](https://github.com/cameronzucker/tuxlink/pull/178) — search subject (tuxlink-g4dj) | OPEN — ready for review |
| PR [#179](https://github.com/cameronzucker/tuxlink/pull/179) — pitfalls TEST-1/DISCOVERY-1/SCHEMA-1 (tuxlink-gyu6) | OPEN — ready for review |
| bd close `tuxlink-o7d4` (clippy debt) | CLOSED — resolved incidentally by #177 |
| bd close `tuxlink-eh7` (wizard completion dead-end) | CLOSED — already fixed in PR #116 (1d1c01b, 2026-05-22) but never closed |
| bd close `tuxlink-mnk4` (ARDOP HF dock dead-end) | CLOSED — already on `origin/main` as `aa8e6ad` (2026-05-30) but never closed |
| bd new `tuxlink-ws45` | OPEN — Phase 9 v0.1.1 follow-ups (mostly absorbed; some lookahead remains) |
| bd new `tuxlink-gyu6` | CLOSED via #179 |
| bd note on `tuxlink-7gb` | DEFERRED — premise wrong (docs/development.md doesn't exist; needs operator scope decision) |

The next session can either review the 3 open PRs or start a fresh bd-ready pick from the audited backlog (§7).

## 1. PR #177 — HTML Forms v0.1 (branch `bd-tuxlink-v1p/html-forms-execution`)

### What ships

- **Phases 0–8 + 10 + 11** of plan rev-4, **PLUS Phase 9** (the prior session had deferred Phase 9 because the WLE Standard Templates source path was empty; this session located the operator's WLE install archive at `.claude/worktree-archives/RMS-personal-install-20260518T073146Z.zip` (74 MB) and shipped all 4 additional forms verbatim).
- 5 forms total: ICS-213, Form-309 (ICS-309 Communications Log), Bulletin, GPS Position Report, Damage Assessment.
- **Codex round 1** (7302-line transcript): 6 findings; 5 applied (P1 #1 onChange lifting, P1 #2 disable global send in form mode, P2 #3 `trim_text(false)`, P2 #4 `Event::GeneralRef` entity decode, P2 #5 backfill `payload.form_id`); P2 #6 (Reply-with-form button) forward-rolled.
- **P2 #6 fix** applied in commit `5350809`.
- **Codex round 2** (12127-line transcript on Phase 9 + P2 #6): 3 P2 findings, all applied in `fd7e373`:
  - P2 #1: gate Reply-with-form button to forms with explicit reply mappings (`hasReplyWithFormSupport(formId)`; currently only ICS-213).
  - P2 #2: ICS-309 entryCount scans all 4 entry fields × 30 rows when restoring drafts.
  - P2 #3: scrub WLE system placeholders (`<MsgSender>`, `<ProgramVersion>`, `<var Templateversion>`) from Phase 9 body templates (they had no `field_values` source).
- **Workspace clippy cleanup** in `1bfb7d6` (17 lints across 9 files) — **first clean clippy run on the branch**. Resolves `tuxlink-o7d4` incidentally.

### Verification on the branch

- Rust workspace: 572 tests passed.
- `tsc --noEmit`: clean.
- Vitest: 183 passed across `src/{mailbox,forms,compose}/`; full repo vitest ~600+ passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: `EXITCODE=0`.

### Commit count

35 commits ahead of `origin/main` (was 30 at first PR-open; +5 from Codex r1 fix, Codex r2 fixes, clippy cleanup, Phase 9, P2 #6).

### Codex transcripts (gitignored)

- `dev/adversarial/2026-05-31-html-forms-post-impl-codex.md` (round 1)
- `dev/adversarial/2026-05-31-html-forms-phase9-codex-r2.md` (round 2)

## 2. PR #178 — search subject populated (branch `bd-tuxlink-g4dj/search-subject`)

Bug: search results rendered with empty subject because `messages_meta` had no `subject` column — only `messages_fts` did, which is index-only.

Fix in one commit (`92626a0`):
- `SCHEMA_VERSION` 1 → 2 (drift triggers operator-driven rebuild).
- Add `subject TEXT NOT NULL DEFAULT ''` to `messages_meta`.
- Plumb through `upsert` / `QueryHit` / `query()` row mapping (column index shift +1 everywhere) / `hit_to_dto`.
- 1 test bumped (`open_detects_schema_drift` expects current=2); 1 new test added (`subject_round_trips_through_messages_meta`).

**Migration:** operators with existing v1 indices return SchemaDrift on next launch; run `tauri_search_rebuild_index` to recreate from mbox (mbox is authoritative).

**Workspace clippy** on this branch returns the pre-existing 14 lints (it's based on `origin/main` which doesn't have #177's clippy cleanup yet). Will clear once #177 lands. PR body notes this.

## 3. PR #179 — pitfalls additions (branch `bd-tuxlink-gyu6/pitfalls-html-forms-session`)

3 new entries to `docs/pitfalls/implementation-pitfalls.md` capturing patterns surfaced this session:

| ID | Title | Surfaced in |
|---|---|---|
| TEST-1 | Filesystem-scan tests in Vite projects must use `import.meta.glob`, not Node `fs` | PR #177 commit 2d8fa1f |
| DISCOVERY-1 | Gitignored references live in `.claude/worktree-archives/`, not just disk | PR #177 Phase 9 |
| SCHEMA-1 | `SCHEMA_VERSION` bump triggers operator-driven rebuild, not silent `ALTER TABLE` | PR #178 |

Pure docs change. +105 −1 to `docs/pitfalls/implementation-pitfalls.md` (676 → 780 lines). Wired into ToC + Tool-Integration checklist + Appendix B summary table + Appendix A historical changelog per the doc's "How to Add a Pitfall" discipline.

## 4. bd-hygiene closes — silently-fixed-but-not-closed sweep

Discovered the pattern when checking `tuxlink-eh7` to start work on it — the code already had the fix from a prior commit. Swept the rest of `bd ready` looking for similar patterns. Three closed:

| bd id | Fixed by | Notes |
|---|---|---|
| `tuxlink-o7d4` | This PR #177 (clippy cleanup commit `1bfb7d6`) | Was about origin/main test+clippy broken on `arq_bandwidth_hz` + SortOrder. The InitConfig literals were updated separately; the SortOrder fix is in this PR. Both broken paths now green. |
| `tuxlink-eh7` | Commit `1d1c01b` in PR #116 (2026-05-22) | "fix(wizard): style the first-run wizard + wire completion hand-off (tuxlink-dj6, tuxlink-eh7)." Author landed both wizard issues in one commit but only closed the `dj6` bd. |
| `tuxlink-mnk4` | Commit `aa8e6ad` by `crag-hemlock-kestrel` (2026-05-30) | "fix(shell): ARDOP HF dock dead-end on cold start + wire View → Toggle Radio Dock." Commit is on `origin/main` already. I briefly pushed a stale local branch then deleted it once I realized. |

**Lesson for future agents:** when starting a bd-ready issue, first `grep` the commit graph for `<bd-id>` references — if a commit already mentions it, verify the code state before re-implementing. Documented in `DISCOVERY-1` style; could promote to a new pitfall in a future session if the pattern recurs.

## 5. Worktree state at session end

Active worktrees that played a role this session:

| Worktree | bd issue | Branch | State |
|---|---|---|---|
| `worktrees/bd-tuxlink-v1p-html-forms-execution/` | tuxlink-v1p | `bd-tuxlink-v1p/html-forms-execution` | LIVE — 35 commits ahead, PR #177 open + ready |
| `worktrees/bd-tuxlink-g4dj-search-subject/` | tuxlink-g4dj | `bd-tuxlink-g4dj/search-subject` | LIVE — 1 commit ahead, PR #178 open + ready |
| `worktrees/bd-tuxlink-gyu6-pitfalls-html-forms-session/` | tuxlink-gyu6 | `bd-tuxlink-gyu6/pitfalls-html-forms-session` | LIVE — 1 commit ahead, PR #179 open + ready |

### Stale / anti-pattern found

- `worktrees/bd-tuxlink-o3f2-ardop-abort-connect/` is checked out on branch `bd-tuxlink-mnk4/ardop-dock-cold-start` (not o3f2's branch). The mnk4 commit on it is already on `origin/main` (see §4). This is the anti-pattern where the worktree path doesn't match its current bd-issue claim. Probably worth disposing in a future session per the ADR 0009 ritual once the operator confirms the worktree isn't otherwise in use. **DID NOT DISPOSE this session** — owned by another agent's prior context; want operator concurrence before doing destructive work on it.

### Worktrees created+disposed this session

- `bd-tuxlink-eh7-wizard-complete-handoff` — created empty, disposed immediately when I realized the bug was already fixed. Branch deletion attempted; remote never had it. Local branch `bd-tuxlink-eh7/wizard-complete-handoff` may still exist on this machine.

### Untracked / gitignored-stateful content per ADR 0009 §"Handoff documents enumerate worktree state"

- All 3 live worktrees have `src-tauri/target/` build artifacts.
- `bd-tuxlink-v1p-html-forms-execution/dev/adversarial/` contains the 2 Codex transcripts (~19400 lines total). Gitignored, archive-only.
- `bd-tuxlink-v1p-html-forms-execution/node_modules/` populated.
- Other worktrees' `node_modules/` populated.

## 6. Memory updates this session

None added to auto-memory yet. Candidate entries from this session's learnings (operator may want me to record):

- **feedback_bd_hygiene_sweep_pattern**: when starting a bd-ready issue, first `git log --all --grep="<bd-id>"`. Three of the four issues I picked this session were already fixed but not closed. Documented as a behavior reminder.
- **feedback_archive_dir_discovery**: when a gitignored reference path resolves empty on disk, check `.claude/worktree-archives/` for compressed materials before deferring (codified as `DISCOVERY-1` pitfall in #179).
- **feedback_phase9_subagent_unwound_codex_fixes**: Codex round 2 caught 3 P2 issues the bundled Phase 9 subagent didn't self-flag: forgot to scrub WLE system placeholders, narrow entryCount derivation, button gate-vs-mapping mismatch. Confirms `feedback_codex_post_subagent_review`'s discipline value.

None of these are critical to record now — `feedback_codex_post_subagent_review` already covers the bias point, and the bd-hygiene + archive-discovery are pitfall-coded.

## 7. What's pending decision / pending review

### Operator post-merge actions (PR #177)

- **T11.2–T11.5 live cross-client smokes** (operator-only per CLAUDE.md RADIO-1): Tuxlink ↔ WLE and Tuxlink ↔ Pat, both directions. Spot-check one Phase 9 form too if time (Bulletin or Position are smallest).
- **Browser smoke** of Compose form flow: launch `pnpm tauri dev` (mind the :1420 port collision), walk DEV-3 fixture inbound render + new-ICS-213 compose, verify Codex r1 P1 #1 onChange round-trip + r2 P2 #2 ICS-309 entry restore.

### Operator post-merge actions (PR #178)

- Run `tauri_search_rebuild_index` once on the dev DB so the local v1 index migrates to v2.

### bd `ws45` (Phase 9 v0.1.1 follow-ups)

- Per-form reply mappings for ICS-309 / Bulletin / Position / Damage Assessment (currently `Reply with form…` button is gated off for those; per-form swap semantics need design thought).
- Catalog browser UI (let users see what forms are bundled without opening Compose).

### Other bd ready picks (audited but unfixed)

- `tuxlink-7gb` (docs/development.md refresh) — premise wrong; commented + deferred.
- `tuxlink-9ky` P1 hardware blocker (Pi BT to UV-Pro EHOSTDOWN). Not addressable software-side.
- `tuxlink-0ja` P1 abort-write TOCTOU. Real bug, requires AbortableByteLink rework. ~60-90 min.
- `tuxlink-1637` P2 ARDOP PINGACK/PING event parsing. ~30-60 min.
- `tuxlink-9h8` P2 register tuxlink client SID with Winlink. External-coordination required.
- `tuxlink-b0i` P2 AX.25 Path::encode C/R bit threading. Protocol correctness. ~60 min.
- `tuxlink-5vx` / `tuxlink-7fr` P1 AX.25 1200-baud headline feature. Too large for one session.

### Sticky reminders

- `feedback_no_carveout_on_cross_provider_adrev`: any future hard-to-undo decision needs a Codex round. PR #177 had two rounds + applied 8/9 findings; PR #178 was plumbing-class (carveout per `feedback_discipline_triage_rule`); PR #179 was docs-only.
- `feedback_codex_quota_gotcha`: the 2 Codex rounds were both ~7-12k lines. Quota is still good but worth keeping an eye on.
- `feedback_main_checkout_is_operator_state`: never `git checkout` a branch in the main repo; always worktrees.

## 8. Critical first action for the next session

The next session should NOT just `bd ready` and pick the top item. There are 3 open PRs awaiting review (#177, #178, #179) — the operator may want them landed first so the next branch is based on the latest main.

**Order of operations next session:**

1. Read this handoff doc top-to-bottom.
2. Check PR review state: `gh pr list --state open --base main`. If #177 / #178 / #179 are still OPEN, ask operator whether to (a) wait for review, (b) pick from a different branch base, or (c) attack a bd issue that doesn't conflict with the open PRs' file footprints.
3. Per `feedback_bd_hygiene_sweep_pattern` (informal): for any bd-ready candidate, `git log --all --grep="<bd-id>"` BEFORE creating a worktree. If a commit mentions the id, inspect it — the issue may already be fixed.
4. Per `feedback_decisive_autonomous_execution`: don't wrap up early; chip the backlog.

---

Agent: peregrine-maple-thistle
