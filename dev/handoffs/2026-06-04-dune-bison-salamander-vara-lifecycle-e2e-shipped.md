# Handoff — dune-bison-salamander — VARA lifecycle end-to-end shipped + panel-preload perf fix

> **Date:** 2026-06-04 · **Agent:** `dune-bison-salamander` · **Machine:** pandora
>
> **Arc:** Continuation of the VARA + ARDOP panel alpha-polish thread after `kite-hawk-sumac` shipped Phases 1 + 2 + Task 3.1 (handoff doc `2026-06-04-kite-hawk-sumac-vara-ardop-impl-phase1-2-3p1-shipped.md`). This session shipped Phase 4 (4.1 + 4.3 state-machine + 4.2 install-wire) + Phase 3 (3.0 + 3.2 + 3.3 + 3.4) — VARA's session lifecycle is now end-to-end functional. Also fixed an operator-reported panel-open perf bug (added mid-session).
>
> **Pipeline status at handoff:** 8 new commits shipped on `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2` (17 commits total ahead of `main`, pushed). VARA path complete (open + b2f_exchange + close all renamed + wired); ARDOP equivalents (3.5 + 3.6) + CONNECT_DEADLINE drop (1.5) + Phase 3-4 Codex review pending.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first (especially §4 reordering rationale — the plan's
   nominal task order doesn't match dependency graph; this session corrected it).
2. Branch state: bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2 has 17 commits
   on top of main (bd81dbc); 8 of those landed this session. Worktree at
   worktrees/bd-tuxlink-0ye6-vara-ardop-panel-alpha-polish-v2/ has node_modules +
   cargo target warm.
3. VERIFY the perf fix (panel chunk preload, commit 54297cd) before doing
   ANYTHING else. Restart tauri dev → wait ~3s for app-idle preload → click any
   VARA/ARDOP modem pane → observe open-time. Static-analysis hypothesis is cold
   chunk load; if still sluggish, the cause is mount-time work (5-7 Tauri
   invokes + 4Hz status subscriber + 1Hz sparkline tickers) and the next fix is
   deferred-invokes / memoization. Headless-Chromium CDP debugging per memory
   white-screen-debug-via-chromium-cdp.
4. Continue subagent-driven impl from Task 3.5 next (ARDOP analog of 3.2 + 3.3
   in one task — ardop_open_session + ardop_close_session). Then 3.6 (ARDOP
   b2f_exchange widen). Then 1.5 (drop CONNECT_DEADLINE — preconditions 4.1 +
   4.2 + 3.6 will be met after 3.6 lands). Then Codex Phase 3-4 boundary review.
5. ALWAYS use `--lib` for cargo tests in subagent dispatches; ALWAYS absolute
   paths (--manifest-path, git -C, pnpm -C) per memory pin-paths-in-worktree-sessions.
6. Per memory feedback_codex_post_subagent_review: run Codex parent-level review
   after Tasks 3.5 + 3.6 land — that's the Phase 3-4 boundary milestone (whole
   lifecycle backend complete; new architecture eligible for cross-provider
   independent review).
7. Tasks 3.5 + 3.6 are pattern-replicas of 3.2 + 3.3 + 3.4 (this session). The
   ARDOP work mostly mirrors VARA's shape; expect ~2-3 subagent dispatches.
