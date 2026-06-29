# Frontend Cohesion Design System — Phase 0 + Status-Bar Pilot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the minimal design-token foundation and prove it on one real surface (the dashboard ribbon) in the actual WebKitGTK engine, without a redesign and with every step revertible.

**Architecture:** Additive only. Scale tokens go into the EXISTING `:root` in `src/App.css` (already the de-facto token home). A tiny shared control-class layer (`src/styles/controls.css`) is added but nothing is forced to adopt it. stylelint lands in **warn** mode so it never blocks. Then the dashboard ribbon (`dash-*` controls) is migrated onto the tokens as the pilot, verified by a before/after WebKitGTK render-harness PNG. The React `Button`/`Select`/`Field` wrapper API is deliberately NOT built or frozen in this plan — that waits until the ribbon AND the radio panes (a later plan) both survive screenshot review.

**Tech Stack:** Tauri 2 + React 18 + TypeScript, Vite, WebKitGTK 4.1 render target, plain CSS (class-based, CSS custom properties). No CSS framework.

**Source of truth (design doc):** [`docs/superpowers/specs/2026-06-29-frontend-cohesion-design-system-design.md`](../specs/2026-06-29-frontend-cohesion-design-system-design.md) (office-hours 2026-06-29, Codex cross-model validated; originated at `~/.gstack/projects/cameronzucker-tuxlink/`). bd issue: `tuxlink-9q6ly`.

## Global Constraints

- **NOT a redesign.** Additive foundation + one-surface pilot. Preserve the current look's intent; only unify sizing/spacing/radius onto a shared scale.
- **Every task is independently revertible.** No task changes more than its named files. Phase 0 (Tasks 1-3) is additive and changes zero pixels on its own.
- **Verification is visual + lint, not unit tests.** This is CSS/token work; there are no new unit tests. The gate per migration is: (a) the WebKitGTK render-harness PNG before/after diff (the real visual check — Chromium/jsdom cannot surface WebKitGTK fit/render defects, per memory `chromium-not-webkitgtk-proxy`), (b) `pnpm lint:css` warnings dropping, (c) `pnpm typecheck` + the existing `src/shell/DashboardRibbon.test.tsx` passing for no functional regression.
- **Token scale (verbatim — add to `src/App.css` `:root`):** `--space-1:2px --space-2:4px --space-3:6px --space-4:8px --space-5:12px --space-6:16px --space-7:20px --space-8:24px --space-9:32px`; `--ctl-h-xs:22px --ctl-h-sm:26px --ctl-h-md:30px`; `--ctl-pad-x-xs:6px --ctl-pad-x-sm:8px --ctl-pad-x-md:10px`; `--type-micro:10px --type-meta:11px --type-control:12px --type-body:13px --type-heading:16px`; `--radius-xs:2px --radius-control:3px --radius-panel:6px --radius-pill:999px`.
- **stylelint is WARN-mode only** in this plan. Do not flip to error. The big-3 React wrapper API is NOT built here.
- **Render-harness PNGs are git-ignored** (`*.png`). Commit only harness scripts, never the PNGs.
- **Stay in this worktree** (`worktrees/bd-tuxlink-9q6ly-frontend-design-system`, branch `bd-tuxlink-9q6ly/frontend-design-system` off `origin/main`). The main checkout is the operator's; never write there.

---

### Task 1: Add the scale tokens to `src/App.css` `:root` (additive, zero visual change)

**Files:**
- Modify: `src/App.css` (the first `:root { … }` block — append the scale tokens after the existing `--mono`/`--sans`/`--tux-*` tokens, inside the same block)

**Interfaces:**
- Produces: the CSS custom properties named in Global Constraints, available app-wide. Later tasks consume `var(--type-*)`, `var(--space-*)`, `var(--radius-*)`, `var(--ctl-h-*)`.

- [ ] **Step 1: Add the tokens.** Inside the existing `:root` block in `src/App.css`, after the existing tokens, add a commented group:

```css
  /* === Control/sizing scale (tuxlink-9q6ly, 2026-06-29) — dense-desktop scale.
     Additive: existing color/font tokens above are unchanged. Surfaces migrate
     onto these incrementally; see docs/superpowers/plans/2026-06-29-frontend-cohesion-design-system.md */
  --space-1: 2px;  --space-2: 4px;  --space-3: 6px;  --space-4: 8px;
  --space-5: 12px; --space-6: 16px; --space-7: 20px; --space-8: 24px; --space-9: 32px;
  --ctl-h-xs: 22px;  --ctl-h-sm: 26px;  --ctl-h-md: 30px;
  --ctl-pad-x-xs: 6px; --ctl-pad-x-sm: 8px; --ctl-pad-x-md: 10px;
  --type-micro: 10px; --type-meta: 11px; --type-control: 12px;
  --type-body: 13px;  --type-heading: 16px;
  --radius-xs: 2px; --radius-control: 3px; --radius-panel: 6px; --radius-pill: 999px;
```

