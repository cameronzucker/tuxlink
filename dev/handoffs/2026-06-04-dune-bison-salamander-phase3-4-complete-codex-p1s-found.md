# Handoff — dune-bison-salamander — Phase 3 + 4 impl complete; Codex review found 5 P1 + 4 P2

> **Date:** 2026-06-04 · **Agent:** `dune-bison-salamander` · **Machine:** pandora
>
> **Arc continuation** from this session's earlier mid-session handoff (`2026-06-04-dune-bison-salamander-vara-lifecycle-e2e-shipped.md`). After the intermediate stopping point at Task 3.4, the operator asked me to push through to finish. Tasks 3.5 (ardop_open/close_session), 3.6 (widened modem_ardop_b2f_exchange), 1.5 (drop CONNECT_DEADLINE + Option<Duration> refactor) shipped. Codex Phase 3-4 boundary review ran and returned 5 P1 + 4 P2 findings — **branch is NOT mergeable as-is**.
>
> **Pipeline status:** 12 commits this session total (3 perf+impl from before mid-handoff + 3 new impl tasks + 1 final handoff). 20 commits ahead of main. Branch pushed but not PR'd. Codex P1 + P2 findings are the next session's first priority before any PR consideration.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first, especially §3 Codex findings.
2. The 5 P1 findings are pre-merge blockers. They cluster into:
   - Close-vs-armed-consumer races (P1#1, #4, #5): close path doesn't wait
     for listener consumer to drain; consumer reinstalls transport after
     close. ARDOP + VARA both affected.
   - Lifecycle violation in VARA b2f (P1#2): vara_send_receive currently
     drops transport + calls vara_stop_session_inner; spec says return
     transport to keep session Open.
   - Routing-flag drain gap (P1#3): Outbox drained without intent filter;
     CMS/R-pool/unflagged messages can leak to wrong RF session.
3. The 4 P2s should ship in the same fix sweep (transport_kind plumbing
   gaps, stale state on stop, listener_armed/exchange stubs).
4. Codex transcript: dev/adversarial/2026-06-04-phase3-4-boundary-codex.md
   (gitignored; 13056 lines; preserve via tar to .claude/worktree-archives/
   if disposing this worktree).
5. ALSO: operator has not verified the panel-preload perf fix (commit
   54297cd). §9 of the mid-session handoff has verification steps.
6. Worktree: bd-tuxlink-0ye6-vara-ardop-panel-alpha-polish-v2 at HEAD
   <FINAL_SHA>; node_modules + cargo target warm. 1033 lib + ~1154 vitest
   passing.
7. Recommended next-session ordering:
   a. Verify panel-preload perf fix
   b. Fix P1#1 + P1#4 + P1#5 together (close-vs-consumer race; shared
      mechanism)
   c. Fix P1#2 (VARA b2f lifecycle)
   d. Fix P1#3 (intent-filtered drain — needs MessageMeta routing_flag
      storage; may be its own task)
   e. Sweep the 4 P2s
   f. Re-run Codex to confirm closure
   g. Then Phase 5 (RadioSessionPanel) or PR landing
```

---

## 1. Session arc (continuation from mid-session handoff)

**Mid-session handoff (`2026-06-04-dune-bison-salamander-vara-lifecycle-e2e-shipped.md`, commit `547ebb5`)** covered: perf fix + Tasks 4.1, 4.3, 3.0, 3.2, 3.3, 4.2, 3.4. After that handoff, operator said "push through to finish" and the session continued:

1. **Task 3.5 ardop_open/close_session** (commit `d7c5a1c`): ARDOP analog of Tasks 3.2 + 3.3. `ardop_open_session(intent, transport_kind)` spawns ardopcf + binds cmd socket + installs abort writer + stores active session mode + auto-arms listener for P2p/RadioOnly. `ardop_close_session()` disarms + aborts + clears active mode + tears down transport. Tests: 1014 → 1026 (+12). Subagent caught that ARDOP's `ModemSession` doesn't have the lock-deadlock issue VARA's had — clean.
2. **Task 3.6 widen modem_ardop_b2f_exchange** (commit `a279c2d`): added `intent: SessionIntent` + `transport_kind: TransportKind` params. Calls `connect_arq` BEFORE B2F per Codex R1 P1 #1. Subagent verified existing `disconnect` is already link-only (no rename needed; ardopcf process teardown happens at `ArdopTransport`'s Drop). Added `CONNECT_DEADLINE_EFFECTIVE_INFINITY = 1 day` as a stop-gap (trait signature required Duration). Tests: 1026 → 1030 (+4).
3. **Task 1.5 drop CONNECT_DEADLINE** (commit `0fe637c`): refactored `ModemTransport::connect_arq` from `Duration` → `Option<Duration>`. Added `CmdSocket::recv_event_blocking()` (wraps `Receiver::recv()`) per Codex R2 P1 #2 (Duration::MAX overflows recv_timeout). Deleted `CONNECT_DEADLINE` + `CONNECT_DEADLINE_EFFECTIVE_INFINITY` constants. `modem_ardop_connect` (legacy Start-button path; Phase 6 deletion target) inlines `Some(Duration::from_secs(120))`. `modem_ardop_b2f_exchange` passes `None`. Sentinel test catches future regression (concat!-split sentinel pattern). Tests: 1030 → 1033 (+3).
4. **Mid-session RAM crash averted**: operator interrupted ~30 min in with "we are going to crash. No admin, just stopping what we can and freeing RAM." Pi 5 had 90 MiB free + 20 GiB swap used; 3 vitest processes (~7.1 GiB) from a parallel agent's `bd-tuxlink-hnkn-p2-native-autofill` worktree were the heaviest single class. Stopped my pending Codex run; operator killed the vitest zombies; RAM reclaimed. Resumed.
5. **Codex Phase 3-4 boundary review** (commit n/a — transcript at `dev/adversarial/2026-06-04-phase3-4-boundary-codex.md`, gitignored). Ran the 8-attack-angle adversarial review per CLAUDE.md custom-prompt pattern. **Returned 5 P1 + 4 P2 findings** (see §3).
6. **Final handoff** (this commit).

---

## 2. Commits shipped this session (branch `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2`)

Continuing the table from the mid-session handoff:

| SHA | Task | Subject |
|---|---|---|
| `54297cd` | perf | perf(shell): preload radio-panel chunks at app idle |
| `e74d18d` | 4.1 | feat(vara-transport): add abort_in_flight side-channel (ABORT, not DISCONNECT) + bounded write |
| `6d10263` | 4.3 | feat(session-arbiter): TransportOwner state machine + take/return APIs for outbound |
| `c569261` | 3.0 | feat(modem-status,vara-status): widen DTOs with lifecycle fields + SocketLost |
| `dc7f7dc` | 3.2 | feat(vara): rename vara_start_session → vara_open_session(intent, transport_kind) |
| `feeb26e` | 3.3 | feat(vara): rename vara_stop_session → vara_close_session() + disarm + abort |
| `cd97505` | 4.2 | feat(vara): wire ABORT side-channel install at vara_open_session |
| `50b686b` | 3.4 | feat(vara): add modem_vara_b2f_exchange(target, intent) Tauri command |
| `547ebb5` | handoff | docs(handoff): mid-session — VARA lifecycle E2E + panel-preload perf fix |
| `d7c5a1c` | 3.5 | feat(ardop): add ardop_open_session(intent, transport_kind) + ardop_close_session() |
| `a279c2d` | 3.6 | feat(ardop): connect_arq inside modem_ardop_b2f_exchange + intent + transport_kind |
| `0fe637c` | 1.5 | refactor(modem-ardop): drop CONNECT_DEADLINE + add Option<Duration> connect_arq |

All 12 commits pushed.

**Test count progression (this session):**
- Pre-session baseline (post kite-hawk-sumac): **954** lib + 1153 vitest
- Post-Task-1.5 (final): **1033** lib (+79) + ~1154 vitest (+1)

Typecheck (`pnpm exec tsc --noEmit`): clean throughout. Phase greenness invariant held at every commit (Codex Round 1 P2 #11).

---

## 3. Codex Phase 3-4 boundary review — 5 P1 + 4 P2 findings

**Transcript:** `dev/adversarial/2026-06-04-phase3-4-boundary-codex.md` (13,056 lines; `.gitignore`d per CLAUDE.md).

**Verdict from Codex's own summary:** *"The implementation has multiple blocking lifecycle and routing issues: Close can be undone by in-flight ARDOP/VARA work, VARA B2F violates the open-session contract, and session intent is not enforced when selecting outbound mail. These need correction before the Phase 3/4 work can be considered faithful to the reviewed plan."*

### P1 — pre-merge blockers (5)

**P1#1 — Do not re-install ARDOP transport after Close**
- **Location:** `src-tauri/src/modem_commands.rs:1000`
- **Mechanism:** If Close Session is clicked while `modem_ardop_b2f_exchange` owns the transport after `take_transport()`, `ardop_close_session_inner` aborts and `reset_to_stopped()` clears the session, but the unconditional `install_transport` reinstall runs when the aborted exchange unwinds. Leaves a live transport in a session the operator just closed.
- **Fix:** Guard the return with a close/generation state, or drop/shutdown when close has won the race.

**P1#2 — Keep VARA B2F inside the open session**
- **Location:** `src-tauri/src/winlink/modem/vara/commands.rs:1355-1357`
- **Mechanism:** A successful VARA Send/Receive currently always drops the transport and calls `vara_stop_session_inner`, which clears `active_intent`/`active_transport_kind` and returns the panel to Closed. Spec §2 says "outbound dial is within-session" — B2F should return to open/idle or listener-armed, not stop the session.
- **Fix:** Return the transport through the arbiter/session instead of stopping. Mirror Task 3.6's ARDOP pattern (which keeps the session Open).

**P1#3 — Filter outbound mail by session intent**
- **Location:** `src-tauri/src/winlink_backend.rs:2165`
- **Mechanism:** B2F engine drains every Outbox item regardless of `intent`; `ExchangeConfig.intent` is not read by the engine. `routing_flag()` is only covered by tests/comments. In a P2P or radio-only exchange, CMS/R-pool/unflagged messages can be offered over the wrong RF session, contrary to spec §3 capability matrix.
- **Fix:** Persist/read routing flags on `MessageMeta` and filter before building proposals. (Or block non-CMS exchange until flags exist as a temporary safety gate.)
- **Note:** Task 3.4 subagent's report independently flagged this — the routing_flag plumbs through `ExchangeConfig.intent` but isn't actually filtering. Now confirmed by Codex.

**P1#4 — Prevent VARA listener from reopening after Close**
- **Location:** `src-tauri/src/winlink/modem/vara/commands.rs:1167-1169`
- **Mechanism:** When a listener is armed, `vara_close_session_inner`'s disarm only sets the consumer's shutdown flag, then immediately proceeds to abort/stop. The consumer still owns the transport; its normal drain path later calls `return_transport`, restoring `VaraState::Open` after Close has returned.
- **Fix:** In the armed-listener case, wait for the consumer to drain, or mark the session "closing" so the consumer's return path drops the transport instead of reinstalling it.

**P1#5 — Prevent ARDOP listener from reinstalling after Close**
- **Location:** `src-tauri/src/modem_commands.rs:638`
- **Mechanism:** Same race as P1#4, ARDOP side. `ardop_set_listen_inner(false)` only signals; `modem_ardop_disconnect_inner` resets the session while the consumer still owns the transport. Consumer's shutdown path unconditionally `install_transport`s it back. Close Session can leave ardopcf running with a transport reattached after status reads Stopped.
- **Fix:** Coordinate the drain or make the consumer drop when close wins.

### P2 — should-fix-in-same-sweep (4)

**P2#1 — Clear VARA abort + owner state on stop**
- **Location:** `src-tauri/src/winlink/modem/vara/commands.rs:1202-1210`
- **Mechanism:** `vara_stop_session_inner` drops only `transport` and active mode; leaves `abort_writer`/`abort_stream` and `transport_owner` untouched. Stale ABORT writer can survive into next session if cloning fails; closed status overlay can incorrectly show `listenerArmed` after a `take_transport()` path.
- **Fix:** Clear `abort_writer`, `abort_stream`, `transport_owner` (reset to `None`) alongside the transport.

**P2#2 — Accept full VARA B2F intent/kind payload**
- **Location:** `src-tauri/src/winlink/modem/vara/commands.rs:1229-1233`
- **Mechanism:** `parse_vara_b2f_intent` only accepts `cms`/`p2p` — no `radio-only`. Command signature has no `transport_kind`. Shared panel sends `SessionIntent + transportKind` for every B2F command; spec capability matrix includes radio-only outbound. VARA radio-only invokes will fail; VARA-FM dials cannot be distinguished.
- **Fix:** Take `SessionIntent` (full enum) + `TransportKind` like ARDOP's `modem_ardop_b2f_exchange`; validate `VaraHf`/`VaraFm`.

**P2#3 — Pass VARA HF/FM kind into listener arming**
- **Location:** `src-tauri/src/winlink/modem/vara/commands.rs:919-924`
- **Mechanism:** `vara_open_session` receives `transport_kind`, but the auto-arm call drops it. `arm_vara_listener_inner` records/rejects as `TransportKind::VaraHf` unconditionally. Opening VARA-FM with P2P/radio-only writes HF arm/reject forensics → debugging confusion + wrong arm record.
- **Fix:** Thread `transport_kind` through `arm_vara_listener_inner`; use it in the arms/reject records.

**P2#4 — Populate lifecycle DTO fields from real state**
- **Location:** `src-tauri/src/winlink/modem/vara/commands.rs:316-324`
- **Mechanism:** `VaraStatus.listener_armed` + `exchange` accessors still return `false`/`None` stubs (Task 3.0 stub accessors were supposed to be wired by Phase 3.2-3.6 but the wiring was missed for these two fields). After auto-arm or during inbound/outbound exchange, backend reports idle/no-listener; shared panel can't render or gate lifecycle correctly.
- **Fix:** Wire `listener_armed` to actual `VaraListenState`; wire `exchange` to arbiter's `transport_owner` state (or a dedicated `exchange_state` field if arbiter doesn't carry it).

### No P3s reported

Codex didn't flag any P3-level issues — implementation is solid above the P1/P2 surface (architecture, naming, test coverage, serde wire format, A1/A2/A3/A6 audit angles all came back clean modulo the listed P1s/P2s).

---

## 4. Recommended next-session task ordering

Pre-merge fix sweep:

1. **Verify panel-preload perf fix** (commit 54297cd) per §9 of mid-session handoff. If still sluggish, instrument via CDP-headless before continuing.
2. **Fix P1#1 + P1#4 + P1#5 together** (close-vs-consumer races, shared mechanism). Probably needs a "session generation counter" or "closing flag" on both `ModemSession` + `VaraSession` so the consumer's drain path can detect "close already won" and drop instead of reinstall. Single ADR-worthy design decision; one subagent dispatch.
3. **Fix P1#2** (VARA b2f lifecycle): refactor `modem_vara_b2f_exchange` to mirror ARDOP's Task 3.6 pattern (`take_transport` → exchange → `install_transport` to keep session Open). Single dispatch.
4. **Fix P1#3** (intent-filtered drain): may be its own larger task. Needs `MessageMeta.routing_flag: Option<RoutingFlag>` storage + write-path tagging (compose / inbound dispatch) + read-path filter in the B2F drain. Could also ship as a temporary safety gate ("non-CMS exchange rejects until routing flags exist") if the full refactor is multi-session.
5. **Sweep P2#1 + P2#2 + P2#3 + P2#4** together (related VARA-side polish). One dispatch.
6. **Re-run Codex Phase 3-4 boundary review** to confirm P1s + P2s closed.
7. Then proceed to Phase 5 (RadioSessionPanel) or PR landing.

**bd issues to file** before next session (operator decides scope):
- `<new>` — P1#1+#4+#5: close-vs-consumer race in both ModemSession + VaraSession lifecycle
- `<new>` — P1#2: VARA b2f returns session to Closed instead of Open (spec §2 violation)
- `<new>` — P1#3: MessageMeta routing_flag storage + intent-filtered Outbox drain
- `<new>` — P2 sweep: VARA stop-state cleanup + b2f payload widening + listener arming HF/FM + DTO wire-in

---

## 5. PR status

| PR | bd | Branch | State | Contents |
|---|---|---|---|---|
| #360 | tuxlink-0ye6 | bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish | merged-dead | v1 spec + plan + mocks |
| #365 | tuxlink-fl6e | bd-tuxlink-fl6e/plan-revision-codex-r1 | merged-dead | R1+R2 plan-fix bundle |
| #368 | tuxlink-k2x1 | bd-tuxlink-k2x1/plan-revision-codex-r3 | merged-dead | R3+R4+R5 plan-fix bundle |
| n/a | tuxlink-0ye6 | bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2 | **PUSHED, NOT PR'D** | 20 impl commits (kite-hawk-sumac × 9 + dune-bison-salamander × 11) |

**Why no PR yet:** Codex found 5 P1 blockers. Per memory `feedback_no_draft_pr_parking`: don't park work as draft, but don't ship pre-blocker work either. Branch stays on origin without a PR until P1s addressed.

---

## 6. Worktrees + state at handoff

- `worktrees/bd-tuxlink-0ye6-vara-ardop-panel-alpha-polish-v2/` — **ACTIVE, in-flight impl worktree**. Branch `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2` at HEAD `0fe637c` (before this handoff commit). node_modules installed; cargo target warm. **Untracked + gitignored content of note:** `dev/adversarial/2026-06-04-phase3-4-boundary-codex.md` (13056 lines, `.gitignore`d, the Codex review transcript — preserve via tar to `.claude/worktree-archives/` if disposing).
- Other worktrees per `git worktree list` — UNTOUCHED.

---

## 7. bd state at handoff

```
in-flight from this session:
  tuxlink-0ye6 (umbrella) — Phase 3 + 4 impl complete BUT Codex review
                            found 5 P1 + 4 P2 blockers. Pre-merge fix
                            sweep is the next work unit.

closed earlier this session:
  tuxlink-12sc — abort install at vara_open_session (Task 4.2)
  tuxlink-fzl7 — VARA Phase 3 outbound RF dial (Task 3.4)

created earlier this session:
  tuxlink-17u9 (P2) — Wire arbiter into modem_*_b2f_exchange + listener
                       consumer (deferred from Task 3.4; may interact with
                       P1#4 + P1#5 close-vs-consumer race fix)

new bd issues to file for Codex findings: see §4 recommended ordering.
```

---

## 8. RAM-pressure incident (mid-session)

~30 minutes before session end, Pi 5 hit critical RAM pressure: 90 MiB free + 20 GiB swap used. Operator interrupted before I could run Codex. Three vitest processes (~7.1 GiB combined RSS, ~22 min runtime) from parallel agent `bd-tuxlink-hnkn-p2-native-autofill` were the heaviest single class. Operator killed the vitest zombies; RAM reclaimed; resumed.

**Watch for in future**: Pi 5 is shared; when multiple worktrees run vitest concurrently, RAM headroom drops fast. Stagger heavy test runs across worktrees, or coordinate via the session-leases mechanism. No code change needed; operator awareness suffices.

---

## 9. Operator verification ask (from mid-session handoff, still open)

Commit `54297cd` ships the panel-preload perf fix for the operator-reported VARA/ARDOP panel-open sluggishness. **Still not runtime-verified.** Per mid-session handoff §9: restart `pnpm tauri dev` → wait ~3 s for app-idle preload → click any sidebar connection → observe panel-open latency. If still sluggish, the cause is mount-time work (5-7 Tauri invokes + 4Hz status subscriber + 1Hz tickers) and the next fix is deferred-invokes / memoization via CDP-headless debugging per memory `feedback_white_screen_debug_via_chromium_cdp`.

---

## 10. Next-session prompt

```
Resume tuxlink from the dune-bison-salamander 2026-06-04 final handoff.

Handoff doc: dev/handoffs/2026-06-04-dune-bison-salamander-phase3-4-complete-codex-p1s-found.md
READ IT FIRST.

State: 20 commits on branch bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish-v2.
Phase 3 + 4 impl is COMPLETE (VARA + ARDOP lifecycle commands, arbiter
state machine, ABORT side-channel, DTO widening, CONNECT_DEADLINE drop).
Codex Phase 3-4 boundary review found 5 P1 + 4 P2 blockers — branch is NOT
mergeable as-is. 1033 lib tests passing.

Critical first actions:
1. Read THIS handoff first, especially §3 Codex findings + §4 fix ordering.
2. The 5 P1 cluster: close-vs-consumer races (#1+#4+#5; same mechanism,
   fix together), VARA b2f lifecycle violation (#2), intent-filtered
   outbound drain (#3).
3. Codex transcript at dev/adversarial/2026-06-04-phase3-4-boundary-codex.md
   (gitignored; 13056 lines) is the canonical source for full reasoning.
4. ALSO outstanding: verify the panel-preload perf fix (commit 54297cd)
   per §9 — operator hasn't confirmed it landed correctly.
5. File bd issues for the 4 P1 fix tasks + the 1 P2-sweep task before
   dispatching (per §4 ordering).
6. ALWAYS use `--lib` for cargo tests + absolute paths per memory
   pin-paths-in-worktree-sessions.
7. Re-run Codex Phase 3-4 review after the fix sweep to confirm closure
   before opening a PR.
8. Untouched operator state: task-amd-main-ui rebase + 5 stashes on main
   checkout. Multiple other live worktrees — coordinate via main-checkout-race
   hook.
```

---

Agent: dune-bison-salamander
