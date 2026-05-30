# HTML Forms v0.1 — design spec

> Date: 2026-05-30 · Agent: yew-cypress-oak · bd: tuxlink-v1p · Parent context: PR #150 (inventory rev-2) §13.1

## 0. Status — DESIGN ONLY (HARD-GATE per `superpowers:brainstorming`)

This is a written design for operator (Cameron) review before any implementation. Per the brainstorming skill's HARD-GATE, **no code is committed** until this spec is approved. The operator is driving today and unavailable for real-time dialogue, so this doc is the proposal surface.

To approve: review and either merge the PR opening this doc, or comment on what should change. Implementation work picks up after approval.

## 1. Purpose & v0.1 scope

Operator goal (verbatim): *"HTML forms was a standout in the comparison matrix and those are a must-have feature for legacy client parity. That seems to cleanly slot into the 0.1 client as it currently exists? Can we just build that?"*

**In scope for v0.1**:
1. **Render incoming form messages** in the reading pane as structured field/value lists (replacing the current "Winlink form attached" placeholder).
2. **Author one canonical form (ICS-213 General Message)** in the compose window, serialize to WLE-compatible wire format, and send to other Winlink clients (WLE + Pat) so they render it correctly.
3. **Bundle a curated set of 4–6 forms** in the tuxlink binary (ICS-213, ICS-309, Bulletin, Position Report, Damage Assessment as the seed; final set per operator review). Operator-installable third-party forms are out of scope for v0.1.

**Out of scope for v0.1** (deferred to v0.5+, with reasons):
- Dynamic forms-catalog auto-update from winlink.org (capability 4.4) — needs a forms-update endpoint contract; defer until v0.5.
- Form-data aggregator views / map view (4.7, 4.8) — power-user feature.
- ICS-309 log generation as a stand-alone tool (4.6) — related but distinct from form-render/author; tracked separately.
- WLE's embedded-WebBrowser-with-local-HTTP-server pattern — see §3 (alternative-rejected).
- Custom-XML-form sideload (4.5) — operator-confirm if v0.1 demand exists.

## 2. Background: how WLE actually works (decompiled findings)

A WLE form is **three files** on disk:

| File | Role | Used by |
|---|---|---|
| `<FormName>.txt` | Metadata + plain-text message template (`Msg:` block with `<var X>` placeholders) | FormManager (catalog enumeration) + send-time template render |
| `<FormName>_Initial.html` | Compose-side HTML with `<input name="X">` widgets and an `onsubmit` handler that POSTs to `http://{FormServer}:{FormPort}` | WLE compose flow (renders in embedded WebBrowser) |
| `<FormName>_Viewer.html` | Read-side HTML that displays the form with values filled in (read-only) | WLE read flow (renders in embedded WebBrowser) |

On the wire, a WLE form message is:

- **Message body**: plain text — the `Msg:` template with `<var X>` placeholders substituted to operator-entered values. This makes the message human-readable on clients that don't render forms (Pat, plain SMTP).
- **Attachment** named `RMS_Express_Form_<FormName>.xml`: the structured XML payload. Contains all field values in `<RMS_Express_Form>` envelope. Read-side clients use this for the structured render.

**Implication**: Tuxlink's current detection in [`parse_raw_rfc5322`](src-tauri/src/ui_commands.rs#L326) — `body.starts_with("<?xml")` — is wrong for WLE compatibility. The XML is in the attachment, not the body. The body is plain rendered text. **Fixing this detection is part of the v0.1 work.**

WLE's compose flow uses a peculiar mechanism: it renders `_Initial.html` in an embedded `WebBrowser` ActiveX control, and the HTML form `<form action="...">` POSTs to an embedded local HTTP server inside the WLE process. WLE captures the POSTed form-data, applies it to the `Msg:` template, and writes the message body + XML attachment. **This pattern is a Windows-WebBrowser-control hack that we don't need to copy** — see §3.

