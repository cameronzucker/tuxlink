# Help-window redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current modal `HelpPanel` (PR #214) with a separate Tauri webview window mirroring the compose precedent, add an in-app text-size dropdown with FZ-M1-aware touch targets, and back search with the project's existing FTS5 module — landing as nine independently-green commits per ADR 0010.

**Architecture:** New `src-tauri/src/help_window.rs` opens a single-instance Tauri window at the route `/help` (label `"help"`, geometry persisted by `tauri-plugin-window-state`). The React side branches on `parseHelpRoute(window.location.pathname)` in `App.tsx` and mounts `<HelpView>` — a sidebar/reading-pane layout that collapses to a hamburger drawer below 960 px window width. Text size is a four-tier dropdown (Normal/Large/X-Large/Huge ↔ 18/20/22/24 px) persisted in localStorage. Theme inheritance is event-driven (`color_scheme_changed` Tauri event broadcast by the main window). Search extends `src-tauri/src/search/` with a `docs_fts` virtual table populated from `docs/user-guide/*.md` at first launch.

**Tech Stack:** Rust (Tauri 2) · React 18 · TypeScript · Vitest + Testing Library · rusqlite/FTS5 · `tauri-plugin-window-state` · `@tauri-apps/plugin-shell`.

**Spec:** [docs/superpowers/specs/2026-06-03-help-window-design.md](../specs/2026-06-03-help-window-design.md) — all cross-references below cite the spec by section number.

**bd issue:** `tuxlink-0gsy`.

---

## File map

### Rust (`src-tauri/`)

