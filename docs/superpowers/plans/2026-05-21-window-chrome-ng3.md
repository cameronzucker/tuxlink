# Custom Dark Window Chrome (ng3 + msr) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the native gray titlebar + menu bar with token-driven HTML dark chrome faithful to the approved Mock B, main-window only, with reliably-firing keyboard accelerators; give the compose window its own minimal title bar (closing the duplicate-menu bug).

**Architecture:** `decorations:false` removes the native chrome. React `<TitleBar>` + `<MenuBar>` + `<ResizeHandles>` render at the top of the main window's `AppShell`. The menu's `menu:*` action vocabulary is preserved but produced **in-process** (HTML clicks + a keydown handler) and routed through one `dispatchMenuAction` — eliminating the app-global `app.emit` broadcast that caused both the compose duplicate-menu bug (tuxlink-msr) and the F7 recursion guard. The compose window gets a minimal HTML title bar.

**Tech Stack:** Tauri 2 (Rust + WebKitGTK), React 18 + TypeScript, Vitest + @testing-library/react, CSS custom-property design tokens.

**Spec:** `docs/superpowers/specs/2026-05-21-window-chrome-ng3-design.md`

**Source of truth for chrome markup/CSS:** the operator-approved interactive prototype (validated 2026-05-21). Its CSS is ported verbatim into `chrome.css` in Task 7.

---

## File Structure

**Create:**
- `src/shell/chrome/menuModel.ts` — `MENU_TREE`, `MenuActionId`, `MENU_ACTION_IDS`, `ACCELERATORS` (single source of truth).
- `src/shell/chrome/menuModel.test.ts` — manifest parity (replaces Rust `menu_event_ids` test).
- `src/shell/chrome/dispatchMenuAction.ts` — routes a `MenuActionId` to a handler.
- `src/shell/chrome/dispatchMenuAction.test.ts`
- `src/shell/chrome/useAccelerators.ts` — keydown → `MenuActionId` hook.
- `src/shell/chrome/useAccelerators.test.ts`
- `src/shell/chrome/MenuBar.tsx` — renders `MENU_TREE`.
- `src/shell/chrome/MenuBar.test.tsx`
- `src/shell/chrome/TitleBar.tsx` — drag region + window controls.
- `src/shell/chrome/TitleBar.test.tsx`
- `src/shell/chrome/ResizeHandles.tsx` — borderless-window edge resize.
- `src/shell/chrome/ResizeHandles.test.tsx`
- `src/shell/chrome/chrome.css` — token-driven chrome styles (ported from prototype).
- `src/compose/ComposeTitleBar.tsx` — minimal compose title bar.
- `src/compose/ComposeTitleBar.test.tsx`

**Modify:**
- `src-tauri/src/ui_commands.rs` — append `app_quit` command.
- `src-tauri/src/lib.rs:84-89,148-169` — remove native-menu install; register `app_quit`.
- `src-tauri/src/menu.rs` — deleted (Task 12).
- `src-tauri/tests/menu_test.rs` — deleted (Task 12; replaced by `menuModel.test.ts`).
- `src-tauri/tauri.conf.json` — main window `decorations:false`.
- `src-tauri/src/compose_window.rs:139-149` — `.decorations(false)`.
- `src-tauri/capabilities/default.json` — window-control permissions.
- `src-tauri/capabilities/compose.json` — `core:window:allow-start-dragging`.
- `src/shell/AppShell.tsx` — render chrome; replace `listen('menu')` with the dispatcher.
- `src/shell/AppShell.test.tsx` — adapt the menu-event tests to the dispatcher.
- `src/App.tsx:41-73` — remove the `menu:file:new` broadcast listener + F7 guard.
- `src/compose/Compose.tsx` — render `<ComposeTitleBar>`.

---

## Task 1: `app_quit` Rust command

With the native menu gone, File→Quit and `Ctrl+Q` need a command that calls `app.exit(0)` (the native menu did this inline; `PredefinedMenuItem::quit` is unsupported on Linux/muda).

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (append, end of file)
- Modify: `src-tauri/src/lib.rs` (invoke_handler)

- [ ] **Step 1: Add the command to `ui_commands.rs`**

Append to `src-tauri/src/ui_commands.rs`:

```rust
/// Exit the application (tuxlink-ng3). With the native menu removed, File → Quit
/// and the Ctrl+Q accelerator invoke this. Mirrors the native menu's old inline
/// `app.exit(0)` (menu.rs) — `PredefinedMenuItem::quit` is unsupported on
/// Linux/muda, so an explicit command is the canonical pattern. This is the ONLY
/// path that exits the process; the window close button keeps the app alive
/// (lib.rs CloseRequested handler).
#[tauri::command]
pub fn app_quit(app: tauri::AppHandle) {
    app.exit(0);
}
```

- [ ] **Step 2: Register it in `lib.rs`**

In `src-tauri/src/lib.rs`, add to the `tauri::generate_handler![…]` list (after `compose_close_self`):

```rust
            crate::ui_commands::app_quit,             // tuxlink-ng3 (HTML File→Quit / Ctrl+Q)
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compiles clean (no new warnings about `app_quit`).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ui_commands.rs src-tauri/src/lib.rs
git commit -m "feat(chrome): add app_quit command for HTML menu Quit (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Window-control capabilities

The HTML controls, drag region, and resize handles call Tauri window commands that are NOT in `core:default`. Grant them explicitly (least-privilege: only what the chrome uses).

**Files:**
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/capabilities/compose.json`

- [ ] **Step 1: Add window permissions to the main window (`default.json`)**

Replace `src-tauri/capabilities/default.json` with:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window. tuxlink-ng3: custom HTML chrome (decorations:false) needs the window-manipulation commands the native titlebar used to perform — minimize/toggle-maximize/close (controls), start-dragging (data-tauri-drag-region), start-resize-dragging (borderless resize handles), is-maximized (control icon state).",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-close",
    "core:window:allow-start-dragging",
    "core:window:allow-start-resize-dragging",
    "core:window:allow-is-maximized"
  ]
}
```

- [ ] **Step 2: Add the drag permission to the compose window (`compose.json`)**

In `src-tauri/capabilities/compose.json`, add `"core:window:allow-start-dragging"` to the `permissions` array (the compose title bar's drag region needs it; close still routes through the `compose_close_self` app command, so no `allow-close`/`allow-destroy`):

```json
  "permissions": [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "core:window:allow-start-dragging"
  ]
```

- [ ] **Step 3: Verify the schema accepts it**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compiles clean (capability JSON is validated at build; an unknown permission identifier fails the build).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/capabilities/default.json src-tauri/capabilities/compose.json
git commit -m "feat(chrome): grant window-control capabilities for custom chrome (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Menu model — single source of truth + parity test

A data-driven menu model feeds the `<MenuBar>` render, the action-ID manifest, and the accelerator map. The manifest test is the TS migration of the Rust `menu_event_ids()` test, so the `menu:*` contract stays enforced.

**Files:**
- Create: `src/shell/chrome/menuModel.ts`
- Test: `src/shell/chrome/menuModel.test.ts`

- [ ] **Step 1: Write the failing manifest test**

Create `src/shell/chrome/menuModel.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { MENU_ACTION_IDS, ACCELERATORS } from './menuModel';

