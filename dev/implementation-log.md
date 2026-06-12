# Implementation log

Reverse-chronological record of significant work items (plans executed, features
shipped, bug-hunt cycles, adversarial reviews). Keyed by date + topic.

---

## 2026-06-12 — SeqInc message serial numbering (Forms-push G12-C, tuxlink-2tom)

22 bundled `.txt` templates (radiogram / RRI / net-log serials) carry the
`SeqInc:` directive + a `{SeqNum}` / `<var SeqNum>` placeholder for per-form
message serial numbering. tuxlink parsed neither. This adds the stateful counter.

- **Parser** (`forms::txt_template`): `TxtTemplate.seq_inc: bool`, parses `SeqInc:`.
- **`forms::sequence::SeqCounterStore`** (new): persisted per-form `last_used`
  serials at `<app_data>/forms-sequence-counters.json`. Infallible `open`
  (degrade-to-empty), atomic write-tmp-then-rename — mirrors `ContactsStore` /
  `FavoritesStore`. `allocate` (increment+persist+return), `peek`, `set_next`
  (reset, clamped ≥1), `status`. Managed as `Arc<Mutex<…>>` in `setup()` (+ a
  temp-path fallback in the `app_data_dir`-unavailable arm so `send_webview_form`
  never regresses).
- **Send path** (`send_webview_form`): on a `SeqInc` governing template, allocate
  the next serial and stamp it into `SeqNum` before rendering — so `<var SeqNum>`
  in Subject/Msg and the serialized XML all carry it. Allocation persists BEFORE
  the network send (a failed send leaves a serial GAP, never a duplicate from a
  concurrent retry). The serial is assigned authoritatively at send; `{SeqNum}`
  is blanked at form-open (`substitute_template`) so the field never shows the
  literal placeholder.
- **Reset affordance**: `forms_sequence_status` + `forms_sequence_reset`
  commands + a "Form sequence numbers" Settings section (`FormSequenceSettings`)
  listing each counter's next serial with a per-form set-next control.

Self-adrev (Codex unavailable — not a gate). Backend unit tests (parser, store
allocate/peek/set_next/status/malformed, send-path stamping, `{SeqNum}` blank);
frontend +4 tests. `tsc` clean. Operator smoke: send an IARU / radiogram form
twice → confirm Msg# increments; reset via Settings → confirm next send uses it.

---

## 2026-06-12 — Reply-form threading (Forms-push G10, tuxlink-hhfx)

WLE `ReplyTemplate:` request/response threading on the ICS-213 General Message
family (8 forms: ICS213, ARC 213, HICS 213, IMS 213, DCS 213, SHARES, ARC
6409-B). Before: `replyActions.replyWithForm` covered only 2 native forms with a
hardcoded sender↔recipient swap that produced a NEW blank 213 and lost the
thread; `parse_form_xml` extracted `reply_template` but nothing consumed it.

Now: replying to a form that advertises a `ReplyTemplate:` opens its
`<X>_SendReply.html` authoring page **pre-bound** with the original field values
(name-aligned) + editable, so the operator fills only the Reply section and the
original 213 is preserved in the thread. Built on the o4p9 `.txt` engine.

- **`forms::txt_template::resolve_sendreply`** — from the form folder + the `.0`
  filename, parse the `.0` and locate its authoring HTML via the `.0`'s own
  `Form:` directive (stem-drift-proof: `HICS213_SendReply.0` →
  `HICS 213_SendReply.html`), with case-insensitive fallback.
- **`http_server::FormSession::open_form_prebound`** — the union of `open`
  (editable + live submit channel) and `open_viewer` (server-side value
  binding): an editable Form-kind session pre-bound with field values.
- **`open_webview_reply` command** — derives the SendReply from the ORIGINAL
  form's own bundled `.txt` `ReplyTemplate:` (local truth, not the inbound XML's
  possibly-synthetic claim), pre-binds the original values + `MsgOriginalBody`,
  spawns the submit forwarder. Returns the resolved `reply_template`.
- **`send_webview_form`** gained `reply_template` + `subject_hint` optional
  params: a reply renders To:/Subject:/Msg: from the SendReply `.0` (whose Msg
  reproduces the original + the reply), display_form = the SendReply viewer, and
  the subject falls back to the operator's "Re: …" (SendReply `.0`s carry no
  `Subject:` directive). Backward-compatible — first-time sends pass neither.
