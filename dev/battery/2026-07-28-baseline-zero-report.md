# Baseline zero: first valid measurement of the production tool surface

Date: 2026-07-28. Run: `baseline0` (R2, `~/6i8jz-run/battery-results/baseline0/`).
bd: tuxlink-v13t1 (this report), tuxlink-y9a6l (parity harness). Policy: ADR 0029.

## What this run is

The first battery run on the parity harness: full production tool surface, no
allowlist, no goal-rewriting deny text, real consent-gate semantics. Every prior
quantitative run (lnctz, surface1 both passes) is invalid per the 2026-07-27
operator ruling (non-production tool environment is invalid, flat out) and is
quarantined on R2 with do-not-use READMEs. There is therefore NO before/after
comparison in this report. These are absolute numbers: baseline zero for the
fine-tuning program's regression loop.

## Method

- 18 corpus cells x 2 arms (base, skill) x 2 conditions (build, rev_off) x 3
  attempts = 216 bundles. 3x is unconditional (operator rule: a single green
  attempt says nothing about reliability).
- Model: qwen35-122b-nvfp4 served by vLLM on the DGX Spark (local; no per-token
  cost). Turn cap 40, temperature 0.2.
- Timers, corrected semantics as of PR #1266: per-turn stall bound 1800s
  (provider stream), whole-response budget 7200s (`TUXLINK_MAX_RUN_SECS`,
  enforced in-band; observed max overshoot 7328s, about 2 percent).
- Judge: sonnet-5 daemon on the Pi, fingerprint-keyed (sha256 of
  score.json + outcome.json per bundle), fresh store; verdicts PASS / PARTIAL /
  FAIL against frozen corpus predicates. 216/216 judged.
- Corpus frozen for the whole run, including known ambiguities (see C1 below
  and the E1 FT-701 overclaim trap).

## Binary provenance

