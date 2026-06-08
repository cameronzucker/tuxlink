# FZ-M1 Responsive Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a touch-friendly "compact" mode to the Tuxlink shell (and every primary surface) so the app is usable on a Panasonic FZ-M1 — a 1280×800, 7", ~216 PPI capacitive-touch rugged tablet — without changing the desktop (≥1366px) layout at all.

**Architecture:** Greenfield responsive work — the entire `src/` tree has exactly one non-print `@media` rule today. Compact rules are **additive and scoped** behind a single `@media (max-width: 1365px)` breakpoint (strictly below the 1366px desktop floor — Codex R1 #1). **All layout** is `@media`-driven; a `useViewport` hook + element-state classes (`.drawer-open`, `.sidebar.is-expanded`) carry only interactive state, and their effect is gated *inside* the media query so CSS and JS can never disagree about the mode (Codex R1 #2). Desktop ≥1366px is byte-identical and a regression-guard test lands *first*. The headline fix replaces the permanent 400px radio dock column (which starves the reader to ~300px at 1280px) with a **collapsible 4th grid column** (the radio "drawer"): closed it is a 44px grip strip; open it is the 400px panel. It **pushes** (reflows the reader) rather than overlaying — because a Tauri child webview (the HTML form viewer) paints *above* parent HTML, so an absolute overlay would be occluded by it; push reflows the reader, the form-viewer's `ResizeObserver` fires on the placeholder resize, and the webview repositions correctly (Codex R1 #5).

**Tech Stack:** React 18 + TypeScript, Vite, Vitest (jsdom) + React Testing Library, Tauri (Rust) for the separate Compose window, plain CSS (no preprocessor). jsdom cannot compute layout or evaluate media queries, so layout/typography assertions are **CSS-string assertions** (import the stylesheet as a string, slice the media block, assert on it — the established pattern in `AppShell.test.tsx`); interactive state is tested with RTL; real-viewport visual proof is an operator browser-smoke at 1280×800 plus an optional Playwright pass.

---

## Provenance & inputs

- **Locked design:** [`docs/design/2026-06-07-fzm1-responsive-design.md`](../../design/2026-06-07-fzm1-responsive-design.md) (operator brainstorm 2026-06-07, agent `basalt-mesa-dahlia`). Decision: **Option A — icon rail + radio drawer.**
- **Audit synthesis (local reference, gitignored):** `dev/scratch/2026-06-07-fzm1-compact-audit-synthesis.md` — per-surface FZ-M1 compact-readiness audit (7 surfaces, workflow `wf_20f7ea5a-9ae`). Raw per-surface JSON: `dev/scratch/2026-06-07-fzm1-compact-audit-raw.json`.
- **bd issue:** `tuxlink-h7q7` (P2). Companion help-window responsive is the separate `tuxlink-0gsy` (out of scope here).

### Resolved design open items (CONVERGED via Codex cross-provider adversarial review, round 1, 2026-06-07)

| # | Open item | CONVERGED resolution | Rationale |
|---|---|---|---|
| 1 | Exact breakpoint value | **Single `@media (max-width: 1365px)`** (strictly below the 1366px desktop floor); no second tier | A `max-width: 1366px` query *includes* 1366px, but the invariant is "desktop **≥**1366px unchanged" — overlap at exactly 1366 (Codex R1 #1, CRITICAL). `1365px` makes compact strictly `<1366`; FZ-M1 (1280) still triggers. No surface needs a second tier; phone-width (<768px) is out of scope. |
| 2 | Drawer mechanism + transition + grip state | **PUSH (collapsible 4th grid column), NOT an `absolute` overlay.** Closed col = 44px grip; open col = 400px panel. Transition via `transform: translateX` within the column; `prefers-reduced-motion` disables it. Grip shows a **real** coarse session state via `deriveDrawerSessionState()`. | A Tauri **child webview paints above parent HTML** (verified: `WebviewFormViewer.tsx:11-14`), so an absolute overlay drawer renders *behind* a form-viewer webview (Codex R1 #5, CRITICAL). Push reflows the reader → the embed placeholder resizes → the existing `ResizeObserver` repositions the webview → no occlusion. Push also keeps the grid's 4th track always reserved (no orphaned grid item if the JS class lags — Codex R1 #2). |
| 3 | Per-surface compact checklist | The table in **§"Per-surface compact checklist"** below | Derived directly from the 7-surface audit. |
| 4 | Icon rail tap-to-expand: overlay vs push | **Overlay** (expanded labeled rail floats over the message list; grid does not reflow) — UNCHANGED. The rail overlays the message *list* (HTML, no webview), so the occlusion hazard does not apply there; and a push rail would needlessly re-starve the reader on every label peek. | The rail vs the drawer differ: the rail floats over HTML (safe to overlay), the drawer can sit over a child webview (must push). |

### Additional CONVERGED resolution (design-internal tension)

**Rail resting width: 48px, not the design's "36px".** A 36px-wide rail cannot host 44px-*wide* tap targets (the design says both "36px rail" and "≥44×44px"). **48px resting rail** reclaims 152px of the original 200px sidebar and gives rail icons a full 44×44 hit area. (Codex R1 #3 concurred the touch-floor wins.)

### Codex round 1 dispositions (cross-provider adrev — `dev/adversarial/2026-06-07-fzm1-plan-codex-r1.md`, gitignored)

13 unique findings (3 CRITICAL, 7 HIGH, 3 MEDIUM). Dispositions baked into the tasks below:

| Codex | Severity | Finding | Disposition |
|---|---|---|---|
| #1 | CRITICAL | `max-width:1366px` includes 1366 → desktop-invariant overlap | **ACCEPT** → `1365px` (open item 1). Boundary tests at 1365/1366/1367 (Task 2). |
| #2 | CRITICAL | CSS (grid drops col under `@media`) and JS (`.compact` makes panel absolute) can contradict → orphaned grid item | **ACCEPT** → PUSH model: grid always reserves the 4th track (44px/400px); layout fully `@media`-driven; `.drawer-open`/`.is-expanded` are interactive-state classes whose effect is gated *inside* `@media` (Tasks 4-6). |
| #3 | HIGH | 24px closed grip violates 44px touch floor | **ACCEPT** → grip is its own 44px column when closed; ≥44px hit area (Tasks 4, 6). |
| #4 | HIGH | `display:contents` not byte-identical (DOM/a11y); future direct-child selectors fragile | **PARTIAL** → keep `display:contents` at desktop (the alternative — conditional wrap — *remounts the live radio panel on resize*, losing session state, which is worse for an emcomm app). `display:contents` elements drop from the a11y tree; impact negligible. Documented + verified via Playwright computed-width parity (Task 4 + Final verification). |
| #5 | CRITICAL | Task 17 doesn't solve R1; child webview paints above HTML → overlay drawer occluded | **ACCEPT** → PUSH (not overlay) so the placeholder resizes and the webview repositions naturally; re-measure on `transitionend` if the column animates (Task 17). |
| #6 | HIGH | Webview signal prop path goes via `MessageView`→`MessageViewLoaded`→`FormMessageBody`, not AppShell→viewer directly | **MOOT under PUSH** — no manual signal threading needed (natural `ResizeObserver`). Task 17 reduced to verification. De-risks the CF coordination. |
| #7 | HIGH | `?raw` import of `AppShell.css` won't inline `@import` → CSS-string tests see the import line, not compact rules | **ACCEPT** → `compactShell.css` raw-imported separately and **concatenated** in the test shim; compact rules also loaded via a normal `import './compactShell.css'` in AppShell.tsx (Vite-bundled), not a CSS `@import` (Task 2). |
| #8 | HIGH | CSS-string regression guard is "theater" — can't catch computed layout/stacking/hit-area | **ACCEPT** → CSS-string tests remain a cheap *first* guard; **Playwright at 1280×800 / 1366×768 / 1440 is now MANDATORY** (Final verification), plus the operator browser-smoke. |
| #9 | HIGH | Grip session-state hardcoded `'disconnected'` = a lie | **ACCEPT** → `deriveDrawerSessionState({ connecting, status, modemIsActive })` returns a real coarse state (Task 5). |
| #10 | HIGH | Coordination understated — webview signal touches CF's content-switch region | **MOOT under PUSH** (#6). Remaining AppShell overlap is panes-className + drawer-state, still different hunks from CF's L869-929/L214. |
| #11 | MEDIUM | Rail expand: no outside-click/Escape dismissal | **ACCEPT** → outside-pointer + Escape dismissal + focus handling (Task 10). |
| #12 | MEDIUM | Rust clamp underspecified; primary monitor ≠ caller's; window-state can restore oversized | **ACCEPT** → derive from current/caller monitor; safe fallback; **post-`build()` clamp** if restored geometry exceeds the work area (Task 12). |
| #13 | MEDIUM | App-level mount test isn't a layout guard | **ACCEPT** → labeled a smoke test; real viewport tests own the shell invariants (Task 7). |

### Claude adversarial review rounds 2-5 dispositions (4 lenses: CSS-cascade / radio-safety / a11y / coordination)

These supersede/augment the task bodies where they conflict — **the corrections below are authoritative.** (Full transcript: workflow `wf_0e94e4a6-194`.) Five "VERIFIED OK" refutations were also returned — do NOT re-investigate: nested `display:contents` grid placement is correct; the `min-width:0` override beats `RadioPanel.css` on specificity (not source order); `translateX(100%)` correctly hides the closed body; `.panes--with-dock` source-order win holds; the grip is a real button with no focus-trap.

| F | Sev | Finding | Authoritative fix |
|---|---|---|---|
| **F1** | **CRITICAL** | `.nav-label { display:none }` strips the accessible name from every rail button → all nameless to AT (WCAG 4.1.2). The plan's own CSS-string test asserts the broken rule. | **Use the visually-hidden CLIP pattern, not `display:none`** (code below). Fix the Task 10 test to assert the clip pattern + add an accessible-name check. |
| **F2+F13** | **HIGH** | `deriveDrawerSessionState` reads binary `modemIsActive` → shows GREEN/grey during an RF *connecting* handshake (the 2026-05-22 runaway window) — never reaches amber. A lying safety indicator. | **Switch on `statusData.status.kind`** (code below): map Connecting→connecting(amber), Connected→connected, Listening→connecting(armed), Disconnecting→disconnecting, Error→error, else disconnected. Add unit branches for each. |
| **F3** | **HIGH** | R6's "honest grip → expand-tap-to-abort" is circular while F2 is unfixed; the only RF stop is inside the collapsed panel. | **PARTIAL.** Fixing F2 makes the grip an honest amber cue (the in-scope mitigation). A new "ribbon Abort for all transports" is a **radio-UX/safety feature the operator owns** (`feedback_no_tuxlink_added_safeguards`, `feedback_radio1_governs_tx_not_ui`) — do NOT build it speculatively in this responsive PR. **File a bd follow-up + surface it as an explicit operator smoke-note** (see revised R6). |
| **F4** | **HIGH** | Hiding `.section-label` (a flex container) also hides the `+` create-folder button — the only folder-creation path in compact. | **Hide section-label TEXT only** (wrap each section heading's text in a span; clip-hide the span; keep the container + `+` button). Task 10 + Task 8. |
| **F5** | **HIGH** | Rail nav-items have no `:focus-visible`; the default outline is clipped by `.panes { overflow:hidden }` → invisible keyboard focus (WCAG 2.4.7). | Add an **inset** focus ring in compact (code below) + a Playwright check. |
| **F6** | **HIGH** | The radio-panel **interior** compact rules (segmented tabs ≥44px, chips, inputs, font floors, `.session-log` min-height) have a checklist row but **no task**. `.radio-panel-segmented` (in `src/radio/sections/ModemLinkSection.css`) is what CF's tabs reuse — it must exist. | **New Task 6b** (Phase 1, after Task 6) — see below. |
| **F7** | **HIGH** | Leftover `@import` clause (L67) contradicts the JS-import fix → false-clean regression guard. | **Fixed** (deleted). AppShell.css is unchanged; sole loader is the JS import. |
| **F8** | MEDIUM | `drawerOpen` not reset when `radioPanelMode→null` via modem-stop (only onClose resets) → next panel opens pre-expanded, violating "manual, default-closed". | Add `useEffect(() => { if (radioPanelMode === null) setDrawerOpen(false); }, [radioPanelMode]);` (Task 5) + a test. |
| **F9** | MEDIUM | Task 2 guard never reads `RadioDrawer.css` — a `display:flex` leak to desktop there is caught only by Playwright. | Add a `RadioDrawer.css?raw` scoping assertion (Task 4): pre-`@media` slice contains exactly the 3 desktop rules, no `display:flex/block`. |
| **F10** | MEDIUM | Opening the drawer leaves focus on the grip; Abort is N blind tabs away. | On open, move focus to panel root (`tabIndex={-1}` + `.focus()`); on close, return to grip (Task 5/4) + RTL test. |
| **F11** | MEDIUM | Coordination row: CF adds a new "Address" `section-label` + a Contacts item *in the `MAILBOX_ITEMS`-style list*. Task 8 must wrap CF's labels too, else raw text shows in the rail. | Task 8 contract = "**every** sidebar label incl. CF's Address/Contacts is `.nav-label`-wrapped"; add a "no bare text node is a direct nav-item child" test. (Verified against CF's Task A7.) |
| **F12** | MEDIUM | Coordination row cites the wrong mount for CF's tabs (per-mode panel, not `RadioPanel.tsx:58`); "zero overlap" still holds. | Doc fix + tie "reuse `.radio-panel-segmented`" to Task 6b. |

**F1 — visually-hidden clip for rail labels (replaces the `display:none` in Task 10's CSS):**

```css
  /* Rail labels: visually hidden but KEPT in the accessibility tree (Claude
   * adrev F1 — display:none would make every icon-only button nameless to a
   * screen reader). Section-label TEXT is wrapped in its own span and clipped
   * the same way, so the `+` create-folder button in the Folders section header
   * stays visible (F4). */
  .layout-b .sidebar .nav-label,
  .layout-b .sidebar .section-label-text {
    position: absolute;
    width: 1px; height: 1px;
    margin: -1px; padding: 0; border: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .layout-b .sidebar .count,
  .layout-b .sidebar .v01-badge { display: none; } /* decorative — safe to drop */
  /* Expanded overlay restores the clipped text. */
  .layout-b .sidebar.is-expanded .nav-label,
  .layout-b .sidebar.is-expanded .section-label-text {
    position: static; width: auto; height: auto;
    margin: 0; overflow: visible; clip-path: none; white-space: normal;
  }
  /* F5 — inset focus ring (the default outline is clipped by .panes overflow). */
  .layout-b .sidebar .nav-item:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
```

> Task 8 must therefore also wrap each **section-label** heading's text in `<span className="section-label-text">…</span>` (for "Mailbox", "Folders", "Connections", and CF's "Address"). The `+` button stays a direct child of the `.section-label` container, unclipped.

**F2+F13 — honest `deriveDrawerSessionState` (replaces the Task 5 body):**

```ts
// src/shell/drawerSessionState.ts
import type { RadioPanelState } from '../radio/RadioPanel';

export interface DrawerStateInputs {
  /** True while a CMS connect exchange is in flight (AppShell `connecting`). */
  connecting: boolean;
  /** The live transport status (AppShell `statusData.status`), or undefined. */
  status?: { kind?: string } | null;
  /** Fallback: modem in any active state (AppShell `useModemIsActive()`). */
  modemIsActive: boolean;
}

/**
 * Coarse session state for the drawer grip tick. CRITICAL (Claude adrev F2):
 * must NOT show 'connected'/'disconnected' during an RF *connecting* handshake —
 * that is exactly the runaway-connect window (2026-05-22) where the operator
 * needs abort urgency. Switch on the transport status kind so 'Connecting' and
 * 'Listening' surface amber. tuxlink-h7q7.
 */
export function deriveDrawerSessionState(i: DrawerStateInputs): RadioPanelState {
  if (i.connecting) return 'connecting';
  const k = i.status?.kind;
  if (k === 'Connecting') return 'connecting';
  if (k === 'Listening') return 'connecting'; // armed → amber, not green
  if (k === 'Disconnecting') return 'disconnecting';
  if (k === 'Error') return 'error';
  if (k === 'Connected') return 'connected';
  if (i.modemIsActive) return 'connecting'; // active but unknown kind → amber (cautious), not green
  return 'disconnected';
}
```

> Unit test branches: `connecting:true`→'connecting'; each `kind`→its mapping; `modemIsActive:true` with no status→'connecting' (cautious); else→'disconnected'. The cautious "active→amber" default is deliberate: an unknown-but-active modem must never read as a safe green. **Confirm the exact `status.kind` string values** against the Rust status type before finalizing (read the `statusData.status` type); adjust the literals to match.

**F3 — revised R6 (authoritative):** the mitigation for a collapsed-drawer live session is the **honest amber grip** (F2 fix), which makes a connecting/active session glanceable so the operator taps to expand → abort (one tap; RADIO-1 governs the TX-consent click, not abort placement). A *zero-expand* abort (ribbon Abort wired to every transport's disconnect) is a **radio-UX/safety feature the operator owns** and is NOT built in this PR (it borders on a tuxlink-added safeguard the operator has said to avoid). **File `tuxlink` follow-up issue** "compact collapsed-drawer abort reachability — decide zero-expand abort" (dep: h7q7) and **call it out explicitly in the operator smoke-note**: "at 1280, start an RF session, collapse the drawer — is the amber grip + one-tap-expand-to-abort acceptable, or do you want a ribbon abort?"

**F6 — New Task 6b: radio-panel interior compact CSS (Phase 1, after Task 6).**
- Files: `src/radio/RadioPanel.css`, `src/radio/sections/ModemLinkSection.css` (+ any `src/radio/sections/*.css` carrying the audited sub-floor selectors).
- `@media (max-width: 1365px)` block covering the §2.3 audit checklist: `.radio-panel-segmented` (ModemLinkSection.css) `min-height:44px; font-size:12px` (**the rule CF's Favorites/Recent/Manual tabs reuse — F12**); `radio-panel-btn-sm`/chips/chip-`✕` ≥44px; inputs/selects ≥44px; native radio bump; Listen header rows; font floors (segmented 11→12, h5 11→12, LIVE 10→11, help/pills 11→12, Listen 9px region→12); `.session-log { min-height: 160px }` (from 240). TDD via CSS-string assertions on each file. Commit: `feat(radio): compact interior — segmented tabs/chips/inputs touch + font floors (tuxlink-h7q7)`.
- **Coordination (C):** CF's new tab strip in `radio-panel-body` reuses `.radio-panel-segmented`; this task makes that class compact-correct so CF gets it for free.

---

## File structure (created / modified)

**New files:**
- `src/shell/useViewport.ts` — the compact-mode hook (`matchMedia`-driven) + the exported `COMPACT_MEDIA_QUERY` constant. Single responsibility: tell React whether we're in compact mode.
- `src/shell/useViewport.test.tsx` — hook tests.
- `src/shell/RadioDrawer.tsx` — the slide-over wrapper around the radio-panel mount block (grip handle + session-state tick + open/close). Single responsibility: drawer chrome + state; it does **not** know about radio internals (it wraps whatever children it's given).
- `src/shell/RadioDrawer.css` — drawer-specific compact CSS.
- `src/shell/RadioDrawer.test.tsx` — drawer behavior tests.
- `src/shell/compactShell.css` — the shell's `@media (max-width: 1365px)` block (panes grid rewrite, rail, ribbon clip fix, chrome/menubar/titlebar/statusbar touch+font floors). Kept separate from `AppShell.css` so the compact rules are reviewable as one unit and the desktop file stays untouched; bundled via a normal `import './compactShell.css'` in `AppShell.tsx` (NOT a CSS `@import` — the `?raw` test path can't see `@import`ed rules; Codex R1 #7).

**Modified files:**
- `src/shell/AppShell.tsx` — panes className (L841), `drawerOpen`/`railExpanded` state (near L242), wrap the radio-panel mount block (L936-1003) in `<RadioDrawer>`, add the `compact` class to the `.layout-b` root, import `compactShell.css`. **Coordination: different hunks from shoal-raven-gorge's content-switch (L869-929) + `selectedFolder` (L214).**
- `src/mailbox/FolderSidebar.tsx` — wrap the bare label text node (L184) in `<span className="nav-label">`; refactor the inline-styled `+` button (L211-225), empty-hint (L261-271), and create-btn so a media query can reach them. **Coordination: different hunk from shoal-raven-gorge's `MAILBOX_ITEMS` (L29-35), but the `.nav-label` wrap is inside the same `.map` body — agree which PR lands it.**
- `src/shell/AppShell.css` — **NO CHANGE** (desktop rules stay byte-identical). The compact stylesheet is loaded via a JS `import './compactShell.css'` in `AppShell.tsx` (after `import './AppShell.css'`), **not** a CSS `@import` here (Codex R1 #7; Claude adrev F7).
- `src/mailbox/MessageView.css` — leave `.reading-pane { min-width: 0 }` as-is (it's correct; the fix is removing the dock column, not fighting min-width). No edit expected — listed so the executor knows *not* to touch it.
- `src/compose/Compose.css` — in-window `@media (max-width: 1365px)` block (the compose window is a separate document; its width ≤1100 matches naturally).
- `src/compose/CheckInForm.css`, `src/compose/Ics309FormV2.css`, `src/compose/PositionFormV2.css` — embedded-form compact blocks.
- `src-tauri/src/.../compose_window.rs` — clamp the default inner height to the monitor work area (the **Rust** fix; CSS cannot reach window geometry). Exact path resolved in Task 12.
- `src/shell/SettingsPanel.css`, `src/shell/ThemeDesigner.css`, `src/shell/AboutDialog.css` — dialog compact blocks (DRY the close-button rule).
- `src/wizard/wizard.css` — wizard compact block (pure CSS, no JS hook).
- `src/forms/forms.css`, `src/forms/FormPicker.css`, `src/compose/WebviewFormHost.css`, `src/mailbox/WebviewFormViewer.css` — forms compact blocks.
- `src/mailbox/WebviewFormViewer.tsx` — wire an explicit reposition trigger to drawer/reader-width changes (Integration Risk R1).
- `src/App.test.tsx` — extend with an App-level mount assertion for the compact wiring.

### Shared-CSS scoping discipline (Integration Risks — read before editing)

- **R1 — embedded-webview occlusion + stale position (resolved by PUSH).** `WebviewFormViewer` (mailbox reader) and `WebviewFormHost` (compose) are child Tauri webviews that **paint above parent HTML** (`WebviewFormViewer.tsx:11-14`) and reposition only on `ResizeObserver(embed + document.body)`. An *absolute overlay* drawer would render *behind* the webview AND leave the placeholder rect unchanged (re-measuring sets the same position — useless). **Resolution:** the drawer **pushes** (a real 4th grid column), so opening it narrows the reader → the embed placeholder resizes → the `ResizeObserver` fires → the webview repositions to the narrower reader, never overlapped. Task 17 *verifies* this (it is no longer a manual signal-threading fix). Codex R1 #5/#6.
- **R2 — Compose is a separate window.** Its default height (820) > FZ-M1 usable (~760) clips the action bar. **Rust** fix (Task 12), separate from the in-window CSS (Task 13).
- **R3 — inline styles can't be reached by `@media`.** The sidebar `+` button / empty-hint / create-btn must be classed (Task 9) *before* the rail CSS pass.
- **R4 — `chrome.css .tux-ctrl` is shared** between the shell titlebar and the compose titlebar. Scope the compose bump to `.tux-compose-titlebar .tux-ctrl`; the shell owns the bare `.tux-ctrl`. No double-application.
- **R5 — `App.css` base input/button/radio sizing is global.** Do **not** add a global `.compact input{…}` rule in `App.css`; keep touch bumps per-surface so rules aren't double-applied. (Native radio/checkbox `width:auto` in `App.css:452-455` is the shared source of unpinned sizes — bump per-surface.)
- **R6 — abort accessibility under a collapsed drawer (emcomm safety).** When a radio session is live and the drawer is collapsed to the 44px grip, the panel's Abort/Disconnect controls are hidden. Per the operator's design choice the drawer does **not** auto-open (manual Option A), so this is NOT changed to auto-open. The mitigation is the **grip session-state tick** (`deriveDrawerSessionState` → amber "connecting" / green "connected"): it makes a live session visible at a glance so the operator knows to tap the grip → expand → abort. This is consistent with `feedback_radio1_governs_tx_not_ui` (RADIO-1 governs the TX-consent click, not button placement) and `feedback_no_tuxlink_added_safeguards` (no new modals/auto-behaviors). **Required:** the grip tick must be present and correct (Task 4 + Task 5 tests already assert `data-session-state`); the Playwright pass screenshots a "connecting" grip. Flagged for the operator smoke as a thing to eyeball: is one expand-tap-to-abort acceptable, or does the grip itself want a direct abort affordance? (Deferred — do not build speculatively.)

---

## Per-surface compact checklist (design open item #3, resolved)

Each row is a compact-scoped change set; full selector lists are in `dev/scratch/2026-06-07-fzm1-compact-audit-synthesis.md` §2. Phases below implement these.

| Surface | Trigger | Layout | Touch (≥44px) | Font floor (≥12px) | Density |
|---|---|---|---|---|---|
| **Shell** | `@media ≤1365px` + `.drawer-open`/`.is-expanded` state classes | panes templates → `48px 380px 1fr` base; with-dock reserves a collapsible 4th track (44px grip / 400px open — push drawer); null legacy 5th col | Connect/Abort, SSID select, grid-edit, GPS/MANUAL segments, set-manually, nav-item, MenuBar buttons, dropdown items, titlebar ctrls, sort trigger, `.row` min-height | `.dash-label`, `.dash-source-segment` (9px), GPS status/error, `.section-label`, nav count/icon, badges, status divider | search-zone 560→360, connection max-w 260→180, dashboard gap 28→16 |
| **Mailbox** | inherits shell | `.nav-label` span for rail hide; keep `.reading-pane min-width:0` | nav rows, reader action-btns, sort trigger, inline `+`→class, attachment Save/Preview, ctx-menu, folder-dialog btns | `.section-label`, nav icon→16, count, `.form-tag`, `.size`, `msg-meta dt`, inline empty-hint/create-btn→class | rail icon centering |
| **Radio drawer** | `.compact` (JS state) | extract aside from grid → `position:absolute` drawer; override `min-width:400→0`, `width: min(400px, 92vw)` | close, primary/danger btns, segmented tabs, btn-sm, chips, chip-`✕`, inputs/selects, native radio, Listen header | segmented/h5/help/pills 11→12, LIVE 10→11, Listen 9px→12 | `.session-log min-height 240→160` |
| **Compose (window)** | (a) Rust height clamp; (b) `@media ≤1365px` on compose doc | n/a (separate window; min-width safe) | action btns, inputs, receipt checkbox, attachments 36→48, `.tux-compose-titlebar .tux-ctrl`, embedded inputs/btns, CheckIn radios, ICS-309 datetime-local | `.compose-hint`, `fix-badge`/`grid-error` 11→12; **do NOT shrink 14px root (ICS-309 rem-based)** | embedded-form padding 16→10, gap 12→8 |
| **Settings dialog** | `@media ≤1365px` | modal width fine | `.tux-settings-opt` min-h 44 + native radio; close btn (DRY ×3) | opt-help 11→13, legend, error | — |
| **Theme dialog** | same | optional card→`min(720px,…)` | **swatch 36×28→44×44 (×24)**, hex/name/select inputs, action btns, close btn | token/group/field help 11→13, hex 12→13 | tighten group padding 14→12 |
| **About dialog** | same | meta grid fine; row-gap 6→10 | footer Close, **5 meta links→inline-block padding+44px**, close btn | `.tux-about-meta` 12→13, credit, prealpha | adjacent-link separation |
| **Wizard** | **pure CSS `@media ≤1365px` in wizard.css** | keep 580px card; lower `.wizard-root` top-pad to `clamp(16px,3vh,32px)`; card pad 38/40→24/28; session-log `max-height:30vh` | submit-row btns, inputs, password toggle, link-button, Retry; **inline Register anchor → flag to design** | bump 12/12.5px→13px; mono log + failed-detail + faint footer → 13px + lighten | vertical-budget check ~760px |
| **HTML forms** | `@media ≤1365px` ×4 files | `.ics309-log-entry`→1-col; `.damage-category` 6→2 col; legend input 200px→100% | picker rows, action btns, native inputs/checkbox, toolbar select/btns, +Add | field `label`, `log-entry>strong`, table `th` 11→12 | + R1 webview reposition guard |

---

## Testing strategy (how each layer is verified)

1. **CSS-string assertions (jsdom):** raw-import each stylesheet (`import css from './x.css?raw'`) — for the shell, import `AppShell.css?raw` (desktop) and `compactShell.css?raw` (compact) **separately** because a `?raw` import does not inline `@import` (Codex R1 #7). Assert the desktop file contains the bare grid templates and **no** `max-width: 1365px`; assert the compact file's rules all live inside the `@media` block. A *first* guard only — see item 5.
2. **Hook + component behavior (RTL + jsdom):** mock `window.matchMedia`; assert `useViewport` returns compact true/false on the threshold; assert the drawer/rail toggles add/remove the state classes; assert `.nav-label` renders; assert inline-style refactors became classes.
3. **App-level mount (`App.test.tsx`):** mount `<App />` (the production path that wraps `QueryClientProvider` *after* selecting AppShell — the tuxlink-n4hz "test the production mount path" lesson) and assert the compact wiring mounts without crashing.
4. **Rust unit test (Phase 3a):** test the height-clamp pure function against work-area inputs.
5. **Operator browser-smoke (the merge gate) + optional Playwright pass at 1280×800:** real media-query evaluation. jsdom cannot do this; the operator browser-smokes at real window sizes before merge.

**Test runner:** `pnpm exec vitest run <files>` (narrow scope — never a full sweep; vitest leaks ~8.5 GB of orphaned workers. `pkill -9 -f vitest` after each run). Typecheck: `pnpm typecheck`. Rust: `cargo test --manifest-path src-tauri/Cargo.toml <filter>`.

---

## Phase 0 — Regression guard + compact mechanism + shared tokens (MUST land first)

### Task 1: The `useViewport` compact hook + shared breakpoint constant

**Files:**
- Create: `src/shell/useViewport.ts`
- Test: `src/shell/useViewport.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/shell/useViewport.test.tsx
import { describe, it, expect, vi, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useViewport, COMPACT_MEDIA_QUERY } from './useViewport';

// jsdom has no real matchMedia; install a controllable mock.
function installMatchMedia(initialMatches: boolean) {
  const listeners = new Set<(e: MediaQueryListEvent) => void>();
  const mql = {
    matches: initialMatches,
    media: COMPACT_MEDIA_QUERY,
    addEventListener: (_: string, cb: (e: MediaQueryListEvent) => void) => listeners.add(cb),
    removeEventListener: (_: string, cb: (e: MediaQueryListEvent) => void) => listeners.delete(cb),
  };
  vi.stubGlobal('matchMedia', (q: string) => {
    expect(q).toBe(COMPACT_MEDIA_QUERY); // the hook MUST use the shared constant
    return mql;
  });
  return {
    fire(matches: boolean) {
      mql.matches = matches;
      listeners.forEach((cb) => cb({ matches } as MediaQueryListEvent));
    },
  };
}

afterEach(() => vi.unstubAllGlobals());

describe('useViewport', () => {
  it('exports the canonical compact media query string (strictly below the 1366px desktop floor)', () => {
    expect(COMPACT_MEDIA_QUERY).toBe('(max-width: 1365px)');
  });

  it('reports compact=true when the media query matches at mount', () => {
    installMatchMedia(true);
    const { result } = renderHook(() => useViewport());
    expect(result.current.isCompact).toBe(true);
  });

  it('reports compact=false above the breakpoint', () => {
    installMatchMedia(false);
    const { result } = renderHook(() => useViewport());
    expect(result.current.isCompact).toBe(false);
  });

  it('updates when the media query changes (resize across the breakpoint)', () => {
    const ctl = installMatchMedia(false);
    const { result } = renderHook(() => useViewport());
    expect(result.current.isCompact).toBe(false);
    act(() => ctl.fire(true));
    expect(result.current.isCompact).toBe(true);
  });
});
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `pnpm exec vitest run src/shell/useViewport.test.tsx`
Expected: FAIL — `useViewport`/`COMPACT_MEDIA_QUERY` not found. Then `pkill -9 -f vitest`.

- [ ] **Step 3: Write the minimal implementation**

```ts
// src/shell/useViewport.ts
import { useEffect, useState } from 'react';

/**
 * The single source of truth for the FZ-M1 compact breakpoint. The CSS
 * `@media (max-width: 1365px)` blocks (compactShell.css, RadioDrawer.css,
 * wizard.css, dialog/forms compact blocks) MUST mirror this exact string.
 * Because the hook evaluates the identical media query via matchMedia, the
 * CSS-driven layout and the JS-driven interactive state can never disagree
 * about whether we are in compact mode.
 *
 * Strictly below 1366px so the "desktop >=1366px is unchanged" invariant holds
 * with no boundary overlap (Codex R1 #1).
 *
 * tuxlink-h7q7 / docs/design/2026-06-07-fzm1-responsive-design.md.
 */
export const COMPACT_MEDIA_QUERY = '(max-width: 1365px)';

export interface Viewport {
  /** True when the viewport is at/below the FZ-M1 compact breakpoint. */
  isCompact: boolean;
}

/**
 * Reports whether the app is in compact (tablet) mode. Used only for the
 * JS-stateful compact bits (the radio slide-over drawer open/close + the
 * icon-rail expand overlay). All *static* compact layout/typography lives in
 * CSS media queries that need no JS — see the design's §Components.
 */
export function useViewport(): Viewport {
  const [isCompact, setIsCompact] = useState<boolean>(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return false;
    return window.matchMedia(COMPACT_MEDIA_QUERY).matches;
  });

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const mql = window.matchMedia(COMPACT_MEDIA_QUERY);
    const onChange = (e: MediaQueryListEvent) => setIsCompact(e.matches);
    setIsCompact(mql.matches);
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  }, []);

  return { isCompact };
}
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `pnpm exec vitest run src/shell/useViewport.test.tsx` → PASS. Then `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/shell/useViewport.ts src/shell/useViewport.test.tsx
git commit -m "feat(shell): useViewport compact-mode hook + shared breakpoint constant (tuxlink-h7q7)"
```

### Task 2: Desktop regression-guard test (lands before any compact CSS)

**Files:**
- Create: `src/shell/AppShell.compact.test.tsx`

This test pins the desktop layout so every subsequent compact task proves desktop is untouched. It also pre-asserts the compact-CSS contract so later CSS tasks have a target.

- [ ] **Step 1: Write the test (it will FAIL until the compact CSS exists)**

```tsx
// src/shell/AppShell.compact.test.tsx
import { describe, it, expect } from 'vitest';
// IMPORTANT (Codex R1 #7): a Vite `?raw` import of AppShell.css does NOT inline
// `@import './compactShell.css'` — it returns the literal import line. So we
// raw-import BOTH files and concatenate. desktopCss (AppShell.css) holds the
// untouched desktop rules; compactCss (compactShell.css) holds the @media block.
import desktopCss from './AppShell.css?raw';
import compactCss from './compactShell.css?raw';

const COMPACT = '@media (max-width: 1365px)';

describe('AppShell desktop regression guard (tuxlink-h7q7)', () => {
  it('keeps the desktop panes grid templates in AppShell.css, unscoped (no @media)', () => {
    // The desktop file must NOT contain any non-print compact media query.
    expect(desktopCss).not.toContain('max-width: 1365px');
    // The three desktop templates exist as bare (un-media-scoped) rules.
    expect(desktopCss).toContain('grid-template-columns: 200px 380px 1fr');
    expect(desktopCss).toContain('grid-template-columns: 200px 380px 1fr 400px');
  });
});

describe('AppShell compact CSS contract (tuxlink-h7q7)', () => {
  it('puts every compact rule inside the 1365px breakpoint (compactShell.css)', () => {
    expect(compactCss).toContain(COMPACT);
    // No compact rule may live outside the media query (would leak to desktop).
    const beforeBlock = compactCss.slice(0, compactCss.indexOf(COMPACT));
    expect(beforeBlock.replace(/\/\*[\s\S]*?\*\//g, '').trim()).toBe('');
  });

  it('rewrites the panes grid inside the compact block: 48px rail + reserved drawer track', () => {
    const block = compactCss.slice(compactCss.indexOf(COMPACT));
    expect(block).toContain('48px 380px 1fr'); // rail + list + reader (drawer track appended)
  });
});
```

> **CSS load mechanism:** AppShell.tsx gets the compact rules via a normal `import './compactShell.css'` (Vite bundles it — NOT a CSS `@import`, which the `?raw` test path can't see). Confirm `vite.config.ts` allows `?raw` imports (default in Vite) by reading how `AppShell.test.tsx` currently obtains its raw CSS; match that mechanism. If the existing test uses a custom `APP_SHELL_CSS_MODULES` shim rather than `?raw`, mirror *that* and concatenate the two files through it instead.

- [ ] **Step 2: Run, verify the compact-contract cases FAIL (guard cases pass)**

Run: `pnpm exec vitest run src/shell/AppShell.compact.test.tsx`
Expected: the "desktop regression guard" case PASSES (desktop templates already exist); the "compact CSS contract" cases FAIL (no compact block yet). Then `pkill -9 -f vitest`.

- [ ] **Step 3: Create the empty compact stylesheet + wire the import**

```css
/* src/shell/compactShell.css */
/* FZ-M1 compact mode — additive, scoped. Mirrors COMPACT_MEDIA_QUERY in
 * src/shell/useViewport.ts (keep both at 1365px). Desktop (>=1366px) is
 * unaffected: EVERY rule here lives inside the media query. tuxlink-h7q7. */
@media (max-width: 1365px) {
  /* panes grid + rail + ribbon + chrome compact rules land in Phase 1-2 */
}
```

Wire it into the bundle via a normal JS import in `src/shell/AppShell.tsx` (NOT a CSS `@import` — the `?raw` test path can't see `@import`ed rules). Add alongside the existing `import './AppShell.css';`:

```tsx
import './AppShell.css';
import './compactShell.css'; // FZ-M1 compact rules (tuxlink-h7q7)
```

- [ ] **Step 4: Run, verify the breakpoint-exists case passes; the `48px` case still fails**

Run: `pnpm exec vitest run src/shell/AppShell.compact.test.tsx`
Expected: "puts every compact rule inside the breakpoint" PASSES; "rewrites the panes grid" still FAILS (no `48px` rule yet — Task 6 adds it). The desktop-guard case PASSES. Then `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/shell/AppShell.compact.test.tsx src/shell/compactShell.css src/shell/AppShell.tsx
git commit -m "test(shell): desktop regression guard + compact-CSS scaffold (tuxlink-h7q7)"
```

### Task 3: Toggle the `.compact` root class from the hook

**Files:**
- Modify: `src/shell/AppShell.tsx` (the `.layout-b` root className; import `useViewport`)
- Modify: `src/shell/AppShell.test.tsx` (assert the class toggles)

- [ ] **Step 1: Write the failing test** (add to `AppShell.test.tsx`, reusing its `renderShell()` + a matchMedia mock)

```tsx
// add near the other describe blocks in AppShell.test.tsx
import { COMPACT_MEDIA_QUERY } from './useViewport';

describe('AppShell compact root class (tuxlink-h7q7)', () => {
  function mockCompact(matches: boolean) {
    vi.stubGlobal('matchMedia', (q: string) => ({
      matches: q === COMPACT_MEDIA_QUERY ? matches : false,
      media: q,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
  }
  afterEach(() => vi.unstubAllGlobals());

  it('adds the compact class to .layout-b when matchMedia matches', () => {
    mockCompact(true);
    const { container } = renderShell();
    expect(container.querySelector('.layout-b')?.classList.contains('compact')).toBe(true);
  });

  it('omits the compact class above the breakpoint', () => {
    mockCompact(false);
    const { container } = renderShell();
    expect(container.querySelector('.layout-b')?.classList.contains('compact')).toBe(false);
  });
});
```

- [ ] **Step 2: Run, verify it fails**

Run: `pnpm exec vitest run src/shell/AppShell.test.tsx` → FAIL (no compact class). `pkill -9 -f vitest`.

- [ ] **Step 3: Wire the hook into the root className**

In `src/shell/AppShell.tsx`: import `useViewport`; call it near the other hooks (~L242, with the other `useState` calls — **a different hunk from shoal-raven-gorge's `selectedFolder` at L214**); compose the root class. Find the `.layout-b` root element (the outermost return element) and change its className from a static `"layout-b"` to:

```tsx
const { isCompact } = useViewport();
// ...
<div className={`layout-b${isCompact ? ' compact' : ''}`} /* ...existing props... */>
```

- [ ] **Step 4: Run, verify it passes**

Run: `pnpm exec vitest run src/shell/AppShell.test.tsx` → PASS. `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/shell/AppShell.tsx src/shell/AppShell.test.tsx
git commit -m "feat(shell): toggle .compact root class from useViewport (tuxlink-h7q7)"
```

---

## Phase 1 — Shell grid + radio drawer (the core fix; HIGH; coordinate with shoal-raven-gorge §5-A)

### Task 4: `RadioDrawer` wrapper component (grip + session-state tick + open/close)

**Files:**
- Create: `src/shell/RadioDrawer.tsx`
- Create: `src/shell/RadioDrawer.css`
- Test: `src/shell/RadioDrawer.test.tsx`

**Design contract:** In desktop, the drawer is transparent (`display: contents` — its child radio panel IS the 4th grid column, byte-identical to today). In compact, the wrapper becomes a `position:absolute` slide-over anchored to `.panes` (`position:relative` already, AppShell.css:53); a grip handle toggles `.is-open`; the grip shows a session-state tick. The wrapper renders **only** when a radio panel is mounted (same `radioPanelMode !== null` condition as `panes--with-dock`).

- [ ] **Step 1: Write the failing test**

```tsx
// src/shell/RadioDrawer.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { RadioDrawer } from './RadioDrawer';

describe('RadioDrawer', () => {
  it('renders its children (the radio panel) and a grip handle', () => {
    render(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="disconnected">
        <div data-testid="panel-child">panel</div>
      </RadioDrawer>,
    );
    expect(screen.getByTestId('panel-child')).toBeInTheDocument();
    expect(screen.getByTestId('radio-drawer-grip')).toBeInTheDocument();
  });

  it('reflects open state via the is-open class for CSS to animate', () => {
    const { container, rerender } = render(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="disconnected">
        <div />
      </RadioDrawer>,
    );
    expect(container.querySelector('.radio-drawer')?.classList.contains('is-open')).toBe(false);
    rerender(
      <RadioDrawer open={true} onToggle={() => {}} sessionState="disconnected">
        <div />
      </RadioDrawer>,
    );
    expect(container.querySelector('.radio-drawer')?.classList.contains('is-open')).toBe(true);
  });

  it('fires onToggle when the grip is tapped', () => {
    const onToggle = vi.fn();
    render(
      <RadioDrawer open={false} onToggle={onToggle} sessionState="connecting">
        <div />
      </RadioDrawer>,
    );
    fireEvent.click(screen.getByTestId('radio-drawer-grip'));
    expect(onToggle).toHaveBeenCalledOnce();
  });

  it('surfaces session state on the grip (data attribute) for the tick styling', () => {
    render(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="connected">
        <div />
      </RadioDrawer>,
    );
    expect(screen.getByTestId('radio-drawer-grip').getAttribute('data-session-state')).toBe('connected');
  });

  it('grip is an accessible toggle button with a ≥44px hit target class', () => {
    render(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="disconnected">
        <div />
      </RadioDrawer>,
    );
    const grip = screen.getByTestId('radio-drawer-grip');
    expect(grip.tagName).toBe('BUTTON');
    expect(grip).toHaveAttribute('aria-expanded', 'false');
    expect(grip).toHaveAttribute('aria-label');
  });
});
```

- [ ] **Step 2: Run, verify it fails**

Run: `pnpm exec vitest run src/shell/RadioDrawer.test.tsx` → FAIL. `pkill -9 -f vitest`.

- [ ] **Step 3: Implement the component**

```tsx
// src/shell/RadioDrawer.tsx
import type { ReactNode } from 'react';
import type { RadioPanelState } from '../radio/RadioPanel';
import './RadioDrawer.css';

export interface RadioDrawerProps {
  /** Drawer open/closed (only meaningful in compact mode; desktop ignores it via CSS). */
  open: boolean;
  /** Toggle handler (grip tap). */
  onToggle: () => void;
  /** Current session state — drives the grip's session-state tick. */
  sessionState: RadioPanelState;
  /** The radio panel mount block. */
  children: ReactNode;
}

/**
 * Wraps the radio-panel mount block. Desktop (>=1366px): `display: contents`
 * (CSS), so the child panel IS the 4th grid column — visually identical to the
 * pre-compact layout (the wrapper is layout- and a11y-transparent; we keep
 * display:contents rather than conditionally wrapping because a conditional
 * wrap would REMOUNT the live radio panel on a resize across the breakpoint,
 * dropping session state — Codex R1 #4). Compact (<1366px): the wrapper IS the
 * grid's collapsible 4th column — 44px (grip only) when closed, 400px (panel)
 * when open. It PUSHES (reflows the reader) rather than overlaying, because a
 * child Tauri webview paints above parent HTML (Codex R1 #5). The grip shows a
 * coarse session-state tick and toggles open/closed. tuxlink-h7q7.
 */
export function RadioDrawer({ open, onToggle, sessionState, children }: RadioDrawerProps) {
  return (
    <div className={`radio-drawer${open ? ' is-open' : ''}`} data-testid="radio-drawer">
      <button
        type="button"
        className="radio-drawer-grip"
        data-testid="radio-drawer-grip"
        data-session-state={sessionState}
        aria-expanded={open}
        aria-label={open ? 'Close radio panel' : 'Open radio panel'}
        onClick={onToggle}
      >
        <span className="radio-drawer-grip-dot" aria-hidden="true" />
      </button>
      <div className="radio-drawer-body">{children}</div>
    </div>
  );
}
```

```css
/* src/shell/RadioDrawer.css */
/* Desktop (>=1366px): the wrapper AND its body are display:contents, so the
 * radio panel sits in the grid's 4th column exactly as before; the grip is
 * hidden. tuxlink-h7q7. */
.radio-drawer { display: contents; }
.radio-drawer-body { display: contents; }
.radio-drawer-grip { display: none; }

@media (max-width: 1365px) {
  /* Compact: the wrapper IS the grid's 4th column (the grid template reserves
   * 44px closed / 400px open — see compactShell.css). It PUSHES the reader; it
   * does NOT overlay (a child webview would paint over an HTML overlay). */
  .radio-drawer {
    display: flex;
    height: 100%;
    min-width: 0;
    overflow: hidden;
    position: relative;
    background: var(--surface);
    border-left: 1px solid var(--border);
  }
  /* Body: fills the column when open; clipped to 0 when the column is 44px. The
   * panel cosmetically slides in (composited transform); the reader reflow that
   * repositions the form-viewer webview is INSTANT (grid-template-columns has no
   * transition), so no transitionend re-measure is needed. */
  .radio-drawer-body {
    display: block;
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    overflow: auto;
    transform: translateX(100%);
    transition: transform 220ms ease;
  }
  .panes.drawer-open .radio-drawer-body { transform: translateX(0); }
  /* Override the panel's own rigid 400px floor so it fits the column. */
  .radio-drawer-body .radio-panel { width: 100%; min-width: 0; }

  /* Grip: ≥44px-wide hit target. Closed → it fills the 44px column (centered).
   * Open → a tab pinned to the panel's left edge to collapse without dropping
   * the connection. Always reachable; never under the webview (it lives in its
   * own grid track, not over the reader). */
  .radio-drawer-grip {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 44px;
    width: 44px;
    min-height: 44px;
    padding: 0;
    background: var(--surface-2);
    border: 0;
    border-right: 1px solid var(--border);
    cursor: pointer;
  }
  .panes.drawer-open .radio-drawer-grip {
    position: absolute;
    top: 50%;
    left: 0;
    transform: translateY(-50%);
    height: 56px;
    min-height: 56px;
    border-radius: 0 6px 6px 0;
    z-index: 1;
  }
  .radio-drawer-grip-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--text-faint);
  }
  .radio-drawer-grip[data-session-state='connecting'] .radio-drawer-grip-dot { background: var(--accent-2, #fbbf24); animation: radio-drawer-pulse 1.2s ease-in-out infinite; }
  .radio-drawer-grip[data-session-state='connected'] .radio-drawer-grip-dot { background: var(--success, #4ade80); box-shadow: 0 0 5px var(--success, #4ade80); }
  .radio-drawer-grip[data-session-state='disconnecting'] .radio-drawer-grip-dot { background: var(--accent-2, #fbbf24); }
  .radio-drawer-grip[data-session-state='error'] .radio-drawer-grip-dot { background: var(--error, #f87171); }
}
@keyframes radio-drawer-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
@media (prefers-reduced-motion: reduce) {
  .radio-drawer-body { transition: none; }
  .radio-drawer-grip-dot { animation: none !important; }
}
```

> **Note:** the `.is-open` class on `.radio-drawer` (set by the component) is retained for the test contract and a11y (`aria-expanded`), but the *layout* keys off `.panes.drawer-open` (set in Task 5) so the grid column and the panel slide stay in lockstep with one source of truth. Both are toggled together.

- [ ] **Step 4: Run, verify it passes**

Run: `pnpm exec vitest run src/shell/RadioDrawer.test.tsx` → PASS. `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/shell/RadioDrawer.tsx src/shell/RadioDrawer.css src/shell/RadioDrawer.test.tsx
git commit -m "feat(shell): RadioDrawer slide-over wrapper with session-state grip (tuxlink-h7q7)"
```

### Task 5: Mount `RadioDrawer` around the radio-panel block + drawer state in AppShell

**Files:**
- Modify: `src/shell/AppShell.tsx` — add `drawerOpen` state; wrap L936-1003; derive `sessionState`.
- Modify: `src/shell/AppShell.radioPanel.test.tsx` — assert the drawer wraps the panel + default-closed in compact.

**Coordination (§5-A, the top risk):** these are different hunks from shoal-raven-gorge's content-switch (L869-929) and `selectedFolder` (L214), but they're on the same file and *adjacent* to their region. The radio-panel block (L936-1003) is below their switch. Wrap, don't reorder. Derive `sessionState` from the existing modem state already present in AppShell (the same value the per-mode panels pass to `RadioPanel state=`); if no single shell-level value exists, default to `'disconnected'` and wire the real value in a follow-up (the grip tick degrades gracefully).

- [ ] **Step 1: Write the failing test** (in `AppShell.radioPanel.test.tsx`, with a matchMedia compact mock + a `radioPanelMode` that mounts a panel)

```tsx
// Assert: when a radio panel is mounted in compact mode, it is wrapped by the
// RadioDrawer (the grip exists) and the drawer defaults to closed (manual
// open — operator chose plain Option A, no auto-open).
it('wraps the radio panel in a closed drawer in compact mode (tuxlink-h7q7)', () => {
  // mockCompact(true) + drive the shell to a state where radioPanelMode != null
  // (follow the existing helper in this file that opens a Telnet/Packet panel).
  // ...render...
  expect(screen.getByTestId('radio-drawer')).toBeInTheDocument();
  expect(screen.getByTestId('radio-drawer').classList.contains('is-open')).toBe(false);
  expect(screen.getByTestId('radio-panel-root')).toBeInTheDocument(); // panel still mounted
});
```

> Read `AppShell.radioPanel.test.tsx` first to reuse its existing panel-opening helper rather than reconstructing the modem-state setup.

- [ ] **Step 2: Run, verify it fails.** `pnpm exec vitest run src/shell/AppShell.radioPanel.test.tsx` → FAIL. `pkill -9 -f vitest`.

- [ ] **Step 3: Implement.** In `AppShell.tsx`:
  - Add state near L242: `const [drawerOpen, setDrawerOpen] = useState(false);`
  - Import `RadioDrawer` and `deriveDrawerSessionState` (define the latter — see below — in a small `src/shell/drawerSessionState.ts` so it is unit-testable on its own).
  - Add `' drawer-open'` to the `.panes` className when `drawerOpen` (this is the single layout source of truth — see Task 4's CSS note). The L841 className becomes:

```tsx
className={`panes${radioPanelMode !== null ? ' panes--with-dock' : ''}${drawerOpen ? ' drawer-open' : ''}`}
```

  - Wrap the entire radio-panel mount block (the conditionals at L936-1003) in:

```tsx
{radioPanelMode !== null && (
  <RadioDrawer
    open={drawerOpen}
    onToggle={() => setDrawerOpen((o) => !o)}
    sessionState={deriveDrawerSessionState({
      connecting,
      status: statusData.status,
      modemIsActive,
    })}
  >
    {/* the existing L936-1003 conditional panels, unchanged */}
  </RadioDrawer>
)}
```

  - When the panel is closed by its `onClose` (which already calls `setSelectedConnection(null); setPinRadioPanel(false)`), also `setDrawerOpen(false)` so a re-opened panel starts closed. Refactor the 6 identical `onClose` handlers into one `closeRadioPanel` callback (DRY) that adds `setDrawerOpen(false)`.

  **`deriveDrawerSessionState` (Codex R1 #9 — a real coarse signal, not a hardcoded `'disconnected'`):**

```ts
// src/shell/drawerSessionState.ts
import type { RadioPanelState } from '../radio/RadioPanel';

export interface DrawerStateInputs {
  /** True while a CMS connect exchange is in flight (AppShell `connecting`). */
  connecting: boolean;
  /** The live transport status (AppShell `statusData.status`), or undefined. */
  status?: { kind?: string } | null;
  /** Whether the modem is in any active state (AppShell `useModemIsActive()`). */
  modemIsActive: boolean;
}

/**
 * Coarse session state for the drawer grip tick. The shell cannot observe every
 * per-transport sub-state, so this surfaces an honest three-way signal
 * (connecting / connected / disconnected). The open panel still shows granular
 * per-mode state. tuxlink-h7q7 / Codex R1 #9.
 */
export function deriveDrawerSessionState(i: DrawerStateInputs): RadioPanelState {
  if (i.connecting) return 'connecting';
  if (i.status?.kind === 'Connected' || i.modemIsActive) return 'connected';
  return 'disconnected';
}
```

  Add a unit test `src/shell/drawerSessionState.test.ts` covering the three branches (connecting → 'connecting'; Connected/modemIsActive → 'connected'; else → 'disconnected').

- [ ] **Step 4: Run, verify it passes.** `pnpm exec vitest run src/shell/AppShell.radioPanel.test.tsx` → PASS. `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/shell/AppShell.tsx src/shell/AppShell.radioPanel.test.tsx
git commit -m "feat(shell): mount radio panel in RadioDrawer with manual open/close (tuxlink-h7q7)"
```

### Task 6: Compact panes grid (rail track + collapsible drawer track + null legacy column)

**Files:**
- Modify: `src/shell/compactShell.css`
- Modify: `src/shell/AppShell.compact.test.tsx`

- [ ] **Step 1: Extend the test** — assert the compact grid: 48px rail; no-dock base = 3 cols; with-dock reserves a 4th track that is 44px closed / 400px open; legacy 5th nulled.

```tsx
it('uses a 48px rail and 3-column base in compact (no radio panel)', () => {
  const block = compactCss.slice(compactCss.indexOf(COMPACT));
  expect(block).toMatch(/\.layout-b \.panes\b[^{]*\{[^}]*grid-template-columns:\s*48px 380px 1fr\s*;/);
});
it('reserves a collapsible 44px grip / 400px open drawer track with-dock (push, not overlay)', () => {
  const block = compactCss.slice(compactCss.indexOf(COMPACT));
  // closed: 4th track is the 44px grip
  expect(block).toMatch(/\.panes--with-dock\b[^{]*\{[^}]*grid-template-columns:\s*48px 380px 1fr 44px\s*;/);
  // open: 4th track widens to the 400px panel
  expect(block).toMatch(/\.panes--with-dock\.drawer-open\b[^{]*\{[^}]*grid-template-columns:\s*48px 380px 1fr 400px\s*;/);
});
it('nulls the legacy 5th column in compact', () => {
  const block = compactCss.slice(compactCss.indexOf(COMPACT));
  expect(block).toContain('panes--with-legacy-dock');
});
```

- [ ] **Step 2: Run, verify the new cases fail.** `pnpm exec vitest run src/shell/AppShell.compact.test.tsx`. `pkill -9 -f vitest`.

- [ ] **Step 3: Add the grid rules** inside the `@media (max-width: 1365px)` block of `compactShell.css`:

```css
  /* Panes grid: 200px sidebar → 48px rail. The radio panel is now a COLLAPSIBLE
   * 4th column (push drawer): 44px grip when closed, 400px when open. It is NOT
   * an absolute overlay (a child webview would paint over it — Codex R1 #5).
   * The legacy 5th column (dead today) collapses into the same template in case
   * a future dual-mount re-applies the class.
   * Reader 1fr at 1280px: ~808px (drawer closed) / ~452px (open) — both beat the
   * desktop dock's ~300px. */
  .layout-b .panes {
    grid-template-columns: 48px 380px 1fr;
  }
  .layout-b .panes--with-dock,
  .layout-b .panes--with-dock.panes--with-legacy-dock {
    grid-template-columns: 48px 380px 1fr 44px; /* closed: grip only */
  }
  .layout-b .panes--with-dock.drawer-open,
  .layout-b .panes--with-dock.panes--with-legacy-dock.drawer-open {
    grid-template-columns: 48px 380px 1fr 400px; /* open: panel */
  }
```

- [ ] **Step 4: Run, verify all compact-grid + the Task 2 `48px` case pass.** `pnpm exec vitest run src/shell/AppShell.compact.test.tsx` → PASS. `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/shell/compactShell.css src/shell/AppShell.compact.test.tsx
git commit -m "feat(shell): compact panes grid — 48px rail + collapsible push-drawer track (tuxlink-h7q7)"
```

### Task 7: App-level mount **smoke** test (production path)

**Files:**
- Modify: `src/App.test.tsx`

> **Scope (Codex R1 #13):** this is a *smoke* test — it proves the production `<App/>` tree (which wraps `QueryClientProvider` *around* AppShell — the tuxlink-n4hz "test the production mount path" lesson) mounts without crashing in compact mode. It is **not** a layout/responsive guard; the shell's compact invariants are owned by the CSS-string tests (first guard) and the mandatory Playwright pass (real guard). Do not over-claim it.

- [ ] **Step 1: Write the test** — mount `<App />` in compact mode and assert it renders the shell without crashing.

```tsx
// add to src/App.test.tsx
import { COMPACT_MEDIA_QUERY } from './shell/useViewport';

describe('App compact wiring (tuxlink-h7q7)', () => {
  it('mounts the production App tree in compact mode without crashing', async () => {
    vi.stubGlobal('matchMedia', (q: string) => ({
      matches: q === COMPACT_MEDIA_QUERY,
      media: q,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
    // follow App.test.tsx's existing wizard-completed mock so it renders AppShell
    // (not the wizard) — reuse whatever invoke/probe stub the file already sets up.
    render(<App />);
    expect(await screen.findByTestId('shell-panes')).toBeInTheDocument();
    vi.unstubAllGlobals();
  });
});
```

> Read `App.test.tsx` first to reuse its existing `invoke`/wizard-completed mocking so `<App/>` resolves to `<AppShell/>`.

- [ ] **Step 2: Run, verify pass/fail.** `pnpm exec vitest run src/App.test.tsx`. Fix wiring until PASS. `pkill -9 -f vitest`.

- [ ] **Step 3: Commit**

```bash
git add src/App.test.tsx
git commit -m "test(app): App-level compact mount test (production path) (tuxlink-h7q7)"
```

---

## Phase 2 — Shell chrome + ribbon + sidebar icon rail (depends on Phase 1; coordinate §5-B)

### Task 8: Wrap the sidebar label in `.nav-label` (rail-hide enablement)

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx` (L184 — the bare label text node inside `MAILBOX_ITEMS.map`)
- Modify: `src/mailbox/FolderSidebar.test.tsx`

**Coordination (§5-B):** this edit is inside the same `.map` body shoal-raven-gorge edits (they add Contacts to `MAILBOX_ITEMS` at L29-35). The `.nav-label` wrap is at L184 (render), a different line. Agree which PR lands it; their Contacts item flows into the rail automatically via the generic map.

- [ ] **Step 1: Write the failing test**

```tsx
// FolderSidebar.test.tsx — the label must be in a .nav-label element so the
// compact rail CSS can hide it without hiding the icon.
it('wraps each folder label in a .nav-label element (rail hide enablement)', () => {
  // render the sidebar (reuse the file's existing render helper)
  const inbox = screen.getByTestId('folder-inbox');
  expect(inbox.querySelector('.nav-label')?.textContent).toBe('Inbox');
  expect(inbox.querySelector('.icon')).toBeInTheDocument(); // icon still separate
});
```

- [ ] **Step 2: Run, verify it fails.** `pnpm exec vitest run src/mailbox/FolderSidebar.test.tsx` → FAIL. `pkill -9 -f vitest`.

- [ ] **Step 3: Implement.** In `FolderSidebar.tsx`, change the mailbox-item render (L181-190 region) so the label is wrapped:

```tsx
<span className="icon" aria-hidden="true">{item.icon}</span>
<span className="nav-label">{item.label}</span>
{typeof count === 'number' && count > 0 && (
  <span className="count" data-testid={`folder-count-${item.id}`}>{count}</span>
)}
```

Apply the same `.nav-label` wrap to the user-folder rows (L256, `{uf.displayName}`) and the Connections accordion labels (L286, `{s.label}`; L312, `{p.label}`) so the rail can hide all sidebar labels uniformly.

- [ ] **Step 4: Run, verify it passes.** `pnpm exec vitest run src/mailbox/FolderSidebar.test.tsx` → PASS. `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/FolderSidebar.tsx src/mailbox/FolderSidebar.test.tsx
git commit -m "refactor(mailbox): wrap sidebar labels in .nav-label for rail hide (tuxlink-h7q7)"
```

### Task 9: Class-ify the sidebar's inline-styled controls (R3 — before the rail CSS pass)

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx` (inline `+` button L211-225; empty-hint L261-271; create-btn fontSize)
- Modify: `src/mailbox/userFolders.css` (new classes)
- Modify: `src/mailbox/FolderSidebar.test.tsx`

- [ ] **Step 1: Write the failing test** — the `+` button and empty-hint must carry classes (not inline styles) so the media query can reach them.

```tsx
it('renders the new-folder + button and empty-hint with classes (media-query reachable)', () => {
  // render with onCreateFolder + zero userFolders
  expect(screen.getByTestId('folder-create-btn').className).toContain('folder-create-btn');
  expect(screen.getByTestId('folders-empty-hint').className).toContain('folders-empty-hint');
});
```

- [ ] **Step 2: Run, verify it fails.** `pkill -9 -f vitest` after.

- [ ] **Step 3: Implement.** Replace the inline `style={{…}}` objects on the `+` button (L211-225), the empty-hint (L263-270), and the create-btn with `className="folder-create-btn"` / `className="folders-empty-hint"`, and move the equivalent declarations into `userFolders.css` (desktop values identical to the current inline values — this is a no-visual-change refactor at desktop). Then add the compact bumps in `userFolders.css`:

```css
/* desktop: preserve the prior inline values exactly */
.folder-create-btn { background: transparent; border: 1px solid var(--border-strong, #2c3744); border-radius: 3px; color: inherit; font-size: 13px; width: 18px; height: 18px; display: inline-flex; align-items: center; justify-content: center; cursor: pointer; padding: 0; line-height: 1; }
.folders-empty-hint { padding: 4px 10px; font-size: 11px; font-style: italic; color: var(--text-faint, #5d6975); }

@media (max-width: 1365px) {
  .folder-create-btn { width: 44px; height: 44px; font-size: 18px; }
  .folders-empty-hint { font-size: 12px; }
}
```

- [ ] **Step 4: Run, verify it passes.** `pkill -9 -f vitest` after.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/FolderSidebar.tsx src/mailbox/userFolders.css src/mailbox/FolderSidebar.test.tsx
git commit -m "refactor(mailbox): class-ify inline sidebar controls for compact reach (tuxlink-h7q7)"
```

### Task 10: Icon-rail CSS (resting 48px rail + tap-to-expand overlay)

**Files:**
- Modify: `src/shell/compactShell.css`
- Modify: `src/shell/AppShell.tsx` (rail-expand state + toggle affordance)
- Modify: `src/shell/AppShell.compact.test.tsx` (CSS assertions) + a behavior test for expand

**Decision:** resting rail = 48px (icons only, labels `display:none`); a top-of-rail expand button toggles `.sidebar.is-expanded`, which overlays the full 200px labeled sidebar over the message list (`position:absolute`, no grid reflow — design open item #4 resolved to overlay). Tap-away/select dismisses.

- [ ] **Step 1: Write the failing tests** — CSS: rail hides `.nav-label`/`.section-label`, enlarges `.icon` to ≥16px, nav-item ≥44px min-height, centered; expanded overlays. Behavior: clicking the expand button toggles `.is-expanded`; selecting a folder collapses it.

```tsx
// CSS-string assertions in AppShell.compact.test.tsx
it('collapses the sidebar to an icon rail in compact', () => {
  const block = css.slice(css.indexOf(COMPACT));
  expect(block).toContain('.layout-b .sidebar .nav-label'); // hidden
  expect(block).toMatch(/\.sidebar \.nav-item\s*\{[^}]*min-height:\s*44px/);
});
it('expanded rail overlays (absolute), does not reflow the grid', () => {
  const block = css.slice(css.indexOf(COMPACT));
  expect(block).toMatch(/\.sidebar\.is-expanded\s*\{[^}]*position:\s*absolute/);
});
```

- [ ] **Step 2: Run, verify fail.** `pkill -9 -f vitest` after.

- [ ] **Step 3: Implement the CSS** in the compact block:

```css
  /* Icon rail — layout is @media-driven (NOT gated on the .compact JS class, so
   * CSS and JS can't disagree about the mode — Codex R1 #2). The 48px column
   * width comes from the panes grid (Task 6); these rules hide labels, center
   * icons, and pin ≥44px rows. The EXPANDED state is the only JS bit
   * (.is-expanded), and its effect is gated inside this @media. */
  .layout-b .sidebar { padding: 8px 0; overflow: visible; }
  .layout-b .sidebar .section-label,
  .layout-b .sidebar .nav-label,
  .layout-b .sidebar .count,
  .layout-b .sidebar .v01-badge { display: none; }
  .layout-b .sidebar .nav-item {
    justify-content: center;
    min-height: 44px;
    padding: 7px 0;
    gap: 0;
  }
  .layout-b .sidebar .nav-item .icon { width: auto; font-size: 18px; }
  /* Expanded overlay — floats the full labeled sidebar over the list (overlay,
   * not push: the list is HTML so no webview-occlusion concern; open item 4). */
  .layout-b .sidebar.is-expanded {
    position: absolute;
    top: 0; left: 0; bottom: 0;
    width: 220px;
    z-index: 6;
    background: var(--surface);
    box-shadow: 4px 0 18px rgba(0, 0, 0, 0.35);
    overflow: auto;
    padding: 12px 0;
  }
  .layout-b .sidebar.is-expanded .section-label,
  .layout-b .sidebar.is-expanded .nav-label,
  .layout-b .sidebar.is-expanded .count { display: revert; }
  .layout-b .sidebar.is-expanded .nav-item { justify-content: flex-start; padding: 7px 18px; gap: 10px; }
  .layout-b .sidebar.is-expanded .nav-item .icon { width: 14px; font-size: 11px; }
  /* The expand toggle (a rail header button) — shown only in compact. */
  .layout-b .rail-expand-btn { display: flex; }
}
.rail-expand-btn { display: none; }
@media (max-width: 1365px) {
```

> The trailing `}` + re-opened `@media` above keep the desktop `.rail-expand-btn { display: none }` rule **outside** the media block (so it hides the button at desktop) while the rest stays inside. In the real file, place `.rail-expand-btn { display: none; }` at file scope (outside the `@media`) and the rest inside — do not literally close/reopen mid-block; this snippet shows the scoping intent.

- [ ] **Step 4: Implement the expand state + affordance + dismissal** in `FolderSidebar` (preferred — keeps sidebar concerns local) or AppShell. Add a `railExpanded` state, a `.rail-expand-btn` at the top of `<nav className="sidebar">` (`aria-expanded`, `aria-label="Expand folders"`), apply `is-expanded` to the nav className. **Dismissal (Codex R1 #11 — the expanded overlay must never get stuck over the list):** collapse on (a) `onSelectFolder`/`onSelectConnection`, (b) an **outside pointer-down** (a `useEffect` document `pointerdown` listener that collapses when the target is outside the nav), and (c) **Escape** keydown. Keep focus visible on the expand button after toggle. Add a behavior test:

```tsx
it('toggles rail expansion and collapses on folder select, outside-click, and Escape (compact)', () => {
  // click rail-expand-btn → nav has is-expanded;
  // click a folder → is-expanded removed;
  // re-expand → pointerdown on document.body (outside nav) → is-expanded removed;
  // re-expand → keydown Escape → is-expanded removed.
});
```

> If `FolderSidebar` needs to know it's compact, pass an `isCompact` prop from AppShell (it already has `useViewport`) rather than calling the hook twice — single source of truth.

- [ ] **Step 5: Run all sidebar/compact tests → PASS.** `pkill -9 -f vitest`. **Commit:**

```bash
git add src/shell/compactShell.css src/shell/AppShell.tsx src/mailbox/FolderSidebar.tsx src/shell/AppShell.compact.test.tsx src/mailbox/FolderSidebar.test.tsx
git commit -m "feat(shell): icon rail with tap-to-expand overlay in compact (tuxlink-h7q7)"
```

### Task 11: Ribbon clip fix + chrome/menubar/titlebar/statusbar touch + font floors

**Files:**
- Modify: `src/shell/compactShell.css` (+ a `@media` block in `src/shell/chrome/chrome.css` and `src/shell/StatusBar.css` if those selectors aren't reachable from compactShell — they are global classes, so compactShell can target them; keep them in compactShell for one reviewable unit, EXCEPT `.tux-ctrl` per R4)
- Modify: `src/shell/AppShell.compact.test.tsx`

- [ ] **Step 1: Write CSS-string assertions** for: search-zone `flex-basis` reduced, `.dash-connection` max-width reduced, dashboard gap reduced; touch min-heights on `.connect-button`/`.abort-button`/`.dash-ssid-select`/`.nav-item`/menubar buttons/titlebar controls/sort trigger; font floors on `.dash-label`/`.dash-source-segment`/`.section-label`/statusbar. (One `it()` per group; assert the compact block `toContain` each rule.)

```tsx
it('fixes the ribbon clip risk in compact', () => {
  const block = css.slice(css.indexOf(COMPACT));
  expect(block).toContain('.search-zone'); // flex-basis reduced
  expect(block).toMatch(/\.dash-source-segment\s*\{[^}]*font-size:\s*12px/); // 9px → 12px floor
});
it('bumps titlebar controls to a 44px touch target in compact', () => {
  const block = css.slice(css.indexOf(COMPACT));
  expect(block).toMatch(/\.tux-ctrl\s*\{[^}]*(min-width|width):\s*44px/);
});
```

- [ ] **Step 2: Run, verify fail.** `pkill -9 -f vitest`.

- [ ] **Step 3: Implement** the compact rules (exact selectors + target values per checklist + synthesis §2.1). Ribbon: `.search-zone { flex: 0 0 360px; }`, `.dashboard .dash-connection { max-width: 180px; }`, `.dashboard { gap: 16px; }`. Touch: each control gets `min-height: 44px` (and `min-width: 44px` for square controls like titlebar `.tux-ctrl` and the sort trigger). Font floors: each enumerated sub-floor selector → `font-size: 12px` (chrome) / `13px` (where the checklist says 13). Keep the bare `.tux-ctrl` bump here (shell owns it per R4); compose scopes its own.

- [ ] **Step 4: Run all compact tests → PASS.** `pkill -9 -f vitest`. **Commit:**

```bash
git add src/shell/compactShell.css src/shell/AppShell.compact.test.tsx
git commit -m "feat(shell): compact ribbon clip fix + chrome/titlebar/statusbar touch & font floors (tuxlink-h7q7)"
```

---

## Phase 3 — Compose window (independent; Rust + CSS)

### Task 12: Rust — clamp the Compose window default height to the monitor work area (R2)

**Files:**
- Modify: `src-tauri/src/.../compose_window.rs` (the `.inner_size(1100.0, 820.0)` call, ~L158 per audit)
- Test: a Rust unit test for the clamp function (same module or a `#[cfg(test)]` block)

- [ ] **Step 1: Locate the file + the builder.** Run: `grep -rn "inner_size\|compose" src-tauri/src --include=*.rs | grep -i compose` to confirm the exact path/line (audit said `compose_window.rs:158`).

- [ ] **Step 2: Write the failing Rust test** — a pure clamp function `clamped_compose_height(default_h: f64, work_area_h: f64, margin: f64) -> f64` returning `default_h.min(work_area_h - margin).max(MIN)`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clamps_to_work_area_on_short_screens() {
        // FZ-M1: ~760px usable; default 820 must clamp below the work area.
        let h = clamped_compose_height(820.0, 760.0, 24.0);
        assert!(h <= 760.0 - 24.0 + 0.01, "got {h}");
        assert!(h >= 560.0, "must not go below min_inner height, got {h}");
    }
    #[test]
    fn leaves_tall_screens_untouched() {
        let h = clamped_compose_height(820.0, 1080.0, 24.0);
        assert_eq!(h, 820.0);
    }
}
```

- [ ] **Step 3: Run, verify it fails.** `cargo test --manifest-path src-tauri/Cargo.toml clamped_compose_height` → FAIL (no fn).

- [ ] **Step 4: Implement** the clamp fn + use it (Codex R1 #12 — do NOT rely solely on a pre-creation primary-monitor read). Two-stage:
  1. **Pre-creation best-effort:** if a monitor handle is resolvable before `build()` (e.g. the parent window's `current_monitor()`), clamp `.inner_size(1100.0, clamped_compose_height(820.0, work_h, 24.0))`. If no work-area API is available pre-creation, leave the default and rely on stage 2.
  2. **Post-`build()` clamp:** after the window exists, read its actual monitor's work area and, if the (possibly window-state-restored) inner height exceeds `work_h - margin`, call `set_size` to clamp it. This catches `tauri-plugin-window-state` restoring an oversized geometry that overrides the default. Guard against a clamp-loop (only shrink, never grow; compare with a small epsilon).
  Note the caller's monitor may not be the primary monitor — prefer `current_monitor()` over `primary_monitor()`.

- [ ] **Step 5: Run → PASS. Commit.**

```bash
git add src-tauri/src
git commit -m "fix(compose): clamp window default height to monitor work area for FZ-M1 (tuxlink-h7q7)"
```

### Task 13: In-window Compose + embedded-form compact CSS

**Files:**
- Modify: `src/compose/Compose.css`, `src/compose/CheckInForm.css`, `src/compose/Ics309FormV2.css`, `src/compose/PositionFormV2.css`
- Test: `src/compose/Compose.test.tsx` (CSS-string assertions if a raw-CSS import exists; else a behavior test that the action bar is reachable)

- [ ] **Step 1–4: TDD per the checklist** — add `@media (max-width: 1365px)` blocks: action btns/inputs ≥44px; receipt checkbox + 44px label row; attachments drop-zone 36→48px; `.tux-compose-titlebar .tux-ctrl { min-width: 44px; min-height: 44px; }` (R4 — scoped to compose, NOT bare `.tux-ctrl`); embedded inputs/buttons ≥44px; CheckIn radios + ICS-309 `datetime-local` sized; font floors `.compose-hint`/`fix-badge`/`grid-error` → 12px; tighten embedded-form padding 16→10, gap 12→8. **Do NOT add a root font-size shrink (ICS-309 is rem-based — it would dip below floor).** Assert via CSS-string slicing; commit per file or as one Compose-CSS commit.

- [ ] **Step 5: Commit**

```bash
git add src/compose
git commit -m "feat(compose): in-window compact CSS — touch, font floors, density (tuxlink-h7q7)"
```

---

## Phase 4 — Dialogs (independent)

### Task 14: Settings + Theme + About compact CSS (DRY the close button)

**Files:**
- Modify: `src/shell/SettingsPanel.css`, `src/shell/ThemeDesigner.css`, `src/shell/AboutDialog.css`
- Test: `src/shell/SettingsPanel.test.tsx` / `ThemeDesigner.test.tsx` / `AboutDialog.test.tsx` (CSS-string assertions or computed-class checks)

- [ ] **Step 1–4: TDD per checklist.** Compact blocks: ThemeDesigner **color swatch 36×28 → 44×44 (the primary, ×24 instances)**, hex/name/select inputs ≥44px, tighten group padding 14→12 to offset; Settings `.tux-settings-opt` min-height 44 + native radio bump; About **5 inline meta links → `display:inline-block; padding; min-height:44px` + row-gap 6→10**; DRY the three close buttons into one compact rule reused by all three (`.tux-settings-close, .tux-theme-designer-close, .tux-about-close { min-width: 44px; min-height: 44px; }` inside the media query — place it in whichever of the three CSS files is the natural shared home, or App.css base-dialog if one exists; keep it ONE rule). Font floors per checklist.

- [ ] **Step 5: Commit**

```bash
git add src/shell/SettingsPanel.css src/shell/ThemeDesigner.css src/shell/AboutDialog.css src/shell/*.test.tsx
git commit -m "feat(shell): compact dialogs — Theme swatch/Settings/About touch & font floors (tuxlink-h7q7)"
```

---

## Phase 5 — Wizard (independent; pure CSS, no JS hook)

### Task 15: `wizard.css` compact block

**Files:**
- Modify: `src/wizard/wizard.css`
- Test: a CSS-string assertion test (new `src/wizard/wizard.compact.test.tsx` mirroring the AppShell CSS-string pattern, or extend an existing wizard test)

- [ ] **Step 1–4: TDD.** `@media (max-width: 1365px)` in `wizard.css`: keep 580px centered card; `.wizard-root` top-pad → `clamp(16px, 3vh, 32px)`; card padding 38/40 → 24/28; `.wizard-session-log { max-height: 30vh; }`; submit-row buttons/inputs/password-toggle/link-button/Retry ≥44px; bump 12/12.5px offenders → 13px; **mono session-log + `wizard-failed-detail` → 13px; lighten the faint `.wizard-footer-copy` color**; `code` 0.88em → 0.92em. **Flag (in the PR body + a code comment) the inline Register anchor — it cannot cleanly reach 44px inline; a design call (restyle as button vs accept line-box) is deferred.**

- [ ] **Step 5: Commit**

```bash
git add src/wizard/wizard.css src/wizard/wizard.compact.test.tsx
git commit -m "feat(wizard): compact CSS — touch, font floors, vertical density (tuxlink-h7q7)"
```

---

## Phase 6 — HTML forms + embedded-webview reposition guard (depends on Phase 1 drawer; R1)

### Task 16: Forms compact CSS (4 files)

**Files:**
- Modify: `src/forms/forms.css`, `src/forms/FormPicker.css`, `src/compose/WebviewFormHost.css`, `src/mailbox/WebviewFormViewer.css`
- Test: CSS-string assertions per file

- [ ] **Step 1–4: TDD per checklist.** `@media (max-width: 1365px)`: `.ics309-log-entry` 3-col → single column; `.damage-category` 6-col → 2-col; `.damage-category > legend > input` 200px → 100%; picker list rows ≥44px; all action buttons/native inputs ≥44px; native checkbox → 22px; toolbar select/buttons ≥44px; font floors field `label`/`log-entry>strong`/table `th` → 12px.

- [ ] **Step 5: Commit**

```bash
git add src/forms src/compose/WebviewFormHost.css src/mailbox/WebviewFormViewer.css
git commit -m "feat(forms): compact CSS — reflow, touch, font floors (tuxlink-h7q7)"
```

### Task 17: Verify the embedded form-viewer webview repositions under the push drawer (R1)

**Files:**
- Test: `src/mailbox/WebviewFormViewer.test.tsx` (assert the existing `ResizeObserver` callback fires on placeholder resize)
- Possibly modify: `src/mailbox/WebviewFormViewer.tsx` (only if the verification reveals a gap)

**Why this is now verification, not a rebuild (Codex R1 #5/#6):** the drawer PUSHES (it is a real grid column — Task 6), so opening it narrows the reader → the `.webview-form-viewer__embed` placeholder resizes → the component's existing `ResizeObserver` (`WebviewFormViewer.tsx:137-147`) fires → `setPosition`/`setSize` reposition the child webview to the narrower reader. The webview is never overlapped because the drawer occupies its own grid track, not an absolute layer over the reader. **No manual signal threading through `MessageView` is needed** — which also removes the coordination hazard with shoal-raven-gorge's content-switch region (Codex R1 #6/#10).

- [ ] **Step 1: Write a test** that proves the reposition wiring fires when the placeholder's observed box changes.

```tsx
// src/mailbox/WebviewFormViewer.test.tsx — mock @tauri-apps Webview + the
// ResizeObserver; render the viewer; drive a resize of the embed element;
// assert setPosition + setSize were called with the new rect. This proves the
// reflow path that the push drawer triggers actually repositions the webview.
it('repositions the child webview when the embed placeholder resizes (push reflow, R1)', () => {
  // install a controllable ResizeObserver mock that captures the callback;
  // mock Webview with setPosition/setSize spies; render; invoke the captured
  // callback after stubbing mountRef.getBoundingClientRect to a new rect;
  // assert setPosition + setSize called.
});
```

- [ ] **Step 2: Run.** If it PASSES, the natural reflow path is sound — no `.tsx` change needed. If the test reveals the observer does not fire on a width-only change of the embed (jsdom ResizeObserver is mocked, so this is really verified in the Playwright pass), add an explicit re-measure: a tiny `repositionSignal?: number` prop bumped from AppShell `drawerOpen`, `useEffect(()=>remeasure(),[repositionSignal])`. Prefer NOT to add it unless needed.

- [ ] **Step 3: Real-viewport confirmation (the actual R1 gate).** In the Final-verification Playwright pass: open a message that renders the form viewer, open the radio drawer, and assert the form content is fully visible (not occluded, not overlapping the drawer) at 1280×800. This is the authoritative R1 check — jsdom cannot prove webview stacking.

- [ ] **Step 4: Commit**

```bash
git add src/mailbox/WebviewFormViewer.test.tsx src/mailbox/WebviewFormViewer.tsx
git commit -m "test(forms): verify form-viewer webview repositions under push drawer (R1, tuxlink-h7q7)"
```

---

## Final verification (before PR)

- [ ] **Full targeted test run** (narrow, not a sweep): `pnpm exec vitest run src/shell src/mailbox/FolderSidebar.test.tsx src/mailbox/WebviewFormViewer.test.tsx src/compose src/wizard src/forms src/App.test.tsx` → all PASS. `pkill -9 -f vitest`.
- [ ] **Typecheck:** `pnpm typecheck` → clean.
- [ ] **Rust:** `cargo test --manifest-path src-tauri/Cargo.toml` (compose clamp) → PASS.
- [ ] **Codex cross-provider adversarial review** (required by `build-robust-features` — see `feedback_no_carveout_on_cross_provider_adrev`; this is design-bearing UX, not plumbing): run rounds against the diff, focusing Codex on: (a) any desktop-layout regression (rules leaking outside the media query / `.compact`), (b) the overlay-vs-push reader-occlusion trade-off, (c) the R1 webview reposition correctness, (d) the rail 36-vs-48px tension, (e) the grip session-state coupling. Converge the PROPOSED open-item resolutions here. Write transcripts to `dev/adversarial/` (gitignored); summarize dispositions in the PR body.
- [ ] **MANDATORY Playwright pass at three widths — 1280×800, 1366×768, 1440×900** (Codex R1 #8 — CSS-string tests are a cheap first guard but cannot prove computed layout, stacking, or hit areas). Assert, with computed values + screenshots:
  - **1440 (desktop):** panes grid computed columns == `200px 380px 1fr [400px]` — byte-identical to a pre-change baseline screenshot. The regression guard's real teeth.
  - **1366 (boundary):** still desktop (no compact rules applied) — proves the `1365px` strict boundary (Codex R1 #1).
  - **1280 (FZ-M1):** rail collapsed to 48px; reader usable closed (~808px) and open (~452px); grip ≥44px and reachable; drawer pushes (reader narrows, not overlapped); titlebar/ribbon controls ≥44px (computed `getBoundingClientRect`).
  - **R1 webview:** open a form-viewer message, open the drawer, screenshot → form content fully visible, not occluded.
- [ ] **Open a READY PR** (`gh pr create --base main`), NOT draft. **Do not self-merge** — the operator browser-smokes the responsive layout at real window sizes before merge.

## Coordination summary (shoal-raven-gorge, `bd-tuxlink-raez/contacts-favorites`)

| Risk | File | Their hunk | Our hunk | Action |
|---|---|---|---|---|
| HIGH (A) | `AppShell.tsx` | content-switch L869-929, `selectedFolder` L214 | panes className L841, drawer state ~L242, wrap L936-1003 | Different hunks; rebase-merge expected. Sequence Phase 1 explicitly; whoever merges second rebases. |
| MED (B) | `FolderSidebar.tsx` | `MAILBOX_ITEMS` L29-35 | `.nav-label` wrap L184 (Task 8), class-ify inline L211-271 (Task 9) | Same `.map` body; agree which PR lands `.nav-label`. Their Contacts item auto-flows into the rail. `selectedFolder` gains `'contacts'` — rail active-state round-trips it. |
| LOW (C) | `RadioPanel.tsx` | Favorites/Recent/Manual tabs in `radio-panel-body` L58 | none (drawer wraps at AppShell, not RadioPanel) | Their new tab strip needs the same compact rule as `.radio-panel-segmented` — reuse the class or add an equivalent compact entry. |
| INT (D) | reader × drawer | reading-pane | form-viewer reposition (Task 17) | Agree the drawer-toggle reposition trigger. |

Maintain bd dep edges as state evolves (`bd dep add`).
