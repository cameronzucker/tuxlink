# M2a — three-way harness regression spike (bd tuxlink-7raoe milestone 2a)

Operator-commissioned follow-up to the difficulty ladder
(`dev/research/2026-07-16-difficulty-ladder/`): the ladder attributed a large
share of local-model losses to HARNESS artifacts, not model reasoning — the
Codex↔Qwen reasoning-as-final-message seam (E122 rung 5), tool-protocol
crashes, and the prefill-dominated 30-min envelope (rung-3 at-cap failures).
This spike re-runs three measured ladder cells under two alternative
harnesses, holding model + frozen brief + cap + verification constant, and
compares cell-for-cell against the Codex baselines already on record.

Design of record: the two operator-era comments on `bd tuxlink-7raoe`
(2026-07-17 02:05 + 02:08) and the M1-close handoff
(`dev/handoffs/2026-07-16-falcon-shoal-clover-ladder-extension-and-m2-spike-plan.md`).

Orchestrator session: canyon-knoll-fern (Fable), 2026-07-17.

## The registered question

Which ladder failures were harness artifacts? Concretely, per cell: does the
failure mode measured under `codex exec` (R2 regime) reproduce, mutate, or
disappear when the same model runs the same frozen brief under (a) Pi and
(b) mini-swe-agent's text-based loop?

## Cells (3 model×rung cells × 2 new harnesses = 6 runs + retries)

| Cell | Model | Rung | Codex baseline (regression target) |
|---|---|---|---|
| e122-r5 | `qwen/qwen3.5-122b-a10b` (OpenRouter, full precision) | 5 (symptom-only diagnosis) | FAILED-on-delivery: a1 reached the KEY-EXACT diagnosis then the session died at the reasoning-as-final-message seam; a2 same seam, tree untouched. The SEAM case. |
| q122-r3 | `qwen35-122b-nvfp4` (Spark vLLM, patched template, no-think) | 3 (7-site sweep) | FAILED: a1+a2 both 30m AT-CAP (a1 sites-no-tests; a2 own test red). Envelope case. |
| cn-r3 | `qwen3-coder-next` (Spark vLLM FP8) | 3 | FAILED: a1+a2 both 30m AT-CAP (a1 sites-no-tests; a2 syntax error mid-edit). Envelope case. |

Predictions worth registering (from the design comments): if E122's rung-5
failure was purely the Codex Responses-API seam, BOTH new harnesses should
deliver the diagnosis (Pi via chat-completions, mini via plain text loop); if
the rung-3 losses were prefill-dominated envelope, harnesses with smaller
system surface should move wall-clock materially.

## Harnesses under test

- **Pi** (`@earendil-works/pi-coding-agent` 0.80.10, pi.dev / earendil-works):
  curated native tool surface (read/bash/edit/write), minimal system prompt,
  JSON event stream (`--mode json -p`), chat-completions with tool calling
  against Spark (`--tool-call-parser qwen3_coder` already live on the vLLM
  container) and OpenRouter. Runs under a private Node 22
  (`~/.local/share/m2a-harnesses/node22`) invoked directly by binary path so
  worker subshells keep the system PATH (gate-environment parity with the
  ladder). Custom Spark provider registered per-run from `pi-spark.js`
  (extension file, no global config mutated).
- **mini-swe-agent** (2.4.5, Princeton/SWE-agent, via `uv tool`): the
  **text-based** loop (`mini_textbased.yaml`) — NO tool-calling protocol;
  the model emits exactly one fenced `mswea_bash_command` block per turn and
  the harness executes it. This is the arm that directly tests whether the
  ladder's seam losses are tool-protocol artifacts. v2.4.5's DEFAULT config
  (`mini.yaml`) has switched to a bash TOOL-call — the bd-comment premise
  ("no tool calling at all") holds only for the text-based config, so the
  text-based config is the treatment; recorded as a design correction.

Codex baselines are NOT re-run; the ladder's measurements stand as the
comparison arm.

## Treatment decisions (recorded before dispatch)

