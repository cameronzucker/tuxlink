# Handoff — Routines: plan 2 CI-green (merge it), plan 4 built but has 2 blockers; plan 5 mocks approved-pending

- **Agent:** sandbar-magnolia-falcon
- **Date:** 2026-07-14
- **Ended:** operator requested handoff — a classifier kept switching the model mid-session (Fable → Opus 4.8); handing off to restore model continuity, not because work wedged.

## READ THIS FIRST — where to resume

1. **PR #1115 (plan 2) is GREEN and marked ready — the operator merges it.** All 9 substantive checks passed on both arches (verify, build-linux, ECT .deb, deb-install) at head `09a465e7`. Nothing blocks it.
2. **PR #1117 (plan 4) is DRAFT and must NOT leave draft until 2 blockers are fixed** — see below. bd **`tuxlink-efcc8`** (P1 bug) carries both with exact file:line and the fix shape.
3. **Wire-walk is WAIVED for this epic** (operator, 2026-07-14: *"I can't provide wire walk gates for features I haven't seen or touched. Use your best judgement. We'll never ship it if we keep gating on that for this particular epic."*). Do not re-gate on it. Agent-drafted definition-of-done flows live at `dev/scratch/routines-p5-flows.md` (gitignored — read it before planning UI work).

## The two blockers on PR #1117 (bd tuxlink-efcc8)

Both were found **independently by two reviewers** (the final whole-branch reviewer and the Codex adversarial round), converging on the same lines. That agreement is why they're trusted.

### C1 — MCP transmit-consent bypass (SECURITY, P1)

`save_routine` ([src-tauri/src/routines/commands.rs:342](../../src-tauri/src/routines/commands.rs)) parses and persists caller-supplied `def_json` **verbatim, including `transmit_ack`**. The only readers of the ack — `transmit_ack_is_valid` (`validate/consent.rs:125`) and `ack_is_recorded` (`session.rs:725`) — accept **any non-empty `by`/`at`** strings. There is no out-of-band ack store, no callsign match, no signature.

Plan 4 exposes `routines_save` over MCP. So an agent can: `routines_save` an automatic routine carrying a forged (or `routines_get`-replayed) `transmit_ack` → the validator raises no `AUTO_TX_UNACKED` → `routines_enable` succeeds → **the routine keys the transmitter unattended**, with no desktop-UI act.

This contradicts three places that all promise it cannot happen: spec §13 (line 231 — "`routines_save` and `routines_enable` have no parameter that can supply it"), plan 4's Global Constraint #1, and `types.rs:40`'s own doc comment ("Recorded only by a UI act; MCP cannot supply it").

**Fix shape (code should match the operator-approved spec, not the reverse):** the save path strips/ignores any incoming `transmit_ack` and preserves the on-disk value; the ack is stamped only by the UI-only command the MCP surface does not expose. Add a regression test: an MCP save carrying a forged ack cannot enable an automatic routine.

### C2 — `--workspace` CI compile break

`src-tauri/tuxlink-mcp-testserver/src/main.rs:176–211` builds an `McpState` struct literal that is **missing the new required `routines` field** (`tuxlink-mcp-core/src/lib.rs:113`) → `error[E0063]`. The testserver is a workspace member and CI runs `--workspace`, so **CI will fail.**

My local gates missed this because they covered only `tuxlink-routines` / `tuxlink-mcp-core` / `tuxlink-security`. **The testserver IS Pi-buildable** (it depends only on mcp-core + security) — compile it locally after fixing. Plan 4 Task 2's stated "testserver coverage per mcp-core convention" was never done; do it.

**Lesson worth keeping:** leaf-crate-only local gates are not a proxy for the `--workspace` CI gate. When a shared struct gains a non-`Option` field, grep for every struct-literal construction of it.

### Also open (lower)

