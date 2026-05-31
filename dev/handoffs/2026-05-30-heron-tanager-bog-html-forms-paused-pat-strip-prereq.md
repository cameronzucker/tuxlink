# Handoff — 2026-05-30 — heron-tanager-bog — HTML Forms paused mid-Phase-0; operator pivoted to "strip Pat completely"

> Date: 2026-05-30 · Agent: heron-tanager-bog · Machine: pandora · Operator decision: pivot to Path B + complete Pat strip; HTML Forms blocked behind new prerequisite

## 0. TL;DR

This session was started to execute the HTML Forms v0.1 plan (PR #151) via `superpowers:subagent-driven-development`. Two tasks landed (T0.1 + T0.2 — backend precursor commits, both path-agnostic and useful regardless of transport choice). Mid-execution, the operator caught a memory/architecture mismatch and pivoted:

> **"Pivot to Path B and ensure Pat is completely stripped. No Pat reliance, no Pat code. We need to not rely on it."**

The HTML Forms spec (rev-2) deliberately chose Path A (Pat REST) for v0.1 outbound because Path B (native B2F + attachments) didn't exist. The pivot promotes Path B to a prerequisite. The remaining HTML Forms plan (T0.3 onward) is invalidated until Path B is built.

**This session's execution is cleanly stopped.** T0.1 + T0.2 are committed and pushed to PR #151's branch — both add the path-agnostic `OutboundMessage.attachments: Vec<OutboundAttachment>` field and compile-fix the one caller. Phase 1+ of the HTML Forms plan stays valid for re-use after the Pat strip lands (forms backend module + frontend tasks are transport-agnostic). T0.3 (`send_with_attachments` on `pat_client`) and the routing portions of T3.1 are wrong-direction; the new bd issue's design phase will replace them.

The Pat strip + native B2F outbound is a substantial design problem (lzhuf compression, B2F `File:` headers, multi-recipient handling, replacing PatBackend at all install sites, deleting the sidecar bundle). Filed as `tuxlink-9phd` (P1, open) with full `superpowers:build-robust-features` discipline expected. HTML Forms (`tuxlink-v1p`) is marked dependent on `tuxlink-9phd` via `bd dep add`.

main HEAD: `2e7309d` (PR #149 merge — inventory rev-1; later merges happened but this worktree branched off main earlier).
PR #151 branch HEAD: `5012707` (T0.2 — caller compile fix).

---

## 1. What this session executed

### 1.1 Plan-ingest + setup
- Read the yew-cypress-oak 2026-05-30 handoff at `dev/handoffs/2026-05-30-yew-cypress-oak-html-forms-brf-handoff.md` (lives on the v1p worktree branch; not on main).
- Read the HTML Forms spec rev-2 (`docs/superpowers/specs/2026-05-30-html-forms-design.md`, 519 lines).
- Read the first 700 lines of the HTML Forms plan rev-3 (`docs/superpowers/plans/2026-05-30-html-forms-v0.1-plan.md`, 2828 lines total) — enough for T0.1 + T0.2 + T0.3 full task text.
- Mapped all 34 task boundaries via `grep ^### Task` (T0.1 through T11.1, no T11.2-T11.5 because the 4 live smokes are operator-driven, not subagent tasks).
- Built TodoWrite with all 34 tasks + final reviewer dispatch.
- Picked moniker `heron-tanager-bog` (3-word hyphenated per get_agent_moniker.py).

### 1.2 Task executions

**T0.1 — Add `OutboundAttachment` struct + extend `OutboundMessage`** (Sonnet implementer + Sonnet spec reviewer + Sonnet code-quality reviewer)
- Commit `3b236af` on branch `bd-tuxlink-v1p/html-forms-design`.
- Added `OutboundAttachment { filename: String, content_type: String, bytes: Vec<u8> }` at `src-tauri/src/winlink_backend.rs:91-95` (derives `Debug, Clone`).
- Extended `OutboundMessage` with `pub attachments: Vec<OutboundAttachment>` as field 6.
- Added test `test_outbound_message_carries_attachments` in `src-tauri/tests/winlink_backend_test.rs`.
- Per task-authorized scope, the implementer added `attachments: vec![]` to TWO pre-existing `OutboundMessage { ... }` literals in the same test file (lines ~70 and ~184).
- TDD ordering followed (test → fail → struct → pass).
- Spec compliance: ✅ all 8 checks pass.
- Code quality: ✅ Approved. One Important issue flagged but not blocking: missing `!` suffix + `BREAKING CHANGE:` footer on the commit message (release-please-relevant; commit was already pushed so amending was off-table per the destructive-git ban; surface to operator at merge time).

**T0.2 — Update `OutboundMessage` callers (compile fix)** (Sonnet implementer + Sonnet spec reviewer + Haiku code-quality reviewer)
- Commit `5012707` on branch `bd-tuxlink-v1p/html-forms-design`.
- Added single line `attachments: vec![]` to the `OutboundMessage` literal in `message_send` at `src-tauri/src/ui_commands.rs:660`.
- `cargo build --manifest-path src-tauri/Cargo.toml` finished clean (`Finished dev profile`).
- `cargo test --manifest-path src-tauri/Cargo.toml --tests -- --test-threads=1` — 510 passing across 16 binaries, 0 FAILED, 4 pre-existing ignored.
- Spec compliance: ✅ all 7 checks pass.
- Code quality: ✅ Approved (1-line trivial fix).

**Push:** both commits pushed to `origin/bd-tuxlink-v1p/html-forms-design` at `d6b6a54..5012707`.

### 1.3 The pivot — operator caught a stale memory + spec choice

Mid-dispatch of T0.3 (`send_with_attachments` on `pat_client`), the operator stopped the subagent and asked:

> "Why do I see Pat_client in this list and a broken CI pipe with anything to do with pat? I thought pat was removed from this project? What's up with that?"

Investigation:

- `src-tauri/src/pat_client.rs` is **still in HEAD** (9.7 KB, 10 fns, last meaningful touch May 20 `6095882 fix(pat): enforce read-side byte cap`).
- `bootstrap.rs:386` calls `PatBackend::spawn(...)` at every app startup.
- The only installed outbound send pipeline today is `PatBackend::send_message → PatClient::send` (multipart POST to Pat's REST `/api/mailbox/...`).
- The native CMS client at `src-tauri/src/winlink/` handles inbound + session-level work but does NOT yet support outbound with attachments.
- The May 20 commit `2da675b — docs(readme)+ui(compose): v0.2.0 currency — bump framing, strip Pat refs, drop phantom dock` removed Pat references from README + UI but left the underlying code Pat-backed.
- The CI URL the operator referenced (`runs/26697376499/job/78684083961`) was still IN PROGRESS, not failed — first CI run on PR #151, queued by the T0.2 push 4 minutes earlier.

**Memory was over-optimistic.** `project_pat_fully_replaced_native_client` (May 21) said "Pat is legacy/deprecated; native is canonical" — that was the goal-state, captured before the bootstrap cutover finished. The actual code state was mid-cutover (native exists for read; outbound still Pat).

**Spec author's reasoning (yew-cypress-oak, earlier today):** §5.1 picked Path A consciously: *"Build Path A first (works with the shipped Pat-via-REST send pipeline). Path B is a parallel work-item that lands when native Telnet send replaces Pat. The forms module is path-agnostic — it produces the (text body, xml bytes, filename) triple; the path-specific code handles transport."*

**Operator decision after surfacing this:** Pivot to Path B; complete the Pat strip; no new Pat code; no Pat reliance.

---

## 2. PR #151 status

- **Branch:** `bd-tuxlink-v1p/html-forms-design`
- **HEAD:** `5012707` (T0.2, pushed to origin)
- **CI:** the `build-linux` job on the latest push has been running ~5 minutes when this handoff was written; status `pending` at end of session.
- **PR body:** still the OLD "DESIGN ONLY" template from the 2026-05-30 morning open. It has NOT been updated to reflect the rev-2 design + rev-3 plan + the pivot to Path B. Operator can update at their discretion (or the next session can do it as a docs-only touch).
- **Operator-pending action:** post a comment on PR #151 noting that the PR is paused behind `tuxlink-9phd` (Pat strip prerequisite). Drafted text below in §4.4.

The 2 commits on this branch (T0.1 + T0.2) are path-agnostic and remain valid — they add the `OutboundMessage.attachments` field that the future native send path will use. They do NOT need reverting.

---

## 3. The new bd issue — `tuxlink-9phd`

**Title:** Strip Pat + add native B2F outbound with attachment support
**Priority:** P1
**Status:** open
**Dependency:** `tuxlink-v1p` (HTML Forms) is blocked on `tuxlink-9phd` (set via `bd dep add tuxlink-v1p tuxlink-9phd`).

**Scope per the bd issue description (paraphrased):**

1. Audit Pat surface area — every file referencing Pat (pat_client.rs, pat_config.rs, pat_process.rs, PatBackend impl, PatBackendSpawnOptions, bootstrap.rs:386 Pat-spawn, app_backend.rs Pat install sites, pat_client_test.rs, sidecar bundling, Settings UI, tauri.conf.json sidecar refs).
2. Build native outbound-with-attachments — needs design phase: B2F File: header encoding, lzhuf compression for attachment bodies, multi-recipient handling, error mapping, native path through `winlink::compose::Composer` + `winlink::transfer::Transfer`.
3. Replace PatBackend at install sites — bootstrap.rs + app_backend.rs install NativeBackend exclusively.
4. Delete Pat code once parity is reached.
5. Operator on-air smoke to verify post-cutover.

**Expected discipline:** full `superpowers:build-robust-features` cycle (brainstorm → spec → 5-round adrev cross-provider per memory `feedback_no_carveout_on_cross_provider_adrev` → plan → 4-round plan review → execute via `subagent-driven-development`).

---

## 4. Next session's starting instructions

The HTML Forms plan is paused. The next session's job is to design + execute the Pat strip + native B2F outbound prerequisite, then resume HTML Forms.

### 4.1 First action — DO NOT continue HTML Forms execution

The most-recent handoff hook will surface this file. The next session must NOT read the HTML Forms plan + start subagent-driven execution. The plan is invalidated at T0.3 and several later tasks.

### 4.2 Spawn the prerequisite work

1. Pick a moniker via `python3 .claude/scripts/get_agent_moniker.py`.
2. Read this handoff + the two key memories: `project_pat_complete_strip_directive_2026_05_30` and `project_pat_fully_replaced_native_client`.
3. `bd update tuxlink-9phd --claim` to take ownership.
4. Create a new worktree per the bd-issue-ownership rule: `python3 .claude/scripts/new_tuxlink_worktree.py tuxlink-9phd strip-pat-add-native-attachments` (or whatever the project's standard worktree-creation tool is) → `worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments/` on branch `bd-tuxlink-9phd/strip-pat-add-native-attachments`.
5. Invoke `superpowers:build-robust-features`. Brainstorming MAY be brief because the architectural decision is already made (Path B; strip Pat completely) — but the design phase has real substance: B2F `File:` header semantics, lzhuf, multi-recipient B2F send, NativeBackend feature-parity audit vs PatBackend, sidecar removal mechanics.
6. Per `feedback_no_carveout_on_cross_provider_adrev`: the 5-round adversarial review (Claude rounds + Codex round) IS mandatory for this work; this is not plumbing.

### 4.3 What stays valid in the HTML Forms plan (for after `tuxlink-9phd` lands)

When the Pat strip + native outbound lands, HTML Forms resumes against PR #151. Most of the existing plan still applies:

- **Drop:** T0.3 (pat_client send_with_attachments) — replace with the native equivalent that `tuxlink-9phd` will produce.
- **Revise:** T3.1 (`send_form` Tauri command) — route through native send, not Pat.
- **Keep unchanged:** Phase 1 (forms backend module — types, validation, parse, serialize, catalog, T1.1-T1.8), Phase 2 (detection bug fix + DTO extension), Phase 4-10 (frontend tasks + hardening cross-cuts), Phase 11 (Codex round + live smokes).

### 4.4 Drafted PR #151 comment for the operator

```
This PR is paused behind tuxlink-9phd (Strip Pat + add native B2F outbound
with attachment support). The two commits already on this branch (3b236af
+ 5012707, both path-agnostic OutboundMessage.attachments field + caller
compile fix) stay valid and don't need reverting. Forward progress on this
PR resumes after the Pat strip prerequisite lands.

See dev/handoffs/2026-05-30-heron-tanager-bog-html-forms-paused-pat-strip-prereq.md
for context.
```

(Operator post if/when desired; this session did not post automatically.)

### 4.5 Open follow-up — T0.1 commit hygiene

The T0.1 commit (`3b236af`) is missing the `!` suffix + `BREAKING CHANGE:` footer that release-please uses to detect breaking changes. The commit is already pushed and the destructive-git ban forbids `--amend` on pushed commits. Mitigation options:

- Operator can include `BREAKING CHANGE:` in the merge-commit message when PR #151 eventually merges (release-please reads merge-commit messages too in some configs).
- Or land a follow-up commit on this branch with an empty body but a `BREAKING CHANGE:` footer (cosmetic — release-please may or may not parse a stand-alone footer commit).
- Or accept the gap; the breaking change is acknowledged in prose in the T0.1 commit body and in the spec.

Not blocking; surface to operator at merge time.

---

## 5. Worktree state at session end

Worktrees touched by this session:

| Worktree | bd issue | PR | State |
|---|---|---|---|
| `bd-tuxlink-v1p-html-forms-design` | tuxlink-v1p | #151 (open, paused) | **LIVE — paused behind tuxlink-9phd. 2 commits ahead of main: 3b236af + 5012707. Working tree clean. Both commits pushed to origin.** No build artifacts left behind (didn't run `cargo build` in main checkout; subagents ran cargo from inside the worktree where `target/` lives — operator can clean with `rm -rf worktrees/bd-tuxlink-v1p-html-forms-design/src-tauri/target` later if disk hygiene calls for it; not urgent per `feedback_shared_cargo_target_dir` — 442 GB free). |

No new worktrees created in this session. The `worktrees/bd-tuxlink-9phd-...` worktree will be created by the next session per §4.2 step 4.

Untracked state in the v1p worktree at end-of-session: **none** in tracked-content terms; this handoff is the only new file and it's about to be committed.

---

## 6. Memory updates this session

Two memory mutations:

- **NEW:** `project_pat_complete_strip_directive_2026_05_30.md` — captures today's operator directive verbatim and the consequences for both HTML Forms and any future outbound work. Cross-links to `project_pat_fully_replaced_native_client` (May 21 mid-cutover state) and `project_fork_enables_aggressive_deletion`.
- **UPDATED:** `MEMORY.md` index — added the new memory's entry.

The May 21 `project_pat_fully_replaced_native_client` memory was NOT modified — it remains accurate as a point-in-time snapshot of the mid-cutover state. Today's memory supersedes it for forward decisions.

---

## 7. Other open work mentioned this session (not blocking forms)

(Unchanged from yew-cypress-oak's handoff §7; no new entries this session.)

- `tuxlink-wqv` (P3) — CLAUDE.md codex review syntax stale. Docs-only fix.
- BT page-timeout (`tuxlink-9ky`, P1) — still gates on-Pi radio work, unchanged.
- ARDOP MVP on-air bring-up — unchanged from `marten-finch-gorge` handoff; depends on tuxlink-9ky.

---

## 8. Adversarial transcripts / scratch (unchanged)

No new adrev rounds this session. The yew-cypress-oak handoff's §6 listing of 9 transcript files is still authoritative; all 9 are gitignored at `dev/adversarial/2026-05-30-html-forms-*.md` on this machine.

Agent: heron-tanager-bog
