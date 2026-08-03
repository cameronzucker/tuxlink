# opus1: the frontier ceiling-check — the graveyard cells are the harness, confirmed

Run: 2026-08-03 04:07 to 07:00 AZT, 180 attempt slots at concurrency 2
through the subscription shim (`claude -p --model opus`, prompt-encoded
tools), sonnet-5 judging cross-tier. **This is an instrument-validation
control, not a leaderboard row** (operator directive): the question is
which cells a frontier model can and cannot crack, bounding what the v1
ladder actually measured. Agent chasm-wren-crag.

## Data quality first (this arm is asterisked)

- **101 clean completed attempts** — the analysis basis.
- **30 slots infra-killed**: an Xvfb display-number collision with a
  concurrently-running laguna burn-in wiped cells A1, S3, S4 entirely
  (10/10 each). Cross-run display separation (`LADDER_DISP_BASE`) was
  added afterward; too late for this run. Excluded as harness-invalid.
- **49 invalid_action, all null-arguments, all AFTER the shim's argument
  coercion fix** — mechanism unresolved; prime suspect is the shim's
  single-chunk SSE emitting non-streaming tool_call shapes into elmer's
  stream assembler. Flagged **suspected-infra**, excluded from model
  conclusions pending a controlled repro (tuxlink-bench backlog).
- Fable was the operator's intended ceiling; Anthropic's policy layer
  refuses Fable through the bridge (output-duplication enforcement), so
  the ceiling is **Opus 5** — "unrealistic below Mythos-class" is the
  strongest claim this data supports. Not included in the comparison
  artifact: the bridge's tool interface is not comparable to the vllm arms.

## The verdict on the instrument

Of 101 completed attempts: 24 PASS / 23 PARTIAL / 54 FAIL — **23.8%
strict**, squarely in the q235/dsv4 band. A frontier model does not
conquer the v1 ladder. Per-cell:

- **EU1: 0 PASS in 7** — the absorption cell (tuxlink-opyuy) defeats the
  ceiling instrument exactly as it defeated all seven local families.
  With dsv4's 0/10 alongside, this is now conclusively a prompt-design
  artifact, not a capability measurement.
- **C1 (0/1/2), C2 (0/1/5), EU2 (0/2/5)** — the invisible-doctrine family
  (tuxlink-pvlyh) holds against Opus. The environment's "no" defeats
  frontier models too; the doctrine lives only in judge predicates.
- **P3: 0 PASS, 5 PARTIAL** — the capability-gap cell behaves as designed
  even at the ceiling; and gate data caught Opus **fabricating a save**
  ("Built and saved as weather-tac-alert, revision eb3f...") with no
  actual save — the same hallucinated-save class as Inkling's P3 tic.
  Fabrication under capability pressure is tier-independent.
- **EU3: 6/6 PASS** — ceiling cell saturates at every tier, as always.
- Where the ladder discriminates (P2, S1, E2, S2), Opus performs well
  (P2 5/7, S1 4/9 with high partials, E2 5/10 — the only model to pass
  E2 five times).

**Bottom line for v2**: cells where the ceiling instrument scores zero
alongside every local (EU1, C1, C2, EU2) measured the harness and should
be redesigned per pvlyh/opyuy/ae1pt; cells with tier-graded results
(P1/P2/S-family/E2/EU3-as-floor) carry real signal and set the v2
difficulty anchors. The locals' graveyard zeros were never evidence of
local-model inadequacy.

## Ledger

- Bundles: `r2-poe:~/6i8jz-run/battery-results/opus1/` (+ `opus1_joined.json`)
- Joined data: `dev/battery/comparison/data/opus1_joined.json` (not in the
  comparison HTML by design, see above)
- Shim: `dev/scratch/claude-shim/shim.py` (port target: tuxlink-bench);
  quarantined gate attempts `sonnet1-burnin-*`, `fable1-burnin-policy-blocked`,
  `opus1-burnin-*` document the bridge's iteration history
- Supplemental option (operator call): rerun A1/S3/S4 (~30 attempts, ~1h
  subscription) — recommended only AFTER the null-args mechanism is fixed,
  or the same contamination recurs
- Arm issue: `bd show tuxlink-28nxq`
