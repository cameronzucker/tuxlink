# HTML Forms v0.1 — design spec (rev-3, native attachment path finalized)

> Date: 2026-05-30 · Agent: yew-cypress-oak · bd: tuxlink-v1p · Parent context: PR #150 (inventory rev-2)
> Rev: 3 — removes the Path A (Pat REST) choice in §5.1; native B2F outbound with attachments is now available per ADR 0016. The spec is unblocked; PR #151 resumes on native rails.

## 0. Status — DESIGN ONLY (HARD-GATE per `superpowers:brainstorming` + `build-robust-features`)

This is the post-adversarial-review design. Per BRF, **no implementation code** is committed until this spec is approved AND a `superpowers:writing-plans` implementation plan is reviewed.

Operator (Cameron) is driving today. Approval flow:

- **Approve / merge** → I proceed to `superpowers:writing-plans` → plan review cycle (min 3 rounds) → execution recommendation → operator decision on approach (subagent / parallel session / agent teams).
- **Comment** → I revise spec + re-run any affected adversarial round.
- **Reject** → regroup.

## 1. Change log

### rev-3 (2026-05-30 — native attachment path finalized)

| Change | Detail |
|---|---|
| **§5.1 rewritten** | Path A (Pat REST) removed. The native B2F attachment path is the only encoding path for v0.1 per [ADR 0016](../../adr/0016-native-b2f-outbound-with-attachments.md). |
| **Spec header updated** | rev-2 → rev-3; status line updated. |
| **§5.2 Module layout** | `pat_client.rs` reference removed; native send pipeline is the only target. |
| **§9 Boundaries** | `pat_client.rs` reference removed. |
| **Effort table (§16)** | "Precursor" row updated to reflect native-only pipeline. |

### rev-2 (2026-05-30 — post-adversarial)

Adversarial review surfaced **24+ P0/P1 findings** in rev-1, most of which would have shipped an implementation incompatible with WLE and/or Pat. Adversarial transcripts (gitignored, local-only) at `dev/adversarial/2026-05-30-html-forms-design-claude-r{1,2,3,4}-*.md`.

Key rev-1 → rev-2 corrections:

| Class | Rev-1 said | Rev-2 says | Source |
|---|---|---|---|
| **Wire — XML element casing** | Mixed case (`<Subjectline>`, `<Mdate>`, `<Approved_Name>`) | All lowercase (`<subjectline>`, `<mdate>`, `<approved_name>`) — WLE lowercases on serialize via `Template.cs:775-777` `.ToLower()`; Pat is case-sensitive on read | R1-F2, R2-I01 (P0) |
| **Wire — `<form_parameters>` elements** | `<xml_file_version>`, `<tuxlink_version>`, `<form_version>` | The WLE-required set: `<xml_file_version>`, `<rms_express_version>` (value `"Tuxlink/0.3.0"`), `<submission_datetime>`, `<senders_callsign>`, `<grid_square>`, `<display_form>`, `<reply_template>` | R1-F1, R2-I02, R2-I03 (P0) |
| **Wire — attachment filename** | `RMS_Express_Form_<id>.xml` | `RMS_Express_Form_<displayFormBasename>.xml` (e.g. `RMS_Express_Form_ICS213_Initial.xml`) — matches WLE `Template.cs:718-724` AND Pat `builder.go:459-467` | R2-I04 (P1) |
| **Wire — MIME content-type** | "application/xml or text/xml" | `text/xml` exclusively (WLE `MimeEncoder.cs:89-92`) | R1-F5 (P1) |
| **Wire — XML declaration** | `<?xml version="1.0" encoding="utf-8"?>` | `<?xml version="1.0"?>` (no encoding attr; UTF-8 BOM prepended) — matches WLE | R1-F3 (P2) |
| **Wire — body charset** | UTF-8 implied | ISO-8859-1 (Latin-1) — WLE `Message.cs:295-298` down-codes; Pat reads per `Content-Type` header | R2-I06 (P1) |
| **Wire — empty fields** | unspecified | Emit `<field></field>` always for declared form fields (matches Pat behavior) | R1-F4 (P1) |
| **Backend — attachments** | "calls existing `send_message` infra" | **`OutboundMessage` has NO `attachments` field today** — requires precursor backend work | R2-I05 (P0) |
| **Backend — B2F wire vs MIME** | Conflated | Rev-2 identified two encoding paths: (1) via Pat REST API (MIME-multipart upload), (2) via native winlink session (B2F `File:` headers). Rev-3 finalizes the native path (2) per ADR 0016 — Path A is removed. | R2-I07 (P1); rev-3 resolution |
| **UX — draft destruction** | Implicit replace | Pre-form-switch unsaved-changes dialog (Save/Discard/Cancel) | R3-1 (P0) |
| **UX — DraftData schema** | "React form state" | Extend `DraftData` with `formId?` + `formFields?`; wire autosave/restore | R3-2 (P0) |
| **UX — reply-to-form behavior** | Deferred to operator question | Must be decided BEFORE shipping reply buttons; rev-2 proposes a default | R3-3 (P0) |
| **Security — XML parser** | Unspecified | Pin `quick-xml` 0.39.x; reject `Event::DocType`; cap nesting depth 8; cap event count 10k | R4-F1 (P0) |
| **Security — payload size cap** | None | `MAX_FORM_XML_BYTES = 256 KiB` enforced at parse boundary | R4-F2 (P0) |
| **Security — form_id validation** | None | Validate `^[A-Za-z0-9_-]{1,64}$` at extraction; document constraint for v0.5+ catalog use | R4-F4 (P0) |
| **Body template — `IsExercise`** | Omitted | Include `<var IsExercise>` (DRILL marker) — Part 97 / EmComm critical | R1-F2 (P0) |
| **DTO — form_payload to frontend** | Only `form_id` added | Add `form_payload: Option<FormPayload>` — parse eagerly in `parse_raw_rfc5322` while attachment bytes are still in hand. Without this, frontend has form ID but no field values to render. | R5-Codex (P1) |

