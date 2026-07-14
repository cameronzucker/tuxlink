# Implementation log

Reverse-chronological record of significant work items (plans executed, features
shipped, bug-hunt cycles, adversarial reviews). Keyed by date + topic.

---

## 2026-07-14 — Routines plans 2+4 merged (#1115, #1117); efcc8 security fix; plan-5 UI plan drafted

Agent crag-lupine-lichen. Plan 2 unblocked by fixing its own flaky quit-gate
drain-timeout test deterministically (channel-park instead of a 500 ms
wall-clock sleep; `ef10bbee`) — operator merged #1115. All 5 review findings
on plan 4 (bd tuxlink-efcc8) fixed and merged (#1117 @ `ed69cc60`): the C1
MCP transmit-consent bypass (save path now discards caller `transmit_ack`
before validation; regression tests both directions), the C2 testserver
`--workspace` compile break (MockRoutines; compiled locally on the Pi), M2
`PortError::InvalidInput` → `invalid_request`, M1 spec §14 wording, and the
Codex P3 WWV floor re-derivation (3900→2280 s for the :18/:45 nearest-window
scheduler, with a monolith drift-guard test pinning the leaf constant to
`next_capture`'s real constants). Process traps recorded in the handoff:
stacked PRs based off non-main branches get zero CI (`ci.yml` branch filter),
and retargeting emits no runnable event — an empty commit arms the first run.
Plan-5 (operator UI) implementation plan re-drafted to
`dev/scratch/routines-p5-ui-plan-draft.md` (15 tasks; UI-only
`routines_acknowledge_automatic` consent stamp required by the C1 fix); bd
tuxlink-fdmg9 tracks execution. Four merged routines worktrees disposed per
ADR 0009. New flaky-test bug filed for `tuxlink-jt9`
`signal_death_is_classified_with_stderr_tail` (arm64).

## 2026-07-13 — v0.90.0 shipped; first-run tour + spatial hint system shipped (PR #1114); r788i arm/session UX designed

- Merged #1089 (contacts consolidation) + #1090 (jt9 deflake) on green; released v0.90.0 (manual release-merge dispatch, operator-approved, after the cron was dropped by GitHub).
- Shipped tuxlink-10bkw end-to-end in one session: office-hours spec (3 adversarial reviews), operator-approved renders, 8-task subagent-driven build, 6-finding fix wave (final review + Codex), wire-walk on operator flows (menu-anchor gap found and built in-branch), CI green, merged. Config schema v7.
- Designed (code-gated) tuxlink-r788i one-click Connect / tri-state footer / scoped Disarm-Abort; spec + WebKitGTK mock pushed on bd-tuxlink-r788i/arm-session-ux awaiting operator approval.

## 2026-07-13 — QA round-3 shipped (#1086) + Contacts/Favorites/Heard consolidation (PR #1089)

Agent crag-fox-savanna. All 8 operator QA-round-3 findings fixed and merged
(#1086): F5's overlay invisibility root-caused to a duplicate z-index
declaration (last-wins) with a CSS-source regression test; FT-8 listening made
session-scoped per operator ruling (autostart retired); setup surface promoted
to the panel's full body per the firstrun-v2 mock; plus popover direction,
ribbon label case, WWV wording/layout, and the Live-decodes count badge. New
render-harness fixtures: view=finder (whole StationFinderPanel), snap-click.py.
Mid-session operator report → tuxlink-r788i (ARDOP/VARA arm/session UX +
unapproved two-step Start/Send; findings only). Then the operator-requested
Contacts consolidation (tuxlink-sbf03): ONE address surface with All/★
Favorites/Heard scopes, FavoritesPanel deleted, uniform row anatomy, per-dial
favorite stars, "Last heard" sort no-op fixed; design mock approved, WebKitGTK
renders approved, Codex adrev round applied (5 fixes, 1 rejected with
rationale). PR #1089 open at session end.

## 2026-07-11 — Station Intelligence L2: capture + slot-decode service (tuxlink-b026z.3)

Plan `docs/superpowers/plans/2026-07-10-station-intel-l2-capture.md`
executed (19 tasks, 3+3 review gates). Shipped: `tuxlink-capture` leaf crate
(51-tap Kaiser 48k→12k decimator with response-verified const table,
wall-clock-true 15 s slot assembler with two-clock-domain gap/anomaly rules,
canonical slot-WAV writer, listener state machine with N=5/k=20 counters +
sweep element, FT8 band table); salvage-on-signal parity in `tuxlink-jt9`
(resolves tuxlink-gujnz) + 3 types.rs contract doc edits; main-crate
`src/ft8/` service (ALSA hw:-only capture source, supervisor/capture/decode
threads with rendezvous backpressure, waterfall tap, 240-slot ring, tmpfs
slot dirs, timedatectl clock probe, pipe-fd watermark for b026z.8), modem
yield/resume arbiter with positive hold latch + choke-point wiring into all
ardopcf/Dire Wolf/VARA spawn paths, opt-in CAT band sweep with provenance
downgrade, six `ft8_*` Tauri commands + `ft8-decodes:slot` /
`ft8-listening:change` events + autostart. E2E: committed SDR fixture
ZOH-upsampled through the faked-source capture path into real jt9, ≥ 90 %
of reference decodes. CI: libasound2-dev in all compiling workflows,
libasound2 runtime Depends. No UI caller by design — the epic's layer-wise
sanction; wire-walk gate runs when L3/L4 make FT8 user-reachable. Delta v3
notes appended (6 contract deltas + 2 implementation-pinned).

---

## 2026-07-10 — Station Intelligence L1: managed-jt9 decode service shipped (tuxlink-b026z.2, PR #1070)

Executed the 3-round-reviewed L1 plan via subagent-driven development
(agent redwood-hemlock-thistle): new std-only leaf crate `src-tauri/tuxlink-jt9`
(parse / message / wav / types / discover / runner) — slot WAV in, structured
FT8 decodes out, jt9 subprocess-only (GPL boundary). 46 tests (24 unit + 18
fake-jt9 lifecycle pinning all 7 taxonomy arms and cross-arm tiebreaks + 4
real-jt9 e2e on the committed SDR fixtures), clippy clean both crates. Fixture
refs regenerated at the production flag set (`-8 -d 3 -p 15 -w 1`); CI installs
wsjtx on both arches + `scripts/check-jt9-provenance.sh` guard (8/8
deny-patterns live-trip-tested, closes b026z.7 scope); deb/rpm Recommend
wsjtx >= 2.5; stale GPL bundle.license fixed to AGPL. Process: per-task
spec+quality reviews, plan gates A/B/C (3–4 rounds each) + final whole-branch
review; substantive gate catches included the sentinel-aware `partial` flag,
bounded version probe, bounded clean-exit drains (decode_slot now returns on
ALL paths), the warm shared e2e harness (arm64 CI flake preempted), and a
fix-forward attributing the deb-install hamlib check to the wsjtx Recommends
chain. Follow-ups filed: tuxlink-iy1av (2.5-era parser verification),
tuxlink-b026z.8 (grandchild pipe-fd leak bound), tuxlink-gujnz
(salvage-on-signal design question). Next: L2 capture subsystem (b026z.3).

---

## 2026-07-01 — Freeze the React Button/Select/Field wrapper API (tuxlink-3m0vx)

Design-system epic: built and froze typed `Button`/`Select`/`Field` wrappers over a
normalized `controls.css`, adopted on the reviewed ribbon + radio-pane surfaces.
Subagent-driven execution (10 tasks) from spec → plan, both operator-approved (the
Hybrid/normalized scale was blessed via a WebKitGTK current-vs-normalized mock).

- **Model:** `tone{neutral,primary,danger} × emphasis{solid,soft,outline} × size{xs,sm,md}`,
  color via a `--ctl-accent` context-token trio (amber in app chrome, green in `.radio-panel`).
- **Adoption:** 31 footer buttons + 47 config Select/Field controls across the radio panes,
  + ribbon Connect/Abort. Doc: [`docs/design/control-wrappers.md`](../docs/design/control-wrappers.md).
- **Caught in review:** the migration orphaned the compact 44px touch-target a11y floor
  (retargeted `.radio-panel-btn` → `.radio-panel .tux-btn`, contract tests updated to the live
  selector); and the render harness never imported `controls.css`, so early visual verifies were
  on unstyled buttons — fixed the harness (shipped app was always correct; App.tsx loads it).
- **Verification:** full `pnpm vitest run` (3434 pass) after each CSS change, WebKitGTK re-verify
  in dark + daylight. Frozen prop enums are the stable control surface going forward.

---

## 2026-06-30 — Elmer tiered model-access onboarding (tuxlink-wpqwy, PR #984)

Keyless-audience model picker as Elmer's first-run surface, executed via
subagent-driven-development from the revision-2 plan (frontend T8b–T11, then Rust
T3/T4/T5, then T12 smoke). Agent `bayou-cedar-delta`.

- **Frontend (186 vitest green, typecheck clean):** ModelTilePicker as the
  onboarding surface with chat gated until onboarded + gear-reopen (T8b); guided
  GetKeyCard with masked entry, hardcoded-keyPageUrl open, sanity validation, and
  per-tile remount so unsaved keys don't carry across providers (T9, T11); typed
  429 recovery callout + Switch-provider→paygo that returns to chat on save/cancel
  (T10); honest per-tier framing + provider footer indicator (T10); #981
  credential-seam regressions ported to the tile flow + Anthropic origin (T11).
- **Rust (CI-verified — Pi can't cold-compile):** `onboarded` sentinel with a
  migration (`onboarded || !is_default()`, `is_default` counting the flag so
  default-content saves persist it) so existing users aren't bounced to the
  picker (T3); `elmer_key_status_for_origins` — statuses only, MCP-denied (T4);
  typed 429 → `rateLimited` outcome (camelCase, matching the shipped FE; the
  plan's `rate_limited` was stale) across `ProviderError`/`RunOutcome`/
  `DetectError`, both turn and detect paths (T5). Each new field/variant carries a
  serde wire-shape test pinning the on-wire literal.
- **Review:** per-task spec+quality reviews caught a Critical stranded-picker bug
  (rate-limit switch never returned to chat), a non-exhaustive d3zwe match, and
  several weakened tests; final whole-branch review = READY TO MERGE. Codex adrev
  found 4 P2 — 3 fixed (real-path detect 429, path-tight shell allowlist,
  primary-flow key-status fetch); the 4th (picker as the settings surface) is a
  deferred operator design-conformance decision.
- **Smoke:** WebKitGTK render harness (new `elmer` view) at 392px + 700px.
  Operator drives reachability manually in a converged R2 build in lieu of the
  agent wire-walk.

---

## 2026-06-15 — Link/transport config persistence remediation (tuxlink-hoi1)

Radio link/transport config did not persist across app restart — the physical
device was retained but the link kind/transport reset to "no link". 3-hunter bug
cycle (consolidated) pinned a destructive full-replace primitive (B1) plus four
contributing defects (B2–B5). Executed the remediation plan (PR #746).

- **B1 + B5 (backend, `ui_commands.rs`)**: `packet_config_set` full-replaced
  `cfg.packet` from the DTO, and `into_packet_config` mapped an absent
  `link_kind` to `link: None` — so any SSID/timing-only write erased the saved
  link. New pure helper `apply_packet_dto` preserves the existing link when the
  DTO carries no `link_kind` ("absent == unchanged"; the UI has no clear-link
  path). The command now also emits `packet_config:change` (B5) — the frontend
  listener was already there but dead, no emitter existed.
- **B2 (frontend, `AprsConnectStrip` + `AppShell`)**: the APRS connect strip
  mounted `ModemLinkSection` with no address props, so tapping the UV-Pro segment
  on a configured link emitted `btMac: null`. Threaded the saved address fields
  (tcpHost/tcpPort/serialDevice/serialBaud/btMac) through from `packetConfig`.
- **B4 + B3 (frontend, `usePacketConfig` + `PacketRadioPanel`)**: optimistic
  writes swallowed persist errors (UI/disk divergence); the panel held a frozen
  snapshot and never re-synced (cross-surface clobber). Added rollback-on-reject
  + subscribed the panel to `packet_config:change` / the same-window CustomEvent.
- **Adversarial review** (Codex [P2] + 2 Claude reviewers, independently
  converged): the B4 rollback re-opened the multi-writer clobber it closed — an
  older rejected write could revert a newer successful one to stale state.
  Guarded with a render-synced `configRef`: roll back only if the optimistic
  value is still current. Regression-tested (fails without the guard).
- New testing-pitfall: absent-field-erases / multi-writer clobber (incl. the
  rollback-as-writer corollary). Operator validates the 4 wire-walk flows + a
  real on-air UV-Pro round where relevant.

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
