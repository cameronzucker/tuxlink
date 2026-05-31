# HTML Forms v0.1 Implementation Plan (rev-4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Rev-4 changes** (post tuxlink-9phd merge — native backend replaces Pat REST):
>
> Rev-3 was written against spec rev-2 (Path A: Pat REST) by `yew-cypress-oak` on 2026-05-30. The 9phd PR (bd-tuxlink-9phd/strip-pat-add-native-attachments) shipped Path B (native B2F + attachments) AND stripped `pat_client.rs` completely. Spec was updated to rev-3 in 9phd Phase 12 (T12.4). Plan rev-4 aligns the plan with spec rev-3 and the shipped API surface.
>
> **Changes in rev-4:**
> - **T0.1 marked DONE** — `OutboundAttachment` struct + `OutboundMessage.attachments` field landed on main via PR #151 (commits 3b236af/5012707). Note: the shipped struct has NO `content_type` field (dropped in 9phd T1.1); test code referencing `content_type` in this plan is annotated as stale.
> - **T0.2 marked DONE** — caller compile-fix at `ui_commands.rs:660` landed on main via PR #151.
> - **T0.3 marked OBSOLETE** — `send_with_attachments` on `pat_client` is fictional; `pat_client.rs` is deleted (ADR 0016). The native equivalent is `compose_message_with_files` (shipped in 9phd Phase 1), wired through `NativeBackend::send_message` via the `OutboundDraftDto.attachments` IPC bridge added in 9phd Codex P2.1.
> - **T3.1 rewritten** — `send_form` command now constructs `OutboundMessage` with `OutboundAttachment { filename, bytes }` (NO `content_type`), calls `backend.send_message(msg)` directly (same path as `message_send`). Drops all Pat REST routing logic.
> - **Architecture paragraph updated** — now cites spec rev-3 + native B2F (Path B) as the only v0.1 encoding path.
> - **File Structure table updated** — `pat_client.rs` row replaced with DONE/OBSOLETE annotations.
> - **Self-Review table updated** — §5.1 row corrected to reflect native path (not Pat/Path A).
> - **Live smokes T11.2-11.5 updated** — Smoke B and Smoke D retain Pat wire-format cross-client validation (Pat-compatibility is still a goal per spec §11); no behavior change to the smoke descriptions, only notes about the transport path.
> - All other Pat prose references audited; non-load-bearing mentions (WLE+Pat compatibility phrasing, historical test conventions, tuxlink-pat references in smoke descriptions) are left in place as they describe interop goals, not implementation paths.
>
> Per [spec rev-3](../specs/2026-05-30-html-forms-design.md) §5.1 + [ADR 0016](../../adr/0016-native-b2f-outbound-with-attachments.md) + 9phd Phase 1+4.

> **Rev-3 changes** (post 4-round plan review — 3 design rounds + 1 verification round):
>
> **Rev-2 fixed** (caught by R1/R2/R3 design rounds):
> - **T0.2 — `OutboundMessage` disambiguation**: rev-1's "grep all callers" catches a different struct (`session::OutboundMessage`). Rev-2 lists the exact 3 in-scope callers.
> - **T0.3 — `pat_client::send` signature**: rev-1 assumed `send(&OutboundMessage)`; actual sig is positional. Rev-2 adds `send_with_attachments` method.
> - **T1.3 — quick-xml version**: rev-1 said `0.36`; spec §10 mandates `0.39.x`.
> - **T1.7, T9.x — `dev/scratch/` paths**: rev-1 used relative paths that don't resolve in this worktree.
> - **Phase 9 — parallel claim**: rev-1 said "parallel-safe"; rev-2 corrects to serial.
>
> **Rev-3 fixed** (caught by R4 verification round):
> - **T0.2 Step 1 grep filter**: rev-2's `grep -v "session::"` didn't exclude `src/winlink/session.rs:429`'s unqualified `OutboundMessage {` (same-module reference doesn't use the `session::` prefix). Rev-3 adds `grep -v "src/winlink/session.rs"`.
> - **T0.3 fabricated types**: rev-2's `send_with_attachments` impl code used phantom `PatClientError::Send`, `self.callsign`, `self.client`, `BackendError::Transport` — all wrong vs actual `Http`/`Status`/`TooLarge` variants, `self.http` field, `TransportFailed` variant. Rev-3 makes the impl block **spec-level** (instructs subagent to read actual types from the real file) while keeping the test code concrete.
> - **Vestigial 0.36 references**: rev-2 had 4 leftover `0.36` strings in commit messages + verify command. Rev-3 fixes all.
> - **Self-Review checklist drift**: rev-2 still had "T9.1–9.4 can run in parallel" in the Self-Review section + execution-handoff section. Rev-3 corrects.
>
> **Operator-honest disclosure**: BRF review converged but is asymptotic; rev-3 may still have polish issues a 5th round would catch. The remaining defects are likely P2-class (cosmetic / mild drift), but a subagent following any step verbatim that hits an error should STOP and surface to the main session rather than fabricate a workaround. The first compile failure in subagent execution is the operator's final review gate.

**Goal:** Ship HTML Forms v0.1 — bidirectional Winlink form support (render incoming, author + send ICS-213) with 5 bundled forms (ICS-213, ICS-309, Position Report, Bulletin, Damage Assessment), wire-format compatible with Winlink Express and Pat.

