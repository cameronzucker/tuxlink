# Ladder 2 runbook (qwen build + Nemotron adversarial review + qwen revise)

Author: tanager-owl-cardinal. This is the pickup doc: a fresh session, or this
session after a token-quota reset, can monitor / resume / grade from here alone.
The **run executes on R2 detached and does not depend on any agent loop** — an
agent token wall does not stop it.

## Design (what is running)

- **Builder (subject):** `qwen35-122b-nvfp4`, local Spark vLLM
  (`https://inference.twin-bramble.ts.net/v1/chat/completions`), keyless.
- **Skill factor:** `base` (raw prompt) and `skill` (Build-Carefully scaffold).
- **Review factor (3 conditions):** `none` (baseline = the build itself),
  `rev_off`, `rev_on` (Nemotron adversarial QA then a qwen revise; reasoning
  OFF vs ON is the factor).
- **Reviewer:** `nvidia/nemotron-3-super-120b-a12b` via OpenRouter, **pinned to
  Nebius fp4 (NVFP4)** (`provider.order=[Nebius]`, `quantizations=[fp4]`),
  reasoning toggled per condition. Cheap ($0.30/$0.90 per M).
- **Corpus:** the 18 cells in `tests/battery/corpus.json`.
- **Grade-as-you-go:** the driver runs the DETERMINISTIC scorer per bundle
  (`elmer_score`). The **LLM backup / predicate judge is Sonnet 5, plan-based,
  decoupled** (see below) — it does NOT gate execution.
- **Determinism:** a bundle that DETERMINISTICALLY fails (outcome != completed
  OR routine not saved OR not validates-green) is re-run to N=3. Green-but-
  incomplete fails are caught by the Sonnet judge and re-run in a follow-up pass.

Per (cell, skill): ONE shared build (`none` = it), then rev_off / rev_on =
review+revise on that build's saved def.

## Where everything lives (R2: `ssh r2-poe`)

- Build tree / binaries: `~/tuxlink-eig6e-build` (branch
  `bd-tuxlink-kz4rg/lift-ladder-iter`). Binaries:
  `src-tauri/target/debug/{elmer_battery,elmer_score}`.
- Run root: `~/tuxlink-eig6e-build/battery-results/ladder2/` (gitignored).
  - `ladder2.sh` `review.py` `catalog.json` — the driver + reviewer + action
    catalog (canonical copies committed at `dev/battery/ladder2/`).
  - `run.log` — progress markers. `manifest.jsonl` — append-only checkpoint.
  - `nohup.log` — detached-launch stdout.
  - `<skill>/<cell>/build/attempt-{1..3}/` — build bundles (none = attempt-1).
  - `<skill>/<cell>/rev_{off,on}/attempt-{1..3}/` — revise bundles, each with
    `critique.txt` (Nemotron review), `critique.meta` (provider + reasoning
    trace), `user_prompt.txt`, `revise_prompt.txt`.
- Each bundle: `outcome.json`, `score.json` (deterministic + `judge_input`),
  `tool_calls.jsonl`, `transcript/*.jsonl`, `deltas.jsonl`, `routines/*.json`.

## Monitor

```bash
ssh r2-poe 'tail -20 ~/tuxlink-eig6e-build/battery-results/ladder2/run.log
  find ~/tuxlink-eig6e-build/battery-results/ladder2 -name score.json | wc -l   # bundles scored
  ps -eo pid,etime,comm | grep -E "ladder2|elmer_battery" | grep -v grep'
```
Expected total bundles at completion (no re-runs): 36 build + 72 revise = 108,
plus determinism re-runs. `run.log` ends with `LADDER2 COMPLETE`.

## Resume (if the detached driver died)

The driver is **idempotent**: a unit with `score.json` is done and skipped; a
crashed unit (dir, no `score.json`) is cleaned and redone. To resume, just
re-launch it with the key in env (below). Nothing is lost.

## Launch / relaunch (needs the OpenRouter key, never on disk / never on argv)

```bash
secret-tool lookup service elmer-openrouter account teacher | ssh r2-poe '
  read -r K; export ORKEY="$K" OPENROUTER_API_KEY="$K"
  cd ~/tuxlink-eig6e-build
  nohup bash battery-results/ladder2/ladder2.sh \
    > battery-results/ladder2/nohup.log 2>&1 &
  echo "ladder2 driver PID $!"'
```

The key lives only in the detached process env. `OPENROUTER_API_KEY` is required
by `elmer_battery` even for the Spark build (a presence check; the Spark ignores
the auth). Both `ORKEY` (used by `review.py`) and `OPENROUTER_API_KEY` must be
exported. To target a subset on resume, the driver honors nothing special —
it simply skips done units, so a plain relaunch resumes everything remaining.

Gotchas baked in (do not "fix" them away): `elmer_battery` needs `xvfb-run -a`
(GTK init) — the driver already wraps it. `elmer_battery` requires
`OPENROUTER_API_KEY` set even for the local Spark.

## Sonnet-5 plan-based judge pass (decoupled, quota-elastic)

The deterministic scorer runs in-loop. The **predicate judge is Sonnet 5 on the
plan** (NOT OpenRouter): dispatch Sonnet judge subagents (Agent tool,
`model: sonnet`) over the durable bundles whenever agent quota is available (now,
after reset, or a fresh session). It is pure-read over `score.json`'s
`judge_input` (predicates + saved def) — it never stalls execution.

For each bundle: judge the saved def against `judge_input.predicates` (NOT the
`saved`/`green` flags — green routines routinely drop requirements). Record a
verdict per (skill, cell, cond). Cells judged green-but-incomplete get a
follow-up determinism re-run (re-launch the driver after deleting those units'
`score.json`, or extend the driver's fail-detection).

## Analysis (after judging)

Compare pass RATES across the 6 conditions to answer: does Nemotron adversarial
review (reasoning off vs on) improve qwen's routines over no-review, and does the
skill scaffold interact with it. Use the determinism re-runs for rates, not
single observations. Record durably in `dev/battery/` and commit.