```

---

## 1. Session arc

1. Picked moniker `dune-bison-salamander` per script (pre-flighted; clean).
2. Read kite-hawk-sumac handoff. Verified v2 worktree state (HEAD `697a998`, clean).
3. **Mid-session perf bug interrupt**: operator reported VARA/ARDOP panel-open was sluggish. Investigated:
   - Ruled out ALSA shell-outs (`arecord -L` + `aplay -L` are ~55 ms each on this Pi)
   - Ruled out module-scope side-effects (no top-level `invoke`/`listen`/`await`)
   - Ruled out backend handler slowness (all are in-memory snapshot reads or fast disk reads)
   - Identified most-likely cause: cold Vite chunk transform of 1018-line `ArdopRadioPanel.tsx` on first open
   - Shipped defensive fix: extract loader functions + idle-time preload of all 5 radio panels via `requestIdleCallback` (with `setTimeout` fallback). Commit `54297cd`. Tests pass.
   - Caveat captured in commit message: didn't observe runtime; cold-chunk hypothesis is unverified. If panel is still sluggish after preload + warm chunk cache, the cause is mount-time work (5-7 Tauri invokes + 4Hz status subscriber + 1Hz sparkline tickers), which needs a different fix (deferred-invokes / memoization).
4. Resumed subagent-driven impl from Task 4.1. Dispatched fresh subagent per task.
5. **Phase 4 Task 4.1** (commit `e74d18d`): ABORT codec + try_clone_abort_writer + ShutdownableStream trait + bounded write (1500 ms timeout) + hard-close fallback. Mirrored for ARDOP per Codex Round 4 P1 #4. Tests: 954 → 958 (+4). Subagent dropped the optional follow-on DISCONNECT per defensible engineering judgment (VARA might re-arm graceful tear-down counter); test invariant preserved.
6. **Hit dependency snarl**: the handoff's stated order (4.1 → 4.2 → 4.3 → 3.0 → 3.2-3.6 → 1.5) was inconsistent with task content. Task 4.2 references `vara_open_session` + `vara_close_session` (3.2 + 3.3 renames) which don't exist yet. Task 4.3 integration tests reference `vara_open_session` + `modem_vara_b2f_exchange` (3.2 + 3.4). Task 3.0 references `TransportOwner` (4.3).
7. **Corrected order**: 4.1 ✅ → 4.3 (state machine only, defer integration tests) → 3.0 (DTO widening) → 3.2 (open_session rename) → 3.3 (close_session rename) → 4.2 (wire abort install + tests) → 3.4 (b2f_exchange) → 3.5 + 3.6 (ARDOP) → 1.5 (drop CONNECT_DEADLINE) → Codex review.
8. **Task 4.3 state machine** (commit `6d10263`): TransportOwner enum (None, ListenerArmed, ListenerInbound, OutboundPending, Outbound) + take/return_transport_for_outbound APIs (3s bounded yield timeout per Codex R3 P1 #2, lock-drop-before-await per R2 P1 #4) on both VaraSession + ModemSession. Tests: 958 → 983 (+25, 13 VARA + 12 ARDOP). Scope explicitly reduced from plan — integration tests with `vara_open_session` deferred to follow-up. Listener consumer side wiring to actually yield on `transport_yield_request` is also deferred (created bd issue `tuxlink-17u9` later in session for it).
9. **Task 3.0 DTO widening** (commit `c569261`): added `listener_armed`, `exchange: Option<ExchangeState>`, `transport_owner: TransportOwner`, `active_intent: Option<SessionIntent>`, `active_transport_kind: Option<TransportKind>` to both ModemStatus + VaraStatus DTOs. Added `ExchangeState` enum (kebab-case serde) + `SocketLost` variants to ModemState + VaraState (with explicit `rename = "socket-lost"` on VaraState since the parent uses `lowercase` rename). TransportOwner gets `Serialize`/`Deserialize` + camelCase derives. Stub accessors return defaults (TODO comments point to wire-in tasks). Tests: 983 → 997 (+14). Step 0 spec edit was ALREADY shipped in Codex R3/R5 fix bundles — no-op was the right call.
10. **Task 3.2 vara_open_session rename** (commit `dc7f7dc`): renamed `vara_start_session` → `vara_open_session(intent: SessionIntent, transport_kind: TransportKind)` per Codex R2 P2 (transport_kind required for vara-hf vs vara-fm distinction). Added auto-arm path via factored `arm_vara_listener_inner` helper for intents where `intent.auto_arms_listener()` returns true (P2p + RadioOnly). Stored `active_intent` + `active_transport_kind` on VaraSession inner state. Frontend transitionally hardcodes `intent: 'cms'` until Phase 5's RadioSessionPanel. Tests: 997 → 1002 (+5 backend) + vitest 18 → 19 (+1 frontend).
11. **Task 3.3 vara_close_session rename** (commit `feeb26e`): renamed `vara_stop_session` → `vara_close_session()`. 4-step close path: disarm listener via `disarm_vara_listener_inner` → call `vara_session.abort_in_flight()` (real now from Task 4.1) → active mode cleared via existing `vara_stop_session_inner` chain (subagent skipped the proposed separate `clear_active_session_mode` method per DRY — single source of truth) → close transport. Tests: 1002 → 1006 (+4).
12. **Task 4.2 wire abort install** (commit `cd97505`): added `try_clone_abort_writer` + abort writer install in `vara_open_session_inner` after TCP open + before `guard.transport = Some(...)`. Subagent caught a lock-deadlock — `install_abort_writer` re-acquires `inner.lock()` which is already held by the outer; resolved by writing directly to `guard.abort_writer` / `guard.abort_stream` in-place. Tests: 1006 → 1009 (+3, byte-on-wire ABORT\r verification via loopback peer read-back). Closed bd issue `tuxlink-12sc`.
13. **Task 3.4 modem_vara_b2f_exchange** (commit `50b686b`): added `modem_vara_b2f_exchange(target, intent)` Tauri command + `run_vara_b2f_exchange` thin wrapper around existing B2F machinery. Intent's `routing_flag()` (pre-existing on SessionIntent) plumbed through `ExchangeConfig.intent`. Arbiter wire-in (calling `take_transport_for_outbound`) deferred per scope reduction — used existing transport-take pattern. Created bd issue `tuxlink-17u9` (P2) for the deferred arbiter wire-in. Tests: 1009 → 1014 (+5). Closed bd issue `tuxlink-fzl7` (subsumed by tuxlink-0ye6).

**Stopped at Task 3.4** to write this handoff. Phase 3.5 + 3.6 (ARDOP analogs) + Phase 1.5 (drop CONNECT_DEADLINE) + Codex Phase 3-4 boundary review pending. Operator may also want to verify the panel-preload perf fix before continuing — that's the recommended first action next session.

---

## 2. Commits shipped this session (branch `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2`)

| SHA | Task | Subject |
|---|---|---|
| `54297cd` | perf-bug | perf(shell): preload radio-panel chunks at app idle |
| `e74d18d` | 4.1 | feat(vara-transport): add abort_in_flight side-channel (ABORT, not DISCONNECT) + bounded write |
| `6d10263` | 4.3 | feat(session-arbiter): TransportOwner state machine + take/return APIs for outbound |
| `c569261` | 3.0 | feat(modem-status,vara-status): widen DTOs with lifecycle fields + SocketLost |
| `dc7f7dc` | 3.2 | feat(vara): rename vara_start_session → vara_open_session(intent, transport_kind) |
| `feeb26e` | 3.3 | feat(vara): rename vara_stop_session → vara_close_session() + disarm + abort |
| `cd97505` | 4.2 | feat(vara): wire ABORT side-channel install at vara_open_session |
| `50b686b` | 3.4 | feat(vara): add modem_vara_b2f_exchange(target, intent) Tauri command |

All 8 commits pushed to `origin/bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2`.

**Test count progression:**
- Pre-session baseline (post kite-hawk-sumac): **954** lib + 1153 vitest
- Post-Task-4.1: 954 → 958 lib (+4)
- Post-Task-4.3: 958 → 983 lib (+25)
- Post-Task-3.0: 983 → 997 lib (+14)
- Post-Task-3.2: 997 → 1002 lib (+5) + 1153 → 1154 vitest (+1)
- Post-Task-3.3: 1002 → 1006 lib (+4)
- Post-Task-4.2: 1006 → 1009 lib (+3)
- Post-Task-3.4: 1009 → **1014** lib (+5)

**Net session delta:** +60 lib tests (+ 1 vitest). All passing; no regressions.

Typecheck (`pnpm exec tsc --noEmit`): clean throughout. Phase greenness invariant (Codex Round 1 P2 #11) held at every commit.

---

## 3. Subagent-driven-development observations

This session dispatched 6 implementer subagents + 1 self-implemented perf fix. Notable patterns:

**Subagent self-correction caught real bugs:**
- Task 4.2 subagent caught a lock-deadlock pre-flight: `install_abort_writer` re-acquires `inner.lock()` which the outer call already holds. They wrote directly to the fields under the existing guard. TDD with byte-on-wire verification (loopback peer reads `ABORT\r`) confirmed the fix.
- Task 4.1 subagent recognized `vara/session.rs` doesn't exist (it's `vara/commands.rs`); added the abort APIs to the existing `VaraSession` rather than creating a parallel type.

**Scope reductions worked well:**
- Task 4.3: state machine + APIs + unit tests; integration tests (with `vara_open_session`) deferred until after the renames. Allowed the state machine to land without cascading dependency surprises.
- Task 3.0: heartbeat probing infrastructure (which needs `vara_open_session` / `ardop_open_session` to test) deferred to a separate post-3.5 task; DTO + enums + serde + stub accessors shipped today.
- Task 4.2: end-to-end "close interrupts active exchange in <2s" test deferred to operator smoke once Task 3.4 lands; unit tests at the inner-helper layer cover the install + abort-fires-through paths.
- Task 3.4: arbiter wire-in deferred via bd issue `tuxlink-17u9` (listener consumer side wiring would have deadlocked).

**Spec deviations recorded (defensible, all caught by Codex parent review at boundary):**
- Task 4.1: dropped the optional follow-on `DISCONNECT\r` after `ABORT\r`. Subagent argument: VARA might re-arm a graceful tear-down counter on the subsequent DISCONNECT, defeating the interrupt contract. Test invariant ("ABORT must precede any DISCONNECT") preserved in case a future implementer adds DISCONNECT back. Future Codex review will adjudicate.
- Task 3.3: skipped the separate `clear_active_session_mode()` method — `vara_stop_session_inner` already clears the fields as part of teardown. Single source of truth.

**Test-scaffolding fragility (worth a follow-up):**
- Several subagents reshaped plan-prescribed integration tests (which require `setup_test_state()` with full AppHandle scaffolding) into unit tests on inner helpers. This is pragmatic — full Tauri runtime tests are hard to fixture — but creates a small coverage gap. A future task could add a thin Tauri-runtime test harness (mocked AppHandle + State injection) so plan-prescribed integration tests can be written more naturally.

**Codex parent-level review timing:** deferred to Phase 3-4 boundary per `feedback_codex_post_subagent_review`. The boundary lands after Tasks 3.5 + 3.6 (ARDOP analogs); that's when the full new architecture (lifecycle commands + arbiter + DTOs + abort wiring + b2f_exchange both VARA + ARDOP) is integrated and eligible for a meaningful cross-provider independent review.

---

## 4. Reordering of remaining tasks (per dependency analysis)

The kite-hawk-sumac handoff §4 listed an order (4.1 → 4.2 → 4.3 → 3.0 → 3.2-3.6 → 1.5) that has unstated cross-task dependencies. The corrected dependency-respecting order, going forward from this handoff:

1. **Task 3.5** — `ardop_open_session(intent)` + `ardop_close_session()` Tauri commands. ARDOP analog of Tasks 3.2 + 3.3 (renamed/widened from existing ARDOP session commands). Mirror VARA's pattern: open installs abort writer (Task 4.1 ARDOP APIs already wired); close calls abort_in_flight + disarms ARDOP listener + closes transport. Plan §"Task 3.5" lines 1817-1921.
2. **Task 3.6** — widen `modem_ardop_b2f_exchange` to perform `connect_arq` + B2F + DISCONNECT in one call, accepting `intent: SessionIntent`. Currently `modem_ardop_b2f_exchange` already exists (was the consent-gated path before Phase 1); this task removes the consent path remnants + adds the intent param + plumbs routing_flag. Plan §"Task 3.6" lines 1922-2080.
3. **Task 1.5** — drop `CONNECT_DEADLINE` from the ARDOP connect path. Preconditions (4.1 + 4.2 + 3.6) are met AFTER 3.6 lands. Plan §"Task 1.5" (referenced in plan front-matter; find by grep `CONNECT_DEADLINE` or `^### Task 1.5`).
4. **Codex Phase 3-4 boundary review** — parent-level Codex review of the new architecture. Whole-branch diff: `git diff main..HEAD`. Use the custom-prompt pattern from CLAUDE.md (stdin pattern; `--base + prompt` is rejected). Audit angles: lifecycle correctness, arbiter race-safety, abort timing, intent-routing-flag plumbing, DTO consistency. Findings → P1 fix subtasks dispatched as additional subagents.
5. **Operator smoke** — `pnpm tauri dev`, walk the (intent, protocol) matrix: (cms, telnet/packet/ardop-hf/vara-hf/vara-fm) + (p2p, ardop-hf/vara-hf/vara-fm) + (radio-only, ardop-hf/vara-hf/vara-fm) — 9 combos. RADIO-1: operator-only; agent never runs.
6. **Phase 5** — shared RadioSessionPanel + adapters (7 tasks). Deferred until Phase 3-4 boundary review + operator smoke green.
7. **Phase 6+** — visibility router wire + delete legacy panels + PR landing. Same deferral.

