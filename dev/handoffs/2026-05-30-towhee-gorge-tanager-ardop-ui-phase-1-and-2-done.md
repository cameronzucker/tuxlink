# Handoff — towhee-gorge-tanager — ARDOP HF UI: Phases 1 + 2 complete, Phase 3 next

> **Date:** 2026-05-30 · **Agent:** towhee-gorge-tanager · **Machine:** pandora (Pi 5)
> **Session intent:** brainstorm → spec → plan → subagent-driven execution of the
> ARDOP HF UI for bd issue `tuxlink-4ek`. Operator chose subagent-driven mode; this
> handoff exists because the operator asked to break the work across sessions
> rather than push all 20 tasks in one continuous run.
>
> **The next session resumes at Phase 3 Task 3.1.**

---

## 0. Where to start (next session)

1. **Read the plan first:** `worktrees/bd-tuxlink-4ek-ardop-ui/docs/superpowers/plans/2026-05-30-ardop-hf-ui-plan.md`. That is the primary artifact; this handoff is just current-state context around it.
2. **Spec:** sibling path `docs/superpowers/specs/2026-05-30-ardop-hf-ui-design.md` (operator-approved as-is — no changes requested).
3. **Re-invoke the `superpowers:subagent-driven-development` skill** to continue execution. Pass the plan path + the explicit instruction to **start at Phase 3 Task 3.1** (`ModemSession` shared state). All earlier tasks are committed; do NOT re-do them.
4. **Pick a new session moniker** via `python3 .claude/scripts/get_agent_moniker.py` — this session was `towhee-gorge-tanager`; the next session gets its own moniker and uses it in all forward commit trailers.

---

## 1. Branch + worktree state