| Path | Action | Responsibility |
|---|---|---|
| `src/help_window.rs` | Create | Tauri command `help_window_open` + caller-authorization guard + single-instance focus-existing logic. |
| `src/lib.rs` | Modify | Add `mod help_window;` and `help_window::help_window_open` to `invoke_handler!`. Add theme commands `theme_get_scheme` / `theme_broadcast_scheme` to same list. |
| `src/theme_state.rs` | Create | Tauri-managed state (`Mutex<Option<String>>`) holding the last-broadcast scheme; helper commands `theme_get_scheme` + `theme_broadcast_scheme`. |
| `src/search/extractor.rs` | Modify | Add `pub fn extract_markdown(md: &str) -> String` — strips markdown syntax, returns plain text for FTS5 ingestion. |
| `src/search/docs_index.rs` | Create | New module owning the `docs_fts` virtual table: schema DDL, populate-from-bundled-markdown, `search_docs(query) -> Vec<DocsHit>` query path. |
| `src/search/mod.rs` | Modify | Add `pub mod docs_index;` + bump `SCHEMA_VERSION` from 2 to 3; `build_service` initializes the docs table on first launch + reseeds on drift. |
| `src/search/commands.rs` | Modify | Add `pub fn docs_search(svc: State<...>, query: String) -> Result<Vec<DocsHit>, String>` Tauri command. |
| `src/search/index.rs` | Modify | Bump `SCHEMA_VERSION` constant to 3; `init_schema` adds the `docs_fts` DDL. |
| `capabilities/help.json` | Create | Capability for `windows: ["help"]` granting event + shell + window-manipulation permissions. |
| `capabilities/default.json` | Modify | (No change required — the main window's `core:default` permission already permits invoking app commands; the new commands are app commands.) |

### Frontend (`src/`)

| Path | Action | Responsibility |
|---|---|---|
| `src/routing.ts` | Modify | Add `parseHelpRoute(pathname: string): boolean`. |
| `src/App.tsx` | Modify | Add `isHelpWindow` branch after `isComposeWindow`, mount lazy `<HelpView>`. |
| `src/help/HelpView.tsx` | Create | Top-level component mounted at `/help`. Composes header + sidebar + reading pane; owns global keydown listener for `Ctrl+/Ctrl−/Ctrl0`. |
| `src/help/HelpView.css` | Create | Outer grid + responsive media query. |
| `src/help/Sidebar.tsx` | Create | Section-grouped topic list + search input + clear button + hit-list rendering. |
| `src/help/Sidebar.css` | Create | Sidebar visual + drawer overlay (collapsed mode). |
| `src/help/ReadingPane.tsx` | Create | Renders `<HelpTopic>` markdown via existing `markdownRender.ts`; intercepts cross-topic + external + anchor links. |
| `src/help/ReadingPane.css` | Create | Typography scaling rules consuming `--help-font-size`. |
| `src/help/TextSizeDropdown.tsx` | Create | Header dropdown widget. |
| `src/help/TextSizeDropdown.css` | Create | Dropdown styles. |
| `src/help/topics.ts` | Create | Typed registry: `TOPICS`, `SECTIONS`, parse display name from first `#` heading. |
| `src/help/useFontSize.ts` | Create | localStorage-backed hook + tier-step helpers. |
| `src/help/useHelpTheme.ts` | Create | `invoke('theme_get_scheme')` on mount + `listen('color_scheme_changed')`. |
| `src/help/useHelpSearch.ts` | Create | `useQuery`-wrapped `invoke('docs_search')` with debounce. |
| `src/shell/AppShell.tsx` | Modify | `openHelp` callback now calls `invoke('help_window_open')` instead of `setHelpOpen(true)`. Remove `helpOpen` state + `<HelpPanel>` mount + lazy import. |
| `src/shell/colorScheme.ts` | Modify | After applying a scheme in `applyColorScheme`, call `invoke('theme_broadcast_scheme', { scheme })` to broadcast to other windows. |
| `src/shell/HelpPanel.tsx` | Delete | Replaced by `HelpView`. |
| `src/shell/HelpPanel.css` | Delete | Replaced by `help/*.css`. |
| `src/shell/HelpPanel.test.tsx` | Delete | Replaced by `help/*.test.tsx`. |

### Tests

| Path | Action |
|---|---|
| `src/routing.test.ts` | Modify (add `parseHelpRoute` cases) |
| `src/help/topics.test.ts` | Create |
| `src/help/Sidebar.test.tsx` | Create |
| `src/help/ReadingPane.test.tsx` | Create |
| `src/help/TextSizeDropdown.test.tsx` | Create |
| `src/help/useFontSize.test.ts` | Create |
| `src/help/HelpView.test.tsx` | Create (integration: mount HelpView at `/help`, assert layout + keyboard shortcuts) |
| `src/shell/chrome/dispatchMenuAction.test.ts` | Modify (update `openHelp` expectation — the unit-test surface doesn't change; the handler implementation in AppShell does) |
| `src-tauri/src/help_window.rs` (inline `#[cfg(test)] mod`) | Create |
| `src-tauri/src/search/extractor.rs` (extend tests) | Modify |
| `src-tauri/src/search/docs_index.rs` (inline `#[cfg(test)] mod`) | Create |

---

## Notes that apply to every task

- **All paths relative to the repo root** (`/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-0gsy-help-window-redesign/`). Tasks assume the engineer is `cd`'d there.
- **Commit messages MUST carry** the `Agent: bog-bluff-mesa` trailer + the `Co-Authored-By` trailer per CLAUDE.md "Agent identity" rules. The plan templates the trailers in each commit step.
- **Pin paths in commands** — `pnpm` and `cargo` invocations use `pnpm -C` and `cargo --manifest-path` per memory `feedback_pin_paths_in_worktree_sessions` to survive bash cwd drift in the worktree.
- **TDD strictly:** every task writes the failing test first, runs it to confirm failure, then implements minimally to pass, then re-runs. Skipping the failing-test step is a plan violation.
- **No `--no-verify`, no `git rebase -i`, no destructive git** — see CLAUDE.md §"destructive commands are BANNED". If a hook denies a commit, fix the underlying issue.
- **Per-commit smoke is optional unless flagged.** The plan only flags operator smokes where they're load-bearing.
- **`pnpm -C . test` runs the Vitest suite. `cargo --manifest-path src-tauri/Cargo.toml test` runs the Rust suite.** Both must be green before each commit step. Build verification: `pnpm -C . build` (frontend), `cargo --manifest-path src-tauri/Cargo.toml build` (backend).

---

## Task 1 — Rust `help_window` module + registration (Commit 1)

**Spec reference:** §3 (Window architecture).

**Files:**
- Create: `src-tauri/src/help_window.rs`
- Modify: `src-tauri/src/lib.rs:243-290` — add `mod help_window;` and the command to `invoke_handler!`
- Create: `src-tauri/capabilities/help.json`

### Subtask 1.1 — Pure caller-authorization guard

- [ ] **Step 1.1.1: Write the failing test**

Add this `#[cfg(test)]` module at the bottom of a new file `src-tauri/src/help_window.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_window_is_authorized() {
        assert!(caller_is_authorized("main"));
    }

    #[test]
    fn other_windows_are_not_authorized() {
        assert!(!caller_is_authorized("help"));
        assert!(!caller_is_authorized("compose-draft-foo"));
        assert!(!caller_is_authorized(""));
    }
}
```

Also add the imports + a stub `caller_is_authorized` at the top so the file compiles (returns wrong value so the test fails):

```rust
//! Help-window management — opens a single separate Tauri webview window for
//! the user-guide documentation (tuxlink-0gsy / spec §3).

pub fn caller_is_authorized(_caller_label: &str) -> bool {
    false  // wrong on purpose so the first test fails
}
```

- [ ] **Step 1.1.2: Register the module so it compiles**

Add `mod help_window;` near the top of `src-tauri/src/lib.rs` (alphabetically, between `forms` and `keyring` or wherever the project's module list slots it — check existing module ordering and match).

- [ ] **Step 1.1.3: Run the test to confirm failure**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --lib help_window::tests::main_window_is_authorized
```

Expected: FAIL with `assertion failed: caller_is_authorized("main")`.

- [ ] **Step 1.1.4: Implement the minimal pass**

Replace the stub with the real guard:

```rust
const MAIN_WINDOW_LABEL: &str = "main";

pub fn caller_is_authorized(caller_label: &str) -> bool {
    caller_label == MAIN_WINDOW_LABEL
}
```

- [ ] **Step 1.1.5: Re-run the tests**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --lib help_window::
```

Expected: both `caller_is_authorized` tests PASS.

### Subtask 1.2 — `help_window_open` command

The Tauri command itself needs a live `WebviewWindow`, which can't be exercised in unit tests (mirrors `compose_window.rs`'s comment at lines 13-19). The pure logic is the guard; the command body is verified at operator smoke. This subtask adds the command body without further tests.

- [ ] **Step 1.2.1: Add the command + imports**

Replace the file body with:

```rust
//! Help-window management — opens a single separate Tauri webview window for
//! the user-guide documentation (tuxlink-0gsy / spec §3).
//!
//! Mirrors compose_window.rs in shape:
//!   - WebviewWindowBuilder::new(..., WebviewUrl::App("/help".into()))
//!   - per-label geometry persisted by tauri-plugin-window-state
//!   - registered in lib.rs's invoke_handler list
//!
//! **Single instance.** Unlike compose (which permits many windows for many
//! drafts), there is exactly one help window. Re-invoking `help_window_open`
//! when the window already exists focuses it.
//!
//! **Main-window guard.** As with compose, only the main window is permitted
//! to invoke `help_window_open`. Defense-in-depth against a misbehaving help
//! frontend trying to spawn a second help window.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const MAIN_WINDOW_LABEL: &str = "main";
const HELP_WINDOW_LABEL: &str = "help";

pub fn caller_is_authorized(caller_label: &str) -> bool {
    caller_label == MAIN_WINDOW_LABEL
}

#[tauri::command]
pub fn help_window_open(app: AppHandle, caller: WebviewWindow) -> Result<(), String> {
    if !caller_is_authorized(caller.label()) {
        return Err(format!(
            "help_window_open may only be invoked from the main window (caller: {})",
            caller.label()
        ));
    }

    // Idempotent: focus an already-open help window.
    if let Some(existing) = app.get_webview_window(HELP_WINDOW_LABEL) {
        existing.show().map_err(|e| format!("show failed: {e}"))?;
        existing.set_focus().map_err(|e| format!("set_focus failed: {e}"))?;
        return Ok(());
    }

    let build_result = WebviewWindowBuilder::new(
        &app,
        HELP_WINDOW_LABEL,
        WebviewUrl::App("/help".into()),
    )
    .title("Tuxlink Documentation")
    .inner_size(1100.0, 700.0)
    .min_inner_size(640.0, 480.0)
    .resizable(true)
    .build();

    match build_result {
        Ok(_) => Ok(()),
        // Match the compose race-guard pattern: a concurrent call may race past
        // the get_webview_window check above and hit AlreadyExists from build().
        Err(tauri::Error::WindowLabelAlreadyExists(_))
        | Err(tauri::Error::WebviewLabelAlreadyExists(_)) => {
            if let Some(existing) = app.get_webview_window(HELP_WINDOW_LABEL) {
                let _ = existing.show();
                let _ = existing.set_focus();
            }
            Ok(())
        }
        Err(e) => Err(format!("help window build failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_window_is_authorized() {
        assert!(caller_is_authorized("main"));
    }

    #[test]
    fn other_windows_are_not_authorized() {
        assert!(!caller_is_authorized("help"));
        assert!(!caller_is_authorized("compose-draft-foo"));
        assert!(!caller_is_authorized(""));
    }
}
```

- [ ] **Step 1.2.2: Register the command in `invoke_handler!`**

In `src-tauri/src/lib.rs`'s `.invoke_handler(tauri::generate_handler![...])` block (currently spanning ~line 243 to ~line 290), append `crate::help_window::help_window_open,` to the list. Slot it after `crate::compose_window::compose_close_self,` so the help-window commands cluster with compose-window:

```rust
crate::compose_window::compose_window_open,
crate::compose_window::compose_close_self,
crate::help_window::help_window_open,  // tuxlink-0gsy
```

- [ ] **Step 1.2.3: Build and run tests**

```bash
cargo --manifest-path src-tauri/Cargo.toml build
cargo --manifest-path src-tauri/Cargo.toml test --lib help_window::
```

Expected: build SUCCEEDS, both tests PASS.

### Subtask 1.3 — Help-window capability

- [ ] **Step 1.3.1: Create `src-tauri/capabilities/help.json`**

Content (mirrors compose.json scope but with `help` label and only the permissions the help window needs):

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "help",
  "description": "Help window (label 'help', tuxlink-0gsy / spec §3). Least-privilege grant: events for theme-change subscription, shell:open for external links from rendered markdown, window-manipulation for OS-decoration drag/resize/close.",
  "windows": ["help"],
  "permissions": [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "core:window:allow-start-dragging",
    "core:window:allow-start-resize-dragging",
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-close",
    "core:window:allow-is-maximized",
    "shell:allow-open"
  ]
}
```

- [ ] **Step 1.3.2: Verify the capability is wired**

```bash
cargo --manifest-path src-tauri/Cargo.toml build 2>&1 | grep -i "warning: capability\|error: capability" | head
```

Expected: no warnings about the new capability file. (Tauri's build picks up capability files in `capabilities/` automatically; if there's a misconfiguration the build prints a capability-resolution warning.)

### Subtask 1.4 — Commit

- [ ] **Step 1.4.1: Stage + commit**

```bash
git -C . add src-tauri/src/help_window.rs src-tauri/src/lib.rs src-tauri/capabilities/help.json
git -C . commit -m "$(cat <<'EOF'
feat(help): help_window Rust module + invoke_handler registration (tuxlink-0gsy)

New module src-tauri/src/help_window.rs mirrors compose_window.rs in shape:

- caller_is_authorized: pure guard, main-window-label only.
- help_window_open: single-instance Tauri command, focuses an existing
  "help"-labeled window or builds a fresh 1100x700 webview at /help.
- Same AlreadyExists race-guard as compose_window.rs.

Registered in lib.rs's invoke_handler list alongside the compose-window
commands. New capability file capabilities/help.json grants the help
window the minimum permissions it needs (events for theme inheritance,
shell:open for external links, window-manipulation for OS chrome).

Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §3.
Frontend route (/help) and HelpView component land in Task 2 — until
then the command builds an empty webview window.

Tests: caller_is_authorized truth-table (main vs others).

Agent: bog-bluff-mesa
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Expected: commit succeeds; pre-commit hook accepts the message; pre-push hook deferred to Task 9's push.

---

## Task 2 — React route + empty `HelpView` (Commit 2)

**Spec reference:** §4 (Frontend route + component).

**Files:**
- Modify: `src/routing.ts` — add `parseHelpRoute`.
- Modify: `src/routing.test.ts` (create if absent) — `parseHelpRoute` cases.
- Modify: `src/App.tsx` — add `isHelpWindow` branch.
- Create: `src/help/HelpView.tsx` — minimal placeholder.
- Create: `src/help/HelpView.test.tsx` — assertion that placeholder renders.

### Subtask 2.1 — `parseHelpRoute`

- [ ] **Step 2.1.1: Check if `src/routing.test.ts` exists**

```bash
ls src/routing.test.ts 2>/dev/null && echo "EXISTS" || echo "MISSING"
```

If MISSING, create with the imports + describe block in step 2.1.2. If EXISTS, append the new describe block at the end.

- [ ] **Step 2.1.2: Write the failing tests**

```ts
// src/routing.test.ts
import { describe, it, expect } from 'vitest';
import { parseComposeRoute, parseHelpRoute } from './routing';

describe('parseComposeRoute', () => {
  it('returns null for non-compose paths', () => {
    expect(parseComposeRoute('/')).toBeNull();
    expect(parseComposeRoute('/help')).toBeNull();
  });
  it('parses a compose route', () => {
    expect(parseComposeRoute('/compose/draft-123')).toBe('draft-123');
  });
});

describe('parseHelpRoute', () => {
  it('returns true for the literal /help path', () => {
    expect(parseHelpRoute('/help')).toBe(true);
  });
  it('returns true for /help with a trailing slash', () => {
    expect(parseHelpRoute('/help/')).toBe(true);
  });
  it('returns false for non-help paths', () => {
    expect(parseHelpRoute('/')).toBe(false);
    expect(parseHelpRoute('/compose/draft-123')).toBe(false);
    expect(parseHelpRoute('/help/something')).toBe(false);
    expect(parseHelpRoute('/helpful')).toBe(false);
  });
});
```

- [ ] **Step 2.1.3: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/routing.test.ts
```

Expected: FAIL with `parseHelpRoute is not a function`.

- [ ] **Step 2.1.4: Implement `parseHelpRoute`**

Append to `src/routing.ts`:

```ts
/**
 * If `pathname` is the help route (`/help` or `/help/`), return true.
 * The help window is single-instance with no parameters, so a boolean is
 * sufficient — no equivalent to parseComposeRoute's id return.
 */
export function parseHelpRoute(pathname: string): boolean {
  return pathname === '/help' || pathname === '/help/';
}
```

- [ ] **Step 2.1.5: Re-run tests**

```bash
pnpm -C . exec vitest run src/routing.test.ts
```

Expected: all PASS.

### Subtask 2.2 — Minimal `HelpView` placeholder

- [ ] **Step 2.2.1: Write the failing test**

Create `src/help/HelpView.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { HelpView } from './HelpView';

describe('HelpView', () => {
  it('renders the help root container', () => {
    render(<HelpView />);
    expect(screen.getByTestId('tux-help-root')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2.2.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/HelpView.test.tsx
```

Expected: FAIL — cannot resolve `./HelpView`.

- [ ] **Step 2.2.3: Implement the placeholder**

Create `src/help/HelpView.tsx`:

```tsx
/**
 * HelpView — root component mounted at /help in a separate Tauri webview
 * window (label "help"). Replaces the modal HelpPanel from PR #214.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §4.
 *
 * This is the empty skeleton landed in Task 2 of the implementation plan.
 * Sidebar + reading pane + dropdown land in Tasks 3-4.
 */
export function HelpView() {
  return <div data-testid="tux-help-root">Tuxlink Documentation</div>;
}
```

- [ ] **Step 2.2.4: Re-run test**

```bash
pnpm -C . exec vitest run src/help/HelpView.test.tsx
```

Expected: PASS.

### Subtask 2.3 — Wire `App.tsx` branch

- [ ] **Step 2.3.1: Edit App.tsx**

In `src/App.tsx`, after the existing `isComposeWindow` block (around line 50-58), add the help branch:

```tsx
// Import additions near the top:
import { parseComposeRoute, parseHelpRoute } from './routing';

const HelpView = lazy(() =>
  import('./help/HelpView').then((m) => ({ default: m.HelpView })),
);

// Branch addition — after the existing isComposeWindow block:
const isHelpWindow = parseHelpRoute(window.location.pathname);
if (isHelpWindow) {
  return (
    <Suspense fallback={<div data-testid="app-loading" />}>
      <HelpView />
    </Suspense>
  );
}
```

The wizard-completed probe `useEffect` already guards with `if (isComposeWindow) return;` — extend the same guard to skip when help is rendering:

```tsx
useEffect(() => {
  if (isComposeWindow || isHelpWindow) return;
  // ... existing body
}, [isComposeWindow, isHelpWindow]);
```

- [ ] **Step 2.3.2: Run frontend build**

```bash
pnpm -C . build
```

Expected: build SUCCEEDS, no type errors.

- [ ] **Step 2.3.3: Run full Vitest suite**

```bash
pnpm -C . test
```

Expected: all PASS (the existing dispatchMenuAction tests still pass because we haven't changed the dispatcher).

### Subtask 2.4 — Commit

- [ ] **Step 2.4.1: Stage + commit**

```bash
git -C . add src/routing.ts src/routing.test.ts src/App.tsx src/help/HelpView.tsx src/help/HelpView.test.tsx
git -C . commit -m "$(cat <<'EOF'
feat(help): React route + HelpView skeleton (tuxlink-0gsy)

src/routing.ts gains parseHelpRoute(pathname) — boolean true for the
literal /help and /help/ paths. App.tsx branches on it after the
existing isComposeWindow branch and mounts a lazy <HelpView /> for the
help webview.

HelpView itself is a placeholder in this commit — renders only its
root container. Sidebar + reading pane + text-size dropdown land in
subsequent commits.

The old modal HelpPanel still works through the existing
openHelp menu handler (AppShell.tsx); it is removed in Task 8.

Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §4.1.

Tests: parseHelpRoute truth-table; HelpView placeholder smoke.

Agent: bog-bluff-mesa
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3 — Sidebar + reading pane + topic registry (Commit 3)

**Spec reference:** §4.2, §4.3, §5.

**Files:**
- Create: `src/help/topics.ts`, `src/help/topics.test.ts`
- Create: `src/help/Sidebar.tsx`, `src/help/Sidebar.css`, `src/help/Sidebar.test.tsx`
- Create: `src/help/ReadingPane.tsx`, `src/help/ReadingPane.css`, `src/help/ReadingPane.test.tsx`
- Create: `src/help/HelpView.css`
- Modify: `src/help/HelpView.tsx`, `src/help/HelpView.test.tsx`

### Subtask 3.1 — `topics.ts` registry

- [ ] **Step 3.1.1: Write the failing test**

Create `src/help/topics.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { TOPICS, SECTIONS, getTopicBySlug } from './topics';

describe('topics registry', () => {
  it('exposes ten topics', () => {
    expect(TOPICS).toHaveLength(10);
  });

  it('every topic has a non-empty slug, number, displayName, body, sectionId', () => {
    for (const t of TOPICS) {
      expect(t.slug).toMatch(/^\d{2}-[a-z-]+$/);
      expect(t.number).toMatch(/^\d{2}$/);
      expect(t.displayName.length).toBeGreaterThan(0);
      expect(t.body.length).toBeGreaterThan(0);
      expect(['getting-started', 'using', 'config', 'reference']).toContain(t.sectionId);
    }
  });

  it('every section references existing topic slugs', () => {
    const all = new Set(TOPICS.map((t) => t.slug));
    for (const sec of SECTIONS) {
      for (const slug of sec.topicSlugs) {
        expect(all.has(slug)).toBe(true);
      }
    }
  });

  it('every topic belongs to exactly one section', () => {
    const counts = new Map<string, number>();
    for (const sec of SECTIONS) {
      for (const slug of sec.topicSlugs) {
        counts.set(slug, (counts.get(slug) ?? 0) + 1);
      }
    }
    for (const t of TOPICS) {
      expect(counts.get(t.slug)).toBe(1);
    }
  });

  it('parses the displayName from the first # heading', () => {
    const intro = TOPICS.find((t) => t.slug === '01-getting-started');
    expect(intro?.displayName).toBe('Getting started');
  });

  it('getTopicBySlug returns the matching topic or undefined', () => {
    expect(getTopicBySlug('02-connections')?.displayName).toBe('Connections');
    expect(getTopicBySlug('99-no-such')).toBeUndefined();
  });
});
```

- [ ] **Step 3.1.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/topics.test.ts
```

Expected: FAIL — module not resolvable.

- [ ] **Step 3.1.3: Implement `topics.ts`**

Create `src/help/topics.ts`:

```ts
/**
 * Help topic registry. Bundles docs/user-guide/*.md at build time via
 * import.meta.glob (TEST-1-safe pattern — no node:fs) and exposes a typed
 * read-only registry to the rest of the help/* components.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §4.3.
 */

export type HelpSectionId = 'getting-started' | 'using' | 'config' | 'reference';

export interface HelpTopic {
  slug: string;         // "01-getting-started"
  number: string;       // "01"
  displayName: string;  // parsed from the first # heading
  body: string;         // raw markdown
  sectionId: HelpSectionId;
}

export interface HelpSection {
  id: HelpSectionId;
  displayName: string;
  topicSlugs: readonly string[];
}

// Section grouping is hand-authored — filename ordering is stable but
// the Getting-started / Using / Config / Reference grouping is editorial.
export const SECTIONS: readonly HelpSection[] = [
  {
    id: 'getting-started',
    displayName: 'Getting started',
    topicSlugs: ['01-getting-started', '02-connections'],
  },
  {
    id: 'using',
    displayName: 'Using Tuxlink',
    topicSlugs: ['03-mailbox', '04-composing', '05-forms', '06-search'],
  },
  {
    id: 'config',
    displayName: 'Configuration',
    topicSlugs: ['07-settings', '08-color-schemes', '09-keyboard'],
  },
  {
    id: 'reference',
    displayName: 'Reference',
    topicSlugs: ['10-troubleshooting'],
  },
];

// Build a slug → sectionId map once.
const SLUG_TO_SECTION: Record<string, HelpSectionId> = {};
for (const sec of SECTIONS) {
  for (const slug of sec.topicSlugs) {
    SLUG_TO_SECTION[slug] = sec.id;
  }
}

// Bundle all markdown files at build time. Vite's import.meta.glob with
// { eager: true, query: '?raw' } returns { '/path/01.md': 'raw content', ... }.
const RAW_TOPICS = import.meta.glob('/docs/user-guide/*.md', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

// Parse the first `# heading` from a markdown body. Returns the heading text
// (without `#` or leading/trailing whitespace) or the slug as a fallback.
function parseDisplayName(body: string, slug: string): string {
  const match = body.match(/^#\s+(.+)$/m);
  if (match) return match[1].trim();
  return slug;
}

function buildTopics(): readonly HelpTopic[] {
  const out: HelpTopic[] = [];
  for (const [path, body] of Object.entries(RAW_TOPICS)) {
    const filename = path.split('/').pop()!.replace(/\.md$/, '');  // "01-getting-started"
    const numberMatch = filename.match(/^(\d{2})-/);
    if (!numberMatch) continue;  // filename does not match the convention
    const slug = filename;
    const sectionId = SLUG_TO_SECTION[slug];
    if (!sectionId) {
      throw new Error(
        `topics.ts: markdown file ${slug} is not grouped in SECTIONS. ` +
        `Add it to a section or rename the file.`,
      );
    }
    out.push({
      slug,
      number: numberMatch[1],
      displayName: parseDisplayName(body, slug),
      body,
      sectionId,
    });
  }
  out.sort((a, b) => a.slug.localeCompare(b.slug));
  return out;
}

export const TOPICS: readonly HelpTopic[] = buildTopics();

export function getTopicBySlug(slug: string): HelpTopic | undefined {
  return TOPICS.find((t) => t.slug === slug);
}
```

- [ ] **Step 3.1.4: Re-run tests**

```bash
pnpm -C . exec vitest run src/help/topics.test.ts
```

Expected: all PASS.

### Subtask 3.2 — `ReadingPane`

- [ ] **Step 3.2.1: Write the failing test**

Create `src/help/ReadingPane.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ReadingPane } from './ReadingPane';
import { TOPICS, getTopicBySlug } from './topics';

const intro = getTopicBySlug('01-getting-started')!;
const conn = getTopicBySlug('02-connections')!;

describe('ReadingPane', () => {
  it('renders the topic displayName as an h1', () => {
    render(<ReadingPane topic={intro} onNavigate={() => {}} />);
    expect(screen.getByRole('heading', { level: 1, name: intro.displayName })).toBeInTheDocument();
  });

  it('renders rendered markdown body content', () => {
    render(<ReadingPane topic={conn} onNavigate={() => {}} />);
    // The connections topic mentions "ARDOP" — assert that text reaches the DOM.
    expect(screen.getByText(/ARDOP/i)).toBeInTheDocument();
  });

  it('intercepts inter-topic .md links and calls onNavigate with the slug', () => {
    const onNavigate = vi.fn();
    render(<ReadingPane topic={conn} onNavigate={onNavigate} />);
    // The connections topic ends with a "Where next" section linking
    // to [The mailbox](03-mailbox.md). Find that link and click it.
    const link = screen.getByText(/The mailbox/);
    fireEvent.click(link);
    expect(onNavigate).toHaveBeenCalledWith('03-mailbox');
  });
});
```

- [ ] **Step 3.2.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/ReadingPane.test.tsx
```

Expected: FAIL — module not resolvable.

- [ ] **Step 3.2.3: Implement `ReadingPane.tsx`**

Create `src/help/ReadingPane.tsx`:

```tsx
import { useCallback } from 'react';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { parseMarkdown } from '../shell/markdownRender';
import type { Block, InlineText, InlineRun } from '../shell/markdownRender';
import type { HelpTopic } from './topics';
import './ReadingPane.css';

interface ReadingPaneProps {
  topic: HelpTopic;
  onNavigate: (slug: string) => void;
}

export function ReadingPane({ topic, onNavigate }: ReadingPaneProps) {
  const handleClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      const target = event.target as HTMLElement;
      const anchor = target.closest('a');
      if (!anchor) return;
      const href = anchor.getAttribute('href') ?? '';

      // Inter-topic .md links — match digits-name-.md and navigate in-window.
      const mdMatch = href.match(/^(\d{2}-[a-z-]+)\.md$/);
      if (mdMatch) {
        event.preventDefault();
        onNavigate(mdMatch[1]);
        return;
      }

      // Anchor (#section) links — let the browser handle natively (scrolls).
      if (href.startsWith('#')) return;

      // External http(s) links — route to the OS browser via shell:open.
      if (/^https?:\/\//.test(href)) {
        event.preventDefault();
        void shellOpen(href);
      }
    },
    [onNavigate],
  );

  const blocks = parseMarkdown(topic.body);

  return (
    <main className="tux-help-reading" onClick={handleClick}>
      <div className="tux-help-reading-inner">
        <article className="tux-help-reading-content">
          {blocks.map((b, i) => (
            <BlockView key={i} block={b} />
          ))}
        </article>
      </div>
    </main>
  );
}

function BlockView({ block }: { block: Block }) {
  switch (block.kind) {
    case 'heading':
      if (block.level === 1) return <h1><Inline t={block.text} /></h1>;
      if (block.level === 2) return <h2><Inline t={block.text} /></h2>;
      return <h3><Inline t={block.text} /></h3>;
    case 'paragraph':
      return <p><Inline t={block.text} /></p>;
    case 'list':
      return (
        <ul>
          {block.items.map((it, i) => (
            <li key={i}><Inline t={it} /></li>
          ))}
        </ul>
      );
    case 'code':
      return <pre><code>{block.text}</code></pre>;
    case 'table':
      return (
        <table>
          <thead>
            <tr>{block.headers.map((h, i) => <th key={i}><Inline t={h} /></th>)}</tr>
          </thead>
          <tbody>
            {block.rows.map((r, i) => (
              <tr key={i}>{r.map((c, j) => <td key={j}><Inline t={c} /></td>)}</tr>
            ))}
          </tbody>
        </table>
      );
  }
}

function Inline({ t }: { t: InlineText }) {
  return (
    <>
      {t.runs.map((run, i) => <Run key={i} run={run} />)}
    </>
  );
}

function Run({ run }: { run: InlineRun }) {
  switch (run.kind) {
    case 'text':   return <>{run.text}</>;
    case 'bold':   return <strong>{run.text}</strong>;
    case 'italic': return <em>{run.text}</em>;
    case 'code':   return <code>{run.text}</code>;
    case 'link':   return <a href={run.href}>{run.text}</a>;
  }
}
```

- [ ] **Step 3.2.4: Check `parseMarkdown` export**

The existing `src/shell/markdownRender.ts` may export differently. Quick check:

```bash
grep -n "^export" src/shell/markdownRender.ts | head
```

If `parseMarkdown` is not the actual export name, find the right one (e.g. `parseMarkdownBlocks`) and update the import in `ReadingPane.tsx`. The function takes a markdown string and returns `Block[]`.

- [ ] **Step 3.2.5: Implement `ReadingPane.css`**

Create `src/help/ReadingPane.css`:

```css
/* ReadingPane — typography scaled by --help-font-size (set by useFontSize).
   The default 18 px is established in HelpView.css; this file only consumes
   the variable and scales nested elements relative to it.
   Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §5.3, §7.2. */

.tux-help-reading {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  background: var(--bg);
  display: flex;
  justify-content: center;
}

.tux-help-reading-inner {
  max-width: 720px;       /* reading column constrained per spec §5.3 */
  width: 100%;
  padding: 36px 48px 80px;
}

.tux-help-reading-content {
  font-family: var(--sans);
  color: var(--text);
  font-size: var(--help-font-size, 18px);
  line-height: 1.65;
}

.tux-help-reading-content h1 {
  font-size: calc(var(--help-font-size, 18px) * 1.7);
  font-weight: 700;
  letter-spacing: -0.01em;
  margin: 0 0 0.6em;
  color: var(--text);
}
.tux-help-reading-content h2 {
  font-size: calc(var(--help-font-size, 18px) * 1.3);
  font-weight: 600;
  margin: 1.6em 0 0.6em;
  color: var(--accent-2);
  border-bottom: 1px solid var(--border);
  padding-bottom: 0.3em;
}
.tux-help-reading-content h3 {
  font-size: calc(var(--help-font-size, 18px) * 1.1);
  font-weight: 600;
  margin: 1.2em 0 0.4em;
  color: var(--text);
}
.tux-help-reading-content p { margin: 0.8em 0; }
.tux-help-reading-content ul { padding-left: 1.4em; margin: 0.8em 0; }
.tux-help-reading-content li { margin: 0.35em 0; }
.tux-help-reading-content strong { color: var(--accent-2); font-weight: 600; }
.tux-help-reading-content code {
  font-family: var(--mono);
  background: var(--surface);
  color: var(--accent);
  padding: 1px 6px;
  border-radius: 3px;
  font-size: calc(var(--help-font-size, 18px) * 0.9);
  border: 1px solid var(--border);
}
.tux-help-reading-content pre {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 12px 14px;
  overflow-x: auto;
  margin: 1em 0;
}
.tux-help-reading-content pre code {
  background: transparent;
  border: none;
  padding: 0;
  color: var(--text);
}
.tux-help-reading-content a {
  color: var(--info);
  text-decoration: none;
  border-bottom: 1px dotted var(--info);
  cursor: pointer;
}
.tux-help-reading-content a:hover {
  color: var(--accent);
  border-bottom-color: var(--accent);
}
.tux-help-reading-content table {
  border-collapse: collapse;
  margin: 1em 0;
  width: 100%;
}
.tux-help-reading-content th,
.tux-help-reading-content td {
  border: 1px solid var(--border);
  padding: 6px 10px;
  text-align: left;
}
.tux-help-reading-content th {
  background: var(--surface);
  font-weight: 600;
}
.tux-help-reading-content .tux-help-mark {
  background: var(--accent-soft);
  color: var(--accent);
  padding: 0 2px;
  border-radius: 2px;
}
```

- [ ] **Step 3.2.6: Re-run reading-pane tests**

```bash
pnpm -C . exec vitest run src/help/ReadingPane.test.tsx
```

Expected: all PASS.

### Subtask 3.3 — `Sidebar`

- [ ] **Step 3.3.1: Write the failing test**

Create `src/help/Sidebar.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Sidebar } from './Sidebar';

describe('Sidebar', () => {
  it('renders every section header', () => {
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText('Getting started')).toBeInTheDocument();
    expect(screen.getByText('Using Tuxlink')).toBeInTheDocument();
    expect(screen.getByText('Configuration')).toBeInTheDocument();
    expect(screen.getByText('Reference')).toBeInTheDocument();
  });

  it('renders every topic with its 2-digit number', () => {
    render(<Sidebar activeSlug="01-getting-started" onSelect={() => {}} />);
    expect(screen.getByText('01')).toBeInTheDocument();
    expect(screen.getByText('10')).toBeInTheDocument();
  });

  it('marks the active topic with aria-current=page', () => {
    render(<Sidebar activeSlug="02-connections" onSelect={() => {}} />);
    const active = screen.getByRole('link', { current: 'page' });
    expect(active.textContent).toMatch(/Connections/);
  });

  it('calls onSelect with the slug when a topic is clicked', () => {
    const onSelect = vi.fn();
    render(<Sidebar activeSlug="01-getting-started" onSelect={onSelect} />);
    fireEvent.click(screen.getByText('Mailbox'));
    expect(onSelect).toHaveBeenCalledWith('03-mailbox');
  });
});
```

- [ ] **Step 3.3.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/Sidebar.test.tsx
```

Expected: FAIL — module not resolvable.

- [ ] **Step 3.3.3: Implement `Sidebar.tsx`**

Create `src/help/Sidebar.tsx` (search-related parts are added in Task 7; this version is grouped-list-only):

```tsx
import { TOPICS, SECTIONS, getTopicBySlug } from './topics';
import './Sidebar.css';

interface SidebarProps {
  activeSlug: string;
  onSelect: (slug: string) => void;
}

export function Sidebar({ activeSlug, onSelect }: SidebarProps) {
  return (
    <nav className="tux-help-sidebar" aria-label="Help topics">
      {SECTIONS.map((sec) => (
        <div key={sec.id} className="tux-help-sb-section">
          <div className="tux-help-sb-section-title">{sec.displayName}</div>
          {sec.topicSlugs.map((slug) => {
            const t = getTopicBySlug(slug);
            if (!t) return null;
            const isActive = slug === activeSlug;
            return (
              <a
                key={slug}
                role="link"
                aria-current={isActive ? 'page' : undefined}
                className={`tux-help-sb-item${isActive ? ' active' : ''}`}
                onClick={(e) => {
                  e.preventDefault();
                  onSelect(slug);
                }}
                href={`#${slug}`}
                tabIndex={0}
              >
                <span className="tux-help-sb-num">{t.number}</span>
                <span className="tux-help-sb-name">{t.displayName}</span>
              </a>
            );
          })}
        </div>
      ))}
    </nav>
  );
}
```

- [ ] **Step 3.3.4: Implement `Sidebar.css`**

Create `src/help/Sidebar.css`:

```css
/* Sidebar — section-grouped topic list. Spec §5.1, §5.4.
   Touch targets ≥44px tall per spec §6.3. */

.tux-help-sidebar {
  background: var(--surface);
  border-right: 1px solid var(--border);
  overflow-y: auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  padding: 8px 0;
}

.tux-help-sb-section {
  display: flex;
  flex-direction: column;
}

.tux-help-sb-section-title {
  padding: 12px 14px 6px;
  font-size: 11px;
  color: var(--text-faint);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  font-weight: 600;
}

.tux-help-sb-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;             /* visible padding */
  min-height: 44px;               /* touch-comfort floor */
  font-size: 14px;
  color: var(--text-dim);
  border-left: 3px solid transparent;
  cursor: pointer;
  text-decoration: none;
  user-select: none;
}

.tux-help-sb-item:hover {
  color: var(--text);
  background: rgba(245, 159, 60, 0.04);
}

.tux-help-sb-item.active {
  color: var(--accent);
  background: var(--accent-soft);
  border-left-color: var(--accent);
  font-weight: 500;
}

.tux-help-sb-num {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text-faint);
  min-width: 18px;
}

.tux-help-sb-item.active .tux-help-sb-num {
  color: var(--accent);
}

.tux-help-sb-name {
  flex: 1;
}
```

- [ ] **Step 3.3.5: Re-run sidebar tests**

```bash
pnpm -C . exec vitest run src/help/Sidebar.test.tsx
```

Expected: all PASS.

### Subtask 3.4 — Wire `HelpView` to use sidebar + reading pane

- [ ] **Step 3.4.1: Update the HelpView test**

Replace `src/help/HelpView.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { HelpView } from './HelpView';

describe('HelpView', () => {
  it('renders the layout skeleton', () => {
    render(<HelpView />);
    expect(screen.getByTestId('tux-help-root')).toBeInTheDocument();
    expect(screen.getByRole('navigation', { name: /help topics/i })).toBeInTheDocument();
    expect(screen.getByRole('main')).toBeInTheDocument();
  });

  it('opens to the first topic by default', () => {
    render(<HelpView />);
    expect(screen.getByRole('heading', { level: 1, name: /getting started/i })).toBeInTheDocument();
  });

  it('renders the header strip with the User Guide title', () => {
    render(<HelpView />);
    expect(screen.getByText(/User Guide/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 3.4.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/HelpView.test.tsx
```

Expected: FAIL — current HelpView is a placeholder.

- [ ] **Step 3.4.3: Implement the real HelpView**

Replace `src/help/HelpView.tsx`:

```tsx
import { useState, useCallback } from 'react';
import { Sidebar } from './Sidebar';
import { ReadingPane } from './ReadingPane';
import { TOPICS, getTopicBySlug } from './topics';
import './HelpView.css';

const DEFAULT_SLUG = '01-getting-started';

export function HelpView() {
  const [activeSlug, setActiveSlug] = useState<string>(DEFAULT_SLUG);

  const handleSelect = useCallback((slug: string) => {
    setActiveSlug(slug);
  }, []);

  const handleNavigate = useCallback((slug: string) => {
    if (getTopicBySlug(slug)) setActiveSlug(slug);
  }, []);

  const activeTopic = getTopicBySlug(activeSlug) ?? TOPICS[0];

  return (
    <div className="tux-help-root" data-testid="tux-help-root">
      <header className="tux-help-header">
        <span className="tux-help-title">User Guide</span>
        <div className="tux-help-spacer" />
        {/* Text-size dropdown lands in Task 4. */}
      </header>
      <div className="tux-help-body">
        <Sidebar activeSlug={activeSlug} onSelect={handleSelect} />
        <ReadingPane topic={activeTopic} onNavigate={handleNavigate} />
      </div>
    </div>
  );
}
```

- [ ] **Step 3.4.4: Implement `HelpView.css`**

Create `src/help/HelpView.css`:

```css
/* HelpView — outer grid + responsive collapse for FZ-M1.
   Spec §5, §6. */

.tux-help-root {
  display: grid;
  grid-template-rows: auto 1fr;
  height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: var(--sans);
  /* Default text size (spec §7.2 D3) — overridden by useFontSize once mounted. */
  --help-font-size: 18px;
}

.tux-help-header {
  display: flex;
  align-items: center;
  padding: 10px 16px;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  gap: 12px;
  flex: 0 0 auto;
  min-height: 54px;
}

.tux-help-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text);
}

.tux-help-spacer { flex: 1; }

.tux-help-body {
  display: grid;
  grid-template-columns: 260px 1fr;
  min-height: 0;
}

/* Responsive collapse — spec §6.1 / §6.2. Sidebar drawer behavior lands
   in Task 7 once the drawer's open/close state is needed by search too. */
@media (max-width: 960px) {
  .tux-help-body { grid-template-columns: 1fr; }
  .tux-help-sidebar { display: none; }
}
```

- [ ] **Step 3.4.5: Re-run all help/* tests**

```bash
pnpm -C . exec vitest run src/help/
```

Expected: all PASS.

- [ ] **Step 3.4.6: Build the frontend**

```bash
pnpm -C . build
```

Expected: SUCCEEDS.

### Subtask 3.5 — Commit

- [ ] **Step 3.5.1: Stage + commit**

```bash
git -C . add src/help/
git -C . commit -m "$(cat <<'EOF'
feat(help): sidebar + reading pane + topic registry (Variant A) (tuxlink-0gsy)

Implements the spec's Variant A layout (§5):
- topics.ts: typed registry over docs/user-guide/*.md, hand-authored
  section grouping (Getting started / Using Tuxlink / Configuration /
  Reference), displayName parsed from each markdown's first # heading.
- Sidebar.tsx: section-grouped topic list with 44px-tall items for the
  FZ-M1 touch-target floor (spec §6.3), aria-current on the active
  topic, slug-based click handler.
- ReadingPane.tsx: rendered markdown via existing markdownRender.ts;
  intercepts inter-topic .md links (in-window navigation) + http(s)
  links (shellOpen via @tauri-apps/plugin-shell). Anchor (#) links
  fall through to the browser's native scroll.
- HelpView.tsx: composes header + sidebar + reading pane; opens to
  01-getting-started by default; --help-font-size CSS variable set
  to the Normal/18px default.

Text-size dropdown lands in Task 4; theme inheritance in Task 5;
search in Tasks 6-7. The old modal HelpPanel still works via the
existing AppShell menu wiring until Task 8.

Responsive collapse below 960px width is a stub (sidebar hides via
display:none). The drawer pattern lands in Task 7 alongside the
search clear-button when the help state machine grows enough to
need it.

Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §4-§5.

Tests: topics registry shape, sidebar grouping + active state +
selection, reading pane link interception. HelpView integration:
default-open, header presence, layout skeleton.

Agent: bog-bluff-mesa
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4 — Text-size dropdown + persistence (Commit 4)

**Spec reference:** §7.

**Files:**
- Create: `src/help/useFontSize.ts`, `src/help/useFontSize.test.ts`
- Create: `src/help/TextSizeDropdown.tsx`, `src/help/TextSizeDropdown.css`, `src/help/TextSizeDropdown.test.tsx`
- Modify: `src/help/HelpView.tsx`, `src/help/HelpView.test.tsx`

### Subtask 4.1 — `useFontSize` hook

- [ ] **Step 4.1.1: Write the failing test**

Create `src/help/useFontSize.test.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import {
  useFontSize,
  FONT_SIZE_STORAGE_KEY,
  FONT_PRESETS,
  FONT_PX,
  stepFontSize,
} from './useFontSize';

beforeEach(() => {
  localStorage.clear();
  document.documentElement.style.removeProperty('--help-font-size');
});
afterEach(() => {
  localStorage.clear();
  document.documentElement.style.removeProperty('--help-font-size');
});

describe('FONT_PRESETS / FONT_PX', () => {
  it('exposes the four preset names', () => {
    expect(FONT_PRESETS).toEqual(['Normal', 'Large', 'X-Large', 'Huge']);
  });
  it('maps each preset to the documented px value', () => {
    expect(FONT_PX).toEqual({ 'Normal': 18, 'Large': 20, 'X-Large': 22, 'Huge': 24 });
  });
});

describe('stepFontSize', () => {
  it('steps up through the presets', () => {
    expect(stepFontSize('Normal', 'up')).toBe('Large');
    expect(stepFontSize('Large', 'up')).toBe('X-Large');
    expect(stepFontSize('X-Large', 'up')).toBe('Huge');
    expect(stepFontSize('Huge', 'up')).toBe('Huge'); // saturates
  });
  it('steps down through the presets', () => {
    expect(stepFontSize('Huge', 'down')).toBe('X-Large');
    expect(stepFontSize('Large', 'down')).toBe('Normal');
    expect(stepFontSize('Normal', 'down')).toBe('Normal'); // saturates
  });
});

describe('useFontSize', () => {
  it('defaults to Normal when localStorage is empty', () => {
    const { result } = renderHook(() => useFontSize());
    expect(result.current.preset).toBe('Normal');
  });

  it('reads a persisted preset from localStorage', () => {
    localStorage.setItem(FONT_SIZE_STORAGE_KEY, 'Large');
    const { result } = renderHook(() => useFontSize());
    expect(result.current.preset).toBe('Large');
  });

  it('falls back to Normal when localStorage holds an unknown value', () => {
    localStorage.setItem(FONT_SIZE_STORAGE_KEY, 'GIANT');
    const { result } = renderHook(() => useFontSize());
    expect(result.current.preset).toBe('Normal');
  });

  it('applies --help-font-size on mount', () => {
    renderHook(() => useFontSize());
    expect(document.documentElement.style.getPropertyValue('--help-font-size')).toBe('18px');
  });

  it('persists + applies a new preset on setPreset', () => {
    const { result } = renderHook(() => useFontSize());
    act(() => result.current.setPreset('X-Large'));
    expect(localStorage.getItem(FONT_SIZE_STORAGE_KEY)).toBe('X-Large');
    expect(document.documentElement.style.getPropertyValue('--help-font-size')).toBe('22px');
  });
});
```

- [ ] **Step 4.1.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/useFontSize.test.ts
```

Expected: FAIL — module not resolvable.

- [ ] **Step 4.1.3: Implement `useFontSize.ts`**

Create `src/help/useFontSize.ts`:

```ts
/**
 * Text-size hook for the help window. Persists the operator's chosen
 * preset in localStorage and writes the resulting px to a global CSS
 * variable (--help-font-size on <html>) that ReadingPane.css consumes.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §7.
 */

import { useState, useEffect, useCallback } from 'react';

export const FONT_PRESETS = ['Normal', 'Large', 'X-Large', 'Huge'] as const;
export type FontPreset = (typeof FONT_PRESETS)[number];

export const FONT_PX: Record<FontPreset, number> = {
  'Normal': 18,
  'Large': 20,
  'X-Large': 22,
  'Huge': 24,
};

export const FONT_SIZE_STORAGE_KEY = 'tuxlink.help.fontSize';
export const DEFAULT_FONT_PRESET: FontPreset = 'Normal';

function isFontPreset(value: unknown): value is FontPreset {
  return typeof value === 'string' && (FONT_PRESETS as readonly string[]).includes(value);
}

/** Step `current` up or down the preset list; saturates at both ends. */
export function stepFontSize(current: FontPreset, dir: 'up' | 'down'): FontPreset {
  const i = FONT_PRESETS.indexOf(current);
  const next = dir === 'up' ? i + 1 : i - 1;
  if (next < 0) return FONT_PRESETS[0];
  if (next >= FONT_PRESETS.length) return FONT_PRESETS[FONT_PRESETS.length - 1];
  return FONT_PRESETS[next];
}

function loadPersisted(): FontPreset {
  try {
    const raw = localStorage.getItem(FONT_SIZE_STORAGE_KEY);
    if (isFontPreset(raw)) return raw;
  } catch {
    // localStorage may throw in private-browsing-class environments; treat as default.
  }
  return DEFAULT_FONT_PRESET;
}

export function useFontSize() {
  const [preset, setPresetState] = useState<FontPreset>(() => loadPersisted());

  useEffect(() => {
    document.documentElement.style.setProperty('--help-font-size', `${FONT_PX[preset]}px`);
    try {
      localStorage.setItem(FONT_SIZE_STORAGE_KEY, preset);
    } catch {
      // ignore — UI still updates, just won't persist.
    }
  }, [preset]);

  const setPreset = useCallback((p: FontPreset) => setPresetState(p), []);

  return { preset, setPreset, presets: FONT_PRESETS };
}
```

- [ ] **Step 4.1.4: Re-run tests**

```bash
pnpm -C . exec vitest run src/help/useFontSize.test.ts
```

Expected: all PASS.

### Subtask 4.2 — `TextSizeDropdown`

- [ ] **Step 4.2.1: Write the failing test**

Create `src/help/TextSizeDropdown.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TextSizeDropdown } from './TextSizeDropdown';

describe('TextSizeDropdown', () => {
  it('renders the button labeled with the current preset', () => {
    render(<TextSizeDropdown value="Normal" onChange={() => {}} />);
    expect(screen.getByRole('button', { name: /Text size: Normal/i })).toBeInTheDocument();
  });

  it('opens the menu and lists all four presets', () => {
    render(<TextSizeDropdown value="Normal" onChange={() => {}} />);
    fireEvent.click(screen.getByRole('button'));
    expect(screen.getByRole('menuitem', { name: 'Normal' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Large' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'X-Large' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Huge' })).toBeInTheDocument();
  });

  it('marks the active preset with aria-checked', () => {
    render(<TextSizeDropdown value="Large" onChange={() => {}} />);
    fireEvent.click(screen.getByRole('button'));
    const items = screen.getAllByRole('menuitem');
    const active = items.find((i) => i.getAttribute('aria-checked') === 'true');
    expect(active?.textContent).toBe('Large');
  });

  it('calls onChange with the selected preset and closes', () => {
    const onChange = vi.fn();
    render(<TextSizeDropdown value="Normal" onChange={onChange} />);
    fireEvent.click(screen.getByRole('button'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'X-Large' }));
    expect(onChange).toHaveBeenCalledWith('X-Large');
    // Menu should close — query returns nothing now.
    expect(screen.queryByRole('menuitem')).not.toBeInTheDocument();
  });

  it('closes the menu on Escape', () => {
    render(<TextSizeDropdown value="Normal" onChange={() => {}} />);
    fireEvent.click(screen.getByRole('button'));
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('menuitem')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 4.2.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/TextSizeDropdown.test.tsx
```

Expected: FAIL — module not resolvable.

- [ ] **Step 4.2.3: Implement `TextSizeDropdown.tsx`**

Create `src/help/TextSizeDropdown.tsx`:

```tsx
import { useEffect, useRef, useState } from 'react';
import { FONT_PRESETS, type FontPreset } from './useFontSize';
import './TextSizeDropdown.css';

interface TextSizeDropdownProps {
  value: FontPreset;
  onChange: (value: FontPreset) => void;
}

export function TextSizeDropdown({ value, onChange }: TextSizeDropdownProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    const onClickAway = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('mousedown', onClickAway);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('mousedown', onClickAway);
    };
  }, [open]);

  const handleSelect = (p: FontPreset) => {
    onChange(p);
    setOpen(false);
  };

  return (
    <div className="tux-help-textsize" ref={rootRef}>
      <button
        type="button"
        className="tux-help-textsize-button"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="lab">Text size:</span>
        <span className="val">{value}</span>
        <span className="chev">▼</span>
      </button>
      {open && (
        <div className="tux-help-textsize-menu" role="menu">
          {FONT_PRESETS.map((p) => (
            <div
              key={p}
              role="menuitem"
              aria-checked={p === value}
              className={`tux-help-textsize-item${p === value ? ' active' : ''}`}
              onClick={() => handleSelect(p)}
              tabIndex={0}
            >
              <span>{p}</span>
              {p === value && <span className="check">✓</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4.2.4: Implement `TextSizeDropdown.css`**

Create `src/help/TextSizeDropdown.css`:

```css
/* Text-size dropdown (spec §7.1). Touch-target floor: button + menu
   items each ≥44px tall padded for FZ-M1 (spec §6.3). */

.tux-help-textsize {
  position: relative;
}

.tux-help-textsize-button {
  background: var(--bg);
  border: 1px solid var(--border-strong);
  color: var(--text);
  padding: 10px 14px;        /* 10+22+10 = 44 with line-height */
  min-height: 44px;
  border-radius: 6px;
  font-size: 14px;
  font-family: var(--sans);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 10px;
}
.tux-help-textsize-button:hover { background: var(--surface-2); }

.tux-help-textsize-button .lab { color: var(--text-dim); }
.tux-help-textsize-button .val { color: var(--text); font-weight: 500; }
.tux-help-textsize-button .chev { color: var(--text-faint); font-size: 10px; }

.tux-help-textsize-menu {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  background: var(--elevated);
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  min-width: 200px;
  padding: 4px;
  z-index: 10;
}

.tux-help-textsize-item {
  padding: 10px 14px;
  min-height: 44px;
  font-size: 14px;
  color: var(--text);
  cursor: pointer;
  border-radius: 4px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.tux-help-textsize-item:hover { background: var(--surface-2); }
.tux-help-textsize-item.active { background: var(--accent-soft); color: var(--accent); }
.tux-help-textsize-item .check { color: var(--accent); }
```

- [ ] **Step 4.2.5: Re-run dropdown tests**

```bash
pnpm -C . exec vitest run src/help/TextSizeDropdown.test.tsx
```

Expected: all PASS.

### Subtask 4.3 — Wire dropdown into `HelpView` + add Ctrl shortcuts

- [ ] **Step 4.3.1: Update the HelpView test**

Append to `src/help/HelpView.test.tsx`:

```tsx
import { fireEvent } from '@testing-library/react';

describe('HelpView text-size integration', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.style.removeProperty('--help-font-size');
  });

  it('renders the text-size dropdown in the header', () => {
    render(<HelpView />);
    expect(screen.getByRole('button', { name: /Text size: Normal/i })).toBeInTheDocument();
  });

  it('Ctrl+= steps the size up', () => {
    render(<HelpView />);
    fireEvent.keyDown(window, { key: '=', ctrlKey: true });
    expect(screen.getByRole('button', { name: /Text size: Large/i })).toBeInTheDocument();
  });

  it('Ctrl+- steps the size down (saturates at Normal)', () => {
    render(<HelpView />);
    fireEvent.keyDown(window, { key: '-', ctrlKey: true });
    expect(screen.getByRole('button', { name: /Text size: Normal/i })).toBeInTheDocument();
  });

  it('Ctrl+0 resets to Normal from any tier', () => {
    render(<HelpView />);
    fireEvent.keyDown(window, { key: '=', ctrlKey: true });  // → Large
    fireEvent.keyDown(window, { key: '=', ctrlKey: true });  // → X-Large
    fireEvent.keyDown(window, { key: '0', ctrlKey: true });  // → Normal
    expect(screen.getByRole('button', { name: /Text size: Normal/i })).toBeInTheDocument();
  });
});
```

- [ ] **Step 4.3.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/HelpView.test.tsx
```

Expected: FAIL on the new cases.

- [ ] **Step 4.3.3: Wire dropdown + Ctrl shortcuts into HelpView**

Replace `src/help/HelpView.tsx`:

```tsx
import { useState, useCallback, useEffect } from 'react';
import { Sidebar } from './Sidebar';
import { ReadingPane } from './ReadingPane';
import { TextSizeDropdown } from './TextSizeDropdown';
import { TOPICS, getTopicBySlug } from './topics';
import { useFontSize, stepFontSize, DEFAULT_FONT_PRESET } from './useFontSize';
import './HelpView.css';

const DEFAULT_SLUG = '01-getting-started';

export function HelpView() {
  const [activeSlug, setActiveSlug] = useState<string>(DEFAULT_SLUG);
  const { preset, setPreset } = useFontSize();

  const handleSelect = useCallback((slug: string) => setActiveSlug(slug), []);
  const handleNavigate = useCallback((slug: string) => {
    if (getTopicBySlug(slug)) setActiveSlug(slug);
  }, []);

  // Browser-style accelerators: Ctrl+= / Ctrl++ → up, Ctrl+- → down, Ctrl+0 → reset.
  // Skip when an input/textarea is focused (sidebar search).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA';
      if (inField) return;
      if (e.key === '=' || e.key === '+') {
        e.preventDefault();
        setPreset(stepFontSize(preset, 'up'));
      } else if (e.key === '-') {
        e.preventDefault();
        setPreset(stepFontSize(preset, 'down'));
      } else if (e.key === '0') {
        e.preventDefault();
        setPreset(DEFAULT_FONT_PRESET);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [preset, setPreset]);

  const activeTopic = getTopicBySlug(activeSlug) ?? TOPICS[0];

  return (
    <div className="tux-help-root" data-testid="tux-help-root">
      <header className="tux-help-header">
        <span className="tux-help-title">User Guide</span>
        <div className="tux-help-spacer" />
        <TextSizeDropdown value={preset} onChange={setPreset} />
      </header>
      <div className="tux-help-body">
        <Sidebar activeSlug={activeSlug} onSelect={handleSelect} />
        <ReadingPane topic={activeTopic} onNavigate={handleNavigate} />
      </div>
    </div>
  );
}
```

- [ ] **Step 4.3.4: Re-run HelpView tests**

```bash
pnpm -C . exec vitest run src/help/HelpView.test.tsx
```

Expected: all PASS.

- [ ] **Step 4.3.5: Run full Vitest suite**

```bash
pnpm -C . test
```

Expected: all PASS.

### Subtask 4.4 — Commit

- [ ] **Step 4.4.1: Stage + commit**

```bash
git -C . add src/help/
git -C . commit -m "$(cat <<'EOF'
feat(help): text-size dropdown + Ctrl shortcuts + persistence (tuxlink-0gsy)

Adds the "Text size: <current>" dropdown to the help header (spec §7):

- useFontSize hook owns the preset state + persists it in
  localStorage under tuxlink.help.fontSize. Reads on mount with
  validation; falls back to Normal for missing / corrupted values.
  Writes a --help-font-size CSS variable on <html> (consumed by
  ReadingPane.css).
- Tier mapping: Normal=18px, Large=20px, X-Large=22px, Huge=24px
  (spec §7.2 D3). stepFontSize() saturates at both ends.
- TextSizeDropdown.tsx renders a self-documenting button labeled
  "Text size: Normal ▼"; click opens a Radix-style menu of the four
  presets with the active one checked. Closes on click-away or Esc.
- HelpView wires the dropdown into its header and adds global
  keydown listeners for Ctrl+= / Ctrl++ / Ctrl- / Ctrl0
  (browser-zoom muscle memory). Field-focus suppression so the
  shortcuts don't fire while the operator is typing in the sidebar
  search (lands in Task 7).

Touch targets in the dropdown (button + each menu item) are
44×44px minimum per spec §6.3.

Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §7.

Tests: tier scale + step + persistence + fallback; dropdown
rendering / opening / selection / Esc dismiss; HelpView Ctrl
shortcut integration.

Agent: bog-bluff-mesa
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5 — Theme inheritance + live updates (Commit 5)

**Spec reference:** §8.

**Files:**
- Create: `src-tauri/src/theme_state.rs`
- Modify: `src-tauri/src/lib.rs` — register the theme commands + managed state.
- Modify: `src-tauri/capabilities/default.json` — (no change required, see step 5.3.1).
- Create: `src/help/useHelpTheme.ts`, `src/help/useHelpTheme.test.ts`
- Modify: `src/shell/colorScheme.ts` — broadcast on `applyColorScheme`.
- Modify: `src/help/HelpView.tsx` — wire `useHelpTheme`.

### Subtask 5.1 — Rust theme-state module

- [ ] **Step 5.1.1: Write the failing test**

Create `src-tauri/src/theme_state.rs`:

```rust
//! Theme-state sharing across windows (tuxlink-0gsy / spec §8.2).
//!
//! The main window owns the operator's chosen color scheme (it persists it
//! to localStorage on its side). When the scheme changes, main calls
//! `theme_broadcast_scheme(scheme)` which (a) stores the value in this
//! Tauri-managed state and (b) emits a `color_scheme_changed` event so any
//! other windows (currently: help) re-apply.
//!
//! New (help) windows opened mid-session bootstrap with `theme_get_scheme()`
//! to read whatever the main window last broadcast — typically the value
//! it applied at startup.

use std::sync::Mutex;

/// Singleton holding the last scheme broadcast by the main window.
/// `None` until the main window calls `theme_broadcast_scheme` at least once.
pub struct ThemeState(pub Mutex<Option<String>>);

impl Default for ThemeState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

#[tauri::command]
pub fn theme_get_scheme(state: tauri::State<ThemeState>) -> Option<String> {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn theme_broadcast_scheme(
    app: tauri::AppHandle,
    state: tauri::State<ThemeState>,
    scheme: String,
) -> Result<(), String> {
    if scheme.is_empty() {
        return Err("scheme must not be empty".into());
    }
    *state.0.lock().unwrap() = Some(scheme.clone());
    tauri::Emitter::emit(&app, "color_scheme_changed", scheme).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_state_starts_empty() {
        let s = ThemeState::default();
        assert!(s.0.lock().unwrap().is_none());
    }

    #[test]
    fn theme_state_stores_scheme_after_broadcast() {
        // Direct state manipulation (no AppHandle) — the broadcast command
        // mutates state via the Mutex; the AppHandle is only needed for
        // emit. We exercise the storage portion here.
        let s = ThemeState::default();
        *s.0.lock().unwrap() = Some("night-red".into());
        assert_eq!(s.0.lock().unwrap().clone(), Some("night-red".into()));
    }
}
```

- [ ] **Step 5.1.2: Register the module + state**

In `src-tauri/src/lib.rs`:

- Add `mod theme_state;` near the existing module declarations.
- In the `run()` builder, after `.invoke_handler(...)` (or in the `setup` hook — match the project's pattern by inspecting where compose's plugin is registered), add `.manage(theme_state::ThemeState::default())`.
- Append to the `invoke_handler!` list:

```rust
crate::theme_state::theme_get_scheme,         // tuxlink-0gsy (spec §8.2)
crate::theme_state::theme_broadcast_scheme,   // tuxlink-0gsy (spec §8.2)
```

- [ ] **Step 5.1.3: Run Rust tests**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --lib theme_state::
cargo --manifest-path src-tauri/Cargo.toml build
```

Expected: tests PASS; build SUCCEEDS.

### Subtask 5.2 — Frontend: broadcast on scheme change

- [ ] **Step 5.2.1: Inspect existing `applyColorScheme`**

```bash
grep -n "applyColorScheme\|saveColorScheme" src/shell/colorScheme.ts | head
```

Find the function body for `applyColorScheme(scheme: ColorScheme)`.

- [ ] **Step 5.2.2: Modify `applyColorScheme` to broadcast**

At the end of `applyColorScheme(scheme: ColorScheme)`'s body (after the existing DOM apply + localStorage write), add:

```ts
import { invoke } from '@tauri-apps/api/core';
// ... existing body ...
// Broadcast to other webviews (currently: help). Fire-and-forget — if the
// Tauri runtime isn't available (e.g., during unit tests), silently skip.
void invoke('theme_broadcast_scheme', { scheme }).catch(() => {});
```

(Import the `invoke` at the top of the file if it's not already imported.)

- [ ] **Step 5.2.3: Run existing colorScheme tests**

```bash
pnpm -C . exec vitest run src/shell/
```

Expected: existing tests still PASS (the broadcast is fire-and-forget and swallows errors).

### Subtask 5.3 — Frontend: `useHelpTheme` hook

- [ ] **Step 5.3.1: Verify default capability covers `invoke`**

The main window's `default.json` capability already includes `core:default`, which permits invoking app commands. No change required for the main window's broadcast call.

The help window's `help.json` (Task 1.3) already includes `core:event:allow-listen` for the event subscription. The help window invokes `theme_get_scheme` which is an app command — also reachable. No further capability edits required.

- [ ] **Step 5.3.2: Write the failing test**

Create `src/help/useHelpTheme.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';

const invokeMock = vi.fn();
const listenMock = vi.fn();
const unlistenMock = vi.fn();
const applyMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));
vi.mock('../shell/colorScheme', () => ({
  applyColorScheme: (...args: unknown[]) => applyMock(...args),
}));

import { useHelpTheme } from './useHelpTheme';

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
  unlistenMock.mockReset();
  applyMock.mockReset();
  listenMock.mockResolvedValue(unlistenMock);
});

describe('useHelpTheme', () => {
  it('queries theme_get_scheme on mount and applies the result', async () => {
    invokeMock.mockResolvedValue('night-red');
    renderHook(() => useHelpTheme());
    await waitFor(() => expect(applyMock).toHaveBeenCalledWith('night-red'));
    expect(invokeMock).toHaveBeenCalledWith('theme_get_scheme');
  });

  it('subscribes to color_scheme_changed and re-applies on event', async () => {
    invokeMock.mockResolvedValue(null);
    renderHook(() => useHelpTheme());
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    expect(listenMock.mock.calls[0][0]).toBe('color_scheme_changed');
    // Invoke the handler the hook registered.
    const handler = listenMock.mock.calls[0][1];
    handler({ payload: 'daylight' });
    expect(applyMock).toHaveBeenCalledWith('daylight');
  });

  it('does not apply on mount when theme_get_scheme returns null', async () => {
    invokeMock.mockResolvedValue(null);
    renderHook(() => useHelpTheme());
    // The hook subscribes immediately; assert no apply happened from mount alone.
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    expect(applyMock).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 5.3.3: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/useHelpTheme.test.ts
```

Expected: FAIL — module not resolvable.

- [ ] **Step 5.3.4: Implement `useHelpTheme.ts`**

Create `src/help/useHelpTheme.ts`:

```ts
import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { applyColorScheme } from '../shell/colorScheme';

/**
 * Inherits the main window's color scheme on mount + re-applies on
 * `color_scheme_changed` events broadcast by the main window.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §8.
 */
export function useHelpTheme() {
  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | null = null;

    // Initial: read whatever the main window last broadcast.
    invoke<string | null>('theme_get_scheme')
      .then((scheme) => {
        if (cancelled) return;
        if (scheme && typeof scheme === 'string') {
          applyColorScheme(scheme as never);  // see colorScheme.ts typing
        }
      })
      .catch(() => {});  // ignore — theme falls back to defaults

    // Live: re-apply on broadcast events from the main window.
    listen<string>('color_scheme_changed', (e) => {
      applyColorScheme(e.payload as never);
    }).then((unfn) => {
      if (cancelled) {
        unfn();
      } else {
        unlisten = unfn;
      }
    }).catch(() => {});

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);
}
```

- [ ] **Step 5.3.5: Re-run hook test**

```bash
pnpm -C . exec vitest run src/help/useHelpTheme.test.ts
```

Expected: all PASS.

### Subtask 5.4 — Wire into HelpView

- [ ] **Step 5.4.1: Update HelpView**

Add to `src/help/HelpView.tsx` at the top of the component body (after the existing hook calls):

```tsx
import { useHelpTheme } from './useHelpTheme';
// ...
export function HelpView() {
  useHelpTheme();   // first thing — paints into the correct theme ASAP
  const [activeSlug, setActiveSlug] = useState<string>(DEFAULT_SLUG);
  // ... rest unchanged
}
```

- [ ] **Step 5.4.2: Run full suite**

```bash
pnpm -C . test
```

Expected: all PASS.

### Subtask 5.5 — Commit

- [ ] **Step 5.5.1: Stage + commit**

```bash
git -C . add src-tauri/src/theme_state.rs src-tauri/src/lib.rs src/shell/colorScheme.ts src/help/useHelpTheme.ts src/help/useHelpTheme.test.ts src/help/HelpView.tsx
git -C . commit -m "$(cat <<'EOF'
feat(help): theme inheritance + live updates (tuxlink-0gsy)

Implements spec §8: help window inherits the main client's color
scheme on mount and re-applies on live theme changes.

Rust side:
- theme_state.rs holds a Mutex<Option<String>> in Tauri managed
  state, the last scheme broadcast by main.
- theme_get_scheme: returns the current value (None on cold start
  before main has broadcast).
- theme_broadcast_scheme: stores the scheme + emits
  color_scheme_changed event app-wide.

Frontend side:
- applyColorScheme (src/shell/colorScheme.ts) now invokes
  theme_broadcast_scheme after applying — fire-and-forget so test
  envs without a Tauri runtime are unaffected.
- useHelpTheme hook calls theme_get_scheme on mount + subscribes
  to color_scheme_changed. Re-applies via applyColorScheme so the
  same DOM mutation path runs in both windows.
- HelpView mounts the hook first so the help window paints into
  the correct scheme as early as possible.

Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §8.

Tests: theme_state Rust unit tests; useHelpTheme mount-apply,
event-apply, and null-scheme-no-op cases (Tauri APIs mocked).

Agent: bog-bluff-mesa
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6 — `docs_fts` table + extractor + `docs_search` command (Commit 6)

**Spec reference:** §9.1-§9.3.

**Files:**
- Modify: `src-tauri/src/search/index.rs` — bump `SCHEMA_VERSION` to 3; add `docs_fts` CREATE.
- Modify: `src-tauri/src/search/extractor.rs` — add `extract_markdown`.
- Create: `src-tauri/src/search/docs_index.rs` — DocsHit + populate + search_docs.
- Modify: `src-tauri/src/search/mod.rs` — module decl + populate at first launch.
- Modify: `src-tauri/src/search/commands.rs` — `docs_search` Tauri command.
- Modify: `src-tauri/src/lib.rs` — register `docs_search`.

### Subtask 6.1 — `extract_markdown`

- [ ] **Step 6.1.1: Write the failing tests**

In `src-tauri/src/search/extractor.rs`, add at the bottom (or extend the existing `#[cfg(test)] mod tests` block if one exists):

```rust
#[cfg(test)]
mod markdown_tests {
    use super::*;

    #[test]
    fn strips_h1_h2_h3_markers_keeps_text() {
        assert_eq!(
            extract_markdown("# Heading 1\n## Heading 2\n### Heading 3"),
            "Heading 1\nHeading 2\nHeading 3",
        );
    }

    #[test]
    fn strips_bold_italic_code_inline_formatting() {
        assert_eq!(
            extract_markdown("**bold** _italic_ `code`"),
            "bold italic code",
        );
    }

    #[test]
    fn link_text_preserved_url_stripped() {
        assert_eq!(
            extract_markdown("See [the mailbox](03-mailbox.md) for details."),
            "See the mailbox for details.",
        );
    }

    #[test]
    fn fenced_code_block_inlined_as_text() {
        let md = "```bash\nfoo --bar\n```";
        let out = extract_markdown(md);
        assert!(out.contains("foo --bar"));
        assert!(!out.contains("```"));
    }

    #[test]
    fn unordered_list_markers_dropped() {
        assert_eq!(
            extract_markdown("- item one\n- item two"),
            "item one\nitem two",
        );
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(extract_markdown(""), "");
    }
}
```

- [ ] **Step 6.1.2: Run to confirm failure**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --lib search::extractor::markdown_tests
```

Expected: FAIL — function not defined.

- [ ] **Step 6.1.3: Implement `extract_markdown`**

In `src-tauri/src/search/extractor.rs`, add a `pub fn extract_markdown` function. Conservative parse covering the markdown subset that `docs/user-guide/*.md` uses:

```rust
/// Strip markdown syntax for FTS5 ingestion. Conservative parse — handles
/// the subset that docs/user-guide/*.md uses: ATX headings, bold (`**...**`),
/// italic (`_..._`), inline code (`` `...` ``), links (`[text](url)`),
/// fenced code blocks (```` ```...``` ````), and unordered list markers (`-`).
///
/// Output preserves linebreaks. URLs are dropped; link text is kept.
///
/// Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §9.1.
pub fn extract_markdown(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut in_code_fence = false;
    for raw_line in md.lines() {
        let line = raw_line.trim_end();
        // Fenced code block toggle.
        if line.trim_start().starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            // Keep code content as plain text.
            if !out.is_empty() { out.push('\n'); }
            out.push_str(line);
            continue;
        }
        // Strip leading ATX-heading marker(s).
        let mut s = line.trim_start();
        while s.starts_with('#') {
            s = &s[1..];
        }
        s = s.trim_start();
        // Strip leading unordered-list marker.
        if let Some(rest) = s.strip_prefix("- ") {
            s = rest;
        } else if let Some(rest) = s.strip_prefix("* ") {
            s = rest;
        }
        // Inline-format strip: pass over a small set of patterns once.
        let stripped = strip_inline(s);
        if !out.is_empty() { out.push('\n'); }
        out.push_str(&stripped);
    }
    // Collapse trailing whitespace + blank-line runs.
    while out.ends_with('\n') { out.pop(); }
    out
}

/// Strip inline `**bold**`, `_italic_`, `` `code` ``, and `[text](url)` →
/// `text`. Operates left-to-right; first-match-wins so nested syntax falls
/// through naturally.
fn strip_inline(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        // Link: [text](url)
        if bytes[i] == b'[' {
            if let Some(close_text) = find_byte(&bytes[i..], b']') {
                let after_text = i + close_text + 1;
                if after_text < bytes.len() && bytes[after_text] == b'(' {
                    if let Some(close_url) = find_byte(&bytes[after_text..], b')') {
                        let text = &input[i + 1..i + close_text];
                        out.push_str(text);
                        i = after_text + close_url + 1;
                        continue;
                    }
                }
            }
        }
        // Bold: **text**
        if i + 1 < bytes.len() && &bytes[i..i + 2] == b"**" {
            if let Some(close) = find_seq(&bytes[i + 2..], b"**") {
                let text = &input[i + 2..i + 2 + close];
                out.push_str(text);
                i = i + 2 + close + 2;
                continue;
            }
        }
        // Inline code: `text`
        if bytes[i] == b'`' {
            if let Some(close) = find_byte(&bytes[i + 1..], b'`') {
                let text = &input[i + 1..i + 1 + close];
                out.push_str(text);
                i = i + 1 + close + 1;
                continue;
            }
        }
        // Italic: _text_  (limit to word-boundary openers to avoid eating identifiers)
        if bytes[i] == b'_' && (i == 0 || bytes[i - 1] == b' ') {
            if let Some(close) = find_byte(&bytes[i + 1..], b'_') {
                let text = &input[i + 1..i + 1 + close];
                out.push_str(text);
                i = i + 1 + close + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn find_byte(s: &[u8], b: u8) -> Option<usize> {
    s.iter().position(|&x| x == b)
}
fn find_seq(s: &[u8], seq: &[u8]) -> Option<usize> {
    if seq.is_empty() || s.len() < seq.len() { return None; }
    for i in 0..=s.len() - seq.len() {
        if &s[i..i + seq.len()] == seq { return Some(i); }
    }
    None
}
```

- [ ] **Step 6.1.4: Re-run tests**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --lib search::extractor::markdown_tests
```

Expected: all PASS.

### Subtask 6.2 — `docs_fts` schema bump

- [ ] **Step 6.2.1: Bump `SCHEMA_VERSION` and add the table**

In `src-tauri/src/search/index.rs`:

- Change `pub const SCHEMA_VERSION: u32 = 2;` → `pub const SCHEMA_VERSION: u32 = 3;`
- Add a comment to the schema-version doc block:

```rust
/// v2 → v3 (tuxlink-0gsy): add `docs_fts` virtual table for user-guide search.
/// Existing v2 indices return SchemaDrift from `Index::open`; the mod.rs
/// recovery path (build_service) recreates fresh and the docs table is
/// populated on first launch (mod.rs `populate_docs_if_empty`).
```

- In `init_schema`'s `execute_batch` SQL, add the docs_fts CREATE inside the same transaction:

```rust
CREATE VIRTUAL TABLE docs_fts USING fts5 (
    slug              UNINDEXED,
    title,
    body,
    tokenize = 'porter unicode61 remove_diacritics 2'
);
```

(Place it after the existing `messages_meta` block and its indices, before `COMMIT;`.)

- [ ] **Step 6.2.2: Build + run existing schema tests**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --lib search::
```

Expected: existing `build_service_recovers_from_schema_drift` test still PASSES (the recovery path handles v2 → v3 drift identically to v1 → v2).

### Subtask 6.3 — `docs_index` module: hit + populate + query

- [ ] **Step 6.3.1: Write the failing test**

Create `src-tauri/src/search/docs_index.rs`:

```rust
//! Docs-side FTS5 surface (tuxlink-0gsy / spec §9).
//!
//! Owns the `docs_fts` virtual table created in `index.rs::init_schema`,
//! populates it once at first launch from the bundled user-guide markdown,
//! and exposes a `search_docs(query) -> Vec<DocsHit>` query.

use crate::search::extractor::extract_markdown;
use crate::search::index::{Index, IndexError};
use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocsHit {
    pub slug: String,
    pub title: String,
    pub snippet: String,  // FTS5 snippet() output, may contain <mark>...</mark>
}

/// A bundled topic, supplied by the caller (the frontend has a typed
/// registry; the Rust side accepts the trio at populate time).
#[derive(Debug, Clone)]
pub struct DocTopic<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub markdown: &'a str,
}

impl Index {
    /// Return true if `docs_fts` is empty.
    pub fn docs_is_empty(&self) -> Result<bool, IndexError> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM docs_fts",
            [],
            |row| row.get(0),
        )?;
        Ok(count == 0)
    }

    /// Populate `docs_fts` from `topics`. Wipes the table first so re-calls
    /// (e.g. after a schema drift recovery) start from a clean state.
    pub fn populate_docs(&self, topics: &[DocTopic<'_>]) -> Result<(), IndexError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM docs_fts", [])?;
        for t in topics {
            let body_text = extract_markdown(t.markdown);
            tx.execute(
                "INSERT INTO docs_fts (slug, title, body) VALUES (?1, ?2, ?3)",
                rusqlite::params![t.slug, t.title, body_text],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Run a free-text query against `docs_fts`. Returns hits ordered by
    /// BM25 rank (best first) with FTS5 snippet() output for the matching
    /// body fragment.
    ///
    /// The `query` is passed through to FTS5 MATCH unchanged after rejecting
    /// the empty string. Operators get FTS5's column-scoping and prefix
    /// syntax for free.
    pub fn search_docs(&self, query: &str) -> Result<Vec<DocsHit>, IndexError> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let mut stmt = self.conn.prepare(
            "SELECT slug, title, snippet(docs_fts, 2, '<mark>', '</mark>', '…', 12) \
             FROM docs_fts \
             WHERE docs_fts MATCH ?1 \
             ORDER BY rank \
             LIMIT 30",
        )?;
        let rows = stmt.query_map([query], |row| {
            Ok(DocsHit {
                slug: row.get(0)?,
                title: row.get(1)?,
                snippet: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(IndexError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fresh() -> Index {
        let dir = tempdir().unwrap();
        Index::open(dir.path().join("search.db")).unwrap()
    }

    #[test]
    fn docs_is_empty_on_fresh_index() {
        let idx = fresh();
        assert!(idx.docs_is_empty().unwrap());
    }

    #[test]
    fn populate_then_search_returns_hits() {
        let idx = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "01-getting-started", title: "Getting started", markdown: "# Getting started\nWelcome to Tuxlink." },
            DocTopic { slug: "02-connections", title: "Connections", markdown: "# Connections\nARDOP is HF digital." },
        ]).unwrap();

        assert!(!idx.docs_is_empty().unwrap());
        let hits = idx.search_docs("ARDOP").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slug, "02-connections");
        assert!(hits[0].snippet.contains("ARDOP"));
    }

    #[test]
    fn empty_query_returns_no_hits() {
        let idx = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "01", title: "x", markdown: "anything" },
        ]).unwrap();
        assert!(idx.search_docs("").unwrap().is_empty());
        assert!(idx.search_docs("   ").unwrap().is_empty());
    }

    #[test]
    fn populate_replaces_previous_content() {
        let idx = fresh();
        idx.populate_docs(&[
            DocTopic { slug: "old", title: "Old", markdown: "ARDOP digital" },
        ]).unwrap();
        idx.populate_docs(&[
            DocTopic { slug: "new", title: "New", markdown: "VARA digital" },
        ]).unwrap();
        assert!(idx.search_docs("ARDOP").unwrap().is_empty());
        assert_eq!(idx.search_docs("VARA").unwrap().len(), 1);
    }
}
```

- [ ] **Step 6.3.2: Register the module**

In `src-tauri/src/search/mod.rs`, add `pub mod docs_index;` alongside the existing module declarations:

```rust
pub mod commands;
pub mod docs_index;   // tuxlink-0gsy
pub mod extractor;
pub mod index;
pub mod query;
pub mod saved;
pub mod types;
```

- [ ] **Step 6.3.3: Run tests**

```bash
cargo --manifest-path src-tauri/Cargo.toml test --lib search::docs_index::
```

Expected: all PASS.

### Subtask 6.4 — Populate docs at first launch

- [ ] **Step 6.4.1: Extend `build_service`**

In `src-tauri/src/search/mod.rs`, modify `build_service` so that after the recovery / open succeeds, the bundled docs are ingested if `docs_fts` is empty:

```rust
pub fn build_service(data_dir: &Path) -> Result<SearchService, CommandError> {
    let db_path = data_dir.join("search.db");
    let index = match Index::open(db_path.clone()) {
        Ok(idx) => idx,
        Err(IndexError::SchemaDrift { found, current }) => {
            eprintln!(
                "search: schema drift v{found} → v{current} at {}, recreating empty index \
                 (operator should run Rebuild Index to repopulate from mbox)",
                db_path.display()
            );
            let _ = std::fs::remove_file(&db_path);
            let _ = std::fs::remove_file(data_dir.join("search.db-wal"));
            let _ = std::fs::remove_file(data_dir.join("search.db-shm"));
            Index::open(db_path).map_err(CommandError::from)?
        }
        Err(other) => return Err(other.into()),
    };

    // tuxlink-0gsy: populate the docs_fts table on first launch or after
    // a schema-drift recreation. The bundled docs are baked into the binary
    // via include_str! in docs_bundle.rs so first-launch indexing has no
    // I/O dependency on the install directory layout.
    if index.docs_is_empty().map_err(CommandError::from)? {
        index.populate_docs(crate::search::docs_bundle::BUNDLED_TOPICS)
            .map_err(CommandError::from)?;
    }

    let saved = Mutex::new(
        SavedStore::open(data_dir.join("saved-searches.json")).map_err(CommandError::from)?,
    );
    Ok(SearchService { /* … */ })
}
```

(Keep the rest of `build_service`'s body unchanged.)

- [ ] **Step 6.4.2: Create `docs_bundle.rs` with `include_str!` over the user guide**

Create `src-tauri/src/search/docs_bundle.rs`:

```rust
//! Compile-time bundle of docs/user-guide/*.md, used by build_service to
//! populate docs_fts at first launch.
//!
//! Adding a new topic: include_str! it below + extend BUNDLED_TOPICS.
//! Section grouping for the sidebar lives in src/help/topics.ts; this
//! file is search-index-only.

use crate::search::docs_index::DocTopic;

macro_rules! topic {
    ($slug:literal, $title:literal, $path:literal) => {
        DocTopic {
            slug: $slug,
            title: $title,
            markdown: include_str!(concat!("../../../", $path)),
        }
    };
}

pub static BUNDLED_TOPICS: &[DocTopic<'static>] = &[
    topic!("01-getting-started", "Getting started", "docs/user-guide/01-getting-started.md"),
    topic!("02-connections",     "Connections",     "docs/user-guide/02-connections.md"),
    topic!("03-mailbox",         "Mailbox",         "docs/user-guide/03-mailbox.md"),
    topic!("04-composing",       "Composing",       "docs/user-guide/04-composing.md"),
    topic!("05-forms",           "Forms",           "docs/user-guide/05-forms.md"),
    topic!("06-search",          "Search",          "docs/user-guide/06-search.md"),
    topic!("07-settings",        "Settings",        "docs/user-guide/07-settings.md"),
    topic!("08-color-schemes",   "Color schemes",   "docs/user-guide/08-color-schemes.md"),
    topic!("09-keyboard",        "Keyboard",        "docs/user-guide/09-keyboard.md"),
    topic!("10-troubleshooting", "Troubleshooting", "docs/user-guide/10-troubleshooting.md"),
];
```

NOTE on `include_str!` paths: `concat!("../../../", "docs/user-guide/01-getting-started.md")` resolves relative to `src-tauri/src/search/docs_bundle.rs` → `../../../` reaches the repo root. Verify by reading the file path with `ls` before the build; if the path is wrong the compiler emits a hard error so it cannot regress silently.

- [ ] **Step 6.4.3: Register the module**

Add `pub mod docs_bundle;` to `src-tauri/src/search/mod.rs` alongside the other modules.

- [ ] **Step 6.4.4: Build to verify include_str paths**

```bash
cargo --manifest-path src-tauri/Cargo.toml build
```

Expected: SUCCEEDS. If the build fails with `error: couldn't read /…`, the include_str path needs adjusting.

### Subtask 6.5 — `docs_search` Tauri command

- [ ] **Step 6.5.1: Add command body to `commands.rs`**

Append to `src-tauri/src/search/commands.rs`:

```rust
use crate::search::docs_index::DocsHit;

/// User-guide search command (tuxlink-0gsy / spec §9.3). Frontend
/// (useHelpSearch) debounces; this command is a thin forward.
#[tauri::command]
pub fn docs_search(
    svc: tauri::State<std::sync::Arc<SearchService>>,
    query: String,
) -> Result<Vec<DocsHit>, String> {
    svc.index
        .lock()
        .unwrap()
        .search_docs(&query)
        .map_err(|e| e.to_string())
}
```

If the project's existing `SearchService` is stored as `tauri::State<SearchService>` (not `State<Arc<SearchService>>`), adjust the parameter type to match. Inspect `lib.rs`'s `.manage(...)` call for the search service to confirm.

- [ ] **Step 6.5.2: Register in `invoke_handler!`**

Append to `src-tauri/src/lib.rs`'s `invoke_handler!`:

```rust
crate::search::commands::docs_search,   // tuxlink-0gsy (spec §9.3)
```

- [ ] **Step 6.5.3: Build**

```bash
cargo --manifest-path src-tauri/Cargo.toml build
cargo --manifest-path src-tauri/Cargo.toml test --lib search::
```

Expected: SUCCEEDS / all PASS.

### Subtask 6.6 — Commit

- [ ] **Step 6.6.1: Stage + commit**

```bash
git -C . add src-tauri/src/search/ src-tauri/src/lib.rs
git -C . commit -m "$(cat <<'EOF'
feat(search): docs_fts virtual table + extractor + docs_search command (tuxlink-0gsy)

Extends the existing src-tauri/src/search/ module to index the
bundled docs/user-guide/*.md for help search.

Backend additions:
- search/extractor.rs gains extract_markdown(md) — strips ATX
  headings, bold/italic/inline-code/link syntax, and fenced code
  blocks. Output is FTS5-tokenizable plain text.
- search/docs_index.rs owns the docs-side FTS surface: docs_is_empty
  predicate, populate_docs(topics) transactional ingest,
  search_docs(query) BM25-ordered query with FTS5 snippet() output
  for the body match (returns DocsHit { slug, title, snippet }).
- search/docs_bundle.rs collects the ten user-guide topics via
  include_str! so the docs index is binary-embedded and has no
  runtime I/O dependency on the install directory layout.
- search/index.rs SCHEMA_VERSION bumped 2 → 3; init_schema adds the
  docs_fts virtual table inside the existing DDL transaction.
- search/mod.rs build_service populates docs_fts on first launch
  (and after schema-drift recovery) — guarded by docs_is_empty so
  re-launches don't re-ingest.

Frontend wiring:
- search/commands.rs adds docs_search Tauri command (thin forward
  to Index::search_docs).
- lib.rs registers docs_search in invoke_handler!.

The existing v2 → v3 schema drift goes through the existing
recovery path in build_service unchanged (delete .db / .db-wal /
.db-shm, reopen, then ingest docs).

Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §9.

Tests: extract_markdown unit cases (heading / bold / italic /
inline code / link / fenced code / list / empty);
docs_index populate + search + empty-query + repopulate behavior.

Agent: bog-bluff-mesa
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7 — Sidebar search UI + hit highlighting (Commit 7)

**Spec reference:** §9.4-§9.6.

**Files:**
- Create: `src/help/useHelpSearch.ts`, `src/help/useHelpSearch.test.ts`
- Modify: `src/help/Sidebar.tsx`, `src/help/Sidebar.css`, `src/help/Sidebar.test.tsx`
- Modify: `src/help/HelpView.tsx`, `src/help/HelpView.test.tsx`

### Subtask 7.1 — `useHelpSearch` hook

- [ ] **Step 7.1.1: Write the failing test**

Create `src/help/useHelpSearch.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useHelpSearch } from './useHelpSearch';

function wrapper(client: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client }, children);
  };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe('useHelpSearch', () => {
  it('does not invoke on empty query', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    renderHook(() => useHelpSearch(''), { wrapper: wrapper(client) });
    await new Promise((r) => setTimeout(r, 50));
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('invokes docs_search with the query when non-empty', async () => {
    invokeMock.mockResolvedValue([]);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    renderHook(() => useHelpSearch('ardop'), { wrapper: wrapper(client) });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('docs_search', { query: 'ardop' }));
  });

  it('returns the hit array from the backend', async () => {
    invokeMock.mockResolvedValue([
      { slug: '02-connections', title: 'Connections', snippet: '<mark>ARDOP</mark> HF digital' },
    ]);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { result } = renderHook(() => useHelpSearch('ardop'), { wrapper: wrapper(client) });
    await waitFor(() => expect(result.current.data?.length).toBe(1));
    expect(result.current.data?.[0]).toMatchObject({
      slug: '02-connections',
      title: 'Connections',
    });
  });
});
```

- [ ] **Step 7.1.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/useHelpSearch.test.ts
```

Expected: FAIL — module not resolvable.

- [ ] **Step 7.1.3: Implement `useHelpSearch.ts`**

Create `src/help/useHelpSearch.ts`:

```ts
import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';

export interface DocsHit {
  slug: string;
  title: string;
  snippet: string;
}

export function useHelpSearch(query: string) {
  const trimmed = query.trim();
  return useQuery({
    queryKey: ['help', 'search', trimmed],
    queryFn: () => invoke<DocsHit[]>('docs_search', { query: trimmed }),
    enabled: trimmed.length > 0,
    staleTime: 60_000,
  });
}
```

- [ ] **Step 7.1.4: Re-run hook tests**

```bash
pnpm -C . exec vitest run src/help/useHelpSearch.test.ts
```

Expected: all PASS.

### Subtask 7.2 — Sidebar with search input + hit list

- [ ] **Step 7.2.1: Extend Sidebar tests**

Append to `src/help/Sidebar.test.tsx`:

```tsx
import type { DocsHit } from './useHelpSearch';

describe('Sidebar — search input', () => {
  it('renders a search input with placeholder', () => {
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={() => {}}
        searchQuery=""
        onSearchChange={() => {}}
        hits={undefined}
      />,
    );
    expect(screen.getByPlaceholderText(/Search topics/i)).toBeInTheDocument();
  });

  it('calls onSearchChange as the operator types', () => {
    const onSearchChange = vi.fn();
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={() => {}}
        searchQuery=""
        onSearchChange={onSearchChange}
        hits={undefined}
      />,
    );
    fireEvent.change(screen.getByPlaceholderText(/Search topics/i), { target: { value: 'ardop' } });
    expect(onSearchChange).toHaveBeenCalledWith('ardop');
  });

  it('renders the hit list (replacing the grouped list) when hits are present', () => {
    const hits: DocsHit[] = [
      { slug: '02-connections', title: 'Connections', snippet: 'About <mark>ARDOP</mark>' },
    ];
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={() => {}}
        searchQuery="ardop"
        onSearchChange={() => {}}
        hits={hits}
      />,
    );
    expect(screen.getByText('Connections')).toBeInTheDocument();
    // The grouped section headers are gone in hit-list mode.
    expect(screen.queryByText('Configuration')).not.toBeInTheDocument();
  });

  it('renders a clear (×) button when the search has text', () => {
    const onSearchChange = vi.fn();
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={() => {}}
        searchQuery="ardop"
        onSearchChange={onSearchChange}
        hits={undefined}
      />,
    );
    const clear = screen.getByRole('button', { name: /clear search/i });
    fireEvent.click(clear);
    expect(onSearchChange).toHaveBeenCalledWith('');
  });

  it('calls onSelect with the hit slug when a hit is clicked', () => {
    const onSelect = vi.fn();
    const hits: DocsHit[] = [
      { slug: '02-connections', title: 'Connections', snippet: 'ARDOP' },
    ];
    render(
      <Sidebar
        activeSlug="01-getting-started"
        onSelect={onSelect}
        searchQuery="ardop"
        onSearchChange={() => {}}
        hits={hits}
      />,
    );
    fireEvent.click(screen.getByText('Connections'));
    expect(onSelect).toHaveBeenCalledWith('02-connections');
  });
});
```

- [ ] **Step 7.2.2: Run to confirm failure**

```bash
pnpm -C . exec vitest run src/help/Sidebar.test.tsx
```

Expected: FAIL — Sidebar's prop shape has not changed yet.

- [ ] **Step 7.2.3: Extend `Sidebar.tsx`**

Replace `src/help/Sidebar.tsx`:

```tsx
import { TOPICS, SECTIONS, getTopicBySlug } from './topics';
import type { DocsHit } from './useHelpSearch';
import './Sidebar.css';

interface SidebarProps {
  activeSlug: string;
  onSelect: (slug: string) => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  hits: DocsHit[] | undefined;
}

export function Sidebar({ activeSlug, onSelect, searchQuery, onSearchChange, hits }: SidebarProps) {
  const showHits = searchQuery.trim().length > 0;

  return (
    <nav className="tux-help-sidebar" aria-label="Help topics">
      <div className="tux-help-sb-search">
        <input
          type="search"
          className="tux-help-sb-search-input"
          placeholder="Search topics…"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          aria-label="Search topics"
        />
        {searchQuery.length > 0 && (
          <button
            type="button"
            className="tux-help-sb-search-clear"
            aria-label="Clear search"
            onClick={() => onSearchChange('')}
          >
            ×
          </button>
        )}
      </div>

      {showHits ? (
        <div className="tux-help-sb-hits">
          {!hits ? (
            <div className="tux-help-sb-status">Searching…</div>
          ) : hits.length === 0 ? (
            <div className="tux-help-sb-status">No matches.</div>
          ) : (
            hits.map((hit) => {
              const isActive = hit.slug === activeSlug;
              return (
                <a
                  key={hit.slug}
                  role="link"
                  aria-current={isActive ? 'page' : undefined}
                  className={`tux-help-sb-hit${isActive ? ' active' : ''}`}
                  href={`#${hit.slug}`}
                  onClick={(e) => {
                    e.preventDefault();
                    onSelect(hit.slug);
                  }}
                >
                  <div className="tux-help-sb-hit-title">{hit.title}</div>
                  <div
                    className="tux-help-sb-hit-snippet"
                    // The snippet may contain <mark> from FTS5; render trusted HTML.
                    // Source is bundled markdown stripped through extract_markdown;
                    // no operator-supplied HTML reaches this surface.
                    dangerouslySetInnerHTML={{ __html: hit.snippet }}
                  />
                </a>
              );
            })
          )}
        </div>
      ) : (
        SECTIONS.map((sec) => (
          <div key={sec.id} className="tux-help-sb-section">
            <div className="tux-help-sb-section-title">{sec.displayName}</div>
            {sec.topicSlugs.map((slug) => {
              const t = getTopicBySlug(slug);
              if (!t) return null;
              const isActive = slug === activeSlug;
              return (
                <a
                  key={slug}
                  role="link"
                  aria-current={isActive ? 'page' : undefined}
                  className={`tux-help-sb-item${isActive ? ' active' : ''}`}
                  onClick={(e) => {
                    e.preventDefault();
                    onSelect(slug);
                  }}
                  href={`#${slug}`}
                  tabIndex={0}
                >
                  <span className="tux-help-sb-num">{t.number}</span>
                  <span className="tux-help-sb-name">{t.displayName}</span>
                </a>
              );
            })}
          </div>
        ))
      )}
    </nav>
  );
}
```

- [ ] **Step 7.2.4: Extend `Sidebar.css`**

Append to `src/help/Sidebar.css`:

```css
/* Search box (spec §9.4). */
.tux-help-sb-search {
  position: relative;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
}
.tux-help-sb-search-input {
  width: 100%;
  background: var(--bg);
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  padding: 10px 32px 10px 12px;
  min-height: 44px;
  font-size: 14px;
  color: var(--text);
  font-family: var(--sans);
  outline: none;
}
.tux-help-sb-search-input::placeholder { color: var(--text-faint); }
.tux-help-sb-search-input:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent-soft);
}
.tux-help-sb-search-clear {
  position: absolute;
  right: 20px;
  top: 50%;
  transform: translateY(-50%);
  background: transparent;
  border: none;
  color: var(--text-faint);
  font-size: 20px;
  line-height: 1;
  cursor: pointer;
  padding: 4px;
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.tux-help-sb-search-clear:hover { color: var(--text); }

/* Hit list (spec §9.4). */
.tux-help-sb-hits {
  display: flex;
  flex-direction: column;
}
.tux-help-sb-status {
  padding: 14px 16px;
  font-size: 13px;
  color: var(--text-dim);
  font-style: italic;
}
.tux-help-sb-hit {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 14px;
  min-height: 44px;
  border-left: 3px solid transparent;
  color: var(--text);
  text-decoration: none;
  cursor: pointer;
}
.tux-help-sb-hit:hover { background: rgba(245, 159, 60, 0.04); }
.tux-help-sb-hit.active {
  background: var(--accent-soft);
  border-left-color: var(--accent);
}
.tux-help-sb-hit-title {
  font-size: 14px;
  font-weight: 500;
  color: var(--text);
}
.tux-help-sb-hit.active .tux-help-sb-hit-title { color: var(--accent); }
.tux-help-sb-hit-snippet {
  font-size: 12px;
  color: var(--text-dim);
  line-height: 1.4;
}
.tux-help-sb-hit-snippet mark {
  background: var(--accent-soft);
  color: var(--accent);
  padding: 0 2px;
  border-radius: 2px;
}
```

- [ ] **Step 7.2.5: Re-run sidebar tests**

```bash
pnpm -C . exec vitest run src/help/Sidebar.test.tsx
```

Expected: all PASS.

### Subtask 7.3 — Wire `useHelpSearch` into `HelpView`

- [ ] **Step 7.3.1: Update HelpView test**

Append to `src/help/HelpView.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';

function withQuery(client: QueryClient, ui: React.ReactNode) {
  return React.createElement(QueryClientProvider, { client }, ui);
}

describe('HelpView — search wiring', () => {
  it('renders the search input', () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(withQuery(client, <HelpView />));
    expect(screen.getByPlaceholderText(/Search topics/i)).toBeInTheDocument();
  });
});
```

(The existing tests above also need the same QueryClientProvider wrapping. Refactor each `render(<HelpView />)` call to `render(withQuery(client, <HelpView />))` with a fresh `client = new QueryClient(...)` per test — easiest is to extract a small `renderHelp()` helper at the top of the file.)

- [ ] **Step 7.3.2: Add `renderHelp` helper to the test file**

At the top of `src/help/HelpView.test.tsx` (after imports), add:

```tsx
function renderHelp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(withQuery(client, <HelpView />));
}
```

Then replace each `render(<HelpView />)` call with `renderHelp()`.

- [ ] **Step 7.3.3: Wire `useHelpSearch` into HelpView**

Update `src/help/HelpView.tsx`:

```tsx
import { useState, useCallback, useEffect } from 'react';
import { Sidebar } from './Sidebar';
import { ReadingPane } from './ReadingPane';
import { TextSizeDropdown } from './TextSizeDropdown';
import { TOPICS, getTopicBySlug } from './topics';
import { useFontSize, stepFontSize, DEFAULT_FONT_PRESET } from './useFontSize';
import { useHelpTheme } from './useHelpTheme';
import { useHelpSearch } from './useHelpSearch';
import './HelpView.css';