- **Frontend** — `replyActions` `formReply` mode + `hasFormReplyTemplate`;
  Compose `webview-reply` FormMode (pre-bound `WebviewFormHost`, restore +
  autosave via shared `persistedFormDraft`); `WebviewFormHost` reply mode routes
  to `open_webview_reply` and threads the `reply_template` to submit; MessageView
  routes ReplyTemplate forms to the SendReply path (precedence over the legacy
  swap; works for non-native forms too).

Self-adrev (Codex unavailable — not a gate); +15 frontend tests, +backend unit
tests for resolve_sendreply / open_form_prebound / reply render. Operator smoke
pending: receive an ICS-213, reply, verify the SendReply opens pre-bound and the
sent reply carries the original + reply with a `Re:` subject to the sender.

---

## 2026-06-12 — Runtime .txt message-template engine (Forms-push G12-A, tuxlink-o4p9)

The G12 audit found the dominant forms gap: the ~131 generic-path catalog/org
forms sent via `send_webview_form` ignored their governing `.txt` template
entirely — generic `Form: <id>` subject, a key:value dump body, and the
operator-typed recipient — discarding the form designer's prescribed `To:`
(often a fixed agency address like DYFI → USGS, silently breaking the
receive-side data pipeline), the templated `Subject:` (routing-significant for
RRI/ICS-213), and the `Msg:` body projection. tuxlink parsed only the `Form:`
directive (for import detection).

New `forms::txt_template`: parses the full WLE `.txt` grammar
(`Form:`/`Display:`/`To:`/`Cc:`/`Subject:`/`ReplyTemplate:`/`Def:`/`Msg:`, the
last being a body block to EOF), a cp1252 decoder (the bundle is Windows-1252;
the importer's `from_utf8_lossy` corrupts smart quotes), a renderer that
substitutes `<var fieldname>` from submitted values AND `<HostTag>`
(`MsgSender`/`ProgramVersion`/`Callsign`/`GridSquare`/`DateTime`/… — this
subsumes most of G12-B/pj7p) with XML-1.0 sanitization, and a resolver that
finds a form's governing `.txt` by its `Form:` directive (NOT a shared stem —
`ICS213_Initial.html` ↔ `ICS213 General Message.txt`).

Wired into `send_webview_form`: render the `.txt` To/Subject/Msg with the
submitted field values; subject + body use the rendered template when present
(generic fallbacks otherwise); recipients **union** the rendered `To:` (form's
prescribed address first) with operator-entered recipients (case-insensitive
dedup, never drops data). A form with no governing `.txt` keeps the prior
behavior. The XML attachment is unchanged, so WLE-receiver interop is untouched
— a correct subject is more compatible, not less.

Recipient-handling decision (documented, operator-testable per the no-Codex /
self-adrev posture): union rather than override or pre-send-review, because it
never silently drops the operator's recipient and honors the form's destination.
A pre-send recipient-review step is a noted future option if the at-submit
recipient change proves surprising.

Discipline: interop-sensitive but well-specified by the audit; built with
rigorous self-adversarial review (Codex unavailable, not a gate). The self-adrev
test design caught a real bug (inline-`Msg:` leading-space). Gates: clippy
`--all-targets -D warnings` clean; txt_template 33/0; merge_txt_recipients 4/0.
The end-to-end send is an operator smoke (send a fixed-To form like DYFI, or a
`<var address>` form, and confirm recipient/subject/body).

---

## 2026-06-12 — Direct print of a rendered form (Forms-push G8b, tuxlink-954o)

Fast-follow to G8 (tuxlink-cumx). The issue title was always "PDF/print"; G8
shipped the PDF half. Operator: getting a hardcopy meant Export-PDF → open the
file → print — an annoying save-to-disk detour. Add a direct Print affordance.

Reuses the exact G8 machinery: the form's live child `WebKitWebView` +
`WebKitPrintOperation`, but calls `run_dialog(None)` instead of
`set_print_settings(file)` + `print()`. `run_dialog` shows GTK's system print
dialog (printer picker + page setup, with "Print to File" as one option) and
prints synchronously on confirm — no intermediate file. Returns
`PrintOperationResponse::{Print,Cancel}` (mapped to a `bool` printed/cancelled).
`forms::pdf_export::print_webview` + a `forms_print` Tauri command +
`printForm()` helper + a "Print…" button beside "Export PDF" in BOTH chromes
(both buttons disable while either op holds the webview).

Notes: no parent window passed to `run_dialog` — gtk3's `Widget::toplevel` is
deprecated and would trip `-D warnings`; the dialog is unparented but
functional. No completion deadline (the operator may deliberate in the dialog);
the closure always sends once it closes.