- **Branch:** `bd-tuxlink-4ek/ardop-ui` (pushed; remote up-to-date).
- **Worktree:** `worktrees/bd-tuxlink-4ek-ardop-ui` (at repo root, gitignored).
- **HEAD:** `aa32b65` — `feat(backend): config_get_ardop / config_set_ardop Tauri commands (tuxlink-4ek)`.
- **bd issue:** `tuxlink-4ek` is **in_progress**, owned by the worktree.
- **PR:** [#147](https://github.com/cameronzucker/tuxlink/pull/147) — *open*, contains the spec + plan + 6 implementation commits + 2 fix commits + this handoff (once committed).
- **Working tree:** clean before this handoff (no uncommitted source changes).
- **Untracked in worktree:** only `node_modules/` (gitignored, harmless).
- **Stashes in worktree:** none.

---

## 2. What's done (6 of 20 tasks, 9 commits)

| # | Task | Commit(s) | Status |
|---|---|---|---|
| 1.1 | Rust `ModemStatus` struct + serde wire contract | `7f96535` | ✅ spec + code reviewed |
| 1.2 | TS `ModemStatus` type mirror | `b7c42a6` → `3a79a49` (Readonly fix) | ✅ |
| 1.3 | `useModemStatus` React hook | `d62b3f6` → `53b9f9b` (listener-leak / error-surface / test-gap fixes) | ✅ |
| 1.4 | `sessionTypes` ARDOP-HF catalog entry | `928c1ae` | ✅ |
| 2.1 | `ArdopUiConfig` struct + `Config.modem_ardop` field | `b76ba51` | ✅ |
| 2.2 | `config_get_ardop` / `config_set_ardop` Tauri commands + new `src-tauri/src/modem_commands.rs` module | `aa32b65` | ✅ |

Two phases done end-to-end. Every commit on the branch has passed an independent spec-compliance reviewer + code-quality reviewer (single subagent each, separate dispatches per the skill).

**Quality gates re-run for this handoff** (worktree HEAD `aa32b65`):
- Frontend (vitest, scoped to `src/modem` + `src/connections`): **17 passed, 0 failed**.
- Backend (cargo test --lib, `config::` + `modem_status::` + `modem_commands::`): **22 passed, 0 failed**.

---

## 3. What's next (14 tasks remain, 5 phases)

The plan's `Self-review` section maps every spec requirement to a task — that mapping is intact.

**Phase 3 — backend session lifecycle (4 tasks). Start here.**
- **3.1** `ModemSession` shared state in `src-tauri/src/modem_status.rs` — `Mutex<ModemSessionInner>` holding the current `ModemStatus` snapshot + the in-process **RADIO-1 consent token**. Adds `mint_consent_token()` + `has_valid_token(candidate)` + `clear_consent_token()`. **rand dependency probably needs to be added to `src-tauri/Cargo.toml`** (the plan notes a deterministic counter is also acceptable — the token is an intra-process replay check, not a security boundary).
- **3.2** `modem_get_status` + `modem_ardop_disconnect` Tauri commands in `modem_commands.rs` — landing in the existing `src-tauri/src/modem_commands.rs` (the module created in Task 2.2). Registers both in `lib.rs`'s `invoke_handler!`. Wires `Arc<ModemSession>` as `tauri::State`.
- **3.3** `modem_ardop_connect` — **the RADIO-1 surface**. Consent-gated. Spawns ardopcf via `ArdopTransport::with_managed_modem(cfg)`. Translates `ArdopUiConfig` (frontend-shaped) into `ArdopConfig.extra_args: Vec<String>` (backend-shaped). See "**RADIO-1 consent semantics**" below.
- **3.4** `ModemStatusBroadcaster` — background thread (std::thread, not tokio — per ADR 0015 and the ARDOP MVP plan's sync+threads directive) that emits `modem:status` Tauri events every 250 ms. Spawned at app startup in `lib.rs`'s `.setup()`.

**Phase 4 — frontend dock (3 tasks).** `ArdopDock` component stopped → running states, `AppShell` conditional 4-col grid swap, `ArdopHfStub` reading-pane stub when ARDOP HF is sidebar-selected but modem is stopped. The approved-during-brainstorm mockup is at `docs/superpowers/specs/2026-05-30-ardop-hf-ui-dock-active.png` — that's the design target.

**Phase 5 — Settings ARDOP section (1 task).** Extend `SettingsPanel.tsx` with fields for binary / capture / playback / PTT / cmd port.

**Phase 6 — RADIO-1 consent UI (2 tasks).** `useConsent` hook + `ConsentModal` + wires the dock's Connect button → mint token via `modem_mint_consent` → `modem_ardop_connect`. The plan's Task 6.2 has a critical note (also restated below) that the token MUST be backend-minted, not frontend-generated.

**Phase 7 — integration tests + Codex adrev + PR (5 tasks).** End-to-end integration test, full test/lint/clippy, **3 rounds of Codex adversarial review** on the implementation diff (RADIO-1 surface; concurrency/mutex correctness; error handling/recoverability), operator on-air smoke prep prose for the PR body, open the implementation PR.

---

## 4. ⚠️ RADIO-1 consent semantics — critical for Phase 3 Task 3.3 + Phase 6

The plan's Task 6.2 surfaced one spec gap that the implementer needs to get right:

> The consent token MUST be **minted on the backend** (via a Tauri command like
> `modem_mint_consent` that calls `session.mint_consent_token()` and returns the
> string), not generated by the frontend. If the frontend generates the token,
> the gate is theater — a frontend bug or compromised renderer can self-mint and
> pass it to the backend, bypassing the consent modal entirely.
>
> Correct flow:
>
> 1. Operator clicks Connect → consent modal appears.
> 2. Operator ticks the acknowledgement → frontend invokes `modem_mint_consent`.
> 3. Backend mints the token, stores it on the `ModemSession`, returns it.
> 4. Frontend stores the token in the `useConsent` hook for the session.
> 5. Frontend invokes `modem_ardop_connect({ target, consentToken })`.
> 6. Backend validates `session.has_valid_token(consentToken)` before any TX.
>
> Stopping the modem (`modem_ardop_disconnect`) clears the in-process token via
> `session.clear_consent_token()`. The next Connect re-prompts.

**Phase 3 Task 3.3's `modem_ardop_connect` must reject any call whose token doesn't match.** Phase 6 Task 6.2 adds the `modem_mint_consent` Tauri command.

---

## 5. Sensible deviations the implementer subagents have made (carry these forward)

The plan's code blocks were directionally right but assumed some things the codebase doesn't have. Each was flagged + accepted by the spec+code-quality reviewers. The next session should expect similar small adaptations on the upcoming tasks:

1. **`serde_json` instead of `toml`** for config persistence — the codebase uses JSON, not TOML. Test names + assertions adapted accordingly.
2. **`config::read_config()` / `config::write_config_atomic()`** are the real function names (plan placeholders were `config::load()` / `config::save()`). Match the existing `config_set_privacy` / `config_set_connect` pattern in `ui_commands.rs`.
3. **`Config` lacks `Default`** — it has required `schema_version`. Adding a new optional field to `Config` means updating every `Config { ... }` literal across the codebase to include the new field (`= None`). Task 2.1 touched 6 files for this reason; future additions will too if they extend `Config`.
4. **Rust 2024 edition lints make `std::env::set_var` unsafe** — wrap in `unsafe { }` with a SAFETY comment explaining the single-threaded test context.
5. **Tests need `TempDir` + `TUXLINK_CONFIG_DIR` env-var override** to isolate from the operator's real config. The mechanism is honored by `config.rs` on `main` (per `tuxlink-efo`).
6. **`config_set_ardop` returns an error pre-wizard** (no config file yet to read). Settings → ARDOP form should only be reachable post-wizard, so this is fine — but the consent modal flow in Phase 6 should not assume the command always succeeds.
7. **Module ordering in `lib.rs`:** `pub mod modem_status` and `pub mod modem_commands` are appended out of alphabetical order. The code-quality reviewer of Task 1.1 flagged this as Minor; it's a cosmetic style issue, not blocking. Resolve when next touching `lib.rs` for Phase 3.

---

## 6. Sibling in-flight work (DON'T merge over)

- **PR [#144](https://github.com/cameronzucker/tuxlink/pull/144)** — v0.2.0 currency README + Pat-strip + dock-on→dock-off mock swap. **Open, unmerged.** Worktree at `worktrees/bd-tuxlink-8j8-v02-currency`, branch `bd-tuxlink-8j8/v02-currency`. bd issue `tuxlink-8j8` in_progress. Operator-pending review. **Not a dependency for `tuxlink-4ek`**, but if you rebase #147 on `main` later, do it AFTER #144 lands to avoid touching the same mockup files.
- **bd follow-up issues filed this session:** `tuxlink-02h` (P3, radio-dock pane impl), `tuxlink-7gb` (P2, dev-docs Pat → native cutover refresh), `tuxlink-cyt` (P3, backend Pat code strip). All open, unscheduled.
- **PR [#140](https://github.com/cameronzucker/tuxlink/pull/140)** — Mac glyph fix from the mock HTMLs. **Already merged 2026-05-30 03:28 UTC.** Reference only.
- **Other live worktrees** (per `git worktree list`): the pre-existing `bd-tuxlink-*` worktrees from prior sessions are untouched this session. No coordination needed.

---

## 7. Operator action items / pending decisions

- **Codex quota on this Pi.** Memory `codex_quota_gotcha` notes ChatGPT-auth has a daily limit. Phase 7 Task 7.3 runs **3 Codex review rounds** on the diff. If quota is hit mid-round, defer the remaining rounds to the next adrev phase rather than substituting Claude (per the cross-provider-adrev discipline).
- **`rand` crate add to `src-tauri/Cargo.toml`.** Phase 3 Task 3.1 uses random hex chars for the consent token. If the next session prefers a deterministic counter (also acceptable — the plan flags this), no Cargo.toml change is needed. Either way, Task 3.1 should commit the choice.
- **ALSA device enumeration mechanism.** Plan-level open item (not blocking for Phase 3). Defaults to freeform string input in Phase 5; if a `arecord -l` / `aplay -l` shell-out is desired, that's a separate Phase 5+ tweak.
- **Whether to merge PR #144 before continuing Phase 3.** Not required, but doing it now keeps the branch list tidier. Operator's call.
- **`tuxlink-eh7` wizard-dead-end bug** is still open (P1). Operator on-air smoke (Phase 7 Task 7.4) won't be able to reach the dock without either the eh7 fix landing first OR a manual restart of the app post-wizard. Plan calls this out; operator may want to fix `eh7` before the on-air smoke.

---

## 8. Files this session changed (in the worktree)

```
src-tauri/src/modem_status.rs     | + (new file, Task 1.1; later phases extend it)
src-tauri/src/modem_commands.rs   | + (new file, Task 2.2; Phase 3 extends it heavily)
src-tauri/src/lib.rs              | M (mod declarations + invoke_handler additions)
src-tauri/src/config.rs           | M (ArdopUiConfig struct + Config.modem_ardop field + tests)
src-tauri/src/wizard.rs           | M (Config literal site updated for new field)
src-tauri/src/bootstrap.rs        | M (Config literal site updated)
src-tauri/src/position/mod.rs     | M (Config literal site updated)
src-tauri/src/ui_commands.rs      | M (Config literal sites updated × 2)
src-tauri/src/winlink_backend.rs  | M (Config literal sites updated × 2)
src/modem/types.ts                | + (TS wire mirror)
src/modem/types.test.ts           | + (2 tests)
src/modem/useModemStatus.ts       | + (React hook)
src/modem/useModemStatus.test.ts  | + (3 tests)
src/connections/sessionTypes.ts   | M (added 'ardop-hf' to ProtocolId + cms intent)
src/connections/sessionTypes.test.ts | M (2 new tests)
docs/superpowers/specs/2026-05-30-ardop-hf-ui-design.md  | + (the spec, committed via PR #147)
docs/superpowers/specs/2026-05-30-ardop-hf-ui-dock-active.png | + (approved mockup)
docs/superpowers/plans/2026-05-30-ardop-hf-ui-plan.md    | + (the plan, this is what the next session executes)
```

Total: ~9 commits since branch creation. Worktree HEAD `aa32b65` is the last task commit; the handoff commit will be next.

---

## 9. Next-session starting prompt

(See the operator-paste block in the controller's final message of this session.)
