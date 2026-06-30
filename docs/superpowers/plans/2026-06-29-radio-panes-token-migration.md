# Radio Panes + Non-Enumerated Ribbon Token Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate the non-enumerated dashboard-ribbon `dash-*` controls and the radio panes onto the existing design-system scale tokens (font-size + border-radius), kill the one stray full-width-stretch button, and prove each surface in the real WebKitGTK engine — without a redesign and with every task revertible.

**Architecture:** Additive token adoption, surface by surface. The scale tokens already live in `src/App.css :root` (shipped by tuxlink-9q6ly). This plan only *consumes* them — it changes no token definitions and adds no new tokens (the radius scale decision below resolves to "snap, no new step"). Each task migrates one CSS surface's `font-size`/`border-radius` literals to `var(--type-*)`/`var(--radius-*)`, verified by a before/after WebKitGTK render-harness PNG. The React `Button`/`Select`/`Field` wrapper API stays UNBUILT and UNFROZEN — that waits until BOTH the ribbon and radio panes survive screenshot review (the gating constraint that motivated splitting this from Phase 0).

**Tech Stack:** Tauri 2 + React 18 + TypeScript, Vite, WebKitGTK 4.1 render target, plain CSS (class-based, CSS custom properties). No CSS framework.

**Source of truth:** [`docs/superpowers/specs/2026-06-29-frontend-cohesion-design-system-design.md`](../specs/2026-06-29-frontend-cohesion-design-system-design.md); Phase 0 plan [`2026-06-29-frontend-cohesion-design-system.md`](2026-06-29-frontend-cohesion-design-system.md) (Out-of-scope list). bd issue: `tuxlink-zj9se` (depends-on closed `tuxlink-9q6ly`).

## Global Constraints

- **NOT a redesign.** Token adoption only. Preserve the current look's intent; unify sizing/radius onto the shared scale. The lone intentional pixel change is `9px → 10px` (`--type-micro`) on tiny labels, matching the pilot's source-segment precedent.
- **Every task is independently revertible.** No task changes more than its named files.
- **Verification is visual + lint, not unit tests.** Per migration the gate is: (a) the WebKitGTK render-harness PNG before/after (Chromium/jsdom cannot surface WebKitGTK fit/render defects — memory `chromium-not-webkitgtk-proxy`); (b) `pnpm lint:css` font-size/radius warnings dropping for the migrated files; (c) `pnpm typecheck` + the relevant existing vitest passing for no functional regression.
- **Scope = font-size + border-radius (the two stylelint-guarded properties) + the one `.radio-panel-btn` flex:1 kill.** Spacing (padding/margin/gap) stays on raw px per Phase 0 ("layout legitimately uses px; scope widens in a later plan"), EXCEPT control-padding that maps 1:1 to a `--space-*` token with zero layout shift MAY be migrated opportunistically inside the same rule. When in doubt, leave padding alone — the render gate is the arbiter.
- **Token values (consume only; defined in `src/App.css`):** `--type-micro:10px --type-meta:11px --type-control:12px --type-body:13px --type-heading:16px`; `--radius-xs:2px --radius-control:3px --radius-panel:6px --radius-pill:999px`; `--space-1..9: 2/4/6/8/12/16/20/24/32px`.
- **Type mapping (apply verbatim — these are 1:1 px-identical except 9px):** `10px→--type-micro` · `11px→--type-meta` · `12px→--type-control` · `13px→--type-body` · `16px→--type-heading` · `9px→--type-micro` (+1px, intentional) · `14px→--type-body (13px) + keep prominence via existing font-weight` (the callsign precedent; flag each occurrence in the PR). LEAVE: `32px` (the SignalSection `.qv` quality-number display stat — no token, out of scope), icon glyph sizes (`16/20px` on `.diag-icon`/`.diag-dismiss`/close glyphs — these are icon metrics, not type).
- **Radius mapping (apply verbatim — the resolved scale decision, NO new token):** `2px→--radius-xs` · `3px→--radius-control` · `6px→--radius-panel` · interactive controls at `4px/5px/7px→--radius-control (3px)` · surfaces/popovers at `8px→--radius-panel (6px)` · count badges/short pills at `9px/12px→--radius-pill`. **Rationale:** the radio panes already de-facto standardized on 3/6/12; the pilot set 3px for ribbon controls; snapping unifies all three surfaces with zero new tokens. The most visible change is the ribbon's `7px` Elmer/egress chips → 3px — verify in the render; **fallback ONLY for those two tall chips to `--radius-panel (6px)` if the render shows boxiness.** Document the choice in the PR for the operator's screenshot-review gate.
- **stylelint stays WARN mode.** Do not flip to error (that is a later Out-of-scope item).
- **Render-harness PNGs are git-ignored** (`*.png`). Commit only harness scripts, never PNGs.
- **Stay in this worktree** (`worktrees/bd-tuxlink-zj9se-design-system-radio-panes`, branch `bd-tuxlink-zj9se/design-system-radio-panes` off `origin/main`). The main checkout is the operator's; never write there.
- **Render provenance:** the snapshot loads `http://localhost:1420/...` (Vite dev). Only ONE `pnpm dev` can bind `:1420` (memory `worktree-dev-port-collision`). Before trusting any render, confirm the `:1420` being served is THIS worktree's dev server (check the terminal you launched it from; if a render looks unexpectedly unchanged, suspect a stale `:1420` from another worktree).

