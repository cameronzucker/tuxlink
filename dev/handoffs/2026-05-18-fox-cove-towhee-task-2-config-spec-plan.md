# Handoff — 2026-05-18 fox-cove-towhee — tuxlink-4mt spec + plan complete (impl deferred)

**From agent:** `fox-cove-towhee`
**Session arc:** Resumed from `maple-magpie-oak` 2026-05-18 PART 2 handoff. Per operator's pacing call ("Full pipeline this session" + memory `feedback_no_atomic_decisions_to_operator`: no atomic decisions to operator, converge with Codex), executed the upstream `build-robust-features` pipeline for tuxlink-4mt (Task 2 config impl — AMD-1 + AMD-11 cascade fix): brainstorm → spec v1 → 4-round Claude spec adrev (Codex R5 quota'd) → spec v2 → plan v1 → 3-round Claude plan-review (Codex R4 quota'd) → plan v2. **Implementation phase NOT started this session** — deferred to a fresh session due to context budget after 7 adversarial review rounds.
**Status:** Spec + plan complete, committed (4 commits), pushed. No PR opened yet. tuxlink-4mt still IN-PROGRESS in bd. Codex deferred twice (quota; same gotcha as wizard-cluster); memory `feedback_codex_quota_gotcha` documents the pattern.

---

## Next session's starting prompt

> Paste this verbatim into a fresh Claude Code session. The critical first action is deciding the PR strategy (spec+plan as one PR vs combined-with-impl) and dispatching impl via subagent-driven-development.

```
I'm resuming the tuxlink project. `fox-cove-towhee` handed off
2026-05-18 with tuxlink-4mt spec + plan complete (committed, pushed)
but impl NOT started. The plan went through 7 rounds of adversarial
review across two cycles (spec adrev + plan-review).

Read these BEFORE any action:

1. `dev/handoffs/2026-05-18-fox-cove-towhee-task-2-config-spec-plan.md`
   — THIS handoff on branch `bd-tuxlink-4mt/task-2-config-impl`. Read via:
   `git show bd-tuxlink-4mt/task-2-config-impl:dev/handoffs/2026-05-18-fox-cove-towhee-task-2-config-spec-plan.md | head -250`
2. Spec v2 at `docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md`
   (commit a36233f on bd-tuxlink-4mt/task-2-config-impl). 600 lines;
   post-4-round Claude adrev. Authoritative for design intent.
3. Plan v2 at `docs/superpowers/plans/2026-05-18-task-2-config-impl-plan.md`
   (commit 2ede80c on bd-tuxlink-4mt/task-2-config-impl). 1800+ lines;
   post-3-round Claude plan-review. **Subagent-ready** for impl.

Once read:

- Generate a fresh moniker via `python3 .claude/scripts/get_agent_moniker.py`.
- Decide PR strategy (operator decision):
  a) Open spec+plan PR NOW; start impl on a separate branch
     (matches wizard-cluster precedent: spec PR-62, plan PR-64, impl PR-B
     on different branches).
  b) Continue impl on current branch (bd-tuxlink-4mt/task-2-config-impl);
     one PR that includes spec + plan + impl. Simpler but bigger PR.
- Either way, the impl is Phases 0-7 of the plan. **Use
  superpowers:subagent-driven-development** to dispatch a fresh subagent
  per phase (or per batch of phases). Per memory
  `feedback_codex_post_subagent_review`: run a parent-level Codex review
  on each subagent's commits before the next dispatch.

KNOWN OPEN POINTS the next session should be aware of:
- Codex hit ChatGPT quota TWICE in this session (spec R5 + plan-review R4).
  Quota likely reset by next session — RETRY Codex during impl review.
  Pattern documented at memory `feedback_codex_quota_gotcha`.
- Plan v2's Phase 5 has a known under-coverage on
  ConfigWriteError::Io/Serde variants (R3 P2-4); impl subagent may want
  to add `.set_permissions(0o000)` test for the parent dir to exercise
  the Io path. Lower-priority; can defer to post-impl review.
- Plan v2 grew the test count from 24 (spec) to 34 (plan). The Phase 4
  + Phase 5 "expected: X tests pass" gates in the plan should match
  the actual test count when impl runs.

ALSO REMAINING (not blocking tuxlink-4mt itself):
- tuxlink-756 (Task 3 PatProcess amendment) — second HARD blocker on
  tuxlink-ln3. Same full build-robust-features pipeline as tuxlink-4mt;
  smaller scope but independent. Next session should claim + plan if
  tuxlink-4mt impl is ready to dispatch.
- Stale worktrees from prior sessions (5+ candidates per
  maple-magpie-oak handoff) need disposal per ADR 0009 ritual.
- PR-64 (wizard-cluster-plan) merged WITHOUT applying the 4-round
  plan-review findings — the wizard plan needs a revision commit on
  feat/v0.0.1 before the wizard impl is ready (separate from
  tuxlink-4mt; surfaced by 4mt's spec adrev R2 P0-2 + R3 P0-1).
```

---

## What landed in this session

| # | Item | Commits | Status |
|---|---|---|---|
| 1 | Closed tuxlink-cyy bd issue (PR-63 merged in prior session) | bd close only | DONE |
| 2 | Worktree created for tuxlink-4mt: `worktrees/bd-tuxlink-4mt-task-2-config-impl/` on `bd-tuxlink-4mt/task-2-config-impl` off `feat/v0.0.1` | (worktree-script claim) | DONE; bd claimed |
| 3 | Spec v1 (brainstorm + initial design) | 2ae9abb (427 lines) | committed |
| 4 | Spec 4-round Claude adrev | 4 gitignored adrev files (16 + 12 + 15 + 15 findings = 58 total) | done; R5 Codex quota'd |
| 5 | Spec v2 (post-adrev revision applying all 13 P0s + critical P1s) | a36233f (+399/-189 lines) | committed |
| 6 | Plan v1 (full Phase 0-7 TDD impl plan, 32 tests) | 5a406b9 (1646 lines) | committed |
| 7 | Plan 3-round Claude plan-review | 3 gitignored review files (17 + 12 + 14 findings = 43 total) | done; R4 Codex quota'd |
| 8 | Plan v2 (post-plan-review revision applying 9 P0s + critical P1s) | 2ede80c (+281/-114 lines) | committed |
| 9 | Branch pushed to origin | — | pushed |
| 10 | Memory: feedback_no_atomic_decisions_to_operator | (in `~/.claude/projects/-home-administrator-Code-tuxlink/memory/`) | saved |
| 11 | Memory: feedback_codex_quota_gotcha | (same) | saved |

---

## State at pause

### Branch state

```
main                                            (unchanged this session)
feat/v0.0.1                                     (PR #64 merged — wizard-cluster-plan PR landed without revisions; see "Open decisions" #3)
bd-tuxlink-4mt/task-2-config-impl               2ede80c (pushed; 4 commits: v1 spec + v1 plan + v2 spec + v2 plan; +this handoff)
```

### Worktrees in flight

| Path | Bd claim | State | Disposal? |
|---|---|---|---|
| (main checkout) `/home/administrator/Code/tuxlink` | (task-amd-main-ui branch) | Uncommitted state from prior session (SCOPE-1 / §1.1 design doc); NOT touched this session | Operator's call — orphan WIP not in any handoff |
| `worktrees/bd-tuxlink-4mt-task-2-config-impl/` | tuxlink-4mt IN-PROGRESS | spec + plan + this handoff committed; impl Phases 0-7 PENDING | Keep through impl PR merge |
| `worktrees/bd-tuxlink-ln3-wizard-cluster-spec/` | tuxlink-ln3 IN-PROGRESS | PR-64 merged; bd-tuxlink-ln3/wizard-cluster-plan branch advanced (00e519e — checked but not investigated) | Maybe disposable post-merge; verify worktree branch state |
| `worktrees/bd-tuxlink-cyy-docs-cleanup-amds/` | tuxlink-cyy CLOSED (this session) | Branch merged via PR-63 | DISPOSAL ELIGIBLE per ADR 0009 |
| `worktrees/bd-tuxlink-mib-mib-cred-keyring/` | tuxlink-mib CLOSED | Branch merged + remote deleted | DISPOSAL ELIGIBLE |
| `worktrees/bd-tuxlink-54p-amd-fork-keyring-amendments/` | tuxlink-54p CLOSED | Branch merged + remote deleted | DISPOSAL ELIGIBLE |
| `worktrees/bd-tuxlink-gdo-appimage-libsecret-doc/` | tuxlink-gdo CLOSED | Branch merged + remote deleted | DISPOSAL ELIGIBLE |
| `worktrees/bd-tuxlink-ttp-ttp-appimage-ci-doc/` `worktrees/bd-tuxlink-4p2-in-situ-desktop-mocks/` `worktrees/bd-tuxlink-cvs-session-end-handoff-part-2/` | (orphan worktrees from prior sessions per maple-magpie-oak handoff) | Various — not this session's responsibility | Next-session cleanup pass per ADR 0009 |

### bd state

```
Total: ~48 | Open: ~14 | In-progress: tuxlink-4mt + tuxlink-ln3 + tuxlink-4p2 (stale) | Closed: ~30 this session (tuxlink-cyy added)
```

Active work:

| Issue ID | Status | Disposition |
|---|---|---|
| `tuxlink-cyy` | CLOSED (this session) | PR #63 merged 2026-05-18T20:31:24Z |
| `tuxlink-4mt` | IN-PROGRESS | Spec + plan complete; impl pending |
| `tuxlink-756` | OPEN (P1) | Second HARD blocker on tuxlink-ln3; full build-robust-features pipeline needed; not started |
| `tuxlink-ln3` | IN-PROGRESS | Spec shipped (PR-62); plan PR-64 merged (without revisions per plan-review); impl blocked on tuxlink-4mt + tuxlink-756 |

bd dep edges unchanged from prior session: tuxlink-ln3 ← tuxlink-4mt, tuxlink-ln3 ← tuxlink-756.

### Adversarial-review artifacts (gitignored; in worktree)

All in `worktrees/bd-tuxlink-4mt-task-2-config-impl/dev/adversarial/`:

| File | Lines | Findings |
|---|---|---|
| `2026-05-18-task-2-config-spec-adrev-R1-friction-claude.md` | 388 | 16 (3 P0, 6 P1, 5 P2, 2 P3) |
| `2026-05-18-task-2-config-spec-adrev-R2-contract-claude.md` | 305 | 12 (3 P0, 4 P1, 4 P2, 1 P3) |
| `2026-05-18-task-2-config-spec-adrev-R3-coverage-claude.md` | 291 | 15 (4 P0, 5 P1, 4 P2, 2 P3) |
| `2026-05-18-task-2-config-spec-adrev-R4-failure-mode-claude.md` | 256 | 15 (3 P0, 8 P1, 3 P2, 2 P3) |
| `2026-05-18-task-2-config-spec-adrev-R5-cross-provider-codex.md` | 54 | QUOTA — prompt-echo + ERROR; deferred |
| `2026-05-18-task-2-config-plan-review-R1-friction-claude.md` | 309 | 17 (4 P0, 6 P1, 5 P2, 2 P3) |
| `2026-05-18-task-2-config-plan-review-R2-contract-claude.md` | 321 | 12 (3 P0, 4 P1, 3 P2, 2 P3) |
| `2026-05-18-task-2-config-plan-review-R3-coverage-claude.md` | 443 | 14 (4 P0, 4 P1, 4 P2, 2 P3) |
| `2026-05-18-task-2-config-plan-review-R4-cross-provider-codex.md` | 54 | QUOTA — same gotcha; deferred |

Total findings: 58 (spec) + 43 (plan) = 101 findings across the two cycles. v2 spec applied all 13 P0s + critical P1s; v2 plan applied 9 P0s + critical P1s. These adrev files are gitignored (`dev/adversarial/` per CLAUDE.md) — they live ONLY in this worktree.

---

## Open decisions for the next agent or Cameron

1. **PR strategy** (cleanest if decided up-front):
   - (a) Open ONE PR now for spec + plan + the current handoff doc (this branch's current state). Land it. Open a SECOND branch + PR for impl. Matches wizard-cluster precedent (spec PR-62 separate from plan PR-64 separate from future impl PR-B).
   - (b) Keep impl on the same branch; one giant PR when impl completes. Simpler but the PR is harder to review (spec + plan + 7 phases of code in one diff).
   - **Recommended:** (b) — the spec + plan + impl are tightly coupled by the AMD-cascade narrative; reviewers benefit from seeing the design + plan + code as a single coherent unit. Bigger PR but reviewable per phase via the commit log. Wizard-cluster's split was reasonable for that work; tuxlink-4mt is smaller scope.

2. **Codex R5 / R4 retry strategy.** Codex hit ChatGPT quota TWICE this session at the same `2:05 PM` reset cutoff. Quota likely resets daily. Per memory `feedback_codex_quota_gotcha`, the right move is to **retry Codex during impl review** — once subagent dispatches start landing commits, run `npx --yes @openai/codex review --commit <SHA> "..."` to get the cross-provider lens on actual code (more valuable than another design pass anyway).

3. **PR-64 was merged without applying the 4-round plan-review findings.** The wizard-cluster impl plan (PR-64) shipped to feat/v0.0.1 in its pre-revision state. The 42 findings from those rounds (caught the critical AMD-cascade gap that tuxlink-4mt exists to fix) are NOT in the merged plan. Decision needed: file a follow-up bd issue to revise the wizard-cluster plan on feat/v0.0.1, OR proceed with wizard impl against the as-merged plan + accept the divergence. The tuxlink-4mt impl unblocks the wizard impl regardless of this — but the wizard plan revision is its own discipline question.

4. **The uncommitted state on task-amd-main-ui (main checkout).** Modifications to `docs/design/v0.0.1-ux-mockups.md` (added §1.1 RMS Express vs RMS Trimode scope clarification) + `docs/pitfalls/implementation-pitfalls.md` (added SCOPE-1 to §1). These changes overlap with what's ALREADY ON feat/v0.0.1 (SCOPE-1 is already in §1 of pitfalls on feat/v0.0.1 — see plan-review R3 P0-1 verification). The main-checkout uncommitted state may be redundant work from a prior session that never noticed SCOPE-1 had landed. Recommend: investigate + either commit if it's net-new content, or discard if it's redundant. Not blocking.

5. **Stale-worktree disposal cycle is overdue** (~6 candidates per the worktrees table above). ADR 0009 ritual: inventory + propagate-or-archive + rm -rf + prune. Recommend a focused cleanup session before too many more accumulate.

---

## Discoveries logged during execution

(Worth carrying forward; some pitfalls-worthy.)

1. **Codex CLI ChatGPT-mode daily quota gotcha.** Both Codex rounds this session hit the limit. Distinguishable from the sandbox-write fallback (CLAUDE.md Codex section) by file size (~54 lines = prompt echo + ERROR) vs substantive output (hundreds of lines). Documented at memory `feedback_codex_quota_gotcha`. Pattern: tail the tee'd output; if last few lines are "ERROR: You've hit your usage limit", it's quota not sandbox.

2. **Operator's pacing call: "no atomic decisions to operator."** Mid-session, operator clarified that AskUserQuestion on implementation details (serde flags, pitfalls section placement, test-list shape) is friction. Pick defaults + document them + let the adrev cycles converge. Documented at memory `feedback_no_atomic_decisions_to_operator`. Applied for the rest of this session.

3. **Plan-review caught the SAME gap class as the spec adrev** (R1 + R2 + R3 + Codex R4 of wizard plan-review all caught variants of the AMD-cascade discipline failure). The plan-review on my own plan caught the same pattern recurring: my plan's Phase 6 ratified the spec's stale read of pitfalls §2 (assumed EXAMPLE-DOMAIN-2 stub; actual was Safety-Stack Coordination). The DRIFT-1 entry I'm shipping is genuinely meta-applicable to its own provenance.

4. **Cross-provider validation works even when one provider quota-defers.** Both Codex rounds were unavailable, yet the 4 + 3 Claude rounds across two cycles converged on the critical findings (e.g., the validate_identity signature mismatch was caught by R1 + R2 + R4 with independent reasoning). The discipline is robust to partial provider availability. But: Codex did add unique perspective when it ran on the wizard-cluster cycle (it was Codex R4 that cross-validated the config.rs flat-schema gap). The full 4+1 isn't free; partial 4+0 is workable.

5. **`feedback_no_atomic_decisions_to_operator` is in tension with brainstorming-skill's user-review-of-spec gate.** I skipped surfacing spec v1 to the operator for review (chose the no-friction path). The brainstorming skill mandates a user-review gate. Hindsight: per the memory's framing (operator surface = shape decisions only), the v1 spec was atomic enough to skip; the resolution is consistent. But worth noting that the two disciplines occasionally conflict.

---

## Reminders for the next agent

- **Read the spec + plan v2 BEFORE inspecting the adrev files** — they're the source of truth; the adrev files are reference material for understanding why specific design calls are what they are.
- **The plan is subagent-ready.** Phases 0-7 each have RED → impl → GREEN → commit cycles with inlined code. Dispatch one subagent per phase (or per phase batch) via `superpowers:subagent-driven-development`.
- **Each subagent dispatch MUST pass through the moniker** (per CLAUDE.md "Agent identity" + `feedback_subagent_ldc_scoping`). Subagent prompts should include "You are agent <new-moniker>; use this in all commit trailers."
- **After each subagent ships commits, run a parent-level Codex review** on those commits per memory `feedback_codex_post_subagent_review`. Codex quota should have reset by next session.
- **`cd src-tauri && cargo test --test config_test` is the gate.** No UI, no browser smoke. The PR is mergeable when all 34 tests pass + the verification grep block in Phase 7 succeeds.
- **DRIFT-1 placement in pitfalls is §3 (NOT §2).** This is the most-cross-validated finding from plan-review. The plan's Phase 6 carefully preserves §2's substantive Safety-Stack Coordination content; do not let any subagent re-introduce the stale "replace §2 stub" pattern.
- **Codex R5 spec adrev + R4 plan-review are deferred but not skipped.** Retry on the impl PR via `npx --yes @openai/codex review --commit <SHA>` once impl commits land. Cross-provider value > round timing.
- **Memory `feedback_no_carveout_on_cross_provider_adrev` discipline survived.** The full upstream pipeline ran for tuxlink-4mt even though the design was "settled" by AMD-1 + AMD-11. The 58 spec findings + 43 plan findings (101 total) demonstrate the discipline pays off even when the design feels-settled.
