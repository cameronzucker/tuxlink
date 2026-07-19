# Handoff — tour radio-dock fix + Routines composer punch list SHIPPED; Contacts backlog filed

- **Agent:** owl-moraine-sycamore
- **Date:** 2026-07-18 (session spanned the 17th–18th)
- **State:** everything this session touched is merged and closed. No in-flight worktrees, no pending decisions.

## Shipped and merged

1. **PR #1145 — onboarding tour radio-dock stop (bd tuxlink-fh53x, CLOSED).**
   Both diagnosed defects fixed per poplar-mink-chasm's handoff: the
   `data-tour-anchor="radio-dock"` moved off the `display:contents`
   `.radio-drawer` onto RadioPanel's boxed chrome root + `.aprs-dock-surface`
   (mirrors the `radio-dock-open` probe), and the stop's fallback changed
   `'center'` → `'skip'` (matches `mailbox`). Product decision settled:
   skip-when-absent, not teach-via-openHint. TDD (4 regression tests watched
   RED); runtime-verified in real WebKitGTK via the render harness, including
   a new `?view=onboarding&state=stop4dock` fixture that mounts the real
   drawer→panel chain. The harness's tour driver is now event-driven (the
   fixed-40ms chain misbehaved under load).

2. **PR #1148 — Routines composer 10-item operator punch list (bd
   tuxlink-7ewvq, CLOSED).** Highlights: the "random oval" was
   SettingsTab.css's bare `.radio` selector leaking onto the palette's RADIO
   header (scoped + CSS-source guard test); the palette is never disabled —
   unpositioned clicks append to the end of the routine; params are a
   key/value grid (JSON behind an "edit as JSON" toggle); the Settings tab is
   gone (sections render inline under the canvas; `tab === 'settings'` in old
   continuity tokens lands on Design); Runs → History; "Start · manual"
   trigger copy; "TRACK 1 · TRACK-1" dedupe; "TX: attended" chip with
   tooltip; type scale up a notch; all "armed" jargon deleted.
   272/272 routines tests (two clean runs), typecheck clean, WebKitGTK
   before/after renders preserved at `dev/scratch/7ewvq-*.png` (main repo,
   gitignored) alongside `dev/scratch/fh53x-*.png` for #1145.

## Filed for future sessions (all open, verbatim operator findings)

- **tuxlink-6vn4x (P1)** — Contacts lack mock-advertised core functionality:
  groups don't discernibly exist in the shipped UI (GroupEditor.tsx exists —
  suspect unreachable, wire-walk class), no mode/frequency add-or-edit,
  Peers worst-hit. Fix session should ALSO claim tuxlink-c6m7 (contact click
  does nothing) and tuxlink-pw5nk (edit collapses the list) and wire-walk
  the whole Contacts surface against the original mocks.
- **tuxlink-pw5nk (P2)** — editing a contact collapses the contact list.
- **render-harness type drift** — 6 pre-existing tsc errors in
  `dev/render-harness/harness.tsx`; no typecheck gate covers the file
  (tsconfig includes only `src`). Issue filed this session (search bd for
  "render-harness harness.tsx has drifted").

## Environment notes

- A 5-day-old orphan Vite (pid 213856, another session's worktree
  `bd-tuxlink-b026z.4-station-intel-l3-panel`) still holds port 1420 on this
  Pi. Not mine to kill per the shared-Pi rule; sessions needing Vite should
  use `pnpm exec vite --port 1430`.
- Background Bash tasks do NOT inherit the shell's persistent cwd — a full
  vitest run silently executed in the main repo (old vitest, wrong branch)
  until re-run with an explicit `cd`. Always `cd` inside background commands
  and verify the `RUN v… <path>` header.
- The 5 local full-suite vitest failures seen mid-session (AppShell.dock /
  elmer / radioPanel / RoutineDesigner) are Pi-contention flakes: all pass in
  isolation and CI's full-suite verify passed both arches on both PRs.
- `gh pr merge` must run as a BARE single command — chained/piped forms trip
  the permission classifier (operator re-confirmed the standing
  merge-after-CI-green authorization; memory saved).

## Worktree state

None. Both session worktrees (`bd-tuxlink-fh53x-radio-dock-tour-fix`,
`bd-tuxlink-7ewvq-routines-composer-ux`) were disposed per the ADR 0009
ritual after their PRs merged (clean inventories; verification PNGs copied
to the main repo's `dev/scratch/` first). This handoff's own ephemeral
worktree (`worktrees/owl-handoff`) is disposed after the direct-to-main
push. The repo-global stash stack (7 old entries from May–June sessions)
was left untouched.
