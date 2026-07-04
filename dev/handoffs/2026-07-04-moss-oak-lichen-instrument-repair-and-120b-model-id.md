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

### Commit `1155964e` — 120b build, mechanical first step
- `run_train.py --model-id` (default 20b; `unsloth/gpt-oss-120b` for the build). Per-expert LoRA
  targeting was ALREADY model-agnostic (dynamic regex discovery), so nothing else changes 20b→120b.
- Extracted `expert_suffixes` / `is_router_param` as pure helpers + GPU-free tests: every expert index
  captured at 20b (32) AND 120b (128) scale, router stays frozen, empty-discovery is a hard error.

**Tests:** 202 passed, 3 skipped (elmer-distill suite). Run from `dev/elmer-distill/` with
`PYTHONPATH=src python3 -m pytest tests/ -q`. (cwd reverts in worktree sessions — `cd` to elmer-distill
explicitly; the repo-root `tests/converge_build_fixtures_test.py` failures are pre-existing + unrelated.)

## NEXT SESSION — the substantive 120b build (pod-gated + needs a design pass)
This is the real remaining work in tuxlink-48nyh. It is NOT mechanical — design it before coding:
1. **Cold transfer via self-RFT / STaR:** generate the 120b's own judge-PASSING scaffolded gold
   (it's ~13/16 scaffolded), render it CLEAN (strip the checklist scaffolding), train the 120b to
   produce cold what it does scaffolded. Closes the ~9-pt cold→scaffold gap. Machinery exists:
   `baseline_g0.run_g0` (scaffolded gen), `judge` (filter), `dataset.assemble` (loss-masked Harmony),
   `run_train --model-id unsloth/gpt-oss-120b`. Missing: the STaR data-gen driver that ties them.
2. **Taint / restraint gold:** the ONE axis the 120b loses (taint-refuse 1/5 vs 20b 5/5) and it's
   trainable. Curate refusal exemplars for the taint scenarios (or borrow the 20b's better restraint
   trajectories for those cells).
3. Apply `expand.py` naturalistic prompts to any training data (info-free placeholders can't teach
   transfer — Fable B2). Keep n≥5 gate rates.
4. **The pod is DOWN** (`213.181.111.130:14521` was the single-H200 job pod; operator stops/re-provisions;
   agents can't). Re-provision when ready to train. All eval-runs/ artifacts on it are gitignored +
   distilled into committed docs — safe to lose.

## State
- **Working tree / branch:** clean, pushed (tip `1155964e`). One untracked dir on disk:
  `dev/elmer-distill/dev/adversarial/2026-07-04-schedule-grounding-codex.md` — the Codex transcript,
  intentionally local-only (gitignore policy). No other worktree obligations beyond this one.
- **Do NOT re-run training on the OLD instrument** — that was the mistake Fable caught. The instrument
  is now repaired; the next run is the 120b cold-transfer build on the REPAIRED gate.
