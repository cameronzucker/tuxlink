# Handoff — 2026-07-11 (owl-kestrel-lichen): Station Intelligence L3 — full BRF gate done, plan execute-ready

One workstream: took tuxlink-b026z.4 (L3 panel integration) from kickoff
through the ENTIRE build-robust-features gate — brainstorm → 5-round
adversarial spec review → plan → 3-round plan review — stopping at the
execution boundary by operator instruction. **No implementation code written
yet.** The next session executes the plan.

## Branch / state

- Branch `bd-tuxlink-b026z.4/station-intel-l3-panel` (off `origin/main` at the
  L2-shipped `f0431195`). Working tree CLEAN, fully pushed (HEAD `d60c038b`).
- Worktree `worktrees/bd-tuxlink-b026z.4-station-intel-l3-panel/` — KEEP (the
  next session executes here; do NOT dispose). `node_modules` installed.
- bd tuxlink-b026z.4 = IN_PROGRESS, claimed, note updated with the plan/spec
  pointers.

## What shipped (docs only — spec, plan, mocks)

- **Spec** `docs/superpowers/specs/2026-07-11-station-intel-l3-panel-design.md`
  — status APPROVED. Survived 5 adversarial rounds: R1 contract-grounding + R2
  state/UX (14 P1), R3 Codex (4 P1, transcript local at
  `dev/adversarial/2026-07-11-station-intel-l3-spec-codex.md`, gitignored), R4
  backend-depth + R5 completeness. Every finding dispositioned; the full log is
  the spec's last section. One operator decision resolved inline: the epic's
  "no WSJT-X installed" is stale post-L0-pivot wording (jt9 IS required, comes
  with the wsjtx package) — read as "no WSJT-X GUI/config"; setup arm says
  "install WSJT-X". No bundling.
- **Plan** `docs/superpowers/plans/2026-07-11-station-intel-l3-panel.md` — v3,
  execute-ready. 32 tasks, 4 phases (A backend/Rust → B frontend hook+pure
  derivations → C 11 components → D integration+gates). 3 review rounds (2
  subagent + 1 clean-pass verifier), all dispositioned; verified zero stale
  refs. Disposition log is the plan's last section.
- **Approved mocks** `docs/design/mockups/2026-07-11-station-intel-l3/` (4 HTML
  + committed). Operator approved v4 as "final shape" — all rendered in the
  REAL WebKitGTK engine via `dev/render-harness/snapshot.py`, not a browser
  (operator rejected laptop-browser renders as approval-grade this session; see
  memory `high-fidelity-mocks-with-realistic-content`).

## Execution guidance (READ THE PLAN FIRST)

- **Method:** subagent-driven-development, one subagent per task, two-stage
  review between. **Parent commits every task** — subagents CANNOT commit in
  this worktree (memory `subagents-cannot-commit-in-worktrees`); they code +
  STOP.
- **Phase A is STRICTLY SEQUENTIAL** (A1→A7): the 7 Rust tasks share
  `lib.rs`/`commands.rs`/`service.rs`/`Cargo.toml`. Do NOT parallel-dispatch
  them. The plan's Phase-A header box has the `bd dep` edges.
- **This Pi cannot cold-compile Rust** — Phase A/B/C tasks push to CI (both
  arches) to compile/test; `pnpm vitest run <file>` runs locally for single
  frontend files. Never claim a Rust test passed locally.
- **Two supervisor edits are flagged** (not "L2 state machine untouched"): the
  device reservation adds a check to `execute_start_sequence` step 7, and
  `configuredDeviceName` stores the resolved name in `Inner` at resolve time.
- **Phase C sequencing** on shared files: StationRail C5→C6→C3; AppShell C2
  (provider+ribbon) vs D1 (catalogPrefill only); StationFinderPanel C1→D1.
- **Exit gates (D2–D6)** incl. the wire-walk gate (D5) tracing Flow 1's
  non-heatmap clauses verbatim; the heatmap clause defers to L5, Flow 2 to L4.

## Worktree state (per ADR 0009 — enumerated for a future disposer)

- Tracked: clean, pushed.
- Untracked (exclude-standard): none.
- Gitignored-stateful on disk: `.superpowers/brainstorm/2018068-1783763161/`
  (companion-server scratch — mocks there are SUPERSEDED by the committed
  `docs/design/mockups/` copies; server STOPPED this session). `dev/scratch/`
  L3 HTML + PNG renders (the harness sources; superseded by committed mocks).
  `dev/adversarial/2026-07-11-station-intel-l3-spec-codex.md` (Codex R3
  transcript, ~11k lines, local-only). None are load-bearing for execution —
  the committed spec/plan/mocks are self-contained.
- Stashes: 7 exist in the repo stash list but ALL belong to OTHER
  sessions/branches (task-amd-main-ui, main, bd-tuxlink-fl6e). NOT this
  session's — do not clear them (they may be another session's recovery net).

## Not done / pending

- All L3 implementation (Phases A–D). The plan is the definition of done.
- L4 (tuxlink-b026z.5, MCP tools) carries an operator scope-expansion note
  from this session (agent band-intelligence + CAT control + station
  recommendation, verbatim on the issue). L4 gets its own BRF cycle after L3.
- L5 (tuxlink-b026z.6, heat layer) is the FT-8 heat toggle's home — L3 ships
  the layer-control housing (Gateways entry only), no dead toggle.
