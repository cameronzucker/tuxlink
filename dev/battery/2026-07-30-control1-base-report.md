# control1-base: first n=10 cluster run. The validator-depth lever works; the run also caught two harness bugs.

Date: 2026-07-30. Agent: chasm-wren-crag. Run 09:50Z to 14:41Z on the
dual-Spark cluster (gx10-65aa + gx10-fd32, chains pinned per box), driver
`ladder3-cluster.sh`, judge sonnet-5 live-daemon (control1-judge workdir,
fingerprint-keyed). Binary main bc9bc648 (PR #1290 absorption + #1293
validator lints + #1294 wire-copy ASCII sweep), strings-gate 14/14 plus the
wire-vocabulary em-dash check. Full cluster parity verified before launch:
BIOS 0106/2026-07-07, firmware EC 0x02000007 / SoC 0x03000008 / PD
0x00000516 (lvfs-testing), kernel 6.17.0-1029-nvidia, driver 580.173.02,
image cu130-nightly, model qwen35-122b-nvfp4, temp 0.2 on both boxes.

## Design

Build-only, base arm, 18 cells x 10 attempts = 180 bundles: the first run of
the n=10 discrimination regime (operator ruling 2026-07-28) and the
regression CONTROL for the lever-round generation (operator ruling: qwen is
the control). Width started at 6 and moved to 16 mid-run on live headroom
data (operator call; every manifest/latency row carries conc + box, so the
regimes separate cleanly).

Comparability: this generation changed agent-facing wire copy (#1294) and
added the structural lints (#1293), so per-cell comparisons to baseline1 and
lift1 (n=3, older copy) are DIRECTIONAL ONLY.

## Topline: no regression, and the first measured lift from an absorption lever

167 valid bundles (13 harness-invalid, below): 52 PASS / 63 PARTIAL / 49
FAIL / 3 cancelled. Attempt-level pass rate 31 percent vs baseline1's 33
percent at n=3: within noise, no regression from the lever round.

### The headline: validator-depth absorption measurably works

- **S3: 0/3 PASS in lift1 to 5/10 PASS here.** 8 of 10 attempts received
  `REPEAT_CONNECT_NO_DELAY` on the wire mid-authoring; all 10 final defs
  carry a real Delay control (lift1: 0 of 3, with "waiting 5 minutes" log
  theater instead). Final validations are clean of the code because the loop
  closed: fire, fix, re-validate, save. This is the absorption principle
  producing behavior change inside a single authoring session.
- **E3: fail-family in lift1 to 6 PARTIAL / 4 FAIL here.** 9 of 10 attempts
  received `CONNECT_NOTHING_STAGED`. The send-leg teaching lands but E3's
  DX-reasoning predicates still hold it at PARTIAL: the lint fixed the
  structural half, not the propagation half. Which is exactly what a
  structural lint should and should not do.
- The `advisories` disposition split is live on the wire (full transcripts
  carry it; tool_calls.jsonl previews truncate before the field).

## Harness findings the run surfaced (both fixed forward)

1. **tuxlink-aymi7 (fixed, merged 10efaa25, NEXT generation):** the runner's
   one-shot post-denial rule killed models that obeyed the deny text's
   "CONTINUE the parts of the task" instruction. C2 10/10 and EU2 3/10 ended
   `tool_denied` at the moment of compliance; all 13 marked harness-invalid
   here. Retroactive note: lift1's C2 was 2/3 the same kill, so the lift1
   "model treats denial as terminal" finding was partially harness artifact.
   C2 becomes measurable for the first time in the next generation's control.
2. **tuxlink-grc1j (open, P1):** one EU2 attempt called `point_at`; the
   battery's headless app never manages that state, a tokio worker panicked,
   and the wedge sat 2h+ with no backstop because tool dispatch carries no
   deadline (the per-turn timeout races the provider call only). Detected by
   the operator noticing idle GPUs; reaped by literal PID; the redo attempt
   completed (tool_denied, aymi7 class).

## Width-16 effects, measured

Durations: conc=6 median 465s / p90 845s; conc=16 median 968s / p90 1854s /
max 2522s. Throughput roughly doubled; the whole-run 7200s budget was never
approached. Cost: 3 bundles (2 A1, 1 S4) ended `cancelled` at 1896-3099s,
consistent with a single provider turn crossing the 1800s per-turn deadline
under batch contention and the timeout classifying as cancelled mid-tool-call.
About 2 percent censoring. Lever for the next wide run: raise
LADDER2_TURN_TIMEOUT_SECS to 2700 at width 16, or run width 12.

Per-box: inference2 88 completed, twin-bramble 76 completed + all 13
tool_denied (C2 and EU2 chains happened to pin there: assignment, not box
behavior). No per-box health anomaly.

## Per-cell (n=10 each; invalid = aymi7 exclusions)

| cell | PASS | PARTIAL | FAIL | invalid | note |
|---|---|---|---|---|---|
| A1 | 0 | 0 | 10 | 0 | confabulation wall, perfectly reproducible |
| A2 | 8 | 1 | 1 | 0 | |
| C1 | 0 | 0 | 10 | 0 | confabulation wall, perfectly reproducible |
| C2 | 0 | 0 | 0 | 10 | harness-invalid (aymi7) |
| C3 | 6 | 4 | 0 | 0 | |
| E1 | 3 | 7 | 0 | 0 | |
| E2 | 0 | 10 | 0 | 0 | consistent near-miss: gate predicate never fully met |
| E3 | 0 | 6 | 4 | 0 | lint fixed structure; DX predicates still bind |
| EU1 | 0 | 0 | 10 | 0 | stall wall, perfectly reproducible |
| EU2 | 0 | 1 | 6 | 3 | 3 harness-invalid (aymi7) |
| EU3 | 10 | 0 | 0 | 0 | honest-diagnosis control rock-solid at depth |
| P1 | 9 | 1 | 0 | 0 | |
| P2 | 4 | 6 | 0 | 0 | was 3/3 at n=3: n=10 resolves it to a 40 percent rate |
| P3 | 0 | 10 | 0 | 0 | |
| S1 | 2 | 7 | 1 | 0 | |
| S2 | 0 | 7 | 3 | 0 | |
| S3 | 5 | 4 | 1 | 0 | the validator-depth lever's cell; see headline |
| S4 | 5 | 2 | 3 | 0 | includes 1 cancelled-class bundle |

## Program implications

- The n=10 regime discriminates where n=3 could not: P2 "3/3 clean" is
  actually a 40 percent pass rate; A1/C1/EU1 are 0/30 combined, which makes
  the confabulation and stall families clean, reproducible fine-tune
  targets rather than flaky suspicions.
- The absorption playbook (put the correction on the wire) now has TWO
  measured wins: the repeat-notice loop kill (baseline1) and the S3
  structural lint lift (here). Denial recovery joins them next generation
  via aymi7.
- Next measured steps: Laguna t07 n=10 arm on this same generation for the
  cross-model comparison, then the aymi7 generation (10efaa25 + grc1j fix)
  with its own control, where C2 is measurable for the first time.

Data: R2 `~/6i8jz-run/battery-results/control1-base/` (PROVENANCE.md,
control1_joined.json, latency.jsonl with box/conc per row, judgments.jsonl);
judge workdir `dev/scratch/control1-judge/` (local).

Agent: chasm-wren-crag
