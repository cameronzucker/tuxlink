# Handoff — 2026-05-21 — basil-vale-thistle — native client: LIVE-VALIDATED against the real CMS

Supersedes this session's earlier `…-protocol-complete.md` and `…-data-plane.md`.
The operator twice said "push through" and clarified that **telnet-to-CMS dev
testing is authorized** (RADIO-1 gates RF transmission, not CMS telnet — see
memory `feedback_cms_telnet_testing_authorized`). So this session went all the
way to a live end-to-end validation.

## Headline

The native Winlink client **authenticated and ran a clean session against the
real CMS** (telnet). Proof, from `native_cms_probe` against `cms-z.winlink.org`:
telnet login → B2F handshake parsed (real `SID B2FWIHJM$`, challenge) →
**MD5 secure-login accepted by the CMS** → `FF` → CMS replied `FQ` → clean close.
No mail transferred. Logged in `dev/live-cms-sessions.log`.

## Complete + tested (141 lib tests pass; checked vs `la5nta/wl2k-go`; no Go ships)

Protocol library — `src-tauri/src/winlink/`:
`message` (container + `to_proposal`), `proposal`, `lzhuf` (compress+decompress,
**byte-identical** to reference), `transfer` (SOH/STX/EOT), `secure` (login MD5 —
**confirmed accepted by the live CMS**), `wire` (CR line framing), `handshake`,
`session` (`send_turn`/`receive_turn` + `run_exchange` driver), `telnet`
(`telnet_login` + `connect_and_exchange`), `compose` (fields → Winlink message).

Plus: `native_mailbox.rs` (Pat-independent on-disk store: `store`/`list`/`read`
per folder, raw bytes keyed by Mid) and `src/bin/native_cms_probe.rs` (the live
validation tool — authorized dev testing, transfers no mail).

## Real bugs the live test caught (and fixed)

1. **Telnet login preamble** — the CMS telnet "post office" prompts `Callsign :`
   then `Password :` *before* the B2F handshake (fixed public password
   `CMSTelnet`, NOT the station password; B2F target call `wl2k`). My scripted
   tests assumed B2F started immediately. Fixed in `telnet.rs::telnet_login`.
2. **`*** error lines`** — the CMS reports failures as `*** ...` lines; `session`
   now returns `ExchangeError::RemoteError` instead of `UnknownCommand`.

## Findings that gate production (memory `project_cms_rejects_unknown_clients`)

- **Production CMS rejects unregistered client SIDs:** `server.winlink.org:8772`
  returns `*** Unknown client types are not allowed … use cms-z.winlink.org`.
  **`cms-z.winlink.org:8772` is the dev CMS** (accepts unregistered clients).
  **Register tuxlink's client name with Winlink before production use.**
- **`CmsSsl` (the config default) needs TLS** — port 8773, TLS. The native client
  does plaintext telnet (8772) only. A TLS wrapper (e.g. rustls) is not built.
- The keyring password is readable via the `keyring` crate (service
  `tuxlink-pat`, account = UPPERCASE callsign) — even where `secret-tool` with an
  `account` attribute didn't find it.

## What's next — `NativeBackend` (the trait wiring), now fully de-risked

`winlink_backend.rs`'s `NativeBackend` is still the `NotImplemented` stub (and
`tests/winlink_backend_test.rs` Test 6 asserts that — rewrite it when you make it
functional). Wire it over the validated library + `native_mailbox::Mailbox`:

- **Clean / no decisions (do first):** `send_message` = `compose_message` (parse
  the trait `OutboundMessage.date` → unix secs) then store to the **outbox**;
  `list_messages`/`read_message_in` = `Mailbox::list`/`read`; `status` = cached
  enum; `stream_log` = a broadcast channel (mirror `PatBackend`). All testable
  with temp dirs. Constructor must change to take a `Config` + a mailbox root
  (no external callers today besides Test 6).
- **`connect` (validated at the library level):** read the outbox → build
  `session::OutboundMessage`s (`Message::from_bytes` → `to_proposal` + Subject) →
  `tokio::task::spawn_blocking` running `telnet::connect_and_exchange` with an
  accept-all decide → store `result.received` to inbox, move sent MIDs
  outbox→sent. The exchange itself is proven by `native_cms_probe`.
  - **PRODUCT DECISIONS (operator) before connect ships:** (a) which CMS host to
    default to — production rejects us, so dev is `cms-z`; does tuxlink register
    its client name? (b) TLS for the `CmsSsl` default transport (native is
    plaintext-only today); (c) the `disconnect`/`Session` semantics vs Pat's.
- Then **compare native vs Pat** + switch the default + remove Pat; and the
  **escalation ADR** (ADR 0011 → full native replacement).

## State

- **Branch:** `bd-tuxlink-0ic/native-winlink-client` (off `feat/v0.0.1`), pushed.
  Worktree `worktrees/bd-tuxlink-0ic-native-winlink-client/`. No PR yet — the
  library + store is a clean, self-contained, live-validated review unit.
- **Working tree:** clean. No stashes. Gitignored-on-disk: build artifacts only.
- **Deps added this session:** `md-5` (secure login).
- **bd:** `tuxlink-0ic` in_progress; note updated.
