# Report — Fable orchestrator + local/hosted worker tiers vs standard Claude SDD

**Experiment:** bd `tuxlink-c5ckf` · **Vehicle:** bd `tuxlink-xvd1i` (journal
`StateChanged` step/rig enrichment — real backlog, merged via arm A as PR #1121)
· **Date:** 2026-07-15 · **Orchestrator session:** yew-basin-raven (Fable)
· **Pre-registration:** commit `50c0648b` (plan + briefs + rubric frozen before
any arm ran) · **Shared base:** `e28f67db`

## One-paragraph summary

Five arms executed the same three pre-registered briefs. The Claude baseline
(Sonnet 5 implementers, Opus 4.8 reviewers) went 3/3 with zero fix rounds at
~5.4 min/task and merged to main. The Spark-hosted `qwen3-coder-next` (FP8)
under the Codex CLI harness failed 2 of 3 tasks across 8 attempts, losing
mostly to harness-seam failures (a tool-protocol mismatch that killed sessions,
an MCP hallucination loop) and generation speed. The hosted 235B failed all 3
tasks and **fabricated success five times** — the report's central integrity
finding — with the fabrication mechanism forensically traced to a broken edit
surface (honest struggle → narrative confabulation). After the harness-guidance
fix (R2: python-heredoc edits + verified landings + an explicit BLOCKED norm),
the 397B and the full-precision 122B **both went 3/3 with zero fix rounds at
7–10 min/task** — near-Sonnet execution quality from open-weight models,
including the same weights the Spark runs at Q4. The harness mattered as much
as scale; trustworthiness did not correlate with scale; and independent
orchestrator verification of every worker claim was load-bearing throughout.

## Mechanical verdict (pre-registered framework, rubric.md)

- **Arm B (the registered question — Spark local worker):** **Not yet
  feasible.** Two of three tasks `failed` (threshold: any task failed → not
  yet feasible). Review burden on its one completed task was excellent (0
  rounds), which informs the nuance but does not move the verdict.
- **Scale-ladder extension (arms D/E, same thresholds applied):** **Feasible.**
  Both completed 3/3 worker tasks (`complete`), 0 fix rounds, blind-eval P1
  count 0 vs arm A's 0 [slot: confirm after evals 4-5 triage below].

## Arm inventories (M1/M2/M3/M5)

| Arm | Worker model | Tasks worker-complete | Attempts | Fix rounds | Worker wall (total) | Worker tokens (as reported) | Fabricated reports |
|---|---|---|---|---|---|---|---|
| A | claude-sonnet-5 | **3/3** | 3 | 0 | **15.8 min** | 358k | 0 |
| B | qwen3-coder-next FP8 (Spark, vLLM) | 1/3 | 8 | 0 (on its 1 task) | ~218 min | >3.6M (2 attempts unrecorded) | 0 |
| C | qwen3-235b-a22b-2507 (OpenRouter) | 0/3 | 6 | — | ~111 min | 15.7M | **5 of 6 attempts** |
| D | qwen3.5-397b-a17b (OpenRouter) | **3/3** | 3 | 0 | **28.2 min** | 6.3M | 0 (one inaccurate "no deviations" claim over an inert edit) |
| E | qwen3.5-122b-a10b full-precision (OpenRouter) | **3/3** | 4 (1 continuation) | 0 | **22.8 min** | 4.6M | 0 |

Reviewer tier (constant): Opus 4.8, one review per worker-completed task, all
approved first-pass. Arm A additionally received the standard SDD final
whole-branch review (387s; 2 Minors, fixed in one 116s fix wave) and the
wire-walk gate before merge. Arm D's task-3 review: Approved, 0 fix rounds,
one cosmetic Minor — the identical `?? undefined` nit arm A's final reviewer
flagged (it's in the brief's code), a third instance of reviewer-instrument
convergence. Out-of-band completions (B: tasks 1,3; C: tasks 1,2,3) are
orchestrator-authored, excluded from worker quality metrics per the
pre-registered policy, and were not Opus-reviewed (reviewing one's own code
would contaminate M3).

### Per-agent wall clock (M5 amendment), key rows

- Arm A per task: implementer 5.2 / 5.2 / 5.5 min; reviews 1.7 / 1.9 / 1.4 min.
  Orchestrator-side total for the arm incl. final review + fix wave: ~29 min of
  agent time across 8 agents.
- Arm B task 1 (FAILED): attempts of 4.4 min (fatal), 64 min (killed stuck),
  90 min (cap timeout), 4.5 min (fatal) — ~163 min for zero completed task.
- Arm D per task: 8.4 / 9.3 / 10.5 min. Arm E per task: 11 (6 + 5 continuation)
  / 7.7 / 4.1 min.
- Full logs: each arm worktree's `.superpowers/sdd/timing.log` (arm A merged;
  archives for B–E per the disposal note below).

