# Handoff — basalt-osprey-willow — HTML Forms completion arc

**Agent:** basalt-osprey-willow · **Date:** 2026-06-05 · **Machine:** pandora

## Critical first action — next session

```
1. READ this handoff doc first (session-start hook only surfaces the filename).
2. Check the in-flight PR landing status:
   gh pr list --json number,title,mergeable,statusCheckRollup --jq '.[] | {n: .number, t: .title[:60], m: .mergeable}'
3. PR #395 + #397 + #398 are all 4/4 CI green and awaiting operator merge.
   Do NOT re-trigger Codex on #398 unless quota has reset (capped at
   2026-06-05 05:19 UTC; reset at 1:23 AM PT per error message).
4. forms::updater BACKEND landed (committed + pushed to branch
   bd-tuxlink-xipa/forms-updater) BUT NO PR yet — IPC + UI surfaces remain.
   See "forms::updater state" below for the next-session task breakdown.
```

## What shipped this session

### PR #392 — HTML Forms P2 (MERGED earlier in session at 03:44 UTC)

Position + ICS-309 + slot library. Already in main.

### PR #395 — CheckIn WLE schema alignment + OSM CSP allowlist

4/4 CI green. Closes bd `tuxlink-4ai0` + `tuxlink-bt2q`. Awaiting operator merge.

Rebuilt the CheckInForm to full WLE schema (~18 fields including all 4 radio groups: Status / Service / Band / Session). Auto-fill: msgsender ← config.callsign; contactname ← config.identifier; grid ← PositionArbiter; datetime ← UTC now (refreshed at submit); locationsource ← GPS when fresh, else Operator. Defaults mirror WLE: organization="Winlink Net", status="EXERCISE", service="AMATEUR", band="NA", session="Telnet".

Codex round caught 5 follow-ups, all applied:
- P1 CSP allowlist missed bare `tile.openstreetmap.org` host (only matched wildcard)
- P2 `Templateversion` corrected to WLE's exact `"Winlink Check-in 5.1.3"` (was my invented string)
- P2 Location is required per WLE
- P2 MsgTo max_length 60 → 75 per WLE
- P2 applySlot no longer leaks invalid radio values into draft autosave

### PR #397 — P2 hardening bundle (gheo + 4g2n + rk6s + m2o6)

4/4 CI green. Closes bd `tuxlink-gheo` + `tuxlink-4g2n` + `tuxlink-rk6s` + `tuxlink-m2o6`. Awaiting operator merge.

- **gheo:** `substitute_template` emits just `/folder` (no folder name); `folder_handler` treats the full wildcard rest as the file path. Fixes nested-folder template assets that broke when axum decoded percent-encoded slashes.
- **4g2n:** 8 MiB asset-size cap via `std::fs::metadata` pre-flight check + 413 on over-cap.
- **rk6s:** `mpsc::channel(1)` + `try_send` + 503 on `TrySendError::Full`.
- **m2o6:** form-label disambiguation — ICS-213 `To/From/Subject` → `Addressee / Originator / Form subject`; Bulletin `Subject` → `Form subject`; CheckIn `Subject` → `Form subject` + `To` → `Addressee (recipient)`.

Codex round caught 1 follow-up (FIFO bypass on the asset-size cap — non-regular files report `len()=0`, bypassing the size check). Fixed via `!md.is_file()` pre-flight + new `folder_route_rejects_non_regular_file` regression test (Unix-only).

### PR #398 — Phase 3 reply extension to Bulletin

4/4 CI green. Closes bd `tuxlink-ltkv`. Awaiting operator merge. Codex round deferred (quota-capped 2026-06-05 05:19 UTC; reset ~1:23 AM PT — re-run when quota available per `feedback_codex_quota_gotcha`).