The form catalog itself is currently ~1000+ forms across 18 sub-categories (ICS USA, ARC/Red Cross, Weather, HICS, state-specific sets for WA/TX/NY/etc., IARU, FMRE Mexico, Radiogram/RRI, LA/HI/OR State, Mapping-GIS, Humanitarian, General Medical). WLE auto-updates this from winlink.org periodically.

## 3. Design approaches considered

### Approach A — Full WLE-format compatibility (the "ingest everything" path)

Adopt WLE's 3-file format directly. Tuxlink ships a `Standard Templates/` directory; users can drop in WLE form files unchanged. To render forms, embed a webview that loads the HTML directly with our IPC bridge replacing the local HTTP server.

**Pros**: Instant access to the 1000-form community catalog. Drop-in interop with WLE installs. Operator familiarity (looks the same).

**Cons**: 
- We inherit every WLE wart (the HTTP-server hack, JS dependencies, ActiveX-specific quirks).
- The HTML forms target Internet Explorer 11 / Windows WebBrowser control behavior — modern WebKitGTK renders some of them differently or breaks.
- Heavy infra: needs a forms-update fetcher, HTML-form-to-IPC bridge, embedded webview lifecycle.
- Hard to make UI feel native — WLE forms look like 2005-era HTML.

### Approach B — Native React forms over a canonical schema (the "clean slate" path)

Ignore WLE's HTML/JS. Define our own internal form schema (TypeScript/Rust types per form: ICS-213, ICS-309, etc.). Hand-write React form components for each. On send, serialize to WLE-compatible XML + plain-text body. On receive, parse the XML and render with the matching React component.

**Pros**: 
- Native Tauri UX, consistent styling, fast.
- Type-safe field handling on both sides.
- No webview integration headaches.
- Each form is a small focused React component — easy to test.

**Cons**: 
- Cuts us off from the community catalog. Only forms we hand-port work.
- Each new form is N hours of React work (vs. just dropping in an HTML file).
- If WLE adds a field to ICS-213, we have to update our component.

### Approach C — Hybrid: native React for bundled forms + later WLE webview compat (the recommended path)

For v0.1, do Approach B exclusively. Ship 4–6 hand-built React forms covering the canonical EmComm cases. Parse incoming XML for ANY recognized form type and render with the matching React component; render unknown forms as a structured "fields and values" key-value list (always better than raw XML).

Defer Approach A's webview-HTML-rendering to v0.5+ — at that point we have proof-of-concept that forms work and can take on the catalog scale problem.

**Pros**: Ships a working v0.1 within days, not weeks. Hand-built ICS-213 + ICS-309 + Position Report covers the 80% case. The "unknown form" fallback (structured XML display) means any WLE form is at least READABLE in tuxlink even if not pretty.

**Cons**: For v0.1, tuxlink will display fewer form types prettily than WLE does. But every form is readable; that's the v0.1 floor.

### Recommendation: **Approach C (hybrid; ship Approach B subset for v0.1, defer A to v0.5+)**

Approach C honors `discipline_triage_rule` (don't over-engineer plumbing — bundle a curated set, defer the catalog), `discipline_no_carveout_on_cross_provider_adrev` (we'll run Codex on the implementation), and the v0.1 timeline aspiration.

## 4. Chosen architecture

