# Handoff — 2026-05-31 — shoal-isthmus-swallow — Pat strip + native B2F attachments — PR #175 open, awaiting operator review

> Date: 2026-05-31 · Agent: shoal-isthmus-swallow · bd: tuxlink-9phd · Machine: pandora · Worktree: `worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments/` · **PR:** https://github.com/cameronzucker/tuxlink/pull/175

## 0. TL;DR

Resumed the magpie-grouse-shoal session at Phase 2 T2.4 and chipped the full backlog end-to-end overnight under operator-confirmed autonomous-execution posture. **All 13 implementation phases shipped (Phase 2 T2.4 → Phase 12), main was merged in, Codex cross-provider review surfaced 4 P1 + 2 P2 findings — all resolved — and PR #175 is open against main.** 22 commits added this session (21 task commits + 1 merge); branch total is 41 commits ahead of main. Operator action required: review + merge.

## 1. Branch + PR state

- **Branch:** `bd-tuxlink-9phd/strip-pat-add-native-attachments`
- **PR:** [#175](https://github.com/cameronzucker/tuxlink/pull/175) — open, base `main`
- **Latest commit:** `af71aa6` (Codex P1.2-P2.2 fixes)
- **Branch state:** clean; all commits pushed
- **Test count post-merge + Codex-fixes:** **626 passed, 0 failed, 3 ignored** across the full workspace (528 lib + 98 across integration bins)
- **Frontend:** `npx tsc --noEmit` clean; `npx vitest run src/wizard` 119 passed; deps installed in worktree (`node_modules/` present)

## 2. Commits this session (newest first)

```
af71aa6 fix: apply 5 Codex post-impl review findings (P1.2-P1.4, P2.1-P2.2)
7f5e913 chore: merge origin/main into bd-tuxlink-9phd
cfaf8b1 docs: ADR sweep + HTML Forms rev-3 + setup docs (P12)
32b64cf build!: deinit + remove external/tuxlink-pat submodule
ba6da3a build!: delete Pat sidecar infrastructure
3d0536f feat(backend)!: delete Pat module + Pat tests + live_cms_smoke
db6367a feat(config): deprecate pat_mbo_address field
5a64de2 refactor: keyring sites + UI hint use 'tuxlink' service name
ef97482 feat(winlink): credentials module with keyring migration
e5178ad refactor(backend,wizard): merge LogSource::Pat into ::Backend
d186e58 feat(wizard)!: replace test-send with verify-CMS-connection
1bc5f54 refactor(bootstrap): drop Pat sidecar resolution + spawn-drain plumbing
53a40b7 refactor(app_backend): test fixtures switch to NativeBackend
882c45c test(backend): add NativeBackend::test_fixture() factory
3fd0a76 test(backend): two-backend end-to-end exchange with attachment
af47452 feat(backend): map SendOutcome.rejected to BackendError::MessageRejected
0ba8737 feat(session): thread wire_log through send_turn + run_exchange
5450964 feat(backend): NativeBackend::send_message pipes attachments to compose
0b810ee fix(compose): drop stale string|null return type from message_send invoke
1101d69 feat(backend)!: WinlinkBackend::send_message returns MessageId not Option
47b12fd test(winlink): from_bytes round-trip + edge cases
```

Plus the prior magpie session's 19 commits (14 code + 5 docs/handoff) bring the branch to 41 commits ahead of main.

## 3. What's done (post-magpie continuation)

### Phase 2 — completed T2.4 (5 regression-lock tests)
Round-trip + 4 edge-case parser tests in `src-tauri/src/winlink/message.rs`. First-run pass per plan rev-2's "tests born passing" expectation.

### Phase 3 — T3.1 (BREAKING)
`WinlinkBackend::send_message` returns `Result<MessageId, BackendError>` (was `Option<MessageId>`). Both impls updated; `ui_commands.rs` caller cleaned up. Plus a follow-up commit (`0b810ee`) caught the FE's `Compose.tsx` stale `invoke<string | null>` generic that the Rust contract change needed to bring along.

### Phase 4 — 4 tasks (NativeBackend wiring + session observability)
- T4.1: NativeBackend::send_message pipes attachments through compose
- T4.2: `wire_log: Option<&dyn Fn(&str)>` threaded through send_turn / run_exchange (plan said sites `:1202+1290`; actual were `native_packet_exchange` + `native_packet_connect`)
- T4.3: FS-rejected MIDs → `BackendError::MessageRejected` (later refined by Codex P1.4 to move accepted MIDs to Sent BEFORE returning the error)
- T4.4: end-to-end two-backend exchange test with attachment (loopback telnet + receiver decodes via Message::from_bytes — strongest correctness signal end-to-end)

### Phase 5 — 4 tasks (install-site flip)
- T5.1: `NativeBackend::test_fixture()` factory; duplicated `native_test_config()` across `src/test_helpers.rs` + `tests/winlink_backend_test.rs` because `#[cfg(test)] pub mod` isn't visible across integration-crate boundary
- T5.2: 3 PatBackend::from_url sites in app_backend.rs → NativeBackend::test_fixture()
- T5.3: bootstrap.rs lost 412 lines of Pat resolution + spawn-drain plumbing
- T5.4 (BREAKING): wizard test-send → verify-CMS-connection. **Massive FE cascade**: 1 Rust file + 11 TS files, -657 LOC net. `test_send` step → `cms_verify`; TestSendOutcome → simple ok/error states; menu model + reducer + types + 5 test files all updated atomically

### Phase 6 — LogSource::Pat merge
Backend (winlink_backend.rs + ui_commands.rs + 3 test files) + frontend (src/session/logProjection.ts — plan said `src/wizard/`; actual is `src/session/`).

### Phase 7 — keyring service-name migration
- T7.1: `winlink::credentials` module with factory-injected EntryLike — **NO real-OS-keyring writes in tests** (verified via test isolation review). Keyring 3.6.3's built-in mock builder uses EntryOnly persistence which can't cover migration semantics; factory + HashMap mock is the chosen pattern.
- T7.2+T7.3: 4 keyring sites (3 reads via credentials::read_password, 1 write via direct keyring::Entry with new service name); user-facing secret-tool hint updated.

### Phase 8 — Config field deprecation
`#[deprecated]` + `#[serde(default, skip_serializing)]` on `pat_mbo_address`. Cascade of `#[allow(deprecated)]` on 20 sites (14 write literals + 6 read sites). Test assertions in wizard_persist_cms_test + wizard_integration_test corrected from `Some(...)` to `is_none()` since skip_serializing means the field doesn't round-trip.

### Phase 9 (the big delete; single-shot subagent dispatch)
**-2941 / +103 LOC across 14 files.** Deletes: pat_client.rs (204), pat_config.rs (141), pat_process.rs (294), PatBackend + PatBackendSpawnOptions in winlink_backend.rs, 3 standalone pat_*_test.rs files (~669 LOC), live_cms_smoke.rs (382), 12 PatBackend tests in winlink_backend_test.rs, 6 PatBackend tests in ui_commands_test.rs. Kept: `now_iso8601_utc`/`days_to_ymd` (used by NativeBackend), `MailboxFolder` enum (already moved to winlink_backend.rs in T0.1), 6 historical comments as tombstones.

### Phase 10 — sidecar infra deletion
build.rs reduced from 211 lines to 3 (`tauri_build::build()`). Build_support.rs deleted. tauri.conf.json externalBin removed. release.yml setup-go + Pat-sidecar-build steps removed. sidecars/ directory removed.

### Phase 11 — submodule deinit
`git submodule deinit -f external/tuxlink-pat`, `git rm external/tuxlink-pat`, `git rm .gitmodules` (file emptied to 0 bytes by the prior rm; removed as cruft). Cargo clean post-deinit.

### Phase 12 — docs + ADR sweep
8 docs files. ADR 0003 + 0011 amended with "superseded by ADR 0016". **ADR 0016 NEW (144 lines)** — wire format inline reference, alternatives considered, watched failure modes, migration steps; modeled on ADR 0014's rigor template. HTML Forms spec rev-2 → rev-3. install.md / development.md / VERSIONING.md / README.md swept.

### Main-merge + Codex review + fixes

After Phase 12 shipped, ran the mandatory cross-provider Codex review (per `feedback_codex_post_subagent_review`). Codex returned 4 P1 + 2 P2 findings — all addressed before PR open per `feedback_no_carveout_on_cross_provider_adrev`:

- **P1.1** (version.txt regression from 81-commit drift): resolved by merging `origin/main` into the branch. 1 lib.rs conflict (search service registration vs bootstrap path) + 1 ardop B2F callsite + 3 InitConfig fixtures (new `arq_bandwidth_hz` field from main).
- **P1.2** (verify-CMS-connection could transmit queued mail): tempdir mailbox isolation.
- **P1.3** (keyring migration ran in failed-connect/Listen paths, polluting real OS keyring): defer `read_password` until after link-open at both packet + telnet sites.
- **P1.4** (mixed-FS rejection left accepted MIDs in Outbox to be re-offered next connection): move accepted MIDs to Sent BEFORE returning rejection error. Both production exchange callers patched + new regression test using FakeAx25Stream + scripted FS YN.
- **P2.1** (`message_send` IPC dropped attachments): new `OutboundAttachmentDto` + DTO bridge. FE passes `[]` for now (file-picker is HTML Forms PR #151).
- **P2.2** (CR/LF/NUL in attachment filenames could inject B2F headers): new `ComposeError::FilenameContainsControlChar` variant + 4 tests.

## 4. What's pending decision

**Nothing blocks PR review.** Operator-action items are routine post-merge tasks documented in the PR body's "Operator post-merge actions" section:
- Per-clone submodule cleanup (`git submodule deinit -f external/tuxlink-pat && rm -rf .git/modules/external/tuxlink-pat`)
- Browser smoke of wizard `cms_verify` step + Compose send (text-only; attachment picker is PR #151)
- Verify keyring migration on first run (or NoEntry on fresh install)
- Confirm release CI no longer needs Go

## 5. Worktree state at session end

| Worktree | bd issue | PR | State |
|---|---|---|---|
| `worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments/` | tuxlink-9phd | [#175](https://github.com/cameronzucker/tuxlink/pull/175) (open) | **LIVE — PR open; clean working tree; all commits pushed.** |

**Untracked state in the worktree at end-of-session:** none (post-merge commit cleared everything).

**Gitignored-stateful content in the worktree** (per ADR 0009 enumeration discipline):
- `dev/adversarial/` — 9 adrev transcripts (the prior 8 from magpie's spec/plan adrev rounds + the new Codex post-impl review at `2026-05-31-pat-strip-native-attachments-post-impl-codex.md` (37,473 lines, contains full diff + Codex source-inspection commands + final findings))
- `src-tauri/target/` — cargo build artifacts (substantial; operator may want `rm -rf src-tauri/target/` post-merge per `feedback_shared_cargo_target_dir`)
- `src-tauri/tests/fixtures/wl2k-go/` — TRACKED (vendored fixture + LICENSE for T1.9)
- `node_modules/` — TRACKED-as-ignored (operator installed via pnpm install during T3.1 follow-up tsc verification)

**Other worktrees on the machine:** unchanged from earlier sessions; this session created no new worktrees + disposed none.

## 6. Memory + bd state

- **bd tuxlink-9phd:** **`in_progress` until merge.** Updated with PR link via `bd update tuxlink-9phd --notes "..."`. The notes include the post-merge close-and-unblock recipe:
  ```
  bd close tuxlink-9phd
  bd update tuxlink-v1p --remove-blocker tuxlink-9phd
  bd dolt push && git push
  ```
- **bd tuxlink-v1p (HTML Forms):** blocked on tuxlink-9phd. When PR #175 merges, the recipe above clears it.
- **Memory updates this session:** none directly written; this handoff doc serves as the structured record.

## 7. Operator-facing decisions surfaced this session

1. **Autonomous-execution posture confirmed** mid-session (operator's "Yes" to "pick + commit + document for design ambiguities" — applied to: choosing factory injection over keyring::mock builder for credentials test isolation; choosing single-commit boundary for Phase 9 + Phase 10; doing P6 + T7.2+T7.3 as combined commits per magpie's batched-pattern note in §8 of the prior handoff).
2. **Branch-vs-main drift** surfaced via Codex P1.1; resolved via merge (not rebase — 81 commits is too much replay risk for an automated session).

## 8. Notable observations / gotchas for next session

1. **Cwd silently reverts to main checkout after subagent dispatches.** Hit this 6+ times this session. The `cd && command` pattern doesn't update the bash tool payload's cwd, so the lease hook denies. Mitigation that worked: standalone `cd <worktree>` Bash call FIRST, then the git op in a subsequent Bash call. Per memory `feedback_pin_paths_in_worktree_sessions` + `feedback_worktree_git_hook_cwd_and_mergebase`.

2. **Plan rev-2 had ~6 fictional API citations** that subagents caught via verify-before-coding: T4.2 wire_log site (plan said `:1202+1290`; actual was `native_packet_exchange`/`native_packet_connect`), T4.3 ExchangeResult shape (plan said `result.outcome.rejected`; actual was `result.rejected`), T6.2 logProjection path (plan said `src/wizard/`; actual was `src/session/`), T4.1 expected fictional `MessageBody.raw_rfc5322` field (actual was correct in the spec but plan summary was off), T3.1 caller location (plan said `ui_commands.rs:613-664`; actual was `:665-666` + ui_commands_test.rs was a fictional cite — that file had ZERO send_message references). All caught + adjusted in commit messages.

3. **The wizard cascade (T5.4) was bigger than plan understated.** Plan said "3+ test files"; reality was 11 TS files including menu model + reducer + types. Subagent's pre-grep enumeration caught the full set; -657 LOC net.

4. **Phase 9 single-shot deletion** worked exactly as plan rev-2 designed. Subagent used internal TodoWrite for 9.1-9.7 sub-tasks; produced one coherent commit with thorough decision tracking (12 deleted tests + 7 kept tests in winlink_backend_test.rs; 6 deleted + 16 kept in ui_commands_test.rs).

5. **Codex review was thorough and useful.** All 6 findings (P1.1-P1.4 + P2.1-P2.2) were real correctness/security issues — not nits. Most important were P1.4 (silent re-send of accepted MIDs on mixed-batch rejection) and P2.2 (B2F header injection via filename). Cross-provider review absolutely earned its keep.

6. **Tests went 510 → 626 over the full PR.** +116 net new tests across 22 commits, with significant deletions in Phase 9 (pat_*_test.rs files + ~18 PatBackend tests in winlink_backend_test.rs + ui_commands_test.rs).

7. **Browser smoke not done** (operator-deferred per `feedback_browser_smoke_before_ship`). UI changes are: wizard cms_verify step, Compose attachments bridge (FE passes [] for now), menu model rename. tsc + vitest cover the type-level correctness; visual fidelity is the operator's call.

## 9. Plan revision discipline reminder

For the operator reviewing the PR: the prior handoff (magpie's `2026-05-30-magpie-grouse-shoal-pat-strip-phase0-phase2-partial.md`) is also on this branch and remains valid context. The plan rev-2 + spec rev-3 are unchanged from then — only the implementation completed.

---

Agent: shoal-isthmus-swallow
