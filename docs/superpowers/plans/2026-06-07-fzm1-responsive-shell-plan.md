# FZ-M1 Responsive Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a touch-friendly "compact" mode to the Tuxlink shell (and every primary surface) so the app is usable on a Panasonic FZ-M1 — a 1280×800, 7", ~216 PPI capacitive-touch rugged tablet — without changing the desktop (≥1366px) layout at all.

**Architecture:** Greenfield responsive work — the entire `src/` tree has exactly one non-print `@media` rule today. Compact rules are **additive and scoped** behind a single `@media (max-width: 1366px)` breakpoint (for static layout/typography/density) plus a `.layout-b.compact` root class toggled by a `useViewport` hook keyed off the **same** media-query string (for JS-stateful interactions: the radio slide-over drawer's open/close and the icon-rail expand overlay). Desktop above 1366px is byte-identical to today and that invariant is enforced by a regression-guard test that lands *first*. The headline fix replaces the permanent 400px radio dock column (which starves the reader to ~300px at 1280px) with a `position:absolute` slide-over drawer.

**Tech Stack:** React 18 + TypeScript, Vite, Vitest (jsdom) + React Testing Library, Tauri (Rust) for the separate Compose window, plain CSS (no preprocessor). jsdom cannot compute layout or evaluate media queries, so layout/typography assertions are **CSS-string assertions** (import the stylesheet as a string, slice the media block, assert on it — the established pattern in `AppShell.test.tsx`); interactive state is tested with RTL; real-viewport visual proof is an operator browser-smoke at 1280×800 plus an optional Playwright pass.

---

## Provenance & inputs

- **Locked design:** [`docs/design/2026-06-07-fzm1-responsive-design.md`](../../design/2026-06-07-fzm1-responsive-design.md) (operator brainstorm 2026-06-07, agent `basalt-mesa-dahlia`). Decision: **Option A — icon rail + radio drawer.**
- **Audit synthesis (local reference, gitignored):** `dev/scratch/2026-06-07-fzm1-compact-audit-synthesis.md` — per-surface FZ-M1 compact-readiness audit (7 surfaces, workflow `wf_20f7ea5a-9ae`). Raw per-surface JSON: `dev/scratch/2026-06-07-fzm1-compact-audit-raw.json`.
- **bd issue:** `tuxlink-h7q7` (P2). Companion help-window responsive is the separate `tuxlink-0gsy` (out of scope here).

### Resolved design open items (PROPOSED — to be converged by the Codex cross-provider adversarial review in `build-robust-features`, not decided unilaterally with the operator)

| # | Open item | PROPOSED resolution | Rationale |
|---|---|---|---|
| 1 | Exact breakpoint value | **Single `@media (max-width: 1366px)`**; no second tier | 1366 catches the FZ-M1's 1280 with an 86px margin and aligns with the 1366×768 laptop class; no surface needs a second tier; phone-width (<768px) is explicitly out of scope. Keeps the desktop regression guard a single media boundary. |
| 2 | Drawer transition + closed-grip session state | **`transform: translateX(100%)→0`, `transition: transform 220ms ease`**, GPU-composited; `prefers-reduced-motion` disables it; **YES — the closed grip shows a subtle session-state tick** (reflects `RadioPanelState`) | `transform` avoids layout thrash on the `.panes` grid; the grip tick serves the design's headline "eyes on the inbox while a session runs" case — with the drawer tucked away the operator otherwise loses all session feedback. Grip is itself a ≥44px hit target. |
| 3 | Per-surface compact checklist | The table in **§"Per-surface compact checklist"** below | Derived directly from the 7-surface audit. |
| 4 | Icon rail tap-to-expand: overlay vs push | **Overlay** (expanded labeled rail floats over the message list; grid does not reflow) | On a 1280px width budget, a *push* re-adds 200px on every expand AND moves the embedded form-viewer webview's placeholder rect → stale-position hazard (see Integration Risk R1). Overlay never reflows the grid, so the webview never moves. The resting 36→48px rail IS in the grid track (reader keeps its reclaimed width permanently); only the *expanded* state overlays. |

### Additional PROPOSED resolution (design-internal tension found during grounding)

**Rail resting width: 48px, not the design's "36px".** The design says "36px icon rail" *and* "touch targets ≥44×44px" — a 36px-wide rail cannot host 44px-*wide* tap targets. **PROPOSED: 48px resting rail** (still reclaims 152px of the original 200px sidebar; the extra 12px vs 36px is negligible against the reader-width win) so rail icons get a full 44×44 hit area. Flag for Codex convergence. If Codex prefers 36px, the fallback is a 44px-tall × 36px-wide hit area with generous vertical spacing (partial compliance on the horizontal axis).

---

## File structure (created / modified)

**New files:**
- `src/shell/useViewport.ts` — the compact-mode hook (`matchMedia`-driven) + the exported `COMPACT_MEDIA_QUERY` constant. Single responsibility: tell React whether we're in compact mode.
- `src/shell/useViewport.test.tsx` — hook tests.
- `src/shell/RadioDrawer.tsx` — the slide-over wrapper around the radio-panel mount block (grip handle + session-state tick + open/close). Single responsibility: drawer chrome + state; it does **not** know about radio internals (it wraps whatever children it's given).
- `src/shell/RadioDrawer.css` — drawer-specific compact CSS.
- `src/shell/RadioDrawer.test.tsx` — drawer behavior tests.
- `src/shell/compactShell.css` — the shell's `@media (max-width: 1366px)` block (panes grid rewrite, rail, ribbon clip fix, chrome/menubar/titlebar/statusbar touch+font floors). Kept separate from `AppShell.css` so the compact rules are reviewable as one unit and the desktop file is untouched except for one `@import`. (If the codebase convention is one CSS file per component and reviewers prefer it inline, fold into `AppShell.css` — note for Codex.)

**Modified files:**
- `src/shell/AppShell.tsx` — panes className (L841), `drawerOpen`/`railExpanded` state (near L242), wrap the radio-panel mount block (L936-1003) in `<RadioDrawer>`, add the `compact` class to the `.layout-b` root, import `compactShell.css`. **Coordination: different hunks from shoal-raven-gorge's content-switch (L869-929) + `selectedFolder` (L214).**
- `src/mailbox/FolderSidebar.tsx` — wrap the bare label text node (L184) in `<span className="nav-label">`; refactor the inline-styled `+` button (L211-225), empty-hint (L261-271), and create-btn so a media query can reach them. **Coordination: different hunk from shoal-raven-gorge's `MAILBOX_ITEMS` (L29-35), but the `.nav-label` wrap is inside the same `.map` body — agree which PR lands it.**
- `src/shell/AppShell.css` — add `@import './compactShell.css';` at top; **no other change** (desktop rules stay byte-identical).
- `src/mailbox/MessageView.css` — leave `.reading-pane { min-width: 0 }` as-is (it's correct; the fix is removing the dock column, not fighting min-width). No edit expected — listed so the executor knows *not* to touch it.
- `src/compose/Compose.css` — in-window `@media (max-width: 1366px)` block (the compose window is a separate document; its width ≤1100 matches naturally).
- `src/compose/CheckInForm.css`, `src/compose/Ics309FormV2.css`, `src/compose/PositionFormV2.css` — embedded-form compact blocks.
- `src-tauri/src/.../compose_window.rs` — clamp the default inner height to the monitor work area (the **Rust** fix; CSS cannot reach window geometry). Exact path resolved in Task 12.
- `src/shell/SettingsPanel.css`, `src/shell/ThemeDesigner.css`, `src/shell/AboutDialog.css` — dialog compact blocks (DRY the close-button rule).
- `src/wizard/wizard.css` — wizard compact block (pure CSS, no JS hook).
- `src/forms/forms.css`, `src/forms/FormPicker.css`, `src/compose/WebviewFormHost.css`, `src/mailbox/WebviewFormViewer.css` — forms compact blocks.
- `src/mailbox/WebviewFormViewer.tsx` — wire an explicit reposition trigger to drawer/reader-width changes (Integration Risk R1).
- `src/App.test.tsx` — extend with an App-level mount assertion for the compact wiring.

### Shared-CSS scoping discipline (Integration Risks — read before editing)

- **R1 — embedded-webview stale position.** `WebviewFormViewer` (mailbox reader) and `WebviewFormHost` (compose) are child Tauri webviews pixel-positioned over a placeholder div, repositioned only on `ResizeObserver(embed + document.body)`. An **overlay** drawer that moves the reader without resizing the observed box leaves the webview stranded. **Guard:** Task 17 wires an explicit re-measure to `drawerOpen` + compact-mode changes. This is a required integration test, not an assumption.
- **R2 — Compose is a separate window.** Its default height (820) > FZ-M1 usable (~760) clips the action bar. **Rust** fix (Task 12), separate from the in-window CSS (Task 13).
- **R3 — inline styles can't be reached by `@media`.** The sidebar `+` button / empty-hint / create-btn must be classed (Task 9) *before* the rail CSS pass.
- **R4 — `chrome.css .tux-ctrl` is shared** between the shell titlebar and the compose titlebar. Scope the compose bump to `.tux-compose-titlebar .tux-ctrl`; the shell owns the bare `.tux-ctrl`. No double-application.
- **R5 — `App.css` base input/button/radio sizing is global.** Do **not** add a global `.compact input{…}` rule in `App.css`; keep touch bumps per-surface so rules aren't double-applied. (Native radio/checkbox `width:auto` in `App.css:452-455` is the shared source of unpinned sizes — bump per-surface.)

---

## Per-surface compact checklist (design open item #3, resolved)

Each row is a compact-scoped change set; full selector lists are in `dev/scratch/2026-06-07-fzm1-compact-audit-synthesis.md` §2. Phases below implement these.

| Surface | Trigger | Layout | Touch (≥44px) | Font floor (≥12px) | Density |
|---|---|---|---|---|---|
| **Shell** | `@media ≤1366px` + `.layout-b.compact` | 3 panes templates → `48px 380px 1fr`; drop dock 4th col; null legacy 5th col; reuse `.panes` `position:relative` as drawer anchor | Connect/Abort, SSID select, grid-edit, GPS/MANUAL segments, set-manually, nav-item, MenuBar buttons, dropdown items, titlebar ctrls, sort trigger, `.row` min-height | `.dash-label`, `.dash-source-segment` (9px), GPS status/error, `.section-label`, nav count/icon, badges, status divider | search-zone 560→360, connection max-w 260→180, dashboard gap 28→16 |
| **Mailbox** | inherits shell | `.nav-label` span for rail hide; keep `.reading-pane min-width:0` | nav rows, reader action-btns, sort trigger, inline `+`→class, attachment Save/Preview, ctx-menu, folder-dialog btns | `.section-label`, nav icon→16, count, `.form-tag`, `.size`, `msg-meta dt`, inline empty-hint/create-btn→class | rail icon centering |
| **Radio drawer** | `.compact` (JS state) | extract aside from grid → `position:absolute` drawer; override `min-width:400→0`, `width: min(400px, 92vw)` | close, primary/danger btns, segmented tabs, btn-sm, chips, chip-`✕`, inputs/selects, native radio, Listen header | segmented/h5/help/pills 11→12, LIVE 10→11, Listen 9px→12 | `.session-log min-height 240→160` |
| **Compose (window)** | (a) Rust height clamp; (b) `@media ≤1366px` on compose doc | n/a (separate window; min-width safe) | action btns, inputs, receipt checkbox, attachments 36→48, `.tux-compose-titlebar .tux-ctrl`, embedded inputs/btns, CheckIn radios, ICS-309 datetime-local | `.compose-hint`, `fix-badge`/`grid-error` 11→12; **do NOT shrink 14px root (ICS-309 rem-based)** | embedded-form padding 16→10, gap 12→8 |
| **Settings dialog** | `@media ≤1366px` | modal width fine | `.tux-settings-opt` min-h 44 + native radio; close btn (DRY ×3) | opt-help 11→13, legend, error | — |
| **Theme dialog** | same | optional card→`min(720px,…)` | **swatch 36×28→44×44 (×24)**, hex/name/select inputs, action btns, close btn | token/group/field help 11→13, hex 12→13 | tighten group padding 14→12 |
| **About dialog** | same | meta grid fine; row-gap 6→10 | footer Close, **5 meta links→inline-block padding+44px**, close btn | `.tux-about-meta` 12→13, credit, prealpha | adjacent-link separation |
| **Wizard** | **pure CSS `@media ≤1366px` in wizard.css** | keep 580px card; lower `.wizard-root` top-pad to `clamp(16px,3vh,32px)`; card pad 38/40→24/28; session-log `max-height:30vh` | submit-row btns, inputs, password toggle, link-button, Retry; **inline Register anchor → flag to design** | bump 12/12.5px→13px; mono log + failed-detail + faint footer → 13px + lighten | vertical-budget check ~760px |
| **HTML forms** | `@media ≤1366px` ×4 files | `.ics309-log-entry`→1-col; `.damage-category` 6→2 col; legend input 200px→100% | picker rows, action btns, native inputs/checkbox, toolbar select/btns, +Add | field `label`, `log-entry>strong`, table `th` 11→12 | + R1 webview reposition guard |

---

## Testing strategy (how each layer is verified)

1. **CSS-string assertions (jsdom):** import the stylesheet string (the existing `APP_SHELL_CSS_MODULES['./AppShell.css']` pattern, `AppShell.test.tsx:19`), `slice` from the `@media (max-width: 1366px)` index, and assert the block `toContain` the expected rules. **Regression guard:** assert the desktop grid templates (`grid-template-columns: 200px 380px 1fr`) appear in the string but **before** the compact `@media` index (i.e. unscoped/untouched).
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
  it('exports the canonical compact media query string', () => {
    expect(COMPACT_MEDIA_QUERY).toBe('(max-width: 1366px)');
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
 * `@media (max-width: 1366px)` blocks (compactShell.css, RadioDrawer.css,
 * wizard.css, dialog/forms compact blocks) MUST mirror this exact string.
 * Because the hook evaluates the identical media query via matchMedia, the
 * CSS-driven layout and the JS-driven interactive state can never disagree
 * about whether we are in compact mode.
 *
 * tuxlink-h7q7 / docs/design/2026-06-07-fzm1-responsive-design.md.
 */
export const COMPACT_MEDIA_QUERY = '(max-width: 1366px)';

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
// Mirror AppShell.test.tsx's CSS-string import (it imports AppShell.css as a
// raw string via the test's CSS-modules shim). compactShell.css is @imported
// from AppShell.css, so once it exists its rules appear in the combined string.
import { APP_SHELL_CSS_MODULES } from './__testRawCss';

const css = APP_SHELL_CSS_MODULES['./AppShell.css'];
const COMPACT = '@media (max-width: 1366px)';

describe('AppShell desktop regression guard (tuxlink-h7q7)', () => {
  it('keeps the desktop panes grid templates unscoped (outside any media query)', () => {
    const compactIdx = css.indexOf(COMPACT);
    const desktopHead = compactIdx === -1 ? css : css.slice(0, compactIdx);
    // The three desktop templates must exist BEFORE the compact block — i.e.
    // they are NOT mutated, only overridden inside the media query.
    expect(desktopHead).toContain('grid-template-columns: 200px 380px 1fr');
    expect(desktopHead).toContain('grid-template-columns: 200px 380px 1fr 400px');
  });
});

describe('AppShell compact CSS contract (tuxlink-h7q7)', () => {
  it('defines a single compact breakpoint at 1366px', () => {
    expect(css).toContain(COMPACT);
    // exactly one non-print compact breakpoint
    const occurrences = css.split(COMPACT).length - 1;
    expect(occurrences).toBeGreaterThanOrEqual(1);
  });

  it('rewrites the panes grid inside the compact block: rail + no permanent dock column', () => {
    const block = css.slice(css.indexOf(COMPACT));
    expect(block).toContain('48px 380px 1fr'); // rail (48px) + list + reader
  });
});
```

> **Note on `./__testRawCss`:** `AppShell.test.tsx` already resolves the raw CSS via a test shim (it reads `APP_SHELL_CSS_MODULES['./AppShell.css']`). Reuse the exact same import mechanism it uses — if that symbol lives inline in `AppShell.test.tsx`, extract it to a tiny shared `src/shell/__testRawCss.ts` in this task so both test files share it. If extraction is undesirable, duplicate the 3-line raw-import in this file. Confirm the mechanism by reading `AppShell.test.tsx:1-25` first.

- [ ] **Step 2: Run, verify the compact-contract cases FAIL (guard cases pass)**

Run: `pnpm exec vitest run src/shell/AppShell.compact.test.tsx`
Expected: the "desktop regression guard" case PASSES (desktop templates already exist); the "compact CSS contract" cases FAIL (no compact block yet). Then `pkill -9 -f vitest`.

- [ ] **Step 3: Create the empty compact stylesheet + wire the import**

```css
/* src/shell/compactShell.css */
/* FZ-M1 compact mode — additive, scoped. Mirrors COMPACT_MEDIA_QUERY in
 * src/shell/useViewport.ts. Desktop (>1366px) is unaffected: every rule here
 * lives inside the media query or under `.layout-b.compact`. tuxlink-h7q7. */
@media (max-width: 1366px) {
  /* panes grid + rail + ribbon + chrome compact rules land in Phase 1-2 */
}
```

Add to the very top of `src/shell/AppShell.css` (above the first rule, after the leading comment banner):

```css
@import './compactShell.css';
```

- [ ] **Step 4: Run, verify the breakpoint-exists case passes; the `48px` case still fails**

Run: `pnpm exec vitest run src/shell/AppShell.compact.test.tsx`
Expected: "defines a single compact breakpoint" PASSES; "rewrites the panes grid" still FAILS (no `48px` rule yet — Phase 1 adds it). Then `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/shell/AppShell.compact.test.tsx src/shell/compactShell.css src/shell/AppShell.css src/shell/__testRawCss.ts
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
 * Wraps the radio-panel mount block. Desktop (>1366px): `display: contents`
 * (CSS), so the child panel IS the 4th grid column — byte-identical to the
 * pre-compact layout. Compact: a position:absolute slide-over with a grip
 * handle that shows session state and toggles open/closed. tuxlink-h7q7.
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
/* Desktop: the wrapper is transparent so the radio panel sits in the grid's
 * 4th column exactly as before. The grip is hidden. tuxlink-h7q7. */
.radio-drawer { display: contents; }
.radio-drawer-grip { display: none; }

@media (max-width: 1366px) {
  /* Compact: the drawer becomes a right-anchored slide-over over .panes
   * (which is position:relative; isolation:isolate already). */
  .layout-b.compact .radio-drawer {
    display: block;
    position: absolute;
    top: 0;
    right: 0;
    height: 100%;
    z-index: 5;
    transform: translateX(100%);
    transition: transform 220ms ease;
    pointer-events: none; /* closed: let the reader beneath receive taps */
  }
  .layout-b.compact .radio-drawer.is-open {
    transform: translateX(0);
    pointer-events: auto;
  }
  .layout-b.compact .radio-drawer-body {
    width: min(400px, 92vw);
    height: 100%;
    overflow: auto;
    background: var(--surface);
    border-left: 1px solid var(--border);
    box-shadow: -8px 0 24px rgba(0, 0, 0, 0.35);
  }
  /* Grip: a ≥44px-tall tab on the drawer's left edge, always tappable so the
   * operator can re-open a tucked-away session. */
  .layout-b.compact .radio-drawer-grip {
    display: flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    top: 50%;
    left: -24px;
    transform: translateY(-50%);
    width: 24px;
    min-height: 56px; /* ≥44px hit target */
    padding: 0;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-right: 0;
    border-radius: 6px 0 0 6px;
    cursor: pointer;
    pointer-events: auto;
  }
  .radio-drawer-grip-dot {
    width: 8px;
    height: 8px;
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
  .layout-b.compact .radio-drawer { transition: none; }
  .radio-drawer-grip-dot { animation: none !important; }
}
```

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
  - Import `RadioDrawer`.
  - Wrap the entire radio-panel mount block (the conditionals at L936-1003) in:

```tsx
{radioPanelMode !== null && (
  <RadioDrawer
    open={drawerOpen}
    onToggle={() => setDrawerOpen((o) => !o)}
    sessionState={/* existing shell modem state, else 'disconnected' */ 'disconnected'}
  >
    {/* the existing L936-1003 conditional panels, unchanged */}
  </RadioDrawer>
)}
```

  - When the panel is closed by its `onClose` (which already calls `setSelectedConnection(null); setPinRadioPanel(false)`), also `setDrawerOpen(false)` so a re-opened panel starts closed. Add `setDrawerOpen(false)` to each `onClose` (or refactor the repeated onClose into one `closeRadioPanel` callback — DRY; the 6 onClose handlers are identical).

- [ ] **Step 4: Run, verify it passes.** `pnpm exec vitest run src/shell/AppShell.radioPanel.test.tsx` → PASS. `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/shell/AppShell.tsx src/shell/AppShell.radioPanel.test.tsx
git commit -m "feat(shell): mount radio panel in RadioDrawer with manual open/close (tuxlink-h7q7)"
```

### Task 6: Compact panes grid (rail track + drop dock column + null legacy column)

**Files:**
- Modify: `src/shell/compactShell.css`
- Modify: `src/shell/AppShell.compact.test.tsx`

- [ ] **Step 1: Extend the test** — assert the compact block drops the 4th dock column and nulls the legacy 5th, and keeps the reader usable.

```tsx
it('drops the permanent dock column in compact (reader not starved)', () => {
  const block = css.slice(css.indexOf(COMPACT));
  // with-dock compact override must NOT contain a 4th fixed 400px track
  expect(block).toMatch(/\.panes--with-dock\s*\{[^}]*grid-template-columns:\s*48px 380px 1fr\s*;?[^}]*\}/);
});
it('nulls the legacy 5th column in compact', () => {
  const block = css.slice(css.indexOf(COMPACT));
  expect(block).toContain('panes--with-legacy-dock');
});
```

- [ ] **Step 2: Run, verify the new cases fail.** `pnpm exec vitest run src/shell/AppShell.compact.test.tsx`. `pkill -9 -f vitest`.

- [ ] **Step 3: Add the grid rules** inside the `@media (max-width: 1366px)` block of `compactShell.css`:

```css
  /* Panes grid: 200px sidebar → 48px rail; the radio dock 4th column is gone
   * (the panel is now an absolute drawer over the reader). The legacy 5th
   * column (dead today) is explicitly nulled in case a future dual-mount
   * re-applies the class. The reader's 1fr now spans ~852px at 1280px instead
   * of ~300px. */
  .layout-b .panes,
  .layout-b .panes--with-dock,
  .layout-b .panes--with-dock.panes--with-legacy-dock {
    grid-template-columns: 48px 380px 1fr;
  }
```

- [ ] **Step 4: Run, verify all compact-grid + the Task 2 `48px` case pass.** `pnpm exec vitest run src/shell/AppShell.compact.test.tsx` → PASS. `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/shell/compactShell.css src/shell/AppShell.compact.test.tsx
git commit -m "feat(shell): compact panes grid — 48px rail, drop dock column, reader reclaims width (tuxlink-h7q7)"
```

### Task 7: App-level mount test (production path)

**Files:**
- Modify: `src/App.test.tsx`

- [ ] **Step 1: Write the test** — mount `<App />` in compact mode and assert it renders the shell without crashing (the `QueryClientProvider`-wraps-AppShell production path; tuxlink-n4hz lesson).

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

@media (max-width: 1366px) {
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
  /* Icon rail: labels hidden, icons centered + enlarged, rows ≥44px tall. */
  .layout-b.compact .sidebar { padding: 8px 0; overflow: visible; }
  .layout-b.compact .sidebar .section-label,
  .layout-b.compact .sidebar .nav-label,
  .layout-b.compact .sidebar .count,
  .layout-b.compact .sidebar .v01-badge { display: none; }
  .layout-b.compact .sidebar .nav-item {
    justify-content: center;
    min-height: 44px;
    padding: 7px 0;
    gap: 0;
  }
  .layout-b.compact .sidebar .nav-item .icon { width: auto; font-size: 18px; }
  /* Expanded overlay — floats the full labeled sidebar over the list. */
  .layout-b.compact .sidebar.is-expanded {
    position: absolute;
    top: 0; left: 0; bottom: 0;
    width: 220px;
    z-index: 6;
    background: var(--surface);
    box-shadow: 4px 0 18px rgba(0, 0, 0, 0.35);
    overflow: auto;
    padding: 12px 0;
  }
  .layout-b.compact .sidebar.is-expanded .section-label,
  .layout-b.compact .sidebar.is-expanded .nav-label,
  .layout-b.compact .sidebar.is-expanded .count { display: revert; }
  .layout-b.compact .sidebar.is-expanded .nav-item { justify-content: flex-start; padding: 7px 18px; gap: 10px; }
  .layout-b.compact .sidebar.is-expanded .nav-item .icon { width: 14px; font-size: 11px; }
  /* The expand toggle (a rail header button). */
  .layout-b.compact .rail-expand-btn { display: flex; }
  .rail-expand-btn { display: none; }
```

- [ ] **Step 4: Implement the expand state + affordance** in `FolderSidebar` (preferred — keeps sidebar concerns local) or AppShell. Add a `railExpanded` state, a `.rail-expand-btn` at the top of `<nav className="sidebar">` (`aria-expanded`, `aria-label="Expand folders"`), apply `is-expanded` to the nav className, and collapse on `onSelectFolder`/tap-away. Add a behavior test:

```tsx
it('toggles rail expansion and collapses on folder select (compact)', () => {
  // render sidebar with a compact prop or wrap; click rail-expand-btn → nav has is-expanded;
  // click a folder → is-expanded removed.
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

- [ ] **Step 4: Implement** the clamp fn + use it at the builder. Resolve the current monitor work area via the Tauri API available in that scope (`monitor.size()` / `available work area`; if the builder lacks a monitor handle pre-creation, clamp against the primary monitor or use `current_monitor()` after creation + `set_size`). Replace the literal `.inner_size(1100.0, clamped_compose_height(820.0, work_h, 24.0))`.

- [ ] **Step 5: Run → PASS. Commit.**

```bash
git add src-tauri/src
git commit -m "fix(compose): clamp window default height to monitor work area for FZ-M1 (tuxlink-h7q7)"
```

### Task 13: In-window Compose + embedded-form compact CSS

**Files:**
- Modify: `src/compose/Compose.css`, `src/compose/CheckInForm.css`, `src/compose/Ics309FormV2.css`, `src/compose/PositionFormV2.css`
- Test: `src/compose/Compose.test.tsx` (CSS-string assertions if a raw-CSS import exists; else a behavior test that the action bar is reachable)

- [ ] **Step 1–4: TDD per the checklist** — add `@media (max-width: 1366px)` blocks: action btns/inputs ≥44px; receipt checkbox + 44px label row; attachments drop-zone 36→48px; `.tux-compose-titlebar .tux-ctrl { min-width: 44px; min-height: 44px; }` (R4 — scoped to compose, NOT bare `.tux-ctrl`); embedded inputs/buttons ≥44px; CheckIn radios + ICS-309 `datetime-local` sized; font floors `.compose-hint`/`fix-badge`/`grid-error` → 12px; tighten embedded-form padding 16→10, gap 12→8. **Do NOT add a root font-size shrink (ICS-309 is rem-based — it would dip below floor).** Assert via CSS-string slicing; commit per file or as one Compose-CSS commit.

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

- [ ] **Step 1–4: TDD.** `@media (max-width: 1366px)` in `wizard.css`: keep 580px centered card; `.wizard-root` top-pad → `clamp(16px, 3vh, 32px)`; card padding 38/40 → 24/28; `.wizard-session-log { max-height: 30vh; }`; submit-row buttons/inputs/password-toggle/link-button/Retry ≥44px; bump 12/12.5px offenders → 13px; **mono session-log + `wizard-failed-detail` → 13px; lighten the faint `.wizard-footer-copy` color**; `code` 0.88em → 0.92em. **Flag (in the PR body + a code comment) the inline Register anchor — it cannot cleanly reach 44px inline; a design call (restyle as button vs accept line-box) is deferred.**

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

- [ ] **Step 1–4: TDD per checklist.** `@media (max-width: 1366px)`: `.ics309-log-entry` 3-col → single column; `.damage-category` 6-col → 2-col; `.damage-category > legend > input` 200px → 100%; picker list rows ≥44px; all action buttons/native inputs ≥44px; native checkbox → 22px; toolbar select/buttons ≥44px; font floors field `label`/`log-entry>strong`/table `th` → 12px.

- [ ] **Step 5: Commit**

```bash
git add src/forms src/compose/WebviewFormHost.css src/mailbox/WebviewFormViewer.css
git commit -m "feat(forms): compact CSS — reflow, touch, font floors (tuxlink-h7q7)"
```

### Task 17: Embedded form-viewer reposition guard (the load-bearing integration fix)

**Files:**
- Modify: `src/mailbox/WebviewFormViewer.tsx` (re-measure on drawer/compact change)
- Modify: `src/shell/AppShell.tsx` (thread a reposition signal — `drawerOpen` + `isCompact` — to the viewer, or expose a context the viewer subscribes to)
- Test: `src/mailbox/WebviewFormViewer.test.tsx`

**Risk R1:** the viewer's child Tauri webview is pixel-positioned and repositions only on `ResizeObserver(embed + document.body)`. An overlay drawer moves the reader without resizing those boxes → stale webview position occluded by/overlapping the drawer.

- [ ] **Step 1: Read `WebviewFormViewer.tsx`** to find the reposition function (the ResizeObserver callback / the `set_position` IPC). Identify the cleanest re-measure trigger (a `useEffect` keyed on a `repositionSignal` prop, or an exposed imperative `remeasure()`).

- [ ] **Step 2: Write the failing test** — when the reposition-signal prop changes (drawer toggles), the viewer re-measures (assert the reposition function/IPC is called).

```tsx
it('re-measures the embedded webview when the drawer toggles in compact (R1)', () => {
  // mock the reposition IPC/callback; render the viewer with repositionSignal=0;
  // rerender with repositionSignal=1; assert the reposition fn was called.
});
```

- [ ] **Step 3: Implement** — add a `repositionSignal` (or `drawerOpen`/`isCompact`) prop; `useEffect(() => { remeasure(); }, [repositionSignal])`. In `AppShell.tsx`, pass a signal derived from `drawerOpen` + `isCompact` down to the reader's `WebviewFormViewer`. (If the viewer is deep in the reading-pane IIFE — shoal-raven-gorge's region — thread the prop through the existing props rather than restructuring; coordinate the prop addition.)

- [ ] **Step 4: Run → PASS.** `pkill -9 -f vitest`.

- [ ] **Step 5: Commit**

```bash
git add src/mailbox/WebviewFormViewer.tsx src/mailbox/WebviewFormViewer.test.tsx src/shell/AppShell.tsx
git commit -m "fix(forms): re-measure embedded form-viewer webview on drawer toggle (R1, tuxlink-h7q7)"
```

---

## Final verification (before PR)

- [ ] **Full targeted test run** (narrow, not a sweep): `pnpm exec vitest run src/shell src/mailbox/FolderSidebar.test.tsx src/mailbox/WebviewFormViewer.test.tsx src/compose src/wizard src/forms src/App.test.tsx` → all PASS. `pkill -9 -f vitest`.
- [ ] **Typecheck:** `pnpm typecheck` → clean.
- [ ] **Rust:** `cargo test --manifest-path src-tauri/Cargo.toml` (compose clamp) → PASS.
- [ ] **Codex cross-provider adversarial review** (required by `build-robust-features` — see `feedback_no_carveout_on_cross_provider_adrev`; this is design-bearing UX, not plumbing): run rounds against the diff, focusing Codex on: (a) any desktop-layout regression (rules leaking outside the media query / `.compact`), (b) the overlay-vs-push reader-occlusion trade-off, (c) the R1 webview reposition correctness, (d) the rail 36-vs-48px tension, (e) the grip session-state coupling. Converge the PROPOSED open-item resolutions here. Write transcripts to `dev/adversarial/` (gitignored); summarize dispositions in the PR body.
- [ ] **Optional Playwright pass at 1280×800** for evidence (computed widths, screenshots).
- [ ] **Open a READY PR** (`gh pr create --base main`), NOT draft. **Do not self-merge** — the operator browser-smokes the responsive layout at real window sizes before merge.

## Coordination summary (shoal-raven-gorge, `bd-tuxlink-raez/contacts-favorites`)

| Risk | File | Their hunk | Our hunk | Action |
|---|---|---|---|---|
| HIGH (A) | `AppShell.tsx` | content-switch L869-929, `selectedFolder` L214 | panes className L841, drawer state ~L242, wrap L936-1003 | Different hunks; rebase-merge expected. Sequence Phase 1 explicitly; whoever merges second rebases. |
| MED (B) | `FolderSidebar.tsx` | `MAILBOX_ITEMS` L29-35 | `.nav-label` wrap L184 (Task 8), class-ify inline L211-271 (Task 9) | Same `.map` body; agree which PR lands `.nav-label`. Their Contacts item auto-flows into the rail. `selectedFolder` gains `'contacts'` — rail active-state round-trips it. |
| LOW (C) | `RadioPanel.tsx` | Favorites/Recent/Manual tabs in `radio-panel-body` L58 | none (drawer wraps at AppShell, not RadioPanel) | Their new tab strip needs the same compact rule as `.radio-panel-segmented` — reuse the class or add an equivalent compact entry. |
| INT (D) | reader × drawer | reading-pane | form-viewer reposition (Task 17) | Agree the drawer-toggle reposition trigger. |

Maintain bd dep edges as state evolves (`bd dep add`).
