# Handoff — fix GitHub #648 (Compose drops un-Entered recipient) — kite-taiga-hawk

Date: 2026-06-13 · Agent: kite-taiga-hawk · bd: **tuxlink-waxd** (P1 bug) · Issue: **GH #648**
Worktree (yours): `worktrees/bd-tuxlink-waxd-recipient-flush` (branch `bd-tuxlink-waxd/recipient-flush`, off main, node_modules installed)

## Your task: fix #648 — the first external bug

**SILENT RECIPIENT DATA-LOSS.** Repro: New Message → type a recipient in To: (e.g.
`w6bi@winlink.org`) → click **Post to Outbox** *without pressing Enter* → the Outbox
message's To: is empty (—). Self-send is incidental; **any** un-Entered recipient is
lost, on **To and Cc**.

### Root cause (confirmed — full code trace, do not re-investigate from scratch)
`src/contacts/RecipientInput.tsx` keeps in-progress typed text in an internal `text`
state (useState, ~line 61). It commits to the real `value` (a chip) **only** on Enter
or dropdown-pick (`onKeyDown`, lines 119-127). There is **no commit-on-blur** (Escape
even *clears* pending text). `Compose.tsx` reads the committed `value` via the `to`
state in `buildRecipients` (line ~521) and **never flushes the pending buffer on send**.
So a typed-but-not-Entered recipient stays in the buffer → `to` is empty →
`message_send` gets `to: []` → `compose_message` (winlink/compose.rs:62-64) loops an
empty slice → no `To:` header written → Outbox shows —.

**The entire Rust write/store/parse/list/view path is correct and tested** (traced
end-to-end; `compose.rs`/`message.rs` are byte-identical v0.55.0..main). The loss is
purely FE buffer-not-flushed. Evidence: issue screenshot shows To: as plain text, not
a committed chip.

### The fix (small, FE-only — no Rust change)
1. **`RecipientInput`** commits pending `text` on **blur** (`onBlur` → `addRawToken(text)`
   when `text.trim()` is non-empty). Standard token-input behavior.
2. **Belt-and-suspenders** for the click-Send-without-blur race: have `RecipientInput`
   expose an imperative `flush()` / `pendingValue()` via `useImperativeHandle`, hold
   refs to the To **and** Cc inputs in `Compose.tsx`, and flush them at the top of
   `buildRecipients` (covers **all** send paths — `message_send` / `send_form` /
   `send_webview_form` all funnel through `buildRecipients`).
3. Apply to **both** To and Cc.

### Regression tests (must FAIL before, PASS after)
- `Compose.test.tsx`: render Compose, type a recipient **without Enter**, click "Post to
  Outbox", assert `invoke('message_send', { draft: { to: ['…'] } })` carries the typed
  recipient. (This is the exact #648 repro.)
- `RecipientInput.test.tsx`: type text + blur → `onChange` emits the committed value.

### Verify
- `pnpm typecheck` + `pnpm vitest run src/compose src/contacts` locally (FE-only fix).
- Run the **full** `pnpm vitest run` before pushing (a Compose/RecipientInput change can
  ripple into far tests — the contract-test lesson). Push → CI is the Rust+build gate.
- Then mark the PR ready. A WebKitGTK smoke is nice but this is logic, not render.

### Operator-gated (do NOT do without Cameron)
- GH issue #648 is mislabeled `enhancement` (it's a bug). **Do not relabel, comment, or
  close** the public issue — issue triage/response is operator-gated for now
  (**tuxlink-olrl** tracks evaluating an agent issue-triage workflow). Cameron handles
  the public reply + close when the fix merges.

## Context — what shipped this session (kite-taiga-hawk, marathon)
- **Contacts reshape (tuxlink-je5d) — PR #646, CI-GREEN, ready.** Unified outline
  (search › collapsible groups › ungrouped), callsign-first identity, polymorphic
  detail with the carried-over favorites **connection record** (new
  `contacts_connection_record` command, shared store by `gateway==callsign`), inline
  group management, multi-select→add-to-group, no Message-all. **Pending: Cameron's
  WebKitGTK smoke, then merge.** (Built via two subagents; frontend agent died mid-run
  on API-overload but had done the bulk — I reviewed + finished + greened it.)
- **Release freeze lifted (tuxlink-yyii) — PR #644 merged.** release-please resumed;
  v0.56.0 + v0.57.0 cut. Lifted only after Phase 7 (`#640`) landed so transmit works.
- Earlier: GPS wizard Location step (tuxlink-9xy1 PR #631), de-flaked + merged #628
  (managed Dire Wolf), s0r1 Find-a-Station fixes.

## In-flight worktrees (disposable; ADR-0009 ritual when convenient)
- `worktrees/bd-tuxlink-je5d-contacts-outline` — active, PR #646 (Contacts).
- `worktrees/bd-tuxlink-yyii-lift-freeze`, `…-yq3l-managed-direwolf`, `…-9xy1-gps-setup-assist`,
  `…-9xy1-gps-foundation` — merged-dead / stale.

## Next-session starting prompt
```
Fix GitHub issue #648 (bd tuxlink-waxd, P1) — Compose silently drops a recipient
typed without pressing Enter, so the Outbox message has an empty To:.
READ FIRST: dev/handoffs/2026-06-13-kite-taiga-hawk-issue648-recipient-flush.md
Work in worktrees/bd-tuxlink-waxd-recipient-flush (branch bd-tuxlink-waxd/recipient-flush,
off main, node_modules installed).

Root cause is CONFIRMED (in the handoff + bd): src/contacts/RecipientInput.tsx keeps
typed text in an internal buffer, commits to `value` only on Enter; Compose reads the
committed `to` and never flushes the buffer on send. Rust path is correct — FE-only fix.
Fix: commit pending text on blur + flush via a ref before buildRecipients (To AND Cc,
all send paths). Add the regression test (type without Enter → click Send → message_send
carries the recipient) — must fail before, pass after. Full vitest before push; CI gates Rust.
DO NOT touch/relabel/close GH #648 — issue response is operator-gated (tuxlink-olrl).

Also: Contacts PR #646 is green + ready, awaiting Cameron's WebKitGTK smoke + merge.
```
