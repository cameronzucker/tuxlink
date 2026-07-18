# Routines observability wire-walk: History vs. a real executed run

**Decree (operator, repo-deletion strength):** full visibility into what
did/didn't run, why/why not, each activity's outputs, and end state, across the
History UI and backend logs. This document is the wire-walk of that decree
against a REAL run, and its findings are the observability requirements for
round 2. History had never been validated against a real run before this walk.

## Method

- Probe routine `heron-observability-probe` authored and saved through the
  live R2 converge build (7d801187) via the MCP shim
  (`/run/user/1000/tuxlink/mcp.sock`): 8 steps exercising a data read with
  output, `$`-resolution into a later step's params, a branch decision, an
  untaken-arm layout, a guaranteed verbatim failure (`data.read`
  `source=heard_stations` honest gap), post-failure steps, and an `end`.
  No radio, no transmit, no internet.
- Executed for real: `run-1784416315-0000`, terminal state `failed`. The
  12-entry journal was captured verbatim (`routines_journal_get`) into
  `dev/render-harness/real-run-20260718.json`.
- The History UI (RunsTab) and dashboard were rendered against that real
  journal in real WebKitGTK via the render harness's new `&real=1` overlay:
  real engine data through the real UI components; only the IPC transport is
  shimmed.

## What PASSES the decree today

| Dimension | Evidence |
|---|---|
| Post-`$`-resolution params | `step_intent` for s4 shows `message: "AKQHQ5KF7FR7"`, the resolved value of `$s2.mid`, not the token |
| Per-step outputs in the journal | `step_ok` carries full output objects (`{"grid":"DM33wp"}`, `{"staged":true,"mid":...}`) |
| Verbatim failure causes | `step_err` and `run_finished.reason` carry the honest-gap text verbatim, unparaphrased, end to end into both UI surfaces |
| Terminal state | `run_finished` state + reason; run list and dashboard both show `failed` |
| Full resolved snapshot | `run_started.snapshot` is the exact definition the run executed; `dry_run` marker present |
| Delay/consent parks | `state_changed` events carry the owning step id (journaled wake times, per spec §6) |
| Intent-before-effect | every action writes `step_intent` before executing |
| Retry visibility | each attempt re-journals intent/err (no attempt counter, minor) |

## Decree GAPS (the round-2 observability requirements)

### Journal layer (`tuxlink-routines`)

- **O1: branch decisions are invisible.** No journal event exists for
  `Control::Branch`: not the resolved value, not the chosen arm, not the jump
  target. The probe demonstrated the compounding trap empirically: branch
  semantics are JUMP (then/else name a jump target; execution falls through
  sequentially afterward), so s4 AND s5 both executed, and nothing in the
  journal or UI can explain why. Requirement: a `branch_taken` event
  `{step, on, value, target}`.
- **O2: steps that never ran are unrecorded.** After s6 failed, s7/s8 simply
  never appear; retry-consumed steps are skipped silently via the executor's
  `consumed` set. The UI cannot show "did not run, because the run failed at
  s6" without deriving it. Requirement: `step_skipped {step, reason}` events
  for the unexecuted remainder at terminal time and for consumed steps.
- **O3: parent runs do not record child run ids.** `Control::Call` journals
  intent/ok/err but provenance flows child-ward only; History cannot navigate
  from a parent run to the child run's journal. Requirement: child `run_id` in
  the call step's journal record.
- **O4 (minor): `end` steps emit no event.** `run_finished` does not say which
  End step terminated a multi-End routine.

### History UI layer (RunsTab)

- **O5: successful steps' outputs are not inspectable.** The only per-step
  detail surface is the failure card. s1's grid, s2's staged message id, the
  log writes: present in the journal, invisible in the UI. The decree names
  "each activity's outputs" explicitly; this is the largest UI gap.
- **O6: sub-second runs render an unusable timeline.** The Gantt collapses to
  a ~4px sliver ("elapsed 00:00:00"); at this scale the timeline is the ONLY
  step-level surface and it carries no information. Requirement: a
  chronological step LIST (params, output, error, expandable) that does not
  depend on timeline scale; the Gantt stays for long runs.
- **O7: no branch/skip presentation.** Depends on O1/O2; the UI must render
  the decision (which arm, on what value) and the not-run steps with reasons.

## Fixture and regression value

`dev/render-harness/real-run-20260718.json` + `&real=1` keep this real run
renderable forever; when O1..O7 land, re-run the same walk and the same
fixture (plus a fresh capture with the new events) becomes the acceptance
check. The probe routine definition lives in the journal snapshot inside the
fixture.
