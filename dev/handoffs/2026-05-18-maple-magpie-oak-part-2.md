# Handoff — 2026-05-18 maple-magpie-oak PART 2 — wizard cluster spec/plan + 3 supporting PRs (5 merged, 2 open, plan-review surfaced critical Task 2 impl gap)

**From agent:** `maple-magpie-oak` (parent; multiple subagent dispatches for adrev rounds + Phase 10 mechanical work; subagent monikers inherited per CLAUDE.md)
**Session arc:** Resumed from `shoal-condor-clover` 2026-05-18 handoff. Operator merged PRs as they shipped; pace energy was "push through with remaining context." Sequence: Phase 10 (PR-59 merged) → AMD-11..14 cluster (PR-60 merged) → libsecret-1 docs (PR-61 merged) → wizard-cluster spec full build-robust-features pipeline (5-round adrev incl Codex; PR-62 merged) → post-cred-handling docs cleanup (PR-63 open, awaiting merge) → wizard-cluster impl plan (PR-64 open) → plan-review-cycle (3 Claude rounds + Codex R4 still in-flight) → **STOP**: plan-review surfaced a critical gating finding (Task 2 config-impl never shipped in code) that needs fresh-context revision next session.
**Status:** 5 PRs merged this session; 2 PRs open + awaiting your merge; 1 Codex round still running in background; plan revision deferred to next session per "next session, hand it off for immediate execution" call.

---

## Next session's starting prompt

> Paste this verbatim into a fresh Claude Code session. **The critical first action is verifying which PRs landed + checking Codex R4 output before any other work.** Then the work order is `tuxlink-4mt` (Task 2 config impl, HARD blocker) FIRST, before any wizard impl plan revision.

```
I'm resuming the tuxlink project. `maple-magpie-oak` handed off
2026-05-18 PART 2 with 2 open PRs + a deferred plan-revision blocked
on a newly-discovered critical prerequisite gap.

Read these BEFORE any action:

1. `dev/handoffs/2026-05-18-maple-magpie-oak-part-2.md` — THIS handoff
   on branch `bd-tuxlink-ln3/wizard-cluster-plan`. Read via:
   `git show bd-tuxlink-ln3/wizard-cluster-plan:dev/handoffs/2026-05-18-maple-magpie-oak-part-2.md | head -300`
2. `docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md`
   (on feat/v0.0.1 via PR #62) — wizard cluster spec, post-adrev,
   canonical for impl design
3. `docs/superpowers/plans/2026-05-18-wizard-cluster-plan.md`
   (on bd-tuxlink-ln3/wizard-cluster-plan via PR #64, OPEN) — wizard
   impl plan, pre-revision; PR-64's body lists the plan-review findings
4. The 3 plan-review files at `dev/adversarial/2026-05-18-wizard-plan-
   review-R[1-3]-*.md` (worktree gitignored; in the
   bd-tuxlink-ln3-wizard-cluster-spec worktree)
5. The Codex R4 plan-review file at `dev/adversarial/2026-05-18-wizard-
   plan-review-R4-cross-provider-codex.md` (worktree gitignored; check
   if Codex finished writing)

Once read:

- Generate a fresh moniker via `python3 .claude/scripts/get_agent_moniker.py`.
- VERIFY PR-63 + PR-64 status (`gh pr view 63 --json state` and
  `gh pr view 64 --json state`). If either MERGED, bd close the
  corresponding issue (tuxlink-cyy / tuxlink-ln3 — but note ln3 closes
  on the FUTURE wizard impl PR-B per its bd description, NOT on the
  plan PR-64 merge).
- VERIFY Codex R4 output: read `dev/adversarial/2026-05-18-wizard-plan
  -review-R4-cross-provider-codex.md` in the bd-tuxlink-ln3-wizard-
  cluster-spec worktree. If Codex completed with findings, integrate
  them into the consolidated revision plan.

NEW WORK ORDER (CRITICAL — do NOT skip):

The wizard impl plan PR-64 cannot be implemented as-drafted because it
assumes infrastructure that doesn't exist. **DO NOT** start subagent-
driven-development on the wizard plan until BOTH of these land:

1. `tuxlink-4mt` (P1) — Task 2 config implementation: update
   src-tauri/src/config.rs to nested AMD-1 schema + drop
   winlink_password_present per AMD-11 + add validate_identity() +
   add write_config_atomic(). Full build-robust-features pipeline.
   THIS IS THE BIGGEST GAP. Without it: src-tauri/src/config.rs ships
   pre-AMD-1 flat schema; wizard plan Phase 3.2 fails to compile.

2. `tuxlink-756` (P1) — Task 3 PatProcess amendment to render Pat's
   non-secret config at Pat-spawn time. Without it: wizard ships but
   Pat can't operate.

Both bd issues have descriptions with full scope + rationale + links to
adrev findings. Both are HARD blockers on tuxlink-ln3 (bd dep edges
added 2026-05-18).

Then return to wizard impl plan revision (which incorporates the 4
rounds of plan-review findings) BEFORE invoking subagent-driven-
development for the wizard impl.

The 5 follow-up bd issues from wizard spec §7.3 ALSO need attention but
are lower priority (docs cleanups + tool fix).
```

