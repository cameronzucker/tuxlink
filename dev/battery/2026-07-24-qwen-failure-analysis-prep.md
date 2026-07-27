# Self-brief: qwen failure-mode analysis (post-compaction pickup)

Notes-to-self written by tanager-owl-cardinal before a manual context compaction.
TASK after compaction: run a **failure-MODE analysis** (mechanism / the *why*) of
the qwen-3.5-122b instrumented battery run, using the captured trajectories +
reasoning, at the reliability bar (rates, not single observations). This mirrors
the prior `lift-corrected-1` modality analysis but for the instrumented run.

## Ground rules (do not repeat this session's mistakes)

- `completed` != correct. `sg` (saved+green) != correct. Judge each cell against
  its **predicates** (`score.json` -> `judge_input.predicates`), not the scorer's
  saved/green flags. Green routines routinely DROP prompt requirements.
- A single run is NOT a finding. Use the **reliability sweep** (N=5) for rates.
  Do not draw cross-arm / cross-model / skill-vs-base conclusions from one obs.
- Failure MODE = mechanism. Read `tool_calls.jsonl` (trajectory) + `deltas.jsonl`
  (captured reasoning = the why) + final `Assistant` msg to determine WHY.
- Ground every source claim; verify before asserting (this session's whole theme).

## Where the data is (R2, `ssh r2-poe`, `cd ~/tuxlink-eig6e-build`)

- **Main run**: `battery-results/qwen-instrumented-1/{base,skill}/{CELL}/`
  - `outcome.json` (outcome, provider_turns, tool_calls, eval_tokens, detail=final msg)
  - `score.json` -> `deterministic{routine_saved,validates_green,verdict}` +
    `judge_input{predicates, artifacts.def}` (predicates = the rubric; def = saved routine)
  - `tool_calls.jsonl` (per call: `tool`, `status`, `args`, `result_preview`)
  - `deltas.jsonl` (per line: `delta_kind` "reasoning"|"assistant", `text`) = REASONING
  - `transcript/*.jsonl` (`message` = User|ToolCall|ToolResult|Assistant)
  - `harness.log`
- **Reliability sweep**: `battery-results/qwen-reliability/{arm}/{CELL}/attempt-{1..5}/`
  (same files per attempt). Cells: `base:S1 base:S3 base:E2 skill:S1 skill:E1 skill:S3`, N=5.
  Check `battery-results/qwen-reliability/run.log` for `QWEN-RELIABILITY COMPLETE`.
  As of writing: ~5/30 attempts done, still running (qwen is slow; turn-cap cells ~15 min).
- **2nd qwen observation of P1-S4** (different day): `battery-results/lift-corrected-1/`.
- **GLM** (not the qwen task, for reference): `battery-results/glm52-3/` = 15/36 ran,
  21 credit-blocked (HTTP 402). Unjudged.
- Total size small (qwen-instrumented-1 ~5.7M, qwen-reliability ~0.7M). R2-ONLY,
  not committed. If R2 gets cleaned, pull first: `scp -r r2-poe:.../battery-results/qwen-* .`

## Tooling

- Scorer: `src-tauri/target/debug/elmer_score --root <dir> --corpus tests/battery/corpus.json`
  (pure file IO; writes score.json + judge-queue.jsonl per bundle). Re-run to (re)score.
- Corpus: `tests/battery/corpus.json`. IMPORTANT loader gotcha: it is a dict; cells are
  under key `prompts` (NOT `cells`): `raw if isinstance(raw,list) else (raw.get("cells") or raw.get("prompts") or [])`.
  Each cell: `id,title,prompt,predicates,reference`. 18 cells: P1-3 S1-4 A1-2 C1-3 E1-3 EU1-3.
- Binary provenance: origin/main 0ae53b5e + this session's harness patches. Has
  reasoning capture. Rebuild on R2: `cargo build --manifest-path src-tauri/Cargo.toml --bin elmer_battery` (NO cargo on the dev Pi).

## Committed artifacts (branch `bd-tuxlink-kz4rg/lift-ladder-iter`, pushed to origin)

- `dev/battery/2026-07-24-instrumented-run-data.md` = the judged SINGLE-observation
  data for both qwen arms (verdicts + 1-line routines + notes). Start here.
- `dev/battery/2026-07-24-lift-corrected-1-failure-modalities.md` = prior-run modality
  analysis + two-front convergence (the m5oia loop / false-infeasibility / etc.).
- Harness fixes: `30cd7608` reasoning capture (deltas.jsonl), `97e57c9c`
  OPENROUTER_PROVIDER_ORDER, `5f241cc5` ELMER_MAX_TOKENS. ADR 0025 amendment corrected.

## qwen failure inventory (this run) + SUSPECTED modes (VERIFY across N=5, do not assume)

Hard failures (harness-level):
- `base/S3` invalid_action (1 call, built nothing) -- suspect malformed/stringified save.
- `base/E2` cancelled (turn-cap 40) -- suspect edit/branch loop.
- `base/EU2`, `base/EU3` built nothing -- diagnostic/help-style prompts.
- `skill/S1` cancelled (turn-cap 40), `skill/E1` cancelled (turn-cap 40).
- `skill/S3` bail (built nothing, 6 calls, reasoning present) -- suspect false-infeasibility
  (concludes catalog-receive needs a receive-only action; base/S3 solved it via 2nd connect).
- `skill/C1`, `skill/EU3` built nothing.
Judge-fails despite `completed`/`sg` (green-but-incomplete): many. Dominant SUSPECTED
pattern to confirm: routine defaults `trigger:manual` when the prompt asked
recurring/daily/regular; skips required setup steps (rig.apply_preset, rig.tune_atu);
no explicit send action; hardcodes a callsign instead of find_stations (skill/EU2).
See the data-record doc for the full per-cell verdicts.

The analysis output should be: per failure MODE, the mechanism (evidenced by
reasoning+trajectory), which cells exhibit it, and its RATE from the N=5 sweep
(when complete). Then attribute each mode: model-capability vs teaching-layer vs
Tuxlink product vs harness. Record durably (commit) as a proper findings doc.

## Env / access

- OpenRouter key (for GLM only, not qwen): `secret-tool lookup service elmer-openrouter account teacher`.
- GLM env knobs: `OPENROUTER_PROVIDER_ORDER=streamlake,z-ai`, `ELMER_MAX_TOKENS=32000`.
- Worktree: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-kz4rg-lift-ladder-iter`
  (cd here for git; STANDALONE `cd` first per the race hook; `node_modules` installed so push works).
- Moniker for commits: `tanager-owl-cardinal`.

## Open threads

1. qwen reliability sweep completing (rates -> update the data-record doc).
2. GLM 21 cells credit-blocked (402) + 15 unjudged -- needs operator OpenRouter top-up.
3. Reliability rule now standing (memory `feedback_reliability_over_binary_rerun_on_fail`).
