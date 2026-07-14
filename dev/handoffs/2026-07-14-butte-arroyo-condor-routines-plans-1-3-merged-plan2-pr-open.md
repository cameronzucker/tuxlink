# Handoff — Routines: plans 1 + 3 merged, plan 2 PR open (CI running), plans 4–6 remain

- **Agent:** butte-arroyo-condor
- **Date:** 2026-07-14 (session ran 2026-07-13 → overnight)
- **Ended:** operator requested a fresh session (a classifier kept switching the model mid-run — session-related, disruptive).

## READ THIS FIRST — where to resume

1. **Check CI on PR #1115** (`gh pr checks 1115`). It was **6 checks pending, 0 failing** at handoff. If green → **ask the operator to merge** (they merge; agent merges are permission-blocked). If red → fix-forward on `bd-tuxlink-ofw4s/routines-p2`.
2. **Then read the operator's decision below on sequencing** — they want the fastest honest path to a **live on-air test**, and the answer materially reorders the remaining plans.
3. Wire-walk for plan 2 is **WAIVED by the operator** (2026-07-14) — they will test live on hardware instead. Do not re-gate on it.

## What is DONE and MERGED

- **Plan 1 — core engine crate** (`src-tauri/tuxlink-routines/`): PR #1105, merged. Definition types, @-refs, write-ahead JSONL journal, Action trait + registry + fakes, executor (branch/retry/end/timeouts/$vars), delays + parallel tracks, call-composition with provenance + depth cap, snapshot resolution, epoch-anchored scheduler math, engine facade with launch recovery.
- **Plan 3 — validator + dry-run**: PR #1113, merged. 22 finding codes across refs/capability/contracts/structure/consent/fleet; enable-time fleet check; dry-run over a capability-mirrored fake registry (Call composition stays in the fake world); 28-fixture corpus (the Laserfiche failure taxonomy as tests) + grounding-scenario fixtures.

## What is BUILT, PUSHED, and IN REVIEW (PR #1115, DRAFT)

**Branch:** `bd-tuxlink-ofw4s/routines-p2` @ `4c16d939` · **Worktree:** `worktrees/bd-tuxlink-ofw4s-routines-p2` · **bd:** `tuxlink-ofw4s`

Plan 2 = the engine mounted into the app as a **running, supervised automation runtime**:

- **17 real actions** — radio (connect: station×band, listen-before-TX, real ARDOP/VARA dial chains via a new gateway-frequency resolver reading the stations cache; listen; APRS send), CAT (read state, validate/apply preset with compared/skipped honesty; switch_vfo + tune_atu surface *honest unsupported* errors — tux-rig has no such verbs and faking ATU tune via raw PTT is banned), data (WWV off-air with real :18/:45 window math, SWPC, station-list refresh, state reads incl. **persisted last-connected-gateway**), local (templated compose via the real forms renderer, catalog request = the KM4ACK flow native, run-scoped identity, log, notify).
- **RadioArbiter** — per-rig leases, the human operator as a first-class holder, seq-guarded releases; the lease tracks **physical rig occupancy** across every arm/Stop/disarm/session-end ordering.
- **Part 97 consent layer** — unacked-automatic refused at start; attended runs **park in the executor before the step timeout** (journaled `AwaitingConsent` → `Running`); grant is operator-UI-only (MCP gets no ack param).
- **Scheduler runtime** — this was a **completeness gap the review caught**: schedules validated, collided, and fired *nothing*. Now a supervised tick loop fires them (anchored, missed-fire policy, overlap suppression, disk-as-authority so a stale due-set can never fire a *disabled* routine).
- **Graceful quit** — gates BOTH exit paths (window close **and** the tray Quit button, which called `app.exit(0)` directly), with a bounded drain so cancelled runs reach their journal terminus (without it, next launch marked them `Interrupted` and a `resume`-policy routine **restarted the run the operator just stopped**).
- **17 Tauri commands** — validate-on-save (one shared registry: validator and runtime cannot disagree), enable/run blocked on errors only, fleet-delta collision reporting, fake-world dry-run, preset + station-set CRUD. **This is the whole operator surface today — there is no UI yet.**
- **Timezone-correct windows** — quiet hours were evaluated in UTC (7h off in Arizona); now local, offset read per-call for DST.

