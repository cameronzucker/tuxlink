# Handoff — 2026-05-30 — magpie-grouse-shoal — Pat strip + native B2F attachments (tuxlink-9phd) — spec/plan done + Phase 0 + Phase 1 complete + Phase 2 3/4 done

> Date: 2026-05-30 · Agent: magpie-grouse-shoal · bd: tuxlink-9phd · Machine: pandora · Worktree: `worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments/`

## 0. TL;DR

Full BRF spec/plan cycle done (5 spec adrev rounds → 3 plan adrev rounds → rev-3 spec + rev-2 plan). Subagent-driven execution underway. **14 of ~40 task commits landed and pushed**. Phase 0 (MailboxFolder move) complete; Phase 1 (compose-with-files + serializer) complete with wl2k-go golden vector passing byte-for-byte; Phase 2 (parser-with-files) is 3/4 done. Session ends at low context (~30% remaining); next session picks up at Phase 2 T2.4 and continues through Phases 3–12 + final Codex round + PR.

## 1. Branch state

- **Branch:** `bd-tuxlink-9phd/strip-pat-add-native-attachments` (off `origin/main` HEAD `9927a62`)
- **Latest commit:** `baeb8d7` (T2.3 — from_bytes parses File: + attachments)
- **Worktree status:** clean (verified via `git status` after T2.3 push)
- **Push state:** all commits pushed to `origin/bd-tuxlink-9phd/strip-pat-add-native-attachments`
- **Test count:** 539 passed, 0 failed, 4 ignored (baseline pre-Phase-0 was ~510; +29 new tests across Phase 0+1+2-partial)

## 2. Commits to date (newest first)

Code commits (14):

```
baeb8d7 feat(winlink): from_bytes parses File: headers + attachment bytes    # T2.3
9114fb2 feat(winlink): ParseError gains attachment variants + #[non_exhaustive]  # T2.2
d948b8d fix(winlink): from_bytes preserves repeated File/To/Cc headers          # T2.1
92402c5 test(winlink): multi-recipient + multi-attachment combined smoke         # T1.11
8a1e1a5 fix(winlink): canonicalize header keys in to_bytes sort + emit           # T1.10
6d01377 test(winlink): byte-identical conformance against wl2k-go fixture        # T1.9 — GOLDEN VECTOR
8bd792c feat(winlink): RFC 2047 Q-encode non-ASCII filenames                     # T1.8
69a5f83 test(winlink): assert to_proposal size includes attachment bytes + CRLFs # T1.7
2aa1aad test(winlink): assert attachment order + zero-attachment degeneracy      # T1.6
6200424 feat(winlink): Message::to_bytes emits File: headers + attachment bytes  # T1.5
ffb008a feat(winlink): compose_message_with_files attaches files to Message      # T1.4
3a140eb feat(winlink): validate attachment filenames in compose                  # T1.3
9349e27 feat(winlink): add attachments field to Message struct                   # T1.2
e30b661 feat(winlink)!: add ComposeError + compose_message_with_files skeleton + drop OutboundAttachment.content_type  # T1.1
09ae0bc refactor(winlink): move MailboxFolder enum out of pat_client (P0/P12)    # T0.1
```

Docs commits (3) — for spec + plan:

```
8f1df89 docs(plan): Pat strip + native B2F attachments — plan rev-2 (post-plan-review R1-R3)
3bd8065 docs(plan): Pat strip + native B2F attachments — implementation plan rev-1 (tuxlink-9phd)
68c35d9 docs(spec): Pat strip + native B2F attachments — rev-3 (post-Codex R5)
8ee9ea0 docs(spec): Pat strip + native B2F attachments — rev-2 (post-Claude R1–R4)
1ee1057 docs(spec): Pat strip + native B2F outbound w/ attachments — design rev-1 (tuxlink-9phd)
```

The 14 code commits each have an `Agent: magpie-grouse-shoal` trailer for grep-discoverability.

## 3. What's done

### Spec + plan (3-rev spec, 2-rev plan)

- **Spec:** [`docs/superpowers/specs/2026-05-30-pat-strip-native-attachments-design.md`](../superpowers/specs/2026-05-30-pat-strip-native-attachments-design.md) (rev-3, commit 68c35d9)
- **Plan:** [`docs/superpowers/plans/2026-05-30-pat-strip-native-attachments-plan.md`](../superpowers/plans/2026-05-30-pat-strip-native-attachments-plan.md) (rev-2, commit 8f1df89)
- **Adrev transcripts (gitignored):** `dev/adversarial/2026-05-30-pat-strip-native-attachments-claude-r{1,2,3,4}-*.md` (spec Claude rounds), `…-codex-r5.md` (spec Codex round), `…-plan-r{1,2,3}-*.md` (plan rounds)

