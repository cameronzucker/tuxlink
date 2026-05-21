# Handoff — 2026-05-21 — basil-vale-thistle — native client: TLS + functional NativeBackend

Final handoff of this session (supersedes the earlier `…-live-validated.md`,
`…-protocol-complete.md`, `…-data-plane.md`). The operator's directive evolved
to: **a built and working 0.0.1 client with real telnet + TLS-wrapped telnet, TLS
default (like Winlink Express).** CMS telnet dev testing is authorized (memory
`feedback_cms_telnet_testing_authorized`).

## What's built, working, and live-validated

A complete native Winlink client — no Pat — behind the existing
`WinlinkBackend` trait. **142 lib tests + the backend integration test pass.**

- **Protocol library** (`src-tauri/src/winlink/`): `message`(+`to_proposal`),
  `proposal`, `lzhuf` (compress+decompress, **byte-identical** to reference),
  `transfer`, `secure` (login MD5), `wire`, `handshake`, `session`
  (`send_turn`/`receive_turn` + `run_exchange`, with a turn cap), `telnet`
  (`telnet_login` + `connect_and_exchange`, **plaintext + TLS**), `compose`.
- **Native store** (`native_mailbox.rs`): Pat-independent; `store`/`list`/`read`/
  `move_to` per folder. Own on-disk format (raw message bytes per `<mid>.b2f`).
- **`NativeBackend`** (`winlink_backend.rs`): functional. `send_message` composes
  + queues to outbox; `list`/`read` from the store; `connect` runs the real CMS
  exchange on a blocking task (TLS default, plaintext optional), accepting
  offered mail into the inbox and moving sent to the sent folder;
  `disconnect`/`status`/`stream_log` mirror `PatBackend`.
- **`src/bin/native_cms_probe.rs`**: live validation tool (env: `TUXLINK_CMS_HOST`
  / `TUXLINK_CMS_PORT` / `TUXLINK_CMS_PLAINTEXT`; default TLS).

### Live validation (real CMS, authorized telnet)

- **Plaintext → `cms-z.winlink.org:8772`: full session SUCCESS** — telnet login,
  B2F handshake, MD5 secure-login accepted, `FF`/`FQ` clean close.
- **TLS → `server.winlink.org:8773`: full transport SUCCESS** — TLS handshake
  (cert `*.winlink.org`), telnet login, B2F handshake, secure-login — all over
  TLS — stopping only at the production client-SID allowlist (handled cleanly as
  `RemoteError`). Logged in `dev/live-cms-sessions.log`.

The live test found + fixed two real bugs the scripted tests missed: the telnet
`Callsign:`/`Password:` login preamble (fixed pw `CMSTelnet`, target `wl2k`), and
`*** ...` CMS error-line handling.

## The one external blocker for production (memory `project_cms_rejects_unknown_clients`)

**Production CMS rejects unregistered client SIDs.** `server.winlink.org` returns
`*** Unknown client types are not allowed … use cms-z.winlink.org`. The code is
correct and complete; production acceptance requires **registering tuxlink's
client name with the Winlink Development Team** (an out-of-band request — the
licensee/project submits it). Until then: `cms-z` (plaintext) accepts it for dev.
`CmsSsl` (TLS) only exists on production, so TLS end-to-end needs the registration.

## What's next — the app-default cutover (gated)

`NativeBackend` is functional but the **app still constructs `PatBackend`** (in
the bootstrap / `lib.rs` setup + `app_backend`). Swapping the default to native is
the remaining step, and it is deliberately gated:

1. **Register "tuxlink"** with Winlink so production+TLS accepts the SID (the
   blocker above). Recommend drafting that request next.
2. **Cutover** the bootstrap to construct `NativeBackend` (config + a mailbox
   root, e.g. `~/.local/share/tuxlink/native-mbox`). The app's mailbox view +
   connect button then drive native. Do it as the operator's "compare vs Pat
   then switch": the native store starts empty, so either a one-time import of
   Pat's `.b2f` mailbox, or accept a fresh native mailbox. Don't remove Pat until
   parity is confirmed (the current UI reads Pat's store).
3. The escalation **ADR** (ADR 0011 → full native replacement).

A dev-only CMS-host override (config field or env) would let the app exercise the
native path against `cms-z` before registration — a small, optional add.

## State

- **Branch:** `bd-tuxlink-0ic/native-winlink-client` (off `feat/v0.0.1`), pushed.
  Worktree clean; no stashes; gitignored-on-disk = build artifacts only.
- **Deps added this session:** `md-5` (secure login), `native-tls` (TLS — reuses
  reqwest's OpenSSL).
- **bd:** `tuxlink-0ic` in_progress.
