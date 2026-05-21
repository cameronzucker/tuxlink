# Handoff — 2026-05-21 — basil-vale-thistle — native Winlink client: protocol library + telnet complete

Supersedes the same session's earlier
`2026-05-21-basil-vale-thistle-native-client-data-plane.md` (the operator said
"push on" after the data plane, so the control plane got built too). Continues
`peregrine-spruce-knoll`'s start. Design is **settled** — not re-brainstormed.

## What this session did

Built the **entire native Winlink protocol library plus the telnet transport**,
test-first, each piece checked against `la5nta/wl2k-go` (read-only reference;
**no Go ships**). 11 commits on `bd-tuxlink-0ic/native-winlink-client`, all
pushed. `cargo test --lib` → **128 passed, 0 failed** (55 in `winlink::`).

Modules under `src-tauri/src/winlink/`:

| Module | What it does | How it's verified |
|---|---|---|
| `message.rs` | Winlink message container (headers + body, serialize/parse) and `to_proposal` (→ offer line + compressed body) | round-trip + end-to-end |
| `proposal.rs` | proposal offer line, batch checksum `F> <hex>`, inbound parse, `FS` answer parse | wl2k-go fixtures |
| `lzhuf.rs` | FBB B2 lzhuf **decompress + compress** | **byte-identical** to reference `.lzh` (vendored vectors) |
| `transfer.rs` | SOH/STX/EOT framed block (≤125-byte chunks, checksum) | hand-computed bytes + round-trip |
| `secure.rs` | secure-login response (MD5 of challenge+password+salt) | wl2k-go's two test vectors |
| `wire.rs` | CR-terminated line framing (trims CRLF newline + null padding) | scripted streams |
| `handshake.rs` | build our `;FW`/identifier/`;PR`/callsign lines; parse server SID/FW/`;PQ` | byte-exact build + scripted parse |
| `session.rs` | `send_turn` / `receive_turn` exchange halves + `run_exchange` driver (handshake → alternating turns → quit) | scripted in-memory streams |
| `telnet.rs` | `connect_and_exchange` (TCP connect, split read/write, run driver) | **loopback** test vs local 127.0.0.1 mock |

**New dependency:** RustCrypto `md-5` (pure-Rust, pairs with the `digest` crate
already present) for the secure-login MD5. Documented inline in `Cargo.toml`.

**Gate status:** `cargo test --lib` = 128 pass; `cargo build --lib` clean (only
the benign build.rs "skipping Pat sidecar in debug" note). Clippy is not
installed on this toolchain and CI doesn't gate on it.

### Standing rules honored

- Plain language; no invented labels.
- The MD5 dependency was a flagged decision — I took the documented default
  (RustCrypto `md-5`) per the no-atomic-decisions-for-plumbing rule rather than
  asking, and marked it as my choice in the commit.
- `connect_and_exchange` against the **real CMS transmits** under the station
  call sign → operator-run, per-run consent-gated (RADIO-1,
  `docs/live-cms-testing-policy.md`). The agent did not run it. The loopback
  test is local-only (127.0.0.1, no live network, no RF).

## What's next — `NativeBackend` (operator/design/transmission-gated)

The protocol library is complete and self-contained. What remains is **app
integration**, which the `WinlinkBackend` trait doc itself defers to "v0.5 Steps
3–10". It is genuinely gated on the operator and on design decisions, not on more
library code:

1. **`NativeBackend`** (`src-tauri/src/winlink_backend.rs`, currently a
   `NotImplemented` stub) implementing the 8-method `WinlinkBackend` trait on top
   of `winlink::session::run_exchange` / `winlink::telnet`. Open work + decisions
   (treat as **proposals**, get the operator's input — these are shape decisions,
   not plumbing):
   - **Mailbox persistence design** — the bd issue's open Codex gap ("native
     mailbox migration from Pat `.b2f`"). Where/how received and sent messages
     are stored, and how that relates to Pat's existing `.b2f` mbox dir that the
     live mailbox view currently reads.
   - **Message composition** — building a Winlink message from the trait's
     `OutboundMessage` (to/cc/subject/body/date): the MID (wl2k-go does
     MD5→base32→12 chars), the `Date` header format (`YYYY/MM/DD HH:MM` UTC), and
     the standard header set including the station `From` address and `Mbo`. This
     couples to station identity/config.
   - **Sync→async bridge** — the library is blocking I/O; the trait is async and
     `Send + Sync`. Wrap the blocking exchange in `tokio::task::spawn_blocking`;
     don't hold a `std::sync::MutexGuard` across `.await`.
   - **Secure-login password** flows from the OS keyring (per the cred design,
     memory `no-disk-creds-default-to-keyring`); `run_exchange` already takes it
     as a parameter (`ExchangeConfig.password`), never storing it.
   - **`connect`/`send`/`list`/`read` only function against the live CMS** — i.e.
     end-to-end validation is an operator-run, consent-gated step. Build + unit-
     test the wiring; the licensee runs the live exchange.
2. **Compare native vs Pat** over the same saved messages, then switch the
   default and later remove Pat — requires the live CMS (operator-run). Do **not**
   remove Pat before parity (the mailbox view depends on it).
3. **New ADR** recording the escalation from ADR 0011 ("fork and patch Pat") to
   "replace Pat with native Rust." Records a decision already made; not a blocker.

## State

- **Branch:** `bd-tuxlink-0ic/native-winlink-client` (off `feat/v0.0.1`),
  **pushed** (this handoff is the last commit). No PR yet — the protocol library
  is a clean, self-contained review unit if you want to land it against
  `feat/v0.0.1` now, ahead of the `NativeBackend` integration.
- **Working tree:** clean. No stashes. Untracked: none. Gitignored-on-disk: only
  build artifacts (`src-tauri/target/`, `gen/schemas/`, `sidecars/pat-*`) —
  nothing at risk (ADR 0009).
- **bd:** `tuxlink-0ic` stays `in_progress` (library done; `NativeBackend`
  integration + compare-vs-Pat pending). Note updated.
- **Dependency added:** `md-5 = "0.10"` (+ `Cargo.lock`).