- **M1:** spec §14 says the format "does not accept" a second schedule; the parser *does* accept it (`triggers: Vec<Trigger>`) and only a **Warning** fires. Reword §14 to "the validator warns" (matching §5), or make it an Error if a hard reject was intended.
- **M2:** `dry_run` maps malformed client `args_json` to `PortError::Internal` (implies a server bug); `run` maps the same class to `Refused`. Align both to an invalid-input code.
- **Codex P3 (untriaged — verify first):** `WWV_MIN_TIMEOUT_S = 3900` assumes only the :18 WWV window, but the shipped `next_capture` (`src-tauri/src/routines/actions/data.rs`) picks the **nearest of :18 (WWV) and :45 (WWVH)**. If so, the floor over-warns and `STEP_TIMEOUT_LIKELY_INSUFFICIENT` fires on a perfectly adequate 45–50 min timeout — the validator disagreeing with the action it validates. Read `data.rs`, then re-derive the constant.
- **Triaged to record, no fix:** `missed_fires_windowed`'s 100k lower-bound cap (no consumer harmed; it's a plan-5 *display* note — render large counts as "100k+"); hoisting `(self.utc_offset)()` out of the reconcile loop (≤1 schedule per routine now, so it's ~1 call).

## What is DONE this session

**Plan 4 built, committed, pushed** — branch `bd-tuxlink-oiigb/routines-p4` @ `5d2c7bc6`, worktree `worktrees/bd-tuxlink-oiigb-routines-p4`, bd `tuxlink-oiigb`, **PR #1117 (draft, stacked on `bd-tuxlink-ofw4s/routines-p2` — retarget to main after #1115 merges)**:

- **Task 1 — one-cadence amendment package** (`c9eaba77`): spec §5/§14 amended (triggers = manual + at most one schedule; multi-cadence = multiple routines via call/fleet); `MULTIPLE_SCHEDULES` warning (new `validate/triggers.rs`); `STEP_TIMEOUT_LIKELY_INSUFFICIENT` (see Codex P3 above); `missed_fires_windowed` (window/align-aware; old signature untouched); corpus re-authored (`deployment-connect-cycle.json` + `wx-post-and-catalog.json`, a SCHEDULE_COLLISION-free fleet-coexistence pair); fixtures + manifest + completeness gate. Review: **APPROVED, zero C/I.**
- **Task 1b — plan gap I caught** (`3e7cd4e7`): `missed_fires_windowed` had **no monolith caller** — the phantom-missed-fire fix would have shipped inert. Reconciler re-pointed to it via the existing `self.utc_offset` seam; two paused-clock reconciler tests.
- **Task 2 — MCP RoutinesPort** (`7721cab5` + fix `5d2c7bc6`): spec §13's exact 10 tools; no consent-grant tool (asserted against the real router tool list); one-validator; verbatim refusals; fake-world `dry_run` with a canary test. Fix round 1 closed a review finding: `routines_journal_get` returned verbatim wire content **without tainting** — now taints via new `TaintReason::RoutinesJournal`. Review: **APPROVED after the fix.**
- **Adjudicated non-goal (recorded, not dropped):** no MCP tool exposes ScheduleStatus/missed-fire health — spec §13's 10-tool list is authoritative; schedule health is an operator surface (plan 5 dashboard).

**Gates run locally (leaf crates only — see C2):** tuxlink-routines 190/190 · tuxlink-mcp-core 103/103 · tuxlink-security 28/28; clippy `--all-targets -D warnings` clean on all three; monolith `cargo metadata --locked` OK; banned-token scan clean. Monolith + testserver compile is CI's job.

## Plan 5 (UI) — mocks rendered, plan draft INCOMPLETE

- **Five full-fidelity mocks exist, rendered through real WebKitGTK 4.1** at 1280×800, in the app's real design tokens, at `/home/administrator/Code/tuxlink/dev/scratch/routines-ui-mocks/` (`dashboard.png`, `designer-canvas.png`, `run-monitor.png`, `consent-dialog.png`, `designer-settings.png`; `REPORT.md` documents the decisions). **The operator has not yet given feedback on them.** Two design calls awaiting his eye: (1) step insertion = `＋` insert points on canvas edges + a right-rail action palette with RIG/TX/NET capability flags; (2) "consent cannot hide" = amber count badge on the Routines menubar item + a status-bar line naming the parked run.
- Known mock nit that must NOT carry into code: the palette shows drifted action names (`net.spacewx_swpc`) — the real palette renders from the action registry (`data.*`).
- **The plan-5 implementation-plan draft never completed** — the drafting agent was still running at handoff; `dev/scratch/routines-p5-ui-plan-draft.md` **does not exist**. Re-dispatch it. Its inputs: spec §12, `dev/scratch/routines-p5-flows.md` (7 definition-of-done flows + the all-18-commands-reachable coverage invariant + contract addenda), the mocks, and the p2 backend surface.

## State

- **Worktrees live:** `bd-tuxlink-ofw4s-routines-p2` (plan 2, PR #1115 green/ready — dispose after merge), `bd-tuxlink-oiigb-routines-p4` (plan 4, PR #1117 draft, **2 blockers**), plus `bd-tuxlink-03d39-routines-spec` and `bd-tuxlink-pgidw-routines-p3` (**both merged — dispose per ADR 0009**).
- **Working tree clean on p4**; everything committed and pushed.
- **Gitignored-but-load-bearing content in the p4 worktree:** `.superpowers/sdd/progress.md` (the full ledger — every carried item, read it before re-deriving anything), `.superpowers/sdd/*-report.md` + `*-brief.md` + review packages, and `dev/adversarial/2026-07-14-routines-p4-codex.md` (the 15,751-line Codex transcript; findings summarized above).
- **bd:** `tuxlink-oiigb` in_progress (plan 4); **`tuxlink-efcc8` open (P1)** — the two blockers.
- **Plans 5 and 6 remain unwritten.** Plan 6 = dockable pop-out surfaces (Routines, Tac Map, APRS Chat; ~30 MiB/window measured) — pure convenience, sequenced after 5.

## What the next session should do, in order

1. Merge #1115 if the operator hasn't (it's his call; it's green and ready).
2. Fix **`tuxlink-efcc8`** on `bd-tuxlink-oiigb/routines-p4`: C1 (strip `transmit_ack` on the save path + regression test), C2 (testserver `McpState` literal + **compile the testserver locally**), then M1/M2 and the Codex P3 verification. Re-run the leaf gates **plus the testserver**, retarget #1117 to main, take it out of draft.
3. Re-dispatch the plan-5 implementation plan (the draft was lost), then execute it.
