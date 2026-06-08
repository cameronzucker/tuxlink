# FZ-M1 compact-shell polish — design (tuxlink-813d)

**Status:** Approved (operator brainstorm 2026-06-08, agent `bison-lupine-sycamore`)
**Follow-up to:** `tuxlink-h7q7` (FZ-M1 responsive shell), merged to `main` as `5f3be81` (PR #464).
**Branch:** `bd-tuxlink-813d/fzm1-compact-polish` off `main`.

## Context

The FZ-M1 responsive/compact shell shipped and merged. An operator browser-smoke of
the converged build (main, 2026-06-08) found four defects that block calling the
compact shell alpha-quality. This document specifies the polish pass that fixes
them. The shipped behavior being changed:

- The radio panel was a **push drawer** — a collapsible 4th grid column that shrank
  the reading pane to ~452px when open. The push design was chosen to avoid a child
  Tauri webview (the form viewer) painting over an HTML overlay.
- The collapsed folder sidebar was a **48px icon rail** — folder labels clipped to
  icons, with a tap-to-expand overlay that mutated the sidebar to `position: absolute`.

## Problems (operator smoke, 2026-06-08)

1. **Tiny reading pane is operationally useless.** The push drawer shrinks the reader
   instead of overlaying it. The operator wants the radio panel to **overlay** —
   float over the reader, dismissed when done — so the reader is never shrunk.
2. **Grid implodes when the rail expands.** Expanding the rail mutates `.sidebar` to
   `position: absolute`, removing it from the grid's auto-placement flow. The three
   remaining grid items shift one column left: the message list collapses into the
   48px first track (disappears), the reader takes the list's old 380px track, and
   the reader's old 1fr track becomes an empty negative-space void.
3. **Collapsed-rail controls are not discoverable.** Four of the five system folders
   share the same `▢` icon, so the icon rail cannot distinguish Sent / Outbox /
   Drafts / Archive. The `+` new-folder control is a contextless bare icon, and the
   connection rows are unlabeled.
4. **Ribbon controls touch / overlap.** The compact `min-height: 44px` bump on the
   GridEdit source segmented control and the folder `+` button crowds their neighbors
   and the ribbon dividers in the tight flex ribbon.

## Design

All compact behavior remains gated under `@media (max-width: 1365px)` and the
compact-only JS path. Desktop (≥1366px) stays byte-identical; the existing
desktop regression-guard test (`AppShell` compact CSS scaffold) must still pass.

### D1 — Radio drawer: overlay (replaces push)

**The radio panel overlays the reading pane; the reading pane is never shrunk.**

- Compact: the radio panel mount becomes an **absolute overlay** pinned to the
  right edge of `.panes` — full panes-height, fixed ~400px width, drop shadow,
  slide in/out via `transform`. The `.panes` grid in compact reserves **no** 4th
  column for the panel; the reading pane keeps its full `1fr` width underneath.
  (The `panes--with-dock` / `drawer-open` grid-template column swaps from
  `compactShell.css` are dropped inside the media query; desktop keeps them.)
- **Closed state:** a thin grip strip (~14px) at the right edge carries the honest
  session-state dot (idle / connecting-amber / connected-green / error). One tap
  opens.
- **Open state:** the grip becomes a close-tab pinned to the panel's left edge.
  Focus management from `tuxlink-h7q7` is retained (focus into panel body on open,
  back to grip on close).
- **Desktop unchanged:** the panel remains the 4th grid column via the existing
  `display: contents` wrapper (no remount across the breakpoint).

**Form-viewer coexistence (the one case overlay cannot paint over).**
A received HTML form renders in a child Tauri `Webview` (`WebviewFormViewer`)
pinned over a placeholder div in the reading pane; a child webview always paints
above parent HTML, so the floating panel would be punched through by an open form.

Decision: **hide the form webview entirely while the drawer is open** (operator call
— yielding/shrinking the form was judged kludgy). On drawer close, the form returns.

- Mechanism: thread a `radioDrawerOpen` boolean from `AppShell` (which owns
  `drawerOpen`) → `MessageView` → `WebviewFormViewer`. When true, the viewer
  **hides** its webview without unmounting (so the loopback session and bound form
  state survive): prefer the Tauri `Webview` hide API if available, else size-to-zero
  / move off-screen, and pause the `ResizeObserver` repositioning while hidden. On
  false, restore position/size from the placeholder rect.
- Do **not** unmount/recreate the webview on drawer toggle — recreation reloads the
  form (re-binds field values, loses scroll) and re-binds the loopback port.

**Abort reachability (`tuxlink-jwgi`, carried not built).** With overlay, a
mid-session Abort is one tap away (open drawer → Abort in the panel); the honest
amber grip dot cues an in-progress session. A zero-expand ribbon abort for all
transports is an operator-owned radio-UX/safety call and is **not** built
speculatively (per `feedback_no_tuxlink_added_safeguards`). Note the ribbon already
carries an Abort during the CMS connect phase (`DashboardRibbon`, rendered while
`connecting`); D1 does not change that.

### D2 — Collapsed rail: vertical-text folder tabs

**The collapsed sidebar shows folder names as vertical text — no icon-only rail.**

- Compact: the sidebar is a **52px rail**. Each folder is a vertical-text tab:
  - **Reading direction: bottom-to-top** (Outlook spine convention). Implemented via
    `writing-mode: vertical-rl` (honest layout sizing) + `transform: rotate(180deg)`.
    `writing-mode: sideways-lr` is **not** used (WebKitGTK support is unreliable).
  - **≥44px touch height per tab.**
  - **Count chips** (Inbox unread, Outbox queued) rendered in the **same vertical
    orientation** as the labels — no horizontal/vertical alternation — in a reserved
    slot so labels align whether or not a tab carries a count.
  - **Active folder** marked with the accent left-border + raised surface.
- Collapsed rail contents (top → bottom): `☰` expand control; system folders
  (Inbox, Sent, Outbox, Drafts, Archive); user folders; a "Connections ›" tab that
  opens the expand overlay.
- Fit is verified at the target resolution: at 1280×800 the realistic folder set
  (5 system + 1 user + Connections) measures **640px content in a 640px rail — fits
  with zero overflow**. The rail scrolls if more long-named user folders are added
  (acceptable degradation).

This eliminates problem 3 wholesale: real names replace the four identical `▢`
icons; the `+` new-folder moves into the expand overlay under the "Folders" heading
(context restored); unlabeled connection rows move into the expand overlay.

### D3 — Expand overlay + grid-implosion fix (structural)

**The collapsed rail never leaves the grid; expansion is a separate overlay.**

- Root cause of problem 2: `.sidebar.is-expanded { position: absolute }` removed the
  grid item from flow.
- Fix: the **collapsed 52px rail stays in the grid at all times** (its track is
  always present). The expanded labeled navigation is a **separate
  absolutely-positioned overlay element** (~240px) rendered over the message list,
  with a scrim, full labeled nav (Mailbox, Folders + new-folder, Connections
  accordion, Peer-to-peer). Because the rail's grid track is never removed, the
  other panes never shift — no void, no disappearing list.
- Dismissal (retained from `tuxlink-h7q7`): selecting a folder, an outside
  pointer-down, or Escape.
- Belt-and-suspenders: the message-list / reading-pane / radio-overlay panes may
  carry explicit `grid-column` placement so auto-flow cannot shift them even if a
  future change re-introduces an out-of-flow child.

### D4 — Ribbon alignment polish

The compact `min-height: 44px` bump on GridEdit's source segmented control
(`.dash-source-segment`, `.dash-grid-value-btn`, `.dash-set-manually`) and the
folder `+` create button crowds neighbors and dividers in the tight flex ribbon.
Fix the spacing/alignment so no control touches or overlaps a neighbor or a
`.dash-divider`: align the segmented control vertically within the ribbon row,
ensure the GridEdit cluster has adequate horizontal gap, and align the folder `+`
button within its section header. Mechanical CSS; no behavior change.

## Implementation targets (files)

- `src/shell/compactShell.css` — drop the compact `panes--with-dock` 4th-column
  reservation (D1); rail rules become vertical-text tabs (D2); remove
  `.sidebar.is-expanded { position: absolute }` (D3); ribbon alignment (D4).
- `src/shell/RadioDrawer.tsx` / `RadioDrawer.css` — overlay (absolute, slide, grip
  strip / close-tab) instead of push column (D1).
- `src/shell/AppShell.tsx` — `.panes` className/grid for overlay (D1); pass
  `radioDrawerOpen` toward MessageView (D1); expand-overlay state (D3).
- `src/shell/AppShell.css` — `.panes` grid + explicit columns if used (D3).
- `src/mailbox/FolderSidebar.tsx` — vertical-text tab structure + reserved count
  slot (D2); separate expand-overlay element with scrim (D3).
- `src/mailbox/MessageView.tsx` + `src/mailbox/WebviewFormViewer.tsx` — thread
  `radioDrawerOpen`; hide/restore the child webview without unmount (D1).
- `src/shell/DashboardRibbon.tsx` (via AppShell.css/compactShell.css) — ribbon
  alignment (D4); no JSX change expected.

## Testing

- **Unit / CSS-string guards** (jsdom can't compute layout): assert the compact
  `.panes` grid no longer reserves a drawer column; the rail tab structure; the
  desktop regression guard still asserts desktop CSS unchanged.
- **App-level mount test** (production path, per
  `feedback_test_production_mount_path_not_just_units`): drawer-open hides the
  form-viewer webview (mock the Tauri webview); expand overlay renders over the list
  without unmounting the list/reader (grid integrity).
- **Authoritative real-viewport check is the operator browser-smoke** of the
  converged build at 1280×800 (overlay floats, reader full-width, rail readable,
  expand intact, form hidden under drawer), 1366×768 (still desktop), ≥1440
  (unchanged). CSS-string assertions are a first guard only.
- One **Codex adrev round** on the implementation diff (the webview-hide and the
  grid-no-shift change have non-obvious failure modes).

## Out of scope

- `tuxlink-jwgi` — zero-expand ribbon abort (operator-owned).
- Wizard inline Register anchor (separate filed issue).
- Any desktop (≥1366px) change.
- Any RF / transmit path change.

## Mockups

Approved visual mockups (visual-companion, 2026-06-08), persisted at:
- `docs/design/mockups/2026-06-08-fzm1-overlay-behavior.html` (D1)
- `docs/design/mockups/2026-06-08-fzm1-vertical-rail.html` (D2/D3)