---

## What landed in this session (full session arc)

| # | Item | Commits / PRs | Status |
|---|---|---|---|
| 1 | Phase 10: tuxlink-pat submodule bump to PR-A merge SHA 4969aa86 | bump 1b9a63c + LDC d714aba on bd-tuxlink-mib/mib-cred-keyring | PR #59 **MERGED** (merge SHA 81b5dc4); tuxlink-mib **CLOSED** |
| 2 | AMD-11..14 cluster: fork+keyring amendments (Tasks 2/6/10 + Subagent Guardrails) | ba599b0 + 4613814 (bd-id correction) on bd-tuxlink-54p/amd-fork-keyring-amendments | PR #60 **MERGED** (d3a072f); tuxlink-54p **CLOSED** |
| 3 | libsecret-1 + secret-service runtime deps documentation | bf6d22f on bd-tuxlink-gdo/appimage-libsecret-doc | PR #61 **MERGED** (38788c9); tuxlink-gdo **CLOSED** |
| 4 | Wizard cluster Wave-2 spec (Tasks 9/10/11/11.5 as one coherent unit) | 9ce433e (pre-adrev, 349 lines) + 8bc67ff (4-Claude-round revision applied, +247/-55) + da605e4 (Codex R5 revision: browser-smoke split, secret-tool typo, snapshot rollback, Pat config handoff, footgun fixes) on bd-tuxlink-ln3/wizard-cluster-spec | PR #62 **MERGED** (f978a05) |
| 5 | Wizard spec 5-round adrev (4 Claude + Codex R5 via stdout-tee fallback) | dev/adversarial/2026-05-18-wizard-cluster-adrev-R[1-5]*.md (gitignored) | done; 56 findings (12 P0); all P0 + most P1 applied in PR-62's commits 2 + 3 |
| 6 | Post-cred-handling docs cleanup: design doc §5.2 + plan Task 11.5 (AMD-15) + plan Task 9 (AMD-16) + CLAUDE.md Codex sandbox note | 69824bc on bd-tuxlink-cyy/docs-cleanup-amds | PR **#63 OPEN** awaiting merge |
| 7 | Wizard cluster impl plan (Phases 0-8; LDC banner; 1303 lines pre-plan-review) | 7ec8f81 on bd-tuxlink-ln3/wizard-cluster-plan | PR **#64 OPEN** awaiting merge — but plan needs substantive revision (see findings below) before impl |
| 8 | Wizard plan 4-round plan-review-cycle dispatch | dev/adversarial/2026-05-18-wizard-plan-review-R[1-4]*.md (gitignored) | 3 Claude rounds COMPLETE (R1 friction 15 findings / R2 contract 12 findings / R3 coverage 15 findings = 42 total, 11 P0); R4 Codex IN-FLIGHT in background (PID 1928561, ~timeout 600s left when stopped writing this handoff); R4 file being written by tee per CLAUDE.md Codex section's stdout fallback workaround |
| 9 | bd issues created for follow-up work | `tuxlink-cyy` (closes on PR-63 merge), `tuxlink-756` (Task 3 PatProcess amendment, HARD blocker), `tuxlink-4mt` (Task 2 config impl, HARD blocker — surfaced by plan-review R1) | wired into ln3 via bd dep edges |
| 10 | bd close PRs that merged | tuxlink-mib + tuxlink-54p + tuxlink-gdo all CLOSED | done |

---

## Critical finding: plan-review caught a load-bearing gap

**The wizard impl plan (PR-64) assumes Task 2 config implementation shipped in code. It did not.**

