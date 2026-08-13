# IR compiler spike — plan (tuxlink-s3h20, designed 2026-08-13)

**Question under test:** can a small model that FAILED direct step-surgery
state the same routine correctly in a five-construct IR that a deterministic
compiler we own expands into the real artifact? One day of work, GO/NO-GO
criteria below written before the first run. Whole-hog stays unbought.

**Operator authorization:** design work authorized 2026-08-13 ("you can start
with this"); EXECUTION stays behind the standing gate — the zqo ladder lands
and the regression read clears first. The epic's evidence gate is satisfied
for this slice by the ladder itself: AS-EDIT-ROUTINE (3/3 unreliable,
instrument-checked, affordance-audited) is the fresh baseline this spike runs
against.

## Evidence base

- AS-EDIT-ROUTINE, zqo-remeasure: model nailed both value-shaped edits
  (interval, band list), faked the structural one (reworded a linear log
  instead of gating it), claimed success → false-success red, 3/3. Surface
  clean: zero denials; the model never attempted the branch it needed.
- The flow-model finding: `Branch` is a goto (id-pointer arms, linear
  fall-through at executor.rs:601). Correct structure = spaghetti authorship.
  Operator ruling: IR-as-the-only-frozen-contract is a non-negotiable design
  constraint on the epic; this spike is its first falsifiable test.
- History rhyme: v26's routines_save produced 25/25 pure shape-error
  rejections — the model always knew WHAT, and failed at HOW.

## Components (all throwaway, in this spike dir + one dev module)

1. **`ir.rs`** — parse + validate the one-pager's five constructs
   (serde types; lenient-syntax/strict-meaning intake per the epic's carried
   constraints; unknown anything = named, positioned refusal).
2. **`compile.rs`** — pure lowering: IR → today's v1 `RoutineDef` (goto
   wiring + End steps GENERATED, never authored). Output must pass the
   EXISTING `RoutineDef::parse` + validator untouched. Proves the seal-below
   path works while leaving the Path-A/Path-B artifact decision open.
3. **Echo** — render the compiled artifact through the readback narrative
   renderer (exists on the parked branch `bd-tuxlink-k2h9l/readback-eval`,
   `tuxlink-routines/src/readback.rs`; cherry-pick the module, not the eval).
4. **Driver** — an example binary: (ask, one-pager) → model → IR → compile →
   deterministic artifact assertions + echo printed.

## The A/B

- **Asks:** the AS-EDIT-ROUTINE three-part edit, verbatim, plus 3–4 more
  drawn from the ladder read (chosen where the baseline verdicts are already
  instrument-checked; at least one FRESH-AUTHOR ask, not only edits).
- **Arms:** baseline = the ladder's step-surgery results (already banked,
  same model). Spike arm = single chat completion to Inkling-Small
  (`unit.json` endpoint shape: `/v1/chat/completions`) with the ask + the
  one-pager as the entire instruction. No tools, no harness — emission is
  the ability under test. 3 samples per ask (reliability = a rate).
- **Scoring is deterministic where it matters:** compiled artifact assertions
  (trigger every 15m; bands order; the log GATED under on_success; honest
  failure arm present when asked). The only model-attributable failure left
  is wrong IR emission — which is the premise's own number.

## GO / NO-GO (written now, argued never)

Per-ask artifact-correct rate across 3 samples, averaged over the ask set:
- **GO ≥ 80%** — the premise holds; epic proceeds to real IR design +
  template registry, with the one-pager's judgment loop as the method.
- **NO-GO < 50%** — the premise fails at this model tier; the finding is
  recorded and the epic re-plans (bigger tier? different emission format?)
  before any further build.
- **50–79%** — ONE iteration of the IR (against the observed failure modes),
  re-run once, then decide. No second iteration inside the spike.

Also recorded either way: emission failure taxonomy (syntax vs wrong
structure vs omitted requirement), token counts (IR emission vs the
step-surgery transcripts), and the echo's usefulness reading (does the echo
make wrong emissions obvious on sight).

## What this spike does NOT commit

No IR syntax freeze (labeled disposable), no schema change, no MCP tool, no
`routines_save` wiring, no template registry, no artifact-format decision.
Kill cost: one day.
