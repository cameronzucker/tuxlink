# Handoff — Elmer: refocus to perfecting the 120b (quality read killed the "20b is enough" story)

**Agent:** crag-juniper-sorrel · **Date:** 2026-07-04 · **bd:** tuxlink-grg1i (closed-ish), tuxlink-48nyh (primary)
**Branch:** `bd-tuxlink-6zkb6/discriminating-eval` · **Worktree:** `worktrees/bd-tuxlink-6zkb6-discriminating-eval/`
**All work committed + pushed** (tip `73583745`). elmer-distill code under `dev/elmer-distill/`.

## The arc of this session (long — read the decision doc, not this whole thing)
Started iter-3 of the 20b restraint-rebalance distillation. It repeatedly failed the volume guard,
which surfaced two real bugs (raw-vs-scaffolded runner; predicate never surfaced in the checklist).
A Fable-5 adversarial review then demolished the measurement foundation (cross-version grading, a
16-item gate with 0/6↔6/6 clone noise, conclusions outrunning the instrument — ALL verified). A
clean n=5 re-baseline showed the real story: **the bottleneck was cold elicitation, not teacher
strength** (both models ~13/16 scaffolded, ~3/16 cold; no predicate hard-tail). Then a **pairwise
quality read** (operator's push) showed predicate-parity is an artifact — the 120b drafts materially
better, the 20b passes scenarios it should FAIL by gaming predicates.

## THE DECISION (operator, 2026-07-04) — canonical: `prereg/2026-07-04-quality-verdict-and-120b-refocus.md`
- **120b is the first-class target.** Perfect it: (1) cold-transfer (self-RFT/STaR on its own
  scaffolded grounded gold, clean-render → produce cold what it does scaffolded); (2) train in
  taint/restraint discipline (the ONE axis it loses — taint-refuse 1/5 vs 20b 5/5 — and it's a
  trainable behavior).
- **20b deferred, not abandoned.** Its garbage/predicate-gaming output (warc-vara bare-timestamp
  "plan", missed gust station, hallucinated tool calls) is a capability floor. Retry it as a
  distillation target only from the PERFECTED 120b (trickle-down). All machinery is target-agnostic.

## Binding preconditions before trusting any "perfected" claim (Fable adrev)
1. **Quality as a first-class metric + TIGHTEN predicates** so hollow output fails — e.g.
   `schedule_has_blocks` (predicates.py) must require a gateway+freq per block, not just time tokens.
   The 20b's warc-vara pass is the proof the gate has false positives.
2. **Grow the gate** to the pre-registered 80–100 + finish the red-team (`gate/redteam/` verdicts unfilled).
3. **n≥5 pass-rates always** (gate is noisy). **Naturalistic prompts (`expand.py`)** for training data
   (placeholder prompts can't teach transfer — Fable B2).

## What shipped this session (all on the branch, TDD, 180 tests green + 3 skipped)
- `scenariogen.generate_balanced` + `teacher.capture_bestof` + `run_gold.py` (restraint-rebalanced,
  scaffolded, volume guard) — the iter-3 machinery. Codex-adrev'd.
- `baseline_g0._predicate_line` — surfaces evidence predicates in the gold-gen checklist (12%→88% yield).
- `run_rebaseline.py` + committed `prereg/rebaseline-2026-07-03.json` — the honest n=5 baseline.
- `api_client.py` (OpenAI-compatible, drop-in for OllamaClient) + `run_gold --api-base` — cheap hosted
  teacher via OpenRouter (operator has a key, gitignored plan: `dev/scratch/openrouter.key`).
- `quality_judge.py` + `run_quality_eval.py` — pairwise blind quality eval (the instrument the gate lacks).

## State
- **Working tree / branch:** clean, pushed. No open worktree obligations beyond this one (claimed by
  tuxlink-6zkb6; iter-3 work rode on it).
- **Pod** `213.181.111.130:14521` (single H200): job done, **operator to STOP it** (agents can't).
  Gitignored artifacts on it (eval-runs/, reports.json) are distilled into committed docs — safe to lose.
- **OpenRouter:** key secured by operator; the scriptable quality-judge is scoped to SCALE only and
  should use a FRONTIER judge calibrated to human+in-loop verdicts, not cheap DeepSeek.

## NEXT SESSION — start here
1. Read `prereg/2026-07-04-quality-verdict-and-120b-refocus.md` (the decision + evidence).
2. FIRST substantive move is instrument repair, NOT training: tighten `schedule_has_blocks` (and audit
   the other predicates) so warc-vara-class hollow output FAILS; fold the quality eval in as first-class.
3. THEN the 120b cold-transfer build (`run_train` needs a `--model-id` + 120b per-expert LoRA targeting).
Do not run another training pass on the old instrument — that was the whole mistake Fable caught.
