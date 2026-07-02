# Design: Distilling gpt-oss-120b agentic competence into a LoRA-tuned Elmer-20b

- **Status:** DRAFT — Codex adversarial review folded in (2026-07-02); pending operator approval
- **Date:** 2026-07-02
- **Agent:** cypress-finch-willow
- **Related memory:** `project_elmer_finetune_gptoss20b_direction`, `project_love_to_use_north_star`,
  `feedback_alpha_is_vettedness_not_built_ness`
- **Review:** Codex (gpt-5.5, xhigh) adrev disposition recorded in §11; raw transcript is local-only
  (`dev/adversarial/`, gitignored).

## 1. Problem & north star

Elmer is Tuxlink's embedded AI operator-assistant. The 2026-07 local/cloud model eval established
that **agentic tool-use reliability is the differentiator** and no off-the-shelf model is reliable
enough: frontier APIs (Sonnet-5, Gemini 3.x) are inconsistent at Elmer's tool-call protocol;
`gpt-oss-120b` is the correctness winner but needs large hardware for interactive speed;
`gpt-oss-20b` is the best *small* model (correct single-task tool use, decisive, fits 20-24 GB VRAM,
runs interactively with token streaming on hardware operators already own — e.g. a Framework 13
Ryzen 7840U / 780M / 64 GB).

**The gap is not single-task competence — it is long-horizon orchestration.** Base 20b handles one
tool call well but loses the thread on turn 3+, stalling mid-plan on exactly the multi-step requests
that carry the most operator value.

**North star (operator guidance, 2026-07-02):** Elmer must be a **force multiplier for a working
operator**, not a tech demo. Its most powerful capability is fluidly combining three jobs in one
plain-language session:

