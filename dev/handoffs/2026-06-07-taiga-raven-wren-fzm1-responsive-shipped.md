# 2026-06-07 taiga-raven-wren — FZ-M1 responsive/compact shell SHIPPED (PR #464)

## Summary

Built the FZ-M1 responsive/compact shell (smoke-walk item 3, `tuxlink-h7q7`) end-to-end with the full `build-robust-features` discipline. **PR #464 is OPEN, ready (not draft), MERGEABLE — awaiting your browser-smoke before merge.** Do not merge unsmokedb; responsive layout needs your eyes at real window sizes.

## Branch / PR / worktree

- **Branch:** `bd-tuxlink-h7q7/fzm1-responsive` (off `origin/main`), pushed, HEAD `497ceb5`, up to date with origin.
- **PR:** https://github.com/cameronzucker/tuxlink/pull/464 — `[taiga-raven-wren] feat: FZ-M1 responsive/compact shell (tuxlink-h7q7)`.
- **Worktree:** `worktrees/bd-tuxlink-h7q7-fzm1-responsive` (clean tree, all committed). Gitignored-but-stateful: `node_modules` (291M), `src-tauri/target` (cargo build), `dev/adversarial/*-codex*.md` (2 Codex transcripts), `dev/scratch/2026-06-07-fzm1-compact-audit-*.{md,json}` (audit synthesis). Keep the worktree until PR #464 merges; dispose per ADR 0009 after.

## What shipped (38 files, +3651/-69 over 8 implementation commits)

Compact mode below `@media (max-width: 1365px)` — **strictly below the 1366px desktop floor**, so desktop ≥1366px is byte-identical (a regression-guard test asserts no leak; `AppShell.css` is unchanged — compact rules live in `compactShell.css`).

- **Radio dock → push drawer** (`RadioDrawer.tsx/.css` + AppShell mount): collapsible 4th grid column, 44px grip / 400px open, **pushes** the reader (not absolute overlay — a child Tauri webview paints above parent HTML, so an overlay would be occluded by the form viewer). Reader ~808px closed / ~452px open vs the old ~300px. Honest grip session-state tick (`deriveDrawerSessionState` switches on `status.kind` — amber during RF connecting, never false-green). Focus moves to panel on open / grip on close.
- **Icon rail** (`FolderSidebar.tsx` + `compactShell.css`): 200px sidebar → 48px rail; tap-to-expand overlay with outside-click/Escape dismiss. Labels **clip-hidden (a11y-safe), not display:none**. Inline `+`/empty-hint controls class-ified.
- **Touch + density on every surface**: shell/ribbon/chrome, mailbox, radio panel interior (incl. the `.radio-panel-segmented` class CF's tabs reuse), Compose + embedded forms, Settings/Theme/About dialogs (DRY close button; ThemeDesigner swatch 36×28→44×44 ×24), wizard, HTML forms (ics309 3→1 col, damage 6→2 col). ≥44px targets, 12–14px floors.
- **Rust** (`compose_window.rs`): two-stage height clamp (pre-creation + post-build) so the 820px default fits the FZ-M1's ~760px usable height — the action bar was clipping off-screen. Pure `clamped_compose_height` + 3 unit tests.

## Verification (all green)

- **181 frontend tests** + **12 Rust tests**; `pnpm typecheck` clean; `pnpm lint:docs` (pre-push) passed. Codex independently re-ran `tsc --noEmit` + `cargo test compose_window` during its post-impl review — both pass.
- CSS-string assertions are a *first* guard (jsdom can't compute layout/media queries). **The authoritative real-viewport check is your browser-smoke.**

## Process / adrev trail (the learning artifact)

audit (7-surface workflow) → plan → **Codex cross-provider adrev R1 (13 findings)** → **Claude 4-lens adrev R2-5 (13 findings)** → converged plan → TDD impl → **Codex post-impl review R2**. The adrev passes caught, before any code: the **overlay→push** pivot (webview occlusion), the **1366→1365** off-by-one, the **a11y label clip**, the **honest grip** state, the `@import`/test-mechanism bug, and the missing radio-interior task. Post-impl Codex flagged one completeness gap (small radio controls: `.radio-panel-btn-sm`, chip-remove `✕`, native radios, help text) — fixed in `497ceb5`. Plan: `docs/superpowers/plans/2026-06-07-fzm1-responsive-shell-plan.md` (has both adrev disposition tables).

## Operator gate — browser-smoke checklist (before merge)

Run `scripts/converge-build.sh` (or `pnpm -C ... tauri dev`) on `bd-tuxlink-h7q7/fzm1-responsive` and resize:
- **1280×800 (FZ-M1):** rail collapses to icons; tap-expand overlay works + dismisses; radio drawer grip opens/closes (push — reader narrows but stays visible, not occluded); reader not starved; ≥44px targets feel right; Compose action bar on-screen.
- **1366×768:** still desktop (no compact) — proves the strict boundary.
- **≥1440:** unchanged from today.
- **Webview:** open a form-viewer message + the radio drawer → form content not occluded.

## Two decisions surfaced (filed, not silently dropped)

1. `tuxlink-jwgi` (P2, deps h7q7) — **collapsed-drawer abort reachability**: Abort is one expand-tap away when the drawer is collapsed mid-session; honest amber grip cues it. Zero-expand ribbon-abort-for-all-transports is a radio-UX/safety call you own — NOT built speculatively (`feedback_no_tuxlink_added_safeguards`). Eyeball during smoke.
2. Wizard inline **Register anchor** (filed P3): can't cleanly hit 44px inline; design call (button vs accept). Low priority (rare path).

## Coordination — `shoal-raven-gorge` (`bd-tuxlink-raez/contacts-favorites`)

Non-overlapping by construction. Whichever PR merges second rebases. Rail derives icons generically from `MAILBOX_ITEMS` (CF's Contacts flows in free); drawer wraps the panel *container* (CF edits the body interior); `.radio-panel-segmented` made compact-correct here so CF's Favorites/Recent/Manual tabs inherit it; AppShell edits are different regions (panes className L845 + drawer state ~L246 + radio-mount wrap vs CF's content-switch L869-929 + selectedFolder L214). As of this session CF had only committed their plan doc — no shared-file code yet.

## bd state

- `tuxlink-h7q7` — in_progress (PR #464 open; stays open until you merge).
- `tuxlink-jwgi` + wizard-anchor issue — open, dep on h7q7.
