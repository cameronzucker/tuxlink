# Handoff — Forms G10 + G12-C shipped (peregrine-lupine-magnolia)

**Date:** 2026-06-12 · **Agent:** peregrine-lupine-magnolia · **Branch (operator):** `bd-tuxlink-xygm/recover-handoffs`

## This session shipped (both merged to `main`)

| Item | What | PR | Merge |
|---|---|---|---|
| **G10** (`tuxlink-hhfx`) | Reply-form threading — honor `ReplyTemplate:`, open `<X>_SendReply.html` PRE-BOUND with the original message, editable | #637 ✅ | `645f94fe` |
| **G12-C** (`tuxlink-2tom`) | SeqInc message serial numbering — persisted per-form counter, auto-stamped at send, Settings reset section | #638 ✅ | `5280f5e6` |

Both CI-green on **both arches** (clippy `-D warnings` + full vitest + bundle build). Both bd issues closed. Both task branches deleted; both worktrees disposed (ADR 0009 ritual).

### G10 design (for reference)
Replying to a form advertising `ReplyTemplate:` (the 8 ICS-213 General Message variants) opens its SendReply page pre-bound with the original field values + editable; the operator fills only the Reply section. Key pieces: `forms::txt_template::resolve_sendreply` (resolves the SendReply HTML via the `.0`'s own `Form:` directive — stem-drift-proof for HICS), `http_server::FormSession::open_form_prebound` (editable + pre-bound = union of `open`/`open_viewer`), `open_webview_reply` command (derives the SendReply from the **local bundle**, not the wire claim), `send_webview_form` reply branch (renders To/Subject/Msg from the SendReply `.0`; `subject_hint` supplies the `Re:` subject since `.0`s carry no `Subject:`). Frontend: `formReply` mode, `webview-reply` Compose FormMode, MessageView routing. **v1 limitation:** `MsgOriginalBody` sent empty (raw body isn't safely available frontend-side; the SendReply reproduces the original via structured fields).

### G12-C design (for reference)
22 bundled `.txt` use `SeqInc:` + `{SeqNum}`/`<var SeqNum>`. New: `forms::sequence::SeqCounterStore` (persisted `<app_data>/forms-sequence-counters.json`, infallible open + atomic write, managed `Arc<Mutex<>>` in `setup()` **with a temp-path fallback** in the `app_data_dir`-unavailable arm). `send_webview_form` allocates the next serial on a `SeqInc` template and stamps `SeqNum` **before render**; allocation persists **before** the send (failed send → serial **gap**, never a **duplicate**). `{SeqNum}` blanked at open; serial assigned at send. Reset via Settings → "Form sequence numbers" (`forms_sequence_status` + `forms_sequence_reset` + `FormSequenceSettings`).

## METHOD (standing corrections honored this session)

1. **No cold cargo on the Pi.** Operator corrected a background `cargo test --no-run` that "murders the Pi." All Rust compilation/clippy/cargo-tests go to **Cloud CI** (draft PR → CI compiles both arches). Locally: only `tsc --noEmit` + scoped `vitest` (cheap). Self-adrev the Rust by re-reading for borrow/type/clippy issues.
2. **Codex is NOT a gate.** Unavailable → rigorous self-adversarial review (multi-angle) + ship; operator testing is the backstop. Do not defer/stall on Codex.
3. **Grounding-first.** Both features started by reading the real bundle (`.0`/`.html`/`.txt` directives, the `{SeqNum}` placeholder shape) before designing — caught the HICS stem-drift and the `{SeqNum}` = placeholder-not-`<var>` facts.

## Operator smokes pending (display/RF — CI can't run them)

- **G10:** receive an ICS-213 → **Reply with form…** → verify the SendReply opens pre-bound with the original → fill Reply → send → verify the reply carries original+reply, `Re:` subject, back to the sender. (Try a non-native one — HICS/ARC 213 — too.)
- **G12-C:** send an IARU Message / radiogram form twice → `Msg#` increments (1, 2, …). Settings → "Form sequence numbers" → set next serial → confirm next send uses it.
- **From the prior session (still pending):** G8 Export PDF, G8b Print…, o4p9 send-a-catalog-form recipient/subject/body.

## Remaining forms backlog (epic `tuxlink-zkuk`, all self-adrev-buildable)

- **`8v3l`** — catalog identity stem-collapse (contained interop). Good next chip.
- **`z0gx`** — G7 self-contained payloads (the big differentiator; larger).
- **Parked** (operator: need evidence first): `yv6i`(G1 aggregation), `09l9`/`ike9`/`td3k`(G2-G4).

## State at handoff

- **`main` HEAD:** `5280f5e6` (includes #637 G10 + #638 G12-C + concurrent sessions' work).
- **Operator branch:** `bd-tuxlink-xygm/recover-handoffs` (this handoff lands here).
- **Worktrees:** both forms worktrees (hhfx, 2tom) disposed. ~140 other worktrees exist from prior sessions (not this session's; a separate hygiene backlog — not touched). `qjgx` holds the `main` checkout (causes the harmless `gh pr merge --delete-branch` "main already used by worktree" error — the remote merge still succeeds; verify via `gh pr view --json state,mergedAt`).
- **bd:** `hhfx` + `2tom` closed; durable in Dolt.

## Build gotchas (cost real time)

- Fresh worktree off `origin/main`; `pnpm install --prefer-offline` before push (pre-push doc-link linter runs `tsx`).
- Before PR: `git fetch origin main` + `git merge origin/main --no-ff` (merge commit needs an `Agent:` trailer); re-check `git diff --stat origin/main..HEAD` shows ONLY your files.
- `send_webview_form` now takes `reply_template` + `subject_hint` + `seq_store` State params — a new managed State means the `app_data_dir`-Err setup arm MUST also manage a fallback or the command breaks.

## Starting prompt for the next session

```
Continue the Forms-push epic (tuxlink-zkuk). READ
dev/handoffs/2026-06-12-peregrine-lupine-magnolia-forms-g10-g12c-shipped.md FIRST.
G10 (reply threading, #637) + G12-C (SeqInc serials, #638) shipped + merged this
session. Next buildable: 8v3l (catalog identity stem-collapse — contained) then
z0gx (G7 self-contained payloads — the differentiator). METHOD: NO cold cargo on
the Pi — write Rust carefully, push a draft PR, let Cloud CI compile both arches;
locally only tsc + scoped vitest. Codex is NOT a gate — self-adrev and ship;
operator testing is the backstop. Ground in the real bundle before designing.
Worktree per item off main; merge on green CI. Operator smokes for G10/G12-C are
listed in the handoff (display/RF — operator runs them).
```
