# Strip Pat + native B2F outbound with attachment support — design spec (rev-1)

> Date: 2026-05-30 · Agent: magpie-grouse-shoal · bd: tuxlink-9phd
> Parent context: PR #151 (HTML Forms v0.1) is paused mid-Phase-0 behind this prerequisite. Operator directive 2026-05-30: pivot to Path B + complete the Pat strip. See [`dev/handoffs/2026-05-30-heron-tanager-bog-html-forms-paused-pat-strip-prereq.md`](../../../dev/handoffs/2026-05-30-heron-tanager-bog-html-forms-paused-pat-strip-prereq.md).

## 0. Status — DESIGN ONLY (HARD-GATE per `superpowers:brainstorming` + `build-robust-features`)

This is the pre-adversarial design. Per BRF, **no implementation code** is committed until this spec is approved AND a `superpowers:writing-plans` implementation plan is reviewed.

Approval flow:

- **Approve / merge** → 5-round cross-provider adversarial review (Claude rounds + at least one Codex round, per the project's no-carveout-on-cross-provider-adrev policy) → `superpowers:writing-plans` → plan review cycle (min 3 rounds) → execution recommendation → operator decision on approach.
- **Comment** → revise spec + re-run any affected adrev round.
- **Reject** → regroup.

## 1. Purpose

`NativeBackend::send_message` silently discards the `attachments` field on its `OutboundMessage` input. Until that ends, `PatBackend` — a 657-LOC Rust module + ~360 LOC in `winlink_backend.rs` + a Go sidecar bundled into release builds + a forked submodule at `external/tuxlink-pat` — has to stay live to handle the half of outbound users care about (everything beyond plain text). Once attachments work natively, every Pat surface deletes in the same cutover.

The operator decision on 2026-05-30 promoted "native B2F outbound with attachments" from a v0.5+ aspiration (per the HTML Forms spec §5.1 Path B) to a P1 prerequisite blocking forward outbound work. The spec covers both the new outbound code AND the Pat-deletion surgery; they happen in one PR because they are inseparable — the build does not compile with `PatBackend` deleted and `NativeBackend::send_message` still silently dropping attachments.

**In scope**:

1. Native compose for messages with one or more attachments, in Winlink B2F wire format as verified against `wl2k-go` (the canonical reference per [[feedback_winlink_re_authoritative_sources]]).
2. `NativeBackend::send_message` wires attachments through the existing session/transfer/lzhuf pipeline.
3. All `PatBackend` install sites (bootstrap, app_backend, wizard) flipped to `NativeBackend`.
4. Pat module tree + tests + Go-build infra + sidecar bundling + `external/tuxlink-pat` submodule **deleted**.
5. ADR 0003 + ADR 0011 marked **Superseded**; new ADR documents the cutover.
6. Operator-driven CMS-telnet smoke (authorized per [[feedback_cms_telnet_testing_authorized]]) verifying real round-trip of a small-attachment message against `cms-z.winlink.org`.

**Out of scope** (filed as separate bd issues if not already):

- Inbound attachment display in the UI (current behavior — render raw RFC 5322 text including the binary attachment blob — is unchanged in this PR; UI parsing is a follow-up).
- HTML Forms support (`tuxlink-v1p`) — this prereq unblocks it; HTML Forms resumes against PR #151 after this lands.
- The `tuxlink-pat` GitHub repo deletion — stays as historical record; project policy does not gate GitHub repo cleanup.
- RF on-air validation — telnet against `cms-z.winlink.org` is the v0.1 smoke; RF transports are unchanged by this work.
- Any v0.5+ modem work, transport changes, ARDOP, AX.25 — unaffected.

## 2. Wire format — Winlink B2F message with attachments

Load-bearing finding from reading `wl2k-go/fbb` (v1.0.1 at `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/`), the authoritative Winlink protocol reference per established project convention. Two prior summaries (the bd issue scope and an audit subagent) characterised this format as "MIME multipart in the body"; that is **wrong**. The format is a Winlink-custom one.

A complete outbound message with attachments, **before** lzhuf compression, is one byte blob with this exact shape:

```
Mid: <12-char base32>\r\n
Body: <text_body_byte_count>\r\n
Content-Transfer-Encoding: 8bit\r\n
Content-Type: text/plain; charset=ISO-8859-1\r\n
Date: YYYY/MM/DD HH:MM\r\n
File: <byte_count> <filename>\r\n           ← one per attachment
File: <byte_count> <filename>\r\n           ← in declaration order within the slot
From: <station_address>\r\n
Mbo: <station_address>\r\n
Subject: <subject>\r\n
To: <recipient_address>\r\n
Type: Private\r\n
\r\n                                         ← single CRLF separates headers from body section
<text_body_bytes>                            ← exactly `Body:` bytes (no trailing CRLF if the header says n)
<attachment_1_bytes>                         ← exactly `File:` 1 byte count
<attachment_2_bytes>                         ← exactly `File:` 2 byte count
```

Header ordering: `Mid:` is emitted first; all other headers are emitted in alphabetical order (per `wl2k-go/fbb/header.go:99-133`). `Cc:` headers (multi-recipient) and `To:` headers (multi-recipient) are repeated per address, NOT comma-joined.

The B2F protocol layer (proposal exchange + framed transfer) is **attachment-agnostic**. The proposal `FC EM <mid> <size> <compressed_size> 0` carries the uncompressed total byte count (headers + body + every attachment), with `<compressed_size>` being the post-lzhuf byte count of the same blob. The `transfer::frame_block` codec streams the compressed bytes in 125-byte chunks within `SOH ... STX ... EOT` envelopes; it does not parse the inner format. The receiver lzhuf-decompresses, parses headers, reads `Body:` bytes as text, then reads each `File:`-headered attachment in declaration order.

### 2.1 Golden vector

`wl2k-go` ships test fixtures at `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/tests/`. One fixture (referenced in their `message_test.go`) is `LPE5NXDVLVSQ.b2f` — uncompressed 31 KB with a 31028-byte JPG attachment named `1469042410710.jpg`. Headers (extract):

```
Mid: LPE5NXDVLVSQ
Body: 104
Content-Transfer-Encoding: 8bit
Content-Type: text/plain; charset=ISO-8859-1
Date: 2016/07/20 19:21
File: 31028 1469042410710.jpg
From: LA5NTA
Mbo: LA5NTA
Subject: 73 fra Brekke
To: LA4TTA
Type: Private

[104 bytes of text body]
[31028 bytes of binary JPG data]
```

This fixture is the authoritative byte-level conformance target for the Rust serializer (see §8 test plan).

### 2.2 Filename encoding (RFC 2047 Q)

Filenames containing non-ASCII characters are RFC 2047 Q-encoded by `wl2k-go` via `mime.QEncoding.Encode("utf-8", name)` (`message.go:437`). Without this, the CMS may either reject or silently corrupt non-ASCII filenames. The Rust port reproduces this behaviour: ASCII-safe filenames are emitted verbatim; non-ASCII filenames are wrapped as `=?utf-8?Q?...?=`. The wl2k-go `WordDecoder` (used on receive) handles all three forms (UTF-8, ISO-8859-1, RFC 2047 Q-encoded), so emitting RFC 2047 Q is forward-compatible.

Filename length is capped at 255 characters by `wl2k-go/fbb/message.go:146`. The Rust port emits the same cap and rejects oversize filenames at compose time with a typed error.

### 2.3 What this format is NOT

To prevent regression to either prior misconception:

- **NOT MIME multipart**: no `multipart/mixed`, no `boundary=...`, no per-part headers, no base64 transfer encoding of binary attachments. Attachments are raw bytes appended directly.
- **NOT a single `body` blob with attachments as foreign elements**: the `Body:` header is the count of *text body* bytes only; attachments are NOT part of the text body count.
- **NOT separated by markers between attachments**: attachment N+1's bytes follow attachment N's bytes with no separator; the parser delimits using each `File:` header's byte count.

## 3. Architecture — what changes vs. what stays

The change is **two files of substantive code edits** (compose.rs, message.rs) plus deletion of Pat module + install-site rewires.

```
                  ┌─────────────────────────────────────────────────┐
                  │ OutboundMessage { to, cc, subject, body,        │
                  │                   attachments: Vec<OutboundAttachment> }
                  │ (already on main via PR #151 T0.1)               │
                  └────────────────────────┬────────────────────────┘
                                           ↓
   NEW           ┌────────────────────────────────────────────────┐
                  │ winlink::compose::compose_message_with_files(   │
                  │   mycall, to, cc, subject, body, files, t)      │
                  │   → Message                                     │
                  └────────────────────────┬────────────────────────┘
                                           ↓
   CHANGED       ┌────────────────────────────────────────────────┐
                  │ winlink::message::Message — gains files field   │
                  │ Message::to_bytes() — emits File: headers       │
                  │   and appends raw file bytes after body.        │
                  │ Message::to_proposal() — size includes files.   │
                  └────────────────────────┬────────────────────────┘
                                           ↓
   UNCHANGED     ┌────────────────────────────────────────────────┐
                  │ winlink::lzhuf::compress(&[u8]) → Vec<u8>       │
                  │ winlink::transfer::frame_block(...)             │
                  │ winlink::session::send_turn / run_exchange      │
                  │ winlink::telnet::connect_and_exchange           │
                  │ winlink::handshake / proposal / secure / wire   │
                  └─────────────────────────────────────────────────┘
                                           ↓
   CHANGED       ┌────────────────────────────────────────────────┐
                  │ NativeBackend::send_message — passes msg.       │
                  │   attachments to compose_message_with_files,    │
                  │   stores resulting Message bytes in outbox.     │
                  └─────────────────────────────────────────────────┘
   DELETED       ┌────────────────────────────────────────────────┐
                  │ PatBackend impl + PatBackendSpawnOptions        │
                  │ src/pat_client.rs, src/pat_config.rs,           │
                  │   src/pat_process.rs                            │
                  │ tests/pat_client_test.rs, pat_config_test.rs,   │
                  │   pat_process_test.rs                           │
                  │ tauri.conf.json externalBin "sidecars/pat"      │
                  │ build.rs Go-toolchain + go-build path           │
                  │ src/bin/live_cms_smoke.rs (Pat-based probe)     │
                  │ external/tuxlink-pat submodule (deinit + rm)    │
                  └─────────────────────────────────────────────────┘
   REWIRED       ┌────────────────────────────────────────────────┐
                  │ bootstrap.rs — drop resolve_pat_binary +        │
                  │   PatBackend::spawn + the spawn thread;         │
                  │   install NativeBackend directly                │
                  │ app_backend.rs — test fixtures switch to        │
                  │   NativeBackend equivalent (TBD: see §4.4)      │
                  │ wizard.rs — test-send path switches to          │
                  │   NativeBackend                                 │
                  │ ui_commands.rs — LogSource::Pat → ::Backend     │
                  └─────────────────────────────────────────────────┘
```

## 4. API surface — new + changed

### 4.1 `winlink::compose::compose_message_with_files`

New function. Keeps the existing `compose_message` (text-only) for callers that don't need attachments; both forward to a shared inner helper. Single-purpose functions compose better than optional-param creep, and the plain-text path is still common.

```rust
/// Build a Private text message with zero or more file attachments.
///
/// Files are appended to the message in declaration order. Filenames are
/// RFC 2047 Q-encoded if they contain non-ASCII characters. Filenames longer
/// than 255 chars are rejected at compose time. Empty files (`data.is_empty()`)
/// are valid and produce `File: 0 <name>` headers with no body bytes.
pub fn compose_message_with_files(
    mycall: &str,
    to: &[&str],
    cc: &[&str],
    subject: &str,
    body: &str,
    files: &[File],
    unix_secs: u64,
) -> Result<Message, ComposeError>;

#[derive(Debug, Clone)]
pub struct File {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum ComposeError {
    #[error("filename exceeds 255-character limit: {0} chars")]
    FilenameTooLong(usize),
}
```

`compose_message` (text-only) keeps its existing signature (`-> Message`, no `Result`); it cannot fail because it has no filename inputs. Implementation:

```rust
pub fn compose_message(
    mycall: &str,
    to: &[&str],
    cc: &[&str],
    subject: &str,
    body: &str,
    unix_secs: u64,
) -> Message {
    // SAFETY: no files → no ComposeError possible.
    compose_message_with_files(mycall, to, cc, subject, body, &[], unix_secs)
        .expect("compose_message_with_files cannot fail with empty files slice")
}
```

This avoids duplicating compose logic and keeps the existing call sites compiling without change.

### 4.2 `winlink::message::Message` — `files` field

`Message` gains:

```rust
pub struct Message {
    headers: Vec<(String, String)>,   // existing
    body: Vec<u8>,                    // existing
    files: Vec<File>,                 // NEW (re-export from compose for convenience)
}
```

`Message::to_bytes()` (existing) is extended:

1. Compute `Body: <text_body_len>` header — set automatically when `body` is set; recomputed if body changes.
2. For each `File` in `files`, ensure a `File: <size> <name>` header exists.
3. Sort headers: `Mid:` first (existing behaviour), then alphabetical (existing). The new `File:` headers naturally land between `Date:` and `From:` — verify by test.
4. Write headers + blank line (single `\r\n`).
5. Write `body` bytes (length must match `Body:` header exactly).
6. For each file in `files` (in declaration order — NOT alphabetical-by-name, NOT by header sort): write file's raw `data` bytes.

`Message::to_proposal()` (existing) is extended: the byte slice passed to `lzhuf::compress` is the full `to_bytes()` output (headers + body + all files concatenated). The proposal's `size` field already reads `data.len()` post-extension — no separate change needed.

### 4.3 `winlink_backend::NativeBackend::send_message`

Currently (per audit) calls `compose::compose_message(...)` with only the text-body args, silently discarding `msg.attachments`. After change:

```rust
async fn send_message(&self, msg: OutboundMessage) -> Result<Option<MessageId>, BackendError> {
    let files: Vec<File> = msg.attachments
        .into_iter()
        .map(|a| File { name: a.filename, data: a.bytes })
        .collect();
    let to_refs: Vec<&str> = msg.to.iter().map(String::as_str).collect();
    let cc_refs: Vec<&str> = msg.cc.iter().map(String::as_str).collect();

    let message = compose_message_with_files(
        &self.callsign,
        &to_refs,
        &cc_refs,
        &msg.subject,
        &msg.body,
        &files,
        unix_secs_from_rfc3339(&msg.date)?,
    ).map_err(|e| BackendError::InvalidInput(e.to_string()))?;

    let id = message.header("Mid").unwrap().to_string();
    self.mailbox.store(MailboxFolder::Outbox, &id, &message.to_bytes())
        .map_err(|e| BackendError::Storage(e.to_string()))?;
    Ok(Some(MessageId::from(id)))
}
```

The `content_type` field on `OutboundAttachment` is unused by the wire format (B2F does not carry per-attachment MIME types) but kept on the struct for future inbound parsing and sender-side UI affordances.

### 4.4 `app_backend::BackendState` test fixtures

Per audit, `app_backend.rs` lines 161, 173, 219 use `PatBackend::from_url("http://127.0.0.1:9")` to construct a stub backend for unit tests. After Pat deletion, these need a `NativeBackend` equivalent. Two options:

- **Option A**: Add `NativeBackend::test_fixture()` factory that returns a backend backed by an in-memory mailbox + no telnet (panics on `connect`/`send_message`). Minimal, test-only.
- **Option B**: Refactor the tests to skip backend construction entirely (they exercise `BackendState::install` lifecycle, not backend internals; a `Mock` impl of the `WinlinkBackend` trait works).

The plan picks Option A (smaller diff, preserves the test pattern). The factory lives behind `#[cfg(test)]`.

### 4.5 Removed APIs

- `pat_client::PatClient` (entire struct + impl)
- `pat_config::{render_pat_config, write_pat_config_atomic, PatConfigError}`
- `pat_process::{PatProcess, PatSpawnOptions}`
- `winlink_backend::{PatBackend, PatBackendSpawnOptions}`
- `bootstrap::{resolve_pat_binary, resolve_pat_binary_inner, is_nonempty_file}` (Pat-specific helpers)
- `SIDECAR_STUB_REASON` const in bootstrap.rs
- `LogSource::Pat` variant in `ui_commands.rs` — renamed to `LogSource::Backend`

## 5. Pat removal phases (commit-level decomposition)

CI must be green at the end of every commit. The atomic invariant: `PatBackend` is referenced in code IFF the file declaring it exists. Therefore:

| Phase | Commits | What |
|---|---|---|
| **P1: Native compose-with-files** | 1 commit | Add `File` struct + `compose_message_with_files` fn + `ComposeError`; extend `Message` with `files` field + serializer; add 8-12 tests including the wl2k-go golden vector. PatBackend untouched; no functional change for existing callers. |
| **P2: NativeBackend wires attachments** | 1 commit | `NativeBackend::send_message` calls `compose_message_with_files` and stores the resulting `Message::to_bytes()`. Update `two_native_backends_exchange_*` integration tests to include an attachment; assert receipt round-trip. PatBackend untouched. |
| **P3: Flip install sites** | 1 commit | `bootstrap.rs`: delete `resolve_pat_binary` + `PatBackend::spawn` + the dedicated spawn thread; replace with `NativeBackend::new(...)` synchronous install. `app_backend.rs` tests: `PatBackend::from_url` → `NativeBackend::test_fixture()`. `wizard.rs`: ephemeral Pat spawn → `NativeBackend` test-send. `ui_commands.rs`: rename `LogSource::Pat` → `::Backend`. After this commit, no caller references `PatBackend`. Tests still pass because PatBackend exists; it's just unused. |
| **P4: Delete Pat module** | 1 commit | `rm src-tauri/src/pat_client.rs src-tauri/src/pat_config.rs src-tauri/src/pat_process.rs`. Delete `PatBackend` + `PatBackendSpawnOptions` from `winlink_backend.rs`. `rm src-tauri/tests/pat_client_test.rs src-tauri/tests/pat_config_test.rs src-tauri/tests/pat_process_test.rs`. Remove `pub mod pat_client; pub mod pat_config; pub mod pat_process;` from `lib.rs`. Delete `src-tauri/src/bin/live_cms_smoke.rs` (Pat-based probe). |
| **P5: Delete sidecar infra** | 1 commit | `tauri.conf.json`: remove `"externalBin": ["sidecars/pat"]`. `build.rs`: delete Go-toolchain check, `go build` invocation, 0-byte-stub creation (the file shrinks dramatically — flag this in the commit body). Delete the `src-tauri/sidecars/` directory entirely (audit confirmed it contained only the Pat sidecar; no other consumers). |
| **P6: Submodule deinit** | 1 commit | `git submodule deinit external/tuxlink-pat && git rm external/tuxlink-pat`. Delete the `[submodule "external/tuxlink-pat"]` block from `.gitmodules`. The forked repo at `github.com/cameronzucker/tuxlink-pat` survives; this only deinits the local linkage. |
| **P7: ADR mutations** | 1 commit | Edit ADR 0003 + ADR 0011 to add `Status: Superseded by ADR 0016` lines. Write new ADR 0016 "Native B2F outbound with attachments; Pat removed" capturing the wl2k-go-verified wire format, the cutover sequence, and the deletion scope. |

Each phase is a separate commit on `bd-tuxlink-9phd/strip-pat-add-native-attachments`. The polish-before-push rule (CLAUDE.md §"Commit and release discipline") applies: rebase locally for cleanup, push the cleaned sequence, do NOT amend after push. Per the no-squash ADR 0010, the merge will preserve all 7 commits on main.

**Why single PR for all 7 phases**: the alternative ("ship P1+P2 first, run for a few days, then ship P3+ later") is tempting but introduces a transient state where `NativeBackend::send_message` handles attachments AND `PatBackend::send_message` still exists. Both are reachable through different install sites, and a config/wizard quirk could resurrect Pat after we thought we'd retired it. Atomic cutover is cleaner.

## 6. ADR mutations

### 6.1 ADR 0003 — Status: Superseded

Original decision (2026-05-05): "No SQLite in v0.0.1; Pat owns the mailbox; tuxlink reads via HTTP API on demand."

Amendment text added by this PR:

> **Status (2026-05-30): Superseded by ADR 0016.** The native Winlink client now owns the mailbox via `NativeBackend` + `winlink::session`. The "Pat owns the mailbox" decision was correct for v0.0.1 (avoid premature mailbox-storage abstraction) but is replaced by native ownership in this PR. The "no SQLite" half of the decision still holds — the mailbox stays file-system-backed (one RFC 5322 file per message in folder directories).

### 6.2 ADR 0011 — Status: Superseded

Original decision (2026-05-18): "Fork upstream Pat as `tuxlink-pat`; refactor cred-handling first; keyring patch as first commit; upstream-contribution policy."

Amendment text added by this PR:

> **Status (2026-05-30): Superseded by ADR 0016.** Pat is completely removed from tuxlink. The forked credentials work (OS keyring integration) is no longer load-bearing — the native backend reads credentials directly from the keyring without Pat as an intermediary. The fork repo at `github.com/cameronzucker/tuxlink-pat` survives as a historical reference (not deleted), but is no longer a dependency of this project.

### 6.3 New ADR 0016

Title: "Native B2F outbound with attachments; Pat removed"

Key sections:

- **Status**: Accepted.
- **Context**: How we got here — Pat as v0.0.1 expedient, native client built incrementally for read-side, half-cutover state by 2026-05-21, operator decision 2026-05-30 to finish the cutover before HTML Forms.
- **Decision**: Native B2F outbound with attachments per the wire format documented in §2 of this spec. Pat module + sidecar + submodule deleted in the same PR.
- **Wire format**: Reproduce the §2 byte-layout reference for future operators (independent of this spec file).
- **Alternatives considered**: (a) Path A — extend Pat via REST for attachments. Rejected because it perpetuates a Go runtime + sidecar + Pat-side bug surface for a feature we own. (b) MIME multipart in the body. Rejected because it does not match what wl2k-go and the WLE reference produce; the CMS expects B2F-format messages.
- **Watched failure modes**: (1) Receiver-side parser misreads `Body:` size and corrupts attachment offsets. (2) Non-ASCII filename round-trip fails (RFC 2047 Q-encode misimplementation). (3) Future operator sees Pat traces in git history and tries to "re-add Pat for X" without reading this ADR.
- **Migration / cutover**: Reference the 7-phase commit decomposition in §5.

ADR length target: 150-200 lines. Standard ADR template applies.

## 7. Migration & operator-facing concerns

### 7.1 No user-visible behaviour change for the existing send path

Sending a plain-text message via the existing compose UI produces the same wire output before/after this PR (the new `compose_message_with_files` with `files=&[]` degenerates to the existing `compose_message` output byte-for-byte). The first golden test in §8 asserts this.

### 7.2 First user-visible new capability

After this PR lands, the UI can pass attachments through `OutboundMessage.attachments`. The current UI does NOT yet expose attachment-attach affordances — that's HTML Forms work (`tuxlink-v1p`) and other follow-up. This PR closes the backend gap that was blocking the UI.

### 7.3 No data migration

The existing native mailbox (`NativeBackend`) already stores RFC 5322 message files on disk. Outbox messages composed before this PR have no attachments; messages composed after this PR may have attachments embedded in their RFC 5322 file (per the wire format). The mailbox storage layer is bytes-in / bytes-out and does not need to interpret the message — no migration.

### 7.4 Operator smoke after merge

Run the extended `native_cms_probe` (or new `native_send_probe`) against `cms-z.winlink.org` with a small attachment (e.g., a 100-byte text file named `smoke.txt`):

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin native_send_probe -- \
  --to <SELF-CALL>@winlink.org \
  --subject "smoke" \
  --body "smoke test" \
  --attach /tmp/smoke.txt
```

Verify on the receiving end (Winlink web inbox or another client) that the attachment arrives intact. CMS telnet is authorized non-RF dev testing per the project's live-CMS testing policy (see [docs/live-cms-testing-policy.md](../../live-cms-testing-policy.md)). RADIO-1 does NOT apply — this is internet telnet, not RF.

### 7.5 Build implications

`build.rs` shrinks dramatically — no Go toolchain check, no `go build` invocation. Release builds no longer require Go installed; the prereq-check skill (and onboarding wizard) should drop Go from their requirement list. Flag this in the new ADR's "Watched failure modes" section: a future operator following an old setup doc may install Go unnecessarily.

## 8. Test plan (TDD)

Every code change is test-first per [`docs/pitfalls/testing-pitfalls.md`](../../../docs/pitfalls/testing-pitfalls.md) and the existing module convention.

### 8.1 Unit tests — `compose.rs`

| Test | Asserts |
|---|---|
| `composes_message_with_single_attachment` | Output has `File: <n> <name>` header; body section has text body + raw attachment bytes; `Body:` header matches text body length |
| `composes_message_with_multiple_attachments_preserves_order` | Two `File:` headers in declaration order; attachment bytes appended in declaration order; receiver could parse using header sizes |
| `composes_message_with_empty_attachment` | `File: 0 <name>` header present; zero bytes appended for that file; following attachments still align |
| `composes_message_with_no_attachments_matches_text_only_path` | Output of `compose_message_with_files(..., &[], t)` is byte-identical to `compose_message(..., t)` |
| `q_encodes_non_ascii_filenames` | Filename `café.txt` produces `File: <n> =?utf-8?Q?caf=C3=A9.txt?=` header (or equivalent valid Q-encoding) |
| `rejects_filename_over_255_chars` | `ComposeError::FilenameTooLong(n)` returned; n is actual char count |
| `composes_multi_recipient_with_attachments` | Three `To:` headers + one attachment; all present in output |

### 8.2 Unit tests — `message.rs`

| Test | Asserts |
|---|---|
| `header_order_mid_first_then_alphabetical_with_files` | `Mid:` first; remaining headers alphabetically; `File:` lands between `Date:` and `From:` |
| `to_bytes_includes_file_data_after_body` | After `\r\n\r\n` separator, `body` bytes then each file's bytes |
| `to_proposal_size_includes_attachment_bytes` | Proposal `size` = `headers + body + sum(file.data.len())` |
| `message_with_files_serializes_deterministically` | Two calls to `to_bytes()` on the same message produce identical output |

### 8.3 Golden vector test (highest correctness signal)

Vendor a copy of `wl2k-go`'s `LPE5NXDVLVSQ.b2f` fixture into `src-tauri/tests/fixtures/wl2k-go/LPE5NXDVLVSQ.b2f` (license-permitting — wl2k-go is GPL-2.0; this is a test fixture and license attribution lives in the test file header).

Build a Rust `Message` with the same headers (Mid, Date, From, To, Mbo, Subject, Type, Content-*) + the same text body + the same file (read raw bytes from the fixture file's attachment slice). Serialize via `to_bytes()`. Assert byte-for-byte equality against the fixture.

If the fixture cannot be vendored (license concern), the test reads the fixture file from `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/tests/` at test time and skips with a friendly message if absent. This is the conformance test that catches subtle wire-format drift (header sort, line terminator, body separator, file-byte alignment).

### 8.4 Integration tests — `winlink_backend.rs`

| Test | Asserts |
|---|---|
| `native_backend_send_message_stores_attachments` | Pass `OutboundMessage` with one attachment; verify outbox file contains `File:` header + attachment bytes |
| `two_native_backends_exchange_message_with_attachment` (extends existing test) | Sender → receiver via in-process telnet loopback; receiver parses message; attachment bytes round-trip intact |

### 8.5 Removed tests

The three Pat test files are deleted entire (`pat_client_test.rs`, `pat_config_test.rs`, `pat_process_test.rs`) — ~18 tests total. No transition tests needed because Pat code is gone in the same commit; nothing else depends on the Pat test surface.

### 8.6 Operator-driven CMS smoke (post-merge)

§7.4 above. Not a CI test; operator runs manually against `cms-z.winlink.org`.

## 9. Risks & open questions

Numbered for adversarial-review reference.

1. **Header sort with `File:`**: assertion is that `File:` lands between `Date:` and `From:` alphabetically. Verify by test. If wl2k-go puts `File:` somewhere else (e.g., grouped with `Mid:` or appended at end), the Rust port must match — adversarial review should validate by inspecting `wl2k-go/fbb/header.go:99-133` directly.
2. **`Body:` header math off-by-one**: a wrong `Body:` value silently corrupts both halves of the message body section. Mitigation: compute `Body:` from `body.len()` in the serializer (not by caller-supplied value); enforce in `set_body()`.
3. **Q-encoded filename round-trip**: receiver parsers (Pat, WLE) must accept the Q-encoded form. wl2k-go's `WordDecoder` decodes it on receive; verify this end-to-end.
4. **CMS rejection on unknown headers**: adding `File:` is standard B2F but verify the CMS does not flag tuxlink (whose client SID is not yet registered with the production Winlink CMS — `cms-z.winlink.org` is the dev CMS that accepts unregistered clients) for this. Smoke against `cms-z.winlink.org` confirms.
5. **lzhuf input size with large attachments**: a 50 MB attachment is lzhuf-compressed in-memory as one buffer. Pi 5 has plenty of RAM but a future low-memory target (some other SBC) could OOM. No hard cap proposed in this PR; if real ops surfaces the need, file a follow-up bd issue with a configurable cap.
6. **`PatBackend::from_url` test fixture replacement**: §4.4 proposes `NativeBackend::test_fixture()`. Adversarial review should verify this is achievable (the existing `NativeBackend::new(...)` signature may require non-trivial args that the test fixture has to fake).
7. **Wizard test-send native replacement**: the wizard currently spawns an ephemeral Pat for "send a test message" UX. The native replacement may need a different test-send flow (e.g., write a message to the outbox then prompt the operator to actually send via the normal session). Spec it explicitly in the plan.
8. **CI on intermediate commits**: P1–P3 leave Pat code present; P4–P6 delete it. The plan must verify CI green at each commit, not just at PR tip. Subagents shipping individual phases should run `cargo build && cargo test --workspace` before declaring done.
9. **`LogSource::Pat` rename ripple**: search for every consumer of the enum variant; UI display labels, log filters, persistence (if the variant name is serialized anywhere). Mitigation: add a serde rename if the variant value was serialized as `"Pat"` somewhere, or update consumers.
10. **External submodule deinit edge cases**: if `external/tuxlink-pat/` has uncommitted local content (an operator's WIP patch), `git submodule deinit` warns but does not lose data. Inventory the submodule directory before deinit per the spirit of [ADR 0009](../../adr/0009-worktree-disposal-ritual.md) (which is about worktrees, but the same "enumerate state before deleting" discipline applies).
11. **release-please configuration**: if release-please's `release-please-config.json` references the `pat` package or scope, that entry needs removal. Grep `release-please*.json` for `pat` references before merge.

## 10. References

- [bd tuxlink-9phd](https://github.com/cameronzucker/tuxlink/issues?q=tuxlink-9phd) — this work item
- [`dev/handoffs/2026-05-30-heron-tanager-bog-html-forms-paused-pat-strip-prereq.md`](../../../dev/handoffs/2026-05-30-heron-tanager-bog-html-forms-paused-pat-strip-prereq.md) — operator pivot context
- [HTML Forms spec rev-2](2026-05-30-html-forms-design.md) — the work this PR unblocks
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/fbb/` — canonical B2F implementation (read-only reference; no Go code ships in tuxlink)
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/fbb/message.go` — `Message.Write()` (the function the Rust serializer mirrors)
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/fbb/header.go:99-133` — header serialization with `Mid:`-first + alphabetical sort
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/tests/LPE5NXDVLVSQ.b2f` — golden vector for §8.3
- [ADR 0003](../../adr/0003-no-sqlite-pat-owns-mailbox.md) — superseded by this work
- [ADR 0011](../../adr/0011-fork-pat-for-tuxlink.md) — superseded by this work
- [`docs/live-cms-testing-policy.md`](../../live-cms-testing-policy.md) — CMS telnet authorization scope (governs §7.4 smoke)
- [`docs/pitfalls/implementation-pitfalls.md`](../../pitfalls/implementation-pitfalls.md) — RADIO-1 entry (clarifies what this work does NOT touch)

---

End of design spec rev-1. Ready for 5-round cross-provider adversarial review.