Discipline: contained, interop-neutral → straight TDD (no BRF/Codex). Gates:
clippy `--all-targets -D warnings` clean, backend pdf_export 5/0, frontend 30/0
(3 new `printForm` tests + both modified components). Like G8, the GTK dialog
needs a display, so it's an operator smoke (open a form → Print… → confirm it
reaches a printer), not a CI test.

---

## 2026-06-12 — On-demand faithful PDF export of a rendered form (Forms-push G8, tuxlink-cumx)

Operator chose faithful render over a structured data sheet (compared two real
PDFs generated from the bundled ICS-213). The audience is a served agency /
non-ham who opens the PDF to read what was sent, so the output must look like
the form, not a summary.

Implementation reuses the live form webview: a form renders in a child
`WebKitWebView` (`compose-form-<token>` authoring / `viewer-form-<token>`
received), and `forms::pdf_export::export_webview_pdf` drives that exact view
through `WebKitPrintOperation` with `GtkPrintSettings` targeting a `file://`
URI + `output-file-format=pdf`. Same engine that painted the form → faithful
output, and zero new rendering dep (drops WLE's wkhtmltopdf/NReco licensed
native dep, per the forms synthesis). A new `forms_export_pdf` Tauri command
(spawn_blocking + a channel awaiting the async `finished`/`failed` signal)
exposes it; a shared `pdfExport.ts` helper (native Save dialog → command) wires
an "Export PDF" button into BOTH the authoring host and the received-form
viewer chrome.

Grounding before writing the FFI surfaced three otherwise-silent traps:
`Manager::get_webview` is gated behind Tauri's `unstable` feature (added — the
app already commits to multi-webview at the JS layer); `webkit2gtk` re-exports
`glib` but not `gtk` (so `gtk` is a direct dep, `glib` reached via webkit2gtk);
and `gtk`/`webkit2gtk` must pin the exact versions wry resolves (`=0.18.2` /
`=2.0.2`) or `PlatformWebview::inner()`'s `WebView` type won't unify. The
`PrintOperation` is held alive past the closure via an `Rc` holder that the
terminal signal releases — a dropped op would cancel the async print.

Discipline: interop-neutral, contained → straight TDD (no BRF/Codex). Gates:
clippy `--all-targets -D warnings` clean, backend pdf_export tests 5/0, frontend
27/0 (helper + both modified components). The GTK print itself needs a display,
so faithful-output fidelity is an operator smoke (open a form → Export PDF →
open the file), flagged on the PR — not a CI test.

---

## 2026-06-12 — Form-value XML-1.0 sanitization (Forms-push G9, tuxlink-nitb)

G9 was filed as "native required/typed field validation." Grounding against the
real bundle reframed it: every authoring form submits via a native `<form>`
button (119/137; 0 use JS `.submit()`) with no `novalidate`, so WebKitGTK
already enforces `required` (used 1429×) and the few HTML5 types — a generic
tuxlink-side validator would duplicate the webview and catch nothing extra on
real forms. The genuinely non-redundant defect is the one the webview does NOT
cover: **output sanitization**. Operator approved narrowing G9 to that.