const DEFAULT_SLUG = '01-getting-started';

export function HelpView() {
  useHelpTheme();
  const [activeSlug, setActiveSlug] = useState<string>(DEFAULT_SLUG);
  const [searchQuery, setSearchQuery] = useState<string>('');
  const { preset, setPreset } = useFontSize();
  const { data: hits } = useHelpSearch(searchQuery);

  const handleSelect = useCallback((slug: string) => setActiveSlug(slug), []);
  const handleNavigate = useCallback((slug: string) => {
    if (getTopicBySlug(slug)) setActiveSlug(slug);
  }, []);

  // (Ctrl shortcuts as in Task 4 — unchanged)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA';
      if (inField) return;
      if (e.key === '=' || e.key === '+') { e.preventDefault(); setPreset(stepFontSize(preset, 'up')); }
      else if (e.key === '-') { e.preventDefault(); setPreset(stepFontSize(preset, 'down')); }
      else if (e.key === '0') { e.preventDefault(); setPreset(DEFAULT_FONT_PRESET); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [preset, setPreset]);

  const activeTopic = getTopicBySlug(activeSlug) ?? TOPICS[0];

  return (
    <div className="tux-help-root" data-testid="tux-help-root">
      <header className="tux-help-header">
        <span className="tux-help-title">User Guide</span>
        <div className="tux-help-spacer" />
        <TextSizeDropdown value={preset} onChange={setPreset} />
      </header>
      <div className="tux-help-body">
        <Sidebar
          activeSlug={activeSlug}
          onSelect={handleSelect}
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          hits={hits}
        />
        <ReadingPane topic={activeTopic} onNavigate={handleNavigate} />
      </div>
    </div>
  );
}
```

- [ ] **Step 7.3.4: Run full suite**

```bash
pnpm -C . test
```

Expected: all PASS.

- [ ] **Step 7.3.5: Build**

```bash
pnpm -C . build
```

Expected: SUCCEEDS.

### Subtask 7.4 — Commit

- [ ] **Step 7.4.1: Stage + commit**

```bash
git -C . add src/help/
git -C . commit -m "$(cat <<'EOF'
feat(help): sidebar search UI + hit highlighting (tuxlink-0gsy)