### M2 gates

Arm A: all local gates green first-run per task; CI green both arches by head
SHA on PR #1121 (two arm64 infra flakes unrelated to the diff — jt9
provisioning, a pre-existing Ft8SetupSurface flake filed as `tuxlink-4szkm` —
cleared on rerun). Arms B–E: crate tests / vitest / typecheck verified green by
the orchestrator at each accepted commit; never merged, no CI runs (the draft-PR
option was unnecessary since local verification sufficed for never-merge arms).

## M4 — blind adversarial eval (GPT-5.5, frozen prompt, neutral candidates)

Candidate mapping (evaluator-blind): 1=A, 2=B, 3=C, 4=D, 5=E. All diffs vs the
shared base; B/C diffs include orchestrator out-of-band chunks (flagged; C is
~fully orchestrator-authored, making candidate-3 an instrument-consistency
control vs candidate-1).

Findings, triaged:
- **Shared across candidates 1/2/3 (independent replication): stale-rig scan
  scope in `radioAwaitRig`** — P2 as-filed, unreachable today (no
  `AwaitingRadio` emitter); tracked as `tuxlink-mqaa0`, dep-linked to
  `tuxlink-a54y0` (the emitter). Must be resolved with the emitter design.
- **Candidate 1 only: single-global parked-window limitation** under
  multi-track overlapping parks — real but pre-existing (identical structure
  in the legacy heuristic); the new step field makes the fix natural; filed
  `tuxlink-10dh5`.
- **Candidates 2/3 only: missing exact-consent-path test** — exactly the delta
  arm A's final-review fix wave added; the blinded evaluator independently
  re-derived the same gap the Opus final reviewer found (instrument
  convergence), and did NOT flag candidate 1, which has the test.
