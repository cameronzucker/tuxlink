# Stage-1 Pilot Pre-Registration (FROZEN) — DIRECTIONAL ONLY

- **Frozen:** 2026-07-02, before the pod calibration run, by cypress-finch-willow.
- **bd:** tuxlink-6zkb6
- **Status:** this is a **directional pilot**, NOT a powered acceptance claim.

## What this pilot can and cannot conclude (Codex C)

At the Stage-1 candidate count (~6-40 scenarios), the eval has **insufficient statistical power**
for an acceptance claim: at n=20 a +20pt lift has a 95% CI half-width ≈ 29 points. A powered
acceptance claim needs **~80-100 independent scenarios** (Stage 2). This pilot answers ONLY the two
go/no-go questions below.

## Frozen metrics

- **Judge pass** (`Verdict.passed`), scored by the upgraded evidence-bound judge (commit history on
  branch `bd-tuxlink-6zkb6/discriminating-eval`).
- Secondary (descriptive): stall count, honesty-violation count, per-family bucket counts.

## Frozen calibration procedure

- Runner: `elmer_distill.calibrate.calibrate` (single-shot / directional).
- Clients: `raw` = gpt-oss:20b, `self_review` = gpt-oss:20b, `teacher` = gpt-oss:120b (temp 0).
- Bucketing: `discriminating` (teacher pass, base fail), `too_easy` (base pass), `too_hard`
  (teacher fail). Teacher-fail audit labels each teacher-fail `invalid | human_solvable |
  above_teacher` — human-solvable ones are retained, not dropped.
- Discrimination-band target: raw base ~10-30%, teacher ~60-90% across the suite.

## Go/no-go for Stage 2 (both required)

1. **Gap exists:** a clear teacher≫base gap on a meaningful `discriminating` subset (not saturated:
   raw base must NOT already pass most scenarios; teacher must pass a solid majority).
2. **Training path works:** `smoke/micro_lora_smoke.py` passes all stages on the A100
   (load → correct LoRA targets incl. expert-MLP, router excluded → 10 steps → merge → GGUF →
   Ollama → a well-formed tool call).

If both hold → Stage 2 (scale gate to ~80-100 powered, full gold-gen, full LoRA, acceptance +
journey eval). If (1) fails → the gate/generator need more heat before any training. If (2) fails →
resolve the Unsloth/GGUF path before spending on gold-gen.

## Frozen inputs

- Gate candidates: `gate/candidates/*.json` at the frozen commit (post operator red-team + the
  operator-authored greenfield subset).
- Simulator: `elmer_distill.simulator` (deterministic mocks) at the frozen commit.
- No change to generator/judge/predicate code after freeze invalidates the run.
