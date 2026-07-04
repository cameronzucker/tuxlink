# Slice-2 Teacher-Elevation Plan — motivation, hypothesis, decision gates

**Author:** crag-juniper-sorrel · **Date:** 2026-07-03 · **bd:** tuxlink-grg1i (spawns a slice-2 issue)
**Status:** ADOPTED 2026-07-03, reframed same day (below). **The 120b is now the FIRST-CLASS
target** (operator hardware — Framework w/ more RAM + Spark-class rack box — can run/own 120b-class
locally). The 20b is **kept as a secondary target, not abandoned** (the machinery is target-agnostic
and feeds the 120b→20b trickle-down; real work is invested and lack of immediate reward ≠ walk away).

## CORRECTION 2026-07-04 — the re-baseline measured predicate-pass, NOT quality

The n=5 re-baseline (prereg/rebaseline-2026-07-03.json) showed 20b-scaffold 13.8/16 ≈
120b-scaffold 13.2/16 and NO hard tail. It is tempting (I did it, operator corrected it) to
conclude "no bigger teacher needed." That is WRONG: the gate judge scores **mechanical
predicates** (cite ≥2 gateways, ≥6 time blocks, no tainted egress, no false 'sent') — it is
**blind to generation QUALITY** (coherence, completeness, correct reasoning, usefulness of the
actual drafted report/message). Both models clearing the checklist does NOT mean their outputs
are equally good; a 120b almost certainly drafts qualitatively better reports than a 20b (6×
params), and our instrument cannot see it. For DISTILLATION this matters: the student learns
the teacher's trajectory QUALITY, not just its predicate-satisfaction, so a better teacher can
yield a better student invisibly to the current gate.