---

### Task 1: Extend the render harness with radio-pane `?view=` mounts + capture BEFORE baselines

**Files:**
- Modify: `dev/render-harness/harness.tsx` (widen the `view` union; import the radio panels; extend the `RESPONSES` IPC shim; add a `radio-*` render branch)

**Interfaces:**
- Consumes: `src/radio/modes/ArdopRadioPanel.tsx`, `VaraRadioPanel.tsx`, `TelnetRadioPanel.tsx` (read their `Props` + the `invoke('…')` calls / hooks their mount fires — Step 1).
- Produces: `?view=radio-ardop`, `?view=radio-vara`, `?view=radio-telnet` routes that render each pane standalone in WebKitGTK inside `<div class="layout-b"><div class="panes panes--with-dock">…</div></div>`, so later tasks can diff before/after.

- [ ] **Step 1: Read the props + IPC each pane fires on mount.** Run:
```bash
cd worktrees/bd-tuxlink-zj9se-design-system-radio-panes
sed -n '1,80p' src/radio/modes/ArdopRadioPanel.tsx
sed -n '1,80p' src/radio/modes/VaraRadioPanel.tsx
sed -n '1,80p' src/radio/modes/TelnetRadioPanel.tsx
sed -n '1,60p' src/radio/RadioPanel.tsx
grep -nE "invoke\(|useQuery|useStatus|useModemStatus|useSessionLog|useSampleHistory|useActiveIdentity|useFavorites|useListenerState|getRigProfile" src/radio/modes/ArdopRadioPanel.tsx src/radio/modes/VaraRadioPanel.tsx src/radio/modes/TelnetRadioPanel.tsx
```
Note every `invoke('cmd')` and every hook that subscribes to a Tauri event; each needs a canned `RESPONSES` entry (else the pane throws "no canned response for 'cmd'" and renders blank — the same failure the ribbon hit before QueryClientProvider was added).

- [ ] **Step 2: Read the existing harness to copy its pattern.** Run:
```bash
cat dev/render-harness/harness.tsx
```
Confirm: the `view` union (~line 28), the `RESPONSES` object (~lines 52–59), the `__TAURI_INTERNALS__.invoke` shim (~lines 61–69), and the `createRoot(...).render(<QueryClientProvider>…)` branch (~lines 81–92). `App.css` + `AppShell.css` are already imported at top.

- [ ] **Step 3: Widen the `view` union** to add the radio views. Replace the existing `const view = … as 'home' | 'browse' | 'grib' | 'ribbon';` with:
```tsx
const view = (params.get('view') ?? 'home') as
  | 'home' | 'browse' | 'grib' | 'ribbon'
  | 'radio-ardop' | 'radio-vara' | 'radio-telnet';
```

