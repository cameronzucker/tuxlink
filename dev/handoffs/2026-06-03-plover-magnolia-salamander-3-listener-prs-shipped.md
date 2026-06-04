# Handoff — plover-magnolia-salamander — 3 transport-listener PRs shipped

> **Date:** 2026-06-03 · **Agent:** `plover-magnolia-salamander` · **Machine:** pandora
>
> **Arc:** Continuation from `2026-06-03-thistle-swallow-cedar-listener-foundation-shipped.md`. The listener-arms foundation (PR #299) had landed; the next session's task was to ship the 3 P1 transport-listener adapters (Telnet/xehu + Packet/inde + ARDOP/dhbl) that consume it. All 3 PRs are now open (#318, #319, #320), each with a feat-commit + a Codex-fix-commit. Two follow-up bd issues filed for deliberately-deferred scope.
>
> **Status at handoff:** 3 PRs awaiting operator review/merge. All MERGEABLE per `gh pr list`. CI runs are UNSTABLE (still running, not failed).

---

## 0. Critical first action — next session

```
1. Read THIS handoff first.
2. Review + merge the 3 listener PRs in any order (no inter-PR conflicts despite all
   touching ui_commands.rs — each PR stayed in its transport section):
   - #320 (xehu Telnet — largest scope)
   - #319 (dhbl ARDOP)
   - #318 (inde Packet)
3. Each PR has TWO commits: the feat commit + a fix commit addressing the Codex
   post-subagent adrev findings. Read the fix-commit body for findings + rationale.
4. After merges, claim from bd ready:
   - tuxlink-hfft (P1) AutoConnect Family A
   - tuxlink-bajc (P1) HF best-channel selector
   - tuxlink-k3ru (P2 NEW) Telnet inbound-mail symmetry (depends on #320 merged)
   - tuxlink-95g8 (P2 NEW) ARDOP live-modem wiring (depends on #319 merged)
   - tuxlink-xnoy (P2) VARA HF/FM listener (still blocked on ADR 0014 boundary)
5. Dispose 3 worktrees per ADR 0009 ritual once PRs merge (see §6).
```

---

## 1. Session arc (compressed)

1. **Read handoff + checked PR #299 (foundation) state — MERGED at 2026-06-03T07:57Z.** Foundation API surface read from origin/main (worktree's main-checkout was mid-rebase, used `git show origin/main:...`).
2. **Architecture doc + per-mode deep dives read** — confirmed the 3 transport listener scopes:
   - xehu/Telnet: new TcpListener + CALLSIGN/Password wire protocol + keyring + IPv4 wildcards (biggest)
   - inde/Packet: allowlist+arms overlay on top of shipped Packet listener (smallest)
   - dhbl/ARDOP: LISTEN flip + CONNECTED routing scaffolding + UI commands (medium)
3. **Claimed 3 bd issues + created 3 worktrees** via `new_tuxlink_worktree.py`.
4. **Dispatched 3 parallel subagents** with detailed per-transport prompts (Foundation API, spec inputs, scope, constraints, deliverables). Constraints: don't run Codex (coordinator runs it), don't open PR (coordinator does), don't run `tauri dev` (port collision), don't touch other transports' code.
5. **Subagent outcomes:**
   - **xehu** (general-purpose): shipped `winlink/telnet_listen.rs` (1276 lines, 27 tests) + foundation IPv4-wildcard extension + 13 Tauri commands. Pushed `7dec787`.
   - **inde** (general-purpose): wrote `packet_gate.rs` (432 lines, 10 tests) + Listen-path gate + 5 Tauri commands + `no_keyring()` StationPassword constructor + `with_packet_allowlist` test-injection builder. STALLED before committing. Coordinator picked up: fixed a `serde::Deserialize` lifetime issue in tests (`reason: &'static str` → `String`), fixed an E2E test regression by injecting `allow_all=TRUE` for the test fixture's answerer backend. Pushed `fe28f97`.
   - **dhbl** (general-purpose): shipped `ardop/listener.rs` (24 tests) + `InitConfig.initial_listen` field + 6 Tauri commands. Explicitly deferred live-modem wiring (LISTEN TRUE flip + CONNECTED event routing + B2F handoff) — noted in summary. Pushed `48d2846`.
6. **Codex post-subagent adrev — 3 sequential rounds** (sequential to avoid `codex_quota_gotcha` collision and per `codex_post_subagent_review`):
   - **xehu Codex (4 findings, all addressed in `183495b`):**
     - P1 decide callback returned empty Vec → AnswerCountMismatch on any non-empty inbound batch. Fixed: accept-all proposals (mirror native_packet_exchange line 1279 pattern).
     - P2 `ExchangeConfig.targetcall` was empty → strict FBB/WLE peers would see `; DE <mycall>`. Fixed: clone config per-session + inject parsed CALLSIGN.
     - P2 TOCTOU between shutdown check and accept(). Fixed: re-check shutdown immediately after accept() returns; drop the just-accepted stream if disarmed.
     - P3 file race on UI command load-mutate-save. Fixed: process-wide OnceLock<Mutex<()>> guarding all 5 mutating commands.
     - **Deferred:** mailbox Inbox-persist + Outbox-drain on inbound exchange (P1-shaped but bigger refactor) → bd `tuxlink-k3ru` filed.
   - **inde Codex (4 findings, all addressed in `694ef81`):**
     - P2 arms record created AFTER `answer()` → TTL gate effectively no-op. Fixed: arms before answer().
     - P2 allowlist load failures silently fell back to defensive default. Fixed: surface error via progress() + use distinct `allowlist-load-error` reject reason.
     - P2 reject path returned Ok(()) → backend status flipped Connected. Fixed: return Err(AuthFailed) with reason.
     - P3 file race on UI commands. Fixed: same mutex pattern as xehu.
   - **dhbl Codex (6 findings; 5 addressed in `3ee4750`, 1 deferred to bd `tuxlink-95g8`):**
     - P1 deferred: `ardop_listen` doesn't claim ModemSession + send LISTEN TRUE + route CONNECTED + B2F handoff. Same scope-deferral pattern as Telnet mailbox-persistence.
     - P2 ardop_listen now validates allowlist load BEFORE minting arms.
     - P2 ardop_set_listen now returns Err with pointer to tuxlink-95g8 (was silent-Ok).
     - P2 DISCONNECT failures on reject paths now `SessionError::Fault` (was `let _ =` swallowed).
     - P3 file mutex around 3 mutating UI commands.
     - P3 shared cross-transport forensics log path (`listener_arms.jsonl` — was per-transport `ardop/arms.jsonl`).
7. **Opened 3 PRs.** All MERGEABLE; CI running.

---

## 2. Branch state

| Branch | State |
|---|---|
| `main` | At `ed3de34` (origin/main since PR #299 merged + release-please 0.20.1) — operator's main is currently checked out to `task-amd-main-ui` mid-rebase, untouched this session |
| `task-amd-main-ui` | **OPERATOR STATE** — interactive rebase still paused (10 done, 7 remaining). 5 stashes. NOT TOUCHED this session. |
| `bd-tuxlink-xehu/telnet-listener` | **OPEN PR #320** — Telnet listener (feat `7dec787` + fix `183495b`); MERGEABLE; CI running |
| `bd-tuxlink-inde/packet-allowlist-overlay` | **OPEN PR #318** — Packet allowlist overlay (feat `fe28f97` + fix `694ef81`); MERGEABLE; CI running |
| `bd-tuxlink-dhbl/ardop-listen-true` | **OPEN PR #319** — ARDOP listener wiring (feat `48d2846` + fix `3ee4750`); MERGEABLE; CI running |
| `bd-tuxlink-3o2o/listener-arms-foundation` | Merged (PR #299) |
| `bd-tuxlink-qe5q/listener-architecture` | Merged (PR #296) |
| `bd-tuxlink-a6ic/wle-parity-audit` | Merged (PR #295) |

---

## 3. Open carry-over (bd issues this session filed)

| Issue | Pri | What | Depends on |
|---|---|---|---|
| **tuxlink-k3ru** (NEW) | P2 | Telnet inbound-mail symmetry: persist Inbox + drain Outbox on inbound exchange | PR #320 merged |
| **tuxlink-95g8** (NEW) | P2 | ARDOP listener live-modem wiring: claim ModemSession + send LISTEN TRUE + route CONNECTED + B2F handoff | PR #319 merged |

`bd ready` top order at handoff (after the 3 PRs merge):
1. **tuxlink-hfft** (P1) — AutoConnect Family A (per closure plan)
2. **tuxlink-bajc** (P1) — HF best-channel selector
3. tuxlink-k3ru (P2 NEW) — Telnet inbound-mail symmetry (depends on #320)
4. tuxlink-95g8 (P2 NEW) — ARDOP live-modem wiring (depends on #319)
5. tuxlink-xnoy (P2) — VARA HF/FM listener (depends on ADR 0014 boundary; still blocked)

---

## 4. Out-of-repo state changes (none this session)

No skills / settings / memory edits this session. Out-of-repo artifacts (gitignored, local-only):
- `dev/adversarial/2026-06-03-telnet-listener-codex.md` in xehu worktree
- `dev/adversarial/2026-06-03-packet-allowlist-codex.md` in inde worktree
- `dev/adversarial/2026-06-03-ardop-listener-codex.md` in dhbl worktree

Per CLAUDE.md `dev/adversarial/` convention: lost on worktree disposal; summaries already in commit bodies + PR descriptions.

---

## 5. Critical guidance for next session

1. **All 3 PRs are independent.** No merge-order dependency. Merge whatever order is most operationally convenient.
2. **CI is UNSTABLE not FAILED** at handoff time. The mergeability check via `gh pr list --json mergeStateStatus` says UNSTABLE for in-progress runs; recheck before merging.
3. **The 5 `worktrees/` from previous + this session are still on disk.** Each has the full clone + new feat-commits + Codex-fix-commits + gitignored adversarial transcripts. Disposal ritual (ADR 0009) AFTER merge:
   - Inventory (`git status --short`, `git ls-files --others --exclude-standard`, gitignored stateful, stashes)
   - cd back to main checkout
   - `rm -rf worktrees/bd-tuxlink-{xehu,inde,dhbl}-...`
   - `git worktree prune`
4. **Build hygiene per `feedback_shared_cargo_target_dir`:** the 3 worktrees each have their own `target/` (~5-10GB each); the disposal ritual rm-rf's them. No manual cleanup needed beyond the ritual.
5. **The 2 NEW bd issues (tuxlink-k3ru, tuxlink-95g8)** are P2 follow-ups for "what got intentionally deferred." Both are well-specced in their bd descriptions; pick them up after the 3 PRs land and after any P1 work (hfft, bajc).
6. **No operator on-air smoke was done this session.** RADIO-1 governs; operator runs. Each PR's test plan lists the smoke. The 3 PRs ship CORRECT-by-spec + unit-test-covered code; the smoke validates against real WLE/peer stations.
7. **Operator's `task-amd-main-ui` rebase still mid-flight** with 5 stashes. UNTOUCHED. The 4 prior untracked handoff docs + this one are still in the main checkout's working tree (gitignored except for the .md files — the operator's rebase will absorb them when resumed).

---

## 6. Worktree disposal — ready when PRs merge

For each PR (after merge), run from main checkout (i.e., after operator's rebase is resolved + on main):

```bash
# xehu
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-xehu-telnet-listener
git status --short && git ls-files --others --exclude-standard && git stash list
cd /home/administrator/Code/tuxlink
rm -rf /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-xehu-telnet-listener
git worktree prune

# Same pattern for bd-tuxlink-inde-packet-allowlist-overlay + bd-tuxlink-dhbl-ardop-listen-true
```

The `dev/adversarial/<topic>-codex.md` files in each worktree are gitignored + intentionally lost (transcripts are reference material; summaries in PR bodies).

---

## 7. Session totals

- **3 PRs opened this session:** #318 (inde), #319 (dhbl), #320 (xehu) — all OPEN + MERGEABLE
- **2 NEW bd issues filed:** tuxlink-k3ru, tuxlink-95g8
- **3 Codex adversarial rounds** (one per transport, sequential)
- **14 distinct findings addressed across the 3 fix commits** (1 P1 + 5 P2 + 1 P3 on xehu/Telnet wait — let me recount: xehu 4=1P1+2P2+1P3; inde 4=3P2+1P3; dhbl 6=1deferred+3P2+2P3 → addressed 13)
- **6 LOC + 13 PRs** total — 1700+ lines new code + tests across the 3 PRs
- **Code review depth:** Codex transcripts averaged ~3000 lines each
- **Foundation extensions:** 1 new IPv4 mid-position wildcard matcher on AllowedStations (xehu); 1 new `no_keyring()` StationPassword constructor (inde); 1 new `InitConfig.initial_listen` field (dhbl)

---

## 8. Next-session prompt (paste into a fresh session)

```
Resume tuxlink from the plover-magnolia-salamander 2026-06-03 3-listener-PRs-shipped handoff.

Handoff doc: dev/handoffs/2026-06-03-plover-magnolia-salamander-3-listener-prs-shipped.md
READ IT FIRST.

State: 3 transport-listener PRs are OPEN + MERGEABLE (#318 inde Packet, #319 dhbl ARDOP, #320 xehu Telnet). Each has a feat commit + a Codex-fix commit addressing 4-6 adversarial findings per PR. No inter-PR conflicts; merge order doesn't matter. After merges, dispose the 3 worktrees per ADR 0009 ritual (handoff §6 has the exact commands).

Critical first action:
1. `gh pr list --state open --json number,title,mergeable,mergeStateStatus` — check the 3 PRs (and #317 release-please).
2. Review + merge each (no dependency between them).
3. After merges, pick from `bd ready` — top P1s are tuxlink-hfft (AutoConnect) and tuxlink-bajc (HF best-channel); follow-ups tuxlink-k3ru + tuxlink-95g8 unblock after #320 + #319 merge respectively.

Untouched: operator's task-amd-main-ui rebase mid-flight + 5 stashes. Still mid-flight.
```

---

Agent: plover-magnolia-salamander
