# Implementation log

Reverse-chronological record of significant work items (plans executed, features
shipped, bug-hunt cycles, adversarial reviews). Keyed by date + topic.

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
