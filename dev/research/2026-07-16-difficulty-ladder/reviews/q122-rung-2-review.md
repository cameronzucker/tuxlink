# q122 rung 2 review — per-step parked windows in ganttModel

**Verdict: Approve-with-minors** — C:0 I:1 M:1

## Scope vs brief
Brief: fix overlapping parked windows in `ganttModel`. Enriched entries (opening
`state_changed` carries `step`) key open parks by step id in a Map so
overlapping parks coexist; a closing `state_changed` with `step` closes exactly
that step; a closing `state_changed` without `step`, `run_finished`, and the
live `now` edge close ALL open windows; LEGACY journals (no `step`) keep
single-slot semantics and byte-identical attribution. Add an overlapping-parks
test to the `describe('ganttModel (a)')` block; all existing tests pass
unmodified. Touch only `RunsTab.tsx` + its test.

The production change conforms fully and correctly. One test-placement
deviation from the brief (see I1).

## Correctness of the fix — sound
`RunsTab.tsx:158-214, 245-272`:
- Enriched parks now live in `parkedByStep: Map<string, {...}>`
  (`:162`); legacy parks in `parkedLegacy` (`:163`). Opening a parked
  `state_changed` routes on `ev.step !== undefined` (`:247-253`) — enriched →
  `parkedByStep.set(step, ...)`, legacy → single slot.
- `closeParkedStep` (`:192-205`) reproduces the old enriched (`exactStep`)
  branch verbatim: `intentEntry = openSnapshot.get(stepId)`, same
  `action: fields?.action`, same `pushBar` shape. I diffed it against the
  removed `exactStep` block — field-for-field identical, so the single-park
  enriched tests (`RunsTab.test.tsx:227`, `:261`) are unaffected.
- `closeParked` (`:174-190`) is the legacy path with the `exactStep` branch
  removed; legacy journals never set `exactStep`, so behavior is unchanged
  (`openSnapshot.size>0` → per-open-intent bars; else `lastClosed`; else drop).
  Verified against the legacy test (`:297`) which still attributes to the most
  recently closed step.
- `closeAllParked` (`:207-214`) closes every enriched window then the legacy
  slot — correctly wired into the no-`step` close, `run_finished` (`:263`), and
  the live `now` edge (`:270-272`).

## TDD guard is real
Traced the new fixture (`RunsTab.test.tsx:321-369`) against BOTH codebases:
- New code: d1 parks T+10, d2 parks T+20 (coexist in the Map), d1 resumes
  T+30 → delay bar d1 (t0 T+10/t1 T+30, track-1), d2 resumes T+40 → delay bar d2
  (t0 T+20/t1 T+40, track-2). Assertions match.
- Old single-slot code: the d2 park (T+20) OVERWRITES the d1 park; resume-d1
  closes it as d2 (t0 T+20/t1 T+30), resume-d2 sees a null slot and no-ops →
  `delay1` (d1) is `undefined` and the test fails. So it genuinely fails
  pre-fix — a real guard, not a tautology.

## Findings

### I1 — new test placed OUTSIDE `describe('ganttModel (a)')`, contra binding instruction (`RunsTab.test.tsx:318-369`)
The brief is explicit: "Extend the `describe('ganttModel (a)')` block ... with
an overlapping-parks fixture test." The diff inserts the `it(...)` AFTER the
`});` that closes the describe (`:318`), so it is a floating **top-level** `it`
at module scope — yet it is indented two spaces as if nested, which is
misleading to a maintainer. It still executes (vitest registers a root-level
`it`) and passes, so coverage is achieved and this is not a correctness bug —
hence Issue, not Critical. But it violates an explicit binding placement
instruction and leaves a structural wart (a bare `it` sandwiched between two
`describe`s with deceptive indentation). Should be moved inside the block before
merge. If the completion report did not disclose this deviation, that is itself
a reportable defect per the brief's "deviating without reporting is a defect."

### M1 — stray double blank lines around the inserted test (`RunsTab.test.tsx:319-320, 370-371`)
Two consecutive blank lines both before and after the new `it`. Cosmetic; a
prettier/lint pass would collapse them. (Also: the new test calls
`ganttModel(journal)` without the explicit `now` arg the brief mentioned, but
the run is finished so `now` is never consulted — harmless, and it matches how
the existing enriched test at `:250` actually calls it.)