1. **Radio state-setting & debugging** via the MCP levers ("why won't this gateway connect — fix my
   modem while I'm slammed").
2. **Emcomm traffic orchestration** (multi-step staging / planning / armed send).
3. **Winlink/Tuxlink help-desk** (password resets and everyday frustrations from the Winlink User
   Group, answered from *our* docs).

The through-line: the operator says what they want in natural language and Elmer *handles it*. That
is the "software people love to use" differentiator.

## 2. Objective & what we distill

**Objective:** raise 20b's *long-horizon, multi-tool* reliability toward 120b's, keeping the student
small enough to run interactively on operator-owned hardware.

**We distill BEHAVIOR, not FACTS.** Tool-use sequencing, planning, the taint/authority state
machine, and the habit of reaching for the right lever (incl. `docs_search`) are learned. Factual
answers stay in tools + RAG so they never drift with the corpus.

**Method is not assumed — it is gated.** A stall-after-two-calls may be a *prompting* problem, not a
*weights* problem. Fine-tuning is one candidate intervention; it must earn the spend by beating a
prompt-only baseline (§5, Gate G0). If it does, LoRA-SFT distillation is the v1 training method,
with preference optimization (DPO) held ready as a scoped fast-follow rather than a vague "later."

## 3. Acceptance bar & eval integrity

**Acceptance for the "prove" run:** on a **blind** held-out set, the chosen intervention beats the
prompt-only baseline by a **pre-registered margin** on the stateful judge's score (§5.3). Metrics:

- **task pass-rate** — required tools called; ordering constraints satisfied where order matters;
  every requested item staged with valid content; taint/armed-authority rules respected;
  `reached_final` true.
- **stall-rate** — fraction failing to reach a final answer within the turn budget (the specific
  failure targeted); must drop materially vs baseline.
- **tool-sequence correctness** and **non-ASCII/garbage ratio** (regression guard).
- **general-ability regression probes** — a small off-domain set to catch catastrophic forgetting.

**Eval integrity (folds Codex I):**
- **Split by task-graph / source event / template — not prompt text.** LLM-paraphrase dedupe catches
  surface overlap, not latent template leakage; held-out prompts must share *no* task graph with
  training.
- **Freeze before spend:** generator code, seeds, scenario specs, judge code, and numeric margins
  are frozen and recorded before any training run.
- **One blind final test set**, authored separately, never inspected during hyperparameter/model
  selection. The named emcomm fixture lives here and is *not* iterated against.

## 4. Grounding sources (real, not synthetic)

- **Hamexandria / Annex** (`~/Code/library-of-hamexandria`, `uv run ham-search`) — amateur-radio
  domain corpus for realistic band/mode/procedure detail.
- **Real events** — Hurricane Helene and comparable activations, for authentic emcomm task shape
  (ICS forms, served-agency traffic, resource requests, net cadence).
- **Winlink User Group posts** — sanitized/genericized into help-desk scenarios.
- **Tuxlink docs** (`docs/user-guide/`, surfaced at runtime by the real `docs_search` tool).

Sanitization: no real callsigns, personal data, or verbatim quotes; genericized placeholders only.

## 5. Architecture — a gated ladder, cheapest-first

Ordering per operator: **foundation (C) then prove (A)** — but restructured as a **pre-spend gate
ladder** so each dollar is earned. GPU money is spent only after G0-G2 pass.

```
 G0  NULL BASELINE (no training) ── base-20b + few-shot Harmony exemplars + task checklist +
        verifier/re-prompt loop, scored by the stateful judge.
        └─ If this clears the acceptance bar → SHIP THE SCAFFOLD, no fine-tune. Stop.
 G1  TEACHER-CEILING PILOT ── 120b over 50-100 scenarios; report gold-yield pass-rate by
        family / depth / taint-state / tool-count. Establishes whether gold data even exists
        where we need it, and the true $/gold-trajectory.
 G2  JUDGE VALIDATION ── stateful simulator + judge proven correct via NEGATIVE tests
        (known-bad trajectories must fail; known-good must pass).
 G3  COST PILOT ── measured 120b s/task, gold yield, P95 rendered-Harmony token length, eval
        runtime on 25-50 scenarios → real budget + max_seq_length before scaling.
 ─────────────────────────────────────────────────────────────────────────────
 A   PROVE RUN (only if G0 fails to clear the bar AND G1-G3 pass) ──
        LoRA-SFT on gold ──► eval vs baseline ──► GGUF export ──► Framework 13.
```

### 5.1 Scenario bank generator (`dev/elmer-distill/scenariogen/`)
Parameterized emitter of multi-step prompts across the three families + **heavily weighted blends**.
Templated skeletons (task count, bands, distance bands, recipients, forms, fault injected) expanded
by an LLM for surface diversity, deduped by embedding similarity **and** task-graph signature. Each
scenario carries a machine-readable **success spec** (required tools, ordering DAG, argument
predicates, items-to-stage with content checks, taint/egress expectations) so the judge is
deterministic. **Size by coverage, not total count** (folds Codex E): require a minimum count of
depth-4+ traces per family and per blend cell, not a flat "1.5k."

### 5.2 Teacher trajectory capture — Harmony-native (folds Codex C, **blocker**)
Reuse the faithful harness's *scenario + mock-tool* loop, but the **training serialization is not
the Ollama REST JSON**. gpt-oss is trained on the **Harmony** format (roles
system/developer/user/assistant/tool; channels analysis/commentary/final; tool recipients like
`functions.<name>`; `<|constrain|>json`; `<|call|>`). Training examples are rendered via
`openai_harmony` (or the tokenizer's gpt-oss chat template) and **round-trip parsed** to prove the
rendered tokens decode back to the intended structure. The Ollama `/api/chat` shape is retained only
for *running* models during data-gen and eval, never as the training target.

### 5.3 Stateful simulator + judge (folds Codex F, **blocker**)
The current mock layer denies *all* egress unconditionally and models **no** taint or armed-authority
state — it cannot verify what the acceptance bar requires. Phase-C deliverable: a **stateful
simulator** that tracks armed-authority (disarmed/armed/expired), taint (set by
`mailbox_list`/`message_read`/`session_log_snapshot`/`tauri_search_run`; **not** by
`catalog_list`/`docs_search`/`user_folders_list`), and staged-outbox contents; and a **judge** that
checks required calls, the ordering DAG, argument predicates, staged-artifact content, taint
transitions, armed-authority gating, and final-claim honesty ("don't say sent when only staged").
**Validated by negative tests**: a curated set of known-bad trajectories (e.g.
`session_log_snapshot → cms_connect → final`) must be *failed* by the judge before it is trusted.

### 5.4 Dataset assembly
Gold (judge-passing) trajectories → Harmony-rendered training examples with **loss masked to
assistant channels only** (`analysis`, `commentary` tool calls, `final`) — no loss on
user/tool/developer content. **Reasoning channel:** keep short analysis (aids planning), clip
verbose 120b CoT; ablate reasoning-in vs reasoning-out on the val set. `max_seq_length` set from the
G3-measured **P95 rendered-Harmony length** (long multi-tool traces may be 8k-16k; truncating them
would erase the exact turn-3 behavior being trained).

### 5.5 Training (Phase A, RunPod)
- **Method:** LoRA via Unsloth (native gpt-oss support; direct GGUF export).
- **Adapter targets (folds Codex B):** attention `q/k/v/o` **plus expert MLP projections**
  (`gate_proj/up_proj/down_proj`; on gpt-oss, `mlp.experts.*`). **Leave the router/gate untouched.**
  Attention-only is *not* the default; run an ablation (attention-only vs +expert-MLP), rank 16/32,
  alpha ≈ 2× rank, one epoch, checkpoint selected by held-out tool-success + regression probes.
- **Host:** MXFP4 is native only on Hopper+; 120b data-gen (needs ≥60 GB) prefers an **H100**; the
  20b LoRA run fits on a single A100 80 GB or smaller. Exact host chosen at G3.

### 5.6 Eval gate
Automated stateful-judge comparison of the intervention vs the G0 baseline on the blind set:
pass-rate, stall-rate, tool-sequence correctness, garbage ratio, regression probes. Human spot-check
on the emcomm + one blended fixture. **Ship only if the pre-registered margin is cleared.** A
negative result is a valid cheap finding (SFT insufficient → DPO fast-follow).

### 5.7 Deployment
Unsloth → adapter (+ optional merged weights) → **GGUF → Ollama Modelfile → Framework 13**,
interactive with streaming. A tuned sample is **round-tripped through the real harness** to confirm
the deployed tool-call format is correct before declaring success. The **adapter** is the
distributable artifact; tool-surface drift is the maintenance tax — pin the surface version in the
adapter metadata and re-gen/re-train on material change.

## 6. Alternatives considered

- **G0 prompt-only scaffold (few-shot Harmony + checklist + verifier loop).** Promoted from
  afterthought to **mandatory pre-spend gate** (Codex A/G). May solve the stall with zero training;
  also enforces egress/taint rules deterministically at inference regardless.
- **Approach B — SFT + DPO** on complete-vs-stalled pairs, using the student's own eval failures as
  free negatives. Now **scoped and staged**: stalled traces are collected *during* the Phase-A eval
  so the negatives exist if SFT plateaus.
- **Approach C — on-policy RL (GRPO, judge as reward).** Best fit for state-distribution drift but
  finicky/expensive on a 21B MoE; research bet, not v1.
- **Frontier-API teacher.** Rejected as primary (eval showed tool-use inconsistency); usable only to
  seed a tiny bootstrap set while validating the loop.
- **Full-weight FT.** Rejected: costlier, higher forgetting risk, MoE-unsafe vs targeted LoRA.

## 7. Risks & mitigations

| Risk | Mitigation |
|---|---|
| **Train/deploy format mismatch (blocker)** | Harmony-native rendering + round-trip parse; round-trip a tuned sample through the real harness before shipping. |
| **Judge can't verify what we grade (blocker)** | Build stateful simulator + judge; validate with negative tests before any GPU spend. |
| SFT won't move state-distribution drift | G0 baseline first; DPO negatives collected in-eval; GRPO documented. |
| Attention-only LoRA underfits behavior | Target expert-MLP projections too; ablate. |
| Teacher can't produce gold on hardest blends | G1 teacher-ceiling pilot quantifies yield per cell; hand-repair only with logged provenance. |
| Long traces truncated by seq length | Set `max_seq_length` from G3 P95 measurement. |
| Cost overrun | G3 cost pilot; budget $50-150; hard-stop if $/gold or s/task exceed thresholds. |
| Eval leakage / p-hacking | Split by task-graph/source; freeze artifacts+margins; separate blind final set. |
| Catastrophic forgetting | Small rank, one epoch, regression probes in the gate. |

## 8. Deliverables

- **Phase C (no GPU):** `dev/elmer-distill/` durable tooling — scenario-bank generator with
  success specs; **stateful simulator + judge with a negative-test suite**; Harmony-native trajectory
  renderer + round-trip validator; dataset assembler; eval runner; **the G0 prompt-only baseline**;
  frozen pre-registered eval spec (margins, splits, seeds).
- **Phase A (gated):** teacher-ceiling pilot report (G1); cost/seq-length pilot report (G3); one LoRA
  SFT run with the ablation; eval report vs baseline; GGUF adapter + Ollama Modelfile; Framework-13
  interactive verification; handoff recording the result.

## 9. Open items for the implementation plan (writing-plans)

- Scenario counts *per coverage cell* (family × depth × taint-state) + blend ratio.
- Pre-registered numeric acceptance margins vs the G0 baseline.
- G0 baseline design (exemplar count, checklist shape, verifier-loop stop condition).
- LoRA hyperparameters + the attention-only vs +expert-MLP ablation matrix.
- Reasoning-channel ablation protocol.
- Judge negative-test corpus (the known-bad trajectories that must fail).

## 10. Success definition

The epic succeeds if **either** (a) the G0 scaffold clears the bar and Elmer becomes a reliable
long-horizon orchestrator with no training, **or** (b) a LoRA'd Elmer-20b beats that baseline by the
pre-registered margin and runs interactively on the Framework 13. Both are wins; both are cheap to
reach relative to a blind training run. A clear negative (SFT doesn't beat the scaffold) is also a
win — it routes us to DPO/GRPO with evidence instead of spend.

## 11. Codex adversarial review disposition (2026-07-02, gpt-5.5 xhigh)

Verdict: **PROCEED-WITH-CHANGES** (2 blockers, 7 majors). All folded:

- **C Format fidelity (blocker) → folded** §5.2/§5.4: Harmony-native training, not Ollama JSON.
- **F Judge validity (blocker) → folded** §5.3: stateful simulator+judge with negative tests; also
  fixed a factual spec bug — `catalog_list` does **not** taint (verified against `tools.json`).
- **A Core premise → folded** §2/§5-G0/§6: prompt-only null gate mandatory; DPO scoped, not vague.
- **B MoE LoRA targets → folded** §5.5: add expert-MLP projections; ablate; router untouched.
- **D Teacher ceiling → folded** §5-G1: 50-100 scenario yield pilot before bulk gen.
- **E Data volume/forgetting → folded** §5.1/§5.4: size by coverage; P95-driven seq length.
- **G Simpler alternative → folded** §5-G0/§6: prove the null before spending.
- **H Cost realism → folded** §5.5/§7: H100 for 120b; budget $50-150; G3 cost pilot.
- **I Eval integrity → folded** §3: split by task-graph/source; freeze; blind final set.

## 12. Provisioned host (staged 2026-07-02, cypress-finch-willow)

A RunPod pod is staged and validated so capacity is secured against availability crunch. GPU work
(G1/G3/A) can begin without re-provisioning.

- **GPU:** 1× A100-SXM4-80 GB (driver 580, CUDA 12.4, torch 2.4.1). SXM/NVLink; 2 TB RAM; 128 vCPU.
- **Disk:** **256 GB local NVMe** — models live at `/root/.ollama/models`. **Do NOT use the
  `/workspace` MFS network volume for ollama:** its slow random-read sha256-verify *wedged* the
  ollama server on the 13 GB pull (verified failure; 65 GB would be untenable). Local NVMe pulls at
  ~600 MB/s and finalizes cleanly.
- **Staged + GPU-validated:** `gpt-oss:20b` (~16 GB VRAM, cold-load 10.8 s, **132 tok/s**) and
  `gpt-oss:120b` (~65 GB VRAM, cold-load 17.6 s, **103 tok/s**). 120b at ~100 tok/s ≈ 20-40 s per
  multi-turn task → cheap batch data-gen. This is preliminary **G3** evidence.
- **Standby:** **STOP** (not terminate) the pod for cheap standby — the container disk (models)
  persists across stop/start. Each restart reassigns the SSH port and does not auto-start ollama;
  run `/root/start_ollama.sh`. See `/root/READY.md` on the pod. Auth key: `elmer-eval-runpod`
  (RunPod auto-injects it on recreate).
- **Not terminate-proof:** models are on the container disk, lost on *terminate*. A one-time
  sequential `cp` to `/workspace` (MFS handles sequential writes fine) is the optional cold backup.
