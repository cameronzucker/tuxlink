# elmer-distill — foundation for distilling gpt-oss-120b → LoRA'd Elmer-20b

CPU/local data-generation + evaluation foundation for the Elmer-20b tool-use
distillation epic. Design: [`docs/superpowers/specs/2026-07-02-elmer-20b-tooluse-distillation-design.md`](../../docs/superpowers/specs/2026-07-02-elmer-20b-tooluse-distillation-design.md).
Plan: [`docs/superpowers/plans/2026-07-02-elmer-20b-distillation-foundation.md`](../../docs/superpowers/plans/2026-07-02-elmer-20b-distillation-foundation.md).
bd: `tuxlink-ct08v`.

## Modules (`src/elmer_distill/`)

| Module | Role |
|---|---|
| `tool_surface.py` | Classify the 50-tool surface (taint / egress / tier2 / staging / stop). |
| `scenario.py` | `Scenario` + machine-readable `SuccessSpec`. |
| `simulator.py` | `StatefulSimulator`: armed-authority + taint + outbox state machine. |
| `judge.py` | Score a trajectory vs a `SuccessSpec` (order / staging / egress replay). |
| `harmony.py` | Render trajectories to the gpt-oss **Harmony** training format + round-trip. |
| `scenariogen.py` | Scenario bank (coverage cells) + task-graph holdout split. |
| `ollama_client.py` / `teacher.py` | G1 teacher-capture (gold yield per cell). |
| `baseline_g0.py` | G0 prompt-only baseline (few-shot + checklist + verifier loop). |
| `dataset.py` | Assemble gold → Harmony JSONL + assistant-loss spans + P95 seq stats. |

## Running the tests

```bash
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
PYTHONPATH=src .venv/bin/python -m pytest -q
```

The Harmony tests need the gpt-oss `o200k_base` vocab. On the pod/CI it downloads
automatically; on a restricted host set `TIKTOKEN_ENCODINGS_BASE` to a directory
containing `o200k_base.tiktoken` (the suite's `conftest.py` autowires a local copy
if found under `/tmp/**/tiktoken_base`, else those tests skip).

## Gate status

- **G2 — stateful judge validated: CLEARED.** `test_judge_negatives.py` proves the judge
  *rejects* five known-bad trajectories (stall, tainted egress, skipped outbox, wrong order, wrong
  recipient), two of them on exactly one reason. Per the spec, GPU work was blocked until this passed.
- **G0 / G1 — runnable against the staged pod** (spec §12): `baseline_g0.run_g0` and
  `teacher.capture` drive gpt-oss:20b / :120b via `OllamaClient`.
- **Pre-registration frozen:** [`prereg/2026-07-02-eval-preregistration.md`](prereg/2026-07-02-eval-preregistration.md).

## Not in this foundation

**Phase A (the LoRA training run) is a deliberate follow-up plan**, to be written only after G1
produces gold yield per coverage cell and G3 produces the P95 rendered-Harmony length + 120b s/task —
so the training config is set from real measurements, not placeholder hyperparameters. If G0 clears
the pre-registered bar, we ship the scaffold and skip training entirely.
