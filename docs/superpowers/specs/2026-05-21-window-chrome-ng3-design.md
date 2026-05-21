# Custom dark window chrome (titlebar + menu bar) — design

**Date:** 2026-05-21
**bd issues:** tuxlink-ng3 (P2, bug — primary) · resolves tuxlink-msr (P2, bug — folded in)
**Author:** fen-cypress-arroyo (design), Cameron Zucker (design authority)
**Status:** Draft — pending adversarial review + operator spec review
**Visual direction:** approved by operator 2026-05-21 against the interactive prototype
(`.superpowers/brainstorm/.../chrome-prototype.html`), faithful to the approved Mock B
(`docs/design/mockups/images/mock-b-principles-faithful.png`).

## 1. Context & goal

The window titlebar and the File/Message/Session/Mailbox/View/Tools/Help menu bar
render in the **native GTK gray** theme. The approved Mock B design shows a **dark
chrome** (dark titlebar over a dark menu bar, same cool-slate palette as the app
body). They are native because `tauri.conf.json` has no `decorations: false` and
[`lib.rs:88`](../../../src-tauri/src/lib.rs#L88) installs a native menu via
`app.set_menu(menu)`.

**Goal:** replace the native chrome with HTML chrome faithful to Mock B —
token-driven (so the tuxlink-8za color schemes recolor it automatically), main-window
only, with the menu bar's keyboard accelerators reimplemented so they fire reliably.

**Folded-in fix (tuxlink-msr).** The compose window currently inherits the entire
main-window menu bar because `app.set_menu` installs the menu **app-globally** — every
webview gets it. The same `app.emit("menu", …)` broadcast is also the cause of the
Codex-F7 recursion guard in [`App.tsx:47-73`](../../../src/App.tsx#L47-L73) (a compose
window had to be prevented from spawning nested compose windows off the broadcast).
Moving the menu into the main window's React tree — dispatching actions **in-process,
main-window-only** — removes the native global menu and the global broadcast at once.
The compose window then has no menu to inherit; it gets its own minimal title bar.
This closes tuxlink-msr in the same change. Operator approved folding it in (2026-05-21).

## 2. Architecture

### 2.1 Window decorations

`tauri.conf.json` → `app.windows[0]` (the `main` window) gains `"decorations": false`.
The compose window is built in code
([`compose_window.rs`](../../../src-tauri/src/compose_window.rs)); its `WebviewWindowBuilder`
gains `.decorations(false)` so it, too, renders the HTML title bar (§4) rather than a
native one.

macOS/Windows are out of scope (§9): this is a Pi/Linux appliance and `decorations:false`
removes the native window controls everywhere, so cross-platform chrome would need
platform-conditional control sets we are not building for v0.0.1.

### 2.2 Native menu removed; the `menu:*` vocabulary preserved

Delete the native-menu install (`build_menu` + `app.set_menu` + `wire_menu_events` at
[`lib.rs:84-89`](../../../src-tauri/src/lib.rs#L84-L89)) and the `Menu`/`Submenu`
builders in [`menu.rs`](../../../src-tauri/src/menu.rs). **The `menu:*` string IDs are
kept as the action vocabulary** — they are the stable contract, independent of whether
a native menu or an HTML menu produces them. `menu_event_ids()` (the pure-function
manifest that [`menu.rs:25`](../../../src-tauri/src/menu.rs#L25) exposes for regression
testing) migrates to a TypeScript constant with an equivalent parity test (§7).

### 2.3 HTML chrome components (main window)

Two new React components, rendered at the top of `AppShell` (main window only):

- **`<TitleBar>`** — app icon, "Tuxlink — {folder}" label, drag region, Adwaita-style
  −/□/× window controls. Height ~38px (Mock B).
- **`<MenuBar>`** — the seven menus as click-to-open HTML dropdowns, with the nested
  flyouts (View → Color scheme; Tools → Settings → Privacy). Height ~30px. Items carry
  the `menu:*` IDs and call the dispatcher (§2.4). Styling per the approved prototype:
  elevated-surface dropdowns, accent-soft hover, mono-font accelerator hints,
  right-flyout submenus.

### 2.4 In-process action dispatch (replaces the global broadcast)

A single `dispatchMenuAction(id: MenuActionId)` function in the main window routes each
`menu:*` ID:

- **Frontend-local actions** — view toggles (`menu:view:session_log`,
  `menu:view:status_bar`), folder switch (`menu:mailbox:*`), color scheme
  (`menu:view:scheme:*`): handled in `AppShell` state, exactly as the existing
  [`AppShell.tsx:104-136`](../../../src/shell/AppShell.tsx#L104-L136) `listen('menu')`
  handler does today — refactored to receive from the dispatcher instead of the event bus.
- **Compose open** (`menu:file:new`): invoke `compose_window_open` with a fresh draft id.
  This is the logic currently in [`App.tsx:56-67`](../../../src/App.tsx#L56-L67); it moves
  into the dispatcher. The main-window-label guard + the F7 recursion guard are **deleted**
  — the dispatcher only exists in the main window, so the broadcast hazard is gone.
- **Reply / Reply All / Forward** (`menu:message:*`): the existing reply actions
  ([`replyActions.ts`](../../../src/mailbox/replyActions.ts)), keyed off the current
  selection.
- **Connect** (`menu:session:connect`): the existing `cms_connect` invoke
  ([`AppShell.tsx:84`](../../../src/shell/AppShell.tsx#L84)).
- **Quit** (`menu:file:quit`): invoke a new `app_quit` command (§2.5).
- Remaining items (`menu:session:*`, `menu:tools:*`, `menu:help:*`, `menu:view:raw_log`,
  `menu:view:radio_dock`) dispatch their existing behavior (or remain the no-op/stub they
  are today). **ng3 changes the producer, not the completeness of any action.**

### 2.5 `app_quit` command

The native menu's quit was handled inline in Rust
([`menu.rs:145-148`](../../../src-tauri/src/menu.rs#L145-L148): `app.exit(0)`), because
`PredefinedMenuItem::quit` is unsupported on Linux/muda. With the native menu gone, add a
small `#[tauri::command] fn app_quit(app: AppHandle) { app.exit(0); }`, invoked by File →
Quit and `Ctrl+Q`. This preserves the invariant that **only Quit exits**; the window
close button keeps the app alive (§5).

## 3. Keyboard accelerators (main window)

A `useAccelerators` hook installs one `keydown` listener on the main window mapping combos
→ `menu:*` IDs → `dispatchMenuAction`. This replaces the OS-level accelerators the native
menu used to provide — several of which never fired reliably on the Pi's GTK (the muda
Linux quirk that also breaks native Quit, [`menu.rs:127`](../../../src-tauri/src/menu.rs#L127)).
Moving to a frontend handler makes every accelerator fire reliably.

Locked set (operator-approved 2026-05-21):

| Shortcut | Action | `menu:*` ID |
|---|---|---|
| `Ctrl+N` | New Message | `menu:file:new` |
| `Ctrl+R` | Reply | `menu:message:reply` |
| `Ctrl+Shift+R` | Reply All | `menu:message:reply_all` |
| `Ctrl+P` | Print | `menu:message:print` |
| `Ctrl+Q` | Quit | `menu:file:quit` |
| `Ctrl+Shift+L` | Toggle Session Log | `menu:view:session_log` |
| `Ctrl+Shift+M` | Show Radio Dock | `menu:view:radio_dock` |
| `F5` **and** `Ctrl+Shift+O` | Connect / open session | `menu:session:connect` |

`Ctrl`/`Cmd` is matched via the platform-appropriate modifier (Linux primary; the
`CmdOrCtrl` convention from the native definitions carries over). `F5` and `Ctrl+Shift+O`
both map to the single `menu:session:connect` action.

**Compose window shortcuts are untouched.** The compose window already self-manages
`Ctrl+S` (save draft) and `Ctrl+Enter` (send) — [`Compose.tsx:25`](../../../src/compose/Compose.tsx#L25).
ng3 must preserve them; it adds no main-window accelerators to the compose window.

## 4. Compose window minimal chrome (closes tuxlink-msr)

With `decorations:false` and no inherited menu, the compose window renders its own
**minimal dark title bar**: app/title label ("New Message"), a drag region, and a single
close control. No menu bar; no minimize/maximize (it is a transient window —
`tauri-plugin-window-state` still persists its geometry per
[`lib.rs:40-59`](../../../src-tauri/src/lib.rs#L40-L59)). Close uses the compose window's
existing self-close path (`compose_close_self` / unsaved-draft handling), **not** the
main window's keep-alive close.

## 5. Window controls, drag region, and the borderless-resize risk

- **Controls.** Minimize → `getCurrentWindow().minimize()`. Maximize → `toggleMaximize()`.
  Close → `getCurrentWindow().close()`, which fires the **existing** `CloseRequested`
  handler at [`lib.rs:122-146`](../../../src-tauri/src/lib.rs#L122-L146): on Linux it
  `prevent_close()` + `minimize()` (keep the app + Pat child alive); only Quit calls
  `app.exit(0)`. The custom × therefore behaves exactly as the native × did. Operator
  confirmed this is the intended behavior (2026-05-21).
- **Drag.** The titlebar carries `data-tauri-drag-region` so the window moves when the
  bar is dragged (clickable controls/menus opt out).
- **Resize (⚠️ the one real platform risk).** A borderless GTK/WebKitGTK window loses its
  native resize grips. Mitigation: invisible edge/corner resize-handle elements that call
  Tauri's `startResizeDragging(direction)`. This is a standard custom-chrome pattern but
  behaves differently across compositors; the Pi runs labwc/Wayland. **"Drag-move and
  edge-resize actually work in the real WebKitGTK app" is the must-pass item of the grim
  smoke (§7)** — not an afterthought. If `startResizeDragging` proves unreliable on
  labwc, the fallback is to keep `minWidth/minHeight` and document edge-resize as
  compositor-dependent; this is an explicit adversarial-review question.

## 6. Tokens & theming

The chrome uses **existing design tokens**, no literal colors:

- Titlebar background `--surface-2`; menu bar background `--surface`; body `--bg`
  (the Mock B relationship: titlebar a step lighter than the menu bar).
- Text `--text`/`--text-dim`/`--text-faint`; menu hover `--surface-2`; open/active
  `--accent-soft` + `--accent-2`; dropdown surface `--elevated` + `--border-strong`;
  close-button hover `--tux-danger`.

Because these are the same primitives the tuxlink-8za schemes override
([`App.css:133-185`](../../../src/App.css#L133-L185)), the chrome recolors automatically
under night-red and grayscale with **zero per-scheme chrome CSS**. Verified in the
prototype (night-red + grayscale screenshots).

## 7. Testing

**Unit / component (no device needed):**

- **Menu manifest parity.** A TS `MENU_ACTION_IDS` constant + a test asserting the
  `<MenuBar>` renders exactly that set — the migration of `menu_event_ids()`'s tested
  contract.
- **Accelerator map.** A test over the keydown→ID table in §3 (including `F5` and
  `Ctrl+Shift+O` both → `menu:session:connect`).
- **Dispatcher.** `dispatchMenuAction` routes each ID to the right effect (mock the
  invokes / state setters).
- **Components.** `<TitleBar>` (drag-region attr present, controls call the window API),
  `<MenuBar>` (open/close, flyouts), compose minimal bar.
- **Rust.** `app_quit` registered in the `invoke_handler`; native-menu code removed
  cleanly (lib + integration suites still green).

**Real-app grim smoke (no Wayland click-injection on this Pi — operator's real clicks are
final confirmation):**

1. ⚠️ **Drag-move + edge-resize** of the borderless main window (the §5 risk).
2. Chrome recolors correctly under each color scheme (titlebar + menus + controls).
3. Compose window shows the minimal title bar with **no** menu (msr fixed).
4. Accelerators fire (at least `Ctrl+N` opens compose, `Ctrl+Q` quits, `F5`/`Ctrl+Shift+O`
   connect).
5. Close button keeps the app alive (minimizes on Linux); only Quit exits.

## 8. Process

Custom chrome is a hard-to-undo, cross-cutting change (window config + native-menu
removal + accelerator reimplementation + GTK platform behavior), so per the project's
discipline-triage rule it gets the full **brainstorm → spec → plan → build-robust-features**
treatment, including at least one Codex adversarial round (CLAUDE.md "Extended
capabilities"). This spec is the brainstorm output; `writing-plans` is next.

## 9. Out of scope

- **macOS/Windows chrome.** Linux-first; if ever needed, those platforms keep native
  decorations rather than the Adwaita HTML controls.
- **New menu actions / completing stubs.** ng3 changes the chrome and the action
  *producer*; it does not add features or complete any stubbed action (e.g. Print stays
  whatever it is today).
- **Custom color-scheme editor.** Deferred (its own issue, per the tuxlink-8za handoff).
- **Compose-window accelerators beyond the existing `Ctrl+S`/`Ctrl+Enter`.**

## 10. Open questions for adversarial review

1. `startResizeDragging` reliability on labwc/Wayland borderless windows (§5) — primary risk.
2. `data-tauri-drag-region` behavior on Wayland (does click-through to controls/menus work,
   or do we need explicit `-webkit-app-region`-style opt-outs?).
3. Any remaining consumer of the app-global `menu` event we have not accounted for (grep
   shows only `App.tsx` + `AppShell.tsx`; confirm no test or future hook depends on the
   broadcast).
4. Focus semantics: when the compose window is focused, main-window accelerators must not
   fire (they live on the main window's listener — confirm no global registration leaks).
