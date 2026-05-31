# Strip Pat + native B2F outbound with attachment support — design spec (rev-2, post-adversarial Claude R1–R4)

> Date: 2026-05-30 · Agent: magpie-grouse-shoal · bd: tuxlink-9phd
> Rev: 2 — incorporates the 4-round Claude adversarial review (R1 wire-format, R2 surgical safety, R3 API design, R4 failure modes). R5 (Codex cross-provider) pending against this rev.
> Parent context: PR #151 (HTML Forms v0.1) is paused mid-Phase-0 behind this prerequisite. Operator directive 2026-05-30: pivot to Path B + complete the Pat strip. See [`dev/handoffs/2026-05-30-heron-tanager-bog-html-forms-paused-pat-strip-prereq.md`](../../../dev/handoffs/2026-05-30-heron-tanager-bog-html-forms-paused-pat-strip-prereq.md).

## 0. Status — DESIGN ONLY (HARD-GATE per `superpowers:brainstorming` + `build-robust-features`)

Approval flow:

- **Approve / merge** → Codex R5 cross-provider review → revise to rev-3 if needed → `superpowers:writing-plans` → plan review cycle (min 3 rounds) → execution recommendation → operator decision on approach.
- **Comment** → revise spec + re-run any affected adrev round.
- **Reject** → regroup.

## 1. Change log from rev-1

Adversarial review surfaced **14 P0s + 25 P1/P2s** across the 4 rounds. Many were factual errors about what's actually in the codebase (signatures, variant names, existing tests) that the rev-1 author wrote from memory rather than verifying. Rev-2 corrects every finding via direct file inspection. Adrev transcripts (gitignored, local-only) at `dev/adversarial/2026-05-30-pat-strip-native-attachments-claude-r{1,2,3,4}-*.md`.

Key rev-1 → rev-2 corrections:

| Class | Source | Rev-1 said | Rev-2 says |
|---|---|---|---|
| **Wire — CRLF terminators** | R1 P0-1 | "no separators between attachments, no trailing CRLF" | A single `\r\n` between body and first attachment when `files.len() > 0`; one `\r\n` after every attachment. Verified `wl2k-go fbb/message.go:466-478`. Without these, `wl2k-go::readSection` errors with `"Unexpected end of section"`. |
| **Wire — Q-encoding charset** | R1 P0-2 | `=?utf-8?Q?...?=` | `=?ISO-8859-1?q?...?=` (lowercase `q`). Verified `wl2k-go fbb/message.go:436-437` (`DefaultCharset = "ISO-8859-1"`); test `fbb/message_test.go:25-37`. Latin-1-unencodable filenames need a separate decision (see §2.2). |
| **Wire — header sort** | R1 P1 | "alphabetical (existing behaviour)" | Case-sensitive byte-order alphabetical on **canonicalized** header keys (`wl2k-go fbb/header.go:99-133` uses `textproto.CanonicalMIMEHeaderKey`). The Rust port must canonicalize keys before sort or it breaks for lowercase-key inputs. |
| **Wire — header/body separator** | R1 P1 | "single `\r\n`" | The serializer writes header lines (each `\r\n`-terminated) + ONE additional `\r\n` = canonical `\r\n\r\n`. The existing parser at `winlink/message.rs:141` already looks for `\r\n\r\n`; spec rev-1 wording invited a regression. |
| **Wire — `compressed_size` framing** | R1 P1 | implicit | `<compressed_size>` in proposal includes the 6-byte lzhuf B2 framing prefix (2-byte CRC16 LE + 4-byte uncompressed-length LE per `lzhuf/writer.go:99-113`); minimum valid value is 6. Existing `winlink/lzhuf.rs::compress` already emits this framing; confirm by inspecting tests. |
| **Parser — `from_bytes` not extended** | R4 P0-1 | (silent — never mentioned `from_bytes`) | The send pipeline is `send_message → mailbox.store → from_bytes → to_proposal → lzhuf`. Mailbox stores raw bytes correctly but `from_bytes` strips attachments because the parser ignores `File:` headers and trailing bytes. **The PR MUST extend `from_bytes` too** or attachments are silently dropped before they ever reach the wire. New §4.6 covers the parser extension. |
| **API — `Mailbox::store` signature** | R3 P0-1 | `mailbox.store(folder, &id, &bytes)` (3 args, returns Result<()>) | Actual: `mailbox.store(folder, raw) -> Result<MessageId, BackendError>` — parses MID from headers itself, returns the id. On-main `NativeBackend::send_message` already has the correct shape; spec rev-1 *regressed* it. Rev-2 §4.3 mirrors the on-main pattern. |
| **API — `MessageId::from(id)`** | R3 P0-2 | `MessageId::from(id)` | No `From<String>` impl exists on `MessageId`. Use `MessageId::new(s)` (the canonical constructor) or `MessageId(s)` (tuple-struct field access). Rev-2 §4.3 uses `MessageId::new`. |
| **API — `BackendError::Storage`** | R3 P0-2 | `BackendError::Storage(...)` variant | Variant doesn't exist. Existing variants: `NotConfigured`, `AuthFailed`, `TransportFailed`, `BackendUnavailable`, `MessageRejected`, `NotImplemented`, `NotFound`, `Io`. `Mailbox::store` already maps internally to `MessageRejected` or `Io`; `?` propagation works without the false `Storage` variant. |
| **API — `Option<MessageId>` return leaks Pat 1.0.0 semantics** | R3 P0-3 | "PatBackend returns `None`; NativeBackend will return `Some`" — left in the trait | Trait now returns `Result<MessageId, BackendError>` (no `Option`). Pat is gone in this PR; the `None` branch is dead-code masquerading as a valid contract. ui_commands.rs sites (lines ~613-664) + tests (lines ~1867-1880) updated in the same commit. New §4.7 covers the trait change. |
| **Naming — `LogSource::Pat → ::Backend` rename collides** | R3 P1-1 + R2 P0 | "rename `LogSource::Pat` to `LogSource::Backend`" | `LogSource::Backend` ALREADY exists in `winlink_backend.rs:290`. The proposed change is a **merge**, not a rename — `Pat` lines logically become `Backend` lines and the variant disappears. Also forces same-commit TS frontend update (`logProjection.ts:30`, `logProjection.test.ts:330`). Wire form (`#[serde(rename_all = "lowercase")]`) emits `"pat"` to the frontend today; rev-2 plan handles the migration. |
| **Naming — `File` struct collision** | R3 P1-2 | `winlink::compose::File { name, data }` | Collides with `std::fs::File`. Rev-2 deletes `File` entirely and uses the existing `OutboundAttachment { filename, content_type, bytes }` for the wire-side type as well. One type, one concept, no translation layer between trait input and compose input. The `content_type` field stays unused for now but with a tighter rationale (see §4.2). |
| **Compose API — `compose_message` panic surface** | R1 P1 + R3 P1-3 | infallible wrapper calling `.expect()` on `compose_message_with_files` | Rev-2 keeps both fns infallible — validation (filename length, callsign format) is enforced via typed wrappers on input, not via runtime errors in compose. Removes the panic site entirely. `ComposeError` deleted; the only validation that could fail (filename > 255 chars) becomes a constructor-level check on a `Filename` newtype. |
| **`ComposeError` non_exhaustive** | R3 P1-4 | not specified | Moot since `ComposeError` deleted. |
| **`OutboundAttachment.content_type` dead field** | R3 P1-5 | "kept for future inbound parsing and UI hints" | Per `project_fork_enables_aggressive_deletion` + the principle of "delete now, restore on need," **the field is deleted** in this PR. If inbound parsing needs it later, restore at that point. |
| **Missed deletion target: `tests/winlink_backend_test.rs`** | R2 P0 | not in §8.5 deletion list | 22 `PatBackend` hits in 615 LOC. **Partial edit**, not delete: remove Pat-specific test cases + Pat-using setup helpers; preserve unrelated coverage. New §5 P9 + §8.5 cover. |
| **Missed deletion target: `tests/ui_commands_test.rs`** | R2 P0 | not in §8.5 deletion list | 10 `PatBackend` hits in 558 LOC. **Partial edit**, same approach. |
| **Missed deletion target: `Cargo.toml [[bin]] live_cms_smoke`** | R2 P0 | not mentioned | Lines 64-65 declare the bin; deleting only the .rs file leaves a dangling declaration that breaks `cargo build`. Cargo.toml entry deleted in the same commit as the .rs file. |
| **Missed deletion target: `build_support.rs`** | R2 P0 | not mentioned | After P10 deletes the Go-build path in `build.rs`, `build_support.rs` (which exists to test that path) becomes dead orphan. Deleted in P10. |
| **Missed deletion target: `.github/workflows/release.yml`** | R2 P1 | not mentioned | Workflow uses `go-version-file: 'external/tuxlink-pat/go.mod'` — breaks on first run post-P11. Workflow edited in P10 (CI surgery is part of the Pat-build deletion phase). |
| **Operator-state migration: `Config.pat_mbo_address`** | R2 P0 | "field deleted" | `Config` uses `#[serde(deny_unknown_fields)]`; deleting the field breaks every operator's existing `config.json` on first read. Rev-2 P8 deprecates the field via `#[serde(default, skip_serializing)]` + a deprecation-log on read; full removal deferred to a future major bump. |
| **Operator-state migration: keyring service-name "tuxlink-pat"** | R4 P0-2 + R2 P0 | not addressed | 6 source sites + 1 frontend UI string (`Step2Credentials.tsx:84`). The native backend uses `"tuxlink-pat"` as its keyring service name (not just Pat). **Decision: rename to `"tuxlink"` + ship one-time migration** that reads from `"tuxlink-pat"` and writes to `"tuxlink"` on first successful auth, then deletes the old entry. Documented in §7.1. |
| **Observability gap: `BackendError::TransportFailed` is too generic** | R4 P0-3 | not addressed | New §4.8 covers session-log capture of `FC EM <mid>` + `FS <answers>` for every send; on failure, include the FS-reject MID set in the error. Operators can distinguish wire-garbage from connection-drop. |
| **HTML Forms (PR #151) spec rev-2 commits to Path A in writing** | R2 P1 | "HTML Forms resumes against PR #151 after this lands" | The HTML Forms spec on `main` (`docs/superpowers/specs/2026-05-30-html-forms-design.md`) commits in writing to Path A (Pat REST). Rev-2 P12 includes a `rev-3` revision of that spec in the same PR — without it, the unblocking is fictional and the next-session author starts on a Pat-dependent spec. |
| **Wizard test-send "deferred to plan"** | R4 P1 + R2 P1 | "spec it explicitly in the plan" | Rev-2 §7.2 commits to a concrete behavior: the wizard's test-send button (a) verifies CMS reach via `native_cms_probe`-equivalent connect-only check; (b) does NOT send a test message. This eliminates the Pat-spawn affordance and avoids RADIO-1 entanglement for a non-load-bearing UX. |
| **`live_cms_smoke.rs` vs `native_send_probe`** | R4 P1 | ambiguous | Rev-2 §7.3: `live_cms_smoke.rs` deleted (Pat-based, legacy). `native_cms_probe.rs` extended in-place to optionally send a small attachment payload behind a `--send <path>` flag (defaulting to connect-only when absent). No new bin file. |
| **No rollback procedure** | R4 P1 | not addressed | New §7.4 documents `git revert -m 1 <merge-sha>` as the rollback path; PR description includes a clear "if regression, revert the merge" note. The 12-commit decomposition makes a clean revert possible. |
| **Submodule deinit data preservation** | R4 P1 | §9 #10 "soft inventory" | Rev-2 §7.5 documents an ADR-0009-shaped 5-step procedure that explicitly accounts for `.git/modules/external/tuxlink-pat/` orphan removal. |
| **Spec error — fixture path** | R1 + R4 + R2 | "`@v1.0.1/tests/LPE5NXDVLVSQ.b2f`" | Actual: `@v1.0.1/lzhuf/testdata/LPE5NXDVLVSQ.b2f` (31,380 bytes). Used by `lzhuf_test.go`, not `message_test.go`. |
| **Spec error — wl2k-go license** | R4 + R2 | "GPL-2.0" | Actual: **MIT** (`@v1.0.1/LICENSE`). Tuxlink is also MIT. Vendoring is straightforward — no fallback needed. |
| **ADR amendment text shape** | R3 P1-6 | separate `**Status (...)**:` paragraph | Existing convention (per ADR 0003 line 4): single `Status:` line near the top. Rev-2 §6.1/§6.2 use that shape. |
| **ADR 0016 outline rigor** | R3 P1-7 | minimal | Rev-2 §6.3 expanded with Watched Failure Modes (4 entries), Alternatives Considered (3 entries), and an explicit Status/Deciders/Context/Decision/Consequences structure matching ADR 0014. |
| **CMS abnormal-response handling** | R4 P1 | not addressed | New §9 entries 6 + 7 cover FS-reject, FS-defer, mid-transfer drop. Mapped to `BackendError::MessageRejected` (FS-reject) and `BackendError::TransportFailed` (drop) with diagnostic context. |
| **Smoke happy-path only** | R4 P1 | single attachment | Rev-2 §7.3 expands smoke to: (a) plain text, (b) one ASCII attachment, (c) two attachments + multi-recipient, (d) non-ASCII filename, (e) empty (0-byte) attachment, (f) large (1 MB) attachment. All via the same probe binary with `--send` arg variations. |

Everything else from rev-1 (high-level approach, single PR, native-only outbound) is preserved.

## 2. Purpose

`NativeBackend::send_message` silently discards the `attachments` field on its `OutboundMessage` input. Until that ends, `PatBackend` — a 657-LOC Rust module + ~360 LOC in `winlink_backend.rs` + a Go sidecar bundled into release builds + a forked submodule at `external/tuxlink-pat` — has to stay live to handle the half of outbound users care about (everything beyond plain text). Once attachments work natively, every Pat surface deletes in the same cutover.

The operator decision on 2026-05-30 promoted "native B2F outbound with attachments" from a v0.5+ aspiration (per the HTML Forms spec §5.1 Path B) to a P1 prerequisite blocking forward outbound work. The spec covers the new outbound code AND the Pat-deletion surgery AND the operator-state migrations (keyring rename, config field deprecation); they happen in one PR because the build does not compile with `PatBackend` deleted and `NativeBackend::send_message` still silently dropping attachments.

**In scope**:

1. Native compose for messages with one or more attachments, in Winlink B2F wire format as verified against `wl2k-go` (the canonical reference per established project policy on Winlink protocol questions: prior-art implementations are ground truth).
2. Native parse for messages with attachments — extending `Message::from_bytes` so the disk→session reload preserves attachment data.
3. `NativeBackend::send_message` wires attachments through the existing session/transfer/lzhuf pipeline.
4. `WinlinkBackend` trait return-type tightening: `Result<Option<MessageId>, BackendError>` → `Result<MessageId, BackendError>` (Pat 1.0.0's no-MID-echo limitation no longer applies).
5. All `PatBackend` install sites (bootstrap, app_backend, wizard) flipped to `NativeBackend`.
6. Pat module tree + tests + Go-build infra + sidecar bundling + `external/tuxlink-pat` submodule **deleted**.
7. Operator-state migrations: `Config.pat_mbo_address` deprecated (read-tolerant, no-write); keyring service-name `"tuxlink-pat"` → `"tuxlink"` with auto-migration.
8. ADR 0003 + ADR 0011 marked **Superseded**; new ADR 0016 documents the cutover.
9. HTML Forms spec (rev-2 on main) revised to rev-3 in the same PR — removes Path A reasoning and points at the now-available native attachment path.
10. Operator-driven CMS-telnet smoke (authorized non-RF dev testing per project policy) verifying real round-trip of multiple attachment configurations against `cms-z.winlink.org`.

**Out of scope** (filed as separate bd issues if not already):

- Inbound attachment display in the UI — `Message::from_bytes` extension populates the structure correctly; the UI continues to render the message body as plain text until a follow-up adds attachment chips/links.
- HTML Forms feature implementation (`tuxlink-v1p`) — this prereq unblocks it; HTML Forms execution resumes on its own branch after this lands.
- The `tuxlink-pat` GitHub repo deletion — stays as historical record; project policy does not gate GitHub repo cleanup.
- RF on-air validation — telnet against `cms-z.winlink.org` is the v0.1 smoke; RF transports (AX.25, ARDOP) are unchanged by this work and exercise the same `transfer::frame_block` codec.
- Any v0.5+ modem work, transport changes, ARDOP-side flow changes — unaffected.

## 3. Wire format — Winlink B2F message with attachments

Load-bearing finding from reading `wl2k-go/fbb` (v1.0.1 at `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/`), the authoritative Winlink protocol reference. Two prior summaries (the bd issue scope and an audit subagent) characterised this format as "MIME multipart in the body"; that is **wrong**. The format is a Winlink-custom one. Rev-1's wire-format claims had two P0 errors (CRLF terminators and Q-encoding charset); rev-2 corrects them.

A complete outbound message with attachments, **before** lzhuf compression, is one byte blob with this exact shape:

```
Mid: <12-char base32>\r\n
Body: <text_body_byte_count>\r\n
Content-Transfer-Encoding: 8bit\r\n
Content-Type: text/plain; charset=ISO-8859-1\r\n
Date: YYYY/MM/DD HH:MM\r\n
File: <byte_count> <filename>\r\n           ← one per attachment, in declaration order
File: <byte_count> <filename>\r\n
From: <station_address>\r\n
Mbo: <station_address>\r\n
Subject: <subject>\r\n
To: <recipient_address>\r\n
Type: Private\r\n
\r\n                                         ← single CRLF that, with the last header's CRLF, forms the canonical \r\n\r\n header/body separator
<text_body_bytes>                            ← exactly Body: byte count
\r\n                                         ← present IFF the message has >= 1 attachment
<attachment_1_bytes>                         ← exactly File: byte count for attachment 1
\r\n                                         ← present after every attachment
<attachment_2_bytes>                         ← exactly File: byte count for attachment 2
\r\n
```

**Header ordering**: `Mid:` is emitted first; all other headers are emitted in canonicalized-key alphabetical order (per `wl2k-go fbb/header.go:99-133`, which uses `textproto.CanonicalMIMEHeaderKey` to normalize keys before sort). The Rust port must canonicalize keys before sorting or it produces non-canonical orderings for lowercase-key inputs. `File:` headers naturally land between `Date:` (`D...`) and `From:` (`Fr...`). Multi-`File:` entries within the same sort slot preserve insertion order (the `header_all()` API returns values in append order, not sorted).

**Multi-recipient headers**: `Cc:` and `To:` headers are repeated per address, NOT comma-joined. Empty `Cc` list emits zero `Cc:` headers (not one empty header).

**Trailing CRLFs**: per `wl2k-go fbb/message.go:466-478`, when the message has one or more files, the serializer writes `body + "\r\n" + (file_bytes + "\r\n")*` — one CRLF between body and first attachment, one CRLF after every attachment including the last. Without these, `wl2k-go::readSection` errors with `"Unexpected end of section"` and refuses the message. When the message has zero files, the body is written without a trailing CRLF — this preserves the zero-attachment degenerate case as byte-identical to plain-text compose.

The B2F protocol layer (proposal exchange + framed transfer) is **attachment-agnostic**. The proposal `FC EM <mid> <size> <compressed_size> 0` carries the uncompressed total byte count (headers + body + all attachment terminator-CRLFs + all attachment bytes), with `<compressed_size>` being the post-lzhuf byte count of the same blob. **`<compressed_size>` includes the 6-byte lzhuf framing prefix** (2-byte CRC16 LE + 4-byte uncompressed-length LE per `lzhuf/writer.go:99-113`). The `transfer::frame_block` codec streams the compressed bytes in 125-byte chunks within `SOH ... STX ... EOT` envelopes; it does not parse the inner format. The receiver lzhuf-decompresses, parses headers (including `File:` entries), reads `Body:` bytes as text, then sequentially reads each `File:`-headered attachment in declaration order, consuming the trailing CRLF after each.

### 3.1 Golden vector

`wl2k-go` ships a test fixture at `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/lzhuf/testdata/LPE5NXDVLVSQ.b2f` — 31,380 bytes uncompressed (used by `lzhuf_test.go`, not `fbb/message_test.go`). It contains a single Latin-1 text body + one binary JPG attachment named `1469042410710.jpg`. Headers (extract):

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
[CRLF]
[31028 bytes of binary JPG data]
[CRLF]
```

This fixture is the authoritative byte-level conformance target for the Rust serializer (see §8 test plan). License: wl2k-go is MIT (verified at `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/LICENSE`); tuxlink is also MIT. Vendoring is straightforward — no licensing concern, no fallback-to-gopath logic needed.

### 3.2 Filename encoding

Filenames containing non-ASCII characters are RFC 2047 Q-encoded by `wl2k-go` via `mime.QEncoding.Encode(DefaultCharset, name)` (`message.go:436-437`) where `DefaultCharset` is `"ISO-8859-1"`. Wire form is `=?ISO-8859-1?q?...?=` (lowercase `q`, NOT the uppercase `Q` rev-1 specified). Tested by `wl2k-go fbb/message_test.go:25-37`.

**Latin-1-unencodable filenames** (e.g., CJK, emoji): `mime.QEncoding.Encode` with charset="ISO-8859-1" is lossy for non-Latin-1 codepoints — wl2k-go does not address this case explicitly. Tuxlink's port rejects such filenames at compose time with a typed error rather than silently corrupting (this is conservative — a future operator who actually needs CJK filenames can switch to the framed-block-title path, which wl2k-go encodes with `mime.QEncoding.Encode("utf-8", ...)`, or extend with a transliteration step).

Filename length is capped at 255 characters by `wl2k-go fbb/message.go:146`. The Rust port emits the same cap and rejects oversize filenames at construction time via a `Filename` newtype (see §4.2) — this moves validation out of `compose_message_with_files` and into a constructor that already lives at the operator-input boundary.

### 3.3 What this format is NOT

To prevent regression to either prior misconception:

- **NOT MIME multipart**: no `multipart/mixed`, no `boundary=...`, no per-part headers, no base64 transfer encoding of binary attachments. Attachments are raw bytes appended directly.
- **NOT a single `body` blob with attachments as foreign elements**: the `Body:` header is the count of *text body* bytes only; attachments are NOT part of the text body count.
- **NOT separated by markers between attachments**: each attachment is followed by a `\r\n` terminator (per §3 above); the parser delimits using each `File:` header's byte count, then consumes the trailing CRLF.

## 4. API surface — new + changed

The change is **two files of substantive code edits** (compose.rs, message.rs) plus the trait return-type tightening + NativeBackend wire-through, plus the install-site flips, plus the deletion surgery. Per R3 + R4 the actual edits are tighter than rev-1's sketch implied.

### 4.1 `winlink::compose::compose_message_with_files`

New function. Keeps the existing `compose_message` (text-only) for callers that don't need attachments; both forward to a shared inner helper. Single-purpose functions compose better than optional-param creep.

```rust
/// Build a Private text message with zero or more file attachments.
///
/// Files are appended to the message in declaration order. Filename validation
/// happens at the [`Filename`] constructor, not here, so this function cannot
/// fail.
pub fn compose_message_with_files(
    mycall: &str,
    to: &[&str],
    cc: &[&str],
    subject: &str,
    body: &str,
    attachments: &[OutboundAttachment],
    unix_secs: u64,
) -> Message;
```

`compose_message` (text-only) keeps its existing signature unchanged. Implementation forwards to `compose_message_with_files` with `&[]` for attachments — no `Result`, no `.expect()`, no panic site.

### 4.2 Re-using `OutboundAttachment`

Rev-1 introduced a parallel `winlink::compose::File` struct. R3 P1-2 flagged the collision with `std::fs::File` and the duplication with the existing `OutboundAttachment` (already on `OutboundMessage.attachments` via PR #151 T0.1). Rev-2 deletes the proposed `File` type and uses `OutboundAttachment` throughout:

```rust
// Already on main (winlink_backend.rs:91):
#[derive(Debug, Clone)]
pub struct OutboundAttachment {
    pub filename: String,           // pre-§4.5: a plain String; post-§4.5: a Filename newtype
    pub bytes: Vec<u8>,
}
```

The rev-1 `content_type: String` field is **deleted** in this PR. It was kept "for future inbound parsing and UI hints" but per the project's aggressive-deletion stance, it's removed now and restored if a real need surfaces. The PR's change to this struct is the field removal; existing callers (UI compose flow + heron-tanager-bog's T0.1 test) get a same-commit update.

A new `Filename` newtype enforces the 255-char cap + Latin-1 encodability check:

```rust
#[derive(Debug, Clone)]
pub struct Filename(String);

impl Filename {
    pub fn new(s: impl Into<String>) -> Result<Self, FilenameError> {
        let s = s.into();
        if s.chars().count() > 255 { return Err(FilenameError::TooLong(s.chars().count())); }
        if !s.chars().all(|c| (c as u32) <= 0xff) { return Err(FilenameError::NotLatin1Encodable); }
        Ok(Filename(s))
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FilenameError {
    #[error("filename exceeds 255-character limit: {0} chars")]
    TooLong(usize),
    #[error("filename contains characters outside ISO-8859-1; Q-encoding would be lossy")]
    NotLatin1Encodable,
}
```

The compose function takes `&[OutboundAttachment]` and uses `filename: String` directly — the `Filename` newtype is the boundary-validation shape; the wire-side carries the validated `String`. (If a more aggressive future cleanup wants to push `Filename` all the way through, the type already exists.)

`OutboundAttachment::filename`'s validation happens at construction. UI sites that build `OutboundAttachment` will need to go through `Filename::new(...)?` — a few-line change tracked in §5 P4.

### 4.3 `winlink_backend::NativeBackend::send_message`

The on-main implementation (`winlink_backend.rs:578-596` on the worktree branch HEAD) is already structurally close to correct — it just calls `compose::compose_message` without the attachment param. The change is a one-line swap + the `Mailbox::store` return-type rename:

```rust
async fn send_message(
    &self,
    msg: OutboundMessage,
) -> Result<MessageId, BackendError> {                      // CHANGED: no Option
    let callsign = self
        .live_config()
        .identity
        .callsign
        .ok_or_else(|| BackendError::NotConfigured("identity.callsign".into()))?;
    let unix_secs = parse_rfc3339_secs(&msg.date).unwrap_or_else(now_unix_secs);
    let to: Vec<&str> = msg.to.iter().map(String::as_str).collect();
    let cc: Vec<&str> = msg.cc.iter().map(String::as_str).collect();
    let message = compose::compose_message_with_files(            // CHANGED: with_files
        &callsign, &to, &cc, &msg.subject, &msg.body,
        &msg.attachments,                                          // NEW: pass through
        unix_secs,
    );
    let id = self.mailbox.store(MailboxFolder::Outbox, &message.to_bytes())?;
    Ok(id)                                                         // CHANGED: no Some(...)
}
```

`Mailbox::store(folder, raw) -> Result<MessageId, BackendError>` parses the MID from headers itself; the caller just propagates the returned id. Trait return is `Result<MessageId, BackendError>` (no `Option`); see §4.7.

### 4.4 `app_backend::BackendState` test fixtures

Per audit, `app_backend.rs` lines 161, 173, 219 use `PatBackend::from_url("http://127.0.0.1:9")` to construct a stub backend for unit tests. After Pat deletion, these need a `NativeBackend` equivalent. The chosen approach is a `#[cfg(test)]` factory on `NativeBackend`:

```rust
#[cfg(test)]
impl NativeBackend {
    /// In-process stub for unit tests that exercise [`BackendState::install`]
    /// lifecycle without touching real telnet or a real mailbox. Uses a temp dir
    /// for the mailbox root. The `connect` and `send_message` methods on this
    /// fixture will fail with `BackendError::NotConfigured` since no callsign
    /// is set.
    pub fn test_fixture() -> Self {
        let tempdir = tempfile::tempdir().unwrap();
        let leaked_path = Box::leak(Box::new(tempdir)).path().to_path_buf();
        Self::new(Config::default(), leaked_path)
    }
}
```

The `Box::leak` keeps the tempdir alive for the test's lifetime without infecting the public API. Cargo.toml needs `tempfile` in `dev-dependencies` if not already present (verify in P1).

### 4.5 Removed APIs

- `pat_client::PatClient` (entire struct + impl)
- `pat_config::{render_pat_config, write_pat_config_atomic, PatConfigError}`
- `pat_process::{PatProcess, PatSpawnOptions}`
- `winlink_backend::{PatBackend, PatBackendSpawnOptions}` + the `PatBackend`-only `WinlinkBackend` impl block
- `bootstrap::{resolve_pat_binary, resolve_pat_binary_inner, is_nonempty_file}`
- `SIDECAR_STUB_REASON` const in bootstrap.rs
- `LogSource::Pat` variant (merged into existing `LogSource::Backend`; emit-site at `winlink_backend.rs:1408+1423` retargets to `::Backend`)
- `OutboundAttachment.content_type` field
- `Cargo.toml` `[[bin]] live_cms_smoke` entry
- `build_support.rs` (Go-toolchain check helper; orphaned after `build.rs` Go-path deletion)

### 4.6 `Message::from_bytes` extension — parser side (R4 P0-1 fix)

The current parser at `winlink/message.rs:139-167` reads headers + body and stops. With attachments, the on-disk bytes look like `headers + \r\n\r\n + body + \r\n + (attachment + \r\n)*`. The mailbox stores raw bytes correctly (it's a passthrough write); the parser needs to consume the trailing attachment region.

Extended parser shape (the function gains a step after the existing `body` assignment):

```rust
pub fn from_bytes(input: &[u8]) -> Result<Message, ParseError> {
    // ... existing header + body parsing unchanged ...
    msg.body = after_headers[..body_size].to_vec();

    // NEW: parse attachments
    let mut offset = sep + 4 + body_size;
    let file_headers: Vec<(usize, String)> = msg
        .header_all("File")
        .into_iter()
        .map(|v| parse_file_header(v))
        .collect::<Result<Vec<_>, _>>()?;

    if !file_headers.is_empty() {
        // Consume the body→first-attachment terminator CRLF
        if input.get(offset..offset+2) != Some(b"\r\n") {
            return Err(ParseError::MissingAttachmentTerminator);
        }
        offset += 2;

        for (size, name) in file_headers {
            if input.len() < offset + size {
                return Err(ParseError::TruncatedAttachment);
            }
            let data = input[offset..offset+size].to_vec();
            offset += size;
            // Consume the trailing CRLF after this attachment
            if input.get(offset..offset+2) != Some(b"\r\n") {
                return Err(ParseError::MissingAttachmentTerminator);
            }
            offset += 2;
            msg.attachments.push(OutboundAttachment { filename: name, bytes: data });
        }
    }

    Ok(msg)
}

fn parse_file_header(value: &str) -> Result<(usize, String), ParseError> {
    let (size_str, name) = value.split_once(' ')
        .ok_or(ParseError::MalformedFileHeader)?;
    let size = size_str.parse::<usize>()
        .map_err(|_| ParseError::MalformedFileHeader)?;
    Ok((size, name.to_string()))
}
```

New `ParseError` variants: `MalformedFileHeader`, `MissingAttachmentTerminator`, `TruncatedAttachment`. The existing `ParseError` enum gains `#[non_exhaustive]` to allow future additions without a breaking change.

The `Message` struct gains an `attachments: Vec<OutboundAttachment>` field (same type as the trait-input side; symmetric). `to_bytes` already accounts for files (per §3); `from_bytes` now does too.

### 4.7 `WinlinkBackend` trait return-type tightening

Per R3 P0-3, the `Result<Option<MessageId>, BackendError>` on `send_message` is a vestige of Pat 1.0.0's no-MID-echo limitation. Pat is gone in this PR; the `None` branch is dead code masquerading as a valid contract. Trait change:

```rust
// Before (winlink_backend.rs:393-396):
async fn send_message(&self, msg: OutboundMessage)
    -> Result<Option<MessageId>, BackendError>;

// After:
async fn send_message(&self, msg: OutboundMessage)
    -> Result<MessageId, BackendError>;
```

Same-commit caller updates:
- `ui_commands.rs:613-664` — the `Ok(None)` branches become unreachable; remove them and the surrounding comment blocks that explain Pat's quirk.
- `ui_commands.rs` test file (line ~1867-1880 per R3 finding) — update mock backends that return `Ok(None)`; they now return `Ok(MessageId::new("test-mid"))`.
- Any other call site identified by grep.

### 4.8 Send-side observability

Per R4 P0-3, `BackendError::TransportFailed` swallows the actual failure mode (wire-garbage vs reject vs drop). The native session loop already has a `LogSink` for `LogSource::Wire` lines (`winlink_backend.rs:432-460`); extend `NativeBackend::send_message` to emit:

- On proposal send: `LogSource::Wire`, message body = the `FC EM <mid> <size> <compressed_size> 0` line as sent.
- On FS-answer receive: `LogSource::Wire`, message body = `FS <answers>` raw.
- On `FS -` (reject) for our MID: `BackendError::MessageRejected { mid, reason: "FS reject from CMS" }`.
- On `FS =` (defer): retry with backoff (existing pattern); log defers.
- On mid-transfer connection drop: `BackendError::TransportFailed { reason: "connection dropped mid-transfer at offset N", source: Some(io_err) }`.

These changes apply to the wire-level helpers in `winlink::session` + `winlink::proposal` modules — adversarial review surfaces the right place; the plan task descriptions identify exact line edits.

## 5. Pat removal phases (commit-level decomposition)

CI must be green at the end of every commit. The atomic invariant: `PatBackend` is referenced in code IFF the file declaring it exists. The phase count grew from rev-1's 7 to 12 because R2 surfaced missed deletion targets (test files, Cargo.toml bin entry, build_support.rs) and R4 surfaced new migration commits (keyring, config). Phase order is chosen so each prior phase enables the next without leaving uncompilable states.

| Phase | Commits | What |
|---|---|---|
| **P1: Compose-with-files (forward path)** | 1 commit | Add `Filename` newtype + `FilenameError`. Delete `OutboundAttachment.content_type` field; update T0.1's test + the heron-tanager-bog `ui_commands.rs:660` literal. Extend `compose::compose_message` → forward to new `compose_message_with_files`. Extend `Message` struct with `attachments: Vec<OutboundAttachment>` + `set_attachments` setter. Extend `Message::to_bytes()` to write `File:` headers + body+CRLF+(attachment+CRLF)* tail. Extend `Message::to_proposal()` (size computation includes attachments). Add 9 unit tests including the wl2k-go golden vector at `lzhuf/testdata/LPE5NXDVLVSQ.b2f`. PatBackend untouched. CI green. |
| **P2: Parse-with-files (return path)** | 1 commit | Extend `Message::from_bytes` per §4.6: parse `File:` headers, consume body→attachment CRLF, read each attachment per its `File:` size, consume trailing CRLF after each. Add `ParseError` variants `MalformedFileHeader`, `MissingAttachmentTerminator`, `TruncatedAttachment`. Mark `ParseError` `#[non_exhaustive]`. Add 6 unit tests including a `from_bytes(to_bytes(msg)) == msg` round-trip for the golden vector. PatBackend untouched. CI green. |
| **P3: Trait return-type tighten** | 1 commit | `WinlinkBackend::send_message` signature changes from `-> Result<Option<MessageId>, BackendError>` to `-> Result<MessageId, BackendError>`. Update PatBackend impl (`winlink_backend.rs:1702`) — wraps Pat's `None` return into a synthesized `MessageId::new("")` for the transitional period (this PR's transitional state is "Pat exists but is being deprecated"; the synthesized empty MID is acceptable because no caller branches on emptiness AND PatBackend is fully removed two phases later). Update NativeBackend impl. Update `ui_commands.rs` call sites (~3 sites + comments). Update test mocks. CI green. |
| **P4: NativeBackend wires attachments** | 1 commit | `NativeBackend::send_message` calls `compose_message_with_files(...)` with `msg.attachments`. Update `two_native_backends_exchange_*` integration tests to pass an attachment through and assert round-trip via `from_bytes` (the new parse path) on the receiver. Add send-side observability (`LogSource::Wire` emits on proposal-send + FS-answer per §4.8). PatBackend untouched. CI green. |
| **P5: Flip install sites** | 1 commit | `bootstrap.rs`: delete `resolve_pat_binary` + `resolve_pat_binary_inner` + `is_nonempty_file` + `SIDECAR_STUB_REASON` + the `PatBackend::spawn` call site + the dedicated spawn thread; install `NativeBackend::new(...)` synchronously instead. `app_backend.rs` tests (3 sites): `PatBackend::from_url(...)` → `NativeBackend::test_fixture()`. `wizard.rs`: ephemeral Pat spawn for test-send → `NativeBackend` connect-only check (no test message sent — see §7.2). After this commit, no production code references PatBackend; the Pat-using tests in winlink_backend_test.rs and ui_commands_test.rs still exist but are not affected because they construct PatBackend directly. CI green. |
| **P6: LogSource::Pat merge** | 1 commit | Remove `LogSource::Pat` variant from the enum at `winlink_backend.rs:290`. Update the Pat-stderr-line emission site (`winlink_backend.rs:1408-1423`) to emit `LogSource::Backend` instead. Frontend update in same commit: `src/wizard/logProjection.ts:30` + `src/wizard/logProjection.test.ts:330` drop the `'pat'` discriminator branch (it becomes `'backend'`). The wire form (`#[serde(rename_all = "lowercase")]`) emits `"backend"` consistently from this commit forward. Historical logs (already on disk) with `"pat"` source are still readable by the parser as a known-but-deprecated value — handle via a serde-time alias if needed (verify by reading the actual existing log files; if no on-disk persistence of LogSource, skip). CI green. |
| **P7: Keyring service-name migration** | 1 commit | Rename `"tuxlink-pat"` → `"tuxlink"` at all 6 source sites (`winlink_backend.rs:994`, `winlink_backend.rs:1219`, `wizard.rs:186`, `bin/live_cms_smoke.rs:45+104` (file is deleted in P9 — these sites are removed there; only the 4 surviving sites in P7), `bin/native_cms_probe.rs:43`). One-time migration helper at `winlink::credentials::migrate_keyring_entry()`: read from `"tuxlink-pat"`, write to `"tuxlink"`, delete old entry, log once. Called on first `connect()` attempt. Update `src/wizard/Step2Credentials.tsx:84` UI string. CI green. |
| **P8: Config field deprecation** | 1 commit | `Config.pat_mbo_address`: change from `pub pat_mbo_address: Option<String>` to `#[serde(default, skip_serializing)] pat_mbo_address: Option<String>` (private field, default to None on read, never written back). Keep `#[serde(deny_unknown_fields)]` on the struct (so future drift still gates correctly). On deserialize, if `pat_mbo_address.is_some()`, log a deprecation warning. Full field removal deferred to a future major bump (tracked as a follow-up bd issue). CI green. |
| **P9: Delete Pat module + Pat tests** | 1 commit | `rm src-tauri/src/pat_client.rs src-tauri/src/pat_config.rs src-tauri/src/pat_process.rs`. Delete `PatBackend` + `PatBackendSpawnOptions` + the `PatBackend` `WinlinkBackend` impl block from `winlink_backend.rs`. `rm src-tauri/tests/pat_client_test.rs src-tauri/tests/pat_config_test.rs src-tauri/tests/pat_process_test.rs`. **Partial edit** of `tests/winlink_backend_test.rs` (22 PatBackend hits, 615 LOC): remove every test case that uses PatBackend (estimated ~8-12 tests); preserve non-Pat coverage. **Partial edit** of `tests/ui_commands_test.rs` (10 PatBackend hits, 558 LOC): same approach (estimated ~3-5 tests). Remove `pub mod pat_client; pub mod pat_config; pub mod pat_process;` from `lib.rs`. `rm src-tauri/src/bin/live_cms_smoke.rs`. Remove `[[bin]] live_cms_smoke` from `Cargo.toml` (lines 64-65). After this commit, `cargo build` + `cargo test` are green; no Pat code remains. |
| **P10: Delete sidecar infra** | 1 commit | `tauri.conf.json`: remove `"externalBin": ["sidecars/pat"]`. `build.rs`: delete Go-toolchain check, `go build` invocation, 0-byte-stub creation. **The file shrinks dramatically** — flag in commit body. `rm src-tauri/build_support.rs` (the helper that tests the Go path). Delete `#[cfg(test)] mod build_support;` declaration. Delete `src-tauri/sidecars/` directory entirely. `.github/workflows/release.yml`: remove the Go-toolchain setup step + the Pat sidecar build step (search for `go-version-file: 'external/tuxlink-pat/go.mod'`). Remove any Cargo dependency that was Pat-specific (verify by inspecting `Cargo.toml` `[dependencies]` and checking whether each non-shared entry is still referenced post-deletion). CI green. |
| **P11: Submodule deinit** | 1 commit | Per the ADR-0009-shaped procedure in §7.5: `git submodule deinit -f external/tuxlink-pat`; `git rm external/tuxlink-pat`; delete the `[submodule "external/tuxlink-pat"]` block from `.gitmodules`; document `.git/modules/external/tuxlink-pat/` orphan-removal step in PR body (operator runs locally; not part of the commit). The forked repo at `github.com/cameronzucker/tuxlink-pat` survives. CI green. |
| **P12: Docs + ADR sweep** | 1 commit | (a) Edit ADR 0003 + ADR 0011 — append `(superseded by [ADR 0016](0016-native-b2f-outbound-with-attachments.md), 2026-05-30)` to the existing `Status:` line. (b) Write new ADR 0016 per §6.3 outline. (c) Revise HTML Forms spec (`docs/superpowers/specs/2026-05-30-html-forms-design.md`) to rev-3: remove §5.1's Path A reasoning, point at native attachments now available, update the spec's change-log row referencing the now-resolved §3 Path A vs Path B question. (d) Update `docs/install.md` + `docs/development.md` (remove Pat-references). (e) Update `VERSIONING.md` (remove "bundled-Pat compatibility break" MAJOR-bump trigger row). (f) Update README.md if it mentions Pat in the architecture overview. CI green (docs-only). |

Each phase is a separate commit on `bd-tuxlink-9phd/strip-pat-add-native-attachments`. Polish-before-push: rebase locally for cleanup, push the cleaned sequence, do NOT amend after push. Per the no-squash ADR 0010, the merge preserves all 12 commits.

**Why single PR for all 12 phases**: the alternative ("ship P1-P4 first, run for a few days, then ship P5+ later") introduces a transient state where `NativeBackend::send_message` handles attachments AND `PatBackend::send_message` still exists. Both are reachable through different install sites, and a config/wizard quirk could resurrect Pat after we thought we'd retired it. Atomic cutover is cleaner. The 12-phase decomposition keeps each commit small enough to review individually while preserving the all-or-nothing semantics at PR-merge time.

## 6. ADR mutations

### 6.1 ADR 0003 — Status amendment

The existing Status line at `docs/adr/0003-no-sqlite-pat-owns-mailbox.md:4` reads:

> Status: Accepted (amended by [ADR 0011](0011-fork-pat-for-tuxlink.md) — dependency target shifted from upstream `la5nta/pat` to the `tuxlink-pat` fork; the ownership-of-mailbox rule and the no-SQLite-in-tuxlink rule themselves remain operative)

Rev-2 appends:

> Status: Accepted (amended by [ADR 0011](0011-fork-pat-for-tuxlink.md) — dependency target shifted from upstream `la5nta/pat` to the `tuxlink-pat` fork; the ownership-of-mailbox rule and the no-SQLite-in-tuxlink rule themselves remain operative; **superseded by [ADR 0016](0016-native-b2f-outbound-with-attachments.md) as of 2026-05-30** — native client now owns mailbox; "no SQLite" half still holds.)

### 6.2 ADR 0011 — Status amendment

Same pattern. Append `superseded by [ADR 0016](0016-native-b2f-outbound-with-attachments.md) as of 2026-05-30` to the existing Status line. Reference: Pat is completely removed from tuxlink; the cred-handling refactor is no longer load-bearing because the native backend reads credentials directly from the OS keyring.

### 6.3 New ADR 0016

Title: "Native B2F outbound with attachments; Pat removed"

Required structure (matching ADR 0014 rigor):

- **Status**: Accepted.
- **Date**: 2026-05-30.
- **Deciders**: cameronzucker, magpie-grouse-shoal (via 5-round adversarial review including Codex R5).
- **Context**: How we got here — Pat as v0.0.1 expedient, native client built incrementally for read-side, half-cutover state by 2026-05-21, operator decision 2026-05-30 to finish the cutover before HTML Forms.
- **Decision**: Native B2F outbound with attachments per the wire format documented in §3 of this spec. Pat module + sidecar + submodule deleted in the same PR. Trait return tightened from `Option<MessageId>` to `MessageId` (Pat's no-MID-echo limitation no longer applies). Keyring service name renamed from `"tuxlink-pat"` to `"tuxlink"` with auto-migration.
- **Wire format reference**: Reproduce the §3 byte-layout reference inline (so future operators don't depend on the spec doc surviving).
- **Alternatives considered**:
  - (a) Path A — extend Pat via REST for attachments. **Rejected** because it perpetuates a Go runtime + sidecar + Pat-side bug surface for a feature we own.
  - (b) MIME multipart in the message body. **Rejected** because it does not match what wl2k-go and the WLE reference produce; the CMS expects B2F-format messages with `File:` headers + raw appended bytes.
  - (c) Phased rollout (ship native attachments first, delete Pat in a follow-up PR). **Rejected** because the transient state allows Pat to be silently re-installed via wizard quirks; atomic cutover is safer.
- **Watched failure modes**:
  - (1) Receiver-side parser misreads `Body:` size and corrupts attachment offsets. *Mitigation*: golden-vector test against wl2k-go's fixture; round-trip parse tests.
  - (2) Non-Latin-1 filenames fail Q-encode silently. *Mitigation*: `Filename::new` rejects at construction; UI shows a typed error.
  - (3) Future operator sees Pat traces in git history and tries to "re-add Pat for X" without reading this ADR. *Mitigation*: ADR explicitly forbids; lib.rs no longer declares pat modules.
  - (4) Operator with stale config + keyring entry experiences silent migration on first run. *Mitigation*: deprecation logs on `pat_mbo_address` read; one-line user-visible note on keyring migration.
- **Migration / cutover**: Reference the 12-phase commit decomposition in §5.
- **Consequences**: Positive (no Go toolchain dep; no sidecar bundling; smaller binary; simpler bootstrap; one source of truth for outbound). Negative (any future "fall back to Pat" affordance requires re-introducing the dep; the keyring rename has a one-time migration moment where an operator without network can't reach the keyring read+write).

ADR length target: 150-200 lines. Standard ADR template applies. Filename: `docs/adr/0016-native-b2f-outbound-with-attachments.md`.

## 7. Operator-state migrations + operator-facing concerns

### 7.1 Keyring service-name migration

The 6 source sites currently use `"tuxlink-pat"` as the keyring service name (with the operator's normalized callsign as the account name). The native backend reads from this keyring entry — it's not Pat-specific. Per R4 P0-2, leaving the name "tuxlink-pat" forever in a Pat-free codebase is wrong; renaming without migration silently orphans Cameron's keyring entry and produces an auth failure on upgrade.

**Decision**: rename to `"tuxlink"` + ship a one-time migration in this PR.

**Migration shape** (P7):

```rust
// In winlink::credentials (new module or existing keyring helper):
pub fn read_password(callsign: &str) -> Result<String, KeyringError> {
    let new_entry = keyring::Entry::new("tuxlink", callsign)?;
    match new_entry.get_password() {
        Ok(pw) => Ok(pw),
        Err(keyring::Error::NoEntry) => {
            // First-run-after-upgrade: look at the old name.
            let old_entry = keyring::Entry::new("tuxlink-pat", callsign)?;
            let pw = old_entry.get_password()?;  // propagates NoEntry if neither exists
            // Migrate.
            new_entry.set_password(&pw)?;
            let _ = old_entry.delete_password();  // best-effort; don't fail migration if old can't be deleted
            log::info!("migrated keyring entry: tuxlink-pat -> tuxlink for callsign {callsign}");
            Ok(pw)
        }
        Err(e) => Err(KeyringError::from(e)),
    }
}
```

Existing call sites (3 in src-tauri/src/, 1 in bin/native_cms_probe.rs, plus the 2 in live_cms_smoke.rs that delete with the file) switch to `read_password(callsign)` instead of `keyring::Entry::new("tuxlink-pat", callsign)`.

User-facing UI string at `src/wizard/Step2Credentials.tsx:84` updated: `secret-tool delete service tuxlink-pat` → `secret-tool delete service tuxlink`. The recovery instruction is a fallback for a user whose keyring is broken; if migration succeeded normally, they never see this.

### 7.2 Wizard test-send behavior

The wizard currently has a "send a test message" affordance that spawns an ephemeral Pat and sends a real test message to the operator's own callsign. Replacing this with a native equivalent is possible (use NativeBackend to send to self) but the affordance is a v0.0.1 UX nice-to-have, not a correctness requirement, and live-network test-sends entangle with RADIO-1 for RF transports.

**Decision** (rev-2): the wizard's test-send step is redesigned in P5 to be **connect-only**:
- The button label changes from "Send Test Message" to "Verify CMS Connection".
- The action calls `NativeBackend::connect(TransportConfig::Cms { mode: Plaintext })` against `cms-z.winlink.org` (or operator's configured CMS) and disconnects without sending. Same probe code path as `native_cms_probe`.
- On success, shows green checkmark + "CMS reachable as `<CALL>`."
- On failure, shows error + the existing diagnostic hints.

This eliminates the only place where the wizard would have entangled with RADIO-1 (no RF involved in CMS telnet anyway, but a "send a message" affordance for any transport could mis-fire). It also matches operator expectations from [[feedback_cms_telnet_testing_authorized]]: CMS telnet is authorized for dev verification; transmission is not.

### 7.3 Operator smoke (post-merge)

`src-tauri/src/bin/live_cms_smoke.rs` is deleted (Pat-based, legacy). `src-tauri/src/bin/native_cms_probe.rs` is extended in-place to take an optional `--send <PATH>` flag that, when present, sends a message with the file at `<PATH>` attached to `<SELF-CALL>@winlink.org`.

The operator smoke after merge runs the probe 6 times:

```bash
P=cargo run --manifest-path src-tauri/Cargo.toml --bin native_cms_probe --

# (a) Connect-only baseline.
$P
# (b) Plain text, no attachment.
$P --send-empty-text
# (c) One ASCII attachment.
echo "hello" > /tmp/a.txt && $P --send /tmp/a.txt
# (d) Two attachments + multi-recipient (manually run with cc).
echo "two" > /tmp/b.txt && $P --send /tmp/a.txt --send /tmp/b.txt --cc <SELF-CALL>
# (e) Non-ASCII filename.
echo "x" > /tmp/café.txt && $P --send /tmp/café.txt
# (f) Empty (0-byte) attachment.
: > /tmp/empty.txt && $P --send /tmp/empty.txt
# (g) Large attachment (1 MB).
head -c 1048576 /dev/urandom > /tmp/big.bin && $P --send /tmp/big.bin
```

Verify each on the receiving end (Winlink web inbox or another client) that the attachment(s) arrive intact. CMS telnet is authorized non-RF dev testing per [docs/live-cms-testing-policy.md](../../live-cms-testing-policy.md). RADIO-1 does NOT apply — this is internet telnet, not RF.

### 7.4 Rollback procedure

If the merged PR shows a regression in production after merge:

```bash
# Find the merge commit
git log --oneline --merges -1 main

# Revert the merge
git revert -m 1 <merge-sha>

# Push
git push origin main
```

Because the PR is atomic (no partial state), the revert is also atomic and clean. Any operator credentials migrated to `"tuxlink"` during the brief production window will need a one-time manual re-migration if they restart Pat afterwards — document this in the revert PR's body.

### 7.5 Submodule deinit ritual

Per the ADR-0009-shaped procedure (which is for worktrees but the "enumerate state before deleting" discipline applies):

```bash
# Step 1 — Inventory the submodule
cd external/tuxlink-pat
git status --short                           # tracked dirty
git stash list                               # any stashed WIP
cd ../..

# Step 2 — If anything at risk: propagate via fork-repo push, or archive
# (manual judgment; operator's call)

# Step 3 — Deinit
git submodule deinit -f external/tuxlink-pat

# Step 4 — Remove from tree + .gitmodules
git rm external/tuxlink-pat

# Step 5 — Clean .git/modules orphan (operator runs locally after merge)
rm -rf .git/modules/external/tuxlink-pat
```

Step 5 is not part of the commit; PR body documents it for each operator who has the submodule already initialized.

## 8. Test plan (TDD)

Every code change is test-first per `docs/pitfalls/testing-pitfalls.md` and the existing module convention.

### 8.1 Unit tests — `compose.rs` (P1)

| Test | Asserts |
|---|---|
| `composes_message_with_single_attachment` | Output has `File: <n> <name>` header; body section has text body + CRLF + raw attachment bytes + CRLF |
| `composes_message_with_multiple_attachments_preserves_order` | Two `File:` headers in declaration order; attachment bytes appended in declaration order, each terminated by CRLF |
| `composes_message_with_empty_attachment` | `File: 0 <name>` header; zero bytes appended for that file; CRLF terminator still present |
| `composes_message_with_no_attachments_matches_text_only_path` | Output of `compose_message_with_files(..., &[], t)` is byte-identical to `compose_message(..., t)` — NO trailing CRLF after body |
| `q_encodes_non_ascii_filenames_with_iso_8859_1` | Filename `café.txt` produces `File: <n> =?ISO-8859-1?q?caf=E9.txt?=` header (lowercase q, charset ISO-8859-1) |
| `filename_constructor_rejects_over_255_chars` | `Filename::new(256-char string)` returns `FilenameError::TooLong(256)` |
| `filename_constructor_rejects_non_latin1` | `Filename::new("日本語.txt")` returns `FilenameError::NotLatin1Encodable` |
| `composes_multi_recipient_with_attachments` | Two `To:` headers + one `Cc:` header + one attachment; all present in output |
| `header_sort_canonicalizes_keys` | Headers with mixed-case keys input → output is canonicalized + alphabetical with Mid first |

### 8.2 Unit tests — `message.rs` (P1 + P2)

P1:

| Test | Asserts |
|---|---|
| `to_bytes_header_order_mid_first_then_alphabetical_with_files` | `Mid:` first; remaining headers alphabetical; `File:` between `Date:` and `From:` |
| `to_bytes_includes_attachment_data_after_body_with_crlfs` | After `\r\n\r\n` separator, `body` bytes then `\r\n` then each file's bytes + trailing `\r\n` |
| `to_bytes_with_zero_files_emits_no_trailing_crlf` | Body terminator absent when files list is empty (byte-identical to plain-text compose path) |
| `to_proposal_size_includes_attachment_bytes_and_crlfs` | Proposal `size` = `header_bytes + body_bytes + (body→files CRLF if files else 0) + sum(file.bytes.len() + 2 for terminators)` |

P2:

| Test | Asserts |
|---|---|
| `from_bytes_parses_attachments_in_order` | Wire bytes with two `File:` headers → `msg.attachments` has 2 entries in declaration order |
| `from_bytes_round_trips_through_to_bytes` | `from_bytes(to_bytes(msg)) == msg` for a message with attachments |
| `from_bytes_errors_on_missing_terminator_crlf` | Wire bytes missing the body→file terminator → `ParseError::MissingAttachmentTerminator` |
| `from_bytes_errors_on_truncated_attachment` | Wire bytes shorter than `File:` size header → `ParseError::TruncatedAttachment` |
| `from_bytes_errors_on_malformed_file_header` | `File: garbage` value → `ParseError::MalformedFileHeader` |
| `from_bytes_handles_empty_attachment` | `File: 0 <name>` → `msg.attachments` has entry with 0-byte data |

### 8.3 Golden vector test (highest correctness signal) (P1)

Vendor a copy of `wl2k-go`'s `LPE5NXDVLVSQ.b2f` fixture into `src-tauri/tests/fixtures/wl2k-go/LPE5NXDVLVSQ.b2f` (wl2k-go is MIT-licensed; tuxlink is MIT-licensed; vendoring is permitted with license attribution in the test file header).

Build a Rust `Message` with the same headers (Mid, Date, From, To, Mbo, Subject, Type, Content-*) + the same text body + the same file (read raw bytes from the fixture file's attachment slice). Serialize via `to_bytes()`. Assert byte-for-byte equality against the fixture.

Also: `from_bytes(fixture_bytes)` round-trips the message — headers match, body matches, single attachment matches by bytes + name.

### 8.4 Integration tests — `winlink_backend.rs` (P4)

| Test | Asserts |
|---|---|
| `native_backend_send_message_stores_attachments` | Pass `OutboundMessage` with one attachment; verify outbox file contains `File:` header + attachment bytes |
| `native_backend_send_message_returns_message_id_not_option` (new) | Returns `Ok(MessageId)`, not `Ok(Some(MessageId))`; test calls and matches on the return shape |
| `native_backend_send_message_emits_wire_log_lines` (new) | After send, the session log contains a `LogSource::Wire` line with the `FC EM` proposal text |
| `two_native_backends_exchange_message_with_attachment` (extends existing) | Sender → receiver via in-process telnet loopback; receiver `from_bytes` yields the attachment intact |
| `from_bytes_parses_attachment_received_from_telnet_loopback` (new) | End-to-end: encode + send + receive + decode preserves attachment bytes exactly |

### 8.5 Removed and partial-edited tests

Full delete:
- `src-tauri/tests/pat_client_test.rs`
- `src-tauri/tests/pat_config_test.rs`
- `src-tauri/tests/pat_process_test.rs`

**Partial edit** (P9):
- `src-tauri/tests/winlink_backend_test.rs` — remove the ~8-12 PatBackend-using test cases (22 PatBackend hits total); preserve non-Pat coverage. Specific test functions to identify by grep'ing for `PatBackend` inside `#[test]` boundaries.
- `src-tauri/tests/ui_commands_test.rs` — remove the ~3-5 PatBackend-using test cases (10 PatBackend hits total); same approach.

The plan task descriptions for P9 will enumerate every test function affected.

### 8.6 Operator-driven CMS smoke (post-merge)

§7.3 above. Not a CI test; operator runs manually against `cms-z.winlink.org`.

## 9. Risks & open questions

Numbered for adversarial-review reference. Rev-2 replaces rev-1's risks (the Body: math one was misframed; new ones added).

1. **Header sort with `File:`**: assertion is that `File:` lands between `Date:` and `From:` alphabetically. Verify by test in §8.2. If wl2k-go puts `File:` somewhere else, the Rust port must match.
2. **CRLF terminator round-trip**: §3's claim depends on `to_bytes` emitting + `from_bytes` consuming the terminators correctly. The new round-trip test in §8.2 catches asymmetry.
3. **Q-encoded filename round-trip**: receiver parsers (Pat, WLE) must accept the Q-encoded form. wl2k-go's `WordDecoder` decodes it; the Rust port's `from_bytes` currently does NOT decode Q-encoded filenames — it stores the encoded form as the filename string. This is acceptable for the round-trip test (`to_bytes(from_bytes(x)) == x`) but means inbound display will show Q-encoded names until a follow-up decodes them. Tracked as a follow-up bd issue. *Adversarial review: confirm acceptable.*
4. **CMS rejection on `File:` header**: adding `File:` is standard B2F but verify the CMS does not flag tuxlink (whose client SID is not yet registered with the production Winlink CMS; `cms-z.winlink.org` is the dev CMS that accepts unregistered clients) for this. Smoke against `cms-z.winlink.org` confirms.
5. **lzhuf input size with large attachments**: a 50 MB attachment is lzhuf-compressed in-memory as one buffer. Pi 5 has plenty of RAM but a future low-memory target could OOM. §7.3 smoke (g) covers 1 MB; larger sizes deferred to a follow-up if real ops demand.
6. **CMS FS-reject handling**: if CMS rejects a proposal (`FS -`), `BackendError::MessageRejected` surfaces. The message stays in the outbox. The UI must know whether to retry or surface to the operator. *Design decision: keep in outbox + log; operator manually retries via UI.* Documented in §4.8.
7. **Mid-transfer connection drop**: `BackendError::TransportFailed` with offset info. The message stays in the outbox; the next connect retries the whole proposal (no resume). This is a conservative choice; resume-via-`Resume:` is a deferred follow-up.
8. **Keyring migration mid-flight failure**: if the migration writes to `"tuxlink"` but fails to delete `"tuxlink-pat"`, both entries exist. Next read prefers `"tuxlink"` (new code path). No harm; just stale data. Documented.
9. **Config field deprecation discoverability**: an operator running `cat ~/.config/tuxlink/config.json` after upgrade sees `pat_mbo_address` still present (their old value). Per P8, the deprecation log fires once on first read. UI does not yet expose this state. Sufficient for v0.0.1; revisit if operators surface confusion.
10. **`PatBackend::from_url` test fixture replacement (`NativeBackend::test_fixture`)**: §4.4's `Box::leak(Box::new(tempdir))` is unusual. *Adversarial review: confirm acceptable for #[cfg(test)] code; the alternative (passing tempdir lifetimes through every test) is heavier.*
11. **Wizard connect-only test-send UX regression**: operators who relied on the wizard's test-message-to-self affordance for end-to-end verification lose it. Mitigation: docs/install.md updated to point at `native_cms_probe --send` for that need.
12. **CI on intermediate commits**: P1–P12 must each leave `cargo build --workspace` + `cargo test --workspace` green. Subagents shipping individual phases run those before declaring done.
13. **`LogSource::Pat` removal ripple in TS frontend**: P6 forces same-commit `logProjection.ts` update. If the frontend type defs have a `'pat'` literal-type in a discriminated union, the TS compiler will require updates at every consumer. R3 + R2 said this is a single update site; verify in P6.
14. **Submodule deinit edge cases**: §7.5 covers. R4 P1 raised concern about operators with WIP in the submodule; §7.5 step 1 inventories.
15. **release-please configuration**: per R2 verified-correct list, no Pat-specific release-please surgery needed. Confirm by `grep -i pat .release-please-*.json` before merge.
16. **HTML Forms spec rev-3 included in this PR (P12)**: ensures the next-session author starts on the post-Path-A spec. *Adversarial review: confirm the rev-3 edit is in scope, not a separate PR.*

## 10. References

- bd tuxlink-9phd — this work item; `bd show tuxlink-9phd` for current state
- [`dev/handoffs/2026-05-30-heron-tanager-bog-html-forms-paused-pat-strip-prereq.md`](../../../dev/handoffs/2026-05-30-heron-tanager-bog-html-forms-paused-pat-strip-prereq.md) — operator pivot context
- [HTML Forms spec rev-2](2026-05-30-html-forms-design.md) — the work this PR unblocks (revised to rev-3 in P12)
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/fbb/` — canonical B2F implementation (read-only reference; no Go code ships in tuxlink)
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/fbb/message.go` — `Message.Write()` (the function the Rust serializer mirrors); lines 436-437 (Q-encode charset), 466-478 (file-byte serialization with CRLF terminators)
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/fbb/header.go:99-133` — header serialization with `Mid:`-first + canonicalized-key alphabetical sort
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/fbb/proposal.go:61-93` — proposal size computation
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/lzhuf/testdata/LPE5NXDVLVSQ.b2f` — golden vector for §8.3 (31,380 bytes; MIT-licensed)
- `~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/LICENSE` — MIT
- [ADR 0003](../../adr/0003-no-sqlite-pat-owns-mailbox.md) — superseded by this work (Pat as authoritative mailbox)
- [ADR 0011](../../adr/0011-fork-pat-for-tuxlink.md) — superseded by this work (fork for cred-handling)
- [ADR 0014](../../adr/0014-clean-sheet-modem-no-prior-art-examination.md) — rigor template for ADR 0016
- [`docs/live-cms-testing-policy.md`](../../live-cms-testing-policy.md) — CMS telnet authorization scope (governs §7.3 smoke)
- [`docs/pitfalls/implementation-pitfalls.md`](../../pitfalls/implementation-pitfalls.md) — RADIO-1 entry (clarifies what this work does NOT touch)
- Adversarial transcripts (gitignored): `dev/adversarial/2026-05-30-pat-strip-native-attachments-claude-r{1,2,3,4}-*.md`

---

End of design spec rev-2. Ready for Codex R5 cross-provider review.