**Gates at handoff:** 176 leaf + 265 routines + 7 quit-gate tests green on the merged tree; `clippy --all-targets --locked -D warnings` clean; both lockfiles `--locked`-consistent; no "workflow" token anywhere.

**Review record:** ~20 real defects caught and fixed across 9 fix rounds. The load-bearing ones: a **dry-run that executed the REAL action catalog** with consent parking suppressed (would have keyed the transmitter); a **stale due-set firing disabled routines**; the **quit-resurrects-cancelled-run** bug; a scheduler **lost wakeup** (first-ever enabled routine could silently never fire); **P1 path traversal** in the definition store; a **Critical arbiter lease leak** on task abort; **lying leases** on cancelled WWV/listen captures.

## What REMAINS

- **Plan 4 — MCP tools + the one-cadence amendment package.** Plan doc written: `docs/superpowers/plans/2026-07-14-routines-04-mcp-amendments.md` (in the p4 worktree). bd `tuxlink-oiigb`, worktree `worktrees/bd-tuxlink-oiigb-routines-p4`, branch `bd-tuxlink-oiigb/routines-p4` (stacked on p2). Contents: MCP `RoutinesPort` (10 tools, **no consent-grant tool**) + **operator decision A (2026-07-13)**: one cadence per routine (spec §5/§14 amendment), `MULTIPLE_SCHEDULES` warning, re-author `deployment-poll` fixture as two routines, WWV step-timeout warning, and fix `missed_fires` over-counting for windowed/aligned schedules.
- **Plan 5 — the UI.** NOT written. Dashboard (ops table), the structured-lane canvas builder, run monitor, consent dialog, missed-fire/refusal surfaces. **This is the gate on a human-driven live test** and the surface the operator will judge the feature by. Contract notes it must honor: `EnableResult{blocked}` is `Ok`-with-a-flag (not `Err`); new event variants (`LibraryChanged`, `ScheduleRefused`, `ScheduleSkipped`, `AwaitingConsent`); the missed-count has no dismiss affordance yet.
- **Plan 6 — dockable pop-out surfaces** (Routines, Tac Map, APRS Chat). NOT written. Pure convenience; ~30 MiB/window measured.

## THE SEQUENCING QUESTION THE OPERATOR ASKED (answer it first)

They asked: *"What else is left to ship this in full so we can end-to-end test it live? Can we execute all of it now?"*

**The honest answer given:** they **cannot** operator-test it today — there is no UI. Two paths:
- **Operator-driven live test** → needs **plan 5**. Recommendation given: merge #1115, run plan 4 in parallel (independent), then **prioritize plan 5 and defer plan 6** — two plan cycles to a testable app instead of three.
- **On-air engine validation sooner** → offer a **minimal operator runbook** (a sample routine JSON + the exact `invoke()` calls) so they can key the G90/VARA rig right after #1115 merges, without any UI. **They did not answer this yet — ask.**

## State

- Working tree clean on all worktrees; everything committed and pushed.
- Worktrees live: `bd-tuxlink-ofw4s-routines-p2` (plan 2), `bd-tuxlink-oiigb-routines-p4` (plan 4, plan doc uncommitted — commit it), plus `bd-tuxlink-03d39-routines-spec` and `bd-tuxlink-pgidw-routines-p3` (both **merged — dispose per ADR 0009**).
- SDD ledgers with every carried item: `.superpowers/sdd/progress.md` in each worktree (gitignored — read them before re-deriving anything).
- Known carried debts: TOCTOU hardening in `routines_run`; `StationProfile` seam if a profile finding ever becomes an Error; listen RMS busy threshold is a **data-gated placeholder pending on-air calibration**; `#[traced_test]`'s EnvFilter can't match this codebase's `target:`-tagged warns (candidate pitfalls entry).
