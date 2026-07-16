# o397 rung 2 review — per-step parked windows in ganttModel

**Verdict: Approve** — C:0 I:0 M:0

## Brief compliance
- Enriched parks keyed by step via an always-present `parkedByStep: Map`; `closeOneParked(step, ts)` closes exactly one window; `closeAllParked(ts)` closes all enriched + the legacy slot. ✓
- Closing `state_changed` with `step` → `closeOneParked`; without `step`, `run_finished`, and live `now` edge → `closeAllParked`. ✓ (brief points 1–2)
- Legacy path preserved verbatim in `legacyParked` with the original `openSnapshot` / `lastClosed` heuristic. ✓ No existing test modified. ✓
- New test correctly nested INSIDE `describe('ganttModel (a)')` (inserted at line 315, before the block's closing `});`). ✓
- Only `RunsTab.tsx` + its test touched; no new deps. ✓

## Correctness
Clean two-function split (`closeOneParked` / `closeAllParked`) reads clearly. The fixture uses two tracks (track-a/d1, track-b/d2), asserts two lanes, two delay bars with correct t0/t1, AND verifies each bar lands in the correct lane — stronger lane-attribution coverage than strictly required. Legacy behavior is byte-identical.

## Scope / hygiene
No drift, no dead code. Comments updated to describe the new per-step vs legacy split accurately. Marginally cleaner than the cn candidate (always-present Map vs nullable Map; correct describe nesting).
