# Handoff — willow-mesa-mink — ARDOP P2P caller UI + VARA listener E2E (PR #348 open)

> **Date:** 2026-06-03 · **Agent:** `willow-mesa-mink` · **Machine:** pandora
>
> **Arc:** Single-session continuation from `2026-06-03-plover-magnolia-salamander-listener-e2e-pr-open.md`. Resumed with PR #344 already merged. Operator-reported gaps in the converged build: (1) no VARA listener capability, (2) ARDOP P2P missing altogether. Built one umbrella PR (#348) covering ARDOP P2P caller UI + VARA listener backend + VARA listener UI. All gates green; one P2 follow-up filed (tuxlink-12sc).
>
> **Status at handoff:** **PR #348 OPEN**, awaiting CI + operator review/merge. All local gates pass: clippy + cargo test + typecheck + vitest. Codex adrev round was inconclusive (Round 1 hit output budget reading source, Round 2 hit input-size cap from a mis-targeted diff) — flagged transparently in PR body. Recommended remediation: re-run a tight diff-only Codex round.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first.
2. Check PR #348 state: `gh pr view 348 --json state,mergeable,mergeStateStatus,statusCheckRollup`
   - If MERGEABLE/CLEAN + CI pass + operator approves → merge.
   - Otherwise, run a targeted Codex round on the diff alone before recommending merge:
     gh pr diff 348 > /tmp/348.diff
     cat /tmp/348.diff | npx --yes @openai/codex exec - 2>&1 | tee dev/adversarial/2026-06-04-listener-vara-ardop-p2p-codex-r3.md
     (the prior two rounds were inconclusive — see PR body §"Codex adversarial review")
3. After merge:
   - Close bd issues: tuxlink-9ls2 (umbrella) + tuxlink-xnoy (absorbed)
   - Dispose worktree per ADR 0009:
     cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9ls2-listener-vara-ardop-p2p
     git status --short && git ls-files --others --exclude-standard
     cd /home/administrator/Code/tuxlink
     rm -rf /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9ls2-listener-vara-ardop-p2p
     git worktree prune