`replyWithForm` extended from ICS-213-only to also include Bulletin. Refactored `buildReplyDraft` from hard-coded ICS-213 mapping to a `switch` on `message.formId` with explicit per-form mapping cases. Bulletin mapping: original `from_name` → new `name` (recipient); carry `level`/`title`/`subjectline` (with Re: prefix); blank `bullnr`/`activitydatetime1`/`message`/`from_name`. Intentionally excluded forms documented inline: Position (broadcast, no recipient), ICS-309 (log, not conversation), Check-In (status, no `_SendReply.0`), Damage Assessment (report).

CI caught one regression: `MessageView.test.tsx` asserted "no Reply-with-form button for Bulletin" (the OLD behavior). Fixed in d5a330a: split into two tests — Bulletin SHOWS the button (new expected behavior); Position/309/DA/Check-In stay hidden.

### forms::updater backend — committed + pushed, no PR yet

Branch: `bd-tuxlink-xipa/forms-updater`. Bd: `tuxlink-xipa`.

**Backend complete:**
- `src-tauri/src/forms/updater.rs` (~600 lines) — mirrors Pat's `internal/forms/forms.go` API: GET `https://api.getpat.io/v1/forms/standard-templates/latest` → JSON `{version, archive_url}` → download zip → extract to staging → atomic swap into runtime root → write VERSION file → rollback on swap failure.
- `Cargo.toml` adds `zip = "2"` (deflate-only feature for smaller dep weight).
- `forms/mod.rs` declares `pub mod updater`.
- 14 unit tests cover: zip extraction (wrapped + unwrapped archives), path-traversal rejection, install end-to-end with mockito, prior-snapshot replacement + `.prev-*` backup, BadArchive error paths, empty-version rejection.

**NOT done (next session):**
- `forms_check_for_update` + `forms_refresh` Tauri IPCs in `ui_commands.rs`
- Register IPCs in `tauri::generate_handler!` in `lib.rs`
- `wle_templates::bundle_root_for_app` runtime/bundle precedence — check `runtime_snapshot_present(runtime_root)`; return runtime path if present, else fall back to resource bundle path
- React UI: "Refresh forms from winlink.org…" button in CatalogBrowser toolbar + confirmation modal showing current vs available version
- Codex full-diff adrev (when quota resets)
- Open PR

The backend is testable in isolation via the existing cargo unit tests; the IPC + UI layer is what makes it operator-reachable.

## Bd state

Closed (when PRs merge): `tuxlink-4ai0`, `tuxlink-bt2q`, `tuxlink-gheo`, `tuxlink-4g2n`, `tuxlink-rk6s`, `tuxlink-m2o6`, `tuxlink-ltkv`.

In-progress: `tuxlink-xipa` (forms::updater — backend committed on branch, IPC+UI pending).