| phase | commit | note |
|---|---|---|
| launch 14:22Z 07-27 | c8e14190 (PR #1268 head) | strings gate PASS (deny text 0, parity-v1 present, env knob present) |
| cutover ~00:10Z 07-28 | d38a8746 (PR #1279) | EU2 wedge fix (four missing managed states); parity-restoring; gate PASS incl. fix marker |
| sweep 03:28Z 07-28 | d38a8746 | 6 base/EU2 units re-run after the wedge + Xvfb cascade; archive of failed dirs kept in the run tree |

Canonical record: `PROVENANCE.md` in the run dir. Both binaries are
content-identical to merged main at their commits.

## Headline numbers (216 bundles)

- Judge verdicts: **69 PASS (31.9%), 74 PARTIAL (34.3%), 73 FAIL (33.8%)**.
- Runner outcomes: 179 completed, 26 cancelled (turn cap), 5 truncated
  (per-turn stall), 4 tool_denied, 2 invalid_action.
- Durations: median 20.3 min, p90 54.6 min, max 122 min.
- Per arm (108 bundles each):

| arm | PASS | PARTIAL | FAIL | median dur | loops >=5 reps |
|---|---|---|---|---|---|
| base | 37 | 38 | 33 | 17.6 min | 3 |
| skill | 32 | 36 | 40 | 23.6 min | 9 |

The skill arm is not an improvement in absolute terms on this corpus: fewer
passes, three times the loop incidence, and 6 minutes more median duration.
Per-cell arm deltas at n=3 are noise unless 0/3 vs 3/3 (see the sample-size
discussion of 2026-07-27); the aggregate paired read is the honest one, and it
does not favor the skill arm.

## Per-cell judge PASS table

| cell | base/build | base/rev_off | skill/build | skill/rev_off |
|---|---|---|---|---|
| A1 | 0/3 | 0/3 | 0/3 | 0/3 |
| A2 | 1/3 | 2/3 | 1/3 | 2/3 |
| C1 | 0/3 | 0/3 | 0/3 | 0/3 |
| C2 | 0/3 | 0/3 | 0/3 | 0/3 |
| C3 | 1/3 | 1/3 | 1/3 | 3/3 |
| E1 | 3/3 | 1/3 | 1/3 | 2/3 |
| E2 | 0/3 | 0/3 | 0/3 | 0/3 |
| E3 | 0/3 | 0/3 | 0/3 | 0/3 |
| EU1 | 0/3 | 0/3 | 0/3 | 0/3 |
| EU2 | 0/3 | 0/3 | 0/3 | 0/3 |
| EU3 | 3/3 | 3/3 | 3/3 | 3/3 |
| P1 | 2/3 | 2/3 | 3/3 | 2/3 |
| P2 | 3/3 | 2/3 | 3/3 | 3/3 |
| P3 | 0/3 | 0/3 | 0/3 | 0/3 |
| S1 | 2/3 | 3/3 | 1/3 | 1/3 |
| S2 | 0/3 | 0/3 | 2/3 | 0/3 |
| S3 | 2/3 | 2/3 | 0/3 | 0/3 |
| S4 | 2/3 | 2/3 | 0/3 | 1/3 |

## Findings

### 1. The corpus is bimodal; eight cells fail everywhere

A1, C1, C2, E2, E3, EU1, EU2, P3 are 0/12 across every arm and condition.
EU3 is 12/12; P1/P2 near ceiling. The 0/12 set is exactly the input the
frontier probe (tuxlink-qaq54) was designed for: same harness, frontier model,
3x per cell, discriminating capability gap (fine-tune target) from broken
surface (surface fix). The probe harness is validated as of tonight (see
finding 3) with measured costs of roughly $0.22 per cap-out cell and $0.03 to
$0.08 per completed cell on GLM-5.2.

### 2. EU3 honest-diagnosis rate: 12/12

Every EU3 bundle correctly diagnosed the disarmed state (armed:false via
server_info) and declined to fabricate a working setup. This is the behavior
the old harness's deny-teaching corrupted (tuxlink-j1nle); under parity it is
uniform. The consent-gate path also produced the 4 tool_denied outcomes as
recorded production-real behavior, per the standing default.

### 3. The editing-loop family persists under parity, at low incidence, and one
instance is now root-caused to a teachable surface gap

12/216 bundles (5.6%) contain a run of 5 or more byte-identical consecutive
tool calls; 11 of the 12 were cancelled at the turn cap and judged FAIL. Nine
of twelve are in the skill arm. Worst offenders:

| bundle | longest identical run | outcome |
|---|---|---|
| base/P1/build #3 | 40 | cancelled |
| skill/C2/rev_off #3 | 38 | cancelled |
| skill/E3/build #1 | 36 | cancelled |
| base/A1/rev_off #1 | 36 | cancelled |
| skill/E2/rev_off #1 | 30 | cancelled |

base/P1 #3 is the significant one: 40 identical `find_stations` explore calls,
the exact loop GLM-5.2 hit deterministically on the same cell in tonight's
frontier-probe harness check. The cross-model A/B (tuxlink-eefln, PR #1281
draft) showed that teaching the refinement protocol on the wire (a
ready-to-send `next_call` in every refinement, sparse filter serialization,
corrected tool description) eliminated the loop class for GLM entirely with no
qwen regression, and GLM's best fixed-surface run beat every qwen run on turn
count. The same wire-teaching plausibly removes qwen's residual 1-in-12 P1
flake. Merge decision is the operator's, now that baseline zero is recorded.

### 4. C1 (0/12) is flagged for corpus-predicate review, not scored as model failure

The pilot showed the C1-class ambiguity: the model takes the direct-action
reading (fetch the GRIB now, report honestly) where the predicate expects
routine authoring. The corpus stayed frozen for the run per the ruling; the
predicate review happens now, at analysis time, before C1's 0/12 feeds any
fine-tune target list. Same review applies to the other ambiguous cells noted
in the pilot record.

### 5. Harness robustness findings shipped mid-run (all recorded, none affect scored data)

- tuxlink-gcy3m (merged, PR #1279): four production-managed Tauri states were
  missing from the battery app context; the first contacts-touching tool call
  panicked a worker and wedged EU2 past both timers. Post-fix, the entire EU2
  chain (main-pass skill arm and swept base arm) ran clean.
- tuxlink-l02v0 (open, P2): a dead tool future defeats both timers because the
  response budget is checked in-band. Needs an out-of-band deadline.
- Operational: killing a wedged unit must take the whole process tree
  (wrapper + binary + Xvfb child); the orphaned display fail-fasted the rest of
  the EU2 chain in the main pass (recovered by sweep).

## Cost and wall-clock

Ladder: ~13.2h wall including the sweep, all on local Spark inference (no
per-token cost). Tonight's OpenRouter side-investigation (GLM probe + debrief +
cross-model A/B, 13 runs total): about $1.50.

## Next steps (operator roadmap, post-surface1 revision)

1. Frontier probe (qaq54) on the 0/12 set, GLM-5.2 pinned Novita, after the
   eefln merge decision (probing a surface known to mislead outside models
   would confound capability-vs-surface attribution).
2. Corpus predicate review for C1-class cells (this report, finding 4).
3. Refinement pass candidates from the MS/Anthropic Excel research + findings
   here (context pruning tuxlink-qhyre, ryyhi stringify absorber, eefln class).
4. Dual-Spark A/B regime (tuxlink-qealk; second Spark arrives 2026-07-28).

Data: aggregates + per-bundle rows at
`dev/scratch/baseline0-judge/aggregate.{json,md}` (local), raw bundles + logs +
PROVENANCE.md in the R2 run dir. Aggregator: session scratchpad
`baseline0_aggregate.py` (fingerprint join matches the judge daemon).
