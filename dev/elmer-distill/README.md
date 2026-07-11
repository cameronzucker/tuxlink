# elmer-distill — data-gen + eval foundation for the Elmer tool-use distillation

**Teacher → student: Qwen 235B (or 397B) → Qwen 3 Coder Next** — in-family knowledge
distillation. A larger same-family Qwen teacher distills into the smaller, distinct
Qwen 3 Coder Next student. This directory is the CPU/local data-generation +
evaluation foundation for the Elmer tool-use distillation epic.

Design: [`docs/superpowers/specs/2026-07-02-elmer-20b-tooluse-distillation-design.md`](../../docs/superpowers/specs/2026-07-02-elmer-20b-tooluse-distillation-design.md).
Plan: [`docs/superpowers/plans/2026-07-02-elmer-20b-distillation-foundation.md`](../../docs/superpowers/plans/2026-07-02-elmer-20b-distillation-foundation.md).
bd: `tuxlink-ct08v`.

> **Status — updated 2026-07-11.** The model pairing and training status supersede
> this doc's original gpt-oss framing.
>
> - **The pairing is Qwen, not gpt-oss.** The teacher is a larger same-family Qwen —
>   **235B** or **397B** — and the student is **Qwen 3 Coder Next**. This is
>   *in-family knowledge distillation*, not self-distillation: teacher and student
>   are distinct-capacity models, so the teacher clears the student by a real
>   margin. That margin is the point — it gives a higher ceiling than
>   gpt-oss-120b → 20b and fixes the teacher-ceiling plateau that stalled the
>   gpt-oss track (there, teacher ≈ base student, so nothing transferred).
> - **Phase A — the training run — has been executed.** It is no longer the deferred
>   follow-up described under [Historical: Phase A scope](#historical-phase-a-scope)
>   (retained below as the original record).
> - **Tuxlink is its own simulator.** The training *data* is self-generated: gold
>   trajectories come from exercising Tuxlink's live tool surface and the
>   armed-authority / taint / outbox state machine, not an outside corpus.
>   Self-generated data, external larger teacher.
> - The data-gen + eval **substrate is model-agnostic** (the runners take a
>   `--model` / `--model-id`). Only the render + tokenizer path is model-specific:
>   `harmony.py` and the `o200k_base` vocab are the **gpt-oss** render target,
>   whereas a Qwen target uses Qwen's own chat template and tokenizer. Confirm the
>   live Qwen render path before relying on the gpt-oss-specific notes in
>   [Modules](#modules-srcelmer_distill) and [Running the tests](#running-the-tests).

## Modules (`src/elmer_distill/`)

| Module | Role |
|---|---|
| `tool_surface.py` | Classify the 50-tool surface (taint / egress / tier2 / staging / stop). |
| `scenario.py` | `Scenario` + machine-readable `SuccessSpec`. |
| `simulator.py` | `StatefulSimulator`: armed-authority + taint + outbox state machine. |
| `judge.py` | Score a trajectory vs a `SuccessSpec` (order / staging / egress replay). |
| `harmony.py` | Render trajectories to a model-specific training format + round-trip. Ships the gpt-oss **Harmony** target; a Qwen target uses Qwen's chat template. |
| `scenariogen.py` | Scenario bank (coverage cells) + task-graph holdout split. |
| `ollama_client.py` / `teacher.py` | G1 teacher-capture (gold yield per cell) from the configured teacher model. |
| `baseline_g0.py` | G0 prompt-only baseline (few-shot + checklist + verifier loop). |
| `dataset.py` | Assemble gold → rendered JSONL + assistant-loss spans + P95 seq stats. |

`tool_surface`, `scenario`, `simulator`, `judge`, `scenariogen`, and `dataset` are
model-agnostic — they define the environment both model families train against. The
model-specific surface is `harmony.py` (render format) plus the tokenizer used for
sequence stats.

## Running the tests

```bash
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
PYTHONPATH=src .venv/bin/python -m pytest -q
```

The gpt-oss render tests need the `o200k_base` vocab. On the pod/CI it downloads
automatically; on a restricted host set `TIKTOKEN_ENCODINGS_BASE` to a directory
containing `o200k_base.tiktoken` (the suite's `conftest.py` autowires a local copy
if found under `/tmp/**/tiktoken_base`, else those tests skip). The Qwen render path
uses Qwen's own tokenizer rather than `o200k_base`.

## Gate status

- **G2 — stateful judge validated: CLEARED.** `test_judge_negatives.py` proves the judge
  *rejects* five known-bad trajectories (stall, tainted egress, skipped outbox, wrong order, wrong
  recipient), two of them on exactly one reason. Per the spec, GPU work was blocked until this passed.
  The judge is model-agnostic, so this gate holds across teacher families.
- **G0 / G1 — runnable against the staged pod** (spec §12): `baseline_g0.run_g0` and
  `teacher.capture` drive the configured teacher/student via `OllamaClient` — the
  Qwen 235B/397B → Coder Next pair (originally gpt-oss:120b/:20b).
- **Pre-registration frozen:** [`prereg/2026-07-02-eval-preregistration.md`](prereg/2026-07-02-eval-preregistration.md).

## Historical: Phase A scope

_This section records the original foundation scope. Phase A has since been executed
(see the Status note above); it is kept for the reasoning trail._

**Phase A (the LoRA training run) was a deliberate follow-up plan**, to be written only
after G1 produced gold yield per coverage cell and G3 produced the P95 rendered-length +
teacher s/task — so the training config was set from real measurements, not placeholder
hyperparameters. The pre-registered fallback was: if G0 cleared the bar, ship the scaffold
and skip training entirely. The Qwen 235B/397B → Coder Next run resolved this toward
training.
