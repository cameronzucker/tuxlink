# Handoff — 2026-05-21 — basil-vale-thistle — native Winlink client built, live-validated, app cut over

Final handoff for this session (supersedes the earlier `…-tls-and-backend.md`,
`…-live-validated.md`, `…-protocol-complete.md`, `…-data-plane.md` from today).
The native Winlink client replaced Pat as the app's backend, end to end.

## What shipped (branch `bd-tuxlink-0ic/native-winlink-client`, ~32 commits, pushed)

A complete native Winlink client — **no Go ships** — checked against
`la5nta/wl2k-go` and **validated against the real CMS over telnet (authorized
dev testing; not RF — see memory `feedback_cms_telnet_testing_authorized`)**.

- **Protocol library** `src-tauri/src/winlink/`: `message`(+`to_proposal`),
  `proposal`, `lzhuf` (compress+decompress, **byte-identical** to the reference),
  `transfer` (SOH/STX/EOT), `secure` (login MD5), `wire` (CR line framing),
  `handshake`, `session` (`send_turn`/`receive_turn` + `run_exchange`, turn cap),
  `telnet` (`telnet_login` + `connect_and_exchange`, **plaintext + TLS**),
  `compose` (fields → Winlink message).
- **Native store** `native_mailbox.rs`: Pat-independent; `store`/`list`/`read`/
  `move_to`, own on-disk format (`<mid>.b2f` per folder).
- **`NativeBackend`** (`winlink_backend.rs`): functional behind the
  `WinlinkBackend` trait — `send_message` composes → outbox; `list`/`read` from
  the store; `connect` runs the real exchange on a blocking task (TLS default).
- **App cutover**: `bootstrap.rs` now installs `NativeBackend` (no Pat spawn).
  Pat code retained `#[allow(dead_code)]` (kept per "don't delete Pat until
  parity"). `cms_connect` command + a **Connect button** in the dashboard ribbon;
  result/errors surface in the **session log**, not beside the button.

**Live validation:** plaintext → `cms-z.winlink.org:8772` = full session
(login → handshake → MD5 secure-login accepted → FF/FQ). TLS →
`server.winlink.org:8773` = full transport (TLS handshake `*.winlink.org` →
login → handshake → secure-login), stopping only at the production client-SID
allowlist. The live test caught + fixed two real bugs (the telnet
`Callsign:`/`Password:` login preamble; `*** ...` CMS error lines).

**UI smoke (grim):** app builds + launches + renders the full Mock-B shell on the
native backend; real identity (N7CPZ/DM33) + real empty mailbox by default.
Screenshots in the main checkout: `dev/scratch/tuxlink-native-backend-smoke.png`,
`dev/scratch/tuxlink-real-identity.png`.

**Gates:** 142 lib + 12 backend-integration + 311 frontend tests pass; tsc clean;
full app + release-relevant code builds. **Deps added:** `md-5` (secure login),
`native-tls` (TLS, reuses reqwest's OpenSSL).

## THE production blocker (not code) — do this first next session

**Register the client name `tuxlink` with Winlink.** Production servers reject
unregistered client SIDs (`*** Unknown client types … use cms-z.winlink.org`).
The request is drafted in `dev/winlink-client-registration-request.md` (and was
posted inline to the operator). Channel: the
[winlink-programs-group](https://groups.google.com/g/winlink-programs-group)
(operator's access was pending). Until registered, production+TLS connects fail
at the SID check; `cms-z` (plaintext) works for dev.

## Next-session work — filed bd issues (the UI batch the operator deferred)

- **`tuxlink-ng3`** (P2): top menu bar + titlebar are native gray; Mock B is dark
  blue. They're native (`tauri.conf` has no `decorations:false`; `app.set_menu`),
  so this needs **custom HTML window chrome** (decorations off + dark-blue
  titlebar/menu, drag region, window controls, re-wired menu events). Biggest.
- **`tuxlink-msr`** (P2): compose opens a separate window with duplicated
  main-window controls — rework per the design spec (inline pane/modal vs. a
  window without the duplicate chrome).
- **`tuxlink-xgn`** (P2): native mailbox has no read/unread state (it's
  `unread:false` always). Add read-tracking + mark-read on open + list/badge
  wiring. Most self-contained of the four.
- **`tuxlink-2ob`** (P3): GPS integration (read a device, feed the dashboard,
  honor `gps_state`/`position_precision`).

Plus, when convenient: the **escalation ADR** (ADR 0011 "fork & patch Pat" →
full native replacement), and eventually **deleting Pat** once parity + the
registration are confirmed.

## Dev workflow notes (so the next session doesn't relearn)

- **Real data by default:** `pnpm tauri dev` now shows the real backend/identity.
  Set `VITE_TUXLINK_FIXTURE=1` to re-enable the Mock B sample fixture for design.
- **Dev CMS host:** `TUXLINK_CMS_HOST=cms-z.winlink.org` points the native
  backend at the dev CMS (accepts the unregistered client) before registration;
  default is `server.winlink.org`.
- **Clicking Connect against `cms-z` downloads + consumes the operator's pending
  mail** (the exchange accepts all offered messages; the CMS then marks them
  delivered). Operator-run; the agent did not click it.
- The probe `src/bin/native_cms_probe.rs` exercises the on-air path directly
  (env: `TUXLINK_CMS_HOST`/`PORT`/`PLAINTEXT`, default TLS).
- **`default-run = "tuxlink"`** in `Cargo.toml` is load-bearing now that there are
  helper bins (`cargo run`/`tauri dev` would otherwise be ambiguous).

## State

- **Branch** `bd-tuxlink-0ic/native-winlink-client` (off `feat/v0.0.1`), pushed
  (0/0). Worktree `worktrees/bd-tuxlink-0ic-native-winlink-client/`. **No PR yet**
  — this is a large, self-contained, live-validated unit; opening a PR against
  `feat/v0.0.1` is a reasonable next step.
- **Working tree:** clean. No stashes. Gitignored-on-disk: build artifacts +
  `node_modules` (installed in the worktree this session) only.
- **bd:** `tuxlink-0ic` in_progress (core done; registration + Pat-deletion +
  UI parity remain). Four UI issues open (above).
