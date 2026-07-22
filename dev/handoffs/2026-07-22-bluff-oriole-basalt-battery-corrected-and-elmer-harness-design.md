# Session handoff — bluff-oriole-basalt (2026-07-22)

Marathon session. Began as fir-kestrel-dahlia's battery continuation (Fable 5),
operator hit the weekly Fable limit mid-session and switched to Opus 4.8, which
finished it. Ended on a strategic pivot: **Elmer as an agentic harness** (design
APPROVED). Context went critically full; operator called the handoff.

## THE MOST IMPORTANT THING TO READ FIRST

**Approved design doc:** `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260722-034743.md`
(Status: APPROVED). Next action the operator chose: **run `/plan-eng-review`** on
it to lock the phase-orchestration engine + artifact contracts + experiment
before any code. Then build the Routine Engineer first slice. The eureka source
(GPT-5.6 soundboard) is at `dev/scratch/tuxlink_elmer_skills_eureka.md`.

## OPERATOR ACTION STILL PENDING (flagged repeatedly, not yet done)

A stray commit sits on the remote `bd-tuxlink-ant8s/ardop-connect-fixes` from a
cwd-reset incident (a chained `git add -A && commit && push` ran in the main
checkout, swept ~106 untracked operator WIP files, and PUSHED before the
wrong-branch line could stop it). Local was restored (`git reset HEAD~1`, mixed).
The REMOTE still carries it. Operator must run, when convenient:
`git push origin +81fd0a2a:bd-tuxlink-ant8s/ardop-connect-fixes`
(force-push is operator-only; the destructive-git hook blocks the agent.)
The recurrence is now HOOK-BLOCKED (see PR #1235 below), so it can't happen again.

## THIS HANDOFF DOC IS ON DISK, NOT YET COMMITTED

Written via Write to dev/handoffs/. NOT git-committed, because context was
critically full (5 cwd-reset incidents this session) and the main checkout is the
operator's lease-held branch. Next session (fresh context, from a clean worktree
off main) or the operator should commit+push it. Do NOT commit it to
bd-tuxlink-ant8s.

## MERGED THIS SESSION (10 PRs, all on main)

- #1229 headless Elmer battery harness (bd hwgdi)
- #1232 the 6epl8 Branch-dialect absorption (belt) + catalog/refusal teaching
  (suspenders) + embedded-$ref interpolation + 8 Codex fixes
- #1233 jt9 ETXTBSY deflake in decode_slot (bd ux4t7, closed)
- #1234 ARM_FALLTHROUGH_LEAK validator finding + retry-leak fix (bd ilrav, closed)
- #1235 **git-chain hook**: bans `git add -A`/`.`, chained mutating git ops, and
  `cd && git <write>` in one call (bd 18san, closed) — prevents the incident above
- #1236 S1-completion battery journal
- #1237 zvy6q: allowlist denial non-terminal (battery harness fix)
- #1238 CORRECTED S4/S3 journal (harness confound)
- #1239 g31en: credits baseline non-fatal for non-OpenRouter endpoints (unblocks
  local vLLM runs)
- #1240 local-vs-API qwen parity journal

## THE CORRECTED BATTERY FINDINGS (honest, after operator caught over-claims)

The eval SATURATES on strong models. On satisfiable tasks **qwen ~ sonnet ~ gpt
all pass** — no qwen capability-win (my earlier "qwen beat Sonnet" was the
arbitrary $2 cost-ceiling cutting Sonnet off, plus a contestable P3 framing; both
retracted). qwen's REAL, defensible edges: **~8-9x fewer output tokens** than
Sonnet on the same tasks (S3: 2,277 vs 18,150; S4: 1,620 vs 14,681), and
**reliability vs glm**. **glm-5.2 is the weak local model**: def-as-string
`invalid_action` on P1/P2/S3 (bd x4wax), P3 timeout even at 1200s/turn. **P3 is a
PRODUCT GAP** (no propagation-prediction affordance), not a model failure — local
qwen's traces name the missing primitive accurately and decline honestly (better
than Sonnet's confabulated proxy). **Local qwen35-122b-nvfp4 @ 256k = functional
parity with API qwen** (twin-bramble vLLM endpoint).

