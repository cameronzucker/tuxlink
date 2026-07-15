# Handoff — Routines UI WebKitGTK smoke SHIPPED (PR #1119): 4 render fixes + empty state + quit-gate flake closed

- **Agent:** ridge-magpie-cove
- **Date:** 2026-07-14 (ran into 07-15 UTC)
- **Ended:** natural completion — bd `tuxlink-3awm9` executed end-to-end and merged.

## READ THIS FIRST — where things stand

1. **PR #1119 is MERGED to main** (merge commit `a17117da`, head `013e78d6`, CI green both arches by SHA). This was the fen-delta-kite handoff's next-session item 2: the first render of the plan-5 Routines UI outside vitest/jsdom, via `dev/render-harness` extended with a `?view=routines` fixture family (11 snapshots: dashboard populated/empty/light/1024px, designer canvas ×2, fresh draft, runs, settings ×2, consent modal with two parks). bd `tuxlink-3awm9` CLOSED.
2. **The UI held up well in the production engine.** Canvas (branch arms, retry/delay, unplaced-step surfacing, delay-anchor rail), consent modal (topmost, "1 of N" pip, ticking timer, correct 2-outcome footer), runs Gantt, settings, light theme — all correct on first render.
3. **Five defects found and fixed, none reachable by jsdom:** dashboard trigger cell hard-clipped the ` · window HH:MM-HH:MM` tail (nowrap + overflow:hidden fixed table); run/stop controls flex-crushed at 1024px (5% column → fixed 72px); `shortRunId` head-slice made every run label identical (backend ids share the `run-<unixsecs>` prefix for ~11 days — now tail-sliced `…6705-0007`); settings window input clipped its own value (WebKitGTK computes `ch` from the absent JetBrains Mono but renders the wider fontconfig fallback — probed 86px vs 98px scrollWidth; now 15ch); no dashboard empty state (added, MessageList idiom, gated on `useRoutines().loaded` after Codex's P2 caught the in-flight-refresh false-"empty").
4. **The quit-gate flake recurred and is CLOSED, with a corrected diagnosis** (bd `tuxlink-t53o2`, also closed). CI run 29393978852 (amd64) hit `confirm_drain_timeout_still_exits_and_warns` with `live_at_exit == 0`. It is NOT "apply the channel-park per ef10bbee" — ef10bbee's park was already in and working. The residual race is at the startup end: between `start_routine` returning and the wedged action's first poll, `cancel_all_live_runs` can land first and the step `tokio::select!` (executor.rs:276) takes the already-fired cancel branch without ever polling the action — the run cancels cleanly and the wedge never wedged. Fix at `013e78d6`: `WedgedBlockingAction` fires an `entered` oneshot inside the same synchronous poll that parks; the test awaits it before requesting quit. Product behavior was correct on every occurrence.
5. **ConsentGate.tsx contained a committed literal NUL byte** (the Codex-P1 pair-key separator from `5d8d15c4`) that made grep and GitHub treat the file as binary — replaced with the six-char escape, byte-identical string at runtime. If a tool ever goes silent on a source file, check for this class.

## Process notes

- Solo inline session (no SDD); one Codex adversarial round on the diff produced exactly 1 finding (the P2 empty-state race), fixed and test-pinned. Transcript + all 11 smoke PNGs archived at `.claude/worktree-archives/bd-tuxlink-3awm9-routines-ui-webkit-smoke-artifacts-20260715T070950Z.tar.gz` (local-only).
- Harness fixture realism was load-bearing twice: `run_started.snapshot` must carry the def (the Gantt derives lanes from `snapshot.tracks`; an empty snapshot renders a void) and run ids must use the real `run-<unixsecs>-<NNNN>` shape (short synthetic ids masked the shortRunId collision). Both are documented in the harness commit.

## State

- **Worktree `bd-tuxlink-3awm9-routines-ui-webkit-smoke` disposed per ADR 0009** (inventory → targeted artifact archive → rm → prune). No stashes created; the repo-wide stash stack (7 pre-existing entries) untouched.
- **bd:** closed `tuxlink-3awm9`, `tuxlink-t53o2`. Created `tuxlink-r6d63` (P3): ribbon Connect button renders dark-red in the Daylight theme — pre-existing shared chrome, spotted in the light-theme smoke PNG, repro via `harness.html?view=ribbon&theme=light`.
- **Main checkout** (`bd-tuxlink-ant8s/ardop-connect-fixes` + staged bd jsonl + README edit + untracked scratch): operator state, untouched all session.
- Remote branch auto-deleted at merge. ~100 historical worktrees under `worktrees/` remain (pre-existing backlog, untouched).

## NEW after the smoke shipped — operator-commissioned A/B experiment (bd `tuxlink-c5ckf`)

Late in this session the operator commissioned a baseline-feasibility experiment for his work AI-architect concept (rationed Claude Teams seat → Fable orchestrator + LOCAL worker tier on DGX-Spark-class hardware). **The full protocol is in bd `tuxlink-c5ckf` — read it before starting.** Summary: vehicle is `tuxlink-xvd1i`; write plan + briefs ONCE and rubric BEFORE either arm runs; arm A = standard SDD (Sonnet implementers + Opus reviewers, full agent inventory documented, MERGES); arm B = same briefs via Codex CLI → Spark endpoint (Qwen3 Coder Next primary, Qwen 3.5 122b Q4, gpt-oss-120b comparator; NEVER merges); symmetric blind Codex eval; report to dev/research/ shaped for a work proposal.

**Blocking fact for the next session: the Spark endpoint URL/hostname is not discoverable from this Pi** (ssh config / /etc/hosts / mDNS all empty) — it IS up with models loaded per the operator; ask him for it first.

## What the next session should consider, in order

1. **The A/B experiment (`tuxlink-c5ckf` + `tuxlink-xvd1i`)** — operator-commissioned, protocol above. First action: ask for the Spark endpoint URL. Note the experiment's arm A completes the P2 journal-enrichment backlog item as real merged work.
2. **Routines plan 6 (dockable pop-out) has NO plan** — full build-robust-features (brainstorm → adversarial → plan) BEFORE any code. Do not skip the brainstorm. Needs operator participation, so sequence around his availability.
3. `tuxlink-r6d63` (light-theme accent chrome) is a quick independent polish item if a session needs a warm-up.