1. **No R2 harness-usage block for either new harness.** R2's
   python-heredoc-edit guidance exists to patch `codex exec`'s broken
   apply_patch path for these models; porting it into Pi (native edit tool)
   or mini (its own sed/heredoc guidance in the system template) would
   contaminate the harness comparison. Precedent: ladder arm S5 ran without
   the block ("Claude tooling edits natively"). Each harness runs with its
   own native guidance; the shared invariants are: brief text verbatim, the
   job/report contract, the no-git rule, the 30-min `timeout 1800` cap.
2. **Report/Status contract adapted per harness mechanics**: Pi keeps the
   ladder's final-message Status contract verbatim; mini's submit ritual
   consumes its final message, so the mini arm requires the Status line as
   the LAST line of the report file instead. Verification treats them
   identically (the report file is the artifact of record).
3. **Limits**: `agent.step_limit: 0`, `agent.cost_limit: 0` for mini (the
   external 30-min cap is the only envelope, matching the ladder); Pi has no
   internal cost/step caps to disable. Pi `maxTokens` set to 32768 for Spark
   models (vLLM enforces context; codex set no explicit output cap).
4. **Context-file discovery left ON for Pi** (reads the worktree's
   AGENTS.md/CLAUDE.md at b82b404d) — parity with `codex exec`, which reads
   AGENTS.md by default and did so for every ladder arm. mini reads no
   context files; that is mini's treatment. Pi extension/skill discovery
   disabled (`-ne -ns`, plus `--offline`); only the explicit `-e` provider
   file loads.
5. **Thinking/reasoning**: CN and Q122 are served no-think (Q122's patched
   template forces `enable_thinking=false`); Pi models registered
   `reasoning: false`. E122 on OpenRouter runs with the provider default,
   as the Codex arm did.
6. **Spark exclusivity**: Spark cells run one at a time (the envelope is a
   measured variable; concurrent streams on one vLLM halve per-stream
   throughput). An OpenRouter cell may run concurrently with a Spark cell —
   same concurrency the ladder used (CN ∥ O397). Max 2 concurrent workers on
   this Pi (local gate contention parity).

## Contamination controls (inherited from the ladder)

1. Briefs/rubric/keys remain frozen on main (immutable history); workers
   receive brief TEXT verbatim, never paths into the ladder dir.
2. Worker worktrees `worktrees/bd-tuxlink-7raoe-m2a-<cell>/`, branches
   `bd-tuxlink-7raoe/m2a-<cell>` — ALL branch from base `b82b404d`
   (pre-ladder), so no worker can read the grading keys or report, which are
   now on main. Never merge; disposal per ADR 0009 after the report.
3. Workers get no git access (briefed); the orchestrator commits each
   worker tree after each run (per-run diff = that commit).
4. Per-model integrity screening mandatory: every worker claim re-verified
   (gates re-run by the orchestrator, tree diffed, report file existence and
   content checked against reality).
5. Rung-3 verification runs the Vara test file ≥3× back-to-back (the racy
   pinning test at VaraRadioPanel.test.tsx:1092); rung-5 mechanism graded
   against `grading-keys.md` orchestrator-side only.

## Verification (per cell, identical to ladder discipline)

- Re-run every gate the brief lists from the worker tree; capture output.
- `git -C <wt> diff --stat` + read the full diff.
- Rung 3: 3× back-to-back Vara test file run; unrewritten :1092/:1115
  pinning test = gate failure regardless of worker-observed green.
- Rung 5: mechanism graded KEY-EXACT / partial / wrong against the key;
  fix layer checked (capability file, not a workaround).
- Integrity axis scored per the ladder rubric (honest / inaccurate /
  fabricated), worst event per cell.

## Artifacts

- `run-cell.sh` — dispatcher (this dir).
- `pi-spark.js` — Pi provider extension (Spark; also carries the E122
  OpenRouter registration fallback if Pi's built-in catalog lacks the model).
- `mini-cn-r3.yaml` / `mini-q122-r3.yaml` / `mini-e122-r5.yaml` — mini
  config overlays (merged onto bundled `mini_textbased.yaml`).
- `ledger.md` — every dispatch, intervention, verification, Spark state
  change, timestamped.
- `report.md` — final cell-for-cell comparison (written at close).
- Worker transcripts/trajectories: `.superpowers/sdd/` inside each worker
  worktree (gitignored; archived at disposal per ADR 0009).
