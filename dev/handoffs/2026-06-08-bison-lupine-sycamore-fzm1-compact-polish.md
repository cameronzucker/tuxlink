# 2026-06-08 bison-lupine-sycamore — FZ-M1 compact-shell polish SHIPPED (PR #470 merged)

## Summary

Took the FZ-M1 (1280×800) compact-shell polish from brainstorm → spec → plan →
subagent-driven TDD → Codex adrev → **two rounds of operator browser-smoke fixes**
→ **MERGED to main** (PR #470, merge commit `a0c1435`). bd `tuxlink-813d` closed.

## What shipped (all compact-only, `@media (max-width: 1365px)` + `isCompact` JS gate; desktop byte-identical)

- **Radio drawer → overlay** (was a push column shrinking the reader). Form-viewer
  child webview hides (`Webview.hide()`, not unmounted) while the drawer is open.
- **Vertical-text folder rail** replacing the indistinct icon rail; full nav in an
  expand flyout; active indicator is an **Outlook-style underline** under the label.
- **Grid-implosion fix**: rail stays in the grid; expand is a separate absolute flyout.
- **Drawer auto-opens** on modem-mode select + Ctrl+Shift+M (was collapsed off-screen).
- **Grip** → 30px pull-tab + ‹/› chevron, z-index above resize handles (was a 16px sliver).
- **Ribbon**: nowrap the grid value; hide verbose GPS-no-fix status in compact; **uniform
  44px centered value bands** so callsign(SSID picker)/grid(segment)/time/connection
  values align; **`.dashboard` is `height: auto`** in compact so it grows to fit the
  44px touch controls (the fixed 56px clipped them in WebKitGTK).

## Process highlights / learnings

- **Codex cross-provider adrev caught a P1 all 3 Claude reviewers missed**: the rail
  rework changed FolderSidebar *markup* (not just CSS), so the compact rail rendered on
  **desktop** too. Reviewers verified `AppShell.css` untouched but not the rendered
  desktop output. Fixed by gating the rail/flyout behind `isCompact` + restoring the
  original desktop nav verbatim + a desktop regression-guard test.
- **Playwright/Chromium is NOT a faithful proxy for the Tauri WebKitGTK webview.** A
  ribbon that fit cleanly in Chromium *clipped* in WebKitGTK (fixed-height container +
  taller font metrics). Cost ~2 extra smoke rounds. Layout/fit fixes MUST be verified
  against the real app (grim), not Playwright-on-:1420. See memory
  [[feedback_chromium_not_webkitgtk_proxy]].
- **Tauri dev webviews don't wire Ctrl+R**; HMR may not reach the webview. To load new
  frontend code in the operator's running app: **restart `pnpm tauri dev`** (Rust is
  cached → fast). Ctrl+R was a no-op, which is why the operator "saw no change."
- Useful technique: inject a `__TAURI_INTERNALS__` mock via Playwright `addInitScript`
  (return `true` for `get_wizard_completed`, no-op events, mock `config_read` /
  `packet_config_get`) to render the real shell in a browser past the wizard gate —
  good for DOM/logic inspection, but still Chromium, so NOT authoritative for layout.

## Branch / worktree state (READ before disposing)

- **PR #470 MERGED**; remote branch `bd-tuxlink-813d/fzm1-compact-polish` deleted.
- Local worktree `worktrees/bd-tuxlink-813d-fzm1-compact-polish` is on the now
  **merged-dead** local branch. **The operator's `pnpm tauri dev` is (was) RUNNING from
  it** — vite :1420 + the WebKitGTK app. **Do NOT `rm -rf` the worktree while the dev
  server runs from it.** Gitignored-but-stateful: `node_modules` (~291M),
  `src-tauri/target`, `dev/scratch/*.png` (grim/diagnostic screenshots),
  `dev/adversarial/2026-06-08-fzm1-compact-polish-codex.md` (Codex transcript).
  Dispose per ADR 0009 after the operator stops tauri dev.
- Main checkout: `bd-tuxlink-xygm/recover-handoffs` (unchanged; this handoff added).
- `gh pr merge` could not update local `main` (it's checked out in
  `worktrees/bd-tuxlink-qjgx-alpha-logging`) — cosmetic; origin/main is correct.

## Open / carried (not built — by design)

- `tuxlink-jwgi` (P2) — zero-expand ribbon abort. Operator-owned radio-UX/safety call;
  the honest amber grip dot + one-tap-open is the current answer.
- Wizard inline Register anchor (filed P3) — design call.

## Verification

631 frontend tests pass; `tsc --noEmit` clean. Regression guards: desktop gate,
webview-hide, grid integrity, drawer auto-open (full AppShell compact mount), each
ribbon fix. Operator confirmed the final WebKitGTK render after restarting tauri dev.