P3 follow-ups filed alongside earlier PRs (still open):
- `tuxlink-8onn` React.lazy form-registry (also fixes vitest cumulative-flake)
- `tuxlink-34te` Bundle offline tile pack for Position map
- `tuxlink-nimx` FormDraftLibrary "Update slot" in-place affordance
- `tuxlink-yhrn` Modal for slot-label entry (replace window.prompt across 4 forms)
- `tuxlink-ba7l` PositionFormV2 GPS retry button
- `tuxlink-hxia` Check-In WLE wire-format keys (only if option-b chosen for re-enable; superseded by PR #395's full WLE alignment)

## Worktrees in flight at handoff

| Worktree | Branch | PR | Status |
|---|---|---|---|
| `bd-tuxlink-hnkn-p2-native-autofill` | `bd-tuxlink-hnkn/p2-native-autofill` | #392 (MERGED) | Local branch is `merged-dead`; dispose via 4-step ritual when convenient |
| `bd-tuxlink-4ai0-checkin-wle-alignment` | `bd-tuxlink-4ai0/checkin-wle-alignment` | #395 | 4/4 CI green; awaiting merge |
| `bd-tuxlink-gheo-forms-p2-hardening` | `bd-tuxlink-gheo/forms-p2-hardening` | #397 | 4/4 CI green; awaiting merge |
| `bd-tuxlink-ltkv-reply-with-form-extension` | `bd-tuxlink-ltkv/reply-with-form-extension` | #398 | 4/4 CI green; Codex deferred; awaiting merge |
| `bd-tuxlink-xipa-forms-updater` | `bd-tuxlink-xipa/forms-updater` | none | Backend committed + pushed; IPC + UI + Codex + PR pending next session |

`worktrees/` listings have many older orphans unrelated to this session's work; don't touch them.

## Failure modes worth carrying

1. **Codex daily quota cap.** Per `feedback_codex_quota_gotcha`, the ChatGPT-auth Codex CLI hits a daily quota. The cap message ("ERROR: You've hit your usage limit") is capacity-defer, NOT skip. Wait for reset (~1:23 AM PT this session) or substitute a Claude review for non-RF non-load-bearing work only. PR #398 fell into this — Codex round deferred.

2. **Vitest worker zombies.** `pkill -f vitest` interrupts can orphan worker processes that consume 8.5 GB RAM combined. Verify `pgrep -f vitest` empty after sweeps; prefer narrow scope (single file path) over full sweeps. See `feedback_vitest_worker_zombies`.

3. **No admin-spiral / operator-decision punts on polish.** When Codex/adrev surfaces a feature gap (not a polish nit), implement the polished default and ship. Don't disable + file "pending operator decision" bd issues — that's the PR #347 + PR #392 pattern the operator called out as admin spiral. See `feedback_no_operator_decision_punts_on_polish` (new memory this session).

4. **Test regex anchoring + label collisions.** When renaming a form label, `getByLabelText(/^Old$/i)` breaks but `getByLabelText(/Old/i)` (unanchored substring) usually still works. The reverse trap: a regex like `/EXERCISE/i` matches both the radio AND the "Exercise ID" text input — fixed in PR #395 by anchoring to `/^EXERCISE$/i`.

5. **`cd` in bash sessions sometimes reverts mid-session** — pin `git -C <path>`, `cargo --manifest-path <path>`, `pnpm -C <path>` rather than persisting `cd`. Cost a couple of "main-checkout hook denied" surprises this session.

## Codex transcripts (local-only, gitignored)

- `dev/adversarial/2026-06-05-checkin-wle-codex.md` (PR #395 round — 5 findings, all applied)
- `dev/adversarial/2026-06-05-p2-hardening-codex.md` (PR #397 round — 1 P2 finding, applied)
- PR #398 + #(future xipa): Codex deferred until quota reset

## Session-end starting prompt for the next session

```
Continuing tuxlink HTML Forms completion. This session shipped PRs #395
(CheckIn WLE), #397 (P2 hardening), #398 (Phase 3 reply extension) —
all 4/4 CI green and awaiting merge. PR #398's Codex round was deferred
(quota cap; resets ~1:23 AM PT).

forms::updater backend (bd tuxlink-xipa) is committed on branch
bd-tuxlink-xipa/forms-updater with 14 passing unit tests but NO PR yet
— IPC plumbing + UI + Codex + PR remain.

Read dev/handoffs/2026-06-05-basalt-osprey-willow-html-forms-completion-arc.md
FIRST for the full chip-order rationale + the forms::updater task
breakdown.

Critical first actions:
1. Check the 3 merge-pending PRs (#395, #397, #398). If merged, dispose
   the dead worktrees via the 4-step ritual.
2. If Codex quota has reset, run the deferred adrev on #398.
3. Continue tuxlink-xipa: add forms_check_for_update + forms_refresh
   Tauri IPCs in ui_commands.rs, runtime-vs-bundle precedence in
   wle_templates::bundle_root_for_app, "Refresh forms…" button +
   modal in CatalogBrowser. Then Codex + PR. Worktree is at
   worktrees/bd-tuxlink-xipa-forms-updater (already has node_modules
   + ~600-line updater.rs as the backend foundation).
```

Agent: basalt-osprey-willow