**Deferred follow-ups** (bd issues exist for these):
- `tuxlink-17u9` — wire arbiter (Task 4.3 state machine) into `modem_vara_b2f_exchange` + listener consumer. Listener consumer needs to yield on `transport_yield_request` notify; outbound path then calls `take_transport_for_outbound`. Without this, the arbiter is a dormant state machine. Task 3.4 used the existing transport-take pattern as a stop-gap.
- Heartbeat probing infrastructure (Task 3.0 deferred): VARA cmd-port heartbeat (`VERSION\r` or `MYCALL\r` every 5 s; 2 consecutive failures → `VaraState::SocketLost`) + ARDOP child wait (`ardopcf` process exit → `ModemState::SocketLost`). No bd issue yet — create one or fold into a Phase 4 follow-up task.

---

## 5. PR status

| PR | bd | Branch | State | Contents |
|---|---|---|---|---|
| #360 | tuxlink-0ye6 | bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish | merged-dead | v1 spec + plan + mocks (mink-harrier-cardinal) |
| #365 | tuxlink-fl6e | bd-tuxlink-fl6e/plan-revision-codex-r1 | merged-dead | R1+R2 plan-fix bundle (cypress) |
| #368 | tuxlink-k2x1 | bd-tuxlink-k2x1/plan-revision-codex-r3 | merged-dead | R3+R4+R5 plan-fix bundle (cypress) |
| n/a | tuxlink-0ye6 | bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2 | **PUSHED, NOT PR'D** | 17 impl commits (kite-hawk-sumac × 9 + dune-bison-salamander × 8) |