- [ ] **Step 4: Import the radio panels** near the existing component imports:
```tsx
import { ArdopRadioPanel } from '../../src/radio/modes/ArdopRadioPanel';
import { VaraRadioPanel } from '../../src/radio/modes/VaraRadioPanel';
import { TelnetRadioPanel } from '../../src/radio/modes/TelnetRadioPanel';
```

- [ ] **Step 5: Extend `RESPONSES`** with the canned values the panes need (fill in the exact command names + a representative shape discovered in Step 1 — realistic callsign/grid/gateway text, NO Lorem). Template (adjust keys to the actual `invoke` names found):
```tsx
const RESPONSES: Record<string, unknown> = {
  // …existing entries…
  config_get_ardop: { /* representative ARDOP config from Step 1's interface */ },
  config_get_rig: { /* representative rig profile */ },
  ardop_list_audio_devices: [],
  packet_list_serial_devices: [],
  // …add one entry per invoke()/event the panes fire on mount…
};
```
If a hook subscribes to an event stream (e.g. `modem:status`) rather than calling `invoke`, render with the pane's default/disconnected state — do not fabricate a live stream; a disconnected snapshot is a valid representative render.

- [ ] **Step 6: Add the `radio-*` render branch.** Extend the render ternary so radio views mount inside the dock layout (the wrapper that scopes radio-pane CSS — `.radio-panel` is a direct child of `.panes--with-dock`):
```tsx
{view === 'ribbon' ? (
  <div className="layout-b">
    <DashboardRibbon data={ribbonData} onConnect={() => undefined} />
  </div>
) : view.startsWith('radio-') ? (
  <div className="layout-b">
    <div className="panes panes--with-dock">
      {view === 'radio-ardop' && <ArdopRadioPanel onClose={() => undefined} />}
      {view === 'radio-vara' && <VaraRadioPanel onClose={() => undefined} />}
      {view === 'radio-telnet' && <TelnetRadioPanel onClose={() => undefined} />}
    </div>
  </div>
) : (
  <RequestCenter initialView={view} onClose={() => undefined} />
)}
```
(Pass whatever required props each pane's interface declares from Step 1 — `onClose` is known; add `mode`/`onFindGateway`/etc. as the interface requires.)

- [ ] **Step 7: Capture the BEFORE baselines.** In one terminal from the worktree root: `pnpm dev` (serves `:1420`; confirm it is THIS worktree's server). In another, capture each pane + re-capture the ribbon (the ribbon view already exists; its non-enumerated controls change in Task 2):
```bash
cd worktrees/bd-tuxlink-zj9se-design-system-radio-panes
for V in ribbon radio-ardop radio-vara radio-telnet; do
  WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    python3 dev/render-harness/snapshot.py \
      "http://localhost:1420/dev/render-harness/harness.html?view=$V&grid=CN87uo" \
      "/tmp/zj9se-$V-before.png" 1366 900 2500
done
```
Read each `/tmp/zj9se-*-before.png`. Expected: ribbon renders (Connect/Abort/Elmer/APRS/egress/seg); each radio pane renders its header + sections (fields, buttons, session log) in the 400px dock showing today's mixed font sizes / 4px button radii. **If a pane is blank**, a mounted `invoke`/hook lacks a canned response — add it to `RESPONSES` (Step 5) and re-snapshot. Do not proceed until all four render.

- [ ] **Step 8: Verify no regression from the harness change + commit** (PNGs are git-ignored — do NOT add them):
```bash
pnpm typecheck
git add dev/render-harness/harness.tsx
git commit -m "test(design-system): add radio-pane view= mounts to render harness (tuxlink-zj9se)"
```

---

### Task 2: Migrate the non-enumerated ribbon `dash-*` controls onto tokens

**Files:**
- Modify: `src/shell/AppShell.css` (the `.layout-b .dashboard` rules for `.connect-button`, `.abort-button`, `.dash-elmer-agent` family, `.dash-aprs-control`, `.dash-aprs-unread`, `.dash-egress-chip`/`.dash-egress-caret`/`.egress-countdown`, `.egress-arm-popover` family, `.seg`/`.seg button`, `.dash-grid-pick-map`)
- Modify: `src/elmer/ElmerPane.css` (the `.elmer-arm-strip .dash-egress-chip`/`.dash-egress-caret`/`.egress-countdown` variants — keep them visually consistent with the ribbon copies)
- Modify: `src/shell/compactShell.css` (only the `.connect-button`/`.abort-button` compact overrides, IF they re-declare font-size/radius)

**Interfaces:**
- Consumes: tokens from `src/App.css`; the `?view=ribbon` harness (Task 1).

**Mapping (apply verbatim per the Global Constraints):**
- `.connect-button` / `.abort-button`: `font-size: 12px → var(--type-control)`; `border-radius: 4px → var(--radius-control)`.
- `.dash-elmer-agent`: `font-size: 12px → var(--type-control)`; `border-radius: 7px → var(--radius-control)`. Child `.dash-elmer-agent-spark` `13px → var(--type-body)`; `.dash-elmer-agent-cd` `11px → var(--type-meta)`.
- `.dash-aprs-control`: `font-size: 12px → var(--type-control)`.
- `.dash-aprs-unread`: `font-size: 10px → var(--type-micro)`; `border-radius: 9px → var(--radius-pill)` (count badge).
- `.dash-egress-chip`: `font-size: 12px → var(--type-control)`; `border-radius: 7px → var(--radius-control)`. `.dash-egress-caret` `10px → var(--type-micro)`.
- `.egress-countdown`: no font-size literal (inherits) — leave.
- `.egress-arm-popover`: `font-size: 12px → var(--type-control)`; `border-radius: 8px → var(--radius-panel)`. Children: `.egress-pop-title 11px → --type-meta`, `.egress-arm-label 11px → --type-meta`, `.egress-arm-button 12px → --type-control` + `border-radius: 6px → var(--radius-panel)`, `.egress-disarm-button 12px → --type-control` + `border-radius: 6px → var(--radius-panel)`, `.egress-pop-help 11px → --type-meta`, `.dash-egress-locked 12px → --type-control`, `.dash-egress-error 11px → --type-meta`.
- `.seg`: `border-radius: 5px → var(--radius-control)`. `.seg button`: `font-size: 11.5px → var(--type-meta)` (11px; resolves the fractional-font tell — the down-snap is intentional, verify legibility in render).
- `.dash-grid-pick-map`: read the rule (AppShell.css ~809) and map its `font-size`/`border-radius` literals per the table.
- LEAVE all colors, `flex`/layout, `gap`, `padding`, `height` (already `30px`/`24px` ≈ `--ctl-h-md`/`--ctl-h-xs` but leave numeric — height tokenization is not in this task's font/radius scope), and the SVG chevron data-URIs.

- [ ] **Step 1: Apply the mapping** in `src/shell/AppShell.css`, then the matching `.elmer-arm-strip` variants in `src/elmer/ElmerPane.css`, then any `compactShell.css` re-declarations. Replace ONLY `font-size` and `border-radius` literals per the table.

- [ ] **Step 2: Capture the AFTER render + diff.** With `pnpm dev` running:
```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://localhost:1420/dev/render-harness/harness.html?view=ribbon&grid=CN87uo" \
    /tmp/zj9se-ribbon-after.png 1366 900 2500
```
Read `/tmp/zj9se-ribbon-before.png` and `/tmp/zj9se-ribbon-after.png` side by side. Expected: Connect/Abort/Elmer/egress chips now share one control radius (3px); the APRS unread badge is pill; the seg control is unified; no control clipped/misaligned; Connect still pinned right. **Decision check:** if the 30px Elmer/egress chips look boxy at 3px, change ONLY those two to `var(--radius-panel)` (6px) and re-snapshot; record which you chose.

- [ ] **Step 3: Verify lint dropped + no functional regression.** Run:
```bash
pnpm lint:css 2>&1 | grep -cE 'AppShell\.css|ElmerPane\.css'   # fewer than before
pnpm typecheck && npx vitest run src/shell/DashboardRibbon.test.tsx
```
Expected: fewer ribbon font-size/radius warnings; typecheck + ribbon test PASS.

- [ ] **Step 4: Commit.**
```bash
git add src/shell/AppShell.css src/elmer/ElmerPane.css src/shell/compactShell.css
git commit -m "refactor(design-system): migrate non-enumerated ribbon dash-* onto scale tokens (tuxlink-zj9se)"
```

---

### Task 3: Migrate the shared radio-panel chrome (`RadioPanel.css`) + kill the stray flex:1 button

**Files:**
- Modify: `src/radio/RadioPanel.css`

**Interfaces:**
- Consumes: tokens from `src/App.css`; the `?view=radio-ardop` harness (Task 1).

**Mapping (font-size + radius literals; verbatim):**
- `.radio-panel 13px → --type-body`; `.radio-panel-name 14px → --type-body` (+ keep weight; flag in PR); `.radio-panel-close 13px → --type-body`; `.radio-panel-find-gateway 12px → --type-control` + `border-radius: 4px → var(--radius-control)`; `.radio-panel-field 13px → --type-body`; `.radio-panel-readonly 13px → --type-body`; `.radio-panel-input-row 13px → --type-body`; `.radio-panel-input 13px → --type-body`; `.radio-panel-radio-label 13px → --type-body`; `.radio-panel-btn 13px → --type-body`; `.radio-panel-chip border-radius: 12px → var(--radius-pill)`.
- Already-token-equal literals to tokenize for cohesion (zero visual change): `12px → --type-control`, `11px → --type-meta`, `10px → --type-micro`, `3px → --radius-control` on the rules in this file.
- LEAVE: all `width: 400px`/`min-width: 400px` (locked panel width), grid-template-column px (`72px`/etc. — layout), paddings, compact-mode `44px`/`22px` touch targets.

- [ ] **Step 1: Kill the stray full-width button (`flex:1` → content-sized).** In `.radio-panel-btn` (RadioPanel.css ~353), remove `flex: 1;`. The buttons sit in `.radio-panel-act` (a flex row with `gap: 6px`); without `flex:1` they size to content instead of each stretching to fill the row. Do NOT touch the `flex:1` on `.session-log-section`, `.log-scroll`, `.sparkline`, `.frame-ribbon` — those are correct vertical/chart fill.

- [ ] **Step 2: Apply the font-size + radius mapping** above in `src/radio/RadioPanel.css`.

- [ ] **Step 3: Capture AFTER + diff (radio-ardop view exercises this shared chrome).** With `pnpm dev` running:
```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://localhost:1420/dev/render-harness/harness.html?view=radio-ardop&grid=CN87uo" \
    /tmp/zj9se-radio-ardop-after.png 1366 900 2500
```
Read before/after. Expected: panel header, fields, inputs now on the type scale; action buttons are content-sized (NOT stretched edge-to-edge) and left-grouped with their gap; find-gateway radius matches other controls; no clipping. **If killing `flex:1` left the buttons cramped or the row ragged**, the correct fix is on `.radio-panel-act` (e.g. it may need `flex-wrap`/`justify-content`) — adjust the container, not by restoring `flex:1` on the button; re-snapshot.

- [ ] **Step 4: Verify lint dropped + typecheck + radio vitest.** Run:
```bash
pnpm lint:css 2>&1 | grep -c 'RadioPanel\.css'   # fewer than before
pnpm typecheck && npx vitest run src/radio/
```
Expected: fewer RadioPanel.css warnings; typecheck + radio tests PASS (if `vitest run src/radio/` matches no tests, run the nearest existing radio-pane test file instead).

- [ ] **Step 5: Commit.**
```bash
git add src/radio/RadioPanel.css
git commit -m "refactor(design-system): migrate radio-panel chrome onto tokens + kill stray flex:1 button (tuxlink-zj9se)"
```

---

### Task 4: Migrate the per-mode radio panels (`ArdopRadioPanel.css`, `VaraRadioPanel.css`)

**Files:**
- Modify: `src/radio/modes/ArdopRadioPanel.css`
- Modify: `src/radio/modes/VaraRadioPanel.css`

**Interfaces:**
- Consumes: tokens; the `?view=radio-ardop` + `?view=radio-vara` harness views.

**Mapping (font-size + radius literals; verbatim):**
- `ArdopRadioPanel.css`: `.ardop-arq-cell 12px → --type-control` + `border-radius: 3px → var(--radius-control)`; `.ardop-meter-k 12px → --type-control`; `.ardop-meter-v 14px → --type-body` (+ weight; flag); `.ardop-stats 12px → --type-control` + `border-radius: 3px → var(--radius-control)`; the scoped `.radio-panel-btn-sm 12px → --type-control` + `border-radius: 3px → var(--radius-control)`.
- `VaraRadioPanel.css`: `.radio-panel-info 12px → --type-control`; `.radio-panel-info code 11px → --type-meta` + `border-radius: 3px → var(--radius-control)`; `.radio-panel-info-compact 11px → --type-meta`.
- LEAVE: paddings, `border-left: 3px` (that is a border width, not a radius), `max-width`/`width` layout, colors.

- [ ] **Step 1: Apply the mapping** in both files.

- [ ] **Step 2: Capture AFTER + diff for both views.** With `pnpm dev` running:
```bash
for V in radio-ardop radio-vara; do
  WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    python3 dev/render-harness/snapshot.py \
      "http://localhost:1420/dev/render-harness/harness.html?view=$V&grid=CN87uo" \
      "/tmp/zj9se-$V-after2.png" 1366 900 2500
done
```
Read before/after for each. Expected: ARQ grid cells, meters, stats, VARA info blocks on the type scale; no clipping/shift.

- [ ] **Step 3: Verify lint + typecheck.** Run:
```bash
pnpm lint:css 2>&1 | grep -cE 'ArdopRadioPanel\.css|VaraRadioPanel\.css'   # fewer
pnpm typecheck
```

- [ ] **Step 4: Commit.**
```bash
git add src/radio/modes/ArdopRadioPanel.css src/radio/modes/VaraRadioPanel.css
git commit -m "refactor(design-system): migrate ARDOP/VARA mode panels onto tokens (tuxlink-zj9se)"
```

---

### Task 5: Migrate the radio-pane sections (`SessionLogSection`, `SignalSection`, `ListenSection`, `ModemLinkSection`, `AuthDiagnosticBanner`)

**Files:**
- Modify: `src/radio/sections/SessionLogSection.css`
- Modify: `src/radio/sections/SignalSection.css`
- Modify: `src/radio/sections/ListenSection.css`
- Modify: `src/radio/sections/ModemLinkSection.css`
- Modify: `src/radio/sections/AuthDiagnosticBanner.css`

**Interfaces:**
- Consumes: tokens; the `?view=radio-ardop` harness (exercises these sections inside the ARDOP panel).

**Mapping (font-size + radius literals; verbatim — all 1:1 px-identical except the 9px and 4px snaps):**
- `SessionLogSection.css`: `.log-scroll 12px → --type-control` + `border-radius: 3px → var(--radius-control)`; `.log-ts 11px → --type-meta`; `.log-controls 12px → --type-control`; `.log-copy 12px → --type-control`; `.log-clear 12px → --type-control`. (LEAVE `flex:1` here — correct fill.)
- `SignalSection.css`: `.qk 11px → --type-meta`; `.qs 11px → --type-meta`; `.lab-row 12px → --type-control`; `.k 11px → --type-meta`; `.v 12px → --type-control`; `.signal-axis-tick 11px → --type-meta`; `.signal-axis-avg 11px → --type-meta`; `.quality border-radius: 6px → var(--radius-panel)`. **LEAVE `.qv 32px`** (display stat, out of scope).
- `ListenSection.css`: `.expander border-radius: 6px → var(--radius-panel)`; `.expander-summary 12px → --type-control`; `.expander-summary::before 9px → var(--type-micro)` (the +1px legibility bump); `.expander-count 11px → --type-meta`; `.listen-status 11px → --type-meta` + `border-radius: 12px → var(--radius-pill)`; `.listener-identity-badge 11px → --type-meta` + `border-radius: 12px → var(--radius-pill)`; `.listen-allow-all-row 12px → --type-control`; `.radio-panel-chip-x 12px → --type-control`; `.radio-panel-help 11px → --type-meta`; `.po-mesh-dot 11px → --type-meta`; `.po-mesh-endpoint 12px → --type-control`; `.po-mesh-rtt 12px → --type-control`.
- `ModemLinkSection.css`: `.radio-panel-segmented button 11px → --type-meta` + `border-radius: 3px → var(--radius-control)`; the scoped `.radio-panel-btn-sm 12px → --type-control` + `border-radius: 3px → var(--radius-control)`; `.modem-link-help 12px → --type-control`.
- `AuthDiagnosticBanner.css`: `.diag-banner border-radius: 6px → var(--radius-panel)`; `.diag-title 13px → --type-body`; `.diag-body 12px → --type-control`; `.diag-actions` (no font) ; `.diag-btn 11px → --type-meta` + `border-radius: 3px → var(--radius-control)`; `.diag-raw-content 11px → --type-meta` + `border-radius: 3px → var(--radius-control)`; `.diag-help 11px → --type-meta`; `.diag-form border-radius: 4px → var(--radius-control)`; `.diag-form input 12px → --type-control` + `border-radius: 3px → var(--radius-control)`; `.row-input 12px → --type-control` + `.row-input > span 12px → --type-control`. **LEAVE** `.diag-icon 16px` + `.diag-dismiss 14px` (icon glyph metrics, not type).

- [ ] **Step 1: Apply the mapping** across all five section files.

- [ ] **Step 2: Capture AFTER + diff.** The ARDOP view renders SessionLog/Signal/Listen/ModemLink inline; AuthDiagnosticBanner needs an error state. With `pnpm dev` running:
```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py \
    "http://localhost:1420/dev/render-harness/harness.html?view=radio-ardop&grid=CN87uo" \
    /tmp/zj9se-radio-ardop-sections-after.png 1366 1200 2500
```
Read before/after. Expected: session log, signal block, listen expander, modem segmented control on the unified type scale; expander chevron now 10px (legible); badges pill-rounded; no shift. **If AuthDiagnosticBanner is not visible in the default ARDOP render** (it only shows on auth failure), verify its mapping by reading the diff and `pnpm lint:css` warning drop rather than the PNG; note in the PR that the banner was lint-verified, not render-verified (it has no reachable harness state).

- [ ] **Step 3: Verify lint + typecheck.** Run:
```bash
pnpm lint:css 2>&1 | grep -cE 'SessionLogSection|SignalSection|ListenSection|ModemLinkSection|AuthDiagnosticBanner'   # fewer
pnpm typecheck
```

- [ ] **Step 4: Commit.**
```bash
git add src/radio/sections/SessionLogSection.css src/radio/sections/SignalSection.css src/radio/sections/ListenSection.css src/radio/sections/ModemLinkSection.css src/radio/sections/AuthDiagnosticBanner.css
git commit -m "refactor(design-system): migrate radio-pane sections onto tokens (tuxlink-zj9se)"
```

---

### Task 6: Migrate `InboundSelectionPanel.css` + final whole-surface verification

**Files:**
- Modify: `src/connections/InboundSelectionPanel.css`

**Interfaces:**
- Consumes: tokens. (This overlay has no radio-pane harness view; verify via lint-drop + a dedicated `?view` only if cheaply reachable — otherwise lint + read-diff verify and note it in the PR.)

**Mapping (font-size + radius literals; verbatim):**
- `.inbound-selection__header h2 16px → --type-heading`; `.inbound-selection__countdown 12px → --type-control`; `.inbound-selection__toolbar button 12px → --type-control` + `border-radius: 4px → var(--radius-control)`; `.inbound-selection__count 12px → --type-control`; `.inbound-selection__row 13px → --type-body`; `.inbound-selection__col-head 11px → --type-meta`; `.inbound-selection__disposition legend 12px → --type-control`; `.inbound-selection__disposition label 13px → --type-body`; `.inbound-selection__go border-radius: 6px → var(--radius-panel)`. **LEAVE** `.inbound-selection__close 20px` (icon glyph), `560px`/`82vh` layout dims.

- [ ] **Step 1: Apply the mapping** in `src/connections/InboundSelectionPanel.css`.

- [ ] **Step 2: Verify lint + typecheck.** Run:
```bash
pnpm lint:css 2>&1 | grep -c 'InboundSelectionPanel'   # fewer
pnpm typecheck
```

- [ ] **Step 3: Whole-surface lint snapshot.** Run and record the total warning count (should be materially lower than the Phase 0 baseline of 3855):
```bash
pnpm lint:css 2>&1 | tail -3
```

- [ ] **Step 4: Full no-regression gate.** Run:
```bash
pnpm typecheck && pnpm build && npx vitest run src/shell/ src/radio/
```
Expected: all PASS (the Rust side is untouched; a full `cargo` run is CI's job per project policy).

- [ ] **Step 5: Re-capture ALL after-baselines for the PR + operator screenshot review.** With `pnpm dev` running:
```bash
for V in ribbon radio-ardop radio-vara radio-telnet; do
  WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    python3 dev/render-harness/snapshot.py \
      "http://localhost:1420/dev/render-harness/harness.html?view=$V&grid=CN87uo" \
      "/tmp/zj9se-$V-final.png" 1366 1200 2500
done
```
Read every before/final pair. This is the screenshot-review gate that unblocks freezing the React wrapper API (a SEPARATE later task — do NOT build the wrappers here).

- [ ] **Step 6: Commit.**
```bash
git add src/connections/InboundSelectionPanel.css
git commit -m "refactor(design-system): migrate inbound-selection panel onto tokens (tuxlink-zj9se)"
```

---

## Self-Review

**Spec coverage** (vs bd `tuxlink-zj9se` + Phase 0 Out-of-scope):
- Non-enumerated ribbon `dash-*` (connect/abort/aprs/egress/seg/grid-pick-map/elmer) → Task 2. ✓
- Off-scale radii 4/5/7/8/9px scale decision → resolved in Global Constraints (snap, no new token) + applied in Tasks 2–6. ✓
- Radio panes onto tokens → Tasks 3 (chrome) + 4 (modes) + 5 (sections) + 6 (inbound). ✓
- Radio-pane `flex:1` kill → Task 3 Step 1 (`.radio-panel-btn` only; correct fills left intact). ✓
- Per-surface WebKitGTK before/after via render harness (add mounts as needed) → Task 1 mounts + per-task captures. ✓
- React `Button`/`Select`/`Field` wrappers NOT frozen → stated in Architecture + Global Constraints; no task builds them. ✓
- DEFERRED (explicitly NOT this task, per Out-of-scope, AFTER screenshot review): wizard gradient/animation/radius, `.tux-dialog` dedup, Sparkline gradients, SessionLog glyphs, fractional fonts beyond the seg/chevron snaps done here, padding/margin scale-out, flipping stylelint to error.

**Placeholder scan:** every mapping lists exact selector → exact token; the one open decision (7px chips 3px-vs-6px) is a render-arbitrated PR-time call with a stated default, not a placeholder; the `RESPONSES` shim content in Task 1 is the one "discover-then-fill" step (unavoidable — the exact IPC names are read in Task 1 Step 1) and is bounded by an explicit template + the blank-render failure mode. ✓

**Type/name consistency:** token names identical to `src/App.css` definitions and the Phase 0 plan; view names `radio-ardop`/`radio-vara`/`radio-telnet` consistent across Task 1 (mount) and Tasks 3–5 (capture); file paths verified against the worktree tree. ✓

## Out of scope (remain follow-up bd issues per the design doc)

Wizard gradient/animation/radius soup, `.tux-dialog` dedup (Catalog/GRIB/FormPicker), Sparkline gradients, SessionLog glyphs, the `.qv` 32px display-stat tokenization, padding/margin/gap scale-out, the React `Button`/`Select`/`Field` wrappers, and flipping stylelint to error — all AFTER the ribbon + radio panes survive this plan's screenshot review.
