# Handoff — 2026-05-21 — sandbar-heron-bayou — read/unread + color schemes

Continued the native-client UI-polish batch on `bd-tuxlink-0ic/native-winlink-client`
(picks up from `2026-05-21-basil-vale-thistle-native-client-shipped.md`). Shipped two
features end-to-end (TDD + real-app grim smoke + pushed + closed); the rest of the UI
batch is now design- or hardware-gated.

## What shipped (branch `bd-tuxlink-0ic/native-winlink-client`, pushed, at origin)

- **`tuxlink-xgn` — read/unread state** (commit `b840b4d`, CLOSED). The native mailbox
  hardcoded `unread:false`; now read-state is tracked end to end:
  - `native_mailbox.rs`: a per-message `<mid>.read` sidecar marks read; `list` reports
    `unread = (Inbox && no marker)` — unread is an Inbox-only concept (Sent/Outbox/Archive
    always read, matching the Mock B sidebar where Sent = total). `mark_read` is a tolerant
    no-op for a missing message; `move_to` carries the marker (no orphan).
  - `WinlinkBackend::mark_read` trait method (best-effort no-op default → PatBackend
    inherits; NativeBackend forwards to the store). `message_read` marks-on-open,
    best-effort (a marker-write failure must not fail the read).
  - `useMessage` invalidates the `['mailbox']` query after an inbox message loads so the
    unread badge updates promptly (not on the 10s poll).
- **`tuxlink-8za` — selectable color schemes** (commit `5af09aa`, CLOSED). Three
  runtime-selectable, persisted schemes as **design-token overrides only** (no
  per-component CSS):
  - Default (unchanged), **Night/tactical** (all-red, night-vision-preserving luminance
    ladder), **Grayscale** (neutral, for an external NVG/red-gel overlay).
  - `App.css` `:root[data-theme="night-red"|"grayscale"]` blocks override the ~18 primitive
    tokens + the 4 literal `--tux-*` tokens; the semantic layer + every component remap
    automatically.
  - `src/shell/colorScheme.ts` (model: localStorage load/save, `<html>` data-theme apply);
    `main.tsx` applies pre-paint (no flash); native **View → Color scheme** submenu
    (`menu:view:scheme:<id>`) handled in `AppShell`.
  - **Scope = presets** (operator-approved 2026-05-21 against the visual mock); a
    user-definable custom-scheme editor is deferred (would be a new issue).

**Gates:** Rust lib 149 + all integration suites pass; frontend 321 (TDD: 11 read/unread
+ 8 colorScheme + menu manifest); tsc clean.

**Smokes (grim, real WebKitGTK on the native backend):**
- read/unread: `dev/scratch/xgn-smoke-fixture.png` (+ `-list.png`) — unread rows show the
  dot + bold subject; Inbox badge = unread count, Sent = total.
- color schemes: `dev/scratch/8za-night-red.png` + `8za-grayscale.png` (+ `-win.png`
  crops) — every web-content component recolors via tokens (no literal-color bypass).
- Candidate-palette design mock: `dev/scratch/color-schemes-mock.html` +
  `color-schemes-mock-shot.png` (the artifact the operator approved).
  - **Note:** there is **no Wayland click-injection tool** on this Pi (grim/slurp only).
    The theme smokes used a temporary `main.tsx` hardcode + Vite HMR to flip schemes on one
    running app; that hardcode was reverted. The interactive click→action paths
    (open message marks read; View→Color-scheme switches) are covered by the layer tests,
    not an injected click — the operator's real clicks are the final confirmation.

## Operator notes addressed (this session)

- **Geographica integration** (message locations + "set grid from map") — filed
  **`tuxlink-0a2`** (P3, FUTURE; relates to `tuxlink-2ob` GPS). Captured, not scheduled.
- **Night-vision / custom color schemes** — built (= `tuxlink-8za`, above).

## Remaining UI batch — all design- or hardware-gated (decisive-execution stop point)

- **`tuxlink-ng3`** (P2, "biggest") — top menu bar + titlebar are native gray; Mock B is
  dark blue. Needs custom window chrome: `decorations:false` + an HTML dark-blue
  titlebar/menu (drag region, window controls, re-wired menu events). **Design-sensitive
  (the dark-blue chrome) → worth a brainstorm/visual pass before building.**
- **`tuxlink-msr`** (P2) — compose window shows duplicated main-window chrome. **Root
  cause found this session:** `lib.rs:88` calls `app.set_menu(menu)` **app-globally**, so
  the compose window inherits the whole File/Message/Session/Mailbox/View/Tools/Help bar.
  The compose *model* is already settled (spec §5.7 Decision 2: separate floating window —
  not inline/modal). **Likely subsumed by ng3:** once ng3 replaces the native menu with
  main-window-only HTML chrome, the compose window stops showing the menu. **Recommend
  sequencing ng3 first;** if msr is done standalone, the fix is `main_window.set_menu(...)`
  per-window instead of `app.set_menu(...)` (verify on GTK — muda has Linux quirks; see
  the `menu.rs` quit-on-Linux comment). Either path needs a compose-window smoke (opening
  it needs a click or a temporary auto-open hack, given no click-injection tooling).
- **`tuxlink-2ob`** (P3) — GPS: read gpsd/serial-NMEA, feed the dashboard, honor
  `gps_state`/`position_precision` (4-char Maidenhead default). Backend plumbing is
  buildable, but full validation needs a GPS device → effectively hardware-gated.

## State

- **Branch** `bd-tuxlink-0ic/native-winlink-client`, pushed, **up to date with origin**
  (`5af09aa`). **No PR yet** (large self-contained unit; opening one vs. `feat/v0.0.1` is
  still a reasonable next step once the UI batch is further along).
- **Working tree:** clean (tracked). No stashes.
- **Worktree** `worktrees/bd-tuxlink-0ic-native-winlink-client/`. Gitignored-on-disk:
  `node_modules/`, `src-tauri/target/`, and `dev/scratch/` (the smoke screenshots + the
  approved color-scheme mock + scratch dev logs/commit-msg — reference only, local).
- **bd:** `tuxlink-0ic` still in_progress (registration + Pat-deletion + UI parity remain).
  `tuxlink-xgn` + `tuxlink-8za` CLOSED. `tuxlink-0a2` (Geographica) filed. `bd dolt push`
  has no configured Dolt remote here — bd state syncs via the git-tracked
  `.beads/issues.jsonl` (re-exported by the pre-commit hook).
- **Production blocker (unchanged, operator-side):** register client name `tuxlink` with
  Winlink before production+TLS connects work (`cms-z.winlink.org` plaintext works for dev;
  request drafted in `dev/winlink-client-registration-request.md`).

## Pending decisions for the operator

1. **ng3 dark-blue chrome design** — the exact chrome look (titlebar height, control style,
   menu treatment) is a design call; recommend a quick visual brainstorm before building.
2. **Sequencing** — recommend **ng3 before msr** (ng3 likely resolves msr's duplicate menu
   for free).
