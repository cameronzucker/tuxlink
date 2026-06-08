# FZ-M1 responsive shell — design

> Status: **locked** (operator brainstorm 2026-06-07, agent `basalt-mesa-dahlia`, visual-companion session).
> Smoke-walk item 3 (`tuxlink-h7q7`). Brainstorm #4 of 4. Companion help-window responsive is separate (`tuxlink-0gsy`).

## Grounding (verified)

The Tuxlink shell is **fixed-layout** — there is **one** non-print `@media` rule in the entire `src/` tree (in `HelpView.css`); the main shell has none. The panes grid (`src/shell/AppShell.css`):

- `.layout-b .panes`: `200px 380px 1fr` (sidebar | list | reader)
- `.panes--with-dock`: `200px 300px 1fr 400px` (+ radio dock)
- `.panes--with-legacy-dock`: `200px 300px 1fr 400px 290px` (+ a 5th column)

On the FZ-M1 (**1280×800, 7", ~216 PPI, touch**), the with-dock layout leaves the `1fr` reader ~**175–380px**, and the legacy-dock layout collapses it to ~90px — the "unusably thin reader" the operator reported. Chrome is also sized for desktop. This is **greenfield responsive work**.

## Decision — Option A: icon rail + radio drawer

Below a **compact breakpoint (~1366px)** the shell enters a compact mode:

1. **Folder sidebar → 36px icon rail.** Icons only (Inbox/Outbox/Sent/Drafts/Contacts/Radio); labels appear on tap/hover or via an expand toggle. Frees ~165px for the reader.
2. **Radio dock → right-side slide-over drawer.** Instead of a permanent 4th/5th grid column, the radio panel becomes a `position:absolute` drawer that slides in from the right via a **grip handle** and tucks away when reading mail — so it **never permanently steals reader width**. Manual open/close (operator chose plain A, not auto-open).
3. **Legacy 5th column retired** in compact mode.
4. **Reader keeps usable width** (~250px in the mock) with mail list + reader always visible — the emcomm "eyes on the inbox while a session runs" case is preserved (drawer open) without starving the reader (drawer closed).

This was chosen over (B) tab-switched panes — the operator wants mail + an in-flight radio session visible together, accepting a ~250px reader vs B's fuller-but-exclusive view.

## Cross-cutting refinements (all primary surfaces)

- **Touch targets ≥ 44×44px** in compact mode (buttons, list rows, menu items, the rail icons, form controls).
- **Chrome text floor 12–14px** at 216 PPI for legibility.
- **Density, not uniform shrink:** tighten wasteful spacing/padding; do not scale the whole UI down (the audit's "elements too large" is about wasted space + desktop sizing, not font scale).
- Apply the compact treatment across **every primary surface**: shell, Compose, Settings, the first-run wizard, HTML forms, the radio panels. (Per `tuxlink-h7q7` "audit every primary UI surface.")

## Components / architecture

- **Breakpoint mechanism:** a `compact` state driven by viewport width (CSS `@media (max-width: ~1366px)` for the static grid/rail/typography; a small `useViewport`/resize hook only where JS state is needed, e.g. the drawer open/close + rail expand). Prefer CSS media queries for the layout; reserve JS for interactive drawer state.
- **`FolderSidebar` compact:** renders icon-rail mode under `compact` (a class), with accessible labels (title/aria + tap-to-expand).
- **Radio drawer:** the radio dock (today a grid column via `panes--with-dock`) becomes an overlay drawer in compact — a `RadioDrawer` wrapper with open/closed state + grip handle + slide transition; the non-compact path keeps the existing column layout unchanged.
- **Typography/touch tokens:** compact-scoped CSS for min hit-area + font floors, applied via the `compact` root class so components don't each re-implement it.

## Error / regression guard
- Desktop (≥ breakpoint) layout must be **unchanged** — compact rules are additive and scoped.
- The drawer must not trap focus or hide the reader content beneath it permanently (closeable; reader reflows to full compact width when closed).

## Testing
- **Visual/component at FZ-M1 viewport** (resize to 1280×800 / ~1280×760 effective): assert the rail collapses, the reader width is usable (not starved), the radio drawer toggles open/closed, the legacy 5th column is absent.
- Touch-target assertions (computed hit-area ≥ 44px) on key controls in compact.
- Desktop snapshot unchanged above the breakpoint.

## Out of scope (v1)
- Phone-width (<768px) layouts — target is the 7" tablet, not handsets.
- The help window's own responsive pass (`tuxlink-0gsy`).
- Re-theming / font-size user preference (separate from responsive density).

## Open items for the implementation plan
- Exact breakpoint value(s) (single ~1366px, or a second tighter one).
- Drawer transition + whether the grip shows session state (a subtle "connecting…" tick on the closed grip is cheap and useful).
- Per-surface audit checklist (which Compose/Settings/wizard/form rules need compact variants).
- Whether the icon rail is tap-to-expand-overlay vs push.