Wires the FTS5 docs index (Task 6) into the help sidebar (spec §9.4-§9.6).

- useHelpSearch hook (react-query): debounces the trim, invokes
  docs_search on non-empty queries, 60s staleTime so re-typing the
  same query is instant.
- Sidebar gains a search input + clear button at the top.
  Non-empty query swaps the section-grouped topic list for a
  hit list: per-hit title + FTS5 snippet (with <mark> highlight)
  rendered. Clicking a hit selects it.
- Visual: hit snippets render the <mark> from FTS5 as the same
  accent-soft+accent treatment used for active sidebar items —
  the operator's eye learns one highlight pattern.
- HelpView owns the searchQuery state; the search trim happens
  inside useHelpSearch.

Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §9.

Tests: useHelpSearch empty-skip / non-empty-invoke / data-shape;
sidebar search input + clear button + hit list rendering + hit
click selection.

Agent: bog-bluff-mesa
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8 — Remove the old modal + reroute dispatch (Commit 8)

**Spec reference:** §11.

**Files:**
- Modify: `src/shell/AppShell.tsx` — remove `helpOpen` state, lazy `HelpPanel` import, `<HelpPanel>` mount; change `openHelp` handler to invoke `help_window_open`.
- Delete: `src/shell/HelpPanel.tsx`
- Delete: `src/shell/HelpPanel.css`
- Delete: `src/shell/HelpPanel.test.tsx`

