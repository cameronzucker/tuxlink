# IR surface alternatives — draft brief for operator ruling (tuxlink-s3h20 adjunct)

Operator prompt (2026-08-18): "JSON to get to a JSON output is... JSON with
extra steps? The compiler design does real work by performing a specific
distribution of labor between the deterministic and model elements — that
makes me wonder if a simpler IR might be beneficial. We should consider,
with Codex input, some alternatives."

## Invariants every candidate must preserve (from the ratified one-pager)

1. Whole-routine restatement; no ids, no step surgery, no placement semantics.
2. Gotos structurally unexpressable; consent/transmit/authority unexpressable.
3. Deterministic compile to today's RoutineDef; existing validator/executor
   unchanged; lenient syntax, strict meaning; refusal never guess.
4. Every compile echoes the plain-language readback.
5. Deterministic artifact assertions must be able to grade emissions (spike
   scoring, and future CI).

## The evidence lens (banked, instrument-checked)

AS-EDIT-ROUTINE, three samples: the model nailed BOTH value-shaped edits
(interval, band list) and failed the STRUCTURAL edit all three times (reworded
a linear log instead of gating it), then claimed success. v26 history: 25/25
routines_save failures were pure shape errors — "always knew WHAT, failed at
HOW." Any alternatives ranking should weight distance-from-structural-emission
above syntax familiarity.

## Candidates

### A. Five-construct JSON IR (current one-pager)
The baseline candidate. Cheap to build (serde), positioned refusals free,
nesting-as-structure. But the model still EMITS structure (the on_success /
on_failure nesting is exactly the shape it failed), and the operator's "JSON
with extra steps" concern applies: surface and target are the same medium,
so the layer's value is purely in what it forbids, not in what it affords.

### B. Readback round-trip text DSL
Make the authoring surface BE (a subset of) the plain-language readback the
renderer already produces. One canonical human-readable form; parse(render(x))
== x. The echo contract collapses from "render an explanation" to "re-state
in the same language" — what the operator audits IS what the model writes.
Costs: a real parser (indentation/keyword grammar), ambiguity management,
more compiler work — which is the design's stated virtue, but also the spike's
scope doubling. Emission fitness: plausibly better than bespoke JSON (natural
text is training-dense), unproven at this tier.

### C. Structure-minimal JSON (defaults-heavy)
Keep JSON, shrink emitted structure toward zero: honest failure arm generated
by default unless overridden; logs auto-derived from connect outputs unless
customized; the ONLY nesting the model can write is the success/failure pair.
Same serde cheapness as A; smaller emission distribution; the compiler's
labor share grows (operator's instinct made concrete). Ceiling: less
expressive surface for future constructs; every default is a semantic
commitment the echo must surface loudly.

### D. Template + slots (structure never emitted)
The model never writes structure at all: it selects a template id from a
small registry ("scheduled connect with fallback and honest failure") and
fills named slots (schedule, window, station set, bands, log lines). Emission
degenerates to the value-filling the model already proved it does correctly.
The GO path in the current plan already names a "template registry" — this
candidate promotes it from consequence to surface. Ceiling: expressiveness is
the registry; novel shapes need a new template (deterministic, reviewed,
operator-visible — arguably a feature for a transmitter-adjacent authoring
surface). Spike arm is trivially cheap to add (same asks, slot-fill prompt,
same artifact assertions).

### E. Code Mode (builder-API program in a sandbox) — considered, positioned
Nearly isomorphic to A semantically; training-distribution advantage is
real but the context-economy advantage does not apply to one small routine;
production cost is a sandboxed interpreter in an offline AGPL Tauri app plus
a weaker positioned-refusal story. Positioned as a NO-GO re-plan candidate,
not a primary arm (operator concurred it was an off-the-cuff example, not a
proposition).

## Questions for the Codex round

1. Attack each candidate against the five invariants: which structurally
   cannot satisfy one (not "harder," CANNOT)?
2. Given the evidence lens (values succeed, structure fails, false-success
   claims), rank A-D for a small local model and justify with the failure
   taxonomy each would produce.
3. Propose any candidate not listed, with the same analysis.
4. Which pair of arms, if the spike runs exactly two, buys the most
   decision-relevant information per day of work?
5. For B specifically: is the round-trip property (parse of the render)
   achievable without the readback renderer's prose degrading into a rigid
   template that stops reading naturally?

## Disposition

This brief plus the Codex round's synthesis go to the operator as a decision
input BEFORE the spike executes. The spike's gates are unchanged (one-pager
read = gate one; ladder-lands-and-regression-read = gate two, status
unverified tonight). Nothing here modifies the epic; the artifact-format
decision (v2 blocks vs seal-below) stays open and orthogonal.