```
                                      ┌──────────────────────────────┐
                                      │  React frontend (Tauri webview)│
                                      │                              │
                                      │  Compose flow:               │
                                      │  • FormPicker → pick ICS-213 │
                                      │  • ICS213Form (React)        │
                                      │  • onSubmit → IPC: send_form │
                                      │                              │
                                      │  Read flow:                  │
                                      │  • MessageView detects form  │
                                      │  • lookup formType → render  │
                                      │    ICS213View (React)        │
                                      │  • unknown → KeyValueView    │
                                      └─────────────┬────────────────┘
                                                    │ Tauri IPC
                                                    │
       ┌────────────────────────────────────────────┴───────────────────────────┐
       │              Rust backend (src-tauri/src/forms/)                       │
       │                                                                        │
       │  forms::catalog::known_forms() → &[FormDef]                           │
       │  forms::serialize::to_wle_xml(form_id, fields) → (xml_bytes, body)   │
       │  forms::parse::detect_form(parsed_msg) → Option<FormPayload>         │
       │  forms::parse::parse_form_xml(xml_bytes) → FormPayload                │
       │                                                                        │
       │  Integration with ui_commands.rs:                                      │
       │  • parse_raw_rfc5322 → detect form via attachment name match           │
       │  • compose flow → new send_form command (parallel to send_message)    │
       └────────────────────────────────────────────────────────────────────────┘
                                                    │
                                                    │ wire format
                                                    ▼
                                      ┌──────────────────────────────┐
                                      │  Winlink B2F message:        │
                                      │                              │
                                      │  Body:  Msg: template text   │
                                      │         (with values subbed) │
                                      │                              │
                                      │  Attachment:                 │
                                      │  RMS_Express_Form_<name>.xml │
                                      │  containing <RMS_Express_Form>│
                                      │  <field>...</field></...>    │
                                      └──────────────────────────────┘
```

### Module layout (new files)

- `src-tauri/src/forms/mod.rs` — module root.
- `src-tauri/src/forms/catalog.rs` — bundled form definitions (struct + Vec of known forms).
- `src-tauri/src/forms/parse.rs` — detect form in an inbound message; parse XML payload.
- `src-tauri/src/forms/serialize.rs` — build XML + text body from form-field values.
- `src-tauri/src/forms/types.rs` — `FormDef`, `FormField`, `FormPayload`, `FieldKind`.
- `src-tauri/tests/forms_test.rs` — integration tests for the round-trip.

- `src/forms/types.ts` — TS mirror of `FormPayload` etc.
- `src/forms/forms.ts` — registry of known form-id → React component pairs.
- `src/forms/ics213/Ics213Form.tsx` — compose-side React component (one per form type).
- `src/forms/ics213/Ics213View.tsx` — read-side React component (one per form type).
- `src/forms/KeyValueView.tsx` — fallback display for unknown forms.
- `src/forms/FormPicker.tsx` — compose-side picker (which form to author).

### Integration points (modify existing files)

- `src-tauri/src/ui_commands.rs`:
  - Fix `is_form` detection (look at attachment names matching `RMS_Express_Form_*.xml`, not body prefix).
  - Add `send_form` Tauri command (parallel to `send_message`) accepting `(form_id, field_values, to, cc, subject)` — does the serialize + build-message work in Rust, calls existing `send_message` infra under the hood.
- `src/mailbox/MessageView.tsx`:
  - When `message.isForm` is true, look up the form by ID, render with matching React component or KeyValueView fallback.
- `src/compose/Compose.tsx`:
  - Add a "Compose form" button that opens FormPicker → selected form → form's React component (Ics213Form etc.).
