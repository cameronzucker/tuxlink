# Handoff — mink-harrier-cardinal — VARA + ARDOP panel plan written + Codex round 1 done; rounds 2-5 + operator decisions pending

> **Date:** 2026-06-04 · **Agent:** `mink-harrier-cardinal` · **Machine:** pandora
>
> **Arc:** Continuation from `willow-mesa-mink` 2026-06-04 VARA + ARDOP panel spec handoff. This session executed the next two pipeline steps — `writing-plans` against the spec, then Codex round 1 of `build-robust-features`'s 5-round cross-provider adversarial review. Round 1 surfaced **6 P1 + 5 P2 findings** that block impl until the operator weighs in on 3 shape decisions and the plan/spec get revised.
>
> **Status at handoff:** PR #360 open with spec + plan + mocks; Codex round 1 findings captured in `dev/adversarial/` (gitignored, local only); bd issues filed for operator decisions + plan-revision backlog. Round 2-5 + plan revision + impl ALL still to come.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first.
2. Operator: answer the three shape-decision bd issues BEFORE anything else.
   - tuxlink-8gq3 — keep backend consent gate after dropping ConsentModal? (P1)
   - tuxlink-qtgg — airtime bound replacement: 120s cap vs 600s vs none? (P1)
   - tuxlink-d8bq — radio-only inbound listener: ship as designed, outbound-only, or drop for alpha? (P2)
   Without these, plan-revision (tuxlink-fl6e) is blocked; without plan-revision, tuxlink-0ye6 umbrella is blocked.
3. Agent: once operator decides, claim tuxlink-fl6e (plan revision).
   Read Codex round 1 transcript at dev/adversarial/2026-06-04-round1-design-coherence-codex.md
   (12,264 lines; the bd-fl6e description summarizes the 6 P1 + 5 P2 deltas).
4. Apply the plan-fix bundle to docs/superpowers/plans/2026-06-04-vara-ardop-panel-alpha-polish.md
   AND docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md per the operator's shape calls.
5. Resume build-robust-features at adrev rounds 2-5 (each Codex). Per memory
   feedback_no_carveout_on_cross_provider_adrev: do NOT skip rounds even after plan revision.
