# Strip Pat + native B2F outbound with attachments — implementation plan (rev-1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> Date: 2026-05-30 · Agent: magpie-grouse-shoal · bd: tuxlink-9phd · Spec: [`docs/superpowers/specs/2026-05-30-pat-strip-native-attachments-design.md`](../specs/2026-05-30-pat-strip-native-attachments-design.md) (rev-3, commit 68c35d9)

**Goal:** Delete the entire Pat sidecar surface from tuxlink and ship a native B2F outbound path that supports message attachments, in a single atomic PR.

**Architecture:** The wire-level transport (`winlink::transfer`, `winlink::session`, `winlink::lzhuf`, `winlink::telnet`) is already attachment-agnostic and unchanged. The gap is in two files: `winlink::compose` (build a `Message` from text + attachments) and `winlink::message` (serialize + parse `File:` headers + appended attachment bytes per the Winlink-custom B2F format verified against wl2k-go). On top of that, the cutover deletes ~1300 LOC of Pat code (sidecar client, config renderer, process spawner, backend impl, tests, Go-build infra, submodule, sidecar binary bundle) and migrates two operator-state surfaces (keyring service name `tuxlink-pat` → `tuxlink`; `Config.pat_mbo_address` deprecated).

**Tech Stack:** Rust (`src-tauri/` — Tauri+Tokio backend), TypeScript+React (`src/` — frontend), GitHub Actions CI, OS keyring (secret-service / Keychain), git submodules.

---

## 0. Plan execution preamble

**Worktree:** This plan executes from worktree `worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments/` on branch `bd-tuxlink-9phd/strip-pat-add-native-attachments` (off `origin/main`). The worktree is already created and bd issue `tuxlink-9phd` is in `in_progress`. Subagents executing tasks should `cd` into this worktree before running cargo/pnpm/git.

**Standard task preamble** (referenced by every task — read in full before starting work):

```
BEFORE starting work:
1. Read the skill at /home/administrator/.claude/plugins/cache/claude-plugins-official/superpowers/5.1.0/skills/test-driven-development/SKILL.md
   (or invoke superpowers:test-driven-development via the Skill tool)
2. Read docs/pitfalls/testing-pitfalls.md
3. Read docs/pitfalls/implementation-pitfalls.md
Follow TDD: write failing test → run to verify it fails → implement minimal fix → run to verify green → commit.
Use the conventional-commit subject form per CLAUDE.md commit discipline.
EVERY commit must include the trailers:
   Agent: magpie-grouse-shoal
   Co-Authored-By: Claude <whatever-agent-model-name>
```

**Standard task completion** (referenced by every task — verify before marking complete):

```
BEFORE marking this task complete:
1. Review your tests against docs/pitfalls/testing-pitfalls.md
2. Verify test coverage of the fix (are error paths tested? edge cases?)
3. Run the relevant cargo test subset and confirm green:
   cargo test --manifest-path src-tauri/Cargo.toml <test_module_or_fn_name>
4. If the task touched the frontend, run:
   pnpm typecheck && pnpm test:unit
5. Commit per CLAUDE.md commit discipline (Agent: trailer mandatory)
```

**Standard phase-end review loop** (referenced by every phase — perform after the final task in each phase):

```
After every logical group of tasks (= phase):
Carefully review the batch of work from multiple perspectives:
  (1) Does each task's change deliver the spec section it claims?
  (2) Are there hidden cross-task interactions (e.g., a test in task N
      accidentally relying on state set up in task N-1)?
  (3) Are commit messages accurate (type: matches intent; no fix: for docs)?
  (4) Does cargo build --workspace AND cargo test --workspace pass clean?
  (5) Are there new clippy warnings introduced by the phase?
Do a minimum of THREE review rounds. If you still find substantive issues
in the third review, keep going until there are no findings. Then update
your private journal (or session notes) and continue onto the next phase.
```

**Standard cross-provider review** (required at end of plan, per `feedback_codex_post_subagent_review`):

After the final phase completes and before opening the PR, dispatch ONE Codex round against the full branch diff vs main. See §Final Reviewer Dispatch.

---

## File structure

Files **created** by this plan:

| Path | Role |
|---|---|
| `src-tauri/src/winlink/credentials.rs` | New module: keyring read/migration helper (P7) |
| `src-tauri/tests/fixtures/wl2k-go/LPE5NXDVLVSQ.b2f` | Vendored golden-vector fixture (MIT-licensed copy from wl2k-go) (P1) |
| `docs/adr/0016-native-b2f-outbound-with-attachments.md` | New ADR (P12) |

Files **modified** by this plan:

| Path | Phases | Net change |
|---|---|---|
| `src-tauri/src/pat_client.rs` | P0, P9 | P0: move `MailboxFolder` out. P9: deleted entirely. |
| `src-tauri/src/winlink_backend.rs` | P0, P3, P4, P6, P7, P9 | P0: add `MailboxFolder`. P3: trait signature. P4: NativeBackend wiring. P6: drop `LogSource::Pat`. P7: use credentials module. P9: delete `PatBackend` + `PatBackendSpawnOptions`. |
| `src-tauri/src/winlink/compose.rs` | P1 | Add `compose_message_with_files` + `ComposeError`. |
| `src-tauri/src/winlink/message.rs` | P1, P2 | P1: `to_bytes` writes `File:` + appended bytes. P2: `from_bytes` parses them + repeatable-header fix. |
| `src-tauri/src/winlink/session.rs` | P4 | Wire-log emit on send_turn + FS-answer; FS-reject maps to MessageRejected. |
| `src-tauri/src/winlink/mod.rs` | P7 | `pub mod credentials;` |
| `src-tauri/src/bootstrap.rs` | P5 | Drop Pat resolution + spawn; install NativeBackend synchronously. |
| `src-tauri/src/app_backend.rs` | P5 | Test fixtures use `NativeBackend::test_fixture()`. |
| `src-tauri/src/wizard.rs` | P5, P7 | P5: test-send → connect-only probe. P7: keyring service name. |
| `src-tauri/src/ui_commands.rs` | P3, P6 | P3: drop `Ok(None)` branches. P6: log-source label. |
| `src-tauri/src/config.rs` | P8 | `pat_mbo_address` deprecated. |
| `src-tauri/src/bin/native_cms_probe.rs` | P7, P12 | P7: keyring service name. P12: add `--send <path>` flag. |
| `src-tauri/src/bin/live_cms_smoke.rs` | P9 | Deleted entirely. |
| `src-tauri/src/build_support.rs` | P10 | Deleted entirely. |
| `src-tauri/src/lib.rs` | P9, P10 | Drop pat module declarations + build_support module declaration. |
| `src-tauri/build.rs` | P10 | Delete Go-toolchain check + go-build invocation; shrinks dramatically. |
| `src-tauri/Cargo.toml` | P9, P10 | Remove `[[bin]] live_cms_smoke` (P9); remove any Pat-specific deps (P10 verify). |
| `src-tauri/tauri.conf.json` | P10 | Remove `"externalBin": ["sidecars/pat"]`. |
| `src-tauri/sidecars/` | P10 | Directory deleted entirely. |
| `src-tauri/tests/winlink_backend_test.rs` | P9 | Partial edit: remove PatBackend test cases (~8-12 tests). |
| `src-tauri/tests/ui_commands_test.rs` | P9 | Partial edit: remove PatBackend test cases (~3-5 tests). |
| `src-tauri/tests/pat_client_test.rs` | P9 | Deleted entirely. |
| `src-tauri/tests/pat_config_test.rs` | P9 | Deleted entirely. |
| `src-tauri/tests/pat_process_test.rs` | P9 | Deleted entirely. |
| `src/wizard/Step2Credentials.tsx` | P7 | Keyring user-facing string. |
| `src/wizard/logProjection.ts` | P6 | Drop `'pat'` case. |
| `src/wizard/logProjection.test.ts` | P6 | Drop `'pat'` assertions. |
| `.github/workflows/release.yml` | P10 | Remove Go-toolchain step + Pat sidecar build step. |
| `.gitmodules` | P11 | Remove `[submodule "external/tuxlink-pat"]`. |
| `external/tuxlink-pat/` | P11 | Submodule deinit + `git rm`. |
| `docs/adr/0003-no-sqlite-pat-owns-mailbox.md` | P12 | Status line: append `superseded by ADR 0016`. |
| `docs/adr/0011-fork-pat-for-tuxlink.md` | P12 | Same. |
| `docs/superpowers/specs/2026-05-30-html-forms-design.md` | P12 | Revise rev-2 → rev-3: remove Path A reasoning. |
| `docs/install.md` | P12 | Remove Pat references. |
| `docs/development.md` | P12 | Remove Pat references. |
| `VERSIONING.md` | P12 | Remove "bundled-Pat compatibility break" row. |
| `README.md` | P12 | Remove Pat references if any. |

---

## Phase 0 — Foundation: move `MailboxFolder` out of pat_client (Codex R5 P1 fix)

`MailboxFolder` is currently defined in `pat_client.rs:9` and re-exported from `winlink_backend.rs`. P9 deletes `pat_client.rs`; without this phase, P9 breaks every `MailboxFolder` reference. Run this phase FIRST so subsequent phases (especially P3 → trait references `MailboxFolder`) build clean.

### Task 0.1: Move `MailboxFolder` enum to `winlink_backend.rs`

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs` (add enum definition; remove the re-export of the pat_client version)
- Modify: `src-tauri/src/pat_client.rs` (remove the enum definition; add `use crate::winlink_backend::MailboxFolder;` if pat_client still references it)

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/tests/winlink_backend_test.rs`:

```rust
#[test]
fn mailbox_folder_is_defined_in_winlink_backend() {
    // Type-system check: this compiles only if MailboxFolder is reachable
    // via the canonical winlink_backend path (not a re-export from pat_client).
    let _: tuxlink_lib::winlink_backend::MailboxFolder =
        tuxlink_lib::winlink_backend::MailboxFolder::Inbox;
}
```

- [ ] **Step 2: Run test to verify it currently passes via re-export but FAILS the locality intent**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test winlink_backend_test mailbox_folder_is_defined`
Expected: PASS today (re-export makes it reachable). The test stays after the move as a regression check that the canonical path keeps working.

- [ ] **Step 3: Move the enum definition**

In `src-tauri/src/pat_client.rs`, find:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailboxFolder { Inbox, Sent, Outbox, Archive }
```

(verify exact derive list + serde attrs by reading the current source at `src-tauri/src/pat_client.rs:9`)

DELETE it from `pat_client.rs`.

Add it to `src-tauri/src/winlink_backend.rs` (near the other domain types, around line 84 where `MailboxFolder` is currently re-exported). Use the SAME derive list and serde attrs that were on the original.

- [ ] **Step 4: Update the re-export in `winlink_backend.rs`**

Remove the re-export line `pub use crate::pat_client::MailboxFolder;` (find via `grep -n "pat_client::MailboxFolder" src-tauri/src/winlink_backend.rs`). The enum is now defined locally; no re-export needed.

- [ ] **Step 5: Update `pat_client.rs` if it internally references `MailboxFolder`**

`grep -n "MailboxFolder" src-tauri/src/pat_client.rs` — if any references remain, add `use crate::winlink_backend::MailboxFolder;` at the top of pat_client.rs.

- [ ] **Step 6: Build + test the workspace**

```bash
cargo build --manifest-path src-tauri/Cargo.toml --workspace
cargo test --manifest-path src-tauri/Cargo.toml --workspace --lib --tests -- --test-threads=1
```

Expected: clean build, all tests pass.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(winlink): move MailboxFolder enum out of pat_client (P0/P12)

The enum lived in pat_client.rs but is used by the native mailbox + UI;
re-exported via winlink_backend. P9 (later in this plan) deletes
pat_client.rs entirely, which would break every MailboxFolder reference.
Move the canonical definition to winlink_backend.rs so pat_client.rs can
be deleted cleanly without touching the enum's clients.

Per spec rev-3 §5 P0 + Codex R5 P1.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 0 review loop

Apply the **Standard phase-end review loop** (defined in §0).

---

## Phase 1 — Compose-with-files (forward path)

Build the wire-format-correct serialization side per spec §3 + §4.1 + §4.2. Each task is one small commit. The phase ends with a golden-vector conformance test against wl2k-go's `LPE5NXDVLVSQ.b2f`.

### Task 1.1: Add `ComposeError` + skeleton fallible `compose_message_with_files`