- `src/mailbox/replyActions.ts`:
  - Already handles form-XML safely (won't quote raw payload on reply/forward). No change needed; existing tests stay green.

## 5. Data model

### Rust types

```rust
// src-tauri/src/forms/types.rs

/// One Winlink form definition known to tuxlink.
pub struct FormDef {
    /// Canonical form ID (e.g. "ICS213", "ICS309", "Position"). Matches WLE
    /// form-name prefix in the `RMS_Express_Form_<id>.xml` attachment name.
    pub id: &'static str,
    /// Display name (e.g. "ICS-213 General Message").
    pub name: &'static str,
    /// Field schema in declaration order. Used to render forms with unknown-
    /// to-tuxlink fields (e.g. an updated WLE field we don't yet know) by
    /// falling back to the KeyValueView with whatever fields the XML contains.
    pub fields: &'static [FormField],
    /// Subject-line template (with %field% substitutions).
    pub subject_template: &'static str,
    /// Plain-text Msg: template (with %field% substitutions). Sent as the
    /// human-readable message body for clients that don't render forms.
    pub body_template: &'static str,
}

pub struct FormField {
    pub id: &'static str,         // matches XML element name
    pub label: &'static str,      // human label for the compose UI
    pub kind: FieldKind,
    pub required: bool,
    pub max_length: Option<usize>,
}

pub enum FieldKind {
    Text,
    LongText,         // multi-line
    Date,
    Time,
    Boolean,          // checkbox; rendered as "Yes" / "No" in XML+text
}

/// A parsed form payload (XML decode result, or compose-time pre-send state).
pub struct FormPayload {
    pub form_id: String,                     // from attachment name
    pub fields: Vec<(String, String)>,       // (id, value) — preserves XML order
}
```

### Wire format (WLE-compatible)

For compose-time send of ICS-213:

```
Message body (text):
GENERAL MESSAGE (ICS 213)
1. Incident Name: HURRICANE WALDO RESPONSE
2. To (Name and Position): JOHN OPERATOR, EOC Comms
3. From (Name and Position): JANE OPERATOR, Field Net Control
4. Subject: REQUEST EXTRA MEDICAL SUPPLIES
...
[Sent via Tuxlink v0.3.0]

Attachment: RMS_Express_Form_ICS213.xml (MIME: application/xml or text/xml)
<?xml version="1.0" encoding="utf-8"?>
<RMS_Express_Form>
  <form_parameters>
    <xml_file_version>1.0</xml_file_version>
    <tuxlink_version>0.3.0</tuxlink_version>
    <form_version>ICS213/1.0</form_version>
  </form_parameters>
  <variables>
    <inc_name>HURRICANE WALDO RESPONSE</inc_name>
    <to_name>JOHN OPERATOR, EOC Comms</to_name>
    <fm_name>JANE OPERATOR, Field Net Control</fm_name>
    <Subjectline>REQUEST EXTRA MEDICAL SUPPLIES</Subjectline>
    <Mdate>2026-05-30</Mdate>
    <mtime>14:30Z</mtime>
    <Message>Need additional bandages and IV bags at field station 3...</Message>
    <Approved_Name>JANE OPERATOR</Approved_Name>
  </variables>
</RMS_Express_Form>
```

**Notes on the XML envelope:**
- `<form_parameters>` block is WLE convention. We include `<tuxlink_version>` (parallel to WLE's `<rms_express_version>`) for forensics — if a receiving WLE operator sees a malformed form, they can blame us by version.
- `<form_version>` lets us version-bump field schemas without breaking older receivers.
- Field names in the XML match the WLE field names (e.g., `inc_name`, `Subjectline`, `Mdate`, `mtime`) — verified by reading the actual `ICS213_Initial.html` field decls. This is the parity gate: WLE must render our XML correctly.

## 6. UI surfaces

### 6.1 Compose flow

Add a new entry point in the compose window: a button labeled **"Compose form…"** next to the existing "New message" entry. Clicking opens `FormPicker`.

```
┌─ FormPicker (modal / dropdown) ─────────────────┐
│ Pick a form to author:                          │
│                                                 │
│ ▸ ICS-213 General Message                       │
│ ▸ ICS-309 Communications Log                    │
│ ▸ Position Report                               │
│ ▸ Bulletin (information broadcast)              │
│ ▸ Damage Assessment                             │
│                                                 │
│              [Cancel]    [Use selected form]   │
└─────────────────────────────────────────────────┘
```

Selecting a form replaces the Compose body with that form's React component (e.g., `Ics213Form`). The To/Cc/Subject row above stays; the body region is replaced.

```
┌─ Ics213Form (in Compose body region) ───────────┐
│ Incident Name:    [HURRICANE WALDO RESPONSE  ]  │
│                                                 │
│ To (Name+Pos):    [JOHN OPERATOR, EOC Comms  ]  │
│ From (Name+Pos):  [JANE OPERATOR, Net Control]  │
│                                                 │
│ Subject:          [REQUEST EXTRA MEDICAL SUP ]  │
│                                                 │
│ Date: [2026-05-30]   Time: [14:30Z]             │
│                                                 │
│ Message:                                        │
│ ┌─────────────────────────────────────────────┐ │
│ │Need additional bandages and IV bags at      │ │
│ │field station 3 by 1700Z. Confirm receipt.   │ │
│ └─────────────────────────────────────────────┘ │
│                                                 │
│ Approved by:      [JANE OPERATOR             ]  │
│ Position/Title:   [Field Net Control         ]  │
│                                                 │
│         [Discard form]    [Send (Ctrl+Enter)]  │
└─────────────────────────────────────────────────┘
```

On submit: serialize via `forms::serialize::to_wle_xml`, build outbound message with XML attachment + text body, hand to existing send infra.

### 6.2 Read flow

When `message.isForm` is true (newly correct after detection fix):

1. Look up the form ID from the attachment name (`RMS_Express_Form_ICS213.xml` → `ICS213`).
2. Fetch and parse the XML attachment.
3. If `ICS213` is a known form: render `Ics213View` with the field values.
4. Else: render `KeyValueView` with whatever field/value pairs the XML contained.

```
┌─ Ics213View (in reading pane) ──────────────────┐
│ 📋 ICS-213 General Message · v1.0               │
│ ─────────────────────────────────               │
│ Incident:  HURRICANE WALDO RESPONSE             │
│                                                 │
│ To:        JOHN OPERATOR, EOC Comms             │
│ From:      JANE OPERATOR, Field Net Control     │
│                                                 │
│ Date:      2026-05-30 · Time: 14:30 UTC         │
│                                                 │
│ Subject:   REQUEST EXTRA MEDICAL SUPPLIES       │
│                                                 │
│ Message:                                        │
│   Need additional bandages and IV bags at field │
│   station 3 by 1700Z. Confirm receipt.          │
│                                                 │
│ Approved by:  JANE OPERATOR                     │
│ Position:     Field Net Control                 │
│                                                 │
│ ─────────────────────────────────               │
│ [Reply with form]   [Reply as plain text]      │
└─────────────────────────────────────────────────┘
```

Replies/forwards continue to use the existing `replyActions.ts` safety pattern (never quote raw form XML in the reply body).

### 6.3 KeyValueView (unknown form fallback)

```
┌─ KeyValueView ──────────────────────────────────┐
│ 📋 Unknown form: <attachment-name>              │
│ ─────────────────────────────────               │
│ <field1>:  <value1>                             │
│ <field2>:  <value2>                             │
│ ...                                             │
│                                                 │
│ The form's specific renderer is not bundled in  │
│ this tuxlink version. Above is the raw field    │
│ data from the XML payload.                      │
│                                                 │
│ Message body (sender's text rendering):         │
│ ┌─────────────────────────────────────────────┐ │
│ │<rendered body>                              │ │
│ └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

The unknown-form case is the "graceful degradation" floor — every WLE form is at minimum readable in tuxlink, even if not visually pretty.

## 7. Catalog management — bundled vs dynamic

For v0.1: **bundled**. Form definitions live in `src-tauri/src/forms/catalog.rs` as `const` arrays. The seed set:

| Form ID | Name | Reason |
|---|---|---|
| `ICS213` | ICS-213 General Message | The canonical EmComm general message — most-used form in nets and incident dispatches. |
| `ICS309` | ICS-309 Communications Log | Standard end-of-incident reporting. |
| `Position` | GPS Position Report | The structured QTH-update message; also the v0.1 capability 3.3 gap. |
| `Bulletin` | Bulletin (broadcast) | Common for net announcements / SHARES / RACES. |
| `DamageAssessment` | Damage Assessment | Most common ARC/disaster-response form. |

(Final list pending operator confirmation; the recommendation above is based on EmComm prevalence and form-type coverage variety.)

**Why bundled**: zero infra (no fetcher, no version-check, no fallback for catalog-fetch failure). 4–6 hand-built forms gets us 80% of EmComm use-cases. Operator demand will tell us when to add more.

**Future: dynamic catalog** (v0.5+): fetch from winlink.org or our own forms repo. Form definitions might be JSON files in a shared `forms/` dir, loaded at startup. Defer the format design until we ship v0.1.

## 8. Boundaries with existing tuxlink code

| Existing surface | Change |
|---|---|
| `parse_raw_rfc5322` | Fix `is_form` detection: look at attachments for `RMS_Express_Form_*.xml`, not body for `<?xml` prefix. Track the form ID in a new `ParsedMessageDto.form_id: Option<String>`. |
| `ParsedMessageDto` | Add `form_id: Option<String>` field. |
| `MessageView.tsx` | Replace the existing form-placeholder block with form-type lookup + render dispatch (FormViewByType / KeyValueView / fallback). |
| `replyActions.ts` | No change — the existing "don't quote form XML" safety logic remains correct (we still detect `isForm` and skip body quoting). |
| `Compose.tsx` | Add a "Compose form" entry point. |
| `Tauri invoke handler` | Register `send_form` command. |

**No deletions, no refactors of unrelated code.** The forms code is purely additive.

## 9. Testing strategy

| Test | Coverage |
|---|---|
| **Rust unit** — `forms::serialize::to_wle_xml(ICS213, {field: value, ...})` | Generates byte-perfect WLE-compatible XML for a known field map. |
| **Rust unit** — round-trip parse | `serialize → parse → equal field map`. Catches encoding/escape/CDATA bugs. |
| **Rust unit** — `forms::parse::detect_form` | Returns Some(form_id) for `RMS_Express_Form_*.xml` attachment; None otherwise. |
| **Rust integration** — full message build | Compose a form → serialize → send through `send_message` infra → received raw bytes parse into expected RFC5322 + attachment shape. |
| **Vitest** — `Ics213Form` renders all required fields | All inputs present, required-field validation. |
| **Vitest** — `Ics213View` renders parsed payload | Given a FormPayload, all field/value pairs displayed. |
| **Vitest** — `KeyValueView` fallback | Given an XML payload with unknown fields, all fields displayed. |
| **Vitest** — `Compose → Form picker → Ics213Form → submit` | End-to-end form-author UX via React Testing Library. |
| **Vitest** — `MessageView` form-render dispatch | Given `message.isForm=true` + a known ICS213 attachment, renders `Ics213View`. Unknown form renders `KeyValueView`. |
| **Live-receive smoke** (operator) | Send a tuxlink-composed ICS-213 to a WLE install, verify WLE renders it correctly in its viewer. Send a WLE-composed ICS-213 to tuxlink, verify tuxlink renders it correctly. Cross-client parity gate. |

The live-receive smoke is the **gate**: ICS-213 round-trips between tuxlink and WLE without information loss. If WLE refuses to render our XML, the spec is wrong.

## 10. Codex round (cross-provider review)

Per `feedback_no_carveout_on_cross_provider_adrev` and the Winlink-protocol-adjacent nature of this work (XML envelope shape, WLE compatibility), the implementation gets a Codex review round before PR submission. Specifically:

- Attack angles: WLE compatibility (does our XML pass WLE's parser?), field-name accuracy (did we miss a required WLE field?), edge cases in the round-trip (long text, special characters, empty/missing fields), reply/forward safety (don't quote XML in replies — existing pattern preserved).

## 11. Migration plan for the existing `is_form` detection bug

The existing detection — `body.starts_with("<?xml")` — is WRONG for WLE-format messages (XML is in attachment, not body). Some tuxlink dev fixtures use the wrong format. Migration:

1. Update `parse_raw_rfc5322`:
   - Old: `let is_form = body.trim_start().starts_with("<?xml");`
   - New: detect form via attachments — `is_form = attachments.iter().any(|a| a.filename.starts_with("RMS_Express_Form_") && a.filename.ends_with(".xml"))`.
   - Also set `form_id` to the attachment's parsed ID.
2. Update `MessageView.tsx` form-detection — uses the new `form_id` field.
3. Update dev fixtures (`devFixture.ts`, `replyActions.test.ts`) to use the correct format (XML in attachment, plain text in body).
4. Existing tests (`MessageView.test.tsx`, `replyActions.test.ts`) get a passes-after-fix update.

This migration lands in the same PR as the new forms surface. There's no v0.0.X user data at risk (no real messages have the wrong format in the wild yet).

## 12. Risks & mitigations

| Risk | Mitigation |
|---|---|
| WLE's XML field names don't exactly match what we generate → other clients can't render | Reading the ICS213 `_Initial.html` directly to verify field names; round-trip test with a real WLE install (operator-validated smoke). |
| Our form schema misses optional WLE fields → looks "wrong" to a WLE operator | Bundle the full WLE field set per form; render missing values as blank, present values as filled. Cross-check against `<FormName>_Viewer.html`. |
| 4-6 forms doesn't cover the operator's actual EmComm use-case | KeyValueView fallback ensures every form is readable. Operator feedback drives the v0.1.1+ form additions. |
| Some WLE forms use rich HTML (tables, images, JS) we don't replicate in React | Defer those; v0.1's hand-built set is text-field-only forms (ICS-213, ICS-309, Position Report, Bulletin, Damage Assessment all fit). v0.5+ could add a webview rendering path. |
| `parse_raw_rfc5322` change breaks existing `MessageView.test.tsx` | Tests are updated as part of this PR; the migration §11 is explicit. |
| Reply-quote regression — replying to a form leaks form data into the reply body | Existing `replyActions.ts` safety preserved by tests (won't quote raw XML); add a test specifically for a form reply with the new format. |

## 13. Open questions for operator review

1. **Form seed list** — is `ICS213 / ICS309 / Position / Bulletin / DamageAssessment` the right 5, or do you want a different mix? (E.g., HICS forms for hospital ops; ARC1077 for Red Cross damage; Radiogram.)
2. **Position Report**: WLE has a dedicated `PositionReport` Form and ALSO the position-report-message capability (§3.3 in the inventory). Build them as the same UX (form) or different UX (button on the dashboard)?
3. **Reply-with-form behavior**: when you reply to an incoming ICS-213, should the default be (a) plain text reply, (b) auto-open an ICS-213 reply form pre-populated with the original subject+from?
4. **Wire-format minor details**: the XML `<form_parameters>` block — should we mimic WLE's exact element names (`<rms_express_version>` etc.) for max compatibility, or use our own (`<tuxlink_version>`) for honesty? Either works; preference?
5. **Compose entry point**: "Compose form…" as a button in the main compose window, OR as a separate menu/dropdown? (Tuxlink's UI conventions don't have menus yet, so a button feels right — confirming.)

These are conceptual design calls, not implementation details. The implementation plan (next via writing-plans skill) defers atomic decisions per `feedback_no_atomic_decisions_to_operator`.

## 14. Approval ask

Per the brainstorming HARD-GATE: this is design-only, no code written. Operator approval flow:

- **Approve** → merge this PR. I'll proceed via `superpowers:writing-plans` to break the work into an implementation plan, then execute (TDD + Codex round + impl PR).
- **Request changes** → comment on what should change. I'll iterate.
- **Reject the whole approach** → say so; I'll regroup before any code.

Estimated implementation effort once approved: 4-6 days (Rust forms module + 5 React components + tests + Codex round + cross-client smoke).

Agent: yew-cypress-oak