6. After rounds 2-5 land + are triaged: subagent-driven implementation per the (revised) plan.
```

---

## 1. Session arc (this turn only)

1. Read spec + handoff. Claimed tuxlink-0ye6. Picked moniker `mink-harrier-cardinal`.
2. Found PR #350 (tuxlink-tccc Arming label fix) already merged — disposition resolved to "defensive overlap" path.
3. Invoked `superpowers:writing-plans` against the spec. Discovered key facts during exploration:
   - The branch we were on (`bd-tuxlink-xygm/recover-handoffs`) is significantly behind main.
   - PR #348's listener code (ListenArmButton, AllowedStationsEditor, ardop_listen/vara_listen Tauri commands) IS on main.
   - The "Dial as" toggle the spec said to roll back IS on main in ArdopRadioPanel (lines 251/549/624 of main's version).
   - `SessionIntent` enum — assumed by spec to exist at `src-tauri/src/winlink/session.rs:109-136` — NOT FOUND by my grep at the time. (**TURNED OUT TO BE WRONG; SEE §3.**)
4. Wrote a 2839-line implementation plan: 8 phases, TDD discipline, bite-sized tasks, full code blocks per step. Saved to `docs/superpowers/plans/2026-06-04-vara-ardop-panel-alpha-polish.md`.
5. Hit the destructive-git hook when trying to commit on the main checkout (other live sessions active). Created worktree at `worktrees/bd-tuxlink-0ye6-vara-ardop-panel-alpha-polish/` on branch `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish` off `origin/main` via `new_tuxlink_worktree.py`.
6. Copied spec + plan + mocks to the worktree, committed (commit `51c3823`), pushed to origin, opened PR #360.
7. Invoked `build-robust-features`. Since brainstorm + plan-writing are done, started at adrev Step 2.
8. Wrote a 50-line attack-angle prompt for Codex round 1: design coherence + WLE-parity claims + spec-vs-code mismatch + load-bearing safeguard analysis.
9. Ran `npx --yes @openai/codex review -` with the prompt via stdin. Codex worked for some time; output was 12,264 lines including its grep + read + exec commands + final findings block. No quota errors.
10. Triaged the findings into 6 P1 + 5 P2. Created 4 bd issues: 3 operator-decisions (tuxlink-8gq3, tuxlink-qtgg, tuxlink-d8bq) + 1 plan-revision bundle (tuxlink-fl6e). Wired bd dep edges: tuxlink-0ye6 ← tuxlink-fl6e ← {tuxlink-8gq3, tuxlink-qtgg, tuxlink-d8bq}.
11. Session ends here — Codex rounds 2-5 + plan/spec revision + impl all deferred.

---

## 2. Codex round 1 findings summary

Full transcript: `dev/adversarial/2026-06-04-round1-design-coherence-codex.md` (12,264 lines, gitignored).

### P1 — must address before impl can start

| # | Title | Where | Severity for impl |
|---|---|---|---|
| 1 | **Add CONNECT to the ARDOP exchange command** | spec §6.1 vs plan Task 3.5 | Spec says Connect-within-open-session does CONNECT+B2F+DISCONNECT; plan says ardop_open_session does NO connect_arq. The existing `modem_ardop_b2f_exchange` assumes the link is up. As written, Open Session leaves ARDOP idle, then B2F runs over unconnected data stream. **Plan fix:** ARDOP's Connect button does `connect_arq(target)` + B2F + DISCONNECT; ardop_open_session spawns ardopcf only. |
| 2 | **Preserve backend consent gate (operator decision)** | plan Tasks 1.1-1.4 | Plan drops `consume_consent_token` atomic + replaces with busy guard. Codex: busy guard is overlap-prevention not authorization. Per Part 97 the operator-click IS the consent, but the backend invariant ("wrong token → fail before any I/O") regresses. **Operator decision** — tuxlink-8gq3. |
| 3 | **Don't drop airtime bound (operator decision)** | plan Task 1.5 | Plan replaces 120s `CONNECT_DEADLINE` with 600s "TCP-wedge guard." Codex: ARQTIMEOUT=30 is link-idle not connect-bound, so 600s IS real airtime. **Operator decision** — tuxlink-qtgg. |
| 4 | **Send VARA ABORT not DISCONNECT** | plan Task 4.1 | Plan wires `abort_in_flight` as `DISCONNECT\r` on cmd port. VARA's command codec already models ABORT separately from DISCONNECT — ABORT is hard teardown, DISCONNECT is graceful (waits for current burst). Spec's "must interrupt within ~2s" fails with DISCONNECT. **Plan fix:** send `ABORT\r` first, optionally `DISCONNECT\r` second; test captures ABORT as first command. |
| 5 | **Auto-arm vs transport ownership** | spec §2 vs current listener consumers | Spec says listener stays armed while operator can redial outbound; current ARDOP + VARA listener consumers take ownership of the transport for the entire armed window. Either outbound has no transport, OR outbound takes it back so listener isn't really armed. **Plan fix:** design backend session arbiter OR explicit disarm-during-outbound-rearm-after transition BEFORE shared-panel work. |
| 6 | **Extend the EXISTING SessionIntent** | plan Task 3.1 | **`SessionIntent` DOES exist** at `src-tauri/src/winlink/session.rs` with variants `Cms` / `RadioOnly` / `PostOffice` / `Mesh` / `P2p`. The spec's reference was right; my grep missed it because I searched `SessionIntent` literal across all files but the file I grep'd in the early reads was `winlink/session.rs` (B2F exchange module, different module). The enum I create in plan Task 3.1 would be a DUPLICATE. **Plan fix:** Task 3.1 becomes "add serde derives + `auto_arms_listener()` + `routing_flag()` methods to the existing enum"; do NOT create `session_intent.rs`. |

### P2 — should fix during revision

| # | Title | Severity |
|---|---|---|
| 7 | Auto-arm-as-WLE-parity framing is over-broad — rewrite spec § disparity by transport/intent; label as tuxlink design choice where decompiled WLE evidence doesn't confirm parity for that exact session type |
| 8 | Radio-only inbound listener semantics (**operator decision** — tuxlink-d8bq) |
| 9 | Lifecycle state should be backend-driven (subscribe to `modem_get_status` / `vara_status`), not React-local `useState`. Otherwise hot reload / ardopcf crash / VARA socket drop / rapid Open-Close all leak state desync |
| 10 | varaHfAdapter + varaFmAdapter share command names; backend `TransportKind::VaraHf` vs `VaraFm` distinguishes. FM sessions will be logged + gated as HF. Plan needs `transportKind` in adapter command args |
| 11 | Phase 1 (strip safeguards) leaves intermediate commits with less RF protection + broken tests. Reorder: safeguard-strip LAST after Open/Close + ABORT + shared panel are green |

---

## 3. SessionIntent — my own error, captured for forensics

My grep `grep -rn "SessionIntent" src-tauri/src/ src/` returned no matches early in the session. I concluded the spec was wrong + planned a NEW enum at `src-tauri/src/winlink/session_intent.rs` (plan Task 3.1).

Codex grep'd and found the enum DOES exist with full variants `Cms / RadioOnly / PostOffice / Mesh / P2p`. The path that surfaced is `src-tauri/src/winlink/session.rs` — which IS what I read (offset 100, limit 170) and IS what the spec referenced (`session.rs:109-136`). The lines I read happened to be `ExchangeRole` in the b2f module — that file's a different `session.rs` at a different path OR the lines were just past my limit OR there are two `session.rs` files.

Most likely: there are two `session.rs` files. One at `src-tauri/src/winlink/session.rs` (B2F exchange module — the one I read), and another at a different path (e.g., `src-tauri/src/winlink/modem/session.rs` or similar) where SessionIntent lives. My initial grep didn't recurse through nested mod paths in the right way.

**Lesson for next session:** verify enum existence via direct read after grep returns nothing, OR via `cargo doc` / `rust-analyzer`. Don't conclude "absent" from one negative grep.

This is the kind of error that compounds across 2800 lines of plan. Cross-provider adrev caught it. Per memory `feedback_no_carveout_on_cross_provider_adrev`: this is the unique value the adrev round delivers.

---

## 4. What's on disk + pushed

| Artifact | Path (relative to worktree root) | Commit | Pushed |
|---|---|---|---|
| Spec (copy of recovery-branch version) | `docs/superpowers/specs/2026-06-04-vara-ardop-panel-alpha-design.md` | `51c3823` | yes |
| **Plan** | `docs/superpowers/plans/2026-06-04-vara-ardop-panel-alpha-polish.md` | `51c3823` | yes |
| Brainstorm mocks | `docs/design/mockups/2026-06-04-vara-panel-mocks.html` | `51c3823` | yes |
| **Codex round 1 findings** | `dev/adversarial/2026-06-04-round1-design-coherence-codex.md` | NOT git-tracked (`.gitignore`d per CLAUDE.md) | local only — preserved on `pandora` |
| This handoff | `dev/handoffs/2026-06-04-mink-harrier-cardinal-vara-ardop-plan-codex-round1.md` | next commit | next push |

**No source code changed this session.** The PR is docs-only. The Codex transcript is the reference material for the plan-revision phase; it's not part of the PR.

---

## 5. Branch + worktree state

| Branch / worktree | State |
|---|---|
| `origin/main` | Latest is PR #354 merge (`61884c2 Merge pull request #354 from cameronzucker/bd-tuxlink-md17/uninstall-desktop-entry`). Untouched this turn. |
| `bd-tuxlink-xygm/recover-handoffs` (main checkout) | Operator's parked recovery branch. Session-start commit `5fd6cc2`. UNCHANGED this turn — hook denied my main-checkout commits, so all work went to the impl-branch worktree. Working tree has untracked handoff/mockup files from previous recovery work + `.beads/issues.jsonl` modified (auto-managed by bd). |
| `bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish` (impl worktree) | Created off `origin/main`. Carries the spec + plan + mocks (+ this handoff when committed). PR #360 open. |
| `task-amd-main-ui` | OPERATOR STATE — interactive rebase still mid-flight, 5 stashes. UNTOUCHED (multiple sessions running). |
| Other live worktrees | `bd-tuxlink-mxyz/tux-rig-rts`, `bd-tuxlink-mpds/rust-app-id`, `bd-tuxlink-yt2g/docs-diagrams-markers`, others — UNTOUCHED. |