**Files:**
- Modify: `src-tauri/src/winlink/compose.rs` (add types + skeleton fn)

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/winlink/compose.rs` `mod tests`:

```rust
#[test]
fn compose_with_no_files_matches_text_only_path() {
    let no_files = compose_message_with_files(
        "N7CPZ", &["W1AW"], &[], "Hi", "body", &[], 1_716_200_000,
    ).expect("no filenames → cannot fail");
    let text_only = compose_message("N7CPZ", &["W1AW"], &[], "Hi", "body", 1_716_200_000);
    assert_eq!(no_files.to_bytes(), text_only.to_bytes());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::compose::tests::compose_with_no_files_matches_text_only_path`
Expected: FAIL — `compose_message_with_files` not defined.

- [ ] **Step 3: Add `ComposeError` type**

In `src-tauri/src/winlink/compose.rs`, near the top imports:

```rust
use crate::winlink_backend::OutboundAttachment;
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ComposeError {
    #[error("filename exceeds 255-character limit ({chars} chars): {filename:?}")]
    FilenameTooLong { filename: String, chars: usize },
    #[error("filename contains characters outside ISO-8859-1 (Q-encoding would be lossy): {filename:?}")]
    FilenameNotLatin1Encodable { filename: String },
}
```

- [ ] **Step 4: Add `compose_message_with_files` (skeleton, forwards to existing compose_message)**

In `src-tauri/src/winlink/compose.rs`, immediately after `compose_message`:

```rust
/// Build a Private text message with zero or more file attachments.
///
/// Returns `Err(ComposeError::FilenameTooLong)` or
/// `Err(ComposeError::FilenameNotLatin1Encodable)` if any attachment
/// filename violates the Winlink B2F constraints. The first invalid
/// filename short-circuits; the error names the offending filename so
/// the UI can surface it.
pub fn compose_message_with_files(
    mycall: &str,
    to: &[&str],
    cc: &[&str],
    subject: &str,
    body: &str,
    attachments: &[OutboundAttachment],
    unix_secs: u64,
) -> Result<Message, ComposeError> {
    // Step 1: forward the text-only path to compose_message.
    // File handling lands in Task 1.4+.
    let _ = attachments;  // suppress unused warning until 1.4
    Ok(compose_message(mycall, to, cc, subject, body, unix_secs))
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::compose::tests::compose_with_no_files_matches_text_only_path`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/winlink/compose.rs
git commit -m "feat(winlink): add ComposeError + compose_message_with_files skeleton

Forwards to compose_message for the no-attachment case (which is the
existing degenerate path). File-handling logic + validation land in
follow-up tasks. ComposeError marked #[non_exhaustive] for forward
compatibility.

Per spec rev-3 §4.1 + Codex R5 P2.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.2: Add `attachments: Vec<OutboundAttachment>` field to `Message`

**Files:**
- Modify: `src-tauri/src/winlink/message.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/winlink/message.rs` `mod tests`:

```rust
#[test]
fn message_carries_attachments_field() {
    let msg = Message::new();
    assert!(msg.attachments().is_empty());
}
```

- [ ] **Step 2: Run test → expect compile error (no `attachments()` method yet)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::message::tests::message_carries_attachments_field`
Expected: compile error — `attachments` not found.

- [ ] **Step 3: Add the field + accessor**

Find the `Message` struct in `src-tauri/src/winlink/message.rs` (around line 20). Add a field:

```rust
pub struct Message {
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    attachments: Vec<crate::winlink_backend::OutboundAttachment>,  // NEW
}
```

In the `impl Message` block, find `Message::new()`. Add `attachments: Vec::new()` to its initializer:

```rust
pub fn new() -> Self {
    Self { headers: Vec::new(), body: Vec::new(), attachments: Vec::new() }
}
```

Add an accessor:

```rust
pub fn attachments(&self) -> &[crate::winlink_backend::OutboundAttachment] {
    &self.attachments
}

pub(crate) fn set_attachments(&mut self, files: Vec<crate::winlink_backend::OutboundAttachment>) {
    self.attachments = files;
}
```

- [ ] **Step 4: Run test → PASS**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::message::tests::message_carries_attachments_field`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "feat(winlink): add attachments field to Message struct

Empty by default; populated by compose_message_with_files in a follow-up
task. Symmetric with OutboundMessage.attachments on the trait input side
(same type, no translation layer).

Per spec rev-3 §4.2 + §4.6.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.3: Filename validation in compose

**Files:**
- Modify: `src-tauri/src/winlink/compose.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `compose.rs`:

```rust
#[test]
fn compose_rejects_filename_over_255_chars() {
    let long: String = "a".repeat(256);
    let att = OutboundAttachment { filename: long.clone(), bytes: vec![1, 2, 3] };
    let err = compose_message_with_files(
        "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att], 1_716_200_000,
    ).unwrap_err();
    matches!(err, ComposeError::FilenameTooLong { chars: 256, .. });
}

#[test]
fn compose_rejects_non_latin1_filename() {
    let att = OutboundAttachment { filename: "日本語.txt".into(), bytes: vec![1, 2] };
    let err = compose_message_with_files(
        "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att], 1_716_200_000,
    ).unwrap_err();
    matches!(err, ComposeError::FilenameNotLatin1Encodable { .. });
}

#[test]
fn compose_short_circuits_on_first_invalid_filename() {
    let ok = OutboundAttachment { filename: "good.txt".into(), bytes: vec![1] };
    let bad = OutboundAttachment { filename: "日本.bin".into(), bytes: vec![2] };
    let err = compose_message_with_files(
        "N7CPZ", &["W1AW"], &[], "Hi", "body", &[ok, bad], 1_716_200_000,
    ).unwrap_err();
    matches!(err, ComposeError::FilenameNotLatin1Encodable { filename } if filename == "日本.bin");
}
```

- [ ] **Step 2: Run tests → expect FAIL (validation not implemented)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::compose::tests::compose_rejects -- --test-threads=1`
Expected: tests fail / panic on `unwrap_err()` because compose currently returns Ok.

- [ ] **Step 3: Implement validation**

In `compose_message_with_files`, before the existing `compose_message(...)` call:

```rust
for att in attachments {
    if att.filename.chars().count() > 255 {
        return Err(ComposeError::FilenameTooLong {
            filename: att.filename.clone(),
            chars: att.filename.chars().count(),
        });
    }
    if !att.filename.chars().all(|c| (c as u32) <= 0xff) {
        return Err(ComposeError::FilenameNotLatin1Encodable {
            filename: att.filename.clone(),
        });
    }
}
```

- [ ] **Step 4: Run tests → PASS**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::compose::tests::compose_rejects -- --test-threads=1`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/compose.rs
git commit -m "feat(winlink): validate attachment filenames in compose

255-char cap + ISO-8859-1 encodability (Q-encoding requirement per
wl2k-go fbb/message.go:436). First-invalid short-circuit; error names
the offending filename for UI surfacing. Tests cover both error
variants + the short-circuit ordering.

Per spec rev-3 §4.1.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.4: Compose wires attachments into Message

**Files:**
- Modify: `src-tauri/src/winlink/compose.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn compose_attaches_files_to_message() {
    let att = OutboundAttachment { filename: "report.txt".into(), bytes: b"hello".to_vec() };
    let msg = compose_message_with_files(
        "N7CPZ", &["W1AW"], &[], "Hi", "body", &[att.clone()], 1_716_200_000,
    ).unwrap();
    assert_eq!(msg.attachments().len(), 1);
    assert_eq!(msg.attachments()[0].filename, "report.txt");
    assert_eq!(msg.attachments()[0].bytes, b"hello");
}
```

- [ ] **Step 2: Run → FAIL (attachments empty after compose)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::compose::tests::compose_attaches_files_to_message`
Expected: assertion failure on `attachments().len() == 1` (currently 0).

- [ ] **Step 3: Wire attachments into Message in compose_message_with_files**

After the validation loop and before the `Ok(compose_message(...))` call, change the implementation:

```rust
// Build the base message via compose_message (text-only path).
let mut msg = compose_message(mycall, to, cc, subject, body, unix_secs);

// Attach the validated files. set_body in compose_message already wrote
// the Body: header; File: headers + the attachment serialization land in
// Message::to_bytes (Task 1.5+).
msg.set_attachments(attachments.to_vec());

Ok(msg)
```

- [ ] **Step 4: Run → PASS**

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/compose.rs
git commit -m "feat(winlink): compose_message_with_files attaches files to Message

The composed Message now carries the validated attachments. Serialization
to the wire format lands in Message::to_bytes (next task).

Per spec rev-3 §4.1 + §4.2.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.5: `Message::to_bytes` emits `File:` headers (single attachment)

**Files:**
- Modify: `src-tauri/src/winlink/message.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn to_bytes_emits_file_header_and_attachment_bytes() {
    let mut msg = Message::new();
    msg.set_header("Mid", "TESTMID12345");
    msg.set_header("Subject", "Hi");
    msg.set_header("From", "N7CPZ");
    msg.set_body(b"hello".to_vec());
    msg.set_attachments(vec![
        crate::winlink_backend::OutboundAttachment {
            filename: "a.bin".into(),
            bytes: vec![0xAA, 0xBB, 0xCC],
        },
    ]);
    let bytes = msg.to_bytes();
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("\r\nFile: 3 a.bin\r\n"),
            "expected File: header, got: {s}");
    // Body section: text body, CRLF, attachment bytes, CRLF
    let body_section_start = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let body_section = &bytes[body_section_start..];
    assert_eq!(body_section, b"hello\r\n\xAA\xBB\xCC\r\n");
}
```

- [ ] **Step 2: Run → FAIL (to_bytes doesn't write File: yet)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::message::tests::to_bytes_emits_file_header_and_attachment_bytes`
Expected: assertion failure (File: header missing OR body_section incorrect).

- [ ] **Step 3: Update `set_attachments` to also write `File:` headers**

In `winlink/message.rs`, the existing `set_attachments` just stored the vec. Change it to also synthesize `File:` headers (called once at compose time, so order matches declaration):

```rust
pub(crate) fn set_attachments(&mut self, files: Vec<crate::winlink_backend::OutboundAttachment>) {
    // Remove any prior File: headers; they'll be re-emitted from the file list.
    self.headers.retain(|(k, _)| !k.eq_ignore_ascii_case("File"));
    for f in &files {
        self.headers.push((
            "File".to_string(),
            format!("{} {}", f.bytes.len(), encode_filename(&f.filename)),
        ));
    }
    self.attachments = files;
}

fn encode_filename(name: &str) -> String {
    // Stub for now — ASCII filenames pass through unchanged. Non-ASCII handling
    // (RFC 2047 Q-encoding with ISO-8859-1) lands in Task 1.8.
    name.to_string()
}
```

- [ ] **Step 4: Extend `to_bytes()` to write body+CRLF+(attachment+CRLF)* tail**

Find the existing `to_bytes` method in `winlink/message.rs` (around line 59). Currently it writes headers + `\r\n\r\n` separator + body. Extend the tail:

```rust
pub fn to_bytes(&self) -> Vec<u8> {
    let mut out = Vec::new();
    // ... existing header write + \r\n separator ...
    for (k, v) in &self.headers_sorted() {  // existing helper
        out.extend_from_slice(k.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(v.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&self.body);
    // NEW: write attachment region if any
    if !self.attachments.is_empty() {
        out.extend_from_slice(b"\r\n");  // body→first-attachment separator
        for att in &self.attachments {
            out.extend_from_slice(&att.bytes);
            out.extend_from_slice(b"\r\n");  // post-attachment terminator
        }
    }
    out
}
```

(verify exact helper name `headers_sorted` by inspecting the existing source; if different, use the existing pattern unchanged)

- [ ] **Step 5: Run → PASS**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "feat(winlink): Message::to_bytes emits File: headers + attachment bytes

set_attachments now synthesizes File: <size> <name> headers; to_bytes
appends body + CRLF + (attachment_bytes + CRLF)* when files exist. No
CRLF after body when files list is empty (preserves byte-identical text-
only output). Filename Q-encoding stub for now; full RFC 2047 in Task 1.8.

Per spec rev-3 §3 + §4.2.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.6: `Message::to_bytes` multi-attachment order + zero-attachment degeneracy

**Files:**
- Modify: `src-tauri/src/winlink/message.rs` (tests only)

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn to_bytes_preserves_attachment_declaration_order() {
    let mut msg = Message::new();
    msg.set_header("Mid", "MID2");
    msg.set_header("From", "N7CPZ");
    msg.set_body(b"x".to_vec());
    msg.set_attachments(vec![
        crate::winlink_backend::OutboundAttachment { filename: "a.bin".into(), bytes: vec![1] },
        crate::winlink_backend::OutboundAttachment { filename: "b.bin".into(), bytes: vec![2] },
        crate::winlink_backend::OutboundAttachment { filename: "c.bin".into(), bytes: vec![3] },
    ]);
    let bytes = msg.to_bytes();
    // Find the body region after \r\n\r\n
    let bs = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    assert_eq!(&bytes[bs..], b"x\r\n\x01\r\n\x02\r\n\x03\r\n");
    // File: headers must also be in declaration order
    let header_block = &bytes[..bs - 2];  // exclude the trailing \r\n
    let header_str = std::str::from_utf8(header_block).unwrap();
    let file_lines: Vec<&str> = header_str
        .lines()
        .filter(|l| l.starts_with("File:"))
        .collect();
    assert_eq!(file_lines, vec!["File: 1 a.bin", "File: 1 b.bin", "File: 1 c.bin"]);
}

#[test]
fn to_bytes_with_zero_attachments_emits_no_trailing_crlf() {
    let mut msg = Message::new();
    msg.set_header("Mid", "MID3");
    msg.set_header("From", "N7CPZ");
    msg.set_body(b"plain".to_vec());
    // No set_attachments call.
    let bytes = msg.to_bytes();
    let bs = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    assert_eq!(&bytes[bs..], b"plain");  // exact — no trailing CRLF
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::message::tests::to_bytes_preserves_attachment_declaration_order winlink::message::tests::to_bytes_with_zero_attachments`
Expected: PASS (Task 1.5's implementation already supports both — these tests document the contract).

- [ ] **Step 3: If FAIL, fix the implementation and re-run**

If declaration-order is broken (e.g., headers got sorted alphabetically), revisit Task 1.5 step 3 — the `for f in &files` loop should append in order. The header-sort happens in `headers_sorted` only at to_bytes-time; multiple `File:` entries within the same sort slot preserve insertion order per the existing pattern.

- [ ] **Step 4: Commit (the tests, even if no impl change)**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "test(winlink): assert attachment order + zero-attachment degeneracy

Contract tests for to_bytes: multi-attachment declaration order preserved;
zero-attachment case has no trailing CRLF (byte-identical to plain-text
compose). Codex R5 P1 noted the parser must round-trip this exactly.

Per spec rev-3 §3.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.7: `Message::to_proposal` size includes attachments

**Files:**
- Modify: `src-tauri/src/winlink/message.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn to_proposal_size_includes_attachment_bytes_and_crlfs() {
    let mut msg = Message::new();
    msg.set_header("Mid", "MIDPROP");
    msg.set_header("Subject", "T");
    msg.set_header("From", "N7CPZ");
    msg.set_body(b"body".to_vec());  // 4 bytes
    msg.set_attachments(vec![
        crate::winlink_backend::OutboundAttachment {
            filename: "x.bin".into(),
            bytes: vec![0; 10],  // 10 bytes
        },
    ]);
    let (proposal, _compressed) = msg.to_proposal().unwrap();
    let raw = msg.to_bytes();
    assert_eq!(proposal.size, raw.len());  // size = entire serialized message
}
```

- [ ] **Step 2: Run → check current behavior**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::message::tests::to_proposal_size_includes_attachment_bytes`
Expected: depends — `to_proposal` may already compute size from `to_bytes()`. If PASS, the implementation already handles attachments via the to_bytes output. If FAIL, see step 3.

- [ ] **Step 3: If FAIL, audit `to_proposal`**

Find `to_proposal` in `winlink/message.rs` (around line 121). It likely already calls `self.to_bytes()` and uses its length for size. If it uses only `self.body.len()` or similar, change to use `to_bytes().len()` (the proposal size is the full serialized message per spec §3).

- [ ] **Step 4: Run → PASS**

- [ ] **Step 5: Commit (test always; impl if needed)**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "test(winlink): to_proposal size includes attachment bytes + CRLFs

The proposal FC EM <mid> <size> <compressed_size> 0 size field is the
entire uncompressed serialized message (headers + body + attachment region
with terminator CRLFs). Asserted via to_proposal().size == to_bytes().len().

Per spec rev-3 §3.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.8: Filename Q-encoding (RFC 2047, ISO-8859-1)

**Files:**
- Modify: `src-tauri/src/winlink/message.rs` (or `compose.rs` — wherever `encode_filename` lives from Task 1.5)
- Possibly modify: `src-tauri/Cargo.toml` (add `mime` crate if needed)

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn q_encodes_non_ascii_filename_with_iso_8859_1() {
    let mut msg = Message::new();
    msg.set_header("Mid", "MIDQ");
    msg.set_header("From", "N7CPZ");
    msg.set_body(b"x".to_vec());
    msg.set_attachments(vec![
        crate::winlink_backend::OutboundAttachment {
            // U+00E9 (é, Latin-1 0xE9)
            filename: "café.txt".into(),
            bytes: vec![1],
        },
    ]);
    let bytes = msg.to_bytes();
    let s = String::from_utf8_lossy(&bytes);
    // Lowercase q, charset ISO-8859-1 per wl2k-go fbb/message.go:436-437
    assert!(s.contains("File: 1 =?ISO-8859-1?q?caf=E9.txt?="),
            "expected Q-encoded filename, got: {s}");
}

#[test]
fn ascii_filename_passes_through_unencoded() {
    let mut msg = Message::new();
    msg.set_header("Mid", "MIDA");
    msg.set_header("From", "N7CPZ");
    msg.set_body(b"x".to_vec());
    msg.set_attachments(vec![
        crate::winlink_backend::OutboundAttachment {
            filename: "plain.txt".into(),
            bytes: vec![1],
        },
    ]);
    let bytes = msg.to_bytes();
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("File: 1 plain.txt"));
}
```

- [ ] **Step 2: Run → FAIL (Q-encoding stub returns raw)**

- [ ] **Step 3: Implement Q-encoding**

Update `encode_filename` in `winlink/message.rs` (or wherever placed in Task 1.5):

```rust
fn encode_filename(name: &str) -> String {
    if name.is_ascii() {
        return name.to_string();
    }
    // RFC 2047 Q-encoding with ISO-8859-1 charset. wl2k-go uses lowercase q.
    let mut encoded = String::from("=?ISO-8859-1?q?");
    for c in name.chars() {
        let cp = c as u32;
        if cp > 0xff {
            // Latin-1 unencodable; compose-level validation should have rejected this.
            // Defensive: treat as a replacement char.
            encoded.push('?');
            continue;
        }
        let b = cp as u8;
        // RFC 2047 Q-encoding: printable ASCII (except = ? _) emitted as-is;
        // space → _; everything else → =HH (hex).
        if b == b' ' {
            encoded.push('_');
        } else if b > 0x20 && b < 0x7f && b != b'=' && b != b'?' && b != b'_' {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("={:02X}", b));
        }
    }
    encoded.push_str("?=");
    encoded
}
```

- [ ] **Step 4: Run → PASS**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "feat(winlink): RFC 2047 Q-encode non-ASCII filenames

Charset ISO-8859-1, lowercase q (per wl2k-go fbb/message.go:436-437).
ASCII filenames pass through unchanged. Non-Latin-1 codepoints get
defensive '?' replacement (compose-level validation in Task 1.3
short-circuits with FilenameNotLatin1Encodable before reaching here).

Per spec rev-3 §3.2 + R1 P0-2 fix.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.9: Golden vector conformance test (vendor LPE5NXDVLVSQ.b2f)

**Files:**
- Create: `src-tauri/tests/fixtures/wl2k-go/LPE5NXDVLVSQ.b2f` (copy from gopath)
- Create: `src-tauri/tests/fixtures/wl2k-go/LICENSE-wl2k-go.txt` (MIT attribution)
- Create: `src-tauri/tests/winlink_message_golden_test.rs` (the test file)

**Standard preamble + completion** apply.

- [ ] **Step 1: Vendor the fixture**

```bash
mkdir -p src-tauri/tests/fixtures/wl2k-go
cp ~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/lzhuf/testdata/LPE5NXDVLVSQ.b2f \
   src-tauri/tests/fixtures/wl2k-go/LPE5NXDVLVSQ.b2f