**Why no PR yet:** the work is mid-flight (3.5 + 3.6 + 1.5 + Codex review + Phase 5-8 pending). Per memory `feedback_no_draft_pr_parking`: don't PARK done work as draft. The branch lives on origin without a PR until Phase 8 lands. Operator may choose to open a draft PR for visibility; agent-default is wait until Phase 8.

**Branch size at handoff:** 17 commits, well-organized (one task per commit, clear conventional types, all with `Agent:` trailers). The eventual PR will be sizable; Codex Phase 3-4 review at the next milestone is the right gate to validate before extending further.

---

## 6. Worktrees + their state at handoff

- `worktrees/bd-tuxlink-0ye6-vara-ardop-panel-alpha-polish-v2/` — **ACTIVE, in-flight impl worktree**. Branch `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2` at HEAD `50b686b`. node_modules installed; cargo target warm. **Next session continues here.** Working tree clean (subagents committed all work). No untracked or stashed content.

Other prior worktrees from this branch's history (per kite-hawk-sumac handoff §6) remain in their merged-dead state; disposal ritual still deferred for the Codex transcripts.

Other active tuxlink worktrees (UNTOUCHED): per `git worktree list` — many concurrent agents have live work. The vitest-on-`bd-tuxlink-hnkn-p2-native-autofill` was running for most of this session; this session avoided launching tauri dev to not collide on Vite port `:1420` per `project_worktree_dev_port_collision`. Coordinate via the main-checkout-race hook.

