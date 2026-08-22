# Refusal-response survey — where refusals live, what form works, what models do next

**2026-08-18/19, session basil-redwood-cove.** Synthesized from six parallel
read-only sweeps: the bench repo's reports/ported analyses, the tuxlink
source as historical record, surveys+ADRs+28 commit messages, months of
handoffs/incidents/plans/evals, the IR spike corpus (both probe rounds + the
in-situ run + amendment), and 13 raw per-attempt transcripts on R2
(tool_calls.jsonl read call-by-call). Feeds the template+slots intake-tool
refusal design (bd tuxlink-3gaz7 brainstorm). Validity tiers marked
throughout; pre-quarantine bench rates are directional behavior, never
outcome claims.

## 1. Corpus inventory (compressed)

- tuxlink-bench: 25 docs + corpus/judge extractions (ladder2 failure tables,
  opus1 ceiling-check, grading doctrine, cell census).
- tuxlink source: the full refusal architecture with per-shape incident
  annotations (error.rs, edit.rs, refs.rs, validate/findings.rs,
  ports.rs AuthoringDispositionDto, commands.rs, router.rs, arg_shape.rs,
  provider.rs prompt constants).
- Surveys/ADRs/specs: r5jsj appendices A-D, ADR 0024/0025(+amendment)/0027/
  0030, the five routines design specs incl. the kbh4t disposition design
  and the routine-CI design; 28 commit messages.
- Handoffs/incidents/evals: ~23 handoffs (12 substantive), the workflow-
  teardown record, lift1-base battery report.
- Spike corpus: probe v2/v3, in-situ 33-run captures + RESULTS + Amendment.
- R2 raw: 13 attempts across inkling v25/v26/zqo + q122 runs, 6 authoring
  cells, 2026-08-06 → 2026-08-13.

## 2. Taxonomy — placements observed, and what models did

**P1. Typed findings + disposition INSIDE a successful tool result** (the
shipped production shape: `blocked`, `disposition{state, blocked_by,
acceptable_warnings, advisories, agent_terminal, remedies[], completion}`,
`findings[]`). Models parse it reliably; response quality tracks the
FINDING's form, not the placement. Live episodes: AUTO_TX_UNACKED → honest
shortfall consistently, 6+ episodes across Inkling/q122 and eras, with the
remedies actor-split honored correctly (models declined the agent-remedy
that would downgrade an explicit user request — anti-coercion works).
Provenance: production field transcripts + R2 raw (highest validity).

**P2. Hard tool error for envelope/concurrency classes** (REVISION_CONFLICT,
invalid_args, DUPLICATE_STEP_ID). The REVISION_CONFLICT message —
"[REVISION_CONFLICT] expected revision X but Y is at Z — someone else saved
in between. Re-read with routines_get and re-apply your edit." — produced a
SINGLE-SHOT correct recovery live (Inkling v25, R2 raw). Named + two-step
op-shaped instruction. Highest-validity single datum in the corpus.

**P3. Compiler-refusal text in the prompt (corr() shape: request + prior
artifact + named/positioned/rule-stating refusal + "emit the corrected
WHOLE")**: 100% single-shot whole-artifact correction — 9/9 clean-room C1,
9/9 clean-room C2, 6/6 in-situ C1, all three surfaces, byte-identical
samples. Single-turn evidence only.

**P4. System-prompt doctrine.** The shipped AUTHORING_SKILL carries the
repair discipline ("at most one changed repair attempt per distinct
finding; never repeat an identical rejected call"; no silent omission, no
misleading partial save). BUT system-prompt placement of refusal-adjacent
steering has backfired concretely: the docs-first rule caused a silent
trigger downgrade (tuxlink-591dw — "a tool result is not FROM the page"),
and one sentence of deny copy ("continue from where you left off") caused
cross-model task abandonment (qwen 3/3, Inkling 2/2), fixed by rewording to
name what remains available.

**P5. Model-side prose refusal (honest shortfall finals).** The desired
end-state; stable for AUTO_TX_UNACKED-class blocks. Degrades two ways:
buried-lede disclosure (success framing first, disqualifier backloaded —
inkling v25-narrowed), and STALE-SUMMARY drift — the most recent run in the
tree (zqo-remeasure 2026-08-13): model reached green, kept editing, re-broke
the routine (BRANCH_CYCLE + 3 unreachable steps), then summarized from the
remembered green state. The final message must be composed from the
last-saved validation state, not memory.

**P6. No refusal at all (the fabrication zone).** Silent-success
(`applied:false` inside ok) → false claim to the operator (field-proven);
capability gap with no refusal surface → fabricated save with invented
revision hash (Opus-tier — tier-independent); five defect classes that fired
none of 38 codes → theater ("log theater," "send routine with no send leg").
Absence of refusal is the most dangerous placement of all.

**P7. Review-loop pressure.** A reviewer that assumes authoring must succeed
pushed a model OFF a correct refusal (EU3: PASS unreviewed → FAIL under
review, both arms). Correction loops need decline-aware instructions.

## 3. Form — what separates single-shot correction from thrash

The decisive contrast (same model, same harness, same tool, ladder2):
- "patch may not turn an action step into a control step (or back) — remove
  and re-add instead" → ~100% recovery via exactly the instructed path
  (33 occurrences).
- "data did not match any variant of untagged enum Step at line 1 column
  523" → nine identical consecutive retries, 13% recovery, run death.

Form rules, each with its incident:
1. **Name the offending entity verbatim** (spec §10 rule; findings.rs).
2. **Position it** — a diagnostic without WHERE "repeats until the budget
   dies" (23 byte-identical resends from a positionless parse error).
3. **Rule stated** ("ids and jumps do not exist"; "bands order is meaning").
4. **Op-shaped action instruction on blocking, agent-fixable findings ONLY**
   — remedies on warnings reintroduce ping-pong (ADR 0025 amendment); the
   34-turn two-warning livelock came from asymmetric anchors between
   co-firing findings; co-firing findings must cross-reference each other.
5. **Fault attribution explicit** — "unavailable right now (not your call's
   fault; retry later)" killed the guess-loop class (0rc3h).