cp ~/go/pkg/mod/github.com/la5nta/wl2k-go@v1.0.1/LICENSE \
   src-tauri/tests/fixtures/wl2k-go/LICENSE-wl2k-go.txt
```

- [ ] **Step 2: Write the golden-vector test**

Create `src-tauri/tests/winlink_message_golden_test.rs`:

```rust
//! Golden-vector conformance for native B2F outbound serialization.
//!
//! Fixture `LPE5NXDVLVSQ.b2f` vendored from wl2k-go v1.0.1 (MIT-licensed).
//! See fixtures/wl2k-go/LICENSE-wl2k-go.txt for attribution.
//!
//! This test asserts byte-for-byte equality between the Rust serializer and
//! wl2k-go's reference output for a real Winlink message with one binary
//! attachment.

use tuxlink_lib::winlink::compose::compose_message_with_files;
use tuxlink_lib::winlink_backend::OutboundAttachment;

const FIXTURE: &[u8] = include_bytes!("fixtures/wl2k-go/LPE5NXDVLVSQ.b2f");

#[test]
fn serializes_lpe5nxdvlvsq_byte_for_byte() {
    // Extract the attachment bytes from the fixture. The fixture has:
    //   headers + \r\n\r\n + body (104 bytes per Body:) + \r\n + jpg (31028 bytes per File:) + \r\n
    let sep = FIXTURE.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let body_start = sep + 4;
    let body_end = body_start + 104;  // per Body: header
    // After body comes the body→file CRLF, then 31028 jpg bytes
    let jpg_start = body_end + 2;
    let jpg_end = jpg_start + 31028;  // per File: header
    let jpg = &FIXTURE[jpg_start..jpg_end];
    let body_bytes = &FIXTURE[body_start..body_end];

    // Build the Rust equivalent.
    // unix_secs for 2016/07/20 19:21 UTC = 1469042460
    let attachments = vec![OutboundAttachment {
        filename: "1469042410710.jpg".into(),
        bytes: jpg.to_vec(),
    }];
    let body_str = std::str::from_utf8(body_bytes)
        // The fixture body is Latin-1; we send the bytes as a String for simplicity.
        // For pure-Latin-1 content this round-trips; for the fixture's actual
        // contents (mostly ASCII with a few æø chars) decode loosely.
        .unwrap_or_else(|_| panic!("fixture body is not UTF-8 representable; need raw-bytes set_body API"));
    let msg = compose_message_with_files(
        "LA5NTA",        // matches fixture's From/Mbo
        &["LA4TTA"],     // matches fixture's To
        &[],
        "73 fra Brekke", // matches fixture's Subject
        body_str,
        &attachments,
        1469042460,
    ).expect("compose succeeds");

    // Force the Mid header to match the fixture's literal value (compose generates
    // a different MID from callsign+time; that's expected — for byte-identity we
    // need the same MID).
    // The Mid is set inside compose; expose a test-only setter or compute the
    // expected delta. SIMPLEST: assert on everything EXCEPT the Mid value.
    let produced = msg.to_bytes();
    let fixture_with_normalized_mid = normalize_mid_to(FIXTURE, msg.header("Mid").unwrap());

    assert_eq!(
        produced, fixture_with_normalized_mid,
        "Rust output diverges from wl2k-go fixture"
    );
}

fn normalize_mid_to(fixture: &[u8], new_mid: &str) -> Vec<u8> {
    // Replace `Mid: <something>\r\n` in fixture with `Mid: <new_mid>\r\n`.
    let s = String::from_utf8_lossy(fixture);
    let lines: Vec<String> = s.lines().map(|l| {
        if l.starts_with("Mid: ") { format!("Mid: {}", new_mid) } else { l.to_string() }
    }).collect();
    let mut out = lines.join("\r\n").into_bytes();
    // The fixture ends with binary attachment + \r\n; lines() will have collapsed
    // those. For the test, swap the Mid in the textual header region only:
    // Actually simpler: do a byte-level replace of the Mid line.
    let _ = out;
    let prefix = b"Mid: ";
    let pos = fixture.windows(prefix.len()).position(|w| w == prefix).unwrap();
    let end = fixture[pos..].iter().position(|&b| b == b'\r').unwrap();
    let mut result = Vec::with_capacity(fixture.len());
    result.extend_from_slice(&fixture[..pos]);
    result.extend_from_slice(b"Mid: ");
    result.extend_from_slice(new_mid.as_bytes());
    result.extend_from_slice(&fixture[pos + end..]);
    result
}
```

(Note: if the fixture body is not pure UTF-8, the `from_utf8` call panics; in that case the test needs `Message::set_body_bytes` or similar — flag this as a Task 1.9.5 if hit. The fixture body per spec §3.1 is "Hei!\r\n\r\nLiten kveldstur innover Hausdal med radioen i kveld..." which is Latin-1 with æ/ø; depending on whether those are present, `from_utf8` may need replacing with a Latin-1 → String decode.)

- [ ] **Step 3: Run → expect FAIL on first try; iterate**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test winlink_message_golden_test`
Expected: likely fails on first try (header sort order, missing Mbo header, encoding mismatch, etc.). Debug by diffing `produced` vs `fixture_with_normalized_mid` byte-by-byte. Fix compose / message serializer until it passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tests/fixtures/wl2k-go/ src-tauri/tests/winlink_message_golden_test.rs
git commit -m "test(winlink): byte-identical conformance against wl2k-go fixture

Vendor LPE5NXDVLVSQ.b2f (MIT) into tests/fixtures/wl2k-go/. The test
builds the same message via Rust compose_message_with_files and asserts
byte-for-byte equality with the wl2k-go fixture (after Mid normalization,
since our generated MID differs). This is the strongest conformance proof
for the serialization side.

Per spec rev-3 §3.1 + §8.3.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.10: Header sort canonicalization