**Worktree disposal:** the `worktrees/bd-tuxlink-0ye6-vara-ardop-panel-alpha-polish/` worktree is THE workspace for tuxlink-0ye6 — keep until tuxlink-0ye6 is merged + closed. Disposal ritual per [ADR 0009](../docs/adr/0009-worktree-disposal-ritual.md). Inside it: `node_modules/` installed (per pre-push lint-docs hook requirement). No stashes. No untracked content beyond what's committed.

---

## 6. bd issues this session

| Issue | Priority | Status | Role |
|---|---|---|---|
| **tuxlink-0ye6** | P1 | in_progress (claimed by mink-harrier-cardinal) | Umbrella; subsumes tuxlink-fzl7 + tuxlink-12sc; NOW also blocked on tuxlink-fl6e |
| **tuxlink-fl6e** | P1 | open (NEW) | Plan revision bundle — apply Codex Round 1 findings; blocks tuxlink-0ye6 |
| **tuxlink-8gq3** | P1 | open (NEW) | Operator decision: keep backend consent gate? Blocks tuxlink-fl6e |
| **tuxlink-qtgg** | P1 | open (NEW) | Operator decision: airtime bound? Blocks tuxlink-fl6e |
| **tuxlink-d8bq** | P2 | open (NEW) | Operator decision: radio-only listener scope? Blocks tuxlink-fl6e |
| tuxlink-fzl7 | P2 | open (blocked) | Subsumed by tuxlink-0ye6 (VARA Phase 3 outbound) |
| tuxlink-12sc | P2 | open (blocked) | Subsumed by tuxlink-0ye6 (VARA disarm ABORT) — Codex P1 #4 amends approach to use ABORT cmd not DISCONNECT |
| tuxlink-tccc | closed | — | PR #350 merged earlier (defensive overlap path) |
| tuxlink-9ls2 | closed | — | PR #348 merged earlier |

