# FZ-M1 Compact-Shell Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the FZ-M1 compact push-drawer with an overlay, fix the rail-expand grid implosion, swap the indistinct icon rail for vertical-text folder tabs, and clean up ribbon control alignment.

**Architecture:** All changes are gated under `@media (max-width: 1365px)` + the compact JS path; desktop (≥1366px) stays byte-identical. The radio panel becomes an absolute overlay (taken out of grid flow) so the reader keeps full width; the form-viewer child webview hides via Tauri's `Webview.hide()` while the drawer is open. The collapsed sidebar rail stays in the grid at all times and the expanded labeled nav is a separate absolute flyout, so panes never shift.

**Tech Stack:** React 19 + TypeScript, Tauri 2 (`@tauri-apps/api/webview`), Vitest + @testing-library/react + jsdom, CSS (no framework). Tests assert on `?raw`-imported CSS strings (jsdom can't compute layout/media queries) plus App-level mounts.

**Spec:** `docs/superpowers/specs/2026-06-08-fzm1-compact-polish-design.md`
**Proven visual values:** `docs/design/mockups/2026-06-08-fzm1-overlay-behavior.html` (D1), `docs/design/mockups/2026-06-08-fzm1-vertical-rail.html` (D2/D3) — lift widths/shadows/transform values from these; they were measured at 1280×800.

**Branch:** `bd-tuxlink-813d/fzm1-compact-polish` (worktree `worktrees/bd-tuxlink-813d-fzm1-compact-polish`, off `main`).

**Commit trailer (every commit):**
```
Agent: bison-lupine-sycamore
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
```

**Run tests scoped (avoid full sweeps — vitest worker zombies leak RAM; `pkill -9 -f vitest` after):**
`pnpm vitest run src/shell/AppShell.compact.test.tsx`

---

## Task 1: Radio drawer → overlay (D1, visual)

Drop the compact 4th-column reservation; make the radio panel an absolute overlay pinned to the panes' right edge. Reader keeps full `1fr`. Closed = grip strip; open = slide-in + close-tab.

**Files:**
- Modify: `src/shell/compactShell.css` (remove compact `panes--with-dock` column swaps)
- Modify: `src/shell/RadioDrawer.css` (compact `.radio-drawer` → absolute overlay)
- Modify: `src/shell/AppShell.compact.test.tsx` (CSS guard)
- Reference (no change expected): `src/shell/RadioDrawer.tsx`, `src/shell/AppShell.tsx:869` (`.panes` className already toggles `drawer-open`)

- [ ] **Step 1: Write the failing CSS-guard test**

In `AppShell.compact.test.tsx`, add to the compact describe block:

```ts
describe('FZ-M1 overlay drawer (tuxlink-813d)', () => {
  it('compact panes grid no longer reserves a 4th radio column', () => {
    const compactBlock = compactCss.slice(compactCss.indexOf(COMPACT));
    // The push-era 4th-column templates must be gone from compact.
    expect(compactBlock).not.toContain('48px 380px 1fr 44px');
    expect(compactBlock).not.toContain('48px 380px 1fr 400px');
    expect(compactBlock).not.toContain('52px 380px 1fr 44px');
    expect(compactBlock).not.toContain('52px 380px 1fr 400px');
  });
  it('compact radio drawer is an absolute overlay, not a grid column', () => {
    const drawerCss = COMPACT_DRAWER_CSS['./RadioDrawer.css'];
    const compactBlock = drawerCss.slice(drawerCss.indexOf('max-width: 1365px'));
    expect(compactBlock).toContain('position: absolute');
  });
});
```

Add the raw-glob for RadioDrawer.css near the top with the others:

```ts
const COMPACT_DRAWER_CSS = import.meta.glob('./RadioDrawer.css', {
  eager: true, query: '?raw', import: 'default',
}) as Record<string, string>;
```

- [ ] **Step 2: Run it; verify it fails**

Run: `pnpm vitest run src/shell/AppShell.compact.test.tsx -t "overlay drawer"`
Expected: FAIL (compact still contains `1fr 44px` / drawer not `position: absolute`).

- [ ] **Step 3: Remove the compact 4th-column reservation in `compactShell.css`**

Delete the compact `panes--with-dock` / `drawer-open` column-template rules (the block currently setting `grid-template-columns: 48px 380px 1fr 44px` and `... 1fr 400px`). Leave the base compact panes rule (`.layout-b .panes { grid-template-columns: 48px 380px 1fr; }` — becomes `52px ...` in Task 3). Desktop templates in `AppShell.css` are untouched.

- [ ] **Step 4: Make the compact drawer an absolute overlay in `RadioDrawer.css`**

Replace the compact `.radio-drawer` flex-column-in-grid rules with an absolute overlay (lift proven values from the overlay mockup's `.radio` / grip rules):

```css
@media (max-width: 1365px) {
  .radio-drawer {
    position: absolute; top: 0; right: 0; bottom: 0;
    width: 404px; z-index: 5;
    display: flex; flex-direction: column;
    background: var(--surface); border-left: 1px solid var(--border);
    box-shadow: -14px 0 34px rgba(0, 0, 0, 0.5);
    transform: translateX(100%); transition: transform 220ms ease;
  }
  .panes.drawer-open .radio-drawer { transform: translateX(0); }
  /* grip: closed → strip poking out at the right edge; open → close-tab on
     the panel's left edge. Keep the honest session-state dot rules as-is. */
  .radio-drawer-grip {
    position: absolute; left: -16px; top: 50%; transform: translateY(-50%);
    width: 16px; height: 60px; min-height: 56px;
    background: var(--surface-2); border: 1px solid var(--border); border-right: 0;
    border-radius: 7px 0 0 7px; display: grid; place-items: center; cursor: pointer; z-index: 6;
  }
  .radio-drawer-body { display: block; flex: 1 1 auto; min-width: 0; height: 100%; overflow: auto; }
}
```

Keep the existing `[data-session-state=...]` dot color rules and the `prefers-reduced-motion` block. Confirm `.radio-drawer-body` is no longer `display: contents` in compact (it must paint the panel).

- [ ] **Step 5: Run the guard test; verify PASS**

Run: `pnpm vitest run src/shell/AppShell.compact.test.tsx`
Expected: PASS. Then `pkill -9 -f vitest`.

- [ ] **Step 6: Commit**

```bash
git add src/shell/compactShell.css src/shell/RadioDrawer.css src/shell/AppShell.compact.test.tsx
git commit -m "feat(shell): compact radio drawer overlays instead of pushing the reader (tuxlink-813d)"
```

---

## Task 2: Form viewer hides while the drawer is open (D1, behavior)

A child Tauri webview paints above HTML; the open overlay would be punched through by an open form viewer. Hide the webview (not unmount) while the drawer is open via `Webview.hide()`/`show()`.

**Files:**
- Modify: `src/mailbox/WebviewFormViewer.tsx` (accept `suppressed` prop; hide/show + pause ResizeObserver)
- Modify: `src/mailbox/MessageView.tsx` (thread `radioDrawerOpen` → the form-viewer render at line 241; add to `MessageViewProps` at line 659)
- Modify: `src/shell/AppShell.tsx` (pass `radioDrawerOpen={drawerOpen}` into `<MessageView …>` at line 908)
- Test: `src/mailbox/WebviewFormViewer.test.tsx`

- [ ] **Step 1: Write the failing test**

In `WebviewFormViewer.test.tsx`, mock the Tauri `Webview` so `.hide`/`.show` are spies, then assert toggling `suppressed` calls them:

```ts
const hide = vi.fn().mockResolvedValue(undefined);
const show = vi.fn().mockResolvedValue(undefined);
vi.mock('@tauri-apps/api/webview', () => ({
  Webview: vi.fn().mockImplementation(() => ({
    setPosition: vi.fn().mockResolvedValue(undefined),
    setSize: vi.fn().mockResolvedValue(undefined),
    hide, show, close: vi.fn().mockResolvedValue(undefined),
  })),
}));
// invoke() resolves open_webview_viewer; see existing mocks in this file.

it('hides the webview when suppressed, shows it when un-suppressed', async () => {
  const { rerender } = render(
    <WebviewFormViewer formId="Quick_Message" fieldValues={{}} onClose={() => {}} suppressed={false} />,
  );
  await waitFor(() => expect(/* webview constructed */ true).toBe(true));
  rerender(<WebviewFormViewer formId="Quick_Message" fieldValues={{}} onClose={() => {}} suppressed />);
  await waitFor(() => expect(hide).toHaveBeenCalled());
  rerender(<WebviewFormViewer formId="Quick_Message" fieldValues={{}} onClose={() => {}} suppressed={false} />);
  await waitFor(() => expect(show).toHaveBeenCalled());
});
```

(Mirror the existing mock setup already present in `WebviewFormViewer.test.tsx` for `invoke`/`getCurrentWindow`.)

- [ ] **Step 2: Run it; verify it fails**

Run: `pnpm vitest run src/mailbox/WebviewFormViewer.test.tsx -t "hides the webview"`
Expected: FAIL (`suppressed` prop doesn't exist; hide/show never called).

- [ ] **Step 3: Implement `suppressed` in `WebviewFormViewer.tsx`**

Add `suppressed?: boolean` to `WebviewFormViewerProps`. Hold the created webview in a ref. Add an effect that hides/shows on `suppressed` change and pauses repositioning while suppressed:

```ts
const webviewRef = useRef<Webview | null>(null);
const suppressedRef = useRef(suppressed);
useEffect(() => { suppressedRef.current = suppressed; }, [suppressed]);
// after `webview = new Webview(...)`: webviewRef.current = webview;
// in the ResizeObserver callback, bail when suppressedRef.current is true:
//   if (cancelled || !webview || !mountRef.current || suppressedRef.current) return;
useEffect(() => {
  const wv = webviewRef.current;
  if (!wv) return;
  if (suppressed) { void wv.hide().catch(() => {}); }
  else {
    void wv.show().catch(() => {});
    // re-sync bounds to the placeholder after un-hide
    const el = mountRef.current;
    if (el) {
      const r = el.getBoundingClientRect();
      void wv.setPosition(new LogicalPosition(Math.max(0, Math.floor(r.left)), Math.max(0, Math.floor(r.top)))).catch(() => {});
      void wv.setSize(new LogicalSize(Math.max(1, Math.floor(r.width)), Math.max(1, Math.floor(r.height)))).catch(() => {});
    }
  }
}, [suppressed]);
```

Do **not** add `suppressed` to the webview-creation effect's deps (it must not recreate the webview).

- [ ] **Step 4: Thread the prop through `MessageView.tsx`**

Add `radioDrawerOpen?: boolean` to `MessageViewProps` (line 659). Pass it down to the form-viewer render (the function returning `<WebviewFormViewer … />` near line 241) as `suppressed={radioDrawerOpen}`. Thread through any intermediate component the render lives in (add a `radioDrawerOpen` param there too).

- [ ] **Step 5: Pass from `AppShell.tsx`**

At the `<MessageView … />` render (around line 908) add `radioDrawerOpen={drawerOpen}`.

- [ ] **Step 6: Run tests; verify PASS**

Run: `pnpm vitest run src/mailbox/WebviewFormViewer.test.tsx src/mailbox/MessageView.test.tsx`
Expected: PASS. Then `pkill -9 -f vitest`.

- [ ] **Step 7: Commit**

```bash
git add src/mailbox/WebviewFormViewer.tsx src/mailbox/WebviewFormViewer.test.tsx src/mailbox/MessageView.tsx src/shell/AppShell.tsx
git commit -m "feat(mailbox): hide form-viewer webview while the radio drawer is open (tuxlink-813d)"
```

---

## Task 3: Rail stays in the grid; expand is a separate flyout (D3, structural fix)

Root cause of the implosion: `.sidebar.is-expanded { position: absolute }` drops the rail from grid flow. Fix: the collapsed rail never leaves the grid; the expanded labeled nav is a separate absolute flyout with a scrim.

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx` (render a separate `.sidebar-flyout` overlay element instead of mutating `.sidebar` to absolute)
- Modify: `src/shell/compactShell.css` (remove `.sidebar.is-expanded { position: absolute … }`; add `.sidebar-flyout` + `.sidebar-scrim` rules)
- Modify: `src/shell/AppShell.css` (add explicit `grid-column` to message list / reader / radio drawer as belt-and-suspenders)
- Test: `src/shell/AppShell.compact.test.tsx`, `src/mailbox/FolderSidebar.test.tsx`

- [ ] **Step 1: Write the failing CSS-guard test**

```ts
it('expanded rail does NOT make .sidebar position:absolute (grid stays intact)', () => {
  const compactBlock = compactCss.slice(compactCss.indexOf(COMPACT));
  expect(compactBlock).not.toMatch(/\.sidebar\.is-expanded\s*{[^}]*position:\s*absolute/);
  // the expanded nav is its own flyout element
  expect(compactBlock).toContain('.sidebar-flyout');
});
```

- [ ] **Step 2: Write the failing component test**

In `FolderSidebar.test.tsx`:

```ts
it('renders a separate flyout overlay when expanded, keeping the rail mounted', () => {
  render(<FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} onCreateFolder={() => {}} />);
  fireEvent.click(screen.getByTestId('rail-expand-btn'));
  expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();      // rail still there
  expect(screen.getByTestId('sidebar-flyout')).toBeInTheDocument();      // flyout overlay
});
```

- [ ] **Step 3: Run both; verify they fail**

Run: `pnpm vitest run src/shell/AppShell.compact.test.tsx src/mailbox/FolderSidebar.test.tsx -t "flyout"`
Expected: FAIL (`.sidebar-flyout` absent; `sidebar-flyout` testid absent).

- [ ] **Step 4: Restructure `FolderSidebar.tsx`**

Keep the collapsed `<nav className="sidebar">` (rail) always rendered (no `is-expanded` class that flips it absolute). When `railExpanded`, additionally render a sibling flyout + scrim:

```tsx
{railExpanded && (
  <>
    <div className="sidebar-scrim" data-testid="sidebar-scrim" onClick={() => setRailExpanded(false)} />
    <nav className="sidebar-flyout" data-testid="sidebar-flyout" aria-label="Folders and connections">
      {/* full labeled nav: Mailbox items, Folders + create button, Connections accordion —
          move the existing labeled-row markup here; the collapsed rail renders the
          compact tab form (Task 4). Keep outside-pointer-down + Escape dismissal. */}
    </nav>
  </>
)}
```

The existing dismissal effect (outside pointerdown / Escape / select-folder) stays, scoped to the flyout. Keep `navRef` covering both rail + flyout (or move the outside-click check to exclude both).

- [ ] **Step 5: Update `compactShell.css`**

Remove the `.layout-b .sidebar.is-expanded { position: absolute; … }` block and its child overrides. Add:

```css
.layout-b .sidebar-scrim { position: absolute; inset: 0; z-index: 7; background: rgba(0,0,0,0.28); }
.layout-b .sidebar-flyout {
  position: absolute; top: 0; left: 0; bottom: 0; width: 240px; z-index: 8;
  background: var(--surface); box-shadow: 6px 0 24px rgba(0,0,0,0.45);
  overflow: auto; padding: 6px 0;
}
```

(`.panes` is already `position: relative` in `AppShell.css` — the flyout anchors to it.)

- [ ] **Step 6: Belt-and-suspenders explicit columns in `AppShell.css`**

Under the compact media query (or as base rules that don't affect desktop layout since values match), pin the non-sidebar panes so auto-flow can never shift them: give `.layout-b .mlist`/message-list `grid-column: 2`, the reading pane `grid-column: 3`. (Confirm class names against `AppShell.tsx`; use the actual reading-pane/list classes.) Do not pin in a way that changes desktop.

- [ ] **Step 7: Run tests; verify PASS**

Run: `pnpm vitest run src/shell/AppShell.compact.test.tsx src/mailbox/FolderSidebar.test.tsx`
Expected: PASS. `pkill -9 -f vitest`.

- [ ] **Step 8: Commit**

```bash
git add src/mailbox/FolderSidebar.tsx src/shell/compactShell.css src/shell/AppShell.css src/shell/AppShell.compact.test.tsx src/mailbox/FolderSidebar.test.tsx
git commit -m "fix(shell): rail stays in grid; expanded nav is a separate flyout (no grid implosion) (tuxlink-813d)"
```

---

## Task 4: Collapsed rail → vertical-text folder tabs (D2)

Replace the indistinct icon rail with vertical-text tabs (bottom-to-top), vertical count chips, 52px rail. Lift proven CSS from `2026-06-08-fzm1-vertical-rail.html`.

**Files:**
- Modify: `src/mailbox/FolderSidebar.tsx` (collapsed rail renders vertical tabs with reserved count slot)
- Modify: `src/shell/compactShell.css` (rail width 48→52px; vertical-tab rules; base `.panes` compact column `52px 380px 1fr`)
- Test: `src/shell/AppShell.compact.test.tsx`

- [ ] **Step 1: Write the failing CSS-guard test**

```ts
it('compact rail uses vertical-text tabs (bottom-to-top)', () => {
  const compactBlock = compactCss.slice(compactCss.indexOf(COMPACT));
  expect(compactBlock).toContain('writing-mode: vertical-rl');
  expect(compactBlock).toContain('rotate(180deg)');           // bottom-to-top
  expect(compactBlock).toContain('grid-template-columns: 52px 380px 1fr');
});
```

- [ ] **Step 2: Run it; verify it fails**

Run: `pnpm vitest run src/shell/AppShell.compact.test.tsx -t "vertical-text tabs"`
Expected: FAIL.

- [ ] **Step 3: Add the vertical-tab CSS to `compactShell.css`**

Set base compact `.layout-b .panes { grid-template-columns: 52px 380px 1fr; }`. Replace the icon-rail nav-item rules with vertical-tab rules (lift from the mockup):

```css
.layout-b .sidebar .vtab {
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  gap: 6px; min-height: 44px; padding: 12px 0; border-left: 2px solid transparent; cursor: pointer;
}
.layout-b .sidebar .vtab .vlabel {
  writing-mode: vertical-rl; text-orientation: mixed; transform: rotate(180deg);
  font-size: 13px; font-weight: 550; line-height: 1;
}
.layout-b .sidebar .vtab.active { background: var(--surface-3); color: var(--text); border-left-color: var(--accent); }
.layout-b .sidebar .vtab .vslot { flex: 0 0 auto; min-height: 16px; display: grid; place-items: center; }
.layout-b .sidebar .vtab .vcount {
  writing-mode: vertical-rl; text-orientation: mixed; transform: rotate(180deg);
  font-size: 10px; font-weight: 700; color: #1a1206; background: var(--accent);
  border-radius: 9px; width: 16px; min-height: 16px; display: grid; place-items: center; padding: 4px 0;
}
```

- [ ] **Step 4: Render vertical tabs in the collapsed rail (`FolderSidebar.tsx`)**

In the collapsed `<nav className="sidebar">`, render each system + user folder as a `.vtab` with a reserved `.vslot` (containing the `.vcount` chip when a count exists) + a `.vlabel`. Keep `data-testid="folder-<id>"` and the click-to-select + `setRailExpanded(false)` behavior. The `☰` expand button stays. The full labeled markup (sections, `+`, Connections accordion) lives only in the flyout from Task 3.

- [ ] **Step 5: Run tests; verify PASS + structure check**

Run: `pnpm vitest run src/shell/AppShell.compact.test.tsx src/mailbox/FolderSidebar.test.tsx`
Expected: PASS. `pkill -9 -f vitest`.

- [ ] **Step 6: Commit**

```bash
git add src/mailbox/FolderSidebar.tsx src/shell/compactShell.css src/shell/AppShell.compact.test.tsx
git commit -m "feat(mailbox): vertical-text folder tabs replace the indistinct compact icon rail (tuxlink-813d)"
```

---

## Task 5: Ribbon alignment polish (D4)

The compact 44px-min-height bump crowds GridEdit's source segmented control + the folder `+`. Fix spacing/alignment so nothing touches a neighbor or `.dash-divider`.

**Files:**
- Modify: `src/shell/compactShell.css` (ribbon GridEdit cluster alignment)
- Test: `src/shell/AppShell.compact.test.tsx` (light guard) + operator browser-smoke (authoritative)

- [ ] **Step 1: Write a light CSS-guard test**

```ts
it('compact ribbon source segment keeps touch height with alignment', () => {
  const compactBlock = compactCss.slice(compactCss.indexOf(COMPACT));
  expect(compactBlock).toContain('.dash-source-segment');
  // alignment hook present (align-items/gap added for the GridEdit cluster)
  expect(compactBlock).toMatch(/dash-item[^}]*align-items|dash-source-segment[^}]*align-items/);
});
```

- [ ] **Step 2: Run it; verify it fails (or trivially passes), then implement alignment**

Add compact rules so the GridEdit cluster (`.dash-item` containing GridEdit) vertically centers its segmented control, gives the segmented control adequate horizontal gap from the grid value + dividers, and aligns the folder `+` in its section header. Tune exact values during browser-smoke against the mockups.

- [ ] **Step 3: Run the guard; verify PASS**

Run: `pnpm vitest run src/shell/AppShell.compact.test.tsx`
Expected: PASS. `pkill -9 -f vitest`.

- [ ] **Step 4: Commit**

```bash
git add src/shell/compactShell.css src/shell/AppShell.compact.test.tsx
git commit -m "fix(shell): compact ribbon GridEdit/folder-+ alignment (tuxlink-813d)"
```

---

## Task 6: Full gate + Codex adrev + browser-smoke prep

**Files:** none (verification)

- [ ] **Step 1: Typecheck**

Run: `pnpm typecheck`
Expected: clean.

- [ ] **Step 2: Targeted test sweep (shell + mailbox), then reap workers**

Run: `pnpm vitest run src/shell src/mailbox` then `pkill -9 -f vitest`
Expected: green. Confirm `pgrep -f vitest` is empty.

- [ ] **Step 3: Desktop regression guard explicitly**

Run: `pnpm vitest run src/shell/AppShell.compact.test.tsx -t "desktop regression"` then reap.
Expected: PASS (desktop CSS untouched).

- [ ] **Step 4: One Codex adversarial round on the diff**

```bash
cat > /tmp/codex-813d.txt <<'EOF'
Adversarial review of the diff against origin/main in this worktree.
Run `git diff origin/main..HEAD`. Audit specifically for:
- The form-viewer webview hide/show: any path where the webview is left
  hidden after the drawer closes, recreated unnecessarily, or its
  ResizeObserver repositions it while suppressed.