// Parity with the former Rust menu_event_ids() (menu.rs) — the menu:* vocabulary
// is the stable contract regardless of producer. Order matches the menu layout.
const EXPECTED_IDS = [
  'menu:file:new', 'menu:file:quit',
  'menu:message:reply', 'menu:message:reply_all', 'menu:message:forward', 'menu:message:print',
  'menu:session:connect', 'menu:session:disconnect', 'menu:session:log',
  'menu:session:test_send', 'menu:session:show_transport',
  'menu:mailbox:inbox', 'menu:mailbox:sent', 'menu:mailbox:outbox',
  'menu:view:session_log', 'menu:view:raw_log', 'menu:view:status_bar', 'menu:view:radio_dock',
  'menu:view:scheme:default', 'menu:view:scheme:night-red', 'menu:view:scheme:grayscale',
  'menu:tools:templates', 'menu:tools:rig_control',
  'menu:tools:settings_connection', 'menu:tools:settings_privacy_gps',
  'menu:tools:settings_privacy_position', 'menu:tools:settings_gps',
  'menu:tools:preferences',
  'menu:help:about', 'menu:help:docs', 'menu:help:report_issue',
];

describe('menu model', () => {
  it('exposes exactly the menu:* action vocabulary', () => {
    expect(MENU_ACTION_IDS).toEqual(EXPECTED_IDS);
  });

  it('every accelerator maps to a real action id', () => {
    for (const a of ACCELERATORS) {
      expect(MENU_ACTION_IDS).toContain(a.id);
    }
  });

  it('binds F5 and Ctrl+Shift+O to connect', () => {
    const connectAccels = ACCELERATORS.filter((a) => a.id === 'menu:session:connect');
    expect(connectAccels.map((a) => a.combo).sort()).toEqual(['Ctrl+Shift+O', 'F5']);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm vitest run src/shell/chrome/menuModel.test.ts`
Expected: FAIL — cannot resolve `./menuModel`.

- [ ] **Step 3: Write `menuModel.ts`**

Create `src/shell/chrome/menuModel.ts`:

```ts
// Single source of truth for the menu (tuxlink-ng3). Feeds <MenuBar>, the
// MENU_ACTION_IDS manifest (parity test = the migrated Rust menu_event_ids), and
// the keyboard ACCELERATORS. The menu:* IDs are the stable action vocabulary.

export type MenuActionId = string;

export interface MenuNode {
  /** Action id (leaf). Omitted for separators and pure submenu parents. */
  id?: MenuActionId;
  label?: string;
  /** Display-only accelerator hint (the real binding lives in ACCELERATORS). */
  accel?: string;
  separator?: boolean;
  submenu?: MenuNode[];
}

export interface TopMenu {
  label: string;
  items: MenuNode[];
}

export const MENU_TREE: TopMenu[] = [
  { label: 'File', items: [
    { id: 'menu:file:new', label: 'New Message', accel: 'Ctrl+N' },
    { separator: true },
    { id: 'menu:file:quit', label: 'Quit', accel: 'Ctrl+Q' },
  ] },
  { label: 'Message', items: [
    { id: 'menu:message:reply', label: 'Reply', accel: 'Ctrl+R' },
    { id: 'menu:message:reply_all', label: 'Reply All', accel: 'Ctrl+Shift+R' },
    { id: 'menu:message:forward', label: 'Forward' },
    { id: 'menu:message:print', label: 'Print', accel: 'Ctrl+P' },
  ] },
  { label: 'Session', items: [
    { id: 'menu:session:connect', label: 'Connect', accel: 'F5' },
    { id: 'menu:session:disconnect', label: 'Disconnect' },
    { separator: true },
    { id: 'menu:session:log', label: 'Session Log' },
    { id: 'menu:session:test_send', label: 'Test send' },
    { id: 'menu:session:show_transport', label: 'Show transport' },
  ] },
  { label: 'Mailbox', items: [
    { id: 'menu:mailbox:inbox', label: 'Inbox' },
    { id: 'menu:mailbox:sent', label: 'Sent' },
    { id: 'menu:mailbox:outbox', label: 'Outbox' },
  ] },
  { label: 'View', items: [
    { id: 'menu:view:session_log', label: 'Toggle Session Log', accel: 'Ctrl+Shift+L' },
    { id: 'menu:view:raw_log', label: 'Show Raw Session Log' },
    { id: 'menu:view:status_bar', label: 'Toggle Status Bar' },
    { id: 'menu:view:radio_dock', label: 'Show Radio Dock', accel: 'Ctrl+Shift+M' },
    { separator: true },
    { label: 'Color scheme', submenu: [
      { id: 'menu:view:scheme:default', label: 'Default' },
      { id: 'menu:view:scheme:night-red', label: 'Night / tactical (red)' },
      { id: 'menu:view:scheme:grayscale', label: 'Grayscale' },
    ] },
  ] },
  { label: 'Tools', items: [
    { id: 'menu:tools:templates', label: 'Templates' },
    { id: 'menu:tools:rig_control', label: 'Rig Control' },
    { separator: true },
    { label: 'Settings', submenu: [
      { id: 'menu:tools:settings_connection', label: 'Connection' },
      { label: 'Privacy', submenu: [
        { id: 'menu:tools:settings_privacy_gps', label: 'GPS state' },
        { id: 'menu:tools:settings_privacy_position', label: 'Position precision' },
      ] },
      { id: 'menu:tools:settings_gps', label: 'GPS' },
    ] },
    { id: 'menu:tools:preferences', label: 'Preferences' },
  ] },
  { label: 'Help', items: [
    { id: 'menu:help:about', label: 'About Tuxlink' },
    { id: 'menu:help:docs', label: 'Documentation' },
    { id: 'menu:help:report_issue', label: 'Report Issue' },
  ] },
];

/** Depth-first flatten of every action id, in layout order. */
function collectIds(nodes: MenuNode[]): MenuActionId[] {
  const out: MenuActionId[] = [];
  for (const n of nodes) {
    if (n.id) out.push(n.id);
    if (n.submenu) out.push(...collectIds(n.submenu));
  }
  return out;
}

export const MENU_ACTION_IDS: MenuActionId[] =
  MENU_TREE.flatMap((m) => collectIds(m.items));

export interface Accelerator {
  /** Human label, e.g. "Ctrl+Shift+O". */
  combo: string;
  key: string;        // KeyboardEvent.key, case-insensitive match (e.g. 'n', 'F5')
  ctrl: boolean;      // Ctrl OR Meta (CmdOrCtrl)
  shift: boolean;
  id: MenuActionId;
}

// Operator-locked set (2026-05-21). F5 and Ctrl+Shift+O both fire connect.
export const ACCELERATORS: Accelerator[] = [
  { combo: 'Ctrl+N', key: 'n', ctrl: true, shift: false, id: 'menu:file:new' },
  { combo: 'Ctrl+R', key: 'r', ctrl: true, shift: false, id: 'menu:message:reply' },
  { combo: 'Ctrl+Shift+R', key: 'r', ctrl: true, shift: true, id: 'menu:message:reply_all' },
  { combo: 'Ctrl+P', key: 'p', ctrl: true, shift: false, id: 'menu:message:print' },
  { combo: 'Ctrl+Q', key: 'q', ctrl: true, shift: false, id: 'menu:file:quit' },
  { combo: 'Ctrl+Shift+L', key: 'l', ctrl: true, shift: true, id: 'menu:view:session_log' },
  { combo: 'Ctrl+Shift+M', key: 'm', ctrl: true, shift: true, id: 'menu:view:radio_dock' },
  { combo: 'F5', key: 'F5', ctrl: false, shift: false, id: 'menu:session:connect' },
  { combo: 'Ctrl+Shift+O', key: 'o', ctrl: true, shift: true, id: 'menu:session:connect' },
];
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm vitest run src/shell/chrome/menuModel.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/shell/chrome/menuModel.ts src/shell/chrome/menuModel.test.ts
git commit -m "feat(chrome): data-driven menu model + action-id manifest (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `dispatchMenuAction` — route an action id to a handler

One function maps each `menu:*` id to a call on a handlers object. This replaces the in-window effects of the old `listen('menu')` handler and the App.tsx compose-open listener. Unknown/stub ids are a safe no-op.

**Files:**
- Create: `src/shell/chrome/dispatchMenuAction.ts`
- Test: `src/shell/chrome/dispatchMenuAction.test.ts`

- [ ] **Step 1: Write the failing test**

Create `src/shell/chrome/dispatchMenuAction.test.ts`:

```ts
import { describe, it, expect, vi } from 'vitest';
import { dispatchMenuAction, type MenuHandlers } from './dispatchMenuAction';

function handlers(): MenuHandlers {
  return {
    openCompose: vi.fn(),
    connect: vi.fn(),
    reply: vi.fn(),
    replyAll: vi.fn(),
    forward: vi.fn(),
    toggleSessionLog: vi.fn(),
    toggleStatusBar: vi.fn(),
    selectFolder: vi.fn(),
    setScheme: vi.fn(),
    quit: vi.fn(),
  };
}

describe('dispatchMenuAction', () => {
  it('routes file:new to openCompose', () => {
    const h = handlers();
    dispatchMenuAction('menu:file:new', h);
    expect(h.openCompose).toHaveBeenCalledOnce();
  });

  it('routes file:quit to quit', () => {
    const h = handlers();
    dispatchMenuAction('menu:file:quit', h);
    expect(h.quit).toHaveBeenCalledOnce();
  });

  it('routes session:connect to connect', () => {
    const h = handlers();
    dispatchMenuAction('menu:session:connect', h);
    expect(h.connect).toHaveBeenCalledOnce();
  });

  it('routes view toggles', () => {
    const h = handlers();
    dispatchMenuAction('menu:view:session_log', h);
    dispatchMenuAction('menu:view:status_bar', h);
    expect(h.toggleSessionLog).toHaveBeenCalledOnce();
    expect(h.toggleStatusBar).toHaveBeenCalledOnce();
  });

  it('routes mailbox folder selection with the folder name', () => {
    const h = handlers();
    dispatchMenuAction('menu:mailbox:sent', h);
    expect(h.selectFolder).toHaveBeenCalledWith('sent');
  });

  it('routes scheme selection with the scheme id', () => {
    const h = handlers();
    dispatchMenuAction('menu:view:scheme:night-red', h);
    expect(h.setScheme).toHaveBeenCalledWith('night-red');
  });

  it('routes reply / reply_all / forward', () => {
    const h = handlers();
    dispatchMenuAction('menu:message:reply', h);
    dispatchMenuAction('menu:message:reply_all', h);
    dispatchMenuAction('menu:message:forward', h);
    expect(h.reply).toHaveBeenCalledOnce();
    expect(h.replyAll).toHaveBeenCalledOnce();
    expect(h.forward).toHaveBeenCalledOnce();
  });

  it('is a safe no-op for stub/unhandled ids', () => {
    const h = handlers();
    expect(() => dispatchMenuAction('menu:tools:preferences', h)).not.toThrow();
    expect(() => dispatchMenuAction('menu:help:about', h)).not.toThrow();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm vitest run src/shell/chrome/dispatchMenuAction.test.ts`
Expected: FAIL — cannot resolve `./dispatchMenuAction`.

- [ ] **Step 3: Write `dispatchMenuAction.ts`**

Create `src/shell/chrome/dispatchMenuAction.ts`:

```ts
import type { MenuActionId } from './menuModel';
import type { MailboxFolder } from '../../mailbox/types';
import { isColorScheme, type ColorScheme } from '../colorScheme';

/** Effects the dispatcher can invoke. Supplied by AppShell (closes over state). */
export interface MenuHandlers {
  openCompose: () => void;
  connect: () => void;
  reply: () => void;
  replyAll: () => void;
  forward: () => void;
  toggleSessionLog: () => void;
  toggleStatusBar: () => void;
  selectFolder: (folder: MailboxFolder) => void;
  setScheme: (id: ColorScheme) => void;
  quit: () => void;
}

/**
 * Route a menu:* action id (from an HTML menu click OR a keyboard accelerator)
 * to the matching handler. In-process, main-window only — there is no app-global
 * event broadcast (which is what caused tuxlink-msr + the F7 recursion guard).
 * Unhandled ids (stub actions: tools/help/raw_log/etc.) are intentionally no-ops.
 */
export function dispatchMenuAction(id: MenuActionId, h: MenuHandlers): void {
  switch (id) {
    case 'menu:file:new': h.openCompose(); return;
    case 'menu:file:quit': h.quit(); return;
    case 'menu:session:connect': h.connect(); return;
    case 'menu:message:reply': h.reply(); return;
    case 'menu:message:reply_all': h.replyAll(); return;
    case 'menu:message:forward': h.forward(); return;
    case 'menu:view:session_log': h.toggleSessionLog(); return;
    case 'menu:view:status_bar': h.toggleStatusBar(); return;
    case 'menu:mailbox:inbox':
    case 'menu:mailbox:sent':
    case 'menu:mailbox:outbox':
      h.selectFolder(id.slice('menu:mailbox:'.length) as MailboxFolder);
      return;
  }
  if (id.startsWith('menu:view:scheme:')) {
    const scheme = id.slice('menu:view:scheme:'.length);
    if (isColorScheme(scheme)) h.setScheme(scheme);
    return;
  }
  // Stub / not-yet-wired actions (tools, help, disconnect, raw_log, …): no-op.
}
```

> **Confirmed:** `colorScheme.ts` exports the type `ColorScheme` (`'default' | 'night-red' | 'grayscale'`) and the guard `isColorScheme`, plus `applyColorScheme` / `saveColorScheme` (used by AppShell's `setScheme` handler in Task 10).

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm vitest run src/shell/chrome/dispatchMenuAction.test.ts`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add src/shell/chrome/dispatchMenuAction.ts src/shell/chrome/dispatchMenuAction.test.ts
git commit -m "feat(chrome): in-process menu action dispatcher (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: `useAccelerators` — keyboard shortcuts

A keydown listener (main window) matches the `ACCELERATORS` table and calls back with the matched `menu:*` id. The same dispatcher then runs the action.

**Files:**
- Create: `src/shell/chrome/useAccelerators.ts`
- Test: `src/shell/chrome/useAccelerators.test.ts`

- [ ] **Step 1: Write the failing test**

Create `src/shell/chrome/useAccelerators.test.ts`:

```ts
import { describe, it, expect, vi } from 'vitest';
import { matchAccelerator } from './useAccelerators';

describe('matchAccelerator', () => {
  it('matches Ctrl+N → file:new', () => {
    expect(matchAccelerator({ key: 'n', ctrlKey: true, metaKey: false, shiftKey: false }))
      .toBe('menu:file:new');
  });

  it('treats Meta as Ctrl (CmdOrCtrl)', () => {
    expect(matchAccelerator({ key: 'n', ctrlKey: false, metaKey: true, shiftKey: false }))
      .toBe('menu:file:new');
  });

  it('distinguishes Ctrl+R from Ctrl+Shift+R', () => {
    expect(matchAccelerator({ key: 'r', ctrlKey: true, metaKey: false, shiftKey: false }))
      .toBe('menu:message:reply');
    expect(matchAccelerator({ key: 'R', ctrlKey: true, metaKey: false, shiftKey: true }))
      .toBe('menu:message:reply_all');
  });

  it('matches F5 with no modifier and Ctrl+Shift+O → connect', () => {
    expect(matchAccelerator({ key: 'F5', ctrlKey: false, metaKey: false, shiftKey: false }))
      .toBe('menu:session:connect');
    expect(matchAccelerator({ key: 'o', ctrlKey: true, metaKey: false, shiftKey: true }))
      .toBe('menu:session:connect');
  });

  it('returns null for unbound combos', () => {
    expect(matchAccelerator({ key: 'z', ctrlKey: true, metaKey: false, shiftKey: false }))
      .toBeNull();
    expect(matchAccelerator({ key: 'n', ctrlKey: false, metaKey: false, shiftKey: false }))
      .toBeNull();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm vitest run src/shell/chrome/useAccelerators.test.ts`
Expected: FAIL — cannot resolve `./useAccelerators`.

- [ ] **Step 3: Write `useAccelerators.ts`**

Create `src/shell/chrome/useAccelerators.ts`:

```ts
import { useEffect } from 'react';
import { ACCELERATORS, type MenuActionId } from './menuModel';

interface KeyState {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}

/** Pure matcher: a key event → the bound action id, or null. CmdOrCtrl = Ctrl|Meta. */
export function matchAccelerator(e: KeyState): MenuActionId | null {
  const ctrl = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  for (const a of ACCELERATORS) {
    if (a.ctrl === ctrl && a.shift === e.shiftKey && a.key.toLowerCase() === key) {
      return a.id;
    }
  }
  return null;
}

/**
 * Install the main-window keyboard accelerators (tuxlink-ng3). On a matching
 * combo, prevents the browser default and calls `onAction(id)`. Lives on the
 * main window only; the compose window keeps its own Ctrl+S / Ctrl+Enter.
 */
export function useAccelerators(onAction: (id: MenuActionId) => void): void {
  useEffect(() => {
    function handler(e: KeyboardEvent) {
      const id = matchAccelerator(e);
      if (id) {
        e.preventDefault();
        onAction(id);
      }
    }
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onAction]);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm vitest run src/shell/chrome/useAccelerators.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/shell/chrome/useAccelerators.ts src/shell/chrome/useAccelerators.test.ts
git commit -m "feat(chrome): keyboard accelerator hook + matcher (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Chrome CSS (token-driven)

Port the operator-approved prototype's chrome CSS verbatim into a stylesheet. Uses only existing tokens so the color schemes recolor it automatically.

**Files:**
- Create: `src/shell/chrome/chrome.css`

- [ ] **Step 1: Write `chrome.css`**

Create `src/shell/chrome/chrome.css` (ported from the validated prototype; tokens only — no literal colors):

```css
/* tuxlink-ng3 custom window chrome. Token-driven so the tuxlink-8za color
 * schemes recolor it with zero per-scheme CSS. Ported from the operator-approved
 * prototype (2026-05-21). */

/* ---- Titlebar (drag region) ---- */
.tux-titlebar {
  display: flex; align-items: center; height: 38px;
  background: var(--surface-2); border-bottom: 1px solid var(--border);
  padding: 0 6px 0 12px; user-select: none; position: relative;
}
.tux-titlebar .tux-drag { position: absolute; inset: 0; }   /* data-tauri-drag-region */
.tux-titlebar .tux-app-icon {
  width: 18px; height: 18px; border-radius: 4px; z-index: 1;
  background: linear-gradient(135deg, var(--accent), var(--accent-2));
  display: inline-flex; align-items: center; justify-content: center;
  font-size: 11px; font-weight: 800; color: var(--tux-accent-fg); margin-right: 8px;
}
.tux-titlebar .tux-app-name { font-size: 13px; font-weight: 600; color: var(--text); z-index: 1; }
.tux-titlebar .tux-app-sub { font-size: 12px; color: var(--text-dim); margin-left: 8px; z-index: 1; }
.tux-controls { margin-left: auto; display: flex; gap: 6px; z-index: 1; }
.tux-ctrl {
  width: 24px; height: 24px; border-radius: 50%; border: none;
  display: inline-flex; align-items: center; justify-content: center;
  background: rgba(255, 255, 255, 0.07); color: var(--text);
  font-size: 13px; line-height: 1; cursor: pointer; transition: background .12s;
}
.tux-ctrl:hover { background: rgba(255, 255, 255, 0.14); }
.tux-ctrl.tux-close:hover { background: var(--tux-danger); color: #fff; }

/* ---- Menubar ---- */
.tux-menubar {
  display: flex; height: 30px; background: var(--surface);
  border-bottom: 1px solid var(--border); padding: 0 8px; align-items: center;
  font-size: 13px; color: var(--text); gap: 2px; position: relative; z-index: 50;
}
.tux-menu { padding: 4px 10px; border-radius: 4px; cursor: default; position: relative; }
.tux-menu:hover { background: var(--surface-2); }
.tux-menu.tux-open { background: var(--accent-soft); color: var(--accent-2); }

/* ---- Dropdowns ---- */
.tux-dropdown {
  position: absolute; top: 30px; left: 0; min-width: 230px;
  background: var(--elevated); border: 1px solid var(--border-strong);
  border-radius: 7px; padding: 5px; box-shadow: 0 14px 38px rgba(0,0,0,0.5); z-index: 100;
}
.tux-mi {
  display: flex; align-items: center; gap: 14px; padding: 7px 10px;
  border-radius: 5px; cursor: default; white-space: nowrap; color: var(--text);
  background: none; border: none; width: 100%; text-align: left; font: inherit;
}
.tux-mi:hover, .tux-mi.tux-sub-open { background: var(--accent-soft); color: var(--accent-2); }
.tux-mi .tux-accel { margin-left: auto; color: var(--text-faint); font-size: 12px; font-family: var(--mono); }
.tux-mi .tux-chev { margin-left: auto; color: var(--text-faint); }
.tux-sep { height: 1px; background: var(--border); margin: 5px 6px; }
.tux-mi.tux-has-sub { position: relative; }
.tux-submenu {
  position: absolute; left: 100%; top: -6px; min-width: 215px;
  background: var(--elevated); border: 1px solid var(--border-strong);
  border-radius: 7px; padding: 5px; box-shadow: 0 14px 38px rgba(0,0,0,0.5);
}

/* ---- Resize handles (borderless window) ---- */
.tux-resize { position: absolute; z-index: 200; }
.tux-resize.n { top: 0; left: 6px; right: 6px; height: 4px; cursor: ns-resize; }
.tux-resize.s { bottom: 0; left: 6px; right: 6px; height: 4px; cursor: ns-resize; }
.tux-resize.e { top: 6px; bottom: 6px; right: 0; width: 4px; cursor: ew-resize; }
.tux-resize.w { top: 6px; bottom: 6px; left: 0; width: 4px; cursor: ew-resize; }
.tux-resize.ne { top: 0; right: 0; width: 8px; height: 8px; cursor: nesw-resize; }
.tux-resize.nw { top: 0; left: 0; width: 8px; height: 8px; cursor: nwse-resize; }
.tux-resize.se { bottom: 0; right: 0; width: 8px; height: 8px; cursor: nwse-resize; }
.tux-resize.sw { bottom: 0; left: 0; width: 8px; height: 8px; cursor: nesw-resize; }

/* ---- Compose minimal title bar ---- */
.tux-compose-titlebar {
  display: flex; align-items: center; height: 34px;
  background: var(--surface-2); border-bottom: 1px solid var(--border);
  padding: 0 6px 0 12px; user-select: none; position: relative;
}
.tux-compose-titlebar .tux-drag { position: absolute; inset: 0; }
.tux-compose-titlebar .tux-app-name { font-size: 13px; font-weight: 600; color: var(--text); z-index: 1; }
```

- [ ] **Step 2: Verify it parses (frontend build)**

Run: `pnpm build`
Expected: build succeeds (CSS import will be added when components import it — this step just confirms valid CSS; if `pnpm build` only bundles imported CSS, defer this check to Task 8).

- [ ] **Step 3: Commit**

```bash
git add src/shell/chrome/chrome.css
git commit -m "feat(chrome): token-driven chrome stylesheet (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: `<MenuBar>` component

Renders `MENU_TREE` as click-to-open dropdowns with flyout submenus; leaf clicks call `onAction(id)`.

**Files:**
- Create: `src/shell/chrome/MenuBar.tsx`
- Test: `src/shell/chrome/MenuBar.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `src/shell/chrome/MenuBar.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MenuBar } from './MenuBar';

describe('MenuBar', () => {
  it('renders all seven top menus', () => {
    render(<MenuBar onAction={vi.fn()} />);
    for (const label of ['File', 'Message', 'Session', 'Mailbox', 'View', 'Tools', 'Help']) {
      expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
    }
  });

  it('opens a dropdown on click and fires onAction for a leaf', () => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: 'File' }));
    fireEvent.click(screen.getByRole('button', { name: /New Message/ }));
    expect(onAction).toHaveBeenCalledWith('menu:file:new');
  });

  it('reveals a submenu leaf (View → Color scheme → Night)', () => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: 'View' }));
    fireEvent.click(screen.getByRole('button', { name: /Night . tactical/ }));
    expect(onAction).toHaveBeenCalledWith('menu:view:scheme:night-red');
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm vitest run src/shell/chrome/MenuBar.test.tsx`
Expected: FAIL — cannot resolve `./MenuBar`.

- [ ] **Step 3: Write `MenuBar.tsx`**

Create `src/shell/chrome/MenuBar.tsx`:

```tsx
import { useState, useCallback } from 'react';
import { MENU_TREE, type MenuActionId, type MenuNode } from './menuModel';
import './chrome.css';

interface MenuBarProps {
  onAction: (id: MenuActionId) => void;
}

function MenuItems({ items, onPick }: { items: MenuNode[]; onPick: (id: MenuActionId) => void }) {
  return (
    <>
      {items.map((node, i) => {
        if (node.separator) return <div key={`sep-${i}`} className="tux-sep" />;
        if (node.submenu) {
          return (
            <div key={node.label} className="tux-mi tux-has-sub">
              {node.label}
              <span className="tux-chev">›</span>
              <div className="tux-submenu">
                <MenuItems items={node.submenu} onPick={onPick} />
              </div>
            </div>
          );
        }
        return (
          <button key={node.id} className="tux-mi" onClick={() => node.id && onPick(node.id)}>
            {node.label}
            {node.accel && <span className="tux-accel">{node.accel}</span>}
          </button>
        );
      })}
    </>
  );
}

export function MenuBar({ onAction }: MenuBarProps) {
  const [openLabel, setOpenLabel] = useState<string | null>(null);

  const pick = useCallback((id: MenuActionId) => {
    onAction(id);
    setOpenLabel(null);
  }, [onAction]);

  return (
    <div className="tux-menubar" role="menubar">
      {MENU_TREE.map((menu) => (
        <div
          key={menu.label}
          className={`tux-menu${openLabel === menu.label ? ' tux-open' : ''}`}
          // hover-to-switch once a menu is open (native menubar behavior)
          onMouseEnter={() => setOpenLabel((cur) => (cur ? menu.label : cur))}
        >
          <button
            role="menuitem"
            onClick={(e) => {
              e.stopPropagation();
              setOpenLabel((cur) => (cur === menu.label ? null : menu.label));
            }}
          >
            {menu.label}
          </button>
          {openLabel === menu.label && (
            <div className="tux-dropdown">
              <MenuItems items={menu.items} onPick={pick} />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
```

> **Implementer note:** the top-level `<button>` is the accessible name for "File"/"View"/etc. (the tests query `getByRole('button', { name: 'File' })`). A click-away close (listening on `document`) should be added when wired into AppShell (Task 11) or here via a `useEffect` — keep it simple; the prototype used a `document` click handler that clears `openLabel`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm vitest run src/shell/chrome/MenuBar.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/shell/chrome/MenuBar.tsx src/shell/chrome/MenuBar.test.tsx
git commit -m "feat(chrome): HTML MenuBar component (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: `<TitleBar>` component

Drag region + app label + Adwaita-style window controls wired to the Tauri window API.

**Files:**
- Create: `src/shell/chrome/TitleBar.tsx`
- Test: `src/shell/chrome/TitleBar.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `src/shell/chrome/TitleBar.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

const win = vi.hoisted(() => ({
  minimize: vi.fn(async () => {}),
  toggleMaximize: vi.fn(async () => {}),
  close: vi.fn(async () => {}),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => win }));

import { TitleBar } from './TitleBar';

describe('TitleBar', () => {
  beforeEach(() => { win.minimize.mockClear(); win.toggleMaximize.mockClear(); win.close.mockClear(); });

  it('renders the app name and active folder', () => {
    render(<TitleBar folderLabel="Inbox" />);
    expect(screen.getByText('Tuxlink')).toBeInTheDocument();
    expect(screen.getByText('— Inbox')).toBeInTheDocument();
  });

  it('has a drag region', () => {
    const { container } = render(<TitleBar folderLabel="Inbox" />);
    expect(container.querySelector('[data-tauri-drag-region]')).not.toBeNull();
  });

  it('wires the window controls', () => {
    render(<TitleBar folderLabel="Inbox" />);
    fireEvent.click(screen.getByRole('button', { name: /minimize/i }));
    fireEvent.click(screen.getByRole('button', { name: /maximize/i }));
    fireEvent.click(screen.getByRole('button', { name: /close/i }));
    expect(win.minimize).toHaveBeenCalledOnce();
    expect(win.toggleMaximize).toHaveBeenCalledOnce();
    expect(win.close).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm vitest run src/shell/chrome/TitleBar.test.tsx`
Expected: FAIL — cannot resolve `./TitleBar`.

- [ ] **Step 3: Write `TitleBar.tsx`**

Create `src/shell/chrome/TitleBar.tsx`:

```tsx
import { getCurrentWindow } from '@tauri-apps/api/window';
import './chrome.css';

interface TitleBarProps {
  folderLabel: string;
}

/**
 * Custom dark titlebar (tuxlink-ng3). Drag region + Adwaita-style controls.
 * Close calls window.close() → the existing lib.rs CloseRequested handler keeps
 * the app alive on Linux (minimizes); only File→Quit / Ctrl+Q exit.
 */
export function TitleBar({ folderLabel }: TitleBarProps) {
  const win = getCurrentWindow();
  return (
    <div className="tux-titlebar">
      <span className="tux-drag" data-tauri-drag-region />
      <span className="tux-app-icon">T</span>
      <span className="tux-app-name">Tuxlink</span>
      <span className="tux-app-sub">— {folderLabel}</span>
      <span className="tux-controls">
        <button className="tux-ctrl tux-min" title="Minimize" aria-label="Minimize"
          onClick={() => void win.minimize()}>−</button>
        <button className="tux-ctrl tux-max" title="Maximize" aria-label="Maximize"
          onClick={() => void win.toggleMaximize()}>□</button>
        <button className="tux-ctrl tux-close" title="Close" aria-label="Close"
          onClick={() => void win.close()}>×</button>
      </span>
    </div>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm vitest run src/shell/chrome/TitleBar.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/shell/chrome/TitleBar.tsx src/shell/chrome/TitleBar.test.tsx
git commit -m "feat(chrome): HTML TitleBar with window controls (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: `<ResizeHandles>` component

Eight invisible edge/corner handles that start a native resize-drag — the mitigation for borderless GTK losing native resize grips (spec §5, the primary risk).

**Files:**
- Create: `src/shell/chrome/ResizeHandles.tsx`
- Test: `src/shell/chrome/ResizeHandles.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `src/shell/chrome/ResizeHandles.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent } from '@testing-library/react';

const win = vi.hoisted(() => ({ startResizeDragging: vi.fn(async () => {}) }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => win,
  ResizeDirection: {
    North: 'North', South: 'South', East: 'East', West: 'West',
    NorthEast: 'NorthEast', NorthWest: 'NorthWest', SouthEast: 'SouthEast', SouthWest: 'SouthWest',
  },
}));

import { ResizeHandles } from './ResizeHandles';

describe('ResizeHandles', () => {
  beforeEach(() => win.startResizeDragging.mockClear());

  it('renders eight handles', () => {
    const { container } = render(<ResizeHandles />);
    expect(container.querySelectorAll('.tux-resize').length).toBe(8);
  });

  it('starts a resize-drag in the handle direction on mousedown', () => {
    const { container } = render(<ResizeHandles />);
    const se = container.querySelector('.tux-resize.se')!;
    fireEvent.mouseDown(se);
    expect(win.startResizeDragging).toHaveBeenCalledWith('SouthEast');
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm vitest run src/shell/chrome/ResizeHandles.test.tsx`
Expected: FAIL — cannot resolve `./ResizeHandles`.

- [ ] **Step 3: Write `ResizeHandles.tsx`**

Create `src/shell/chrome/ResizeHandles.tsx`:

```tsx
import { getCurrentWindow, ResizeDirection } from '@tauri-apps/api/window';
import './chrome.css';

// A borderless (decorations:false) GTK window has no native resize grips
// (spec §5). These invisible edge/corner handles call startResizeDragging so the
// window stays resizable. PRIMARY RISK: validate on labwc/Wayland in the grim smoke.
const HANDLES: { cls: string; dir: ResizeDirection }[] = [
  { cls: 'n', dir: ResizeDirection.North },
  { cls: 's', dir: ResizeDirection.South },
  { cls: 'e', dir: ResizeDirection.East },
  { cls: 'w', dir: ResizeDirection.West },
  { cls: 'ne', dir: ResizeDirection.NorthEast },
  { cls: 'nw', dir: ResizeDirection.NorthWest },
  { cls: 'se', dir: ResizeDirection.SouthEast },
  { cls: 'sw', dir: ResizeDirection.SouthWest },
];

export function ResizeHandles() {
  const win = getCurrentWindow();
  return (
    <>
      {HANDLES.map((h) => (
        <div
          key={h.cls}
          className={`tux-resize ${h.cls}`}
          onMouseDown={(e) => {
            if (e.button === 0) void win.startResizeDragging(h.dir);
          }}
        />
      ))}
    </>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm vitest run src/shell/chrome/ResizeHandles.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/shell/chrome/ResizeHandles.tsx src/shell/chrome/ResizeHandles.test.tsx
git commit -m "feat(chrome): borderless-window resize handles (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: Frontend cutover — wire chrome into AppShell

Render the chrome, route the menu through the dispatcher + accelerators, and remove the App.tsx broadcast listener. **Native decorations are still on after this task** (flipped in Task 11), so the app temporarily shows both the native and HTML chrome — functional, one commit, resolved next task.

**Files:**
- Modify: `src/shell/AppShell.tsx`
- Modify: `src/shell/AppShell.test.tsx`
- Modify: `src/App.tsx`

- [ ] **Step 1: Update `AppShell.tsx`**

In `src/shell/AppShell.tsx`:

(a) Add imports:
```tsx
import { TitleBar } from './chrome/TitleBar';
import { MenuBar } from './chrome/MenuBar';
import { ResizeHandles } from './chrome/ResizeHandles';
import { useAccelerators } from './chrome/useAccelerators';
import { dispatchMenuAction, type MenuHandlers } from './chrome/dispatchMenuAction';
import { invoke } from '@tauri-apps/api/core';
import { newDraftId } from '../routing'; // see Step 1d
```

(b) Replace the existing `useEffect` that calls `listen('menu', …)` ([AppShell.tsx:104-136](../../../src/shell/AppShell.tsx#L104-L136)) with a handlers object + dispatcher + accelerators:
```tsx
  const handlers: MenuHandlers = useMemo(() => ({
    openCompose: () => { void invoke('compose_window_open', { draftId: newDraftId() }); },
    connect: onConnect,
    // Reply/Reply All/Forward stay no-ops: the menu never drove reply (AppShell's
    // old listen('menu') ignored menu:message:reply; openReplyWindow needs the
    // PARSED selected message, held only by the reading pane). Reply happens from
    // the reading-pane buttons (MessageView). Keeping these no-ops = no regression.
    // Making Ctrl+R/Reply actually open a reply window is a small ENHANCEMENT
    // (lift the parsed message to AppShell) — operator decision at the execution gate.
    reply: () => {},
    replyAll: () => {},
    forward: () => {},
    toggleSessionLog: () => setShowSessionLog((s) => !s),
    toggleStatusBar: () => setShowStatusBar((s) => !s),
    selectFolder: (folder) => { setSelectedFolder(folder); setSelectedMessage(null); },
    setScheme: (id) => { applyColorScheme(id); saveColorScheme(id); },
    quit: () => { void invoke('app_quit'); },
  }), [onConnect]);

  const onMenuAction = useCallback((id: string) => dispatchMenuAction(id, handlers), [handlers]);
  useAccelerators(onMenuAction);
```

> **Decision (decisive):** `reply`/`replyAll`/`forward` are documented no-ops in ng3 — this preserves current behavior exactly (menu-driven reply was never wired; `openReplyWindow(message, mode)` in `replyActions.ts` needs the parsed selected message that only the reading pane holds). The reading-pane reply buttons remain the reply path. Wiring `Ctrl+R`/Reply to actually open a reply window is a separate small enhancement surfaced to the operator at the execution gate.

(c) Render the chrome at the top of the returned JSX, before `<DashboardRibbon>`:
```tsx
  return (
    <div className="layout-b" data-testid="app-shell-root">
      <TitleBar folderLabel={FOLDER_LABELS[selectedFolder]} />
      <MenuBar onAction={onMenuAction} />
      <ResizeHandles />
      <DashboardRibbon data={statusData} onConnect={onConnect} connecting={connecting} />
      {/* …unchanged… */}
```

(d) Move `newDraftId()` from `App.tsx` to `src/routing.ts` (exported) so both AppShell and App can use it. Add to `src/routing.ts`:
```ts
/** Fresh draft id for a new compose window. Stable per click. */
export function newDraftId(): string {
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  const rand = Math.random().toString(36).slice(2, 8);
  return `draft-${ts}-${rand}`;
}
```

(e) Add `useMemo` to the React import line.

- [ ] **Step 2: Remove the broadcast listener from `App.tsx`**

In `src/App.tsx`, delete the entire `menu:file:new` listener `useEffect` ([App.tsx:41-73](../../../src/App.tsx#L41-L73)) and the local `newDraftId` (now imported from routing). The dispatcher in AppShell handles compose-open in-process now. Keep the compose-route detection and the wizard-probe effect.

- [ ] **Step 3: Update `AppShell.test.tsx`**

The menu tests previously fired events via the captured `listen('menu')` handler (`h.menuHandler`). Rewrite them to drive the menu through the rendered `<MenuBar>` (click File → New, View → Toggle Session Log, etc.) OR by asserting the dispatcher path. Also extend the `@tauri-apps/api/window` mock to include `minimize/toggleMaximize/close/startResizeDragging` (used by TitleBar/ResizeHandles now mounted in the shell):

```tsx
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: 'main',
    setTitle: vi.fn(async () => {}),
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    startResizeDragging: vi.fn(async () => {}),
  }),
  ResizeDirection: { North:'North',South:'South',East:'East',West:'West',NorthEast:'NorthEast',NorthWest:'NorthWest',SouthEast:'SouthEast',SouthWest:'SouthWest' },
}));
```
Update the "View menu toggles session log/status bar" tests to click through `<MenuBar>` instead of calling `h.menuHandler`. Remove the now-unused `h.menuHandler` hoist if no test needs it.

- [ ] **Step 4: Run the frontend suite**

Run: `pnpm vitest run`
Expected: PASS — all suites green (the new chrome suites + the adapted AppShell suite). Fix any selector drift.

- [ ] **Step 5: Commit**

```bash
git add src/shell/AppShell.tsx src/shell/AppShell.test.tsx src/App.tsx src/routing.ts
git commit -m "feat(chrome): render HTML chrome in AppShell; dispatch menu in-process (ng3)

Removes the App.tsx menu broadcast listener (the F7 recursion source). Native
decorations still on (flipped next commit); transient doubled chrome.

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: Remove native chrome — decorations off + delete native menu

The atomic cutover: turn off native decorations and remove the native menu. After this commit there is exactly one chrome (the HTML one).

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/lib.rs`
- Delete: `src-tauri/src/menu.rs`
- Delete: `src-tauri/tests/menu_test.rs`

- [ ] **Step 1: Turn off main-window decorations**

In `src-tauri/tauri.conf.json`, add `"decorations": false` to the `app.windows[0]` object:
```json
      {
        "title": "tuxlink",
        "width": 1200,
        "height": 820,
        "minWidth": 900,
        "minHeight": 600,
        "decorations": false
      }
```

- [ ] **Step 2: Remove the native menu install from `lib.rs`**

In `src-tauri/src/lib.rs`:
- Delete `pub mod menu;` (line 6).
- In `.setup(...)`, delete the three menu lines ([lib.rs:84-89](../../../src-tauri/src/lib.rs#L84-L89)):
```rust
            let menu = crate::menu::build_menu(app.handle())?;
            app.set_menu(menu)?;
            crate::menu::wire_menu_events(app.handle());
```
  (Leave the tray install + bootstrap that follow.)

- [ ] **Step 3: Delete the native menu module + its test**

```bash
git rm src-tauri/src/menu.rs src-tauri/tests/menu_test.rs
```
(The `menu:*` contract is now enforced by `src/shell/chrome/menuModel.test.ts`.)

- [ ] **Step 4: Confirm nothing else references `menu.rs`**

Run: `grep -rn "crate::menu\|mod menu\|menu_event_ids\|build_menu\|wire_menu_events" src-tauri/src src-tauri/tests`
Expected: no matches (tray.rs builds its own tray menu independently — confirm it does not import `menu.rs`).

- [ ] **Step 5: Build + test the Rust side**

Run: `cargo build --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: compiles clean; all remaining Rust tests pass (149 lib + integration suites, minus the deleted menu test).

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/
git commit -m "feat(chrome)!: remove native titlebar + menu; HTML chrome is canonical (ng3)

decorations:false + delete the app-global native menu. The menu:* contract now
lives in menuModel.test.ts. Resolves the duplicate compose menu root cause.

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 12: Compose window minimal chrome (closes tuxlink-msr)

Give the compose window `decorations:false` + a minimal HTML title bar (label + drag + close). Close reuses the existing `handleRequestClose`.

**Files:**
- Modify: `src-tauri/src/compose_window.rs`
- Create: `src/compose/ComposeTitleBar.tsx`
- Test: `src/compose/ComposeTitleBar.test.tsx`
- Modify: `src/compose/Compose.tsx`

- [ ] **Step 1: Turn off compose-window decorations**

In `src-tauri/src/compose_window.rs`, add `.decorations(false)` to the `WebviewWindowBuilder` chain ([compose_window.rs:139-149](../../../src-tauri/src/compose_window.rs#L139-L149)), after `.resizable(true)`:
```rust
    .resizable(true)
    .decorations(false)
    .center()
```

- [ ] **Step 2: Write the failing `ComposeTitleBar` test**

Create `src/compose/ComposeTitleBar.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ComposeTitleBar } from './ComposeTitleBar';

describe('ComposeTitleBar', () => {
  it('renders the title and a drag region', () => {
    const { container } = render(<ComposeTitleBar onClose={vi.fn()} />);
    expect(screen.getByText('New Message')).toBeInTheDocument();
    expect(container.querySelector('[data-tauri-drag-region]')).not.toBeNull();
  });

  it('calls onClose when the close control is clicked', () => {
    const onClose = vi.fn();
    render(<ComposeTitleBar onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: /close/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

Run: `pnpm vitest run src/compose/ComposeTitleBar.test.tsx`
Expected: FAIL — cannot resolve `./ComposeTitleBar`.

- [ ] **Step 4: Write `ComposeTitleBar.tsx`**

Create `src/compose/ComposeTitleBar.tsx`:

```tsx
import '../shell/chrome/chrome.css';

interface ComposeTitleBarProps {
  onClose: () => void;
}

/**
 * Minimal dark title bar for the compose window (tuxlink-ng3 / closes msr).
 * No menu (the compose window must not show the main menu). Close delegates to
 * the existing handleRequestClose (unsaved-changes prompt → compose_close_self).
 */
export function ComposeTitleBar({ onClose }: ComposeTitleBarProps) {
  return (
    <div className="tux-compose-titlebar">
      <span className="tux-drag" data-tauri-drag-region />
      <span className="tux-app-name">New Message</span>
      <span className="tux-controls">
        <button className="tux-ctrl tux-close" title="Close" aria-label="Close" onClick={onClose}>×</button>
      </span>
    </div>
  );
}
```

- [ ] **Step 5: Render it in `Compose.tsx`**

In `src/compose/Compose.tsx`, import and render `<ComposeTitleBar onClose={handleRequestClose} />` as the first child of the compose layout's root element. (`handleRequestClose` already exists at [Compose.tsx:262](../../../src/compose/Compose.tsx#L262).)

```tsx
import { ComposeTitleBar } from './ComposeTitleBar';
// …inside the returned JSX, as the first element:
<ComposeTitleBar onClose={handleRequestClose} />
```

- [ ] **Step 6: Run the frontend suite**

Run: `pnpm vitest run`
Expected: PASS (new ComposeTitleBar suite + existing Compose suite still green).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/compose_window.rs src/compose/ComposeTitleBar.tsx src/compose/ComposeTitleBar.test.tsx src/compose/Compose.tsx
git commit -m "feat(chrome): minimal compose-window title bar; closes msr duplicate menu (ng3)

Agent: fen-cypress-arroyo
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 13: Full gates + adversarial review + grim real-app smoke

**Files:** none (verification + the build-robust-features adversarial round).

- [ ] **Step 1: Full quality gates**

Run:
```bash
pnpm vitest run                                              # all frontend suites
pnpm tsc --noEmit                                            # type check
cargo test --manifest-path src-tauri/Cargo.toml             # Rust lib + integration
```
Expected: all green. Record counts in the handoff.

- [ ] **Step 2: Codex adversarial round** (CLAUDE.md "Extended capabilities")

Run a Codex review of the branch focused on the spec §10 open questions (resize on labwc/Wayland; drag-region click-through; leftover `menu` consumers; compose-window accelerator leak):
```bash
npx --yes @openai/codex review --base feat/v0.0.1 \
  "Custom window chrome: borderless GTK resize correctness, data-tauri-drag-region click-through to controls/menus on Wayland, any remaining app-global 'menu' event consumer, and whether main-window accelerators can fire while a compose window is focused." \
  2>&1 | tee dev/adversarial/2026-05-21-window-chrome-ng3-codex.md
```
Triage findings; fix or file follow-ups. (Codex quota: a usage-limit message is a capacity-defer, not a skip.)

- [ ] **Step 3: grim real-app smoke (operator-run — no Wayland click-injection on this Pi)**

Launch `pnpm tauri dev` (`--manifest-path src-tauri/Cargo.toml` not needed; the tauri CLI reads `src-tauri/tauri.conf.json`) and walk:
1. ⚠️ **Drag the titlebar to move; drag each edge/corner to resize** (the §5 primary risk).
2. **Flip View → Color scheme** through default / night-red / grayscale — the whole chrome recolors.
3. **New Message** (button + `Ctrl+N`): the compose window shows the minimal title bar with **no menu** (msr fixed).
4. **Accelerators**: `Ctrl+N`, `Ctrl+Q` (quits), `F5` and `Ctrl+Shift+O` (connect), `Ctrl+Shift+L` (toggle log).
5. **Close button** minimizes/keeps alive; only Quit exits.

Capture grim screenshots to `dev/scratch/ng3-*.png`.

- [ ] **Step 4: Update the bd issues + implementation log**

```bash
bd close tuxlink-ng3 tuxlink-msr --reason="HTML dark chrome shipped; compose minimal bar removes inherited menu"
```
Add a top entry to `dev/implementation-log.md` (date + topic + gates + smoke result).

---

## Self-Review (completed by author)

**Spec coverage:** §2.1 decorations → Task 11/12; §2.2 native-menu removal + vocabulary → Tasks 3, 11; §2.3 components → Tasks 7,8,9; §2.4 dispatcher + broadcast removal → Tasks 4, 10; §2.5 app_quit → Task 1; §3 accelerators → Tasks 3, 5; §4 compose chrome → Task 12; §5 controls/drag/resize + capabilities → Tasks 2, 8, 9; §6 tokens → Task 6; §7 testing → every task + Task 13; §8 process → Task 13 Step 2. All sections covered.

**Placeholder scan:** the `reply/replyAll/forward` handler bodies in Task 10 are decisive no-ops (preserve current behavior; menu-driven reply was never wired) with the enhancement surfaced to the operator at the execution gate — not a hidden TODO. No other placeholders.

**Type consistency:** `MenuActionId`, `MENU_ACTION_IDS`, `MENU_TREE`, `ACCELERATORS`, `MenuHandlers`, `dispatchMenuAction`, `matchAccelerator`, `useAccelerators`, `MenuBar`, `TitleBar`, `ResizeHandles`, `ComposeTitleBar`, `app_quit` are used consistently across tasks. `ColorScheme` + `isColorScheme` confirmed against `colorScheme.ts`'s actual exports.
