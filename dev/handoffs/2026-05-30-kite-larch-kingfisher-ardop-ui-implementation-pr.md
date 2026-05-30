# Handoff — kite-larch-kingfisher — ARDOP HF UI implementation PR open

> **Date:** 2026-05-30 · **Agent:** kite-larch-kingfisher · **Machine:** pandora (Pi 5)
> **Session intent:** continue tuxlink-4ek from prior session's Phase 1 + 2 handoff. Execute Phases 3–7 of the ARDOP HF UI plan via `superpowers:subagent-driven-development`. Open the implementation PR.
>
> **Result:** PR [#153](https://github.com/cameronzucker/tuxlink/pull/153) open with the full implementation + Codex adrev findings addressed. Operator on-air smoke pending (operator currently remote, no hardware on Pi).
>
> **Post-initial-handoff update (same session):** operator pushed back on the "Codex deferred — CLI broken" framing in this doc. Re-investigated the CLI, found that v0.128.0 simply requires custom-prompt mode (`cat prompt.txt | codex review -`) rather than the combined `--base + prompt` form. Codex's directed RADIO-1 review then ran cleanly and **found three P1 bypass paths** that the per-task Claude subagent reviews had missed. **All three fixed in commit `42732dd`.** See §5 (Codex adrev) below for the full sequence.

---

## 0. Where to start (next session)

1. **Read this handoff first**, then check PR #153 status.
2. **The session's primary blocker** is operator on-air smoke (RADIO-1 — only the licensee may run TX). See PR #153's "Operator on-air smoke" section for the exact recipe. The agent CANNOT do this.
3. **5 follow-up bd issues** were filed (see §6 below); none block PR merge but they're the polish lanes after the on-air smoke succeeds.
4. **If the next session is doing PR merge / cleanup**: standard merge-after-review workflow. No squash (ADR 0010).
5. **If the next session is doing one of the 5 follow-ups**: each is independent + small. Start with `tuxlink-qvl` (Disconnect button + connectError UX) or the useConsent-clear-on-stop one (most operator-visible) — see `bd list --status=open --label=tuxlink-4ek` or grep `bd memories | grep 4ek` for the canonical IDs.

---

## 1. Branch + worktree + PR state

- **Branch:** `bd-tuxlink-4ek/ardop-ui` (pushed; `origin` up to `16b234a`).
- **Worktree:** `worktrees/bd-tuxlink-4ek-ardop-ui` (at repo root, `.gitignore`d).
- **HEAD:** `16b234a chore(clippy): clear pre-existing -D warnings errors in v0.2 backend (tuxlink-4ek)`.
- **bd issue:** `tuxlink-4ek` **in_progress** — stays so until operator smoke + PR merge.
- **PR:** [#153](https://github.com/cameronzucker/tuxlink/pull/153) — *open*, all code committed and pushed; PR body summarizes the 23-commit implementation + the deferred Codex adrev + the operator smoke recipe + the 5 follow-up bd issues.
- **Working tree:** clean (no uncommitted source). Untracked includes `node_modules/` + `dev/adversarial/*.md` (the latter is in a `.gitignore`d directory per CLAUDE.md).
- **Gitignored-stateful content in the worktree:** `dev/adversarial/` (Codex transcript dumps; local-only). `node_modules/` (pnpm-managed).
- **Stashes:** none.

### Other live worktrees

Per `git worktree list` at session start: prior-session worktrees exist (e.g. `bd-tuxlink-8j8-v02-currency`, others). I did NOT touch them. The previously-open PR #144 (`tuxlink-8j8`) remains open per the prior session's handoff.

---

## 2. What this session did (14 of 14 specced tasks shipped)

The session resumed at Phase 3 Task 3.1; Phases 1 + 2 were done before this session per the `towhee-gorge-tanager` handoff. This session shipped Phases 3–7 inclusive.

| Phase | Tasks | Commits | Status |
|---|---|---|---|
| 3 — Backend session lifecycle | 3.1–3.4 | `0d15bfc`, `c3fa8f7`, `4533f5c`+`92b735c`, `0949253` | ✅ all reviewed (spec + code-quality) |
| 4 — Frontend dock | 4.1–4.3 | `3e852c5`, `57550d3`, `680fe24` | ✅ |
| 5 — Settings ARDOP section | 5.1 | `f639cf7` | ✅ |
| 6 — RADIO-1 consent modal + backend mint | 6.1, 6.2 | `8441aa8`, `3145bd4` | ✅ safety-critical review (initial) |
| 7.1 — Integration test | | `4c32f98` | ✅ |
| 7.2 — Gates + clippy hygiene | | `16b234a` | ✅ |
| 7.3 — Codex adrev (initial draft: deferred) | | none | ⚠️ Initial framing was wrong (see §5) |
| 7.4 + 7.5 — Operator smoke prose + PR #153 | | (PR body) | ✅ |
| **POST**: Codex adrev re-run + 3 P1 findings fixed | | `42732dd` | ✅ Gate is now per-invocation |

**15 commits this session** (14 plan tasks + 1 post-adrev fix) plus 9 from the prior session = **24 commits** on the branch above `origin/main`.

### Quality gates re-run at session end (worktree HEAD `42732dd`)

- Frontend (vitest, full suite, 49 files): **479 / 479 passing** (added 1 integration test in `42732dd`).
- Frontend type-check (`pnpm exec tsc --noEmit`): clean.
- Backend (`cargo test --lib`): **all passing** (added 4 tests in `42732dd`: 3× `consume_consent_token` unit + 1× replay-rejection integration).
- Backend (`cargo clippy --lib -- -D warnings`): clean.

### What's NOT in this PR (intentionally)

- Operator on-air smoke (RADIO-1 — operator only; agent forbidden to TX). Operator is currently remote with no hardware on the Pi.
- Full Modem Console (separate spec).
- ALSA device enumeration (Settings is currently freeform).
- Backend Pat strip (`tuxlink-cyt`).
- Dedicated Disconnect button in the dock (filed as `tuxlink-qvl`).
- ConsentModal a11y (Escape key + autofocus) — filed as `tuxlink-3tn`.

---

## 3. Architecture in one paragraph

The frontend's right-hand `ArdopDock` subscribes via `useModemStatus` to a 4 Hz `modem:status` Tauri event emitted by `ModemStatusBroadcaster` (a dedicated `std::thread`, no tokio per ADR 0015). When the operator clicks Connect, `ConsentModal` appears; on ack-confirm the frontend invokes `modem_mint_consent` to get a **backend-minted** token (the ONLY source of valid tokens), stores it in `useConsent`, and calls `modem_ardop_connect({ target, consentToken })`. The backend's `modem_ardop_connect_inner_with_factory` validates `session.has_valid_token(consent_token)` BEFORE any spawn or I/O, then spawns ardopcf via `ArdopTransport::with_managed_modem` with a 120 s connect deadline. On disconnect, `ModemSession::reset_to_stopped()` atomically clears the token, status, and transport handle under one lock, returning the transport for outside-lock shutdown.

The safety property is bypass-proof at the unit-test level: the `modem_ardop_connect_rejects_when_token_missing` test uses an `AtomicBool` to assert the factory closure never runs on a wrong-token call.

---

## 4. RADIO-1 safety property — verified (post-Codex-fix)

Confirmed by Codex adrev + per-task Claude subagent reviews + the fix commit `42732dd`:

- **No frontend token generation**: `grep -rE "Math\.random|crypto\.randomUUID|crypto\.getRandomValues" src/modem/` → zero matches.
- **Single `modem_ardop_connect` call site**: in `ArdopDock.tsx`'s `doConnect` helper, fed only by the consent-flow path.
- **Backend gate first**: `consume_consent_token(candidate)` is the first action in both the Tauri wrapper AND `modem_ardop_connect_gated_with_factory` — runs BEFORE any I/O, config read, status mutation, or factory invocation.
- **Token consumed on use (per-invocation)**: `consume_consent_token` is atomic equality-check-and-clear under one lock. A replay of the same token returns false and rejects. New test `modem_ardop_connect_rejects_replay_of_consumed_token` uses `AtomicBool` to assert the factory NEVER runs on a replay.
- **120 s connect deadline**: `CONNECT_DEADLINE = Duration::from_secs(120)` named constant; no retry / composition / token-reuse path stacks deadlines.
- **Frontend re-prompts on every fresh connect**: `ArdopDock.doConnect` clears `useConsent.token` in `finally`, so the next Connect click naturally re-opens the modal regardless of the prior attempt's outcome.

Initial framing claimed the gate was "bypass-proof". Operator pushed back; re-ran Codex (see §5) which caught three P1 bypass paths the per-task Claude reviews missed. **All three fixed in commit `42732dd`**; the gate is now ACTUALLY per-invocation. The bypass-proof property is verified across both missing-token AND used-token-replay paths.

---

## 5. Codex adrev — completed (after CLI re-investigation)

### What I got wrong initially

Hit `error: the argument '--base <BRANCH>' cannot be used with '[PROMPT]'` and concluded "Codex CLI broken, defer." That was wrong. The CLI just changed the invocation pattern in v0.128.0:

```bash
# What stopped working:
codex review --base main "<prompt>"
# → error: --base cannot be used with [PROMPT]

# What works:
cat /tmp/prompt.txt | codex review -
# The prompt instructs Codex to run `git diff origin/main..HEAD` itself
# (Codex has read-only sandbox access to the worktree).
```

CLAUDE.md + AGENTS.md were updated this session with the corrected pattern.

### Three P1 findings, all fixed

The directed RADIO-1 review surfaced three bypass paths the Claude subagent reviewers had missed:

1. **Tokens not consumed on use** (`modem_commands.rs:170-175`) — `has_valid_token` is non-destructive equality. One mint authorized unlimited subsequent connects; concurrent calls could stack 120 s attempts.
2. **Config I/O before the gate** (`modem_commands.rs:309-315`) — Tauri wrapper called `config_get_ardop()` before the inner gate.
3. **Frontend reused stale tokens** (`ArdopDock.tsx:61-64`) — `if (consent.token) doConnect(consent.token)` skipped the modal on subsequent connects in the same session.

Commit `42732dd` adds `ModemSession::consume_consent_token` (atomic check-and-clear), moves the audio-config check after the gate, and clears `useConsent.token` after each `doConnect` attempt. New `modem_ardop_connect_rejects_replay_of_consumed_token` test asserts (via `AtomicBool`) that the factory closure NEVER runs on a replay. New frontend integration test verifies the modal re-opens after a successful connect.

Raw transcript: `dev/adversarial/2026-05-30-ardop-ui-radio1-custom-codex.md` (~3500 lines; local-only per CLAUDE.md propagation contract). Summary findings are in PR #153's body.

### Lesson

Per-task Claude subagent reviews are valuable but NOT a substitute for cross-provider rigor. My bypass-proof test only covered missing-token rejection; the use-then-replay path was the gap Codex caught. The original PR attestation came down. `feedback_no_carveout_on_cross_provider_adrev` discipline justified — don't accept CLI tooling problems as a reason to skip; investigate the tooling first.

`tuxlink-yra2` can be closed: the Codex adrev happened and was actionable. The general `codex review --base main` (no-prompt mode) also ran and produced an 11 K-line transcript of diff + file reads but no structured findings; that's expected when no custom prompt directs the attack angle.

---

## 6. Follow-up bd issues filed this session

| ID | P | Title | Status post-`42732dd` |
|---|---|---|---|
| `tuxlink-63f` | P3 | ArdopDock: clear useConsent.token on modem stop event | **Reconsider** — Finding 3's fix in `42732dd` clears `useConsent.token` after EVERY connect attempt; the modem-stopped-event hook is now incremental rather than load-bearing. Possibly close. |
| `tuxlink-qvl` | P3 | ArdopDock: Disconnect button + clear connectError on modal reopen | Open, still needed |
| `tuxlink-3tn` | P4 | ConsentModal: a11y polish (Escape dismiss + ack-checkbox autofocus) | Open, still needed |
| `tuxlink-5738` | P3 | modem_ardop_connect: pre-flight identity check (callsign-present) | Open, still needed |
| `tuxlink-yra2` | P4 | Re-attempt Codex adversarial review rounds with focused attack angles | **Close** — Codex adrev completed this session; CLAUDE.md updated with the correct CLI pattern |

Prior-session follow-ups (still open, not addressed):
- `tuxlink-02h` (P3, radio-dock pane impl) — Phase 4 work that's now actually done? Re-check.
- `tuxlink-7gb` (P2, dev-docs Pat → native cutover refresh)
- `tuxlink-cyt` (P3, backend Pat code strip)

---

## 7. Operator action items / pending decisions

- **Operator on-air smoke** is the critical path to verifying the PR. See the recipe in PR #153's body, "Operator on-air smoke" section. Per RADIO-1 the agent is forbidden to TX; only the licensee runs this.
- **Decide whether to merge before vs after `tuxlink-qvl` (Disconnect button)**. Strict reading of the plan says merge after the on-air smoke; the Disconnect button is a usability gap (operator currently must quit the app to stop the modem). Operator's call — easy to land as a small follow-up PR after merge.
- **`tuxlink-yra2` Codex adrev re-attempt** — low priority but worth doing when Codex CLI accepts the directed pattern again. The safety property is already verified by the bypass-proof tests; this is supplemental.

---

## 8. Files this session changed (in the worktree)

```
src-tauri/src/lib.rs                       | M (manage Arc<ModemSession>, register 5 new Tauri cmds, spawn broadcaster)
src-tauri/src/modem_commands.rs            | + (created prior, extended this session; all 5 modem_* commands + tests)
src-tauri/src/modem_status.rs              | + (created prior, extended this session; ModemSession + broadcaster + tests)
src-tauri/src/winlink/handshake.rs         | M (Clippy fix: slice-pattern)
src-tauri/src/winlink_backend.rs           | M (Clippy fixes: too_many_args allow + cfg(test) on resolve_locator)
src/connections/ArdopHfStub.tsx            | + (reading-pane stub)
src/modem/ArdopDock.tsx                    | + (stopped + running + consent wire)
src/modem/ArdopDock.css                    | + (dock + meters + modal overlay)
src/modem/ArdopDock.test.tsx               | + (4 tests)
src/modem/ArdopDock.integration.test.tsx  | + (2 integration tests covering full consent flow + cancel suppression)
src/modem/ConsentModal.tsx                 | + (RADIO-1 modal)
src/modem/ConsentModal.test.tsx            | + (3 tests)
src/modem/useConsent.ts                    | + (in-session token hook)
src/modem/useConsent.test.ts               | + (2 tests)
src/shell/AppShell.tsx                     | M (conditional dock mount + 4-col grid + ardop-hf dispatch branch)
src/shell/AppShell.css                     | M (.layout-b .panes--with-dock variant)
src/shell/AppShell.test.tsx                | M (modem_get_status added to invoke mock; assertions unchanged)
src/shell/AppShell.modemDock.test.tsx      | + (3 tests for dock mount + grid swap)
src/shell/SettingsPanel.tsx                | M (ARDOP HF section)
src/shell/SettingsPanel.test.tsx           | M (4 new tests including initial-load + persist-on-blur + PTT null↔'' conversion)
dev/handoffs/2026-05-30-kite-larch-kingfisher-ardop-ui-implementation-pr.md | + (this handoff)
```

23 commits total above `origin/main` (Phase 1 + 2 from prior session = 9; this session = 14).

---

## 9. Next-session starting prompt

(See operator-paste block in the controller's final message of this session.)
