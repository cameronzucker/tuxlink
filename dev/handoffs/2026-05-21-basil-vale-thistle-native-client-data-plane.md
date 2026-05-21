# Handoff — 2026-05-21 — basil-vale-thistle — native Winlink client: data plane complete

Continues `peregrine-spruce-knoll`'s 2026-05-21 session
(`dev/handoffs/2026-05-21-peregrine-spruce-knoll-native-client-start.md` — read
it for the settled design and the full protocol map). The design is **settled**;
this session did not re-brainstorm it.

## What this session did

Finished the entire **data plane** of the native client — every place where the
exact bytes on the wire matter — test-first, each piece checked against
`la5nta/wl2k-go` (read-only reference; **no Go ships**). Five commits on
`bd-tuxlink-0ic/native-winlink-client`, all pushed:

1. **`3c5b240` — proposal handling** (`winlink/proposal.rs`). The batch checksum
   line `F> <hex>`, parsing an inbound `F<code> ...` offer line, and parsing the
   `FS <answers>` reply (both the letter form `Y/N/R/L/H` and the symbol form
   `+/-/=`, including accept-at-offset). Anchored to wl2k-go's own fixtures.
2. **`930e677` — lzhuf decompression** (`winlink/lzhuf.rs`). An independent Rust
   port of the FBB B2 lzhuf (LZSS window + adaptive Huffman + CRC16). Decompresses
   wl2k-go's reference `.lzh` files exactly, including a 100 KB input that
   exercises the Huffman tree-rebuild path.
3. **`815ab2e` — lzhuf compression** (`winlink/lzhuf.rs`). The match finder
   (per-first-byte binary search trees) + bit writer. `compress(x)` is
   **byte-for-byte identical** to wl2k-go's `.lzh` for both fixtures — i.e. we
   reproduce its exact match-selection, so the real CMS accepts our bodies as it
   does Pat's.
4. **`10164b5` — framed block transfer** (`winlink/transfer.rs`). The
   SOH/STX/EOT frame: header (title + resume offset), ≤125-byte data chunks, EOT
   checksum. Byte-exact frame + chunk-boundary asserted; round-trip + bad-checksum
   + bad-header covered.
5. **`7b560a9` — message → wire bridge** (`winlink/message.rs`). `to_proposal`
   serializes a message, compresses it, and returns the offer line + compressed
   bytes. A round-trip test composes **all four modules** end to end:
   message → proposal+compressed → frame → read back → decompress → parse, and
   the message matches.

**Gate status:** `cargo test --lib` → **104 passed, 0 failed** (31 are the new
`winlink::*` tests). `cargo build --lib` clean (the only "warning" is the benign
build.rs note about skipping the Pat sidecar in debug). Clippy is **not
installed** on this toolchain and CI does **not** gate on it (release.yml only
runs `cargo build --release`), so "clippy clean" was not claimed.

Vendored conformance vectors under
`src-tauri/src/winlink/testdata/lzhuf/` (gettysburg + pi, public-domain texts +
their reference `.lzh`) with `PROVENANCE.md`, so the codec tests run in CI without
the Go module present.

## Two standing rules (unchanged, do not relearn)

- **Plain language only** — no jargon, no invented capitalized labels. Say what a
  thing does.
- **Never present your choice as the operator's decision.** Mark proposals as
  proposals; his approved spec is the only source of truth.

## What's next — the control plane (ordered)

The data plane is pure functions matching a fixed wire format, which is why it
came out clean and byte-exact. The remaining work is the **control plane**:
stateful I/O, coupled to interfaces that aren't built yet, and including the
operator-gated live step. Treat the interface shapes below as **proposals**, not
decisions.

