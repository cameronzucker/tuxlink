# Rung 4 brief — make IPC-layer invoke failures observable (vehicle: bd tuxlink-o1e9w)

You are implementing ONE improvement in the tuxlink repository. Your working
directory is the repository root of a dedicated checkout; work only there.
This brief states the GOAL and the acceptance criteria; the design — which
files, what API, how far to sweep — is yours to discover and decide. Explore
the codebase as needed.

## Repo context (starting points only)

- Tuxlink is a Tauri 2.x Linux desktop app; the frontend is React 18 +
  TypeScript under `src/` (Vite, vitest, `pnpm`). The Rust backend is under
  `src-tauri/` — you will not need to change it.
- The frontend calls the backend with `invoke` from `@tauri-apps/api/core`.

## The problem

When an `invoke()` fails at the IPC boundary — argument-shape mismatch, a
command missing from the invoke handler, unmanaged state, serde enum-rename
drift between the UI's string unions and the Rust enums — the JS promise
rejects WITHOUT any backend log line. Nothing reaches the structured
forensic logs. Field debugging of exactly this failure class has repeatedly
required instrumented rebuilds. The project already has a frontend-to-
forensic-log error channel; find it and use it.

## The goal

Every `invoke()` rejection anywhere in the frontend should land in the
structured frontend error log, identifying which command failed, without
changing any caller-visible behavior (callers that catch rejections must
still receive them exactly as before).

## Acceptance criteria (what the orchestrator will verify)

1. A CENTRAL mechanism (one chokepoint, not per-call-site discipline)
   catches invoke rejections, reports them to the project's frontend error
   channel with a source that names the failed command, and RETHROWS so
   existing caller catch-paths are unchanged. Successful invokes pass
   through with their resolved value and no reporting.
2. Unit tests prove: (a) a rejected invoke is reported once with the
   command name in the source and the rejection still propagates to the
   caller; (b) a successful invoke passes its value through and reports
   nothing.
3. At least one real production module is migrated to the new mechanism and
   its existing tests still pass, demonstrating the migration is
   drop-in.
4. An honest statement in your report of the remaining migration surface
   (how many files/call sites still import `invoke` directly, and how you
   measured that) plus anything you recommend to keep future code on the
   chokepoint. Migrating the entire codebase is NOT required — a complete,
   correct core plus a demonstrated migration beats a rushed sweep.
5. `pnpm typecheck` green; every test file you touched or added green via
   scoped `pnpm vitest run <files>`.

## Constraints (binding)

- No new dependencies. No Rust changes.
- Do NOT run any `git` command. Do not commit. The orchestrator commits.

## Completion report (your final message)

1. Files created/modified (paths).
2. The design in 3-5 sentences: what the chokepoint is, how callers adopt
   it, what stays unmigrated.
3. Test names added; verbatim final output lines of every gate command you
   ran.
4. The remaining-surface measurement (criterion 4).
5. Any deviation from this brief, with the reason (deviating without
   reporting is a defect).