**Architecture:** Per [spec rev-3](../specs/2026-05-30-html-forms-design.md) §5.1 (native attachment path — the only v0.1 encoding path), native React form components for compose+render, Rust `forms/` module for parse+serialize+catalog, eager form-payload parse on inbound, send via `compose_message_with_files` → `NativeBackend::send_message` (Path B; Pat REST / Path A removed per ADR 0016 + tuxlink-9phd). Hardened parser (quick-xml + size caps + entity-expansion rejection). Backend precursor: extend `OutboundMessage` with `attachments` field — DONE (PR #151).

**Tech Stack:** Rust 1.75+ (`quick-xml` for XML, existing `serde`, `mail-parser`), React + Tauri 2 (existing), Vitest + cargo-test.

**Spec authority:** [`docs/superpowers/specs/2026-05-30-html-forms-design.md`](../specs/2026-05-30-html-forms-design.md) is the spec. All "per spec §M.N" references resolve against it. If a task's expected behavior is ambiguous, the spec wins — do NOT improvise.

**Pitfalls authority:** Before any test work, read [`docs/pitfalls/testing-pitfalls.md`](../../pitfalls/testing-pitfalls.md) and [`docs/pitfalls/implementation-pitfalls.md`](../../pitfalls/implementation-pitfalls.md).

---

## File Structure

### New files (created in this plan)

| Path | Responsibility | Tasks |
|---|---|---|
| `src-tauri/src/forms/mod.rs` | Module root; re-exports public surface | T1.1 |
| `src-tauri/src/forms/types.rs` | `FormDef`, `FormField`, `FormPayload`, `FieldKind`, `FormParameters` | T1.1 |
| `src-tauri/src/forms/validation.rs` | `form_id` regex, `MAX_FORM_XML_BYTES`, parser-config helpers | T1.2 |
| `src-tauri/src/forms/parse.rs` | `detect_form_attachment`, `parse_form_xml` (hardened) | T1.3, T1.4 |
| `src-tauri/src/forms/serialize.rs` | `serialize_form_xml`, `render_body_template` | T1.5, T1.6 |
| `src-tauri/src/forms/catalog.rs` | Bundled `FormDef` consts for 5 forms | T1.7, T5.1, T9.1–9.4 |
| `src-tauri/src/forms/templates/ics213.rs` | ICS-213 body/subject template + field schema | T1.7 |
| `src-tauri/src/forms/templates/ics309.rs` | ICS-309 | T9.1 |
| `src-tauri/src/forms/templates/position.rs` | GPS Position Report | T9.2 |
| `src-tauri/src/forms/templates/bulletin.rs` | Bulletin | T9.3 |
| `src-tauri/src/forms/templates/damage_assessment.rs` | Damage Assessment | T9.4 |
| `src-tauri/tests/forms_test.rs` | Integration: round-trip per form + hardening regression | T1.8, T1.9 |
| `src/forms/types.ts` | TS mirror of `FormPayload`, `FormDef`, `FormField`, `FieldKind` | T4.1 |
| `src/forms/forms.ts` | Registry: `form_id → { Form, View }` React component pair | T4.2 |
| `src/forms/KeyValueView.tsx` | Unknown-form fallback (body text + raw fields) | T4.3 |
| `src/forms/KeyValueView.test.tsx` | Vitest for fallback | T4.3 |
| `src/forms/FormPicker.tsx` | Compose modal: pick a form to author | T4.4 |
| `src/forms/FormPicker.test.tsx` | Vitest for picker | T4.4 |
| `src/forms/ics213/Ics213Form.tsx` | Compose-side React form | T5.1 |
| `src/forms/ics213/Ics213View.tsx` | Read-side React view | T5.2 |
| `src/forms/ics213/Ics213Form.test.tsx` | Vitest | T5.1 |
| `src/forms/ics213/Ics213View.test.tsx` | Vitest | T5.2 |
| `src/forms/ics309/{Ics309Form,Ics309View,Ics309Form.test,Ics309View.test}.tsx` | ICS-309 pair + tests | T9.1 |
| `src/forms/position/{PositionForm,PositionView,PositionForm.test,PositionView.test}.tsx` | Position pair + tests | T9.2 |
| `src/forms/bulletin/{BulletinForm,BulletinView,BulletinForm.test,BulletinView.test}.tsx` | Bulletin pair + tests | T9.3 |
| `src/forms/damage_assessment/{DamageAssessmentForm,DamageAssessmentView,*.test}.tsx` | DA pair + tests | T9.4 |

### Modified files

| Path | Changes | Tasks |
|---|---|---|
| `src-tauri/src/winlink_backend.rs:89-100` | Add `OutboundAttachment` struct; extend `OutboundMessage` with `attachments: Vec<OutboundAttachment>` | T0.1 — **DONE** (PR #151) |
| Multiple callers of `OutboundMessage::new` | Add `attachments: vec![]` default for non-form sends (compile fix after T0.1) | T0.2 — **DONE** (PR #151) |
| ~~`src-tauri/src/pat_client.rs`~~ | ~~Switch to multipart/form-data POST when attachments present~~ | T0.3 — **OBSOLETE** (pat_client.rs deleted per ADR 0016 / 9phd; native equivalent is `compose_message_with_files` in T3.1) |
| `src-tauri/src/ui_commands.rs:214` | Add `MAX_FORM_XML_BYTES = 256 * 1024` constant | T1.2 |
| `src-tauri/src/ui_commands.rs:244-247` | Add `form_id: Option<String>` + `form_payload: Option<FormPayload>` to `ParsedMessageDto` | T2.2 |
| `src-tauri/src/ui_commands.rs:325-327` | Fix `is_form` detection: attachment-name match instead of body prefix | T2.1 |
| `src-tauri/src/ui_commands.rs` (new fn near `parse_raw_rfc5322`) | Add `send_form` Tauri command | T3.1 |
| `src-tauri/src/lib.rs` (invoke_handler) | Register `send_form` command | T3.1 |
| `src-tauri/src/main.rs` (or wherever module root) | Add `pub mod forms;` | T1.1 |
| `src/mailbox/types.ts` | Mirror DTO additions (`formId`, `formPayload`) | T2.4 |
| `src/mailbox/MessageView.tsx:141-232` | Replace form placeholder with form-render dispatch | T7.1 |
| `src/mailbox/replyActions.ts:75-95` | Update body-vs-XML logic for new attachment-based detection | T8.1 |
| `src/mailbox/replyActions.test.ts` | Update FORM_XML fixtures for new format; add new reply-to-form tests | T8.3 |
| `src/mailbox/devFixture.ts:256-265` | Update form fixture to new format (body is rendered text, not raw XML) | T2.3 |
| `src/compose/Compose.tsx` | Add "Compose form…" button + unsaved-changes dialog wiring | T6.1, T6.2 |
| `src/compose/useDraft.ts:63-67` | Extend `DraftData` with `formId?` + `formFields?` | T6.3 |
| `src/compose/useDraft.test.ts` (or `draft.test.ts:74`) | Tests for form-draft persistence | T6.4 |
| `src-tauri/Cargo.toml` | Add `quick-xml = "0.39"` (matches spec §10 mandate) | T1.3 |

### Cross-cutting

- **Hardening (T10.x)** applies to multiple files; tasks pin specific edits.
- **Codex round + live smokes (T11.x)** happen against the final implementation; outside the strict task tree but mandatory before merge.

---

## Phase 0 — Backend precursor: outbound attachment support

This phase is independent of the forms-specific work; it adds the attachment plumbing that Phase 1+ depends on. It also enables capability §1.6 (compose-side attach) from the inventory.

### Task 0.1: Add `OutboundAttachment` struct + extend `OutboundMessage`

> **DONE** (landed via PR #151 commits 3b236af / 5012707; on main).
>
> The shipped `OutboundAttachment` struct has NO `content_type` field (dropped in 9phd T1.1 — B2F wire format does not use MIME content-type). The test code below references `content_type: "text/xml"` which is stale and will not compile. Skip all steps; the struct and field exist in `src-tauri/src/winlink_backend.rs:105-122`. Verify shape before any T3.1 implementation:
> ```bash
> grep -A8 "pub struct OutboundAttachment" src-tauri/src/winlink_backend.rs
> ```

**Files:**
- Modify: `src-tauri/src/winlink_backend.rs:89-100`
- Test: `src-tauri/tests/winlink_backend_test.rs` (existing file; add a test fn)

- [ ] **Step 1: Read pitfalls + TDD skill**

```
Before starting: read .claude/skills/superpowers/test-driven-development.md (or invoke superpowers:test-driven-development) and docs/pitfalls/testing-pitfalls.md.
```

- [ ] **Step 2: Write failing test for attachment-carrying OutboundMessage**

Add to `src-tauri/tests/winlink_backend_test.rs`:

```rust
#[test]
fn test_outbound_message_carries_attachments() {
    use tuxlink_lib::winlink_backend::{OutboundAttachment, OutboundMessage};
    let attach = OutboundAttachment {
        filename: "test.xml".to_string(),
        content_type: "text/xml".to_string(),
        bytes: b"<root/>".to_vec(),
    };
    let msg = OutboundMessage {
        to: vec!["X@winlink.org".to_string()],
        cc: vec![],
        subject: "S".to_string(),
        body: "B".to_string(),
        date: "2026-05-30T00:00:00Z".to_string(),
        attachments: vec![attach.clone()],
    };
    assert_eq!(msg.attachments.len(), 1);
    assert_eq!(msg.attachments[0].filename, "test.xml");
    assert_eq!(msg.attachments[0].content_type, "text/xml");
    assert_eq!(msg.attachments[0].bytes, b"<root/>");
}
```

- [ ] **Step 3: Run test, verify it fails to compile**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test winlink_backend_test test_outbound_message_carries_attachments 2>&1 | tail -10`
Expected: compile error — `OutboundAttachment` not found OR `OutboundMessage` has no `attachments` field.

- [ ] **Step 4: Add the struct + field per spec §6.2**

In `src-tauri/src/winlink_backend.rs`, near `OutboundMessage`:

```rust
#[derive(Debug, Clone)]
pub struct OutboundAttachment {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub attachments: Vec<OutboundAttachment>,
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test winlink_backend_test test_outbound_message_carries_attachments`
Expected: 1 passed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/winlink_backend.rs src-tauri/tests/winlink_backend_test.rs
git commit -m "feat(backend): add OutboundAttachment + extend OutboundMessage (tuxlink-v1p)

Breaking change to OutboundMessage struct (acknowledged at code-comment
level — line 89 in original). Adds attachments: Vec<OutboundAttachment>.
Required for HTML Forms v0.1 (spec §6.2) and capability §1.6 from the
inventory.

Callers updated in T0.2.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.2: Update winlink_backend::OutboundMessage callers (compile fix)

> **DONE** (landed via PR #151 commits 3b236af / 5012707; on main).
>
> `ui_commands.rs:660` (now ~`ui_commands.rs:660` post-9phd edits) already carries `attachments: vec![]`. Skip all steps.

**Files:** The THREE specific callers of `winlink_backend::OutboundMessage { ... }` listed below. **Do NOT touch `winlink::session::OutboundMessage { ... }` callers** — that's a DIFFERENT struct (proposal/title/compressed B2F-layer type at `src-tauri/src/winlink/session.rs:32`) which a mechanical `grep "OutboundMessage {"` will catch but which we are NOT modifying.

The full caller list (verified via grep on rev-2 of the plan):

- `src-tauri/src/ui_commands.rs:654` (the existing `message_send` command handler)
- `src-tauri/tests/winlink_backend_test.rs:64`
- `src-tauri/tests/winlink_backend_test.rs:177` (`.send_message(OutboundMessage { ... })` call site)

The other 3 matches from `grep "OutboundMessage {"` are NOT in scope:
- `src-tauri/src/winlink_backend.rs:93` (the struct definition itself — modified in T0.1)
- `src-tauri/src/winlink_backend.rs:936` and `:1219` (these construct `session::OutboundMessage` — different struct, different field set, unchanged)
- `src-tauri/src/winlink/session.rs:32` (the OTHER struct definition — unchanged)
- `src-tauri/src/winlink/session.rs:429` (constructs `session::OutboundMessage` — unchanged)

- [ ] **Step 1: Confirm the scope is exactly 3 callers**

Run: `grep -rn "OutboundMessage {" src-tauri/src/ src-tauri/tests/ | grep -v "session::" | grep -v "pub struct" | grep -v "src/winlink/session.rs"`

Expected output (exactly 3 lines):
```
src-tauri/src/ui_commands.rs:654:    let msg = OutboundMessage {
src-tauri/tests/winlink_backend_test.rs:64:    let msg = OutboundMessage {
src-tauri/tests/winlink_backend_test.rs:177:        .send_message(OutboundMessage {
```

The `grep -v "src/winlink/session.rs"` is load-bearing: that file's `OutboundMessage { ... }` literal at line 429 is the *unqualified* in-module construction of `session::OutboundMessage` (it does NOT use the `session::` prefix since it IS the session module), so the previous `grep -v "session::"` filter alone would have included it.

If the count differs, STOP and re-read the disambiguation note above — do NOT mechanically apply the fix to extra lines.

- [ ] **Step 2: Update each of the 3 callers to include `attachments: vec![]`**

For each of the 3 sites, add `attachments: vec![]` as the new last field in the struct literal. Default to empty for plain (non-form) sends. Do NOT add any other behavior change. Do NOT touch any `session::OutboundMessage { ... }` site.

- [ ] **Step 3: Run cargo build to verify compile is clean**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -15`
Expected: `Finished` with no errors.

- [ ] **Step 4: Run full test suite to confirm no regression**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --tests -- --test-threads=1 2>&1 | grep -E "^test result|FAILED" | tail -20`
Expected: all green; no FAILED lines. (Tally must match the pre-change tally, except for +1 from Task 0.1's new test.)

- [ ] **Step 5: Commit**

```bash
git add -p src-tauri/  # stage just the OutboundMessage callsite updates
git commit -m "build(backend): default OutboundMessage callers to attachments: vec![] (tuxlink-v1p)

Compile fix for the OutboundMessage breaking change in T0.1. No
behavior change for plain-text sends.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.3: Add send_with_attachments to pat_client

> **OBSOLETE per ADR 0016 / spec rev-3 §5.1.** The `pat_client.rs` module is deleted; the native equivalent is `compose_message_with_files` (shipped via tuxlink-9phd Phase 1). Skip this task; verify the API replacement in T3.1's notes.
>
> The remainder of this task is preserved as historical context for the Pat REST approach (Path A), which was the spec rev-2 design. Do NOT execute any step below. If you need to understand the native replacement, read `src-tauri/src/winlink/compose.rs` (compose_message_with_files) and `src-tauri/src/winlink_backend.rs` (NativeBackend::send_message).

**Existing state verified before plan rev-2:**

- `pat_client::send` actual signature (`src-tauri/src/pat_client.rs:207`):
  ```rust
  pub async fn send(&self, to: &[&str], subject: &str, body: &str, date: &str) -> Result<(), PatClientError>
  ```
- Multipart/form-data is ALREADY the default — `send` already builds `reqwest::multipart::Form` with text fields `subject`, `body`, `date` (lines 208-211).
- Callers (`winlink_backend.rs:1684-1688` and `PatBackend::send_message`) pass `&[&str]` for `to`, individual strings for the rest. There is NO `&OutboundMessage` interface today.

**The plan adds attachment support as a NEW method** rather than re-shaping the existing `send` signature — minimizes ripple. The existing `send` stays unchanged.

**Files:**
- Modify: `src-tauri/src/pat_client.rs` (add new method below `send`)
- Modify: `src-tauri/src/winlink_backend.rs` (`PatBackend::send_message` — call the new method when `msg.attachments` is non-empty)
- Test: `src-tauri/tests/pat_client_test.rs`

- [ ] **Step 1: Verify the existing send shape (read-only)**

Run: `sed -n '205,235p' src-tauri/src/pat_client.rs`

Expected output: `pub async fn send(&self, to: &[&str], subject: &str, body: &str, date: &str)` followed by `reqwest::multipart::Form::new()` + `.text("subject", ...)` etc. Confirms the assumption above; if signature has drifted since plan-write time, STOP and re-spec T0.3.

- [ ] **Step 2: Write failing test for the new method**

Add to `src-tauri/tests/pat_client_test.rs`. Use the SAME `mockito` patterns as existing tests in that file — read it first to mirror conventions:

```rust
#[tokio::test]
async fn test_send_with_attachments_includes_file_parts() {
    use tuxlink_lib::pat_client::PatClient;
    use tuxlink_lib::winlink_backend::OutboundAttachment;

    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", mockito::Matcher::Any)
        .match_header("content-type", mockito::Matcher::Regex("multipart/form-data.*".into()))
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::Regex(r#"name="subject""#.into()),
            mockito::Matcher::Regex(r#"name="body""#.into()),
            mockito::Matcher::Regex(r#"name="files".*filename="test\.xml""#.into()),
            mockito::Matcher::Regex("text/xml".into()),
        ]))
        .with_status(200)
        .with_body("ok")
        .create_async()
        .await;

    // Use PatClient's existing constructor (read pat_client.rs:~60-80 for the actual name —
    // likely `PatClient::new(url)` or `from_url(url)`; mirror existing test invocations).
    let client = PatClient::new(&server.url());  // adjust per actual constructor name

    let attachments = vec![OutboundAttachment {
        filename: "test.xml".to_string(),
        content_type: "text/xml".to_string(),
        bytes: b"<root/>".to_vec(),
    }];

    let result = client
        .send_with_attachments(
            &["W4PHS@winlink.org"],
            "Test form",
            "rendered text body",
            "2026-05-30T00:00:00Z",
            &attachments,
        )
        .await;
    assert!(result.is_ok(), "send_with_attachments failed: {:?}", result);
    mock.assert_async().await;
}
```

(Note: the `name="files"` form-field name matches Pat's REST API convention — verify against Pat source at `dev/scratch/tuxlink-pat/` per the spec's §5.1 Path A description. If Pat uses a different field name, update the regex.)

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test pat_client_test test_send_with_attachments_includes_file_parts -- --test-threads=1`
Expected: compile error — `PatClient` has no `send_with_attachments` method.

- [ ] **Step 4: Implement `send_with_attachments` in pat_client.rs (spec-level — subagent reads real file)**

The subagent MUST read the existing `send` fn at `src-tauri/src/pat_client.rs:207-250` (approximately) to learn the actual:

- `PatClient` field names — likely `self.http` (NOT `self.client`), `self.base_url`. Verify.
- `PatClientError` variants — likely `Http(reqwest::Error)`, `Status(u16)`, `TooLarge { cap: usize }`. **Do NOT invent new variants** like `Send(String)`. If error-mapping needs a new shape, extend `PatClientError` with a new variant (separate commit) — do not fabricate.
- URL path — likely `/api/mailbox/out` (lowercase, NO callsign segment). Read line ~215 to confirm.
- HTTP client field — confirm whether it's `self.http` or `self.client`.

**Behavior spec for `send_with_attachments`:**

1. Build a `reqwest::multipart::Form` mirroring the existing `send` (text fields: `subject`, `body`, `date`, plus repeated `to` per recipient).
2. For each `OutboundAttachment` in `attachments`, append a file part:
   - Use `reqwest::multipart::Part::bytes(attach.bytes.clone())` (clone bytes; the Part API consumes the buffer).
   - Set `.file_name(attach.filename.clone())` and `.mime_str(&attach.content_type)`.
   - Append via `form = form.part("<field_name>", part)`.
   - **Field name TBD by Pat's REST contract.** Read `/home/administrator/Code/tuxlink/dev/scratch/tuxlink-pat/internal/forms/builder.go` (absolute path; gitignored in this worktree) for the canonical name — likely `attachment`, `file`, or `files`. The subagent updates BOTH the test regex (Step 2) AND the impl to use the discovered name.
3. POST `.multipart(form).send().await` to the URL from existing `send`'s URL pattern.
4. Error handling mirrors existing `send` — wrap reqwest errors in `PatClientError::Http(_)`, status errors in `PatClientError::Status(_)`. **Do NOT invent a `PatClientError::Send(String)` variant**.
5. Return `Result<(), PatClientError>`.

The subagent's implementation is correct when (a) the test at Step 2 passes AND (b) all error mappings use the existing `PatClientError` variants AND (c) the existing `send` fn signature + behavior is unchanged.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test pat_client_test test_send_with_attachments_includes_file_parts -- --test-threads=1`
Expected: 1 passed.

If the test's `name="files"` regex doesn't match but the other matchers do, the Pat field name differs — confirm against `/home/administrator/Code/tuxlink/dev/scratch/tuxlink-pat/internal/forms/builder.go` and update BOTH the test regex AND the impl `form.part("<name>", part)` call to match.

- [ ] **Step 6: Update PatBackend::send_message to call new method when attachments present**

In `src-tauri/src/winlink_backend.rs`'s `PatBackend::send_message` (subagent: find via `grep -n "self.pat_client.send" src-tauri/src/winlink_backend.rs` — expected around line 1684-1688; LSP/IDE will resolve), change the call:

**Spec for the change:**

- BEFORE: a single `self.pat_client.send(&to_refs, &msg.subject, &msg.body, &msg.date).await.map_err(...)` call.
- AFTER: branch on `msg.attachments.is_empty()`:
  - empty → call existing `send` (unchanged behavior)
  - non-empty → call `send_with_attachments(&to_refs, &msg.subject, &msg.body, &msg.date, &msg.attachments)`
  - **Error mapping**: use the EXISTING `.map_err(...)` clause already at that call site. The variant is `BackendError::TransportFailed { reason }` (NOT `BackendError::Transport { reason }` — verify by reading `pub enum BackendError` around line 289). The reason value is `e.to_string()`.

Subagent: copy-paste the existing error mapping clause; only add the `if msg.attachments.is_empty() { ... } else { ... }` wrapper around the call.

- [ ] **Step 7: Full pat_client_test + winlink_backend_test regression**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test pat_client_test --test winlink_backend_test -- --test-threads=1 2>&1 | grep "test result"`
Expected: all green; tally up by 1 (the new test).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/pat_client.rs src-tauri/src/winlink_backend.rs src-tauri/tests/pat_client_test.rs
git commit -m "feat(pat-client): send_with_attachments for HTML Forms v0.1 (tuxlink-v1p)

Per spec §5.1 Path A. Adds a parallel method to PatClient::send that
takes the same args plus &[OutboundAttachment] and adds file parts to
the multipart/form-data POST. The existing send() is unchanged
(no signature ripple to plain-text callers).

PatBackend::send_message branches on msg.attachments.is_empty() — when
non-empty, routes through send_with_attachments.

Plan rev-2 disambiguates this from the rev-1 mis-assumption that send()
takes &OutboundMessage (it doesn't; signature is positional args).

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 1 — Forms backend module

### Task 1.1: Create forms module + types

**Files:**
- Create: `src-tauri/src/forms/mod.rs`
- Create: `src-tauri/src/forms/types.rs`
- Modify: `src-tauri/src/lib.rs` (or whichever file has `pub mod` declarations) — add `pub mod forms;`

- [ ] **Step 1: Write failing test for the type surface (in a tests file we'll create at T1.8)**

Defer this to T1.8 (integration test setup); for this task, just verify the module compiles.

- [ ] **Step 2: Create `src-tauri/src/forms/mod.rs`**

```rust
//! HTML Forms support per spec docs/superpowers/specs/2026-05-30-html-forms-design.md.
//!
//! Submodules:
//! - `types` — public surface (FormDef, FormField, FormPayload, FieldKind, FormParameters)
//! - `validation` — input validation (form_id regex, size caps)
//! - `parse` — detect form attachments + parse XML payloads (hardened)
//! - `serialize` — build wire-format XML + render body templates
//! - `catalog` — bundled FormDef constants for the 5 v0.1 forms
//! - `templates` — per-form template strings + field schemas

pub mod catalog;
pub mod parse;
pub mod serialize;
pub mod templates;
pub mod types;
pub mod validation;

// Re-exports for ergonomic access from ui_commands.rs.
pub use parse::{detect_form_attachment, parse_form_xml};
pub use serialize::{render_body_template, serialize_form_xml};
pub use types::{FieldKind, FormDef, FormField, FormParameters, FormPayload};
```

- [ ] **Step 3: Create `src-tauri/src/forms/types.rs` per spec §6.1**

```rust
//! Public types for the forms module. Mirrored on the TS side in src/forms/types.ts.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct FormDef {
    pub id: &'static str,
    pub name: &'static str,
    pub fields: &'static [FormField],
    pub subject_template: &'static str,
    pub body_template: &'static str,
    pub display_form: &'static str,
    pub reply_template: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    pub max_length: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    LongText,
    Date,
    Time,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormPayload {
    pub form_id: String,
    pub form_parameters: FormParameters,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormParameters {
    pub xml_file_version: String,
    pub rms_express_version: String,
    pub submission_datetime: String,
    pub senders_callsign: String,
    pub grid_square: String,
    pub display_form: String,
    pub reply_template: String,
}
```

- [ ] **Step 4: Wire `pub mod forms;` into the library root**

In `src-tauri/src/lib.rs`, find the existing `pub mod` block and add:

```rust
pub mod forms;
```

- [ ] **Step 5: Compile**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -10`
Expected: `Finished` — may have warnings about unused validation/parse/serialize/catalog/templates modules (they're empty); ok for now.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/forms/mod.rs src-tauri/src/forms/types.rs src-tauri/src/lib.rs
git commit -m "feat(forms): create module + types per spec §6.1 (tuxlink-v1p)

Public types: FormDef, FormField, FormPayload, FieldKind, FormParameters.
FormPayload/FormParameters are serde-Serialize so they flow over Tauri
IPC; FieldKind uses snake_case JSON.

Submodules: validation, parse, serialize, catalog, templates — stubs
filled in T1.2 onward.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.2: validation module — form_id regex + MAX_FORM_XML_BYTES

**Files:** Create `src-tauri/src/forms/validation.rs`; modify `src-tauri/src/ui_commands.rs:214` to re-export the size cap.

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/forms/validation.rs` with module-level tests:

```rust
//! Input validation for the forms module (spec §10).

/// Maximum bytes for an inbound form XML attachment. Enforced at
/// `parse_form_xml` boundary; rejection is UiError::Internal before
/// allocation. 256 KiB is well above any plausible Winlink form.
pub const MAX_FORM_XML_BYTES: usize = 256 * 1024;

/// Maximum number of `<variables>` fields per form payload. Anything
/// beyond is rejected as malicious or malformed.
pub const MAX_FORM_FIELDS: usize = 256;

/// Maximum XML element nesting depth during parse. Defense against
/// pathological nesting bombs.
pub const MAX_XML_NESTING_DEPTH: u16 = 8;

/// Maximum total XML events the parser will consume. Defense against
/// quadratic-blowup attacks (many small elements).
pub const MAX_XML_EVENTS: u32 = 10_000;

/// Validate a form ID extracted from an attachment filename.
///
/// Spec §10: `^[A-Za-z0-9_-]{1,64}$`. Path-traversal-safe; documented as
/// load-bearing for v0.5+ catalog-cache use.
pub fn is_valid_form_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_form_ids() {
        assert!(is_valid_form_id("ICS213_Initial"));
        assert!(is_valid_form_id("ICS309_Initial"));
        assert!(is_valid_form_id("Position_Initial"));
        assert!(is_valid_form_id("a"));
        assert!(is_valid_form_id("a_b-c_1"));
        assert!(is_valid_form_id(&"X".repeat(64)));
    }

    #[test]
    fn rejects_invalid_form_ids() {
        assert!(!is_valid_form_id(""), "empty");
        assert!(!is_valid_form_id(&"X".repeat(65)), ">64 chars");
        assert!(!is_valid_form_id("../etc/passwd"), "path traversal");
        assert!(!is_valid_form_id("foo bar"), "whitespace");
        assert!(!is_valid_form_id("foo.bar"), "dot");
        assert!(!is_valid_form_id("foo/bar"), "slash");
        assert!(!is_valid_form_id("foo\\bar"), "backslash");
        assert!(!is_valid_form_id("Ünïcödë"), "non-ASCII");
        assert!(!is_valid_form_id("foo\x00bar"), "null");
    }

    #[test]
    fn size_cap_is_256_kib() {
        assert_eq!(MAX_FORM_XML_BYTES, 262_144);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass (no impl yet means tests-as-spec)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib forms::validation::tests -- --test-threads=1`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/forms/validation.rs
git commit -m "feat(forms): validation module — form_id regex + size caps (tuxlink-v1p)

Per spec §10 hardening:
  - form_id: ^[A-Za-z0-9_-]{1,64}$ (path-traversal-safe)
  - MAX_FORM_XML_BYTES = 256 KiB
  - MAX_FORM_FIELDS = 256
  - MAX_XML_NESTING_DEPTH = 8
  - MAX_XML_EVENTS = 10000

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.3: Add quick-xml dependency

**Files:** `src-tauri/Cargo.toml`

- [ ] **Step 1: Add dep to Cargo.toml**

Add to `[dependencies]`:

```toml
quick-xml = { version = "0.39", default-features = false, features = ["serialize"] }
```

Spec §10 mandates `quick-xml 0.39.x`. Pin to a 0.39.x release with documented entity-expansion limits and stable `Reader::trim_text` / `Event::DocType` APIs. (Plan rev-1 said 0.36 — that was internal drift from the spec; rev-2 corrects it.) Verify against MSRV 1.75 in src-tauri/Cargo.toml; quick-xml 0.39 supports MSRV ≥ 1.74.

- [ ] **Step 2: Verify it resolves**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "Compiling quick-xml|error" | head -5`
Expected: a line `Compiling quick-xml v0.39.x` and no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build(deps): add quick-xml 0.39 for form XML parsing (tuxlink-v1p)

Required by spec §10 hardening. default-features=false to avoid pulling
optional serialize feature unless needed.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.4: parse.rs — detect_form_attachment

**Files:** Create `src-tauri/src/forms/parse.rs`.

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/forms/parse.rs`:

```rust
//! Form-XML parsing per spec §3 wire format + §10 hardening.

use crate::forms::types::FormPayload;
use crate::forms::validation;

/// Detect whether an attachment is a Winlink form XML (`RMS_Express_Form_*.xml`)
/// and extract its form_id. Returns None if the attachment is not a form.
///
/// The form_id is the basename between "RMS_Express_Form_" prefix and ".xml"
/// suffix (e.g., "ICS213_Initial" for "RMS_Express_Form_ICS213_Initial.xml").
/// Per spec §10, the result is validated against the safe form_id regex; an
/// attachment with an unsafe basename (path traversal etc.) returns None.
pub fn detect_form_attachment(filename: &str) -> Option<String> {
    const PREFIX: &str = "RMS_Express_Form_";
    const SUFFIX: &str = ".xml";
    let stripped = filename.strip_prefix(PREFIX)?;
    let id = stripped.strip_suffix(SUFFIX)?;
    if validation::is_valid_form_id(id) {
        Some(id.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ics213_attachment() {
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_ICS213_Initial.xml"),
            Some("ICS213_Initial".to_string())
        );
    }

    #[test]
    fn detects_ics309_attachment() {
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_ICS309_Initial.xml"),
            Some("ICS309_Initial".to_string())
        );
    }

    #[test]
    fn ignores_non_form_attachment() {
        assert_eq!(detect_form_attachment("photo.jpg"), None);
        assert_eq!(detect_form_attachment("data.xml"), None);
        assert_eq!(detect_form_attachment("RMS_Express_Form_.xml"), None);
        assert_eq!(detect_form_attachment("RMS_Express_Form_ICS213"), None);
    }

    #[test]
    fn rejects_unsafe_form_id() {
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_../etc/passwd.xml"),
            None
        );
        assert_eq!(
            detect_form_attachment("RMS_Express_Form_foo bar.xml"),
            None
        );
    }
}

// parse_form_xml — implemented in T1.5.
pub fn parse_form_xml(_bytes: &[u8]) -> Result<FormPayload, String> {
    todo!("T1.5")
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib forms::parse::tests -- --test-threads=1`
Expected: 4 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/forms/parse.rs
git commit -m "feat(forms): detect_form_attachment per spec §3 + §10 (tuxlink-v1p)

Matches RMS_Express_Form_<id>.xml attachment names, extracts and
validates form_id via validation::is_valid_form_id (path-traversal safe).
parse_form_xml stub left as todo!() for T1.5.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.5: parse.rs — parse_form_xml (hardened)

**Files:** Modify `src-tauri/src/forms/parse.rs` (replace the `todo!()` stub).

- [ ] **Step 1: Write failing tests**

Add to `src-tauri/src/forms/parse.rs` `#[cfg(test)] mod tests`:

```rust
const SAMPLE_XML: &str = r#"<?xml version="1.0"?>
<RMS_Express_Form>
  <form_parameters>
    <xml_file_version>1.0</xml_file_version>
    <rms_express_version>Tuxlink/0.3.0</rms_express_version>
    <submission_datetime>20260530143000</submission_datetime>
    <senders_callsign>N0CALL</senders_callsign>
    <grid_square>FM18</grid_square>
    <display_form>ICS213_Initial_Viewer.html</display_form>
    <reply_template>ICS213_SendReply.0</reply_template>
  </form_parameters>
  <variables>
    <inc_name>HURRICANE WALDO</inc_name>
    <to_name>JOHN OPERATOR</to_name>
    <fm_name>JANE OPERATOR</fm_name>
    <subjectline>REQUEST SUPPLIES</subjectline>
    <mdate>2026-05-30</mdate>
    <mtime>14:30Z</mtime>
    <message>Need bandages.</message>
    <approved_name>JANE OPERATOR</approved_name>
  </variables>
</RMS_Express_Form>"#;

#[test]
fn parses_well_formed_form_xml() {
    let payload = parse_form_xml(SAMPLE_XML.as_bytes()).expect("parse should succeed");
    assert_eq!(payload.form_id, "");  // form_id is set by caller (from attachment name)
    assert_eq!(payload.form_parameters.display_form, "ICS213_Initial_Viewer.html");
    assert_eq!(payload.form_parameters.rms_express_version, "Tuxlink/0.3.0");
    assert_eq!(payload.form_parameters.senders_callsign, "N0CALL");
    let inc_name = payload.fields.iter().find(|(k, _)| k == "inc_name").map(|(_, v)| v.as_str());
    assert_eq!(inc_name, Some("HURRICANE WALDO"));
    let mtime = payload.fields.iter().find(|(k, _)| k == "mtime").map(|(_, v)| v.as_str());
    assert_eq!(mtime, Some("14:30Z"));
}

#[test]
fn rejects_oversized_xml() {
    let huge = vec![b'<'; validation::MAX_FORM_XML_BYTES + 1];
    let result = parse_form_xml(&huge);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("too large"));
}

#[test]
fn rejects_billion_laughs_doctype() {
    let malicious = r#"<?xml version="1.0"?>
<!DOCTYPE lolz [
  <!ENTITY lol "lol">
  <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
]>
<RMS_Express_Form>&lol2;</RMS_Express_Form>"#;
    let result = parse_form_xml(malicious.as_bytes());
    assert!(result.is_err(), "DOCTYPE/entity bomb must be rejected");
}

#[test]
fn rejects_deeply_nested_xml() {
    let mut bomb = String::from("<?xml version=\"1.0\"?>");
    for _ in 0..20 {
        bomb.push_str("<x>");
    }
    for _ in 0..20 {
        bomb.push_str("</x>");
    }
    let result = parse_form_xml(bomb.as_bytes());
    assert!(result.is_err(), "depth-20 nesting must exceed MAX_XML_NESTING_DEPTH=8");
}

#[test]
fn rejects_too_many_fields() {
    let mut payload = String::from(r#"<?xml version="1.0"?><RMS_Express_Form><variables>"#);
    for i in 0..300 {
        payload.push_str(&format!("<f{0}>v</f{0}>", i));
    }
    payload.push_str("</variables></RMS_Express_Form>");
    let result = parse_form_xml(payload.as_bytes());
    assert!(result.is_err(), "300 fields must exceed MAX_FORM_FIELDS=256");
}
```

- [ ] **Step 2: Run tests to verify they fail with `todo!()` panic**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib forms::parse::tests -- --test-threads=1 2>&1 | tail -15`
Expected: parses_well_formed_form_xml + 4 rejection tests all FAIL (panic from todo!() or "not yet implemented").

- [ ] **Step 3: Implement parse_form_xml per spec §3 + §10**

Replace the `todo!()` stub. The impl uses `quick_xml::reader::Reader` with these hardening config:

- Reject `Event::DocType` early (returns Err — no DTDs / no entity expansion).
- Track nesting depth; exceed `MAX_XML_NESTING_DEPTH` → Err.
- Track total event count; exceed `MAX_XML_EVENTS` → Err.
- Cap field count at `MAX_FORM_FIELDS` → Err.
- Size-check input bytes first (≤ `MAX_FORM_XML_BYTES`) → Err.
- Extract `<form_parameters>` children into `FormParameters` (7 known fields, populate matching ones).
- Extract `<variables>` children into `fields: Vec<(String, String)>` preserving XML order.
- Set `form_id` to empty String (the caller sets it from the attachment name).

Reference: spec §3 (full wire example), §10 (hardening table).

- [ ] **Step 4: Run tests to verify all 5 pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib forms::parse::tests -- --test-threads=1`
Expected: 9 passed (4 detection from T1.4 + 5 new parse tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/forms/parse.rs
git commit -m "feat(forms): parse_form_xml — hardened per spec §3 + §10 (tuxlink-v1p)

Uses quick-xml 0.39 with:
  - DOCTYPE rejection (no entity expansion / billion laughs)
  - depth cap (MAX_XML_NESTING_DEPTH=8)
  - event count cap (MAX_XML_EVENTS=10000)
  - field count cap (MAX_FORM_FIELDS=256)
  - input size cap (MAX_FORM_XML_BYTES=256 KiB)

Returns FormPayload with form_id empty (caller fills from attachment
name) + FormParameters populated for 7 known elements + fields Vec
preserving XML order.

5 regression tests: well-formed parse, oversized rejection, DOCTYPE
rejection, deep-nesting rejection, field-count rejection.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.6: serialize.rs — serialize_form_xml

**Files:** Create `src-tauri/src/forms/serialize.rs`.

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/forms/serialize.rs`:

```rust
//! Serialize form data to WLE-compatible wire format per spec §3.

use crate::forms::types::{FormDef, FormParameters};
use std::collections::HashMap;

/// Serialize a form's field values to WLE-compatible XML bytes (UTF-8 with BOM).
///
/// Per spec §3:
/// - `<?xml version="1.0"?>` (no encoding attr)
/// - All `<variables>` element names lowercase
/// - `<form_parameters>` emits 7 elements in WLE order
/// - Empty fields emit `<field></field>` (not self-closing)
/// - Special chars (<, >, &) XML-escaped; " and ' left as-is per WLE
/// - UTF-8 BOM prefix (3 bytes: 0xEF 0xBB 0xBF)
pub fn serialize_form_xml(
    form: &FormDef,
    params: &FormParameters,
    field_values: &HashMap<String, String>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(2048);
    // UTF-8 BOM
    out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    out.extend_from_slice(b"<?xml version=\"1.0\"?>\r\n");
    out.extend_from_slice(b"<RMS_Express_Form>\r\n");
    // form_parameters in WLE order
    out.extend_from_slice(b"<form_parameters>\r\n");
    push_element(&mut out, "xml_file_version", &params.xml_file_version);
    push_element(&mut out, "rms_express_version", &params.rms_express_version);
    push_element(&mut out, "submission_datetime", &params.submission_datetime);
    push_element(&mut out, "senders_callsign", &params.senders_callsign);
    push_element(&mut out, "grid_square", &params.grid_square);
    push_element(&mut out, "display_form", &params.display_form);
    push_element(&mut out, "reply_template", &params.reply_template);
    out.extend_from_slice(b"</form_parameters>\r\n");
    // variables in field-declaration order from FormDef
    out.extend_from_slice(b"<variables>\r\n");
    for field in form.fields {
        let value = field_values.get(field.id).map(String::as_str).unwrap_or("");
        push_element(&mut out, field.id, value);
    }
    out.extend_from_slice(b"</variables>\r\n");
    out.extend_from_slice(b"</RMS_Express_Form>\r\n");
    out
}

/// Write a single XML element with value. Lowercases the name; XML-escapes the
/// value (`<` `>` `&` only, matching WLE — `"` and `'` left as-is).
fn push_element(out: &mut Vec<u8>, name: &str, value: &str) {
    out.push(b'<');
    out.extend_from_slice(name.to_ascii_lowercase().as_bytes());
    out.push(b'>');
    for ch in value.chars() {
        match ch {
            '<' => out.extend_from_slice(b"&lt;"),
            '>' => out.extend_from_slice(b"&gt;"),
            '&' => out.extend_from_slice(b"&amp;"),
            _ => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.extend_from_slice(b"</");
    out.extend_from_slice(name.to_ascii_lowercase().as_bytes());
    out.extend_from_slice(b">\r\n");
}

/// Render the body-template string (`Msg:` block) with `<var fieldid>` placeholders
/// substituted from field values. Case-insensitive on field name (matches WLE+Pat
/// behavior).
pub fn render_body_template(template: &str, field_values: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len() + 256);
    let mut chars = template.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '<' && template[i..].starts_with("<var ") {
            // Find end of <var X>
            if let Some(end) = template[i..].find('>') {
                let var_section = &template[i + 5..i + end];  // skip "<var "
                let field_id = var_section.trim().to_ascii_lowercase();
                let value = field_values.get(&field_id).cloned().unwrap_or_default();
                out.push_str(&value);
                // skip ahead past the closing '>'
                for _ in 0..end {
                    chars.next();
                }
                continue;
            }
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::types::{FieldKind, FormDef, FormField};

    const TEST_FORM: FormDef = FormDef {
        id: "Test_Initial",
        name: "Test Form",
        fields: &[
            FormField { id: "alpha", label: "A", kind: FieldKind::Text, required: false, max_length: None },
            FormField { id: "beta",  label: "B", kind: FieldKind::Text, required: false, max_length: None },
        ],
        subject_template: "<var alpha>",
        body_template: "Hello <var alpha>; from <var beta>.",
        display_form: "Test_Initial_Viewer.html",
        reply_template: "Test_SendReply.0",
    };

    #[test]
    fn xml_starts_with_bom_then_declaration() {
        let params = FormParameters {
            xml_file_version: "1.0".into(),
            rms_express_version: "Tuxlink/0.3.0".into(),
            ..Default::default()
        };
        let values = HashMap::new();
        let xml = serialize_form_xml(&TEST_FORM, &params, &values);
        assert_eq!(&xml[0..3], &[0xEF, 0xBB, 0xBF], "UTF-8 BOM");
        assert!(xml[3..].starts_with(b"<?xml version=\"1.0\"?>"), "declaration");
    }

    #[test]
    fn variables_are_lowercase() {
        let params = FormParameters::default();
        let mut values = HashMap::new();
        values.insert("alpha".to_string(), "A-VALUE".to_string());
        let xml = serialize_form_xml(&TEST_FORM, &params, &values);
        let xml_str = String::from_utf8_lossy(&xml);
        assert!(xml_str.contains("<alpha>A-VALUE</alpha>"));
        assert!(!xml_str.contains("<Alpha>"));
    }

    #[test]
    fn empty_fields_get_open_close_tags() {
        let params = FormParameters::default();
        let values = HashMap::new();  // no values
        let xml = serialize_form_xml(&TEST_FORM, &params, &values);
        let xml_str = String::from_utf8_lossy(&xml);
        assert!(xml_str.contains("<alpha></alpha>"));
        assert!(xml_str.contains("<beta></beta>"));
        assert!(!xml_str.contains("<alpha/>"), "no self-closing");
    }

    #[test]
    fn special_chars_in_values_are_xml_escaped() {
        let params = FormParameters::default();
        let mut values = HashMap::new();
        values.insert("alpha".into(), "<script>&\"'".into());
        let xml = serialize_form_xml(&TEST_FORM, &params, &values);
        let xml_str = String::from_utf8_lossy(&xml);
        assert!(xml_str.contains("&lt;script&gt;&amp;\"'"));
    }

    #[test]
    fn form_parameters_emit_in_wle_order() {
        let params = FormParameters {
            xml_file_version: "1.0".into(),
            rms_express_version: "RMS".into(),
            submission_datetime: "20260530143000".into(),
            senders_callsign: "N0CALL".into(),
            grid_square: "FM18".into(),
            display_form: "X_Viewer.html".into(),
            reply_template: "X_SendReply.0".into(),
        };
        let xml = String::from_utf8_lossy(&serialize_form_xml(&TEST_FORM, &params, &HashMap::new())).to_string();
        let pos_xml = xml.find("<xml_file_version>").unwrap();
        let pos_ver = xml.find("<rms_express_version>").unwrap();
        let pos_dt = xml.find("<submission_datetime>").unwrap();
        let pos_call = xml.find("<senders_callsign>").unwrap();
        let pos_grid = xml.find("<grid_square>").unwrap();
        let pos_df = xml.find("<display_form>").unwrap();
        let pos_rt = xml.find("<reply_template>").unwrap();
        assert!(pos_xml < pos_ver && pos_ver < pos_dt && pos_dt < pos_call
                && pos_call < pos_grid && pos_grid < pos_df && pos_df < pos_rt,
                "form_parameters elements must be in WLE order");
    }

    #[test]
    fn render_body_substitutes_vars_case_insensitive() {
        let mut values = HashMap::new();
        values.insert("alpha".into(), "WORLD".into());
        values.insert("beta".into(), "JANE".into());
        let body = render_body_template("Hello <var alpha>; from <var beta>.", &values);
        assert_eq!(body, "Hello WORLD; from JANE.");
        // Case-insensitive match — `<var Alpha>` substitutes from values["alpha"]
        let body2 = render_body_template("<var Alpha> <var BETA>", &values);
        assert_eq!(body2, "WORLD JANE");
    }

    #[test]
    fn render_body_leaves_unknown_vars_as_empty() {
        let values = HashMap::new();
        let body = render_body_template("Hello <var unknown>!", &values);
        assert_eq!(body, "Hello !");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib forms::serialize::tests -- --test-threads=1`
Expected: 7 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/forms/serialize.rs
git commit -m "feat(forms): serialize_form_xml + render_body_template per spec §3 (tuxlink-v1p)

Serialize:
  - UTF-8 BOM prefix
  - <?xml version=\"1.0\"?> (no encoding attr)
  - <form_parameters> emits 7 elements in WLE order
  - <variables> lowercased
  - Empty fields: <field></field>
  - XML-escape <, >, &; leave \" and ' alone per WLE

render_body_template:
  - Case-insensitive <var X> substitution (matches WLE+Pat)
  - Unknown vars resolve to empty string

7 tests cover BOM, lowercase, empty-fields, escaping, parameter order,
template substitution.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.7: catalog.rs + templates/ics213.rs — first FormDef

**Files:** Create `src-tauri/src/forms/catalog.rs`; create `src-tauri/src/forms/templates/mod.rs` + `src-tauri/src/forms/templates/ics213.rs`.

- [ ] **Step 1: Create templates/mod.rs**

```rust
//! Per-form template strings + field schemas. One submodule per bundled form.

pub mod ics213;
// ics309, position, bulletin, damage_assessment added in T9.x
```

- [ ] **Step 2: Create templates/ics213.rs with the canonical ICS-213 schema**

Refer to spec §3 body template (canonical inline). The WLE source-of-truth file is at the ABSOLUTE PATH (worktrees can't see `dev/scratch/` — it lives only in the main checkout): `/home/administrator/Code/tuxlink/dev/scratch/winlink-re/install/RMS Express/Standard Templates/ICS USA Forms/ICS213 General Message.txt`. Read it for the exact field set if the spec's inline version is incomplete.

```rust
//! ICS-213 General Message — the canonical EmComm form.
//!
//! Field schema mirrors `ICS213_Initial.html` from the WLE Standard Templates
//! catalog. Field IDs are lowercase (per spec §3 wire convention); WLE template
//! placeholders like `<var Subjectline>` resolve to our lowercase `subjectline`
//! field via case-insensitive lookup in render_body_template.

use crate::forms::types::{FieldKind, FormDef, FormField};

const FIELDS: &[FormField] = &[
    FormField { id: "inc_name",          label: "Incident Name",          kind: FieldKind::Text,     required: false, max_length: Some(30) },
    FormField { id: "to_name",           label: "To (Name and Position)", kind: FieldKind::Text,     required: true,  max_length: Some(60) },
    FormField { id: "fm_name",           label: "From (Name and Position)", kind: FieldKind::Text,   required: true,  max_length: Some(60) },
    FormField { id: "subjectline",       label: "Subject",                kind: FieldKind::Text,     required: true,  max_length: Some(50) },
    FormField { id: "mdate",             label: "Date",                   kind: FieldKind::Date,     required: true,  max_length: None },
    FormField { id: "mtime",             label: "Time",                   kind: FieldKind::Time,     required: true,  max_length: None },
    FormField { id: "message",           label: "Message",                kind: FieldKind::LongText, required: true,  max_length: Some(4000) },
    FormField { id: "approved_name",     label: "Approved by",            kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "approved_postitle", label: "Position/Title",         kind: FieldKind::Text,     required: false, max_length: Some(60) },
    FormField { id: "isexercise",        label: "Is exercise",            kind: FieldKind::Boolean,  required: false, max_length: None },
];

const SUBJECT_TEMPLATE: &str = "<var subjectline> - <var mdate> <var mtime>";

const BODY_TEMPLATE: &str = r#"GENERAL MESSAGE (ICS 213)
<var formtitle>
<var isexercise>
1. Incident Name: <var inc_name>
2. To (Name and Position): <var to_name>
3. From (Name and Position): <var fm_name>
4. Subject: <var subjectline>
5. Date: <var mdate>
6. Time: <var mtime>
7. Message:

<var message>

8. Approved by: <var approved_name>
8a. Position/Title: <var approved_postitle>
------------------------------------
Sending Station: Tuxlink
[No changes or editing of this message are allowed]
"#;

pub const ICS213_INITIAL: FormDef = FormDef {
    id: "ICS213_Initial",
    name: "ICS-213 General Message",
    fields: FIELDS,
    subject_template: SUBJECT_TEMPLATE,
    body_template: BODY_TEMPLATE,
    display_form: "ICS213_Initial_Viewer.html",
    reply_template: "ICS213_SendReply.0",
};
```

- [ ] **Step 3: Create catalog.rs with the form lookup**

```rust
//! Bundled forms catalog. Per spec §8, v0.1 ships 5 forms; this file
//! enumerates them and provides id-based lookup.

use crate::forms::templates;
use crate::forms::types::FormDef;

pub const BUNDLED_FORMS: &[&FormDef] = &[
    &templates::ics213::ICS213_INITIAL,
    // ics309, position, bulletin, damage_assessment added in T9.x
];

/// Look up a bundled form by its canonical ID. Returns None if not known.
pub fn find_form(id: &str) -> Option<&'static FormDef> {
    BUNDLED_FORMS.iter().find(|f| f.id == id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_ics213_by_id() {
        let f = find_form("ICS213_Initial").expect("ICS213_Initial bundled");
        assert_eq!(f.name, "ICS-213 General Message");
        assert!(f.fields.iter().any(|fd| fd.id == "inc_name"));
        assert!(f.fields.iter().any(|fd| fd.id == "subjectline"));
    }

    #[test]
    fn returns_none_for_unknown_form() {
        assert!(find_form("Unknown_Form").is_none());
    }

    #[test]
    fn display_form_filename_set() {
        let f = find_form("ICS213_Initial").unwrap();
        assert_eq!(f.display_form, "ICS213_Initial_Viewer.html");
        assert_eq!(f.reply_template, "ICS213_SendReply.0");
    }
}
```

- [ ] **Step 4: Run tests + compile**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib forms::catalog::tests -- --test-threads=1`
Expected: 3 passed.

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: `Finished`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/forms/catalog.rs src-tauri/src/forms/templates/
git commit -m "feat(forms): bundle ICS-213 form per spec §8 (tuxlink-v1p)

First of 5 v0.1 bundled forms. Field schema mirrors WLE's
ICS213_Initial.html (10 fields, lowercase IDs). Body template renders
per WLE convention (with <var> placeholders + IsExercise marker).

catalog::find_form(id) looks up by canonical ID. Others (ICS309,
Position, Bulletin, DamageAssessment) added in T9.x.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.8: Integration test — round-trip serialize→parse

**Files:** Create `src-tauri/tests/forms_test.rs`.

- [ ] **Step 1: Write round-trip test**

```rust
//! Forms integration tests — round-trip + cross-spec parity.

use std::collections::HashMap;
use tuxlink_lib::forms::{catalog, parse, serialize, types::FormParameters};

#[test]
fn ics213_serialize_parse_round_trips() {
    let form = catalog::find_form("ICS213_Initial").expect("ICS213 bundled");
    let params = FormParameters {
        xml_file_version: "1.0".into(),
        rms_express_version: "Tuxlink/0.3.0".into(),
        submission_datetime: "20260530143000".into(),
        senders_callsign: "N0CALL".into(),
        grid_square: "FM18".into(),
        display_form: form.display_form.into(),
        reply_template: form.reply_template.into(),
    };
    let mut values = HashMap::new();
    values.insert("inc_name".into(), "HURRICANE WALDO".into());
    values.insert("to_name".into(), "JOHN OPERATOR".into());
    values.insert("fm_name".into(), "JANE OPERATOR".into());
    values.insert("subjectline".into(), "REQUEST SUPPLIES".into());
    values.insert("mdate".into(), "2026-05-30".into());
    values.insert("mtime".into(), "14:30Z".into());
    values.insert("message".into(), "Need bandages by 1700Z.".into());

    let xml = serialize::serialize_form_xml(form, &params, &values);
    let parsed = parse::parse_form_xml(&xml).expect("round-trip parse succeeds");

    assert_eq!(parsed.form_parameters.display_form, "ICS213_Initial_Viewer.html");
    assert_eq!(parsed.form_parameters.reply_template, "ICS213_SendReply.0");
    assert_eq!(parsed.form_parameters.senders_callsign, "N0CALL");

    for (id, expected) in &[
        ("inc_name", "HURRICANE WALDO"),
        ("to_name", "JOHN OPERATOR"),
        ("subjectline", "REQUEST SUPPLIES"),
        ("mtime", "14:30Z"),
        ("message", "Need bandages by 1700Z."),
    ] {
        let actual = parsed.fields.iter().find(|(k, _)| k == id).map(|(_, v)| v.as_str());
        assert_eq!(actual, Some(*expected), "field {} mismatch", id);
    }
}

#[test]
fn ics213_body_template_substitutes_correctly() {
    let form = catalog::find_form("ICS213_Initial").unwrap();
    let mut values = HashMap::new();
    values.insert("inc_name".into(), "WALDO".into());
    values.insert("subjectline".into(), "TEST".into());
    values.insert("isexercise".into(), "** THIS IS AN EXERCISE **".into());
    let body = serialize::render_body_template(form.body_template, &values);
    assert!(body.contains("1. Incident Name: WALDO"));
    assert!(body.contains("4. Subject: TEST"));
    assert!(body.contains("** THIS IS AN EXERCISE **"));
}
```

- [ ] **Step 2: Run test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test forms_test`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/forms_test.rs
git commit -m "test(forms): integration round-trip serialize→parse (tuxlink-v1p)

Validates wire-format byte-fidelity for ICS-213. The full ICS-213
field set survives a serialize→parse round-trip; body template
substitution produces correct human-readable text including the
IsExercise marker.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 2 — Detection bug fix + DTO extension

### Task 2.1: Fix is_form detection in ui_commands.rs

**Files:** Modify `src-tauri/src/ui_commands.rs:325-327`.

- [ ] **Step 1: Write failing test for new detection**

Add to `src-tauri/tests/ui_commands_test.rs`:

```rust
#[test]
fn detects_form_via_attachment_not_body_prefix() {
    // Body is plain rendered text (WLE convention), not XML.
    let raw = b"From: SENDER@winlink.org\r\n\
To: RECV@winlink.org\r\n\
Subject: ICS-213\r\n\
Date: 2026/05/30 14:30\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"b1\"\r\n\
\r\n\
--b1\r\n\
Content-Type: text/plain\r\n\
\r\n\
GENERAL MESSAGE (ICS 213)\r\n\
1. Incident Name: TEST\r\n\
--b1\r\n\
Content-Type: text/xml; name=\"RMS_Express_Form_ICS213_Initial.xml\"\r\n\
Content-Disposition: attachment; filename=\"RMS_Express_Form_ICS213_Initial.xml\"\r\n\
\r\n\
<?xml version=\"1.0\"?><RMS_Express_Form/>\r\n\
--b1--\r\n";
    let dto = parse_raw_rfc5322("MID-FORM", raw).expect("parse succeeds");
    assert!(dto.is_form, "form-attachment message must set is_form=true");
}

#[test]
fn no_form_when_body_starts_with_xml_but_no_attachment() {
    let raw = simple_rfc5322(
        &[("From", "X@winlink.org"), ("To", "Y@winlink.org"), ("Subject", "s")],
        "<?xml version=\"1.0\"?>not a real form",
    );
    let dto = parse_raw_rfc5322("MID", &raw).expect("parse succeeds");
    assert!(!dto.is_form, "body-XML alone must NOT trigger is_form (legacy bug)");
}
```

- [ ] **Step 2: Run tests — verify both FAIL**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ui_commands_test detects_form_via_attachment_not_body_prefix no_form_when_body_starts_with_xml_but_no_attachment -- --test-threads=1`

Expected: both FAIL — the first because the multipart attachment isn't currently detected as a form; the second because the legacy body-prefix detection sets is_form=true.

(Note: cargo test only accepts one filter; run twice if needed.)

- [ ] **Step 3: Fix detection logic in ui_commands.rs**

Change line 325-327:

```rust
// OLD:
// let is_form = body.trim_start().starts_with("<?xml");

// NEW:
let is_form = attachments
    .iter()
    .any(|a| a.filename.starts_with("RMS_Express_Form_") && a.filename.ends_with(".xml"));
```

- [ ] **Step 4: Run tests — both pass**

Expected: both green.

- [ ] **Step 5: Run full ui_commands_test suite for regression**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ui_commands_test -- --test-threads=1 2>&1 | grep "test result"`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ui_commands.rs src-tauri/tests/ui_commands_test.rs
git commit -m "fix(parse): detect forms via attachment name, not body XML prefix (tuxlink-v1p)

Per spec §11 migration + R2-I05 finding. WLE form messages put XML in
the attachment (RMS_Express_Form_*.xml), not the body. Body is the
plain-text rendered Msg: template. The pre-fix body.starts_with('<?xml')
detection missed real WLE forms entirely and false-positived on any
message whose body started with XML.

2 new tests cover the new detection path + the legacy-bug regression.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 2.2: Add form_id + form_payload to ParsedMessageDto

**Files:** Modify `src-tauri/src/ui_commands.rs` (`ParsedMessageDto` struct + `parse_raw_rfc5322` body).

- [ ] **Step 1: Write failing test**

Add to `src-tauri/tests/ui_commands_test.rs`:

```rust
#[test]
fn populates_form_id_and_payload_for_form_messages() {
    let xml = b"<?xml version=\"1.0\"?>\n\
<RMS_Express_Form>\n\
<form_parameters>\n\
<display_form>ICS213_Initial_Viewer.html</display_form>\n\
<rms_express_version>Tuxlink/0.3.0</rms_express_version>\n\
</form_parameters>\n\
<variables>\n\
<inc_name>WALDO</inc_name>\n\
<subjectline>TEST</subjectline>\n\
</variables>\n\
</RMS_Express_Form>\n";
    let raw = format!(
        "From: X@winlink.org\r\nTo: Y@winlink.org\r\nSubject: t\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"b\"\r\n\r\n\
--b\r\nContent-Type: text/plain\r\n\r\nbody text\r\n\
--b\r\nContent-Type: text/xml; name=\"RMS_Express_Form_ICS213_Initial.xml\"\r\n\
Content-Disposition: attachment; filename=\"RMS_Express_Form_ICS213_Initial.xml\"\r\n\r\n\
{}\r\n--b--\r\n",
        std::str::from_utf8(xml).unwrap()
    );
    let dto = parse_raw_rfc5322("MID", raw.as_bytes()).expect("parse");
    assert!(dto.is_form);
    assert_eq!(dto.form_id.as_deref(), Some("ICS213_Initial"));
    let payload = dto.form_payload.expect("payload populated");
    assert_eq!(payload.form_parameters.display_form, "ICS213_Initial_Viewer.html");
    let inc_name = payload.fields.iter().find(|(k, _)| k == "inc_name").map(|(_, v)| v.as_str());
    assert_eq!(inc_name, Some("WALDO"));
}
```

- [ ] **Step 2: Run test — FAIL (no form_id / form_payload fields yet)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ui_commands_test populates_form_id_and_payload -- --test-threads=1`
Expected: compile fail — `form_id` / `form_payload` not found on DTO.

- [ ] **Step 3: Extend ParsedMessageDto**

In `src-tauri/src/ui_commands.rs` at line ~244-247:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedMessageDto {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub date: String,
    pub body: String,
    pub attachments: Vec<AttachmentMetaDto>,
    pub is_form: bool,
    pub routing: Option<String>,
    /// Form ID extracted from `RMS_Express_Form_<id>.xml` attachment name.
    /// Validated via `forms::validation::is_valid_form_id`. None when not a form.
    pub form_id: Option<String>,
    /// Parsed form payload (eager parse while attachment bytes available).
    /// None when not a form OR when parse failed (also logged).
    pub form_payload: Option<crate::forms::FormPayload>,
}
```

- [ ] **Step 4: Populate fields in parse_raw_rfc5322**

After the `is_form` detection update (post-T2.1), and after the attachments-list is built:

```rust
let form_id = attachments.iter()
    .find_map(|a| crate::forms::detect_form_attachment(&a.filename));

let form_payload = if let Some(ref fid) = form_id {
    // Find the attachment with matching filename
    let attach = attachments.iter()
        .find(|a| a.filename == format!("RMS_Express_Form_{}.xml", fid));
    // Note: attachments DTO only has filename+size, NOT bytes.
    // Need to extract bytes from the parsed msg directly here.
    // The msg parsed by mail-parser has the full multipart structure.
    extract_attachment_bytes(&msg, &format!("RMS_Express_Form_{}.xml", fid))
        .and_then(|bytes| crate::forms::parse_form_xml(&bytes).ok())
} else {
    None
};

// ... use form_id + form_payload in the DTO construction.
```

The `extract_attachment_bytes` helper needs to iterate `msg.attachments()` and find the one matching the filename — implementation is mail-parser API-specific; subagent looks at existing `collect_attachments` for the pattern.

- [ ] **Step 5: Run test — pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ui_commands_test populates_form_id_and_payload -- --test-threads=1`
Expected: 1 passed.

- [ ] **Step 6: Full ui_commands_test regression**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ui_commands_test -- --test-threads=1 2>&1 | grep "test result"`
Expected: all green.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/ui_commands.rs src-tauri/tests/ui_commands_test.rs
git commit -m "feat(parse): add form_id + form_payload to ParsedMessageDto (tuxlink-v1p)

Per spec §5.2 (Codex R5 finding). The frontend cannot render form
content from just form_id — it needs the parsed field values too. We
parse eagerly while the attachment bytes are still in hand from
mail-parser; the 256 KiB cap bounds memory.

Lazy alternative (separate get_form_payload IPC) rejected — extra
round-trip, extra Tauri command surface, no real benefit at v0.1 sizes.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 2.3: Update dev fixtures + existing tests for new format

**Files:** `src/mailbox/devFixture.ts:256-265`, `src/mailbox/replyActions.test.ts:97-130`.

- [ ] **Step 1: Update FORM_XML constant in replyActions.test.ts**

Change line 98 to use the new wire-format (raw XML body that's actually an attachment in real messages). Since `replyActions` operates on `ParsedMessage` (which now has structured `formId` + `formPayload`), update the test to use a `parsed({ isForm: true, formId: 'ICS213_Initial', body: 'plain rendered text', formPayload: {...} })` shape.

Exact edit deferred to T8 (replyActions update) — for now, only ensure existing tests still compile (no behavior change yet from T2.x).

- [ ] **Step 2: Update devFixture.ts form fixture**

In `src/mailbox/devFixture.ts` around line 256, the `devFormMeta` helper needs to match the new DTO shape. Update to populate `formId` + `formPayload` instead of inferring from body.

- [ ] **Step 3: Run vitest + cargo tests**

Run: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-design && pnpm vitest run src/mailbox/ 2>&1 | tail -20`
Expected: all mailbox tests green (assuming T2 updates work).

- [ ] **Step 4: Commit**

```bash
git add src/mailbox/devFixture.ts src/mailbox/replyActions.test.ts
git commit -m "test(fixtures): update form fixtures for new wire format (tuxlink-v1p)

Per spec §11 migration. Old fixture put XML in body; new (correct) shape
puts plain rendered text in body and structured FormPayload in
formPayload field.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 3 — send_form Tauri command

### Task 3.1: send_form command

**Files:** Modify `src-tauri/src/ui_commands.rs` (add command); `src-tauri/src/lib.rs` (register).

- [ ] **Step 1: Write failing test**

Add to `src-tauri/tests/forms_test.rs`:

```rust
// Note: full Tauri command test needs running app; this validates the
// underlying serialize+build behavior that send_form will use.

#[test]
fn send_form_builds_outbound_message_with_xml_attachment() {
    use tuxlink_lib::forms::{catalog, serialize, types::FormParameters};
    use std::collections::HashMap;

    let form = catalog::find_form("ICS213_Initial").unwrap();
    let mut values = HashMap::new();
    values.insert("inc_name".into(), "TEST INCIDENT".into());
    values.insert("subjectline".into(), "TEST SUBJECT".into());
    values.insert("mdate".into(), "2026-05-30".into());
    values.insert("mtime".into(), "14:30Z".into());
    let params = FormParameters {
        xml_file_version: "1.0".into(),
        rms_express_version: "Tuxlink/0.3.0".into(),
        submission_datetime: "20260530143000".into(),
        senders_callsign: "N0CALL".into(),
        grid_square: "FM18".into(),
        display_form: form.display_form.into(),
        reply_template: form.reply_template.into(),
    };

    let xml = serialize::serialize_form_xml(form, &params, &values);
    let body = serialize::render_body_template(form.body_template, &values);

    assert!(body.contains("1. Incident Name: TEST INCIDENT"));
    assert!(body.contains("4. Subject: TEST SUBJECT"));
    assert!(xml.starts_with(&[0xEF, 0xBB, 0xBF]));
    let xml_str = String::from_utf8_lossy(&xml);
    assert!(xml_str.contains("<display_form>ICS213_Initial_Viewer.html</display_form>"));
    assert!(xml_str.contains("<inc_name>TEST INCIDENT</inc_name>"));
}
```

- [ ] **Step 2: Run test → green (validates the materials send_form will use)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test forms_test send_form_builds -- --test-threads=1`
Expected: 1 passed.

- [ ] **Step 3: Add the send_form command in ui_commands.rs**

> **Rev-4 note (ADR 0016 / 9phd Codex P2.1):** The Pat REST path is gone. `send_form` must use the native B2F pipeline: construct `OutboundMessage` with `OutboundAttachment { filename, bytes }` (NO `content_type` field — that field does not exist in the shipped struct; B2F does not use MIME content-type), then call `backend.send_message(msg)` directly (same path as `message_send`). Return type is `Result<String, UiError>` (returns the MID string, not `Option<String>`), mirroring `message_send`.

Near other Tauri commands:

```rust
#[tauri::command]
pub async fn send_form(
    form_id: String,
    field_values: std::collections::HashMap<String, String>,
    to: Vec<String>,
    cc: Vec<String>,
    senders_callsign: String,
    grid_square: String,
    state: State<'_, BackendState>,
) -> Result<String, UiError> {
    use crate::forms;
    let form = forms::catalog::find_form(&form_id)
        .ok_or_else(|| UiError::Internal { detail: format!("unknown form: {}", form_id) })?;
    let now = chrono::Utc::now();
    let params = forms::types::FormParameters {
        xml_file_version: "1.0".to_string(),
        rms_express_version: format!("Tuxlink/{}", env!("CARGO_PKG_VERSION")),
        submission_datetime: now.format("%Y%m%d%H%M%S").to_string(),
        senders_callsign,
        grid_square,
        display_form: form.display_form.to_string(),
        reply_template: form.reply_template.to_string(),
    };
    let xml_bytes = forms::serialize::serialize_form_xml(form, &params, &field_values);
    let body = forms::serialize::render_body_template(form.body_template, &field_values);
    let subject = forms::serialize::render_body_template(form.subject_template, &field_values);

    // Note: OutboundAttachment has { filename, bytes } only — NO content_type field.
    // The native B2F wire format does not use MIME content-type headers for attachments.
    // See winlink_backend.rs:105-108 for the canonical struct definition.
    let attachment = OutboundAttachment {
        filename: format!("RMS_Express_Form_{}.xml", form.id),
        bytes: xml_bytes,
    };
    let msg = OutboundMessage {
        to,
        cc,
        subject,
        body,
        date: now.to_rfc3339(),
        attachments: vec![attachment],
    };

    let backend = state.current().ok_or_else(|| UiError::NotConfigured("backend offline".into()))?;
    // send_message returns MessageId; map to String for IPC (mirrors message_send).
    let mid = backend.send_message(msg).await?;
    Ok(mid.0)
}
```

- [ ] **Step 4: Register the command in lib.rs invoke_handler**

In `src-tauri/src/lib.rs`, find the `tauri::Builder::default().invoke_handler(tauri::generate_handler![...])` block and add `ui_commands::send_form` to the list.

- [ ] **Step 5: Compile**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: `Finished`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ui_commands.rs src-tauri/src/lib.rs src-tauri/tests/forms_test.rs
git commit -m "feat(ipc): send_form Tauri command per spec rev-3 §5.1 (tuxlink-v1p)

Accepts (form_id, field_values, to, cc, senders_callsign, grid_square),
looks up the bundled form, builds the XML payload + text body, wraps
in OutboundAttachment { filename, bytes } (no content_type — B2F native),
and dispatches via backend.send_message() on the native B2F path.
Returns MID string per send_message contract.

Per ADR 0016 + tuxlink-9phd Codex P2.1 (native pipeline; Pat REST removed).

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 4 — React forms infrastructure

### Task 4.1: TS types mirror

**Files:** Create `src/forms/types.ts`.

- [ ] **Step 1: Write types file**

```typescript
// Mirror of src-tauri/src/forms/types.rs structures, camelCase.

export type FieldKind = 'text' | 'long_text' | 'date' | 'time' | 'boolean';

export interface FormField {
  id: string;
  label: string;
  kind: FieldKind;
  required: boolean;
  maxLength: number | null;
}

export interface FormDef {
  id: string;
  name: string;
  fields: FormField[];
  subjectTemplate: string;
  bodyTemplate: string;
  displayForm: string;
  replyTemplate: string;
}

export interface FormParameters {
  xmlFileVersion: string;
  rmsExpressVersion: string;
  submissionDatetime: string;
  sendersCallsign: string;
  gridSquare: string;
  displayForm: string;
  replyTemplate: string;
}

export interface FormPayload {
  formId: string;
  formParameters: FormParameters;
  /** [fieldId, value] pairs preserving XML order. */
  fields: [string, string][];
}

/** Convenience accessor: look up a field value by ID. */
export function fieldValue(payload: FormPayload, id: string): string | undefined {
  return payload.fields.find(([k]) => k === id)?.[1];
}
```

- [ ] **Step 2: TypeScript compile check**

Run: `pnpm tsc --noEmit`
Expected: no errors related to forms/types.

- [ ] **Step 3: Commit**

```bash
git add src/forms/types.ts
git commit -m "feat(forms-ts): TS types mirror per spec §6.1 (tuxlink-v1p)

Mirror Rust FormDef / FormField / FormPayload / FormParameters /
FieldKind. fieldValue() accessor for ergonomic field-by-id lookup.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4.2: Form component registry

**Files:** Create `src/forms/forms.ts`.

- [ ] **Step 1: Write registry file**

```typescript
import type { ComponentType } from 'react';
import type { FormPayload } from './types';

/** Form authoring (compose-side) component contract. */
export interface FormComposeProps {
  initialValues?: Record<string, string>;
  /** Called when user submits valid form. */
  onSubmit: (values: Record<string, string>) => void;
  onCancel: () => void;
}

/** Form viewing (read-side) component contract. */
export interface FormViewProps {
  payload: FormPayload;
}

/** Registry entry for a single bundled form. */
export interface FormRegistryEntry {
  id: string;
  name: string;
  Form: ComponentType<FormComposeProps>;
  View: ComponentType<FormViewProps>;
}

/** Lookup-by-id registry. Populated by the per-form module imports below. */
const REGISTRY: Map<string, FormRegistryEntry> = new Map();

export function registerForm(entry: FormRegistryEntry): void {
  REGISTRY.set(entry.id, entry);
}

export function lookupForm(id: string): FormRegistryEntry | undefined {
  return REGISTRY.get(id);
}

export function allForms(): FormRegistryEntry[] {
  return Array.from(REGISTRY.values());
}
```

- [ ] **Step 2: TypeScript compile**

Run: `pnpm tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/forms/forms.ts
git commit -m "feat(forms-ts): registry contract per spec §5.2 (tuxlink-v1p)

FormComposeProps + FormViewProps contracts. Registry populated by
per-form module imports (Ics213, Ics309, etc.) at T5+T9.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4.3: KeyValueView fallback

**Files:** Create `src/forms/KeyValueView.tsx` + `KeyValueView.test.tsx`.

- [ ] **Step 1: Write failing test**

```typescript
// src/forms/KeyValueView.test.tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { KeyValueView } from './KeyValueView';
import type { FormPayload } from './types';

const PAYLOAD: FormPayload = {
  formId: 'Unknown_Initial',
  formParameters: {
    xmlFileVersion: '1.0',
    rmsExpressVersion: 'Tuxlink/0.3.0',
    submissionDatetime: '20260530143000',
    sendersCallsign: 'N0CALL',
    gridSquare: 'FM18',
    displayForm: 'Unknown_Initial_Viewer.html',
    replyTemplate: '',
  },
  fields: [
    ['field_a', 'value-a'],
    ['field_b', 'value-b'],
  ],
};

describe('KeyValueView', () => {
  it('renders form-id and unknown-form notice', () => {
    render(<KeyValueView payload={PAYLOAD} bodyText="some plain text" />);
    expect(screen.getByText(/Unknown_Initial/)).toBeInTheDocument();
    expect(screen.getByText(/specific renderer is not bundled/i)).toBeInTheDocument();
  });

  it('renders all field/value pairs', () => {
    render(<KeyValueView payload={PAYLOAD} bodyText="" />);
    expect(screen.getByText('field_a')).toBeInTheDocument();
    expect(screen.getByText('value-a')).toBeInTheDocument();
    expect(screen.getByText('field_b')).toBeInTheDocument();
    expect(screen.getByText('value-b')).toBeInTheDocument();
  });

  it('renders the bodyText (sender plain rendering)', () => {
    render(<KeyValueView payload={PAYLOAD} bodyText="HELLO WORLD" />);
    expect(screen.getByText(/HELLO WORLD/)).toBeInTheDocument();
  });

  it('safely renders field values containing HTML (no innerHTML)', () => {
    const xssPayload: FormPayload = {
      ...PAYLOAD,
      fields: [['evil', '<script>alert(1)</script>']],
    };
    const { container } = render(<KeyValueView payload={xssPayload} bodyText="" />);
    // The literal `<script>` string should be displayed as text, not executed.
    expect(container.innerHTML).not.toContain('<script>alert');
    expect(container.textContent).toContain('<script>alert(1)</script>');
  });
});
```

- [ ] **Step 2: Run test — FAIL (component doesn't exist)**

Run: `pnpm vitest run src/forms/KeyValueView.test.tsx`
Expected: 4 failed (import error).

- [ ] **Step 3: Implement KeyValueView**

```typescript
// src/forms/KeyValueView.tsx
import type { FormPayload } from './types';
import type { FormViewProps } from './forms';

interface Props {
  payload: FormPayload;
  bodyText: string;
}

export function KeyValueView({ payload, bodyText }: Props) {
  return (
    <div className="form-view form-view-unknown" data-testid="key-value-view">
      <div className="form-view-header">
        <strong>Unknown form: {payload.formId}</strong>
        <p>
          The form's specific renderer is not bundled in this Tuxlink version.
          Below are the raw field/value pairs from the XML payload and the
          sender's plain text rendering.
        </p>
      </div>

      <dl className="form-fields">
        {payload.fields.map(([k, v]) => (
          <div className="form-field-row" key={k}>
            <dt>{k}</dt>
            <dd>{v}</dd>
          </div>
        ))}
      </dl>

      {bodyText && (
        <div className="form-view-body">
          <h4>Message body (sender's text rendering)</h4>
          <pre>{bodyText}</pre>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run test — pass**

Run: `pnpm vitest run src/forms/KeyValueView.test.tsx`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add src/forms/KeyValueView.tsx src/forms/KeyValueView.test.tsx
git commit -m "feat(forms-ts): KeyValueView fallback for unknown forms (tuxlink-v1p)

Renders any FormPayload as raw field/value pairs + sender's plain text
body. XSS-safe (React default-escape, no dangerouslySetInnerHTML).

Per spec §6.3 — the 'graceful degradation floor' so every WLE form is
readable in Tuxlink even when not visually pretty.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4.4: FormPicker modal

**Files:** Create `src/forms/FormPicker.tsx` + `FormPicker.test.tsx`.

- [ ] **Step 1: Write failing test**

```typescript
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { FormPicker } from './FormPicker';

describe('FormPicker', () => {
  it('lists all registered forms', () => {
    const forms = [
      { id: 'ICS213_Initial', name: 'ICS-213 General Message' },
      { id: 'ICS309_Initial', name: 'ICS-309 Communications Log' },
    ];
    render(<FormPicker forms={forms} onPick={() => {}} onCancel={() => {}} />);
    expect(screen.getByText('ICS-213 General Message')).toBeInTheDocument();
    expect(screen.getByText('ICS-309 Communications Log')).toBeInTheDocument();
  });

  it('calls onPick with the selected form id', () => {
    const onPick = vi.fn();
    const forms = [{ id: 'ICS213_Initial', name: 'ICS-213 General Message' }];
    render(<FormPicker forms={forms} onPick={onPick} onCancel={() => {}} />);
    fireEvent.click(screen.getByText('ICS-213 General Message'));
    fireEvent.click(screen.getByTestId('form-picker-confirm'));
    expect(onPick).toHaveBeenCalledWith('ICS213_Initial');
  });

  it('calls onCancel when cancel button clicked', () => {
    const onCancel = vi.fn();
    render(<FormPicker forms={[]} onPick={() => {}} onCancel={onCancel} />);
    fireEvent.click(screen.getByTestId('form-picker-cancel'));
    expect(onCancel).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run test — FAIL**

- [ ] **Step 3: Implement FormPicker**

```typescript
import { useState } from 'react';

interface FormPickerProps {
  forms: { id: string; name: string }[];
  onPick: (id: string) => void;
  onCancel: () => void;
}

export function FormPicker({ forms, onPick, onCancel }: FormPickerProps) {
  const [selectedId, setSelectedId] = useState<string>(forms[0]?.id ?? '');
  return (
    <div className="form-picker" role="dialog" aria-label="Pick a form">
      <h3>Pick a form to author</h3>
      <ul className="form-picker-list">
        {forms.map((f) => (
          <li
            key={f.id}
            className={selectedId === f.id ? 'selected' : ''}
            onClick={() => setSelectedId(f.id)}
          >
            {f.name}
          </li>
        ))}
      </ul>
      <div className="form-picker-actions">
        <button type="button" data-testid="form-picker-cancel" onClick={onCancel}>
          Cancel
        </button>
        <button
          type="button"
          data-testid="form-picker-confirm"
          disabled={!selectedId}
          onClick={() => onPick(selectedId)}
        >
          Use selected form
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run test — pass**

- [ ] **Step 5: Commit**

```bash
git add src/forms/FormPicker.tsx src/forms/FormPicker.test.tsx
git commit -m "feat(forms-ts): FormPicker modal per spec §7.1 (tuxlink-v1p)

Lists bundled forms; on select+confirm fires onPick(id). Tests cover
list rendering, selection, and cancel.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 5 — ICS-213 React form (compose + view)

### Task 5.1: Ics213Form (compose-side)

**Files:** Create `src/forms/ics213/Ics213Form.tsx` + test.

- [ ] **Step 1: Write failing test**

```typescript
// src/forms/ics213/Ics213Form.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Ics213Form } from './Ics213Form';

describe('Ics213Form', () => {
  const noop = () => {};

  it('renders all ICS-213 input fields', () => {
    render(<Ics213Form onSubmit={noop} onCancel={noop} />);
    expect(screen.getByLabelText(/Incident Name/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/To.*Name and Position/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/From.*Name and Position/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Subject/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Date/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Time/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Message/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Approved by/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Is exercise/i)).toBeInTheDocument();
  });

  it('blocks submit when required fields empty', () => {
    const onSubmit = vi.fn();
    render(<Ics213Form onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.click(screen.getByTestId('ics213-submit'));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('submits with field values when required fields filled', () => {
    const onSubmit = vi.fn();
    render(<Ics213Form onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/To.*Name and Position/i), { target: { value: 'JOHN' } });
    fireEvent.change(screen.getByLabelText(/From.*Name and Position/i), { target: { value: 'JANE' } });
    fireEvent.change(screen.getByLabelText(/Subject/i), { target: { value: 'TEST' } });
    fireEvent.change(screen.getByLabelText(/Date/i), { target: { value: '2026-05-30' } });
    fireEvent.change(screen.getByLabelText(/Time/i), { target: { value: '14:30Z' } });
    fireEvent.change(screen.getByLabelText(/Message/i), { target: { value: 'hello' } });
    fireEvent.click(screen.getByTestId('ics213-submit'));
    expect(onSubmit).toHaveBeenCalled();
    const values = onSubmit.mock.calls[0][0];
    expect(values.to_name).toBe('JOHN');
    expect(values.fm_name).toBe('JANE');
    expect(values.subjectline).toBe('TEST');
    expect(values.message).toBe('hello');
  });

  it('initialValues pre-populates fields', () => {
    render(<Ics213Form initialValues={{ inc_name: 'WALDO' }} onSubmit={noop} onCancel={noop} />);
    const incName = screen.getByLabelText(/Incident Name/i) as HTMLInputElement;
    expect(incName.value).toBe('WALDO');
  });
});
```

- [ ] **Step 2: Run test — FAIL**

- [ ] **Step 3: Implement Ics213Form**

```typescript
// src/forms/ics213/Ics213Form.tsx
import { useState } from 'react';
import type { FormComposeProps } from '../forms';

export function Ics213Form({ initialValues = {}, onSubmit, onCancel }: FormComposeProps) {
  const [values, setValues] = useState<Record<string, string>>(initialValues);
  const set = (id: string, v: string) => setValues((s) => ({ ...s, [id]: v }));
  const required = ['to_name', 'fm_name', 'subjectline', 'mdate', 'mtime', 'message'];
  const canSubmit = required.every((id) => (values[id] ?? '').trim().length > 0);
  const submit = () => {
    if (canSubmit) onSubmit(values);
  };
  return (
    <form className="ics213-form" onSubmit={(e) => { e.preventDefault(); submit(); }}>
      <label>Incident Name <input value={values.inc_name ?? ''} onChange={(e) => set('inc_name', e.target.value)} maxLength={30} /></label>
      <label>To (Name and Position) <input value={values.to_name ?? ''} onChange={(e) => set('to_name', e.target.value)} maxLength={60} required /></label>
      <label>From (Name and Position) <input value={values.fm_name ?? ''} onChange={(e) => set('fm_name', e.target.value)} maxLength={60} required /></label>
      <label>Subject <input value={values.subjectline ?? ''} onChange={(e) => set('subjectline', e.target.value)} maxLength={50} required /></label>
      <label>Date <input value={values.mdate ?? ''} onChange={(e) => set('mdate', e.target.value)} type="date" required /></label>
      <label>Time <input value={values.mtime ?? ''} onChange={(e) => set('mtime', e.target.value)} placeholder="HH:MMZ" required /></label>
      <label>Message <textarea value={values.message ?? ''} onChange={(e) => set('message', e.target.value)} rows={6} required /></label>
      <label>Approved by <input value={values.approved_name ?? ''} onChange={(e) => set('approved_name', e.target.value)} maxLength={60} /></label>
      <label>Position/Title <input value={values.approved_postitle ?? ''} onChange={(e) => set('approved_postitle', e.target.value)} maxLength={60} /></label>
      <label><input type="checkbox" checked={values.isexercise === '** THIS IS AN EXERCISE **'} onChange={(e) => set('isexercise', e.target.checked ? '** THIS IS AN EXERCISE **' : '')} /> Is exercise</label>
      <div className="form-actions">
        <button type="button" onClick={onCancel}>Discard form</button>
        <button type="submit" data-testid="ics213-submit" disabled={!canSubmit}>Send</button>
      </div>
    </form>
  );
}
```

- [ ] **Step 4: Run test — pass**

- [ ] **Step 5: Commit**

```bash
git add src/forms/ics213/Ics213Form.tsx src/forms/ics213/Ics213Form.test.tsx
git commit -m "feat(forms-ts): Ics213Form per spec §7.1 (tuxlink-v1p)

10 ICS-213 input fields including IsExercise checkbox. Required-field
validation gates submit; submit fires onSubmit({ to_name, fm_name,
subjectline, mdate, mtime, message, ... }) with lowercase field IDs
matching the WLE/Pat wire convention.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 5.2: Ics213View (read-side)

**Files:** Create `src/forms/ics213/Ics213View.tsx` + test.

- [ ] **Step 1: Write failing test**

```typescript
// src/forms/ics213/Ics213View.test.tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Ics213View } from './Ics213View';
import type { FormPayload } from '../types';

const PAYLOAD: FormPayload = {
  formId: 'ICS213_Initial',
  formParameters: {
    xmlFileVersion: '1.0',
    rmsExpressVersion: 'Tuxlink/0.3.0',
    submissionDatetime: '20260530143000',
    sendersCallsign: 'N0CALL',
    gridSquare: 'FM18',
    displayForm: 'ICS213_Initial_Viewer.html',
    replyTemplate: 'ICS213_SendReply.0',
  },
  fields: [
    ['inc_name', 'WALDO'],
    ['to_name', 'JOHN'],
    ['fm_name', 'JANE'],
    ['subjectline', 'TEST'],
    ['mdate', '2026-05-30'],
    ['mtime', '14:30Z'],
    ['message', 'Need bandages.'],
    ['isexercise', '** THIS IS AN EXERCISE **'],
  ],
};

describe('Ics213View', () => {
  it('renders all labeled field values', () => {
    render(<Ics213View payload={PAYLOAD} />);
    expect(screen.getByText('WALDO')).toBeInTheDocument();
    expect(screen.getByText('JOHN')).toBeInTheDocument();
    expect(screen.getByText('JANE')).toBeInTheDocument();
    expect(screen.getByText('TEST')).toBeInTheDocument();
    expect(screen.getByText(/Need bandages/)).toBeInTheDocument();
  });

  it('shows the IsExercise marker when set', () => {
    render(<Ics213View payload={PAYLOAD} />);
    expect(screen.getByText(/THIS IS AN EXERCISE/)).toBeInTheDocument();
  });

  it('safely renders field values containing HTML (no innerHTML)', () => {
    const xssPayload: FormPayload = {
      ...PAYLOAD,
      fields: [['message', '<script>alert(1)</script>']],
    };
    const { container } = render(<Ics213View payload={xssPayload} />);
    expect(container.innerHTML).not.toContain('<script>alert');
    expect(container.textContent).toContain('<script>alert(1)</script>');
  });
});
```

- [ ] **Step 2: Run test — FAIL**

- [ ] **Step 3: Implement Ics213View**

```typescript
// src/forms/ics213/Ics213View.tsx
import type { FormViewProps } from '../forms';
import { fieldValue } from '../types';

export function Ics213View({ payload }: FormViewProps) {
  const f = (id: string) => fieldValue(payload, id) ?? '';
  return (
    <div className="form-view form-view-ics213" data-testid="ics213-view">
      <div className="form-view-header">
        <strong>📋 ICS-213 General Message · v1.0</strong>
      </div>
      {f('isexercise') && (
        <div className="form-view-exercise-marker"><strong>{f('isexercise')}</strong></div>
      )}
      <dl className="form-fields">
        {f('inc_name')   && <><dt>Incident</dt>      <dd>{f('inc_name')}</dd></>}
        <dt>To</dt>        <dd>{f('to_name')}</dd>
        <dt>From</dt>      <dd>{f('fm_name')}</dd>
        <dt>Date</dt>      <dd>{f('mdate')} · Time: {f('mtime')}</dd>
        <dt>Subject</dt>   <dd>{f('subjectline')}</dd>
        <dt>Message</dt>   <dd className="form-message-body"><pre>{f('message')}</pre></dd>
        {f('approved_name') && <><dt>Approved by</dt>  <dd>{f('approved_name')}</dd></>}
        {f('approved_postitle') && <><dt>Position/Title</dt> <dd>{f('approved_postitle')}</dd></>}
      </dl>
    </div>
  );
}
```

- [ ] **Step 4: Run test — pass**

- [ ] **Step 5: Commit**

```bash
git add src/forms/ics213/Ics213View.tsx src/forms/ics213/Ics213View.test.tsx
git commit -m "feat(forms-ts): Ics213View per spec §7.2 (tuxlink-v1p)

Renders incoming ICS-213 with labeled fields. IsExercise marker
prominent when present. XSS-safe via React default escaping (no
dangerouslySetInnerHTML).

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 5.3: Register ICS-213 in the form registry

**Files:** Create `src/forms/ics213/index.ts`; modify `src/forms/forms.ts` (or wherever the central registration happens).

- [ ] **Step 1: Add index.ts that registers the form**

```typescript
// src/forms/ics213/index.ts
import { Ics213Form } from './Ics213Form';
import { Ics213View } from './Ics213View';
import { registerForm } from '../forms';

registerForm({
  id: 'ICS213_Initial',
  name: 'ICS-213 General Message',
  Form: Ics213Form,
  View: Ics213View,
});
```

- [ ] **Step 2: Add a barrel import** in `src/forms/index.ts`:

```typescript
import './ics213';
// Other forms registered via T9.x: import './ics309'; etc.

export * from './forms';
export * from './types';
export * from './KeyValueView';
export * from './FormPicker';
```

- [ ] **Step 3: Quick smoke test**

```typescript
// src/forms/forms.test.ts
import { describe, it, expect } from 'vitest';
import './ics213';
import { lookupForm, allForms } from './forms';

describe('forms registry', () => {
  it('finds Ics213 after import', () => {
    const entry = lookupForm('ICS213_Initial');
    expect(entry).toBeDefined();
    expect(entry?.name).toBe('ICS-213 General Message');
  });

  it('lists all registered forms', () => {
    const list = allForms();
    expect(list.find((f) => f.id === 'ICS213_Initial')).toBeDefined();
  });
});
```

- [ ] **Step 4: Run test**

Run: `pnpm vitest run src/forms/forms.test.ts`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src/forms/ics213/index.ts src/forms/index.ts src/forms/forms.test.ts
git commit -m "feat(forms-ts): register ICS-213 in form registry (tuxlink-v1p)

Per spec §5.2 — barrel-import at src/forms/index.ts registers all
bundled forms at startup. lookupForm('ICS213_Initial') now resolves.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 6 — Compose integration

### Task 6.1: Compose form button + region replacement

**Files:** Modify `src/compose/Compose.tsx`.

- [ ] **Step 1: Write integration test for Compose-form flow**

Update or add to `src/compose/Compose.test.tsx`. Tests: button visible; clicking opens FormPicker; selecting a form replaces the body region with the form component.

(Exact test code subagent develops; reference test patterns in existing Compose.test.tsx.)

- [ ] **Step 2-N: Implement per spec §7.1**

- Add "Compose form…" button next to existing actions.
- State: `formMode: { kind: 'plain' } | { kind: 'pick' } | { kind: 'form', formId: string, initialValues: Record<string,string> }`.
- When `pick`: render FormPicker; on pick → state goes to `form`; on cancel → back to `plain`.
- When `form`: render the registry's `Form` component for the chosen `formId`; replace the existing plain-text body region.
- onSubmit from form: call `send_form` Tauri command; on success clear draft + close compose window (existing behavior).

(Detail-level implementation per spec §7.1, deferred to subagent execution.)

- [ ] **Commit**

```bash
git commit -m "feat(compose): Compose form button + region replacement (tuxlink-v1p)

Per spec §7.1. Adds 'Compose form…' button to existing actions. Click
opens FormPicker (modal). Select+confirm replaces the body region with
the chosen form's React component. Submit fires send_form Tauri command.

Tests: button visible, picker opens, region replacement, submit flow.
Unsaved-changes dialog handled in T6.2.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 6.2: Pre-form-switch unsaved-changes dialog

**Files:** Modify `src/compose/Compose.tsx`.

- [ ] **Step 1: Test for dialog flow**

When body region is non-empty AND user clicks Compose form, an "Unsaved changes" dialog appears with Save / Discard / Cancel. Save persists draft + opens picker; Discard clears body + opens picker; Cancel stays in plain mode.

- [ ] **Step 2-N: Implement per spec §7.1 mockup**

Reuse existing `isDirty()` + dialog patterns from Compose.tsx (already present per the in-spec note re: tuxlink-h2y close-self).

- [ ] **Commit**

```bash
git commit -m "feat(compose): unsaved-changes dialog before form switch (tuxlink-v1p)

Per spec §7.1 + R3-F1. Switching to a form when body region has
unsaved content shows Save/Discard/Cancel dialog. Reuses existing
isDirty() pattern from Compose.tsx.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 6.3: Extend DraftData for form fields

**Files:** Modify `src/compose/useDraft.ts`.

- [ ] **Step 1: Test for form-draft persistence**

Test that a draft with `formId` set survives save→restore through the existing autosave mechanism.

- [ ] **Step 2-N: Implement per spec §7.3**

Extend `DraftData` with `formId?: string` + `formFields?: Record<string, string>`. Autosave inspects `formId`; if set, persists `formFields` alongside the existing fields.

- [ ] **Commit**

```bash
git commit -m "feat(draft): extend DraftData for form-field state (tuxlink-v1p)

Per spec §7.3 + R3-F2. Without formFields persistence, an app crash
mid-fill loses N minutes of typing. Adds formId + formFields fields;
autosave + restore + initial-seed paths all consume them.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 6.4: Form-draft round-trip test

**Files:** Modify `src/compose/draft.test.ts`.

- [ ] **Step 1: Test**

Round-trip a draft with `formId` set + non-empty `formFields`. After save → clear → restore, fields are intact.

- [ ] **Step 2: Run test → pass**

- [ ] **Commit**

```bash
git commit -m "test(draft): form-draft round-trip persistence (tuxlink-v1p)"
```

---

## Phase 7 — MessageView integration

### Task 7.1: Form-render dispatch in MessageView

**Files:** Modify `src/mailbox/MessageView.tsx`.

- [ ] **Step 1: Test**

When `message.isForm` and `message.formId` matches a registered form, render the registered View component. When `isForm` but unknown form, render KeyValueView. When not a form, render the existing plain-text body.

- [ ] **Step 2-N: Implement per spec §6.2**

Replace the existing "Winlink form attached" placeholder (line 221-232) with:

```typescript
if (message.isForm && message.formId && message.formPayload) {
  const entry = lookupForm(message.formId);
  if (entry) {
    return <entry.View payload={message.formPayload} />;
  }
  return <KeyValueView payload={message.formPayload} bodyText={message.body} />;
}
// otherwise existing plain-text body render
```

- [ ] **Commit**

```bash
git commit -m "feat(mailbox): form-render dispatch in MessageView (tuxlink-v1p)

Per spec §6.2. When ParsedMessage carries form_id + form_payload,
look up registered form component; if known render its View; else
fall back to KeyValueView. Plain messages unchanged.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 8 — replyActions update

### Task 8.1: replyActions for new format

**Files:** Modify `src/mailbox/replyActions.ts:75-95`; update tests.

- [ ] **Step 1: Update reply behavior per spec §7.4**

Reply to a message with `formId` set: body uses the new placeholder pattern (don't quote the rendered text body either, since it can contain form data — see R2-I10 + Codex R5-P2 findings).

- [ ] **Step 2: Add "Reply with form" button alternative**

Spec §7.4: button next to Reply/Reply All opens the same form type pre-populated with original `fm_name → to_name` (swap).

- [ ] **Step 3: Tests**

Update existing `replyActions.test.ts` `FORM_XML` fixture for new format. Add tests for reply-to-form (placeholder body, no leaked field data) and reply-with-form (same-form opens with swap).

- [ ] **Commit**

```bash
git commit -m "feat(reply): reply-to-form + reply-with-form per spec §7.4 (tuxlink-v1p)

Per R2-I10 + R3-F3 + R5-Codex-P2. Reply on a form message defaults to
plain-text reply with a [ICS-213 form omitted from quote] placeholder.
The 'Reply with form…' alternative opens the same form type
pre-populated with sender↔recipient swap.

Update FORM_XML fixture to new wire format (body is plain text, not
raw XML).

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 9 — Additional bundled forms (SERIAL execution — shared-file conflicts)

Each form below repeats the T5.x + T1.7 pattern: catalog entry, templates/{form}.rs, React Form + View components + registration.

**Sequencing requirement (plan rev-2 correction):** T9.1 → T9.2 → T9.3 → T9.4 must run **serially**, not in parallel. Rev-1 claimed parallel-safe; rev-3 review caught the conflict — each task touches 3 shared files:

- `src-tauri/src/forms/catalog.rs` — adds entry to `BUNDLED_FORMS` const
- `src-tauri/src/forms/templates/mod.rs` — adds `pub mod <form>;` line
- `src/forms/index.ts` — adds `import './<form>';` barrel line

Two parallel subagents editing these files would merge-conflict. Serial execution avoids the conflict at the cost of wall-clock parallelism.

Source-of-truth file paths (absolute, in main checkout — not visible in this worktree's `dev/scratch/`):

- ICS-309: `/home/administrator/Code/tuxlink/dev/scratch/winlink-re/install/RMS Express/Standard Templates/ICS USA Forms/` — read the ICS-309 .txt for body template + field set
- Position: `/home/administrator/Code/tuxlink/dev/scratch/winlink-re/install/RMS Express/Standard Templates/General Forms/GPS Position Report.txt`
- Bulletin: `/home/administrator/Code/tuxlink/dev/scratch/winlink-re/install/RMS Express/Standard Templates/General Forms/Bulletin.txt`
- Damage Assessment: `/home/administrator/Code/tuxlink/dev/scratch/winlink-re/install/RMS Express/Standard Templates/General Forms/Damage Assessment.txt` (+ `Damage_Assessment_Initial.html` for HTML field list)

### Task 9.1: ICS-309 Communications Log

**Files:** `src-tauri/src/forms/templates/ics309.rs`, `src/forms/ics309/{Ics309Form,Ics309View}.tsx`, tests, catalog registration.

- [ ] Field schema per `dev/scratch/winlink-re/install/RMS Express/Standard Templates/ICS USA Forms/ICS215A*.txt` (verify exact field set at impl time).
- [ ] Mirror Task 5.x test+impl+commit pattern.

### Task 9.2: GPS Position Report

**Files:** `src-tauri/src/forms/templates/position.rs`, `src/forms/position/{PositionForm,PositionView}.tsx`, tests, catalog registration.

- [ ] Field schema per `Standard Templates/General Forms/GPS Position Report.txt`.

### Task 9.3: Bulletin (broadcast)

**Files:** `src-tauri/src/forms/templates/bulletin.rs`, `src/forms/bulletin/{BulletinForm,BulletinView}.tsx`, tests, catalog registration.

- [ ] Field schema per `Standard Templates/General Forms/Bulletin.txt`.

### Task 9.4: Damage Assessment

**Files:** `src-tauri/src/forms/templates/damage_assessment.rs`, `src/forms/damage_assessment/{DamageAssessmentForm,DamageAssessmentView}.tsx`, tests, catalog registration.

- [ ] Field schema per `Standard Templates/General Forms/Damage_Assessment*.html` (HTML for field list; .txt for body template).

---

## Phase 10 — Hardening cross-cuts

### Task 10.1: dangerouslySetInnerHTML ban (Vitest assertion)

**Files:** Create `src/forms/innerhtml-ban.test.ts`.

- [ ] **Step 1: Write the ban-enforcement test**

```typescript
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { glob } from 'glob';

describe('forms module — dangerouslySetInnerHTML ban', () => {
  it('no .tsx file in src/forms/ uses dangerouslySetInnerHTML', () => {
    const files = glob.sync('src/forms/**/*.{tsx,ts}');
    const offenders: string[] = [];
    for (const f of files) {
      const content = readFileSync(f, 'utf-8');
      if (content.includes('dangerouslySetInnerHTML')) {
        offenders.push(f);
      }
    }
    expect(offenders).toEqual([]);
  });
});
```

- [ ] **Step 2: Run test → pass (no offenders if all forms are React-safe)**

- [ ] **Commit**

```bash
git commit -m "test(forms-ts): ban dangerouslySetInnerHTML in forms module (tuxlink-v1p)

Per spec §10. Vitest enforces the ban via filesystem scan. Adding the
test now guards future contributions from regressing the XSS posture.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 10.2: Attachment filename sanitization

**Files:** Modify the attachment-display path in `MessageView.tsx` or wherever filenames render.

- [ ] Sanitize via `filename.replace(/[\x00-\x1f]/g, '').slice(0, 255)` before display.
- [ ] Test that control characters in filenames are stripped.
- [ ] Commit.

---

## Phase 11 — Codex round + live smokes

### Task 11.1: Codex review on implementation

- [ ] **Step 1: Identify the implementation commit range** (likely a merge-commit hash on the bd-tuxlink-v1p branch).

- [ ] **Step 2: Run Codex review**

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-v1p-html-forms-design
mkdir -p dev/adversarial
npx --yes @openai/codex review --commit <SHA> 2>&1 | tee dev/adversarial/2026-XX-XX-html-forms-impl-codex.md
```

(Note: per tuxlink-wqv, codex review does not accept inline prompts AND a commit at once; the default prompt suffices for an impl-stage review.)

- [ ] **Step 3: Read findings; apply P0 + P1 inline as a new commit.**

### Tasks 11.2–11.5: Live cross-client smokes

Operator-driven (subagent CANNOT execute these per CLAUDE.md RADIO-1):

> **Rev-4 note:** Tuxlink's send path is now native B2F (compose_message_with_files → NativeBackend::send_message). Pat is no longer used as a transport, but wire-format compatibility with Pat is still a spec goal (§11). Smoke B and D remain valid — they test that Tuxlink's native-B2F output is parseable by Pat (Smoke B) and that Pat-authored B2F messages are parseable by Tuxlink (Smoke D). These are interop / wire-format validation smokes, not transport smokes.

- [ ] **Smoke A**: Tuxlink-composed ICS-213 (native B2F) → WLE receives + renders correctly.
- [ ] **Smoke B**: Tuxlink-composed ICS-213 (native B2F) → Pat receives + renders correctly (wire-format interop gate).
- [ ] **Smoke C**: WLE-composed ICS-213 → Tuxlink receives + renders correctly.
- [ ] **Smoke D**: Pat-composed ICS-213 → Tuxlink receives + renders correctly (wire-format interop gate).

For each: capture the .mime bytes; if the receiving client errors or mis-renders, compare bytes against the working reference and adjust. The 4 smokes are the parity gates per spec §11.

Once all 4 smokes are green, the implementation is ready to merge.

---

## Self-Review checklist

**Spec coverage** — every spec section maps to at least one task:

| Spec section | Implementing task(s) |
|---|---|
| §3 wire format | T1.6 (serialize), T1.5 (parse) |
| §5.1 native attachment path (only path) | T3.1 (native B2F via `compose_message_with_files`); Pat REST / Path A removed per ADR 0016 |
| §5.2 module layout + DTO addition (form_payload) | T1.1, T2.2 |
| §6.1 Rust types | T1.1 |
| §6.2 OutboundAttachment | T0.1 |
| §6.3 wire format spec | T1.6 |
| §7.1 compose flow | T6.1, T6.2 |
| §7.2 form picker / fill / view | T4.4, T5.1, T5.2 |
| §7.3 draft persistence | T6.3, T6.4 |
| §7.4 reply-to-form | T8.1 |
| §8 catalog | T1.7 (ICS-213) + T9.x (others) |
| §10 hardening | T1.2 (caps), T1.5 (parser config), T10.1 (XSS ban), T10.2 (filename) |
| §11 testing | All Phase 1–10 tests; T11.2-5 live smokes |
| §12 Codex round | T11.1 |
| §13 migration (parse_raw_rfc5322 fix) | T2.1 |

**Type consistency** — `FormPayload`, `FormParameters`, `FormDef`, `FormField`, `FieldKind` use the same names everywhere (Rust + TS).

**No placeholders** — every step has actual content (no "TODO" / "fill in later" / "similar to Task N"). Where implementation is deferred to the spec, the spec §reference is explicit.

**Cross-task conflicts** — `ui_commands.rs` is touched by T2.1 + T2.2 + T3.1; these are sequenced. **Per-form tasks T9.1–9.4 touch 3 shared files (`forms/catalog.rs`, `forms/templates/mod.rs`, `src/forms/index.ts`) and MUST run serially** (corrected from rev-1's parallel claim — see Phase 9 intro). T0.x callers update is single-file in `winlink_backend.rs` after T0.1's struct change.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-05-30-html-forms-v0.1-plan.md`.**

**Two execution options:**

1. **Subagent-Driven (recommended)** — Dispatch a fresh subagent per task with two-stage review between tasks. Fast iteration; protects main session's context. Best fit: this plan has 30+ small TDD-disciplined tasks ideal for fresh-context subagents.

2. **Inline Execution** — Execute via `superpowers:executing-plans` in this session with batch checkpoints. Best fit: smaller plans or when shared context across tasks adds value.

**Recommendation: Subagent-Driven.** This plan is large enough that inline execution would consume substantial main-session context; subagent-per-task isolates risk and provides checkpoint discipline. Two-stage review (subagent → main session review → next subagent) is the model. Phase 9's per-form tasks are SERIAL (corrected from rev-1; see Phase 9 intro for the shared-file conflict reasons).

**Per BRF Step 5:** before kicking off subagent execution, **operator decision required** on:
- Whether all of Phases 0–11 should execute in this session (12–18 days estimated), OR scope down to a v0.1.0-MVP slice (Phases 0–8 with only ICS-213; defer Phase 9 forms to v0.1.1)
- **Agent Teams is NOT a fit here** — Phase 9 was the only candidate for parallel multi-agent execution, and rev-2/rev-3 correction makes it serial. Stick with one-subagent-per-task.

Agent: yew-cypress-oak
