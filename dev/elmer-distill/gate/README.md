# Frozen hard GATE — candidate scenarios

The acceptance gate for the Elmer distillation (spec §6-8). Each scenario is
**hand-authored, top-tier, outcome-graded**, and carries `provenance`
(source event, operator job, expected artifact, why it is hard). Scenarios are
graded by evidence-bound predicates bound to real simulator tool outputs — never
free-text substring matching for correctness.

## Status: CANDIDATES (pre-freeze, pre-red-team)

`candidates/` holds a **representative batch** across the families
(command-post, radio-debug-under-fault, real taint-refusal, helpdesk,
blended, multi-artifact). It establishes the evidence-bound pattern — it is
**not** the full suite.

## Two gates before this suite is frozen

1. **Operator red-team (Codex G).** The operator reviews these candidates for
   *real* difficulty and realism — flagging any that are "hard only in the
   author's head."
2. **Operator-authored greenfield subset (Codex A/G + wire-walk).** The operator
   supplies their own genuinely-hard tasks/incidents from real operating
   experience (emcomm *and* everyday Winlink-support frustrations), authored
   **before** reviewing the drafted candidates so the author's synthesis does not
   anchor them. These are marked `operator_authored: true` and are NOT selected
   by teacher success during calibration.

Only after both gates + empirical calibration (`calibrate.py`, keep the
discrimination band) is the suite frozen. Stage-1 is ~40 candidates =
**directional pilot only**; a powered acceptance claim needs ~80-100 (Stage 2).

## Adding a candidate

Author a JSON `Scenario` with full `provenance` and evidence-bound `predicates`;
`tests/test_gate_lint.py` validates it loads, uses known predicates, and
references real tools.