- Grid integrity: any compact state where a pane can still shift columns
  (rail leaving flow, implicit 4th column from the absolute drawer).
- Desktop (>=1366px) leakage: any rule that escaped the @media guard.
Read: src/shell/compactShell.css, src/shell/RadioDrawer.css,
src/mailbox/WebviewFormViewer.tsx, src/mailbox/FolderSidebar.tsx,
src/shell/AppShell.tsx. Output findings as markdown.
EOF
cat /tmp/codex-813d.txt | npx --yes @openai/codex review - 2>&1 | tee dev/adversarial/2026-06-08-fzm1-compact-polish-codex.md
wc -l dev/adversarial/2026-06-08-fzm1-compact-polish-codex.md   # >5 lines = real review
```
Triage findings: fix real ones (new commits), note dispositions for the PR body. (`dev/adversarial/` is gitignored.)

- [ ] **Step 5: Push + open PR (operator browser-smoke is the merge gate)**

```bash
git push
gh pr create --base main --head bd-tuxlink-813d/fzm1-compact-polish \
  --title "[bison-lupine-sycamore] fix(shell): FZ-M1 compact polish — overlay drawer, vertical rail, grid fix (tuxlink-813d)" \
  --body "<summary + Codex dispositions + browser-smoke checklist>"
```

- [ ] **Step 6: Operator browser-smoke checklist (authoritative, before merge)**

Operator runs the converged build on this branch and resizes:
- **1280×800:** radio drawer overlays (reader stays full width); open a received HTML form, then open the drawer → form hidden, no punch-through; close drawer → form returns. Rail = vertical-text tabs, one-tap switch; expand flyout opens over the list with the grid intact (no void). Ribbon GridEdit/`+` not touching.
- **1366×768:** still desktop (no compact).
- **≥1440:** unchanged.

---

## Self-review notes (writing-plans)

- **Spec coverage:** D1 → Tasks 1+2; D2 → Task 4; D3 → Task 3; D4 → Task 5; testing/adrev/browser-smoke → Task 6. Out-of-scope items (`tuxlink-jwgi`, wizard anchor) excluded.
- **Type consistency:** `suppressed` prop (WebviewFormViewer) ↔ `radioDrawerOpen` (MessageView/AppShell) wiring is explicit; `drawer-open` panes class is pre-existing and reused.
- **Known latitude:** exact CSS pixel values (drawer width, shadow, ribbon gaps) are lifted from the verified mockups and final-tuned during browser-smoke — intentional, not a placeholder.
