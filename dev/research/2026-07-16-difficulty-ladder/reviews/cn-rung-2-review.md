# cn rung 2 review — per-step parked windows in ganttModel

**Verdict: Approve-with-minors** — C:0 I:1 M:1

## Brief compliance
- Enriched parks keyed by step id via `parkedByStep: Map | null`; overlapping parks coexist. ✓
- Closing `state_changed` WITH `step` closes exactly that step's window (`closeParked(ts, ev.step)`); closing WITHOUT `step`, `run_finished`, and the live `now` edge close ALL open windows. ✓ (matches brief points 1–2)
- Legacy single-slot preserved in `parkedLegacy` with the original `openSnapshot` / `lastClosed` heuristic verbatim (RunsTab.tsx close-all branch). ✓ No existing test modified. ✓
- Only `RunsTab.tsx` + its test touched; no new deps. ✓

## Correctness
Implementation is correct. Legacy journals (opening `state_changed` has no `step`) route to `parkedLegacy` and close via the unchanged heuristic — byte-identical bars. The overlapping fixture (d1 parks, d2 parks, d1 resumes, d2 resumes) produces two delay bars with correct t0/t1 attribution.

## Findings
- **Important — test placed OUTSIDE the target describe block** (`RunsTab.test.tsx`, the added `it` after line 318). The brief bindingly said "Extend the `describe('ganttModel (a)')` block". The base file closes that describe at line 318 (`});`); the new `it('handles overlapping parked windows …')` is inserted after that close, before `describe('radioAwaitRig')`, so it is a floating top-level `it` at 2-space indent, not a member of `ganttModel (a)`. It still runs and passes, so the gate is green, but the placement violates a binding instruction and reads as a stray/misindented test. Compare o397's candidate, which nests it correctly.
- **Minor — nullable-Map juggling.** `parkedByStep: Map<...> | null` plus the `if (parkedByStep.size === 0) parkedByStep = null` bookkeeping is more convoluted than a plain always-present `Map` (o397's approach). Harmless but adds needless null branches.