1. **Secure login response** (small, security-sensitive, has reference vectors).
   MD5 of `challenge + password + 64-byte salt`, then a specific bit-reduction →
   8 decimal digits. wl2k-go's `secure_test.go` gives exact vectors to test
   against: `("23753528","FOOBAR") → "72768415"` and
   `("23753528","FooBar") → "95074758"`. The salt is in
   `wl2k-go/fbb/secure.go`.
   - **DECISION NEEDED (proposal):** this needs MD5, and there is **no direct
     `md5` dependency** today (the tree has `sha1`/`sha2`/`digest`/`ring`/`openssl`
     transitively, but no MD5). *Proposed:* add the RustCrypto `md-5` crate (pairs
     with the `digest` crate already present; pure-Rust, no C dep). Hand-rolling
     MD5 is the alternative. Flagging because the project is security-conscious
     about crypto dependencies — operator's call.
   - **DESIGN NEEDED (proposal):** where the secure-login **password** comes from.
     It should flow from the OS keyring per the existing credential design
     (`docs/superpowers/specs/2026-05-18-cred-handling-design.md`, memory
     `no-disk-creds-default-to-keyring`), never from disk/env. This intersects the
     keyring work — worth the operator's input before wiring.
2. **Handshake** (`winlink/handshake.rs`, pure + testable). Build the `;FW:` line,
   the SID `[NAME-VER-CODES]` line, the `;PR:` secure-login response line, and the
   `; TARGET DE MYCALL (LOCATOR)` line; parse the remote's SID, `;FW`, and the
   `;PQ` password challenge. Reference: `wl2k-go/fbb/handshake.go`. Lines are
   **CR-terminated** (`\r`); reading trims whitespace + leading/trailing NUL
   (`wl2k-go/fbb/helpers.go::cleanString`). The pure string build/parse can be
   tested without a socket.
3. **Exchange driver** (`winlink/session.rs`). The turn loop: after the
   handshake, alternate "send our proposals + read FS + send accepted blocks" and
   "read proposals + verify checksum + answer FS + read accepted blocks", handling
   `FF` (no more) / `FQ` (quit) / `F>` (end of batch). Reference:
   `wl2k-go/fbb/b2f.go` (`handleOutbound`/`handleInbound`) + `wl2k.go::Exchange`.
   Testable with a **mock duplex transport** (scripted reader + capture writer) —
   no transmission. **Coupled** to a mailbox/handler interface (what to send,
   where to store received), so design it alongside step 5.
4. **Telnet transport** (`winlink/telnet.rs`). TCP connect, then run the handshake
   + exchange over the socket. **The connect path transmits.** Per
   `docs/live-cms-testing-policy.md` and the RADIO-1 pitfall, the live-CMS test is
   **operator-run, per-run consent-gated — the agent must not transmit.** Write
   the code, let the licensee run it. (Telnet to the CMS is internet, not RF, but
   keep to the consent gate per the policy.)
5. **`NativeBackend`** implementing the existing `WinlinkBackend` trait
   (`src-tauri/src/winlink_backend.rs`, 8 async methods) so the app can use the
   native client in place of `PatBackend`. This defines the mailbox interface that
   step 3 couples to.
6. **Compare native vs Pat** over the same saved messages; once they match, switch
   the default to native, then later remove the Pat sidecar (do **not** remove Pat
   before parity — the live mailbox view depends on it).
7. **New ADR** recording the escalation from ADR 0011 ("fork and patch Pat") to
   "replace Pat with native Rust." Records a decision already made; not a blocker.

## State

- **Branch:** `bd-tuxlink-0ic/native-winlink-client` (off `feat/v0.0.1`), **pushed
  (ahead 0 after this session's push)**. Worktree:
  `worktrees/bd-tuxlink-0ic-native-winlink-client/`. No PR opened yet — open one
  against `feat/v0.0.1` when the milestone is further along, or sooner for smaller
  reviews (the data plane is a clean, self-contained review unit if you want to
  land it now).
- **Working tree:** clean. No stashes. Untracked: none. Gitignored-on-disk: only
  build artifacts (`src-tauri/target/`, `src-tauri/gen/schemas/`,
  `src-tauri/sidecars/pat-*`) — nothing at risk per ADR 0009.
- **bd:** `tuxlink-0ic` stays `in_progress` (epic continues; data plane done,
  control plane pending). Note appended this session.
- **New module layout:** `src-tauri/src/winlink/` now has `mod.rs`, `message.rs`,
  `proposal.rs`, `lzhuf.rs`, `transfer.rs`, and `testdata/lzhuf/`.
