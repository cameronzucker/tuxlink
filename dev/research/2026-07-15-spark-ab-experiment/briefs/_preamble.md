# Worker brief — journal `StateChanged` step/rig enrichment (one task of three)

You are implementing ONE task of a three-task feature in the tuxlink repository.
Your working directory is the repository root of a dedicated worktree; work only
there.

## Repo context (all you need; do not explore beyond it unless a step says to)

- Tuxlink is a Tauri 2.x Linux desktop app: Rust backend under `src-tauri/`,
  React 18 + TypeScript frontend under `src/` (Vite, vitest).
- `src-tauri/tuxlink-routines/` is a self-contained Rust engine crate (its own
  Cargo.toml; serde + tokio only). It journals every run state transition to a
  JSONL write-ahead file (`src/journal.rs`) from the executor
  (`src/executor.rs`).
- The frontend run monitor (`src/routines/designer/RunsTab.tsx`) renders that
  journal as a Gantt; `src/routines/routinesApi.ts` mirrors the wire types.
- The feature (bd tuxlink-xvd1i): `state_changed` journal entries gain optional
  `step`/`rig` context so the monitor attributes parked intervals exactly
  instead of via an adjacent-step heuristic. Old journals must keep parsing and
  rendering exactly as before.

## Global constraints (binding)

- MSRV is Rust 1.75; clippy denies `incompatible_msrv` — no APIs stabilized in
  1.76+ (e.g. `Result::inspect_err` is banned).
- CI runs `cargo clippy --all-targets -- -D warnings`: arm against common traps
  — inline format args (`format!("{x}")`, not `format!("{}", x)`), no needless
  `.clone()`/borrows, `matches!` for single-pattern booleans.
- No new dependencies. No files touched beyond the task's **Files** list.
- Back-compat both directions is a hard requirement (the task's tests pin it).
- Comments: match the surrounding density and voice — this codebase writes
  load-bearing doc comments; write yours to state constraints, not narration.
- TDD: execute the steps IN ORDER, run every listed command, and capture real
  output. Do not skip the "verify it fails" steps.
- Do NOT run any `git` command. Do not commit. The orchestrator commits.

## Completion report (your final message)

1. Files touched (paths).
2. Test names added/modified.
3. Verbatim final output lines of every gate command the task lists.
4. Any deviation from the brief, with the reason (deviating without reporting
   is a defect).

---
