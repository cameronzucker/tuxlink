# Handoff — 2026-05-21 — peregrine-spruce-knoll — native Winlink client: started

## What this session did

1. **Fixed CI and merged PR #88** (the Pat spawn + app-start bootstrap, `tuxlink-22l`).
   The `build-linux` failure the prior handoff didn't know about was a *logical*
   merge conflict: `feat/v0.0.1` had moved ahead and grown a new `PatSpawnOptions`
   construction in `wizard.rs` that was missing the `http_announce_timeout` field
   #88's branch added. Each branch compiled alone; the merge didn't. Merged
   `feat/v0.0.1` into the branch, added the field, ran all gates green
   (cargo 16 bins / 0 fail, vitest 311, tsc+build), pushed → CI passed → merged.
   Closed `tuxlink-22l`, `tuxlink-xx3`, `tuxlink-xyd` (all delivered by #88).
   Verified merging was safe under the radio-transmission rule: the app spawns Pat
   in local `http` mode and does **not** connect to the CMS on launch (the connect
   path is deferred), so nothing transmits automatically.

2. **Recovered the prior design for replacing Pat** (the operator's main goal). It
   was already worked out on 2026-05-18 — see below. Did **not** re-brainstorm it.

3. **Started building the native client** (epic `tuxlink-0ic`, branch
   `bd-tuxlink-0ic/native-winlink-client`). Two pieces done, test-first:
   - `src-tauri/src/winlink/message.rs` — the Winlink message format, both ways
     (build a message → wire bytes; parse wire bytes → message).
   - `src-tauri/src/winlink/proposal.rs` — the proposal line that offers a message.
   - 3 tests passing. Both commits pushed.

## Two standing rules the operator set this session (do not relearn the hard way)

- **Plain language only. No jargon, no invented labels.** The operator rejects
  terms like "test oracle", "golden fixtures", "parity", "soak", and especially
  invented capitalized names. Say what a thing does. (A prior session's opaque
  label "Mock-D" hid an unapproved UI choice and cost the operator a full day —
  see ADR 0013, which corrected the UI to "mock-b".) Saved as memory
  `feedback_plain_language_no_fabricated_approval`.
- **Never present your choice as the operator's decision.** Mark what *you*
  propose as a proposal pending his approval; his approved spec is the only
  source of truth. Quote his actual words when citing a decision.

## The design (settled — build it, don't re-debate it)

Replace the Pat Go sidecar with a **native Rust Winlink client**. The decision and
plan are in the 2026-05-18 Codex review at
`dev/adversarial/2026-05-18-pat-greenfield-vs-keep-codex.md` (this folder is
git-ignored, so the file is local-only on pandora). Plus ADR 0011 (the fork that
set this up) and ADR 0002. Operator confirmed this session: do the **full** native
replacement **now** (not deferred), starting with the telnet path.

Key points, in plain words:
- **"Clean-room" here means a clean Rust *implementation*, not a new protocol.**
  The client must speak the *existing* Winlink B2F protocol to talk to the real
  CMS — we can't reinvent that. So we read `la5nta/wl2k-go` only as a reference for
  the exact format and check our output against it; **we ship no Go code.** (This
  is different from the future VARA *modem* replacement, which IS a brand-new
  signal with no compatibility requirement — see memory
  `project_v05_modem_design_posture`.)
- **The real protocol code lives in `wl2k-go`, not in Pat itself.** Reference it at
  `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/` (read-only).
- **Keep Pat running until the native client produces the same results**, then
  remove it. Do NOT delete Pat now — the app's live mailbox view (just shipped via
  #88) depends on it.
- Scope of this first milestone: telnet to the CMS only. AX.25/packet and the VARA
  modem are separate, later milestones.

## Protocol map (gathered this session — saves the next session the reading)

The wire flow when connecting to the CMS over telnet (no radio):
TCP connect → banner `[WL2K-5.0-...$]` → `;PQ:` line → `CMS>` → secure-login
(challenge/response using the Winlink password) → the message exchange below →
`FF` (no more) / `FQ` (quit).

The message exchange (reference: `wl2k-go/fbb/b2f.go`):
- To send: one **proposal line** per message —
  `F<code> <type> <mid> <uncompressed-size> <compressed-size> 0` (e.g.
  `FC EM TJKYEIMMHSRB 527 123 0`). `<code>` is `C` (standard compressed) or `D`
  (gzip). Then a **checksum line** `F> <hex>`: sum every byte of every proposal
  line (each followed by CR), negate, AND with 0xFF, print as 2 hex digits.
- The other side replies `FS <answers>` — one character per proposal:
  `Y/+` accept, `N/R/-` reject (already have it), `L/=/H` defer.
- Accepted messages transfer as framed compressed blocks: a header
  `SOH <len> <title> NUL <offset> NUL`, then data chunks `STX <len> <bytes>`
  (max 125 bytes each), then `EOT <checksum>`. Receiving mirrors this and verifies
  the running byte-sum is 0 mod 256.
- Message body is compressed with the **FBB lzhuf** variant (reference:
  `wl2k-go/lzhuf/` — `lzhuf.go`, `reader.go`, `writer.go`, plus `lzhuf/testdata/`
  with known input/output samples to check our implementation against). This is the
  trickiest piece; do it test-first against those samples.

The message format (reference: `wl2k-go/fbb/message.go` + `header.go`) — already
built in `winlink/message.rs`:
- email-like: `Mid` header first, the rest alphabetical, each `Key: value\r\n`,
  a blank `\r\n`, then the body (its byte length is the `Body` header), then
  attachments. Date format `YYYY/MM/DD HH:MM` UTC. Body charset ISO-8859-1.
  Headers/proposal/exchange types live in `wl2k-go/fbb/proposal.go`, `handshake.go`.

## What's next (ordered)

1. Finish the proposal handling in `winlink/proposal.rs`: the batch checksum line
   `F> <hex>`, parsing an inbound proposal line, and parsing the `FS` answer string.
2. The **lzhuf** compression in `winlink/lzhuf.rs`, test-first against
   `wl2k-go/lzhuf/testdata/`. (Hardest piece — de-risk early.)
3. The framed block transfer (SOH/STX/EOT) in `winlink/transfer.rs`.
4. The exchange logic that drives the back-and-forth (`winlink/session.rs`):
   inbound and outbound turns, `FF`/`FQ`/`F>` handling, checksum verification.
5. The telnet connection + banner + secure-login (`winlink/telnet.rs`). When this
   reaches the point of talking to the real CMS, that is an **operator-run,
   per-run-consented** test — the agent must not transmit (see
   `docs/live-cms-testing-policy.md`).
6. A `NativeBackend` implementing the existing `WinlinkBackend` trait
   (`src-tauri/src/winlink_backend.rs`, 8 async methods) so the app can use the
   native client in place of `PatBackend`.
7. Check native vs Pat over the same saved messages; once they match, switch the
   default to native and later remove the Pat sidecar.

A new ADR should record the escalation from ADR 0011's "fork and patch Pat" to
"replace Pat with native Rust" — write it when convenient (it records a decision
the operator has already made; not a blocker).

## State

- **Branch:** `bd-tuxlink-0ic/native-winlink-client` (off `feat/v0.0.1`), pushed.
  Worktree: `worktrees/bd-tuxlink-0ic-native-winlink-client/`. No PR opened yet
  (open one against `feat/v0.0.1` when the milestone is further along, or sooner if
  you prefer smaller reviews).
- **Working tree:** clean (both increments committed + pushed).
- **bd:** `tuxlink-0ic` in progress (claims this worktree). `tuxlink-22l/xx3/xyd`
  closed. The old `tuxlink-22l` worktree
  (`worktrees/bd-tuxlink-22l-pat-spawn-bootstrap/`) is now merged and can be
  disposed via the ADR 0009 ritual; it has a committed live-cms session-log line
  but nothing else at risk.
- **Other open issues** (`bd ready`): `tuxlink-b2s` (CI skips frontend-only PRs),
  `tuxlink-qn8` (wizard keyring guidance), `tuxlink-8za` (color schemes),
  `tuxlink-cs7` (AppImage packaging), and others — none block the native client.
