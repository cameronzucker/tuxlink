# Handoff — ladder M1 fully closed (orig + extension MERGED); next = milestone-2a harness spike

- **Agent:** falcon-shoal-clover
- **Date:** 2026-07-16→17 (overnight ladder + same-day operator-attended
  extension; this closes the whole session)
- **Ended:** operator-directed handoff to give the next orchestrator a clean
  context window for the long-running milestone-2 follow-up. **Start on
  tuxlink-7raoe milestone 2a immediately** (operator's words: "I'm into
  that... start on 7raoe immediately").

## READ THIS FIRST

1. **Everything is merged.** PR #1125 (original 6-rung, 5-arm ladder) and
   PR #1127 (post-hoc extension: 235B + two Nemotrons) are both on main.
   Canonical results: `dev/research/2026-07-16-difficulty-ladder/report.md`
   (the `# EXTENSION` section at the bottom carries the 235B/Nemotron
   results), `scores.md` (per-cell grades incl. 19+10 Opus reviews),
   `ledger.md` (every dispatch/verification/Spark state change).
2. **The three findings that shape milestone 2:**
   - N235 (qwen3-235b-2507): THREE verified fabrications under R2 —
     integrity failure is model-intrinsic under struggle; removed from all
     routing tiers.
   - NU550 (nemotron-3-ultra-550b): only non-frontier model to land the
     rung-5 key-exact fix; hosted-only; envelope+hygiene-limited, not
     reasoning-limited.
   - A large share of local-model losses were HARNESS artifacts: the
     Codex↔Qwen reasoning-as-final-message seam (cost E122 two correct
     diagnoses), tool-protocol crashes, and the prefill-dominated 30-min
     envelope (killed every Spark arm above rung 2).
3. **Milestone 2a is designed and recorded — execute it.** Full rationale
   in TWO bd comments on `tuxlink-7raoe` (read them: `bd comments
   tuxlink-7raoe`). Summary: three-way harness regression — **Pi**
   (pi.dev / earendil-works: imperative TS SDK, per-turn injection, JSON
   events, <1k-token system prompt) vs **mini-swe-agent** (Princeton,
   ~100 lines Python, NO tool-calling protocol — model emits fenced bash;
   directly tests whether the seam losses are tool-protocol artifacts) vs
   the **Codex baselines already measured**. Three cells, same frozen
   briefs/caps/verification: rung 5 with a 122B-class model (the E122 seam
   case), rung 3 with Q122-on-Spark and with coder-next (envelope cases).
   Ladder cell baselines are the regression targets; briefs are on main at
   `dev/research/2026-07-16-difficulty-ladder/briefs/`. OpenCode is the
   maintained fallback; Aider's per-model edit formats are prior art worth
   reading for the heredoc-fragility problem (sank N235's rung 2).

## State at close

- **Worktrees:** all ladder worktrees disposed per ADR 0009 (arm forensics
  in `.claude/worktree-archives/bd-tuxlink-7raoe-ladder-arm-*-sdd-
  forensics-*.tar.gz` — two timestamps, 5 overnight + 3 extension arms).
  Only `worktrees/bd-tuxlink-7raoe-m1-close-handoff` (this handoff's
  branch) remains — dispose after its PR merges. NOTE: 8 never-merge
  branches `bd-tuxlink-7raoe/ladder-arm-*` remain LOCAL-ONLY on purpose
  (candidate evidence; the S5 arm holds harvestable fixes for six real
  backlog issues — see the #1125 handoff's Harvest section; still undone,
  still valuable, gac1d is a 1-line production bug fix).
- **Spark:** serving `qwen3-coder-next` (restored + verified). Patched
  122B template persists at `/home/administrator/serving/`; the working
  vllm-q122 launch recipe (incl. the mandatory `--enable-auto-tool-choice
  --tool-call-parser qwen3_coder` flags) is in `ledger.md`.
- **bd:** `tuxlink-7raoe` in_progress (M1 complete; M2a next; notes +
  2 comments current). `tuxlink-1at3f` = TuxBench (P2, post-DefCon unless
  pulled forward). `tuxlink-860t9` annotated with the second jt9-flake
  family member (amd64).
- **CI note:** two flake events on docs-only PRs tonight — the jt9
  signal-death family (#1125) and a runner-level null-conclusion kill
  (#1127); both cleared on rerun.
- **Main checkout:** untouched all session (all work in worktrees; the
  dmwte Routines session remains live — worktrees stay mandatory).

## Milestone-2a execution notes for the next orchestrator

- Reuse `run-rung.sh` (on main in the ladder dir) as the dispatch pattern;
  the new harnesses need equivalent wrappers (Pi: `--mode json` / RPC;
  mini-swe-agent: fork the loop, keep the 30-min cap + transcript tee).
- Keep the ladder's verification discipline verbatim: re-run every gate,
  diff every tree, 3x rerun for the rung-3 pinning-test race, grade rung 5
  against `grading-keys.md` (on the ladder branch history, NOT main —
  `git show fc83ddaa:dev/research/2026-07-16-difficulty-ladder/grading-keys.md`
  ... NOTE: keys ARE on main now via #1125; workers must not read the repo
  docs dir — mini-swe-agent workers get bash access to the worktree, and
  main now CONTAINS the grading keys + report. **Contamination control for
  M2a: branch worker worktrees from base `b82b404d` (pre-ladder), same as
  the original arms.**
- Per-model integrity screening stays mandatory regardless of harness.

## Operator context from this session

- Operator merged #1125 and #1127 himself; gh pr merge was
  classifier-denied for the agent — expect to hand merges to the operator
  or retry.
- Extension was operator-funded and operator-directed post-report; the
  "dense Nemotron" recollection was corrected pre-dispatch (both current
  Nemotrons are MoE) — flag architecture facts before spending.