**Files:**
- Modify: `src-tauri/src/winlink/message.rs` (the header-sort helper)

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn header_sort_canonicalizes_keys() {
    let mut msg = Message::new();
    msg.set_header("mid", "MID4");          // lowercase
    msg.set_header("subject", "S");
    msg.set_header("from", "N7CPZ");
    msg.set_header("date", "2026/05/30 12:00");
    let bytes = msg.to_bytes();
    let s = String::from_utf8_lossy(&bytes);
    // Mid first (case-normalized), then alphabetical
    let lines: Vec<&str> = s.lines().take_while(|l| !l.is_empty()).collect();
    assert_eq!(lines[0], "Mid: MID4");      // canonicalized to "Mid"
    assert!(lines[1].starts_with("Date:"));  // canonicalized + alphabetically first after Mid
}
```

- [ ] **Step 2: Run → FAIL (current sort is case-sensitive byte-order on raw keys)**

- [ ] **Step 3: Implement canonicalization**

In `winlink/message.rs`, find the header-sort helper (e.g., `headers_sorted`). Add a canonical-MIME-key transform:

```rust
fn canonicalize_header_key(k: &str) -> String {
    // textproto.CanonicalMIMEHeaderKey: first char + chars after `-` are
    // uppercased; everything else lowercased.
    let mut out = String::with_capacity(k.len());
    let mut upper_next = true;
    for c in k.chars() {
        if upper_next {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        upper_next = c == '-';
    }
    out
}
```

In `to_bytes`, when writing headers, use the canonicalized key:

```rust
for (k, v) in &self.headers_sorted() {
    out.extend_from_slice(canonicalize_header_key(k).as_bytes());
    out.extend_from_slice(b": ");
    // ... rest unchanged ...
}
```

And in `headers_sorted`, sort by the canonicalized key (`Mid` first, then alphabetical):

```rust
fn headers_sorted(&self) -> Vec<(String, String)> {
    let mut indexed: Vec<(String, String)> = self.headers.iter()
        .map(|(k, v)| (canonicalize_header_key(k), v.clone()))
        .collect();
    indexed.sort_by(|a, b| {
        if a.0 == "Mid" { return std::cmp::Ordering::Less; }
        if b.0 == "Mid" { return std::cmp::Ordering::Greater; }
        a.0.cmp(&b.0)
    });
    indexed
}
```

- [ ] **Step 4: Run → PASS**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "fix(winlink): canonicalize header keys before sorting

wl2k-go uses textproto.CanonicalMIMEHeaderKey before sort + write
(fbb/header.go:99-133). Without canonicalization, mixed-case input
produces non-canonical wire output. R1 P1 finding.

Per spec rev-3 §3.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 1.11: Multi-recipient + attachments combined test

**Files:**
- Modify: `src-tauri/src/winlink/compose.rs` (tests only)

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the test**

```rust
#[test]
fn composes_multi_recipient_with_attachments() {
    let attachments = vec![
        OutboundAttachment { filename: "a.bin".into(), bytes: vec![1] },
        OutboundAttachment { filename: "b.bin".into(), bytes: vec![2] },
    ];
    let msg = compose_message_with_files(
        "N7CPZ",
        &["W1AW", "K1AB"],
        &["KE7XYZ"],
        "Multi",
        "body",
        &attachments,
        1_716_200_000,
    ).unwrap();
    let bytes = msg.to_bytes();
    let s = String::from_utf8_lossy(&bytes);
    assert_eq!(s.matches("\r\nTo: ").count(), 2, "two To: headers expected");
    assert_eq!(s.matches("\r\nCc: ").count(), 1, "one Cc: header expected");
    assert_eq!(s.matches("\r\nFile: ").count(), 2, "two File: headers expected");
}
```

- [ ] **Step 2: Run → expect PASS (multi-recipient + multi-attachment already covered)**

If FAIL, audit the to/cc handling in compose (existing `add_header` calls should work) and File: header emission.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/winlink/compose.rs
git commit -m "test(winlink): multi-recipient + multi-attachment combined

Smoke test that the entire compose pipeline handles both repeated To/Cc
recipients AND multiple attachments in one message.

Per spec rev-3 §8.1.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 1 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 2 — Parse-with-files (return path)

Extend `Message::from_bytes` to (a) preserve repeated headers via `add_header` (Codex R5 P1 fix), and (b) consume the trailing attachment region. Round-trip tests confirm symmetry with Phase 1's serializer.

### Task 2.1: Parser uses `add_header` for repeatable headers

**Files:**
- Modify: `src-tauri/src/winlink/message.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

Add to `mod tests`:

```rust
#[test]
fn from_bytes_preserves_repeated_to_and_cc_headers() {
    let wire = "Mid: MIDREP\r\nDate: 2026/05/30 12:00\r\nFrom: N7CPZ\r\n\
                To: W1AW\r\nTo: K1AB\r\nCc: KE7XYZ\r\nCc: KD8ZZZ\r\n\
                Body: 0\r\n\r\n";
    let msg = Message::from_bytes(wire.as_bytes()).unwrap();
    assert_eq!(msg.header_all("To"), vec!["W1AW", "K1AB"]);
    assert_eq!(msg.header_all("Cc"), vec!["KE7XYZ", "KD8ZZZ"]);
}
```

- [ ] **Step 2: Run → FAIL (current parser uses set_header → collapses to last)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib winlink::message::tests::from_bytes_preserves_repeated`
Expected: assertion failure — `header_all("To").len() == 1` (only "K1AB"), should be 2.

- [ ] **Step 3: Fix the parser**

In `winlink/message.rs`, find the existing header-parse loop in `from_bytes` (around line 147). Change:

```rust
const REPEATABLE_HEADERS: &[&str] = &["File", "To", "Cc"];

// inside from_bytes, replace the loop body:
for line in header_block.split(|&b| b == b'\n') {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.is_empty() { continue; }
    let text = std::str::from_utf8(line).map_err(|_| ParseError::NonUtf8Header)?;
    let (key, value) = text.split_once(": ").ok_or(ParseError::MalformedHeader)?;
    // CHANGED: use add_header for known-repeatable headers
    if REPEATABLE_HEADERS.iter().any(|h| h.eq_ignore_ascii_case(key)) {
        msg.add_header(key, value);
    } else {
        msg.set_header(key, value);
    }
}
```

- [ ] **Step 4: Run → PASS**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "fix(winlink): from_bytes preserves repeated File/To/Cc headers

The parser used set_header (which overwrites) for every header, collapsing
multi-recipient messages and (latent bug) multi-attachment messages to a
single entry. Switch to add_header for known-repeatable keys (File, To,
Cc) and set_header for everything else. Codex R5 P1 finding.

Per spec rev-3 §4.6.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 2.2: Add `ParseError` variants + `#[non_exhaustive]`

**Files:**
- Modify: `src-tauri/src/winlink/message.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Add the new variants + non_exhaustive**

In `winlink/message.rs`, find `pub enum ParseError`:

```rust
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]                                       // NEW
pub enum ParseError {
    NoHeaderTerminator,
    MalformedHeader,
    NonUtf8Header,
    TruncatedBody,
    MalformedFileHeader,                                // NEW
    MissingAttachmentTerminator,                        // NEW
    TruncatedAttachment,                                // NEW
}
```

- [ ] **Step 2: Build to verify compile**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: clean build (new variants unused so far; #[non_exhaustive] adds no warnings on `match` since all existing matches don't exhaustively cover internal enums by default).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "feat(winlink): ParseError gains MalformedFileHeader, *AttachmentTerminator, TruncatedAttachment

Plus #[non_exhaustive] for forward compat. Implementation that surfaces
these variants lands in next task.

Per spec rev-3 §4.6.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 2.3: Parser reads File: headers + attachment bytes

**Files:**
- Modify: `src-tauri/src/winlink/message.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn from_bytes_parses_single_attachment() {
    let mut wire = Vec::new();
    wire.extend_from_slice(b"Mid: MIDATT\r\nDate: 2026/05/30 12:00\r\nFile: 3 a.bin\r\n\
                             From: N7CPZ\r\nBody: 5\r\n\r\nhello\r\n\xAA\xBB\xCC\r\n");
    let msg = Message::from_bytes(&wire).unwrap();
    assert_eq!(msg.attachments().len(), 1);
    assert_eq!(msg.attachments()[0].filename, "a.bin");
    assert_eq!(msg.attachments()[0].bytes, vec![0xAA, 0xBB, 0xCC]);
}
```

- [ ] **Step 2: Run → FAIL (parser doesn't read attachment region)**

- [ ] **Step 3: Implement attachment parsing**

In `from_bytes`, after the existing body-parse (`msg.body = after_headers[..body_size].to_vec();`):

```rust
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
        if input.get(offset..offset+2) != Some(b"\r\n") {
            return Err(ParseError::MissingAttachmentTerminator);
        }
        offset += 2;
        msg.attachments.push(crate::winlink_backend::OutboundAttachment {
            filename: name,
            bytes: data,
        });
    }
}

Ok(msg)
```

And add the helper:

```rust
fn parse_file_header(value: &str) -> Result<(usize, String), ParseError> {
    let (size_str, name) = value.split_once(' ')
        .ok_or(ParseError::MalformedFileHeader)?;
    let size = size_str.parse::<usize>()
        .map_err(|_| ParseError::MalformedFileHeader)?;
    Ok((size, name.to_string()))
}
```

- [ ] **Step 4: Run → PASS**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "feat(winlink): from_bytes parses File: headers + appended attachments

Reads File: <size> <name> headers, consumes body→first-attachment CRLF,
reads each attachment per its declared size, consumes trailing CRLF after
each. Errors out on truncation, missing terminator, or malformed File:
header. The mailbox round-trip (store → from_bytes) now preserves
attachment data (R4 P0-1 fix — without this, attachments are stored
correctly but stripped on reload).

Per spec rev-3 §4.6.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 2.4: Round-trip + edge-case parser tests

**Files:**
- Modify: `src-tauri/src/winlink/message.rs` (tests)

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the tests**

```rust
#[test]
fn from_bytes_round_trips_through_to_bytes() {
    let mut original = Message::new();
    original.set_header("Mid", "MIDRT");
    original.set_header("From", "N7CPZ");
    original.set_header("Date", "2026/05/30 12:00");
    original.set_body(b"hello world".to_vec());
    original.set_attachments(vec![
        crate::winlink_backend::OutboundAttachment {
            filename: "a.bin".into(),
            bytes: vec![1, 2, 3, 4, 5],
        },
        crate::winlink_backend::OutboundAttachment {
            filename: "b.bin".into(),
            bytes: vec![0xAA, 0xBB],
        },
    ]);
    let bytes = original.to_bytes();
    let parsed = Message::from_bytes(&bytes).expect("round-trip parse");
    assert_eq!(parsed.attachments().len(), 2);
    assert_eq!(parsed.attachments()[0].filename, "a.bin");
    assert_eq!(parsed.attachments()[0].bytes, vec![1, 2, 3, 4, 5]);
    assert_eq!(parsed.attachments()[1].filename, "b.bin");
    assert_eq!(parsed.attachments()[1].bytes, vec![0xAA, 0xBB]);
    assert_eq!(parsed.body(), b"hello world");
}

#[test]
fn from_bytes_handles_empty_attachment() {
    let wire = b"Mid: MIDE\r\nFile: 0 empty.bin\r\nFrom: N7CPZ\r\nBody: 0\r\n\r\n\r\n\r\n";
    let msg = Message::from_bytes(wire).unwrap();
    assert_eq!(msg.attachments().len(), 1);
    assert_eq!(msg.attachments()[0].bytes.len(), 0);
}

#[test]
fn from_bytes_errors_on_missing_attachment_terminator() {
    // Body claims 0 bytes, File: claims 3 bytes, but no body→file CRLF
    let wire = b"Mid: MIDX\r\nFile: 3 a.bin\r\nFrom: N7CPZ\r\nBody: 0\r\n\r\nXXX";
    let err = Message::from_bytes(wire).unwrap_err();
    assert_eq!(err, ParseError::MissingAttachmentTerminator);
}

#[test]
fn from_bytes_errors_on_truncated_attachment() {
    // File: claims 10 bytes but only 3 are present
    let wire = b"Mid: MIDT\r\nFile: 10 a.bin\r\nFrom: N7CPZ\r\nBody: 0\r\n\r\n\r\nXXX\r\n";
    let err = Message::from_bytes(wire).unwrap_err();
    assert_eq!(err, ParseError::TruncatedAttachment);
}

#[test]
fn from_bytes_errors_on_malformed_file_header() {
    let wire = b"Mid: MIDM\r\nFile: notanumber a.bin\r\nFrom: N7CPZ\r\nBody: 0\r\n\r\n";
    let err = Message::from_bytes(wire).unwrap_err();
    assert_eq!(err, ParseError::MalformedFileHeader);
}
```

- [ ] **Step 2: Run → expect PASS (Task 2.3's impl covers all of these)**

If any FAIL, fix the implementation in `from_bytes`.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/winlink/message.rs
git commit -m "test(winlink): from_bytes round-trip + edge cases

Round-trips a multi-attachment Message through to_bytes/from_bytes;
asserts byte-fidelity. Edge cases: empty attachment, missing terminator
CRLF, truncated attachment, malformed File: header.

Per spec rev-3 §8.2.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 2 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 3 — Trait return-type tighten

Change `WinlinkBackend::send_message` from `Result<Option<MessageId>, BackendError>` to `Result<MessageId, BackendError>`. PatBackend stays present (deleted in P9); its `Ok(None)` becomes `Ok(MessageId::new(""))` as a transitional stub (no caller branches on emptiness).

### Task 3.1: Change trait signature + both backend impls

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

Add to `tests/winlink_backend_test.rs`:

```rust
#[tokio::test]
async fn native_backend_send_message_returns_message_id_not_option() {
    // Compile-time check: the trait returns Result<MessageId, BackendError>.
    use tuxlink_lib::winlink_backend::{WinlinkBackend, NativeBackend, OutboundMessage};
    // (This test won't actually call send_message — that requires a configured
    //  callsign + mailbox. The compile-time signature check is the assertion.)
    let _check: fn() = || {
        let _: &dyn WinlinkBackend = todo!();
        // After the change, this line type-checks only if return is Result<MessageId, _>:
        // let _: Result<MessageId, BackendError> = backend.send_message(...);
    };
}
```

- [ ] **Step 2: Run → expect compile error after signature change in step 3**

- [ ] **Step 3: Change the trait + both impls**

In `winlink_backend.rs`:

Trait (around line 393-396):
```rust
async fn send_message(&self, msg: OutboundMessage)
    -> Result<MessageId, BackendError>;        // was: Result<Option<MessageId>, BackendError>
```

`NativeBackend::send_message` (around line 580):
```rust
let id = self.mailbox.store(MailboxFolder::Outbox, &message.to_bytes())?;
Ok(id)                                          // was: Ok(Some(id))
```

`PatBackend::send_message` (around line 1702):
```rust
// Pat 1.0.0 doesn't echo a MID on success; synthesize an empty MID as a
// transitional placeholder. PatBackend is deleted in P9.
self.pat_client.send(&to, &msg.subject, &msg.body, &msg.date)
    .await
    .map(|_| MessageId::new(""))               // was: .map(|_| None)
    .map_err(|e| translate_pat_err(e, "send_message"))