### Phase 0 — Foundation (1/1 complete)

- T0.1: `MailboxFolder` enum moved from `pat_client.rs` to `winlink_backend.rs`. Re-export removed. Transitive import updates in `wizard.rs`, `bin/live_cms_smoke.rs`, `tests/pat_client_test.rs`. `as_path` widened private→`pub(crate)` for cross-module access. Pre-existing `modem_ardop: None` test-fixture fixes also cleared. **Two-stage review: spec ✅ + code-quality ✅.**

### Phase 1 — Compose-with-files + serializer (11/11 complete)

- T1.1: `ComposeError` + skeleton fallible `compose_message_with_files` + **delete `OutboundAttachment.content_type`** (BREAKING). **Two-stage review: spec ✅ + code-quality ✅** (minor doc nits accepted).
- T1.2: `Message.attachments: Vec<OutboundAttachment>` field + accessor + `pub(crate) fn set_attachments`. Also bumped `OutboundAttachment` derives to add `PartialEq, Eq` (transitively required). Controller spec + code-quality subagent ✅.
- T1.3: Filename validation in compose (255-char cap + Latin-1 encodability + first-invalid short-circuit). Controller review ✅.
- T1.4: Compose wires `attachments` into Message via `set_attachments`. Controller review ✅.
- T1.5: `Message::to_bytes` emits `File:` headers + body+CRLF+(attachment+CRLF)* tail. `set_attachments` now also synthesizes `File:` headers (idempotent re-population). Controller review ✅.
- T1.6: Multi-attachment ordering + zero-attachment degeneracy tests. Passed first-run (T1.5 was correct). Controller ✅.
- T1.7: `to_proposal` size includes attachments. Passed first-run (existing impl already used `to_bytes().len()`). Controller ✅.
- T1.8: RFC 2047 Q-encoding (charset=ISO-8859-1, lowercase `q`) for non-ASCII filenames. Controller ✅.
- T1.9: **GOLDEN VECTOR TEST — passed byte-for-byte against wl2k-go's `LPE5NXDVLVSQ.b2f` fixture** (31380 bytes, 104-byte Latin-1 body + 31028-byte JPG attachment). Strongest correctness signal of Phase 1. Vendored fixture + MIT LICENSE at `src-tauri/tests/fixtures/wl2k-go/`. `set_attachments` widened `pub(crate)→pub` so integration test can construct Message directly. Controller ✅.
- T1.10: Header sort canonicalization (`mid`→`Mid`, `content-type`→`Content-Type`). Golden vector still passes (its inputs were already canonical). Controller ✅.
- T1.11: Multi-recipient + multi-attachment combined smoke test. Passed first-run. Controller ✅.

### Phase 2 — Parser-with-files (3/4 complete)

- T2.1: Parser `from_bytes` uses `add_header` for repeatable headers (File/To/Cc) instead of `set_header` which overwrites. Codex R5 P1 fix. Controller ✅.
- T2.2: `ParseError` gains `MalformedFileHeader`, `MissingAttachmentTerminator`, `TruncatedAttachment` variants + `#[non_exhaustive]`. Controller ✅ (controller-direct edit; trivial enum addition).
- T2.3: `from_bytes` parses `File:` headers + attachment bytes. Closes the silent-data-loss bug (Plan R4 P0-1) — mailbox round-trip now preserves attachments end-to-end. Controller ✅.

### Test count growth across phases

- Pre-Phase-0 baseline: ~510 passed
- After Phase 0 (T0.1): 522 (the move surfaced 3 pre-existing `modem_ardop: None` test-fixture failures that the implementer fixed inline)
- After Phase 1 (T1.1–T1.11): 537 (+15 new tests)
- After Phase 2 partial (T2.1–T2.3): 539 (+2 new tests)
- Current: **539 passed, 0 failed, 4 ignored**

## 4. What's in-progress / pending (the next session's work)

### Phase 2 — Parser (1/4 remaining)

