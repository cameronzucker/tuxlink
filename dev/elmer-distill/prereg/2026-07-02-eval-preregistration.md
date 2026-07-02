# Elmer-20b Distillation — Eval Pre-Registration (FROZEN)

- **Frozen:** 2026-07-02, before any GPU spend, by cypress-finch-willow.
- **bd:** tuxlink-ct08v
- **Purpose (spec §3):** fix the metrics, acceptance margins, seeds, and splits *before* running
  G0/G1/A so the eval cannot be retrofitted to a favorable result. Changing anything below after a
  run invalidates that run.

## Metrics (all from `elmer_distill.judge.Judge`)

- **Primary — task pass-rate:** fraction of blind-holdout scenarios with `Verdict.passed == True`
  (all required tools called, ordering satisfied, every staged item complete, taint/armed-authority
  respected, reached a final answer).
- **Secondary:**
  - **stall-rate:** fraction of runs whose final turn still has tool_calls OR that hit the turn
    budget without a final assistant answer (the specific "stops after the second tool call"
    failure).
  - **tool-sequence correctness:** fraction of ordering edges satisfied across the holdout.
  - **garbage ratio:** non-ASCII ratio of final answers (regression guard).
- **Regression probe:** a small off-domain general-ability set, scored for gross degradation
  (catastrophic-forgetting guard).

## Pre-registered acceptance margins

An intervention (G0 scaffold, or a Phase-A LoRA) is accepted **only if**, on the **blind holdout**:

1. task pass-rate beats the frozen **G0 baseline** pass-rate by **≥ 20 absolute percentage points**, AND
2. stall-rate is cut by **≥ 50% relative** to the G0 baseline, AND
3. garbage ratio does not regress (≤ baseline + 1 absolute point), AND
4. no gross regression on the general-ability probe.

**Gate order (spec §5):** G0 (prompt-only) is measured first. If G0 alone clears the bar vs base-20b,
**ship the scaffold — no fine-tune.** GPU training proceeds only if G0 fails to clear it AND the G1
teacher-ceiling + G3 cost/seq pilots pass.

## Frozen seeds & split

- Scenario generation: `elmer_distill.scenariogen.generate(seed=1, n_per_cell=<set at G1 sizing>)`.
- Holdout split: `split_by_task_graph(scenarios, holdout_frac=0.18, seed=0)` — by **task-graph
  signature**, so holdout shares no task graph with training (Codex adrev I).

## Blind final fixtures (never inspected during selection)

- `emcomm-cmdpost-01` (the named blended command-post fixture; `tests/fixtures/scenarios/`).
- One additional `blended` depth-6 scenario, drawn from the holdout split and **sequestered** — not
  used for any hyperparameter or checkpoint selection.

Held-out fixtures are scored once, at the end, to produce the reported numbers. Any inspection of a
blind fixture during iteration converts it to a training/dev item and forfeits its blind status.

## What is frozen

Generator code (`scenariogen.py`), judge code (`judge.py` + `simulator.py`), the negative-test
corpus (`test_judge_negatives.py`), the Harmony renderer (`harmony.py`), the seeds and split
parameters above, and these margins. Material changes require a new dated pre-registration.