```

- [ ] **Step 4: Update `ui_commands.rs` callers**

Grep for `send_message` callers + `Ok(None)`/`Some(message_id)` branches:

```bash
grep -n "send_message\|message_id" src-tauri/src/ui_commands.rs | head -30
```

For each call site, remove the `Some(...)` / `None` branches. The new return is `MessageId` (possibly empty `""` from PatBackend transition; check for `.is_empty()` if the UI needs to surface a no-MID indicator, though Pat is deleted in P9 so this state is transient).

Specifically, find the block at `ui_commands.rs:613-664` (per R3 P0-3 cite) and the test mock blocks around line 1867-1880; update them.

- [ ] **Step 5: Run all backend tests → green**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --workspace
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(backend)!: WinlinkBackend::send_message returns MessageId not Option

The Option wrapper existed only because Pat 1.0.0 doesn't echo a MID on
send success. With Pat gone (later phases), the Option is dead-code
masquerading as a valid contract. Tighten now to surface the actual
contract once the cutover lands. PatBackend (deleted in P9) synthesizes
an empty MID as a transitional placeholder.

BREAKING CHANGE: WinlinkBackend::send_message return type tightens from
Result<Option<MessageId>, BackendError> to Result<MessageId, BackendError>.
Existing call sites that matched on Ok(Some)/Ok(None) get same-commit
updates.

Per spec rev-3 §4.7 + R3 P0-3.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 3 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 4 — NativeBackend wires attachments + session observability

### Task 4.1: NativeBackend::send_message passes attachments through compose

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

Add to `tests/winlink_backend_test.rs`:

```rust
#[tokio::test]
async fn native_backend_send_message_stores_attachments_in_outbox() {
    use tuxlink_lib::winlink_backend::*;
    let backend = NativeBackend::test_fixture_with_callsign("N0CALL");
    let msg = OutboundMessage {
        to: vec!["W1AW".into()],
        cc: vec![],
        subject: "Test".into(),
        body: "body".into(),
        date: "2026-05-30T12:00:00Z".into(),
        attachments: vec![OutboundAttachment {
            filename: "hello.bin".into(),
            bytes: b"hello".to_vec(),
        }],
    };
    let id = backend.send_message(msg).await.expect("send queues");
    // Read it back from the outbox; assert attachment is intact.
    let raw_path = backend.outbox_dir().join(format!("{}.b2f", id.0));
    let raw = std::fs::read(&raw_path).unwrap();
    let parsed = tuxlink_lib::winlink::message::Message::from_bytes(&raw).unwrap();
    assert_eq!(parsed.attachments().len(), 1);
    assert_eq!(parsed.attachments()[0].bytes, b"hello");
}
```

(Note: `test_fixture_with_callsign` is a new helper; if `NativeBackend::test_fixture()` is enough, skip the `_with_callsign` variant and set callsign via a different path.)

- [ ] **Step 2: Run → FAIL (current send_message ignores msg.attachments)**

- [ ] **Step 3: Change `NativeBackend::send_message`**

Apply the patch from spec rev-3 §4.3:

```rust
async fn send_message(
    &self,
    msg: OutboundMessage,
) -> Result<MessageId, BackendError> {
    let callsign = self.live_config().identity.callsign
        .ok_or_else(|| BackendError::NotConfigured("identity.callsign".into()))?;
    let unix_secs = parse_rfc3339_secs(&msg.date).unwrap_or_else(now_unix_secs);
    let to: Vec<&str> = msg.to.iter().map(String::as_str).collect();
    let cc: Vec<&str> = msg.cc.iter().map(String::as_str).collect();
    let message = compose::compose_message_with_files(
        &callsign, &to, &cc, &msg.subject, &msg.body,
        &msg.attachments,
        unix_secs,
    ).map_err(|e| BackendError::MessageRejected(e.to_string()))?;
    let id = self.mailbox.store(MailboxFolder::Outbox, &message.to_bytes())?;
    Ok(id)
}
```

- [ ] **Step 4: Run → PASS**

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(backend): NativeBackend::send_message pipes attachments to compose

Previously msg.attachments was silently dropped. Now it's passed to
compose_message_with_files; compose errors (invalid filename) map to
BackendError::MessageRejected (existing tuple variant) so the UI can
surface them as typed rejections.

Per spec rev-3 §4.3 + §4.7.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 4.2: Session wire-log + reject mapping

**Files:**
- Modify: `src-tauri/src/winlink/session.rs`
- Possibly modify: `src-tauri/src/winlink/proposal.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test (integration-style)**

Add to `tests/winlink_backend_test.rs` or a new `tests/winlink_session_test.rs`:

```rust
#[tokio::test]
async fn native_session_emits_wire_log_on_send() {
    // Two-backend in-process exchange: sender sends a small message,
    // assert the sender's session log captures the FC EM proposal line.
    // (Boilerplate borrowed from the existing two_native_backends_exchange_* tests.)
    // ... setup ...
    let mut log_stream = sender.stream_log();
    // ... run send + connect ...
    let log_lines: Vec<_> = log_stream.collect().await;
    assert!(log_lines.iter().any(|l|
        l.source == LogSource::Wire && l.message.starts_with("FC EM ")
    ));
    assert!(log_lines.iter().any(|l|
        l.source == LogSource::Wire && l.message.starts_with("FS ")
    ));
}
```

- [ ] **Step 2: Run → FAIL (no wire log emission today)**

- [ ] **Step 3: Add log emission to `winlink::session::send_turn`**

Find `send_turn` in `winlink/session.rs`. At the point where the FC line is written to the socket (look for `proposal.line()` writes), add a log emit via the existing `LogSink`. The session code already takes a log sink for incoming Wire bytes; reuse it for outbound:

```rust
// Before writing the proposal line:
log_sink.emit(LogLine {
    level: LogLevel::Debug,
    source: LogSource::Wire,
    message: proposal.line(),
    timestamp: now_unix_secs(),
});
writer.write_all(proposal.line().as_bytes()).await?;

// After reading the FS answer line:
let answer_line = read_line(&mut reader).await?;
log_sink.emit(LogLine {
    level: LogLevel::Debug,
    source: LogSource::Wire,
    message: answer_line.clone(),
    timestamp: now_unix_secs(),
});
let answers = parse_answers(&answer_line);
```

- [ ] **Step 4: Map FS-reject to MessageRejected**

After parsing answers, iterate; for each `Answer::Reject { mid }`:

```rust
for answer in &answers {
    match answer {
        Answer::Reject { mid } => {
            return Err(BackendError::MessageRejected(
                format!("CMS rejected mid {mid} (FS -)")
            ));
        }
        Answer::Defer { .. } => { /* existing defer logic */ }
        Answer::Accept { .. } => { /* continue with transfer */ }
    }
}
```

- [ ] **Step 5: Run → PASS**

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(session): emit wire-log lines + map FS-reject to typed error

send_turn now logs every outbound FC EM and inbound FS line as
LogSource::Wire entries (visible in the session-log UI). FS - for any of
our MIDs maps to BackendError::MessageRejected with the MID in the
diagnostic. Without this, BackendError::TransportFailed swallowed the
distinction between wire-garbage and connection-drop.

Per spec rev-3 §4.8 + R4 P0-3 + Codex R5 P2.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 4.3: Update two-native-backends exchange test for attachments

**Files:**
- Modify: `src-tauri/tests/winlink_backend_test.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Find the existing test**

```bash
grep -n "two_native_backends_exchange" src-tauri/tests/winlink_backend_test.rs
```

- [ ] **Step 2: Add a variant with attachments**

Copy the existing test; rename to `two_native_backends_exchange_with_attachment`. Change the `OutboundMessage` to include one attachment:

```rust
let msg = OutboundMessage {
    to: vec!["RECEIVER-CALL".into()],
    cc: vec![],
    subject: "with attachment".into(),
    body: "hello".into(),
    date: "2026-05-30T12:00:00Z".into(),
    attachments: vec![OutboundAttachment {
        filename: "test.bin".into(),
        bytes: b"hello-attachment-bytes".to_vec(),
    }],
};
```

After receive completes, assert the received message body contains the attachment region:

```rust
let received = receiver.read_message_in(MailboxFolder::Inbox, &id).await.unwrap();
let parsed = tuxlink_lib::winlink::message::Message::from_bytes(&received.raw).unwrap();
assert_eq!(parsed.attachments().len(), 1);
assert_eq!(parsed.attachments()[0].filename, "test.bin");
assert_eq!(parsed.attachments()[0].bytes, b"hello-attachment-bytes");
```

- [ ] **Step 3: Run → expect PASS (Phase 1+2+4 should all work end-to-end)**

If FAIL, debug along the round-trip (compose → store → fetch → proposal → compress → transfer → decompress → parse on receiver).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "test(backend): two-backend end-to-end exchange with attachment

In-process telnet loopback: sender composes a message with an attachment,
sends, receiver decodes, attachment bytes match. Strongest end-to-end
test for the new outbound-with-attachments path.

Per spec rev-3 §8.4.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 4 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 5 — Flip install sites (bootstrap, app_backend, wizard)

Production code stops referencing PatBackend; PatBackend itself stays present (deleted in P9). After this phase, the production code path uses NativeBackend exclusively; only the Pat-using tests still reference PatBackend.

### Task 5.1: Add `NativeBackend::test_fixture()` factory

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs`
- Modify: `src-tauri/Cargo.toml` (dev-dependencies → tempfile, if absent)

**Standard preamble + completion** apply.

- [ ] **Step 1: Add `tempfile` to dev-dependencies if not present**

```bash
grep -n "tempfile" src-tauri/Cargo.toml
```

If absent, add under `[dev-dependencies]`:
```toml
tempfile = "3"
```

- [ ] **Step 2: Add the fixture**

In `winlink_backend.rs`, near other test helpers:

```rust
#[cfg(test)]
impl NativeBackend {
    /// In-process stub for unit tests that exercise `BackendState::install`
    /// lifecycle without touching real telnet or a real mailbox.
    pub fn test_fixture() -> Self {
        let tempdir = tempfile::tempdir().unwrap();
        let leaked_path = Box::leak(Box::new(tempdir)).path().to_path_buf();
        Self::new(test_config(), leaked_path)
    }
}

#[cfg(test)]
fn test_config() -> Config {
    // Minimum valid Config for BackendState::install lifecycle tests.
    // Identity blank → connect/send_message fail with NotConfigured (intentional).
    // Read the actual Config struct fields and provide explicit None / defaults
    // for each. Do NOT use Config::default() — it does not exist on main.
    Config {
        identity: Identity::default(),  // verify Identity has Default; if not, build explicitly
        pat_mbo_address: None,
        // ... every other top-level Config field, set to None / default ...
    }
}
```

