# Handoff — 2026-05-20 — Mock D rebuild (pika-glade-bluff)

**From:** `pika-glade-bluff`. **For:** a fresh session to EXECUTE the v0.0.1 main-UI rebuild to **Mock D**.
**Branch:** `bd-tuxlink-cbz/fidelity-polish` (worktree `worktrees/bd-tuxlink-cbz-fidelity-polish/`), off `origin/feat/v0.0.1`. Pushed. Latest: `09c910c`.
**bd:** `tuxlink-yd4` (P1, the rebuild decision — claim it).

---

## 0. TL;DR

The v0.0.1 main UI was built to the **synthesis** layout (sidebar + ribbon + session-log + dock) and a flat warm-neutral palette. The operator put it next to the approved mock and rejected it: *"great value at best… bland, disproportionate, generally incorrect."* They chose to rebuild to **Mock D (Mail.app-minimal)** literally. Your job: make the real app **be** mock-d. The decision is locked; the strategy is written; the foundation (palette) is ported. You execute topology → rows → reading-pane → status-bar → font → fixture → **real-app validation**.

## 1. ⚠️ READ-FIRST GATE — the mistake that must not repeat

I validated the UI against a **dev gallery rendered in Chromium (Playwright)** with synthetic data. It looked great there. The **real compiled app is WebKitGTK** (Tauri on Linux) with an **empty backend**, and it looked nothing like the mock. The gallery is a *lying proxy*. **I deleted it on purpose.**

