# Handoff — 2026-05-20 — Mock D rebuild EXECUTED (hemlock-raven-wren)

**From:** `hemlock-raven-wren` (continues `pika-glade-bluff`).
**Branch:** `bd-tuxlink-cbz/fidelity-polish` (worktree `worktrees/bd-tuxlink-cbz-fidelity-polish/`), off `feat/v0.0.1`. **Pushed.** Latest: `a68d41b`. **PR #86** updated (body now reflects Mock D).
**bd:** `tuxlink-yd4` (rebuild) + `tuxlink-9zd` (tray) both claimed/in_progress — **deliberately NOT closed** (see §3).

---

## 0. TL;DR

The v0.0.1 main UI is **rebuilt to Mock D** and validated in the **real Tauri/WebKitGTK app** (grim vs `mock-d-mailapp-minimal.png`). The pika-glade-bluff brief is fully executed: topology → rows → reading pane → status bar → Inter → dev fixture → real-app validation → tests. Gates green (vitest 318, tsc, build). The window-size + tray-strand bug (`tuxlink-9zd`) is fixed in code. **Two operator gates remain before close/merge** (§3) — neither is implementable by an agent.

## 1. What was done (commits, newest first)

- `a68d41b docs(adr)` — **ADR 0012** records the Mock D supersession; design-doc §3 banner now points to it; ADR README indexed.
- `cf679e5 feat(ui)` — **Mock D rebuild** (`tuxlink-yd4`):
  - **Topology** (`AppShell.tsx`/`.css`, new `TabStrip.tsx`): tab strip (Inbox/Outbox/Sent/Drafts + counts) · `420px 1fr` panes · minimal status bar. Ribbon/sidebar/dock removed from default (files **parked**, not deleted). Session log behind **View → Session Log** (`menu:view:session_log`).
  - **Rows** (`MessageList.tsx`): 3-line mock `.row` (unread-dot+sender·date / subject+form-tag+size / preview). `formatRowDate` = compact smart-UTC. `preview`+`formTag` added to `MessageMeta` (optional; fixture-supplied, backend follow-up).
  - **Reading pane** (`MessageView.tsx`/`.css`): actions (Reply amber·Reply All·Forward·**Print**) → h1 → dl.msg-meta (From/To/Date/Via) → pre.msg-body. Reply→compose wiring reused.
  - **Status bar** (`StatusBar.tsx` now **pure-props** + `useStatusData` hook): ● state · callsign · grid / version. Status fetch lifted to AppShell (single poll; also feeds the window title `Tuxlink — <Folder> · <callsign>`). +`formatStatusState`.
  - **Font**: Inter variable (latin subset) bundled at `src/fonts/` + `@font-face` (CSP `default-src 'self'` → must be same-origin, offline-first).
  - **Dev fixture** (`devFixture.ts`): mock-content sample data + pre-selected K0SWE, gated on `import.meta.env.MODE==='development'` → vite dev server ONLY (off in tests + release; tree-shaken from `vite build`).
- `39da45e fix(window)` — **`tuxlink-9zd`**: Linux `minimize()` (not `hide()`) on close → window stays in compositor list, recoverable even when the SNI tray never registers; exclude `StateFlags::VISIBLE` from window-state; tray "Show Window" is unconditional (`unminimize+show+focus`, never `is_visible()`); default window 800×600 → **1200×820** + min sizes.

## 2. Validation (the lying-proxy lesson, honored)

Validated against the **real compiled Tauri/WebKitGTK app** via `grim`, NOT a Chromium/Playwright gallery (that proxy caused the original miss). Screenshots: `dev/scratch/realapp-mockd-0{1,3}.png` + `realapp-mockd-crop.png` (**gitignored** — local only). The grim shot matches the mock on topology, palette, density, proportion, type, the 3-line rows (incl. ICS-213 form-tag), and the reading pane.

To capture: `WAYLAND_DISPLAY=wayland-0 XDG_RUNTIME_DIR=/run/user/1000 grim dev/scratch/x.png`. Compositor is **labwc + wayvnc** (headless 1920×1080). A **fresh `tauri dev` launch focuses its window** (that's how to get a clean frontmost grab); HMR does NOT raise/refocus, and there's no `swaymsg`/`wlrctl`/`wtype` installed to raise it manually.

Gates: `pnpm exec vitest run` → **318 passed**. `pnpm exec tsc --noEmit` → clean. `pnpm build` → clean (Inter woff2 fingerprinted into `dist/assets`).

## 3. ⚠️ TWO OPERATOR GATES REMAIN (why yd4/9zd are NOT closed)

1. **Operator visual sign-off on Mock D fidelity** → then close `tuxlink-yd4` + `tuxlink-cbz`, merge PR #86. The operator personally rejected the *prior* build at exactly this gate, so an agent must **not** auto-close on a self-judged grim match. The live dev server is running (§4) — the operator can look now.
2. **Operator verification of `tuxlink-9zd`** close-to-tray recoverability on the **real labwc/wf-panel-pi compositor** (agent can't drive the WM) + **X11 regression** check. The code fix is in; the issue stays open until the operator confirms. **While testing: the X now minimizes (Linux) instead of hiding — the window stays in the panel/window list; quit via File→Quit / Ctrl+Q / terminal.**

## 4. Working-tree / runtime state

- **Tree clean**, branch pushed (`a68d41b`), 0 unpushed.
- **A `pnpm tauri dev` server is RUNNING** (I relaunched it for fresh-window validation; log at `dev/scratch/tauri-dev.log`, vite on `:1420`, window PID was 4034081). Left running so the operator has a live app to review. Stop with: `kill -- -$(ps -o pgid= -p $(pgrep -f 'pnpm tauri dev') | tr -d ' ')`.
- Worktree untracked: `dev/scratch/` (now gitignored), `node_modules`, `src/fonts/` (committed).
- bd: `tuxlink-yd4`, `tuxlink-9zd` in_progress (claimed). No Dolt remote → issue state in local Dolt + committed `.beads/issues.jsonl`.

## 5. Minor follow-ups (non-blocking, operator's call)

- Reading-pane Date shows **UTC only** (`… UTC`); the mock also shows local (`· 06:45 CDT`). UTC-only kept for emcomm correctness; add local if wanted.
- Action buttons are plain labels (Reply / Reply All / Forward / Print); the mock shows `Reply (⌘R)`. The `⌘` glyph is wrong on Linux (it's Ctrl, and the accelerator is already in the Message menu) so it was dropped. Add `(Ctrl+R)` if the hint is wanted on the button.
- `DashboardRibbon` + `FolderSidebar` are now **dead code** (parked per the brief). If never reused, a later cleanup can delete them.