**`bd ready` after this session:** tuxlink-8gq3, tuxlink-qtgg, tuxlink-d8bq are the three operator-input items; tuxlink-fl6e + tuxlink-0ye6 are blocked on them.

---

## 7. Out-of-repo state / cleanup

- `pnpm install` ran inside the impl worktree (required by pre-push `lint:docs` hook).
- No background processes left running.
- No stashes created.
- HTTP server / dev server: none launched.
- `/tmp/codex-prompt-round1.txt`: leftover from Codex round 1 invocation; harmless temp file (~3 KB).

---

## 8. Untouched state (operator owns)

- Main checkout's working tree has untracked handoff/mockup files from previous recovery work (`2026-06-03-plover-magnolia-salamander-*`, `2026-06-04-jay-condor-shoal-*`, `2026-06-03-listener-ui-mocks.html`). Plus `.beads/issues.jsonl` shown modified — that's bd's auto-managed sync, safe to leave.
- `task-amd-main-ui` rebase + 5 stashes — UNTOUCHED.
- The recovery branch's spec + handoff + mockup files (committed at `73fd66c...` per the willow-mesa-mink handoff) have NOT been merged to main. The impl branch (PR #360) is a parallel copy — when PR #360 lands first, no conflict (same content); if the recovery branch goes via its own PR later, it'll need to drop the now-duplicate spec/mockup files.