4. tuxlink-12sc (P2) — VARA disarm ABORT side-channel — is a known follow-up. Operator decides when to schedule.
5. Prior-session deferred cleanup still pending in main checkout (NOT touched this session):
   - .beads/issues.jsonl staged in main checkout (the bog-bluff-mesa session's bd state)
   - 3 orphan files committed THIS session as part of this handoff push:
     - dev/handoffs/2026-06-03-plover-magnolia-salamander-3-listener-prs-shipped.md
     - dev/handoffs/2026-06-03-plover-magnolia-salamander-listener-e2e-pr-open.md
     - docs/design/mockups/2026-06-03-listener-ui-mocks.html
     (these are the orphans the bluff-birch-cove session was attempting to land on bd-tuxlink-xygm/recover-handoffs)
6. Untouched operator state: task-amd-main-ui rebase still mid-flight + 5 stashes.
```

---

## 1. What shipped (PR #348)

Three commits on `bd-tuxlink-9ls2/listener-vara-ardop-p2p` off `origin/main`:

| Commit | Phase | LOC | Subject |
|---|---|---|---|
| `39b86fa` | Phase 1 | +113 / -4 | feat(ardop): expose P2P caller intent — UI selector + parse_b2f_intent |
| `e9353f2` | Phase 2 | +1605 / -11 | feat(vara): listener backend — LISTEN ON gating + serve_inbound_one + B2F answer |
| `de195ac` | Phase 3 | +89 | feat(vara-ui): Listen section — arm button + allowlist editor on VaraRadioPanel |

**Total:** +1807 / -15 across 10 files. Phase 2 was implemented by a `general-purpose` subagent dispatch (Codex post-subagent review pattern was attempted but inconclusive — see §3). Phases 1 + 3 were coordinator-direct.

### Per-phase content

**Phase 1 — ARDOP P2P caller UI:**
- `RadioPanelMode` widened: `ardop-hf` intent becomes `'cms' | 'p2p'`
- `ArdopRadioPanel.tsx`: new `intent` local state + "Dial as" selector at the top of the Connect form
- `modem_ardop_b2f_exchange` Tauri command: new `intent: String` arg
- `parse_b2f_intent` helper (narrowly accepts `"cms" | "p2p"` only; rejects unknown values)
- `run_ardop_b2f_exchange`: takes `intent: SessionIntent`, skips the keyring password lookup for non-CMS dials (peers don't challenge per FBB)

**Phase 2 — VARA listener backend (subagent-implemented):**
- NEW `src-tauri/src/winlink/modem/vara/listener.rs` (815 LOC including tests): full mirror of `ardop/listener.rs` — `parse_peer_call`, `station_password_no_keyring`, `allowed_stations_path` (under `listener/vara/`), `decide_for_vara_event`, `set_listen`, `serve_inbound_one`, `InboundOutcome`, `VaraListenerError`.
- `VaraSession::take_transport()` + `return_transport()` + `send_listen_on()` lifecycle primitives
- `vara_listen` / `vara_set_listen` / `vara_allowed_stations_*` Tauri commands (mirror ARDOP), plus `vara_listener_consumer_task` (mirror `ardop_listener_consumer_task`)
- `winlink_backend::run_vara_b2f_answer` — drives `run_exchange_with_role(Answer)` directly on `try_clone()`'d data-socket halves (no intermediate framing wrapper)
- `lib.rs` registers `VaraListenState` managed-state + 6 new Tauri commands

**Phase 3 — VARA listener UI:**
- `VaraRadioPanel.tsx`: Listen section with `ListenArmButton` + `AllowedStationsEditor`, driven by the shared `useListenerState` hook
- Callsign-only allowlist (no IP layer); no station-password block (matches ARDOP posture)
- Arm button busy-disabled when transport not Open

### Key architectural decisions

1. **VARA refuses to arm without an open session.** Unlike ARDOP which has an auto-spawn path (`start_modem_listen_only`), VARA is an externally-managed Windows process (Wine on x86 Linux). `vara_listen` validates `VaraSession::snapshot().state == Open` before sending `LISTEN ON`.
2. **VARA wire form is `LISTEN ON` / `LISTEN OFF`** (not ARDOP's `LISTEN TRUE` / `LISTEN FALSE`). VARA's parser doesn't have a first-class Listen-echo variant; the subagent's `set_listen` drains async events for 500ms after send rather than blocking on a specific echo.
3. **`run_vara_b2f_answer` uses raw `try_clone()` on data-socket halves** — no intermediate framing wrapper. ARDOP needs `Arc<Mutex>` shared-handle wrapping because its data is a `&mut dyn ReadWrite` trait object; VARA's `&mut TcpStream` clones cleanly at the OS layer.
4. **VARA arms TTL record uses `TransportKind::VaraHf`.** Both VARA HF and VARA FM share the same panel + same listener arms; the variant just records a transport tag for the forensics log.

---

## 2. Verification gates passed

| Gate | Command | Result |
|---|---|---|
| CI clippy | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings` | ✅ Exit 0, no warnings (the exact CI command — same one that bit me in PR #344's CI cascade) |
| Cargo lib tests | `cargo test --manifest-path src-tauri/Cargo.toml --lib` | ✅ 951 passed, 0 failed (40.6s on Pi) |
| TypeScript | `pnpm typecheck` | ✅ Clean |
| Vitest | `pnpm exec vitest run` | ✅ 1090 tests / 104 files passing (326s on Pi) |

The clippy run is the exact CI invocation; that addresses the PR #344 CI cascade lesson where a `borrowed_box` lint slipped to CI because local dev cargo doesn't enforce `-D warnings`.

---

## 3. Codex adversarial review — INCONCLUSIVE

Two Codex rounds were attempted. Both produced incomplete output:

| Round | Approach | Outcome | Output file |
|---|---|---|---|
| R1 | Full prompt with attack-angle guide + "read these files" instructions; ran via `npx codex review -` with prompt on stdin | 4022 lines. Codex spent its output budget reading source files via exec commands, never reached the "produce findings" step. | `dev/adversarial/2026-06-04-listener-vara-ardop-p2p-codex.md` |
| R2 | Tighter prompt + the full diff piped as stdin; instructions to work from the diff alone | 14643 lines. Hit Codex's 1MB input limit: `Error: turn/start: turn/start failed: Input exceeds the maximum length of 1048576 characters` — because I generated the diff from the main checkout (`cwd=/home/administrator/Code/tuxlink`) where `HEAD` is `bd-tuxlink-xygm/recover-handoffs`, NOT my worktree's `HEAD`. The piped diff was the WRONG diff (recover-handoffs branch vs origin/main, which includes many files I didn't touch). Agent error. | `dev/adversarial/2026-06-04-listener-vara-ardop-p2p-codex-r2.md` |

**Lesson for next agent:** when running Codex from a worktree, either:
- Run Codex from inside the worktree directory (`cd worktrees/bd-tuxlink-XX-slug && cat prompt | npx codex …`), OR
- Generate the diff with `git -C <worktree-path> diff origin/main..HEAD > /tmp/diff.patch` to avoid the main-checkout-HEAD-confusion

**Recommendation for #348:** before merge, run R3 with the tight-diff approach correctly:
```bash
gh pr diff 348 > /tmp/348.diff
cat /tmp/codex-tight-prompt /tmp/348.diff | npx --yes @openai/codex exec - 2>&1 \
  | tee dev/adversarial/2026-06-04-listener-vara-ardop-p2p-codex-r3.md
```

Per project memory `no_carveout_on_cross_provider_adrev`, skipping cross-provider Codex review is problematic. Mitigating factors here: (a) Phase 2 is a near-byte-mirror of the ARDOP listener pattern that Codex DID review in PR #344 (4 P1 + 3 P2 findings, all addressed); (b) CI gates all green; (c) the mechanical similarity reduces (but does not eliminate) net new review value.

---

## 4. Self-audited concerns (no Codex round)

Caught one real concern myself before push:

**Disarm-during-active-B2F (P2 follow-up, filed as tuxlink-12sc).** The `vara_set_listen(false)` path sets a shutdown flag that the consumer task observes only between loop iterations. While `run_vara_b2f_answer` is running (tens of seconds for a small message), the consumer is blocked — disarm clicked → modem keeps transmitting B2F responses until B2F completes naturally. ARDOP's equivalent (PR #344 Codex P1#3) shipped an ABORT side-channel for this; VARA's architecture (consumer owns transport via `take_transport`) needs an extra clone of the cmd_writer in the handle to enable the same.

**Mitigation worth noting:** worst-case is bounded by the data-socket 2s read_timeout chains; not a license violation, but the button doesn't match operator intuition. Documented in PR body §"Known v1 limitation."

---

## 5. Branch / worktree state

| Branch | State |
|---|---|
| `main` | Unchanged this session. |
| `bd-tuxlink-9ls2/listener-vara-ardop-p2p` | **OPEN PR #348** — 3 commits, all gates green, awaiting CI + review/merge. |
| `bd-tuxlink-xygm/recover-handoffs` | Operator's current branch in main checkout. Pre-session state: 3 untracked orphan files + 1 staged `.beads/issues.jsonl`. This session lands one new handoff here (THIS file). The 3 orphan files and the staged bd JSONL are STILL untouched (per the auto-mode classifier's deny on operator-state mutations earlier). |
| `task-amd-main-ui` | Operator state — interactive rebase still mid-flight (10 done, 7 remaining). 5 stashes. UNTOUCHED. |

**Worktrees on disk** (per `git worktree list`):
- `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9ls2-listener-vara-ardop-p2p` — THIS session's work; clean (tracked changes all committed); dispose per ADR 0009 after #348 merges
- 11+ other worktrees from prior sessions (Cameron/operator-managed; this session didn't touch)

---

## 6. bd issues this session

| Issue | Status | What |
|---|---|---|
| **tuxlink-9ls2** | in_progress | The umbrella for this session's work — closes on PR #348 merge |
| **tuxlink-12sc** | open (NEW) | P2 follow-up: VARA disarm ABORT side-channel |
| tuxlink-xnoy | open → close on #348 merge | VARA HF + FM listener completion (absorbed into 9ls2) |

`bd ready` ordering after #348 merges + 9ls2/xnoy close (per the prior handoff's outlook):
1. **tuxlink-hfft** (P1) — AutoConnect Family A
2. **tuxlink-bajc** (P1) — HF best-channel selector
3. Other P1 backlog
4. **tuxlink-12sc** (P2) — VARA disarm ABORT (this session's follow-up)

---

## 7. Out-of-repo state changes

- HTTP server, scheduled wakeups: all clean (no running daemons)
- Codex adrev transcripts (gitignored, local-only):
  - `dev/adversarial/2026-06-04-listener-vara-ardop-p2p-codex.md` (R1, inconclusive)
  - `dev/adversarial/2026-06-04-listener-vara-ardop-p2p-codex-r2.md` (R2, input-size error)
- No skills/settings/memory edits this session

---

## 8. Untouched state (operator owns)

- `task-amd-main-ui` interactive rebase still mid-flight, 5 stashes
- 3 orphan files in `dev/handoffs/` + `docs/design/mockups/` (pre-session state from bluff-birch-cove's recovery attempt) — STILL untracked
- Staged `.beads/issues.jsonl` in main checkout — STILL staged (auto-mode classifier blocked this from being touched earlier in the session; operator can verify + commit or reset as they see fit)

---

## 9. Next-session prompt (paste into a fresh session)

```
Resume tuxlink from the willow-mesa-mink 2026-06-03 listener-vara-ardop-p2p-pr-open handoff.

Handoff doc: dev/handoffs/2026-06-03-willow-mesa-mink-listener-vara-ardop-p2p-pr-open.md
READ IT FIRST.

State: PR #348 (ARDOP P2P caller UI + VARA listener end-to-end) is OPEN. All local gates passed (clippy + cargo test + typecheck + vitest). Codex adversarial review was INCONCLUSIVE both attempts (R1 budget exhaustion, R2 input-size cap from agent error using wrong-cwd diff). One P2 follow-up filed as tuxlink-12sc (VARA disarm ABORT side-channel — known v1 limitation, documented in PR body).

Critical first actions:
1. `gh pr view 348 --json state,mergeable,mergeStateStatus,statusCheckRollup` to check CI status.
2. **Run a tight-diff Codex round before merge** (the previous two rounds were inconclusive — see handoff §3 for the exact pattern; agent error in R2 was using the main checkout's HEAD rather than the worktree's). The recommended invocation is in handoff §0 step 2.
3. After merge: close tuxlink-9ls2 + tuxlink-xnoy; dispose worktree per ADR 0009 (paths in handoff §0).

Untouched operator state: task-amd-main-ui rebase still mid-flight + 5 stashes. Three orphan files + staged bd JSONL in main checkout from the prior session — still untouched (auto-mode classifier blocked agent mutations).
```

---

Agent: willow-mesa-mink
