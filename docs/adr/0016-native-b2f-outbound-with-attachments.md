# 16. Native B2F outbound with attachments; Pat removed

Date: 2026-05-30
Status: Accepted
Deciders: cameronzucker, magpie-grouse-shoal (via 5-round adversarial review including Codex R5)

## Context

Tuxlink v0.0.1 was built on Pat ([ADR 0003](0003-no-sqlite-pat-owns-mailbox.md)) — the upstream `la5nta/pat` Go binary, then (per [ADR 0011](0011-fork-pat-for-tuxlink.md)) a tuxlink-owned fork called `tuxlink-pat`, managed as a git submodule at `external/tuxlink-pat/` and compiled into a sidecar binary at build time. Tuxlink's Tauri backend spawned the sidecar as a child process and spoke its REST API for Winlink operations. ADR 0011 forked Pat to address plaintext-credential storage; the keyring refactor shipped.

Over the following months a native Winlink engine was built incrementally in Rust for the read side (CMS connection, B2F session, mailbox, AX.25 packet transport, ARDOP HF core). By 2026-05-21 that engine was handling real messages against the Winlink CMS test server. The remaining hold-out was outbound attachments: `NativeBackend::send_message` silently discarded the `attachments` field on its `OutboundMessage` input, leaving Pat live as the sole attachment-capable path.

