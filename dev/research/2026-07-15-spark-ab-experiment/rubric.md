# Pre-registered evaluation rubric — Spark local-worker A/B experiment

Frozen BEFORE either arm executes (see the commit that introduces this file; its
SHA + timestamp are the pre-registration record). Scoring the arms against
anything not written here is a protocol violation and must be flagged as
post-hoc in the report.

## Experimental design (what varies, what is held constant)

**Single manipulated variable: the implementation-worker tier.**

| Held constant | Value in BOTH arms |
|---|---|
| Plan + per-task briefs | `plan.md` / `briefs/task-N.md`, byte-identical |
| Base commit | same `main` SHA, recorded in the report |
| Orchestrator | this session (Fable), same intervention policy (below) |
| Task-review tier | Claude Opus reviewer per task, same review prompt template |
| Gates | crate tests, `pnpm vitest run`, `pnpm typecheck` locally; clippy + full suites via CI (arm A real PR, arm B draft PR) |
| Adversarial eval | one blind Codex pass per arm, same prompt template |

| Varies | Arm A (baseline, MERGES) | Arm B (comparison, NEVER merges) |
|---|---|---|
| Implementer | fresh Claude Sonnet subagent per task | fresh Codex CLI session per task → vLLM on DGX Spark, model `qwen3-coder-next` (Qwen3-Coder-Next-FP8, 262k ctx) |
| Transport | Claude Code Agent tool | `codex exec` with `-c model_provider=spark` overrides (Responses API) |

**Orchestrator intervention policy (both arms):** interventions are allowed only
to unstick a worker mechanically (rerun a crashed process, repeat the brief,
point at a file the brief already names). Design decisions, code, or hints
beyond the brief's text are prohibited; every intervention is logged verbatim in
the report. An arm whose worker cannot finish after 3 interventions on a task
records that task as **failed**, and the orchestrator completes it out-of-band
(flagged, excluded from that arm's quality metrics but included in the
completion metric).

## Metrics (collected per task, per arm)

### M1 — Task completion
`complete` (deliverable lands, all binding constraints honored) / `partial`
(deliverable lands with a deviation the reviewer had to fix or waive) /
`failed` (3-intervention rule above, or worker abandoned).

### M2 — Gates
For each: pass on first attempt / pass after N fix cycles / never passed.
- Crate tests (`cargo test --manifest-path src-tauri/tuxlink-routines/Cargo.toml`)
- `pnpm vitest run src/routines/designer/RunsTab.test.tsx` (Task 3) + full `pnpm vitest run` at arm end
- `pnpm typecheck`
- CI clippy + full suites on the arm's PR (verify by head SHA per project rule)

### M3 — Review burden
Opus reviewer rounds until accept, per task (a round = one review + one fix
cycle). Reviewer uses the same prompt template both arms (recorded in the
report appendix). Count also the number of distinct defects the reviewer found.

### M4 — Defect count (blind symmetric adversarial pass)
After both arms finish: export each arm's full diff against the shared base SHA
to neutral filenames (`candidate-1.diff` = arm A, `candidate-2.diff` = arm B —
mapping recorded here, invisible to the evaluator). One Codex CLI review pass
per diff, identical prompt template, run back-to-back in fresh sessions with no
arm-identifying context. Orchestrator triages every finding: P1 (correctness /
data loss / back-compat break), P2 (behavioral edge case, missing test), P3
(style/nit). Report confirmed counts per severity; discarded findings listed
with reasons.

**Practicality-cap amendment (2026-07-15, operator, after arm B task 1 concluded but before any later task ran):** worker attempts are capped at 30 minutes wall clock with at most one retry per task; a task not finished inside that envelope records as failed (out-of-band completion path unchanged). Rationale: the cloud baseline ceiling measured in this experiment is ~5-6 min/task (arm A implementers: 309s/311s/330s), so multi-hour local attempts are outside any real-world practicality envelope even at equal quality. Arm B task 1's pre-amendment data (4 attempts, ~3.9h, >1.1M tokens, FAILED) stands as recorded.

### M5 — Efficiency
- **Wall time per task, broken down BY AGENT** (amendment 2026-07-15, operator
  request, before any arm's data was scored: an elaboration of the original
  "wall time per task" line, flagged here per this rubric's post-hoc rule).
  For every task in every arm, report each agent's start → completion wall
  clock separately: implementer, task reviewer, fix subagent(s), and (whole
  branch) the final reviewer — plus per-arm totals and the
  implementation-vs-review split. Sources: harness-reported `duration_ms` for
  Claude agents; dispatched/finished stamps in each arm's
  `.superpowers/sdd/timing.log` for Codex workers. Failed/retried worker
  attempts count in the arm's total wall clock AND are itemized per attempt.
- Tokens where the harness reports them (Codex prints `tokens used`; Claude
  subagent usage as visible to the orchestrator).
- Intervention count (per the policy above).

### M6 — Diff quality (secondary, descriptive)
- Diff size (lines added/removed) vs the plan's scope.
- Tests added (count + names).
- Unsolicited scope drift (files touched beyond the brief's Files list).

## Verdict framework (for the work-proposal report)

The experiment answers: **"can a local-model worker tier execute pre-planned,
well-briefed implementation tasks at acceptable quality under an expensive
orchestrator + constant review tier?"**

- **Feasible** — arm B completes ≥ 2 of 3 tasks `complete`/`partial` with M4
  P1-defect count ≤ arm A + 1 and review burden ≤ 2× arm A.
- **Feasible with caveats** — arm B completes all tasks but exceeds one of the
  thresholds above; the report names which capability gap drove it.
- **Not yet feasible** — any arm-B task `failed`, or M4 P1 defects > arm A + 1
  AND review burden > 2× arm A.

Thresholds are declared now, before data, to keep the verdict honest. The
report may argue nuance around them but must state the mechanical verdict
first.

## Blind-eval prompt template (frozen)

```
You are doing adversarial code review of a candidate diff for the tuxlink
repository (Tauri 2.x; Rust engine crate src-tauri/tuxlink-routines; React 18 +
TS frontend). The diff implements: journal state_changed entries gain optional
step/rig context; the run monitor prefers the exact fields with the legacy
heuristic as fallback; old journals must parse and render exactly as before.

Read the diff at <PATH>. Audit for: (1) wire-format back-compat breaks in
either direction, (2) incorrect attribution logic in ganttModel/radioAwaitRig
(exact path, fallback path, and their interaction), (3) missing or wrong
executor emission context, (4) test gaps against the stated requirements,
(5) MSRV 1.75 violations or clippy -D warnings hazards. Output findings as
markdown: severity (P1/P2/P3), file:line, one-paragraph failure scenario each.
State explicitly if you find no findings in a category.
```

## Report skeleton

`report.md` in this directory: experiment summary → arm inventories (agent
count, model, rounds, tokens, wall time — the tables M1–M6 fill) → blind-eval
findings + triage → mechanical verdict → nuance → transferability notes for the
work proposal (what breaks at higher stakes, what the seat-cost math looks
like).