`push_element` (the chokepoint for both `serialize_form_xml` and
`serialize_catalog_form_xml`) escaped only `<>&`, passing XML-1.0-illegal
characters (C0 controls — NUL / vertical-tab / form-feed, the kind a copy-paste
from a PDF injects) straight into the attachment. Result: non-well-formed XML
that a strict receiver (WLE's .NET `XmlReader`, downstream aggregators) rejects
or mis-parses — corruption on our send. (tuxlink's own quick_xml parser is
lenient and preserves the illegal char, which masked it.)

Fix: an `is_xml10_legal` filter mapping every field value onto the XML 1.0
`Char` set (drops the illegal controls, keeps tab/CR/LF and all real text),
applied in `push_element` and to substituted values in `render_body_template`.
Legitimate content (`/`, quotes, accents, whitespace) passes through unchanged.
TDD: 4 failing round-trip + well-formedness tests first (reproduced the defect:
illegal chars `[12,11]`, `[0,31]` in output), then the fix. Gates: clippy
`--all-targets -D warnings` clean, full `cargo test` 1759/0. Backend-only;
straight TDD (contained correctness fix, no BRF/Codex ceremony).

---

## 2026-06-11 — In-app form import (Forms-push G5+G6+G11, tuxlink-z0le/fwob/48uc)

Shipped the in-app form-import flow so a stuck onboarding member can bring an
organization's custom Winlink forms (single `.html`, a folder, or a `.zip`)
into the custom-forms dir with a validate-before-write report, then see them in
the catalog — the originating gap (import was a manual file-drop into a hidden
XDG path with no UI). Built via build-robust-features: brainstorm + office-hours
(evidenced-floor scope: interop + import for the one evidenced user; aggregation
deferred) → spec + 4 Claude adversarial rounds → 16-task TDD plan → execution.

**Design correction the adversarial review forced:** the obvious HTML
`is-a-form` heuristic rejects 100% of the real bundle (Windows-1252; some
authoring forms carry zero `<form>`; literal `{FormServer}` placeholders).
Detection flipped to the `.txt` `Form:` directive as the import unit.

**Backend** (`src-tauri/src/forms/import.rs`, new): two-phase `preview`→`commit`
over a 0700 `tempfile` staging dir; `.txt`-directive detection + orphan-HTML
fallback + companion resolution; strict per-path-component validation + symlink
rejection + entry-count/per-file/total/ratio caps (the updater extractor lacked
the count + ratio guards); folder-aware classification (cross-folder stem dupes
→ Skip, never collapsed — the `(folder,id)` engine fix stays tuxlink-8v3l);
opaque-token `ImportStagingRegistry` (single-shot consume → `TokenExpired`,
TTL reap, boot sweep); `commit` shares `updater::INSTALL_LOCK`, re-classifies
under the lock (two TOCTOU guards → `CommitConflict`), writes with `.prev`
backup + rollback. Plus `open_forms_folder` (backend `xdg-open` — the frontend
`shell:allow-open` is URL-scoped), `forms_custom_delete` uninstall, and the
`/folder/*` CSP exfil-hole closure (403 html/htm/svg + CSP/nosniff on assets).
`walk_html` now enumerates `.htm` so detection == surfacing.

**Frontend**: `importApi.ts` bindings; `ImportSheet.tsx` (ZIP-first picker →
report → confirm-overwrite → commit, amber override + actionable no-viewer
warnings, cancel-on-unmount); `CatalogBrowser` wiring (custom-categories-first
sort, Import/Update-standard/Open-folder footer, empty-custom CTA, per-form
Remove, Escape state machine, post-commit refresh+highlight). `dialog:allow-open`
granted on the main window (was missing). G11 help: the HTML-forms topic now
documents the import flow (searchable).

Gates: clippy `--all-targets -D warnings` clean; full `cargo test` 0 failures;
`tsc --noEmit` clean; full vitest 2397/2397. **NOT merge-ready** until the
deferred cross-provider Codex adversarial round (tuxlink-yqo4, Codex quota
returns Jun 13) runs on the diff — the 4 Claude rounds are not a substitute.

---

## 2026-06-11 — Align Network Post Office send-flow with CMS (tuxlink-b6ad)

Network Post Office was the only send/receive mode with a per-message Outbox
selection checklist (the source of the confusing "Connect to receive only" copy
the operator flagged). The 6c9y design's own §1.1 routing table shows Network PO
carries **normal** mail into normal Winlink routing — same destination as CMS,
differing only on the transport axis — so the send-time-selection *leakage guard*
is justified only for **Telnet RMS Post Office** (local `-L` pool, never
forwarded globally); it was applied to Network PO purely for UI symmetry between
the two PO panes, breaking consistency with every other transport (CMS/VARA/
ARDOP/Packet all just Connect + drain). Operator chose to align it.

**Change:**
- Backend `ui_commands.rs`: new `po_drain_selection(local, selected)` →
  `if local { Some(selected) } else { None }`. `post_office_exchange` uses it, so
  Network PO (Mesh) drains the whole Outbox like CMS (`build_outbound_proposals`
  already treats `None` as drain-all); local `-L` keeps the explicit-selection
  leakage guard. One pure helper, unit-tested.
- Frontend `TelnetPostOfficeRadioPanel.tsx`: the "Send from Outbox" checklist
  renders only for `mode === 'local'`; network shows a one-line send note; the
  Connect label drops "& send N" outside local mode. Confusing empty-state copy
  removed for network; local empty-state tidied.

Gates: typecheck clean; cargo 4/4 (new `po_drain_selection` test + existing
local-mode PO integration still green); clippy `--all-targets` clean; vitest
2361 passing (panel 47/47; the 3 App-mount timeouts were clippy/CPU contention,
confirmed passing in isolation). Relates to `tuxlink-u5hl` (compose-time routing
flag stays unnecessary for Network PO — its mail is undifferentiated from CMS).

---

## 2026-06-11 — NWS weather glyphs for the SFT tabular forecast (tuxlink-n6tp)

Replaced the raw NWS condition codes (`Vryhot`, `Ptcldy`, `Mosunny`, …) in the
Tabular State Forecast grid (`CatalogReplyView`) with custom inline-SVG weather
icons, so the report reads at-a-glance like a modern weather app. Brainstormed +
operator-approved (mock: `dev/scratch/2026-06-10-nws-weather-glyphs-mock.html`);
presentation-only/reversible → straight TDD-against-spec, no cross-provider adrev.

**Shipped (all in `src/catalog/`):**
- `weatherGlyph.ts` — `resolveGlyph(code)` maps a normalized NWS SFT code →
  `{kind, label, accent}`, returning `null` for unmapped codes so the grid falls
  back to raw text (never blanks). Vocabulary grounded against the NWS SFT
  abbreviation set (incl. `Dust`/`Haze`/`Smoke`/`Frost`); `Sunny`/`Hot`/`Vryhot`
  collapse to one sun shape differing only by accent.
- `WeatherGlyph.tsx` + `.css` — themed inline-SVG icon set encoding the sky-cover
  gradient (sun shrinks / cloud grows from Sunny→Cloudy, fixing the
  mostly-sunny↔partly-cloudy ambiguity the operator flagged). Colours via CSS
  classes + SVG inheritance (render-stable under WebKitGTK); `role="img"` +
  `aria-label`/`<title>` = decoded plain-English label.
- `CatalogReplyView` `Cell` swapped to `<WeatherGlyph>`; legacy `condClass`
  folded into `conditionTextClass` (the fallback path).

Scope: SFT grid only (ZFP zone product is narrative prose, no codes). Gates:
typecheck clean, full vitest 2355 passing (10 new). Browser-smoke + design-review
of rendered icons deferred post-merge per `browser_smoke_before_ship`.

---

## 2026-06-11 — U3 Find-a-Station map UI shipped (tuxlink-gife)

Built the propagation-aware **Find a Station** map UI — the user-facing unit of
the Find-a-Station feature (umbrella `tuxlink-axq0`) — TDD-against-design from
`docs/design/2026-06-10-find-a-station-propagation-map-design.md` §7/§8/§12 + the
approved Mock-D surface. Plan:
`docs/superpowers/plans/2026-06-11-find-a-station-u3-map-ui.md`.

**Shipped (all in `src/catalog/`):**
- Pure core: `bandPlan` (freq→band), `reachability` (REL→tier + best-band),
  `stationModel` (station/channel aggregation — N0DAJ multi-mode/SSID collapse),
  `propagationApi` (first TS binding for U1 `propagation_predict_path`),
  `channelGrouping` (groups + `Use→` dial builder).
- Hooks: `useStationPrediction`, `useReachabilityMap` — both degrade to
  distance-only when voacapl isn't bundled (`UiError::Unavailable`).
- UI: `StationFinderControls` (conditions/band/mode + freshness), `StationFinderMap`
  (reachability-weighted pins on offline `BaseMap`), `StationRail` (header →
  aiming hero → path forecast → channels), `StationFinderPanel` (Mock-D assembly,
  FZ-M1 compact).

**Integration / cleanup:** wired into AppShell's Tools menu (renamed "Find a
Station…", action id `find_gateway` kept); widened `catalogPrefillMode` to VARA
HF/FM (VaraRadioPanel consumes prefill); deleted `CatalogBuilderPanel` +
`StationResults` (reverting the #550 operator-location pin).

**Verification:** 55 new catalog tests + AppShell open-from-menu production-mount
test, green; `pnpm typecheck` clean. Frontend-only (cargo unchanged — CI covers).

**Process notes:**
- Grounded the Tauri v2 camelCase→snake_case arg convention before writing the U1
  binding (a snake_case invoke key silently fails at runtime; a mock won't catch
  it). Confirmed against working `tsLocal`→`ts_local` usage.
- Hit a vitest cleanup-hook quirk: the runner calls `invoke` mocks with no args
  during teardown; under `mockRejectedValue` that stray call floats an unhandled
  rejection. Fix: cmd-gate the mocks + commit hook state in `.finally`. (memory
  `feedback_vitest_invoke_mock_cleanup_call`.)
- Codex adversarial round **deferred** — ChatGPT-auth usage limit hit (resets
  2026-06-13). Per project policy, not substituted with Claude; to run next
  adrev window.

**Not shipped (follow-ups):** `tuxlink-hhxs` (bundle voacapl into the .deb —
OPERATOR DECISION); until then prediction degrades to distance-only in a packaged
build. Live SFI/K-index feed; area-coverage ranking; 24-h sparkline.