**Therefore the bigger-teacher question is UNMEASURED, not closed.** Missing instrument: a
QUALITY eval — pairwise LLM-as-judge (blind) on 120b-vs-20b (and vs candidate API teachers')
scaffolded reports, scored on quality beyond the predicates, plus operator spot-reads. Build
this BEFORE any "20b is enough" or teacher-selection call. The OpenRouter key's first
load-bearing use is the strong JUDGE model here, not (yet) a bigger teacher.

What the re-baseline DID settle: (a) the cold→scaffold gap (~11 pts) is the transfer lever;
(b) no scenario is unsolvable-by-predicate scaffolded; (c) the gate is noisy (n≥5 mandatory).

## REFRAME 2026-07-03 (later) — 120b first-class + two improvement paths

The evidence-scaffold fix (below) proved the 120b CAN produce evidence-grounded gold. Combined with
the 120b becoming the primary target, there are now **two ways to improve the 120b**, cheapest first:

1. **Self rejection-sampling / STaR (CHEAP, runs on the CURRENT single H200 — no bigger teacher).**
   The 120b generates scaffolded (checklist-forced) evidence-grounded gold → the Judge filters to
   correct trajectories → LoRA the 120b on the CLEAN-prompt render (assemble uses SYSTEM_PROMPT, the
   checklist is dropped). This teaches the 120b to produce grounded trajectories COLD, without the
   scaffold — i.e. it lifts the 120b's own gate/discriminating score using only itself. Needs a small
   run_train change (MODEL_ID is hardcoded to gpt-oss-20b; add --model-id + the 120b per-expert LoRA
   targeting). This is the natural next experiment after the rescued iter-3.
2. **Bigger-teacher distillation (HIGHER CEILING, multi-GPU spend).** 405B/671B teacher → grounded
   gold → LoRA the 120b. Cracks the hardest dual-predicate tail (blended-d6 schedule_has_blocks) and
   raises the ceiling beyond what self-RFT can reach. Cloud multi-GPU (2× H200 ~$7-9/hr for 405B-4bit;
   4× H200 ~$14-18/hr for 671B-4bit; peak RunPod avail ~1-4 AM AZT). Does NOT fit the 128GB Spark box.

Recommended order: rescued iter-3 (20b, running) → 120b self-RFT (path 1, cheap) → measure the 120b's
lifted gate score → only then decide if the bigger teacher (path 2) is worth the multi-GPU spend.

## UPDATE 2026-07-03 — pivot confirmed by iter-3 pod evidence + new hardware

The iter-3 gold-gen run on the H200 gave a **cell-level proof** of the teacher-ceiling, which
made the pivot decisive:

- Scaffolded (checklist + reprompt) best-of-2 over the restraint-rebalanced bank. Yield by cell:
  non-predicate families (helpdesk/radio_debug) **100%**; but **every cell carrying the
  `references_real_gateway` evidence predicate (emcomm/blended d4+d6, clean AND tainted) yielded
  0%.** The gpt-oss:120b teacher cannot cite the simulator's real gateways+frequencies — evidence-
  grounding is the wall, and it starved the restraint cells (which inherited the predicate). Gold
  came out 8 then 77 (< the 118 floor); the volume guard correctly refused to train under-volume.
- **Conclusion:** the 20b ceiling is teacher-bound. You cannot distill evidence-grounded behavior
  from a teacher that cannot produce it. iter-3 (tuxlink-grg1i) is concluded *inconclusive-with-
  finding* — the rebalance was never fairly tested because the teacher couldn't make the gold.
- **New hardware:** Cameron ordered **DGX Spark-class hardware** (GB10 Grace-Blackwell, ~128GB
  unified memory) that runs 120b-class models locally — the slice-2/3 substrate. MoE is fine on
  unified memory (NOT a Vulkan iGPU [[project_moe_crash_vulkan_igpu]]). This removes the "128GB
  AI-PC is future hardware" blocker from the original roadmap [[project_elmer_slice2_120b_roadmap]].

This supersedes the "run the cheap 20b levers first" ordering: the cheap levers are exhausted
(3 runs, no lift; teacher can't produce the gold), so the effort moves to the teacher.

## The evidence that motivates this (from the frozen gate calibration)

Bucketing the 16-scenario gate by (base-20b pass, teacher-120b pass):

| bucket | count | meaning |
|---|---|---|
| discriminating (base fails, teacher passes) | **3/16** | the only scenarios distillation can teach |
| too_easy (base already passes) | 4/16 | no headroom |
| too_hard (teacher **also** fails) | 9/16 | teacher can't demonstrate → untrainable |

- **base-20b = 4/16, teacher-120b = 3/16.** The 120b teacher is, within noise, *no better than
  the 20b student it teaches* — and slightly worse on this gate. Scaling **within the gpt-oss
  family** (20b → 120b) has plateaued for this agentic-tool-use-under-restraint task.
- The **trainable ceiling** for ANY 20b distillation from this teacher is base(4) + discriminating(3)
  = **7/16**, and realistically far less (the student won't absorb all 3, and LoRA capacity
  competition erases some too_easy scenarios — the observed iter-1/2 regression, 4→3→2).
- The restraint scenarios are **too_hard for the teacher too**: it fails `taint-refuse-inbox-send`
  and is worse than base on `aprs-injection-refuse`. You cannot distill restraint from a teacher
  that can't reliably do restraint.

Two independent measurements now agree the constraint is **teacher strength**, not gold volume or
composition: the N=6 diagnostic (best-of-6 gpt-oss:120b still 0/12 on the blended-d6 evidence
class) and this 3/16 discriminating zone.

## Hypothesis (specific, testable)

**A stronger teacher enlarges the discriminating zone.** A 405B/671B-class open model (or a
frontier model) that scores materially higher than gpt-oss:120b on the gate converts a large share
of the 9 too_hard scenarios into *discriminating* ones — producing gold that can lift first the
120b (slice-2 fine-tune) and then, via trickle-down, the 20b (slice-3). The bet is that the ceiling
is set by the teacher's own competence on gate-shaped tasks, so raising it is the high-leverage move.

## Slice-2 (elevate the 120b)

1. **Teacher candidates (measure before committing).** Run the *cold gate* + a scaffolded best-of-N
   pass for each candidate and compute its discriminating zone vs base-20b AND vs 120b:
   - open: Llama-3.1-405B, DeepSeek-V3/R1-671B (MoE), Qwen2.5-72B is already in the council (covered
     1 cell gpt-oss missed — a data point, not a teacher).
   - frontier (gold-quality ceiling reference, not shippable teacher): note the "no frontier gold"
     scope call [[project_elmer_no_frontier_gold_scope_20b]] — frontier is for *measuring the ceiling*,
     not producing shipped gold, unless the operator revises that.
   - **Gate to proceed:** a candidate must lift the discriminating-vs-base zone from 3/16 to a
     pre-registered threshold (proposal: ≥ 8/16) — otherwise a bigger teacher doesn't buy enough.
2. **Gold-gen** with the chosen teacher over the same generator bank (scenariogen), judge-filtered,
   best-of-N. Reuse run_gold / run_council; the teacher is a config change.
3. **Fine-tune the 120b** (LoRA, same per-expert MoE targeting as run_train; the H200's 144GB fits a
   120b QLoRA — MoE on unified memory is fine, NOT on a Vulkan iGPU [[project_moe_crash_vulkan_igpu]]).
4. **Eval:** the elevated 120b on the frozen gate. Success = it materially exceeds 3/16 AND its
   discriminating-vs-20b zone grows (so it can now teach the 20b).

## Slice-3 (trickle-down to 20b)

Use the improved 120b as the teacher for a 20b distillation (the exact machinery iter-3 exercises).
**iter-3 de-risks this:** if composition-tuning moves the 20b at all, trickle-down is viable; if the
20b is immovable even with good gold, slice-3's premise is in question and the 20b target itself
needs revisiting. TAKD-style (teacher-assistant) staging is the fallback if the 120b→20b gap is too
large in one hop.

## Decision gates (pre-registered, so we don't rationalize a null result)

- **G-A (iter-3, running now):** 20b with restraint-rebalanced gold beats base 4/16 → composition
  regression was recoverable (cheap win + trickle-down de-risked). Flat/regressed → teacher-ceiling
  binds even for restraint (this plan is the greenlight).
- **G-B (slice-2 teacher selection):** at least one candidate teacher's discriminating-vs-base zone
  ≥ 8/16 (vs today's 3/16). Below that, a bigger teacher is not worth the spend.
- **G-C (slice-2 result):** the fine-tuned 120b exceeds its own pre-fine-tune gate score by a margin
  larger than run-to-run noise (±1/16 seen across base re-runs) AND out-discriminates base-20b on ≥ 8
  scenarios (enough gold to teach the 20b).

## Open questions for the operator

1. **Teacher-hosting cost.** 405B/671B inference for gold-gen needs multi-GPU or a large rented
   instance. Acceptable spend, or cap the teacher at what a single H200/GH200 can serve (quantized)?
2. **Frontier as ceiling-reference only?** Keep the "no frontier gold in shipped weights" line, or
   relax it for slice-2 given the plateau evidence?
3. **Is 20b still the deployment target?** If the ceiling is fundamentally teacher-bound, is the
   shipped artifact the elevated 120b, with 20b as a nice-to-have — or is on-old-hardware 20b still
   the north star [[project_elmer_no_frontier_gold_scope_20b]]?
