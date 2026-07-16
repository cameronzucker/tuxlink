# Rung 6 brief — delay-bar DOM rendering test (vehicle: bd tuxlink-y6195 item 5)

You are implementing ONE test-coverage task in the tuxlink repository. Your
working directory is the repository root of a dedicated checkout; work only
there.

## Repo context (all you need; do not explore beyond it)

- Tuxlink is a Tauri 2.x Linux desktop app; the frontend is React 18 +
  TypeScript under `src/` (Vite, vitest, `pnpm`).
- The routines run monitor `src/routines/designer/RunsTab.tsx` renders a
  Gantt from a run journal. Bars carry `className={"bar " + bar.kind}` and
  `data-testid={"bar-" + (bar.stepId ?? 'delay') + "-" + bar.kind + "-" + bi}`.
  A `waiting` park from a delay control step produces a bar with
  `kind: 'delay'`, styled dashed via CSS (`.bar.delay` in `RunsTab.css`).
- `src/routines/designer/RunsTab.test.tsx` covers the `ganttModel` LOGIC for
  delay bars, but NO test drives a delay bar through the rendered DOM — the
  rendering path for `bar delay` is untested (bd tuxlink-y6195 item 5).

## The task

Add ONE rendered-component test to `RunsTab.test.tsx` that mounts `RunsTab`
with a journal producing a delay bar and pins the DOM behavior:

1. Extend the existing journal fixture builder `makeDelayRunJournal` in
   `RunsTab.test.tsx` (it builds enriched delay-park journals) to produce a
   finished run whose delay step `d1` parked (`waiting`, step `d1`) and
   resumed (`running`, step `d1`).
2. Feed the journal to the component the way the file's other rendered
   tests do (the `routines_journal` invoke-mock override), select the run,
   and locate the delay bar by its `data-testid` (`bar-d1-delay-…`).
3. Assert the bar renders with class `bar delay`.
4. Delay bars are clickable like any other bar — clicking one selects the
   step and opens the step detail pane. Assert that clicking the delay bar
   opens step detail for `d1`.

Use explicit vitest imports; this project sets `globals: false`.

## Constraints (binding)

- Touch ONLY `src/routines/designer/RunsTab.test.tsx`. No production-code
  changes. No new dependencies.
- Do NOT run any `git` command. Do not commit. The orchestrator commits.

## Gates (run these exact commands from the repo root; capture real output)

- `pnpm vitest run src/routines/designer/RunsTab.test.tsx`
- `pnpm typecheck`

## Completion report (your final message)

1. Files touched (paths).
2. Test names added.
3. Verbatim final output lines of both gate commands.
4. Any deviation from the brief, with the reason (deviating without
   reporting is a defect).
