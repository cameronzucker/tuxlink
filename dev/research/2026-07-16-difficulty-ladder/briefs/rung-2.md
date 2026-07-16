# Rung 2 brief — per-step parked windows in ganttModel (vehicle: bd tuxlink-10dh5)

You are implementing ONE localized bug fix in the tuxlink repository. Your
working directory is the repository root of a dedicated checkout; work only
there.

## Repo context (all you need; do not explore beyond it)

- Tuxlink is a Tauri 2.x Linux desktop app; the frontend is React 18 +
  TypeScript under `src/` (Vite, vitest, `pnpm`).
- The routines run monitor renders a journal of run events as a Gantt.
  `src/routines/designer/RunsTab.tsx` exports `ganttModel(entries, now)`
  (line ~147), which builds lanes and bars from `JournalEntry[]`
  (types in `src/routines/routinesApi.ts`: `state_changed` events carry
  `state` plus optional `step?: string` / `rig?: string`).
- Parked intervals (states `waiting`, `awaiting_consent`, `awaiting_radio`)
  are tracked by a SINGLE nullable slot (line ~160):

```ts
let parked: {
  state: RunState;
  ts: number;
  openSnapshot: Map<string, JournalEntry>;
  exactStep?: string;
} | null = null;
```

  A park opens in the `state_changed` case (line ~230) whenever the state is
  one of the three parked states; any other `state_changed` closes it via
  `closeParked(ts)` (line ~172), which attributes the bar to `exactStep` when
  present (enriched journals) or falls back to the legacy heuristic
  (`openSnapshot` / `lastClosed`). `run_finished` and the live `now` edge
  also close an open park.

## The bug (bd tuxlink-10dh5)

In a multi-track run, parks can OVERLAP: delay `d1` parks, delay `d2` parks,
then `d1` resumes, then `d2` resumes. The single `parked` slot means the
second open OVERWRITES the first: one interval is lost and the close is
misattributed. The enriched `step` field makes exact tracking possible.

## The fix (approach is binding; exact code is yours)

1. For ENRICHED entries (the opening `state_changed` carries `step`), key
   open parked windows by step id — a `Map<string, ...>` of open parks —
   so overlapping parks coexist. A closing `state_changed` that carries
   `step` (the executor emits resume as `running` with the SAME step id)
   closes exactly that step's window.
2. A closing `state_changed` WITHOUT `step`, `run_finished`, and the live
   `now` edge each close ALL windows still open at that timestamp.
3. LEGACY journals (opening entry has no `step`) must behave EXACTLY as
   today: single-slot semantics, same heuristic attribution in
   `closeParked`, byte-identical bars for old fixtures. Do not change the
   legacy attribution logic.
4. Every existing test in `src/routines/designer/RunsTab.test.tsx` must
   still pass unmodified.

## Test to add (binding)

Extend the `describe('ganttModel (a)')` block in
`src/routines/designer/RunsTab.test.tsx` with an overlapping-parks fixture
test: two tracks with delay steps `d1` and `d2`; journal sequence — `d1`
parks (`waiting`, step `d1`), `d2` parks (`waiting`, step `d2`), `d1`
resumes (`running`, step `d1`), `d2` resumes (`running`, step `d2`) — with
distinct timestamps. Assert TWO delay bars, each attributed to its own
step's lane, each spanning exactly its own park interval (t0 = its park
timestamp, t1 = its resume timestamp). Model the fixture on the existing
`'attributes a parked window to the exact step named on state_changed, not
the heuristic'` test (line ~227): inline `JournalEntry[]` literal, explicit
`now` argument. Use explicit vitest imports; this project sets
`globals: false`. TDD: write the test first and show it FAIL against the
current single-slot code, then make it pass.

## Constraints (binding)

- Touch ONLY `src/routines/designer/RunsTab.tsx` and its test file. No new
  dependencies.
- Do NOT run any `git` command. Do not commit. The orchestrator commits.

## Gates (run these exact commands from the repo root; capture real output)

- `pnpm vitest run src/routines/designer/RunsTab.test.tsx`
- `pnpm typecheck`

## Completion report (your final message)

1. Files touched (paths).
2. Test names added/modified.
3. TDD evidence: the failing run (verbatim key lines) and the passing run.
4. Verbatim final output lines of both gate commands.
5. Any deviation from the brief, with the reason (deviating without
   reporting is a defect).