The shipped `src-tauri/src/config.rs` (verified 2026-05-18 against feat/v0.0.1):
- Has FLAT schema with top-level `callsign`, `grid_square`, `pat_mbo_address`, `winlink_password_present`, `wizard_completed` — NOT the nested `connect`/`identity`/`privacy` shape per AMD-1
- Still carries `winlink_password_present` — AMD-11 said to drop it but only updated the plan-text amendment
- Has NO `validate_identity()` function (per AMD-1's loose validator)
- Has NO `write_config_atomic()` function (Task 2 spec calls for it; wizard impl plan Phase 3.2 calls it)

**Gap pattern (worth a pitfalls entry):** AMD plan-text amendments document intent. Code amendments implement that intent. They don't auto-cascade. AMD-1 (2026-05-17) + AMD-11 (2026-05-18) shipped as plan amendments; the corresponding code update to config.rs never shipped as a separate bd issue. The wizard plan inherited the assumption that "AMD = implemented" — false.

Plan-review surfaced this as R1 P0-1 + P0-3. Verified by direct read of config.rs. **This is the kind of failure mode the plan-review-cycle exists to catch — and it did. Working as intended.**

New bd issue `tuxlink-4mt` (P1) tracks the Task 2 config-impl gap. **HARD blocker on the wizard impl plan.** Bd dep edge added: tuxlink-ln3 ← tuxlink-4mt.

---

## State at pause

### What's pushed to origin

```
main                                            (unchanged this session)
feat/v0.0.1                                     ~17 commits ahead (PRs 59, 60, 61, 62 all merged here)
bd-tuxlink-cyy/docs-cleanup-amds                69824bc (PR #63 OPEN)
bd-tuxlink-ln3/wizard-cluster-plan              7ec8f81 + this handoff commit (PR #64 OPEN)
```

The other branches I touched (`bd-tuxlink-mib/mib-cred-keyring`, `bd-tuxlink-54p/amd-fork-keyring-amendments`, `bd-tuxlink-gdo/appimage-libsecret-doc`, `bd-tuxlink-ln3/wizard-cluster-spec`) merged + deleted on remote via `gh pr merge --delete-branch`; local branches still exist as references.

### Worktrees in flight

| Path | Bd claim | State | Disposal? |
|---|---|---|---|
| `worktrees/bd-tuxlink-mib-mib-cred-keyring/` | tuxlink-mib CLOSED | Branch merged + remote deleted | DISPOSAL ELIGIBLE per ADR 0009 (next session) |
| `worktrees/bd-tuxlink-54p-amd-fork-keyring-amendments/` | tuxlink-54p CLOSED | Branch merged + remote deleted | DISPOSAL ELIGIBLE per ADR 0009 |
| `worktrees/bd-tuxlink-gdo-appimage-libsecret-doc/` | tuxlink-gdo CLOSED | Branch merged + remote deleted | DISPOSAL ELIGIBLE per ADR 0009 |
| `worktrees/bd-tuxlink-cyy-docs-cleanup-amds/` | tuxlink-cyy IN-PROGRESS | PR #63 OPEN | Dispose after merge |
| `worktrees/bd-tuxlink-ln3-wizard-cluster-spec/` | tuxlink-ln3 IN-PROGRESS | PR #64 OPEN; this handoff lives here on the wizard-cluster-plan branch | Keep through plan revision + impl-PR work |
| (5 prior-session orphan worktrees per shoal-condor-clover handoff — bd-tuxlink-cvs, ttp, x4s, 4p2, etc.) | Various | Various; not this session's responsibility | Next-session cleanup pass |

### bd state

```
Total: ~48 | Open: ~16 | In Progress: 3 (cyy on PR-63 merge, ln3 on PR-B-future merge, 4p2 stale) | Closed: ~29
```

In-progress + recently-touched:

| Issue ID | Title | Disposition |
|---|---|---|
| `tuxlink-mib` | Cred-handling refactor | CLOSED — shipped via PR-A #2 + PR-B #59 |
| `tuxlink-54p` | AMD-* amendments for Tasks 5/6/9/11 | CLOSED — shipped via PR #60 |
| `tuxlink-gdo` | AppImage secret-service dep doc | CLOSED — shipped via PR #61 |
| `tuxlink-cyy` | Post-cred-handling docs cleanup | IN-PROGRESS; closes on PR #63 merge |
| `tuxlink-ln3` | Wave-2 onboarding-wizard cluster spec + plan | IN-PROGRESS; spec shipped via PR #62; plan PR #64 open + needs revision; closes on FUTURE wizard impl PR-B merge (Phase 8 of the plan) |
| `tuxlink-756` | Task 3 PatProcess amendment for Pat config rendering | NEW (P1); HARD blocker on tuxlink-ln3 wizard impl |
| `tuxlink-4mt` | Task 2 config implementation (AMD-1 + AMD-11 cascades + validate_identity + write_config_atomic) | NEW (P1); HARD blocker on tuxlink-ln3 wizard impl; surfaced by plan-review R1 |

bd dep edges added:
- tuxlink-ln3 ← tuxlink-756 (blocks)
- tuxlink-ln3 ← tuxlink-4mt (blocks)
- tuxlink-ko0 + tuxlink-1r5 + tuxlink-e4x + tuxlink-d76 ← tuxlink-ln3 (chain; the wizard task impls depend on ln3)

---

## Open decisions for the next agent or Cameron

1. **Merge PR-63** (`tuxlink-cyy` docs cleanup; small) — straightforward; no CI risk; merges cleanly.
2. **Merge PR-64 OR amend it first?** PR-64 is the pre-plan-review wizard impl plan. The 4-round plan-review-cycle found 42 findings (11 P0) including the Task 2 gap. Options:
   - (a) Merge as-is; plan-revision becomes a follow-up commit on feat/v0.0.1 (more visible history but a "broken plan landed then fixed" pattern).
   - (b) Apply revision on the existing PR-64 branch as a 2nd commit; review the revised plan; then merge (cleaner).
   - Recommended: (b) — but the revision is HEAVY (requires both content changes AND landing tuxlink-4mt + tuxlink-756 first OR documenting them as explicit external blockers in the plan).
3. **Sequencing of tuxlink-4mt + tuxlink-756 vs plan revision.** Both new bd issues are HARD blockers on the wizard plan. The cleanest order:
   - Land tuxlink-4mt (Task 2 config impl) via its own full pipeline → merges to feat/v0.0.1
   - Land tuxlink-756 (Task 3 PatProcess amendment) via its own full pipeline → merges to feat/v0.0.1
   - THEN revise wizard plan to point at the now-existing infrastructure → merge revised plan PR-64
   - THEN start subagent-driven-development on the wizard plan via the wizard impl PR-B
   - This is the "wave-by-wave" pattern that prevents the Wave-1 revocation problem.
4. **Codex R4 plan-review completion.** Background process PID 1928561 still running. Check `dev/adversarial/2026-05-18-wizard-plan-review-R4-cross-provider-codex.md` in the bd-tuxlink-ln3-wizard-cluster-spec worktree for whatever Codex produced. May add findings to the consolidated revision list.
5. **Worktree disposal cycle.** Multiple stale worktrees (3 merged this session + 5 from prior sessions). Next session should do a disposal pass per ADR 0009 4-step ritual.

---

## Discoveries logged during execution

(Worth carrying forward; some are pitfalls-worthy.)

1. **Codex CLI sandbox blocks writes to `dev/adversarial/`.** 2026-05-18 wizard spec R5 + plan R4. `apply_patch` to gitignored dirs rejected by Codex's default `read_only` sandbox. **Workaround documented in CLAUDE.md Codex section** (PR #63): pipe stdout via `2>&1 | tee dev/adversarial/<name>.md` for fallback capture. Alternative: pass `-c sandbox_permissions='["disk-full-write-access"]'`. Without this knowledge, the operator/agent would think Codex produced nothing when it actually wrote to stdout.

2. **Codex CLI `--commit/--base + PROMPT` mutually-exclusive bug.** 2026-05-18. Help text says they coexist; binary rejects. Workaround: use `codex exec` (free-form) with the commit SHA mentioned in the prompt body, OR drop the PROMPT positional arg entirely.

3. **AMD plan-text amendment ≠ code amendment (the load-bearing gap from plan-review R1).** AMD-1 + AMD-11 shipped as plan amendments 2026-05-17 + 05-18 but the corresponding `src-tauri/src/config.rs` code update never shipped. Same pattern likely exists for other AMDs. **Worth a pitfalls entry: AMD-DRIFT-N or similar**, with a checklist for "when amending the plan, also file the bd issue for the code-impl side." Add this in next session when also writing tuxlink-4mt's plan.

4. **Plan-review-cycle worked as intended.** 4 rounds, 1 still running, 42 findings already from 3 rounds. R1's P0-1 caught the AMD-cascade gap; R3's P0-1 caught the missing orphan-keyring detection; R3's P0-4 caught a 2nd RADIO-1 consent gap. Without the cycle, the wizard impl would have failed to compile (R1 issue) AND would have shipped non-Part-97-compliant (R3 P0-4) AND would have leaked credentials on app-quit timing windows (R3 P0-1). All caught before any code shipped. The memory `feedback_no_carveout_on_cross_provider_adrev` is validated again.

5. **Multi-PR-per-session pace works when each PR is contained.** 5 PRs merged this session because each was scoped to a single coherent unit (Phase 10 mech; AMD cluster; gdo docs; spec + revisions; cleanup). The big multi-pipeline work (wizard spec → adrev → revisions → plan → plan-review) consumed substantial context; future sessions might benefit from doing the spec pipeline in one session + plan pipeline in another.

---

## Reminders for the next agent

- **Read the plan-review findings before deciding what to revise.** R1, R2, R3 are detailed punch-lists in `dev/adversarial/2026-05-18-wizard-plan-review-R[1-3]-*.md`. R4 Codex may still be writing — check the file.
- **The Codex R4 background process** (PID 1928561) is in the original session's process group; it will continue running until 10-minute timeout. The file `dev/adversarial/2026-05-18-wizard-plan-review-R4-cross-provider-codex.md` receives stdout via `tee`. Read whenever; partial output is fine.
- **`git push` was last run at this handoff commit.** Verify with `git status` (should show "up to date with origin").
- **DO NOT start subagent-driven-development on the wizard impl plan until tuxlink-4mt AND tuxlink-756 land first.** Both are HARD blockers; the plan as-written calls functions that don't exist + assumes config shape that doesn't exist.
- **The worktree disposal cycle is overdue** (5+ candidates). Schedule a focused cleanup pass + apply ADR 0009 ritual to each.
- **PR-63 (`tuxlink-cyy`) is safe to merge** anytime; small docs PR; no CI risk.
- **PR-64 (wizard plan) should NOT be merged as-is.** Apply the plan-review revision first (which incorporates findings + adjusts for tuxlink-4mt + tuxlink-756 as external dependencies), THEN merge.
- **Memory `feedback_no_carveout_on_cross_provider_adrev` is the load-bearing discipline here.** Don't shortcut the plan-review-cycle even when it feels like extra work. It caught real, production-breaking issues this session.

---

## ADDENDUM (post-handoff-commit): Codex R4 plan-review completed

Codex R4 finished after the main handoff was committed. The full punch-list lives at `dev/adversarial/2026-05-18-wizard-plan-review-R4-cross-provider-codex.md` in this worktree (3933 lines, sandbox-tee'd capture). R4 found **3 P0s + 4 P1s + 4 P2s + 3 P3s** — including 5+ that the 3 Claude rounds missed. Highlights:

### Codex R4 P0s (read these before doing ANYTHING with the wizard plan revision)

1. **Keyring crate dependency + features not pinned.** `cargo add keyring` is insufficient: `keyring 4.x` is sample-oriented; `3.6.x` needs explicit feature flags (`secret-service`, `apple-keychain`, `wincred`). Plan must include compile-proven `Cargo.toml` snippet AND import paths AND init-call BEFORE Phase 1.4 ships. Affects `tuxlink-4mt` indirectly (Task 2 doesn't use keyring but the wizard plan's keyring usage depends on this getting pinned correctly).
2. **Phase 0 verification too loose.** Grep-for-tokens is unreliable. Phase 0 should assert exact API existence (`grep -nE "fn write_config_atomic" src-tauri/src/config.rs`) + ancestry/merge verification + check `external/tuxlink-pat` submodule SHA matches PR-A merge commit + add a Task 3 PatProcess test proving Pat config is rendered before spawn. Consistent with R1 P0-1 finding (config gap).
3. **`TUXLINK_TEST_SEND_MOCK=1` fails OPEN — Part 97 safety violation.** My plan's safety gate is opt-in (absence routes to LIVE). R4 recommends INVERTING: mock should be DEFAULT in dev/agent/CI; LIVE should require a positive `TUXLINK_TEST_SEND_LIVE=1` env var AND per-invocation operator consent inside the Rust command before `pat_client.send()`. **This is a sharper Part-97-aligned design than my plan's**. Plan revision must adopt the inverted model. New tests required: env-absence-does-not-transmit; mock-mode-never-calls-pat-client; live-mode-aborts-without-consent.

### Codex R4 P1s the Claude rounds missed

- **Tauri capability JSON shape likely invalid.** My `wizard:allow-get-wizard-completed` permission naming uses plugin-prefix syntax; app-defined commands use a different naming scheme + need `src-tauri/permissions/*.toml` entries + `build.rs` `AppManifest::commands(...)` to be capability-gated for real. The `wizard.json` capability as-drafted might fail schema/build validation OR give a FALSE sense of scoping (commands still callable without capability check).
- **Snapshot rollback swallows non-NoEntry errors.** `let prior = entry.get_password().ok();` treats every read failure as "no prior credential" — including locked store, backend failure, bad encoding. Should only map `NoEntry → None`; any other error aborts before `set_password` mutates anything.
- **Frontend TDD cannot start.** `package.json` likely has no `vitest`, `@testing-library/react`, `@testing-library/jest-dom`, or `jsdom`. Plan needs a Phase 1.0 test-harness installation task BEFORE Phase 1.1.
- **(R4 also confirms R3 P0-1: orphan-keyring recovery has no plan task — cross-platform enumeration is hard.)**

### Codex R4 P2s worth folding in

- TDD pattern degrades in later phases (sketched bodies in Phases 4/5/6 should be tightened to fail→impl→pass→commit).
- Phase 5 needs a testable boundary around PatClient (currently concrete blocking client; introduce a trait or DI for unit-testability).
- CI recipe should use `dbus-run-session` not `dbus-launch` (the cred-handling Phase 9 recipe uses `dbus-run-session` for reliable session-bus env propagation).
- `wizard_run_test_send` is async Tauri command but `pat_client` uses `reqwest::blocking`. Holding the wizard mutex while doing blocking HTTP + 30s poll ties up runtime threads. Either convert to async or wrap in `tauri::async_runtime::spawn_blocking`.

### Updated work order (incorporating R4 P0-3 specifically)

The next-session plan revision MUST flip the test-send safety gate. New shape:
```
- LIVE transmission: requires TUXLINK_TEST_SEND_LIVE=1 env var SET
  + per-invocation consent dialog inside the Rust command (mirroring
  consent_gate.rs from live_cms_smoke binary)
- MOCK transmission: DEFAULT when LIVE env var is unset (no opt-in
  needed; subagents + CI just work mocked without thinking about it)
- LIVE-mode tests assert: consent gate fires before pat_client.send();
  if not satisfied, command errors with TestSendOutcome::Failed
  cause="consent withheld" — NO transmission occurs.
```

This is functionally equivalent to the `live_cms_smoke` binary's existing consent gate (`consent_gate.rs` per Task 6); the wizard's `wizard_run_test_send` should reuse the same gate or a derivative.

### Net plan-review-cycle status

| Round | Findings | P0 |
|---|---|---|
| R1 friction (Claude) | 15 | 3 |
| R2 contract (Claude) | 12 | 3 |
| R3 coverage (Claude) | 15 | 5 |
| R4 cross-provider (Codex) | 14 | 3 |
| **Total** | **56** | **14** |

Of the 14 P0s, the most critical for next-session sequencing:
- R1 P0-1/P0-3 + R4 P0-2 → already captured in tuxlink-4mt (Task 2 config impl bd issue)
- R4 P0-3 (inverted safety gate) → critical plan-revision content
- R3 P0-1 + R4 P1 (orphan-keyring) → plan-revision new phase or new bd issue
- R3 P0-4 (RADIO-1 consent in wizard_run_test_send) → SAME as R4 P0-3 from a different angle
- R4 P1 (Tauri capability ACL shape) → plan-revision Task 3.4 content

The 5 follow-up bd issues from wizard spec §7.3 + the 5 new findings from plan-review = roughly 10 distinct work items beyond the 2 HARD blockers (tuxlink-4mt + tuxlink-756). Next session has substantial work; the wizard impl is at least 3-4 sessions away from starting (Task 2 config impl + Task 3 PatProcess + plan revision + then start subagent-driven-development).