**Your validation MUST be the real app:**
- Run it: `cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-cbz-fidelity-polish && pnpm tauri dev` (absolute path; first build is slow — own Rust `target/`).
- Screenshot it (Wayland): bring the window to front, then `grim /home/administrator/Code/tuxlink/dev/scratch/realapp.png` (or `sleep 3 && grim …` then click the window). Wayland socket is `wayland-0`, `XDG_RUNTIME_DIR=/run/user/1000`. Read the PNG and **put it next to** `docs/design/mockups/images/mock-d-mailapp-minimal.png`.
- The mock is **HTML/CSS source**, not just a PNG: `docs/design/mockups/2026-05-17-mocks-v1-four-directions.html` (`MOCK D` block ~line 1533; shared `:root` + classes lines 7–890). **READ THE WHOLE FILE** (it's ~25.7k tokens — read in 2 chunks; do NOT bail to grep like I did).

## 2. The locked decision (`tuxlink-yd4`)

v0.0.1 adopts **Mock D**: tabs + 2-pane, **no dashboard ribbon, no folder sidebar, no default session-log pane**. This SUPERSEDES design-doc §3 decision #4 (synthesis layout) — banner already added at `docs/design/v0.0.1-ux-mockups.md` §3. Decisions #2 (compose = separate window) and #5 (modem) still stand. **Do not re-add the synthesis chrome.** A formal ADR superseding §3 is a follow-up.

## 3. Codex strategy (full transcript: `dev/adversarial/2026-05-20-cbz-fidelity-strategy-codex.md`, gitignored)

Verdict: *source-of-truth + topology mismatch, not a color pass.* **Port the mock CSS/structure wholesale; keep the React data plumbing.** Highest-leverage order:

1. **Lock the decision** (DONE — yd4 + §3 banner).
2. **Change topology before colors** — removing ribbon/sidebar/session-log and restoring `420px 1fr` panes fixes more "incorrect feeling" than any palette tweak.
3. **Port tokens + elevation exactly** (DONE — see §4).
4. **Port row + reading-pane density/type exactly.**
5. Then secondary: tab counts, status-bar content, session-log access, tests/screenshots.

## 4. What's DONE this session

- **Decision locked**: `tuxlink-yd4` filed; §3 superseded banner committed.
- **Foundation ported** (`09c910c`): `src/App.css` now carries the mock's **exact cool-slate `:root`** (verbatim) + the elevation ladder + `--sans`/`--mono`, with legacy `--tux-*` **aliased** to the real tokens (no re-interpretation). Existing components recolor via the aliases. Gates green: tsc + `pnpm build`. (Vitest unaffected since last green run = 291.)
- **Inter NOT bundled** — `--sans` leads with Inter but the Pi lacks it (falls back to system sans = part of the "generic" feel). A `@font-face` was intentionally NOT added (Vite fails the build on an unresolvable CSS `url()`); see the note in App.css.
- **Gallery deleted** (see §1).

The mock's exact tokens (now in App.css): `--bg #0d1318 / --surface #141c23 / --surface-2 #1a2330 / --elevated #1e2832 / --border #1f2832 / --border-strong #2c3744 / --border-soft #1a2028 / --text #e4ebf2 / --text-dim #94a0ad / --text-faint #5d6975 / --accent #f59f3c / --accent-2 #ffba6e / --accent-soft rgba(245,159,60,.12) / --unread-dot #ffd166 / --success #5dd6a0 / --error #ee6b6b / --info #6bb8ee / --form-tag #c084fc`.

## 5. The execution plan (with exact mock specs)

**Step A — Topology (biggest fix).** Reshape `src/shell/AppShell.tsx` + `AppShell.css` to mock-d. Tauri gives the native titlebar + native menu bar (`menu.rs`), so render only the mock's `layout-D` part:
- grid rows: `tab-strip (auto) | panes (1fr) | statusbar (auto)`, height 100vh.
- **tab-strip**: tabs for folders (mock shows `Inbox [3]` / `Sent [87]`; include the functional folders Inbox/Outbox/Sent/Drafts as tabs with counts). Active tab styling per mock `.tab-strip`/`.tab` (HTML lines 290–330).
- **panes**: `grid-template-columns: 420px 1fr` → MessageList | MessageView.
- **statusbar**: status-dot + state · callsign · grid · `v0.0.1` (right). Mock `.statusbar` HTML ~line 720.
- **Remove** `DashboardRibbon`, `FolderSidebar`, `SessionLog` from the default composition. Keep the component files (reused/parked).
- **Session log** → behind `View → Session Log` (`menu:view:session_log` event already wired in `menu.rs` + AppShell listened for `menu:view:status_bar`). Render `SessionLog` only when toggled on. Not default pixels.
- **Status data**: callsign/grid/connection come from `config_read` + `backend_status` (logic currently in `DashboardRibbon` via `src/shell/useStatus.ts` formatters — move that fetch into AppShell/StatusBar). Rework `src/shell/StatusBar.tsx` to render dot+state·callsign·grid·version.
- This breaks `AppShell.test.tsx` (asserts synthesis regions) — rewrite for the new structure (TDD).

**Step B — Rows.** Rework `src/mailbox/MessageList.tsx` `MessageRow` to the mock 3-line grid (HTML `.row` lines 397–487): row 1 `from` (unread-dot + sender, 13px/600) + `date` (right, 12px faint tabular); row 2 `subject` (14px/600; read = 400/dim) with inline `.form-tag` badge + `.size` (right); row 3 `preview` (12px faint, ellipsis). Padding `11px 18px`; selected = `--accent-soft` bg; unread-dot 7px `#ffd166` with glow `box-shadow: 0 0 6px rgba(255,209,102,.6)`. **`MessageMeta` has no `preview`** — add a `preview`/`snippet` field (fixture-only is fine for now; backend `snippet` is a follow-up). Use `bodySize` for `.size` immediately. Rewrite the row tests.

**Step C — Reading pane.** Rework `src/mailbox/MessageView.tsx` DOM to mock order (HTML lines 1623–1657 + CSS 489–557): `.actions` FIRST (Reply primary amber + Reply All + Forward + Print) → `h1.subject-line` (22px/600, `-0.01em`, margin-bottom 18px) → `dl.msg-meta` (grid `60px 1fr`; `dt` uppercase 10px faint; `dd` 13px dim; `.addr` mono bright; From/To/Date) → `pre.msg-body` (14.5px/1.65, `--sans`). Pane padding `28px 32px`. Keep the existing reply→compose wiring (`replyActions.ts` — it's good; just match the markup/labels).

**Step D — Font.** Fetch Inter **variable** woff2 → `src/fonts/` (or `public/fonts/`), add `@font-face` in App.css. This materially lifts the "refined vs generic" feel.

**Step E — Dev fixture.** The real app is empty (no Pat backend; `tuxlink-22l` stubbed) so you can't *see* rows/reading-pane without data. Add a **dev-only** fixture: e.g., a dev flag that feeds sample `MessageMeta[]` (with `preview`) + a sample `ParsedMessage` into the mailbox hooks when there's no backend (or mock the IPC in dev). REQUIRED to validate density/type against the mock.

**Step F — Validate in the REAL app** (§1) and iterate until the grim shot matches `mock-d-mailapp-minimal.png` on palette/density/proportion/type. Then rewrite remaining tests; run `pnpm exec vitest run` + `tsc` + `pnpm build`.

## 6. Pre-existing real-app blockers (fix EARLY so you can see the rebuild)

- **Window default 800×600 is too small** (`src-tauri/tauri.conf.json` `app.windows[0]`) — the panes need ≥~900px so it clips. Bump to ~`1200×820` + add `minWidth`. **Gotcha:** `tauri-plugin-window-state` (`lib.rs:40`) persists geometry (incl. visibility) for ALL windows — a config bump may be overridden by saved state; clear `~/.local/share`/`~/.config` window-state or set `min_inner_size` programmatically, and consider excluding `VISIBLE` from the main window's StateFlags.
- **`tuxlink-9zd` (P1) — tray strands the window.** On this Wayland/`wf-panel-pi` session the tray icon doesn't appear at all; close-to-tray (`lib.rs` CloseRequested → `window.hide()`) then leaves the window unrecoverable (process alive, no icon). Recovery: `pkill -f target/debug/tuxlink` then relaunch. Fix per the issue (minimize-fallback / verify-tray-first / exclude VISIBLE). **While testing: don't click the window X; quit via the terminal.**

## 7. Branch / PR / bd state

- **Branch** `bd-tuxlink-cbz/fidelity-polish` has, in order: `f0c5be1` (synthesis CSS + reply logic), `adbbf4b` (Codex P2 reply fixes), `09c910c` (this session's foundation + §3 supersession). **PR #86** is open against `feat/v0.0.1` but its body describes the *superseded* synthesis fidelity work — **DO NOT MERGE as-is.** Continue the rebuild on this branch (the synthesis CSS gets overwritten; **reuse** `src/mailbox/replyActions.ts` + its tests — the reply→compose logic is sound), then update the PR body. (Alternatively: fresh branch off `feat/v0.0.1`, cherry-pick `replyActions` + `09c910c`, close #86. Your call.)
- **bd issues** (no Dolt remote configured → they live in local Dolt + the main-checkout `.beads/issues.jsonl`, which is **uncommitted in the main checkout** — enumerated here so nothing is lost):
  - `tuxlink-yd4` (P1) — THE rebuild decision (claim it).
  - `tuxlink-9zd` (P1) — tray strands window on Wayland.
  - `tuxlink-8za` (P2) — selectable color schemes + night/tactical modes (operator request; the `--tux-*`→real-token aliasing makes themes a token-set swap).
  - `tuxlink-cbz` (P1, in_progress) — original "visual fidelity"; its synthesis-CSS half is superseded by yd4; the mock-d rebuild fulfills its "match the mock" intent.

## 8. Working-tree / worktree state

- Worktree `worktrees/bd-tuxlink-cbz-fidelity-polish/` (claimed by yd4/cbz). `node_modules` installed. Clean tree at handoff (only the two committed files changed since `adbbf4b`).
- The operator may still have `pnpm tauri dev` running on `:1420`. Gallery files removed. Codex transcripts in `dev/adversarial/` (gitignored).
- Pushed: yes (`09c910c` on origin). Handoff commit follows.
EOF