- [ ] **Step 2: Verify nothing consumes them yet (zero visual change).** Run:
```bash
grep -c -- '--type-body' src/App.css        # expect 1 (the definition only)
grep -rl 'var(--type-body)' src --include=*.css | head   # expect EMPTY (no consumers yet)
```
Expected: token defined once, no consumers — so the app looks identical.

- [ ] **Step 3: Verify the build is unaffected.** Run:
```bash
pnpm typecheck && pnpm build
```
Expected: PASS (CSS additions don't affect TS; `vite build` succeeds).

- [ ] **Step 4: Commit.**
```bash
git add src/App.css
git commit -m "feat(design-system): add control/sizing scale tokens to :root (tuxlink-9q6ly)"
```

---

### Task 2: Add the shared control-class layer (`src/styles/controls.css`), additive

**Files:**
- Create: `src/styles/controls.css`
- Modify: `src/App.tsx:6` area (add `import './styles/controls.css';` immediately AFTER `import './App.css';` so token defs load first)

**Interfaces:**
- Consumes: the tokens from Task 1.
- Produces: classes `.tux-btn` / `.tux-btn-primary` / `.tux-btn-sm` / `.tux-field` / `.tux-select`. Nothing in the app uses them yet (the ribbon pilot in Task 5 migrates onto tokens, not these classes — the classes are the foundation for later React wrappers; they ship now so the foundation is reviewable).

- [ ] **Step 1: Create `src/styles/controls.css`:**
```css
/* Shared control primitives (tuxlink-9q6ly). Built on the scale tokens in
 * App.css :root. Additive — no markup adopts these yet; they are the reviewable
 * foundation the big-3 React wrappers (Button/Select/Field) will wrap later
 * (deferred until the ribbon + radio panes survive screenshot review). */
.tux-btn {
  height: var(--ctl-h-sm);
  padding: 0 var(--ctl-pad-x-md);
  font-size: var(--type-control);
  font-weight: 600;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-control);
  background: var(--surface-2);
  color: var(--text);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: max-content;          /* size to content, never stretch (anti flex:1) */
}
.tux-btn-sm { height: var(--ctl-h-xs); padding: 0 var(--ctl-pad-x-sm); }
.tux-btn-primary { background: var(--accent); border-color: var(--accent); color: var(--bg); }
.tux-btn:disabled { opacity: 0.6; cursor: default; }
.tux-field {
  height: var(--ctl-h-sm);
  padding: 0 var(--ctl-pad-x-sm);
  font-size: var(--type-body);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-control);
  background: var(--surface-2);
  color: var(--text);
}
.tux-select { /* extends .tux-field; custom chevron is added per-surface for now */
  height: var(--ctl-h-sm);
  padding: 0 var(--ctl-pad-x-sm);
  font-size: var(--type-body);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-control);
  background: var(--surface-2);
  color: var(--text);
  appearance: none;
  -webkit-appearance: none;
}
```

- [ ] **Step 2: Import it in `src/App.tsx`.** After the existing `import './App.css';` line, add:
```ts
import './styles/controls.css';
```

- [ ] **Step 3: Verify additive (no adoption, no visual change).** Run:
```bash
grep -rl 'tux-btn\|tux-field\|tux-select' src --include=*.tsx | head   # expect EMPTY (no markup uses them)
pnpm typecheck && pnpm build
```
Expected: no consumers; build PASS; app looks identical.

- [ ] **Step 4: Commit.**
```bash
git add src/styles/controls.css src/App.tsx
git commit -m "feat(design-system): add shared control-class layer (additive, unused) (tuxlink-9q6ly)"
```

---

### Task 3: Add stylelint in WARN mode + `lint:css` script

**Files:**
- Create: `.stylelintrc.json`
- Modify: `package.json` (add `stylelint` + `stylelint-config-standard` devDeps and a `lint:css` script)

**Interfaces:**
- Produces: `pnpm lint:css` — reports (does not fail on) raw `font-size`/`border-radius` px literals, the loudest "no-scale" tells. Spacing (padding/margin/gap) is intentionally NOT guarded yet (layout legitimately uses px); scope widens in a later plan.

- [ ] **Step 1: Add devDeps.** Run:
```bash
pnpm add -D stylelint stylelint-config-standard
```

- [ ] **Step 2: Create `.stylelintrc.json`** — warn-only guard on raw font-size/radius units (var() tokens pass; raw px warns):
```json
{
  "extends": ["stylelint-config-standard"],
  "rules": {
    "declaration-property-unit-allowed-list": [
      { "font-size": [], "border-radius": [] },
      { "severity": "warning", "message": "Use a var(--type-*) / var(--radius-*) token, not a raw px (tuxlink-9q6ly)" }
    ]
  },
  "ignoreFiles": ["dist/**", "node_modules/**", "src-tauri/**"]
}
```

- [ ] **Step 3: Add the `lint:css` script** to `package.json` `scripts`:
```json
"lint:css": "stylelint \"src/**/*.css\""
```

- [ ] **Step 4: Verify it RUNS and WARNS without failing.** Run:
```bash
pnpm lint:css; echo "exit=$?"
```
Expected: prints warnings for existing raw `font-size`/`border-radius` px (e.g. the `dash-*` 9/13/14px, the wizard 14/10/7px radii) AND **exit=0** (warnings don't fail the build). If exit is non-zero, the rule severity is wrong — fix to `"severity": "warning"`.

- [ ] **Step 5: Commit.**
```bash
git add .stylelintrc.json package.json pnpm-lock.yaml
git commit -m "build(design-system): add stylelint in warn mode + lint:css (tuxlink-9q6ly)"
```

---

### Task 4: Add a `view=ribbon` mount to the render harness + capture the BEFORE baseline

**Files:**
- Modify: `dev/render-harness/harness.tsx` (currently mounts `<RequestCenter>`; add a `ribbon` branch mounting `<DashboardRibbon>` with shimmed data)

**Interfaces:**
- Consumes: `src/shell/DashboardRibbon.tsx` (read its props first — Step 1).
- Produces: a `?view=ribbon` route that renders the dashboard ribbon standalone in WebKitGTK, so Task 5 can diff before/after.

- [ ] **Step 1: Read the props you must shim.** Run:
```bash
sed -n '1,60p' src/shell/DashboardRibbon.tsx        # the props interface + what Tauri data it reads
grep -nE 'invoke\(|useStatus|props\.|interface .*Props' src/shell/DashboardRibbon.tsx | head -30
```
Note the required props + any `invoke('…')` IPC the component calls on mount; you will shim those in `window.__TAURI_INTERNALS__` the same way the existing harness shims `config_read`/`catalog_list` (see `dev/render-harness/harness.tsx` lines ~20-58).

- [ ] **Step 2: Add the ribbon branch to `harness.tsx`.** Widen the `view` type to include `'ribbon'`, import `DashboardRibbon`, extend the IPC shim with the canned values the ribbon needs (callsign e.g. `N7CPZ`, grid from the existing `grid` param, a connection status string, GPS source), and render `<DashboardRibbon …/>` when `view==='ribbon'`. Mirror the existing `createRoot(...).render(...)` pattern; pass realistic props (no Lorem — real callsign/grid/connection text) so the render is representative.

- [ ] **Step 3: Capture the BEFORE baseline.** In one terminal: `pnpm dev` (serves :1420). In another:
```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://localhost:1420/dev/render-harness/harness.html?view=ribbon&grid=CN87uo" \
    /tmp/ribbon-before.png 1366 120 2500
```
Open `/tmp/ribbon-before.png` (or Read it). Expected: the dashboard ribbon renders — callsign/grid/source-segments/connection/Connect — showing today's mixed font sizes. If it doesn't render, the shim is missing an IPC the component calls; add it and re-snapshot.

- [ ] **Step 4: Verify no regression from the harness change + commit** (PNG is git-ignored — do NOT add it):
```bash
pnpm typecheck && npx vitest run src/shell/DashboardRibbon.test.tsx
git add dev/render-harness/harness.tsx
git commit -m "test(design-system): add view=ribbon to render harness for the pilot (tuxlink-9q6ly)"
```

---

### Task 5: Migrate the dashboard ribbon `dash-*` controls onto the tokens (the pilot)

**Files:**
- Modify: `src/shell/AppShell.css` (the `.layout-b .dashboard .dash-*` rules)
- Modify: `src/shell/StatusBar.css` (only if it re-declares any `dash-*` sizing; otherwise leave it)

**Interfaces:**
- Consumes: tokens from Task 1; the `view=ribbon` harness from Task 4.

**Mapping (apply verbatim; these are the raw values found in AppShell.css → token):**
- `font-size: 10px` (`.dash-label`, `.dash-gps-no-fix-status`, `.dash-set-manually`, `.dash-grid-error`) → `var(--type-micro)`
- `font-size: 13px` (`.dash-value`, `.dash-grid-value-btn`, `.dash-grid-input`) → `var(--type-body)`
- `font-size: 12px` (`.dashboard` base) → `var(--type-control)`
- `font-size: 9px` (`.dash-source-segment`) → `var(--type-micro)` (10px — Codex + design doc flag 9px as too small; the +1px is intentional and the one deliberate visual change in this task)
- `font-size: 14px` (`.dash-value.callsign`, `.dash-callsign-text`, `.dash-callsign-select`) → **DECISION (call it in the PR):** keep callsign prominence via `font-weight: 700` at `var(--type-body)` (13px), OR add `--type-strong: 14px` to the scale and use it. Default: weight-only at `--type-body` (one fewer scale step). Capture both in the before/after if unsure.
- `border-radius: 3px` (selects, segments) → `var(--radius-control)`
- raw paddings → nearest `--space-*` (`1px`→`var(--space-1)` 2px, `4px`→`var(--space-2)`, `6px`→`var(--space-3)`, `16px`→`var(--space-6)`). Layout gaps on `.dashboard` (`gap:28px`, `padding:0 18px`) MAY stay as-is — they are layout, not control sizing; migrate only if it doesn't shift the ribbon layout in the PNG.

- [ ] **Step 1: Apply the mapping** in `src/shell/AppShell.css` — replace each raw `font-size`/`border-radius` on the `dash-*` rules with the token per the table. Leave colors, flex layout, and the SVG chevron data-URIs untouched (out of scope).

- [ ] **Step 2: Capture the AFTER render + diff.** With `pnpm dev` running:
```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://localhost:1420/dev/render-harness/harness.html?view=ribbon&grid=CN87uo" \
    /tmp/ribbon-after.png 1366 120 2500
```
Read `/tmp/ribbon-before.png` and `/tmp/ribbon-after.png` side by side. Expected: the ribbon now uses one coherent type rhythm (micro/control/body), unified radii; the 9px segment is now legibly 10px; no control is clipped or misaligned; the layout is otherwise unchanged. If anything shifted unexpectedly, the mapping touched layout — revert that line and re-snapshot.

- [ ] **Step 3: Verify lint warnings dropped + no functional regression.** Run:
```bash
pnpm lint:css 2>&1 | grep -c 'dash-'   # fewer dash-* warnings than before Task 5
pnpm typecheck && npx vitest run src/shell/DashboardRibbon.test.tsx
```
Expected: dash-* font-size/radius warnings reduced; typecheck + ribbon test PASS.

- [ ] **Step 4: Commit.**
```bash
git add src/shell/AppShell.css src/shell/StatusBar.css
git commit -m "refactor(design-system): migrate dashboard ribbon dash-* onto scale tokens (pilot) (tuxlink-9q6ly)"
```

---

## Self-Review

**Spec coverage** (vs design doc):
- Phase 0 minimal foundation → Task 1 (tokens) + Task 2 (control classes) + Task 3 (stylelint warn). ✓
- Pilot-first de-risk (Codex) → status-bar/ribbon is the pilot (Tasks 4-5); React wrapper API NOT frozen (stated in Global Constraints + Task 2). ✓
- WebKitGTK before/after PNG verification → Tasks 4-5 via render harness. ✓
- Additive / revertible → Tasks 1-3 add-only, Tasks 4-5 one-surface. ✓
- Token scale verbatim → Global Constraints + Task 1. ✓

**Placeholder scan:** no TBD/TODO; every code step shows the code; commands have expected output. The one open decision (callsign 14px) is explicitly flagged as a PR-time call with a stated default, not a placeholder. ✓

**Type/name consistency:** token names identical across Global Constraints, Task 1 (def), Task 2 + Task 5 (consumers). Class names `.tux-btn`/`.tux-field`/`.tux-select` consistent. Harness `view=ribbon` consistent across Tasks 4-5. ✓

## Out of scope (follow-up bd issues, per design doc)

Radio panes (`flex:1` kill), wizard gradient/animation/radius soup, `.tux-dialog` dedup (Catalog/GRIB/FormPicker), Sparkline gradients, SessionLog glyphs, fractional fonts, the React `Button`/`Select`/`Field` wrappers, and flipping stylelint to error — all AFTER the ribbon + radio panes survive screenshot review.
