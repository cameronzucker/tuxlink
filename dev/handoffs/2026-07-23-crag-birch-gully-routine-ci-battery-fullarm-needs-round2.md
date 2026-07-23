# Session handoff — crag-birch-gully (2026-07-23)

Marathon session. Built the Routine CI slice-1a **battery** (Base/MatchedControl/Full
arms + a scoring harness), merged it, ran two R2 pilots, fixed what they caught.
**The harness is ~80% validated; the Full arm — the crux of the whole experiment —
still needs one more robustness round before the Stage-1 run is meaningful.** Handing
off here because the full run wants a clean context (operator's call).

## THE ONE THING TO KNOW

Do **NOT** launch the full Stage-1 battery yet. The **Full arm produces no routine on
real qwen** — its Intent phase parse fails. Two more fixes (F1b + F2b below) + a
re-pilot are needed first. Base + MatchedControl arms + grading + Claude-as-judge all
work; the plumbing is solid.

## What is MERGED to main (both landed, CI-green)

- **PR #1241 → `2c4cfa5d`**: slice-1a. The workflow module (`src-tauri/src/elmer/workflow/`),
  the battery arms in `src-tauri/src/bin/elmer_battery.rs`, the scoring binary
  `src-tauri/src/bin/elmer_score.rs`, the 18-task corpus `tests/battery/corpus.json`,
  and the matrix runner `dev/battery/run-matrix.sh`.
- **PR #1243 → `bb348f8d`**: pilot fixes (bd tuxlink-ch4po): F1 (intent_text threading),
  F2 (context budget), judge=Claude refactor, xvfb wrapping. All reviewed + CI-green.

## What is VALIDATED (two R2 pilots, ~$1 total)

- **Plumbing**: `run-matrix.sh` (auto `xvfb-run -a` when headless) → per-cell bundles →
  `elmer_score` → `score.json` + `scores.jsonl` + `judge-queue.jsonl`. 6/6 cells run.
- **Base + MatchedControl arms**: produce validating routines on satisfiable tasks
  (base/A2 saved `daily-vara-send.json`, deterministic `pass`).
- **Grading**: deterministic layer sound (EU3 no_routine → `n/a`, never failed; A2 valid
  routine → `pass`). Pilot #1's sonnet judge caught A2 det-pass but semantically-wrong
  ("find_stations limit 5, no distance narrowing → fails 'best gateway'") — the det/judge
  divergence the methodology wants.
- **Claude-as-judge**: `elmer_score` no longer calls any API; it emits a `judge_input`
  package per cell (`judge:null` placeholder) + a `judge-queue.jsonl` roll-up. The
  orchestrating Claude agent reads the queue and fills verdicts. (Operator: "you should
  be the judge. You're paid for.")
- **Cost model**: real credits, not token estimate (which overshoots ~3.8× via caching).

## What still BROKEN — must fix before Stage 1 (the Full arm)

### F1b — Full-arm Intent phase parse fails on real qwen output
- Symptom (re-pilot): `full/A2` and `full/EU3` stop at Intent with
  `phase artifact did not parse: invalid type: map, expected a string`. workflow_run has
  only [router, intent], savedRoutine=null. So the Full arm saves NOTHING → the whole
  Base-vs-Full measurement is impossible until fixed.
- Diagnosis: F1 (intent_text threading, PR #1243) WORKED — the error moved from "missing
  field outcome" to "invalid type: map, expected a string", i.e. the model now sees the
  request and answers, but nests an OBJECT where the `Intent` schema wants a flat String
  (Intent fields outcome/trigger/success/failure are `String`; qwen emits e.g.
  `"trigger": {"schedule": ...}`). `parse_artifact` (phases.rs:220) does
  `serde_json::from_str -> parse_if_string(Object) -> from_value::<Intent>` and the
  from_value rejects the map-where-string.
- FIX options (do both): (a) SHOW the exact expected JSON shape + field types in each
  phase's `phase_instruction` (phases.rs) so the model emits flat strings; (b) make
  `parse_artifact` tolerant — when a String field gets an object/array, coerce it to its
  compact JSON string. Iterate against REAL qwen output.
- DEBUG AID needed: the Full arm's phases use `NullTranscript` (13a `SessionPhaseModel.run_phase`),
  so the raw Intent JSON is NOT captured in the bundle. Wire the battery's transcript sink
  into the phase runs (or persist each phase's `final_text` into the bundle) so you can SEE
  what the model produced. Repro: one qwen `full/A2` cell (free, local).

### F2b — context guard still overflows on a ballooning cell
- Symptom (re-pilot): `matched-control/EU3` → provider_error HTTP 400: "requested 4096
  output tokens and your prompt contains at least 258049 input tokens ... 262145 > 262144".
- Diagnosis: F2 (PR #1243) now SETS max_tokens=4096 (was 0 → the original bug is gone), but
  its byte/3 prompt estimate UNDER-counts, so a cell whose real prompt is 258k slips past the
  guard and 258049+4096 > 262144. Also the qwen `EU3` prompt ballooning to 258k in a few
  turns is itself suspect (verbose looping on the diagnosis task?) — worth understanding.
- FIX: harden the estimate (real tokenizer, or trust the server's returned usage to trim
  next turn) + confirm `resolve_window()` returns Some for this vLLM endpoint (reviewer F2
  minor #2). Consider a smaller num_ctx cap for qwen. Investigate the EU3 balloon.

## THE FULL-RUN CONFIG (ready once F1b/F2b land)

R2: `ssh r2-poe` (key `~/.ssh/id_ed25519_r2poe`). Build dir `~/tuxlink-battery-build`
(tracks origin/main detached; advance: `git fetch origin main && git checkout -q origin/main`;
if a local run-matrix.sh edit blocks checkout, `git checkout -- dev/battery/run-matrix.sh`
first). Build: `export PATH=$HOME/.cargo/bin:$PATH && cargo build --manifest-path
src-tauri/Cargo.toml --bin elmer_battery --bin elmer_score` (~45s incremental). vLLM qwen is
UP at `https://inference.twin-bramble.ts.net/v1/chat/completions` serving `qwen35-122b-nvfp4`
(256k ctx), no auth. Cells MUST run under `xvfb-run` (run-matrix.sh does this auto).

**Stage-1 models.tsv (NO sonnet — operator: run cheap first, sonnet fast-follow). Endpoint =
full /v1/chat/completions. Key col = the env var re-exported as OPENROUTER_API_KEY per model.**
```
qwen35      qwen35-122b-nvfp4                  https://inference.twin-bramble.ts.net/v1/chat/completions  TWIN_BRAMBLE_KEY
glm52       z-ai/glm-5.2                       https://openrouter.ai/api/v1/chat/completions              OPENROUTER_API_KEY
gptoss120b  openai/gpt-oss-120b                https://openrouter.ai/api/v1/chat/completions              OPENROUTER_API_KEY
nemotron    nvidia/nemotron-3-super-120b-a12b  https://openrouter.ai/api/v1/chat/completions              OPENROUTER_API_KEY
```
- Export `TWIN_BRAMBLE_KEY=EMPTY` (qwen vLLM is open) + `OPENROUTER_API_KEY` from the Pi
  keyring PIPED to R2 env, never disk: `secret-tool lookup service elmer-openrouter | ssh
  r2-poe 'IFS= read -r K; export OPENROUTER_API_KEY="$K"; ...'`.
- OpenRouter pricing (per Mtok): glm-5.2 $0.83/$2.60 (priciest); gpt-oss-120b $0.04/$0.17;
  nemotron $0.08/$0.45 (has a `:free` variant too). qwen local = $0. **Full Stage-1 ≈ $2–4.**
- Run: `bash dev/battery/run-matrix.sh --models <tsv> --corpus tests/battery/corpus.json
  --out battery-results/stage1 --bin src-tauri/target/debug/elmer_battery --cell-ceiling-usd
  5 --turn-cap 40 --turn-timeout-secs 180` (all 3 arms, all 18 tasks by default; ~216 cells;
  hours of wall-clock; idempotent — re-runnable, skips existing outcome.json). Ping operator
  at $25 OpenRouter spend (won't happen at ~$4). $45 ledger hard-stop is the backstop.
- Score: `src-tauri/target/debug/elmer_score --root battery-results/stage1 --corpus
  tests/battery/corpus.json` (NO --judge-* flags anymore — pure file I/O). Produces
  score.json/cell + scores.jsonl + judge-queue.jsonl.
- **JUDGE (Claude, YOU): read judge-queue.jsonl; for each cell emit verdict per its rubric
  (predicates + expected_gap + classification): pass|fail|gap-honest|confabulated|
  non-routine-handled|non-routine-confabulated + honest_about_gap + rationale. Fan out judge
  SUBAGENTS over batches for 216 cells. This is a MANUAL agent step with no automated
  backstop (reviewer minor) — do not skip it.** Corpus semantics (classification/expected_gap/
  judge_primary/no_routine_expected) are DRAFT — confirm with operator before trusting scores.

## bd + deferred

- bd `tuxlink-ch4po` (in_progress): pilot fixes; F1b/F2b remain → keep open or file a follow-up.
- Deferred slice-1a review MINORS (not filed yet): M1 elmer_score resolve_model fallback returns
  arm on corrupt manifest; M2 judge verdict unvalidated string; M3 collect_defs picks first def
  alphabetical; F2 minors (estimate under-count, window-resolution None). File as bd P3.
- **Adrev (tuxlink-u2qge) = Stage 2**, after Stage-1 results (operator: adrev pairings depend on
  which models the workflow elevated). NOT a Stage-1 blocker.

## Worktree / branch state

- `worktrees/bd-tuxlink-ch4po-pilot-fixes` (this handoff written here) — branch `pilot-fixes`
  is MERGED-DEAD (PR #1243 merged). Disposable via ADR-0009 ritual. Has node_modules.
- `worktrees/bd-tuxlink-w8zxt-routine-ci-1a` — slice-1a, MERGED-DEAD (PR #1241). Disposable.
  Its `.superpowers/sdd/progress.md` is the slice-1a build ledger + all review minors.
- Main checkout is the operator's (branch bd-tuxlink-ant8s); untouched.
- R2 `~/tuxlink-battery-build` on origin/main; `pilot/` has models-*.tsv + re-pilot-run.sh +
  results/ (pilot#1) + results2/ (re-pilot). `pilot/results2/qwen35/full/A2/` is the F1b repro.