- **T2.4: Round-trip + edge-case parser tests.** 5 tests covering: `from_bytes_round_trips_through_to_bytes` (multi-attachment round-trip via Message::to_bytes→from_bytes), `from_bytes_handles_empty_attachment` (File: 0 case), `from_bytes_errors_on_missing_attachment_terminator`, `from_bytes_errors_on_truncated_attachment`, `from_bytes_errors_on_malformed_file_header`. Plan rev-2 §Phase 2 Task 2.4 has the exact test code. These should mostly pass first-run (T2.3's impl covers them); if any FAIL, fix the impl.

### Phase 3 — Trait return-type tighten (1 task)

- **T3.1**: Change `WinlinkBackend::send_message` return from `Result<Option<MessageId>, BackendError>` to `Result<MessageId, BackendError>`. PatBackend impl wraps Pat's no-MID return as `MessageId::new("")` transitionally (PatBackend deleted in P9). Update `ui_commands.rs:613-664` callers. Update test mocks. Breaking change — `feat(backend)!:` commit subject with `BREAKING CHANGE:` footer.

### Phase 4 — NativeBackend wires attachments + session observability (4 tasks)

**REWRITTEN in plan rev-2 with verified API shapes.** Critical references (verified during planning):
- `winlink/proposal.rs:81-90` — `Answer` is `Accept { resume_offset }`, `Reject` (UNIT), `Defer` (UNIT) — NOT field-form
- `winlink/session.rs:209` — `send_turn<R: BufRead, W: Write>` is SYNC, no log sink param today
- `winlink_backend.rs:442` — `pub type WireSink = Arc<dyn Fn(&str) + Send + Sync>` is the canonical wire-log mechanism
- `winlink_backend.rs:1202` — `wire_log: &dyn Fn(&str)` is how existing call-sites thread it
- For FS-reject MID: `send_turn` line 259-265 already collects rejected MIDs into `outcome.rejected: Vec<String>` — caller maps to `BackendError::MessageRejected`

Tasks:
- **T4.1**: NativeBackend::send_message passes attachments through `compose_message_with_files(...)?` (per spec §4.3). Maps `ComposeError`→`BackendError::MessageRejected`. Returns `MessageId` (post-T3.1).
- **T4.2**: Thread `wire_log: Option<&dyn Fn(&str)>` parameter through `send_turn` + `run_exchange` + `run_exchange_with_role`. Emit on FC EM send + FS receive.
- **T4.3**: Map `SendOutcome.rejected` to `BackendError::MessageRejected(format!("CMS rejected mid(s): ..."))` at the caller of run_exchange.
- **T4.4**: Update two-native-backends exchange test for attachments. Use `MessageBody.raw_rfc5322` (not `received.raw` — that was a rev-1 fictional field name).

### Phase 5 — Flip install sites (4 tasks)

- **T5.1**: Add `NativeBackend::test_fixture()` factory. Reuse existing `native_test_config()` at `tests/winlink_backend_test.rs:208` (do NOT invent a new `test_config` — that's a plan-rev-2 correction). `tempfile` is already a regular dep. `IdentityConfig` (not `Identity`) has no `Default`.
- **T5.2**: Replace `PatBackend::from_url("http://127.0.0.1:9")` at `app_backend.rs:161,173,219` with `NativeBackend::test_fixture()`.
- **T5.3**: `bootstrap.rs` — drop `resolve_pat_binary` + `PatBackend::spawn` + spawn thread; install `NativeBackend::new(config, mailbox_root)` synchronously.
- **T5.4**: `wizard.rs` test-send → connect-only NativeBackend probe (rename button "Send Test Message" → "Verify CMS Connection"). **Frontend cascade**: also update `TestSendOutcome` discriminated union + reducer + 3+ test files in `src/wizard/` (plan R2 P0-7 flagged the cascade scope).

### Phase 6 — LogSource::Pat merge (1 task)

- **T6.1+6.2**: Remove `LogSource::Pat` variant (merge into existing `::Backend`). Update emit-sites at `winlink_backend.rs:1408,1423`. Frontend: drop `'pat'` case in `src/wizard/logProjection.ts:30` + `logProjection.test.ts:330`.

### Phase 7 — Keyring service-name migration (3 tasks)

- **T7.1**: New `src-tauri/src/winlink/credentials.rs` module with `read_password(callsign)` helper. Tries `"tuxlink"` first; falls back to `"tuxlink-pat"` and migrates on first hit. Tests need isolation — plan rev-2 §Rev-2 known residuals suggests injecting an Entry-factory closure so tests use a mock instead of writing to real OS keyring.
- **T7.2**: Replace 4 keyring sites: `winlink_backend.rs:994,1219`, `wizard.rs:186`, `bin/native_cms_probe.rs:43`. (NOT `bin/live_cms_smoke.rs:45` — that file is deleted in P9.)
- **T7.3**: Update `src/wizard/Step2Credentials.tsx:84` user-facing string (`secret-tool delete service tuxlink-pat` → `service tuxlink`).

### Phase 8 — Config field deprecation (1 task)

- **T8.1**: `Config.pat_mbo_address` stays `pub` (per Codex R5 P1 — making it private breaks `Config { ... }` literals at 10+ sites). Add `#[serde(default, skip_serializing)]` + `#[deprecated(note = "...")]`. **Cascade**: every `Config { ... }` literal site needs `#[allow(deprecated)]` guard or the build fills with warnings. Grep `pat_mbo_address:` across src/ + tests/ to enumerate.

### Phase 9 — Delete Pat module + test surgery (SINGLE SUBAGENT DISPATCH)

Per plan rev-2's structural P0 fix: tasks 9.1-9.7 must be ONE subagent dispatch (not 7) because each sub-step leaves the tree in an uncompilable state until the final commit. The subagent uses internal TodoWrite sub-steps:
- 9.1: `git rm` pat_client.rs, pat_config.rs, pat_process.rs
- 9.2: Delete `PatBackend` + `PatBackendSpawnOptions` from winlink_backend.rs
- 9.3: `git rm` the 3 pat_*_test.rs files
- 9.4: **Partial edit** `tests/winlink_backend_test.rs` — remove 22 PatBackend hits (~8-12 test functions)
- 9.5: **Partial edit** `tests/ui_commands_test.rs` — remove 10 PatBackend hits (~3-5 test functions)
- 9.6: Delete `src/bin/live_cms_smoke.rs` + Cargo.toml `[[bin]] live_cms_smoke` entry (lines 64-65)
- 9.7: Verify `cargo build && cargo test` workspace green → ONE commit

### Phase 10 — Delete sidecar infra (5 tasks)

- T10.1: `tauri.conf.json` — remove `"externalBin": ["sidecars/pat"]`
- T10.2: `build.rs` — delete Go-toolchain check + go-build + 0-byte-stub creation
- T10.3: Delete `src-tauri/sidecars/` directory + `src-tauri/build_support.rs`
- T10.4: `.github/workflows/release.yml` — remove `setup-go` + Pat-sidecar-build steps
- T10.5: Verify clean release build

### Phase 11 — Submodule deinit (1 task)

- T11.1: Inventory `external/tuxlink-pat` (`git stash list` etc.) → `git submodule deinit -f external/tuxlink-pat` → `git rm external/tuxlink-pat` → remove `[submodule "external/tuxlink-pat"]` from `.gitmodules`. Document `.git/modules/external/tuxlink-pat/` orphan-cleanup in PR body.

### Phase 12 — Docs + ADR sweep (6 tasks)

- T12.1: ADR 0003 — append `superseded by ADR 0016` to existing Status line
- T12.2: ADR 0011 — same pattern
- T12.3: **Write ADR 0016** "Native B2F outbound with attachments; Pat removed" — full ADR per spec rev-3 §6.3 outline. Read ADR 0014 (`docs/adr/0014-clean-sheet-modem-no-prior-art-examination.md`) for rigor template.
- T12.4: Revise HTML Forms spec (`docs/superpowers/specs/2026-05-30-html-forms-design.md`) rev-2 → rev-3 — remove Path A (Pat REST) reasoning; point at native attachments now available.
- T12.5: `docs/install.md` + `docs/development.md` — drop Pat / Go-toolchain references
- T12.6: `VERSIONING.md` — drop "bundled-Pat compatibility break" row; `README.md` — drop Pat in architecture overview if present

### Final reviewer dispatch + PR open

After Phase 12: dispatch ONE parent-level Codex round against the full branch diff vs main per `feedback_codex_post_subagent_review`. Apply any P0/P1 findings. Then `gh pr create --base main --head bd-tuxlink-9phd/strip-pat-add-native-attachments` per the PR-body template in plan rev-2 §Final Reviewer Dispatch.

## 5. Worktree state at session end

Single worktree touched by this session:

| Worktree | bd issue | PR | State |
|---|---|---|---|
| `worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments/` | tuxlink-9phd | not opened yet (deferred to post-execution) | **LIVE — Phase 2 3/4 done. Working tree CLEAN. 14 code commits + 5 docs commits all pushed to `origin/bd-tuxlink-9phd/strip-pat-add-native-attachments`.** |

**Untracked state in the worktree at end-of-session:** none. Last `git status --short` returned empty after T2.3 commit.

**Gitignored-stateful content in the worktree** (per ADR 0009 enumeration discipline):
- `dev/adversarial/` — 8 adrev transcripts from the spec + plan adrev rounds (gitignored; local-only per CLAUDE.md)
- `src-tauri/target/` — cargo build artifacts from 14 incremental builds (substantial; the operator may want to `rm -rf src-tauri/target/` post-merge per `feedback_shared_cargo_target_dir`)
- `src-tauri/tests/fixtures/wl2k-go/` — TRACKED (vendored fixture + LICENSE for T1.9 golden test)

**Other worktrees on the machine:** unchanged from earlier sessions; this session created no new worktrees + disposed none.

## 6. Memory + bd state

- **bd tuxlink-9phd:** `in_progress`, owned by Cameron Zucker. Stays in_progress until final PR merges. Use `bd close tuxlink-9phd` at merge.
- **bd tuxlink-v1p (HTML Forms):** blocked on tuxlink-9phd via existing `bd dep`. When the strip PR merges, run `bd update tuxlink-v1p --remove-blocker tuxlink-9phd` to clear the gate.
- **Memory updates this session:** none. The two pre-existing memories (`project_pat_complete_strip_directive_2026_05_30` + `project_pat_fully_replaced_native_client`) remain accurate; this PR is the canonical implementation of the directive they captured.

## 7. Operator-facing decisions surfaced this session

1. **CPU load on the Pi** (during T0.1's 33-min cargo workspace build): operator chose "continue as-is." Subsequent incremental builds were ~1-3 min each — much more tolerable. Subagent-driven execution remains the chosen approach.
2. **Subagent commit-blocking by lease-race hook** (starting T1.4): worked around by having subagents do code+tests but NOT commit; controller commits from this (lease-holder) session. This pattern stabilized — every subsequent task used it without issue.

## 8. Notable observations / gotchas for next session

1. **Subagent-spec-review pattern shifted** mid-session: for trivial mechanical changes (1 file, <30 LOC, follows exact plan text), I switched to controller-side spec review + subagent code-quality review only. For larger changes I dispatched both subagents. This batched workflow saved many subagent dispatches without losing review independence. Next session can adopt the same pattern; it's not codified in the SDD skill but works in practice for the per-task scope sizes Phase 1-12 produces.

2. **Pi CPU load** during workspace builds: T0.1 was 33 minutes (cold build); incrementals are 1-3 minutes. Phase 4+5 will touch winlink_backend.rs + session.rs + bootstrap.rs — possibly some recompile cost but still incremental. Phase 9's mass-delete commit will require a clean cargo check (no behavior change, just symbol removals; should be fast post-deletion).

3. **The `set_attachments` pub(crate)→pub change in T1.9** is a real API loosening. It's defensible (`OutboundAttachment` is `pub` so validation was always convenience-not-guarantee), but if a future operator wants tighter visibility, the alternatives are: (a) move the golden test inside the lib as a unit test (`#[cfg(test)] mod`), or (b) add a `#[cfg(test)] pub fn set_attachments_for_testing` wrapper. Neither was worth the friction now.

4. **Plan rev-2 "Rev-2 known residuals" addendum** lists per-task tightenings that subagents handle inline via the verify-before-coding discipline. Most have been irrelevant during Phase 0-2 (the residuals were heavier for Phase 4-8). Next session: re-skim that addendum before each Phase 4+ task.

5. **CPU on the Pi** dropped after T0.1; subsequent tasks pegged it less. Operator may want to leave the dev-Pi alone during Phase 4-5 (the install-site flip touches enough files that the rebuild may be substantial again).

6. **No regressions across 14 commits.** All `cargo test --workspace` runs since T0.1 have been green. The golden vector test (T1.9) and parser round-trip (T2.3) are the strongest forward correctness signals.

## 9. Plan revision discipline reminder

Per the standard task preamble in plan rev-2: **subagents verify every API shape before patching** (grep for type, fn, method, enum-variant before writing code; if anything doesn't match the plan's snippets, surface the discrepancy first). This is the primary mitigation for the residual "fictional API claim" findings from plan review R1-R3 that weren't fully patched in plan rev-2.

For Phase 4 especially: the rev-2 rewrite of Phase 4 grounds everything in verified API shapes, but the executing subagents should STILL re-grep before patching session.rs, proposal.rs, and winlink_backend.rs — these are the most-modified files of the PR.

---

Agent: magpie-grouse-shoal
