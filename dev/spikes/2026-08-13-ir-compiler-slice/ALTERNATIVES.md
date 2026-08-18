# IR surface alternatives — evidence package (2026-08-18, tuxlink-qqmys)

Operator-directed evaluation of alternative authoring surfaces for the IR
compiler spike, run the night of 2026-08-17/18 (session basil-redwood-cove)
with the operator steering every round. This document is the durable record;
the session notes are not. Every claim carries a validity tier, because the
evening's central methodological ruling (below) is that untiered evidence is
how programs derail.

## The operator's methodological rulings (recorded verbatim-intent)

1. **Test how we build and ship.** A test instrument must match the shipped
   configuration; divergence in EITHER direction (scaffolding added, context
   stripped) is fixture drift, the same defect class that voided the bench.
2. **Controlled formats embed untestable assumptions.** "We don't know where
   the failures will surface, so guessing which parameters to control or
   paper over with simulation or non-production testing formats is making
   assumptions about what's important, rather than testing and observing."
   With the data we have, nothing stronger can be said.
3. Iteration with the operator in the loop is legitimate evidence work; the
   parked-program doctrine targets unwatched loops accumulating authority,
   not iteration itself.
4. Serving facts: Inkling's sampling is fixed server-side (params are not a
   lever; omit them); the API model id is `thinkingmachines/Inkling-Small-NVFP4`.

## The two questions (operator synthesis, 03:00, the package's spine)

The evening's instrument confusion resolves into two questions that were
improperly collapsed into one test:

1. **Emittability — "will the model do the thing at all?"** (the GLM-5.2
   trauma question: some models simply will not produce certain emission
   types.) A floor screen is VALID in a clean room, because a flat refusal
   or structural incapacity would surface even in the easiest
   configuration — passing is necessary-not-sufficient, failing would have
   been decisive. **ANSWERED: probably not a problem for any tested
   surface** (JSON IR, template+slots, text DSL; content and tool-argument
   delivery both clean). This is the full and only claim the probe rounds
   support about model capability.
2. **Comparative fitness — "which surface works, beyond emittability?"**
   Requires the shipped configuration, because we do not know where
   failures surface and every controlled parameter is an untested
   assumption about what matters. **NOT ANSWERED; the probe instrument is
   unfit for this question by construction.** The paths that can answer it
   are the two instruments in "What cannot be known" below.

The language-design findings (next tier) belong to neither question — they
are facts about the artifact grammar, valid independent of any model.

## Candidates evaluated

A. Five-construct JSON IR (the ratified one-pager).
B. Plain-text DSL (keyword/indentation grammar).
C. Structure-minimal JSON, defaults-heavy (not probed; analysis only).
D. Template + slots (structure never emitted; model fills values).
E. Code Mode builder-API (considered and positioned: nearly isomorphic to A
   semantically; its context-economy advantage does not apply to one small
   routine; production cost is a sandboxed interpreter; NO-GO re-plan
   candidate, not a primary arm).
F. Flat tagged outcome clauses (Codex-proposed; analysis only).

## Evidence, by validity tier

### Tier 1 — facts about the language, full validity

- **The five-construct grammar permits bounded loops by composition.**
  connect is a step; outcome blocks contain steps; therefore
  connect-inside-on_failure unrolls a retry. Observed emitted 3/3 on the
  smuggled-retry ask (probe v3, A-F2). "Gotos unexpressable" holds;
  retry-by-composition is grammatical. Consequences if allowed: compiler
  lowering is mechanically cheap (finite tree), but readback verbosity grows
  with depth and unrolled copies reintroduce a keep-consistent-under-edit
  surface — the exact skill the ladder showed unreliable. Alternative:
  a flatness rule (refuse connect inside outcome blocks, positioned
  refusal), with retry arriving later as the first-class construct the
  one-pager already lists as deliberately deferred. **Operator ruling
  needed; per the predicated-on-future-results principle, the spike's
  throwaway compiler can carry both behaviors behind a flag and generate
  the operational evidence the ruling needs.**
- **Surface shape determines what discipline costs.** Template+slots cannot
  express smuggled structure (no slot exists); the JSON IR can (composition);
  the text DSL held the line in-sample but by model discipline, not by
  structural impossibility. The template registry also carries the
  honest-failure doctrine in its shape (failure slots exist, so they get
  filled), where the JSON IR emitted success-only when asked success-only.

### Tier 2 — clean-room feasibility screens (valid for question 1 ONLY)

Probe rounds v2 (15 completions, 5 wings) and v3 (36 completions, 12
discriminating cells) against live Inkling, full tables and raw emissions in
`probes/`. All surfaces parsed, nested, corrected-from-refusal, and held or
composed as above; tool-call argument delivery clean 6/6 with no stringified
arguments. **These results establish only that no candidate surface is
flatly unemittable at this tier.** They do NOT establish production-shape
reliability: the instructions were condensed ad-hoc sheets (not the ratified
one-pager verbatim), the context was a clean room (no system prompt, no tool
surface, no conversation), asks were fresh-author (the banked failure is the
edit direction), and the serving distribution is narrow so repeated samples
mostly re-measure one mode. Instrument errata are recorded in the v3 results
header, including the grader miss that initially passed the composed-retry
emissions — shape-semantic assertions, not key allowlists.

### Tier 3 — analysis (framing, decisive of nothing)

Codex design round (`dev/adversarial/2026-08-18-ir-alternatives-codex.md`,
local-only): ranked D > C > A > B for a small local model on
distance-from-structural-emission; killed the readback-round-trip variant of
B as stated (the existing renderer emits ids and consent language and is
lossy — a round-trip surface requires a NEW controlled-language renderer);
flagged that D's selector must be a closed semantic kind or it reintroduces
identifier discovery; proposed F; recommended a two-arm A-vs-D spike with
distractor templates and split scoring (model-expressed meaning vs
compiler-guaranteed behavior).

## What cannot be known with standing instruments

Production-shape reliability — the model authoring routines inside the real
Elmer environment (true system prompt, the real ~92-95-tool surface as
measured from the parity manifest at time of use, MCP delivery, conversation
context) — is not measurable today. The clean-room probes assume away
dimensions we have positive evidence matter (the ladder failed in the full
stack; the clean room passed). The two instrument paths:

1. **The bench**, which is quarantined until the fixture-validity program
   (bd tuxlink-10iw0, P1) re-establishes seam-by-seam parity with
   production. The IR epic's evidence path runs THROUGH that program — an
   explicit dependency, recorded here so it is not rediscovered.
2. **In-app observation** on a converged build with the spike's throwaway
   compiler reachable through the real agent environment — observational,
   operator-in-loop, no transmission involved.

The spike's ratified "no tools, no harness" isolation inherits the same
critique; whether it runs as designed (a narrow seam-test) or amended toward
the shipped shape is an operator ruling on the spike plan, not a probe-side
choice.

## Open operator decisions (all pending, none blocking each other)

1. Flatness vs blessed-unrolling (Tier-1 finding; operational evidence
   generatable inside the spike behind a flag).
2. Spike arms and environment (A-vs-D per the Codex recommendation, with
   composed-semantics scoring; isolation as ratified vs shipped-shape
   amendment per ruling 2 above).
3. Each candidate's shipped instruction artifact (only A has one — the
   one-pager; D and B need theirs drafted before their cells mean anything;
   where any of them lives in the real Elmer context is itself undesigned).
4. The one-pager status header question (the literal "entire instruction"
   includes its own meta-commentary; a shipped version presumably sheds it).

Gate status unchanged: the spike executes only after the operator's
one-pager read and the ladder/regression-read gate, neither cleared by
anything in this package.
