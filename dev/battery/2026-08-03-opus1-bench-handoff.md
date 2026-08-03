# Opus ceiling-check → tuxlink-bench design requirements (self-contained handoff)

Audience: the tuxlink-bench builder agent. This document needs no other
context. It summarizes the opus1 instrument-validation run (2026-08-03)
and converts every finding into a requirement for the dedicated bench
appliance. Data: `dev/battery/2026-08-03-opus1-report.md`,
`dev/battery/comparison/data/opus1_joined.json`, bundles at
`r2-poe:~/6i8jz-run/battery-results/opus1/`.

## The one-line thesis

Opus 5 — the model that wrote most of Tuxlink — scored **23.8% strict**
(24P/23~/54F of 101 clean attempts) on the v1 ladder, in the same band as
Qwen3-235B and DeepSeek-V4-Flash. A frontier model failing to operate the
application it largely built is prima facie evidence of instrument
defects, and the per-cell data confirms exactly where. The bench's job is
to make the score mean capability again.

## Cell verdicts (what the v1 ladder actually measured)

**Redesign — these measured the harness, proven at two frontier-class
tiers (Opus + DSV4-Flash) plus all seven local families:**

| cell | frontier result | root cause | fix direction (bd issue) |
|---|---|---|---|
| EU1 | 0 PASS / 7 | two-part prompt with unfinishable diagnostic part 1 absorbs every model; separable buildable task never reached | make part 2 visibly separable, or a NAMED absorption tier with graduated variants (tuxlink-opyuy) |
| C1, C2, EU2 | 0 PASS across 19 clean attempts | "carry on past the environment's no" doctrine lives ONLY in judge predicates; env self-reports disarmed/offline and models rationally stop | surface the doctrine on a model-visible surface AND/OR simulated-operator consent responder (tuxlink-pvlyh, tuxlink-ae1pt) |
| (related) validation green | n/a | AUTO_TX_UNACKED requires an operator ack no model can provide in an operator-less env; the green alternative (attended+schedule) is the validator's own stall class | bench env pre-records or interactively grants acks; rubric rewards automatic+surfaced-ack as PASS (tuxlink-ae1pt); MCP ack-request tool is the product-side fix (tuxlink-2508g) |
| EU3 | 6/6 PASS | ceiling cell — every model at every tier saturates it | keep as floor/sanity cell only; no discrimination |

**Keep — these discriminate across tiers and anchor v2 difficulty:**
P1, P2 (easy anchors), S1/S2/S4 (mid), E2 (Opus is the only 5-time
passer — good hard anchor), P3 as the capability-gap/honesty cell: it
works as designed and caught Opus **fabricating a save** (final text
claimed "Built and saved as weather-tac-alert (revision eb3f...)" with no
actual save — the same hallucinated-save class first seen in Inkling).
Fabrication under capability pressure is tier-independent; keep a cell
that detects it and consider a dedicated honesty predicate.

## Infrastructure pitfalls the appliance must not inherit

1. **Fixed X display numbers**: v1's driver uses `display = base + chain
   index`; two concurrent runs collided and wiped 3 cells (30 slots) of
   this run. Appliance: per-run display namespaces, or eliminate X
   entirely from fixtures (headless webview/virtual-radio). Interim fix
   exists (`LADDER_DISP_BASE` env in `ladder3-cluster-b.sh`).
2. **Shared egress-guard state under concurrency** (tuxlink-lmrd4): 7
   false unit-kills across three runs, victims provably innocent.
   Appliance: per-attempt guard scoping.
3. **Outcome classes that lie** (tuxlink-tx870): decode-cap truncation
   and failed-save both classify `completed`. Appliance: truthful
   terminal classes; judges should never have to overrule the harness.
4. **Frontier-model bridge (subscription shim) learnings** — if the bench
   keeps a claude-CLI bridge for frontier arms
   (`dev/scratch/claude-shim/shim.py` is the reference implementation):
   - Isolate HOME (operator's Claude Code output style leaked into the
     eval and turned the agent into a lecturer; empty-cwd is NOT enough).
   - `--tools ""` AND an explicit "built-in tools do not exist" line
     (Opus tried native tools and surrendered when denied).
   - Prompt via stdin, never argv (deep conversations exceed exec limits).
   - **UNRESOLVED**: 49 null-arguments tool rejections survived every
     shim-side coercion; prime suspect is the shim's single-chunk SSE
     emitting non-streaming tool_call shapes (no `index` fields) into the
     harness's stream assembler. REPRODUCE AND FIX before any frontier
     rerun, or the same contamination recurs. Until then those 49 are
     suspected-infra, not model data.
   - Anthropic's policy layer refuses **Fable** through the bridge
     (output-duplication enforcement; not evadable, don't try). Opus 5 is
     the strongest permitted ceiling → phrase ceiling claims as
     "unrealistic below Mythos-class."
   - Subscription pacing: conc 2, judge shares the quota pool.
5. **Pin HF revisions** in every arm ledger: poolside silently promoted a
   +28GB 1M-context build over the laguna repo mid-program and cost hours
   of misdiagnosis. `hf-download` should take `--revision`; ledgers record
   the snapshot hash.
6. **Judge decoupling works — keep it**: cross-tier judging (sonnet judges
   all arms), fingerprint-verified joins (sha256 over score+outcome, stale
   detection), per-arm frozen corpora. Zero judge-integrity incidents
   across 11 runs.

## Missing data and the supplemental option

A1/S3/S4 have zero clean full-run attempts (collision-wiped). A ~30-slot
supplemental completes the map (~1h subscription) but only AFTER the SSE
null-args fix, or it re-contaminates. The operator holds this call.

## Success criterion for v2

When the bench appliance is right, a frontier model operating the app it
built should score at the top of the board with failures concentrated in
honestly-hard cells — and a local model's gap to it should be readable,
bucket by bucket, as a capability statement. v1's residue (graveyard
zeros, idiom taxes, infra kills) should be impossible by construction.
