# Handoff — 2026-05-21 — fen-cypress-arroyo — ng3 dark chrome + cms-z + native-client connect cluster

## TL;DR
**ng3 (custom dark window chrome) is CODE-COMPLETE and smoke-clean** on
`bd-tuxlink-0ic/native-winlink-client`. Live cms-z testing then surfaced a
**native-client connect cluster** (stall / abort / manual-grid) that is the next
session's primary work. ng3 still owes its **Codex adrev + merge to feat**.

## 🚨 CRITICAL FIRST ACTIONS (next session)
1. **Read this handoff.** Then `bd ready` for the connect cluster.
2. **The connect-stall (`tuxlink-gqo`, P1) is localized — PROBE TRACE captured (operator, 2026-05-21):**
   ```
   Connecting to cms-z.winlink.org:8773 as N7CPZ via Tls ...
   Exchange ended: Connect(Os { code: 110, kind: TimedOut })
   ```
   It dies at **TCP connect** (ETIMEDOUT) — `cms-z:8773` (CmsSsl/TLS port) doesn't
   accept from the Pi; `TcpStream::connect` with no timeout then sits ~75–130s = the
   "stall". **Two-part fix:** (a) add `connect_timeout` (~10–15s, fail fast → clear
   "CMS unreachable"); (b) **root cause — does cms-z expose TLS on 8773 at all?**
   Standard Winlink CMS telnet is **8772 plaintext**. FIRST diagnostic next session
   (30s differential):
   ```bash
   TUXLINK_CMS_HOST=cms-z.winlink.org TUXLINK_CMS_PORT=8772 TUXLINK_CMS_PLAINTEXT=1 \
     cargo run --manifest-path src-tauri/Cargo.toml --bin native_cms_probe
   ```
   If 8772/plaintext connects, the CmsSsl default (8773 TLS) is wrong for cms-z — make
   the dev default transport **Telnet 8772** (or find cms-z's real TLS port).
3. **ng3 close-out gate:** run the **Codex adversarial round** on the branch
   (`npx --yes @openai/codex review --base feat/v0.0.1 "<chrome attack angles>"`)
   **before** merging — cross-provider adrev is non-skippable per project norms
   (see memory `no-carveout-on-cross-provider-adrev`). Then merge **ng3→feat**,
   then **feat→main** (operator approved feat→main only AFTER ng3 lands; main has
   2 unique dependabot commits, so it's a MERGE not a force-push).

## Branch / state
- Branch `bd-tuxlink-0ic/native-winlink-client`, worktree
  `worktrees/bd-tuxlink-0ic-native-winlink-client/`. **Pushed** at session end.
- Tracked tree clean. Gitignored on-disk scratch: `dev/scratch/` (process_icon.py +
  the menu/ribbon/layout/submenu **verification harnesses** + screenshots — local
  reference only), `node_modules/`, `src-tauri/target/`.
- **Gates:** frontend **352** tests green, `tsc` clean, `cargo test` (lib 149 +
  integration suites) green.
- bd state syncs via the git-tracked `.beads/issues.jsonl` (pre-commit re-export);
  no Dolt remote here.

## What shipped this session (committed on the branch)

### ng3 — custom dark window chrome (the headline)
Full pipeline: brainstorm → spec
(`docs/superpowers/specs/2026-05-21-window-chrome-ng3-design.md`) → plan
(`docs/superpowers/plans/2026-05-21-window-chrome-ng3.md`) → subagent-driven build
(Tasks 1–12) → operator grim smoke → re-smoke → fixes.
- `decorations:false`; native menu **removed** (`menu.rs` + `menu_test.rs` deleted);
  HTML chrome in `src/shell/chrome/` (`menuModel`, `dispatchMenuAction`,
  `useAccelerators`, `MenuBar`, `TitleBar`, `ResizeHandles`, `chrome.css`).
- **In-process menu dispatch** replaces the app-global `app.emit` broadcast — fixes
  the compose duplicate-menu (`tuxlink-msr`) AND the Codex-F7 recursion class.
- `app_quit` command; window-control capabilities granted (`default.json`/`compose.json`).
- **Accelerators (operator-locked):** `Ctrl+N` New (now under **Message**, not File),
  `Ctrl+R`/`Ctrl+Shift+R` Reply/Reply All, `Ctrl+P` Print, `Ctrl+Q` Quit,
  `Ctrl+Shift+L` toggle log, `Ctrl+Shift+M` radio dock, **`F5` + `Ctrl+Shift+O`** Connect.
  Reply/Reply All/Forward are **wired** (operator option b) to open a reply window
  from the selection (via `useMessage` + `openReplyWindow`).
- Compose window: own minimal title bar (closes `tuxlink-msr`); **separate window,
  CENTERED** (Wayland can't dock a separate window — see `tuxlink-a9f`).
- **tuxlink icon** (`tuxlink-9dg`): operator PNG was RGB with white rounded corners →
  flood-filled to transparent (`dev/scratch/process_icon.py`); bundle regenerated via
  `tauri icon` (iOS/Android variants pruned); in-app titlebar (26px) + README header.

### Smoke / re-smoke fixes (all committed + visually verified)
- **Layout grid regression** (`.layout-b` 1fr landed on MenuBar → huge gap) — `e122970`.
- Menu fidelity: button chrome reset, dropdown anchored below, submenus collapse
  (hover-only), **hover highlight made perceptible**, **submenu no longer overlaps the
  parent border** (pushed clear + hover bridge).
- **#5** connection error: raw `Error: <reason>` → concise human-readable
  `humanizeConnectionError()` (full reason → session log + title tooltip).
- Icon size bump; New Message → Message menu (`menu:file:new`→`menu:message:new`).

### cms-z + backend
- **Default CMS host → `cms-z.winlink.org`** (`7c50359`) until tuxlink is registered
  (production rejects unregistered SIDs). `TUXLINK_CMS_HOST` still overrides.
  **TODO(register): revert default to `server.winlink.org` post-registration.**
- The app's active backend is **NativeBackend** (Pat is legacy dead-code — see memory
  `project_pat_fully_replaced_native_client`). `live_cms_smoke` is the Pat/legacy probe;
  **use `native_cms_probe`** for the native path.

## OPEN WORK — native-client connect cluster (next session's focus)
| Issue | P | What |
|---|---|---|
| **`tuxlink-gqo`** | P1 bug | **Connect stalls silently.** Diagnosis: `TcpStream::connect` (telnet.rs:111) has **no connect timeout** (~75–130s OS default); 60s read/write timeouts only post-connect; **no per-step progress logging** between "Connecting…" and the result. Fix: `connect_timeout` (~10–15s) + per-step log lines. **Pinpoint via the probe trace first.** |
| **`tuxlink-9z2`** | P1 | **Abort control.** Abort button in the ribbon (shown while connecting) + a `cms_abort` command that `.shutdown()`s the in-flight socket (the connect runs in `tokio::task::spawn_blocking`; shutdown unblocks blocked I/O). |
| **`tuxlink-2y5`** | P2 | **Manual grid entry** (no GPS) — Settings/edit-grid field, 4-char Maidenhead default. Distinct from `tuxlink-2ob` (GPS device). |
| **`tuxlink-dq2`** | P2 | Compose-open slowness (2nd WebKitGTK webview re-parses the eager bundle). Subsumed by `tuxlink-a9f` if the in-window panel is built; else code-split the compose route. |
| **`tuxlink-a9f`** | P3 | Revisit compose as an **in-window docked panel** (Gmail-style). Wayland blocks separate-window docking; the panel also fixes the perf. Needs its own brainstorm + plan; supersedes AMD-6. |
| **`tuxlink-ng3`** | P2 | in_progress — chrome code-complete + smoked. Close on **Codex adrev + merge**. |
| **`tuxlink-msr`** | P2 | Resolved by ng3's compose chrome (no inherited menu). Close when ng3 merges. |
| **`tuxlink-9dg`** | P3 | Icon — done; close on merge. |

## Decisions made this session
- **Compose = separate window, CENTERED** for v0.0.1 (Wayland can't position a separate
  window client-side; that's also why it opened center, not bottom-right). In-window
  panel deferred to `tuxlink-a9f`.
- **Reply-from-menu/accelerator wired** (operator option b) — beyond pure chrome.
- **main update DEFERRED** until ng3 lands on feat, then **feat→main as a merge**
  (main has 2 dependabot commits; not a fast-forward; never force-push the public default).
- **SID forge BELAYED** — operator's Winlink Google-group membership was approved; may
  get tuxlink properly registered instead. cms-z covers dev meanwhile.
- **Pat fully legacy** — native client canonical (memory written).

## Also done (operator-directed, not ng3)
- Rescued the parallel **bd-5jh** modem-rig doc (it was untracked in the main checkout):
  committed in its worktree, pushed, **PR #103** → `feat/v0.0.1`. `tuxlink-5jh` noted,
  close on merge.
- ⚠️ **Leftover untracked main-checkout copies** (now redundant; operator may delete):
  `docs/hardware/modem-test-rig.md` (→ PR #103) and `assets/tuxlink_icon.png` (original
  RGB icon; the transparent version is committed on the branch).
