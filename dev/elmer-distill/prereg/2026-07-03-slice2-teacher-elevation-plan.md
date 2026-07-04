# Slice-2 Teacher-Elevation Plan — motivation, hypothesis, decision gates

**Author:** crag-juniper-sorrel · **Date:** 2026-07-03 · **bd:** tuxlink-grg1i (spawns a slice-2 issue)
**Status:** DRAFT for operator review. Scoped in parallel with the iter-3 20b run per the
operator's 2026-07-03 call ("focus effort on 120b with a larger model, then trickle down").

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
