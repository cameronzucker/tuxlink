# Help → separate Tauri window + in-app font controls

**Date:** 2026-06-03
**Author:** bog-bluff-mesa
**bd issues:** tuxlink-0gsy (this redesign) · tuxlink-h7q7 (parallel: main-client FZ-M1 audit)
**Status:** Proposed — pending operator review of this spec
**Mock companion:** [docs/design/mockups/2026-06-03-help-window-mocks.html](../../design/mockups/2026-06-03-help-window-mocks.html) — high-fidelity dark mock: three layout candidates (A/B/C), four font-control widget candidates, live size preview at 16/18/20/22/24 px, FZ-M1 device-target constraint block.

---

## 1. Premise

The current help surface — `HelpPanel.tsx` from [PR #214](https://github.com/cameronzucker/tuxlink/pull/214) — is an inline modal overlay. It occupies a fixed `min(960 px, vw − 48) × min(680 px, vh − 48)` centered panel with a dim backdrop at z-index 100, captures pointer events, and inherits the surrounding chrome's ~14 px text size. The modal succeeded at "ship help content with low surface-area risk", and the bundled documentation it renders (ten `.md` topics under `docs/user-guide/`) is fine. The delivery mechanism is wrong for the userbase.

### 1.1 What is wrong with the current modal

- **The modal blocks the client.** A pointer-capturing backdrop means the operator cannot reference help while *doing* the procedure being referenced. Operators trying to use help as a step-by-step guide (e.g. "wire up the ARDOP HF connection") have to open help → memorize → close help → act → re-open help. That breaks the help affordance for exactly the use case help most needs to serve.
- **Fixed modest size + chrome-class text.** The reading column is ~720 px wide regardless of monitor, and the text inherits the surrounding chrome's ~14 px sizing. The tuxlink userbase skews older and emcomm-radio-adjacent — small fixed text in a fixed-size overlay is the worst of both ergonomics.
- **No window-manager affordance.** The modal cannot sit beside the client window via the OS window manager. It cannot be moved to a second monitor. It cannot be minimized while the client is in use. The modality removes every option the operator's WM would otherwise give them.

### 1.2 Why a separate Tauri window is the right replacement

The codebase already contains the multi-window precedent: [src-tauri/src/compose_window.rs](../../../src-tauri/src/compose_window.rs) opens a labeled, geometry-persisted webview window per Winlink draft. That window is the operator-facing exception to the project's "inline UI, no window clutter" preference (memory: `feedback_inline_ui_no_window_clutter`) — and the help redesign is a second cause sufficient to justify a second exception.

The shipped infrastructure that this design reuses:

- `WebviewWindowBuilder::new(... WebviewUrl::App("/help".into()))` to open a labeled window mounting a routed `/help` view inside the same React bundle.
- `tauri-plugin-window-state` to persist per-label size + position across launches.
- Same Tauri capability model as compose; same `invoke_handler` registration in `lib.rs`.
- Same React routing pattern as compose: `App.tsx` reads `window.location.pathname`, branches on `/help` vs `/compose/<id>` vs main, mounts the relevant root component.

The marginal lift is therefore the React `HelpView` component, the Rust `help_window` module mirroring `compose_window.rs`, the routing tweak in `App.tsx`, the menu dispatch tweak in `dispatchMenuAction.ts`, and the FTS5 extension in `src-tauri/src/search/`. No new dependencies; no new architecture.

### 1.3 Why this is *not* a system-browser-rendered help

The browser route was considered. The browser's real advantage is OS-level zoom (Ctrl+/Ctrl−) and the operator's already-configured browser preferences. The disadvantages outweigh that for this audience: opening Firefox/Chromium on Help click is jarring to an older operator who may not have a default browser configured, breaks the "self-contained desktop app" framing, decouples help theme from client theme, and requires either bundling a local HTTP server inside Tauri or serving via `file://` (which limits search index strategies). The separate-Tauri-window option keeps the app self-contained and lets the spec ship its own font-size controls; matching browser-level zoom is straightforward with the design below.

---

## 2. Decisions

The brainstorm settled the following decisions; this spec implements them.

| # | Decision | Rationale (compressed) |
|---|---|---|
| **D1** | **Layout: Variant A** — left sidebar (260 px) with section-grouped topic list, right reading pane (max-width 720 px). | Userbase reads documentation through an old-internet / Wikipedia mental model. Sidebar-ToC-plus-reading-pane signals competence and matches that mental model (memory: `feedback_userbase_old_internet_navigation`). |
| **D2** | **Font-size widget: dropdown menu** — button labeled `Text size: <current>` in header right; opens a preset menu (Normal / Large / X-Large / Huge) with the current selection checked. | The self-documenting label ("Text size: Large") wins over the one-click change of a segmented control for this audience; the button itself documents the affordance without requiring the operator to recognize what `A−` and `A+` mean. |
| **D3** | **Default text size: Normal (18 px).** Tier scale: Normal 18 / Large 20 / X-Large 22 / Huge 24. | 18 px is noticeably larger than the ~14 px client chrome but not aggressively accessibility-forward; tight 2-px ladder gives perceptible-but-not-jarring step changes; range spans comfortable-desktop through FZ-M1-without-OS-scaling. |
| **D4** | **Theme: always follow client theme.** Help inherits on launch and updates live if the client's theme changes mid-session. No theme picker inside the help window. | Visual consistency with the client; zero added chrome; operators who prefer light-on-dark already have it on the client surface. |
| **D5** | **Search: sidebar full-text search backed by SQLite FTS5.** Extend the existing `src-tauri/src/search/` module with a `docs_fts` virtual table populated from the bundled `docs/user-guide/*.md`. | The userbase + ~1000 lines of bundled documentation justify real FTS over substring search. FTS5 + the project's existing search infrastructure is the lower-risk path than a parallel JS-side index. |
| **D6** | **Window lifecycle: single-instance, label `help`.** Re-clicking Help → Documentation when the window is already open focuses the existing window (mirrors `compose_window.rs`'s `AlreadyExists` handling). Closing the main client does *not* close the help window. | Matches operator expectation ("I clicked Help twice, give me my window"); matches compose precedent; gives help full WM independence. |
| **D7** | **FZ-M1 device-target support.** Default window size **1100 × 700**. Sidebar auto-collapses to a hamburger drawer below **960 px** window width. All touch targets ≥ **44 × 44 px**. | Panasonic FZ-M1 (7″ 1280×800 rugged tablet) is a primary deployment device; help must be operable at that screen size. Parallel bd issue `tuxlink-h7q7` covers the main-client audit. |
| **D8** | **Print, theme override, multi-window help — out of scope for v1.** | Each defers cleanly to a later PR; none affect the architecture established here. See §13. |

---

## 3. Window architecture (Rust)

### 3.1 New module: `src-tauri/src/help_window.rs`

Mirrors `src-tauri/src/compose_window.rs` in shape. The compose precedent already paid the architectural cost of "separate Tauri window with capability bridge, per-label geometry, and a defense-in-depth caller-authorization guard." Help reuses the pattern verbatim except for the differences below.

```rust
//! Help-window management — opens a single separate Tauri webview window
//! for the user-guide documentation (tuxlink-0gsy / spec §3).
//!
//! Mirrors compose_window.rs in shape:
//!   - `WebviewWindowBuilder::new(..., WebviewUrl::App("/help".into()))`
//!   - per-label geometry persisted by tauri-plugin-window-state
//!   - registered in lib.rs's invoke_handler list
//!
//! **Single instance.** Unlike compose (which permits many windows for many
//! drafts), there is exactly one help window. Re-invoking `help_window_open`
//! when the window already exists focuses it.
//!
//! **No draft id.** The window label is the literal "help" and the URL is the
//! literal "/help". No user input is interpolated into either, so the
//! draft_id-style validation that compose_window.rs performs is unnecessary.
//!
//! **Main-window guard.** As with compose, only the main window is permitted
//! to invoke `help_window_open`. Defense-in-depth against a misbehaving help
//! frontend trying to spawn a second help window (which would also fail the
//! single-instance check, but the Rust guard makes the intent explicit).

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

    // Build fresh.
    WebviewWindowBuilder::new(&app, HELP_WINDOW_LABEL, WebviewUrl::App("/help".into()))
        .title("Tuxlink Documentation")
        .inner_size(1100.0, 700.0)
        .min_inner_size(640.0, 480.0)
        .decorations(true) // OS-native chrome — see §3.2.
        .build()
        .map_err(|e| format!("build failed: {e}"))?;

    Ok(())
}
```

### 3.2 Decorations choice

The compose window currently uses native OS decorations (titlebar, resize, min/max/close from the WM). The mock illustrates a custom in-app titlebar (matching the main client's chrome-style titlebar), but the simpler ship is to stay with OS decorations and skip the custom drag-region wiring. This is a small UX trade-off — slight visual inconsistency between the main client (custom chrome) and help (OS chrome) — in exchange for not duplicating the drag-region machinery. Mark as a v1 cleanup opportunity in §14.

### 3.3 Registration

In `src-tauri/src/lib.rs`'s `run()` builder, add `help_window_open` to the `invoke_handler!` macro alongside the existing commands. Capability: a new `tauri.conf.json` capability file `src-tauri/capabilities/help.json` analogous to `compose.json`, granting the help window only the commands it needs (`docs_search`, `shell:open` for external links — see §9 and §10). The main window's capability also needs `help_window_open` added.

### 3.4 What gets persisted, what does not

| Concern | Persistence | Mechanism |
|---|---|---|
| Window size + position | Persisted across launches | `tauri-plugin-window-state` keyed on `help` label |
| Text-size preset (Normal/Large/...) | Persisted across launches | `localStorage` keyed on `tuxlink.help.fontSize` (per-window; the main client does not read it) |
| Active topic | Not persisted | Reset to "01-getting-started" on each open |
| Search query | Not persisted | Reset on each open |
| Theme | Not persisted directly; reflects client theme at open time | See §8 |

The active-topic decision is deliberate: each help open starts at the top so the operator's last-session breadcrumb does not interfere with the next session's intent. (If a later operator request shows this is wrong, the change is a one-line localStorage flip.)

---

## 4. Frontend route + component (React)

### 4.1 Routing

`App.tsx` already branches on `window.location.pathname` to mount `<Compose>` instead of `<AppShell>` for compose webviews. Help follows the same shape:

```tsx
// src/App.tsx — incremental
import { parseHelpRoute } from './routing';            // new
const HelpView = lazy(() =>
  import('./help/HelpView').then((m) => ({ default: m.HelpView })),
);

// after existing isComposeWindow branch:
const isHelpWindow = parseHelpRoute(window.location.pathname);
if (isHelpWindow) {
  return (
    <Suspense fallback={<div data-testid="app-loading" />}>
      <HelpView />
    </Suspense>
  );
}
```

`parseHelpRoute` is a one-liner: `path === '/help'`. The compose route accepts `/compose/<id>` and parses the id; help has no id, so the check is trivial.

### 4.2 Component layout

```
src/help/
  HelpView.tsx          ← top-level component mounted at /help
  HelpView.css          ← layout grid + responsive collapse rules
  Sidebar.tsx           ← topic list + search input
  Sidebar.css           ← sidebar styles
  ReadingPane.tsx       ← rendered-markdown viewport
  ReadingPane.css       ← typography + size-tier rules
  TextSizeDropdown.tsx  ← header dropdown widget
  TextSizeDropdown.css  ← dropdown styles
  useFontSize.ts        ← localStorage-backed hook
  useHelpTheme.ts       ← live client-theme inheritance (§8)
  useHelpSearch.ts      ← FTS5 query hook (§9)
  topics.ts             ← bundled markdown + section-grouped index (§4.3)
```

The existing `src/shell/HelpPanel.tsx` is deleted in the cutover commit (§11). The markdown-rendering logic in `src/shell/markdownRender.ts` is **kept and reused** — it has tests and no modal coupling.

### 4.3 Topic registry (`topics.ts`)

The current `HelpPanel.tsx` uses `import.meta.glob` over `docs/user-guide/*.md` with the `?raw` query. The new module factors that into a typed registry:

```ts
// src/help/topics.ts
export interface HelpTopic {
  slug: string;          // "01-getting-started"
  number: string;        // "01"
  displayName: string;   // "Getting started"
  section: HelpSection;  // "getting-started" | "using" | "config" | "reference"
  body: string;          // raw markdown
}

export interface HelpSection {
  id: 'getting-started' | 'using' | 'config' | 'reference';
  displayName: string;   // "Getting started" / "Using Tuxlink" / "Configuration" / "Reference"
  topicSlugs: readonly string[];
}

export const TOPICS: readonly HelpTopic[];     // built at module load from import.meta.glob
export const SECTIONS: readonly HelpSection[]; // hand-authored ordering
```

Section ordering and which topic belongs in which section is hand-authored in this file — the markdown filenames carry numeric prefixes for stable file ordering, but the section grouping (`Getting started / Using / Config / Reference`) is a human curation decision and lives in code, not in filename conventions.

`displayName` is parsed from each markdown's first `#` heading.

---

## 5. Layout — Variant A details

### 5.1 Geometry at the default 1100 × 700 window size

```
┌─────────────────────────────── 1100 px ──────────────────────────────┐
│ OS titlebar — "Tuxlink Documentation"                                │ 28–32 (OS)
├──────────────────────────────────────────────────────────────────────┤
│ Header strip (54 px)                                                 │
│ [User Guide]               [search input — 240 px]  [Text size: …]  │
├─────────────────────────────────┬────────────────────────────────────┤
│ Sidebar (260 px)                │ Reading pane (840 px frame)        │
│                                 │                                    │
│   GETTING STARTED               │       ┌── 720 px reading column ──┐│
│   01 Getting started            │       │ # Title                   ││
│   02 Connections        ●active │       │                           ││
│                                 │       │ Body paragraphs…          ││
│   USING TUXLINK                 │       │                           ││
│   03 Mailbox                    │       │                           ││
│   04 Composing                  │       └───────────────────────────┘│
│   05 Forms                      │                                    │
│   06 Search                     │                                    │
│                                 │                                    │
│   CONFIGURATION                 │                                    │
│   07 Settings                   │                                    │
│   08 Color schemes              │                                    │
│   09 Keyboard shortcuts         │                                    │
│                                 │                                    │
│   REFERENCE                     │                                    │
│   10 Troubleshooting            │                                    │
└─────────────────────────────────┴────────────────────────────────────┘
```

Sidebar items are 44 px tall (touch-comfort floor). Section header rows are 28 px, uppercase, faint-color, with letter-spacing for separation.

### 5.2 CSS structure

The component layout uses CSS Grid for the outer skeleton (header / split body) and Flexbox for the within-sidebar item list and within-pane reading column. CSS variables drive the size tiers (§7), the theme tokens (§8), and the responsive breakpoint (§6).

```css
/* HelpView.css — outer skeleton */
.tux-help-root {
  display: grid;
  grid-template-rows: auto 1fr;
  height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: var(--sans);
}
.tux-help-header { /* see §7 for inner */ }
.tux-help-body {
  display: grid;
  grid-template-columns: 260px 1fr;
  min-height: 0;
}
@media (max-width: 960px) {
  .tux-help-body { grid-template-columns: 1fr; }
}
```

### 5.3 Reading column max-width

The reading column is constrained to `max-width: 720px` and horizontally centered within whatever space the reading pane provides. This holds the line length in the comfortable 60–75 character range even when the window is maximized on a wide monitor (memory: `feedback_no_stretched_full_width_ui`). The window's "extra" width is ornamental — visual breathing room around the reading column, not added text width.

### 5.4 Active-topic visual treatment

The active sidebar entry has:

- `color: var(--accent)` (the orange accent token, currently `#f59f3c`)
- `background: var(--accent-soft)` (12% opacity of the same accent)
- A left-edge border `border-left: 3px solid var(--accent)`
- Slightly bolder weight (`font-weight: 500`)

The same treatment is reused for search-hit highlights (§9.5) so the operator's eye learns one pattern.

---

## 6. Responsive collapse for FZ-M1 (D7)

### 6.1 Breakpoint behavior

Below **960 px window width**, the sidebar collapses to a hamburger button at the top-left of the header. Above 960 px, the sidebar is permanently visible. The breakpoint is enforced in CSS only (no JS measurement) so resize is responsive without React state churn.

| Window width | Sidebar state | Topic-nav affordance |
|---|---|---|
| ≥ 960 px (e.g. 1100 default) | Permanent left sidebar | Always-visible topic list |
| < 960 px (e.g. narrow side-by-side; FZ-M1 portrait) | Hamburger "Contents" button | Tap hamburger → drawer overlay |

### 6.2 Drawer behavior (collapsed mode)

The drawer is a left-edge slide-in overlaying the reading pane. Tapping outside the drawer closes it. The drawer's content is the same topic list as the permanent sidebar; the section grouping is preserved. The drawer is dismiss-on-select (tapping a topic closes the drawer and navigates to that topic).

Drawer width: 280 px (slightly wider than the permanent sidebar to give touch targets more breathing room).

### 6.3 Touch-target sizing across the UI

Every interactive element must clear a 44 × 44 px tap area when the window is in collapsed mode. In permanent-sidebar mode, items can be visually smaller as long as the tap-region (clickable padding box) still clears 44 × 44 px. The size-tier definitions:

| Element | Visual size | Min tap region |
|---|---|---|
| Sidebar topic item | 40 px height (visual) | 44 × full-width (sidebar) |
| Hamburger button | 36 × 36 px (visual) | 44 × 44 px (padded) |
| Text-size dropdown button | 40 px height (visual) | 44 × 140 px (padded) |
| Dropdown menu items | 40 px height (visual) | 44 × 180 px (padded) |
| Sidebar search input | 36 px height | 44 px (padded box around input) |

CSS uses `padding` to extend the tap region beyond the visible bounds where needed; the visible rendering stays compact for desktop while the touch surface meets the FZ-M1 contract.

### 6.4 No JS resize handlers

The collapse is pure CSS media query. The drawer open/close state is React-side (a boolean), but the *threshold* is CSS-only. This means no `window.matchMedia` listener, no resize debouncing, no re-render storm during a drag-resize.

---

## 7. Text size control (D2, D3)

### 7.1 Widget visual

The button reads `Text size: <Current>` with a chevron, e.g. `Text size: Large ▼`. Click opens a vertical menu of presets with the current one checked. The widget lives at the right end of the header strip, opposite the User Guide title.

### 7.2 Tier mapping

| Preset name | px value | CSS variable |
|---|---|---|
| Normal | 18 px | `--help-font-size: 18px` |
| Large | 20 px | `--help-font-size: 20px` |
| X-Large | 22 px | `--help-font-size: 22px` |
| Huge | 24 px | `--help-font-size: 24px` |

`ReadingPane.css` consumes `--help-font-size` as the base for the rendered markdown:

```css
.tux-help-reading-content { font-size: var(--help-font-size); line-height: 1.65; }
.tux-help-reading-content h1 { font-size: calc(var(--help-font-size) * 1.7); }
.tux-help-reading-content h2 { font-size: calc(var(--help-font-size) * 1.3); }
.tux-help-reading-content code { font-size: calc(var(--help-font-size) * 0.9); }
/* … */
```

All other typography scales relative to the base. Headings, code, and inline elements all preserve their ratio across tiers.

### 7.3 Persistence

The preset selection is persisted in `localStorage` under the key `tuxlink.help.fontSize` as the literal preset name (`'Normal' | 'Large' | 'X-Large' | 'Huge'`). On open, `useFontSize.ts` reads the value; on change, it writes. The hook:

```ts
// src/help/useFontSize.ts (sketch)
const PRESETS = ['Normal', 'Large', 'X-Large', 'Huge'] as const;
type Preset = (typeof PRESETS)[number];
const PX: Record<Preset, number> = { Normal: 18, Large: 20, 'X-Large': 22, Huge: 24 };
const STORAGE_KEY = 'tuxlink.help.fontSize';

export function useFontSize() {
  const [preset, setPreset] = useState<Preset>(() => {
    const raw = localStorage.getItem(STORAGE_KEY);
    return PRESETS.includes(raw as Preset) ? (raw as Preset) : 'Normal';
  });
  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, preset);
    document.documentElement.style.setProperty('--help-font-size', `${PX[preset]}px`);
  }, [preset]);
  return { preset, setPreset, presets: PRESETS };
}
```

### 7.4 Keyboard accelerators

- `Ctrl++` (and `Ctrl+=` for keyboards without a dedicated `+`) → step up one preset
- `Ctrl+−` → step down one preset
- `Ctrl+0` → reset to default (Normal)

Implementation: a global keydown listener in `HelpView.tsx` that scopes to the help window only. Falls through to webview's native handlers if the operator is in an `<input>` (the sidebar search box).

### 7.5 Tier validation

`PRESETS.includes(...)` rejects unknown localStorage values silently (treating malformed input as "use default"). This guards against a hand-edited localStorage or a corrupted value bricking the help window.

---

## 8. Theme integration (D4)

### 8.1 Source of truth

The client persists its current theme via `applyColorScheme` / `saveColorScheme` in `src/shell/colorScheme.ts`. The theme tokens are applied to `:root` as CSS variables (the `--bg`, `--surface`, `--text` family from `src/App.css`). The help window inherits the same token family because it loads the same `App.css`.

### 8.2 Live updates

When the client's theme changes mid-session, the help window must reflect it. Mechanism: a Tauri event emitted by the main window's theme-change code; the help window subscribes:

```ts
// src/help/useHelpTheme.ts (sketch)
export function useHelpTheme() {
  useEffect(() => {
    // Initial: load whichever scheme the main client last persisted.
    invoke('color_scheme_current').then((scheme: string) => applyColorScheme(scheme));

    // Live: subscribe to broadcasts from the main window.
    const unlisten = listen<string>('color_scheme_changed', (e) =>
      applyColorScheme(e.payload),
    );
    return () => { unlisten.then((f) => f()); };
  }, []);
}
```

`color_scheme_current` is a thin existing-or-new Tauri command that reads the persisted scheme name. `color_scheme_changed` is a new event the main client emits when the operator picks a new scheme in Settings or the Theme Designer; the change is currently localized to the main window and needs to be lifted to an `app.emit` broadcast.

### 8.3 No theme override in help

The brainstorm settled "always follow client" (D4) — no light-paper mode for long reads, no per-window theme picker. The trade-off (some operators may prefer light text on light paper for extended reading) is accepted in v1 in exchange for zero added chrome and zero state-divergence between main and help.

---

## 9. Search (D5)

### 9.1 Backend extension

The existing `src-tauri/src/search/` module owns:

- `mod.rs::build_service(data_dir: &Path) -> SearchService` — constructs the SQLite-backed search service.
- `index.rs` — SQLite schema + insert/update logic; uses an FTS5 virtual table `messages_fts` for mailbox content.
- `query.rs` — query parsing + execution.
- `extractor.rs` — content extraction (B2F → plain text).
- `commands.rs` — Tauri command surface.

The docs search extends this module:

- **New FTS5 virtual table:** `docs_fts(slug, title, body)`. `slug` is the topic key (`'01-getting-started'`), `title` is the topic display name, `body` is the plaintext-extracted markdown.
- **New extractor:** `extractor::extract_markdown(md: &str) -> String` — strips markdown syntax (`#` headings, `*` emphasis, fenced code blocks, link text but not URLs, etc.) and returns plain text suitable for tokenization.
- **Build-time vs first-launch indexing:** the bundled `docs/user-guide/*.md` is known at compile time. The simpler ship is first-launch indexing — `SearchService::build_service` checks whether `docs_fts` is populated and, if not, ingests the topics. This avoids a build-time SQLite-population step and keeps the docs source-of-truth in markdown files. Cost: a one-time ~100 ms FTS5 ingest on first launch. Trade-off acceptable.
- **New query path:** `query::search_docs(svc: &SearchService, q: &str) -> Result<Vec<DocsHit>>`. Returns hits with BM25 ranking and a snippet from FTS5's `snippet()` aggregate.

### 9.2 Hit model

```rust
pub struct DocsHit {
    pub slug: String,        // "02-connections"
    pub title: String,       // "Connections"
    pub snippet: String,     // FTS5 snippet() output with <mark>...</mark>
    pub rank: f64,           // FTS5 BM25 (lower = better)
}
```

### 9.3 Tauri command

```rust
#[tauri::command]
pub fn docs_search(svc: State<Arc<SearchService>>, query: String) -> Result<Vec<DocsHit>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);  // empty query = no hits, not an error
    }
    svc.search_docs(&query).map_err(|e| e.to_string())
}
```

Registered in `lib.rs`'s `invoke_handler!` macro. Capability granted to the help window via `capabilities/help.json`.

### 9.4 Frontend integration

```ts
// src/help/useHelpSearch.ts (sketch)
export function useHelpSearch(query: string) {
  return useQuery({
    queryKey: ['help', 'search', query],
    queryFn: () => invoke<DocsHit[]>('docs_search', { query }),
    enabled: query.trim().length > 0,
    staleTime: 60_000,
  });
}
```

`Sidebar.tsx` reads the search input via local state, debounces by 120 ms, and passes the debounced value to `useHelpSearch`. When the query is non-empty:

- The section-grouped topic list is **replaced** by a flat hit list, ordered by FTS5 rank.
- Each hit shows: topic title, slug number, and a single line of snippet (with `<mark>...</mark>` rendered as a yellow inline highlight).
- Selecting a hit navigates to that topic; the reading pane scrolls to the first match and visually highlights it (§9.5).
- The search input shows an `×` clear button when non-empty; clearing returns the sidebar to the grouped topic list.

### 9.5 In-pane hit highlighting

When a search hit is selected, the reading pane is rendered with the matched terms wrapped in `<mark class="tux-help-mark">` spans. The first match is scrolled into view. `<mark>` uses `var(--accent-soft)` background and `var(--accent)` text — same family as the sidebar active-item treatment so the visual pattern is consistent.

If the query spans multiple terms (`"ardop settings"`), all of them get the same highlight treatment. Quoted phrases get the highlight as a single span.

### 9.6 What does *not* go in search

- Code blocks (FTS5 ingests them as text, but the snippet renderer prefers prose lines if both exist).
- The keyboard shortcut table in `09-keyboard.md` — this content is high-value but its structure doesn't render well as snippets; the topic itself surfaces via title-match search.
- External links (URLs in the markdown).

These are heuristic in the extractor, not config — i.e. the extractor produces plaintext that happens to deprioritize these.

---

## 10. Cross-link and external-link behavior

### 10.1 Inter-topic links (`.md` relative paths)

Markdown like `[The mailbox](03-mailbox.md)` stays in-window. The link handler in `ReadingPane.tsx` intercepts clicks on `<a>` elements whose `href` matches `\d+-[a-z-]+\.md$`, resolves to the topic slug, and swaps the active topic. No window navigation; the URL bar (if visible) does not change.

### 10.2 External links (`http://` / `https://`)

External links are routed through `@tauri-apps/plugin-shell::open`, which opens them in the operator's default browser. This is unchanged from the current `AboutDialog.tsx` and `HelpPanel.tsx` behavior.

### 10.3 In-document anchors (`#`)

Markdown with `#section-id` hash links scroll to the matching heading. Implementation: the markdown renderer (`markdownRender.ts`) generates `id="..."` on heading elements from a slugified version of the heading text. The link handler intercepts hash-only links and calls `scrollIntoView({ behavior: 'smooth', block: 'start' })`.

### 10.4 What is *not* a link

Bare URLs in prose (e.g. `https://winlink.org`) without `[text](url)` syntax are *not* auto-linked. The markdown renderer keeps the conservative parse the current `markdownRender.ts` uses. Operators wanting external navigation must use the explicit link syntax in the source markdown.

---

## 11. Migration / cutover

### 11.1 What gets deleted

- `src/shell/HelpPanel.tsx` (260 lines)
- `src/shell/HelpPanel.css` (215 lines)
- `src/shell/HelpPanel.test.tsx` (70 lines)
- The `helpOpen` / `setHelpOpen` state in `src/shell/AppShell.tsx`
- The inline render of `<HelpPanel … />` inside `AppShell.tsx`'s overlay slot

### 11.2 What gets re-routed

- `src/shell/chrome/dispatchMenuAction.ts`'s case for `menu:help:docs` currently sets `helpOpen=true`. It now calls `invoke('help_window_open')`.
- `src/shell/chrome/dispatchMenuAction.test.ts`'s assertion for the menu item updates accordingly.

### 11.3 What gets kept

- `docs/user-guide/*.md` — content is unchanged.
- `src/shell/markdownRender.ts` + tests — unchanged; reused inside `ReadingPane.tsx`.
- `src/shell/AboutDialog.tsx` — unchanged; the About dialog stays an inline modal (its content is small, modal makes sense).

### 11.4 Cutover sequence

Single PR, ordered commits per ADR 0010 (no-squash):

1. `feat(help): help_window Rust module + invoke_handler registration` — adds the module, capability file, registers the command. No frontend changes; build still passes.
2. `feat(help): React route + HelpView skeleton` — adds `parseHelpRoute`, the `App.tsx` branch, an empty `HelpView` mounted at `/help`. Manual smoke: `Help → Documentation` opens the new (empty) window.
3. `feat(help): Sidebar + topic registry + reading pane (Variant A)` — implements §4 and §5; the new window now renders the documentation in the new layout. Old modal still exists in parallel and is unreachable.
4. `feat(help): text-size dropdown + persistence (§7)` — adds the dropdown widget, font CSS variables, keyboard accelerators.
5. `feat(help): theme inheritance + live updates (§8)` — adds the theme-change event + listener.
6. `feat(search): docs_fts virtual table + extractor + docs_search command (§9 backend)` — extends `src-tauri/src/search/`.
7. `feat(help): sidebar search UI + hit highlighting (§9 frontend)` — wires the frontend.
8. `refactor(help): remove old modal HelpPanel + update dispatchMenuAction` — deletes §11.1 files, re-routes §11.2.
9. `test(help): integration test for help-window dispatch + visual snapshot of HelpView` — coverage for the new surface.

Each commit is independently green (build + tests pass). The old modal works through commit 7 and is removed in commit 8. The integration test in commit 9 covers the dispatch + the rendered view.

### 11.5 No backwards compatibility

The localStorage key the old modal used (if any) is *not* migrated — the new key (`tuxlink.help.fontSize`) is a fresh namespace. Operators relaunching after the cutover start at the Normal-default state.

---

## 12. Testing approach

### 12.1 Unit tests (Vitest, no Tauri runtime)

- `parseHelpRoute('/help')` → true; `parseHelpRoute('/compose/abc')` → false; `parseHelpRoute('/')` → false.
- `useFontSize` — persist + read; invalid stored value falls back to default; preset progression via Ctrl+ / Ctrl−.
- `Sidebar.test.tsx` — section grouping renders; active topic gets accent treatment; search input debouncing; clear button behavior.
- `ReadingPane.test.tsx` — markdown rendering; in-topic link intercepts swap topic; external link calls `shellOpen`; in-page anchors scroll.
- `TextSizeDropdown.test.tsx` — opens/closes; selected item gets check; selecting an item dismisses the menu.
- `topics.ts` — `TOPICS` and `SECTIONS` arrays are well-formed; every topic belongs to exactly one section.

### 12.2 Unit tests (Rust)

- `help_window::caller_is_authorized('main')` → true; `'compose-x'` → false.
- `search::extractor::extract_markdown(...)` — known input/output pairs; headings preserved as text, fenced code blocks become single-line, link text preserved, URLs stripped.
- `search::query::search_docs(...)` against an in-memory index — sanity hits for known terms, empty-query path returns empty, special chars survive tokenization.

### 12.3 Integration / smoke (operator-run)

The Tauri runtime cannot be exercised in Vitest. The following smokes are documented in the PR body and verified by the operator before merge:

1. **Open help; window appears at 1100 × 700.** Re-clicking Help → Documentation focuses the existing window (single-instance).
2. **Resize to 800 × 600; sidebar collapses to hamburger.** Tap hamburger → drawer opens. Tap a topic → drawer closes; reading pane shows the topic.
3. **Text-size dropdown.** Select each tier; reading text resizes; close + reopen window; preset persists.
4. **Theme follow.** Open help; switch the main client's theme; help updates without re-open.
5. **Search.** Type "ardop" in the sidebar search; relevant topics surface; click one; matched term is highlighted in the reading pane.
6. **External link.** Click an `http://` link in the help content; default browser opens.
7. **In-topic link.** Click `[mailbox](03-mailbox.md)`; reading pane swaps to the mailbox topic.
8. **Close main client; help stays open** (lifecycle independence). Close help; main client unaffected.
9. **FZ-M1 smoke** (when an FZ-M1 is available; otherwise the resize smoke in step 2 substitutes for the layout-fitness check).

### 12.4 What is *not* tested

- Visual regression (no Percy / Loki in the project yet). The mock companion is the visual reference.
- HiDPI rendering on FZ-M1 (deferred until physical device is in hand; step 9 above is the stand-in).
- Print path (out of scope for v1, §13).

---

## 13. Out of scope for v1

- **Print** — no print button in the header. The Tauri webview's underlying browser would support `window.print()` if added later; the print stylesheet would be a small CSS-only follow-up. Not shipped now to keep v1 scope tight.
- **Light-paper theme override** — settled in D4 to follow client theme always. If a future operator request shows this is wrong, the change is a small dropdown next to the text-size widget.
- **Multi-window help** — only one help window at a time (D6). If a future workflow demands "two help topics open side-by-side", the window-label scheme would need to extend from the literal `'help'` to `'help-<topic>'`. Not anticipated.
- **Offline-rendered version** — the in-window help is the canonical viewer. Operators wanting to print or share the user guide can use the markdown source files under `docs/user-guide/` directly.
- **Help search index for translations** — the FTS5 index uses English-only tokenization. Future localization work would need per-language indexing.
- **Telemetry on help usage** — no analytics; no "which topic gets opened most" tracking.
- **Bookmarks within help** — operators returning to a frequently-referenced spot rely on the topic list + search + browser-style scroll position (which is *not* persisted across sessions, §3.4).

---

## 14. Open questions

These are not blocking the spec but are worth flagging for the implementation pass:

- **Custom titlebar follow-up** (§3.2). v1 ships OS-native decorations (settled in §3.2 for parity with compose and to avoid duplicating drag-region wiring). A small follow-up PR can replace the OS titlebar with the in-app chrome-style titlebar the mock illustrates, once the FZ-M1 main-client audit (`tuxlink-h7q7`) settles whether the main client's custom chrome stays as-is.
- **Color of `<mark>` highlight** (§9.5). The spec uses `var(--accent-soft)` background + `var(--accent)` text, mirroring the active-sidebar treatment. If operator smoke shows this is too subtle on the dark theme, alternatives are `var(--unread-dot)` (warm yellow) or a soft underline.
- **Search ranking tuning** (§9.1). FTS5 BM25 defaults work fine on prose-heavy text. If hit ordering surprises operators (e.g. an "intro" topic ranking higher than a deeper hit), the column weights in the FTS5 table definition (`docs_fts(slug, title, body)` allows weighting `title` more heavily) can be tuned.
- **Hamburger drawer animation** (§6.2). Sliding from the left at 250 ms is the default. If the FZ-M1's webview struggles with the animation, the fallback is an instant snap.

---

## 15. Cross-references

- Mock companion: [docs/design/mockups/2026-06-03-help-window-mocks.html](../../design/mockups/2026-06-03-help-window-mocks.html)
- Compose-window precedent: [src-tauri/src/compose_window.rs](../../../src-tauri/src/compose_window.rs)
- Existing help (to be deleted): [src/shell/HelpPanel.tsx](../../../src/shell/HelpPanel.tsx), [src/shell/HelpPanel.css](../../../src/shell/HelpPanel.css), [src/shell/HelpPanel.test.tsx](../../../src/shell/HelpPanel.test.tsx)
- Existing markdown renderer (kept and reused): [src/shell/markdownRender.ts](../../../src/shell/markdownRender.ts)
- Existing search infrastructure (extended): [src-tauri/src/search/](../../../src-tauri/src/search/)
- Sister bd issue (parallel main-client FZ-M1 audit): `tuxlink-h7q7`
- ADR 0008 (worktrees mandatory under bd-issue ownership)
- ADR 0010 (no-squash-merge — informs the cutover commit sequence in §11.4)

Memory references:
- `feedback_inline_ui_no_window_clutter` — the rule the help window is the *second* exception to (compose was the first).
- `feedback_userbase_old_internet_navigation` — locks in Variant A layout.
- `feedback_no_stretched_full_width_ui` — locks in 720 px reading-column max.
- `feedback_high_fidelity_mocks` — informed the mock companion's fidelity.

---

*End of spec.*
