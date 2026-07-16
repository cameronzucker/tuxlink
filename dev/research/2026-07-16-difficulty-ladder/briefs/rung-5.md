# Rung 5 brief — Station Data window: empty-roster bug (vehicle: bd tuxlink-gac1d)

You are diagnosing and fixing ONE reported bug in the tuxlink repository.
Your working directory is the repository root of a dedicated checkout; work
only there. This brief gives you the SYMPTOM as reported; finding the root
cause is the task. Explore the codebase as needed.

## Repo context (starting points only)

- Tuxlink is a Tauri 2.x Linux desktop app; the frontend is React 18 +
  TypeScript under `src/` (Vite, vitest, `pnpm`); the Rust/Tauri side is
  under `src-tauri/`.
- The app can pop station data out into a separate OS window ("Station
  Data"), rendered by `src/aprs/StationsView.tsx`.

## The bug report (symptom, verbatim from the field)

> With APRS running in the main window and a roster of stations already
> accumulated, popping out the Station Data window shows an EMPTY roster.
> Stations only start appearing if fresh RF traffic arrives after the
> window opened; everything accumulated before the pop-out never shows.
> There is a snapshot handshake that is supposed to seed the popped window
> from the main window's roster — it silently never delivers in the real
> app. All existing unit tests pass, including the handshake's own tests.

## The task

1. ROOT-CAUSE the failure: find the exact mechanism by which the handshake
   never delivers in the running app while its unit tests stay green.
   Your report must name the mechanism precisely and explain why tests
   cannot see it.
2. FIX it minimally, following existing repo conventions for whatever layer
   the fix lives in.
3. State explicitly what CAN and CANNOT be verified by unit test for this
   class of defect, and verify what can be.

## Acceptance criteria (what the orchestrator will verify)

1. The root-cause statement identifies the true mechanism (the orchestrator
   holds a grading key prepared in advance).
2. The fix is minimal, at the correct layer, and consistent with the
   sibling declarations already in the repo.
3. `pnpm typecheck` green; `pnpm vitest run src/aprs/useEnvStations.test.ts`
   green; any test you add or touch green.

## Constraints (binding)

- No new dependencies.
- Do NOT run any `git` command. Do not commit. The orchestrator commits.

## Completion report (your final message)

1. Root cause: the mechanism, the exact file:line(s) at fault, and why unit
   tests pass anyway.
2. The fix: files touched, with the reasoning for the layer you chose.
3. Verbatim final output lines of every gate command you ran.
4. What remains unverifiable by unit test, stated plainly.
5. Any deviation from this brief, with the reason (deviating without
   reporting is a defect).