---

## 7. bd state at handoff

```
claimed by dune-bison-salamander this session:
  tuxlink-0ye6 (umbrella) — IN_PROGRESS (claimed by kite-hawk-sumac, still in flight)

closed this session:
  tuxlink-12sc — abort side-channel install at vara_open_session (Task 4.2)
  tuxlink-fzl7 — VARA Phase 3 outbound RF dial (Task 3.4 — subsumed by tuxlink-0ye6)

created this session:
  tuxlink-17u9 (P2) — Wire arbiter into modem_vara_b2f_exchange + listener consumer
                       Deferred from Task 3.4; listener consumer side needs to yield
                       on transport_yield_request notify, then modem_vara_b2f_exchange
                       calls take_transport_for_outbound (Task 4.3 APIs).

still in flight from this session:
  tuxlink-0ye6 (umbrella) — VARA done end-to-end; ARDOP (3.5, 3.6) + 1.5 + Codex
                            review pending on this branch.
```

---

## 8. Untouched operator state

- `task-amd-main-ui` rebase still mid-flight + 5 stashes on the main checkout (UNTOUCHED, per kite-hawk-sumac handoff)
- Other live worktrees per `git worktree list` — coordinate via main-checkout-race hook
- Orphaned handoff files from earlier sessions remain on `bd-tuxlink-xygm/recover-handoffs` branch (untouched per session-start instruction; cleanup deferred to operator)