On 2026-05-30 the HTML Forms v0.1 spec (PR #151) committed to "Path A" — routing form XML through Pat's REST API — because native attachment encoding was not yet available. The operator decision that day: complete the native attachment work first, then delete Pat in the same PR, then resume HTML Forms on a clean native spec. The rationale was that a transient half-cutover state (NativeBackend with attachments AND PatBackend) would allow Pat to be silently re-activated by wizard or config quirks, and that shipping the dependency gap in the HTML Forms spec (which would outlive the PR) was worse than a brief pivot.

The Pat sidecar bundling imposed:

- A Go toolchain requirement on every source build.
- A `build.rs` invocation of `bash make.bash` inside the submodule on release builds.
- A process-lifetime IPC layer (spawn, HTTP REST, signal-based shutdown) for every outbound operation.
- A forked Go codebase (`tuxlink-pat`) to maintain alongside the Rust engine.
- A `external/tuxlink-pat` submodule that every clone had to recurse.

The attachment-support gap was the last technical reason to keep Pat alive.

## Decision

Strip Pat entirely and implement native B2F outbound with attachments in the same PR.

1. **`NativeBackend` is the sole `WinlinkBackend` implementation.** `PatBackend`, `PatBackendSpawnOptions`, and all Pat install sites (bootstrap, `AppBackend` enum, wizard) are deleted.

2. **`NativeBackend::send_message` encodes attachments in Winlink B2F wire format**, verified byte-for-byte against the `wl2k-go` v1.0.1 golden fixture (`lzhuf/testdata/LPE5NXDVLVSQ.b2f`, 31 380 bytes). See Wire format reference below.

3. **`WinlinkBackend` trait return type tightened** from `Result<Option<MessageId>, BackendError>` to `Result<MessageId, BackendError>`. The `None` branch existed solely because Pat 1.0.0 did not echo the MID on submission; that limitation no longer applies.

4. **Pat module tree deleted**: `src-tauri/src/pat_client.rs`, `src-tauri/src/pat_config.rs`, `src-tauri/src/pat_process.rs`, and the `PatBackend` impl block from `winlink_backend.rs`. Integration tests that use PatBackend are removed from the test files (partial edits, not wholesale deletions); non-Pat coverage is preserved.

5. **Build infrastructure deleted**: `build.rs` Go-toolchain check and `bash make.bash` invocation; `build_support.rs`; `src-tauri/sidecars/` directory; `tauri.conf.json` `externalBin` entry; `.github/workflows/release.yml` Go-toolchain setup and Pat-sidecar build steps.

6. **Submodule deleted**: `git submodule deinit -f external/tuxlink-pat`; `git rm external/tuxlink-pat`; `[submodule "external/tuxlink-pat"]` block removed from `.gitmodules`. The forked repo at `github.com/cameronzucker/tuxlink-pat` survives as historical record; no tuxlink PR gates on its deletion.

7. **Operator-state migrations**:
   - `Config.pat_mbo_address` field: kept `pub` with `#[serde(default, skip_serializing)]` (write-side suppression) and `#[deprecated]` annotation; on deserialize with a non-None value, logs a one-shot deprecation warning. Full field removal deferred to the next major bump.
   - Keyring service name renamed from `"tuxlink-pat"` to `"tuxlink"`. `winlink::credentials::read_password` reads the new name first; on `NoEntry`, transparently migrates from the old name (read → write new → delete old) and logs `"migrated keyring entry: tuxlink-pat -> tuxlink"`.

8. **ADR 0003 and ADR 0011** Status lines appended with `superseded by ADR 0016` in the same PR.

9. **HTML Forms spec rev-2 → rev-3** in the same PR: removes Path A (Pat REST) choice; pins the native attachment path as the only encoding path for v0.1.

## Wire format reference

A complete Winlink B2F message with attachments, before lzhuf compression, has this exact byte shape (verified against `wl2k-go fbb/message.go:466-478` and golden fixture `LPE5NXDVLVSQ.b2f`):

```
Mid: <12-char base32>\r\n
Body: <text_body_byte_count>\r\n
Content-Transfer-Encoding: 8bit\r\n
Content-Type: text/plain; charset=ISO-8859-1\r\n
Date: YYYY/MM/DD HH:MM\r\n
File: <byte_count> <filename>\r\n    ← one per attachment, in declaration order
From: <station_address>\r\n
Mbo: <station_address>\r\n
Subject: <subject>\r\n
To: <recipient_address>\r\n
Type: Private\r\n
\r\n                                 ← header/body separator (canonical \r\n\r\n)
<text_body_bytes>                    ← exactly Body: byte count
\r\n                                 ← IFF message has >= 1 attachment
<attachment_1_bytes>                 ← exactly File: byte count for attachment 1
\r\n                                 ← after every attachment, including the last
```

Key invariants:

- **Header sort**: `Mid:` first; all remaining headers in canonicalized-key alphabetical order (`textproto.CanonicalMIMEHeaderKey`). Multiple `File:` entries preserve insertion order within their sort slot (`F...` falls between `Date:` and `From:`).
- **Multi-recipient**: `To:` and `Cc:` are repeated per address, NOT comma-joined. Empty Cc list emits zero `Cc:` headers.
- **Trailing CRLF**: one `\r\n` between body and first attachment; one `\r\n` after every attachment. Without these, `wl2k-go::readSection` errors with `"Unexpected end of section"`.
- **Non-ASCII filenames**: RFC 2047 Q-encoded as `=?ISO-8859-1?q?...?=` (lowercase `q`). Non-Latin-1 filenames (CJK, emoji) are rejected at compose time with a typed `ComposeError::FilenameNotLatin1Encodable`.
- **`compressed_size` in proposal**: includes the 6-byte lzhuf framing prefix (2-byte CRC16 LE + 4-byte uncompressed-length LE per `lzhuf/writer.go:99-113`); minimum valid value is 6.

The message format is **not MIME multipart**. There are no `boundary=...` markers, no per-part headers, and no base64 transfer encoding of binary attachments. Attachments are raw bytes appended directly after the text body. This matches what wl2k-go and Winlink Express produce; CMS expects this format.

## Alternatives considered

### Path A — extend Pat REST API for attachments

`pat_client.rs` could have been extended to POST form XML as a multipart attachment to Pat's `/api/mailbox/Outbox`. This was the HTML Forms spec rev-2 "Path A" decision.

**Rejected** because it perpetuates the Go runtime + sidecar process + REST IPC layer + submodule maintenance burden for a feature the native engine can own directly. The native attachment implementation costs ~350 LOC of Rust (`compose.rs` extension + `message.rs` parser extension); it does not require a Go toolchain in CI or a sidecar in production builds. The break-even on implementation cost was immediate given that the full Pat stack was the alternative.

### MIME multipart in the message body

One prior design iteration (rev-1 of the design spec) assumed that attachments were MIME-multipart-encoded in the message body, which would have matched standard email encoding patterns.

**Rejected** because it is factually wrong. The Winlink B2F format uses `File:` headers + raw appended bytes, not MIME multipart. Implementing MIME multipart would have produced messages that wl2k-go and the Winlink CMS would reject or misparse. The wire format section above is the corrected ground truth, verified against `wl2k-go`'s source and golden fixture.

### Phased rollout — native attachments first, delete Pat in a follow-up PR

Ship native attachment encoding first, then delete Pat in a subsequent PR once confidence is established.

**Rejected** because the transient half-cutover state — NativeBackend handling attachments AND PatBackend still present and addressable — introduces a class of failure where a wizard quirk, config migration, or operator override silently re-selects PatBackend after the operator believed they were on the native path. The atomic cutover in one PR eliminates the ambiguous state. The 13-phase commit decomposition keeps each individual commit reviewable; the all-or-nothing semantics are enforced at PR-merge time.

## Watched failure modes

1. **Receiver-side parser misreads `Body:` size and corrupts attachment offsets.** This silently drops attachment bytes or misattributes them to the text body. *Mitigation*: golden-vector test (`LPE5NXDVLVSQ.b2f`) asserts that `Message::from_bytes(serialize_then_compress(msg))` round-trips to the original message byte-for-byte for body and all attachments. Round-trip tests cover single-attachment, multi-attachment, and zero-attachment cases.

2. **Non-Latin-1 filenames silently produce lossy Q-encoding.** `mime.QEncoding.Encode("ISO-8859-1", name)` truncates or substitutes non-Latin-1 characters without error. *Mitigation*: `compose_message_with_files` validates all filenames before emitting any headers; `ComposeError::FilenameNotLatin1Encodable` surfaces to the UI as a typed, named error before any wire encoding begins.

3. **Operator with stale config tries to re-add Pat without reading this ADR.** After the Pat module tree is deleted, `lib.rs` no longer declares `pub mod pat_client`, `pub mod pat_config`, or `pub mod pat_process`. Any attempt to reference `PatBackend` in new code fails at compile time. *Mitigation*: this ADR explicitly documents the deletion rationale; the compile-time failure is the primary gate.

4. **Keyring migration fails silently on first upgrade.** An operator running a post-strip build for the first time has a keyring entry under `"tuxlink-pat"` but not `"tuxlink"`. If the migration read from `"tuxlink-pat"` fails (keyring locked, daemon not running), the operator sees an `AuthFailed` error rather than a migration-specific message. *Mitigation*: `read_password` logs `"migrated keyring entry: tuxlink-pat -> tuxlink"` on success; on `NoEntry` for the old name (i.e., no migration to perform), the caller receives `NotConfigured` immediately with the standard "run the wizard" hint; on other keyring errors, the error message includes the keyring backend's own diagnostic string which operators recognize from wizard-troubleshooting context.

## Migration / cutover

The Pat removal is executed as a 13-phase commit decomposition on `bd-tuxlink-9phd/strip-pat-add-native-attachments`. Each phase is a single focused commit:

- P0: Move `MailboxFolder` enum out of `pat_client.rs` into `winlink_backend.rs` (where it is already re-exported) before any deletion touches the file.
- P1–P6: Native compose + parser extension + trait tightening + install-site flips.
- P7: Keyring service-name migration.
- P8: `Config.pat_mbo_address` deprecation.
- P9: Pat module + Pat tests deleted.
- P10: Build infrastructure + sidecar + CI deleted.
- P11: Submodule deinit + `.gitmodules` cleanup.
- P12: Docs + ADR sweep (this file).

All 13 commits merge as a single PR per [ADR 0010](0010-no-squash-merge.md) (no-squash). Rollback path: `git revert -m 1 <merge-sha>` reverts the merge commit; the 13-commit decomposition makes the revert clean.

Full operator-state migration details and the operator smoke procedure are documented in the [design spec](../superpowers/specs/2026-05-30-pat-strip-native-attachments-design.md) §7.

## Consequences

**Positive:**

- Go toolchain removed from release build requirements. CI no longer needs a Go setup step. Build time shrinks by the `bash make.bash` invocation.
- No sidecar process in production. The AppImage no longer bundles a Pat binary. Process count drops by one at runtime.
- Single source of truth for outbound message encoding. `winlink::compose` owns the format; the format is documented here and tested against the golden vector.
- `WinlinkBackend` trait is simpler: `Result<MessageId, BackendError>` with no `Option` escape hatch.
- HTML Forms v0.1 (PR #151) is unblocked to resume against a spec that does not depend on Pat.
- Approximately −2 941 LOC of Go+Rust Pat code removed; approximately +360 LOC of native compose+parse+credentials code added.

**Negative:**

- Any future "fall back to Pat" affordance requires reintroducing the Go dep, the sidecar, and the submodule. Given that Pat is deleted at the module level and not re-added, the cost of reversal is non-trivial. This is intentional — the decision is a one-way door.
- The keyring rename has a one-time migration moment. Operators without network access at first launch (e.g., an airgapped EmComm exercise) who have a stale `"tuxlink-pat"` keyring entry will still see the keyring migration succeed (migration is local), but the subsequent CMS connection attempt will fail as expected without network. The migration itself is not network-dependent.
- Operators with Pat installed separately as a standalone tool are unaffected — tuxlink's Pat deletion does not remove any system-level Pat binary the operator may have installed independently.
