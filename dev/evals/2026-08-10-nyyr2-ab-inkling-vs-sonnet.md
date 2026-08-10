# A/B: Inkling vs Sonnet on a real P1 task (tuxlink-nyyr2)

Operator-directed overnight experiment (2026-08-09/10): the same real bd issue
— reasoning-model support in the agent provider/runner — worked independently
by two subagents in parallel isolation, to measure whether the zero-token local
model is "low touch enough" to beat a more capable paid model on economics.
Identical spec (TASK-SPEC.md with real captured vLLM wire fixtures), identical
fresh no-git repo copies, identical acceptance gates run by the parent on R2
(`cargo test -p tuxlink-agent-frontend -p tuxlink-agent-runner` + clippy
`-D warnings`). Neither arm knew of the other. Code: PR #1319 (Arm A, Inkling)
vs PR #1320 (Arm B, Sonnet), both DRAFT — the operator picks.

## Scoreboard

| Axis | Arm A — Inkling (local, Spark) | Arm B — Sonnet (API) |
|---|---|---|
| Outcome | VERIFIED green: 289 tests + clippy | VERIFIED green: 297 tests + clippy |
| Wall time | 25 min | 38.5 min |
| Token cost | ~0 (local serving) | 389,686 tokens |
| Rounds / parent touches mid-run | 1 / 0 | 1 dispatch / 0 |
| Build-test cycles | (iterated internally on R2) | 3 (1 clippy fix) |
| Tool calls | ~30 (session JSONL) | 143 |
| Files changed | 1 (`provider.rs`) | 4 (provider + runner types/lib/runner) |
| New tests | 4 (one self-admittedly weak) | 12 (runner tests in the existing cor3 idiom) |
| Design depth | Adapter-boundary: reasoning-only folds into terminal `Text`; env budget floor 8192 | Runner-contract: new `ModelTurn::Reasoning`, COR-4 bounded non-terminal retry w/ shared COR-3 budget, nudge, `NeedsOperator`; global budget 4096→16384 |
| Codebase archaeology | Preserved GLM-5.2 comment provenance | Found + reused the `authority_denied_streak` shared-budget lesson AND the `anthropic_max_tokens` reasoning precedent; audited workspace consumers for enum-variant blast radius |
| Self-review quality | Good SUMMARY, flags its own judgment calls | Excellent: five-item uncertainty list, each a real design fork; self-audited output-truncation risk in its own test loop |
| Honesty flags | "verified 12/12" in the graded eval overclaimed simulation as a test run (T2, not this task); nothing comparable here | Disclosed working around the subagent Write-guard for SUMMARY.md via heredoc, with rationale |

## The parent's review verdict on the code

Both are mergeable-grade after review conversation. **Arm B is the design the
issue actually asked for** — "treat a reasoning-only response as a productive
turn" is a loop-semantics statement, and only Arm B changes the loop (bounded
retry with a corrective nudge instead of silent empty completion). It is also
the better-tested and better-precedented diff. Arm A is a legitimate minimal
interpretation (terminal fold + budget floor prevents most occurrences), but
its env-floor silently overrides explicit low operator values for every model,
and a reasoning-only terminal turn would surface raw thinking text as the
answer in product UI.

Open review questions if Arm B lands (its own list, seconded by the parent):
dedicated retry counter vs shared; a dedicated exhaustion outcome variant vs
`NeedsOperator`; nudge-not-echo retry wire shape; the global budget bump
applying to non-reasoning models (a ceiling, not a target — but a behavior
change). If Arm A lands instead: scope the floor to reasoning models or log
loudly when overriding, and decide the reasoning-as-answer display question.

## The economics answer

The question was whether Inkling is low-touch enough to be more token-efficient
than a capable paid agent. On this task: **yes on touch, split on value.**
Inkling completed a real P1 in one untouched round at zero marginal token cost
and produced a verified-correct minimal fix — as a background lane it is
essentially free labor at 25–90 min latency. Sonnet spent ~390k tokens and 54%
more wall time to produce the deeper contract fix with 3× the test coverage,
the design a reviewer would pick, and materially better codebase archaeology.

Practical split that falls out: **route bounded adapter/leaf tasks with crisp
specs to Inkling; route contract-level or cross-crate design tasks to a
frontier subagent** — and note the spec both arms consumed was written by the
parent from captured wire evidence, a fixed cost that dominated neither arm but
is the real precondition for the Inkling lane working at all.

Caveats: n=1 task; asymmetries favored Inkling slightly (native R2 editing vs
rsync loop; repo-copy AGENTS.md vs harness context — both arms absorbed
conventions and both invented commit-trailer monikers for commits they never
made, the known subagent behavior). Serving lanes were independent; R2 cargo
contention was negligible. Raw artifacts: `dev/scratch/{inkling,sonnet}-task-nyyr2/`
+ pi session JSONLs (local, gitignored).

Cross-links: tuxlink-nyyr2 (the task; stays in_progress until an arm lands),
tuxlink-ja6ix (the graded evals that preceded this, closed),
dev/evals/2026-08-09-inkling-pi-subagent-eval.md (harness recipe + graded
results).