Everything else from rev-1 is preserved.

## 2. Purpose & v0.1 scope (unchanged)

Operator goal: HTML Forms is the v0.1 must-have with highest EmComm leverage (per PR #150 §13.1). Build it autonomously.

**In scope for v0.1**:
1. **Render incoming form messages** — parse XML attachment, render structured field/value display per form type, replacing the current "Winlink form attached" placeholder.
2. **Author one canonical form (ICS-213 General Message)** — fill out via native React, serialize to WLE+Pat-compatible wire format, send.
3. **Bundle 4–6 forms** in the binary: ICS-213, ICS-309, GPS Position Report, Bulletin, Damage Assessment.

**Out of scope** (deferred): dynamic forms-catalog auto-update; form-data aggregator views / maps; ICS-309 stand-alone tool; embedded-WebBrowser-with-local-HTTP-server pattern; custom-XML-form sideload.

## 3. Background: how WLE forms actually work (corrected)

A WLE form is **three files** on disk:

| File | Role |
|---|---|
| `<FormName>.txt` | Catalog metadata + plain-text message-body template (with `<var X>` placeholders) |
| `<FormName>_Initial.html` | Compose-side HTML rendered in embedded WebBrowser (with local-HTTP-server form-action hack) |
| `<FormName>_Viewer.html` | Read-side HTML for displaying received forms |

On the wire, a WLE form message is:

- **Body** (plain text, ISO-8859-1, quoted-printable transfer encoding): the `Msg:` template with substitutions. Human-readable for non-form-aware clients.
- **Attachment** named `RMS_Express_Form_<DisplayFormBasename>.xml` (Content-Type `text/xml`, base64 transfer encoding): the structured payload.

**XML payload shape** (from WLE `Template.cs:680-754` + Pat `builder.go:106-156`):

```xml
<?xml version="1.0"?>
<RMS_Express_Form>
  <form_parameters>
    <xml_file_version>1.0</xml_file_version>
    <rms_express_version>Tuxlink/0.3.0</rms_express_version>
    <submission_datetime>20260530143000</submission_datetime>
    <senders_callsign>N0CALL</senders_callsign>
    <grid_square>FM18lu</grid_square>
    <display_form>ICS213_Initial_Viewer.html</display_form>
    <reply_template>ICS213_SendReply.0</reply_template>
  </form_parameters>
  <variables>
    <inc_name>HURRICANE WALDO RESPONSE</inc_name>
    <to_name>JOHN OPERATOR, EOC Comms</to_name>
    <fm_name>JANE OPERATOR, Field Net Control</fm_name>
    <subjectline>REQUEST EXTRA MEDICAL SUPPLIES</subjectline>
    <mdate>2026-05-30</mdate>
    <mtime>14:30Z</mtime>
    <message>Need additional bandages and IV bags at field station 3...</message>
    <approved_name>JANE OPERATOR</approved_name>
    <approved_postitle>Field Net Control</approved_postitle>
  </variables>
</RMS_Express_Form>
```

**Important wire-format constraints** (all verified against WLE decompile + Pat source):

- All `<variables>` element names are lowercase ASCII. Internal underscores preserved (`<approved_name>`).
- Element ORDER in `<form_parameters>` MUST match the WLE-emitted order above (xml_file_version → rms_express_version → submission_datetime → senders_callsign → grid_square → display_form → reply_template).
- `<display_form>` is REQUIRED — Pat returns HTTP 400 (`forms.go:511-517`) and WLE shows "display form name is blank" error if missing.
- `<rms_express_version>` is the canonical version-tag element (Pat uses this name too); the value can be `"Tuxlink/<semver>"` to identify originator without breaking aggregator compatibility.
- Empty fields emit `<field></field>` (not self-closing, not omitted), per Pat behavior.
- Special chars: `<`, `>`, `&` are XML-escaped; `"`, `'` are NOT (matches WLE).
- File charset: UTF-8 with BOM (3-byte `EF BB BF` prefix). Pat strips BOM (`forms.go:462`); WLE's `XmlReader` handles natively.

**Body template** for ICS-213 (from real `ICS213 General Message.txt` — full canonical version):

```
GENERAL MESSAGE (ICS 213)
<var FormTitle>
<var IsExercise>
1. Incident Name: <var inc_name>       <var txtStr>
2. To (Name and Position): <var to_name>
3. From (Name and Position): <var fm_name>
4. Subject: <var subjectline>
5. Date: <var mdate>
6. Time: <var mtime>
7. Message:

<var message>

8. Approved by: <var approved_name>
8a. Position/Title: <var approved_postitle>
    [Sender: <var theMsgSender> Lat: <var mapLat>, Lon: <var mapLon>, MGRS: <var MGRS>; Location source: <var locationSource>]
------------------------------------
Sending Station: <MsgSender>
Senders Software Version: Tuxlink/<ProgramVersion>
Senders Template Version: <var Templateversion>
[No changes or editing of this message are allowed]
```

The `<var IsExercise>` line is operationally critical — it substitutes to "** THIS IS AN EXERCISE **" when the operator checked the exercise box (otherwise empty). EmComm operators trained on WLE rely on this marker. Tuxlink composing without it risks an exercise being mistaken for a real incident or vice versa.

## 4. Design approaches (unchanged from rev-1)

Same three approaches as rev-1. Recommendation stands: **Approach C — native React forms for v0.1, defer WLE-HTML-webview-compat to v0.5+**.

## 5. Chosen architecture

### 5.1 Native attachment path (the only v0.1 encoding path)

Tuxlink composes form messages via the native B2F attachment path. Pat was removed in the PR that landed [ADR 0016](../../adr/0016-native-b2f-outbound-with-attachments.md); Path A (Pat REST multipart) no longer exists.

| Path | Encoding work |
|---|---|
| **Native B2F attachment path** | `compose_message_with_files` emits `File: <size> <name>` headers + raw attachment bytes per the Winlink B2F wire format (see ADR 0016 §"Wire format reference"); lzhuf compression + B2F proposal exchange handled by the existing native transfer pipeline. |

**Decision for v0.1**: The forms module is transport-agnostic — it produces the (text body, xml bytes, filename) triple; `compose_message_with_files` handles the B2F encoding. No Pat dependency; no multipart/form-data. This is the same native send pipeline that handles plain-text outbound today, extended for attachments per the design spec `2026-05-30-pat-strip-native-attachments-design.md`.

### 5.2 Module layout

**Rust backend** (new files in `src-tauri/src/forms/`):

- `mod.rs` — module root
- `catalog.rs` — bundled `FormDef` definitions for 5 forms
- `parse.rs` — detect form via attachment-name match + parse XML payload (hardened)
- `serialize.rs` — build XML envelope + text body from form-field values (lowercase elements, full `<form_parameters>`, etc.)
- `types.rs` — `FormDef`, `FormField`, `FormPayload`, `FieldKind`
- `validation.rs` — `form_id` regex + size caps + parser-config helpers

**Backend integration changes** (existing files):

- `winlink_backend.rs`: extend `OutboundMessage` with `attachments: Vec<OutboundAttachment>`. **Breaking change to the struct** (acknowledged at line 89 — "Adding fields is an acknowledged breaking change"). Add `OutboundAttachment { filename: String, content_type: String, bytes: Vec<u8> }`.
- `winlink/compose.rs`: `compose_message_with_files` is the entry point for form messages with XML attachments — same function used for any native outbound attachment per ADR 0016.
- `ui_commands.rs`: 
  - **Bug fix in scope**: `is_form` detection moves from `body.starts_with("<?xml")` to attachment-name match (`attachments.iter().any(|a| a.filename.starts_with("RMS_Express_Form_") && a.filename.ends_with(".xml"))`).
  - Add `form_id: Option<String>` to `ParsedMessageDto` (extracted from attachment name; normalized for lookup).
  - **Add `form_payload: Option<FormPayload>`** to `ParsedMessageDto` — the parsed form fields, populated eagerly while `parse_raw_rfc5322` still has the raw attachment bytes available. Without this, the frontend would have only the form ID and no field values to render (Codex R5-P1). Eager parse is safe: `MAX_FORM_XML_BYTES = 256 KiB` caps allocation; parse is ~1ms per form. Lazy alternative (separate `get_form_payload` IPC) considered and rejected — extra round-trip + extra IPC surface for no real benefit at v0.1 sizes.
  - Add `send_form` Tauri command: `(form_id, field_values, to, cc, subject)` → serialize → attach to `OutboundMessage` → existing `send_message` path.

**React frontend** (new files in `src/forms/`):

- `types.ts` — TS mirror of `FormPayload`, `FormDef`, `FormField`, `FieldKind`
- `forms.ts` — registry `Record<form_id, { compose: Component, view: Component }>`
- `ics213/{Ics213Form.tsx, Ics213View.tsx}` — per-form compose + view (one pair per bundled form)
- `KeyValueView.tsx` — unknown-form fallback (renders body text + raw field/value dump)
- `FormPicker.tsx` — compose-side modal picker (5 forms × name + description)

**Frontend integration changes**:

- `src/mailbox/MessageView.tsx`: form-render dispatch (lookup `message.form_id` → component or KeyValueView).
- `src/compose/Compose.tsx`: 
  - Add "Compose form…" button.
  - Pre-form-switch unsaved-changes dialog (Save / Discard / Cancel) before replacing body region with form component.
- `src/compose/useDraft.ts`: extend `DraftData` with `formId?: string | null` + `formFields?: Record<string, string>`. Update autosave + restore + `compose_window_open` seed paths.
- `src/mailbox/replyActions.ts`: update for new body-vs-XML separation (body now plain-text-rendered, XML in attachment — different from rev-1's body-XML assumption). Add explicit tests for reply behavior on form messages.

### 5.3 Reply-to-form default (rev-2 decision)

Rev-1 deferred this. Rev-2 picks a default: **Reply to a form message defaults to plain-text reply, NOT auto-open the same form**. Rationale:

- Operator can always escalate to a same-form reply via the Compose-form button.
- Auto-opening a form on Reply forces the operator into structured response when free-text is sometimes the right answer ("Acknowledged, will dispatch.").
- The pre-rev-2 quote-preserving logic in `replyActions.ts` already handles "don't quote form XML" — we extend it to "don't quote the rendered text body either" (which would be redundant noise). Reply body becomes: original sender's most recent line/two + `[ICS-213 form omitted from quote — view original for full content]`.
- Operator-decision-overridable: a future preference toggle could flip this default. Out of scope for v0.1.

## 6. Data model (corrected)

### 6.1 Rust types

```rust
// src-tauri/src/forms/types.rs

pub struct FormDef {
    /// Canonical form ID (lowercase, alphanumeric + dash/underscore, ≤64 chars).
    /// Matches the WLE template name basename (e.g. "ICS213_Initial"), used both
    /// for attachment naming and for the React component registry lookup.
    pub id: &'static str,
    /// Display name in the FormPicker.
    pub name: &'static str,
    /// Field schema in declaration order (must match WLE element-emit order
    /// so byte-level diff against WLE-composed forms is minimal).
    pub fields: &'static [FormField],
    /// Subject-line template (with `%fieldid%` placeholders — lowercase).
    pub subject_template: &'static str,
    /// Plain-text Msg: template (with `<var fieldid>` placeholders, matching
    /// WLE convention). Substitutions are case-insensitive on the lookup side
    /// (per Pat's `placeholder.go` regex `(?i)`) but stored lowercase here.
    pub body_template: &'static str,
    /// `<display_form>` value emitted in `<form_parameters>` (WLE+Pat both
    /// require this; missing → Pat returns HTTP 400, WLE shows error dialog).
    pub display_form: &'static str,
    /// `<reply_template>` value emitted in `<form_parameters>`.
    pub reply_template: &'static str,
}

pub struct FormField {
    pub id: &'static str,         // lowercase ASCII; matches XML element name
    pub label: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    pub max_length: Option<usize>,  // SEND-side cap; RECV-side has separate cap
}

pub enum FieldKind {
    Text,
    LongText,
    Date,         // ISO format YYYY-MM-DD
    Time,         // 24h UTC HH:MM[Z]
    Boolean,      // checkbox; serialized as "Yes" / "" (matches WLE)
}

pub struct FormPayload {
    pub form_id: String,                     // from attachment name, validated
    pub form_parameters: FormParameters,     // <form_parameters> block contents
    pub fields: Vec<(String, String)>,       // (lowercase_id, value) — XML order preserved
}

pub struct FormParameters {
    pub xml_file_version: String,             // "1.0"
    pub rms_express_version: String,          // "Tuxlink/<semver>" or sender's value
    pub submission_datetime: String,          // YYYYMMDDhhmmss UTC (WLE format)
    pub senders_callsign: String,
    pub grid_square: String,                  // 4-char Maidenhead default per project convention
    pub display_form: String,                 // basename match for catalog lookup
    pub reply_template: String,               // .0 reply template filename
}
```

### 6.2 Outbound attachment type

```rust
// src-tauri/src/winlink_backend.rs (additions)

pub struct OutboundAttachment {
    pub filename: String,        // e.g. "RMS_Express_Form_ICS213_Initial.xml"
    pub content_type: String,    // "text/xml" for forms
    pub bytes: Vec<u8>,          // raw bytes (UTF-8 with BOM for XML)
}

pub struct OutboundMessage {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub attachments: Vec<OutboundAttachment>,  // NEW
}
```

This change is acknowledged-breaking per the existing `OutboundMessage` comment.

### 6.3 Wire format spec (corrected — see §3)

Full canonical XML payload in §3. Notable normative bits:

- XML declaration: `<?xml version="1.0"?>` (NO encoding attribute, UTF-8 BOM prepended)
- All `<variables>` element names lowercase ASCII
- `<form_parameters>` includes all 7 elements in WLE order
- Empty fields: `<field></field>`
- MIME content-type: `text/xml`, base64 transfer encoding, double-quoted filename

## 7. UI surfaces (with R3 fixes)

### 7.1 Compose flow — with draft-protection dialog

```
┌─ Compose window ───────────────────────┐
│ To:      [JOHN@winlink.org]            │
│ Subject: [REQUEST EXTRA MEDICAL]       │
│ ─────────────────────────────────       │
│  [Body region]                          │
│                                         │
│  Body text or form fields here...       │
│                                         │
│  [Save Draft]  [Compose form…]  [Send] │
└────────────────────────────────────────┘
```

**Clicking "Compose form…" when body region has unsaved content** triggers a dialog:

```
┌─ Unsaved changes ───────────────────────┐
│ Switching to a form will replace your   │
│ current message body. Your draft will   │
│ be saved automatically — you can return │
│ to it from Drafts.                       │
│                                         │
│ Continue?                                │
│         [Cancel]    [Save & continue]   │
└─────────────────────────────────────────┘
```

After confirm: save current body as a draft (existing `useDraft` path), then mount the form component.

### 7.2 Form picker, form fill, form view (unchanged from rev-1 §6.1–§6.3)

UI layouts in rev-1 §6 stand; the corrected wire format flows through unchanged.

### 7.3 Draft persistence (R3-F2 fix)

`DraftData` schema extends:

```typescript
interface DraftData {
  to: string;
  subject: string;
  body: string;
  requestAck: boolean;
  formId?: string;              // NEW — if set, draft is a form-fill in progress
  formFields?: Record<string, string>;  // NEW — field id (lowercase) → value
}
```

Autosave path (`useDraft.ts:131`) inspects `formId`; if set, persists `formFields` instead of `body`. Restore path: if a draft has `formId`, the compose window mounts the form component with `formFields` pre-populated.

### 7.4 Reply-to-form behavior (R3-F3 fix)

Reply to a message where `message.form_id` is set:

- **Default**: plain-text reply. Body pre-population: `"On <date>, <from> wrote:\n[ICS-213 form omitted from quote — view original for full content]\n\n"`.
- **Operator override**: a "Reply with form…" alternative button (next to Reply / Reply All) opens the same form type with `to_name` pre-populated from the original `fm_name` (and reverse).
- **Existing `replyActions.ts` test**: keeps the "do not quote raw XML" assertion (now even stronger: also do not quote the rendered text body, since it can contain sensitive form values).

## 8. Catalog (unchanged)

Bundled in binary as `const` arrays in `src-tauri/src/forms/catalog.rs`. Seed set (operator-confirmable in §15):

| Form ID | Display Form | Reply Template | Tuxlink display name |
|---|---|---|---|
| `ICS213_Initial` | `ICS213_Initial_Viewer.html` | `ICS213_SendReply.0` | ICS-213 General Message |
| `ICS309_Initial` | `ICS309_Initial_Viewer.html` | `ICS309_SendReply.0` | ICS-309 Communications Log |
| `Position_Initial` | (TBD per catalog) | (TBD) | GPS Position Report |
| `Bulletin_Initial` | (TBD per catalog) | (TBD) | Bulletin (broadcast) |
| `DamageAssessment_Initial` | (TBD per catalog) | (TBD) | Damage Assessment |

(Verify exact `<display_form>` filename for each by reading the WLE `Standard Templates/` files at impl time.)

## 9. Boundaries — full integration list

**New files**: `src-tauri/src/forms/{mod,catalog,parse,serialize,types,validation}.rs`, `src-tauri/tests/forms_test.rs`, `src/forms/{types,forms,FormPicker,KeyValueView,ics213/Ics213Form,ics213/Ics213View}.tsx`, plus 4 more form-component pairs.

**Modify**:

- `src-tauri/src/winlink_backend.rs`: add `OutboundAttachment` struct; add `attachments: Vec<OutboundAttachment>` to `OutboundMessage` (breaking but acknowledged).
- `src-tauri/src/winlink/message.rs`: extend `Message` with attachment support (B2F wire format) — completed by ADR 0016 PR.
- `src-tauri/src/ui_commands.rs`: fix `is_form` detection; add `form_id` to DTO; add `send_form` command.
- `src/mailbox/MessageView.tsx`: form-render dispatch.
- `src/mailbox/replyActions.ts`: updated body-vs-XML logic + reply-to-form behavior; tests added.
- `src/compose/Compose.tsx`: "Compose form" entry point + unsaved-changes dialog.
- `src/compose/useDraft.ts`: extend `DraftData`.

**No deletions**.

## 10. Security & hardening (R4 — NEW section)

Per BRF security round, the following hardening is **mandatory before merge**, not optional:

| Concern | Mitigation |
|---|---|
| XML billion-laughs entity expansion | Use `quick-xml` 0.39.x with `Reader::trim_text(false)` and explicit `Event::DocType` rejection. Cap nesting depth at 8; cap total event count at 10k. Regression test with a corpus of malicious XML samples (entity bombs, deeply nested, etc.). |
| OOM on large attachment | `MAX_FORM_XML_BYTES = 256 * 1024` (256 KiB) enforced at the `forms::parse::parse_form_xml` boundary; reject larger with `UiError::Internal("form XML too large")` before allocation. |
| Field count explosion | Cap `FormPayload.fields.len() <= 256` during parse. Reject and log if exceeded. |
| `form_id` path traversal | Validate extracted ID against regex `^[A-Za-z0-9_-]{1,64}$` at extraction. Reject otherwise. Documented as load-bearing for v0.5+ catalog cache + attachment save features. |
| React XSS via field value | All field-value renders go through React's default-escape path. **Explicit ban on `dangerouslySetInnerHTML` in the forms module**, enforced by a Vitest test (`expect(component).not.toContain('dangerouslySetInnerHTML')`). |
| Attachment filename rendering | Sanitize filename before display (`.replace(/[\x00-\x1f]/g, '')` + length cap 255 chars). |
| Reply-quote leak | `replyActions.ts` updated to detect `form_id` set on the source message; skip quoting body content (which contains the form-rendered text); insert the placeholder string. New test cases. |
| `send_form` Tauri command | Register in the Tauri allowlist with the same restrictions as `send_message`; no broader scope. |
| UTF-8 validation | Reject non-UTF-8 XML attachment bytes early; surface as `UiError::Internal("form XML not valid UTF-8")`. |
| Streaming-receive memory pressure | Native B2F receive: the send pipeline is attachment-agnostic at the frame layer; attachment bytes are buffered per-file by the parser. Size cap (256 KiB per form XML) bounds memory per receive. Large binary attachments are not in scope for v0.1 forms. |

## 11. Testing strategy (expanded)

| Test class | Coverage |
|---|---|
| **Rust unit — serialize** | Byte-exact output for each bundled form given a known field map (including lowercase elements, full `<form_parameters>`, BOM, MIME envelope). |
| **Rust unit — parse** | Round-trip per form (serialize → parse → equal field map). Detection from attachment name. Form ID validation. Empty-field handling. |
| **Rust unit — hardening** | Billion-laughs rejection; size cap rejection; field count cap; path-traversal `form_id` rejection; malformed UTF-8 rejection. |
| **Rust integration — message build** | `send_form → OutboundMessage with attachment → compose_message_with_files → wire bytes match expected B2F structure`. |
| **Vitest — Ics213Form** | All required fields render; required-field validation triggers on empty submit; long-text Message field wraps. |
| **Vitest — Ics213View** | Given a `FormPayload`, all field/value pairs displayed; XSS-safe rendering (no innerHTML). |
| **Vitest — KeyValueView** | Body text + raw field dump displayed; unknown XML doesn't crash. |
| **Vitest — Compose flow** | "Compose form" → dirty-body dialog → save draft → mount form. Round-trip form-fill → submit → IPC. |
| **Vitest — Draft persistence** | Form-in-progress survives unmount/remount via `DraftData`. |
| **Vitest — Reply behavior** | Reply to form message uses placeholder, not raw form data. Reply-with-form opens correct form pre-populated. |
| **Live-receive smoke A** | tuxlink-composed ICS-213 → **WLE** receives + renders correctly. Cross-client parity gate. |
| **Live-receive smoke B** | tuxlink-composed ICS-213 → **Pat** receives + renders correctly. |
| **Live-receive smoke C** | WLE-composed ICS-213 → tuxlink receives + renders correctly. |
| **Live-receive smoke D** | Pat-composed ICS-213 → tuxlink receives + renders correctly. |

The four live smokes are the **parity gates**. Any one failing means the wire format is wrong somewhere.

## 12. Codex round (impl-stage; design-stage already done)

Adversarial design review per BRF has been completed (Claude R1-R4 done; Codex R5 in progress at time of writing). The impl-stage Codex round runs against the implementation commit before the final PR opens; per `feedback_no_carveout_on_cross_provider_adrev`, this is non-optional for Winlink-protocol-adjacent work.

## 13. Migration (parse_raw_rfc5322 detection bug fix)

In scope for this PR. Change `parse_raw_rfc5322`:

- Old: `let is_form = body.trim_start().starts_with("<?xml");`
- New: `let is_form = attachments.iter().any(|a| a.filename.starts_with("RMS_Express_Form_") && a.filename.ends_with(".xml"));` + extract `form_id` via the validated regex.

Update dev fixtures (`devFixture.ts`, `replyActions.test.ts`) to use the correct wire format (XML in attachment + plain text in body). Update tests against the new shape.

No user-data risk: no real CMS forms have been received in production with the wrong format detection.

## 14. Risks (expanded)

| Risk | Mitigation |
|---|---|
| Live smoke A fails (WLE rejects our form) | The 4 smokes are gating; if any fails, do not ship until fixed. Debug via comparing tuxlink-emitted bytes to WLE-emitted bytes for the same form. |
| `OutboundMessage` breaking change ripples broadly | Acknowledged at code-comment level already. Update all `OutboundMessage::new` callers in the same PR. |
| Forms catalog drift (we ship ICS213/v1.0, WLE has v1.2) | WLE renders blanks for missing fields; tuxlink tolerates schema drift equally. Document; accept. Operator can refresh forms once auto-update ships (v0.5+). |
| Performance: large form-XML on slow RF | Native B2F pipeline buffers per-file; size cap (256 KiB) is well under any plausible form. |

## 15. Open questions for operator review (narrowed by BRF)

The BRF rounds resolved 3 of rev-1's open questions; the remaining 4 are:

1. **Form seed list** — is `ICS213 / ICS309 / Position / Bulletin / DamageAssessment` the right 5? Substitutions? (E.g., HICS forms for hospital ops; ARC1077 specifically; Radiogram.)
2. **Position Report vs. position-report-message dashboard button** — same UX (form) or different (dashboard button)? Note: WLE has both.
3. **Reply-to-form default**: rev-2 picks **plain-text reply with placeholder**. Confirm, or flip to "auto-open same form pre-populated."
4. **Forms-catalog versioning**: when `forms::catalog::ICS213_Initial` ships at form-schema v1.0, and WLE later ships v1.1 with a new field, what's our update story? Bundle update in next tuxlink release? Hot-update from a winlink.org endpoint (out of v0.1 scope)?

Rev-1's other operator questions (XML envelope element naming, Compose entry point) are now decided by BRF findings (use `<rms_express_version>` per Pat-aggregator-compat; button in Compose window per §7.1).

## 16. Effort estimate (revised post-BRF)

Rev-1: "4–6 days once approved" — wildly optimistic. The OutboundMessage backend gap alone is ~2 days.

Rev-2: **12–18 days** for a thorough v0.1 with all 4 live smokes green. Breakdown:

| Phase | Work | Days |
|---|---|---|
| Precursor | `OutboundMessage` + `OutboundAttachment` + `compose_message_with_files` native B2F send + tests — **completed by ADR 0016 PR** | 0 (done) |
| Forms backend | Rust `forms` module (catalog + parse + serialize + validation + tests) | 3-4 |
| Detection fix + DTO | `ui_commands.rs` + `ParsedMessageDto.form_id` + fixture updates | 1 |
| Frontend — forms surface | `FormPicker` + ICS-213 compose + view; `KeyValueView`; draft-protection dialog | 3-4 |
| Frontend — additional forms (4 more) | ICS-309, Position, Bulletin, DamageAssessment | 1-2 |
| Reply behavior | `replyActions.ts` updates + tests | 0.5 |
| Codex impl round | Adversarial review on the implementation commit | 0.5 |
| Live smokes (4) | Operator-driven cross-client smokes | 1-2 |

Plan can subagent-parallelize: precursor + forms backend serialize/parse can run concurrently, frontend can run concurrent with hardening. With 3-way subagent parallelism, calendar can be ~8-10 days.

## 17. Approval ask

Per BRF: approval of this design unblocks `superpowers:writing-plans` for the implementation plan, then 3-round plan review, then operator-decision on execution approach (subagent / parallel session / agent teams).

To approve: merge this PR.

To request changes: comment on what should change. Spec re-runs affected adversarial round if structural.

To reject the whole approach: say so; I regroup.

---

## Appendix A: Adversarial findings (rev-1 → rev-2)

**55 findings across 5 rounds**, gitignored at `dev/adversarial/2026-05-30-html-forms-design-{claude-r{1,2,3,4}-*,codex-r5}.md`.

| Round | Reviewer | Angle | Findings | P0/P1/P2 | New beyond prior rounds |
|---|---|---|---|---|---|
| R1 | Claude opus-4-7 (`sorrel-moss-hemlock`-style) | Wire-format pedant | 12 | 2 / 7 / 3 | All 12 |
| R2 | Claude opus-4-7 | Interop & backward compat | 12 | 4 / 6 / 2 | 8 net-new |
| R3 | Claude opus-4-7 | UX flow gaps | 12 | 3 / 7 / 2 | 12 net-new (orthogonal angle) |
| R4 | Claude opus-4-7 | Security & safety | 12 | 3 / 6 / 3 | 12 net-new (orthogonal angle) |
| R5 | Codex gpt-5.5 (`pine-thistle-raven`) | Independent holistic | 7 | 0 / 5 / 2 | **1 net-new** (P1 — carry parsed form payloads to reader) |

**Codex's catch (R5-P1, the only finding all 4 Claude rounds missed)**: rev-2 added `form_id` to `ParsedMessageDto` but didn't carry the parsed field VALUES, so the frontend would have only the form ID with no data to render. Fixed in §5.2 by adding `form_payload: Option<FormPayload>` to the DTO with eager parse.

All P0 findings (8) and P1 findings (~25) are addressed in rev-2's normative text. Remaining P2 findings are tracked as TODO comments in the implementation plan.

## Appendix B: References

- Adversarial transcripts: `dev/adversarial/2026-05-30-html-forms-design-claude-r{1,2,3,4}-*.md` (and r5-codex if completed)
- WLE source (decompiled): `dev/scratch/winlink-re/decompiled/rms-express/` (esp. `Template.cs`, `MergeFormVariables.cs`, `MessageEditor.cs`, `MimeEncoder.cs`, `FormServer.cs`)
- Pat source: `dev/scratch/tuxlink-pat/internal/forms/{forms.go, builder.go, placeholder.go}`
- WLE form templates: `dev/scratch/winlink-re/install/RMS Express/Standard Templates/ICS USA Forms/`
- Tuxlink current state: `src-tauri/src/{ui_commands.rs, winlink_backend.rs, winlink/message.rs}`, `src/{mailbox/MessageView.tsx, mailbox/replyActions.ts, compose/Compose.tsx, compose/useDraft.ts}`

Agent: yew-cypress-oak
