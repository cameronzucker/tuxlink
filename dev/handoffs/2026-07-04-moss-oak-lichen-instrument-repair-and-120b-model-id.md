# Handoff — Elmer: instrument repair done (predicate false-positive killed), 120b build teed up

**Agent:** moss-oak-lichen · **Date:** 2026-07-04 · **bd:** tuxlink-48nyh (primary, P0), tuxlink-6zkb6 (worktree owner)
**Branch:** `bd-tuxlink-6zkb6/discriminating-eval` · **Worktree:** `worktrees/bd-tuxlink-6zkb6-discriminating-eval/`
**All work committed + pushed** (tip `1155964e`). elmer-distill code under `dev/elmer-distill/`.

## What this session did (the CRITICAL GATE, satisfied)
Prior session's gate: **do NOT run training first — instrument repair first**, because the 20b
proved the gate has false positives (passed `warc-vara` with a bare-timestamp "plan" that gamed
`schedule_has_blocks`). That is done, thoroughly, TDD, Codex-adrev'd. No training was run.

### Commit `9cdb11f1` — instrument repair
- **`schedule_has_blocks` is now evidence-bound.** A block counts only if a real `find_stations`
  gateway callsign co-occurs with THAT gateway's own frequency (±1 kHz) in the same entry. Bare
  timestamps fail; the 120b's real band/station/freq plan passes. Time-FORMAT tolerance (HH:MM +
  hour ranges, unicode dash) preserved — the discriminator is gateway+freq presence, not clock
  notation. Signature changed to `(text, records, n)`; judge dispatch passes `find_stations` records.
- **The scaffold was TEACHING the hollow output.** `baseline_g0._predicate_line` literally said
  "write a schedule as HH:00 00:00 01:00 …"; `expand._PRED_GLOSS` was vague. Both now require naming
  the gateway + freq per block. Scaffold and gate now agree on "done".
- **Quality folded in as a FIRST-CLASS metric.** `quality_judge.combined_summary` + `run_quality_eval`
  now report the mechanical gate AND the pairwise quality win-rate in one run, and surface the
  **`parity_artifact`** cell — scenarios both models pass mechanically yet the 120b wins on quality
  (the exact blind spot that made predicate-parity look like "20b is enough").