---

## 9. Why this matters

The Round 1 Codex adrev validated the entire `build-robust-features` pipeline rationale. The plan as I wrote it had:

- A duplicate enum (P1 #6) — would have created two `SessionIntent`s in the codebase and caused merge mayhem.
- A subtly wrong VARA abort semantic (P1 #4) — would have shipped a "Close Session" that takes ~30s mid-burst to actually stop transmission. This is the EXACT failure mode tuxlink-12sc + the spec § "watched failure modes" was meant to fix.
- A bypassed RADIO-1-grade gate (P1 #2) — operator decision pending, but the design as written drops a backend invariant without a working replacement. Could be a regulatory concern.
- A 600s airtime ceiling (P1 #3) — actually 5× the previous 120s. Possible Part 97 issue.
- An auto-arm-vs-transport-ownership conflict (P1 #5) — current listener consumers OWN the transport; the spec's "listener stays armed while operator dials outbound" is architecturally undefined.
- A "ARDOP exchange runs over unconnected stream" (P1 #1) — would have shipped a B2F that talks to a disconnected modem.

ALL six surfaced from one round of adversarial review against the design + plan. The cross-provider review is the unique value (memory `no-carveout-on-cross-provider-adrev`). Rounds 2-5 will surface more — Codex round 1 was specifically the "design coherence + spec-vs-code" angle. Remaining angles: subagent-executability + cross-cutting failure modes + test adequacy + integration-with-existing-systems.

---

## 10. Next-session prompt

```
Resume tuxlink from the mink-harrier-cardinal 2026-06-04 plan + Codex round 1 handoff.

Handoff doc: dev/handoffs/2026-06-04-mink-harrier-cardinal-vara-ardop-plan-codex-round1.md
(on branch bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish; PR #360)
READ IT FIRST.

State: spec + plan + mocks are PR'd as #360 on bd-tuxlink-0ye6/vara-ardop-panel-alpha-polish.
Codex Round 1 of the 5-round cross-provider adrev found 6 P1 + 5 P2 findings against
the plan + spec, including a duplicate-SessionIntent (the enum DOES exist; my grep
missed it), wrong VARA-abort cmd (DISCONNECT instead of ABORT), and an auto-arm vs
transport-ownership architecture conflict.

Critical first actions:
1. OPERATOR — answer the three shape decisions BEFORE any agent action:
   - tuxlink-8gq3 (P1): keep backend consent gate after dropping ConsentModal?
   - tuxlink-qtgg (P1): airtime bound — 120s, 600s, no cap + ABORT-only, modem-native only?
   - tuxlink-d8bq (P2): radio-only inbound listener — ship as designed, outbound-only for alpha, or drop?
   Without these, plan revision is blocked → impl is blocked → umbrella is blocked.

2. AGENT — once operator unblocks: claim tuxlink-fl6e, read
   dev/adversarial/2026-06-04-round1-design-coherence-codex.md (local-only, 12k lines),
   apply the plan-fix bundle per tuxlink-fl6e description, commit + push to the impl branch.

3. AGENT — resume build-robust-features at Codex round 2 (cross-task ambiguity +
   plan executability). Per no-carveout-on-cross-provider-adrev: do NOT skip
   rounds 2-5 even after plan revision. Codex quota gotcha applies; defer if
   quota hits.

4. AGENT — after rounds 2-5 + triage: subagent-driven impl per the revised plan,
   in this worktree (NOT new branch).

Worktree: worktrees/bd-tuxlink-0ye6-vara-ardop-panel-alpha-polish/
  (node_modules installed; spec + plan + mocks + this handoff committed at HEAD)
Untouched operator state: task-amd-main-ui rebase still mid-flight + 5 stashes.
Other live worktrees per `git worktree list` — coordinate via main-checkout race hook.
```

---

Agent: mink-harrier-cardinal
