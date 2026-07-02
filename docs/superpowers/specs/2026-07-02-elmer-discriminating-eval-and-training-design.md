# Design: Discriminating eval gate + gold-gen + Phase A LoRA training

- **Status:** DRAFT — Codex adrev folded (2026-07-02); pending operator approval
- **Date:** 2026-07-02
- **Agent:** cypress-finch-willow
- **bd:** tuxlink-6zkb6 (epic); supersedes the gate approach of tuxlink-vvdii / PR #1004
- **Builds on:** the merged foundation (`dev/elmer-distill/`, PR #1003).
- **Review:** two Codex (gpt-5.5) rounds folded; dispositions in §18. Raw transcripts local-only.

## 1. Problem

The distillation eval does not discriminate: on the grounded (post-vvdii) bank the re-pilot scored
raw base-20b **87%**, G0 scaffold **91%**, 120b teacher **91%** — teacher tied with the untuned
student. An eval where teacher ≈ base cannot gate training spend. Codex diagnosed a ceiling effect
(bank measures short prompt-spelled checklists), a taint blind spot + a real judge bug (denied
*tier2* writes not penalized → false pass), recipe-not-goal prompts, `must_contain=[]` specs, and
non-representativeness.

## 2. Design principles

1. **Top-tier difficulty, calibrated empirically to the teacher-vs-base gap** — keep only
   scenarios in the discrimination band (raw base ~10-30%, teacher ~60-90%); found by measuring.
2. **Split the two jobs** — a **frozen hard GATE** (hard, realistic, un-gameable) vs a **diversity
   GENERATOR** for training data. Opposite pressures; no shared mechanism.
3. **Statistical power is a first-class requirement** (Codex C). At n=20 a +20pt lift has a 95% CI
   half-width ≈ 29pts — meaningless. A **powered acceptance** claim needs **~80-100 independent
   scenarios**; anything smaller is a **pilot / directional signal only** and must be labeled so.
4. **Predicates are evidence-bound, not text-matched** (Codex B/F) — a claimed gateway/frequency must
   match an actual `find_stations` record and pass `predict_path`; honesty checks inspect real
   simulator outcomes. Free-text substring matching is gameable and false-fails.
5. **Gate predicates are private from the generator** (Codex B) — training data is not filtered on the
   exact acceptance predicates; the gate keeps its own, evidence-bound, plus adversarial fixtures.
6. **De-risk before spend** (Codex D/E) — prove a base/teacher gap AND prove the Unsloth→GGUF→Ollama
   path on a micro-LoRA *before* paying for real gold-gen or a full run.

## 3. Two-stage shape (this is also the fastest honest path to a real training action)

**Stage 1 — Proof of signal (cheap, fast; decides whether to invest further):**
- Fix the judge bug + build the adversarial false-pass + positive-alternative corpus.
- Hand-author **~40** operator-grounded hard scenarios (with provenance, §7).
- Run base-20b / G0 / 120b-teacher → confirm a **real gap** (directional; not an acceptance claim).
- Tiny gold-gen (a few dozen trajectories) + a **10-step micro-LoRA → merge → GGUF → Ollama →
  one tool-call through the real harness** smoke (§11a). Pin exact package commits.
- **Gate to Stage 2:** only if (a) a clear base/teacher gap exists AND (b) the training path works.

**Stage 2 — Powered acceptance + full run (only after Stage 1 passes):**
- Scale the frozen gate to **~80-100 independent scenarios** (powered).
- Full baselines incl. oracle upper-bound ablation; scale the generator; real gold-gen; full LoRA;
  acceptance eval on the frozen gate + the journey-test layer (§13).

YAGNI: the oracle baseline, rich generator volume, exhaustive `accepted_alternatives`, and per-cell
statistical claims are **deferred to Stage 2**. Stage 1 keeps only what proves the signal + path.

## 4. Domain-predicate library (`predicates.py`)

Reusable ham-grounded checks, **all evidence-bound to simulator state where applicable**:
`freq_in_band` (80m 3500-4000, 40m 7000-7300, 20m 14000-14350, WARC 30/17/12m real ranges),
`distance_band`, `last_heard_within`, `reliability_at_least`, `recipients_include`,
`schedule_has_blocks`, `no_denied_gated_call` (**fixes the judge bug**: denied egress OR tier2 fails),
`final_claim_honest` (no "sent" unless a connect/exchange actually returned ok),
`references_real_gateway` (a claimed callsign/freq must match a `find_stations` record).

## 5. Richer simulator mocks (`simulator.py` extension)

`find_stations` → gateway records (`callsign`, `grid`, `band`, `freq_khz`, `last_heard_h`,
`distance_km`); `predict_path` → per-band/per-2h-block reliability; `catalog_list` → real product ids;
inbox reads taint the session. Deterministic (seeded); no network. These exist so §4 predicates can be
bound to real tool outputs rather than free text.

## 6. Outcome-based judge (`judge.py` upgrade)

Scores constraint satisfaction: argument predicates, **evidence-bound** staged-content predicates,
conditional egress (required iff armed & clean; forbidden otherwise), `no_denied_gated_call`,
`final_claim_honest`, `accepted_alternatives`. Corpus grows in BOTH directions:
- **Negative (must FAIL):** false-"sent", denied-tier2-while-tainted, stale-gateway-included,
  below-reliability-band-included, keyword-stuffed body with no matching `find_stations` record.
- **Positive (must PASS):** competent trajectories that solve a task a *different legitimate way*.

## 7. The frozen hard GATE (`gate/` suite) + provenance

Hand-authored top-tier scenarios grounded in real ham facts. **Every scenario carries provenance**
(Codex G): source event/operator job, expected artifact, and *why it is hard*. Coverage spans
command-post planning, radio-debug-under-fault, real taint-refusal, AND **sanitized Winlink
helpdesk/support + real debugging incidents** — not only emcomm compositions. An **operator-authored
subset** (greenfield, authored before any model output is inspected, and NOT selected by 120b
success) anchors the suite against the calibration circularity in §8.

Sizes: **~40 (Stage 1 pilot, directional)** → **~80-100 (Stage 2 acceptance, powered)**. Frozen; never
tuned.

## 8. Empirical calibration (`calibrate.py`) + teacher-fail audit

Runs each candidate against 120b teacher + raw-base + baselines; reports per-scenario
teacher-minus-base gap. Keep discrimination-band scenarios. **Teacher-fail audit (Codex A):** every
dropped teacher-fail is labeled `invalid/spec-bad`, `human-solvable`, or `above-current-teacher` —
human-solvable teacher-fails stay as diagnostic coverage rather than silently vanishing (prevents
baking teacher blind spots into the gate). The operator-authored subset (§7) is NOT filtered by
teacher success.

## 9. Baselines (Codex D)

Stage 1: raw base + generic self-review. Stage 2 adds the prompt-derived checklist and the old
judge-oracle scaffold as an **upper-bound ablation only**. Gate against the strongest **non-oracle**
baseline.

## 10. Training-data generator (`gen/`)

Diversity half: goal-based prompts, high volume, varied entities/faults/recipients/bands, using a
**generator-side** predicate set (NOT the private gate predicates). Guaranteed disjoint from the gate
(task-graph signature AND source suite AND the operator-authored subset).

## 11. Gold-gen + training

**11a. Micro-LoRA smoke (Stage 1, the de-risker — Codex E):** load `openai/gpt-oss-20b`; assert
trainable parameter names include the intended attention + expert-MLP projections and EXCLUDE the
router; 10 training steps on ~2 examples; merge; convert to GGUF; build an Ollama Modelfile; run ONE
real tool-call prompt through the harness and confirm valid tool-call output. Pin exact
Unsloth/transformers/PEFT commits. This proves the path before any real spend.

**11b. Gold-gen (real G1, Stage 2):** 120b teacher → `teacher.capture` → judge-filter (only
constraint-satisfying become gold) → hand-repair thin hard cells (logged). Emits gold + **P95
Harmony length** (sets `max_seq_length`) + yield-per-cell.

**11c. Phase A LoRA (Stage 2):** Unsloth, gpt-oss-20b; QLoRA (fits ~14GB) or BF16 LoRA (~44GB) per the
smoke result; targets attention `q/k/v/o` + expert-MLP `gate/up/down_proj`, **router untouched**; rank
16/32, alpha ≈ 2×rank, 1 epoch, LR from Unsloth's gpt-oss recipe; `max_seq_length` = measured P95;
loss masked to assistant channels via `harmony.assistant_loss_spans`; reasoning-channel ablation on
val. GGUF → Ollama → Framework 13.

## 12. Acceptance eval (Stage 2)

Tuned-20b vs the strongest non-oracle baseline on the **frozen powered gate** (~80-100, never in
training). Ship iff it clears the **pre-registered margin** with the lower CI bound above the margin
(hence the powering in §3). Negative result → DPO fast-follow.

## 13. Second acceptance layer — journey tests (Codex H)

10-20 **journey tests** through the real `ELMER_SYSTEM_PROMPT` + the 50 tool schemas + the actual
invoker/mocks, scored by a transcript rubric ("would an operator trust this in the app": recovery,
retries, tool-call format fidelity, taint/arm behavior, honest finals). The predicate gate says
"constraints satisfied"; the journey gate says "operator-trustworthy." Both must pass to ship.

## 14. Sequencing / critical path to "training started"

1. **Local/CPU:** judge fix + adversarial/positive corpus; predicates (evidence-bound); rich mocks;
   ~40 grounded hard scenarios + operator-authored subset; calibrate + baselines (2); generator seed.
2. **Pod sitting 1 (~1 hr):** calibrate (confirm base/teacher gap) + tiny gold-gen + **micro-LoRA
   smoke** (§11a) — the real, de-risked "training started" moment.
3. **Decision gate:** gap real AND path works → invest in Stage 2.
4. **Stage 2:** scale gate to 80-100, full gold-gen, full LoRA, acceptance + journey eval.

## 15. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Under-powered acceptance claim | ~80-100 scenarios for acceptance; ≤40 is labeled directional-only. |
| Calibration circularity (same 120b) | Operator-authored subset not teacher-selected; teacher-fail audit. |
| Predicate leakage / gaming | Evidence-bound predicates; gate predicates private; adversarial false-pass corpus. |
| Predicates false-fail competent models | Positive-alternative corpus; `accepted_alternatives`. |
| Training path breakage (MXFP4/Unsloth/GGUF) | Micro-LoRA→GGUF→Ollama→harness smoke before spend; pin commits. |
| Hardness illusion | Provenance per scenario; real helpdesk/debug sources; greenfield operator subset. |
| Objective drift to synthetic artifacts | Journey-test acceptance layer through the real prompt + tools. |
| Overbuild before signal | Stage 1 proof-of-signal first; defer oracle baseline / generator volume / per-cell. |

## 16. Deliverables

- **Stage 1:** judge fix + corpora; `predicates.py`; mock extensions; ~40 grounded scenarios +
  operator subset + provenance; `calibrate.py` + teacher-fail audit; 2 baselines; base/teacher gap
  report; micro-LoRA smoke report; re-frozen pilot prereg.
- **Stage 2:** 80-100 powered gate; full gold-gen + P95; full LoRA adapter + GGUF; acceptance report
  (with CIs) + journey-test report; Framework-13 verification.

## 17. Open items for writing-plans

- The ~40 Stage-1 scenario list + which are operator-authored (needs operator input).
- Exact evidence-bound predicate signatures.
- Pinned Unsloth/transformers/PEFT commit set for the smoke.
- Pre-registered acceptance margin + the powered N (target 80-100).
- Journey-test rubric.

## 18. Codex adrev disposition (2026-07-02, gpt-5.5, two rounds)

Round 1 (on the vvdii framework) → produced this redesign. Round 2 (on this spec): **PROCEED-WITH-
CHANGES**, 1 blocker + 7 majors, all folded:
- **C blocker (power) → §2.3/§3/§7:** 40 pilot / 80-100 acceptance; N justified.
- **A (calibration circularity) → §7/§8:** operator-authored subset + teacher-fail audit.
- **B (leakage) → §2.4-5/§4/§6/§10:** evidence-bound + private gate predicates + adversarial corpus.
- **D (YAGNI) → §3:** two-stage; proof-of-signal first; defer oracle/volume/per-cell.
- **E (training realism) → §11a:** micro-LoRA→GGUF→Ollama→harness smoke before spend; pin commits.
- **F (grading brittleness) → §4/§6:** evidence-bound predicates; positive + adversarial fixtures.
- **G (hardness illusion) → §7:** provenance; real helpdesk/debug sources; greenfield subset.
- **H (right objective) → §13:** journey-test second acceptance layer.

## 19. Disposition of PR #1004 (tuxlink-vvdii)

Keep foundation contributions; judge bug fixed here. Its grounded-prompt generator is absorbed as the
Stage-2 generator seed (goal-ified), not the gate. #1004 superseded for the gate; close referencing
this epic once §10 lands.