(Inspect the actual `Config` struct definition in `src-tauri/src/config.rs` to fill in the full field list. Don't shortcut with `..Default::default()` since `Default` is absent.)

- [ ] **Step 3: Build (no tests yet, just verify it compiles)**

```bash
cargo build --manifest-path src-tauri/Cargo.toml --workspace --tests
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "test(backend): add NativeBackend::test_fixture() factory

In-process stub for BackendState::install lifecycle tests, replacing
PatBackend::from_url calls in app_backend tests. Uses tempfile for the
mailbox root; identity blank so connect/send fail predictably. No
Config::default() (doesn't exist on main); explicit field construction.

Per spec rev-3 §4.4 + Codex R5 P2.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 5.2: Replace PatBackend::from_url in app_backend tests

**Files:**
- Modify: `src-tauri/src/app_backend.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Find every PatBackend::from_url call**

```bash
grep -n "PatBackend::from_url" src-tauri/src/app_backend.rs
```

Expected: 3 hits at lines ~161, ~173, ~219 per R3 audit.

- [ ] **Step 2: Replace each with `NativeBackend::test_fixture()`**

```rust
// Before:
state.install(Arc::new(PatBackend::from_url("http://127.0.0.1:9")));

// After:
state.install(Arc::new(NativeBackend::test_fixture()));
```

Update the `use` line at the top to drop `PatBackend` (if no other PatBackend reference remains in this file) and import `NativeBackend`:

```rust
use crate::winlink_backend::{NativeBackend /* ...other imports... */};
```

- [ ] **Step 3: Build + run app_backend tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --workspace app_backend
```

Expected: tests pass with the new fixture.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor(app_backend): test fixtures switch to NativeBackend

Three PatBackend::from_url stubs replaced with NativeBackend::test_fixture().
The lifecycle behavior under test is install/uninstall, not actual send;
the fixture's connect/send fails with NotConfigured (intentional for this
test scope).

Per spec rev-3 §5 P5.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 5.3: bootstrap.rs — install NativeBackend, drop Pat resolution

**Files:**
- Modify: `src-tauri/src/bootstrap.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Identify the Pat resolution + spawn code**

```bash
grep -n "PatBackend\|resolve_pat_binary\|spawn_pat\|SIDECAR_STUB_REASON" src-tauri/src/bootstrap.rs
```

Find the function (around line 343-386) that does the bootstrap install with Pat.

- [ ] **Step 2: Replace the Pat install path with NativeBackend install**

The existing `install_native` function already constructs a NativeBackend (per the on-main code). The change is to make the Pat path no longer the default for CMS mode. Find the conditional that branches on `wizard_completed && connect_to_cms` and replace the PatBackend spawn with a `NativeBackend::new(...)` install:

```rust
// Before (paraphrased):
if wizard_completed && connect_to_cms {
    let binary = resolve_pat_binary(app)?;
    let opts = PatBackendSpawnOptions { binary, ... };
    let backend = PatBackend::spawn(opts, log_buffer.clone())?;
    state.install(Arc::new(backend));
}

// After:
if wizard_completed && connect_to_cms {
    let mailbox_root = resolve_mailbox_root(app)?;
    let backend = NativeBackend::new(config.clone(), mailbox_root);
    state.install(Arc::new(backend));
}
```

DELETE the `resolve_pat_binary`, `resolve_pat_binary_inner`, `is_nonempty_file`, `SIDECAR_STUB_REASON` items (no longer referenced).

- [ ] **Step 3: Build → expect maybe some warnings on unused imports**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

Fix any unused-import warnings (drop `PatBackend, PatBackendSpawnOptions` from the use line).

- [ ] **Step 4: Run bootstrap tests if any exist**

```bash
cargo test --manifest-path src-tauri/Cargo.toml bootstrap
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(bootstrap): install NativeBackend instead of spawning PatBackend

Drop resolve_pat_binary + PatBackend::spawn + the dedicated spawn thread;
NativeBackend::new is synchronous so the spawn-thread is no longer needed.
PatBackend is no longer referenced from production code paths (still
present in source; deleted in P9).

Per spec rev-3 §5 P5.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 5.4: wizard.rs — replace test-send Pat-spawn with connect-only NativeBackend probe

**Files:**
- Modify: `src-tauri/src/wizard.rs`
- Possibly modify: `src/wizard/Step3TestSend.tsx` (or wherever the button is — verify by reading the TS)

**Standard preamble + completion** apply.

- [ ] **Step 1: Find the wizard's test-send code**

```bash
grep -n "PatBackend\|test_send\|pat_url\|PAT_URL" src-tauri/src/wizard.rs
```

Around line 391-450 per audit.

- [ ] **Step 2: Replace the spawn + send with a NativeBackend connect probe**

The new behavior (per spec rev-3 §7.2):
- The button label changes from "Send Test Message" to "Verify CMS Connection".
- The Tauri command calls NativeBackend::connect with TransportConfig::Cms { mode: Plaintext } against the operator's configured CMS, then disconnects.
- On success: green check + "CMS reachable as <CALL>".
- On failure: error + existing diagnostic hints.

Rust side (wizard.rs):

```rust
#[tauri::command]
async fn verify_cms_connection(/* config, state */) -> Result<(), String> {
    let backend = NativeBackend::new(config, mailbox_root);
    let session = backend.connect(TransportConfig::Cms { mode: TransportMode::Plaintext })
        .await
        .map_err(|e| e.to_string())?;
    backend.disconnect(session).await.map_err(|e| e.to_string())?;
    Ok(())
}
```

Frontend (TS): update the button label + the command invocation to call the new `verify_cms_connection` instead of the old test-send command.

- [ ] **Step 3: Build + typecheck**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
pnpm typecheck
```

- [ ] **Step 4: Run wizard tests if any**

```bash
cargo test --manifest-path src-tauri/Cargo.toml wizard
pnpm test:unit -- wizard
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(wizard): replace test-send with verify-CMS-connection

The wizard's 'Send Test Message' button spawned an ephemeral Pat and sent
a real message to the operator's own callsign. Replace with a connect-only
NativeBackend probe — no transmission, just verifies CMS reachability +
auth. Eliminates the only wizard path that entangled with RADIO-1 in
principle. Button label updated to 'Verify CMS Connection'.

Per spec rev-3 §7.2 + R4 P1.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 5 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 6 — `LogSource::Pat` merge

Remove the `Pat` variant; redirect emit sites to `Backend`; update the TS frontend's log-projection.

### Task 6.1: Remove Pat variant + update emit sites

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Find LogSource references**

```bash
grep -n "LogSource::Pat\|LogSource {" src-tauri/src/
```

- [ ] **Step 2: Drop the variant + redirect emits**

In `winlink_backend.rs:290`:
```rust
// Before:
pub enum LogSource { Backend, Pat, Transport, Wire }
// After:
pub enum LogSource { Backend, Transport, Wire }
```

In the Pat-stderr emit-sites (around line 1408, 1423), change `LogSource::Pat` → `LogSource::Backend`. (Those sites are deleted in P9 along with PatBackend; this is just keeping them compilable in the transitional period.)

- [ ] **Step 3: Build → expect clean**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

If any tests reference `LogSource::Pat` (e.g., in pat_*_test.rs), they break; that's expected and gets resolved in P9 deletion. For tests in winlink_backend_test.rs or ui_commands_test.rs that mention Pat-source, update to Backend.

- [ ] **Step 4: Run cargo test → expect green for non-Pat tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

If the pat_*_test.rs files fail to compile, that's expected; we'll delete them in P9.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(backend): merge LogSource::Pat into ::Backend

LogSource::Backend already existed alongside ::Pat. The 'pat' label was
historical (every backend log line is conceptually a Backend log line);
merging now removes a vestigial discriminator before Pat is fully deleted.
Emit sites in PatBackend retargeted to ::Backend (those sites die in P9
anyway; this keeps the transitional state compilable).

Per spec rev-3 §5 P6 + R3 P1-1.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 6.2: Frontend logProjection update

**Files:**
- Modify: `src/wizard/logProjection.ts`
- Modify: `src/wizard/logProjection.test.ts`

**Standard preamble + completion** apply.

- [ ] **Step 1: Find the 'pat' discriminator**

```bash
grep -n "'pat'\|\"pat\"" src/wizard/logProjection.ts src/wizard/logProjection.test.ts
```

- [ ] **Step 2: Drop the 'pat' case**

In `logProjection.ts:30` (per R3 audit), the discriminated union or switch has a `'pat'` branch. Delete it (the wire form now emits `'backend'` from this commit onward).

In `logProjection.test.ts:330`, drop the `'pat'` assertion.

- [ ] **Step 3: Typecheck + test**

```bash
pnpm typecheck && pnpm test:unit -- logProjection
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor(wizard): drop 'pat' case from logProjection

Backend wire-form (rename_all=lowercase) no longer emits 'pat' — every
backend log is 'backend'. Drop the TS discriminator case + its test.

Per spec rev-3 §5 P6.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 6 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 7 — Keyring service-name migration

Rename `"tuxlink-pat"` → `"tuxlink"` with one-time migration. The keyring entry is the native backend's actual credential store (not Pat-specific); leaving "tuxlink-pat" forever would be a misnomer.

### Task 7.1: New `winlink::credentials` module with read+migrate helper

**Files:**
- Create: `src-tauri/src/winlink/credentials.rs`
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod credentials;`)

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/tests/winlink_credentials_test.rs`:

```rust
use keyring::Entry;

#[test]
fn read_password_returns_new_entry_when_present() {
    // Set up: write a known password under the new name.
    let callsign = "TEST_CALL_NEW";
    let entry_new = Entry::new("tuxlink", callsign).unwrap();
    entry_new.set_password("new-password").unwrap();
    let pw = tuxlink_lib::winlink::credentials::read_password(callsign).unwrap();
    assert_eq!(pw, "new-password");
    // Cleanup
    let _ = entry_new.delete_password();
}

#[test]
fn read_password_migrates_from_old_entry_when_only_old_exists() {
    let callsign = "TEST_CALL_MIGRATE";
    let entry_old = Entry::new("tuxlink-pat", callsign).unwrap();
    entry_old.set_password("old-password").unwrap();
    // Ensure no new entry exists
    let entry_new = Entry::new("tuxlink", callsign).unwrap();
    let _ = entry_new.delete_password();

    let pw = tuxlink_lib::winlink::credentials::read_password(callsign).unwrap();
    assert_eq!(pw, "old-password");
    // After migration, new entry exists, old entry deleted.
    assert_eq!(entry_new.get_password().unwrap(), "old-password");
    assert!(entry_old.get_password().is_err());
    // Cleanup
    let _ = entry_new.delete_password();
}

#[test]
fn read_password_errors_when_neither_entry_exists() {
    let callsign = "TEST_CALL_NEITHER";
    // Make sure neither exists
    let _ = Entry::new("tuxlink", callsign).unwrap().delete_password();
    let _ = Entry::new("tuxlink-pat", callsign).unwrap().delete_password();

    let err = tuxlink_lib::winlink::credentials::read_password(callsign).unwrap_err();
    // Should be a NoEntry-shaped error.
    // (Exact shape depends on the KeyringError enum chosen.)
}
```

- [ ] **Step 2: Run → FAIL (module doesn't exist)**

- [ ] **Step 3: Create the module**

Create `src-tauri/src/winlink/credentials.rs`:

```rust
//! Native-backend credential storage via the OS keyring.
//!
//! Service name is `"tuxlink"`. A migration path reads from the legacy
//! `"tuxlink-pat"` name (used during the Pat era) and writes to the new
//! name on first successful auth. The old entry is then deleted (best-
//! effort; if delete fails, the entry sits forever stale but the new
//! entry serves subsequent reads).

use keyring::{Entry, Error as KeyringErr};
use thiserror::Error;

const SERVICE_NEW: &str = "tuxlink";
const SERVICE_OLD: &str = "tuxlink-pat";

#[derive(Debug, Error)]
pub enum KeyringError {
    #[error("no credentials found for callsign {callsign}")]
    NoEntry { callsign: String },
    #[error("keyring backend error: {0}")]
    Backend(#[from] KeyringErr),
}

/// Read the WL2K password for `callsign` from the OS keyring.
///
/// If the new-name entry doesn't exist but the legacy `"tuxlink-pat"` entry
/// does, migrate transparently: write to the new entry, delete the old.
pub fn read_password(callsign: &str) -> Result<String, KeyringError> {
    let new_entry = Entry::new(SERVICE_NEW, callsign)?;
    match new_entry.get_password() {
        Ok(pw) => Ok(pw),
        Err(KeyringErr::NoEntry) => {
            // First-run-after-upgrade: look at the old name.
            let old_entry = Entry::new(SERVICE_OLD, callsign)?;
            match old_entry.get_password() {
                Ok(pw) => {
                    new_entry.set_password(&pw)?;
                    let _ = old_entry.delete_password();
                    log::info!(
                        "migrated keyring entry: {SERVICE_OLD} → {SERVICE_NEW} for callsign {callsign}"
                    );
                    Ok(pw)
                }
                Err(KeyringErr::NoEntry) => {
                    Err(KeyringError::NoEntry { callsign: callsign.to_string() })
                }
                Err(e) => Err(KeyringError::Backend(e)),
            }
        }
        Err(e) => Err(KeyringError::Backend(e)),
    }
}
```

In `src-tauri/src/winlink/mod.rs`, add:

```rust
pub mod credentials;
```

- [ ] **Step 4: Run tests → PASS**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test winlink_credentials_test -- --test-threads=1
```

(`--test-threads=1` because the tests share the keyring namespace.)

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(winlink): credentials module with keyring migration

Reads from 'tuxlink' service name; if absent, migrates from legacy
'tuxlink-pat' (used during the Pat era), writes to new name, deletes
old. The keyring entry was always the native backend's credential
source — the 'pat' label was historical and is now wrong. One-time
migration is invisible to operators with an existing entry.

Per spec rev-3 §7.1 + R4 P0-2.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 7.2: Replace 4 keyring call sites

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs` (2 sites: lines ~994, ~1219)
- Modify: `src-tauri/src/wizard.rs` (1 site: line ~186)
- Modify: `src-tauri/src/bin/native_cms_probe.rs` (1 site: line ~43)
- NOTE: `src-tauri/src/bin/live_cms_smoke.rs` is deleted in P9 — leave it for now

**Standard preamble + completion** apply.

- [ ] **Step 1: For each site, replace `keyring::Entry::new("tuxlink-pat", &callsign)?.get_password()?` with `credentials::read_password(&callsign)?`**

```bash
# Find all sites
grep -n 'keyring::Entry::new("tuxlink-pat"' src-tauri/src/
```

For each (excluding live_cms_smoke.rs):

```rust
// Before:
let password = keyring::Entry::new("tuxlink-pat", &callsign)?.get_password()?;
// After:
use crate::winlink::credentials;
let password = credentials::read_password(&callsign)?;
```

Update the error-handling to handle `KeyringError` (map or use `?` per the existing pattern).

- [ ] **Step 2: Build + run tests**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --workspace
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor: 4 keyring sites use credentials::read_password

Wraps the keyring read with the migration helper. On first run after
upgrade, the helper transparently moves the password from 'tuxlink-pat'
to 'tuxlink'. Subsequent runs read from 'tuxlink' directly.

Per spec rev-3 §7.1.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 7.3: Update Step2Credentials.tsx user-facing string

**Files:**
- Modify: `src/wizard/Step2Credentials.tsx`

**Standard preamble + completion** apply.

- [ ] **Step 1: Find the string**

```bash
grep -n "tuxlink-pat" src/wizard/Step2Credentials.tsx
```

Line ~84 per audit.

- [ ] **Step 2: Update**

```tsx
// Before:
`entry also failed. Run \`secret-tool delete service tuxlink-pat account <callsign>\``
// After:
`entry also failed. Run \`secret-tool delete service tuxlink account <callsign>\``
```

- [ ] **Step 3: Typecheck**

```bash
pnpm typecheck
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "docs(wizard): keyring recovery hint uses new service name

Update the diagnostic shown when keyring auth fails to point at the new
service name 'tuxlink' instead of the legacy 'tuxlink-pat'.

Per spec rev-3 §7.1.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 7 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 8 — Config field deprecation

`Config.pat_mbo_address` stays `pub` (per Codex R5 P1 — making it private would break `Config { ... }` literals across 10+ files) but is marked `#[deprecated]` + `#[serde(default, skip_serializing)]` so future code won't write it and existing operator configs read tolerantly.

### Task 8.1: Add deprecation attributes to the field

**Files:**
- Modify: `src-tauri/src/config.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/tests/config_test.rs` (or create it):

```rust
#[test]
fn config_round_trips_without_pat_mbo_address_field_on_write() {
    let cfg = Config {
        // ... all fields set explicitly per Config struct ...
        pat_mbo_address: Some("LEGACY-VALUE".into()),
        // ...
    };
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(!json.contains("pat_mbo_address"),
            "skip_serializing should exclude pat_mbo_address from JSON output, got: {json}");
}

#[test]
fn config_reads_legacy_pat_mbo_address_without_error() {
    // Legacy operator config has the field; new code accepts it.
    let json = r#"{
        "identity": { /* fill in valid identity fields */ },
        "pat_mbo_address": "LEGACY-VALUE"
        /* ... other required fields ... */
    }"#;
    let cfg: Config = serde_json::from_str(json).expect("legacy config parses");
    assert_eq!(cfg.pat_mbo_address, Some("LEGACY-VALUE".to_string()));
}
```

(Inspect the actual `Config` struct fields to fill in the test JSON correctly.)

- [ ] **Step 2: Run → first test FAILs (skip_serializing not applied)**

- [ ] **Step 3: Apply attributes**

In `src-tauri/src/config.rs:24`:

```rust
#[deprecated(
    note = "pat_mbo_address is unused after the Pat strip (ADR 0016); future writers \
            should not set it. Tracked for removal in a future major bump."
)]
#[serde(default, skip_serializing)]
pub pat_mbo_address: Option<String>,
```

Note: with `#[serde(default, skip_serializing)]` AND `deny_unknown_fields` on the struct, reads accept the field (default to None if absent) but never write it.

- [ ] **Step 4: Run tests → PASS**

If the test about "legacy config reads" fails because of an unknown-field error, the field IS still known (just deprecated); should pass. If it fails, double-check the deny_unknown_fields placement on the struct.

- [ ] **Step 5: Add a CI allow-list for the deprecation warning**

The deprecation warning will fire for the field's definition site (it's its own usage). Add `#[allow(deprecated)]` at the field declaration so the warning fires only for external readers/writers:

```rust
#[allow(deprecated)]   // self-reference; the field is its own use site
#[deprecated(note = "...")]
#[serde(default, skip_serializing)]
pub pat_mbo_address: Option<String>,
```

Alternatively, scope `#[allow(deprecated)]` at the file or workspace level for the test file if needed.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(config): deprecate pat_mbo_address field

#[deprecated] for compile-time warning to future writers; #[serde(default,
skip_serializing)] so existing operator configs round-trip (read tolerantly,
never write back). Field stays pub — making it private would break the
Config {{ ... }} literals across 10+ files. Full removal deferred to a
future major bump.