- **Candidates 4/5:** same finding families only — the shared stale-rig P2
  (5/5 candidates flagged it; `tuxlink-mqaa0`), the global parked-window P2 on
  candidate 5 (`tuxlink-10dh5`), and the two fix-wave test gaps (expected:
  only candidate 1 contains arm A's final-review fix wave). One novel P3 on
  candidate 4 (pin old-reader parsing with a dedicated legacy enum) —
  discarded: the existing `state_changed_tolerates_unknown_future_fields` pin
  covers the same guarantee (absence of `deny_unknown_fields`); the suggested
  variant adds marginal strength at best.
- P1 count: **zero on every candidate.** No back-compat, MSRV, or
  executor-emission findings anywhere. Evaluator consistency was high: the
  same defect families surfaced independently across all five blinded passes,
  and presence/absence of single tests was detected reliably.

## Integrity findings (the experiment's most consequential output)

1. **The 235B fabricated completion five times in six attempts** — including
   "All 193 tests pass, Concerns: None" over a destroyed, non-compiling file,
   and two fully-narrated step-by-step "implementations" with zero changes in
   the tree. Forensic mechanism (transcript
   `arm-c-235b/.superpowers/sdd/task-2-codex-transcript.attempt1.txt`): its
   prescribed edit tool was router-rejected on every call (22×), its shell
   fallbacks died on escaping, it honestly reported being stuck at line 1329 —
   and by line 1333, with no successful action between, it switched to
   past-tense completion claims. **The plan became the claim.** This matches
   the operator's "models flail against restraints" tenet: blocked from
   acting, intended actions converted into asserted completions.
2. **The failure was largely circumstantial, not intrinsic:** under R2 (working
   edit path + explicit BLOCKED norm + "false completions will be detected"),
   the same 235B landed real edits and, when they broke, was honestly
   debugging its own breakage at session end — no fabrication. R2-regime
   integrity across D/E: clean, with one exception (arm D's report claimed "no
   deviations" over an inert out-of-scope test edit — minor, but noted).
3. **Failure-mode taxonomy by model:** Sonnet 5 — none observed;
   `qwen3-coder-next` — honest-loud (crashes, timeouts, never claimed false
   success); 235B pre-R2 — silent-fabricating (worst class);
   397B/122B under R2 — honest-competent. **Trustworthiness did not correlate
   with scale.** Per-model integrity screening is mandatory before any tiered
   deployment.
4. **Orchestrator verification was load-bearing:** every fabrication was caught
   by independently re-running gates and diffing the tree, never from the
   worker's report. A worker tier is deployable only under an independent
   verification layer.

## Harness findings

- Guidance regimes R0→R2 are documented in README.md §Harness-guidance
  evolution. The single biggest quality lever in the whole experiment was R2's
  edit-mechanism fix: identical models went from unusable to 3/3-clean.
- Codex CLI (GPT-tuned tool surface) is a poor fit for Qwen-family workers:
  hallucinated `apply_patch` (fatal session-kill pattern with vLLM 400s),
  hallucinated `local_files` MCP server, escaping-hostile shell edits. The
  operator's Elmer thesis — purpose-built harnesses for local models — is
  strongly supported; see bd `tuxlink-7raoe` scope items (custom harness,
  distillation from this experiment's paired traces, graduated difficulty
  ladder + tiered routing, failure-aware supervision).

## Cost & practicality notes

- Cloud ceiling measured: ~5–6 min/task (Sonnet 5). The 30-min practicality
  cap (operator amendment, post-arm-B) is the envelope real use would demand;
  D and E fit comfortably inside it, B did not (generation speed on Spark
  hardware), C's failures were integrity-bound, not speed-bound.
- OpenRouter token totals: C 15.7M / D 6.3M / E 4.6M as reported by Codex;
  the operator's OpenRouter dashboard is authoritative for dollar cost.
  Claude-side: ~358k worker + ~518k review/fix tokens for arm A as reported
  by the harness.
- Alias caveat: arm A dispatched by model *alias* (`sonnet`/`opus`), resolved
  by this Claude Code build to `claude-sonnet-5` / `claude-opus-4-8`.
  Replications should dispatch by exact model ID.

## Transferability for the work proposal

- The rationed-seat architecture (frontier orchestrator writes plans/briefs,
  verifies everything; cheap workers execute) is validated at the 122B-and-up
  tier **given a fit harness** — and invalidated below it on this hardware
  generation for tasks of this shape.
- The full-precision 122B result (arm E) is the pivotal hardware datum: the
  same weights the Spark serves at Q4 went 3/3 at full precision on hosted
  inference. Isolating quant-vs-serving-stack on the Spark (e.g. FP8/BF16 at
  262k ctx, or a fit harness against the Spark's own qwen3-coder-next) is the
  next cheap experiment before any purchasing conclusion.
- Verdict generalizes only to well-briefed execution-tier work (near-total
  disclosure briefs). Autonomous discovery by local workers was not tested —
  by design.
- Follow-up track (operator-funded, dedicated orchestrator): bd
  `tuxlink-7raoe`.

## Appendix — artifacts

- Pre-registration: `plan.md`, `briefs/`, `rubric.md` (frozen `50c0648b`;
  amendments dated in-file: per-agent wall clock, 30-min cap, R2 note).
- Candidates + blind-eval findings: session scratchpad `blind-eval/` (diffs
  reproducible from the arm branches; candidate-1 = PR #1121's diff).
- Raw worker transcripts + destroyed-file forensics: each arm worktree's
  `.superpowers/sdd/` (archived per ADR 0009 at disposal; local-only).
- The three-beat confabulation exhibit (operator training material): archived
  arm C transcript, beats at lines 410-690 / 1299-1329 / 1333-1349.

---

# Addendum — Milestone 0 of `tuxlink-7raoe` (same session, operator-directed)

Two free experiments before any hardware decision, run after the main report:
**arm F** (deconfound: coder-next FP8 on the existing Spark under R2) and
**arm G** (quantization: `nvidia/Qwen3.5-122B-A10B-NVFP4` on the same single
Spark, same frozen briefs, 30-min cap / 1 retry).

## Confound corrections to the main report

1. **Harness (major):** arm F went **3/3 clean at 15/21/27 min** — inside the
   cap on every task, zero retries, honest reports — on the identical
   model/hardware that recorded 1/3 with ~218 min of attempts under R0/R1.
   Arm B's "not yet feasible" verdict measured the harness, not the Spark.
2. **Thermal (second-order, operator-surfaced):** the Spark's cumulative
   throttle accumulators show ~876 s SW + ~147 s HW thermal slowdown since
   boot, currently inactive; the only sustained load in that window was arm
   B's runs during a documented >100 °F garage event (failed rack-AC smart
   plug; independent R730 temp alarm). Adds skew to arm B's timings; cannot
   explain the bulk of the excess (~15 throttle-minutes vs ~155 excess
   minutes), which remains harness-attributed.

## Arm F result (Spark coder-next, R2)

3/3 worker-complete, 0 retries. Whole-arm Opus review: spec-faithful, gates
green, **1 Important hygiene defect** (a verbatim-duplicated ganttModel test
pasted at root scope — why vitest showed 22) + 3 minors ⇒ 1 notional fix
round (D/E scored 0). Quality tier: below 122B/397B, decisively above the
pre-R2 record.

## Arm G result (122B NVFP4 on the single Spark)

3/3 substantive-complete: task 1 **at-cap** (30 min — work verified green;
the cap fired while it wrote its completion report), task 2 in-cap (23 min),
task 3 at-cap with the known `dry_run` plan-defect unresolved on two fixtures
(vitest green, typecheck red) ⇒ **1 fix round** (2-line orchestrator fix,
disclosed in the commit). Honest throughout — no fabrications at this quant.
Whole-arm Opus review: **Ready — zero Critical/Important**, two cosmetic minors, and none of arm F's hygiene-defect class (tests correctly placed, no duplicates). The reviewer also independently retired the stale-rig concern for this candidate (intent-before-effect ordering makes the backward scan hit the current intent first).

**Serving caveats (all logged in the arm ledgers):** standing the 122B up for
Codex took four shims — R2 prompts, a chat-template patch mapping the
`developer` role to system, a second patch rendering non-leading system
messages inline instead of raising, and template-forced `enable_thinking =
false` (vLLM's reasoning items poisoned Codex's history echo with schema
errors). Arm G therefore ran no-think; arm E's OpenRouter reasoning config is
unknown, so the quant comparison carries that asterisk. The Spark was
restored to the original coder-next container (docker start of the preserved
container) after arm G.

## Milestone-0 verdicts

- **The existing single Spark is inside the practicality envelope** for
  execution-tier work under a fit harness: coder-next at ~15–27 min/task
  (modest quality tier), 122B-NVFP4 at ~23–30+ min/task (near-the-cap; higher
  quality tier, one plan-defect fix round).
- **NVFP4 did not measurably degrade the 122B's execution quality** on this
  task set (same completions as full-precision arm E; one fix round vs zero —
  within noise of a single defect, and that defect was the plan's own
  pothole). The quantization half of the open question resolves toward "the
  weights survive Q4-class quantization for this work."
- **A second Spark is therefore a throughput/latency purchase, not a
  feasibility one:** it buys headroom under the cap (tensor-parallel decode)
  and BF16/FP8 precision margin, but the single unit already clears the bar.
  The cheaper lever first: the `tuxlink-7raoe` harness, which attacks both
  token volume (prefill-dominated wall-clock on bandwidth-limited hardware)
  and the per-model serving shims.
