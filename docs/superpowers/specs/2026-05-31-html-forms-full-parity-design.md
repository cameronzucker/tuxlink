# HTML Forms full WLE-parity — design

> bd: `tuxlink-izgv` · Designed: 2026-05-31 · Agent: `dahlia-heron-spruce`
>
> Supersedes [`docs/superpowers/specs/2026-05-30-html-forms-design.md`](2026-05-30-html-forms-design.md)
> (which described the v0.1 native-React-only design that shipped in PR #177).

## 0. Status — DESIGN ONLY (HARD-GATE per `superpowers:brainstorming`)

No code work begins until this design is approved + per-phase implementation
plans are written via `superpowers:writing-plans`. Per-phase plans land as
separate bd issues + separate PRs.

## 1. Change log

### rev-1 (2026-05-31 — first cut after operator critique of v0.1)

Initial design. Captures the full conversational brainstorm that started from
the operator's pushback on PR #177 forms: *"imitations of form rather than
imitations of function."*

## 2. Purpose & scope

**Target: full WLE feature parity for forms.** A tuxlink operator can do
anything a WLE operator can do with forms — fill in any form WLE bundles,
receive and render messages with any form attachment, drop a custom HTML form
into a directory and have it Just Work, refresh the bundled catalog from
winlink.org.

**Why this scope** (operator framing 2026-05-31):
*"Forms work must be done anyway since they're load-bearing in the existing
WLE community. Can we not just do the whole thing?"* Half-scoped forms become
a recurring user complaint and a UX-credibility hit; doing the architectural
work once for the full catalog avoids a long-tail of "tuxlink doesn't do
HICS / Radiogram / ARC" complaints that would each require fresh native React
work under v0.1's design.

**Background — why v0.1 missed the mark**: PR #177 treated forms as a
wire-format compatibility problem and solved that problem correctly (XML
envelope, body template, encoding quirks against the WLE decompile + Pat
source). What it didn't ask: *how does the operator actually create the
data that fills these fields?* For ICS-213 and Bulletin the implicit answer
("fill in inputs") is correct. For GPS Position Report (data sits in
`PositionArbiter` waiting to be pulled), ICS-309 Comms Log (aggregated from
`messages_meta` over a time window — manually typing 30 log entries is an
emcomm error magnet), and Damage Assessment (could pull incident metadata
if we had it), fill-in is the wrong UX entirely.

The deeper pattern (worth a pitfall entry): **wire-format compatibility ≠
operational correctness**. Field schema doesn't dictate UX.

## 3. Decisions captured in this design

| Decision | Resolution | Confirmed via |
|---|---|---|
| Parity scope | Full WLE forms surface | Operator: *"do the whole thing"* |
| Render architecture | Hybrid: 5 native React forms + embedded child webview for everything else | Operator selection 2026-05-31 |
| Native form set | ICS-213, Bulletin, GPS Position Report, ICS-309 Comms Log, **Winlink Check-In** (new) | Hamexandria research + operator review |
| Template source | Bundle WLE Standard Forms snapshot in binary; in-app auto-update from winlink.org | Operator selection 2026-05-31 |
| Webview hosting | Embedded child webview replaces compose body region (one-window UX); honors `feedback_inline_ui_no_window_clutter` | Operator selection 2026-05-31 |
| Form-submit capture | **Lazy** `127.0.0.1:0` HTTP server (only running while a form is open) with WLE `{FormServer}/{FormPort}/{FormFolder}` substitution + per-open random token + no IPC exposed to child webview | Codex adversarial review reversed initial Tauri-custom-protocol recommendation (see [`dev/adversarial/2026-05-31-form-submit-capture-codex.md`](../../../dev/adversarial/2026-05-31-form-submit-capture-codex.md)) |
| Phasing | 4 phases (P0 land valid #177 subset; P1 webview infra; P2 native auto-fill forms; P3 catalog freshness) | Operator selection 2026-05-31 |

## 4. The native form set

| Form | Native justification | Phase introduced |
|---|---|---|
| **ICS-213 General Message** | Already native in PR #177; truly fill-in; tuxlink-theme continuity | P0 (preserved from #177) |
| **Bulletin** | Already native in PR #177; truly fill-in | P0 (preserved from #177) |
| **GPS Position Report** | Pull from `PositionArbiter` (existing gpsd client); map widget for override; one-click send | P2 (rebuild) |
| **ICS-309 Comms Log** | Time-range picker; query `messages_meta`; auto-aggregate; preview-before-send | P2 (rebuild) |
| **Winlink Check-In** ⬅ new | High-frequency net participation; GPS auto-fill + per-form save/reuse slot library; explicitly called out in Hamexandria tutorial corpus | P2 (new) |

Damage Assessment moves to the webview path (operators use it rarely; hand-fill
is fine via WLE HTML + tuxlink CSS skin; if we add "active incident" state
later we can elevate it).

Everything else (HICS, ARC, Radiogram, ICS-205, …) ships via webview.

## 5. Architecture

### 5.1 Rendering modes in the compose body

The existing compose window has three mutually-exclusive rendering modes for
its body region:

1. **Plain textarea** — for free-text messages (existing)
2. **Native React form** — one of the 5 forms above (P0 keeps ICS-213+Bulletin; P2 adds the other three)
3. **Embedded child webview** — for everything else, including custom forms (P1+)

The form picker is the entry switch between them. The compose chrome
(titlebar, recipients, cc, subject, attachments, send-action-bar) is
identical across all three modes.

### 5.2 Wire-format pipeline (one path for all rendering modes)

Every form, regardless of how it was rendered, produces a `FormPayload` (the
type from PR #177's `forms::types`). This goes through the existing
`forms::serialize` to build the WLE-compatible XML envelope and rendered text
body, then attaches to an `OutboundMessage` via the existing
`compose_message_with_files` native B2F path.

**No new wire format work**. PR #177's `parse.rs`, `serialize.rs`, `types.rs`,
`catalog.rs`, `validation.rs` are reused as-is.

### 5.3 Embedded HTTP server — lazy lifecycle + WLE substitution

The webview-path forms need a real HTTP origin (per Codex adrev §5.3) because
WLE's Standard Forms templates use the `{FormServer}:{FormPort}{FormFolder}`
substitution convention; matching the WLE ecosystem contract requires giving
forms a real port to POST to.

**Lifecycle:**
- Binds `127.0.0.1:0` (kernel-assigned ephemeral port) when an operator opens
  a webview-form.
- Tears down when the operator dismisses the form (cancel, successful submit,
  or compose window close).
- **Not persistent** — addresses operator's stated port-discipline concern
  for pandora-services-style envs.

**Hardening (per Codex recipe):**
- Per-open random token in the form URL and POST target: form serves at
  `http://127.0.0.1:<port>/forms/<token>/<id>`; POST goes to
  `http://127.0.0.1:<port>/submit/<token>`. Token regenerates per open; a
  same-machine probe during the brief window without the token gets 403.
- Serves only the selected form + normalized allowlisted adjacent assets (CSS
  skin, fallback submit JS). No path traversal; no arbitrary file serving.
- **No Tauri IPC exposed to the child form webview.** The form webview is
  isolated; it talks to tuxlink only via the HTTP server, not via IPC. An
  XSS payload in custom-form HTML cannot reach tuxlink's command surface.
- Parses real `application/x-www-form-urlencoded` and `multipart/form-data`,
  preserving repeated field names (checkboxes, multi-selects, table rows
  with duplicate names) and submitter button value (WLE distinguishes Submit
  vs Cancel via `name="Submit"` button value).
- Diagnostics: developer-mode "Show URL / copy curl / save captured POST"
  surfaces for debugging field-emcomm issues.

### 5.4 Why not Tauri custom protocol + JS injection

This was the initial recommendation; Codex adrev (2026-05-31) reversed it for
the following concrete reasons:

- **WLE ecosystem contract**: WLE templates use `{FormServer}:{FormPort}`
  substitution. Custom protocol can't substitute these (no port exists).
  Custom-form authors who target the WLE convention would have their forms
  break in tuxlink while working in WLE/Pat.
- **JS-injection brittleness** (concrete cases): submit-time materialization
  (`onsubmit="hidden.value = buildPayload()"` — fallback button reads DOM
  before this runs), programmatic `form.submit()` (doesn't fire submit
  event), `Object.fromEntries(new FormData(...))` data loss for repeated
  field names, file input handling, rich/custom controls invisible to
  FormData, iframed forms unreachable from top-frame eval.
- **WebKitGTK 2.40+ requirement** for custom-protocol POST body access; Wry
  gates this behind `linux-body` feature. On the Pi target this is a spike,
  not a foundation.

Option A's DOM extractor is preserved as a **diagnostic / rescue tool** in
developer mode, not the canonical submit path.

### 5.5 Skin CSS

A `forms/skin.rs`-generated stylesheet is injected into every webview-rendered
form. Overrides:
- Background, text color to tuxlink dark-theme palette
- Input / textarea / select styling to match tuxlink's compose inputs
- Submit / Cancel buttons to match tuxlink's amber accent
- Table styling for ICS-309-style data tables in viewers

The skin uses `:where()` for zero specificity so any inline styles in the
template still win where they have specific intent. Aggressive enough to feel
tuxlink-native; conservative enough to not break form layouts.

### 5.6 Tauri capabilities

A new `forms-webview.json` capability scoped to the child webview's label
pattern (`compose-form-*`). Grants:
- HTTP fetch to the loopback server (the form needs to load its own assets)
- **Nothing else**. Crucially: no IPC, no window control, no fs access.

The form-server origin is constrained via Tauri's capability mechanism so
even if a custom form embeds a malicious URL, the webview can't reach
anything outside its allowed origin set.

## 6. Phasing

### Phase 0 — Resolve PR #177

**Goal**: ship the v0.1 work minus the misshapen forms; don't carry broken UX
into the new design.

**Keep**:
- Wire-format machinery (`forms::parse`, `serialize`, `types`, `catalog`,
  `validation`)
- `OutboundMessage::attachments` field + `compose_message_with_files` native
  B2F send (already landed in ADR 0016 PR)
- ICS-213 native React compose + view
- Bulletin native React compose + view
- All `*View` components (so received forms of any bundled type still render
  via native View when available)
- Compose window controls + capabilities + CSS fixes from this session

**Remove from picker (keep View)**:
- GPS Position Report compose form
- ICS-309 Comms Log compose form
- Damage Assessment compose form

**Rationale**: their compose UX is wrong and shipping it sends a "tuxlink
forms are broken" signal. Their View components stay so received messages
render correctly. P2 rebuilds Position + ICS-309 with the right UX; Damage
Assessment moves to webview-default in P1.

### Phase 1 — Webview infrastructure (the foundational mile)

**Goal**: every WLE form works in tuxlink, rendered as WLE HTML with tuxlink
CSS skin. From this phase onward operators have full catalog coverage.

**Deliverables**:
- Bundled WLE Standard Forms snapshot in the binary (chosen version pinned;
  bundle-size budget decided in plan)
- `forms::templates` — bundled + custom template enumeration + `{FormFolder}`
  resolution
- `forms::skin` — tuxlink CSS asset generation
- `forms::http_server` — lazy `127.0.0.1:0` axum server (per §5.3 hardening)
- `forms-webview.json` capability (per §5.6)
- React `WebviewFormHost` — child webview embed + tuxlink-chrome fallback
  submit button
- React `CatalogBrowser` — replaces flat `FormPicker` with hierarchical
  catalog (WLE-style folders: Standard / General / ICS / HICS / ARC /
  Custom); both tree and search UIs (specifics in plan)
- Custom-forms directory enumeration (`~/.local/share/tuxlink/forms/custom/`
  default; operator-overridable)
- Receive-side fallback: render unknown-type received forms via the same
  webview path (loading the WLE Viewer HTML)

### Phase 2 — Native auto-fill / auto-gen forms

**Goal**: the 3 forms where tuxlink has data WLE doesn't get hand-crafted
native UX.

**Deliverables**:
- **GPS Position Report (native)**: pull from `PositionArbiter`; map widget
  for override (TBD: which map lib — leaflet w/ offline tiles?); one-click
  send.
- **ICS-309 Comms Log (native)**: time-range picker; `messages_meta` query;
  preview pane showing aggregated rows; submit emits the standard XML
  attachment, optionally also attaching a CSV/PDF (WLE generates both —
  TBD which we ship in P2 vs P3).
- **Winlink Check-In (native, new)**: GPS auto-fill; per-form save/reuse
  slot library (`FormDraftLibrary`) — operators routinely participate in
  multiple weekly nets and save filled-in templates labeled by net.

### Phase 3 — Catalog freshness + custom-form ergonomics

**Goal**: keep the catalog fresh from winlink.org; promote the save/reuse
library beyond Check-In.

**Deliverables**:
- `forms::updater` — winlink.org Standard Forms zip pull, integrity check,
  atomic snapshot swap with rollback on bad zip
- In-app "Refresh forms" action with operator confirmation before applying
- `FormDraftLibrary` extended across all native forms (not just Check-In)
- Form-aware reply via WLE `_SendReply.0` templates (currently plain-text
  per PR #177; this is the operationally-correct path to support per-form
  replies operators expect from WLE)
- PDF export for ICS-309 (deferred from P2 if not shipped there)

## 7. Components by phase

### New / modified Rust modules

| Module | Phase | Purpose |
|---|---|---|
| `forms/templates.rs` | P1 | Bundled + custom template enumeration; `{FormFolder}` resolution |
| `forms/skin.rs` | P1 | tuxlink CSS skin asset |
| `forms/http_server.rs` | P1 | Lazy `axum` server: GET `/forms/<token>/<id>`, GET `/skin.css`, GET `/bridge.js`, POST `/submit/<token>`; lifecycle scoped to single form-open session |
| `forms/multipart.rs` | P1 | Parse `application/x-www-form-urlencoded` + `multipart/form-data` preserving repeated names + submitter |
| `forms/updater.rs` | P3 | winlink.org zip pull + atomic swap + rollback |

### New / modified React components

| Component | Phase | Purpose |
|---|---|---|
| `compose/CatalogBrowser.tsx` | P1 | Replaces `FormPicker`; hierarchical tree + search |
| `compose/WebviewFormHost.tsx` | P1 | Mounts child webview; renders tuxlink-chrome fallback submit button below it |
| `compose/CheckInForm.tsx` | P2 | Native Winlink Check-In |
| `compose/PositionFormV2.tsx` | P2 | Native rebuild (gpsd pull + map widget) |
| `compose/Ics309FormV2.tsx` | P2 | Native rebuild (time-range + log aggregation + preview) |
| `compose/FormDraftLibrary.tsx` | P2 (Check-In), generalized P3 | Save/reuse slot library UI |

### Existing components preserved (P0 keep-list)

- `forms/parse.rs`, `serialize.rs`, `types.rs`, `catalog.rs`, `validation.rs`
- `compose/ics213/Ics213Form.tsx` + `Ics213View.tsx`
- `compose/bulletin/BulletinForm.tsx` + `BulletinView.tsx`
- All `*View` components (Ics213View, Ics309View, BulletinView, PositionView,
  DamageAssessmentView, KeyValueView)
- `forms/FormPicker.tsx` (will be deprecated/replaced when CatalogBrowser
  lands in P1; kept short-term for the 2 native forms P0 ships)

## 8. Data flow

### 8.1 Native path (P0 + P2)

```
operator picks form in CatalogBrowser
   → React routes to native form component
   → operator fills, clicks Send
   → onSubmit → existing send_form Tauri command
   → forms::serialize::build_xml_attachment
   → OutboundMessage { body=rendered_text, attachments=[xml] }
   → compose_message_with_files (existing native B2F)
   → outbox → CMS / RF
```

### 8.2 Webview path (P1+)

```
operator picks form in CatalogBrowser
   → React → invoke('open_webview_form', { form_id })
       → Rust spawns forms::http_server on 127.0.0.1:0
       → generates per-open token
       → returns { url, port, token }
   → React WebviewFormHost mounts <webview src=url>
       → webview navigates to http://127.0.0.1:<port>/forms/<token>/<id>
       → http_server reads template + substitutes
         {FormServer}=127.0.0.1, {FormPort}=<port>,
         {FormFolder}=/forms/<token>/<form-folder>
       → injects <link rel=stylesheet href=/skin.css>
       → injects <script src=/bridge.js> (diagnostic fallback only)
       → returns HTML
   → operator fills form, clicks form's native submit
       → POST http://127.0.0.1:<port>/submit/<token>
       → http_server validates token, parses body
       → builds FormPayload (preserves repeated names, submitter)
       → calls forms::serialize::build_xml_attachment
       → emits IPC event with FormPayload to parent compose window
       → returns 200 + small "submitted" HTML page
   → React Compose receives event, updates outbound state
   → React dismisses WebviewFormHost
   → React → invoke('close_webview_form_server')
       → Rust http_server teardown
   → existing Send flow takes over
```

### 8.3 Receive path (all forms, P1+)

```
inbound message arrives via existing native B2F
   → existing parse_raw_rfc5322 extracts attachments + form_id
   → forms::parse extracts FormPayload from XML attachment
   → ParsedMessageDto carries form_id + form_payload to React
   → MessageView lookup_form(form_id):
       - if native View exists (ICS-213, ICS-309, Bulletin, Position,
         Damage Assessment, KeyValueView fallback) → render native View
       - else → render webview-fallback Viewer (P1+):
           - spawn http_server (read-only mode; no submit endpoint)
           - load WLE Viewer HTML for the form_id with FormPayload
             injected as form values via JS bridge
```

## 9. Error handling

| Failure | Handling |
|---|---|
| `127.0.0.1:0` bind fails | Surface "form server unavailable; native forms still work" error; native path still serves ICS-213/Bulletin/Check-In/etc. |
| Template not found (catalog drift, custom-form missing) | 404 in webview → fallback to KeyValueView with raw payload |
| Submit POST malformed | Capture POST to `dev/scratch/` for debug (dev mode only); show "submit failed, retry" toast in compose; preserve form draft |
| Token mismatch on submit | 403; log; require operator re-open form |
| WLE Standard Forms zip download fails (P3) | Operator stays on bundled snapshot; "Update available" badge stays in UI; retry on next operator action |
| Atomic-swap failure mid-update (P3) | Rollback to prior snapshot; surface error; operator can retry |
| Custom-form HTML with XSS payload | Webview is isolated — no IPC, scoped capability, scoped origin allowlist. The blast radius is the webview itself, which gets discarded on close. tuxlink Compose, mailbox, settings all out of reach. |

## 10. Security model

**Threat model**: a malicious or buggy custom-form HTML file authored by an
operator (or downloaded from a peer) tries to escape its webview and reach
tuxlink internals (compose state, message archive, credentials, GPS, …).

**Mitigations**:
1. Child webview gets `forms-webview.json` capability with **no Tauri IPC**,
   no fs access, no window control. The form webview talks to tuxlink only
   via the loopback HTTP server.
2. HTTP server's origin is `127.0.0.1:<random_port>`; webview is allowed
   only that origin via Tauri's URL allowlist.
3. Per-open random token in URL + POST target; an attacker proxying
   `127.0.0.1:<port>` doesn't know the token without intercepting the
   `open_webview_form` IPC response (which is in-process and not network-visible).
4. HTTP server serves only allowlisted paths (`/forms/<token>/<id>`,
   `/skin.css`, `/bridge.js`, `/submit/<token>`). No directory traversal.
5. CSP on form-server origin: `script-src 'self' 'unsafe-inline'`
   (`unsafe-inline` is required because WLE templates have inline `<script>`
   blocks; this is constrained to the form-server origin which has no
   tuxlink IPC reach).
6. Submitted form data is validated against `FormDef` (existing PR #177
   validation) before being incorporated into `OutboundMessage`.

**Out of scope**: defending against an operator who deliberately runs a
custom form they know is malicious. The threat is "operator unintentionally
runs a form that becomes malicious due to a bug or compromise upstream."

## 11. Testing strategy

| Layer | Tests |
|---|---|
| Native React forms (P0 + P2) | vitest as today; CSS-blindness is a known limit (cf. TEST-1 pitfall PR #179); browser smoke before each phase ships |
| Webview asset serving | Rust unit tests on `http_server` route handlers using `axum::oneshot`; integration test against a fixture form (bundled ICS-213 HTML works for this) |
| Submit capture | Rust unit on multipart + urlencoded parsing (preserves repeated names, submitter value); round-trip test FormPayload → serialize → parse for byte fidelity |
| Custom-form discovery | Rust unit on `templates.rs` enumeration of bundled + custom dirs; resolution of `{FormFolder}` for both |
| Catalog updater (P3) | Mock winlink.org HTTP response; verify atomic swap and rollback on corrupted zip |
| Capability scoping | Rust unit on capability ACL — the form-webview capability has no IPC entries; assertion-driven test |
| End-to-end | Operator browser smoke before each phase ships (cf. `feedback_browser_smoke_before_ship`); pre-merge Codex adrev each phase per existing project discipline (cf. `feedback_no_carveout_on_cross_provider_adrev`) |

## 12. Deferred to per-phase implementation plans

These ARE design decisions but they're per-phase detail; will be resolved
in the writing-plans output for each phase:

- **Catalog browser UX specifics** (P1): WLE-style nested folders vs flat
  with search vs both; visual treatment of folder vs leaf entries; how
  custom forms appear in the tree
- **Bundled snapshot details** (P1): which WLE Standard Forms version pin;
  bundle size budget; integrity hash check at runtime
- **Custom-forms directory location** (P1): default
  `~/.local/share/tuxlink/forms/custom/`; operator-overridable via Settings;
  hot-reload vs restart-to-pick-up
- **Skin CSS scope** (P1): exact override list; which WLE-template styles to
  preserve; theme variables exposed for operator tweaking
- **Map widget choice for Position Report** (P2): leaflet + offline tile
  cache vs maplibre vs something simpler; bundle-size implications
- **PDF vs CSV emit for ICS-309** (P2 or P3): WLE generates both; which
  ships when; PDF generation library choice
- **Form draft library schema** (P2): per-form-id named slots; free-text
  labels; sync across compose windows; SQLite vs simple JSON file
- **Reply-to-form via `_SendReply.0` templates** (P3): mechanics of detecting
  a `_SendReply.0` template existence and routing reply through it

## 13. Open questions for operator review

1. **PDF generation library for ICS-309**: any operator preference (e.g.,
   wkhtmltopdf vs typst vs pure-Rust printpdf)? Or punt to "browser print
   from webview viewer" minimal path?
2. **Map widget choice for Position Report**: leaflet w/ offline tile cache
   is the obvious choice but adds ~200KB JS + ~tens-of-MB tile cache;
   acceptable trade?
3. **Custom-form discovery hot-reload**: should new files dropped into the
   custom-forms dir be picked up live, or require operator-triggered
   refresh? (Live is nicer UX but adds inotify/FSWatcher complexity.)
4. **Form draft library scope at v1**: just Winlink Check-In (named slots
   per net), or generalize to all 5 native forms from day 1?

## 14. Risks

| Risk | Mitigation |
|---|---|
| WLE Standard Forms zip URL or content format changes upstream | Pin known-good version in bundled snapshot; updater fails gracefully (rollback); operator can manually drop a newer zip into `/forms/custom/`. Treat winlink.org as a best-effort source, not a hard dependency. |
| WebKitGTK + axum + Tauri 2 child-webview pattern is novel; could have integration surprises | Front-load a spike at P1 kickoff: bring up a minimal webview-loading-loopback-form before building the full catalog browser. If the spike reveals blockers, revisit Option A (with Codex's named caveats) as fallback. |
| Custom-form HTML triggers an XSS-like behavior we didn't anticipate | Layered defenses per §10; in worst case, the form's blast radius is its own webview, which is dismissed on close. tuxlink IPC, mailbox, credentials, GPS all out of reach. |
| Bundle-size growth from Standard Forms snapshot | Bundle plan sets a budget (e.g., ≤20MB total binary growth); if exceeded, ship a smaller "core" snapshot + auto-download the rest on first online run. |
| Operator on a network with strict egress filtering can't reach winlink.org for updates | Bundled snapshot always works; operator can sideload updates from a USB stick. Document the path. |
| Native-vs-webview boundary becomes contested (operators want form X promoted to native) | Document the criterion explicitly: native iff tuxlink has unique data layer to add. New native forms each get a bd issue + their own design pass. |
| PR #177 ships the broken-as-fill-in forms removed from picker, but operators using those forms internally via debug paths discover them | Browser-smoke each phase; explicit operator confirmation P0 ships with picker trimmed before #177 merges. |

## 15. Migration story

Operators currently on PR #177's design (after P0 ships):
- ICS-213 + Bulletin compose flows: unchanged
- Position / ICS-309 / Damage Assessment in picker: GONE in P0; come back via
  webview (P1) and then native (P2 for the first two)
- Received form messages: render via existing View components (preserved)
- Draft messages with form_payload (from removed compose forms): they're in
  the draft store; on attempt-to-open they go through the same render path
  → if the form is no longer in picker, fall through to View-only mode
  with "form compose not available" message

P1 onward:
- New webview-rendered forms work everywhere a previous form did
- Custom forms picked up from `~/.local/share/tuxlink/forms/custom/`
  automatically
- P3 auto-update: operators opt in; updates atomic-swap with rollback

## 16. Decision log

Full conversational brainstorm transcript not preserved verbatim; key
turn points:

1. **Operator critique of v0.1** (2026-05-31): forms shipped in PR #177 are
   "imitations of form rather than imitations of function"; specifically
   GPS Position has no GPS pull, ICS-309 manually typed is operationally
   wrong.

2. **Triage proposal rejected as too atomic**: my initial proposal to pull
   3 forms + file 3 small bd issues was redirected by operator:
   *"end-state feature parity. That's the goal."*

3. **Scope target locked**: full WLE forms surface parity. Operator
   framing: *"forms work must be done anyway since they're load-bearing
   in the existing WLE community. Can we not just do the whole thing?"*

4. **Architecture chosen**: hybrid (native + webview), not webview-only or
   native-only. Native-only doesn't scale to custom forms (operators need
   to drop new HTML and have it work — Pat does this); webview-only
   sacrifices UX innovation potential for the 5 popular forms where
   tuxlink has unique data to contribute.

5. **Hamexandria research surfaced Winlink Check-In** as the most-frequently-
   demonstrated form in EmComm tutorial corpus; missed entirely by the v0.1
   spec.

6. **Codex adrev reversed initial submit-capture recommendation** from
   "Tauri custom protocol + JS injection" to "lazy loopback HTTP with
   hardening." The decisive insight: WLE templates use a
   `{FormServer}:{FormPort}{FormFolder}` substitution convention that's the
   ecosystem contract any compatibility-seeking host has to match. Custom-
   protocol can't substitute a non-existent port. Cost of running an
   ephemeral lazy listener is small; cost of breaking custom-form compat
   is high. Full transcript: [`dev/adversarial/2026-05-31-form-submit-capture-codex.md`](../../../dev/adversarial/2026-05-31-form-submit-capture-codex.md).

7. **PR #177 disposition**: stack on top, not scrap. The wire-format work
   (parse / serialize / types / OutboundMessage attachments / native B2F
   path) is correct and reusable across both native and webview paths.
   Pull only the misshapen form UIs (Position / ICS-309 / Damage
   Assessment) before #177 merges (P0 deliverable).

## 17. Approval ask

Operator review of this spec is the gate for moving to per-phase
implementation plans via `superpowers:writing-plans`. Each phase gets its
own plan + own bd issue + own PR. Approval doesn't commit anything to code
yet; it just unlocks plan-writing for P0 (the immediate next step) and
defines the design contract that P1-P3 plans will reference.