- **Codex adrev** (`dev/adversarial/2026-07-04-schedule-grounding-codex.md`, gitignored) drove:
  - F1 substring-callsign false-pos (`NOTAA7WLX` ⊃ `AA7WL`) → bounded-callsign helper applied across
    `schedule_has_blocks` + `references_real_gateway` + `aprs_positions_cited` + `aprs_gust_alert_cited`
    (the operator's "audit the other predicates similarly").
  - F3/F4 false-negs on markdown-table + comma-in-block formats → entries split on newline/semicolon/
    bullet only (not comma/pipe), so good-model formats stop being under-credited.
  - F6 checklist/grader unit mismatch (kHz vs kHz-or-MHz) reconciled.
  - **Accepted residuals (documented + pinned by tests):** F2 coincidental-number (must also name the
    real gateway → basically doing the task), F5 continuation-line (rare; scaffold steers away from it).

### Commit `1155964e` — run_train `--model-id`
- `run_train.py --model-id` (default 20b; `unsloth/gpt-oss-120b` for the build). Per-expert LoRA
  targeting was ALREADY model-agnostic (dynamic regex discovery), so nothing else changes 20b→120b.
- Extracted `expert_suffixes` / `is_router_param` as pure helpers + GPU-free tests: every expert index
  captured at 20b (32) AND 120b (128) scale, router stays frozen, empty-discovery is a hard error.

### Commit `3b4fe72e` — mixed-source gold + naturalistic prompts (the "plus taint gold" half)
- **`gold_pipeline.capture_mixed`** routes the taint/restraint cells to a SEPARATE teacher (the 20b,
  5/5) and every other cell to the quality teacher (the 120b), merging gold with per-source
  `_teacher_model` provenance — the operator's "borrow the 20b's better restraint trajectories."
- **`run_gold --restraint-model gpt-oss:20b`** (borrow restraint) + **`--expand-prompts`** (apply
  expand.py so training PROMPTS are natural operator language). Both default OFF (preserves the iter
  composition-isolation); the 120b build turns them on. The clean-render for cold-transfer already
  existed (run_g0 scaffolds the teacher's context only; saved trajectory is clean).

**Tests:** 206 passed, 3 skipped (elmer-distill suite). Run from `dev/elmer-distill/` with
`PYTHONPATH=src python3 -m pytest tests/ -q`. (cwd reverts in worktree sessions — `cd` to elmer-distill
explicitly; the repo-root `tests/converge_build_fixtures_test.py` failures are pre-existing + unrelated.)

## NEXT SESSION — the 120b build is now CODE-COMPLETE; what remains is pod-gated
The first 120b cold-transfer run is fully wired end-to-end (all unit-tested with fakes; no GPU used):

```
# on the pod (both gpt-oss:120b AND gpt-oss:20b loaded in ollama):
python3 run_rebaseline.py --repeats 5 --out prereg/rebaseline-2026-07-XX-repaired-gate.json  # FIRST: re-baseline on the REPAIRED gate
python3 run_gold.py --model gpt-oss:120b --restraint-model gpt-oss:20b --expand-prompts --out eval-runs/gold-120b
python3 run_assemble.py --gold eval-runs/gold-120b/gold --out eval-runs/train-120b.jsonl
python3 run_train.py --model-id unsloth/gpt-oss-120b --data eval-runs/train-120b.jsonl --out /root/elmer-train/adapter-120b --precision 4bit --r 32
# ^ 4bit is MANDATORY: bf16 is blocked (tuxlink-5tfkm — MXFP4 experts only unpack to per-expert
#   Linear4bit in the 4bit path; bf16 keeps them fused so the expert-LoRA regex finds nothing) AND
#   a bf16 120b base (~240GB) won't fit one 144GB H200 anyway.
```

**OPEN (unbuilt) — serving + eval-ing the trained 120b.** `run_serve.py` / `smoke/gguf_export.py`
are hardcoded to the 20b and reload the base in **bf16 to merge the adapter** before GGUF export —
for a 120b that is ~240GB and will NOT fit one H200. So the 20b's serve→ollama→run_eval acceptance
path does NOT transfer. Options to decide before the run: (a) 2×H200 pod for the bf16 merge; (b) an
in-process peft eval (load base-4bit + adapter via transformers/peft, drive the agentic loop COLD,
judge) — fits one H200, avoids the merge wall entirely, and directly measures cold-transfer; (c) a
4bit-quantized GGUF export path. (b) is the cleanest but is a new ~run_eval-sized module. Everything
UP TO the trained adapter (re-baseline → gold → assemble → train) is wired + fits one H200 at 4bit.

1. **Re-baseline FIRST on the repaired gate** (`run_rebaseline --repeats 5`) — the gate changed this
   session (schedule false-positive killed), so the old 13.2/13.8 numbers are stale. Do NOT train
   against stale numbers (the mistake Fable caught). n≥5 mandatory (gate is noisy).
2. **Run the pipeline above.** Watch `run_gold`'s report: `mixed_teachers` provenance + the
   `--min-volume 118` floor + `gold_restraint_frac`. If the 120b's quality-cell yield or the 20b's
   restraint-cell yield underfills, raise `--n` (note it in the report).
3. **Acceptance:** run_eval on the frozen gate + `run_quality_eval` — check the NEW `parity_artifact`
   field shrinks (the 120b, post-train, should stop losing quality where it mechanically ties).
4. **Optional future:** the ITERATIVE STaR outer loop (train → re-generate with the improved 120b →
   refilter → retrain) is not built — single-round self-RFT is what's wired. Add only if one round
   underperforms.
5. **The pod is DOWN** (`213.181.111.130:14521` was the single-H200 pod; operator stops/re-provisions;
   agents can't). All eval-runs/ artifacts on it are gitignored + distilled into committed docs — safe
   to lose. Re-provision before any train.

## State
- **Working tree / branch:** clean, pushed (tip `1155964e`). One untracked dir on disk:
  `dev/elmer-distill/dev/adversarial/2026-07-04-schedule-grounding-codex.md` — the Codex transcript,
  intentionally local-only (gitignore policy). No other worktree obligations beyond this one.
- **Do NOT re-run training on the OLD instrument** — that was the mistake Fable caught. The instrument
  is now repaired; the next run is the 120b cold-transfer build on the REPAIRED gate.