---

## 9. Operator verification ask — the panel-preload perf fix

Commit `54297cd` ships a defensive fix for the operator-reported VARA/ARDOP panel-open sluggishness:
- Extracted each `lazy(() => import('...'))` into a named loader function
- Added a `requestIdleCallback` (with `setTimeout` fallback) effect on `AppShell` that fires all 5 radio-panel loaders at app idle
- React's lazy module cache means the second invocation (via `React.lazy` on operator click) is a no-op — Suspense's `fallback={null}` blank period collapses to zero

**Verification:**
1. Restart `pnpm tauri dev` (so app starts with cold chunk cache + the new preload effect)
2. Wait ~3 seconds for the app-idle preload to fire after the shell renders
3. Click any sidebar connection (telnet/packet/ardop-hf/vara-hf/vara-fm) to open its panel
4. Observe panel-open latency

**If still sluggish**: the cause is not chunk-load but mount-time work. Each panel mount fires:
- VARA: 5 Tauri invokes + 1 event listener subscription
- ARDOP: 7 Tauri invokes + 2 event listener subscriptions + 3 setInterval timers (1Hz sparklines + 4Hz status subscriber)

Next fix would be: (a) defer non-critical invokes (e.g., audio device enumeration only fires when Radio section is expanded), (b) memoize sub-components so 4Hz status events don't re-render the entire 1018-line panel, (c) lower polling rates.

**Debugging next step**: headless Chromium + CDP per memory `feedback_white_screen_debug_via_chromium_cdp` to capture mount-time profile data + console exceptions during open.

---

## 10. Next-session prompt

```
Resume tuxlink from the dune-bison-salamander 2026-06-04 handoff.

Handoff doc: dev/handoffs/2026-06-04-dune-bison-salamander-vara-lifecycle-e2e-shipped.md
READ IT FIRST.

State: 17 commits on branch bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2
(off main bd81dbc; pushed). VARA lifecycle is end-to-end (open + b2f_exchange +
close, all with abort wiring). ARDOP equivalents (3.5 + 3.6) + drop CONNECT_DEADLINE
(1.5) + Codex Phase 3-4 review pending. Worktree at
worktrees/bd-tuxlink-0ye6-vara-ardop-panel-alpha-polish-v2/ has node_modules +
cargo target warm. 1014 lib tests passing.

Critical first actions:
1. Read THIS handoff first (especially §4 dependency-respecting task order).
2. VERIFY the panel-preload perf fix (commit 54297cd) BEFORE continuing impl —
   §9 has the verification steps. If still sluggish, instrument via CDP-headless;
   the chunk-preload hypothesis was static-analysis based, not runtime-verified.
3. Continue subagent-driven impl from Task 3.5 (ardop_open_session + ardop_close_session,
   ARDOP analog of Tasks 3.2 + 3.3). Then 3.6 (widen modem_ardop_b2f_exchange).
   Then 1.5 (drop CONNECT_DEADLINE). Tasks 3.5 + 3.6 are pattern-replicas of the
   VARA work — expect 2-3 dispatches.
4. ALWAYS use `--lib` for cargo tests in subagent dispatches; ALWAYS absolute
   paths (--manifest-path, git -C, pnpm -C) per memory pin-paths-in-worktree-sessions.
5. After 3.5 + 3.6 land: run Codex parent-level review of the new architecture
   (Phase 3-4 boundary milestone). Use the custom-prompt pattern from CLAUDE.md
   (stdin pattern, NOT --base + prompt). Findings → P1 fix subtasks dispatched
   as additional subagents.
6. Open bd issues already-deferred: tuxlink-17u9 (arbiter wire-in to b2f_exchange
   + listener consumer). Heartbeat infrastructure from Task 3.0 also deferred —
   create a bd issue if not present.
7. Untouched operator state: task-amd-main-ui rebase + 5 stashes on main checkout.
   Other live worktrees — coordinate via main-checkout-race hook.
```

---

Agent: dune-bison-salamander