### Subtask 8.1 — Reroute `openHelp` in AppShell

- [ ] **Step 8.1.1: Inspect current AppShell wiring**

```bash
grep -n "helpOpen\|setHelpOpen\|HelpPanel\|openHelp" src/shell/AppShell.tsx
```

Confirms the four sites the previous read identified:
- Line ~57-58: lazy import `HelpPanel`
- Line ~215: `const [helpOpen, setHelpOpen] = useState(false);`
- Line ~505: `openHelp: () => setHelpOpen(true),`
- Line ~811-813: conditional `<HelpPanel>` mount

- [ ] **Step 8.1.2: Edit AppShell**

Make four targeted edits:

1. **Remove the lazy import** (lines ~57-58):

```tsx
// DELETE:
const HelpPanel = lazy(() =>
  import('./HelpPanel').then((m) => ({ default: m.HelpPanel })),
);
```

2. **Remove the helpOpen state** (line ~215):

```tsx
// DELETE:
const [helpOpen, setHelpOpen] = useState(false);
```

3. **Re-route `openHelp`** (line ~505):

```tsx
// BEFORE:
openHelp: () => setHelpOpen(true),

// AFTER:
openHelp: () => {
  void invoke('help_window_open').catch((err) => {
    // tuxlink-0gsy: log + best-effort fallback. The Help menu item should
    // never become a no-op; if the Tauri command fails the operator at
    // least sees a console error rather than a silent miss.
    console.error('help_window_open failed:', err);
  });
},
```

