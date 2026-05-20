# 12. v0.0.1 main UI adopts Mock D (Mail.app-minimal), superseding the synthesis layout

Date: 2026-05-20
Status: **SUPERSEDED by [ADR 0013](0013-v001-main-ui-is-mock-b-not-mock-d.md).** This ADR's premise — that the operator approved Mock D — was incorrect. The approved v0.0.1 design is **Mock B (principles-faithful)**; "Mock D" was a misidentification by the decision-locking session, not an operator decision. Retained as the historical record of that error (see ADR 0013 §Context).
Deciders: pika-glade-bluff (decision-locking agent — recorded in error), hemlock-raven-wren (implementing agent)

## Context

The v0.0.1 main UI was built to the **synthesis** layout selected in design-doc §3: a top dashboard ribbon (callsign · grid · GPS · UTC · connection), a left folder sidebar, a center message list, a right reading pane, a reserved dock column, and a bottom session-log strip plus a minimal status bar. This combined elements of brainstorm Mocks B and C.

On 2026-05-20 the operator placed the built synthesis UI next to the four approved brainstorm mockups (`docs/design/mockups/2026-05-17-mocks-v1-four-directions.html`) and rejected it:

> great value at best… bland, disproportionate, generally incorrect.

Shown the explicit fork between the built synthesis and **Mock D (Mail.app-minimal)**, the operator chose to rebuild the v0.0.1 main UI to **Mock D literally**.

A cross-provider Codex review of the gap (transcript: `dev/adversarial/2026-05-20-cbz-fidelity-strategy-codex.md`, gitignored) diagnosed the miss as **source-of-truth + topology mismatch, not a color pass**: the build had drifted to a flat warm-neutral palette and the wrong chrome (ribbon/sidebar/dock). The verdict was to port the mock's CSS tokens + structure wholesale and keep the React data plumbing.

A contributing-cause note for the original miss: the prior session validated the UI against a **dev gallery rendered in Chromium (Playwright)** with synthetic data, which looked correct, while the real compiled app is **WebKitGTK** (Tauri on Linux) with an empty backend and looked nothing like the mock. The gallery was a lying proxy. See Consequences.

## Decision

### 1. v0.0.1 adopts Mock D topology

The main shell renders the mock's `layout-D` and nothing more:

- **Tab strip** for folder navigation (the functional folders Inbox / Outbox / Sent / Drafts as tabs with counts), replacing the folder sidebar.
- **Two panes**, `grid-template-columns: 420px 1fr` — message list | reading pane.
- **Minimal status bar**: `● <state> · <callsign> · <grid>` (left), version (right). This is the *only* at-a-glance callsign/grid surface — the dashboard ribbon is removed entirely.

### 2. Removed from the default composition (not deleted)

The `DashboardRibbon`, `FolderSidebar`, and reserved dock column are removed from the default render. Their component files are retained (parked) for possible later reuse; they are simply not mounted.

### 3. Session log deferred behind the View menu

The session log is **not default pixels**. It is reached via **View → Session Log** (`menu:view:session_log`) and renders as a bottom strip only when toggled on — preserving the emcomm debug surface (Mock D's own escape-valve note) without spending it on the default layout.

### 4. Palette / elevation / type ported verbatim

The cool-slate `:root` tokens, elevation ladder, and `--sans`/`--mono` are ported verbatim from the approved mock. Inter (variable, latin subset) is bundled locally (`src/fonts/`, same-origin per the window CSP) so WebKitGTK renders the approved face rather than a generic system sans. The legacy `--tux-*` names remain aliased to the real tokens (no re-interpretation of color).

## Consequences

- Design-doc §3 carries a SUPERSEDED-for-v0.0.1 banner pointing to `tuxlink-yd4` / this ADR; spec §4.1's grid is superseded for v0.0.1. This ADR is the canonical record (propagation contract: ADR + the spec section it amends + one operational pointer).
- The reply→compose wiring (`replyActions.ts`) is reused unchanged; only markup/labels were reshaped to the mock. Decisions #2 (compose = separate Tauri window) and #5 (modem) are unaffected.
- **Validation rule (the lying-proxy lesson):** UI fidelity for this app is validated against the **real compiled Tauri/WebKitGTK app** (`pnpm tauri dev` + a `grim` screenshot compared to `mock-d-mailapp-minimal.png`), never a Chromium/Playwright gallery. A dev-only fixture (gated on `import.meta.env.MODE === 'development'`) populates the empty-backend app so rows/reading-pane are visible for that comparison; it is excluded from tests and release builds.
- Re-adding the ribbon/sidebar/default-session-log to v0.0.1 requires a new operator decision (do not reintroduce silently).

## Alternatives considered

- **Recolor the synthesis in place** (treat it as a palette problem). Rejected: Codex + the operator both identified topology — not color — as the dominant cause of the "incorrect" feel; recoloring the wrong chrome would not have closed the gap.
- **A hybrid (Mock D body + a slim ribbon).** Rejected for v0.0.1: the operator chose Mock D *literally*; the status bar is the minimum-viable callsign/grid surface, and adding chrome back is exactly the drift that produced the rejected build.