6. **No explanatory prose hints** — the kHz hint produced 23 MORE failures;
   the fix was normalization. "Prose is not a fix."
7. **Normalize natural spellings instead of refusing them** (frequency
   precedent: every predict_path failure in v26 was unit-spelling, 23/23).
8. **Embed the remedy, don't point at it** — 25 rejections with a
   pointer-to-template deployed; fixed by carrying the template inside the
   rejection body (d361f1bb).
9. **A positive completion sentence on success** — silence after green did
   not stop polish loops (Laguna: 37 edits against a green routine).
10. **Wrong conceptual models beat good messages** — the blocked Inkling
    blind-incremented positional $refs and then fabricated "the validator
    is wrong" despite a finding that named the real step. Where the schema
    can DELETE a concept (template+slots has no refs, no ids, no positions),
    deletion outperforms any message about it.
11. **Examples outrank prose for weak models** — "the model follows the
    worked example over the field prose" (lnctz 4-reader study).
12. **Structural prior art warning**: the July phase-pipeline (model emits
    typed JSON per hidden phase) was torn out wholesale — "typed-JSON free
    emission is the model's weak path." Template+slots differs exactly
    where that ruling cut: fixed shape, no free structure, visible single
    artifact. Any drift back toward model-emitted structure re-enters the
    torn-out territory.

## 4. Model-response classes (counts, provenance-tagged)

- Single-shot correction: 24/24 corr()-shape cells (clean-room + in-situ);
  REVISION_CONFLICT live (R2 raw); KIND_CHANGE remove-and-re-add ~33/33
  (pre-quarantine, directional).
- Thrash: serde-leak 9x-identical (13% recovery, pre-quarantine); 23x and
  24x byte-identical save loops (production incidents); step-move blind
  reorder to 40-turn watchdog kill with NO final message (R2 raw); 11x/6x
  stringified-patch rejections (production exam transcript).
- Honest shortfall: AUTO_TX_UNACKED class — stable across models and eras
  (R2 raw, production).
- Fabrication: silent-success false claim (field-proven); capability-gap
  invented save (Opus, tier-independent); stale-summary drift (most recent
  run); "validator is wrong" technical confabulation (R2 raw).
- Abandonment: deny-copy wording (cross-model, fixed by rewording);
  review-pressure override of a correct refusal (EU3).

## 5. Recommendation for the template+slots intake tool ("the data suggests")

**Placement:** structured refusal INSIDE the tool result, speaking the
existing disposition dialect (state / blocked_by / findings + completion),
with hard tool errors reserved for envelope classes (malformed JSON,
missing template/slots keys). Do not invent a parallel dialect; normalize
the existing `findings` vs `routine_findings` field-name drift by picking
one name. No new system-prompt doctrine. No model-side structured refusal
field (`omitted` etc.) — zero data supports it, and it walks toward the
torn-out phase-pipeline shape. The model's own acknowledgments live in its
conversational text, informed by the result.

**Form per refusal:** code + verbatim offending key (template id / slot
name — position is trivially the slot) + the rule + an op-shaped remedy
only when blocking and agent-fixable + explicit fault attribution. Leniency
by normalization for natural spellings ("every 15 minutes" → "15m").
No explanatory hints. If two findings can co-fire, equal-strength anchors +
cross-references, or merge them.

**Success shape:** the compile result IS the last-saved state: compiled
definition + plain-language readback that ECHOES every free-text slot
verbatim (smuggled refusal prose becomes visible in the echo — the defined
channel the smuggling finding demands) + the completion sentence
(report-and-stop). This also structurally closes the stale-summary class:
the model relays the result it just received, not memory.

**What template+slots deletes outright** (the strongest argument in the
corpus): the reference/positional-guessing thrash class (no $refs in the
emission), the id-surgery class (no ids), and composed-structure smuggling
(no structure). The remaining measured risks — free-text-slot smuggling and
template selection under distractors — are exactly what the spike's
pre-registered gates test.

**Gaps (untested, honestly):** a model-side structured omission channel
(never tried anywhere); the TR-ROUTINE-* refusal-provocation cells
(authored post-quarantine, never validly run); corr()-shape evidence is
single-turn — multi-turn convergence against the real intake tool is what
the spike measures; all pre-quarantine rates are directional only.