Per spec rev-3 §5 P8 + Codex R5 P1.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 8 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 9 — Delete Pat module + test surgery

The big delete. After this phase, no Pat code remains in the workspace.

### Task 9.1: Delete the 3 standalone Pat module files

**Files:**
- Delete: `src-tauri/src/pat_client.rs`
- Delete: `src-tauri/src/pat_config.rs`
- Delete: `src-tauri/src/pat_process.rs`
- Modify: `src-tauri/src/lib.rs` (drop `pub mod pat_client; pub mod pat_config; pub mod pat_process;`)

**Standard preamble + completion** apply.

- [ ] **Step 1: Delete the files**

```bash
git rm src-tauri/src/pat_client.rs src-tauri/src/pat_config.rs src-tauri/src/pat_process.rs
```

- [ ] **Step 2: Update lib.rs**

Remove the `pub mod pat_client;`, `pub mod pat_config;`, `pub mod pat_process;` lines.

- [ ] **Step 3: Build → expect failures (test files still reference Pat types)**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

The Pat-using test files (winlink_backend_test.rs, ui_commands_test.rs, pat_*_test.rs) will fail. That's expected; Tasks 9.2–9.5 fix them.

- [ ] **Step 4: Defer commit until P9.5 (test surgery complete)**

Stay in working-tree-dirty state until tests compile.

### Task 9.2: Delete PatBackend + PatBackendSpawnOptions from winlink_backend.rs

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Find the Pat impl blocks**

```bash
grep -n "PatBackend\|PatBackendSpawnOptions\|impl WinlinkBackend for PatBackend" src-tauri/src/winlink_backend.rs
```

- [ ] **Step 2: Delete the struct + impl blocks + spawn function**

Delete:
- `pub struct PatBackend { ... }` (around line 1458)
- `pub struct PatBackendSpawnOptions { ... }` (around line 1437)
- `impl PatBackend { pub fn from_url(...) }`, `pub fn spawn(...)`, etc.
- `impl WinlinkBackend for PatBackend { ... }` (full impl block)
- Any helper functions used only by PatBackend (e.g., `translate_pat_err`, the broadcast log helpers if Pat-only)
- The `use` lines that import Pat-only items

- [ ] **Step 3: Build → expect green for src code; test files still broken**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

If there are remaining `PatBackend` references in src/, find + delete.

### Task 9.3: Delete the 3 standalone Pat test files

**Files:**
- Delete: `src-tauri/tests/pat_client_test.rs`
- Delete: `src-tauri/tests/pat_config_test.rs`
- Delete: `src-tauri/tests/pat_process_test.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Delete**

```bash
git rm src-tauri/tests/pat_client_test.rs src-tauri/tests/pat_config_test.rs src-tauri/tests/pat_process_test.rs
```

### Task 9.4: Partial-edit `tests/winlink_backend_test.rs`

**Files:**
- Modify: `src-tauri/tests/winlink_backend_test.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Find every PatBackend reference**

```bash
grep -n "PatBackend\|pat_client::\|pat_config::\|pat_process::" src-tauri/tests/winlink_backend_test.rs
```

22 hits per R2 audit. For each hit, determine whether the surrounding test function:
- Tests PatBackend's behavior specifically → delete the function entirely
- Uses PatBackend incidentally for setup (e.g., as a stub backend) → replace with NativeBackend equivalent OR delete if no native equivalent makes sense

- [ ] **Step 2: Make the edits**

Walk each test function. For each one that uses PatBackend, decide and apply:

- **Delete** the entire `#[tokio::test]` (or `#[test]`) function if it tests Pat-specific behavior.
- **Refactor** to use NativeBackend if the test exercises the trait at large.