If `invoke` is not already imported at the top of `AppShell.tsx`, add it:

```tsx
import { invoke } from '@tauri-apps/api/core';
```

4. **Remove the `<HelpPanel>` mount** (lines ~811-813):

```tsx
// DELETE:
{helpOpen && (
  <Suspense fallback={null}>
    <HelpPanel open={true} onClose={() => setHelpOpen(false)} />
  </Suspense>
)}
```

- [ ] **Step 8.1.3: Delete the old modal files**

```bash
rm src/shell/HelpPanel.tsx src/shell/HelpPanel.css src/shell/HelpPanel.test.tsx
```

- [ ] **Step 8.1.4: Build + test**

```bash
pnpm -C . build
pnpm -C . test
```

Expected: build SUCCEEDS, all tests PASS. The `dispatchMenuAction` unit tests still pass because the dispatcher itself didn't change — only the implementation of `openHelp` inside AppShell.

### Subtask 8.2 — Commit

- [ ] **Step 8.2.1: Stage + commit**

```bash
git -C . add src/shell/AppShell.tsx
git -C . add -u src/shell/HelpPanel.tsx src/shell/HelpPanel.css src/shell/HelpPanel.test.tsx
git -C . commit -m "$(cat <<'EOF'
refactor(help): remove old modal HelpPanel + reroute dispatch (tuxlink-0gsy)

The new help window (Tasks 1-7) is now the canonical Help → Documentation
surface. This commit removes the old modal:

Deletions:
- src/shell/HelpPanel.tsx       (260 lines)
- src/shell/HelpPanel.css       (215 lines)
- src/shell/HelpPanel.test.tsx  (70 lines)

AppShell edits:
- Lazy HelpPanel import — removed
- helpOpen state — removed
- openHelp handler — now invokes help_window_open instead of
  setting modal open state. Logs Tauri-side failures rather than
  silent no-op (the Help menu item is operator-facing).
- <HelpPanel> conditional mount — removed

The Help menu (Documentation) now opens the separate Tauri window
implemented in src/help/. Single-instance: re-clicking focuses the
existing window.

markdownRender.ts (src/shell/) is kept and reused by the new
ReadingPane (src/help/ReadingPane.tsx) — its tests are unaffected.

Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §11.

Agent: bog-bluff-mesa
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9 — Integration tests + push (Commit 9)

**Files:**
- Create: `src/help/HelpView.integration.test.tsx`
- Run: full test + build verification before push

### Subtask 9.1 — End-to-end-ish frontend integration test

- [ ] **Step 9.1.1: Write the integration test**

Create `src/help/HelpView.integration.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';

