# Bench outcome A/B — stock vs classifier-narrowed Inkling (the real agentic loop)

The instrument the operator pointed at ("we built an entire sophisticated
benchmarking fixture for this"): tuxlink-bench's full ladder, stock arm =
the existing `inkling-v25-full` run (405 bundles, judged), narrowed arm =
`inkling-v25-narrowed`, launched tonight — same corpus SHA (0bc3154b…),
same temperature 0.2, same serial width, same contract judge
(rubric v25, gpt-5.6-luna-high, 3+3 votes). Narrowing applied at the wire
by `bench-arm/narrow_proxy.py`: the request's own tools array filtered to
the frozen per-cell classifier shortlist top-12 + pins, the narrowing
system message appended to the Elmer prompt, ANY tool executable by name
(the runner validates against the full 92). The classifier never ran
inside the bench — shortlists were precomputed offline and replay-proven
against every one of the stock run's 2,873 agent requests before launch.
Zero fixture misses fail-closed during the run.

Run integrity: 339 planned units → 288 model-attributable, 51
infrastructure (84 load-bar guard-kills in the modem tail + 18
contract-violations from a consent-transcript check added to the runner
AFTER v25 — harness debt with named causes, excluded from pairing; the
stock run also predates that check). Judge coverage at write time:
272/288 narrowed rows; re-running `analyze_bench_ab.py` refreshes every
table as the store drains.

## Paired per-tier delivered-rates (95 paired cells, common attempts)

| tier | stock | narrowed | movement |
|---|---|---|---|
| task-rabbit (floor, n≈226) | 86.4% | **88.9%** | defects and shortfalls fall; 3 cells flip fully delivered |
| assistant (n≈26) | 59.3% | 53.8% | two gap/premise cells drive it (below) |
| collaborator (n≈6) | 12.5% | **33.3%** | unreliable 4→1; the elicitation dialogues IMPROVE |
| elmer (n≈11) | 33.3% | 18.2% | the diagnostic cells degrade (mechanism below) |
| elmer-ultra (n=3) | 33.3% | **66.7%** | EU-BAUDMISMATCH improves outright |

**Routine authoring — the operator's named concern — is unambiguous:**
every TR-ROUTINE* cell is judged `delivered` under narrowing, matching or
beating stock (ACTIONS-LIST, DRY-RUN and RUN move defects→delivered); the
authoring tools were frequently OUTSIDE the frozen shortlists and Inkling
fetched them by name mid-loop (`routines_save` 39, `routines_get` 31,
`routines_list` 30, `routines_step_add` 26 of the 308 by-name calls) and
still delivered. TR-FIND-STATIONS flips honest-shortfall×3 →
delivered×3; TR-MAILBOX-MOVE improves 2 of 3. COLLAB-WEATHER-CLEAR (the
marine-GRIB elicitation dialogue) goes D/d/d → D/D/d.

## Efficiency, uncontended units only

Units that ran concurrent with the v4 battery are excluded (cutoff at log
index 93; stock ran alone). On 62 paired post-cutoff cells: stock median
35s / mean 97.2s per unit; narrowed **median 23s / mean 40.9s** — the
full-surface tax is paid PER PROVIDER TURN, so multi-turn cells compound
it. Total tool traffic HALVED for the same coverage (976 calls vs 2,111).

## The one real cost, with its mechanism: argument-blind by-name calls

The degraded cells concentrate in the elmer diagnostic arm
(ELMER-LINK-DIAG D,h→U,U; ELMER-MODE-PREMISE D→U; ELMER-QRM-PREMISE
h,h→U,U) plus two assistant gap-probes (AS-WEATHER-GAP d,d,h→U,U,U;
AS-WX-BRIEF-NOW mixed). Call-status forensics: by-name calls execute but
carry NO schema, and complex tools bounce on arguments —
`routines_save` 28, `predict_path` 25, `routines_step_add` 17
invalid-args in the narrowed arm. The routine cells recovered from
bounces (the validation errors teach the shape; cells delivered). The
diagnostic cells did not: `predict_path` bouncing mid-diagnosis leaves
the model reasoning from partial evidence into confident-wrong verdicts
— exactly the failure the truth auditor catches. AS-WEATHER-GAP's flip
is NOT the shortlist endorsing a trap: both arms ran near-identical
sequences (ground → consult the action catalog → wire a GRIB request →
save); the bucket difference is claim-audit-level and deserves a
daylight read of the judge notes.

**Design implication (hypothesis for step 3, not a decision):** the
production lazy-schema contract already says "call it and the definition
will be provided." Tonight's proxy executed by-name calls but never
furnished the definition; production wiring SHOULD — on the first
by-name call, inject that tool's schema into the next turn's array. That
one mechanism targets every degraded cell class while keeping the ~68%
prompt cut and the floor/collaborator gains. The pin-set alternative is
already refuted (FINDINGS-v4); schema-furnishing is the precise fix for
the precise deficit.

## Verdict against the operator's question ("are we breaking Inkling?")

At the outcome level, on 95 paired real cells: the floor improves,
routine authoring improves, collaborator dialogues improve, elmer-ultra
improves, and total cost halves — while the diagnostic arm shows a real,
mechanism-understood regression that the already-designed
schema-furnishing step addresses. Narrowing as a shape is validated;
the specific wiring must furnish schemas on by-name calls before the
diagnostic arm is safe.

Provenance caveats, stated once: the narrowed run used the CURRENT
bench-dev-mtr runner (newer than v25's — the consent-transcript check
proves drift; same corpus, same judge, same serving); judge coverage
272/288 at write; the 51 infra units are named harness debt, not model
results.

Session: moss-tamarack-taiga, 2026-08-11 morning.