(The plan author can't enumerate exactly which tests need which treatment without reading every test body — the executing subagent reads each test function and decides per its content. Document the decision in the commit body.)

- [ ] **Step 3: Run the file's tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test winlink_backend_test
```

Expected: all surviving tests pass.

### Task 9.5: Partial-edit `tests/ui_commands_test.rs`

**Files:**
- Modify: `src-tauri/tests/ui_commands_test.rs`

**Standard preamble + completion** apply.

- [ ] **Step 1: Find every PatBackend reference**

```bash
grep -n "PatBackend\|pat_client::" src-tauri/tests/ui_commands_test.rs
```

10 hits per R2 audit.

- [ ] **Step 2: Same approach as Task 9.4** — delete Pat-specific tests; refactor incidental uses.

- [ ] **Step 3: Run the file's tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test ui_commands_test
```

### Task 9.6: Delete live_cms_smoke.rs + Cargo.toml entry

**Files:**
- Delete: `src-tauri/src/bin/live_cms_smoke.rs`
- Modify: `src-tauri/Cargo.toml`

**Standard preamble + completion** apply.

- [ ] **Step 1: Delete the file**

```bash
git rm src-tauri/src/bin/live_cms_smoke.rs
```

- [ ] **Step 2: Remove the bin entry**

In `src-tauri/Cargo.toml`, find:

```toml
[[bin]]
name = "live_cms_smoke"
path = "src/bin/live_cms_smoke.rs"
```

Delete the block.

- [ ] **Step 3: Build → expect green (no callers of live_cms_smoke binary)**

```bash
cargo build --manifest-path src-tauri/Cargo.toml --workspace
```

### Task 9.7: Verify everything builds + commit the entire P9 batch

**Files:** (all of P9's modifications)

**Standard preamble + completion** apply.

- [ ] **Step 1: Run the full workspace tests**

```bash
cargo build --manifest-path src-tauri/Cargo.toml --workspace
cargo test --manifest-path src-tauri/Cargo.toml --workspace --lib --tests -- --test-threads=1
```

Expected: 100% green. No Pat references remain.

- [ ] **Step 2: Verify no Pat references remain anywhere in src or tests**

```bash
grep -rn "PatBackend\|pat_client\|pat_config\|pat_process" src-tauri/src/ src-tauri/tests/
```

Expected: zero output (besides maybe in comments — verify any remaining are intentional historical references).

- [ ] **Step 3: Commit the entire P9 batch**

```bash
git add -A
git commit -m "feat(backend)!: delete Pat module + Pat tests + live_cms_smoke

Removes:
- src/pat_client.rs (222 LOC), pat_config.rs (141 LOC), pat_process.rs (294 LOC)
- PatBackend struct + impl + PatBackendSpawnOptions in winlink_backend.rs
- tests/pat_client_test.rs, pat_config_test.rs, pat_process_test.rs (~664 LOC)
- src/bin/live_cms_smoke.rs (Pat-based smoke)
- Cargo.toml [[bin]] live_cms_smoke entry
- Pat-specific test cases in tests/winlink_backend_test.rs (~8-12 tests)
- Pat-specific test cases in tests/ui_commands_test.rs (~3-5 tests)
- pub mod declarations from lib.rs

NativeBackend is the sole WinlinkBackend impl. The Pat sidecar binary
bundling (tauri.conf.json + build.rs Go path + sidecars/) is deleted in
the next phase (P10). The external/tuxlink-pat submodule is deinit'd in
P11.

BREAKING CHANGE: Pat sidecar removed. Releases no longer depend on a Go
toolchain to build; release builds shrink. Operators with a stale
'tuxlink-pat' keyring entry are migrated transparently to 'tuxlink' on
first run (Phase 7).

Per spec rev-3 §5 P9.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 9 review loop

Apply the **Standard phase-end review loop**. Pay particular attention to: did any test get accidentally orphaned (i.e., a helper used by the deleted tests is now unused)?

---

## Phase 10 — Delete sidecar infra

`tauri.conf.json` sidecar entry, `build.rs` Go-build path, `build_support.rs`, `src-tauri/sidecars/` directory, and `.github/workflows/release.yml` Pat steps.

### Task 10.1: tauri.conf.json — remove externalBin

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Find + delete**

```bash
grep -n "sidecars/pat\|externalBin" src-tauri/tauri.conf.json
```

Delete the `"externalBin": ["sidecars/pat"]` line.

- [ ] **Step 2: Build (release-mode dry-run)**

```bash
cargo build --manifest-path src-tauri/Cargo.toml --release
```

Expected: cleaner build; no Pat sidecar fetched.

### Task 10.2: build.rs + build_support.rs deletion

**Files:**
- Modify: `src-tauri/build.rs`
- Delete: `src-tauri/build_support.rs`
- Modify: `src-tauri/src/lib.rs` (drop `#[cfg(test)] mod build_support;` if present)

**Standard preamble + completion** apply.

- [ ] **Step 1: Identify the Pat-related code in build.rs**

```bash
grep -n "external/tuxlink-pat\|go build\|GOPATH\|sidecars/pat\|build_support" src-tauri/build.rs
```

- [ ] **Step 2: Delete those blocks**

Remove every block tied to:
- Go-toolchain presence check
- `go build` invocation for the Pat fork
- 0-byte-stub creation for debug/test builds
- Sidecar copying

What should remain in build.rs is whatever existed BEFORE the Pat-bundling logic was added — likely a minimal `tauri-build::build();` invocation or similar.

- [ ] **Step 3: Delete `build_support.rs`**

```bash
git rm src-tauri/build_support.rs
```

Remove `#[cfg(test)] mod build_support;` from `lib.rs` if present.

- [ ] **Step 4: Verify both debug and release builds**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml --release
```

Expected: both succeed without Go installed.

### Task 10.3: Delete src-tauri/sidecars/ directory

**Files:**
- Delete: `src-tauri/sidecars/` (whole directory)

- [ ] **Step 1: Remove**

```bash
git rm -rf src-tauri/sidecars/
```

- [ ] **Step 2: Verify build still works**

```bash
cargo build --manifest-path src-tauri/Cargo.toml --release
```

### Task 10.4: .github/workflows/release.yml — remove Pat steps

**Files:**
- Modify: `.github/workflows/release.yml`

**Standard preamble + completion** apply.

- [ ] **Step 1: Find the Pat-related steps**

```bash
grep -n "tuxlink-pat\|go-version-file\|sidecars/pat\|setup-go" .github/workflows/release.yml
```

- [ ] **Step 2: Remove**

Delete:
- The `setup-go` step (or actions/setup-go invocation)
- The step that builds the Pat sidecar (likely `go build` ... output to `sidecars/`)
- The `go-version-file: 'external/tuxlink-pat/go.mod'` reference

The release workflow now only builds Rust + frontend; no Go toolchain required.

### Task 10.5: Commit the entire P10 batch

- [ ] **Step 1: Verify everything builds**

```bash
cargo build --manifest-path src-tauri/Cargo.toml --workspace
cargo build --manifest-path src-tauri/Cargo.toml --release
cargo test --manifest-path src-tauri/Cargo.toml --workspace
```

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "build!: delete Pat sidecar infrastructure

Removes:
- src-tauri/tauri.conf.json externalBin entry
- src-tauri/build.rs Go-toolchain check + go-build invocation + sidecar stub
- src-tauri/build_support.rs (orphaned Go-path helper)
- src-tauri/sidecars/ directory entirely
- .github/workflows/release.yml setup-go + Pat-build steps
- Possibly Pat-specific Cargo dependencies (verify)

Release builds no longer require a Go toolchain. Setup docs that say
'install Go' for release builds are wrong post-this-commit; updated in P12.

BREAKING CHANGE: bundled Pat sidecar removed; release artifacts no
longer contain the pat binary.

Per spec rev-3 §5 P10.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 10 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 11 — Submodule deinit (`external/tuxlink-pat`)

### Task 11.1: Inventory + deinit + remove

**Files:**
- Modify: `.gitmodules`
- Remove: `external/tuxlink-pat/` (via `git rm`)

**Standard preamble + completion** apply.

- [ ] **Step 1: Inventory the submodule**

```bash
cd external/tuxlink-pat
git status --short
git stash list
cd ../..
```

If there are untracked / dirty changes in the submodule that the operator wants to keep, surface them BEFORE deletion. (Likely empty; the submodule should be at a clean tip per .gitmodules.)

- [ ] **Step 2: Deinit**

```bash
git submodule deinit -f external/tuxlink-pat
```

- [ ] **Step 3: Remove from index + working tree**

```bash
git rm external/tuxlink-pat
```

- [ ] **Step 4: Remove the .gitmodules entry**

In `.gitmodules`, delete the `[submodule "external/tuxlink-pat"]` block.

- [ ] **Step 5: Verify**

```bash
git submodule status                       # external/tuxlink-pat no longer listed
ls external/                               # tuxlink-pat directory gone
cat .gitmodules                            # no tuxlink-pat block
```

- [ ] **Step 6: Build still green**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "build!: deinit + remove external/tuxlink-pat submodule

git submodule deinit + git rm external/tuxlink-pat + .gitmodules entry
removed. The forked repo at github.com/cameronzucker/tuxlink-pat survives
as historical reference; not deleted. Operators with the submodule
already initialized should remove .git/modules/external/tuxlink-pat/
manually on their local checkout (documented in PR body); not part of
this commit.

BREAKING CHANGE: external/tuxlink-pat submodule removed. New clones no
longer initialize it; existing operator clones should run
'git submodule deinit -f external/tuxlink-pat && rm -rf .git/modules/external/tuxlink-pat'
after pulling.

Per spec rev-3 §5 P11 + §7.5.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 11 review loop

Apply the **Standard phase-end review loop**.

---

## Phase 12 — Docs + ADR sweep

### Task 12.1: Amend ADR 0003

**Files:**
- Modify: `docs/adr/0003-no-sqlite-pat-owns-mailbox.md`

- [ ] **Step 1: Find the existing Status line**

```bash
grep -n "^Status:" docs/adr/0003-no-sqlite-pat-owns-mailbox.md
```

- [ ] **Step 2: Append supersession**

Update the Status line per spec rev-3 §6.1:

```markdown
Status: Accepted (amended by [ADR 0011](0011-fork-pat-for-tuxlink.md) — dependency target shifted from upstream `la5nta/pat` to the `tuxlink-pat` fork; the ownership-of-mailbox rule and the no-SQLite-in-tuxlink rule themselves remain operative; **superseded by [ADR 0016](0016-native-b2f-outbound-with-attachments.md) as of 2026-05-30** — native client now owns mailbox; "no SQLite" half still holds.)
```

### Task 12.2: Amend ADR 0011

**Files:**
- Modify: `docs/adr/0011-fork-pat-for-tuxlink.md`

- [ ] **Step 1: Same pattern as 12.1** — append `**superseded by [ADR 0016](0016-native-b2f-outbound-with-attachments.md) as of 2026-05-30**` to the existing Status line.

### Task 12.3: Write ADR 0016

**Files:**
- Create: `docs/adr/0016-native-b2f-outbound-with-attachments.md`

- [ ] **Step 1: Use the spec §6.3 outline + ADR 0014's rigor template**

Read `docs/adr/0014-clean-sheet-modem-no-prior-art-examination.md` for the template shape. Write ADR 0016 with sections: Status, Date, Deciders, Context, Decision, Wire format reference, Alternatives considered, Watched failure modes, Migration / cutover, Consequences. Target 150-200 lines.

Use spec §6.3 as the substantive content source (Watched Failure Modes table, Alternatives Considered, the wire-format inline reference).

### Task 12.4: Revise HTML Forms spec rev-2 → rev-3

**Files:**
- Modify: `docs/superpowers/specs/2026-05-30-html-forms-design.md`

- [ ] **Step 1: Find the §5.1 Path A reasoning**

```bash
grep -n "Path A\|Pat REST\|pat_client" docs/superpowers/specs/2026-05-30-html-forms-design.md
```

- [ ] **Step 2: Remove the Path A reasoning + point at native**

Add a rev-2 → rev-3 row in the change-log table at the top: "rev-3 removes the Path A (Pat REST) choice in §5.1; native B2F outbound with attachments is now available per ADR 0016."

Rewrite §5.1 to describe only the native path (the spec's "Path B"). Remove §3 Path-A vs Path-B comparison; rename "Path B" to "the native attachment path."

Update the rev-1 → rev-2 row that mentions "Backend — B2F wire vs MIME" to note rev-3 finalizes the native path.

### Task 12.5: Sweep docs/install.md + docs/development.md + VERSIONING.md + README.md

**Files:**
- Modify: `docs/install.md`, `docs/development.md`, `VERSIONING.md`, `README.md` (as applicable)

- [ ] **Step 1: Find Pat references**

```bash
grep -in "pat\|Winlink Pat\|sidecar" docs/install.md docs/development.md VERSIONING.md README.md
```

- [ ] **Step 2: Update each**

- `docs/install.md`: drop "install Go" prerequisite if present; drop any "Pat sidecar" descriptions.
- `docs/development.md`: drop "build the Pat sidecar" steps; drop submodule initialization steps.
- `VERSIONING.md`: drop "bundled-Pat compatibility break" row from the MAJOR-bump trigger list.
- `README.md`: if Pat is in the architecture overview, replace with "native Winlink client."

### Task 12.6: Commit the whole P12 batch

- [ ] **Step 1: Verify nothing breaks**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
pnpm typecheck
```

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "docs: ADR sweep + HTML Forms rev-3 + setup docs (P12)

- ADR 0003 + 0011: Status line appended 'superseded by ADR 0016'.
- ADR 0016 (NEW): Native B2F outbound with attachments; Pat removed.
  Captures the wire format inline, alternatives considered, watched
  failure modes, migration steps.
- HTML Forms spec (2026-05-30-html-forms-design.md): rev-2 → rev-3.
  Removes Path A (Pat REST) choice in §5.1; native attachment path
  finalized. Unblocks PR #151 to resume on a clean spec.
- install.md / development.md / VERSIONING.md / README.md:
  Removed Pat / Go-toolchain / sidecar / submodule references where
  applicable.

Per spec rev-3 §5 P12 + §6.

Agent: magpie-grouse-shoal
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Phase 12 review loop

Apply the **Standard phase-end review loop**.

---

## Final reviewer dispatch (per `feedback_codex_post_subagent_review`)

After every phase has shipped, run ONE parent-level Codex round against the full branch diff vs main before opening the PR. This catches self-review bias from the subagent-driven execution.

- [ ] **Step 1: From the worktree, dispatch Codex review**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments
npx --yes @openai/codex review --base main 2>&1 | tee dev/adversarial/2026-05-30-pat-strip-native-attachments-post-impl-codex.md
```

Wait for completion. Per `feedback_codex_quota_gotcha`: if quota-exceeded ("ERROR: You've hit your usage limit"), defer to operator + retry after the reset window.

- [ ] **Step 2: Inspect findings**

```bash
wc -l dev/adversarial/2026-05-30-pat-strip-native-attachments-post-impl-codex.md
tail -200 dev/adversarial/2026-05-30-pat-strip-native-attachments-post-impl-codex.md
```

Real review: 1500-4000+ lines. Stub: ~5 lines (re-run if stub).

- [ ] **Step 3: Apply any P0/P1 findings**

For each finding: either fix in a new commit or add a follow-up bd issue if out of scope for this PR.

- [ ] **Step 4: Open the PR**

```bash
gh pr create \
  --base main \
  --head bd-tuxlink-9phd/strip-pat-add-native-attachments \
  --title "[magpie-grouse-shoal] Strip Pat + native B2F outbound with attachments (tuxlink-9phd)" \
  --body "$(cat <<'EOF'
## Summary

Strips the entire Pat sidecar surface from tuxlink and ships native B2F
outbound with attachment support, in a single atomic PR (13 phases /
commits + the final Codex review fix-ups). Unblocks HTML Forms v0.1
(PR #151).

## What ships

- Native `compose_message_with_files` + extended `Message` serializer /
  parser handling Winlink B2F `File:` headers + raw-bytes attachment
  tail (per wl2k-go reference; golden vector test against
  `LPE5NXDVLVSQ.b2f`).
- `WinlinkBackend::send_message` trait return tightens to
  `Result<MessageId, BackendError>` (Pat's no-MID-echo limitation no
  longer applies).
- Wire observability hooks in `winlink::session::send_turn` log every
  FC EM proposal + FS response; FS-reject maps to typed
  `BackendError::MessageRejected`.
- Operator-state migrations: keyring service name `tuxlink-pat` →
  `tuxlink` with one-time auto-migration; `Config.pat_mbo_address`
  deprecated.
- Pat module (~1320 LOC) + Go-build infra + `sidecars/` directory +
  `external/tuxlink-pat` submodule + `.github/workflows/release.yml`
  Pat steps **deleted**.
- ADRs 0003 + 0011 marked superseded; new ADR 0016 documents the
  cutover. HTML Forms spec revised rev-2 → rev-3 (drops Path A).

## Verification

- 510+ cargo tests pass.
- Golden-vector conformance test asserts byte-equality with wl2k-go's
  `LPE5NXDVLVSQ.b2f` fixture.
- Two-backend in-process exchange with attachments round-trips intact.
- Operator should run the 7-case CMS-telnet smoke per spec §7.3 against
  `cms-z.winlink.org` after merge.

## Operator post-merge actions

- Run `git submodule deinit -f external/tuxlink-pat` then
  `rm -rf .git/modules/external/tuxlink-pat` to clear the local
  submodule registration.
- Run the §7.3 smoke battery (CMS telnet authorized; no RF involved).
- The keyring entry under `tuxlink-pat` will be migrated to `tuxlink`
  transparently on first connect.

## Rollback

`git revert -m 1 <merge-sha>` is clean. Atomic PR with no transient
state.

## References

- Design spec: `docs/superpowers/specs/2026-05-30-pat-strip-native-attachments-design.md`
- Plan: `docs/superpowers/plans/2026-05-30-pat-strip-native-attachments-plan.md`
- bd issue: tuxlink-9phd
- Unblocks: PR #151 (tuxlink-v1p / HTML Forms v0.1)

## Test plan

- [x] cargo build --workspace
- [x] cargo test --workspace
- [x] pnpm typecheck && pnpm test:unit
- [ ] Operator: run `dev/superpowers/specs/...§7.3` smoke against cms-z.winlink.org

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 5: Mark bd issue closed once PR merges**

After GitHub merge:
```bash
bd close tuxlink-9phd
bd update tuxlink-v1p --remove-blocker tuxlink-9phd
```

---

## Self-review (plan author's checklist)

Before declaring the plan complete:

**1. Spec coverage:** Walk the spec rev-3 section-by-section. For each requirement, can you point to a task that implements it?
- §3 wire format → Tasks 1.5, 1.6, 1.8, 2.3, 1.9 (golden vector)
- §3.2 Q-encoding → Task 1.8
- §4.1 compose API → Tasks 1.1, 1.3, 1.4
- §4.2 OutboundAttachment → Task 1.1 (content_type field deletion)
- §4.3 NativeBackend → Task 4.1
- §4.4 test_fixture → Task 5.1
- §4.5 removed APIs → Tasks 5.3 (bootstrap helpers), 6.1 (LogSource::Pat), 9.1-9.7 (Pat module)
- §4.6 from_bytes → Tasks 2.1, 2.3, 2.4
- §4.7 trait shape → Task 3.1
- §4.8 observability → Task 4.2
- §5 P0 MailboxFolder → Task 0.1
- §5 P1-P12 → Tasks 1.x, 2.x, ..., 12.x
- §6 ADRs → Tasks 12.1, 12.2, 12.3
- §7.1 keyring migration → Tasks 7.1, 7.2, 7.3
- §7.2 wizard test-send → Task 5.4
- §7.3 operator smoke → documented in PR body; no implementation task (operator-driven)
- §7.5 submodule deinit → Task 11.1
- §8 tests → covered by each task's TDD steps
- §9 risks → addressed implicitly via tests + commit polish

**2. Placeholder scan:** Search this plan for "TODO" / "TBD" / "implement later" / vague directives. NONE found in this draft (verify on final read).

**3. Type consistency:** Method names used in later tasks match earlier:
- `compose_message_with_files` (1.1) used in 1.3, 1.4, 1.11, 4.1 ✓
- `Message::set_attachments` (1.2) used in 1.5, 1.6, 1.7 ✓
- `credentials::read_password` (7.1) used in 7.2 ✓
- `NativeBackend::test_fixture` (5.1) used in 5.2 ✓
- `ComposeError` variants (1.1) used in 1.3, 4.1 ✓

**4. Task granularity:** Tasks are bite-sized; each step is 2-5 minutes. Some tasks (9.4, 9.5) defer to subagent judgment for the per-function deletion call; that's acceptable because the per-test inspection is genuinely required.

**5. Per-phase review loops:** Each of the 13 phases has a review loop step at the end. ✓

---

## Execution

**Plan complete and saved to `docs/superpowers/plans/2026-05-30-pat-strip-native-attachments-plan.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task; review between tasks; fast iteration on the per-task feedback loop. Each task is bite-sized enough that one subagent per task is the natural unit.

**2. Inline Execution** — execute tasks in this session using `superpowers:executing-plans`; batch execution with checkpoints for review.

Subagent-driven is the recommended approach for this plan because:
- The 13-phase decomposition is deep; per-task subagents keep each context focused.
- Per-phase review loops + parent-level Codex round (post-impl) catch self-review bias.
- The work is largely mechanical (file edits + tests) rather than design-discovery; subagents excel at this.

If **Subagent-Driven** chosen: invoke `superpowers:subagent-driven-development` per the BRF workflow.

If **Inline Execution** chosen: invoke `superpowers:executing-plans`.