### FOUR self-induced seam-defects caught + the judging discipline learned
1. zvy6q terminal-denial (fixed #1237) — voided 3/4 S4 cells.
2. g31en credits-404 (fixed #1239) — blocked local runs.
3. The arbitrary **$2 per-cell ceiling** (bd l264r, NOT yet fixed in code) — it
   CANCELS despite its comment saying "advisory"; NOT derived from the $50 budget
   (that's LEDGER_HARD_STOP=$45). It cost-cancelled the frontier models and I
   mis-attributed it as capability. **Reframe l264r + fix DEFAULT_CELL_CEILING_USD
   in `src-tauri/src/bin/elmer_battery.rs` (make truly advisory / raise to ledger
   stop / remove).**
4. P3-as-pass/fail — I judged a corpus-marked gap-cartography cell as pass/fail.
Discipline (saved to memory `feedback_check_outcome_before_judging_def`): read the
outcome status BEFORE judging the artifact; verify task satisfiability against the
LIVE tool surface before any pass/fail.

## PARKED BENCHMARK WORK (resume when the operator wants)

- New-model sweep (gpt-oss-120b, nemotron-3-super-120b-a12b, inkling via
  OpenRouter) was STOPPED mid-nemotron-S1 per the eureka. Partial data on R2 in
  `~/tuxlink-battery-build/battery-results/post-6epl8-1/`. Resume:
  `run-newmodels.sh` on R2 (OPENROUTER_API_KEY from Pi keyring `secret-tool lookup
  service elmer-openrouter` piped to env, NEVER disk). gpt-oss validated authoring
  cleanly via OpenRouter — confirming the local Nemotron/gpt-oss "no tool parser
  configured" note is why prior LOCAL attempts failed (serving config, not model).
- 4-model matrix (qwen/glm/sonnet/gpt) IS complete on real guards; token curve
  captured (`/tmp/tok_table.py` on R2). NOT yet journaled — the
  `dev/battery/journal.md` needs a final corrected-matrix + token-curve entry.
- Ledger ~$27 of $50 (may undercount; credits-delta lags on some cells).

## R2 (r2-poe) state
`~/tuxlink-battery-build` tracks origin/main DETACHED (advance:
`git fetch origin main && git checkout -q origin/main`, never bare pull). Battery
binary built. Operator's dev:converged runs there — never broad-pkill; kill only
exact battery PIDs.

## bd issues filed this session
1yavg (Tuxlink Bench productization, P2 maturity-gated), 77620 (qwen failure
catalog / fine-tune assessment, P2), l264r (arbitrary ceiling, P3), x4wax (glm
def-string, P2), 7004n (concurrency-test flake, P3). Closed: zvy6q, g31en, ilrav,
18san, ux4t7. hwgdi (battery) still in_progress.

## Worktrees alive (ADR 0009 enumeration)
`worktrees/bd-tuxlink-6epl8-*`, `bd-tuxlink-hwgdi-*` (several journal branches),
`bd-tuxlink-zvy6q-*`, `bd-tuxlink-g31en-*`, `bd-tuxlink-ilrav-*`,
`bd-tuxlink-18san-*` — all with MERGED PRs (branches now dead per ADR 0017), safe
to dispose via the ritual. `bd-tuxlink-ux4t7-*` already disposed. Plus older
parked ones from prior sessions (32aew consent-pair, fg0em designer-radio).
Gitignored valuables: `dev/scratch/*battery*`, `dev/adversarial/*codex*`.

## Memories written/updated
`feedback_check_outcome_before_judging_def` (new), `feedback_no_disk_creds_default`
(+cross-machine corollary), `feedback_worktree_git_mechanics` (+5th occurrence,
the pushed incident). Design/architecture insight logged to gstack learnings.