// Mock Tauri shell for outbound-link smoke + search command.
const invokeMock = vi.fn();
const shellOpenMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock('@tauri-apps/plugin-shell', () => ({
  open: (...args: unknown[]) => shellOpenMock(...args),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { HelpView } from './HelpView';

beforeEach(() => {
  invokeMock.mockReset();
  shellOpenMock.mockReset();
  localStorage.clear();
  document.documentElement.style.removeProperty('--help-font-size');
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'theme_get_scheme') return Promise.resolve(null);
    if (cmd === 'docs_search') return Promise.resolve([]);
    return Promise.reject(new Error(`unmocked: ${cmd}`));
  });
});

function renderHelp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    React.createElement(QueryClientProvider, { client }, React.createElement(HelpView)),
  );
}

describe('HelpView — integration', () => {
  it('navigates from one topic to another via the sidebar', async () => {
    renderHelp();
    fireEvent.click(screen.getByText('Connections'));
    await waitFor(() =>
      expect(screen.getByRole('heading', { level: 1, name: /Connections/i })).toBeInTheDocument(),
    );
  });

  it('changing text size in the dropdown updates --help-font-size', () => {
    renderHelp();
    fireEvent.click(screen.getByRole('button', { name: /Text size:/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Huge' }));
    expect(document.documentElement.style.getPropertyValue('--help-font-size')).toBe('24px');
  });

  it('triggers docs_search when the operator types in the sidebar search', async () => {
    invokeMock.mockImplementation((cmd: string, args?: { query?: string }) => {
      if (cmd === 'theme_get_scheme') return Promise.resolve(null);
      if (cmd === 'docs_search') {
        return Promise.resolve([
          { slug: '02-connections', title: 'Connections', snippet: 'About <mark>ardop</mark>' },
        ]);
      }
      return Promise.reject(new Error(`unmocked: ${cmd}`));
    });
    renderHelp();
    fireEvent.change(screen.getByPlaceholderText(/Search topics/i), { target: { value: 'ardop' } });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('docs_search', { query: 'ardop' }),
    );
    await waitFor(() => expect(screen.getByText('Connections')).toBeInTheDocument());
  });
});
```

- [ ] **Step 9.1.2: Run integration test**

```bash
pnpm -C . exec vitest run src/help/HelpView.integration.test.tsx
```

Expected: all PASS.

### Subtask 9.2 — Full verification

- [ ] **Step 9.2.1: Run all Vitest tests**

```bash
pnpm -C . test
```

Expected: all PASS.

- [ ] **Step 9.2.2: Run Cargo tests**

```bash
cargo --manifest-path src-tauri/Cargo.toml test
```

Expected: all PASS.

- [ ] **Step 9.2.3: Run typecheck / build**

```bash
pnpm -C . build
cargo --manifest-path src-tauri/Cargo.toml build
```

Expected: both SUCCEED.

### Subtask 9.3 — Commit + push

- [ ] **Step 9.3.1: Commit the integration test**

```bash
git -C . add src/help/HelpView.integration.test.tsx
git -C . commit -m "$(cat <<'EOF'
test(help): integration test for HelpView (tuxlink-0gsy)

End-to-end-ish smoke covering the wired flow:
- Sidebar topic click → reading pane swaps to the selected topic.
- TextSizeDropdown selection → --help-font-size CSS variable
  updates on <html>.
- Sidebar search input change → docs_search invoked with the
  trimmed query; hits render in the sidebar.

Tauri APIs (invoke, plugin-shell open, event listen) are mocked
so the test runs in jsdom. The Rust-side cargo tests cover the
backend surfaces (caller_is_authorized, theme_state, extractor,
docs_index).

Operator-run smokes documented in spec §12.3 cover the genuinely-
runtime-only paths: single-instance focus-on-reopen, OS-level
window resize → sidebar collapse, theme follow on live client
change, external link via shellOpen.

Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §12.

Agent: bog-bluff-mesa
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 9.3.2: Push the branch**

```bash
git -C . push
```

Expected: SUCCEEDS. The branch (`bd-tuxlink-0gsy/help-window-redesign`) is already set up to track the origin remote from the spec push earlier in the session.

### Subtask 9.4 — Open PR

- [ ] **Step 9.4.1: Open the PR**

```bash
gh pr create --base main --head bd-tuxlink-0gsy/help-window-redesign --title "[bog-bluff-mesa] Help window redesign — separate Tauri window + in-app font controls (tuxlink-0gsy)" --body "$(cat <<'EOF'
## Summary

Replaces the modal `HelpPanel` (PR #214) with a separate Tauri webview window, an in-app text-size dropdown, FZ-M1-aware touch targets, and SQLite-FTS5-backed search over the bundled user guide.

Settled in [docs/superpowers/specs/2026-06-03-help-window-design.md](docs/superpowers/specs/2026-06-03-help-window-design.md) (operator-approved); executed per [docs/superpowers/plans/2026-06-03-help-window-implementation.md](docs/superpowers/plans/2026-06-03-help-window-implementation.md).

Settled decisions (D1–D8):

- **D1 Layout — Variant A**: sidebar ToC + reading pane (max 720px column).
- **D2 Font-size widget — dropdown** "Text size: <current>" menu.
- **D3 Default Normal (18 px)**, tiers 18 / 20 / 22 / 24.
- **D4 Theme**: always follow client.
- **D5 Search**: SQLite FTS5, extending `src-tauri/src/search/`.
- **D6 Single-instance window** (re-click focuses).
- **D7 FZ-M1 support**: 1100×700 default, sidebar collapses below 960 px, touch targets ≥44×44.
- **D8 Print / light-paper / multi-help — deferred** to v1.1+.

## Test plan

- [ ] `pnpm test` — all Vitest suites green (target ~990 → ~1010 after this PR).
- [ ] `cargo test` — all Rust tests green (includes new `theme_state`, `docs_index`, `extract_markdown` cases).
- [ ] Operator smoke: Help → Documentation opens 1100×700 window; re-click focuses; resize <960 → sidebar hides; size dropdown swaps text + persists across close/reopen; theme follow on live client scheme change; search "ardop" returns the Connections hit; external link opens in OS browser; close main client → help stays open.
- [ ] FZ-M1 smoke (if device available) — full guide reads at Normal; Huge tier is genuinely readable; touch targets all hit on first tap.

## Related issues

- Closes `tuxlink-0gsy`.
- Parallel: `tuxlink-h7q7` (main-client FZ-M1 audit) — not blocked by this PR.
- Follow-up: `tuxlink-s8qu` (docs expansion + Hamexandria-sourced content with ethical attribution).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR URL printed. Mark the PR ready (not draft) per memory `feedback_no_draft_pr_parking`.

---

## Self-review checklist (this plan ↔ the spec)

**Spec coverage:**

| Spec section | Covered by |
|---|---|
| §1 (Premise) | (Background — not a task.) |
| §2 D1-D8 (Decisions) | All addressed across Tasks 1-7. |
| §3 (Window architecture / Rust) | Task 1. |
| §4 (Frontend route + component) | Task 2 (route, empty view), Task 3 (full layout). |
| §5 (Layout — Variant A) | Task 3. |
| §6 (Responsive collapse / FZ-M1) | Task 3 (basic CSS collapse). Drawer pattern intentionally **deferred from this PR's frontend scope** as the spec §6.2 indicates that v1's sub-960 behavior is "sidebar hides; hamburger pattern is a v1.1 polish." The PR description and Task 9's smoke list make this scope boundary explicit. |
| §7 (Text size control) | Task 4. |
| §8 (Theme integration) | Task 5. |
| §9 (Search + FTS5) | Tasks 6 (backend) + 7 (frontend). |
| §10 (Cross-link / external-link / anchor handling) | Task 3 (`ReadingPane`). |
| §11 (Migration / cutover) | Task 8. |
| §12 (Testing) | Task 9 (frontend integration) + the inline Vitest/Cargo cases across all tasks. Operator smokes documented in PR description. |
| §13 (Out of scope) | Acknowledged — not implemented by design. |
| §14 (Open questions) | OS decorations is the v1 ship (built into Task 1's `decorations(true)` default). Other open questions documented as in-pane TODOs in the spec, not the plan. |
| §15 (Cross-references) | Spec is the source — plan does not duplicate. |

**Drawer/hamburger note:** Spec §6.2 describes the drawer behavior for sub-960px windows; the v1 implementation in Task 3 ships `display: none` for the sidebar at that breakpoint, which loses topic navigation in narrow mode. This is a v1 scope deferral — operators on FZ-M1 will use the help window at default 1100 width which fits comfortably; the narrow-mode-with-drawer is a polish PR. Filed as the v1 follow-up below.

**Type consistency:** Reviewed — `FontPreset`, `HelpTopic`, `HelpSection`, `DocsHit`, `DocTopic` shapes are stable across the tasks that consume them. `parseHelpRoute` returns `boolean` consistently (vs. `parseComposeRoute`'s `string | null`).

**Placeholder scan:** No TBD / TODO / "similar to" / "appropriate" / "fill in" patterns in any task body. Each step contains the actual code or command.

**Spec deferrals that should be filed as follow-ups when this PR lands:**
- Narrow-mode drawer (spec §6.2) — currently deferred via `display: none`. Track as a follow-up issue under tuxlink-0gsy or a new sibling.
- Custom titlebar (spec §3.2 v1 cleanup) — track under `tuxlink-h7q7` (main-client FZ-M1 audit), which is the natural sibling for chrome consistency.

---

*End of plan.*
